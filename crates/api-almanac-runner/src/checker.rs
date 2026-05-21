use api_almanac_model::Expect;
use crate::response::HttpResponse;
use serde::Serialize;
use std::collections::HashMap;

// ── Result types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub expected: String,
    pub actual: Option<String>,
}

// ── Check evaluation ───────────────────────────────────────────────────────

pub fn run_checks(expect: &Expect, response: &HttpResponse) -> Vec<Check> {
    let mut checks = Vec::new();

    if let Some(expected_status) = expect.status {
        checks.push(Check {
            name: "status".into(),
            passed: response.status == expected_status,
            expected: expected_status.to_string(),
            actual: Some(response.status.to_string()),
        });
    }

    if let Some(ref rule) = expect.time_ms {
        let (passed, label) = check_numeric(rule, response.duration_ms as f64);
        checks.push(Check {
            name: "time_ms".into(),
            passed,
            expected: label,
            actual: Some(response.duration_ms.to_string()),
        });
    }

    for (header_name, rule) in &expect.headers {
        let actual = response
            .headers
            .get(&header_name.to_lowercase())
            .or_else(|| response.headers.get(header_name))
            .cloned();
        let (passed, expected) = check_string_rule(rule, actual.as_deref());
        checks.push(Check {
            name: format!("headers.{header_name}"),
            passed,
            expected,
            actual,
        });
    }

    if !expect.json.is_empty() {
        match serde_json::from_str::<serde_json::Value>(&response.body) {
            Ok(json_val) => {
                for (path, rule) in &expect.json {
                    let actual = get_json_path(&json_val, path);
                    let (passed, expected) = check_string_rule(rule, actual.as_deref());
                    checks.push(Check {
                        name: format!("json.{path}"),
                        passed,
                        expected,
                        actual,
                    });
                }
            }
            Err(_) => {
                for path in expect.json.keys() {
                    checks.push(Check {
                        name: format!("json.{path}"),
                        passed: false,
                        expected: "JSON response body".into(),
                        actual: Some("body is not valid JSON".into()),
                    });
                }
            }
        }
    }

    checks
}

// ── Capture extraction ─────────────────────────────────────────────────────

/// Extract captured values from a response according to a capture map.
/// Supported paths: `json.<dot.path>`, `header.<name>`, `headers.<name>`.
pub fn apply_captures(
    captures: &HashMap<String, String>,
    response: &HttpResponse,
) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let json = serde_json::from_str::<serde_json::Value>(&response.body).ok();

    for (var_name, path) in captures {
        let value = if let Some(json_path) = path.strip_prefix("json.") {
            json.as_ref().and_then(|j| get_json_path(j, json_path))
        } else if let Some(hdr) = path
            .strip_prefix("headers.")
            .or_else(|| path.strip_prefix("header."))
        {
            response
                .headers
                .get(&hdr.to_lowercase())
                .or_else(|| response.headers.get(hdr))
                .cloned()
        } else {
            None
        };

        if let Some(v) = value {
            result.insert(var_name.clone(), v);
        }
    }

    result
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Navigate a JSON value using a simple dot-notation path (e.g. `"user.email"`).
/// Array indexing with `key[0]` is also supported.
pub fn get_json_path(val: &serde_json::Value, path: &str) -> Option<String> {
    let mut current = val;
    for segment in path.split('.') {
        if let Some((key, idx_str)) = segment.split_once('[') {
            if let Some(idx_str) = idx_str.strip_suffix(']') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    current = current.get(key)?.get(idx)?;
                    continue;
                }
            }
        }
        current = current.get(segment)?;
    }
    scalar_to_string(current)
}

fn scalar_to_string(val: &serde_json::Value) -> Option<String> {
    match val {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Null => Some("null".into()),
        _ => Some(val.to_string()),
    }
}

fn check_numeric(rule: &str, actual: f64) -> (bool, String) {
    let r = rule.trim();
    if let Some(n) = r.strip_prefix("<=").map(str::trim).and_then(|s| s.parse::<f64>().ok()) {
        return (actual <= n, format!("<= {n}"));
    }
    if let Some(n) = r.strip_prefix('<').map(str::trim).and_then(|s| s.parse::<f64>().ok()) {
        return (actual < n, format!("< {n}"));
    }
    if let Some(n) = r.strip_prefix(">=").map(str::trim).and_then(|s| s.parse::<f64>().ok()) {
        return (actual >= n, format!(">= {n}"));
    }
    if let Some(n) = r.strip_prefix('>').map(str::trim).and_then(|s| s.parse::<f64>().ok()) {
        return (actual > n, format!("> {n}"));
    }
    if let Ok(n) = r.parse::<f64>() {
        return (actual == n, format!("= {n}"));
    }
    (false, format!("(invalid rule: {r})"))
}

fn check_string_rule(rule: &str, actual: Option<&str>) -> (bool, String) {
    match rule.trim() {
        "exists" => (actual.is_some(), "exists".into()),
        r if r.starts_with("equals ") => {
            let expected = &r["equals ".len()..];
            (actual == Some(expected), format!("equals {expected}"))
        }
        r if r.starts_with("contains ") => {
            let expected = &r["contains ".len()..];
            let passed = actual.map_or(false, |a| a.contains(expected));
            (passed, format!("contains {expected}"))
        }
        r => {
            (actual == Some(r), format!("equals {r}"))
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(status: u16, body: &str, duration_ms: u64) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".into(),
            headers: [("content-type".into(), "application/json".into())]
                .into_iter()
                .collect(),
            body: body.into(),
            duration_ms,
            url: "http://example.com".into(),
        }
    }

    #[test]
    fn status_pass() {
        let expect = Expect { status: Some(200), ..Default::default() };
        let checks = run_checks(&expect, &resp(200, "{}", 50));
        assert!(checks[0].passed);
    }

    #[test]
    fn status_fail() {
        let expect = Expect { status: Some(201), ..Default::default() };
        let checks = run_checks(&expect, &resp(200, "{}", 50));
        assert!(!checks[0].passed);
    }

    #[test]
    fn time_ms_pass() {
        let expect = Expect { time_ms: Some("< 500".into()), ..Default::default() };
        let checks = run_checks(&expect, &resp(200, "{}", 100));
        assert!(checks[0].passed);
    }

    #[test]
    fn time_ms_fail() {
        let expect = Expect { time_ms: Some("< 100".into()), ..Default::default() };
        let checks = run_checks(&expect, &resp(200, "{}", 200));
        assert!(!checks[0].passed);
    }

    #[test]
    fn header_contains_pass() {
        let expect = Expect {
            headers: [("content-type".into(), "contains application/json".into())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let checks = run_checks(&expect, &resp(200, "{}", 50));
        assert!(checks[0].passed, "expected header check to pass");
    }

    #[test]
    fn json_exists_pass() {
        let expect = Expect {
            json: [("id".into(), "exists".into())].into_iter().collect(),
            ..Default::default()
        };
        let checks = run_checks(&expect, &resp(200, r#"{"id":"usr_1"}"#, 50));
        assert!(checks[0].passed);
    }

    #[test]
    fn json_equals_pass() {
        let expect = Expect {
            json: [("name".into(), "equals Ada".into())].into_iter().collect(),
            ..Default::default()
        };
        let checks = run_checks(&expect, &resp(200, r#"{"name":"Ada"}"#, 50));
        assert!(checks[0].passed);
    }

    #[test]
    fn json_nested_path() {
        let expect = Expect {
            json: [("user.email".into(), "equals ada@example.com".into())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let body = r#"{"user":{"email":"ada@example.com"}}"#;
        let checks = run_checks(&expect, &resp(200, body, 50));
        assert!(checks[0].passed);
    }

    #[test]
    fn capture_json_field() {
        let mut captures = HashMap::new();
        captures.insert("auth.token".into(), "json.access_token".into());
        let response = resp(200, r#"{"access_token":"tok_abc"}"#, 50);
        let captured = apply_captures(&captures, &response);
        assert_eq!(captured["auth.token"], "tok_abc");
    }

    #[test]
    fn capture_header() {
        let mut captures = HashMap::new();
        captures.insert("x_request_id".into(), "header.content-type".into());
        let response = resp(200, "{}", 50);
        let captured = apply_captures(&captures, &response);
        assert_eq!(captured["x_request_id"], "application/json");
    }
}
