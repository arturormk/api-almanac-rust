# API Almanac

**A local-first workbench for exploring, saving, and rerunning HTTP API calls.**

API Almanac is a free, open-source desktop app that turns exploratory HTTP calls into durable, revisitable knowledge. Send requests, save responses, generate type sketches, add notes, and run lightweight spot checks — all in plain YAML files you own.

> API Almanac is not an enterprise API platform. It is a developer's field notebook for APIs, with executable requests and response archaeology.

---

## Features

- **Request editor** — method, URL, query params, headers, body (JSON / form / text), cases
- **Environment system** — named variable sets (`local`, `staging`, `production`) with `{{double_braces}}` template syntax; secrets read from OS environment variables at runtime via `{{secret.VAR_NAME}}`
- **Response viewer** — pretty-printed JSON, headers, status badge, duration
- **TypeSketch** — automatically infers the observed response shape as a readable YAML sketch
- **Expectations & captures** — lightweight assertions on status, headers, and JSON fields; capture values from responses for use in subsequent requests
- **Notes** — per-request freeform notes saved to YAML and included in Markdown exports
- **Spot-check runner** — run all project requests in sequence, carry captured values forward, export a Markdown report
- **Markdown export** — generate a readable Markdown notebook for any request: definition, cases, expectations, last response, and type sketch
- **Response persistence** — saves the last response per request to `.api-almanac/`; shows it automatically when you reopen a request
- **Analyzer plugins** — run any external executable against the current response; receive HTML / YAML / Markdown artifacts back. Plugins can be written in any language.
- **CLI** — `almanac` binary for `list`, `run`, `spot-check`, `sketch`, and `export-md` without opening the GUI

### Local-first and Git-friendly

All project data lives in plain YAML files you control. No accounts, no cloud, no lock-in. Projects are structured to diff cleanly and commit safely.

```
my-api/
  almanac.yaml
  environments/
    local.yaml
    staging.yaml
  requests/
    auth/
      login.yaml
    users/
      create.yaml
      get.yaml
  .api-almanac/        ← generated; add to .gitignore if preferred
    responses/
```

---

## Screenshots

> _Screenshots coming soon._

---

## Getting started

### Prerequisites

| Tool | Version |
|---|---|
| [Rust](https://rustup.rs) | stable (2021 edition) |
| [Node.js](https://nodejs.org) | 18 or later |
| [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) | platform-specific (WebView2 on Windows, webkit2gtk on Linux) |

### Run in development

```bash
git clone https://github.com/your-org/apialmanac-rust
cd apialmanac-rust
npm install
npm run tauri dev
```

The app window opens automatically. The Vite dev server runs at `http://localhost:1420` and Rust hot-reloads on file change.

### Build for release

```bash
npm run tauri build
```

The installer is written to `src-tauri/target/release/bundle/`.

### Run the CLI

```bash
cargo build -p api-almanac-cli

# list all requests in the current project
cargo run -p api-almanac-cli -- list

# run a single request with environment substitution
cargo run -p api-almanac-cli -- run requests/auth/login.yaml --env local

# run all requests as a spot check
cargo run -p api-almanac-cli -- spot-check --env staging

# infer a type sketch from the last response
cargo run -p api-almanac-cli -- sketch users.get --env local

# export a request to Markdown
cargo run -p api-almanac-cli -- export-md requests/users/get.yaml
```

The CLI walks up from the current directory to find `almanac.yaml`. Request arguments accept a file path (`requests/auth/login.yaml`) or a request ID (`auth.login`).

---

## Project structure

```
Cargo.toml                   workspace root
src-tauri/                   Tauri 2 backend + app config
  src/lib.rs                 Tauri commands
crates/
  api-almanac-model/         request / environment / project structs + YAML serde
  api-almanac-runner/        async HTTP executor (reqwest)
  api-almanac-typesketch/    infer response shape as YAML sketch
  api-almanac-export/        Markdown notebook generation
  api-almanac-tools/         external analyzer plugin contract
  api-almanac-store/         response persistence + redaction
  api-almanac-cli/           almanac binary
src/                         React + TypeScript frontend (Vite)
docs/
  BLUEPRINT.md               product vision and milestone plan
examples/
  tools/                     sample analyzer plugins
```

---

## Project file format

### `almanac.yaml`

```yaml
id: my-api
name: My API
description: Notes about the My API REST service
```

### `environments/local.yaml`

```yaml
id: local
name: Local

vars:
  base_url: http://localhost:8000
  auth.token: "{{secret.LOCAL_API_TOKEN}}"
```

Values using `{{secret.VAR_NAME}}` read the OS environment variable `VAR_NAME` at runtime. The secret value is never written to disk.

### `requests/users/get.yaml`

```yaml
id: users.get
name: Get user
method: GET
url: "{{base_url}}/users/{{user_id}}"

headers:
  Authorization: "Bearer {{auth.token}}"
  Accept: application/json

query:
  include: profile

expect:
  default:
    status: 200
    json:
      id: exists
      email: exists

capture:
  last_user.id: json.id

redact:
  - headers.Authorization

notes: |
  Run auth.login first to populate auth.token in the session.
  The returned user ID is captured as last_user.id.
```

---

## Analyzer plugins

Plugins are external executables. API Almanac sends request + response data as JSON on stdin; the plugin writes artifacts to stdout.

Place a manifest YAML and executable in your project's `tools/` directory:

**`tools/my-plugin.yaml`**
```yaml
id: my-plugin
name: My Plugin
command:
  executable: python3
  args:
    - tools/my-plugin.py
```

**`tools/my-plugin.py`**
```python
import json, sys

bundle = json.load(sys.stdin)
body   = bundle["response"]["body"]

json.dump({
    "title": "My Plugin",
    "artifacts": [{"kind": "html", "title": "Result", "content": f"<p>{body}</p>"}],
    "diagnostics": [],
}, sys.stdout)
```

See `examples/tools/` for a working sample. Plugins can be written in any language that reads stdin and writes stdout.

---

## Running tests

```bash
cargo test --workspace
```

---

## Contributing

Contributions are welcome. Please open an issue to discuss significant changes before submitting a pull request.

- Code style: `cargo fmt` and `cargo clippy` before committing
- New Tauri commands should have corresponding TypeScript types in `src/App.tsx`
- New model behaviour should have unit tests in the relevant crate

---

## License

MIT — see [LICENSE](LICENSE).

---

## Roadmap

See [`docs/BLUEPRINT.md`](docs/BLUEPRINT.md) for the full product vision. Some areas still to explore:

- Response history (timestamped, not just latest)
- Flow definitions (ordered multi-request sequences)
- Secret backend (OS keyring, `.env` file)
- Response diff view
- OpenAPI import / hint generation
- Obsidian plugin integration
