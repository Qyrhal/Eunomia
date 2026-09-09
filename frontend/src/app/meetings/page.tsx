"use client";

import { useEffect, useState } from "react";
import { Search, X } from "lucide-react";
import { api } from "@/lib/api";

type Tag = { name: string };
type Recording = {
  id: string;
  title: string;
  summary?: string;
  transcript?: string;
  notes?: string;
  duration?: number;
  tags?: Tag[];
  recording_at?: string;
  created_at?: string;
  url?: string;
};

const fmtDuration = (s: number | undefined) => {
  if (!s) return "–";
  const m = Math.floor(s / 60);
  const sec = s % 60;
  return sec ? `${m}m ${sec}s` : `${m}m`;
};

const fmtDate = (iso?: string) => {
  if (!iso) return "–";
  const d = new Date(iso);
  return d.toLocaleString();
};

export default function MeetingsPage() {
  const [recordings, setRecordings] = useState<Recording[] | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [selected, setSelected] = useState<Recording | null>(null);

  const load = () => {
    setBusy(true);
    setErr(null);
    api
      .get<{ recordings: Recording[] }>("/api/connectors/pocketai/recordings?limit=100")
      .then((r) => setRecordings(r.recordings))
      .catch((e) => setErr(e.message))
      .finally(() => setBusy(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function runSearch(e?: React.FormEvent) {
    e?.preventDefault();
    if (!query.trim()) {
      load();
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      const res = await api.get<{ success: boolean; data: { results: Recording[] } }>(
        `/api/connectors/pocketai/search?q=${encodeURIComponent(query.trim())}`
      );
      setRecordings(res.data?.results ?? []);
    } catch (e2) {
      setErr(e2 instanceof Error ? e2.message : "Search failed.");
    } finally {
      setBusy(false);
    }
  }

  if (err && recordings === null) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Meetings</div>
        <h1 className="font-display text-3xl mb-6">Recordings</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--text-secondary)" }}>
            PocketAI isn&apos;t connected — nothing to transcribe yet.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-5xl">
      <div className="flex items-baseline justify-between mb-6">
        <div>
          <div className="eyebrow mb-2">Meetings</div>
          <h1 className="font-display text-3xl">Recordings</h1>
        </div>
      </div>

      <form onSubmit={runSearch} className="ledger p-3 mb-6 flex items-center gap-2">
        <div className="flex items-center gap-2 flex-1">
          <Search size={15} style={{ color: "var(--text-muted)" }} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search recordings by title, tag, or content…"
            className="field flex-1 px-3 py-2 text-[13.5px]"
          />
        </div>
        <button
          type="submit"
          disabled={busy}
          className="px-4 py-2 text-[13px] font-medium disabled:opacity-40"
          style={{ background: "var(--accent)", color: "var(--surface)", borderRadius: 4 }}
        >
          {busy ? "Searching…" : "Search"}
        </button>
        {query && (
          <button
            type="button"
            onClick={() => {
              setQuery("");
              load();
            }}
            className="px-3 py-2 text-[13px]"
            style={{ color: "var(--text-muted)" }}
          >
            Clear
          </button>
        )}
      </form>

      {err && <div className="text-[12.5px] mb-4" style={{ color: "var(--critical)" }}>{err}</div>}

      <div className="grid gap-6" style={{ gridTemplateColumns: selected ? "1fr 1fr" : "1fr" }}>
        <div>
          {recordings?.length === 0 && (
            <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
              {query ? "No recordings matched your search." : "No recordings yet — connect PocketAI to start transcribing."}
            </p>
          )}
          {recordings && recordings.length > 0 && (
            <ul className="ledger overflow-hidden hairline-rows">
              {recordings.map((r) => (
                <li key={r.id}>
                  <button
                    onClick={() => setSelected(r)}
                    className="w-full text-left px-4 py-3 hover:bg-[var(--surface-2)] transition-colors"
                    style={{ background: selected?.id === r.id ? "var(--surface-2)" : undefined }}
                  >
                    <div className="flex items-baseline justify-between gap-3">
                      <span className="text-[13.5px] font-medium truncate">{r.title || "(untitled)"}</span>
                      <span className="eyebrow shrink-0">{fmtDuration(r.duration)}</span>
                    </div>
                    {(r.summary || r.transcript) && (
                      <div className="text-[12px] mt-0.5 line-clamp-2" style={{ color: "var(--text-secondary)" }}>
                        {r.summary || r.transcript}
                      </div>
                    )}
                    <div className="flex items-center gap-3 mt-1">
                      <span className="font-mono text-[11px]" style={{ color: "var(--text-muted)" }}>
                        {fmtDate(r.recording_at || r.created_at)}
                      </span>
                      {r.tags && r.tags.length > 0 && (
                        <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                          {r.tags.map((t) => t.name).join(" · ")}
                        </span>
                      )}
                    </div>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        {selected && (
          <aside className="ledger p-5 h-fit sticky top-8">
            <div className="flex items-start justify-between gap-3 mb-3">
              <div>
                <div className="eyebrow mb-1">Recording</div>
                <h2 className="font-display text-xl leading-tight">{selected.title || "(untitled)"}</h2>
              </div>
              <button onClick={() => setSelected(null)} aria-label="Close">
                <X size={16} style={{ color: "var(--text-muted)" }} />
              </button>
            </div>
            <div className="flex items-center gap-3 mb-4">
              <span className="font-mono text-[11.5px]" style={{ color: "var(--text-muted)" }}>
                {fmtDate(selected.recording_at || selected.created_at)}
              </span>
              <span className="font-mono text-[11.5px]" style={{ color: "var(--text-muted)" }}>
                {fmtDuration(selected.duration)}
              </span>
            </div>
            {selected.tags && selected.tags.length > 0 && (
              <div className="flex flex-wrap gap-1.5 mb-4">
                {selected.tags.map((t) => (
                  <span
                    key={t.name}
                    className="px-2 py-0.5 text-[11px] font-mono"
                    style={{ background: "var(--surface-2)", color: "var(--text-secondary)", borderRadius: 3 }}
                  >
                    {t.name}
                  </span>
                ))}
              </div>
            )}
            {selected.summary && (
              <>
                <div className="eyebrow mb-1.5">Summary</div>
                <p className="text-[13px] whitespace-pre-wrap mb-4" style={{ color: "var(--text-secondary)" }}>
                  {selected.summary}
                </p>
              </>
            )}
            {selected.transcript && (
              <>
                <div className="eyebrow mb-1.5">Transcript</div>
                <p className="text-[12px] whitespace-pre-wrap mb-4 leading-relaxed" style={{ color: "var(--text-secondary)" }}>
                  {selected.transcript}
                </p>
              </>
            )}
            {selected.notes && (
              <>
                <div className="eyebrow mb-1.5">Notes</div>
                <p className="text-[12px] whitespace-pre-wrap mb-4" style={{ color: "var(--text-secondary)" }}>
                  {selected.notes}
                </p>
              </>
            )}
            {selected.url && (
              <a
                href={selected.url}
                target="_blank"
                rel="noreferrer"
                className="inline-block mt-2 text-[12.5px]"
                style={{ color: "var(--accent)" }}
              >
                Open in PocketAI ↗
              </a>
            )}
          </aside>
        )}
      </div>
    </div>
  );
}
