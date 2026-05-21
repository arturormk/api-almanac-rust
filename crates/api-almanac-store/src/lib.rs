use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error type ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ── Stored response ────────────────────────────────────────────────────────

/// A full HTTP response saved to disk, with run-time metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredResponse {
    /// ISO 8601 UTC timestamp of the run.
    pub ran_at: String,
    pub environment: Option<String>,
    pub case: Option<String>,
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub duration_ms: u64,
    pub url: String,
}

// ── File paths ─────────────────────────────────────────────────────────────

fn response_path(root: &Path, request_id: &str) -> std::path::PathBuf {
    root.join(".api-almanac")
        .join("responses")
        .join(format!("{}.json", sanitize_id(request_id)))
}

/// Replace characters that are invalid in file names on common platforms.
fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .collect()
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Return an ISO 8601 UTC string for the current moment.
pub fn now_iso8601() -> String {
    use chrono::Utc;
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Persist the latest response for `request_id` to `.api-almanac/responses/`.
/// Overwrites any previously saved response for the same request.
pub fn save_latest_response(
    root: &Path,
    request_id: &str,
    response: &StoredResponse,
) -> Result<(), StoreError> {
    let path = response_path(root, request_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(response)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Load the latest saved response for `request_id`.
/// Returns `None` if no response has been saved yet.
pub fn load_latest_response(
    root: &Path,
    request_id: &str,
) -> Result<Option<StoredResponse>, StoreError> {
    let path = response_path(root, request_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

/// Apply the request's `redact:` rules to a response before saving.
///
/// Each rule is a dot-prefixed path:
/// - `headers.Authorization`      → replaces the header value with `[REDACTED]`
/// - `json.access_token`          → replaces a top-level JSON body field
/// - `json.data.nested.field`     → replaces a nested JSON body field
///
/// Rules that don't match anything are silently ignored.
pub fn apply_redaction(mut response: StoredResponse, rules: &[String]) -> StoredResponse {
    for rule in rules {
        if let Some(header_name) = rule.strip_prefix("headers.") {
            let lc = header_name.to_lowercase();
            let key = response
                .headers
                .keys()
                .find(|k| k.to_lowercase() == lc)
                .cloned();
            if let Some(k) = key {
                response.headers.insert(k, "[REDACTED]".to_string());
            }
        } else if let Some(json_path) = rule.strip_prefix("json.") {
            if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&response.body) {
                if redact_json_path(&mut value, json_path) {
                    if let Ok(new_body) = serde_json::to_string_pretty(&value) {
                        response.body = new_body;
                    }
                }
            }
        }
    }
    response
}

// ── Internal helpers ───────────────────────────────────────────────────────

/// Returns `true` if anything was modified.
fn redact_json_path(value: &mut serde_json::Value, path: &str) -> bool {
    match path.split_once('.') {
        None => {
            if let serde_json::Value::Object(map) = value {
                if map.contains_key(path) {
                    map.insert(path.to_string(), serde_json::Value::String("[REDACTED]".to_string()));
                    return true;
                }
            } else if let serde_json::Value::Array(arr) = value {
                return arr.iter_mut().fold(false, |acc, item| redact_json_path(item, path) || acc);
            }
            false
        }
        Some((head, tail)) => {
            if let serde_json::Value::Object(map) = value {
                if let Some(child) = map.get_mut(head) {
                    return redact_json_path(child, tail);
                }
            } else if let serde_json::Value::Array(arr) = value {
                return arr.iter_mut().fold(false, |acc, item| redact_json_path(item, path) || acc);
            }
            false
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample() -> StoredResponse {
        StoredResponse {
            ran_at: "2026-05-21T14:30:00Z".into(),
            environment: Some("local".into()),
            case: None,
            status: 200,
            status_text: "OK".into(),
            headers: [
                ("content-type".to_string(), "application/json".to_string()),
                ("authorization".to_string(), "Bearer tok123".to_string()),
            ]
            .into(),
            body: r#"{"id":"usr_1","token":"secret","name":"Ada"}"#.into(),
            duration_ms: 184,
            url: "https://example.com/users/usr_1".into(),
        }
    }

    #[test]
    fn round_trip_save_load() {
        let tmp = TempDir::new().unwrap();
        let resp = sample();
        save_latest_response(tmp.path(), "users.get", &resp).unwrap();
        let loaded = load_latest_response(tmp.path(), "users.get")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.status, 200);
        assert_eq!(loaded.ran_at, "2026-05-21T14:30:00Z");
        assert_eq!(loaded.environment, Some("local".into()));
        assert_eq!(loaded.body, resp.body);
    }

    #[test]
    fn load_missing_returns_none() {
        let tmp = TempDir::new().unwrap();
        assert!(load_latest_response(tmp.path(), "nonexistent").unwrap().is_none());
    }

    #[test]
    fn save_creates_directory() {
        let tmp = TempDir::new().unwrap();
        // Directory does not exist yet
        let dir = tmp.path().join(".api-almanac").join("responses");
        assert!(!dir.exists());
        save_latest_response(tmp.path(), "users.get", &sample()).unwrap();
        assert!(dir.exists());
    }

    #[test]
    fn save_overwrites_previous() {
        let tmp = TempDir::new().unwrap();
        save_latest_response(tmp.path(), "users.get", &sample()).unwrap();
        let mut updated = sample();
        updated.status = 404;
        save_latest_response(tmp.path(), "users.get", &updated).unwrap();
        let loaded = load_latest_response(tmp.path(), "users.get")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.status, 404);
    }

    #[test]
    fn redact_header() {
        let resp = apply_redaction(sample(), &["headers.authorization".to_string()]);
        assert_eq!(resp.headers["authorization"], "[REDACTED]");
        assert_eq!(resp.headers["content-type"], "application/json");
    }

    #[test]
    fn redact_header_case_insensitive() {
        let resp = apply_redaction(sample(), &["headers.Authorization".to_string()]);
        // the stored key is lowercase "authorization"
        assert_eq!(resp.headers["authorization"], "[REDACTED]");
    }

    #[test]
    fn redact_json_top_level_field() {
        let resp = apply_redaction(sample(), &["json.token".to_string()]);
        let val: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert_eq!(val["token"], "[REDACTED]");
        assert_ne!(val["id"], "[REDACTED]");
        assert_ne!(val["name"], "[REDACTED]");
    }

    #[test]
    fn redact_nested_json_field() {
        let mut r = sample();
        r.body = r#"{"user":{"password":"s3cr3t","name":"Ada"}}"#.into();
        let resp = apply_redaction(r, &["json.user.password".to_string()]);
        let val: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert_eq!(val["user"]["password"], "[REDACTED]");
        assert_eq!(val["user"]["name"], "Ada");
    }

    #[test]
    fn redact_unknown_rule_is_silent() {
        let resp = apply_redaction(sample(), &["json.nonexistent".to_string()]);
        assert_eq!(resp.body, sample().body);
    }

    #[test]
    fn sanitize_id_replaces_special_chars() {
        assert_eq!(sanitize_id("users/get"), "users-get");
        assert_eq!(sanitize_id("auth.login"), "auth.login");
        assert_eq!(sanitize_id("a:b"), "a-b");
    }

    #[test]
    fn now_iso8601_looks_valid() {
        let ts = now_iso8601();
        assert!(ts.contains('T'), "{ts}");
        assert!(ts.ends_with('Z'), "{ts}");
        assert_eq!(ts.len(), 20);
    }
}
