use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_vars() {
        let yaml = r#"
id: local
name: Local
vars:
  base_url: http://localhost:8000
  auth.token: "{{secret.LOCAL_API_TOKEN}}"
"#;
        let env: Environment = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(env.id, "local");
        assert_eq!(env.vars["base_url"], "http://localhost:8000");
        assert_eq!(env.vars["auth.token"], "{{secret.LOCAL_API_TOKEN}}");

        let serialized = serde_yaml::to_string(&env).unwrap();
        let env2: Environment = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(env2.vars.len(), env.vars.len());
    }

    #[test]
    fn round_trip_empty_vars() {
        let yaml = r#"
id: empty
name: Empty
"#;
        let env: Environment = serde_yaml::from_str(yaml).unwrap();
        assert!(env.vars.is_empty());
    }
}
