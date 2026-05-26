use crate::environment::Environment;
use crate::error::ModelError;
use crate::project::AlmanacProject;
use crate::request::RequestDef;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Parse a leading "N-" numeric prefix from a filename stem or directory name.
/// Returns `(N, rest_of_name)`. If no valid prefix is found, returns `(u32::MAX, full_name)`.
///
/// Examples: `"1-login"` → `(1, "login")`, `"10-auth"` → `(10, "auth")`,
/// `"login"` → `(u32::MAX, "login")`, `"-bad"` → `(u32::MAX, "-bad")`.
pub fn parse_order_prefix(name: &str) -> (u32, &str) {
    let bytes = name.as_bytes();
    let mut end = 0;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end > 0 && end < bytes.len() && bytes[end] == b'-' {
        if let Ok(n) = name[..end].parse::<u32>() {
            return (n, &name[end + 1..]);
        }
    }
    (u32::MAX, name)
}

/// Strip a leading "N-" numeric prefix for display. Returns the rest of the name.
///
/// Example: `"1-auth"` → `"auth"`, `"users"` → `"users"`.
pub fn strip_order_prefix(name: &str) -> &str {
    parse_order_prefix(name).1
}

/// A request paired with the path of its YAML file relative to the project root.
#[derive(Debug, Clone)]
pub struct RequestEntry {
    /// Path relative to the project root, e.g. `requests/auth/login.yaml`.
    pub file_path: PathBuf,
    pub request: RequestDef,
}

impl RequestEntry {
    /// Folder component relative to `requests/`, e.g. `"1-auth"` or `""` for root.
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

    /// Numeric order derived from the filename prefix, or `u32::MAX` if unprefixed.
    pub fn order(&self) -> u32 {
        let stem = self.file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        parse_order_prefix(stem).0
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
    /// its path relative to the project root. Results are sorted by numeric prefix
    /// within each directory, with un-prefixed files sorted last (order = MAX).
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

    /// Return all subdirectory paths under `requests/`, relative to the `requests/` dir.
    /// Sorted by numeric prefix at each level, then alphabetically. Includes empty dirs.
    pub fn list_folders(&self) -> Result<Vec<String>, ModelError> {
        let dir = self.root.join("requests");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut folders = Vec::new();
        collect_subdirs(&dir, &dir, &mut folders)?;
        Ok(folders)
    }

    /// Create a group directory under `requests/`. The last path component receives an
    /// auto-assigned numeric prefix (one after the highest existing sibling prefix).
    /// Intermediate parent directories are created if missing (without prefix).
    ///
    /// Returns the full folder path relative to `requests/` with the prefix applied to
    /// the last component, e.g. `"3-payments"` or `"1-auth/2-oauth"`.
    pub fn create_group(&self, path: &str) -> Result<String, ModelError> {
        let requests_dir = self.root.join("requests");
        let (parent_rel, label) = match path.rfind('/') {
            Some(idx) => (&path[..idx], &path[idx + 1..]),
            None => ("", path),
        };
        let parent_abs = if parent_rel.is_empty() {
            requests_dir.clone()
        } else {
            requests_dir.join(parent_rel)
        };
        let max_order = sorted_subdirs_in(&parent_abs)?
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .map(|n| parse_order_prefix(n).0)
            .filter(|&n| n != u32::MAX)
            .max()
            .unwrap_or(0);
        let prefix = max_order + 1;
        let new_name = format!("{prefix}-{label}");
        let new_dir = parent_abs.join(&new_name);
        std::fs::create_dir_all(&new_dir)?;
        let gitkeep = new_dir.join(".gitkeep");
        if !gitkeep.exists() {
            std::fs::write(&gitkeep, "")?;
        }
        let full_path = if parent_rel.is_empty() {
            new_name
        } else {
            format!("{parent_rel}/{new_name}")
        };
        Ok(full_path)
    }

    /// Rename a group by moving its directory. The caller is responsible for preserving
    /// the numeric prefix in `new_folder` when desired. Intermediate parent directories
    /// for the new path are created automatically.
    pub fn rename_group(&self, old_folder: &str, new_folder: &str) -> Result<(), ModelError> {
        let requests_dir = self.root.join("requests");
        let old_dir = requests_dir.join(old_folder);
        let new_dir = requests_dir.join(new_folder);
        if let Some(parent) = new_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&old_dir, &new_dir)?;
        Ok(())
    }

    /// Delete a group directory and all its contents.
    pub fn delete_group(&self, folder: &str) -> Result<(), ModelError> {
        let dir = self.root.join("requests").join(folder);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Delete a request YAML file. No-op if the file does not exist.
    pub fn delete_request(&self, relative_path: &Path) -> Result<(), ModelError> {
        let abs = self.root.join(relative_path);
        if abs.exists() {
            std::fs::remove_file(&abs)?;
        }
        Ok(())
    }

    /// Update the `name` display field of a request in-place. The file name and `id`
    /// field are left unchanged so existing references remain valid.
    pub fn rename_request_name(&self, relative_path: &Path, new_name: &str) -> Result<PathBuf, ModelError> {
        let abs = self.root.join(relative_path);
        let text = std::fs::read_to_string(&abs)?;
        let mut req: RequestDef = serde_yaml::from_str(&text)
            .map_err(|e| ModelError::Yaml { path: abs.display().to_string(), source: e })?;
        req.name = new_name.to_string();
        let yaml = serde_yaml::to_string(&req)
            .map_err(|e| ModelError::Yaml { path: abs.display().to_string(), source: e })?;

        // Derive new file name: preserve numeric prefix, embed uid for uniqueness.
        let old_stem = abs.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let (prefix_num, _) = parse_order_prefix(old_stem);
        let new_slug = safe_file_slug(new_name);
        let new_file_name = if prefix_num == u32::MAX {
            format!("{}-{new_slug}.yaml", req.uid)
        } else {
            format!("{prefix_num}-{}-{new_slug}.yaml", req.uid)
        };

        let folder_abs = abs.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
        })?;
        let new_abs = folder_abs.join(&new_file_name);

        if new_abs == abs {
            std::fs::write(&abs, yaml)?;
        } else {
            std::fs::write(&new_abs, yaml)?;
            std::fs::remove_file(&abs)?;
        }

        let new_rel = relative_path
            .parent()
            .map(|p| p.join(&new_file_name))
            .unwrap_or_else(|| PathBuf::from(&new_file_name));
        Ok(new_rel)
    }

    /// Move a request file to a different folder (physical move; `id` field unchanged).
    /// Pass `new_folder = ""` to move to the root of `requests/`.
    /// Returns the new path relative to the project root.
    pub fn move_request(&self, old_rel_path: &Path, new_folder: &str) -> Result<PathBuf, ModelError> {
        let file_name = old_rel_path.file_name().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no filename")
        })?;
        let new_rel_path = if new_folder.is_empty() {
            PathBuf::from("requests").join(file_name)
        } else {
            PathBuf::from("requests").join(new_folder).join(file_name)
        };
        let old_abs = self.root.join(old_rel_path);
        let new_abs = self.root.join(&new_rel_path);
        if let Some(parent) = new_abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&old_abs, &new_abs)?;
        Ok(new_rel_path)
    }

    /// Reorder a request within its folder by assigning consecutive 1..=N prefixes.
    ///
    /// `rel_path` is the current path relative to the project root.
    /// `new_position` is the 0-based target index among siblings (clamped to valid range).
    ///
    /// Returns a map of `old_relative_path → new_relative_path` for every file that
    /// was renamed (use it to update `selectedFilePath` in the frontend).
    pub fn reorder_request(
        &self,
        rel_path: &Path,
        new_position: usize,
    ) -> Result<HashMap<PathBuf, PathBuf>, ModelError> {
        let abs_path = self.root.join(rel_path);
        let folder_abs = abs_path.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
        })?;

        let mut siblings = yaml_files_in(folder_abs)?;
        let current_idx = siblings.iter().position(|p| p == &abs_path).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "file not found among siblings")
        })?;

        let item = siblings.remove(current_idx);
        let insert_at = new_position.min(siblings.len());
        siblings.insert(insert_at, item);

        renumber_files(&self.root, folder_abs, siblings)
    }

    /// Duplicate a request YAML file in the same folder.
    ///
    /// The copy gets a unique display name (`"{name} copy"`, `"{name} copy 2"`, …), a
    /// new `id` derived from the copy name, and a freshly generated `uid`.
    /// No numeric prefix is assigned so the copy sorts last; the user can reorder via drag-and-drop.
    /// Returns the new path relative to the project root.
    pub fn duplicate_request(&self, relative_path: &Path) -> Result<PathBuf, ModelError> {
        let abs_path = self.root.join(relative_path);
        let original: RequestDef = load_yaml(&abs_path)?;

        let folder_abs = abs_path.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
        })?;

        let siblings = yaml_files_in(folder_abs)?;
        let existing_names: Vec<String> = siblings
            .iter()
            .filter_map(|p| load_yaml::<RequestDef>(p).ok())
            .map(|r| r.name)
            .collect();

        let new_name = copy_name(&original.name, &existing_names);
        let new_id = derive_copy_id(&original.id, &new_name);

        let mut new_req = original;
        new_req.uid = crate::uid::generate_uid();
        new_req.id = new_id;
        new_req.name = new_name;

        // UID guarantees uniqueness; no collision check needed.
        let new_file_name = format!("{}-{}.yaml", new_req.uid, safe_file_slug(&new_req.name));
        let new_rel_path = relative_path
            .parent()
            .map(|p| p.join(&new_file_name))
            .unwrap_or_else(|| PathBuf::from(&new_file_name));

        self.save_request(&new_rel_path, &new_req)?;
        Ok(new_rel_path)
    }

    /// Ensure every request YAML file under `requests/` has a non-empty, unique `uid`.
    ///
    /// For files missing a uid, prepends `uid: {XXXXXXXX}\n` to the raw YAML text,
    /// preserving comments, field order, and other content exactly.
    /// For files with a duplicate uid (e.g. after a manual copy), re-serializes the
    /// offending file with a freshly generated uid.
    ///
    /// This is called once per `open_project` / `open_recent_project` and is idempotent
    /// after the first run (files already having a uid are not touched).
    pub fn ensure_all_uids(&self) -> Result<(), ModelError> {
        use std::collections::HashSet;

        let dir = self.root.join("requests");
        if !dir.exists() {
            return Ok(());
        }
        let mut abs_paths = Vec::new();
        collect_yaml_files(&dir, &mut abs_paths)?;
        let mut seen: HashSet<String> = HashSet::new();

        for abs_path in abs_paths {
            let text = std::fs::read_to_string(&abs_path)?;
            let req: RequestDef = serde_yaml::from_str(&text)
                .map_err(|e| ModelError::Yaml { path: abs_path.display().to_string(), source: e })?;

            if req.uid.is_empty() {
                // Prepend uid line to preserve all existing content (comments, field order).
                let uid = loop {
                    let candidate = crate::uid::generate_uid();
                    if seen.insert(candidate.clone()) { break candidate; }
                };
                let new_text = if let Some(rest) = text.strip_prefix("---\n") {
                    format!("---\nuid: {uid}\n{rest}")
                } else {
                    format!("uid: {uid}\n{text}")
                };
                std::fs::write(&abs_path, new_text)?;
            } else if !seen.insert(req.uid.clone()) {
                // Collision (rare): regenerate uid and re-serialize.
                let uid = loop {
                    let candidate = crate::uid::generate_uid();
                    if seen.insert(candidate.clone()) { break candidate; }
                };
                let mut updated = req;
                updated.uid = uid;
                let yaml = serde_yaml::to_string(&updated)
                    .map_err(|e| ModelError::Yaml { path: abs_path.display().to_string(), source: e })?;
                std::fs::write(&abs_path, yaml)?;
            }
        }
        Ok(())
    }

    /// Rename every request YAML under `requests/` to the canonical format:
    /// `{index}-{uid}-{safe_file_slug(name)}.yaml`
    ///
    /// Files whose name already matches are left untouched (idempotent).
    /// Within each folder the existing sort order (by numeric prefix) is preserved and
    /// 1-based indices are re-assigned.
    ///
    /// Must be called after `ensure_all_uids` so every file has a non-empty uid.
    pub fn normalize_file_names(&self) -> Result<(), ModelError> {
        let requests_dir = self.root.join("requests");
        if !requests_dir.exists() {
            return Ok(());
        }
        normalize_dir(&requests_dir)?;
        for subdir in sorted_subdirs_in(&requests_dir)? {
            normalize_dir(&subdir)?;
        }
        Ok(())
    }

    /// Reorder a group (directory) among its siblings by assigning consecutive 1..=N prefixes.
    ///
    /// `folder` is relative to `requests/`, e.g. `"2-users"` or `"1-auth/3-oauth"`.
    /// `new_position` is the 0-based target index among siblings (clamped to valid range).
    ///
    /// Returns a map of `old_folder_path → new_folder_path` (both relative to `requests/`)
    /// for every directory that was renamed.
    pub fn reorder_group(
        &self,
        folder: &str,
        new_position: usize,
    ) -> Result<HashMap<String, String>, ModelError> {
        let requests_dir = self.root.join("requests");
        let folder_abs = requests_dir.join(folder);
        let parent_abs = folder_abs.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "folder has no parent")
        })?;

        let mut siblings = sorted_subdirs_in(parent_abs)?;
        let current_idx = siblings.iter().position(|p| p == &folder_abs).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "folder not found among siblings")
        })?;

        let item = siblings.remove(current_idx);
        let insert_at = new_position.min(siblings.len());
        siblings.insert(insert_at, item);

        let mut renames = HashMap::new();
        for (i, old_abs) in siblings.iter().enumerate() {
            let n = (i + 1) as u32;
            let dir_name = old_abs.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let bare = strip_order_prefix(dir_name);
            let new_name = format!("{n}-{bare}");
            let new_abs = parent_abs.join(&new_name);
            if old_abs != &new_abs {
                std::fs::rename(old_abs, &new_abs)?;
                let old_rel = old_abs
                    .strip_prefix(&requests_dir)
                    .unwrap_or(old_abs)
                    .to_string_lossy()
                    .replace('\\', "/");
                let new_rel = new_abs
                    .strip_prefix(&requests_dir)
                    .unwrap_or(&new_abs)
                    .to_string_lossy()
                    .replace('\\', "/");
                renames.insert(old_rel, new_rel);
            }
        }
        Ok(renames)
    }
}

// ── Environment inheritance ────────────────────────────────────────────────

/// Resolve the effective variable set for `env_id` by walking the parent chain.
/// Parent vars are laid down first; child vars override them. Case and session
/// vars are NOT included here — those are applied later in the caller.
///
/// Returns `Err` if a parent id is not found in `all_envs` or if a cycle is detected.
pub fn resolve_env_vars(
    env_id: &str,
    all_envs: &[Environment],
) -> Result<HashMap<String, String>, String> {
    // Walk from the requested env up to the root, collecting ids in child→root order.
    let mut chain: Vec<&str> = Vec::new();
    let mut current_id = env_id;
    loop {
        if chain.contains(&current_id) {
            chain.push(current_id);
            return Err(format!(
                "environment inheritance cycle: {}",
                chain.join(" → ")
            ));
        }
        let env = all_envs
            .iter()
            .find(|e| e.id == current_id)
            .ok_or_else(|| format!("parent environment '{}' not found", current_id))?;
        chain.push(current_id);
        match &env.parent {
            Some(p) => current_id = p.as_str(),
            None => break,
        }
    }
    // Replay root→leaf so child vars win.
    let mut merged = HashMap::new();
    for id in chain.iter().rev() {
        let env = all_envs.iter().find(|e| e.id == *id).unwrap();
        merged.extend(env.vars.clone());
    }
    Ok(merged)
}

// ── Private helpers ────────────────────────────────────────────────────────

/// Generate a copy display name: `"{name} copy"`, `"{name} copy 2"`, etc.
fn copy_name(original: &str, existing: &[String]) -> String {
    let base = format!("{original} copy");
    if !existing.contains(&base) {
        return base;
    }
    let mut i = 2u32;
    loop {
        let candidate = format!("{original} copy {i}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        i += 1;
    }
}

/// Derive an `id` for a duplicate: reuse the namespace of the original id
/// (everything before the last `.`) and append a slug of the new name.
fn derive_copy_id(original_id: &str, new_name: &str) -> String {
    let slug = slugify(new_name);
    match original_id.rfind('.') {
        Some(idx) => format!("{}.{slug}", &original_id[..idx]),
        None => slug,
    }
}

/// Slugify a string for use in `id` and copy-name derivation: lowercase, spaces → hyphens,
/// strip non-alphanumeric (except hyphens), collapse consecutive hyphens.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, ' ' | '-' | '_') {
            if !out.ends_with('-') {
                out.push('-');
            }
        }
    }
    out.trim_matches('-').to_owned()
}

/// Filesystem-safe slug for request file names.
///
/// Differences from `slugify`:
/// - Folds common accented/diacritic chars to their ASCII base (e.g. `í → i`, `ç → c`)
/// - Replaces unknown/unsafe chars with `_` instead of silently dropping them
/// - Spaces and hyphens → `-`; underscores → `_`; consecutive separators collapsed
fn safe_file_slug(s: &str) -> String {
    fn fold(c: char) -> char {
        match c {
            'à'|'á'|'â'|'ã'|'ä'|'å'|'À'|'Á'|'Â'|'Ã'|'Ä'|'Å'|'æ'|'Æ' => 'a',
            'è'|'é'|'ê'|'ë'|'È'|'É'|'Ê'|'Ë' => 'e',
            'ì'|'í'|'î'|'ï'|'Ì'|'Í'|'Î'|'Ï' => 'i',
            'ò'|'ó'|'ô'|'õ'|'ö'|'ø'|'Ò'|'Ó'|'Ô'|'Õ'|'Ö'|'Ø'|'œ'|'Œ' => 'o',
            'ù'|'ú'|'û'|'ü'|'Ù'|'Ú'|'Û'|'Ü' => 'u',
            'ý'|'ÿ'|'Ý'|'Ÿ' => 'y',
            'ñ'|'Ñ' => 'n',
            'ç'|'Ç' => 'c',
            'ß' => 's',
            _ => c,
        }
    }

    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let ch = fold(ch);
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '_' {
            if !out.ends_with('_') && !out.ends_with('-') {
                out.push('_');
            }
        } else if ch.is_alphanumeric() || ch == ' ' || ch == '-' {
            if !out.ends_with('-') && !out.ends_with('_') {
                out.push('-');
            }
        } else {
            if !out.ends_with('_') && !out.ends_with('-') {
                out.push('_');
            }
        }
    }
    out.trim_matches(|c| c == '-' || c == '_').to_owned()
}

/// Return a file name `"{stem}.yaml"` that does not exist in `existing`.
/// Falls back to `"{stem}-2.yaml"`, `"{stem}-3.yaml"`, etc.
fn unique_file_name(stem: &str, existing: &std::collections::HashSet<String>) -> String {
    let candidate = format!("{stem}.yaml");
    if !existing.contains(&candidate) {
        return candidate;
    }
    let mut i = 2u32;
    loop {
        let candidate = format!("{stem}-{i}.yaml");
        if !existing.contains(&candidate) {
            return candidate;
        }
        i += 1;
    }
}

/// Sort key: `(numeric_prefix, lowercase_remainder)` from the last path component.
fn order_key(path: &Path) -> (u32, String) {
    let name = path
        .file_stem()
        .or_else(|| path.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let (n, rest) = parse_order_prefix(name);
    (n, rest.to_lowercase())
}

/// Return all `.yaml`/`.yml` files directly inside a directory, sorted by numeric prefix.
fn yaml_files_in(dir: &Path) -> Result<Vec<PathBuf>, ModelError> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() && is_yaml(&path) {
            files.push(path);
        }
    }
    files.sort_by(|a, b| order_key(a).cmp(&order_key(b)));
    Ok(files)
}

/// Recursively collect all `.yaml`/`.yml` files under a directory,
/// traversing subdirectories in numeric-prefix order.
fn collect_yaml_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ModelError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    entries.sort_by(|a, b| order_key(a).cmp(&order_key(b)));
    for path in entries {
        if path.is_dir() {
            collect_yaml_files(&path, out)?;
        } else if path.is_file() && is_yaml(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Recursively collect subdirectory paths relative to `base`, in numeric-prefix order.
fn collect_subdirs(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), ModelError> {
    for path in sorted_subdirs_in(dir)? {
        let rel = path.strip_prefix(base).unwrap_or(&path);
        out.push(rel.to_string_lossy().replace('\\', "/"));
        collect_subdirs(base, &path, out)?;
    }
    Ok(())
}

/// Return direct subdirectories of `dir` sorted by numeric prefix then alphabetically.
fn sorted_subdirs_in(dir: &Path) -> Result<Vec<PathBuf>, ModelError> {
    let mut dirs = Vec::new();
    if !dir.exists() {
        return Ok(dirs);
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    dirs.sort_by(|a, b| order_key(a).cmp(&order_key(b)));
    Ok(dirs)
}

/// Renumber a list of sibling yaml files with consecutive prefixes 1..=N,
/// renaming on disk any files whose name changes. Returns the old→new relative-path map.
fn renumber_files(
    root: &Path,
    folder_abs: &Path,
    siblings: Vec<PathBuf>,
) -> Result<HashMap<PathBuf, PathBuf>, ModelError> {
    let mut renames = HashMap::new();
    for (i, old_abs) in siblings.iter().enumerate() {
        let n = (i + 1) as u32;
        let stem = old_abs.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let bare = strip_order_prefix(stem);
        let new_name = format!("{n}-{bare}.yaml");
        let new_abs = folder_abs.join(&new_name);
        if old_abs != &new_abs {
            std::fs::rename(old_abs, &new_abs)?;
            let old_rel = old_abs
                .strip_prefix(root)
                .unwrap_or(old_abs)
                .to_path_buf();
            let new_rel = new_abs
                .strip_prefix(root)
                .unwrap_or(&new_abs)
                .to_path_buf();
            renames.insert(old_rel, new_rel);
        }
    }
    Ok(renames)
}

/// Rename all YAML files directly inside `folder_abs` to canonical
/// `{idx}-{uid}-{slug}.yaml` format (1-based index, existing sort order preserved).
fn normalize_dir(folder_abs: &Path) -> Result<(), ModelError> {
    let files = yaml_files_in(folder_abs)?;
    for (i, old_abs) in files.iter().enumerate() {
        let req: RequestDef = load_yaml(old_abs)?;
        let expected = format!("{}-{}-{}.yaml", i + 1, req.uid, safe_file_slug(&req.name));
        let current = old_abs.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if current != expected {
            std::fs::rename(old_abs, folder_abs.join(&expected))?;
        }
    }
    Ok(())
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

    // ── parse_order_prefix ────────────────────────────────────────────────

    #[test]
    fn parse_order_prefix_extracts_number() {
        assert_eq!(parse_order_prefix("1-login"), (1, "login"));
        assert_eq!(parse_order_prefix("10-auth"), (10, "auth"));
        assert_eq!(parse_order_prefix("100-users"), (100, "users"));
    }

    #[test]
    fn parse_order_prefix_no_prefix_returns_max() {
        let (n, rest) = parse_order_prefix("login");
        assert_eq!(n, u32::MAX);
        assert_eq!(rest, "login");
    }

    #[test]
    fn parse_order_prefix_leading_dash_is_not_prefix() {
        let (n, _) = parse_order_prefix("-bad");
        assert_eq!(n, u32::MAX);
    }

    #[test]
    fn strip_order_prefix_removes_prefix() {
        assert_eq!(strip_order_prefix("1-auth"), "auth");
        assert_eq!(strip_order_prefix("42-users"), "users");
        assert_eq!(strip_order_prefix("no-prefix"), "no-prefix");
    }

    // ── load_project ──────────────────────────────────────────────────────

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

    // ── load_environments ─────────────────────────────────────────────────

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

    // ── load_requests ─────────────────────────────────────────────────────

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
    fn load_requests_sorted_by_prefix() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "requests/2-register.yaml",
            "id: register\nname: Register\nmethod: POST\nurl: /register\n",
        );
        write(
            tmp.path(),
            "requests/1-login.yaml",
            "id: login\nname: Login\nmethod: POST\nurl: /login\n",
        );
        write(
            tmp.path(),
            "requests/10-verify.yaml",
            "id: verify\nname: Verify\nmethod: GET\nurl: /verify\n",
        );
        let loader = ProjectLoader::new(tmp.path());
        let entries = loader.load_requests().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].request.id, "login");    // 1-login
        assert_eq!(entries[1].request.id, "register"); // 2-register
        assert_eq!(entries[2].request.id, "verify");   // 10-verify
    }

    #[test]
    fn load_requests_unprefixed_sorted_last() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "requests/login.yaml",
            "id: login\nname: Login\nmethod: POST\nurl: /login\n",
        );
        write(
            tmp.path(),
            "requests/1-health.yaml",
            "id: health\nname: Health\nmethod: GET\nurl: /health\n",
        );
        let loader = ProjectLoader::new(tmp.path());
        let entries = loader.load_requests().unwrap();
        assert_eq!(entries[0].request.id, "health"); // prefixed → first
        assert_eq!(entries[1].request.id, "login");  // unprefixed → last
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

    // ── save_request ──────────────────────────────────────────────────────

    #[test]
    fn save_request_round_trips() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());

        let req = crate::request::RequestDef {
            uid: "TESTUID1".into(),
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

    // ── missing dirs ──────────────────────────────────────────────────────

    #[test]
    fn missing_directories_return_empty() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        assert!(loader.load_environments().unwrap().is_empty());
        assert!(loader.load_requests().unwrap().is_empty());
    }

    // ── save_environment ──────────────────────────────────────────────────

    #[test]
    fn save_environment_round_trips() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        let env = crate::environment::Environment {
            id: "local".into(),
            name: "Local".into(),
            parent: None,
            vars: [("base_url".into(), "http://localhost:8000".into())].into_iter().collect(),
        };
        loader.save_environment(&env).unwrap();
        let loaded = loader.load_environments().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "local");
        assert_eq!(loaded[0].vars["base_url"], "http://localhost:8000");
    }

    // ── delete_environment ────────────────────────────────────────────────

    #[test]
    fn delete_environment_removes_file() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        let env = crate::environment::Environment {
            id: "staging".into(),
            name: "Staging".into(),
            parent: None,
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

    // ── list_folders ──────────────────────────────────────────────────────

    #[test]
    fn list_folders_returns_all_subdirs() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/auth/login.yaml",
            "id: auth.login\nname: Login\nmethod: POST\nurl: /login\n");
        write(tmp.path(), "requests/auth/oauth/token.yaml",
            "id: auth.oauth.token\nname: Token\nmethod: POST\nurl: /token\n");
        write(tmp.path(), "requests/users/list.yaml",
            "id: users.list\nname: List\nmethod: GET\nurl: /users\n");
        let loader = ProjectLoader::new(tmp.path());
        let folders = loader.list_folders().unwrap();
        assert!(folders.contains(&"auth".to_string()));
        assert!(folders.contains(&"auth/oauth".to_string()));
        assert!(folders.contains(&"users".to_string()));
    }

    #[test]
    fn list_folders_sorted_by_prefix() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("requests/2-users")).unwrap();
        fs::create_dir_all(tmp.path().join("requests/1-auth")).unwrap();
        fs::create_dir_all(tmp.path().join("requests/10-health")).unwrap();
        let loader = ProjectLoader::new(tmp.path());
        let folders = loader.list_folders().unwrap();
        assert_eq!(folders, vec!["1-auth", "2-users", "10-health"]);
    }

    #[test]
    fn list_folders_includes_empty_dirs() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        let created = loader.create_group("empty-group").unwrap();
        let folders = loader.list_folders().unwrap();
        assert!(folders.contains(&created));
    }

    // ── create_group ──────────────────────────────────────────────────────

    #[test]
    fn create_group_assigns_prefix_one_when_first() {
        let tmp = TempDir::new().unwrap();
        let loader = ProjectLoader::new(tmp.path());
        let path = loader.create_group("payments").unwrap();
        assert_eq!(path, "1-payments");
        assert!(tmp.path().join("requests/1-payments").is_dir());
        assert!(tmp.path().join("requests/1-payments/.gitkeep").exists());
    }

    #[test]
    fn create_group_assigns_next_prefix_after_siblings() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("requests/1-auth")).unwrap();
        fs::create_dir_all(tmp.path().join("requests/2-users")).unwrap();
        let loader = ProjectLoader::new(tmp.path());
        let path = loader.create_group("health").unwrap();
        assert_eq!(path, "3-health");
    }

    #[test]
    fn create_group_nested_assigns_prefix_within_parent() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("requests/1-auth")).unwrap();
        let loader = ProjectLoader::new(tmp.path());
        let path = loader.create_group("1-auth/oauth").unwrap();
        assert_eq!(path, "1-auth/1-oauth");
        assert!(tmp.path().join("requests/1-auth/1-oauth").is_dir());
    }

    // ── rename_group ──────────────────────────────────────────────────────

    #[test]
    fn rename_group_moves_directory() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/auth/login.yaml",
            "id: auth.login\nname: Login\nmethod: POST\nurl: /login\n");
        let loader = ProjectLoader::new(tmp.path());
        loader.rename_group("auth", "authentication").unwrap();
        assert!(!tmp.path().join("requests/auth").exists());
        assert!(tmp.path().join("requests/authentication/login.yaml").exists());
    }

    // ── delete_group ──────────────────────────────────────────────────────

    #[test]
    fn delete_group_removes_directory_and_contents() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/auth/login.yaml",
            "id: auth.login\nname: Login\nmethod: POST\nurl: /login\n");
        let loader = ProjectLoader::new(tmp.path());
        loader.delete_group("auth").unwrap();
        assert!(!tmp.path().join("requests/auth").exists());
    }

    // ── delete_request ────────────────────────────────────────────────────

    #[test]
    fn delete_request_removes_file() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/ping.yaml",
            "id: ping\nname: Ping\nmethod: GET\nurl: /ping\n");
        let loader = ProjectLoader::new(tmp.path());
        loader.delete_request(std::path::Path::new("requests/ping.yaml")).unwrap();
        assert!(!tmp.path().join("requests/ping.yaml").exists());
    }

    // ── rename_request_name ───────────────────────────────────────────────

    #[test]
    fn rename_request_renames_file_and_updates_name() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/ping.yaml",
            "uid: TESTUID1\nid: ping\nname: Ping\nmethod: GET\nurl: /ping\n");
        let loader = ProjectLoader::new(tmp.path());
        let new_path = loader.rename_request_name(
            std::path::Path::new("requests/ping.yaml"), "Health Check").unwrap();
        assert!(!tmp.path().join("requests/ping.yaml").exists());
        assert!(tmp.path().join("requests/TESTUID1-health-check.yaml").exists());
        assert_eq!(new_path.to_string_lossy(), "requests/TESTUID1-health-check.yaml");
        let entries = loader.load_requests().unwrap();
        let req = entries.iter().find(|e| e.request.id == "ping").unwrap();
        assert_eq!(req.request.name, "Health Check");
        assert_eq!(req.request.id, "ping");
    }

    #[test]
    fn rename_request_preserves_numeric_prefix() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/10-arturo-b-copy.yaml",
            "uid: TESTUID2\nid: arturo.b-copy\nname: Arturo B copy\nmethod: GET\nurl: /users\n");
        let loader = ProjectLoader::new(tmp.path());
        let new_path = loader.rename_request_name(
            std::path::Path::new("requests/10-arturo-b-copy.yaml"), "Arturo B").unwrap();
        assert!(!tmp.path().join("requests/10-arturo-b-copy.yaml").exists());
        assert!(tmp.path().join("requests/10-TESTUID2-arturo-b.yaml").exists());
        assert_eq!(new_path.to_string_lossy(), "requests/10-TESTUID2-arturo-b.yaml");
    }

    // ── safe_file_slug ────────────────────────────────────────────────────

    #[test]
    fn safe_file_slug_folds_accented_chars() {
        assert_eq!(safe_file_slug("García"), "garcia");
        assert_eq!(safe_file_slug("São Paulo"), "sao-paulo");
        assert_eq!(safe_file_slug("café"), "cafe");
        assert_eq!(safe_file_slug("Ångström"), "angstrom");
        assert_eq!(safe_file_slug("naïve"), "naive");
    }

    #[test]
    fn safe_file_slug_replaces_unsafe_chars() {
        // space before '(' collapses: the '-' absorbs the paren into a single separator
        assert_eq!(safe_file_slug("Get (Users)"), "get-users");
        assert_eq!(safe_file_slug("A+B"), "a_b");
        assert_eq!(safe_file_slug("hello world"), "hello-world");
        assert_eq!(safe_file_slug("foo (bar+baz)"), "foo-bar_baz");
    }

    // ── normalize_file_names ──────────────────────────────────────────────

    #[test]
    fn normalize_file_names_renames_to_canonical_format() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/1-arturo/arturo-b-copy.yaml",
            "uid: AAABBB11\nid: arturo.b-copy\nname: Arturo B\nmethod: GET\nurl: /users\n");
        write(tmp.path(), "requests/1-arturo/old-name.yaml",
            "uid: CCCDDD22\nid: arturo.ps-copy\nname: Santiago García\nmethod: GET\nurl: /sg\n");
        let loader = ProjectLoader::new(tmp.path());
        loader.normalize_file_names().unwrap();

        let folder = tmp.path().join("requests/1-arturo");
        assert!(folder.join("1-AAABBB11-arturo-b.yaml").exists(), "first file should be renamed");
        assert!(folder.join("2-CCCDDD22-santiago-garcia.yaml").exists(), "second file with accented name");
        assert!(!folder.join("arturo-b-copy.yaml").exists());
        assert!(!folder.join("old-name.yaml").exists());
    }

    #[test]
    fn normalize_file_names_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/login.yaml",
            "uid: IDEM0001\nid: login\nname: Login\nmethod: POST\nurl: /login\n");
        let loader = ProjectLoader::new(tmp.path());
        loader.normalize_file_names().unwrap();
        // First pass: should rename to canonical
        assert!(tmp.path().join("requests/1-IDEM0001-login.yaml").exists());
        // Second pass: should be a no-op
        loader.normalize_file_names().unwrap();
        assert!(tmp.path().join("requests/1-IDEM0001-login.yaml").exists());
    }

    // ── move_request ──────────────────────────────────────────────────────

    #[test]
    fn move_request_relocates_file() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/ping.yaml",
            "id: ping\nname: Ping\nmethod: GET\nurl: /ping\n");
        let loader = ProjectLoader::new(tmp.path());
        let new_path = loader.move_request(
            std::path::Path::new("requests/ping.yaml"),
            "health",
        ).unwrap();
        assert!(!tmp.path().join("requests/ping.yaml").exists());
        assert!(tmp.path().join("requests/health/ping.yaml").exists());
        assert_eq!(new_path.to_string_lossy(), "requests/health/ping.yaml");
    }

    #[test]
    fn move_request_to_root_uses_empty_folder() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/auth/login.yaml",
            "id: auth.login\nname: Login\nmethod: POST\nurl: /login\n");
        let loader = ProjectLoader::new(tmp.path());
        let new_path = loader.move_request(
            std::path::Path::new("requests/auth/login.yaml"),
            "",
        ).unwrap();
        assert!(tmp.path().join("requests/login.yaml").exists());
        assert_eq!(new_path.to_string_lossy(), "requests/login.yaml");
    }

    // ── reorder_request ───────────────────────────────────────────────────

    #[test]
    fn reorder_request_renumbers_siblings() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/1-login.yaml",
            "id: login\nname: Login\nmethod: POST\nurl: /login\n");
        write(tmp.path(), "requests/2-register.yaml",
            "id: register\nname: Register\nmethod: POST\nurl: /register\n");
        write(tmp.path(), "requests/3-verify.yaml",
            "id: verify\nname: Verify\nmethod: GET\nurl: /verify\n");

        let loader = ProjectLoader::new(tmp.path());
        // Move 3-verify to position 0 (first)
        let renames = loader.reorder_request(
            Path::new("requests/3-verify.yaml"),
            0,
        ).unwrap();

        // verify should now be 1-verify, login→2, register→3
        assert!(tmp.path().join("requests/1-verify.yaml").exists());
        assert!(tmp.path().join("requests/2-login.yaml").exists());
        assert!(tmp.path().join("requests/3-register.yaml").exists());
        assert!(!tmp.path().join("requests/3-verify.yaml").exists());

        assert_eq!(renames.len(), 3);
    }

    #[test]
    fn reorder_request_assigns_prefixes_to_unprefixed() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/login.yaml",
            "id: login\nname: Login\nmethod: POST\nurl: /login\n");
        write(tmp.path(), "requests/register.yaml",
            "id: register\nname: Register\nmethod: POST\nurl: /register\n");

        let loader = ProjectLoader::new(tmp.path());
        // Move register to position 0
        let renames = loader.reorder_request(
            Path::new("requests/register.yaml"),
            0,
        ).unwrap();

        assert!(tmp.path().join("requests/1-register.yaml").exists());
        assert!(tmp.path().join("requests/2-login.yaml").exists());
        assert_eq!(renames.len(), 2);
    }

    #[test]
    fn reorder_request_clamped_position() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "requests/1-a.yaml",
            "id: a\nname: A\nmethod: GET\nurl: /a\n");
        write(tmp.path(), "requests/2-b.yaml",
            "id: b\nname: B\nmethod: GET\nurl: /b\n");

        let loader = ProjectLoader::new(tmp.path());
        // Position 100 should clamp to last
        loader.reorder_request(Path::new("requests/1-a.yaml"), 100).unwrap();

        // a should move to the end: 1-b, 2-a
        assert!(tmp.path().join("requests/1-b.yaml").exists());
        assert!(tmp.path().join("requests/2-a.yaml").exists());
    }

    // ── reorder_group ─────────────────────────────────────────────────────

    #[test]
    fn reorder_group_renumbers_siblings() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("requests/1-auth")).unwrap();
        fs::create_dir_all(tmp.path().join("requests/2-users")).unwrap();
        fs::create_dir_all(tmp.path().join("requests/3-health")).unwrap();

        let loader = ProjectLoader::new(tmp.path());
        // Move 3-health to position 0 (first)
        let renames = loader.reorder_group("3-health", 0).unwrap();

        assert!(tmp.path().join("requests/1-health").exists());
        assert!(tmp.path().join("requests/2-auth").exists());
        assert!(tmp.path().join("requests/3-users").exists());
        assert!(!tmp.path().join("requests/3-health").exists());
        assert_eq!(renames.len(), 3);
    }

    #[test]
    fn reorder_group_assigns_prefixes_to_unprefixed() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("requests/auth")).unwrap();
        fs::create_dir_all(tmp.path().join("requests/users")).unwrap();

        let loader = ProjectLoader::new(tmp.path());
        let renames = loader.reorder_group("users", 0).unwrap();

        assert!(tmp.path().join("requests/1-users").exists());
        assert!(tmp.path().join("requests/2-auth").exists());
        assert_eq!(renames.len(), 2);
    }

    // ── resolve_env_vars ──────────────────────────────────────────────────

    fn mk_env(id: &str, parent: Option<&str>, vars: &[(&str, &str)]) -> Environment {
        Environment {
            id: id.into(),
            name: id.into(),
            parent: parent.map(str::to_owned),
            vars: vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[test]
    fn resolve_env_vars_no_parent() {
        let envs = vec![mk_env("local", None, &[("base_url", "http://localhost")])];
        let vars = resolve_env_vars("local", &envs).unwrap();
        assert_eq!(vars["base_url"], "http://localhost");
    }

    #[test]
    fn resolve_env_vars_child_overrides_parent() {
        let envs = vec![
            mk_env("base", None, &[("tenant", "acme"), ("base_url", "https://api.example.com")]),
            mk_env("local", Some("base"), &[("base_url", "http://localhost")]),
        ];
        let vars = resolve_env_vars("local", &envs).unwrap();
        assert_eq!(vars["tenant"], "acme");            // inherited
        assert_eq!(vars["base_url"], "http://localhost"); // overridden
    }

    #[test]
    fn resolve_env_vars_three_level_chain() {
        let envs = vec![
            mk_env("root",   None,          &[("a", "1"), ("b", "1"), ("c", "1")]),
            mk_env("mid",    Some("root"),   &[("b", "2"), ("c", "2")]),
            mk_env("leaf",   Some("mid"),    &[("c", "3")]),
        ];
        let vars = resolve_env_vars("leaf", &envs).unwrap();
        assert_eq!(vars["a"], "1"); // from root
        assert_eq!(vars["b"], "2"); // root overridden by mid
        assert_eq!(vars["c"], "3"); // mid overridden by leaf
    }

    #[test]
    fn resolve_env_vars_missing_parent_returns_err() {
        let envs = vec![mk_env("staging", Some("nonexistent"), &[])];
        let result = resolve_env_vars("staging", &envs);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("nonexistent"));
    }

    #[test]
    fn resolve_env_vars_cycle_returns_err() {
        let envs = vec![
            mk_env("a", Some("b"), &[]),
            mk_env("b", Some("a"), &[]),
        ];
        let result = resolve_env_vars("a", &envs);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cycle"));
    }
}
