use api_almanac_model::Expect;
use crate::response::HttpResponse;
use serde::Serialize;
use std::collections::HashMap;
use roxmltree::Document as XmlDocument;

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

    if !expect.xml.is_empty() {
        match XmlDocument::parse(&response.body) {
            Ok(doc) => {
                for (path, rule) in &expect.xml {
                    let actual = get_xml_path(&doc, path);
                    let (passed, expected) = check_string_rule(rule, actual.as_deref());
                    checks.push(Check {
                        name: format!("xml.{path}"),
                        passed,
                        expected,
                        actual,
                    });
                }
            }
            Err(_) => {
                for path in expect.xml.keys() {
                    checks.push(Check {
                        name: format!("xml.{path}"),
                        passed: false,
                        expected: "XML response body".into(),
                        actual: Some("body is not valid XML".into()),
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

/// Navigate an XML document using a dot-notation path.
///
/// Path format mirrors `sketch_xml`: first segment is the root tag name, subsequent
/// segments are child tag names, `@attr` reads an attribute, and `[N]` selects the
/// Nth same-tag sibling (e.g. `response.items.item[0].name`).
pub fn get_xml_path(doc: &XmlDocument, path: &str) -> Option<String> {
    let root = doc.root_element();
    let (head, rest) = split_first_segment(path);
    if head != root.tag_name().name() {
        return None;
    }
    match rest {
        None => node_text(root),
        Some(r) => navigate_xml(root, r),
    }
}

fn navigate_xml<'a>(node: roxmltree::Node<'a, 'a>, path: &str) -> Option<String> {
    let (head, rest) = split_first_segment(path);

    if let Some(attr_name) = head.strip_prefix('@') {
        return node.attribute(attr_name).map(str::to_string);
    }

    let (tag, idx) = parse_tag_index(head);
    let children: Vec<_> = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == tag)
        .collect();
    let child = match idx {
        Some(i) => *children.get(i)?,
        None => *children.first()?,
    };

    match rest {
        None => node_text(child),
        Some(r) => navigate_xml(child, r),
    }
}

/// Split a dot-notation path at the first `.` that is not inside `[...]`.
fn split_first_segment(path: &str) -> (&str, Option<&str>) {
    let bytes = path.as_bytes();
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => depth += 1,
            b']' => depth = depth.saturating_sub(1),
            b'.' if depth == 0 => return (&path[..i], Some(&path[i + 1..])),
            _ => {}
        }
    }
    (path, None)
}

fn parse_tag_index(segment: &str) -> (&str, Option<usize>) {
    if let Some(bracket) = segment.find('[') {
        if let Some(close) = segment[bracket..].find(']') {
            let tag = &segment[..bracket];
            let idx_str = &segment[bracket + 1..bracket + close];
            if let Ok(i) = idx_str.parse::<usize>() {
                return (tag, Some(i));
            }
        }
    }
    (segment, None)
}

fn node_text(node: roxmltree::Node) -> Option<String> {
    let text: String = node
        .children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string();
    if text.is_empty() { None } else { Some(text) }
}

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

    fn xml_resp(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: [("content-type".into(), "application/xml".into())]
                .into_iter()
                .collect(),
            body: body.into(),
            duration_ms: 50,
            url: "http://example.com".into(),
        }
    }

    #[test]
    fn xml_leaf_equals() {
        let expect = Expect {
            xml: [("response.name".into(), "equals Ada".into())].into_iter().collect(),
            ..Default::default()
        };
        let checks = run_checks(&expect, &xml_resp("<response><name>Ada</name></response>"));
        assert!(checks[0].passed, "{:?}", checks[0]);
    }

    #[test]
    fn xml_attribute_exists() {
        let expect = Expect {
            xml: [("response.user.@id".into(), "exists".into())].into_iter().collect(),
            ..Default::default()
        };
        let checks = run_checks(&expect, &xml_resp(r#"<response><user id="42"><name>Ada</name></user></response>"#));
        assert!(checks[0].passed, "{:?}", checks[0]);
    }

    #[test]
    fn xml_array_index() {
        let expect = Expect {
            xml: [("items.item[1].name".into(), "equals Grace".into())].into_iter().collect(),
            ..Default::default()
        };
        let body = "<items><item><name>Ada</name></item><item><name>Grace</name></item></items>";
        let checks = run_checks(&expect, &xml_resp(body));
        assert!(checks[0].passed, "{:?}", checks[0]);
    }

    #[test]
    fn xml_invalid_body_fails() {
        let expect = Expect {
            xml: [("root.field".into(), "exists".into())].into_iter().collect(),
            ..Default::default()
        };
        let checks = run_checks(&expect, &xml_resp("not xml"));
        assert!(!checks[0].passed);
    }
}
