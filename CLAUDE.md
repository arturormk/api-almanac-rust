# API Almanac

A local-first workbench for exploring, saving, and rerunning HTTP API calls.
Canonical data format is YAML; Markdown is a generated export/view only.
See `docs/BLUEPRINT.md` for the full product vision.

## Workspace layout

```
Cargo.toml              workspace root
src-tauri/              Tauri 2 Rust backend + app config
  src/lib.rs            Tauri commands
  src/main.rs           entry point
  tauri.conf.json       app config
crates/
  api-almanac-model/    project/request/environment structs + YAML serde
src/                    React + TypeScript frontend (Vite)
docs/
  BLUEPRINT.md          product vision and milestone plan
```

## Running

```bash
npm install             # first time only
npm run tauri dev       # starts Vite dev server + Tauri window
```

## Testing

```bash
cargo test --workspace  # runs all Rust unit tests
```

## CLI (almanac binary)

```bash
cargo build -p api-almanac-cli          # build the binary
cargo run -p api-almanac-cli -- list    # list requests in CWD project
cargo run -p api-almanac-cli -- run requests/auth/login.yaml --env local
cargo run -p api-almanac-cli -- spot-check --env staging
cargo run -p api-almanac-cli -- sketch users.get --env local
cargo run -p api-almanac-cli -- export-md requests/users/get.yaml
```

The binary walks up from the working directory to find `almanac.yaml` (the project root).
Request arguments accept either a file path (`requests/auth/login.yaml`) or a request ID (`auth.login`).

## Crate responsibilities

- `api-almanac-model` — full data model: project, request, environment, body, case structs; variable resolver; project loader; resolved request
- `api-almanac-runner` — async HTTP executor (`Runner`), `HttpResponse`, `RunnerError`; backed by `reqwest`
- `api-almanac-typesketch` — infer observed JSON shape as a YAML sketch (`SketchNode`, `sketch_json`, `to_yaml_string`)
- `api-almanac-export` — Markdown notebook generation (`render_request_md`)
- `api-almanac-tools` — command-based analyzer plugin contract: `PluginManifest`, `PluginBundle`, `PluginOutput`, `run_plugin`
- `api-almanac-store` — response persistence: `StoredResponse`, `save_latest_response`, `load_latest_response`, `apply_redaction`, `now_iso8601`
- `api-almanac-cli` — `almanac` binary: `list`, `run`, `spot-check`, `sketch`, `export-md` subcommands
- `src-tauri` — Tauri 2 app; exposes Tauri commands to the frontend

## Tauri commands

| Command | Description |
|---|---|
| `greet(name)` | Smoke-test command |
| `execute_request(method, url, headers, query, body_content?, body_kind?)` | Run an ad-hoc HTTP request; returns `HttpResponse` |
| `open_project()` | Open a folder-picker dialog, assign UIDs, normalize file names, and load the project; returns `ProjectData` |
| `get_request(file_path)` | Load a single request definition by path relative to project root; returns `RequestData` |
| `run_project_request(file_path, env_id?, case_name?)` | Run a project request with env variable substitution; returns `HttpResponse` |
| `save_request(file_path, data)` | Write edited `RequestData` back to its YAML file |
| `get_session_vars()` | Return all session-captured variables as `Record<string,string>` |
| `clear_session_vars()` | Clear all captured session variables |
| `sketch_json(body)` | Infer TypeSketch YAML from a JSON string; returns YAML string |
| `save_sketch(request_id, yaml)` | Write sketch to `sketches/{id}.typesketch.yaml` |
| `export_request_markdown(file_path)` | Generate Markdown notebook for a request; writes to `docs/` and returns relative path |
| `get_latest_response(request_id)` | Load last persisted response from `.api-almanac/responses/`; returns `StoredResponse | null` |
| `list_environments()` | Load all environments with full vars; returns `Vec<EnvironmentData>` |
| `save_environment(data)` | Write `environments/{id}.yaml`, returns updated `ProjectData` |
| `create_environment(name)` | Slugify name to id, create empty env YAML, returns `ProjectData` |
| `delete_environment(env_id)` | Remove env file, returns updated `ProjectData` |
| `list_plugins()` | List plugin manifests from `tools/*.yaml` in the open project |
| `run_plugin_command(plugin_id, file_path, response_*)` | Run a plugin against the current response; returns `PluginOutput` |
| `create_group(folder)` | Create directory `requests/{folder}` with `.gitkeep`; returns updated `ProjectData` |
| `rename_group(old_folder, new_folder)` | Rename request directory; returns updated `ProjectData` |
| `delete_group(folder)` | Remove request directory and all contents; returns updated `ProjectData` |
| `delete_request(file_path)` | Delete a request YAML file; returns updated `ProjectData` |
| `rename_request(file_path, new_name)` | Update `name:` field in YAML and rename the file to match (numeric prefix preserved, `id` unchanged); returns `MoveResult` |
| `move_request(file_path, new_folder)` | Move YAML file to different folder; returns `MoveResult { new_file_path, project }` |

## AppState

`AppState { project_path: Mutex<Option<PathBuf>> }` — managed Tauri state tracking the currently open project directory.

## Frontend layout (M3)

Two-column grid: 220 px sidebar + main area.

- **Sidebar** — Open Project button, project name, env selector, request tree grouped by folder with method badges
- **Main area** — URL bar (method select + URL input + Save/Run buttons), request pane (Params/Headers/Body tabs), response pane (status badge, Body/Headers tabs)

**Run returns `RunResult`** (`response`, `checks`, `captured`) when in project mode. Ad-hoc returns plain `HttpResponse`.

**Request pane tabs:** Params · Headers · Body · Notes (dot indicator when non-empty; notes are saved to YAML and included in Markdown export)

**Mode distinction:**
- *Ad-hoc mode* (no request selected): URL bar is free-form; Send calls `execute_request`
- *Project mode* (request selected): URL shows template (`{{base_url}}/...`); Run calls `run_project_request` applying env substitution from Rust; Save persists edits to YAML; dirty dot in tab bar indicates unsaved changes

## Model crate modules (api-almanac-model)

| Module | Contents |
|---|---|
| `project` | `AlmanacProject` — top-level `almanac.yaml` struct |
| `request` | `RequestDef`, `Case` — full request definition with headers, body, cases, capture, redact |
| `environment` | `Environment` — named var set |
| `body` | `RequestBody`, `BodyKind` — json / text / form body |
| `resolver` | `VariableResolver` — resolves `{{var}}` templates against env + case |
| `resolved` | `ResolvedRequest`, `ResolvedBody` — fully substituted, ready for HTTP execution |
| `loader` | `ProjectLoader` — reads `almanac.yaml`, `environments/*.yaml`, `requests/**/*.yaml` |
| `checker` *(runner crate)* | `run_checks(expect, response) → Vec<Check>`, `apply_captures(capture_map, response) → HashMap` |
| `error` | `ModelError` — io / yaml / json / not-found variants |

## Key conventions

- YAML files are the canonical source of truth for project data
- Markdown is generated output only — never parse it back
- Secrets are referenced symbolically (`{{secret.FOO}}`), never stored in YAML
- Responses and history live under `.api-almanac/` to keep the project tree clean
- Template variables use `{{double_braces}}` syntax
- `{{secret.VAR_NAME}}` in environment var values reads OS env var `VAR_NAME` at runtime — not stored in YAML
