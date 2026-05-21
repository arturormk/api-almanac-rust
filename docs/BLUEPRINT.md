# API Almanac

**A local-first workbench for exploring, saving, and rerunning HTTP API calls.**

API Almanac is a proposed open-source developer tool for learning, exploring, testing, remembering, and revisiting HTTP APIs. It is not intended to be a strict API documentation platform, an enterprise governance system, an OpenAPI compliance editor, or a replacement for a formal test suite. Its purpose is more practical and personal: help a developer understand how an API works by using it, save the calls and results that mattered, and make that knowledge easy to return to months or years later.

This document is a descriptive blueprint, not a binding specification. It captures the current conception of the project so development can begin with a coherent direction. Every decision here is open to revision as the implementation teaches us more.

---

## 1. Product identity

### 1.1 Working name

The project name is:

> **API Almanac**

The current tagline is:

> **A local-first workbench for exploring, saving, and rerunning HTTP API calls.**

The name emphasizes durable, revisitable API knowledge rather than strict documentation or formal compliance. An almanac is something you consult later. It contains useful facts, observations, recurring checks, patterns, and practical notes. This matches the intended use case: “I figured out how this API worked once; now I want to remember, rerun, and adapt that knowledge later.”

### 1.2 Positioning

API Almanac should be understood as:

* A local-first API exploration tool.
* A practical notebook for executable API calls.
* A response memory and analysis environment.
* A lightweight spot-check runner.
* A tool-friendly workspace where external analyzers can turn API responses into readable artifacts.

API Almanac should not primarily present itself as:

* An enterprise API lifecycle platform.
* A strict OpenAPI authoring tool.
* A formal contract-testing platform.
* A hosted collaboration SaaS.
* A compliance or governance product.
* A documentation generator first and foremost.

Documentation may emerge from usage, but documentation is not the central product. The central product is the developer’s working memory of APIs they actually call.

### 1.3 Basic product promise

A developer should be able to use API Almanac to answer questions like:

* How did I authenticate against this API?
* What request body worked for this endpoint?
* What were the important headers?
* What did the response look like?
* What fields did I need to consume?
* What weird behavior did I notice?
* Can I rerun these requests today and see whether the API still behaves?
* Can I generate a lightweight sketch of this response shape?
* Can I export my API notes as readable Markdown?

A concise product statement might be:

> API Almanac helps developers keep executable notes about APIs: requests, environments, cases, captures, expectations, responses, sketches, and reminders for future use.

---

## 2. Core philosophy

### 2.1 Local-first

API Almanac should be local-first by default.

Project data should live in ordinary files on the developer’s machine. The project should be suitable for Git, backups, review, and long-term preservation. The tool should not require a cloud account or hosted service to be useful.

The local-first approach matters because API Almanac is partly about memory. A developer may return to an API project years later. The files should remain understandable, portable, and recoverable even if the GUI is unavailable.

### 2.2 File-based and Git-friendly

The canonical project should be represented as plain files. The current direction is to use YAML as the canonical project format.

The files should be:

* Human-readable enough to inspect.
* Machine-readable enough to be stable.
* Friendly to Git diffs.
* Usable by command-line tools.
* Usable by future Python or Rust libraries.
* Reasonably resilient to version changes.

Generated artifacts such as Markdown notebooks, response sketches, or reports may also live in the project, but the core source of truth should remain simple and explicit.

### 2.3 Exploration over ceremony

API Almanac is for developers who need to understand and use APIs in practice.

It should avoid excessive ceremony. It should not require a user to define a full OpenAPI schema just to try an endpoint. It should not force strict modeling when the user is still figuring out what matters.

The product should make it easy to start with a request, then gradually add:

* Variables.
* Environments.
* Cases.
* Expectations.
* Captures.
* Notes.
* Saved responses.
* Response sketches.
* Spot-check runs.

### 2.4 Durable memory

The app should make it easy to keep the result of exploration.

A request is not just an ephemeral action. It can become a durable note:

* This is the call.
* These are the cases I tried.
* This is what came back.
* These fields mattered.
* This is what I expected.
* This is what broke once.
* This is how to rerun it later.

This “future self” use case is central.

### 2.5 Honest informality

API Almanac may infer useful things from observed responses, but it should be careful with language.

For example, TypeSketch-style output should be described as:

* Observed response shape.
* Response sketch.
* Example-derived sketch.
* Inferred from last response.
* Inferred from saved responses.

It should avoid overstating this as:

* A guaranteed schema.
* A formal contract.
* A complete type definition.

The tool should be powerful, but honest about what it knows.

---

## 3. Relationship to existing tools

API Almanac will inevitably overlap with tools such as Postman, Bruno, Paw, Insomnia, REST Client files, and other HTTP API utilities.

The goal is not simply to clone an existing request runner.

The differentiating emphasis is:

* 100% open source and free to use.
* Local-first.
* Plain-file project structure.
* Developer-owned data.
* Exploration and revisitable memory.
* Lightweight request cases and spot checks.
* Response archaeology, especially through TypeSketch-like analysis.
* Markdown export as an API notebook view.
* Tool-friendly plugin/analyzer architecture.

A possible positioning line:

> API Almanac is not an enterprise API platform. It is a developer’s field notebook for APIs, with executable requests and response archaeology.

Another:

> API Almanac is for people who want their API test calls to remain usable knowledge.

---

## 4. Canonical project format

### 4.1 YAML as the source of truth

The current decision is that YAML should be the canonical project format.

Markdown was considered as a possible executable source format, but the current direction is safer:

> YAML is the canonical project format. Markdown is an export, notebook, or documentation view.

This avoids making Markdown carry too much machine-readable responsibility. It also avoids the need to parse arbitrary human-authored Markdown back into a reliable internal model.

The GUI can edit YAML-backed project structures. Markdown can then be generated from those structures.

### 4.2 Why not Markdown as the canonical format?

Markdown remains attractive because it can make API calls feel like notebook pages. A Markdown request could contain:

* Human notes.
* HTTP request block.
* JSON body block.
* Variables.
* Expectations.
* Captures.
* Last response.

However, if Markdown becomes the source of truth, the app must parse and preserve a semi-structured document while users edit it freely. This risks becoming fragile, especially once cases, environments, captures, response history, and plugin artifacts are added.

Markdown is still valuable, but as a generated view:

* Easy to read in Obsidian.
* Easy to render on GitHub.
* Useful for sharing.
* Useful for documentation.
* Useful as a long-term notebook artifact.

### 4.3 Possible project tree

A possible workspace layout:

```text
my-api-almanac/
  almanac.yaml
  environments/
    local.yaml
    staging.yaml
    production.yaml
  requests/
    auth/
      login.yaml
      refresh-token.yaml
      me.yaml
    users/
      create.yaml
      get.yaml
      list.yaml
      delete.yaml
  responses/
    users.get.latest.json
    users.create.latest.json
  sketches/
    users.get.latest.typesketch.yaml
  docs/
    auth/
      login.md
    users/
      get.md
      create.md
  .api-almanac/
    history/
      ...
```

This is illustrative only. The final layout may differ.

One likely pattern is:

* User-authored canonical files remain visible and clean.
* Generated history and internal metadata live under `.api-almanac/`.
* Selected responses, sketches, and Markdown exports can be written to visible folders when useful.

### 4.4 Request YAML example

A request file might look like this:

```yaml
id: users.create
name: Create user
tags:
  - users
  - write
  - smoke-test

method: POST
url: "{{base_url}}/users"

headers:
  Authorization: "Bearer {{auth.token}}"
  Content-Type: application/json
  Accept: application/json

body:
  kind: json
  value:
    name: "{{user.name}}"
    email: "{{user.email}}"
    role: "{{user.role}}"
    send_welcome_email: "{{user.send_welcome_email}}"

cases:
  normal-user:
    user.name: Ada Lovelace
    user.email: ada@example.com
    user.role: user
    user.send_welcome_email: false

  admin-user:
    user.name: Grace Hopper
    user.email: grace@example.com
    user.role: admin
    user.send_welcome_email: false

  invalid-email:
    user.name: Invalid Email
    user.email: not-an-email
    user.role: user
    user.send_welcome_email: false

expect:
  default:
    status: 201
    time_ms: "< 750"
    headers:
      Content-Type: "contains application/json"
    json:
      id: exists
      name: "equals {{user.name}}"
      email: "equals {{user.email}}"
      role: "equals {{user.role}}"
      created_at: exists

  cases:
    invalid-email:
      status: 422
      json:
        error: exists
        error.code: "equals invalid_email"

capture:
  created_user.id: json.id
  created_user.email: json.email

redact:
  - headers.Authorization
  - headers.Set-Cookie
  - json.access_token
  - json.refresh_token
  - json.password
  - json.token

tools:
  typesketch:
    enabled: true
    save: true
```

This is not a final schema. It is a sketch of the kind of model the app may need.

### 4.5 Environment files

Environment files might provide values such as:

```yaml
id: local
name: Local

vars:
  base_url: http://localhost:8000
  auth.token: "{{secret.LOCAL_API_TOKEN}}"
```

Another environment:

```yaml
id: staging
name: Staging

vars:
  base_url: https://staging.example.com
  auth.token: "{{secret.STAGING_API_TOKEN}}"
```

Secrets should not be stored in plain YAML unless the user explicitly chooses that tradeoff. The default design should prefer references to secrets, not secret values.

### 4.6 Secrets

Secrets should be referenced symbolically:

```yaml
auth.token: "{{secret.EXAMPLE_API_TOKEN}}"
```

Potential secret backends:

* OS keyring.
* Local encrypted store.
* `.env` file.
* Environment variables.
* CI secret provider.

The project files should be safe to commit by default.

---

## 5. Markdown export

### 5.1 Markdown as notebook view

Markdown should be treated as an export or generated notebook view, not the canonical internal format.

The generated Markdown should be useful in:

* Obsidian.
* GitHub.
* VS Code.
* Static documentation sites.
* Long-term archival folders.

An Obsidian plugin could later refresh or render these files, but that is not part of the core API Almanac product at first.

### 5.2 Example generated Markdown

A generated Markdown file might look like:

````markdown
# Create user

Creates a new user in the API.

This request is useful when testing onboarding, permissions, and later user lookup flows. The returned user ID is captured as `created_user.id` so that follow-up requests can reuse it.

Related requests:

- [Login](../auth/login.md)
- [Get user](./get-user.md)
- [List users](./list-users.md)

---

## Request

```http
POST {{base_url}}/users
Authorization: Bearer {{auth.token}}
Content-Type: application/json
Accept: application/json
```

```json
{
  "name": "{{user.name}}",
  "email": "{{user.email}}",
  "role": "{{user.role}}",
  "send_welcome_email": {{user.send_welcome_email}}
}
```

---

## Cases

| Case | user.name | user.email | user.role | user.send_welcome_email |
|---|---|---|---|---|
| normal-user | Ada Lovelace | ada@example.com | user | false |
| admin-user | Grace Hopper | grace@example.com | admin | false |
| invalid-email | Invalid Email | not-an-email | user | false |

---

## Expectations

```yaml
status: 201
time_ms: < 750
headers:
  Content-Type: contains application/json
json:
  id: exists
  name: equals {{user.name}}
  email: equals {{user.email}}
  role: equals {{user.role}}
  created_at: exists
```

---

## Last response

```http
HTTP/1.1 201 Created
Content-Type: application/json
Location: /users/usr_123

{
  "id": "usr_123",
  "name": "Ada Lovelace",
  "email": "ada@example.com",
  "role": "user",
  "send_welcome_email": false,
  "created_at": "2026-05-20T13:12:30Z"
}
```

---

## Observed response sketch

```yaml
id: string
name: string
email: email
role: string
send_welcome_email: boolean
created_at: datetime
```

---

## Notes

Run [Login](../auth/login.md) before this request if `auth.token` is not already available.

After a successful run, try [Get user](./get-user.md), which can use `{{created_user.id}}`.
````

### 5.3 Markdown export principle

The Markdown export should answer:

* What is this call?
* Why does it exist?
* How is it run?
* What cases are interesting?
* What is expected?
* What happened last time?
* What shape did the response have?
* What should future me remember?

Markdown is not merely a pretty serialization. It is a readable API memory artifact.

---

## 6. Core concepts

### 6.1 Request

A request is an executable HTTP call.

It includes:

* Method.
* URL.
* Headers.
* Query parameters.
* Body.
* Authentication references.
* Variables.
* Cases.
* Expectations.
* Captures.
* Redaction rules.
* Notes.
* Tool/analyzer settings.

### 6.2 Environment

An environment is a named set of variables and secret references.

Examples:

* Local.
* Staging.
* Production.
* Customer sandbox.
* Mock server.

### 6.3 Case

A case is a named variation of a request.

For example:

* `normal-user`.
* `admin-user`.
* `invalid-email`.
* `missing-token`.
* `large-page`.
* `empty-result`.

Cases allow a user to clone the intent of a request without creating many nearly identical request files.

The UI can expose cases as a dropdown or side panel.

### 6.4 Capture

A capture extracts values from a response and stores them for later use.

Example:

```yaml
capture:
  auth.token: json.access_token
  auth.refresh_token: json.refresh_token
  created_user.id: json.id
```

This supports flows such as:

1. Login.
2. Capture token.
3. Create user.
4. Capture user ID.
5. Fetch user.
6. Delete user.

Captures should be scoped carefully. Some may be session-only; others may be saved if the user chooses.

### 6.5 Expectation

An expectation is a lightweight assertion about a response.

Examples:

```yaml
status: 200
json.id: exists
json.email: "equals {{user.email}}"
header.Content-Type: "contains application/json"
time_ms: "< 500"
```

Expectations are not meant to replace a full formal test suite. They support spot checks, smoke tests, and practical confidence.

### 6.6 Redaction

Redaction rules prevent sensitive values from being saved into project files, response histories, Markdown exports, or generated artifacts.

Examples:

```yaml
redact:
  - headers.Authorization
  - headers.Set-Cookie
  - json.access_token
  - json.refresh_token
  - json.password
  - json.token
```

Redaction is critical because local-first and Git-friendly workflows should not accidentally leak tokens or private data.

### 6.7 Response

A response may be saved as:

* Latest response.
* Timestamped response history.
* Redacted response.
* Metadata-only response.
* Response summary.

Response-saving modes may include:

* Do not save response.
* Save last response.
* Save response summary.
* Save full history.

The default should probably be conservative.

### 6.8 Artifact

An artifact is derived from a request, response, run, or project.

Artifacts may include:

* HTML visualizations.
* YAML TypeSketch output.
* Markdown summaries.
* Response diffs.
* Generated code.
* Tables.
* Charts.
* Spot-check reports.

Artifacts may be generated by built-in tools or external executables.

---

## 7. Response analysis and TypeSketch

### 7.1 TypeSketch integration

The existing TypeSketch project is highly aligned with API Almanac.

TypeSketch informally documents JSON by turning examples into concise, type-oriented YAML sketches. This is useful when learning an API response because the hard part is often not merely sending the request, but understanding the shape of the data returned.

API Almanac should integrate TypeSketch-style response analysis as a first-class concept.

A typical workflow:

1. Run request.
2. Inspect raw response.
3. Generate response sketch.
4. Notice fields, arrays, optional structures, dates, URLs, IDs, and nested objects.
5. Save sketch.
6. Add notes.
7. Rerun later and compare.

### 7.2 Rust rewrite

The current direction is to rewrite TypeSketch in Rust for API Almanac.

Possible forms:

* A Rust crate used internally by the app.
* A bundled CLI executable.
* Both.

A crate might be named:

```text
api-almanac-typesketch
```

A CLI might support:

```bash
api-almanac-typesketch analyze --format yaml < response.json
api-almanac-typesketch analyze --format html < response-bundle.json
```

Even if TypeSketch is built into API Almanac, it should ideally also work as a standalone command-line tool.

### 7.3 TypeSketch output language

The UI should avoid calling TypeSketch output a formal schema unless formal schema generation is actually implemented.

Preferred terms:

* Response sketch.
* Observed shape.
* Type sketch.
* Example-derived shape.
* Inferred from latest response.
* Inferred from saved responses.

Less preferred terms:

* Contract.
* Guaranteed schema.
* Complete schema.

### 7.4 Example sketch

A JSON response like:

```json
{
  "id": "usr_123",
  "email": "ada@example.com",
  "name": "Ada Lovelace",
  "created_at": "2026-05-20T13:12:30Z",
  "roles": ["admin"],
  "profile": {
    "avatar_url": "https://example.com/avatar.png",
    "bio": null
  }
}
```

Might become:

```yaml
id: string
email: email
name: string
created_at: datetime
roles:
  - string
profile:
  avatar_url: url
  bio: string | null
```

### 7.5 Sketches as loose expectations

Later, saved sketches could become loose response-shape expectations.

For example:

```yaml
expect:
  status: 200
  body_matches_sketch: sketches/users.get.latest.typesketch.yaml
```

This would support drift detection:

* Field disappeared.
* Field changed type.
* Array item shape changed.
* New field appeared.
* Field became nullable.

This should remain practical rather than formal unless the project later adds JSON Schema/OpenAPI generation.

---

## 8. Plugin and tool architecture

### 8.1 External executable analyzers

A major architectural idea is to support response analysis through external executables.

Instead of starting with a heavy native plugin ABI, API Almanac can define a simple tool contract:

> API Almanac runs an external executable, sends it request/response data, and receives one or more artifacts to display or save.

This is powerful, neutral, and language-agnostic.

A plugin or analyzer could be written in:

* Rust.
* Python.
* Node.js.
* Go.
* Bash.
* Any language that can read stdin and write stdout.

This fits the project because API Almanac does not need extreme plugin performance. Typical use may involve running a few requests interactively or a few dozen requests as a spot check. Process startup overhead is acceptable if the architecture remains simple and open.

### 8.2 Why external executables are attractive

External command analyzers provide:

* Language neutrality.
* Easy debugging.
* Easy experimentation.
* Compatibility with existing scripts.
* Low coupling between core app and extensions.
* No Rust ABI problem.
* No need to embed Python.
* A Unix-like tool philosophy.

The core app can focus on:

* Running requests.
* Managing environments.
* Saving responses.
* Applying redaction.
* Calling tools.
* Displaying artifacts.

Tools can focus on:

* Analyzing responses.
* Rendering visualizations.
* Generating code.
* Producing reports.
* Comparing outputs.

### 8.3 Plugin contract

The simplest contract:

* Plugin receives JSON on stdin.
* Plugin writes JSON on stdout.
* Plugin writes diagnostics to stderr.
* Non-zero exit indicates failure.

Input bundle example:

```json
{
  "api_almanac_plugin_api": "0.1",
  "request": {
    "id": "users.get",
    "name": "Get user",
    "method": "GET",
    "url": "https://api.example.com/users/usr_123",
    "headers": {
      "Accept": "application/json"
    }
  },
  "response": {
    "status": 200,
    "headers": {
      "Content-Type": "application/json"
    },
    "duration_ms": 184,
    "body": {
      "id": "usr_123",
      "email": "ada@example.com",
      "roles": ["admin"]
    }
  },
  "environment": {
    "name": "local"
  },
  "case": {
    "name": "normal-user"
  },
  "options": {}
}
```

Output example:

```json
{
  "title": "TypeSketch",
  "artifacts": [
    {
      "kind": "html",
      "title": "Observed response shape",
      "content": "<section><h2>Observed response shape</h2><pre><code>id: string\nemail: email\nroles:\n  - string</code></pre></section>"
    },
    {
      "kind": "yaml",
      "title": "TypeSketch YAML",
      "content": "id: string\nemail: email\nroles:\n  - string\n"
    }
  ],
  "diagnostics": []
}
```

Error example:

```json
{
  "error": {
    "message": "Response body is not valid JSON"
  }
}
```

### 8.4 Plugin manifest

A plugin manifest might look like:

```yaml
id: typesketch
name: TypeSketch
version: 0.1.0
kind: response_analyzer

command:
  executable: api-almanac-typesketch
  args:
    - analyze
    - --format
    - artifact-bundle

input:
  stdin: response_bundle_json

output:
  stdout: artifact_bundle_json

permissions:
  network: false
  read_workspace: false
  write_workspace: false
  read_secrets: false
```

This is only a sketch. The manifest should stay as simple as possible at first.

### 8.5 Artifact types

Plugin outputs should not be limited to HTML, though HTML is especially useful in a Tauri app.

Possible artifact kinds:

* `html` — rendered in an isolated panel.
* `markdown` — rendered through the app’s Markdown renderer.
* `json` — saved or inspected as structured data.
* `yaml` — saved or inspected as structured data.
* `text` — plain output.
* `file` — reference to generated file path.

### 8.6 HTML rendering safety

Plugin-generated HTML should be treated as untrusted unless it comes from a built-in trusted tool.

The viewer should aim to:

* Isolate plugin HTML from the main app UI.
* Prevent plugin HTML from invoking privileged Tauri commands.
* Restrict external network access.
* Restrict file access.
* Use a restrictive content security policy.
* Sanitize HTML where appropriate.

Plugin HTML should be an artifact, not a full extension of the app frontend.

### 8.7 Trust and permissions

The app may eventually distinguish between:

* Built-in tools.
* Workspace tools.
* User-installed tools.
* Third-party tools.

A simple warning may be enough initially:

> External command plugins run with your OS user permissions. Only install and run plugins you trust. API Almanac controls what data is passed to the plugin, and plugin-rendered HTML is isolated from the main app.

### 8.8 Future Wasm plugins

A later plugin layer could support WebAssembly-based plugins for stronger sandboxing and easier distribution.

However, command plugins are likely simpler and more useful for the first versions.

Possible extension roadmap:

1. Built-in Rust analyzers.
2. Command-based analyzers.
3. Optional WebAssembly plugins.
4. Maybe long-lived plugin servers if performance or interactivity requires them.

---

## 9. Application architecture

### 9.1 Tauri 2 GUI

The GUI is expected to be implemented using Tauri 2.

Tauri provides a native desktop shell with a webview frontend and Rust backend. This fits the desired architecture:

* Rust for core application logic.
* Web UI for rich editing and visualization.
* Cross-platform desktop packaging.
* Local filesystem access through controlled backend commands.

The frontend could be implemented in React, Svelte, Vue, or another suitable web framework. The choice is not settled by this blueprint.

### 9.2 Rust core

The Rust backend should own the core logic.

Possible crates:

```text
api-almanac-model       # project, request, environment, case model
api-almanac-runner      # HTTP execution
api-almanac-store       # response/history/artifact persistence
api-almanac-typesketch  # Rust TypeSketch implementation
api-almanac-tools       # tool/plugin execution contract
api-almanac-export      # Markdown/export generation
api-almanac-cli         # command-line interface
```

This modular design would make it easier to test the core without the GUI and expose the same behavior through a CLI.

### 9.3 CLI

A CLI should exist early, even if minimal.

Possible commands:

```bash
api-almanac validate ./my-api
api-almanac list requests
api-almanac run users.get --env local --case normal-user
api-almanac run-folder smoke --env staging
api-almanac sketch users.get --latest
api-almanac export markdown ./docs
api-almanac tools run typesketch --request users.get --latest
```

The CLI would help with:

* Testing the core.
* CI integration.
* Power-user workflows.
* Interop with Claude Code, Codex, shell scripts, and other automation.

### 9.4 Python integration

Python integration is desirable, but Python should not be part of the critical GUI runtime path initially.

Preferred Python integration:

* An independent Python package that understands the same file formats.
* Optional command plugins written in Python.
* Scripts that read/write API Almanac projects.
* Possible Python bindings to Rust TypeSketch later.

Avoid initially:

* Embedding Python inside the Tauri app.
* Making the GUI depend on a Python runtime.
* Requiring Python for core functionality.

Possible Python package:

```python
from api_almanac import Project

project = Project.open("./my-api")
request = project.requests["users.create"]

print(request.method)
print(request.url)
print(request.cases)
```

The Python package can become useful for:

* Bulk project edits.
* Migration scripts.
* CI checks.
* Custom exports.
* Integration with data science notebooks.
* External analyzers.

### 9.5 TypeSketch and Python

The existing Python TypeSketch project can inform the Rust rewrite.

Longer-term possibilities:

* Keep `typesketch-python` as an independent tool.
* Implement `api-almanac-typesketch` in Rust.
* Expose Rust TypeSketch to Python through bindings if useful.
* Let Python plugins call the Rust CLI rather than embedding it.

---

## 10. GUI concepts

### 10.1 Main workspace areas

The GUI might include:

* Project tree.
* Request editor.
* Environment selector.
* Case selector.
* Send/run controls.
* Response viewer.
* Headers/cookies/timeline tabs.
* TypeSketch/analysis tabs.
* Expectations/results tab.
* Notes panel.
* History/artifacts panel.

### 10.2 Request editor

The request editor should make common API work easy:

* Method dropdown.
* URL field.
* Query parameters table.
* Headers table.
* Auth section.
* Body editor.
* Cases editor.
* Expectations editor.
* Captures editor.
* Redaction editor.

It should be possible to use the GUI without thinking about the YAML most of the time, while preserving the YAML as the canonical project data.

### 10.3 Response viewer

The response viewer should include:

* Status.
* Duration.
* Headers.
* Body.
* Pretty JSON view.
* Raw view.
* Saved response indicator.
* Redaction status.
* Generated artifacts.

Possible tabs:

```text
Response | Headers | Cookies | Timeline | TypeSketch | Diff | Tests | Notes | Artifacts
```

### 10.4 TypeSketch tab

After a response is received, the TypeSketch tab can show:

* Observed response shape.
* YAML sketch.
* Maybe HTML visualization.
* Copy button.
* Save sketch button.
* Compare with previous sketch.
* Use as loose expectation.

### 10.5 Spot-check runner

API Almanac should support running a selected set of requests as a spot check.

This is not necessarily a full test suite. It is more like:

> “Run the handful of calls that tell me whether this API/server is basically alive and behaving as remembered.”

A run report could show:

* Passed/failed expectations.
* Status changes.
* Duration changes.
* Response shape changes.
* Notable diffs.
* Errors.

### 10.6 Notes

Notes are important because API Almanac is partly a memory tool.

Notes may exist at several levels:

* Project notes.
* Folder/group notes.
* Request notes.
* Case notes.
* Response notes.
* Artifact notes.

The app should make it easy to write down what a developer learned while exploring an API.

---

## 11. Response history and persistence

### 11.1 Response-saving modes

Different users and projects will have different needs.

Possible modes:

* Do not save responses.
* Save last response only.
* Save response metadata only.
* Save redacted last response.
* Save full redacted history.
* Save selected responses manually.

The default should avoid accidental leakage of secrets or private data.

### 11.2 Response metadata

Saved response metadata may include:

```yaml
timestamp: 2026-05-20T15:12:30+02:00
environment: local
case: normal-user
status: 201
duration_ms: 184
content_type: application/json
body_saved: true
body_redacted: true
```

### 11.3 History storage

History may be stored under `.api-almanac/` to avoid cluttering the canonical project.

Example:

```text
.api-almanac/
  history/
    users.create/
      2026-05-20T15-12-30.response.json
      2026-05-20T15-12-30.meta.yaml
      2026-05-20T15-12-30.typesketch.yaml
```

Visible exports can then be generated separately.

---

## 12. Expectations and spot checks

### 12.1 Lightweight expectations

Expectations should be simple enough to write and understand.

Possible examples:

```yaml
status: 200
time_ms: "< 500"
headers:
  Content-Type: "contains application/json"
json:
  id: exists
  email: "equals {{user.email}}"
  roles: "contains admin"
```

### 12.2 Avoid premature complexity

The app should not begin with a full custom test language unless needed.

Simple declarative checks are probably enough for the first versions.

Later escape hatches may include:

* JavaScript tests.
* External command validators.
* JSON Schema validation.
* TypeSketch-based shape drift checks.

### 12.3 Run reports

Spot-check run reports should be readable and useful.

They may include:

* Request name.
* Environment.
* Case.
* Status.
* Duration.
* Pass/fail expectations.
* Captured values.
* Response sketch changes.
* Error messages.

Reports could be exportable as Markdown or HTML.

---

## 13. Captures and flows

### 13.1 Captures

Captures allow later requests to use values from earlier responses.

Example login flow:

```yaml
capture:
  auth.token: json.access_token
  auth.refresh_token: json.refresh_token
```

Example create-user flow:

```yaml
capture:
  created_user.id: json.id
```

### 13.2 Flow support

API Almanac may eventually support named flows:

```yaml
id: user-lifecycle
name: User lifecycle
steps:
  - request: auth.login
    case: admin
  - request: users.create
    case: normal-user
  - request: users.get
    vars:
      user_id: "{{created_user.id}}"
  - request: users.delete
    vars:
      user_id: "{{created_user.id}}"
```

This is not necessarily MVP, but the data model should not make it impossible.

---

## 14. Redaction and privacy

### 14.1 Redaction is essential

API Almanac will handle real API calls. Responses may contain:

* Tokens.
* Cookies.
* Password-like fields.
* Personal data.
* Internal IDs.
* Emails.
* Customer data.

Redaction should be a first-class feature, not an afterthought.

### 14.2 Redaction timing

Redaction should happen before:

* Saving responses.
* Generating Markdown exports.
* Passing data to untrusted plugins, if configured.
* Writing run reports.

### 14.3 Tool data policy

When running external tools, the app should control what is passed.

Possible options:

* Pass full redacted response.
* Pass metadata only.
* Pass body only.
* Pass selected fields.
* Pass unredacted response only for trusted tools and explicit user approval.

---

## 15. Example external analyzer plugin

A Python analyzer could be as simple as:

```python
#!/usr/bin/env python3

import html
import json
import sys

bundle = json.load(sys.stdin)
body = bundle.get("response", {}).get("body")

if isinstance(body, dict):
    items = "".join(
        f"<li><code>{html.escape(str(key))}</code></li>"
        for key in body.keys()
    )
    content = f"""
    <section>
      <h2>Top-level response keys</h2>
      <ul>{items}</ul>
    </section>
    """
else:
    content = "<section><p>Response body is not a JSON object.</p></section>"

json.dump({
    "title": "Response Keys",
    "artifacts": [
        {
            "kind": "html",
            "title": "Top-level keys",
            "content": content
        }
    ],
    "diagnostics": []
}, sys.stdout)
```

This illustrates the desired spirit: any developer can create a useful analyzer with minimal ceremony.

---

## 16. Possible built-in analyzers

Initial or eventual built-in analyzers might include:

### 16.1 TypeSketch

Generate observed response shape from JSON.

### 16.2 Response diff

Compare latest response with a previous saved response.

### 16.3 JSON table

Render an array of objects as an HTML table.

### 16.4 Sensitive data detector

Warn before saving or exporting likely secrets.

### 16.5 OpenAPI hint generator

Generate a rough OpenAPI fragment from an observed request/response.

This should be clearly labeled as a hint, not authoritative documentation.

### 16.6 TypeScript type generator

Generate approximate TypeScript interfaces from observed JSON or TypeSketch output.

### 16.7 Python model generator

Generate approximate Python dataclasses or Pydantic models from observed JSON or TypeSketch output.

### 16.8 Availability report

Turn a batch run into a readable summary of API/server availability.

---

## 17. Relationship to OpenAPI

API Almanac should not require OpenAPI.

However, it may later support:

* Import from OpenAPI.
* Export rough OpenAPI hints.
* Link requests to OpenAPI operations.
* Compare observed responses against OpenAPI schemas.

The product should remain useful when no OpenAPI spec exists, when the spec is incomplete, or when the developer is simply consuming someone else’s API and wants practical working notes.

A guiding principle:

> API Almanac should help you when the official documentation is missing, stale, overwhelming, or insufficiently practical.

---

## 18. Relationship to Obsidian

Markdown export makes API Almanac naturally compatible with Obsidian workflows.

Possible future Obsidian plugin features:

* Refresh Markdown exports from API Almanac project files.
* Render request summaries.
* Link API notes with broader project notes.
* Show latest response sketch.
* Open a request in API Almanac.

However, Obsidian integration should not be a core requirement for API Almanac proper.

API Almanac should produce useful Markdown. Other tools can consume it.

---

## 19. Development approach

### 19.1 Suggested initial milestones

#### Milestone 0 — Project skeleton

* Create repository.
* Set up Rust workspace.
* Set up Tauri 2 app.
* Set up basic frontend.
* Define initial project file layout.
* Add basic test infrastructure.

#### Milestone 1 — Core model

* Define request/environment/case structs.
* Load/save YAML.
* Validate basic project files.
* Resolve variables for selected environment and case.
* Render final HTTP request.

#### Milestone 2 — Runner

* Execute simple HTTP requests.
* Support headers, query params, and JSON body.
* Display response status, headers, duration, and body.
* Save latest response.

#### Milestone 3 — GUI editing

* Project tree.
* Request editor.
* Environment selector.
* Case selector.
* Send button.
* Response viewer.

#### Milestone 4 — Expectations and captures

* Implement simple expectations.
* Show pass/fail results.
* Implement captures.
* Store captured values in session scope.

#### Milestone 5 — TypeSketch MVP

* Port core TypeSketch logic to Rust.
* Generate YAML sketch from JSON response.
* Show sketch in GUI tab.
* Save latest sketch.

#### Milestone 6 — Markdown export

* Generate Markdown notebook files from YAML project.
* Include request, cases, expectations, latest response, and TypeSketch output.

#### Milestone 7 — Command analyzer plugins

* Define plugin input/output JSON contract.
* Run external executable.
* Capture artifacts.
* Render HTML/Markdown artifacts safely.
* Add one sample Python analyzer.

#### Milestone 8 — Spot-check runner

* Run selected request group.
* Show summary report.
* Save run report.
* Export report as Markdown/HTML.

### 19.2 Keep the MVP narrow

The first usable product does not need:

* Cloud sync.
* Team collaboration.
* Full plugin marketplace.
* Full OpenAPI support.
* Embedded Python.
* Complex scripting language.
* Full formal contract testing.
* Perfect Markdown round-trip editing.

A good MVP might simply be:

> Create requests, run them, save responses, generate TypeSketch sketches, add notes, and export Markdown.

---

## 20. Risks and design cautions

### 20.1 YAML complexity

YAML can become messy if the model grows too quickly.

Mitigation:

* Keep the first schema simple.
* Prefer explicit structures.
* Provide migrations when format changes.
* Avoid overly clever templating.

### 20.2 Markdown overpromising

Markdown export should not become a second canonical format too early.

Mitigation:

* Treat Markdown as generated.
* Avoid expecting arbitrary Markdown round-trips.
* Allow manual notes in YAML or dedicated notes fields.

### 20.3 Plugin safety

External executables are powerful but risky.

Mitigation:

* Make plugin execution explicit.
* Clearly label trust levels.
* Redact before passing data when configured.
* Isolate rendered HTML.
* Avoid giving plugin HTML access to Tauri commands.

### 20.4 Scope creep toward Postman clone

The app could drift toward becoming a large generic API client.

Mitigation:

* Keep the “Almanac” identity visible.
* Prioritize memory, sketches, notes, and rerunnable knowledge.
* Avoid enterprise-only features early.

### 20.5 Scope creep toward documentation platform

Markdown export and response sketches could make the app look like a documentation generator.

Mitigation:

* Keep documentation secondary.
* Focus on executable exploration first.
* Present generated docs as notebook artifacts.

---

## 21. Open questions

Important open questions include:

* Exact YAML schema.
* Project folder layout.
* Frontend framework for Tauri UI.
* Secret storage backend.
* Whether notes live in YAML, Markdown sidecars, or both.
* How response history should be pruned or retained.
* How much request scripting is needed.
* Whether flows belong in the first version.
* How strict expectations should be.
* How TypeSketch should represent optional/null/mixed fields across multiple responses.
* How external plugin permissions should be presented to users.
* Whether the CLI should be built before or after the GUI MVP.
* Whether Markdown export should be one file per request, one file per folder, or both.

---

## 22. One-sentence essence

> API Almanac is a local-first API workbench that turns exploratory HTTP calls into durable, rerunnable knowledge: requests, cases, responses, sketches, notes, and lightweight spot checks.

---

## 23. Current architectural summary

Current preferred direction:

```text
Tauri 2 GUI
  -> Rust core
      -> YAML project model
      -> HTTP runner
      -> response store
      -> TypeSketch analyzer
      -> Markdown exporter
      -> command analyzer/plugin runner
  -> optional CLI using same core
  -> optional Python package reading/writing same formats
```

Canonical project data:

```text
YAML
```

Generated human-readable notebook view:

```text
Markdown
```

Response analysis:

```text
Built-in Rust TypeSketch + external executable analyzers
```

Plugin philosophy:

```text
Pipe request/response bundles to tools; receive artifacts back.
```

Product philosophy:

```text
Explore APIs by using them. Save what you learn. Rerun what matters. Help future you remember.
```

