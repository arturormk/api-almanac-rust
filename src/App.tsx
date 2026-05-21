import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

// ── Types ──────────────────────────────────────────────────────────────────

type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
type BodyKind = "none" | "json" | "text" | "form";
type RequestTab = "params" | "headers" | "body" | "notes";
type ResponseTab = "body" | "headers" | "sketch" | "tools";

interface KvRow {
  id: number;
  key: string;
  value: string;
  enabled: boolean;
}

interface HttpResponse {
  status: number;
  status_text: string;
  headers: Record<string, string>;
  body: string;
  duration_ms: number;
  url: string;
}

interface StoredResponse extends HttpResponse {
  ran_at: string;
  environment: string | null;
  case: string | null;
}

interface CheckItem {
  name: string;
  passed: boolean;
  expected: string;
  actual: string | null;
}

interface RunResult {
  response: HttpResponse;
  checks: CheckItem[];
  captured: Record<string, string>;
}

interface RequestSummary {
  id: string;
  name: string;
  method: string;
  folder: string;
  file_path: string;
}

interface EnvSummary {
  id: string;
  name: string;
}

interface ProjectData {
  name: string;
  id: string;
  description?: string;
  requests: RequestSummary[];
  environments: EnvSummary[];
}

interface RequestData {
  id: string;
  name: string;
  method: string;
  url: string;
  headers: Record<string, string>;
  query: Record<string, string>;
  body_content?: string;
  body_kind?: string;
  notes?: string;
  tags: string[];
  case_names: string[];
}

// ── Spot-check types ───────────────────────────────────────────────────────

interface SpotCheckResult {
  request_id: string;
  request_name: string;
  folder: string;
  status: number | null;
  duration_ms: number | null;
  checks: CheckItem[];
  captured: Record<string, string>;
  error: string | null;
}

interface SpotCheckReport {
  ran_at: string;
  environment: string | null;
  total: number;
  passed: number;
  failed: number;
  errored: number;
  duration_ms: number;
  results: SpotCheckResult[];
}

interface EnvironmentData {
  id: string;
  name: string;
  vars: Record<string, string>;
}

interface PluginManifest {
  id: string;
  name: string;
  description?: string;
  command: { executable: string; args: string[] };
}

interface Artifact {
  kind: "html" | "markdown" | "yaml" | "json" | "text";
  title: string;
  content: string;
}

interface PluginOutput {
  title?: string;
  artifacts: Artifact[];
  diagnostics: string[];
  error?: { message: string };
}

let nextId = 1;
const mkRow = (key = "", value = ""): KvRow => ({
  id: nextId++,
  key,
  value,
  enabled: true,
});

function mapToRows(m: Record<string, string>): KvRow[] {
  const rows = Object.entries(m).map(([k, v]) => mkRow(k, v));
  return rows.length > 0 ? rows : [mkRow()];
}

function slugify(s: string): string {
  return s
    .toLowerCase()
    .trim()
    .replace(/\s+/g, "-")
    .replace(/[^a-z0-9-]/g, "")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

// ── KV editor ──────────────────────────────────────────────────────────────

function KvEditor({
  rows,
  onChange,
  keyPlaceholder = "Key",
  valuePlaceholder = "Value",
}: {
  rows: KvRow[];
  onChange: (rows: KvRow[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
}) {
  const update = (id: number, field: keyof KvRow, val: string | boolean) =>
    onChange(rows.map((r) => (r.id === id ? { ...r, [field]: val } : r)));
  const remove = (id: number) => onChange(rows.filter((r) => r.id !== id));
  return (
    <div className="kv-editor">
      {rows.map((row) => (
        <div key={row.id} className="kv-row">
          <input
            type="checkbox"
            checked={row.enabled}
            onChange={(e) => update(row.id, "enabled", e.target.checked)}
            title="Enable"
          />
          <input
            className="kv-input"
            placeholder={keyPlaceholder}
            value={row.key}
            onChange={(e) => update(row.id, "key", e.target.value)}
          />
          <input
            className="kv-input"
            placeholder={valuePlaceholder}
            value={row.value}
            onChange={(e) => update(row.id, "value", e.target.value)}
          />
          <button className="kv-remove" onClick={() => remove(row.id)} title="Remove">×</button>
        </div>
      ))}
      <button className="kv-add" onClick={() => onChange([...rows, mkRow()])}>+ Add</button>
    </div>
  );
}

// ── Status badge ───────────────────────────────────────────────────────────

function StatusBadge({ status }: { status: number }) {
  const cls =
    status >= 200 && status < 300 ? "badge-ok"
      : status >= 300 && status < 400 ? "badge-redirect"
        : status >= 400 && status < 500 ? "badge-client-err"
          : "badge-server-err";
  return <span className={`status-badge ${cls}`}>{status}</span>;
}

// ── Pretty body ────────────────────────────────────────────────────────────

function PrettyBody({ body, contentType }: { body: string; contentType?: string }) {
  const isJson =
    contentType?.includes("json") ||
    body.trimStart().startsWith("{") ||
    body.trimStart().startsWith("[");
  if (isJson) {
    try {
      return <pre className="response-body">{JSON.stringify(JSON.parse(body), null, 2)}</pre>;
    } catch { /* fall through */ }
  }
  return <pre className="response-body">{body}</pre>;
}

// ── Method badge ───────────────────────────────────────────────────────────

function MethodBadge({ method }: { method: string }) {
  return <span className={`method-badge method-${method.toLowerCase()}`}>{method}</span>;
}

// ── Check results panel ────────────────────────────────────────────────────

function ChecksPanel({
  checks,
  captured,
}: {
  checks: CheckItem[];
  captured: Record<string, string>;
}) {
  const [expanded, setExpanded] = useState(false);
  if (checks.length === 0 && Object.keys(captured).length === 0) return null;

  const passed = checks.filter((c) => c.passed).length;
  const total = checks.length;
  const allPassed = passed === total;

  return (
    <div className="checks-panel">
      {checks.length > 0 && (
        <>
          <div
            className={`checks-summary ${allPassed ? "checks-ok" : "checks-fail"}`}
            onClick={() => setExpanded((e) => !e)}
            role="button"
            title={expanded ? "Collapse" : "Expand checks"}
          >
            <span className="checks-icon">{allPassed ? "✓" : "✗"}</span>
            <span>
              {allPassed
                ? `All ${total} check${total !== 1 ? "s" : ""} passed`
                : `${total - passed} check${total - passed !== 1 ? "s" : ""} failed (${passed}/${total} passed)`}
            </span>
            <span className="checks-toggle">{expanded ? "▲" : "▼"}</span>
          </div>
          {expanded && (
            <table className="checks-table">
              <tbody>
                {checks.map((c, i) => (
                  <tr key={i} className={c.passed ? "check-pass" : "check-fail"}>
                    <td className="check-icon">{c.passed ? "✓" : "✗"}</td>
                    <td className="check-name">{c.name}</td>
                    <td className="check-expected">{c.expected}</td>
                    <td className="check-actual">{c.actual ?? "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}
      {Object.keys(captured).length > 0 && (
        <div className="captured-row">
          <span className="captured-label">Captured:</span>
          {Object.entries(captured).map(([k, v]) => (
            <span key={k} className="captured-item" title={`${k} = ${v}`}>
              <span className="captured-key">{k}</span>
              <span className="captured-val">{v.length > 30 ? v.slice(0, 30) + "…" : v}</span>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Spot-check panel ───────────────────────────────────────────────────────

function SpotCheckPanel({
  project,
  selectedEnvId,
  onClose,
}: {
  project: ProjectData;
  selectedEnvId: string | null;
  onClose: () => void;
}) {
  const [running, setRunning] = useState(false);
  const [current, setCurrent] = useState(0);
  const [results, setResults] = useState<SpotCheckResult[]>([]);
  const [ranAt, setRanAt] = useState<string | null>(null);
  const [totalMs, setTotalMs] = useState(0);
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [exportErr, setExportErr] = useState<string | null>(null);

  const envName = project.environments.find((e) => e.id === selectedEnvId)?.name ?? null;
  const total = project.requests.length;
  const done = !running && ranAt !== null;

  const passed = done
    ? results.filter((r) => !r.error && (r.checks.length === 0 || r.checks.every((c) => c.passed))).length
    : 0;
  const failed = done
    ? results.filter((r) => !r.error && r.checks.some((c) => !c.passed)).length
    : 0;
  const errored = done ? results.filter((r) => !!r.error).length : 0;

  async function runAll() {
    setRunning(true);
    setCurrent(0);
    setResults([]);
    setRanAt(null);
    setExportPath(null);
    setExportErr(null);

    await invoke("clear_session_vars");

    const startedAt = new Date().toISOString();
    setRanAt(startedAt);
    const wallStart = Date.now();
    const acc: SpotCheckResult[] = [];

    for (let i = 0; i < project.requests.length; i++) {
      setCurrent(i + 1);
      const req = project.requests[i];
      try {
        const result = await invoke<RunResult>("run_project_request", {
          filePath: req.file_path,
          envId: selectedEnvId,
          caseName: null,
        });
        acc.push({
          request_id: req.id,
          request_name: req.name,
          folder: req.folder,
          status: result.response.status,
          duration_ms: result.response.duration_ms,
          checks: result.checks,
          captured: result.captured,
          error: null,
        });
      } catch (e) {
        acc.push({
          request_id: req.id,
          request_name: req.name,
          folder: req.folder,
          status: null,
          duration_ms: null,
          checks: [],
          captured: {},
          error: String(e),
        });
      }
      setResults([...acc]);
    }

    setTotalMs(Date.now() - wallStart);
    setRunning(false);
  }

  async function exportReport() {
    if (!ranAt) return;
    const report: SpotCheckReport = {
      ran_at: ranAt,
      environment: envName,
      total,
      passed,
      failed,
      errored,
      duration_ms: totalMs,
      results,
    };
    try {
      const path = await invoke<string>("export_spot_check_report", { report });
      setExportPath(path);
    } catch (e) {
      setExportErr(String(e));
    }
  }

  return (
    <div className="spot-check-panel">
      <div className="spot-check-header">
        <span className="spot-check-title">Spot Check</span>
        {envName && <span className="spot-check-env">{envName}</span>}
        <button className="spot-check-close" onClick={onClose} title="Close">×</button>
      </div>

      <div className="spot-check-body">
        {!running && !done && (
          <div className="spot-check-start">
            <p className="spot-check-desc">
              Run all {total} request{total !== 1 ? "s" : ""} in sequence
              {envName ? ` with environment "${envName}"` : " (no environment)"}.
              Captures are passed forward to subsequent requests.
            </p>
            <button
              className="spot-check-run-btn"
              onClick={runAll}
              disabled={total === 0}
            >
              ▶ Run {total} Request{total !== 1 ? "s" : ""}
            </button>
          </div>
        )}

        {running && (
          <div className="spot-check-progress">
            Running {current} / {total}…
          </div>
        )}

        {done && (
          <div className="spot-check-summary">
            <span className="sc-badge sc-pass">{passed} passed</span>
            {failed > 0 && <span className="sc-badge sc-fail">{failed} failed</span>}
            {errored > 0 && <span className="sc-badge sc-error">{errored} error{errored !== 1 ? "s" : ""}</span>}
            <span className="sc-badge sc-time">{totalMs} ms</span>
            <button className="sc-export-btn" onClick={exportReport}>Export MD</button>
          </div>
        )}

        {exportPath && (
          <div className="export-flash">Saved → <code>{exportPath}</code></div>
        )}
        {exportErr && (
          <div className="response-error">{exportErr}</div>
        )}

        {results.length > 0 && (
          <table className="sc-table">
            <thead>
              <tr>
                <th>#</th>
                <th>Request</th>
                <th>Folder</th>
                <th>Status</th>
                <th>Duration</th>
                <th>Checks</th>
              </tr>
            </thead>
            <tbody>
              {results.map((r, i) => {
                const cp = r.checks.filter((c) => c.passed).length;
                const ct = r.checks.length;
                const ok = !r.error && (ct === 0 || cp === ct);
                return (
                  <tr
                    key={r.request_id}
                    className={r.error ? "sc-row-error" : ok ? "sc-row-pass" : "sc-row-fail"}
                  >
                    <td className="sc-num">{i + 1}</td>
                    <td className="sc-name">{r.request_name}</td>
                    <td className="sc-folder">{r.folder || "—"}</td>
                    <td className="sc-status">{r.status ?? "—"}</td>
                    <td className="sc-dur">{r.duration_ms !== null ? `${r.duration_ms} ms` : "—"}</td>
                    <td className="sc-checks">
                      {r.error ? (
                        <span className="sc-err-label" title={r.error}>error</span>
                      ) : ct === 0 ? (
                        <span className="sc-no-checks">—</span>
                      ) : (
                        <span className={cp === ct ? "sc-checks-pass" : "sc-checks-fail"}>
                          {cp}/{ct} {cp === ct ? "✓" : "✗"}
                        </span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

// ── Environment panel ─────────────────────────────────────────────────────

function EnvironmentPanel({
  selectedEnvId,
  onClose,
  onProjectChange,
}: {
  selectedEnvId: string | null;
  onClose: () => void;
  onProjectChange: (project: ProjectData, newEnvId: string | null) => void;
}) {
  const [envs, setEnvs] = useState<EnvironmentData[]>([]);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [editVars, setEditVars] = useState<KvRow[]>([mkRow()]);
  const [isDirty, setIsDirty] = useState(false);
  const [addingNew, setAddingNew] = useState(false);
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // Load all environments on mount
  useEffect(() => {
    invoke<EnvironmentData[]>("list_environments")
      .then((data) => {
        setEnvs(data);
        if (data.length > 0) {
          const first = data[0];
          setEditingId(first.id);
          setEditName(first.name);
          setEditVars(Object.keys(first.vars).length > 0 ? mapToRows(first.vars) : [mkRow()]);
        }
      })
      .catch((e) => setError(String(e)));
  }, []);

  function selectEnv(env: EnvironmentData, list = envs) {
    if (isDirty) {
      if (!window.confirm("You have unsaved changes. Discard them?")) return;
    }
    const fresh = list.find((e) => e.id === env.id) ?? env;
    setEditingId(fresh.id);
    setEditName(fresh.name);
    setEditVars(Object.keys(fresh.vars).length > 0 ? mapToRows(fresh.vars) : [mkRow()]);
    setIsDirty(false);
    setError(null);
  }

  async function handleSave() {
    if (!editingId) return;
    setSaving(true);
    try {
      const data: EnvironmentData = {
        id: editingId,
        name: editName.trim() || editingId,
        vars: Object.fromEntries(
          editVars.filter((r) => r.enabled && r.key.trim()).map((r) => [r.key, r.value])
        ),
      };
      const newProject = await invoke<ProjectData>("save_environment", { data });
      const updated = envs.map((e) => (e.id === data.id ? data : e));
      setEnvs(updated);
      setIsDirty(false);
      onProjectChange(newProject, editingId);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function handleCreate() {
    const name = newName.trim();
    if (!name) return;
    try {
      const newProject = await invoke<ProjectData>("create_environment", { name });
      const allEnvs = await invoke<EnvironmentData[]>("list_environments");
      setEnvs(allEnvs);
      setAddingNew(false);
      setNewName("");
      // Select the newly created env
      const created = allEnvs.find((e) => e.name === name) ?? allEnvs[allEnvs.length - 1];
      if (created) selectEnv(created, allEnvs);
      onProjectChange(newProject, created?.id ?? null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete() {
    if (!editingId) return;
    if (!window.confirm(`Delete environment "${editName}"? This cannot be undone.`)) return;
    try {
      const deletedId = editingId;
      const newProject = await invoke<ProjectData>("delete_environment", { envId: deletedId });
      const remaining = envs.filter((e) => e.id !== deletedId);
      setEnvs(remaining);
      if (remaining.length > 0) {
        selectEnv(remaining[0], remaining);
      } else {
        setEditingId(null);
        setEditName("");
        setEditVars([mkRow()]);
      }
      onProjectChange(newProject, deletedId === selectedEnvId ? null : selectedEnvId);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="env-panel">
      <div className="env-panel-header">
        <span className="env-panel-title">Environments</span>
        <button className="env-panel-close" onClick={onClose} title="Close">×</button>
      </div>

      <div className="env-panel-body">
        {/* Left: env list */}
        <div className="env-list">
          <div className="env-list-items">
            {envs.map((env) => (
              <button
                key={env.id}
                className={`env-list-item${editingId === env.id ? " env-list-item-active" : ""}`}
                onClick={() => selectEnv(env)}
              >
                {env.name}
              </button>
            ))}
            {envs.length === 0 && (
              <p className="env-list-empty">No environments yet.</p>
            )}
          </div>

          <div className="env-list-add">
            {addingNew ? (
              <div className="env-new-row">
                <input
                  className="env-new-input"
                  placeholder="Environment name"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); if (e.key === "Escape") { setAddingNew(false); setNewName(""); } }}
                  autoFocus
                />
                <button className="env-new-confirm" onClick={handleCreate} title="Create">✓</button>
                <button className="env-new-cancel" onClick={() => { setAddingNew(false); setNewName(""); }} title="Cancel">×</button>
              </div>
            ) : (
              <button className="env-add-btn" onClick={() => setAddingNew(true)}>+ New Environment</button>
            )}
          </div>
        </div>

        {/* Right: editor */}
        <div className="env-editor">
          {editingId ? (
            <>
              <div className="env-editor-name-row">
                <label className="env-editor-label">Name</label>
                <input
                  className="env-editor-name"
                  value={editName}
                  onChange={(e) => { setEditName(e.target.value); setIsDirty(true); }}
                  spellCheck={false}
                />
                <span className="env-editor-id" title="Environment ID (filename)">{editingId}</span>
              </div>

              <div className="env-vars-label">Variables</div>
              <div className="env-vars-editor">
                <KvEditor
                  rows={editVars}
                  onChange={(rows) => { setEditVars(rows); setIsDirty(true); }}
                  keyPlaceholder="Variable name"
                  valuePlaceholder="Value or {{secret.ENV_VAR}}"
                />
              </div>

              {error && <div className="response-error">{error}</div>}

              <p className="env-secret-hint">
                Use <code>{"{{secret.VAR_NAME}}"}</code> to read the OS environment variable <code>VAR_NAME</code> at runtime — the value is never stored in YAML.
              </p>

              <div className="env-editor-actions">
                <button
                  className="env-delete-btn"
                  onClick={handleDelete}
                  title="Delete this environment"
                >
                  Delete
                </button>
                <button
                  className="env-save-btn"
                  onClick={handleSave}
                  disabled={saving || !isDirty}
                >
                  {saving ? "Saving…" : isDirty ? "Save" : "Saved"}
                </button>
              </div>
            </>
          ) : (
            <p className="env-editor-empty">
              {envs.length === 0
                ? "Click \"+ New Environment\" to create your first environment."
                : "Select an environment from the list to edit it."}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Sidebar ────────────────────────────────────────────────────────────────

function Sidebar({
  project,
  selectedFilePath,
  selectedEnvId,
  onNewProject,
  onOpenProject,
  onAddRequest,
  onSelectRequest,
  onEnvChange,
  onRunChecks,
  onEditEnvs,
}: {
  project: ProjectData | null;
  selectedFilePath: string | null;
  selectedEnvId: string | null;
  onNewProject: () => void;
  onOpenProject: () => void;
  onAddRequest: () => void;
  onSelectRequest: (filePath: string) => void;
  onEnvChange: (envId: string | null) => void;
  onRunChecks: () => void;
  onEditEnvs: () => void;
}) {
  const folders = new Map<string, RequestSummary[]>();
  if (project) {
    for (const req of project.requests) {
      const folder = req.folder || "";
      if (!folders.has(folder)) folders.set(folder, []);
      folders.get(folder)!.push(req);
    }
  }
  const sortedFolders = Array.from(folders.entries()).sort(([a], [b]) => {
    if (a === "") return -1;
    if (b === "") return 1;
    return a.localeCompare(b);
  });

  return (
    <div className="sidebar">
      <div className="sidebar-header">
        <div className="sidebar-btn-row">
          <button className="sidebar-btn" onClick={onOpenProject} title="Open existing project">Open</button>
          <button className="sidebar-btn" onClick={onNewProject} title="Create new project">New</button>
        </div>
        {project && <div className="sidebar-project-name">{project.name}</div>}
      </div>

      {project && project.environments.length > 0 && (
        <div className="sidebar-env-bar">
          <span className="sidebar-env-label">Env</span>
          <select
            className="sidebar-env-select"
            value={selectedEnvId ?? ""}
            onChange={(e) => onEnvChange(e.target.value || null)}
          >
            <option value="">None</option>
            {project.environments.map((env) => (
              <option key={env.id} value={env.id}>{env.name}</option>
            ))}
          </select>
        </div>
      )}

      <div className="sidebar-tree">
        {project ? (
          sortedFolders.length > 0 ? (
            sortedFolders.map(([folder, reqs]) => (
              <div key={folder || "__root__"} className="sidebar-folder">
                {folder && <div className="sidebar-folder-name">{folder}</div>}
                {reqs.map((req) => (
                  <button
                    key={req.file_path}
                    className={`sidebar-req${selectedFilePath === req.file_path ? " sidebar-req-active" : ""}`}
                    onClick={() => onSelectRequest(req.file_path)}
                  >
                    <MethodBadge method={req.method} />
                    <span className="sidebar-req-name">{req.name}</span>
                  </button>
                ))}
              </div>
            ))
          ) : (
            <p className="sidebar-empty">No requests yet.</p>
          )
        ) : (
          <p className="sidebar-empty">Open or create a project to get started.</p>
        )}
      </div>

      {project && (
        <div className="sidebar-footer">
          <button className="sidebar-add-req-btn" onClick={onAddRequest}>+ Add Request</button>
          <button className="sidebar-checks-btn" onClick={onRunChecks}>Run Checks</button>
          <button className="sidebar-env-edit-btn" onClick={onEditEnvs}>Edit Envs</button>
        </div>
      )}
    </div>
  );
}

// ── Session vars bar ───────────────────────────────────────────────────────

function SessionBar({
  vars,
  onClear,
}: {
  vars: Record<string, string>;
  onClear: () => void;
}) {
  const count = Object.keys(vars).length;
  if (count === 0) return null;
  return (
    <div className="session-bar">
      <span className="session-label">Session ({count})</span>
      <div className="session-vars">
        {Object.entries(vars).map(([k, v]) => (
          <span key={k} className="session-var" title={`${k} = ${v}`}>
            <span className="session-var-key">{k}</span>
          </span>
        ))}
      </div>
      <button className="session-clear" onClick={onClear} title="Clear all captured session variables">
        Clear
      </button>
    </div>
  );
}

// ── App ────────────────────────────────────────────────────────────────────

export default function App() {
  // Project
  const [project, setProject] = useState<ProjectData | null>(null);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [selectedEnvId, setSelectedEnvId] = useState<string | null>(null);
  const [selectedCase, setSelectedCase] = useState<string>("");
  const [caseNames, setCaseNames] = useState<string[]>([]);
  const [isDirty, setIsDirty] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved">("idle");

  // New-request mode
  const [isNewRequest, setIsNewRequest] = useState(false);
  const [newReqDisplayName, setNewReqDisplayName] = useState("");
  const [newReqFolder, setNewReqFolder] = useState("");

  // Stored id/name for save
  const [reqId, setReqId] = useState("");
  const [reqName, setReqName] = useState("");

  // Request edit state
  const [notes, setNotes] = useState("");
  const [method, setMethod] = useState<HttpMethod>("GET");
  const [url, setUrl] = useState("");
  const [params, setParams] = useState<KvRow[]>([mkRow()]);
  const [reqHeaders, setReqHeaders] = useState<KvRow[]>([mkRow()]);
  const [bodyKind, setBodyKind] = useState<BodyKind>("none");
  const [bodyContent, setBodyContent] = useState("");

  // Session & results
  const [sessionVars, setSessionVars] = useState<Record<string, string>>({});
  const [lastChecks, setLastChecks] = useState<CheckItem[]>([]);
  const [lastCaptured, setLastCaptured] = useState<Record<string, string>>({});

  // UI state
  const [reqTab, setReqTab] = useState<RequestTab>("params");
  const [resTab, setResTab] = useState<ResponseTab>("body");
  const [response, setResponse] = useState<HttpResponse | null>(null);
  const [savedMeta, setSavedMeta] = useState<{ ran_at: string; environment: string | null } | null>(null);
  const [sketchYaml, setSketchYaml] = useState<string | null>(null);
  const [reqError, setReqError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [exportedPath, setExportedPath] = useState<string | null>(null);
  const [plugins, setPlugins] = useState<PluginManifest[]>([]);
  const [pluginResults, setPluginResults] = useState<Record<string, PluginOutput>>({});
  const [pluginLoading, setPluginLoading] = useState<Record<string, boolean>>({});
  const [showSpotCheck, setShowSpotCheck] = useState(false);
  const [showEnvEditor, setShowEnvEditor] = useState(false);

  const isProjectMode = selectedFilePath !== null;
  const showSave = isProjectMode || (project !== null && isNewRequest);
  const activeRows = (rows: KvRow[]) => rows.filter((r) => r.enabled && r.key.trim());
  const toMap = (rows: KvRow[]) =>
    Object.fromEntries(activeRows(rows).map((r) => [r.key, r.value]));

  function markDirty() { setIsDirty(true); setSaveStatus("idle"); }

  function loadRequestData(data: RequestData) {
    setReqId(data.id);
    setReqName(data.name);
    setMethod(data.method as HttpMethod);
    setUrl(data.url);
    setParams(mapToRows(data.query));
    setReqHeaders(mapToRows(data.headers));
    if (data.body_content && data.body_kind) {
      setBodyKind(data.body_kind as BodyKind);
      setBodyContent(data.body_content);
    } else {
      setBodyKind("none");
      setBodyContent("");
    }
    setNotes(data.notes ?? "");
    setCaseNames(data.case_names ?? []);
    setSelectedCase("");
    setIsDirty(false);
    setSaveStatus("idle");
  }

  function buildRequestData(overrides?: Partial<RequestData>): RequestData {
    return {
      id: reqId, name: reqName || reqId, method, url,
      headers: toMap(reqHeaders), query: toMap(params),
      body_content: bodyKind !== "none" ? bodyContent : undefined,
      body_kind: bodyKind !== "none" ? bodyKind : undefined,
      notes: notes.trim() || undefined, tags: [], case_names: [],
      ...overrides,
    };
  }

  async function reloadProject() {
    try {
      const data = await invoke<ProjectData>("reload_project");
      setProject(data);
    } catch { /* ignore */ }
  }

  async function refreshSessionVars() {
    try {
      const vars = await invoke<Record<string, string>>("get_session_vars");
      setSessionVars(vars);
    } catch { /* ignore */ }
  }

  async function reloadPlugins() {
    try {
      const list = await invoke<PluginManifest[]>("list_plugins");
      setPlugins(list);
    } catch { /* no project or no tools dir — ok */ }
  }

  function resetToAdhoc() {
    setSelectedFilePath(null);
    setIsNewRequest(false);
    setNewReqDisplayName(""); setNewReqFolder("");
    setUrl(""); setParams([mkRow()]); setReqHeaders([mkRow()]);
    setBodyKind("none"); setBodyContent(""); setNotes("");
    setResponse(null); setSavedMeta(null); setSketchYaml(null); setReqError(null);
    setIsDirty(false); setSaveStatus("idle");
    setCaseNames([]); setSelectedCase("");
    setLastChecks([]); setLastCaptured({});
    setPlugins([]); setPluginResults({}); setPluginLoading({});
  }

  // ── Project actions ───────────────────────────────────────────────────────

  async function openProject() {
    try {
      const data = await invoke<ProjectData>("open_project");
      setProject(data); setSelectedEnvId(null); resetToAdhoc();
      await reloadPlugins();
    } catch (e) {
      if (String(e) !== "cancelled") setReqError(`Failed to open project: ${e}`);
    }
  }

  async function newProject() {
    try {
      const data = await invoke<ProjectData>("create_project");
      setProject(data); setSelectedEnvId(null); resetToAdhoc();
      await reloadPlugins();
    } catch (e) {
      if (String(e) !== "cancelled") setReqError(`Failed to create project: ${e}`);
    }
  }

  function addNewRequest() {
    setSelectedFilePath(null); setIsNewRequest(true);
    setNewReqDisplayName(""); setNewReqFolder("");
    setMethod("GET"); setUrl(""); setParams([mkRow()]); setReqHeaders([mkRow()]);
    setBodyKind("none"); setBodyContent(""); setNotes("");
    setResponse(null); setSavedMeta(null); setSketchYaml(null); setReqError(null);
    setIsDirty(false); setSaveStatus("idle");
    setCaseNames([]); setSelectedCase("");
    setLastChecks([]); setLastCaptured({});
    setPluginResults({}); setPluginLoading({});
  }

  async function selectRequest(filePath: string) {
    setResponse(null); setSavedMeta(null); setSketchYaml(null); setReqError(null); setIsNewRequest(false);
    setLastChecks([]); setLastCaptured({});
    setPluginResults({}); setPluginLoading({});
    try {
      const data = await invoke<RequestData>("get_request", { filePath });
      setSelectedFilePath(filePath);
      loadRequestData(data);
      try {
        const saved = await invoke<StoredResponse | null>("get_latest_response", { requestId: data.id });
        if (saved) {
          setResponse(saved);
          setSavedMeta({ ran_at: saved.ran_at, environment: saved.environment });
        }
      } catch { /* no saved response — that's fine */ }
    } catch (e) { setReqError(String(e)); }
  }

  // ── Save actions ──────────────────────────────────────────────────────────

  async function save() {
    if (isNewRequest) await saveNew();
    else if (selectedFilePath) await saveExisting();
  }

  async function saveExisting() {
    if (!selectedFilePath) return;
    setSaveStatus("saving");
    try {
      await invoke("save_request", { filePath: selectedFilePath, data: buildRequestData() });
      setIsDirty(false); setSaveStatus("saved");
      setTimeout(() => setSaveStatus("idle"), 2000);
    } catch (e) { setSaveStatus("idle"); setReqError(String(e)); }
  }

  async function saveNew() {
    const displayName = newReqDisplayName.trim() || "New Request";
    const id = slugify(displayName) || "new-request";
    const folder = slugify(newReqFolder);
    const filePath = folder ? `requests/${folder}/${id}.yaml` : `requests/${id}.yaml`;
    setSaveStatus("saving");
    try {
      await invoke("save_request", { filePath, data: buildRequestData({ id, name: displayName }) });
      await reloadProject();
      setSelectedFilePath(filePath); setReqId(id); setReqName(displayName);
      setIsNewRequest(false); setIsDirty(false); setSaveStatus("saved");
      setTimeout(() => setSaveStatus("idle"), 2000);
    } catch (e) { setSaveStatus("idle"); setReqError(String(e)); }
  }

  // ── Run action ────────────────────────────────────────────────────────────

  async function send() {
    if (!url.trim() && !isProjectMode) return;
    setLoading(true); setReqError(null); setResponse(null); setSavedMeta(null); setSketchYaml(null);
    setLastChecks([]); setLastCaptured({}); setPluginResults({});
    try {
      let resp: HttpResponse;
      if (isProjectMode) {
        const result = await invoke<RunResult>("run_project_request", {
          filePath: selectedFilePath,
          envId: selectedEnvId,
          caseName: selectedCase || null,
        });
        resp = result.response;
        setLastChecks(result.checks);
        setLastCaptured(result.captured);
        if (Object.keys(result.captured).length > 0) {
          await refreshSessionVars();
        }
      } else {
        resp = await invoke<HttpResponse>("execute_request", {
          method, url: url.trim(),
          headers: toMap(reqHeaders), query: toMap(params),
          bodyContent: bodyContent || null,
          bodyKind: bodyKind !== "none" ? bodyKind : null,
        });
      }
      setResponse(resp);
      const ct = resp.headers["content-type"] ?? "";
      const looksJson = ct.includes("json") || resp.body.trimStart().startsWith("{") || resp.body.trimStart().startsWith("[");
      if (looksJson && resp.body.trim()) {
        try {
          const yaml = await invoke<string>("sketch_json", { body: resp.body });
          setSketchYaml(yaml);
        } catch { /* non-JSON body — no sketch */ }
      }
      setResTab("body");
    } catch (e) { setReqError(String(e)); }
    finally { setLoading(false); }
  }

  async function saveSketch() {
    if (!reqId || !sketchYaml) return;
    try {
      await invoke("save_sketch", { requestId: reqId, yaml: sketchYaml });
    } catch (e) { setReqError(String(e)); }
  }

  async function runPlugin(pluginId: string) {
    if (!response || !selectedFilePath) return;
    setPluginLoading((prev) => ({ ...prev, [pluginId]: true }));
    try {
      const result = await invoke<PluginOutput>("run_plugin_command", {
        pluginId,
        filePath: selectedFilePath,
        responseStatus: response.status,
        responseStatusText: response.status_text,
        responseHeaders: response.headers,
        responseBody: response.body,
        responseDurationMs: response.duration_ms,
        responseUrl: response.url,
      });
      setPluginResults((prev) => ({ ...prev, [pluginId]: result }));
    } catch (e) {
      setPluginResults((prev) => ({
        ...prev,
        [pluginId]: { artifacts: [], diagnostics: [], error: { message: String(e) } },
      }));
    } finally {
      setPluginLoading((prev) => ({ ...prev, [pluginId]: false }));
    }
  }

  async function exportMarkdown() {
    if (!selectedFilePath) return;
    try {
      const path = await invoke<string>("export_request_markdown", { filePath: selectedFilePath });
      setExportedPath(path);
      setTimeout(() => setExportedPath(null), 4000);
    } catch (e) { setReqError(String(e)); }
  }

  async function clearSession() {
    await invoke("clear_session_vars");
    setSessionVars({});
  }

  const methodsWithBody: HttpMethod[] = ["POST", "PUT", "PATCH"];

  const saveBtnClass = [
    "save-button",
    isDirty || isNewRequest ? "save-button-dirty" : "",
    saveStatus === "saved" ? "save-button-saved" : "",
  ].filter(Boolean).join(" ");

  return (
    <div className="app" onKeyDown={(e) => e.key === "Enter" && (e.metaKey || e.ctrlKey) && send()}>
      <header className="toolbar">
        <span className="app-name">API Almanac</span>
      </header>

      <Sidebar
        project={project}
        selectedFilePath={selectedFilePath}
        selectedEnvId={selectedEnvId}
        onNewProject={newProject}
        onOpenProject={openProject}
        onAddRequest={addNewRequest}
        onSelectRequest={selectRequest}
        onEnvChange={setSelectedEnvId}
        onRunChecks={() => { setShowSpotCheck(true); setShowEnvEditor(false); setSelectedFilePath(null); setIsNewRequest(false); }}
        onEditEnvs={() => { setShowEnvEditor(true); setShowSpotCheck(false); setSelectedFilePath(null); setIsNewRequest(false); }}
      />

      <div className="main-area">
        {showEnvEditor ? (
          <EnvironmentPanel
            selectedEnvId={selectedEnvId}
            onClose={() => setShowEnvEditor(false)}
            onProjectChange={(newProject, newEnvId) => {
              setProject(newProject);
              if (newEnvId !== undefined) setSelectedEnvId(newEnvId);
            }}
          />
        ) : showSpotCheck && project ? (
          <SpotCheckPanel
            project={project}
            selectedEnvId={selectedEnvId}
            onClose={() => setShowSpotCheck(false)}
          />
        ) : (
          <>
        {/* New-request name bar */}
        {isNewRequest && (
          <div className="new-req-bar">
            <input
              className="new-req-name"
              placeholder="Request name"
              value={newReqDisplayName}
              onChange={(e) => setNewReqDisplayName(e.target.value)}
              autoFocus
            />
            <input
              className="new-req-folder"
              placeholder="Folder (optional)"
              value={newReqFolder}
              onChange={(e) => setNewReqFolder(e.target.value)}
            />
          </div>
        )}

        {/* URL bar */}
        <div className="url-bar">
          <select
            className="method-select"
            value={method}
            onChange={(e) => { setMethod(e.target.value as HttpMethod); if (isProjectMode) markDirty(); }}
          >
            {(["GET","POST","PUT","PATCH","DELETE","HEAD","OPTIONS"] as HttpMethod[]).map(
              (m) => <option key={m}>{m}</option>
            )}
          </select>
          <input
            className="url-input"
            placeholder={isProjectMode ? "{{base_url}}/endpoint" : "https://api.example.com/endpoint"}
            value={url}
            onChange={(e) => { setUrl(e.target.value); if (isProjectMode) markDirty(); }}
            onKeyDown={(e) => e.key === "Enter" && send()}
            spellCheck={false}
          />
          {isProjectMode && caseNames.length > 0 && (
            <select
              className="case-select"
              value={selectedCase}
              onChange={(e) => setSelectedCase(e.target.value)}
              title="Select case"
            >
              <option value="">Default</option>
              {caseNames.map((c) => <option key={c} value={c}>{c}</option>)}
            </select>
          )}
          {showSave && (
            <button
              className={saveBtnClass}
              onClick={save}
              disabled={saveStatus === "saving"}
              title={isNewRequest ? "Save as new request" : "Save changes to YAML"}
            >
              {saveStatus === "saving" ? "Saving…" : saveStatus === "saved" ? "Saved" : isNewRequest ? "Save New" : "Save"}
            </button>
          )}
          {isProjectMode && !isNewRequest && (
            <button
              className="export-button"
              onClick={exportMarkdown}
              title="Export as Markdown notebook to docs/"
            >
              Export MD
            </button>
          )}
          <button
            className="send-button"
            onClick={send}
            disabled={loading || (!isProjectMode && !url.trim())}
          >
            {loading ? "Sending…" : isProjectMode ? "Run" : "Send"}
          </button>
        </div>
        {exportedPath && (
          <div className="export-flash">Exported → <code>{exportedPath}</code></div>
        )}

        {/* Session vars bar */}
        <SessionBar vars={sessionVars} onClear={clearSession} />

        {/* Request pane */}
        <div className="request-pane">
          <div className="tab-bar">
            {(["params","headers","body","notes"] as RequestTab[]).map((t) => (
              <button
                key={t}
                className={`tab${reqTab === t ? " tab-active" : ""}`}
                onClick={() => setReqTab(t)}
              >
                {t.charAt(0).toUpperCase() + t.slice(1)}
                {t === "params" && activeRows(params).length > 0 && (
                  <span className="tab-count">{activeRows(params).length}</span>
                )}
                {t === "headers" && activeRows(reqHeaders).length > 0 && (
                  <span className="tab-count">{activeRows(reqHeaders).length}</span>
                )}
                {t === "notes" && notes.trim() && (
                  <span className="notes-dot" title="Has notes" />
                )}
              </button>
            ))}
            {isProjectMode && isDirty && <span className="dirty-dot" title="Unsaved changes" />}
          </div>
          <div className="tab-content">
            {reqTab === "params" && (
              <KvEditor rows={params} onChange={(v) => { setParams(v); if (isProjectMode) markDirty(); }}
                keyPlaceholder="Parameter" valuePlaceholder="Value" />
            )}
            {reqTab === "headers" && (
              <KvEditor rows={reqHeaders} onChange={(v) => { setReqHeaders(v); if (isProjectMode) markDirty(); }}
                keyPlaceholder="Header" valuePlaceholder="Value" />
            )}
            {reqTab === "body" && (
              <div className="body-editor">
                <div className="body-kind-bar">
                  {(["none","json","text","form"] as BodyKind[]).map((k) => (
                    <label key={k} className={`body-kind${bodyKind === k ? " body-kind-active" : ""}`}>
                      <input type="radio" name="bodyKind" value={k} checked={bodyKind === k}
                        onChange={() => { setBodyKind(k); if (isProjectMode) markDirty(); }} />
                      {k === "none" ? "None" : k.toUpperCase()}
                    </label>
                  ))}
                </div>
                {bodyKind !== "none" && (
                  <textarea
                    className="body-textarea"
                    placeholder={bodyKind === "json" ? '{\n  "key": "value"\n}' : bodyKind === "form" ? "key=value&key2=value2" : "Request body"}
                    value={bodyContent}
                    onChange={(e) => { setBodyContent(e.target.value); if (isProjectMode) markDirty(); }}
                    spellCheck={false}
                  />
                )}
                {bodyKind !== "none" && !methodsWithBody.includes(method) && (
                  <p className="body-warn">Note: {method} requests typically don't include a body.</p>
                )}
              </div>
            )}
            {reqTab === "notes" && (
              <div className="notes-editor">
                <textarea
                  className="notes-textarea"
                  placeholder="Add notes about this request — what it does, caveats, related requests, what to check…"
                  value={notes}
                  onChange={(e) => { setNotes(e.target.value); if (isProjectMode || isNewRequest) markDirty(); }}
                  spellCheck
                />
                {notes.trim() && isProjectMode && (
                  <p className="notes-hint">Notes are included when exporting to Markdown.</p>
                )}
              </div>
            )}
          </div>
        </div>

        {/* Response pane */}
        <div className="response-pane">
          {reqError && (
            <div className="response-error"><strong>Error:</strong> {reqError}</div>
          )}
          {loading && <div className="response-loading">Sending request…</div>}
          {response && (
            <>
              <div className="response-meta">
                <StatusBadge status={response.status} />
                <span className="response-status-text">{response.status_text}</span>
                <span className="response-duration">{response.duration_ms} ms</span>
                <span className="response-url" title={response.url}>{response.url}</span>
                {savedMeta && (
                  <span className="saved-badge" title={`Saved ${savedMeta.ran_at}${savedMeta.environment ? ` · env ${savedMeta.environment}` : ""}`}>
                    saved · {new Date(savedMeta.ran_at).toLocaleString()}
                  </span>
                )}
              </div>
              <ChecksPanel checks={lastChecks} captured={lastCaptured} />
              <div className="tab-bar">
                {([
                  "body",
                  "headers",
                  ...(sketchYaml ? ["sketch" as ResponseTab] : []),
                  ...(plugins.length > 0 ? ["tools" as ResponseTab] : []),
                ] as ResponseTab[]).map((t) => (
                  <button
                    key={t}
                    className={`tab${resTab === t ? " tab-active" : ""}`}
                    onClick={() => setResTab(t)}
                  >
                    {t.charAt(0).toUpperCase() + t.slice(1)}
                    {t === "headers" && (
                      <span className="tab-count">{Object.keys(response.headers).length}</span>
                    )}
                    {t === "tools" && Object.keys(pluginResults).length > 0 && (
                      <span className="tab-count">{Object.keys(pluginResults).length}</span>
                    )}
                  </button>
                ))}
              </div>
              <div className="tab-content response-content">
                {resTab === "body" && (
                  <PrettyBody body={response.body} contentType={response.headers["content-type"]} />
                )}
                {resTab === "headers" && (
                  <table className="headers-table">
                    <tbody>
                      {Object.entries(response.headers).sort(([a],[b]) => a.localeCompare(b)).map(([k,v]) => (
                        <tr key={k}>
                          <td className="header-key">{k}</td>
                          <td className="header-value">{v}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
                {resTab === "sketch" && sketchYaml && (
                  <div className="sketch-pane">
                    <div className="sketch-actions">
                      <button className="sketch-btn" onClick={() => navigator.clipboard.writeText(sketchYaml)}>
                        Copy
                      </button>
                      {isProjectMode && reqId && (
                        <button className="sketch-btn sketch-save-btn" onClick={saveSketch}>
                          Save Sketch
                        </button>
                      )}
                    </div>
                    <pre className="sketch-yaml">{sketchYaml}</pre>
                  </div>
                )}
                {resTab === "tools" && (
                  <div className="tools-pane">
                    {plugins.map((plugin) => (
                      <div key={plugin.id} className="tool-section">
                        <div className="tool-header">
                          <span className="tool-name">{plugin.name}</span>
                          {plugin.description && (
                            <span className="tool-desc">{plugin.description}</span>
                          )}
                          <button
                            className="tool-run-btn"
                            onClick={() => runPlugin(plugin.id)}
                            disabled={!!pluginLoading[plugin.id]}
                          >
                            {pluginLoading[plugin.id] ? "Running…" : "Run"}
                          </button>
                        </div>
                        {pluginResults[plugin.id] && (
                          <div className="tool-artifacts">
                            {pluginResults[plugin.id].error && (
                              <div className="tool-error">
                                {pluginResults[plugin.id].error!.message}
                              </div>
                            )}
                            {pluginResults[plugin.id].artifacts.map((artifact, i) => (
                              <div key={i} className="artifact">
                                <div className="artifact-header">
                                  <span className="artifact-title">{artifact.title}</span>
                                  <span className="artifact-kind">{artifact.kind}</span>
                                </div>
                                {artifact.kind === "html" ? (
                                  <iframe
                                    className="artifact-iframe"
                                    sandbox="allow-scripts"
                                    srcDoc={artifact.content}
                                    title={artifact.title}
                                  />
                                ) : (
                                  <pre className="artifact-pre">{artifact.content}</pre>
                                )}
                              </div>
                            ))}
                            {pluginResults[plugin.id].diagnostics?.length > 0 && (
                              <div className="artifact-diagnostics">
                                {pluginResults[plugin.id].diagnostics.map((d, i) => (
                                  <div key={i}>{d}</div>
                                ))}
                              </div>
                            )}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </>
          )}
          {!response && !reqError && !loading && (
            <div className="response-empty">
              {isProjectMode
                ? "Click Run to execute with environment substitution."
                : isNewRequest
                  ? "Fill in the details above and click Save New, then Run."
                  : "Send a request to see the response."}
            </div>
          )}
        </div>
          </>
        )}
      </div>
    </div>
  );
}
