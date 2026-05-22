import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove as _arrayMove,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import "./App.css";

// ── Types ──────────────────────────────────────────────────────────────────

type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
type BodyKind = "none" | "json" | "text" | "form";
type RequestTab = "params" | "headers" | "body" | "cases" | "notes" | "captures" | "expects";
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
  uid: string;
  id: string;
  name: string;
  method: string;
  folder: string;   // raw folder path, may include numeric prefix e.g. "1-auth"
  file_path: string;
  order: number;    // numeric prefix from filename; Number.MAX_SAFE_INTEGER if unprefixed
}

interface EnvSummary {
  id: string;
  name: string;
}

interface FolderSummary {
  path: string;   // raw relative path, e.g. "1-auth" or "1-auth/2-oauth"
  label: string;  // display: prefix stripped at each component, e.g. "auth" or "auth/oauth"
  order: number;  // numeric order of the last component
}

interface ProjectData {
  name: string;
  id: string;
  description?: string;
  requests: RequestSummary[];
  environments: EnvSummary[];
  folders: FolderSummary[];
}

interface MoveResult {
  new_file_path: string;
  project: ProjectData;
}

interface ReorderResult {
  moved_path: string;
  project: ProjectData;
}

interface CreateGroupResult {
  folder_path: string;
  project: ProjectData;
}

interface RecentProject {
  path: string;
  name: string;
}

interface ExpectData {
  status?: number;
  time_ms?: string;
  headers?: Record<string, string>;
  json?: Record<string, string>;
}

interface RequestData {
  uid: string;
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
  cases: Record<string, Record<string, string>>;
  capture?: Record<string, string>;
  expect?: ExpectData;
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
  parent?: string;
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

// ── Captures editor ────────────────────────────────────────────────────────

function CapturesEditor({
  rows,
  onChange,
  liveValues,
}: {
  rows: KvRow[];
  onChange: (rows: KvRow[]) => void;
  liveValues: Record<string, string>;
}) {
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const toggle = (id: number) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  const update = (id: number, field: keyof KvRow, val: string | boolean) =>
    onChange(rows.map((r) => (r.id === id ? { ...r, [field]: val } : r)));
  const remove = (id: number) => {
    setExpanded((prev) => { const n = new Set(prev); n.delete(id); return n; });
    onChange(rows.filter((r) => r.id !== id));
  };
  return (
    <div className="kv-editor">
      {rows.map((row) => {
        const live = row.key ? liveValues[row.key] : undefined;
        const isOpen = expanded.has(row.id);
        const eyeTitle = live
          ? (live.length > 80 ? `${live.slice(0, 80)}… (${live.length} chars)` : live)
          : "No value captured yet";
        return (
          <div key={row.id} className="capture-row-wrap">
            <div className="kv-row">
              <input
                type="checkbox"
                checked={row.enabled}
                onChange={(e) => update(row.id, "enabled", e.target.checked)}
                title="Enable"
              />
              <input
                className="kv-input"
                placeholder="Variable name"
                value={row.key}
                onChange={(e) => update(row.id, "key", e.target.value)}
              />
              <input
                className="kv-input"
                placeholder="Path (e.g. json.id, headers.x-token)"
                value={row.value}
                onChange={(e) => update(row.id, "value", e.target.value)}
              />
              <button
                className={`capture-eye${live ? " has-value" : ""}${isOpen ? " open" : ""}`}
                title={eyeTitle}
                disabled={!live}
                onClick={() => toggle(row.id)}
              >👁</button>
              <button className="kv-remove" onClick={() => remove(row.id)} title="Remove">×</button>
            </div>
            {isOpen && live && (
              <div className="capture-expanded">
                <code className="capture-value">{live}</code>
              </div>
            )}
          </div>
        );
      })}
      <button className="kv-add" onClick={() => onChange([...rows, mkRow()])}>+ Add</button>
    </div>
  );
}

// ── Expectations editor ────────────────────────────────────────────────────

function ExpectationsEditor({
  status, onStatusChange,
  timeMs, onTimeMsChange,
  headers, onHeadersChange,
  json, onJsonChange,
}: {
  status: string; onStatusChange: (v: string) => void;
  timeMs: string; onTimeMsChange: (v: string) => void;
  headers: KvRow[]; onHeadersChange: (rows: KvRow[]) => void;
  json: KvRow[]; onJsonChange: (rows: KvRow[]) => void;
}) {
  return (
    <div className="expects-editor">
      <div className="expects-scalars">
        <label className="expects-label">Status</label>
        <input
          className="expects-scalar-input"
          type="number"
          placeholder="200"
          value={status}
          onChange={(e) => onStatusChange(e.target.value)}
        />
        <label className="expects-label">Time (ms)</label>
        <input
          className="expects-scalar-input"
          type="text"
          placeholder="&lt; 500"
          value={timeMs}
          onChange={(e) => onTimeMsChange(e.target.value)}
        />
      </div>
      <div className="expects-section">
        <div className="expects-section-label">Headers</div>
        <KvEditor
          rows={headers}
          onChange={onHeadersChange}
          keyPlaceholder="Content-Type"
          valuePlaceholder="contains application/json"
        />
      </div>
      <div className="expects-section">
        <div className="expects-section-label">JSON</div>
        <KvEditor
          rows={json}
          onChange={onJsonChange}
          keyPlaceholder="json.field.path"
          valuePlaceholder="exists · equals value · contains text"
        />
      </div>
      <p className="captures-hint">
        Rules: <code>exists</code> · <code>equals X</code> · <code>contains X</code> · time ops: <code>&lt; 500</code> <code>&lt;= 1000</code>
      </p>
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
  onRunComplete,
}: {
  project: ProjectData;
  selectedEnvId: string | null;
  onRunComplete: (summary: { passed: number; failed: number; errored: number }) => void;
}) {
  const [running, setRunning] = useState(false);
  const [current, setCurrent] = useState(0);
  const [results, setResults] = useState<SpotCheckResult[]>([]);
  const [ranAt, setRanAt] = useState<string | null>(null);
  const [totalMs, setTotalMs] = useState(0);
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [exportErr, setExportErr] = useState<string | null>(null);

  const envName = project.environments.find((e) => e.id === selectedEnvId)?.name ?? null;
  const orderedRequests = flattenTree(buildTree(project.folders, project.requests));
  const total = orderedRequests.length;
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

    for (let i = 0; i < orderedRequests.length; i++) {
      setCurrent(i + 1);
      const req = orderedRequests[i];
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
    const finalPassed = acc.filter((r) => !r.error && (r.checks.length === 0 || r.checks.every((c) => c.passed))).length;
    const finalFailed = acc.filter((r) => !r.error && r.checks.some((c) => !c.passed)).length;
    const finalErrored = acc.filter((r) => !!r.error).length;
    onRunComplete({ passed: finalPassed, failed: finalFailed, errored: finalErrored });
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

function isDescendant(candidateId: string, ancestorId: string, envs: EnvironmentData[]): boolean {
  let cur: string | undefined = candidateId;
  const seen = new Set<string>();
  while (cur) {
    if (seen.has(cur)) return false;
    seen.add(cur);
    if (cur === ancestorId) return true;
    cur = envs.find((e) => e.id === cur)?.parent;
  }
  return false;
}

function EnvironmentPanel({
  selectedEnvId,
  onProjectChange,
}: {
  selectedEnvId: string | null;
  onProjectChange: (project: ProjectData, newEnvId: string | null) => void;
}) {
  const [envs, setEnvs] = useState<EnvironmentData[]>([]);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [editParentId, setEditParentId] = useState<string | null>(null);
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
          setEditParentId(first.parent ?? null);
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
    setEditParentId(fresh.parent ?? null);
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
        parent: editParentId ?? undefined,
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
        setEditParentId(null);
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

              <div className="env-editor-name-row">
                <label className="env-editor-label">Inherits from</label>
                <select
                  className="env-editor-parent"
                  value={editParentId ?? ""}
                  onChange={(e) => { setEditParentId(e.target.value || null); setIsDirty(true); }}
                >
                  <option value="">(none)</option>
                  {envs
                    .filter((e) => e.id !== editingId && !isDescendant(e.id, editingId!, envs))
                    .map((e) => <option key={e.id} value={e.id}>{e.name}</option>)}
                </select>
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

              {(() => {
                if (!editParentId) return null;
                const ownKeys = new Set(editVars.filter((r) => r.enabled && r.key.trim()).map((r) => r.key));
                const merged: Record<string, string> = {};
                const chain: string[] = [];
                let cur: string | undefined = editParentId;
                const visited = new Set<string>();
                while (cur && !visited.has(cur)) {
                  visited.add(cur); chain.push(cur);
                  cur = envs.find((e) => e.id === cur)?.parent;
                }
                for (const id of chain.reverse()) {
                  const env = envs.find((e) => e.id === id);
                  if (env) Object.assign(merged, env.vars);
                }
                const rows = Object.entries(merged);
                if (rows.length === 0) return null;
                return (
                  <div className="env-inherited">
                    <div className="env-vars-label">
                      Inherited from <em>{envs.find((e) => e.id === editParentId)?.name ?? editParentId}</em>
                    </div>
                    {rows.map(([k, v]) => (
                      <div key={k} className={`env-inherited-row${ownKeys.has(k) ? " env-inherited-overridden" : ""}`}>
                        <span className="env-inherited-key">{k}</span>
                        <span className="env-inherited-value">{v}</span>
                        {ownKeys.has(k) && <span className="env-inherited-badge">overridden</span>}
                      </div>
                    ))}
                  </div>
                );
              })()}

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

// ── Sidebar tree types & builder ───────────────────────────────────────────

type TreeItem =
  | { kind: "folder"; path: string; label: string; children: TreeItem[] }
  | { kind: "request"; req: RequestSummary };

function buildTree(
  allFolders: FolderSummary[],
  requests: RequestSummary[],
  parentPath: string = ""
): TreeItem[] {
  const levelReqs = requests
    .filter((r) => r.folder === parentPath)
    .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name));

  const directChildren = allFolders
    .filter((f) => {
      if (parentPath === "") return !f.path.includes("/");
      const prefix = parentPath + "/";
      return f.path.startsWith(prefix) && !f.path.slice(prefix.length).includes("/");
    })
    .sort((a, b) => a.order - b.order || a.label.localeCompare(b.label));

  return [
    ...levelReqs.map((req) => ({ kind: "request" as const, req })),
    ...directChildren.map((f) => ({
      kind: "folder" as const,
      path: f.path,
      label: f.label,
      children: buildTree(allFolders, requests, f.path),
    })),
  ];
}

function flattenTree(items: TreeItem[]): RequestSummary[] {
  const result: RequestSummary[] = [];
  for (const item of items) {
    if (item.kind === "request") result.push(item.req);
    else result.push(...flattenTree(item.children));
  }
  return result;
}

// ── Sortable drag-and-drop wrapper ─────────────────────────────────────────

function SortableItem({
  id,
  data,
  children,
}: {
  id: string;
  data: Record<string, unknown>;
  children: (dragHandleProps: React.HTMLAttributes<HTMLElement>) => React.ReactNode;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id, data });
  return (
    <div
      ref={setNodeRef}
      style={{
        transform: CSS.Transform.toString(transform),
        transition,
        opacity: isDragging ? 0.4 : 1,
      }}
    >
      {children({ ...attributes, ...listeners })}
    </div>
  );
}

// ── Sidebar ────────────────────────────────────────────────────────────────

type CtxMenu =
  | { kind: "folder"; path: string; x: number; y: number; siblingIndex: number; siblingCount: number }
  | { kind: "request"; filePath: string; reqName: string; x: number; y: number; siblingIndex: number; siblingCount: number };

type Renaming =
  | { kind: "folder"; path: string; value: string }
  | { kind: "request"; filePath: string; value: string };

function truncatePath(p: string): string {
  const parts = p.replace(/\\/g, "/").split("/").filter(Boolean);
  if (parts.length <= 2) return p;
  return "…/" + parts.slice(-2).join("/");
}

function Sidebar({
  project,
  selectedFilePath,
  selectedEnvId,
  recentProjects,
  onNewProject,
  onOpenProject,
  onOpenRecent,
  onAddRequest,
  onAddRequestToFolder,
  onSelectRequest,
  onEnvChange,
  onRunChecks,
  onEditEnvs,
  onCreateGroup,
  onRenameGroup,
  onDeleteGroup,
  onRenameRequest,
  onDeleteRequest,
  onMoveRequest,
  onDuplicateRequest,
  onReorderRequest,
  onReorderGroup,
}: {
  project: ProjectData | null;
  selectedFilePath: string | null;
  selectedEnvId: string | null;
  recentProjects: RecentProject[];
  onNewProject: () => void;
  onOpenProject: () => void;
  onOpenRecent: (path: string) => void;
  onAddRequest: () => void;
  onAddRequestToFolder: (folder: string) => void;
  onSelectRequest: (filePath: string) => void;
  onEnvChange: (envId: string | null) => void;
  onRunChecks: () => void;
  onEditEnvs: () => void;
  onCreateGroup: (path: string) => void;
  onRenameGroup: (oldFolder: string, newLabel: string) => void;
  onDeleteGroup: (folder: string) => void;
  onRenameRequest: (filePath: string, newName: string) => void;
  onDeleteRequest: (filePath: string) => void;
  onMoveRequest: (filePath: string, newFolder: string) => void;
  onDuplicateRequest: (filePath: string) => void;
  onReorderRequest: (filePath: string, newPosition: number) => void;
  onReorderGroup: (folder: string, newPosition: number) => void;
}) {
  const [showRecentMenu, setShowRecentMenu] = useState(false);
  const [ctxMenu, setCtxMenu] = useState<CtxMenu | null>(null);
  const [renaming, setRenaming] = useState<Renaming | null>(null);
  const [moving, setMoving] = useState<{ filePath: string; x: number; y: number } | null>(null);
  const [newGroup, setNewGroup] = useState<{ parentPath: string; value: string } | null>(null);
  const [activeDragId, setActiveDragId] = useState<string | null>(null);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));

  const recentWrapRef = useRef<HTMLDivElement>(null);
  const ctxMenuRef = useRef<HTMLDivElement>(null);
  const movingRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!showRecentMenu) return;
    function handle(e: MouseEvent) {
      if (recentWrapRef.current && !recentWrapRef.current.contains(e.target as Node)) {
        setShowRecentMenu(false);
      }
    }
    document.addEventListener("mousedown", handle);
    return () => document.removeEventListener("mousedown", handle);
  }, [showRecentMenu]);

  useEffect(() => {
    if (!ctxMenu) return;
    function handle(e: MouseEvent) {
      if (ctxMenuRef.current && !ctxMenuRef.current.contains(e.target as Node)) {
        setCtxMenu(null);
      }
    }
    document.addEventListener("mousedown", handle);
    return () => document.removeEventListener("mousedown", handle);
  }, [ctxMenu]);

  useEffect(() => {
    if (!moving) return;
    function handle(e: MouseEvent) {
      if (movingRef.current && !movingRef.current.contains(e.target as Node)) {
        setMoving(null);
      }
    }
    document.addEventListener("mousedown", handle);
    return () => document.removeEventListener("mousedown", handle);
  }, [moving]);

  const allFolders = project?.folders ?? [];
  const tree = project ? buildTree(allFolders, project.requests) : [];

  function handleDragStart(event: DragStartEvent) {
    setActiveDragId(String(event.active.id));
  }

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    setActiveDragId(null);
    if (!over || active.id === over.id) return;

    // dnd-kit exposes the SortableContext id as sortable.containerId on the data
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const activeContainer: string = (active.data.current as any)?.sortable?.containerId ?? "";
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const overContainer: string = (over.data.current as any)?.sortable?.containerId ?? activeContainer;

    if (activeContainer.startsWith("reqs:") && overContainer.startsWith("reqs:")) {
      const activeFolder = activeContainer.slice(5);
      const overFolder = overContainer.slice(5);
      if (activeFolder === overFolder) {
        // Same folder → reorder
        const sibs = (project?.requests ?? [])
          .filter((r) => r.folder === activeFolder)
          .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name));
        const newIdx = sibs.findIndex((r) => r.file_path === String(over.id));
        if (newIdx >= 0) onReorderRequest(String(active.id), newIdx);
      } else {
        // Cross-folder drop → move
        onMoveRequest(String(active.id), overFolder);
      }
    } else if (activeContainer.startsWith("dirs:") && overContainer.startsWith("dirs:")) {
      const activeParent = activeContainer.slice(5);
      const overParent = overContainer.slice(5);
      if (activeParent === overParent) {
        const sibs = allFolders
          .filter((f) => {
            if (activeParent === "") return !f.path.includes("/");
            const pref = activeParent + "/";
            return f.path.startsWith(pref) && !f.path.slice(pref.length).includes("/");
          })
          .sort((a, b) => a.order - b.order || a.label.localeCompare(b.label));
        const newIdx = sibs.findIndex((f) => f.path === String(over.id));
        if (newIdx >= 0) onReorderGroup(String(active.id), newIdx);
      }
    }
  }

  function renderItems(items: TreeItem[], depth: number, parentFolder: string): React.ReactNode {
    const reqItems = items
      .filter((i): i is { kind: "request"; req: RequestSummary } => i.kind === "request")
      .map((i) => i.req);
    const folderItems = items.filter(
      (i): i is { kind: "folder"; path: string; label: string; children: TreeItem[] } =>
        i.kind === "folder"
    );

    return (
      <>
        {/* Requests sortable group */}
        <SortableContext
          id={`reqs:${parentFolder}`}
          items={reqItems.map((r) => r.file_path)}
          strategy={verticalListSortingStrategy}
        >
          {reqItems.map((req) => {
            const isActive = selectedFilePath === req.file_path;
            const isRenamingThis =
              renaming?.kind === "request" && renaming.filePath === req.file_path;
            // Compute sibling index for Up/Down menu
            const reqSibs = (project?.requests ?? [])
              .filter((r) => r.folder === parentFolder)
              .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name));
            const reqSibIdx = reqSibs.findIndex((r) => r.file_path === req.file_path);

            return (
              <SortableItem
                key={req.file_path}
                id={req.file_path}
                data={{ type: "request", folder: parentFolder }}
              >
                {(dragHandleProps) => (
                  <div
                    className={`sidebar-req${isActive ? " sidebar-req-active" : ""}`}
                    style={{ paddingLeft: depth * 12 }}
                  >
                    <span className="drag-handle" title="Drag to reorder" {...dragHandleProps}>
                      ⠿
                    </span>
                    <div
                      className="sidebar-req-main"
                      onClick={() => !isRenamingThis && onSelectRequest(req.file_path)}
                    >
                      <MethodBadge method={req.method} />
                      {isRenamingThis ? (
                        <input
                          className="rename-input"
                          value={renaming.value}
                          autoFocus
                          onClick={(e) => e.stopPropagation()}
                          onChange={(e) =>
                            setRenaming({
                              kind: "request",
                              filePath: req.file_path,
                              value: e.target.value,
                            })
                          }
                          onKeyDown={(e) => {
                            if (e.key === "Enter" && renaming.value.trim()) {
                              onRenameRequest(req.file_path, renaming.value.trim());
                              setRenaming(null);
                            } else if (e.key === "Escape") {
                              setRenaming(null);
                            }
                          }}
                          onBlur={() => setRenaming(null)}
                        />
                      ) : (
                        <span className="sidebar-req-name">{req.name}</span>
                      )}
                    </div>
                    <button
                      className="sidebar-more-btn"
                      onClick={(e) => {
                        e.stopPropagation();
                        setCtxMenu({
                          kind: "request",
                          filePath: req.file_path,
                          reqName: req.name,
                          x: e.clientX,
                          y: e.clientY,
                          siblingIndex: reqSibIdx,
                          siblingCount: reqSibs.length,
                        });
                      }}
                      title="Options"
                    >
                      ⋮
                    </button>
                  </div>
                )}
              </SortableItem>
            );
          })}
        </SortableContext>

        {/* Folders sortable group */}
        <SortableContext
          id={`dirs:${parentFolder}`}
          items={folderItems.map((i) => i.path)}
          strategy={verticalListSortingStrategy}
        >
          {folderItems.map((item) => {
            const isRenamingThis =
              renaming?.kind === "folder" && renaming.path === item.path;
            const isAddingSubfolder = newGroup?.parentPath === item.path;
            // Compute sibling index for Up/Down menu
            const dirParent = item.path.includes("/")
              ? item.path.slice(0, item.path.lastIndexOf("/"))
              : "";
            const dirSibs = allFolders
              .filter((f) => {
                if (dirParent === "") return !f.path.includes("/");
                const pref = dirParent + "/";
                return f.path.startsWith(pref) && !f.path.slice(pref.length).includes("/");
              })
              .sort((a, b) => a.order - b.order || a.label.localeCompare(b.label));
            const dirSibIdx = dirSibs.findIndex((f) => f.path === item.path);

            return (
              <SortableItem
                key={item.path}
                id={item.path}
                data={{ type: "folder", parentFolder }}
              >
                {(dragHandleProps) => (
                  <div className="sidebar-folder">
                    <div className="sidebar-folder-row" style={{ paddingLeft: depth * 12 }}>
                      <span className="drag-handle" title="Drag to reorder" {...dragHandleProps}>
                        ⠿
                      </span>
                      {isRenamingThis ? (
                        <input
                          className="rename-input rename-input-folder"
                          value={renaming.value}
                          autoFocus
                          onChange={(e) =>
                            setRenaming({
                              kind: "folder",
                              path: item.path,
                              value: e.target.value,
                            })
                          }
                          onKeyDown={(e) => {
                            if (e.key === "Enter" && renaming.value.trim()) {
                              onRenameGroup(item.path, renaming.value.trim());
                              setRenaming(null);
                            } else if (e.key === "Escape") {
                              setRenaming(null);
                            }
                          }}
                          onBlur={() => setRenaming(null)}
                        />
                      ) : (
                        <span className="sidebar-folder-name">{item.label}</span>
                      )}
                      <button
                        className="sidebar-more-btn"
                        onClick={(e) => {
                          e.stopPropagation();
                          setCtxMenu({
                            kind: "folder",
                            path: item.path,
                            x: e.clientX,
                            y: e.clientY,
                            siblingIndex: dirSibIdx,
                            siblingCount: dirSibs.length,
                          });
                        }}
                        title="Folder options"
                      >
                        ⋮
                      </button>
                    </div>
                    <div className="sidebar-folder-children">
                      {renderItems(item.children, depth + 1, item.path)}
                      {isAddingSubfolder && (
                        <div
                          className="new-group-input-row"
                          style={{ paddingLeft: (depth + 1) * 12 }}
                        >
                          <input
                            className="rename-input"
                            placeholder="subfolder-name"
                            value={newGroup!.value}
                            autoFocus
                            onChange={(e) =>
                              setNewGroup({ parentPath: item.path, value: e.target.value })
                            }
                            onKeyDown={(e) => {
                              if (e.key === "Enter" && newGroup!.value.trim()) {
                                onCreateGroup(item.path + "/" + newGroup!.value.trim());
                                setNewGroup(null);
                              } else if (e.key === "Escape") {
                                setNewGroup(null);
                              }
                            }}
                            onBlur={() => setNewGroup(null)}
                          />
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </SortableItem>
            );
          })}
        </SortableContext>
      </>
    );
  }

  return (
    <div className="sidebar">
      <div className="sidebar-header">
        <div className="sidebar-btn-row">
          <button className="sidebar-btn" onClick={onOpenProject} title="Open existing project">Open</button>
          <button className="sidebar-btn" onClick={onNewProject} title="Create new project">New</button>
        </div>
        {recentProjects.length > 0 && (
          <div className="sidebar-recent-wrap" ref={recentWrapRef}>
            <button
              className={`sidebar-btn sidebar-recent-btn${showRecentMenu ? " sidebar-recent-btn-open" : ""}`}
              onClick={() => setShowRecentMenu((v) => !v)}
              title="Open a recent project"
            >
              Open Recent ▾
            </button>
            {showRecentMenu && (
              <div className="recent-menu">
                {recentProjects.map((rp) => (
                  <button
                    key={rp.path}
                    className="recent-menu-item"
                    onClick={() => { setShowRecentMenu(false); onOpenRecent(rp.path); }}
                    title={rp.path}
                  >
                    <span className="recent-item-name">{rp.name}</span>
                    <span className="recent-item-path">{truncatePath(rp.path)}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
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

      <DndContext sensors={sensors} onDragStart={handleDragStart} onDragEnd={handleDragEnd}>
      <div className="sidebar-tree">
        {project ? (
          <>
            {renderItems(tree, 0, "")}
            {newGroup?.parentPath === "" && (
              <div className="new-group-input-row">
                <input
                  className="rename-input"
                  placeholder="folder or parent/child"
                  value={newGroup.value}
                  autoFocus
                  onChange={(e) => setNewGroup({ parentPath: "", value: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && newGroup.value.trim()) {
                      onCreateGroup(newGroup.value.trim());
                      setNewGroup(null);
                    } else if (e.key === "Escape") {
                      setNewGroup(null);
                    }
                  }}
                  onBlur={() => setNewGroup(null)}
                />
              </div>
            )}
            {tree.length === 0 && !newGroup && (
              <p className="sidebar-empty">No requests yet.</p>
            )}
          </>
        ) : (
          <p className="sidebar-empty">Open or create a project to get started.</p>
        )}
      </div>

      <DragOverlay>
        {activeDragId ? (
          <div className="drag-overlay-item">
            {activeDragId.endsWith(".yaml")
              ? (project?.requests.find((r) => r.file_path === activeDragId)?.name ?? activeDragId)
              : (allFolders.find((f) => f.path === activeDragId)?.label ?? activeDragId)}
          </div>
        ) : null}
      </DragOverlay>
      </DndContext>

      {/* Context menu */}
      {ctxMenu && (
        <div
          ref={ctxMenuRef}
          className="context-menu"
          style={{ top: ctxMenu.y, left: ctxMenu.x }}
        >
          {ctxMenu.kind === "folder" ? (
            <>
              <button
                className="context-menu-item"
                onClick={() => {
                  // Show bare label (prefix stripped) in the rename input
                  const parts = ctxMenu.path.split("/");
                  const folderLabel =
                    allFolders.find((f) => f.path === ctxMenu.path)?.label ??
                    parts[parts.length - 1] ?? ctxMenu.path;
                  setRenaming({ kind: "folder", path: ctxMenu.path, value: folderLabel });
                  setCtxMenu(null);
                }}
              >
                Rename
              </button>
              {ctxMenu.siblingIndex > 0 && (
                <button
                  className="context-menu-item"
                  onClick={() => {
                    onReorderGroup(ctxMenu.path, ctxMenu.siblingIndex - 1);
                    setCtxMenu(null);
                  }}
                >
                  Move up
                </button>
              )}
              {ctxMenu.siblingIndex < ctxMenu.siblingCount - 1 && (
                <button
                  className="context-menu-item"
                  onClick={() => {
                    onReorderGroup(ctxMenu.path, ctxMenu.siblingIndex + 1);
                    setCtxMenu(null);
                  }}
                >
                  Move down
                </button>
              )}
              <button
                className="context-menu-item"
                onClick={() => {
                  setNewGroup({ parentPath: ctxMenu.path, value: "" });
                  setCtxMenu(null);
                }}
              >
                Add Subfolder
              </button>
              <button
                className="context-menu-item"
                onClick={() => {
                  onAddRequestToFolder(ctxMenu.path);
                  setCtxMenu(null);
                }}
              >
                Add Request Here
              </button>
              <div className="context-menu-sep" />
              <button
                className="context-menu-item context-menu-danger"
                onClick={() => {
                  const p = ctxMenu.path;
                  setCtxMenu(null);
                  onDeleteGroup(p);
                }}
              >
                Delete Group
              </button>
            </>
          ) : (
            <>
              <button
                className="context-menu-item"
                onClick={() => {
                  setRenaming({ kind: "request", filePath: ctxMenu.filePath, value: ctxMenu.reqName });
                  setCtxMenu(null);
                }}
              >
                Rename
              </button>
              <button
                className="context-menu-item"
                onClick={() => {
                  const fp = ctxMenu.filePath;
                  setCtxMenu(null);
                  onDuplicateRequest(fp);
                }}
              >
                Duplicate
              </button>
              {ctxMenu.siblingIndex > 0 && (
                <button
                  className="context-menu-item"
                  onClick={() => {
                    onReorderRequest(ctxMenu.filePath, ctxMenu.siblingIndex - 1);
                    setCtxMenu(null);
                  }}
                >
                  Move up
                </button>
              )}
              {ctxMenu.siblingIndex < ctxMenu.siblingCount - 1 && (
                <button
                  className="context-menu-item"
                  onClick={() => {
                    onReorderRequest(ctxMenu.filePath, ctxMenu.siblingIndex + 1);
                    setCtxMenu(null);
                  }}
                >
                  Move down
                </button>
              )}
              <button
                className="context-menu-item"
                onClick={() => {
                  setMoving({ filePath: ctxMenu.filePath, x: ctxMenu.x, y: ctxMenu.y });
                  setCtxMenu(null);
                }}
              >
                Move to...
              </button>
              <div className="context-menu-sep" />
              <button
                className="context-menu-item context-menu-danger"
                onClick={() => {
                  const fp = ctxMenu.filePath;
                  setCtxMenu(null);
                  onDeleteRequest(fp);
                }}
              >
                Delete
              </button>
            </>
          )}
        </div>
      )}

      {/* Move picker */}
      {moving && (
        <div
          ref={movingRef}
          className="move-picker"
          style={{ top: moving.y, left: moving.x }}
        >
          <div className="move-picker-title">Move to</div>
          <button
            className="move-picker-item"
            onClick={() => { onMoveRequest(moving.filePath, ""); setMoving(null); }}
          >
            Root (top level)
          </button>
          {allFolders.map((f) => (
            <button
              key={f.path}
              className="move-picker-item"
              onClick={() => { onMoveRequest(moving.filePath, f.path); setMoving(null); }}
            >
              {f.label}
            </button>
          ))}
        </div>
      )}

      {project && (
        <div className="sidebar-footer">
          <button className="sidebar-add-req-btn" onClick={onAddRequest}>+ Add Request</button>
          <button
            className="sidebar-add-req-btn"
            onClick={() => setNewGroup({ parentPath: "", value: "" })}
          >
            + New Group
          </button>
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
  // cases: full case data for editing; editingCaseName: which case is open in the Cases tab
  const [cases, setCases] = useState<Record<string, KvRow[]>>({});
  const [editingCaseName, setEditingCaseName] = useState<string | null>(null);
  const [newCaseInput, setNewCaseInput] = useState("");
  const [isDirty, setIsDirty] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved">("idle");

  // New-request mode
  const [isNewRequest, setIsNewRequest] = useState(false);
  const [newReqDisplayName, setNewReqDisplayName] = useState("");
  const [newReqFolder, setNewReqFolder] = useState("");

  // Stored id/name for save
  const [reqUid, setReqUid] = useState("");
  const [reqId, setReqId] = useState("");
  const [reqName, setReqName] = useState("");

  // Request edit state
  const [notes, setNotes] = useState("");
  const [method, setMethod] = useState<HttpMethod>("GET");
  const [url, setUrl] = useState("");
  const [params, setParams] = useState<KvRow[]>([mkRow()]);
  const [reqHeaders, setReqHeaders] = useState<KvRow[]>([mkRow()]);
  const [captures, setCaptures] = useState<KvRow[]>([mkRow()]);
  const [expectStatus, setExpectStatus] = useState<string>("");
  const [expectTimeMs, setExpectTimeMs] = useState<string>("");
  const [expectHeaders, setExpectHeaders] = useState<KvRow[]>([mkRow()]);
  const [expectJson, setExpectJson] = useState<KvRow[]>([mkRow()]);
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
  const [mainPane, setMainPane] = useState<'request' | 'checks' | 'environments'>('request');
  const [spotCheckSummary, setSpotCheckSummary] = useState<{ passed: number; failed: number; errored: number } | null>(null);
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);

  const isProjectMode = selectedFilePath !== null;
  const showSave = isProjectMode || (project !== null && isNewRequest);
  const caseNames = Object.keys(cases).sort();
  const activeRows = (rows: KvRow[]) => rows.filter((r) => r.enabled && r.key.trim());
  const toMap = (rows: KvRow[]) =>
    Object.fromEntries(activeRows(rows).map((r) => [r.key, r.value]));

  function markDirty() { setIsDirty(true); setSaveStatus("idle"); }

  function loadRequestData(data: RequestData) {
    setReqUid(data.uid);
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
    setCaptures(mapToRows(data.capture ?? {}));
    setExpectStatus(data.expect?.status != null ? String(data.expect.status) : "");
    setExpectTimeMs(data.expect?.time_ms ?? "");
    setExpectHeaders(mapToRows(data.expect?.headers ?? {}));
    setExpectJson(mapToRows(data.expect?.json ?? {}));
    const newCases: Record<string, KvRow[]> = {};
    for (const [name, vars] of Object.entries(data.cases ?? {})) {
      newCases[name] = mapToRows(vars);
    }
    setCases(newCases);
    const firstCase = Object.keys(newCases).sort()[0] ?? null;
    setEditingCaseName(firstCase);
    setNewCaseInput("");
    setSelectedCase("");
    setIsDirty(false);
    setSaveStatus("idle");
  }

  function buildRequestData(overrides?: Partial<RequestData>): RequestData {
    const casesMap: Record<string, Record<string, string>> = {};
    for (const [name, rows] of Object.entries(cases)) {
      casesMap[name] = toMap(rows);
    }
    const expectsActive =
      expectStatus.trim() || expectTimeMs.trim() ||
      activeRows(expectHeaders).length > 0 || activeRows(expectJson).length > 0;
    return {
      uid: reqUid, id: reqId, name: reqName || reqId, method, url,
      headers: toMap(reqHeaders), query: toMap(params),
      body_content: bodyKind !== "none" ? bodyContent : undefined,
      body_kind: bodyKind !== "none" ? bodyKind : undefined,
      notes: notes.trim() || undefined, tags: [], cases: casesMap,
      capture: toMap(captures),
      expect: expectsActive ? {
        status: expectStatus.trim() ? parseInt(expectStatus.trim(), 10) : undefined,
        time_ms: expectTimeMs.trim() || undefined,
        headers: toMap(expectHeaders),
        json: toMap(expectJson),
      } : undefined,
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

  async function loadRecentProjects() {
    try {
      const list = await invoke<RecentProject[]>("list_recent_projects");
      setRecentProjects(list);
    } catch { /* ignore */ }
  }

  useEffect(() => { loadRecentProjects(); }, []);

  function resetToAdhoc() {
    setSelectedFilePath(null);
    setReqUid(""); setReqId(""); setReqName("");
    setIsNewRequest(false);
    setNewReqDisplayName(""); setNewReqFolder("");
    setUrl(""); setParams([mkRow()]); setReqHeaders([mkRow()]); setCaptures([mkRow()]);
    setExpectStatus(""); setExpectTimeMs(""); setExpectHeaders([mkRow()]); setExpectJson([mkRow()]);
    setBodyKind("none"); setBodyContent(""); setNotes("");
    setResponse(null); setSavedMeta(null); setSketchYaml(null); setReqError(null);
    setIsDirty(false); setSaveStatus("idle");
    setCases({}); setEditingCaseName(null); setNewCaseInput(""); setSelectedCase("");
    setLastChecks([]); setLastCaptured({});
    setPlugins([]); setPluginResults({}); setPluginLoading({});
    setSpotCheckSummary(null); setMainPane('request');
  }

  // ── Project actions ───────────────────────────────────────────────────────

  async function openProject() {
    try {
      const data = await invoke<ProjectData>("open_project");
      const storedEnv = localStorage.getItem(`almanac.lastEnv.${data.id}`);
      const restoredEnvId = (storedEnv && data.environments.some(e => e.id === storedEnv)) ? storedEnv : null;
      setProject(data); setSelectedEnvId(restoredEnvId); resetToAdhoc();
      await reloadPlugins();
      await loadRecentProjects();
    } catch (e) {
      if (String(e) !== "cancelled") setReqError(`Failed to open project: ${e}`);
    }
  }

  async function newProject() {
    try {
      const data = await invoke<ProjectData>("create_project");
      setProject(data); setSelectedEnvId(null); resetToAdhoc();
      await reloadPlugins();
      await loadRecentProjects();
    } catch (e) {
      if (String(e) !== "cancelled") setReqError(`Failed to create project: ${e}`);
    }
  }

  async function openRecentProject(path: string) {
    try {
      const data = await invoke<ProjectData>("open_recent_project", { path });
      const storedEnv = localStorage.getItem(`almanac.lastEnv.${data.id}`);
      const restoredEnvId = (storedEnv && data.environments.some(e => e.id === storedEnv)) ? storedEnv : null;
      setProject(data); setSelectedEnvId(restoredEnvId); resetToAdhoc();
      await reloadPlugins();
      await loadRecentProjects();
    } catch (e) {
      setReqError(`Failed to open project: ${e}`);
    }
  }

  function addNewRequest() {
    setSelectedFilePath(null); setIsNewRequest(true);
    setNewReqDisplayName(""); setNewReqFolder("");
    setMethod("GET"); setUrl(""); setParams([mkRow()]); setReqHeaders([mkRow()]);
    setBodyKind("none"); setBodyContent(""); setNotes("");
    setResponse(null); setSavedMeta(null); setSketchYaml(null); setReqError(null);
    setIsDirty(false); setSaveStatus("idle");
    setCases({}); setEditingCaseName(null); setNewCaseInput(""); setSelectedCase("");
    setLastChecks([]); setLastCaptured({});
    setPluginResults({}); setPluginLoading({});
  }

  function addNewRequestToFolder(folder: string) {
    setSelectedFilePath(null); setIsNewRequest(true);
    setNewReqDisplayName(""); setNewReqFolder(folder);
    setMethod("GET"); setUrl(""); setParams([mkRow()]); setReqHeaders([mkRow()]);
    setBodyKind("none"); setBodyContent(""); setNotes("");
    setResponse(null); setSavedMeta(null); setSketchYaml(null); setReqError(null);
    setIsDirty(false); setSaveStatus("idle");
    setCases({}); setEditingCaseName(null); setNewCaseInput(""); setSelectedCase("");
    setLastChecks([]); setLastCaptured({});
    setPluginResults({}); setPluginLoading({});
  }

  // ── Group & request management ────────────────────────────────────────────

  async function createGroup(path: string) {
    if (!path.trim()) return;
    try {
      const result = await invoke<CreateGroupResult>("create_group", { label: path.trim() });
      setProject(result.project);
    } catch (e) { setReqError(String(e)); }
  }

  async function renameGroup(oldFolder: string, newLabel: string) {
    const label = newLabel.trim();
    if (!label) return;
    const selectedId = project?.requests.find((r) => r.file_path === selectedFilePath)?.id;
    try {
      const data = await invoke<ProjectData>("rename_group", { oldFolder, newLabel: label });
      setProject(data);
      if (selectedId) {
        const updated = data.requests.find((r) => r.id === selectedId);
        if (updated) setSelectedFilePath(updated.file_path);
      }
    } catch (e) { setReqError(String(e)); }
  }

  async function deleteGroup(folder: string) {
    const reqs = project?.requests.filter(
      (r) => r.folder === folder || r.folder.startsWith(folder + "/")
    ) ?? [];
    const msg =
      reqs.length > 0
        ? `Delete group "${folder}" and ${reqs.length} request${reqs.length !== 1 ? "s" : ""} inside?`
        : `Delete group "${folder}"?`;
    if (!window.confirm(msg)) return;
    const wasInsideGroup = selectedFilePath?.startsWith("requests/" + folder + "/") ?? false;
    try {
      const data = await invoke<ProjectData>("delete_group", { folder });
      setProject(data);
      if (wasInsideGroup) resetToAdhoc();
    } catch (e) { setReqError(String(e)); }
  }

  async function renameRequestName(filePath: string, newName: string) {
    if (!newName.trim()) return;
    try {
      const data = await invoke<ProjectData>("rename_request", { filePath, newName: newName.trim() });
      setProject(data);
      if (selectedFilePath === filePath) setReqName(newName.trim());
    } catch (e) { setReqError(String(e)); }
  }

  async function deleteProjectRequest(filePath: string) {
    const req = project?.requests.find((r) => r.file_path === filePath);
    const displayName = req?.name ?? filePath;
    if (!window.confirm(`Delete request "${displayName}"?`)) return;
    try {
      const data = await invoke<ProjectData>("delete_request", { filePath });
      setProject(data);
      if (selectedFilePath === filePath) resetToAdhoc();
    } catch (e) { setReqError(String(e)); }
  }

  async function moveProjectRequest(filePath: string, newFolder: string) {
    try {
      const result = await invoke<MoveResult>("move_request", { filePath, newFolder });
      setProject(result.project);
      if (selectedFilePath === filePath) setSelectedFilePath(result.new_file_path);
    } catch (e) { setReqError(String(e)); }
  }

  async function duplicateProjectRequest(filePath: string) {
    try {
      const result = await invoke<MoveResult>("duplicate_request", { filePath });
      setProject(result.project);
      selectRequest(result.new_file_path);
    } catch (e) { setReqError(String(e)); }
  }

  async function reorderRequest(filePath: string, newPosition: number) {
    try {
      const result = await invoke<ReorderResult>("reorder_request", { filePath, newPosition });
      setProject(result.project);
      if (selectedFilePath === filePath) {
        setSelectedFilePath(result.moved_path);
      } else {
        // Another sibling may have been renamed — re-derive from id
        const selectedId = project?.requests.find((r) => r.file_path === selectedFilePath)?.id;
        if (selectedId) {
          const updated = result.project.requests.find((r) => r.id === selectedId);
          if (updated) setSelectedFilePath(updated.file_path);
        }
      }
    } catch (e) { setReqError(String(e)); }
  }

  async function reorderGroup(folder: string, newPosition: number) {
    const selectedId = project?.requests.find((r) => r.file_path === selectedFilePath)?.id;
    try {
      const result = await invoke<ReorderResult>("reorder_group", { folder, newPosition });
      setProject(result.project);
      if (selectedId) {
        const updated = result.project.requests.find((r) => r.id === selectedId);
        if (updated) setSelectedFilePath(updated.file_path);
      }
    } catch (e) { setReqError(String(e)); }
  }

  async function selectRequest(filePath: string) {
    setMainPane('request');
    setResponse(null); setSavedMeta(null); setSketchYaml(null); setReqError(null); setIsNewRequest(false);
    setLastChecks([]); setLastCaptured({});
    setPluginResults({}); setPluginLoading({});
    try {
      const data = await invoke<RequestData>("get_request", { filePath });
      setSelectedFilePath(filePath);
      loadRequestData(data);
      try {
        const saved = await invoke<StoredResponse | null>("get_latest_response", { requestUid: data.uid });
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
      const freshData = await invoke<RequestData>("get_request", { filePath });
      setSelectedFilePath(filePath);
      setReqUid(freshData.uid);
      setReqId(freshData.id);
      setReqName(freshData.name);
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
    if (!reqUid || !sketchYaml) return;
    try {
      await invoke("save_sketch", { requestUid: reqUid, yaml: sketchYaml });
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
        recentProjects={recentProjects}
        onNewProject={newProject}
        onOpenProject={openProject}
        onOpenRecent={openRecentProject}
        onAddRequest={addNewRequest}
        onAddRequestToFolder={addNewRequestToFolder}
        onSelectRequest={selectRequest}
        onEnvChange={(envId) => {
          setSelectedEnvId(envId);
          setSpotCheckSummary(null);
          if (project) {
            if (envId) localStorage.setItem(`almanac.lastEnv.${project.id}`, envId);
            else localStorage.removeItem(`almanac.lastEnv.${project.id}`);
          }
        }}
        onRunChecks={() => setMainPane('checks')}
        onEditEnvs={() => setMainPane('environments')}
        onCreateGroup={createGroup}
        onRenameGroup={renameGroup}
        onDeleteGroup={deleteGroup}
        onRenameRequest={renameRequestName}
        onDeleteRequest={deleteProjectRequest}
        onMoveRequest={moveProjectRequest}
        onDuplicateRequest={duplicateProjectRequest}
        onReorderRequest={reorderRequest}
        onReorderGroup={reorderGroup}
      />

      <div className="main-area">
        {project && (
          <div className="main-area-tabs">
            <button
              className={`main-area-tab${mainPane === 'request' ? ' active' : ''}`}
              onClick={() => setMainPane('request')}
            >
              Request{isDirty ? ' ●' : ''}
            </button>
            <button
              className={`main-area-tab${mainPane === 'checks' ? ' active' : ''}`}
              onClick={() => setMainPane('checks')}
            >
              {spotCheckSummary
                ? `Checks · ${spotCheckSummary.passed}/${spotCheckSummary.passed + spotCheckSummary.failed + spotCheckSummary.errored}`
                : 'Checks'}
            </button>
            <button
              className={`main-area-tab${mainPane === 'environments' ? ' active' : ''}`}
              onClick={() => setMainPane('environments')}
            >
              Environments
            </button>
          </div>
        )}

        <div className="main-area-panel" style={{display: mainPane === 'environments' && !!project ? 'flex' : 'none'}}>
          {project && (
            <EnvironmentPanel
              key={project.id}
              selectedEnvId={selectedEnvId}
              onProjectChange={(newProject, newEnvId) => {
                setProject(newProject);
                if (newEnvId !== undefined) setSelectedEnvId(newEnvId);
              }}
            />
          )}
        </div>

        <div className="main-area-panel" style={{display: mainPane === 'checks' && !!project ? 'flex' : 'none'}}>
          {project && (
            <SpotCheckPanel
              key={`${project.id}-${selectedEnvId ?? 'none'}`}
              project={project}
              selectedEnvId={selectedEnvId}
              onRunComplete={(summary) => setSpotCheckSummary(summary)}
            />
          )}
        </div>

        <div className="main-area-panel" style={{display: mainPane === 'request' ? 'flex' : 'none'}}>
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
            {(["params","headers","body","cases","notes","captures","expects"] as RequestTab[]).map((t) => (
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
                {t === "cases" && caseNames.length > 0 && (
                  <span className="tab-count">{caseNames.length}</span>
                )}
                {t === "notes" && notes.trim() && (
                  <span className="notes-dot" title="Has notes" />
                )}
                {t === "captures" && activeRows(captures).length > 0 && (
                  <span className="tab-count">{activeRows(captures).length}</span>
                )}
                {t === "expects" && (() => {
                  const n = (expectStatus.trim() ? 1 : 0) + (expectTimeMs.trim() ? 1 : 0)
                    + activeRows(expectHeaders).length + activeRows(expectJson).length;
                  return n > 0 ? <span className="tab-count">{n}</span> : null;
                })()}
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
            {reqTab === "cases" && (
              <div className="cases-editor">
                <div className="case-list">
                  {caseNames.map((name) => (
                    <button
                      key={name}
                      className={`case-list-item${editingCaseName === name ? " active" : ""}`}
                      onClick={() => setEditingCaseName(name)}
                    >
                      {name}
                    </button>
                  ))}
                  <div className="case-add-row">
                    <input
                      className="case-add-input"
                      placeholder="New case name"
                      value={newCaseInput}
                      onChange={(e) => setNewCaseInput(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          const slug = newCaseInput.trim();
                          if (slug && !cases[slug]) {
                            setCases((prev) => ({ ...prev, [slug]: [mkRow()] }));
                            setEditingCaseName(slug);
                            setNewCaseInput("");
                            markDirty();
                          }
                        }
                      }}
                    />
                    <button
                      className="case-add-btn"
                      onClick={() => {
                        const slug = newCaseInput.trim();
                        if (slug && !cases[slug]) {
                          setCases((prev) => ({ ...prev, [slug]: [mkRow()] }));
                          setEditingCaseName(slug);
                          setNewCaseInput("");
                          markDirty();
                        }
                      }}
                    >
                      Add
                    </button>
                  </div>
                </div>
                <div className="case-vars">
                  {editingCaseName && cases[editingCaseName] ? (
                    <>
                      <div className="case-vars-header">
                        <span className="case-vars-title">{editingCaseName}</span>
                        <button
                          className="case-delete-btn"
                          title="Delete this case"
                          onClick={() => {
                            setCases((prev) => {
                              const next = { ...prev };
                              delete next[editingCaseName];
                              return next;
                            });
                            const remaining = caseNames.filter((n) => n !== editingCaseName);
                            setEditingCaseName(remaining[0] ?? null);
                            markDirty();
                          }}
                        >
                          Delete case
                        </button>
                      </div>
                      <KvEditor
                        rows={cases[editingCaseName]}
                        onChange={(rows) => {
                          setCases((prev) => ({ ...prev, [editingCaseName]: rows }));
                          markDirty();
                        }}
                        keyPlaceholder="Variable"
                        valuePlaceholder="Value"
                      />
                    </>
                  ) : (
                    <p className="cases-empty">
                      {caseNames.length === 0
                        ? "No cases yet. Add one to define named variations of this request."
                        : "Select a case to edit its variables."}
                    </p>
                  )}
                </div>
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
            {reqTab === "captures" && (
              <div className="captures-editor">
                <CapturesEditor
                  rows={captures}
                  onChange={(v) => { setCaptures(v); if (isProjectMode) markDirty(); }}
                  liveValues={lastCaptured}
                />
                <p className="captures-hint">Supported paths: <code>json.field</code> · <code>json.nested.array[0]</code> · <code>headers.x-header-name</code></p>
              </div>
            )}
            {reqTab === "expects" && (
              <ExpectationsEditor
                status={expectStatus}
                onStatusChange={(v) => { setExpectStatus(v); if (isProjectMode) markDirty(); }}
                timeMs={expectTimeMs}
                onTimeMsChange={(v) => { setExpectTimeMs(v); if (isProjectMode) markDirty(); }}
                headers={expectHeaders}
                onHeadersChange={(v) => { setExpectHeaders(v); if (isProjectMode) markDirty(); }}
                json={expectJson}
                onJsonChange={(v) => { setExpectJson(v); if (isProjectMode) markDirty(); }}
              />
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
        </div>
      </div>
    </div>
  );
}
