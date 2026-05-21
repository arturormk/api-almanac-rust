use crate::environment::Environment;
use crate::error::ModelError;
use crate::project::AlmanacProject;
use crate::request::RequestDef;
use std::path::{Path, PathBuf};

/// A request paired with the path of its YAML file relative to the project root.
#[derive(Debug, Clone)]
pub struct RequestEntry {
    /// Path relative to the project root, e.g. `requests/auth/login.yaml`.
    pub file_path: PathBuf,
    pub request: RequestDef,
}

impl RequestEntry {
    /// Folder component relative to `requests/`, e.g. `"auth"` or `""` for root.
    pub fn folder(&self) -> String {
        let path_str = self.file_path.to_string_lossy();
        let rel = path_str
            .strip_prefix("requests/")
            .or_else(|| path_str.strip_prefix("requests\\"))
            .unwrap_or(&path_str);
        let parent = Path::new(rel)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("");
        parent.replace('\\', "/")
    }
}

pub struct ProjectLoader {
    root: PathBuf,
}

impl ProjectLoader {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load_project(&self) -> Result<AlmanacProject, ModelError> {
        let path = self.root.join("almanac.yaml");
        if !path.exists() {
            return Err(ModelError::ProjectNotFound(path.display().to_string()));
        }
        load_yaml(&path)
    }

    pub fn load_environments(&self) -> Result<Vec<Environment>, ModelError> {
        let dir = self.root.join("environments");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut envs = Vec::new();
        for path in yaml_files_in(&dir)? {
            envs.push(load_yaml(&path)?);
        }
        Ok(envs)
    }

    /// Load all request YAML files from `requests/**/*.yaml`, each paired with
    /// its path relative to the project root.
    pub fn load_requests(&self) -> Result<Vec<RequestEntry>, ModelError> {
        let dir = self.root.join("requests");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut abs_paths = Vec::new();
        collect_yaml_files(&dir, &mut abs_paths)?;
        let mut entries = Vec::new();
        for abs_path in abs_paths {
            let file_path = abs_path
                .strip_prefix(&self.root)
                .unwrap_or(&abs_path)
                .to_path_buf();
            let request: RequestDef = load_yaml(&abs_path)?;
            entries.push(RequestEntry { file_path, request });
        }
        Ok(entries)
    }

    /// Write a request definition to the file at `relative_path` (relative to the
    /// project root). The parent directory is created if it does not exist.
    pub fn save_request(&self, relative_path: &Path, request: &RequestDef) -> Result<(), ModelError> {
        let abs_path = self.root.join(relative_path);
        if let Some(parent) = abs_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let yaml = serde_yaml::to_string(request)
            .map_err(|e| ModelError::Yaml { path: abs_path.display().to_string(), source: e })?;
        std::fs::write(&abs_path, yaml)?;
        Ok(())
    }

    /// Write an environment to `environments/{env.id}.yaml`. Creates the directory if needed.
    pub fn save_environment(&self, env: &Environment) -> Result<(), ModelError> {
        let dir = self.root.join("environments");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.yaml", env.id));
        let yaml = serde_yaml::to_string(env)
            .map_err(|e| ModelError::Yaml { path: path.display().to_string(), source: e })?;
        std::fs::write(&path, yaml)?;
        Ok(())
    }

    /// Remove `environments/{env_id}.yaml`. No-op if the file does not exist.
    pub fn delete_environment(&self, env_id: &str) -> Result<(), ModelError> {
        let path = self.root.join("environments").join(format!("{env_id}.yaml"));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}

fn load_yaml<T>(path: &Path) -> Result<T, ModelError>
where
    T: serde::de::DeserializeOwned,
{
    let text = std::fs::read_to_string(path)?;
    serde_yaml::from_str(&text).map_err(|e| ModelError::Yaml {
        path: path.display().to_string(),
        source: e,
    })
}

/// Return all `.yaml`/`.yml` files directly inside a directory (non-recursive).
fn yaml_files_in(dir: &Path) -> Result<Vec<PathBuf>, ModelError> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() && is_yaml(&path) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Recursively collect all `.yaml`/`.yml` files under a directory.
fn collect_yaml_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ModelError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_yaml_files(&path, out)?;
        } else if path.is_file() && is_yaml(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yaml" | "yml")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn load_project_from_disk() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "almanac.yaml",
            "id: test-api\nname: Test API\ndescription: For testing\n",
        );
        let loader = ProjectLoader::new(tmp.path());
        let project = loader.load_project().unwrap();
        assert_eq!(project.id, "test-api");
        assert_eq!(project.name, "Test API");
        assert_eq!(project.description, Some("For testing".into()));
    }

    #[test]
    fn missing_project_file_returns_error() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        assert!(matches!(
            loader.load_project(),
            Err(ModelError::ProjectNotFound(_))
        ));
    }

    #[test]
    fn load_environments_from_disk() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "environments/local.yaml",
            "id: local\nname: Local\nvars:\n  base_url: http://localhost:8000\n",
        );
        write(
            tmp.path(),
            "environments/staging.yaml",
            "id: staging\nname: Staging\nvars:\n  base_url: https://staging.example.com\n",
        );
        let loader = ProjectLoader::new(tmp.path());
        let envs = loader.load_environments().unwrap();
        assert_eq!(envs.len(), 2);
        let local = envs.iter().find(|e| e.id == "local").unwrap();
        assert_eq!(local.vars["base_url"], "http://localhost:8000");
    }

    #[test]
    fn load_requests_carries_file_path() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "requests/auth/login.yaml",
            "id: auth.login\nname: Login\nmethod: POST\nurl: \"{{base_url}}/auth/login\"\n",
        );
        write(
            tmp.path(),
            "requests/users/list.yaml",
            "id: users.list\nname: List users\nmethod: GET\nurl: \"{{base_url}}/users\"\n",
        );
        let loader = ProjectLoader::new(tmp.path());
        let entries = loader.load_requests().unwrap();
        assert_eq!(entries.len(), 2);

        let login = entries.iter().find(|e| e.request.id == "auth.login").unwrap();
        assert_eq!(login.folder(), "auth");

        let list = entries.iter().find(|e| e.request.id == "users.list").unwrap();
        assert_eq!(list.folder(), "users");
    }

    #[test]
    fn folder_of_root_level_request_is_empty() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "requests/ping.yaml",
            "id: ping\nname: Ping\nmethod: GET\nurl: \"{{base_url}}/ping\"\n",
        );
        let loader = ProjectLoader::new(tmp.path());
        let entries = loader.load_requests().unwrap();
        assert_eq!(entries[0].folder(), "");
    }

    #[test]
    fn save_request_round_trips() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());

        let req = crate::request::RequestDef {
            id: "users.get".into(),
            name: "Get user".into(),
            method: "GET".into(),
            url: "{{base_url}}/users/{{id}}".into(),
            headers: Default::default(),
            query: Default::default(),
            body: None,
            cases: Default::default(),
            expect: None,
            capture: Default::default(),
            redact: Default::default(),
            notes: None,
            tags: Default::default(),
        };

        let rel = std::path::Path::new("requests/users/get.yaml");
        loader.save_request(rel, &req).unwrap();

        let loaded = load_yaml::<crate::request::RequestDef>(&tmp.path().join(rel)).unwrap();
        assert_eq!(loaded.id, "users.get");
        assert_eq!(loaded.url, "{{base_url}}/users/{{id}}");
    }

    #[test]
    fn missing_directories_return_empty() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        assert!(loader.load_environments().unwrap().is_empty());
        assert!(loader.load_requests().unwrap().is_empty());
    }

    #[test]
    fn save_environment_round_trips() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        let env = crate::environment::Environment {
            id: "local".into(),
            name: "Local".into(),
            vars: [("base_url".into(), "http://localhost:8000".into())].into_iter().collect(),
        };
        loader.save_environment(&env).unwrap();
        let loaded = loader.load_environments().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "local");
        assert_eq!(loaded[0].vars["base_url"], "http://localhost:8000");
    }

    #[test]
    fn delete_environment_removes_file() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        let env = crate::environment::Environment {
            id: "staging".into(),
            name: "Staging".into(),
            vars: Default::default(),
        };
        loader.save_environment(&env).unwrap();
        assert_eq!(loader.load_environments().unwrap().len(), 1);
        loader.delete_environment("staging").unwrap();
        assert!(loader.load_environments().unwrap().is_empty());
    }

    #[test]
    fn delete_nonexistent_environment_is_noop() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        loader.delete_environment("does-not-exist").unwrap();
    }
}
