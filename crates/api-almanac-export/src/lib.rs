use api_almanac_model::{BodyKind, RequestBody, RequestDef};
use api_almanac_store::StoredResponse;

// ── Public API ─────────────────────────────────────────────────────────────

/// Generate a Markdown notebook string for a single request definition.
///
/// - `sketch` — pre-rendered YAML from a saved TypeSketch file (optional)
/// - `last_response` — the most recently saved response (optional)
pub fn render_request_md(
    req: &RequestDef,
    sketch: Option<&str>,
    last_response: Option<&StoredResponse>,
) -> String {
    let mut out = String::new();

    // ── Title ──────────────────────────────────────────────────────────────
    out.push_str(&format!("# {}\n", req.name));

    // Tags
    if !req.tags.is_empty() {
        let tags = req.tags.iter().map(|t| format!("`{t}`")).collect::<Vec<_>>().join(" ");
        out.push_str(&format!("\n{tags}\n"));
    }

    // Notes
    if let Some(notes) = &req.notes {
        let trimmed = notes.trim();
        if !trimmed.is_empty() {
            out.push('\n');
            out.push_str(trimmed);
            out.push('\n');
        }
    }

    // ── Request ────────────────────────────────────────────────────────────
    out.push_str("\n---\n\n## Request\n\n");

    // HTTP block: method + URL + headers
    let mut http_lines = vec![format!("{} {}", req.method, req.url)];
    let mut sorted_headers: Vec<(&String, &String)> = req.headers.iter().collect();
    sorted_headers.sort_by_key(|(k, _)| k.to_lowercase());
    for (k, v) in sorted_headers {
        http_lines.push(format!("{k}: {v}"));
    }
    out.push_str("```http\n");
    out.push_str(&http_lines.join("\n"));
    out.push_str("\n```\n");

    // Query parameters
    if !req.query.is_empty() {
        let mut sorted_q: Vec<(&String, &String)> = req.query.iter().collect();
        sorted_q.sort_by_key(|(k, _)| k.as_str());
        let pairs = sorted_q.iter()
            .map(|(k, v)| format!("`{k}={v}`"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("\nQuery parameters: {pairs}\n"));
    }

    // Body
    if let Some(body) = &req.body {
        let (lang, content) = body_to_block(body);
        out.push_str(&format!("\n```{lang}\n{content}\n```\n"));
    }

    // ── Cases ──────────────────────────────────────────────────────────────
    if !req.cases.is_empty() {
        out.push_str("\n---\n\n## Cases\n\n");

        // Collect all variable names across all cases
        let mut all_vars: Vec<String> = req.cases.values()
            .flat_map(|vars| vars.keys().cloned())
            .collect();
        all_vars.sort();
        all_vars.dedup();

        let header_cols = all_vars.iter()
            .map(|v| format!(" `{v}` "))
            .collect::<Vec<_>>()
            .join("|");
        let sep_cols = all_vars.iter().map(|_| ":---|").collect::<String>();
        out.push_str(&format!("| Case |{header_cols}|\n"));
        out.push_str(&format!("|:-----|{sep_cols}\n"));

        let mut case_names: Vec<&String> = req.cases.keys().collect();
        case_names.sort();
        for name in case_names {
            let vars = &req.cases[name];
            let cells = all_vars.iter()
                .map(|v| format!(" {} ", vars.get(v).map(String::as_str).unwrap_or("")))
                .collect::<Vec<_>>()
                .join("|");
            out.push_str(&format!("| {name} |{cells}|\n"));
        }
    }

    // ── Expectations ───────────────────────────────────────────────────────
    if let Some(expect) = &req.expect {
        out.push_str("\n---\n\n## Expectations\n\n");
        if let Ok(yaml) = serde_yaml::to_string(expect) {
            out.push_str("```yaml\n");
            out.push_str(&yaml);
            out.push_str("```\n");
        }
    }

    // ── Last response ──────────────────────────────────────────────────────
    if let Some(resp) = last_response {
        out.push_str("\n---\n\n## Last response\n\n");

        // HTTP response block: status line + headers + blank line + body
        out.push_str(&format!(
            "```http\nHTTP/1.1 {} {}\n",
            resp.status, resp.status_text
        ));
        let mut sorted: Vec<_> = resp.headers.iter().collect();
        sorted.sort_by_key(|(k, _)| k.to_lowercase());
        for (k, v) in sorted {
            out.push_str(&format!("{k}: {v}\n"));
        }
        out.push('\n');
        // Pretty-print JSON bodies; leave others as-is
        let body_display = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resp.body) {
            serde_json::to_string_pretty(&v).unwrap_or_else(|_| resp.body.clone())
        } else {
            resp.body.clone()
        };
        out.push_str(&body_display);
        out.push_str("\n```\n");

        // Metadata line
        let mut meta = format!("*{}*", resp.ran_at);
        if let Some(env) = &resp.environment {
            meta.push_str(&format!(" · env `{env}`"));
        }
        if let Some(case) = &resp.case {
            meta.push_str(&format!(" · case `{case}`"));
        }
        meta.push_str(&format!(" · {} ms", resp.duration_ms));
        out.push('\n');
        out.push_str(&meta);
        out.push('\n');
    }

    // ── Observed response sketch ───────────────────────────────────────────
    if let Some(sketch) = sketch {
        let trimmed = sketch.trim_end();
        if !trimmed.is_empty() {
            out.push_str("\n---\n\n## Observed response sketch\n\n");
            out.push_str(&format!("```yaml\n{trimmed}\n```\n"));
        }
    }

    out
}

// ── Internal helpers ───────────────────────────────────────────────────────

fn body_to_block(body: &RequestBody) -> (&'static str, String) {
    match body.kind {
        BodyKind::Json => {
            let content = serde_json::to_string_pretty(&body.value)
                .unwrap_or_else(|_| serde_yaml::to_string(&body.value).unwrap_or_default());
            ("json", content)
        }
        _ => {
            let content = match &body.value {
                serde_yaml::Value::String(s) => s.clone(),
                other => serde_yaml::to_string(other).unwrap_or_default(),
            };
            ("text", content)
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn minimal() -> RequestDef {
        RequestDef {
            id: "users.get".into(),
            name: "Get user".into(),
            method: "GET".into(),
            url: "{{base_url}}/users/{{id}}".into(),
            tags: vec![],
            headers: HashMap::new(),
            query: HashMap::new(),
            body: None,
            cases: HashMap::new(),
            expect: None,
            capture: HashMap::new(),
            redact: vec![],
            notes: None,
        }
    }

    #[test]
    fn renders_title_and_request() {
        let md = render_request_md(&minimal(), None, None);
        assert!(md.contains("# Get user"), "{md}");
        assert!(md.contains("## Request"), "{md}");
        assert!(md.contains("GET {{base_url}}/users/{{id}}"), "{md}");
    }

    #[test]
    fn renders_tags_and_notes() {
        let mut req = minimal();
        req.tags = vec!["users".into(), "read".into()];
        req.notes = Some("Run login first.".into());
        let md = render_request_md(&req, None, None);
        assert!(md.contains("`users`"), "{md}");
        assert!(md.contains("Run login first."), "{md}");
    }

    #[test]
    fn omits_optional_sections_when_absent() {
        let md = render_request_md(&minimal(), None, None);
        assert!(!md.contains("## Cases"), "{md}");
        assert!(!md.contains("## Expectations"), "{md}");
        assert!(!md.contains("## Observed response sketch"), "{md}");
    }

    #[test]
    fn renders_cases_table() {
        let mut req = minimal();
        let mut case_a = HashMap::new();
        case_a.insert("user.id".into(), "usr_1".into());
        let mut case_b = HashMap::new();
        case_b.insert("user.id".into(), "usr_2".into());
        req.cases.insert("alice".into(), case_a);
        req.cases.insert("bob".into(), case_b);
        let md = render_request_md(&req, None, None);
        assert!(md.contains("## Cases"), "{md}");
        assert!(md.contains("alice"), "{md}");
        assert!(md.contains("usr_1"), "{md}");
    }

    #[test]
    fn absent_case_field_renders_empty_cell() {
        let mut req = minimal();
        let mut case_a = HashMap::new();
        case_a.insert("user.name".into(), "Ada".into());
        case_a.insert("user.role".into(), "admin".into());
        let mut case_b = HashMap::new();
        case_b.insert("user.name".into(), "Grace".into());
        // case_b has no user.role
        req.cases.insert("ada".into(), case_a);
        req.cases.insert("grace".into(), case_b);
        let md = render_request_md(&req, None, None);
        assert!(md.contains("ada"), "{md}");
        assert!(md.contains("Grace"), "{md}");
    }

    #[test]
    fn renders_expectations() {
        let mut req = minimal();
        req.expect = Some(api_almanac_model::Expect {
            status: Some(200),
            time_ms: Some("< 500".into()),
            headers: HashMap::new(),
            json: HashMap::new(),
        });
        let md = render_request_md(&req, None, None);
        assert!(md.contains("## Expectations"), "{md}");
        assert!(md.contains("status:"), "{md}");
    }

    #[test]
    fn renders_sketch() {
        let md = render_request_md(&minimal(), Some("id: string\nemail: email\n"), None);
        assert!(md.contains("## Observed response sketch"), "{md}");
        assert!(md.contains("id: string"), "{md}");
        assert!(md.contains("email: email"), "{md}");
    }

    #[test]
    fn renders_query_params() {
        let mut req = minimal();
        req.query.insert("page".into(), "1".into());
        req.query.insert("limit".into(), "20".into());
        let md = render_request_md(&req, None, None);
        assert!(md.contains("Query parameters:"), "{md}");
        assert!(md.contains("`page=1`"), "{md}");
    }

    #[test]
    fn renders_json_body() {
        use api_almanac_model::RequestBody;
        let mut req = minimal();
        req.method = "POST".into();
        req.body = Some(RequestBody {
            kind: BodyKind::Json,
            value: serde_yaml::from_str(r#"name: Ada"#).unwrap(),
        });
        let md = render_request_md(&req, None, None);
        assert!(md.contains("```json"), "{md}");
        assert!(md.contains("Ada"), "{md}");
    }

    #[test]
    fn renders_last_response() {
        use api_almanac_store::StoredResponse;
        let resp = StoredResponse {
            ran_at: "2026-05-21T14:30:00Z".into(),
            environment: Some("local".into()),
            case: None,
            status: 200,
            status_text: "OK".into(),
            headers: [("content-type".to_string(), "application/json".to_string())].into(),
            body: r#"{"id":"usr_1"}"#.into(),
            duration_ms: 184,
            url: "https://example.com/users/usr_1".into(),
        };
        let md = render_request_md(&minimal(), None, Some(&resp));
        assert!(md.contains("## Last response"), "{md}");
        assert!(md.contains("HTTP/1.1 200 OK"), "{md}");
        assert!(md.contains("content-type: application/json"), "{md}");
        assert!(md.contains("usr_1"), "{md}");
        assert!(md.contains("2026-05-21T14:30:00Z"), "{md}");
        assert!(md.contains("env `local`"), "{md}");
        assert!(md.contains("184 ms"), "{md}");
    }

    #[test]
    fn omits_last_response_when_none() {
        let md = render_request_md(&minimal(), None, None);
        assert!(!md.contains("## Last response"), "{md}");
    }
}
