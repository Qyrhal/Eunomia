"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { ArrowRight, Plug } from "lucide-react";
import { entities, sources, type SourceRow } from "@/lib/api";
import SyncStatusCard from "@/components/SyncStatusCard";
import EntityGraph from "@/components/EntityGraph";
import StatRing from "@/components/StatRing";

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

function StatTile({ label, value }: { label: string; value: string }) {
  return (
    <div className="ledger p-5 flex flex-col justify-center gap-1.5">
      <div className="eyebrow">{label}</div>
      <div className="font-display text-2xl" style={{ color: "var(--ink)" }}>
        {value}
      </div>
    </div>
  );
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
  const totalRecords = rows?.reduce((sum, r) => sum + r.record_count, 0) ?? 0;
  const lastSyncs = connected.map((r) => r.sync_status.last_ok).filter((d): d is string => Boolean(d));
  const lastSync = lastSyncs.length ? lastSyncs.sort().at(-1)! : null;

  return (
    <div className="flex flex-col gap-10 max-w-5xl">
      <section className="grid gap-5" style={{ gridTemplateColumns: "auto 1fr" }}>
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
          <StatTile label="Entities tracked" value={entityCount !== null ? entityCount.toLocaleString() : "–"} />
          <StatTile label="Last sync" value={relativeTime(lastSync)} />
        </div>
      </section>

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

      <section className="flex flex-col gap-4">
        <div className="eyebrow">Entity network</div>
        <EntityGraph />
      </section>
    </div>
  );
}
