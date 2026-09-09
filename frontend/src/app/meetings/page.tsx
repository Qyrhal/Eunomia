"use client";

import { useEffect, useState } from "react";
import { Search, X, Clock, Tag } from "lucide-react";
import { api } from "@/lib/api";

type Tag = { id?: string; name: string; color?: string | null };
type TranscriptSegment = { start: number; end: number; text: string; speaker?: string };
type Summary = { markdown?: string; text?: string };
type Recording = {
  id: string;
  title: string;
  folder_id?: string;
  duration?: number;
  state?: string;
  language?: string | null;
  recording_at?: string;
  created_at?: string;
  updated_at?: string;
  tags?: Tag[];
  transcript?: { metadata?: { duration?: number; source?: string }; segments?: TranscriptSegment[] };
  summarizations?: Record<string, { v2?: { summary?: Summary }; processingStatus?: string }>;
};

type Insight = string;

const fmtDuration = (s: number | undefined) => {
  if (!s) return "–";
  const m = Math.floor(s / 60);
  const sec = Math.round(s % 60);
  return sec ? `${m}m ${sec}s` : `${m}m`;
};

const fmtDate = (iso?: string) => {
  if (!iso) return "–";
  const d = new Date(iso);
  return d.toLocaleString();
};

const getSummaryMarkdown = (r: Recording): string | null => {
  const summs = r.summarizations;
  if (!summs) return null;
  for (const key of Object.keys(summs)) {
    const s = summs[key];
    if (s.processingStatus === "completed" && s.v2?.summary?.markdown) {
      return s.v2.summary.markdown;
    }
  }
  return null;
};

export default function MeetingsPage() {
  const [recordings, setRecordings] = useState<Recording[] | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [selected, setSelected] = useState<Recording | null>(null);
  const [detail, setDetail] = useState<Recording | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [searchResults, setSearchResults] = useState<{
    insights: Insight[];
    static_facts: string[];
    recordings: Recording[];
  } | null>(null);

  const load = () => {
    setBusy(true);
    setErr(null);
    api
      .get<{ recordings: Recording[] }>("/api/connectors/pocketai/recordings?limit=100")
      .then((r) => {
        setRecordings(r.recordings);
        setSearchResults(null);
      })
      .catch((e) => setErr(e.message))
      .finally(() => setBusy(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function openRecording(rec: Recording) {
    setSelected(rec);
    setDetailLoading(true);
    setDetail(null);
    try {
      const res = await api.get<{ success: boolean; data: Recording }>(
        `/api/connectors/pocketai/recording/${rec.id}`
      );
      setDetail(res.data || res);
    } catch (e) {
      // If detail fails, fall back to the list metadata
      setDetail(rec);
    } finally {
      setDetailLoading(false);
    }
  }

  async function runSearch(e?: React.FormEvent) {
    e?.preventDefault();
    if (!query.trim()) {
      load();
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      const res = await api.get<{
        success: boolean;
        insights: Insight[];
        static_facts: string[];
        recordings: Recording[];
      }>(`/api/connectors/pocketai/search?q=${encodeURIComponent(query.trim())}`);
      setSearchResults({
        insights: res.insights || [],
        static_facts: res.static_facts || [],
        recordings: res.recordings || [],
      });
      setRecordings(null);
    } catch (e2) {
      setErr(e2 instanceof Error ? e2.message : "Search failed.");
    } finally {
      setBusy(false);
    }
  }

  const displayingResults = searchResults?.recordings?.length ? searchResults.recordings : recordings;

  if (err && recordings === null && !searchResults) {
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
            placeholder="Search recordings…"
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

      {searchResults && (
        <div className="ledger p-5 mb-6">
          <div className="eyebrow mb-3">Search Insights</div>
          {searchResults.insights.length === 0 && searchResults.static_facts.length === 0 && (
            <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
              No insights for this query.
            </p>
          )}
          {searchResults.insights.length > 0 && (
            <ul className="flex flex-col gap-2 mb-4">
              {searchResults.insights.map((insight, i) => (
                <li key={i} className="text-[13px]" style={{ color: "var(--text-secondary)" }}>
                  {insight}
                </li>
              ))}
            </ul>
          )}
          {searchResults.static_facts.length > 0 && (
            <>
              <div className="eyebrow mb-2 mt-3">Facts</div>
              <ul className="flex flex-col gap-1.5">
                {searchResults.static_facts.map((fact, i) => (
                  <li key={i} className="text-[12px] font-mono" style={{ color: "var(--text-muted)" }}>
                    {fact}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}

      <div className="grid gap-6" style={{ gridTemplateColumns: selected ? "1fr 1.2fr" : "1fr" }}>
        <div>
          {displayingResults?.length === 0 && (
            <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
              {query ? "No recordings matched your search." : "No recordings yet — connect PocketAI to start transcribing."}
            </p>
          )}
          {displayingResults && displayingResults.length > 0 && (
            <ul className="ledger overflow-hidden hairline-rows">
              {displayingResults.map((r) => (
                <li key={r.id}>
                  <button
                    onClick={() => openRecording(r)}
                    className="w-full text-left px-4 py-3 hover:bg-[var(--surface-2)] transition-colors"
                    style={{ background: selected?.id === r.id ? "var(--surface-2)" : undefined }}
                  >
                    <div className="flex items-baseline justify-between gap-3">
                      <span className="text-[13.5px] font-medium truncate">{r.title || "(untitled)"}</span>
                      <span className="eyebrow shrink-0 flex items-center gap-1">
                        <Clock size={10} />
                        {fmtDuration(r.duration)}
                      </span>
                    </div>
                    <div className="flex items-center gap-3 mt-1">
                      <span className="font-mono text-[11px]" style={{ color: "var(--text-muted)" }}>
                        {fmtDate(r.recording_at || r.created_at)}
                      </span>
                      {r.tags && r.tags.length > 0 && (
                        <span className="text-[11px] flex items-center gap-1" style={{ color: "var(--text-muted)" }}>
                          <Tag size={9} />
                          {r.tags.map((t) => t.name).join(", ")}
                        </span>
                      )}
                    </div>
                    {r.state && r.state !== "completed" && (
                      <span className="text-[11px] font-mono mt-0.5 inline-block" style={{ color: "var(--warning)" }}>
                        {r.state}
                      </span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        {selected && (
          <aside className="ledger p-5 h-fit sticky top-8 flex flex-col gap-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="eyebrow mb-1">Recording</div>
                <h2 className="font-display text-xl leading-tight">{selected.title || "(untitled)"}</h2>
              </div>
              <button onClick={() => { setSelected(null); setDetail(null); }} aria-label="Close">
                <X size={16} style={{ color: "var(--text-muted)" }} />
              </button>
            </div>

            <div className="flex items-center gap-3 text-[11.5px] font-mono" style={{ color: "var(--text-muted)" }}>
              <span>{fmtDate(selected.recording_at || selected.created_at)}</span>
              <span>·</span>
              <span>{fmtDuration(selected.duration)}</span>
              {selected.transcript?.metadata?.source && (
                <>
                  <span>·</span>
                  <span>source: {selected.transcript.metadata.source}</span>
                </>
              )}
            </div>

            {(detail || selected) && (() => {
              const rec = detail || selected!;
              const tags = rec.tags || [];
              if (tags.length > 0) {
                return (
                  <div className="flex flex-wrap gap-1.5">
                    {tags.map((t, i) => (
                      <span
                        key={t.id || i}
                        className="px-2 py-0.5 text-[11px] font-mono"
                        style={{ background: "var(--surface-2)", color: "var(--text-secondary)", borderRadius: 3 }}
                      >
                        {t.name}
                      </span>
                    ))}
                  </div>
                );
              }
              return null;
            })()}

            {detailLoading && (
              <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
                Loading transcript…
              </p>
            )}

            {!detailLoading && (detail || selected) && (() => {
              const rec = detail || selected!;
              const summary = getSummaryMarkdown(rec);
              if (summary) {
                return (
                  <>
                    <div className="eyebrow">Summary</div>
                    <div
                      className="text-[13px] whitespace-pre-wrap"
                      style={{ color: "var(--text-secondary)" }}
                    >
                      {summary}
                    </div>
                  </>
                );
              }
              return null;
            })()}

            {!detailLoading && (detail || selected) && (() => {
              const rec = detail || selected!;
              const segments = rec.transcript?.segments || [];
              if (segments.length === 0) return null;
              return (
                <>
                  <div className="eyebrow">Transcript ({segments.length} segments)</div>
                  <div className="max-h-96 overflow-y-auto flex flex-col gap-2 pr-2">
                    {segments.map((seg, i) => (
                      <div key={i} className="text-[12px] leading-relaxed">
                        <span className="font-mono text-[10px] mr-2" style={{ color: "var(--text-muted)" }}>
                          {seg.speaker ? `${seg.speaker} ` : ""}[{Math.floor(seg.start / 60)}:{String(Math.floor(seg.start % 60)).padStart(2, "0")}–{Math.floor(seg.end / 60)}:{String(Math.floor(seg.end % 60)).padStart(2, "0")}]
                        </span>
                        <span style={{ color: "var(--text-secondary)" }}>{seg.text}</span>
                      </div>
                    ))}
                  </div>
                </>
              );
            })()}

            {!detailLoading && (detail || selected) && (() => {
              const rec = detail || selected!;
              const summary = getSummaryMarkdown(rec);
              const segments = rec.transcript?.segments || [];
              if (!summary && segments.length === 0) {
                return (
                  <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
                    No transcript or summary available for this recording.
                  </p>
                );
              }
              return null;
            })()}
          </aside>
        )}
      </div>
    </div>
  );
}
