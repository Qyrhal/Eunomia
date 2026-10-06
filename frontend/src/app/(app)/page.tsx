"use client";

import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import Link from "next/link";
import { AlertTriangle, ArrowRight, Check, Copy, Plug } from "lucide-react";
import { auth, entities, sources, type SourceRow } from "@/lib/api";
import SyncStatusCard from "@/components/SyncStatusCard";
import StatRing from "@/components/StatRing";

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

function CopyField({ value, mono = true }: { value: string; mono?: boolean }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    await navigator.clipboard.writeText(value).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }
  return (
    <div className="field flex items-center gap-2 px-3 py-2">
      <code className={`flex-1 min-w-0 truncate text-[12px] ${mono ? "font-mono" : ""}`} style={{ color: "var(--ink-dim)" }}>
        {value}
      </code>
      <button onClick={copy} aria-label="Copy" className="shrink-0" style={{ color: "var(--ink-faint)" }}>
        {copied ? <Check size={13} color="var(--good)" /> : <Copy size={13} />}
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
    <section className="ledger p-6 flex flex-col gap-4">
      <div>
        <div className="eyebrow mb-1">Connect an MCP client</div>
        <p className="text-[13px]" style={{ color: "var(--ink-dim)" }}>
          MCP and the web app share the same tools — anything you can do here, an agent connected below can
          do too.
        </p>
      </div>

      <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
        MCP URL
        <CopyField value={mcpUrl} />
      </label>

      {!token ? (
        <button
          onClick={generate}
          disabled={busy}
          className="self-start px-4 py-2 text-[13px] font-medium rounded-xl disabled:opacity-50"
          style={{ background: "var(--felt)", color: "var(--canvas)" }}
        >
          {busy ? "Generating…" : "Generate a token"}
        </button>
      ) : (
        <>
          <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
            Personal API token — shown once, store it now
            <CopyField value={token} />
          </label>
          <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
            Claude Code
            <CopyField value={claudeCodeCommand(mcpUrl, token)} />
          </label>
          <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
            Other MCP clients (Streamable HTTP)
            <div className="field px-3 py-2">
              <pre className="text-[11.5px] font-mono whitespace-pre-wrap break-all" style={{ color: "var(--ink-dim)" }}>
                {mcpConfig(mcpUrl, token)}
              </pre>
            </div>
          </label>
        </>
      )}

      {error && (
        <p className="text-[12.5px]" style={{ color: "var(--critical)" }}>
          {error}
        </p>
      )}
    </section>
  );
}

function relativeTime(iso: string | null): string {
  if (!iso) return "never synced";
  const ms = Date.now() - new Date(iso).getTime();
  const mins = Math.round(ms / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  return `${days}d ago`;
}

function StatTile({ label, value, href }: { label: string; value: string; href?: string }) {
  const content = (
    <>
      <div className="eyebrow">{label}</div>
      <div className="font-display text-2xl" style={{ color: "var(--ink)" }}>
        {value}
      </div>
    </>
  );
  if (href) {
    return (
      <Link href={href} className="ledger p-5 flex flex-col justify-center gap-1.5">
        {content}
      </Link>
    );
  }
  return <div className="ledger p-5 flex flex-col justify-center gap-1.5">{content}</div>;
}

// A source that's been failing this many syncs in a row has moved past
// transient backoff (see `sources/scheduler.py`'s `_BACKOFF` table, where
// index 2 is already a full hour) into "this needs attention".
const FAILURE_ALERT_THRESHOLD = 3;

function FailureBanner({ rows, dismissed, onDismiss }: { rows: SourceRow[]; dismissed: Set<string>; onDismiss: (key: string) => void }) {
  const failing = rows.filter((r) => r.connected && r.sync_status.consecutive_failures >= FAILURE_ALERT_THRESHOLD && !dismissed.has(r.key));
  if (failing.length === 0) return null;
  return (
    <section className="flex flex-col gap-2.5">
      {failing.map((s) => (
        <div
          key={s.key}
          className="ledger p-4 flex items-center gap-3"
          style={{ borderColor: "var(--critical)" }}
        >
          <AlertTriangle size={16} color="var(--critical)" className="shrink-0" />
          <div className="flex-1">
            <div className="text-[13px] font-medium">
              {s.label} has failed {s.sync_status.consecutive_failures} syncs in a row
            </div>
            {s.sync_status.last_error && (
              <div className="text-[11.5px] font-mono truncate" style={{ color: "var(--ink-faint)" }}>
                {s.sync_status.last_error}
              </div>
            )}
          </div>
          <Link href="/connectors" className="field px-3 py-1.5 text-[12px] shrink-0" style={{ color: "var(--ink)" }}>
            Review
          </Link>
          <button onClick={() => onDismiss(s.key)} className="text-[12px] shrink-0" style={{ color: "var(--ink-faint)" }}>
            Dismiss
          </button>
        </div>
      ))}
    </section>
  );
}

export default function DashboardPage() {
  const [rows, setRows] = useState<SourceRow[] | null>(null);
  const [entityCount, setEntityCount] = useState<number | null>(null);
  const [syncingKey, setSyncingKey] = useState<string | null>(null);
  const [dismissedFailures, setDismissedFailures] = useState<Set<string>>(new Set());

  const load = useCallback(() => sources.list().then(setRows).catch(() => setRows([])), []);
  useEffect(() => {
    load();
    entities
      .list()
      .then((res) => setEntityCount(res.total))
      .catch(() => setEntityCount(0));
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
  const total = rows?.length ?? 0;
  const totalRecords = rows?.reduce((sum, r) => sum + r.record_count, 0) ?? 0;
  const lastSyncs = connected.map((r) => r.sync_status.last_ok).filter((d): d is string => Boolean(d));
  const lastSync = lastSyncs.length ? lastSyncs.sort().at(-1)! : null;

  return (
    <div className="flex flex-col gap-10 max-w-5xl">
      <section className="grid gap-5 sm:[grid-template-columns:auto_1fr]">
        <div className="ledger flex items-center justify-center p-6">
          <StatRing
            size={132}
            strokeWidth={11}
            value={healthy}
            max={total}
            color="var(--felt)"
            valueLabel={rows ? `${healthy}/${total}` : "–"}
            label="sources healthy"
            ariaLabel={`${healthy} of ${total} sources syncing cleanly`}
          />
        </div>
        <div className="grid sm:grid-cols-3 gap-4">
          <StatTile label="Total records" value={rows ? totalRecords.toLocaleString() : "–"} />
          <StatTile
            label="Entities tracked"
            value={entityCount !== null ? entityCount.toLocaleString() : "–"}
            href="/entities"
          />
          <StatTile label="Last sync" value={relativeTime(lastSync)} />
        </div>
      </section>

      {rows && (
        <FailureBanner
          rows={rows}
          dismissed={dismissedFailures}
          onDismiss={(key) => setDismissedFailures((prev) => new Set(prev).add(key))}
        />
      )}

      <McpCard />

      <section className="flex flex-col gap-4">
        <div className="eyebrow">What&apos;s next</div>
        {rows === null && (
          <p className="text-[13px]" style={{ color: "var(--ink-faint)" }}>
            Loading…
          </p>
        )}
        {rows !== null && connected.length === 0 && (
          <p className="text-[13px]" style={{ color: "var(--ink-faint)" }}>
            Nothing connected yet — see &ldquo;what&apos;s not&rdquo; below.
          </p>
        )}
        <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {connected.map((s) => (
            <SyncStatusCard key={s.key} source={s} onSync={() => sync(s.key)} syncing={syncingKey === s.key} />
          ))}
        </div>
      </section>

      {disconnected.length > 0 && (
        <section className="flex flex-col gap-4">
          <div className="eyebrow">What&apos;s not</div>
          <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {disconnected.map((s) => (
              <div key={s.key} className="ledger p-5 flex flex-col gap-4">
                <div className="flex items-center gap-2.5">
                  <div className="w-8 h-8 rounded-lg flex items-center justify-center" style={{ background: "var(--surface-raised)", color: "var(--ink-dim)" }}>
                    <Plug size={14} />
                  </div>
                  <div className="text-[13.5px] font-medium">{s.label}</div>
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {s.record_types.map((rt) => (
                    <span key={rt} className="pill">
                      {rt}
                    </span>
                  ))}
                </div>
                <Link href="/connectors" className="self-start field px-3 py-1.5 text-[12px] flex items-center gap-1.5" style={{ color: "var(--ink)" }}>
                  Connect <ArrowRight size={12} />
                </Link>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
