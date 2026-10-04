"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { connectors, type PocketSummary } from "@/lib/api";

export default function MeetingsPage() {
  const [summary, setSummary] = useState<PocketSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    connectors
      .pocketaiSummary(30)
      .then(setSummary)
      .catch((e) => setError(e.message));
  }, []);

  if (error) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Meetings</div>
        <h1 className="font-display text-3xl mb-6">Recordings</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--ink-dim)" }}>
            PocketAI isn&apos;t connected — nothing to transcribe yet.
          </p>
          <Link href="/connectors" className="px-4 py-2 text-[13px] font-medium rounded-xl inline-block" style={{ background: "var(--felt)", color: "var(--canvas)" }}>
            Connect PocketAI
          </Link>
        </div>
      </div>
    );
  }

  const maxTag = Math.max(1, ...(summary?.tag_breakdown.map((t) => t.count) ?? [0]));

  return (
    <div className="flex flex-col gap-7 max-w-4xl">
      <div>
        <div className="eyebrow mb-2">Meetings</div>
        <h1 className="font-display text-3xl">Recordings</h1>
      </div>

      <div className="grid md:grid-cols-2 gap-5">
        <div className="ledger p-6" style={{ background: "var(--ink)", borderColor: "var(--ink)" }}>
          <div className="eyebrow" style={{ color: "var(--canvas)", opacity: 0.65 }}>
            Recordings — 30 days
          </div>
          <div className="font-mono text-4xl my-2" style={{ color: "var(--canvas)" }}>
            {summary ? summary.recordings_count : "–"}
          </div>
          <div className="text-[12.5px] font-mono mt-4 pt-4" style={{ color: "var(--canvas)", opacity: 0.75, borderTop: "1px solid rgba(255,255,255,0.14)" }}>
            {summary ? `${summary.total_duration_minutes.toFixed(0)} min total` : ""}
          </div>
        </div>

        <div className="ledger p-6">
          <div className="eyebrow mb-4">Tags</div>
          <div className="flex flex-col gap-2.5">
            {summary?.tag_breakdown.slice(0, 6).map((t) => (
              <div key={t.tag} className="flex items-center gap-3">
                <span className="w-24 text-[12.5px] truncate" style={{ color: "var(--ink-dim)" }}>
                  {t.tag}
                </span>
                <div className="flex-1 h-1.5" style={{ background: "var(--surface-raised)" }}>
                  <div className="h-1.5" style={{ width: `${Math.round((t.count / maxTag) * 100)}%`, background: "var(--series-2)" }} />
                </div>
                <span className="font-mono text-[12.5px] font-medium w-10 text-right">{t.count}</span>
              </div>
            ))}
            {summary && summary.tag_breakdown.length === 0 && (
              <span className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
                No tagged recordings in this period.
              </span>
            )}
          </div>
        </div>
      </div>

      <div className="ledger p-6">
        <div className="eyebrow mb-3">Recent recordings</div>
        <ul className="hairline-rows">
          {summary?.recent_recordings.map((r, i) => (
            <li key={i} className="flex items-center justify-between py-2.5 text-[13.5px]">
              <div>
                <div>{r.title || "Untitled recording"}</div>
                <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "var(--ink-faint)" }}>
                  {r.recorded_at ? new Date(r.recorded_at).toLocaleDateString() : ""} · {r.tags.join(", ")}
                </div>
              </div>
              <span className="font-mono font-medium" style={{ color: "var(--ink-dim)" }}>
                {r.duration_minutes.toFixed(0)} min
              </span>
            </li>
          ))}
          {summary && summary.recent_recordings.length === 0 && (
            <li className="text-[12.5px] py-6 text-center" style={{ color: "var(--ink-faint)" }}>
              No recent recordings.
            </li>
          )}
        </ul>
      </div>
    </div>
  );
}
