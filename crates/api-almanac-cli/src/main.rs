use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};

use api_almanac_export::render_request_md;
use api_almanac_model::{ProjectLoader, RequestEntry, VariableResolver};
use api_almanac_runner::{apply_captures, run_checks, Check, HttpResponse, Runner};
use api_almanac_store::{apply_redaction, load_latest_response, now_iso8601, save_latest_response, StoredResponse};
use api_almanac_typesketch as typesketch;

// ── CLI definition ─────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "almanac",
    about = "API Almanac — local-first HTTP API workbench",
    long_about = "Run HTTP requests defined in an API Almanac project.\n\
                  Run in a project directory or pass --project to specify the root."
)]
struct Cli {
    /// Project root directory (default: current directory, or nearest
    /// ancestor containing almanac.yaml)
    #[arg(short = 'p', long, default_value = ".")]
    project: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all requests in the project
    List,

    /// Run a single request
    Run {
        /// File path relative to project root (e.g. requests/auth/login.yaml)
        /// or request ID (e.g. auth.login)
        request: String,
        /// Environment ID
        #[arg(short = 'e', long)]
        env: Option<String>,
        /// Case name
        #[arg(short = 'c', long)]
        case: Option<String>,
    },

    /// Run all project requests in sequence (spot check)
    SpotCheck {
        /// Environment ID
        #[arg(short = 'e', long)]
        env: Option<String>,
    },

    /// Run a request and print the TypeSketch of its response
    Sketch {
        /// File path or request ID
        request: String,
        /// Environment ID
        #[arg(short = 'e', long)]
        env: Option<String>,
    },

    /// Export a request as a Markdown notebook to docs/
    ExportMd {
        /// File path or request ID
        request: String,
    },
}

// ── Entry point ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let root = match find_project_root(&cli.project) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("almanac: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = dispatch(&root, cli.command).await {
        eprintln!("almanac: {e}");
        std::process::exit(1);
    }
}

async fn dispatch(root: &Path, command: Commands) -> Result<()> {
    match command {
        Commands::List => cmd_list(root),
        Commands::Run { request, env, case } => {
            cmd_run(root, &request, env.as_deref(), case.as_deref()).await
        }
        Commands::SpotCheck { env } => cmd_spot_check(root, env.as_deref()).await,
        Commands::Sketch { request, env } => cmd_sketch(root, &request, env.as_deref()).await,
        Commands::ExportMd { request } => cmd_export_md(root, &request),
    }
}

// ── Project helpers ────────────────────────────────────────────────────────

/// Walk up from `start` until a directory containing `almanac.yaml` is found.
fn find_project_root(start: &Path) -> Result<PathBuf> {
    let abs = start
        .canonicalize()
        .with_context(|| format!("cannot access: {}", start.display()))?;
    let mut dir: &Path = &abs;
    loop {
        if dir.join("almanac.yaml").exists() {
            return Ok(dir.to_path_buf());
        }
        dir = dir.parent().ok_or_else(|| {
            anyhow!(
                "no almanac.yaml found in '{}' or any parent directory",
                start.display()
            )
        })?;
    }
}

/// Find the entry matching a file path (ending in .yaml) or a request ID.
fn resolve_entry<'a>(entries: &'a [RequestEntry], request: &str) -> Result<&'a RequestEntry> {
    if request.ends_with(".yaml") || request.ends_with(".yml") {
        let p = PathBuf::from(request);
        entries
            .iter()
            .find(|e| e.file_path == p)
            .ok_or_else(|| anyhow!("request file not found: {}", request))
    } else {
        entries
            .iter()
            .find(|e| e.request.id == request)
            .ok_or_else(|| anyhow!("no request with ID '{}' in this project", request))
    }
}

fn load_env_vars(root: &Path, env_id: Option<&str>) -> Result<HashMap<String, String>> {
    match env_id {
        None => Ok(HashMap::new()),
        Some(id) => {
            let envs = ProjectLoader::new(root)
                .load_environments()
                .context("failed to load environments")?;
            envs.into_iter()
                .find(|e| e.id == id)
                .map(|e| e.vars)
                .ok_or_else(|| anyhow!("environment '{}' not found", id))
        }
    }
}

// ── Core execution ─────────────────────────────────────────────────────────

async fn run_entry(
    root: &Path,
    entry: &RequestEntry,
    env_id: Option<&str>,
    case_name: Option<&str>,
    session: &HashMap<String, String>,
) -> Result<(HttpResponse, Vec<Check>, HashMap<String, String>)> {
    let mut vars = load_env_vars(root, env_id)?;
    // Session captures override env vars; case vars override both.
    vars.extend(session.iter().map(|(k, v)| (k.clone(), v.clone())));
    if let Some(cn) = case_name {
        let case = entry
            .request
            .cases
            .get(cn)
            .ok_or_else(|| anyhow!("case '{}' not found in '{}'", cn, entry.request.id))?;
        vars.extend(case.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    let resolved = VariableResolver::from_vars(vars)
        .resolve_request(&entry.request)
        .context("variable resolution failed")?;
    let resp = Runner::new()
        .run(&resolved)
        .await
        .context("HTTP request failed")?;
    let checks = entry
        .request
        .expect
        .as_ref()
        .map(|ex| run_checks(ex, &resp))
        .unwrap_or_default();
    let captured = apply_captures(&entry.request.capture, &resp);
    Ok((resp, checks, captured))
}

// ── Output helpers ─────────────────────────────────────────────────────────

fn status_symbol(status: u16) -> &'static str {
    if status < 400 { "✓" } else { "✗" }
}

fn print_result_line(resp: &HttpResponse, checks: &[Check]) {
    let sym = status_symbol(resp.status);
    print!("{sym}  {}  ({} ms)  {}", resp.status, resp.duration_ms, resp.url);
    if !checks.is_empty() {
        let p = checks.iter().filter(|c| c.passed).count();
        let t = checks.len();
        if p == t {
            print!("  checks {p}/{t} ✓");
        } else {
            print!("  checks {p}/{t} ✗");
        }
    }
    println!();
}

fn print_failed_checks(checks: &[Check]) {
    for c in checks.iter().filter(|c| !c.passed) {
        let actual = c.actual.as_deref().unwrap_or("—");
        println!("    ✗ {}  expected: {}  actual: {}", c.name, c.expected, actual);
    }
}

fn print_captured(captured: &HashMap<String, String>) {
    if captured.is_empty() {
        return;
    }
    let mut pairs: Vec<_> = captured.iter().collect();
    pairs.sort_by_key(|(k, _)| k.as_str());
    let items: Vec<_> = pairs
        .iter()
        .map(|(k, v)| {
            let disp = if v.len() > 50 {
                format!("{}…", &v[..50])
            } else {
                v.to_string()
            };
            format!("{k}={disp}")
        })
        .collect();
    println!("  captured: {}", items.join("  "));
}

fn pretty_body(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        serde_json::to_string_pretty(&v).unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    }
}

// ── Commands ───────────────────────────────────────────────────────────────

fn cmd_list(root: &Path) -> Result<()> {
    let loader = ProjectLoader::new(root);
    let project = loader.load_project().context("failed to load almanac.yaml")?;
    let entries = loader.load_requests().context("failed to load requests")?;

    println!("{} — {} request(s)\n", project.name, entries.len());
    if entries.is_empty() {
        println!("  (no requests yet — add YAML files under requests/)");
        return Ok(());
    }

    let id_w = entries
        .iter()
        .map(|e| e.request.id.len())
        .max()
        .unwrap_or(2)
        .max(2);
    let m_w = entries
        .iter()
        .map(|e| e.request.method.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let fol_w = entries
        .iter()
        .map(|e| e.folder().len())
        .max()
        .unwrap_or(6)
        .max(6);

    println!(
        "  {:<id_w$}  {:<m_w$}  {:<fol_w$}  Name",
        "ID", "Method", "Folder"
    );
    println!(
        "  {:<id_w$}  {:<m_w$}  {:<fol_w$}  ──────────────────────────",
        "─".repeat(id_w),
        "─".repeat(m_w),
        "─".repeat(fol_w)
    );
    for e in &entries {
        println!(
            "  {:<id_w$}  {:<m_w$}  {:<fol_w$}  {}",
            e.request.id,
            e.request.method,
            e.folder(),
            e.request.name
        );
    }
    Ok(())
}

async fn cmd_run(
    root: &Path,
    request: &str,
    env_id: Option<&str>,
    case_name: Option<&str>,
) -> Result<()> {
    let loader = ProjectLoader::new(root);
    let entries = loader.load_requests().context("failed to load requests")?;
    let entry = resolve_entry(&entries, request)?;

    let (resp, checks, captured) =
        run_entry(root, entry, env_id, case_name, &HashMap::new()).await?;

    print_result_line(&resp, &checks);
    print_failed_checks(&checks);
    print_captured(&captured);
    if !resp.body.is_empty() {
        println!("\n{}", pretty_body(&resp.body));
    }

    // Persist latest response
    let stored = apply_redaction(
        StoredResponse {
            ran_at: now_iso8601(),
            environment: env_id.map(String::from),
            case: case_name.map(String::from),
            status: resp.status,
            status_text: resp.status_text.clone(),
            headers: resp.headers.clone(),
            body: resp.body.clone(),
            duration_ms: resp.duration_ms,
            url: resp.url.clone(),
        },
        &entry.request.redact,
    );
    let _ = save_latest_response(root, &entry.request.id, &stored);

    if checks.iter().any(|c| !c.passed) {
        std::process::exit(1);
    }
    Ok(())
}

async fn cmd_spot_check(root: &Path, env_id: Option<&str>) -> Result<()> {
    let loader = ProjectLoader::new(root);
    let project = loader.load_project().context("failed to load almanac.yaml")?;
    let entries = loader.load_requests().context("failed to load requests")?;
    let total = entries.len();

    if total == 0 {
        println!("No requests to run.");
        return Ok(());
    }

    let env_tag = env_id.map(|id| format!(" [{id}]")).unwrap_or_default();
    println!(
        "Spot check — {}{} — {} request(s)\n",
        project.name, env_tag, total
    );

    let counter_width = format!("{total}").len();
    let mut session: HashMap<String, String> = HashMap::new();
    let mut n_pass = 0usize;
    let mut n_fail = 0usize;
    let mut n_err = 0usize;
    let wall = std::time::Instant::now();

    for (i, entry) in entries.iter().enumerate() {
        let counter = format!("{:>counter_width$}/{total}", i + 1);
        print!("  {counter}  {:<42}", entry.request.name);
        let _ = std::io::stdout().flush();

        match run_entry(root, entry, env_id, None, &session).await {
            Ok((resp, checks, captured)) => {
                let p = checks.iter().filter(|c| c.passed).count();
                let t = checks.len();
                let all_ok = checks.iter().all(|c| c.passed);
                if all_ok {
                    n_pass += 1;
                    if t > 0 {
                        println!("✓  {} ({} ms)  {p}/{t} checks", resp.status, resp.duration_ms);
                    } else {
                        println!("✓  {} ({} ms)", resp.status, resp.duration_ms);
                    }
                } else {
                    n_fail += 1;
                    println!("✗  {} ({} ms)  {p}/{t} checks", resp.status, resp.duration_ms);
                    print_failed_checks(&checks);
                }
                session.extend(captured);
            }
            Err(e) => {
                n_err += 1;
                println!("!  error: {e}");
            }
        }
    }

    let elapsed_ms = wall.elapsed().as_millis();
    println!();
    print!("Summary:  {n_pass} passed");
    if n_fail > 0 {
        print!("  {n_fail} failed");
    }
    if n_err > 0 {
        print!("  {n_err} error(s)");
    }
    println!("  ({elapsed_ms} ms total)");

    if n_fail > 0 || n_err > 0 {
        std::process::exit(1);
    }
    Ok(())
}

async fn cmd_sketch(root: &Path, request: &str, env_id: Option<&str>) -> Result<()> {
    let loader = ProjectLoader::new(root);
    let entries = loader.load_requests().context("failed to load requests")?;
    let entry = resolve_entry(&entries, request)?;

    let (resp, checks, captured) =
        run_entry(root, entry, env_id, None, &HashMap::new()).await?;
    print_result_line(&resp, &checks);
    print_captured(&captured);
    println!();

    let b = resp.body.trim();
    if b.is_empty() || (!b.starts_with('{') && !b.starts_with('[')) {
        println!("(response body is not JSON — no sketch available)");
        return Ok(());
    }
    let value: serde_json::Value =
        serde_json::from_str(&resp.body).context("response body is not valid JSON")?;
    print!("{}", typesketch::to_yaml_string(&typesketch::sketch_json(&value)));
    Ok(())
}

fn cmd_export_md(root: &Path, request: &str) -> Result<()> {
    let loader = ProjectLoader::new(root);
    let entries = loader.load_requests().context("failed to load requests")?;
    let entry = resolve_entry(&entries, request)?;

    let sketch_path = root
        .join("sketches")
        .join(format!("{}.typesketch.yaml", entry.request.id));
    let sketch = std::fs::read_to_string(&sketch_path).ok();
    let last_resp = load_latest_response(root, &entry.request.id).unwrap_or(None);
    let md = render_request_md(&entry.request, sketch.as_deref(), last_resp.as_ref());

    let folder = entry.folder();
    let out_dir = if folder.is_empty() {
        root.join("docs")
    } else {
        root.join("docs").join(&folder)
    };
    std::fs::create_dir_all(&out_dir).context("failed to create docs directory")?;
    let out_path = out_dir.join(format!("{}.md", entry.request.id));
    std::fs::write(&out_path, &md).context("failed to write Markdown")?;

    let rel = out_path
        .strip_prefix(root)
        .unwrap_or(&out_path)
        .to_string_lossy();
    println!("Exported → {rel}");
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn find_root_in_current_dir() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "almanac.yaml", "id: test\nname: Test\n");
        let root = find_project_root(tmp.path()).unwrap();
        assert_eq!(root, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn find_root_walks_up() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "almanac.yaml", "id: test\nname: Test\n");
        let subdir = tmp.path().join("requests").join("auth");
        fs::create_dir_all(&subdir).unwrap();
        let root = find_project_root(&subdir).unwrap();
        assert_eq!(root, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn find_root_missing_returns_error() {
        let tmp = TempDir::new().unwrap();
        assert!(find_project_root(tmp.path()).is_err());
    }

    #[test]
    fn resolve_entry_by_id() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "almanac.yaml",
            "id: test\nname: Test\n",
        );
        write(
            tmp.path(),
            "requests/users/get.yaml",
            "id: users.get\nname: Get user\nmethod: GET\nurl: https://example.com/users\n",
        );
        let loader = ProjectLoader::new(tmp.path());
        let entries = loader.load_requests().unwrap();
        let e = resolve_entry(&entries, "users.get").unwrap();
        assert_eq!(e.request.id, "users.get");
    }

    #[test]
    fn resolve_entry_by_path() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "requests/ping.yaml",
            "id: ping\nname: Ping\nmethod: GET\nurl: https://example.com/ping\n",
        );
        let loader = ProjectLoader::new(tmp.path());
        let entries = loader.load_requests().unwrap();
        let e = resolve_entry(&entries, "requests/ping.yaml").unwrap();
        assert_eq!(e.request.id, "ping");
    }

    #[test]
    fn resolve_entry_unknown_id_returns_error() {
        let entries: Vec<RequestEntry> = vec![];
        assert!(resolve_entry(&entries, "nonexistent").is_err());
    }

    #[test]
    fn cmd_export_md_writes_file() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "almanac.yaml", "id: test\nname: Test\n");
        write(
            tmp.path(),
            "requests/users/get.yaml",
            "id: users.get\nname: Get user\nmethod: GET\nurl: https://example.com/users\n",
        );
        cmd_export_md(tmp.path(), "users.get").unwrap();
        let md_path = tmp.path().join("docs/users/users.get.md");
        assert!(md_path.exists());
        let content = fs::read_to_string(md_path).unwrap();
        assert!(content.contains("# Get user"));
    }
}
