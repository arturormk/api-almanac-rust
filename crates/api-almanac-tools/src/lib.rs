use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write as _;
use std::path::Path;

// ── Plugin manifest ─────────────────────────────────────────────────────────

/// Describes a command-based analyzer plugin stored in `tools/*.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub command: PluginCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommand {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
}

// ── Plugin input bundle (sent to the plugin on stdin) ──────────────────────

#[derive(Debug, Serialize)]
pub struct PluginBundle {
    pub api_almanac_plugin_api: String,
    pub request: BundleRequest,
    pub response: BundleResponse,
    pub options: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct BundleRequest {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct BundleResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    /// Parsed JSON value when the body is valid JSON; raw string otherwise.
    pub body: serde_json::Value,
    pub duration_ms: u64,
}

// ── Plugin output (received from the plugin on stdout) ─────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOutput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub error: Option<PluginError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// "html" | "markdown" | "yaml" | "json" | "text"
    pub kind: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginError {
    pub message: String,
}

// ── Error ───────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ToolsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("plugin process failed: {0}")]
    PluginFailed(String),
    #[error("plugin error: {0}")]
    PluginError(String),
}

// ── Runner ──────────────────────────────────────────────────────────────────

/// Execute a plugin, feed it the bundle on stdin, and return its output.
/// The process is run with `current_dir = project_root` so that relative
/// paths in `manifest.command.args` resolve from the project root.
pub fn run_plugin(
    project_root: &Path,
    manifest: &PluginManifest,
    bundle: &PluginBundle,
) -> Result<PluginOutput, ToolsError> {
    let input = serde_json::to_string(bundle)?;

    let mut child = std::process::Command::new(&manifest.command.executable)
        .args(&manifest.command.args)
        .current_dir(project_root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| ToolsError::Io(std::io::Error::new(
            e.kind(),
            format!("could not start '{}': {e}", manifest.command.executable),
        )))?;

    // Write the bundle to stdin then close (sends EOF)
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ToolsError::PluginFailed(if stderr.is_empty() {
            format!("exited with status {}", output.status)
        } else {
            stderr
        }));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: PluginOutput = serde_json::from_str(&stdout)
        .map_err(|e| ToolsError::Json(e))?;

    if let Some(err) = &result.error {
        return Err(ToolsError::PluginError(err.message.clone()));
    }

    Ok(result)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_deserializes() {
        let json = r#"{
            "id": "top-keys",
            "name": "Top-level Keys",
            "description": "Lists top-level keys from a JSON response body.",
            "command": { "executable": "python3", "args": ["tools/top-keys.py"] }
        }"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "top-keys");
        assert_eq!(m.command.executable, "python3");
        assert_eq!(m.command.args, vec!["tools/top-keys.py"]);
        assert_eq!(m.description.unwrap(), "Lists top-level keys from a JSON response body.");
    }

    #[test]
    fn plugin_output_deserializes() {
        let json = r#"
{
  "title": "Test",
  "artifacts": [
    { "kind": "html", "title": "Keys", "content": "<p>hello</p>" }
  ],
  "diagnostics": []
}
"#;
        let out: PluginOutput = serde_json::from_str(json).unwrap();
        assert_eq!(out.artifacts.len(), 1);
        assert_eq!(out.artifacts[0].kind, "html");
        assert!(out.error.is_none());
    }

    #[test]
    fn plugin_output_with_error_deserializes() {
        let json = r#"{"error":{"message":"not valid JSON"},"artifacts":[],"diagnostics":[]}"#;
        let out: PluginOutput = serde_json::from_str(json).unwrap();
        assert_eq!(out.error.unwrap().message, "not valid JSON");
    }

    #[test]
    fn bundle_serializes_json_body() {
        let bundle = PluginBundle {
            api_almanac_plugin_api: "0.1".into(),
            request: BundleRequest {
                id: "users.get".into(),
                name: "Get user".into(),
                method: "GET".into(),
                url: "https://api.example.com/users/1".into(),
                headers: HashMap::new(),
            },
            response: BundleResponse {
                status: 200,
                status_text: "OK".into(),
                headers: HashMap::new(),
                body: serde_json::json!({"id": 1, "name": "Ada"}),
                duration_ms: 100,
            },
            options: serde_json::json!({}),
        };
        let s = serde_json::to_string(&bundle).unwrap();
        assert!(s.contains("\"api_almanac_plugin_api\":\"0.1\""), "{s}");
        assert!(s.contains("\"name\":\"Ada\""), "{s}");
    }

    #[cfg(unix)]
    #[test]
    fn run_plugin_with_echo() {
        let manifest = PluginManifest {
            id: "echo-test".into(),
            name: "Echo test".into(),
            description: None,
            command: PluginCommand {
                executable: "sh".into(),
                args: vec![
                    "-c".into(),
                    r#"echo '{"artifacts":[],"diagnostics":[]}'"#.into(),
                ],
            },
        };
        let bundle = PluginBundle {
            api_almanac_plugin_api: "0.1".into(),
            request: BundleRequest {
                id: "t".into(), name: "T".into(),
                method: "GET".into(), url: "http://x".into(),
                headers: HashMap::new(),
            },
            response: BundleResponse {
                status: 200, status_text: "OK".into(),
                headers: HashMap::new(),
                body: serde_json::json!({}),
                duration_ms: 1,
            },
            options: serde_json::json!({}),
        };
        let result = run_plugin(Path::new("/tmp"), &manifest, &bundle).unwrap();
        assert_eq!(result.artifacts.len(), 0);
    }
}
