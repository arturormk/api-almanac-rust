use crate::body::{BodyKind, RequestBody};
use crate::environment::Environment;
use crate::error::ModelError;
use crate::request::{Case, RequestDef};
use crate::resolved::{ResolvedBody, ResolvedRequest};
use std::collections::HashMap;

pub struct VariableResolver {
    vars: HashMap<String, String>,
}

impl VariableResolver {
    /// Build a resolver from an environment and an optional case.
    /// Case variables override environment variables.
    pub fn new(env: &Environment, case: Option<&Case>) -> Self {
        let mut vars = env.vars.clone();
        if let Some(c) = case {
            vars.extend(c.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        Self { vars }
    }

    /// Build a resolver from a bare variable map (useful in tests).
    pub fn from_vars(vars: HashMap<String, String>) -> Self {
        Self { vars }
    }

    /// Replace all `{{key}}` occurrences with their values.
    /// After user-defined vars are substituted, any remaining `{{secret.NAME}}`
    /// tokens are resolved from `std::env::var("NAME")`. Unknown placeholders
    /// (including secret refs with no matching OS env var) are left as-is.
    pub fn resolve_str(&self, input: &str) -> String {
        let mut result = input.to_string();
        for (key, value) in &self.vars {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }
        result = resolve_secret_refs(&result);
        result
    }

    /// Recursively resolve all string leaves in a YAML value.
    pub fn resolve_value(&self, value: &serde_yaml::Value) -> serde_yaml::Value {
        match value {
            serde_yaml::Value::String(s) => serde_yaml::Value::String(self.resolve_str(s)),
            serde_yaml::Value::Mapping(m) => {
                let resolved = m
                    .iter()
                    .map(|(k, v)| (k.clone(), self.resolve_value(v)))
                    .collect();
                serde_yaml::Value::Mapping(resolved)
            }
            serde_yaml::Value::Sequence(seq) => {
                serde_yaml::Value::Sequence(seq.iter().map(|v| self.resolve_value(v)).collect())
            }
            other => other.clone(),
        }
    }

    /// Produce a fully resolved request ready for HTTP execution.
    pub fn resolve_request(&self, req: &RequestDef) -> Result<ResolvedRequest, ModelError> {
        let method = self.resolve_str(&req.method);
        let url = self.resolve_str(&req.url);
        let headers = req
            .headers
            .iter()
            .map(|(k, v)| (self.resolve_str(k), self.resolve_str(v)))
            .collect();
        let query = req
            .query
            .iter()
            .map(|(k, v)| (self.resolve_str(k), self.resolve_str(v)))
            .collect();
        let body = req
            .body
            .as_ref()
            .map(|b| self.resolve_body(b))
            .transpose()?;

        Ok(ResolvedRequest {
            id: req.id.clone(),
            name: req.name.clone(),
            method,
            url,
            headers,
            query,
            body,
        })
    }

    fn resolve_body(&self, body: &RequestBody) -> Result<ResolvedBody, ModelError> {
        let resolved_value = self.resolve_value(&body.value);
        let content = match body.kind {
            BodyKind::Json => serde_json::to_string_pretty(&resolved_value)?,
            BodyKind::Text => match &resolved_value {
                serde_yaml::Value::String(s) => s.clone(),
                other => serde_yaml::to_string(other)
                    .map_err(|e| ModelError::Yaml { path: "<body>".into(), source: e })?,
            },
            BodyKind::Form => match &resolved_value {
                serde_yaml::Value::Mapping(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let key = yaml_as_str(k);
                        let val = yaml_as_str(v);
                        format!("{}={}", key, val)
                    })
                    .collect::<Vec<_>>()
                    .join("&"),
                _ => String::new(),
            },
        };
        let content_type = match body.kind {
            BodyKind::Json => "application/json",
            BodyKind::Text => "text/plain",
            BodyKind::Form => "application/x-www-form-urlencoded",
        };
        Ok(ResolvedBody { kind: body.kind.clone(), content, content_type })
    }
}

/// Expand any `{{secret.NAME}}` tokens by reading `std::env::var("NAME")`.
/// Unresolvable secret refs are left intact.
fn resolve_secret_refs(s: &str) -> String {
    const PREFIX: &str = "{{secret.";
    if !s.contains(PREFIX) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find(PREFIX) {
        out.push_str(&rest[..start]);
        rest = &rest[start + PREFIX.len()..];
        if let Some(end) = rest.find("}}") {
            let name = &rest[..end];
            if let Ok(val) = std::env::var(name) {
                out.push_str(&val);
            } else {
                out.push_str(PREFIX);
                out.push_str(name);
                out.push_str("}}");
            }
            rest = &rest[end + 2..];
        } else {
            out.push_str(PREFIX);
        }
    }
    out.push_str(rest);
    out
}

fn yaml_as_str(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::{BodyKind, RequestBody};

    fn env_with_vars(pairs: &[(&str, &str)]) -> Environment {
        Environment {
            id: "test".into(),
            name: "Test".into(),
            parent: None,
            vars: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn case_with_vars(pairs: &[(&str, &str)]) -> Case {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn resolves_simple_string() {
        let env = env_with_vars(&[("base_url", "http://localhost:8000")]);
        let resolver = VariableResolver::new(&env, None);
        assert_eq!(
            resolver.resolve_str("{{base_url}}/users"),
            "http://localhost:8000/users"
        );
    }

    #[test]
    fn unknown_placeholder_left_intact() {
        let env = env_with_vars(&[]);
        let resolver = VariableResolver::new(&env, None);
        assert_eq!(resolver.resolve_str("{{unknown}}"), "{{unknown}}");
    }

    #[test]
    fn case_overrides_env() {
        let env = env_with_vars(&[("user.email", "env@example.com")]);
        let case = case_with_vars(&[("user.email", "case@example.com")]);
        let resolver = VariableResolver::new(&env, Some(&case));
        assert_eq!(
            resolver.resolve_str("{{user.email}}"),
            "case@example.com"
        );
    }

    #[test]
    fn resolves_multiple_placeholders() {
        let resolver = VariableResolver::from_vars(
            [("base_url", "https://api.example.com"), ("version", "v1")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        assert_eq!(
            resolver.resolve_str("{{base_url}}/{{version}}/users"),
            "https://api.example.com/v1/users"
        );
    }

    #[test]
    fn resolves_json_body() {
        let resolver = VariableResolver::from_vars(
            [("user.name", "Ada Lovelace"), ("user.email", "ada@example.com")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        let body = RequestBody {
            kind: BodyKind::Json,
            value: serde_yaml::from_str(
                r#"
name: "{{user.name}}"
email: "{{user.email}}"
"#,
            )
            .unwrap(),
        };
        let resolved = resolver.resolve_body(&body).unwrap();
        assert_eq!(resolved.kind, BodyKind::Json);
        let json: serde_json::Value = serde_json::from_str(&resolved.content).unwrap();
        assert_eq!(json["name"], "Ada Lovelace");
        assert_eq!(json["email"], "ada@example.com");
    }

    #[test]
    fn secret_ref_resolved_from_env() {
        std::env::set_var("ALMANAC_TEST_SECRET_42", "supersecret");
        let resolver = VariableResolver::from_vars(Default::default());
        assert_eq!(
            resolver.resolve_str("Bearer {{secret.ALMANAC_TEST_SECRET_42}}"),
            "Bearer supersecret"
        );
        std::env::remove_var("ALMANAC_TEST_SECRET_42");
    }

    #[test]
    fn unknown_secret_ref_left_intact() {
        std::env::remove_var("ALMANAC_TEST_MISSING_VAR_XYZ");
        let resolver = VariableResolver::from_vars(Default::default());
        assert_eq!(
            resolver.resolve_str("{{secret.ALMANAC_TEST_MISSING_VAR_XYZ}}"),
            "{{secret.ALMANAC_TEST_MISSING_VAR_XYZ}}"
        );
    }

    #[test]
    fn secret_ref_via_env_var_in_environment() {
        std::env::set_var("ALMANAC_TEST_TOKEN_99", "tok-abc");
        let env = env_with_vars(&[("auth.token", "{{secret.ALMANAC_TEST_TOKEN_99}}")]);
        let resolver = VariableResolver::new(&env, None);
        // {{auth.token}} expands to {{secret.ALMANAC_TEST_TOKEN_99}}, then secret resolves
        assert_eq!(resolver.resolve_str("Bearer {{auth.token}}"), "Bearer tok-abc");
        std::env::remove_var("ALMANAC_TEST_TOKEN_99");
    }

    #[test]
    fn resolves_full_request() {
        let env = env_with_vars(&[("base_url", "https://api.example.com")]);
        let case = case_with_vars(&[("user_id", "usr_123")]);
        let resolver = VariableResolver::new(&env, Some(&case));

        let req: RequestDef = serde_yaml::from_str(
            r#"
id: users.get
name: Get user
method: GET
url: "{{base_url}}/users/{{user_id}}"
headers:
  Accept: application/json
"#,
        )
        .unwrap();

        let resolved = resolver.resolve_request(&req).unwrap();
        assert_eq!(resolved.url, "https://api.example.com/users/usr_123");
        assert_eq!(resolved.headers["Accept"], "application/json");
        assert!(resolved.body.is_none());
    }
}
