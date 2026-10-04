"use client";

import type { SourceRow } from "@/lib/api";

function relativeTime(iso: string | null): string {
  if (!iso) return "never synced";
  const ms = Date.now() - new Date(iso).getTime();
  const mins = Math.round(ms / 60000);
  if (mins < 1) return "synced just now";
  if (mins < 60) return `synced ${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `synced ${hours}h ago`;
  const days = Math.round(hours / 24);
  return `synced ${days}d ago`;
}

function healthColor(failures: number): string {
  if (failures === 0) return "var(--good)";
  if (failures <= 2) return "var(--warning)";
  return "var(--critical)";
}

export default function SyncStatusCard({ source, onSync, syncing }: { source: SourceRow; onSync: () => void; syncing: boolean }) {
  const { sync_status: status } = source;
  return (
    <div className="ledger p-5 flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: healthColor(status.consecutive_failures) }} aria-hidden />
          <span className="text-[13.5px] font-medium">{source.label}</span>
        </div>
        <span className="eyebrow">{source.key}</span>
      </div>
      <div className="text-[12px]" style={{ color: "var(--ink-dim)" }}>
        {source.record_types.join(" · ")}
      </div>
      <div className="text-[11.5px] font-mono" style={{ color: "var(--ink-faint)" }}>
        {relativeTime(status.last_ok)}
        {status.consecutive_failures > 0 && (
          <span style={{ color: "var(--critical)" }}> · {status.consecutive_failures} failure{status.consecutive_failures === 1 ? "" : "s"} in a row</span>
        )}
      </div>
      {status.last_error && (
        <div className="text-[11px] font-mono truncate" style={{ color: "var(--critical)" }}>
          {status.last_error}
        </div>
      )}
      <button
        onClick={onSync}
        disabled={syncing}
        className="self-start field px-3 py-1.5 text-[12px] disabled:opacity-40"
        style={{ color: "var(--accent)" }}
      >
        {syncing ? "Syncing…" : "Sync now"}
      </button>
    </div>
  );
}
