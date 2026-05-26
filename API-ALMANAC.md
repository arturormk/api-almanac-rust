# API Almanac — Project Format Reference

This document is a complete specification of the API Almanac project format. Its primary audience is an LLM that has been given an API documentation source (OpenAPI spec, docs page, cURL examples, etc.) and needs to produce a valid API Almanac project from it. Read this document in full before generating any files.

---

## Project directory layout

```
my-project/
├── almanac.yaml              # required — project identity
├── environments/             # one file per named environment
│   ├── local.yaml
│   ├── staging.yaml
│   └── production.yaml
├── requests/                 # one file per HTTP request; nest freely
│   ├── auth/
│   │   ├── login.yaml
│   │   └── refresh.yaml
│   └── users/
│       ├── list.yaml
│       ├── get.yaml
│       ├── create.yaml
│       └── delete.yaml
├── docs/                     # generated Markdown exports — do not author
├── sketches/                 # generated type sketches — do not author
└── .api-almanac/             # runtime cache — do not author
```

- `requests/` is scanned recursively; any depth of nesting is fine.
- `environments/` is scanned non-recursively; all env files are direct children.
- `docs/`, `sketches/`, and `.api-almanac/` are generated. Never create or edit them.

### Sidebar display order

Folders and request files are displayed in the order determined by a numeric prefix on the file or directory name. The prefix format differs between folders and files:

- **Folder prefixes** — unpadded integer followed by a dash: `1-auth/`, `2-users/`
- **Request file prefixes** — four-digit zero-padded integer, then a UID, then a slug: `0001-A1B2C3D4-login.yaml`, `0002-B2C3D4E5-register.yaml`

Examples:

- `0001-A1B2C3D4-login.yaml` sorts before `0002-B2C3D4E5-register.yaml` sorts before `0010-C3D4E5F6-verify.yaml`
- `requests/1-auth/0001-A1B2C3D4-login.yaml` — folder `auth` is first; `login` is first within it
- Files and folders without a numeric prefix are sorted after all prefixed siblings

**When generating a new project from scratch, omit all numeric prefixes.** The app assigns them automatically: when a project is first opened all request files are renamed to the canonical `{0001}-{uid}-{slug}.yaml` form, and new or duplicated requests are placed at the end of their folder with the next available index. Plain names like `requests/auth/login.yaml` are valid and will appear in alphabetical order.

---

## File formats

### `almanac.yaml`

Project root marker and identity. Required; must be at the project root.

```yaml
id: my-api          # required — slug identifier, kebab-case
name: My API        # required — human-readable label
description: |      # optional — free-form description
  A sample API project.
```

| Field         | Type   | Required | Notes                                |
|---------------|--------|----------|--------------------------------------|
| `id`          | string | yes      | Slug; used as a prefix in CLI output |
| `name`        | string | yes      | Displayed in the UI sidebar          |
| `description` | string | no       | Free text; omit if not useful        |

---

### `environments/{id}.yaml`

Defines a named variable set. The file's `id` field **must match** its filename stem (e.g. `local.yaml` → `id: local`).

```yaml
id: local                               # required — matches filename stem
name: Local                             # required — human-readable label
vars:                                   # optional — key/value pairs
  base_url: http://localhost:8000
  auth.token: "{{secret.LOCAL_API_TOKEN}}"
  timeout_ms: "5000"
```

| Field  | Type              | Required | Notes                                           |
|--------|-------------------|----------|-------------------------------------------------|
| `id`   | string            | yes      | Must equal the filename stem                    |
| `name` | string            | yes      | Shown in the environment selector               |
| `vars` | map[string]string | no       | All values are strings; supports `{{secret.X}}` |

**Secret references:** A variable value of the form `{{secret.VAR_NAME}}` is resolved at runtime by reading the OS environment variable `VAR_NAME`. Secrets are never stored in the YAML files. Always use this pattern for tokens, passwords, and API keys.

Typical environments to create: `local`, `staging`, `production`.

---

### `requests/**/*.yaml`

Defines a single HTTP request. Store at `requests/{group}/{name}.yaml`. The file can be nested at any depth.

```yaml
uid: A1B2C3D4        # optional — 8-char [A-Z0-9] stable identity; auto-generated on first open
id: users.create     # required — dot-notation, globally unique
name: Create user    # required — human-readable label
tags:                # optional — for grouping/filtering
  - users
  - write

method: POST              # required — HTTP verb, uppercase
url: "{{base_url}}/users" # required — supports {{var}} templates

headers:                  # optional
  Authorization: "Bearer {{auth.token}}"
  Content-Type: application/json

query:                    # optional — URL query parameters
  notify: "true"

body:                     # optional
  kind: json              # json | text | form
  value:                  # arbitrary YAML; becomes the request body
    name: "{{user.name}}"
    email: "{{user.email}}"
    role: viewer

cases:                    # optional — named variable overrides
  admin:
    user.name: Grace Hopper
    user.email: grace@example.com
    user.role: admin

expect:                   # optional — assertions checked after the run
  status: 201
  time_ms: "< 750"
  headers:
    Content-Type: "contains application/json"
  json:
    id: exists
    name: "equals {{user.name}}"

capture:                  # optional — save response values into session vars
  created_user.id: json.id
  created_user.token: json.access_token

redact:                   # optional — scrub before saving response to disk
  - headers.Authorization
  - json.access_token

notes: |                  # optional — free-form notes shown in the UI
  Run auth.login first to populate auth.token.
```

#### Field reference

| Field     | Type              | Required | Notes                                                 |
|-----------|-------------------|----------|-------------------------------------------------------|
| `uid`     | string            | no       | 8-char `[A-Z0-9]`; omit when generating — app adds it on first open |
| `id`      | string            | yes      | Dot-notation (`group.action`); must be unique         |
| `name`    | string            | yes      | Shown in sidebar and Markdown export                  |
| `tags`    | list[string]      | no       | Arbitrary tags                                        |
| `method`  | string            | yes      | `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, etc. |
| `url`     | string            | yes      | Full URL; use `{{base_url}}` from environment         |
| `headers` | map[string]string | no       | HTTP headers; values support `{{vars}}`               |
| `query`   | map[string]string | no       | Query params; values support `{{vars}}`               |
| `body`    | object            | no       | See body schema below                                 |
| `cases`   | map[string]map    | no       | See cases below                                       |
| `expect`  | object            | no       | See expect schema below                               |
| `capture` | map[string]string | no       | See capture below                                     |
| `redact`  | list[string]      | no       | Dot-notation paths to scrub in saved responses        |
| `notes`   | string            | no       | Markdown-friendly free text                           |

---

#### Body schema

```yaml
body:
  kind: json    # json | text | form
  value: ...    # any YAML structure (for json) or a plain string (for text) or a flat map (for form)
```

- `json` — `value` is any YAML structure; serialized to JSON and sent with `Content-Type: application/json`
- `text` — `value` is a plain string sent as-is
- `form` — `value` is a flat string→string map; serialized as `application/x-www-form-urlencoded`

---

#### Cases

Each case is a named map of variable overrides. When a case is selected at run time its variables are merged on top of the environment variables.

```yaml
cases:
  happy-path:
    user.role: viewer
  edge-admin:
    user.role: admin
    user.name: "Root User"
```

Case names are arbitrary strings. Use kebab-case. Cases layer on top of env vars; they do not replace them — only the keys listed in the case are overridden.

---

#### Expect schema

`expect` is a flat object directly under the request key — there is no `default:` nesting.

```yaml
expect:
  status: 200                            # integer HTTP status code (exact match)
  time_ms: "< 500"                       # rule string — see time_ms rules below
  headers:
    Content-Type: "contains application/json"
    X-Request-Id: exists
  json:
    id: exists
    email: "equals user@example.com"
    role: "contains admin"
    "items[0].name": exists
```

**`time_ms` rule strings** (applied to response duration in milliseconds):

| Rule    | Meaning                       |
|---------|-------------------------------|
| `< N`   | duration is less than N       |
| `<= N`  | duration is less than or equal to N |
| `> N`   | duration is greater than N    |
| `>= N`  | duration is greater than or equal to N |
| `N`     | duration equals N exactly     |

**`headers` and `json` rule strings** (applied to a string value):

| Rule             | Meaning                                        |
|------------------|------------------------------------------------|
| `exists`         | field is present in the response               |
| `equals VALUE`   | field value equals VALUE exactly               |
| `contains VALUE` | field value contains VALUE as a substring      |
| *(bare string)*  | treated as `equals <string>` — shorthand form  |

**JSON path syntax** — keys in `expect.json` use dot-notation into the parsed JSON body:

```
id              → top-level field "id"
user.email      → response.user.email
items[0].name   → response.items[0].name
roles[2]        → response.roles[2]
```

Array indexing uses `key[N]` within a segment. Dots separate path components.

---

#### Capture

Saves a value from the response into a session variable so subsequent requests can reference it via `{{variable_name}}`.

```yaml
capture:
  auth.token: json.access_token        # json.<dot.path> — reads from JSON body
  current.user_id: json.user.id        # nested JSON path
  session.id: headers.X-Session-Id     # headers.<Header-Name> — reads response header
```

Supported path prefixes:

| Prefix        | Source                              |
|---------------|-------------------------------------|
| `json.<path>` | JSON body at dot-notation path      |
| `headers.<name>` | Response header (case-insensitive) |
| `header.<name>`  | Alias for `headers.<name>`         |

The captured key (left-hand side) becomes a session variable name. Use any dot-notation naming you like (e.g. `auth.token`, `created_user.id`). These are available as `{{auth.token}}` in any request run in the same session.

---

#### Redact

List of dot-notation paths to scrub before the response is saved to `.api-almanac/`. The fields are replaced with `[REDACTED]` in the stored file. This does not affect the live response display.

```yaml
redact:
  - headers.Authorization     # scrub a request header echo
  - json.access_token         # scrub a token from the saved body
  - json.password             # scrub a password echo
```

---

## Template variable syntax

Any string field in a request YAML may embed `{{variable_name}}` placeholders.

| Syntax                | Resolved from                                    |
|-----------------------|--------------------------------------------------|
| `{{base_url}}`        | Active environment's `vars` map                  |
| `{{auth.token}}`      | Environment vars, or a session-captured value    |
| `{{secret.VAR_NAME}}` | OS environment variable `VAR_NAME` at run time   |
| `{{case_var}}`        | Active case overrides (merge over env vars)      |

Resolution order (highest priority first): case vars → environment vars → OS env (for `{{secret.*}}`). Unresolved placeholders are left as-is in the final request.

Variable names may contain dots (e.g. `user.email`, `created_user.id`). Dots are part of the name, not path separators in the vars map.

---

## Naming conventions

| Thing              | Convention                                         | Example                        |
|--------------------|----------------------------------------------------|--------------------------------|
| Project `id`       | kebab-case slug                                    | `stripe-api`                   |
| Environment `id`   | lowercase word or kebab-case; equals filename stem | `local`, `staging`             |
| Request `id`       | dot-notation: `group.action`                       | `users.create`, `auth.login`   |
| Request filename   | Plain kebab-case when hand-authored; app normalizes to `{0001}-{uid}-{slug}.yaml` on first open | `requests/users/create.yaml` → `requests/users/0001-A1B2C3D4-create.yaml` |
| Folder grouping    | Group by API resource or feature area              | `auth/`, `users/`, `payments/` |
| Variable names     | dot-notation; be consistent across requests        | `user.id`, `auth.token`        |
| Secret env vars    | UPPER_SNAKE_CASE OS env var name                   | `{{secret.STRIPE_API_KEY}}`    |
| Case names         | kebab-case describing the scenario                 | `happy-path`, `invalid-email`  |

---

## Worked example

**API spec excerpt (hypothetical):**

> `POST /auth/token`
> Request body (JSON): `{ "email": string, "password": string }`
> Success response (200): `{ "access_token": string, "user_id": string }`
> Authorization: none required

**Generated files:**

`almanac.yaml`
```yaml
id: my-api
name: My API
```

`environments/local.yaml`
```yaml
id: local
name: Local
vars:
  base_url: http://localhost:8000
  login.email: dev@example.com
  login.password: "{{secret.LOCAL_PASSWORD}}"
```

`requests/auth/login.yaml`
```yaml
id: auth.login
name: Login
method: POST
url: "{{base_url}}/auth/token"
headers:
  Content-Type: application/json
body:
  kind: json
  value:
    email: "{{login.email}}"
    password: "{{login.password}}"
expect:
  status: 200
  json:
    access_token: exists
    user_id: exists
capture:
  auth.token: json.access_token
  current.user_id: json.user_id
redact:
  - json.access_token
notes: Sets auth.token for use in authenticated requests.
```

---

## LLM generation checklist

Follow these steps when converting API documentation into an API Almanac project:

1. **Create `almanac.yaml`** — choose a slug `id` from the API name and set a `name`.

2. **Create environment files** — create at least a `local.yaml`. Add `base_url` as a var pointing to the local server. For staging/production, add those too if known. Never put real secrets in `vars`; use `{{secret.VAR_NAME}}` and note which OS env vars the user must set.

3. **Group requests by resource** — create one subdirectory per API resource or feature area under `requests/`. Use the resource name as the folder (e.g. `users/`, `payments/`, `webhooks/`). Do not add numeric prefixes — the user can reorder in the GUI.

4. **Create one YAML per endpoint** — for each endpoint:
   - Set `id` as `resource.action` (e.g. `users.create`)
   - Set `method` and `url` with `{{base_url}}` prefix
   - Add all documented headers (use `{{auth.token}}` for bearer auth)
   - Add `query` params if the endpoint accepts them
   - Add `body` with the appropriate `kind` and the documented fields; use `{{vars}}` for any values that vary by environment or case
   - Add `expect.status` matching the documented success code
   - Add `expect.json` checks for key documented response fields (`exists` is sufficient for most)
   - Add `capture` for any IDs or tokens the response returns that other requests will need
   - Add `redact` for tokens and passwords
   - Add `notes` explaining prerequisites (e.g. "run auth.login first")
   - Omit the `uid` field — the app generates it on first open

5. **Wire up auth** — identify the login/token endpoint. Add `capture` to save the token. Reference `{{auth.token}}` in the `Authorization` header of every protected request.

6. **Add cases for variable inputs** — if an endpoint behaves differently based on role, plan, or status, add a `cases` block with named scenarios. Each case only needs to list the variables that differ from the defaults.

7. **Use `{{secret.*}}` for all credentials** — passwords, API keys, and tokens must never appear as plain-text values in any YAML file.

8. **Verify ids are unique** — every request YAML in the project must have a unique `id`. Check before finishing.

9. **Verify `expect` is flat** — the `expect` field is a direct object with `status`, `time_ms`, `headers`, and `json` keys. Do not nest it under `default:` or any other key.
