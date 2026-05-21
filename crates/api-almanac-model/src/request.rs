use crate::body::RequestBody;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A named set of variable overrides for a request.
pub type Case = HashMap<String, String>;

/// Simple assertions checked against the HTTP response after a run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Expect {
    /// Expected HTTP status code (exact match).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// Maximum response time rule, e.g. `"< 500"` or `"<= 1000"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_ms: Option<String>,
    /// Header checks: header name → rule string (`"contains X"`, `"equals X"`).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    /// JSON body checks: dot-notation path → rule string (`"exists"`, `"equals X"`, `"contains X"`).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub json: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub query: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<RequestBody>,
    #[serde(default)]
    pub cases: HashMap<String, Case>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect: Option<Expect>,
    #[serde(default)]
    pub capture: HashMap<String, String>,
    #[serde(default)]
    pub redact: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_minimal() {
        let yaml = r#"
id: users.get
name: Get user
method: GET
url: "{{base_url}}/users/{{user_id}}"
"#;
        let req: RequestDef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.id, "users.get");
        assert_eq!(req.method, "GET");
        assert!(req.headers.is_empty());
        assert!(req.body.is_none());
        assert!(req.cases.is_empty());

        let serialized = serde_yaml::to_string(&req).unwrap();
        let req2: RequestDef = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(req2.url, req.url);
    }

    #[test]
    fn round_trip_full() {
        let yaml = r#"
id: users.create
name: Create user
tags:
  - users
  - write
method: POST
url: "{{base_url}}/users"
headers:
  Authorization: "Bearer {{auth.token}}"
  Content-Type: application/json
body:
  kind: json
  value:
    name: "{{user.name}}"
    email: "{{user.email}}"
cases:
  normal-user:
    user.name: Ada Lovelace
    user.email: ada@example.com
  admin-user:
    user.name: Grace Hopper
    user.email: grace@example.com
capture:
  created_user.id: json.id
redact:
  - headers.Authorization
notes: Run login first to get auth.token
"#;
        let req: RequestDef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.tags, vec!["users", "write"]);
        assert_eq!(req.headers["Content-Type"], "application/json");
        assert!(req.body.is_some());
        assert_eq!(req.cases.len(), 2);
        assert!(req.cases["normal-user"]["user.name"] == "Ada Lovelace");
        assert_eq!(req.capture["created_user.id"], "json.id");
        assert_eq!(req.redact, vec!["headers.Authorization"]);
        assert_eq!(req.notes, Some("Run login first to get auth.token".to_string()));
    }
}
