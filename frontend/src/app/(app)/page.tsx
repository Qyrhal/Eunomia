"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { ArrowRight } from "lucide-react";
import { entities, sources, type SourceRow } from "@/lib/api";
import SyncStatusCard from "@/components/SyncStatusCard";
import EntityGraph from "@/components/EntityGraph";
import StatRing from "@/components/StatRing";

const HEALTH_CAP = 5;

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

function healthColor(failures: number): string {
  if (failures === 0) return "var(--good)";
  if (failures <= 2) return "var(--warning)";
  return "var(--critical)";
}

export default function DashboardPage() {
  const [rows, setRows] = useState<SourceRow[] | null>(null);
  const [entityCount, setEntityCount] = useState<number | null>(null);
  const [syncingKey, setSyncingKey] = useState<string | null>(null);

  const load = useCallback(() => sources.list().then(setRows).catch(() => setRows([])), []);
  useEffect(() => {
    load();
    entities
      .list()
      .then((rows) => setEntityCount(rows.length))
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

  return (
    <div className="flex flex-col gap-10 max-w-5xl">
      <section className="flex flex-wrap items-start gap-10">
        <StatRing
          size={168}
          strokeWidth={12}
          value={healthy}
          max={total}
          color="var(--signal)"
          valueLabel={rows ? `${healthy}/${total}` : "–"}
          label="sources healthy"
          ariaLabel={`${healthy} of ${total} sources syncing cleanly`}
        />

        <div className="flex-1 min-w-[260px] flex flex-col gap-3 pt-1">
          <div className="text-[12px] font-mono" style={{ color: "var(--ink-faint)" }}>
            {entityCount ?? "–"} entities tracked
          </div>
          <div className="flex gap-7 overflow-x-auto pb-1 -mx-1 px-1">
            {connected.map((s) => (
              <div key={s.key} className="flex flex-col items-center gap-2 shrink-0">
                <StatRing
                  size={60}
                  strokeWidth={6}
                  value={Math.max(0, HEALTH_CAP - s.sync_status.consecutive_failures)}
                  max={HEALTH_CAP}
                  color={healthColor(s.sync_status.consecutive_failures)}
                  ariaLabel={`${s.label}: ${
                    s.sync_status.consecutive_failures === 0
                      ? "syncing cleanly"
                      : `${s.sync_status.consecutive_failures} failed sync${s.sync_status.consecutive_failures === 1 ? "" : "s"} in a row`
                  }`}
                />
                <div className="text-[12px] font-medium text-center whitespace-nowrap" style={{ color: "var(--ink)" }}>
                  {s.label}
                </div>
                <div className="text-[10.5px] font-mono whitespace-nowrap" style={{ color: "var(--ink-faint)" }}>
                  {relativeTime(s.sync_status.last_ok)}
                </div>
              </div>
            ))}
            {rows !== null && connected.length === 0 && (
              <p className="text-[13px]" style={{ color: "var(--ink-faint)" }}>
                Nothing connected yet — see &ldquo;what&apos;s not&rdquo; below.
              </p>
            )}
          </div>
        </div>
      </section>

      <section className="flex flex-col gap-4">
        <div className="eyebrow">What&apos;s next</div>
        {rows === null && (
          <p className="text-[13px]" style={{ color: "var(--ink-faint)" }}>
            Loading…
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
          <ul className="ledger overflow-hidden hairline-rows">
            {disconnected.map((s) => (
              <li key={s.key} className="px-4 py-3.5 flex items-center gap-4">
                <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: "var(--ink-faint)" }} aria-hidden />
                <div className="flex-1 min-w-0">
                  <div className="text-[13.5px] font-medium">{s.label}</div>
                  <div className="text-[12px] mt-0.5" style={{ color: "var(--ink-dim)" }}>
                    {s.record_types.join(" · ")}
                  </div>
                </div>
                <Link href="/connectors" className="field px-3 py-1.5 text-[12px] flex items-center gap-1.5 shrink-0" style={{ color: "var(--ink)" }}>
                  Connect <ArrowRight size={12} />
                </Link>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="flex flex-col gap-4">
        <div className="eyebrow">Entity network</div>
        <EntityGraph />
      </section>
    </div>
  );
}
