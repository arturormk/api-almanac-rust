use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlmanacProject {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_minimal() {
        let yaml = r#"
id: my-api
name: My API
"#;
        let project: AlmanacProject = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(project.id, "my-api");
        assert_eq!(project.name, "My API");
        assert!(project.description.is_none());

        let serialized = serde_yaml::to_string(&project).unwrap();
        let project2: AlmanacProject = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(project2.id, project.id);
    }

    #[test]
    fn round_trip_with_description() {
        let yaml = r#"
id: my-api
name: My API
description: A sample API project
"#;
        let project: AlmanacProject = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(project.description, Some("A sample API project".to_string()));
    }
}
