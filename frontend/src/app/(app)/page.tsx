"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { ArrowRight } from "lucide-react";
import { entities, sources, type SourceRow } from "@/lib/api";
import SyncStatusCard from "@/components/SyncStatusCard";
import EntityGraph from "@/components/EntityGraph";

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
  const everSynced = connected.filter((r) => r.sync_status.last_ok).length;

  return (
    <div className="flex flex-col gap-10 max-w-5xl">
      <div>
        <div className="eyebrow mb-2">Dashboard</div>
        <h1 className="font-display text-3xl">The register</h1>
      </div>

      <div className="grid grid-cols-2 sm:grid-cols-4 gap-5">
        <StatTile label="Entities tracked" value={entityCount ?? "–"} />
        <StatTile label="Sources connected" value={rows ? `${connected.length}/${rows.length}` : "–"} />
        <StatTile label="Syncing cleanly" value={rows ? everSynced : "–"} />
        <StatTile label="Needs attention" value={rows ? connected.filter((r) => r.sync_status.consecutive_failures > 0).length : "–"} />
      </div>

      <section className="flex flex-col gap-4">
        <div className="eyebrow">What&apos;s next</div>
        {rows === null && (
          <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
            Loading…
          </p>
        )}
        {rows !== null && connected.length === 0 && (
          <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
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
          <ul className="ledger overflow-hidden hairline-rows">
            {disconnected.map((s) => (
              <li key={s.key} className="px-4 py-3.5 flex items-center gap-4">
                <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: "var(--text-muted)" }} aria-hidden />
                <div className="flex-1 min-w-0">
                  <div className="text-[13.5px] font-medium">{s.label}</div>
                  <div className="text-[12px] mt-0.5" style={{ color: "var(--text-secondary)" }}>
                    {s.record_types.join(" · ")}
                  </div>
                </div>
                <Link href="/connectors" className="field px-3 py-1.5 text-[12px] flex items-center gap-1.5 shrink-0" style={{ color: "var(--accent)" }}>
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

function StatTile({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="ledger p-5">
      <div className="eyebrow mb-2">{label}</div>
      <div className="font-mono text-3xl">{value}</div>
    </div>
  );
}
