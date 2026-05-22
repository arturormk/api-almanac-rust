# Prompt: Generate the Open-Meteo demo project

You are working inside the `api-almanac-rust` repository. Your task is to generate a complete API Almanac project tree rooted at `demos/open-meteo/` that demonstrates real API calls against the Open-Meteo weather API.

## Step 1 — fetch the API documentation

Fetch the Open-Meteo API reference:

```
https://open-meteo.com/en/docs
```

Read it carefully. Note the base URL, query parameters, response shape, and any sub-APIs (e.g. historical, marine, air-quality). Open-Meteo is a free, no-auth API — do not invent any authentication scheme.

## Step 2 — read the project format

Read `CLAUDE.md` in the repo root (it is always present). It documents the full YAML schema for every file type. The authoritative struct definitions are in:

- `crates/api-almanac-model/src/project.rs` — `AlmanacProject` (almanac.yaml)
- `crates/api-almanac-model/src/environment.rs` — `Environment` (environments/*.yaml)
- `crates/api-almanac-model/src/request.rs` — `RequestDef`, `Expect` (requests/**/*.yaml)
- `crates/api-almanac-model/src/body.rs` — `RequestBody`, `BodyKind`

Key rules:
- `almanac.yaml` fields: `id`, `name`, `description`
- `environments/*.yaml` fields: `id`, `name`, `parent` (optional), `vars`
- `requests/**/*.yaml` fields: `uid` (8-char [A-Z0-9], skip if you can't generate stable ones), `id`, `name`, `tags`, `method`, `url`, `headers`, `query`, `body` (`kind`: json|text|form, `value`), `cases`, `expect` (`status`, `time_ms`, `headers`, `json`), `capture`, `redact`, `notes`
- Template syntax: `{{var_name}}`, `{{secret.ENV_VAR_NAME}}`
- Open-Meteo needs no auth, so no `Authorization` header or `{{secret.*}}` references are needed
- Request `id` follows dot-notation by folder path, e.g. `forecast.current` for `requests/forecast/current.yaml`

## Step 3 — generate the project tree

Create the following files under `demos/open-meteo/`:

```
almanac.yaml
environments/
  default.yaml        # base_url only; no secrets needed
requests/
  forecast/
    current-weather.yaml     # current conditions (temperature, wind, weather code)
    hourly-forecast.yaml     # next 48h: temperature_2m + precipitation
    daily-forecast.yaml      # 7-day: sunrise, sunset, precipitation_sum, temp max/min
  historical/
    past-week.yaml           # start_date / end_date set to realistic fixed dates
  air-quality/
    european-aqi.yaml        # european_aqi + pm10 + pm2_5
```

Guidelines for the request files:

- Use `latitude` and `longitude` query params with realistic defaults (e.g. Madrid: 40.4168, -3.7038) — put them in `cases` or as literal values.
- Add a `default` case in requests that vary by location so the user can switch cities.
- Add `expect: status: 200` on every request.
- Add `expect.time_ms: "< 3000"` as a reasonable SLA guard.
- Add `expect.json` checks where the response shape is predictable (e.g. `"hourly.time": "exists"`, `"current.temperature_2m": "exists"`).
- Add meaningful `notes` (one or two sentences) explaining what the request is for and which parameters to tweak.
- Keep `tags` concise: `[forecast]`, `[historical]`, `[air-quality]`.
- Do not invent fields that are not in the structs — if unsure, omit.

## Step 4 — verify

After writing all files, run:

```bash
cargo run -p api-almanac-cli -- list
```

from inside `demos/open-meteo/` (or pass `--project demos/open-meteo`) if a `--project` flag exists. Fix any YAML parse errors the CLI reports.

## Definition of done

- `demos/open-meteo/almanac.yaml` exists and parses cleanly
- At least one environment file under `environments/`
- At least five request files covering at least two endpoint families
- Every request has `expect.status: 200`
- No placeholder text like `TODO` or `FIXME` remains in the generated files
