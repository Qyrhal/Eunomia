"use client";

import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import Link from "next/link";
import { AlertTriangle, ArrowRight, Check, Copy, RefreshCw } from "lucide-react";
import { auth, entities, sources, type EntityKind, type EntityMemory, type SourceRow } from "@/lib/api";
import AuthorTag from "@/components/AuthorTag";

// Same origin as the app: the frontend proxies /mcp to the backend.
const noSubscribe = () => () => {};
function useMcpUrl(): string {
  return useSyncExternalStore(noSubscribe, () => `${window.location.origin}/mcp`, () => "");
}

function claudeCodeCommand(url: string, token: string) {
  return `claude mcp add --transport http eunomia ${url} --header "Authorization: Bearer ${token}"`;
}

function mcpConfig(url: string, token: string) {
  return JSON.stringify(
    { mcpServers: { eunomia: { type: "http", url, headers: { Authorization: `Bearer ${token}` } } } },
    null,
    2
  );
}

/** Compact age: "now", "4m", "3h", "2d". */
function age(iso: string | null | undefined): string {
  if (!iso) return "–";
  const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.round(hours / 24)}d`;
}

function since(iso: string | null): string {
  if (!iso) return "never";
  const a = age(iso);
  return a === "now" ? "just now" : `${a} ago`;
}

function stamp(iso: string | null | undefined): string {
  if (!iso) return "–";
  return new Date(iso).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

// ---- MCP connect card ----

function CopyField({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    await navigator.clipboard.writeText(value).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }
  return (
    <div className="field flex items-center gap-2 h-8 pl-2.5 pr-1">
      <code className="flex-1 min-w-0 truncate text-[12px] font-mono" style={{ color: "var(--ink-dim)" }}>
        {value}
      </code>
      <button onClick={copy} aria-label="Copy" className="btn btn-ghost btn-sm shrink-0" style={{ width: 24, height: 24, padding: 0 }}>
        {copied ? <Check size={13} strokeWidth={1.75} color="var(--good)" /> : <Copy size={13} strokeWidth={1.75} />}
      </button>
    </div>
  );
}

function McpCard() {
  const mcpUrl = useMcpUrl();
  const [token, setToken] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function generate() {
    setBusy(true);
    setError(null);
    try {
      const res = await auth.tokens.create("MCP");
      setToken(res.token);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not generate a token.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section id="connect" className="ledger p-4 flex flex-col gap-3.5 scroll-mt-6" aria-labelledby="connect-title">
      <div className="flex flex-col gap-1">
        <h2 id="connect-title" className="section-title">Connect an MCP client</h2>
        <p className="text-[12.5px] leading-relaxed" style={{ color: "var(--ink-dim)" }}>
          MCP and the web app share the same tools. Anything you can do here, a connected agent can do too.
        </p>
      </div>

      <label className="label flex flex-col gap-1.5">
        MCP URL
        <CopyField value={mcpUrl} />
      </label>

      {!token ? (
        <button onClick={generate} disabled={busy} className="btn btn-primary self-start">
          {busy ? "Generating…" : "Generate a token"}
        </button>
      ) : (
        <>
          <label className="label flex flex-col gap-1.5">
            Personal API token, shown once: store it now
            <CopyField value={token} />
          </label>
          <label className="label flex flex-col gap-1.5">
            Claude Code
            <CopyField value={claudeCodeCommand(mcpUrl, token)} />
          </label>
          <label className="label flex flex-col gap-1.5">
            Other MCP clients (Streamable HTTP)
            <div className="field px-2.5 py-2">
              <pre className="text-[11.5px] font-mono whitespace-pre-wrap break-all" style={{ color: "var(--ink-dim)" }}>
                {mcpConfig(mcpUrl, token)}
              </pre>
            </div>
          </label>
        </>
      )}

      {error && (
        <p className="text-[12.5px] rounded-md px-2.5 py-2" style={{ color: "var(--critical)", background: "var(--critical-soft)" }}>
          {error} Try again, or mint a token in Settings.
        </p>
      )}

      <div className="flex flex-col gap-1.5 pt-3.5" style={{ borderTop: "var(--hair) solid var(--border)" }}>
        <p className="text-[12px] leading-relaxed" style={{ color: "var(--ink-faint)" }}>
          Or connect every agent on this machine at once (Claude Code, Codex, Hermes, Cursor, …) from the install folder.{" "}
          <Link href="/docs" className="underline" style={{ color: "var(--accent-text)" }}>
            Read the docs
          </Link>
        </p>
        <CopyField value="./scripts/connect-agents.sh --email you@… --password …" />
      </div>
    </section>
  );
}

// ---- failure banner ----

// A source that's been failing this many syncs in a row has moved past
// transient backoff (the scheduler's backoff table reaches a full hour by
// index 2) into "this needs attention".
const FAILURE_ALERT_THRESHOLD = 3;

function FailureBanner({ rows, dismissed, onDismiss }: { rows: SourceRow[]; dismissed: Set<string>; onDismiss: (key: string) => void }) {
  const failing = rows.filter((r) => r.connected && r.sync_status.consecutive_failures >= FAILURE_ALERT_THRESHOLD && !dismissed.has(r.key));
  if (failing.length === 0) return null;
  return (
    <section className="flex flex-col gap-2">
      {failing.map((s) => (
        <div
          key={s.key}
          role="alert"
          className="flex items-center gap-3 rounded-[10px] px-3.5 py-2.5"
          style={{ background: "var(--critical-soft)", border: "var(--hair) solid var(--critical)" }}
        >
          <AlertTriangle size={15} strokeWidth={1.75} color="var(--critical)" className="shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-[13px] font-medium">
              {s.label} has failed {s.sync_status.consecutive_failures} syncs in a row
            </div>
            {s.sync_status.last_error && (
              <div className="text-[11.5px] font-mono truncate" style={{ color: "var(--ink-dim)" }}>
                {s.sync_status.last_error}
              </div>
            )}
          </div>
          <Link href="/connectors" className="btn btn-sm shrink-0">
            Review
          </Link>
          <button onClick={() => onDismiss(s.key)} className="btn btn-ghost btn-sm shrink-0">
            Dismiss
          </button>
        </div>
      ))}
    </section>
  );
}

// ---- live memory ----
// The entity lives in the inspector, not a table column: graph-palette.spec
// asserts entity names are absent from the page behind the palette.

// The API returns more on each memory than lib/api.ts declares; read what is there.
type Memory = EntityMemory & { type?: string; updated_at?: string };
type LiveRow = Memory & { entityName: string; entityKind: EntityKind };

// ponytail: no "recent memories" endpoint exists, so this fans out one
// entities.get per entity on the first page. Ceiling: only memories on the
// first ENTITY_CAP entities show. Upgrade path: a backend
// `GET /api/memories?sort=-created_at&limit=n` endpoint.
const ENTITY_CAP = 24;
const ROW_LIMIT = 12;

async function loadLiveMemory(): Promise<LiveRow[]> {
  const list = await entities.list();
  const details = await Promise.all(list.results.slice(0, ENTITY_CAP).map((e) => entities.get(e.id).catch(() => null)));
  return details
    .flatMap((d) => (d ? (d.memory as Memory[]).map((m) => ({ ...m, entityName: d.name, entityKind: d.kind })) : []))
    .sort((a, b) => (b.created_at ?? "").localeCompare(a.created_at ?? ""))
    .slice(0, ROW_LIMIT);
}

const authorOf = (m: Memory) => m.owner_email || m.source || null;

function LiveMemory({ rows, selectedId, onSelect }: { rows: LiveRow[] | null; selectedId: string | null; onSelect: (id: string) => void }) {
  return (
    <section className="ledger" aria-labelledby="live-memory-title">
      <div className="flex items-center justify-between gap-3 h-11 px-3" style={{ borderBottom: "var(--hair) solid var(--border)" }}>
        <h2 id="live-memory-title" className="section-title">
          Live memory
        </h2>
        {rows && rows.length > 0 && <span className="label">Latest {rows.length}</span>}
      </div>
      {rows === null ? (
        <div aria-busy="true" aria-label="Loading memories">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="flex items-center gap-4 h-10 px-3" style={{ borderTop: i ? "var(--hair) solid var(--border)" : undefined }}>
              <div className="skeleton h-5 w-16" />
              <div className="skeleton h-3.5 flex-1 max-w-[340px]" />
              <div className="skeleton h-3.5 w-8 ml-auto" />
            </div>
          ))}
        </div>
      ) : rows.length === 0 ? (
        <div className="px-3 py-8 text-[13px] flex flex-wrap items-center gap-x-3 gap-y-2" style={{ color: "var(--ink-dim)" }}>
          No memories yet. Connect an agent and what it learns lands here, tagged with who wrote it.
          <a href="#connect" className="btn btn-sm">
            Connect an agent <ArrowRight size={12} strokeWidth={1.75} />
          </a>
        </div>
      ) : (
        <table className="data-table" style={{ tableLayout: "fixed" }}>
          <thead>
            <tr>
              <th style={{ width: 112 }}>Author</th>
              <th>Memory</th>
              <th className="hidden md:table-cell" style={{ width: 160 }}>Entity</th>
              <th className="text-right" style={{ width: 56 }}>
                Age
              </th>
            </tr>
          </thead>
          <tbody
            onKeyDown={(e) => {
              // Arrow keys walk the selection; no animation on keyboard moves.
              if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
              const i = rows.findIndex((r) => r.id === selectedId);
              const next = rows[Math.min(rows.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)))];
              if (!next) return;
              e.preventDefault();
              onSelect(next.id);
              (e.currentTarget.querySelector(`[data-memory-id="${next.id}"]`) as HTMLElement | null)?.focus();
            }}
          >
            {rows.map((m) => {
              const selected = m.id === selectedId;
              const author = authorOf(m);
              return (
                <tr
                  key={m.id}
                  onClick={() => onSelect(m.id)}
                  // A row outline, not .frame-selected: its ::after would become an extra table cell.
                  style={selected ? { background: "var(--accent-soft)", outline: "1px solid var(--accent)", outlineOffset: -1 } : undefined}
                >
                  <td className="overflow-hidden">{author && <AuthorTag name={author} />}</td>
                  <td className="overflow-hidden">
                    <button
                      type="button"
                      aria-pressed={selected}
                      data-memory-id={m.id}
                      onClick={(e) => {
                        e.stopPropagation();
                        onSelect(m.id);
                      }}
                      className="block w-full truncate text-left"
                      style={{ color: "var(--ink)" }}
                      title={m.text}
                    >
                      {m.text}
                    </button>
                  </td>
                  <td className="hidden md:table-cell overflow-hidden">
                    <span className="flex items-center gap-1.5 min-w-0 text-[12.5px]" style={{ color: "var(--ink-dim)" }}>
                      <span className="dot" style={{ background: `var(--kind-${m.entityKind})` }} aria-hidden />
                      <span className="truncate">{m.entityName}</span>
                    </span>
                  </td>
                  <td className="text-right font-mono text-[12px]" style={{ color: "var(--ink-faint)" }}>
                    <time dateTime={m.created_at}>{age(m.created_at)}</time>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </section>
  );
}

function Inspector({ row }: { row: LiveRow }) {
  const author = authorOf(row);
  const mono = (v: string) => <span className="font-mono text-[12px]">{v}</span>;
  const updated = row.updated_at && row.updated_at.slice(0, 16) !== row.created_at?.slice(0, 16) ? row.updated_at : null;
  const fields: { label: string; value: React.ReactNode; show?: boolean }[] = [
    { label: "Author", value: author ? <AuthorTag name={author} /> : <span style={{ color: "var(--ink-faint)" }}>Not recorded</span> },
    { label: "Entity", value: <span className="truncate">{row.entityName}</span> },
    { label: "Kind", value: <span className="capitalize">{row.entityKind}</span> },
    { label: "Type", value: <span className="capitalize">{row.type}</span>, show: Boolean(row.type) },
    { label: "Written", value: mono(stamp(row.created_at)) },
    { label: "Updated", value: mono(stamp(updated)), show: Boolean(updated) },
    { label: "Source", value: mono(row.source ?? ""), show: Boolean(row.source && row.source !== author) },
  ];

  return (
    <aside aria-label="Selected memory" className="panel frame-selected p-4 flex flex-col gap-4">
      {/* Re-keyed per row so the content settles in with a short fade, the panel itself stays put. */}
      <div key={row.id} className="fade-in flex flex-col gap-4">
        <div className="flex flex-col gap-1.5">
          <span className="label">Memory</span>
          <p className="text-[14px] leading-snug" style={{ color: "var(--ink)" }}>
            {row.text}
          </p>
        </div>
        <dl className="grid gap-y-2.5 gap-x-3 text-[13px]" style={{ gridTemplateColumns: "72px minmax(0, 1fr)" }}>
          {fields.filter((f) => f.show !== false).map(({ label, value }) => (
            <div key={label} className="contents">
              <dt className="label self-center">{label}</dt>
              <dd className="min-w-0 flex items-center">{value}</dd>
            </div>
          ))}
        </dl>
      </div>
      <Link href="/entities" className="btn btn-sm self-start">
        Open entities <ArrowRight size={12} strokeWidth={1.75} />
      </Link>
    </aside>
  );
}

// ---- stats + sources ----

function Stat({ label, value, href, children }: { label: string; value: string; href?: string; children?: React.ReactNode }) {
  const body = (
    <>
      <span className="label">{label}</span>
      <span className={`flex items-center gap-2 text-[18px] leading-none ${/\d/.test(value) ? "font-mono tracking-tight" : "font-medium"}`} style={{ color: "var(--ink)" }}>
        {children}
        {value}
      </span>
    </>
  );
  const cls = "flex flex-col gap-2.5 px-3.5 py-3 min-w-0 bg-[var(--surface)]";
  return href ? (
    <Link href={href} className={`${cls} transition-[background-color,transform] duration-150 ease-[var(--ease-out)] hover:bg-[var(--surface-raised)] active:scale-[0.98]`}>
      {body}
    </Link>
  ) : (
    <div className={cls}>{body}</div>
  );
}

// The full catalogue lives on /connectors; the dashboard shows a short list.
const NOT_CONNECTED_SHOWN = 6;

function healthColor(failures: number, lastOk: string | null): string {
  if (failures === 0) return lastOk ? "var(--good)" : "var(--ink-faint)";
  return failures < FAILURE_ALERT_THRESHOLD ? "var(--warning)" : "var(--critical)";
}

export default function DashboardPage() {
  const [rows, setRows] = useState<SourceRow[] | null>(null);
  const [entityCount, setEntityCount] = useState<number | null>(null);
  const [live, setLive] = useState<LiveRow[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [syncingKey, setSyncingKey] = useState<string | null>(null);
  const [dismissedFailures, setDismissedFailures] = useState<Set<string>>(new Set());

  const load = useCallback(() => sources.list().then(setRows).catch(() => setRows([])), []);
  useEffect(() => {
    load();
    entities
      .list()
      .then((res) => setEntityCount(res.total))
      .catch(() => setEntityCount(0));
    const refresh = () => loadLiveMemory().then(setLive).catch(() => setLive((prev) => prev ?? []));
    refresh();
    // Live: rows refresh in place on a fixed-layout table, nothing jumps.
    const timer = setInterval(refresh, 60_000);
    return () => clearInterval(timer);
  }, [load]);

  async function sync(key: string) {
    setSyncingKey(key);
    try {
      await sources.sync(key);
      await load();
    } finally {
      setSyncingKey(null);
    }
  }

  const connected = rows?.filter((r) => r.connected) ?? [];
  const disconnected = rows?.filter((r) => !r.connected) ?? [];
  const healthy = connected.filter((r) => r.sync_status.consecutive_failures === 0 && r.sync_status.last_ok).length;
  const totalRecords = rows?.reduce((sum, r) => sum + r.record_count, 0) ?? 0;
  const lastSyncs = connected.map((r) => r.sync_status.last_ok).filter((d): d is string => Boolean(d));
  const lastSync = lastSyncs.length ? lastSyncs.sort().at(-1)! : null;
  const selected = live?.find((m) => m.id === selectedId) ?? live?.[0] ?? null;

  return (
    <div className="flex flex-col gap-6 max-w-[1240px]">
      <h1 className="page-title">Dashboard</h1>

      <section
        aria-label="Summary"
        className="grid grid-cols-2 lg:grid-cols-4 overflow-hidden rounded-[10px]"
        style={{ gap: "var(--hair)", background: "var(--border)", border: "var(--hair) solid var(--border)" }}
      >
        <Stat label="Entities" value={entityCount !== null ? entityCount.toLocaleString() : "–"} href="/entities" />
        <Stat label="Records" value={rows ? totalRecords.toLocaleString() : "–"} />
        <Stat label="Sources healthy" value={!rows ? "–" : connected.length ? `${healthy}/${connected.length}` : "None"}>
          {rows && connected.length > 0 && (
            <span className="dot" aria-hidden style={{ background: healthy === connected.length ? "var(--good)" : "var(--warning)" }} />
          )}
        </Stat>
        <Stat label="Last sync" value={lastSync ? since(lastSync) : "Never"} />
      </section>

      {rows && (
        <FailureBanner
          rows={rows}
          dismissed={dismissedFailures}
          onDismiss={(key) => setDismissedFailures((prev) => new Set(prev).add(key))}
        />
      )}

      {/* Mobile order: live memory, inspector + connect, then sources. */}
      <div className="grid gap-x-6 gap-y-8 lg:grid-cols-[minmax(0,1fr)_320px] items-start">
        <div className="min-w-0 lg:col-start-1 lg:row-start-1">
          <LiveMemory rows={live} selectedId={selected?.id ?? null} onSelect={setSelectedId} />
        </div>

        <div className="flex flex-col gap-6 min-w-0 lg:col-start-2 lg:row-start-1 lg:row-span-2 lg:sticky lg:top-8">
          {selected && <Inspector row={selected} />}
          <McpCard />
        </div>

        <div className="flex flex-col gap-8 min-w-0 lg:col-start-1 lg:row-start-2">
          <section className="flex flex-col gap-3" aria-labelledby="whats-next">
            <h2 id="whats-next" className="section-title">
              What&apos;s next
            </h2>
            {rows === null ? (
              <div className="ledger p-3 flex flex-col gap-3" aria-busy="true">
                <div className="skeleton h-4 w-1/2" />
                <div className="skeleton h-4 w-1/3" />
              </div>
            ) : connected.length === 0 ? (
              <p className="text-[13px]" style={{ color: "var(--ink-dim)" }}>
                Nothing connected yet. Pick a source under &ldquo;what&apos;s not&rdquo; below.
              </p>
            ) : (
              <div className="ledger overflow-x-auto">
                <table className="data-table" style={{ minWidth: 520 }}>
                  <thead>
                    <tr>
                      <th>Source</th>
                      <th className="text-right">Records</th>
                      <th>Last sync</th>
                      <th style={{ width: 1 }}>
                        <span className="sr-only">Actions</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {connected.map((s) => {
                      const st = s.sync_status;
                      const busy = syncingKey === s.key;
                      return (
                        <tr key={s.key}>
                          <td>
                            <div className="flex items-center gap-2 min-w-0">
                              <span className="dot" aria-hidden style={{ background: healthColor(st.consecutive_failures, st.last_ok) }} />
                              <span className="font-medium truncate">{s.label}</span>
                              <AuthorTag name={s.key} />
                            </div>
                          </td>
                          <td className="text-right font-mono text-[12.5px]">{s.record_count.toLocaleString()}</td>
                          <td className="text-[12.5px] whitespace-nowrap">
                            <span className="font-mono" style={{ color: "var(--ink-dim)" }}>
                              {since(st.last_ok)}
                            </span>
                            {st.consecutive_failures > 0 && <span style={{ color: "var(--critical)" }}> · {st.consecutive_failures} failed</span>}
                          </td>
                          <td className="text-right">
                            <button onClick={() => sync(s.key)} disabled={busy} className="btn btn-sm">
                              <RefreshCw size={12} strokeWidth={1.75} className={busy ? "animate-spin" : undefined} />
                              {busy ? "Syncing…" : "Sync now"}
                            </button>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          {disconnected.length > 0 && (
            <section className="flex flex-col gap-3" aria-labelledby="whats-not">
              <h2 id="whats-not" className="section-title">
                What&apos;s not
              </h2>
              <ul className="ledger hairline-rows">
                {disconnected.slice(0, NOT_CONNECTED_SHOWN).map((s) => (
                  <li key={s.key} className="flex items-center gap-3 min-h-11 px-3 py-2">
                    <span className="dot" aria-hidden style={{ background: "var(--border-strong)" }} />
                    <div className="flex-1 min-w-0 flex flex-col sm:flex-row sm:items-center gap-x-3 gap-y-0.5">
                      <span className="text-[13px] font-medium truncate">{s.label}</span>
                      <span className="text-[12px] font-mono truncate" style={{ color: "var(--ink-faint)" }}>
                        {s.record_types.join(", ")}
                      </span>
                    </div>
                    <Link href="/connectors" className="btn btn-sm shrink-0">
                      Connect <ArrowRight size={12} strokeWidth={1.75} />
                    </Link>
                  </li>
                ))}
                {disconnected.length > NOT_CONNECTED_SHOWN && (
                  <li className="flex items-center min-h-11 px-3">
                    <Link href="/connectors" className="text-[13px] inline-flex items-center gap-1.5 underline-offset-2 hover:underline" style={{ color: "var(--accent-text)" }}>
                      See all {disconnected.length} in Connectors <ArrowRight size={12} strokeWidth={1.75} />
                    </Link>
                  </li>
                )}
              </ul>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
