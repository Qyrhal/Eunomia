"use client";

import { useEffect, useState } from "react";
import { Search, X } from "lucide-react";
import { FullRecord, Hit, getRecord, listSources, search } from "@/lib/admin";

const MODES = ["hybrid", "keyword", "semantic"] as const;

export default function DataBrowser() {
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<(typeof MODES)[number]>("hybrid");
  const [sources, setSources] = useState<string[]>([]);
  const [sourceFilter, setSourceFilter] = useState<string>("");
  const [hits, setHits] = useState<Hit[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [selected, setSelected] = useState<FullRecord | null>(null);

  useEffect(() => {
    listSources().then((rows) => setSources(rows.map((r) => r.key))).catch(() => {});
  }, []);

  async function run(e?: React.FormEvent) {
    e?.preventDefault();
    if (!query.trim()) return;
    setBusy(true);
    setErr(null);
    try {
      const res = await search({
        query,
        mode,
        sources: sourceFilter ? [sourceFilter] : undefined,
        limit: 40,
      });
      setHits(res.results);
      setSelected(null);
    } catch (e2) {
      setErr(e2 instanceof Error ? e2.message : "Search failed.");
    } finally {
      setBusy(false);
    }
  }

  async function open(id: string) {
    const rec = await getRecord(id);
    if ("error" in rec) setErr(rec.error);
    else setSelected(rec);
  }

  return (
    <div className="max-w-6xl">
      <h1 className="font-display text-3xl mb-1">Data</h1>
      <p className="text-[13px] mb-6" style={{ color: "var(--text-secondary)" }}>
        Everything Eunomia has pulled in, masked and indexed. This is what the agent sees.
      </p>

      <form onSubmit={run} className="ledger p-3 mb-6 flex flex-wrap items-center gap-2">
        <div className="flex items-center gap-2 flex-1 min-w-[240px]">
          <Search size={15} style={{ color: "var(--text-muted)" }} />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search your mail, calendar, transactions, files…"
            className="field flex-1 px-3 py-2 text-[13.5px]"
          />
        </div>
        <select
          value={sourceFilter}
          onChange={(e) => setSourceFilter(e.target.value)}
          className="field px-2.5 py-2 text-[12.5px]"
          aria-label="Source filter"
        >
          <option value="">All sources</option>
          {sources.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <div className="flex" role="group" aria-label="Search mode">
          {MODES.map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => setMode(m)}
              className="px-2.5 py-2 text-[12px] font-mono border first:rounded-l last:rounded-r"
              style={{
                background: mode === m ? "var(--accent)" : "var(--surface-2)",
                color: mode === m ? "var(--surface)" : "var(--text-secondary)",
                borderColor: "var(--border)",
              }}
            >
              {m}
            </button>
          ))}
        </div>
        <button
          type="submit"
          disabled={busy || !query.trim()}
          className="px-4 py-2 text-[13px] font-medium disabled:opacity-40"
          style={{ background: "var(--accent)", color: "var(--surface)", borderRadius: 4 }}
        >
          {busy ? "Searching…" : "Search"}
        </button>
      </form>

      {err && (
        <div className="text-[12.5px] mb-4" style={{ color: "var(--critical)" }}>
          {err}
        </div>
      )}

      <div className="grid gap-6" style={{ gridTemplateColumns: selected ? "1fr 1fr" : "1fr" }}>
        <div>
          {hits === null && (
            <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
              Run a search to see results.
            </p>
          )}
          {hits?.length === 0 && (
            <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
              Nothing matched. Try a broader term or a different mode.
            </p>
          )}
          {hits && hits.length > 0 && (
            <ul className="ledger overflow-hidden hairline-rows">
              {hits.map((h) => (
                <li key={h.id}>
                  <button
                    onClick={() => open(h.id)}
                    className="w-full text-left px-4 py-3 hover:bg-[var(--surface-2)] transition-colors"
                    style={{ background: selected?.id === h.id ? "var(--surface-2)" : undefined }}
                  >
                    <div className="flex items-baseline justify-between gap-3">
                      <span className="text-[13.5px] font-medium truncate">{h.title || "(untitled)"}</span>
                      <span className="eyebrow shrink-0">{h.type}</span>
                    </div>
                    <div className="text-[12px] mt-0.5 line-clamp-2" style={{ color: "var(--text-secondary)" }}>
                      {h.snippet}
                    </div>
                    {h.occurred_at && (
                      <div className="font-mono text-[11px] mt-1" style={{ color: "var(--text-muted)" }}>
                        {new Date(h.occurred_at).toLocaleString()}
                      </div>
                    )}
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
                <div className="eyebrow mb-1">{selected.type}</div>
                <h2 className="font-display text-xl leading-tight">{selected.title || "(untitled)"}</h2>
              </div>
              <button onClick={() => setSelected(null)} aria-label="Close">
                <X size={16} style={{ color: "var(--text-muted)" }} />
              </button>
            </div>
            {selected.occurred_at && (
              <div className="font-mono text-[11.5px] mb-3" style={{ color: "var(--text-muted)" }}>
                {new Date(selected.occurred_at).toLocaleString()}
              </div>
            )}
            {selected.body_text && (
              <p className="text-[13px] whitespace-pre-wrap mb-4" style={{ color: "var(--text-secondary)" }}>
                {selected.body_text}
              </p>
            )}
            <div className="eyebrow mb-1.5">Payload</div>
            <pre className="field p-3 text-[11.5px] font-mono overflow-x-auto mb-4">
              {JSON.stringify(selected.payload, null, 2)}
            </pre>
            {selected.links.length > 0 && (
              <>
                <div className="eyebrow mb-1.5">Links</div>
                <ul className="hairline-rows text-[12px]">
                  {selected.links.map((l, i) => (
                    <li key={i} className="py-1.5 font-mono">
                      <span style={{ color: "var(--accent)" }}>{l.rel}</span>{" "}
                      <span style={{ color: "var(--text-muted)" }}>{l.direction}</span> {l.target_id}
                    </li>
                  ))}
                </ul>
              </>
            )}
            {selected.url && (
              <a
                href={selected.url}
                target="_blank"
                rel="noreferrer"
                className="inline-block mt-3 text-[12.5px]"
                style={{ color: "var(--accent)" }}
              >
                Open at source ↗
              </a>
            )}
          </aside>
        )}
      </div>
    </div>
  );
}
