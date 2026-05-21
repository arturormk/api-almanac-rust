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

| Field         | Type   | Required | Notes                                  |
|---------------|--------|----------|----------------------------------------|
| `id`          | string | yes      | Slug; used as a prefix in CLI output   |
| `name`        | string | yes      | Displayed in the UI sidebar            |
| `description` | string | no       | Free text; omit if not useful          |

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

| Field  | Type              | Required | Notes                                             |
|--------|-------------------|----------|---------------------------------------------------|
| `id`   | string            | yes      | Must equal the filename stem                      |
| `name` | string            | yes      | Shown in the environment selector                 |
| `vars` | map[string]string | no       | All values are strings; supports `{{secret.X}}`   |

**Secret references:** A variable value of the form `{{secret.VAR_NAME}}` is resolved at runtime by reading the OS environment variable `VAR_NAME`. Secrets are never stored in the YAML files. Always use this pattern for tokens, passwords, and API keys.

Typical environments to create: `local`, `staging`, `production`.

---

### `requests/**/*.yaml`

Defines a single HTTP request. Store at `requests/{group}/{name}.yaml`. The file can be nested at any depth.

```yaml
id: users.create          # required — dot-notation, globally unique
name: Create user         # required — human-readable label
tags:                     # optional — for grouping/filtering
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

#### Body schema

```yaml
body:
  kind: json    # json | text | form
  value: ...    # any YAML structure (for json/form) or a plain string (for text)
```

- `json` — value is serialized to JSON and sent with `Content-Type: application/json`
- `text` — value is a plain string sent as-is
- `form` — value is a flat map serialized as `application/x-www-form-urlencoded`

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

#### Expect schema

```yaml
expect:
  status: 200                            # integer HTTP status code
  time_ms: "< 500"                       # rule string (see below)
  headers:
    Content-Type: "contains application/json"
  json:
    id: exists
    email: "equals user@example.com"
    role: "contains admin"
```

**Rule strings:**

| Rule              | Meaning                              |
|-------------------|--------------------------------------|
| `exists`          | Field is present in the response     |
| `equals VALUE`    | Field value equals VALUE exactly     |
| `contains VALUE`  | Field value contains VALUE substring |
| `< N`             | Numeric less-than (for `time_ms`)    |
| `<= N`            | Numeric less-than-or-equal           |

`headers` and `json` keys use dot-notation paths into the response. `json.id` means the `id` key at the top level of the JSON body; `json.user.email` means `response.user.email`.

#### Capture

Saves a value from the response into a session variable so subsequent requests can reference it.

```yaml
capture:
  auth.token: json.access_token     # json.<dot.path> → reads JSON body
  session.id: headers.X-Session-Id  # headers.<Header-Name> → reads header
```

The left-hand key becomes a session variable name usable as `{{auth.token}}` in any subsequent request run in the same session.

---

## Template variable syntax

Any string field in a request YAML may embed `{{variable_name}}` placeholders.

| Syntax                | Resolved from                                          |
|-----------------------|--------------------------------------------------------|
| `{{base_url}}`        | Active environment's `vars` map                        |
| `{{auth.token}}`      | Environment vars, or a session-captured value          |
| `{{secret.VAR_NAME}}` | OS environment variable `VAR_NAME` at run time         |
| `{{case_var}}`        | Active case overrides (override env vars)              |

Resolution order: case vars → environment vars → OS env (for `{{secret.*}}`). Unresolved placeholders are left as-is.

Variable names may contain dots (e.g. `user.email`, `created_user.id`). Dots are part of the name, not path separators in the vars map.

---

## Naming conventions

| Thing              | Convention                                          | Example                        |
|--------------------|-----------------------------------------------------|--------------------------------|
| Project `id`       | kebab-case slug                                     | `stripe-api`                   |
| Environment `id`   | lowercase word or kebab-case; equals filename stem  | `local`, `staging`             |
| Request `id`       | dot-notation: `group.action`                        | `users.create`, `auth.login`   |
| Request filename   | kebab-case, matches the action part of the id       | `requests/users/create.yaml`   |
| Folder grouping    | Group by API resource or feature area               | `auth/`, `users/`, `payments/` |
| Variable names     | dot-notation; be consistent across requests         | `user.id`, `auth.token`        |
| Secret env vars    | UPPER_SNAKE_CASE OS env var name                    | `{{secret.STRIPE_API_KEY}}`    |

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

2. **Create environment files** — create at least a `local.yaml`. Add `base_url` as a var. For staging/production, add those too. Never put real secrets in `vars`; use `{{secret.VAR_NAME}}` and note which OS env vars the user must set.

3. **Group requests by resource** — create one subdirectory per API resource or feature area under `requests/`. Use the resource name as the folder (e.g. `users/`, `payments/`, `webhooks/`).

4. **Create one YAML per endpoint** — for each endpoint:
   - Set `id` as `resource.action` (e.g. `users.create`)
   - Set `method` and `url` with `{{base_url}}` prefix
   - Add all documented headers (use `{{auth.token}}` for bearer auth)
   - Add `query` params if the endpoint accepts them
   - Add `body` with `kind: json` and the documented fields; use `{{vars}}` for any values that vary
   - Add `expect.status` matching the documented success code
   - Add `expect.json` checks for documented response fields
   - Add `capture` for any IDs or tokens the response returns that other requests will need
   - Add `redact` for tokens and passwords
   - Add `notes` explaining prerequisites (e.g. "run auth.login first")

5. **Wire up auth** — identify the login/token endpoint. Add `capture` to save the token. Reference `{{auth.token}}` in the `Authorization` header of every protected request.

6. **Add cases for variable inputs** — if an endpoint behaves differently based on role, plan, or status, add a `cases` block with named scenarios.

7. **Use `{{secret.*}}` for all credentials** — passwords, API keys, and tokens must never appear as plain-text values in any YAML file.

8. **Verify ids are unique** — every request YAML in the project must have a unique `id`. Check before finishing.
