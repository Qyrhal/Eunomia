"use client";

import { useCallback, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { SourceRow, listSources, syncSource } from "@/lib/admin";

export default function SourcesPage() {
  const [rows, setRows] = useState<SourceRow[] | null>(null);
  const [syncing, setSyncing] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const load = useCallback(() => listSources().then(setRows).catch(() => setRows([])), []);
  useEffect(() => {
    load();
  }, [load]);

  async function run(key: string) {
    setSyncing(key);
    setNote(null);
    try {
      const r = await syncSource(key);
      setNote(`${key}: ${JSON.stringify(r)}`);
      load();
    } catch (e) {
      setNote(e instanceof Error ? e.message : "Sync failed.");
    } finally {
      setSyncing(null);
    }
  }

  return (
    <div className="max-w-3xl">
      <h1 className="font-display text-3xl mb-1">Sources</h1>
      <p className="text-[13px] mb-6" style={{ color: "var(--text-secondary)" }}>
        Where the data comes from, and whether the last pull went through.
      </p>

      {rows === null && (
        <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
          Loading…
        </p>
      )}
      {rows?.length === 0 && (
        <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
          No sources registered.
        </p>
      )}

      <ul className="ledger overflow-hidden hairline-rows">
        {rows?.map((s) => (
          <li key={s.key} className="px-4 py-3.5 flex items-center gap-4">
            <span
              className="w-1.5 h-1.5 rounded-full shrink-0"
              style={{ background: s.last_error ? "var(--critical)" : s.enabled ? "var(--good)" : "var(--text-muted)" }}
              aria-hidden
            />
            <div className="flex-1 min-w-0">
              <div className="flex items-baseline gap-2">
                <span className="text-[13.5px] font-medium">{s.label}</span>
                <span className="eyebrow">{s.key}</span>
                {!s.enabled && (
                  <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                    not connected
                  </span>
                )}
              </div>
              <div className="text-[12px] mt-0.5" style={{ color: "var(--text-secondary)" }}>
                {s.record_types.join(" · ")}
              </div>
              {s.last_error ? (
                <div className="text-[11.5px] font-mono mt-1" style={{ color: "var(--critical)" }}>
                  {s.last_error}
                </div>
              ) : s.last_run ? (
                <div className="text-[11.5px] font-mono mt-1" style={{ color: "var(--text-muted)" }}>
                  last run {new Date(s.last_run).toLocaleString()}
                </div>
              ) : null}
            </div>
            <button
              onClick={() => run(s.key)}
              disabled={syncing === s.key}
              className="field px-3 py-1.5 text-[12px] flex items-center gap-1.5 shrink-0 disabled:opacity-40"
              style={{ color: "var(--accent)" }}
            >
              <RefreshCw size={12} className={syncing === s.key ? "animate-spin" : ""} />
              Sync now
            </button>
          </li>
        ))}
      </ul>

      {note && (
        <pre className="field mt-4 p-3 text-[11.5px] font-mono overflow-x-auto">{note}</pre>
      )}
    </div>
  );
}
