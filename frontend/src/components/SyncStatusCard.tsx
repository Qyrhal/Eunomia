"use client";

import { RefreshCw } from "lucide-react";
import type { SourceRow } from "@/lib/api";

function relativeTime(iso: string | null): string {
  if (!iso) return "Never synced";
  const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (mins < 1) return "Synced just now";
  if (mins < 60) return `Synced ${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `Synced ${hours}h ago`;
  return `Synced ${Math.round(hours / 24)}d ago`;
}

function healthColor(failures: number, lastOk: string | null): string {
  if (failures === 0) return lastOk ? "var(--good)" : "var(--ink-faint)";
  if (failures <= 2) return "var(--warning)";
  return "var(--critical)";
}

/** One source's sync health: status, record count, last error, and a sync button. */
export default function SyncStatusCard({ source, onSync, syncing }: { source: SourceRow; onSync: () => void; syncing: boolean }) {
  const { sync_status: status } = source;
  return (
    <div className="ledger p-4 flex flex-col gap-3">
      <div className="flex items-center gap-2 min-w-0">
        <span className="dot" style={{ background: healthColor(status.consecutive_failures, status.last_ok) }} aria-hidden />
        <span className="text-[13px] font-medium truncate">{source.label}</span>
        <span className="label font-mono ml-auto shrink-0">{source.key}</span>
      </div>
      <dl className="grid grid-cols-2 gap-3">
        <div className="flex flex-col gap-1">
          <dt className="label">Records</dt>
          <dd className="font-mono text-[15px]">{source.record_count.toLocaleString()}</dd>
        </div>
        <div className="flex flex-col gap-1 min-w-0">
          <dt className="label">Last sync</dt>
          <dd className="text-[12.5px] truncate" style={{ color: "var(--ink-dim)" }}>
            {relativeTime(status.last_ok)}
          </dd>
        </div>
      </dl>
      <div className="text-[12px] font-mono truncate" style={{ color: "var(--ink-faint)" }}>
        {source.record_types.join(", ")}
      </div>
      {(status.consecutive_failures > 0 || status.last_error) && (
        <div className="rounded-md px-2.5 py-2 text-[12px] flex flex-col gap-0.5" style={{ background: "var(--critical-soft)", color: "var(--critical)" }}>
          {status.consecutive_failures > 0 && (
            <span>
              {status.consecutive_failures} failure{status.consecutive_failures === 1 ? "" : "s"} in a row. Check the credentials, then sync again.
            </span>
          )}
          {status.last_error && <span className="font-mono truncate">{status.last_error}</span>}
        </div>
      )}
      <button onClick={onSync} disabled={syncing} className="btn btn-sm self-start">
        <RefreshCw size={12} strokeWidth={1.75} className={syncing ? "animate-spin" : undefined} />
        {syncing ? "Syncing…" : "Sync now"}
      </button>
    </div>
  );
}
