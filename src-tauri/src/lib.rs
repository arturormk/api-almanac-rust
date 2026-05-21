use api_almanac_export::render_request_md;
use api_almanac_store::{apply_redaction, load_latest_response, now_iso8601, save_latest_response, StoredResponse};
use api_almanac_tools as tools;
use api_almanac_typesketch as typesketch;
use api_almanac_model::{
    AlmanacProject, BodyKind, Environment, ProjectLoader, RequestDef, ResolvedBody, ResolvedRequest,
    VariableResolver,
};
use api_almanac_runner::{apply_captures, run_checks, Check, HttpResponse, Runner};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

// ── App state ──────────────────────────────────────────────────────────────

pub struct AppState {
    pub project_path: Mutex<Option<PathBuf>>,
    /// Variables captured from responses during the current session.
    pub session_vars: Mutex<HashMap<String, String>>,
}

// ── Wire types (command boundary) ─────────────────────────────────────────

#[derive(Serialize)]
pub struct ProjectData {
    pub name: String,
    pub id: String,
    pub description: Option<String>,
    pub requests: Vec<RequestSummary>,
    pub environments: Vec<EnvSummary>,
}

#[derive(Serialize)]
pub struct RequestSummary {
    pub id: String,
    pub name: String,
    pub method: String,
    pub folder: String,
    pub file_path: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RequestData {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub query: HashMap<String, String>,
    pub body_content: Option<String>,
    pub body_kind: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Names of cases defined on this request (read-only; ignored on save).
    #[serde(default)]
    pub case_names: Vec<String>,
}

#[derive(Serialize)]
pub struct EnvSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentData {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

/// Result of running a project request: response + check results + captured values.
#[derive(Serialize)]
pub struct RunResult {
    pub response: HttpResponse,
    pub checks: Vec<CheckItem>,
    pub captured: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CheckItem {
    pub name: String,
    pub passed: bool,
    pub expected: String,
    pub actual: Option<String>,
}

// ── Spot-check types ───────────────────────────────────────────────────────

/// One request's result within a spot-check run.
#[derive(Serialize, Deserialize, Clone)]
pub struct SpotCheckResult {
    pub request_id: String,
    pub request_name: String,
    pub folder: String,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub checks: Vec<CheckItem>,
    pub captured: HashMap<String, String>,
    pub error: Option<String>,
}

/// Full report returned after a spot-check run.
#[derive(Serialize, Deserialize, Clone)]
pub struct SpotCheckReport {
    pub ran_at: String,
    pub environment: Option<String>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub errored: usize,
    pub duration_ms: u64,
    pub results: Vec<SpotCheckResult>,
}

// ── Shared helpers ─────────────────────────────────────────────────────────

fn load_project_data(loader: &ProjectLoader) -> Result<ProjectData, String> {
    let project = loader.load_project().map_err(|e| e.to_string())?;
    let requests = loader.load_requests().map_err(|e| e.to_string())?;
    let environments = loader.load_environments().map_err(|e| e.to_string())?;
    Ok(ProjectData {
        name: project.name,
        id: project.id,
        description: project.description,
        requests: requests
            .into_iter()
            .map(|e| RequestSummary {
                id: e.request.id.clone(),
                name: e.request.name.clone(),
                method: e.request.method.clone(),
                folder: e.folder(),
                file_path: e.file_path.to_string_lossy().into_owned(),
            })
            .collect(),
        environments: environments
            .into_iter()
            .map(|env| EnvSummary { id: env.id, name: env.name })
            .collect(),
    })
}

fn check_to_item(c: Check) -> CheckItem {
    CheckItem {
        name: c.name,
        passed: c.passed,
        expected: c.expected,
        actual: c.actual,
    }
}

// ── Commands ───────────────────────────────────────────────────────────────

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello from API Almanac backend, {}!", name)
}

#[tauri::command]
async fn open_project(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ProjectData, String> {
    use tauri_plugin_dialog::DialogExt;
    let folder = app
        .dialog()
        .file()
        .set_title("Open API Almanac Project")
        .blocking_pick_folder();
    let path = match folder {
        Some(p) => p.into_path().map_err(|e| e.to_string())?,
        None => return Err("cancelled".into()),
    };
    let loader = ProjectLoader::new(&path);
    let data = load_project_data(&loader)?;
    *state.project_path.lock().unwrap() = Some(path);
    Ok(data)
}

#[tauri::command]
async fn create_project(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ProjectData, String> {
    use tauri_plugin_dialog::DialogExt;
    let folder = app
        .dialog()
        .file()
        .set_title("Choose Folder for New Project")
        .blocking_pick_folder();
    let path = match folder {
        Some(p) => p.into_path().map_err(|e| e.to_string())?,
        None => return Err("cancelled".into()),
    };
    let almanac_path = path.join("almanac.yaml");
    if !almanac_path.exists() {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("New Project")
            .to_string();
        let id = name
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '-', "-");
        let project = AlmanacProject { id, name, description: None };
        let yaml = serde_yaml::to_string(&project).map_err(|e| e.to_string())?;
        std::fs::write(&almanac_path, yaml).map_err(|e| e.to_string())?;
    }
    let loader = ProjectLoader::new(&path);
    let data = load_project_data(&loader)?;
    *state.project_path.lock().unwrap() = Some(path);
    Ok(data)
}

#[tauri::command]
fn reload_project(state: State<'_, AppState>) -> Result<ProjectData, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    load_project_data(&ProjectLoader::new(&root))
}

#[tauri::command]
fn get_request(state: State<'_, AppState>, file_path: String) -> Result<RequestData, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let loader = ProjectLoader::new(&root);
    let entries = loader.load_requests().map_err(|e| e.to_string())?;
    let entry = entries
        .into_iter()
        .find(|e| e.file_path.to_string_lossy() == file_path)
        .ok_or_else(|| format!("request not found: {file_path}"))?;
    Ok(request_def_to_data(entry.request))
}

/// Run a project request with env + case variable substitution, expectation checks,
/// and capture extraction. Captured values are stored in the session for later requests.
#[tauri::command]
async fn run_project_request(
    state: State<'_, AppState>,
    file_path: String,
    env_id: Option<String>,
    case_name: Option<String>,
) -> Result<RunResult, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let loader = ProjectLoader::new(&root);

    let entries = loader.load_requests().map_err(|e| e.to_string())?;
    let entry = entries
        .into_iter()
        .find(|e| e.file_path.to_string_lossy() == file_path)
        .ok_or_else(|| format!("request not found: {file_path}"))?;

    let environments = loader.load_environments().map_err(|e| e.to_string())?;
    let env = env_id
        .as_deref()
        .and_then(|id| environments.iter().find(|e| e.id == id));
    let case = case_name
        .as_deref()
        .and_then(|name| entry.request.cases.get(name));

    // Build resolver: env vars → case vars → session vars (session wins)
    let mut vars: HashMap<String, String> = env
        .map(|e| e.vars.clone())
        .unwrap_or_default();
    if let Some(c) = case {
        vars.extend(c.clone());
    }
    {
        let session = state.session_vars.lock().unwrap();
        vars.extend(session.clone());
    }
    let resolved = VariableResolver::from_vars(vars)
        .resolve_request(&entry.request)
        .map_err(|e| e.to_string())?;

    let response = Runner::new().run(&resolved).await.map_err(|e| e.to_string())?;

    // Evaluate expectations
    let checks = entry
        .request
        .expect
        .as_ref()
        .map(|ex| run_checks(ex, &response))
        .unwrap_or_default()
        .into_iter()
        .map(check_to_item)
        .collect();

    // Apply captures and store in session
    let captured = apply_captures(&entry.request.capture, &response);
    if !captured.is_empty() {
        let mut session = state.session_vars.lock().unwrap();
        session.extend(captured.clone());
    }

    // Persist latest response (with redaction applied)
    let env_name = env_id.as_deref()
        .and_then(|id| environments.iter().find(|e| e.id == id))
        .map(|e| e.name.clone());
    let stored = StoredResponse {
        ran_at: now_iso8601(),
        environment: env_name,
        case: case_name.clone(),
        status: response.status,
        status_text: response.status_text.clone(),
        headers: response.headers.clone(),
        body: response.body.clone(),
        duration_ms: response.duration_ms,
        url: response.url.clone(),
    };
    let stored = apply_redaction(stored, &entry.request.redact);
    let _ = save_latest_response(&root, &entry.request.id, &stored);

    Ok(RunResult { response, checks, captured })
}

#[tauri::command]
fn save_request(
    state: State<'_, AppState>,
    file_path: String,
    data: RequestData,
) -> Result<(), String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let loader = ProjectLoader::new(&root);
    let req = request_data_to_def(data);
    loader
        .save_request(std::path::Path::new(&file_path), &req)
        .map_err(|e| e.to_string())
}

/// Return all variables currently held in the session (from captures).
#[tauri::command]
fn get_session_vars(state: State<'_, AppState>) -> HashMap<String, String> {
    state.session_vars.lock().unwrap().clone()
}

/// Clear all captured session variables.
#[tauri::command]
fn clear_session_vars(state: State<'_, AppState>) {
    state.session_vars.lock().unwrap().clear();
}

/// Infer a TypeSketch YAML sketch from a JSON response body string.
#[tauri::command]
fn sketch_json(body: String) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("not valid JSON: {e}"))?;
    Ok(typesketch::to_yaml_string(&typesketch::sketch_json(&value)))
}

/// Save a TypeSketch YAML sketch to `sketches/<request_id>.typesketch.yaml`
/// inside the currently open project.
#[tauri::command]
fn save_sketch(
    state: State<'_, AppState>,
    request_id: String,
    yaml: String,
) -> Result<(), String> {
    let root = state
        .project_path
        .lock()
        .unwrap()
        .clone()
        .ok_or("no project open")?;
    let dir = root.join("sketches");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{request_id}.typesketch.yaml"));
    std::fs::write(path, yaml).map_err(|e| e.to_string())
}

/// List all plugin manifests found in `tools/*.yaml` in the open project.
#[tauri::command]
fn list_plugins(state: State<'_, AppState>) -> Result<Vec<tools::PluginManifest>, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let tools_dir = root.join("tools");
    if !tools_dir.exists() {
        return Ok(vec![]);
    }
    let mut paths: Vec<_> = std::fs::read_dir(&tools_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("yaml" | "yml")))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            serde_yaml::from_str::<tools::PluginManifest>(&text).map_err(|e| e.to_string())
        })
        .collect()
}

/// Run a command-based analyzer plugin against the currently loaded response.
/// The response fields are passed directly from the frontend (it holds the last response).
#[tauri::command]
fn run_plugin_command(
    state: State<'_, AppState>,
    plugin_id: String,
    file_path: String,
    response_status: u16,
    response_status_text: String,
    response_headers: HashMap<String, String>,
    response_body: String,
    response_duration_ms: u64,
    response_url: String,
) -> Result<tools::PluginOutput, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;

    // Load the manifest
    let manifest_path = root.join("tools").join(format!("{plugin_id}.yaml"));
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("manifest not found for '{plugin_id}': {e}"))?;
    let manifest: tools::PluginManifest =
        serde_yaml::from_str(&text).map_err(|e| format!("invalid manifest: {e}"))?;

    // Load the request definition for context
    let loader = ProjectLoader::new(&root);
    let entries = loader.load_requests().map_err(|e| e.to_string())?;
    let entry = entries
        .into_iter()
        .find(|e| e.file_path.to_string_lossy() == file_path)
        .ok_or_else(|| format!("request not found: {file_path}"))?;

    // Body: pass as parsed JSON when valid, raw string otherwise
    let body_val: serde_json::Value = serde_json::from_str(&response_body)
        .unwrap_or(serde_json::Value::String(response_body));

    let bundle = tools::PluginBundle {
        api_almanac_plugin_api: "0.1".into(),
        request: tools::BundleRequest {
            id: entry.request.id.clone(),
            name: entry.request.name.clone(),
            method: entry.request.method.clone(),
            url: response_url,
            headers: entry.request.headers.clone(),
        },
        response: tools::BundleResponse {
            status: response_status,
            status_text: response_status_text,
            headers: response_headers,
            body: body_val,
            duration_ms: response_duration_ms,
        },
        options: serde_json::json!({}),
    };

    tools::run_plugin(&root, &manifest, &bundle).map_err(|e| e.to_string())
}

/// Export a request as a Markdown notebook file under `docs/` in the project root.
/// Returns the path of the written file (relative to project root).
#[tauri::command]
fn export_request_markdown(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<String, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let loader = ProjectLoader::new(&root);

    let entries = loader.load_requests().map_err(|e| e.to_string())?;
    let entry = entries
        .into_iter()
        .find(|e| e.file_path.to_string_lossy() == file_path)
        .ok_or_else(|| format!("request not found: {file_path}"))?;

    let sketch_path = root.join("sketches").join(format!("{}.typesketch.yaml", entry.request.id));
    let sketch = std::fs::read_to_string(&sketch_path).ok();

    let last_resp = load_latest_response(&root, &entry.request.id)
        .unwrap_or(None);

    let md = render_request_md(&entry.request, sketch.as_deref(), last_resp.as_ref());

    let folder = entry.folder();
    let doc_dir = if folder.is_empty() {
        root.join("docs")
    } else {
        root.join("docs").join(&folder)
    };
    std::fs::create_dir_all(&doc_dir).map_err(|e| e.to_string())?;
    let doc_file = doc_dir.join(format!("{}.md", entry.request.id));
    std::fs::write(&doc_file, &md).map_err(|e| e.to_string())?;

    let rel = doc_file
        .strip_prefix(&root)
        .unwrap_or(&doc_file)
        .to_string_lossy()
        .into_owned();
    Ok(rel)
}

/// Load the most recently saved response for a request (by request ID).
/// Returns `None` if no response has been saved yet.
#[tauri::command]
fn get_latest_response(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<Option<StoredResponse>, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    load_latest_response(&root, &request_id).map_err(|e| e.to_string())
}

/// Export a spot-check report as Markdown to `reports/spot-check-{timestamp}.md`.
/// Returns the relative path of the written file.
#[tauri::command]
fn export_spot_check_report(
    state: State<'_, AppState>,
    report: SpotCheckReport,
) -> Result<String, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let md = render_report_md(&report);
    let reports_dir = root.join("reports");
    std::fs::create_dir_all(&reports_dir).map_err(|e| e.to_string())?;
    // Sanitise ran_at to a safe filename: take first 19 chars and replace colons/T
    let safe_ts: String = report.ran_at.chars().take(19)
        .map(|c| if c == ':' || c == 'T' { '-' } else { c })
        .collect();
    let path = reports_dir.join(format!("spot-check-{safe_ts}.md"));
    std::fs::write(&path, &md).map_err(|e| e.to_string())?;
    Ok(path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().into_owned())
}

// ── Spot-check Markdown renderer ───────────────────────────────────────────

fn render_report_md(report: &SpotCheckReport) -> String {
    let mut out = String::new();
    out.push_str("# Spot-check Report\n\n");
    if let Some(env) = &report.environment {
        out.push_str(&format!("**Environment:** {env}  \n"));
    }
    out.push_str(&format!("**Ran at:** {}  \n", report.ran_at));
    out.push_str(&format!("**Duration:** {} ms  \n", report.duration_ms));
    let result_line = match (report.failed, report.errored) {
        (0, 0) => format!("All {} passed.", report.passed),
        (f, 0) => format!("{} passed, {} failed out of {}.", report.passed, f, report.total),
        (0, e) => format!("{} passed, {} error(s) out of {}.", report.passed, e, report.total),
        (f, e) => format!("{} passed, {} failed, {} error(s) out of {}.", report.passed, f, e, report.total),
    };
    out.push_str(&format!("**Result:** {result_line}\n\n---\n\n"));

    // Summary table
    out.push_str("## Summary\n\n");
    out.push_str("| # | Request | Folder | Status | Duration | Checks |\n");
    out.push_str("|---|---------|--------|--------|----------|--------|\n");
    for (i, r) in report.results.iter().enumerate() {
        let folder_cell = if r.folder.is_empty() { "—" } else { &r.folder };
        let status_cell = r.status.map(|s| s.to_string()).unwrap_or_else(|| "—".into());
        let dur_cell = r.duration_ms.map(|d| format!("{d} ms")).unwrap_or_else(|| "—".into());
        let checks_cell = if r.error.is_some() {
            "error".into()
        } else if r.checks.is_empty() {
            "—".into()
        } else {
            let p = r.checks.iter().filter(|c| c.passed).count();
            let t = r.checks.len();
            format!("{p}/{t} {}", if p == t { "✓" } else { "✗" })
        };
        out.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n",
            i + 1, r.request_name, folder_cell, status_cell, dur_cell, checks_cell));
    }

    // Details
    out.push_str("\n---\n\n## Details\n\n");
    for (i, r) in report.results.iter().enumerate() {
        let heading = if r.folder.is_empty() {
            format!("{}. {}", i + 1, r.request_name)
        } else {
            format!("{}. {} ({})", i + 1, r.request_name, r.folder)
        };
        out.push_str(&format!("### {heading}\n\n"));
        if let Some(err) = &r.error {
            out.push_str(&format!("**Error:** {err}\n\n"));
            continue;
        }
        if let Some(s) = r.status { out.push_str(&format!("**Status:** {s}  \n")); }
        if let Some(d) = r.duration_ms { out.push_str(&format!("**Duration:** {d} ms  \n")); }
        if !r.captured.is_empty() {
            let mut pairs: Vec<String> = r.captured.keys().cloned().collect();
            pairs.sort();
            out.push_str(&format!("**Captured:** {}  \n", pairs.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")));
        }
        if !r.checks.is_empty() {
            out.push_str("\n#### Checks\n\n| Check | Expected | Actual | |\n|-------|----------|--------|-|\n");
            for c in &r.checks {
                out.push_str(&format!("| {} | {} | {} | {} |\n",
                    c.name, c.expected,
                    c.actual.as_deref().unwrap_or("—"),
                    if c.passed { "✓" } else { "✗" }));
            }
        }
        out.push('\n');
    }
    out
}

/// Execute an ad-hoc HTTP request (no project, no checks, no captures).
#[tauri::command]
async fn execute_request(
    method: String,
    url: String,
    headers: HashMap<String, String>,
    query: HashMap<String, String>,
    body_content: Option<String>,
    body_kind: Option<String>,
) -> Result<HttpResponse, String> {
    let body = build_body(body_content, body_kind);
    let req = ResolvedRequest {
        id: "adhoc".into(),
        name: "Ad-hoc request".into(),
        method,
        url,
        headers,
        query,
        body,
    };
    Runner::new().run(&req).await.map_err(|e| e.to_string())
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn build_body(content: Option<String>, kind: Option<String>) -> Option<ResolvedBody> {
    let content = content.filter(|c| !c.is_empty())?;
    let kind = kind.as_deref().unwrap_or("text");
    let (body_kind, content_type) = match kind {
        "json" => (BodyKind::Json, "application/json"),
        "form" => (BodyKind::Form, "application/x-www-form-urlencoded"),
        _ => (BodyKind::Text, "text/plain"),
    };
    Some(ResolvedBody { kind: body_kind, content, content_type })
}

fn request_def_to_data(req: RequestDef) -> RequestData {
    let (body_content, body_kind) = req
        .body
        .map(|b| {
            let content = serde_json::to_string(&b.value).unwrap_or_default();
            let kind = match b.kind {
                BodyKind::Json => "json",
                BodyKind::Text => "text",
                BodyKind::Form => "form",
            };
            (Some(content), Some(kind.to_string()))
        })
        .unwrap_or((None, None));

    let mut case_names: Vec<String> = req.cases.keys().cloned().collect();
    case_names.sort();

    RequestData {
        id: req.id,
        name: req.name,
        method: req.method,
        url: req.url,
        headers: req.headers,
        query: req.query,
        body_content,
        body_kind,
        notes: req.notes,
        tags: req.tags,
        case_names,
    }
}

fn request_data_to_def(data: RequestData) -> RequestDef {
    use api_almanac_model::RequestBody;

    let body = data.body_content.filter(|c| !c.is_empty()).map(|content| {
        let kind = match data.body_kind.as_deref().unwrap_or("text") {
            "json" => BodyKind::Json,
            "form" => BodyKind::Form,
            _ => BodyKind::Text,
        };
        let value: serde_yaml::Value =
            serde_json::from_str(&content).unwrap_or(serde_yaml::Value::String(content));
        RequestBody { kind, value }
    });

    RequestDef {
        id: data.id,
        name: data.name,
        method: data.method,
        url: data.url,
        headers: data.headers,
        query: data.query,
        body,
        cases: Default::default(),
        expect: None,
        capture: Default::default(),
        redact: Default::default(),
        notes: data.notes,
        tags: data.tags,
    }
}

fn slugify(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut slug = String::new();
    let mut last_was_dash = true;
    for c in lower.chars() {
        if c.is_alphanumeric() {
            slug.push(c);
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    if slug.ends_with('-') { slug.pop(); }
    if slug.is_empty() { "env".to_string() } else { slug }
}

// ── Environment commands ───────────────────────────────────────────────────

#[tauri::command]
fn list_environments(state: State<'_, AppState>) -> Result<Vec<EnvironmentData>, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let loader = ProjectLoader::new(&root);
    let envs = loader.load_environments().map_err(|e| e.to_string())?;
    Ok(envs.into_iter().map(|e| EnvironmentData { id: e.id, name: e.name, vars: e.vars }).collect())
}

#[tauri::command]
fn save_environment(state: State<'_, AppState>, data: EnvironmentData) -> Result<ProjectData, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let loader = ProjectLoader::new(&root);
    let env = Environment { id: data.id, name: data.name, vars: data.vars };
    loader.save_environment(&env).map_err(|e| e.to_string())?;
    load_project_data(&loader).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_environment(state: State<'_, AppState>, name: String) -> Result<ProjectData, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let loader = ProjectLoader::new(&root);
    let base_id = slugify(&name);
    let mut id = base_id.clone();
    let mut n = 2u32;
    while root.join("environments").join(format!("{id}.yaml")).exists() {
        id = format!("{base_id}-{n}");
        n += 1;
    }
    let env = Environment { id, name, vars: Default::default() };
    loader.save_environment(&env).map_err(|e| e.to_string())?;
    load_project_data(&loader).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_environment(state: State<'_, AppState>, env_id: String) -> Result<ProjectData, String> {
    let root = state.project_path.lock().unwrap().clone().ok_or("no project open")?;
    let loader = ProjectLoader::new(&root);
    loader.delete_environment(&env_id).map_err(|e| e.to_string())?;
    load_project_data(&loader).map_err(|e| e.to_string())
}

// ── Entry point ────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            project_path: Mutex::new(None),
            session_vars: Mutex::new(HashMap::new()),
        })
        .setup(|app| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                if let Some(icon) = app.default_window_icon() {
                    let _ = window.set_icon(icon.clone());
                }
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            execute_request,
            open_project,
            create_project,
            reload_project,
            get_request,
            run_project_request,
            save_request,
            get_session_vars,
            clear_session_vars,
            sketch_json,
            save_sketch,
            export_request_markdown,
            get_latest_response,
            list_plugins,
            run_plugin_command,
            export_spot_check_report,
            list_environments,
            save_environment,
            create_environment,
            delete_environment,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
