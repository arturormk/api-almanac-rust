use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyKind {
    Json,
    Text,
    Form,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    pub kind: BodyKind,
    pub value: serde_yaml::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_json_body() {
        let yaml = r#"
kind: json
value:
  name: "{{user.name}}"
  role: admin
"#;
        let body: RequestBody = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(body.kind, BodyKind::Json);

        let serialized = serde_yaml::to_string(&body).unwrap();
        let body2: RequestBody = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(body2.kind, body.kind);
    }

    #[test]
    fn round_trip_text_body() {
        let yaml = r#"
kind: text
value: "plain text body"
"#;
        let body: RequestBody = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(body.kind, BodyKind::Text);
    }
}
