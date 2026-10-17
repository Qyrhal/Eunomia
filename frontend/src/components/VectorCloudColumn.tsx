"use client";

import { useMemo, useState } from "react";
import { Search } from "lucide-react";
import ErrorLine, { failure } from "./ErrorLine";
import { useCloudCompare } from "@/lib/queries/entities";
import type { CloudPoint } from "@/lib/types";

/** Most points one comparison takes (the backend's `MAX_COMPARE`). */
export const MAX_COMPARE = 8;
// ponytail: renders the first ROW_CAP matches; the filter narrows the rest. Virtualise if people scroll thousands.
const ROW_CAP = 200;
const TOP = 3;

type Point = CloudPoint & { color: string };
type Props = {
  points: Point[];
  space: "semantic" | "lexical";
  meanSimilarity: number | null;
  vaultIds: string[];
  checked: string[];
  onToggle: (id: string) => void;
  onClear: () => void;
  /** highlight a point in the graph without changing the comparison */
  onFocus: (id: string) => void;
  nameOf: (vaultId: string) => string;
};

const num = (x: number) => x.toFixed(2);

function countBy(points: Point[], key: (p: Point) => string): [string, number][] {
  const m = new Map<string, number>();
  for (const p of points) m.set(key(p), (m.get(key(p)) ?? 0) + 1);
  return [...m].sort((a, b) => b[1] - a[1]);
}

export default function VectorCloudColumn({ points, space, meanSimilarity, vaultIds, checked, onToggle, onClear, onFocus, nameOf }: Props) {
  const byId = useMemo(() => new Map(points.map((p) => [p.id, p])), [points]);
  const kinds = useMemo(() => countBy(points, (p) => p.kind), [points]);
  const vaults = useMemo(() => countBy(points, (p) => p.vault), [points]);
  const ranked = useMemo(() => [...points].sort((a, b) => b.centrality - a.centrality), [points]);
  const central = ranked.slice(0, TOP);
  const isolated = points.length > TOP ? ranked.slice(-TOP).reverse() : [];
  const what = space === "semantic" ? "model embeddings" : "word vectors (no model key)";

  const [filter, setFilter] = useState("");
  const q = filter.trim().toLowerCase();
  const matches = q ? points.filter((p) => p.label.toLowerCase().includes(q) || p.kind.includes(q)) : points;

  const pointLink = (id: string, extra?: React.ReactNode) => {
    const p = byId.get(id);
    if (!p) return null;
    return (
      <button type="button" className="flex items-center gap-2 min-w-0 w-full text-left text-[13px]" onClick={() => onFocus(id)}>
        <span className="dot shrink-0" style={{ background: p.color }} aria-hidden />
        <span className="truncate" style={{ color: "var(--ink)" }}>{p.label}</span>
        {extra}
      </button>
    );
  };

  return (
    <div className="flex flex-col gap-3 min-w-0">
      <section className="ledger hairline-rows" aria-label="Cloud insights">
        <div className="grid grid-cols-2">
          <div className="p-3 flex flex-col gap-1">
            <span className="label" style={{ color: "var(--ink-faint)" }}>Points</span>
            <span className="font-mono text-[26px] leading-none tracking-[-0.02em]">{points.length}</span>
          </div>
          <div className="p-3 flex flex-col gap-1" style={{ borderLeft: "var(--hair) solid var(--border)" }}>
            <span className="label" style={{ color: "var(--ink-faint)" }}>Mean similarity</span>
            <span className="font-mono text-[26px] leading-none tracking-[-0.02em]">{meanSimilarity === null ? "n/a" : num(meanSimilarity)}</span>
          </div>
        </div>
        <p className="px-3 py-2 text-[12px]" style={{ color: "var(--ink-faint)" }}>
          Similarity is the cosine of the full {what}, not the 3D picture. 1 is identical, 0 unrelated.
        </p>
        <div className="p-3 flex flex-col gap-1.5">
          <span className="label" style={{ color: "var(--ink-faint)" }}>By kind</span>
          <ul className="flex flex-wrap gap-x-4 gap-y-1 text-[13px]">
            {kinds.map(([k, n]) => (
              <li key={k}>
                {k} <span className="font-mono" style={{ color: "var(--ink-faint)" }}>{n}</span>
              </li>
            ))}
          </ul>
        </div>
        {vaults.length > 1 && (
          <div className="p-3 flex flex-col gap-1.5">
            <span className="label" style={{ color: "var(--ink-faint)" }}>By vault</span>
            <ul className="flex flex-wrap gap-x-4 gap-y-1 text-[13px]">
              {vaults.map(([v, n]) => (
                <li key={v}>
                  {nameOf(v)} <span className="font-mono" style={{ color: "var(--ink-faint)" }}>{n}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
        <div className="p-3 flex flex-col gap-1.5">
          <span className="label" style={{ color: "var(--ink-faint)" }}>Most central</span>
          <ul className="flex flex-col gap-1">
            {central.map((p) => (
              <li key={p.id}>{pointLink(p.id, <span className="font-mono text-[12px] ml-auto shrink-0" style={{ color: "var(--ink-faint)" }}>{num(p.centrality)}</span>)}</li>
            ))}
          </ul>
        </div>
        {isolated.length > 0 && (
          <div className="p-3 flex flex-col gap-1.5">
            <span className="label" style={{ color: "var(--ink-faint)" }}>Most isolated</span>
            <ul className="flex flex-col gap-1">
              {isolated.map((p) => (
                <li key={p.id}>{pointLink(p.id, <span className="font-mono text-[12px] ml-auto shrink-0" style={{ color: "var(--ink-faint)" }}>{num(p.centrality)}</span>)}</li>
              ))}
            </ul>
            <p className="text-[12px]" style={{ color: "var(--ink-faint)" }}>Scores are the similarity to the cloud&apos;s average direction.</p>
          </div>
        )}
      </section>

      {checked.length > 0 && (
        <Comparison
          vaultIds={vaultIds}
          checked={checked}
          byId={byId}
          what={what}
          onClear={onClear}
          pointLink={pointLink}
        />
      )}

      <section className="ledger flex flex-col min-h-0" aria-label="Rows">
        <div className="p-2 flex items-center gap-2" style={{ borderBottom: "var(--hair) solid var(--border)" }}>
          <div className="field flex items-center gap-2 h-8 px-2.5 flex-1">
            <Search size={14} strokeWidth={1.75} style={{ color: "var(--ink-faint)" }} aria-hidden />
            <input
              className="bg-transparent outline-none text-[13px] flex-1 min-w-0"
              placeholder="Filter rows"
              aria-label="Filter rows"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
          </div>
          <span className="font-mono text-[12px] shrink-0" style={{ color: "var(--ink-faint)" }}>
            {checked.length}/{MAX_COMPARE}
          </span>
        </div>
        {checked.length >= MAX_COMPARE && (
          <p className="px-3 py-2 text-[12px]" style={{ color: "var(--ink-dim)", borderBottom: "var(--hair) solid var(--border)" }}>
            Comparing {MAX_COMPARE}, the most at once. Unpick a row to add another.
          </p>
        )}
        <table className="data-table" style={{ tableLayout: "fixed" }}>
          <thead>
            <tr>
              <th>Memory</th>
              <th style={{ width: 96 }}>Kind</th>
            </tr>
          </thead>
          <tbody>
            {matches.length === 0 ? (
              <tr>
                <td colSpan={2} style={{ height: 56, color: "var(--ink-dim)" }}>
                  No rows match. Clear the filter to see every point.
                </td>
              </tr>
            ) : (
              matches.slice(0, ROW_CAP).map((p) => {
                const on = checked.includes(p.id);
                const full = !on && checked.length >= MAX_COMPARE;
                return (
                  <tr key={p.id} style={on ? { background: "var(--accent-soft)", outline: "1px solid var(--accent)", outlineOffset: -1 } : undefined}>
                    <td className="overflow-hidden">
                      <button
                        type="button"
                        aria-pressed={on}
                        disabled={full}
                        className="flex items-center gap-2 w-full min-w-0 text-left disabled:opacity-50"
                        onClick={() => onToggle(p.id)}
                      >
                        <span className="dot shrink-0" style={{ background: p.color }} aria-hidden />
                        <span className="truncate" style={{ color: "var(--ink)" }}>{p.label}</span>
                      </button>
                    </td>
                    <td className="truncate" style={{ color: "var(--ink-dim)" }}>{p.kind}</td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
        {matches.length > ROW_CAP && (
          <p className="px-3 py-2 text-[12px]" style={{ color: "var(--ink-faint)", borderTop: "var(--hair) solid var(--border)" }}>
            Showing {ROW_CAP} of {matches.length}. Filter to find the rest.
          </p>
        )}
      </section>
    </div>
  );
}

function Comparison({
  vaultIds,
  checked,
  byId,
  what,
  onClear,
  pointLink,
}: {
  vaultIds: string[];
  checked: string[];
  byId: Map<string, Point>;
  what: string;
  onClear: () => void;
  pointLink: (id: string, extra?: React.ReactNode) => React.ReactNode;
}) {
  const query = useCloudCompare(vaultIds, checked);
  const data = query.data;
  const pair = (a: string, b: string) => data?.pairs.find((p) => (p.a === a && p.b === b) || (p.a === b && p.b === a));
  const two = checked.length === 2 ? pair(checked[0], checked[1]) : undefined;

  return (
    <section className="ledger hairline-rows" aria-label="Comparison">
      <div className="px-3 h-10 flex items-center gap-2">
        <span className="section-title text-[13px]">Compare {checked.length}</span>
        <button type="button" className="btn btn-ghost btn-sm ml-auto" onClick={onClear}>
          Clear
        </button>
      </div>
      {query.isError ? (
        <div className="p-3">
          <ErrorLine error={failure(query.error, "Could not compare these points.", " Try again in a moment.")} />
        </div>
      ) : !data ? (
        <div className="p-3 flex flex-col gap-2" aria-busy="true" aria-label="Loading the comparison">
          <span className="skeleton block h-4 w-3/4" />
          <span className="skeleton block h-4 w-1/2" />
        </div>
      ) : (
        <>
          {checked.length === 1 && (
            <p className="px-3 py-2 text-[12px]" style={{ color: "var(--ink-faint)" }}>Pick another row to compare the two.</p>
          )}
          {two && (
            <dl className="grid grid-cols-2">
              <div className="p-3 flex flex-col gap-1">
                <dt className="label" style={{ color: "var(--ink-faint)" }}>Cosine similarity</dt>
                <dd className="font-mono text-[26px] leading-none tracking-[-0.02em]">{num(two.similarity)}</dd>
              </div>
              <div className="p-3 flex flex-col gap-1" style={{ borderLeft: "var(--hair) solid var(--border)" }}>
                <dt className="label" style={{ color: "var(--ink-faint)" }}>Distance</dt>
                <dd className="font-mono text-[26px] leading-none tracking-[-0.02em]">{num(two.distance)}</dd>
              </div>
            </dl>
          )}
          {checked.length >= 3 && (
            <div className="overflow-x-auto">
              <table className="data-table">
                <caption className="sr-only">Pairwise cosine similarity</caption>
                <thead>
                  <tr>
                    <th><span className="sr-only">Row</span></th>
                    {checked.map((_, i) => (
                      <th key={i} className="font-mono text-right">{i + 1}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {checked.map((a, i) => (
                    <tr key={a}>
                      <th scope="row" className="font-mono">{i + 1}</th>
                      {checked.map((b) => {
                        const s = a === b ? 1 : pair(a, b)?.similarity;
                        return (
                          <td key={b} className="font-mono text-[12px] text-right" style={{ color: a === b ? "var(--ink-faint)" : "var(--ink)" }}>
                            {s === undefined ? "" : num(s)}
                          </td>
                        );
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <ol className="flex flex-col hairline-rows">
            {checked.map((id, i) => {
              const near = data.neighbours.find((n) => n.id === id)?.near ?? [];
              return (
                <li key={id} className="p-3 flex flex-col gap-1.5">
                  <div className="flex items-center gap-2 min-w-0">
                    {checked.length >= 3 && <span className="font-mono text-[12px] shrink-0" style={{ color: "var(--ink-faint)" }}>{i + 1}</span>}
                    {pointLink(id)}
                  </div>
                  <span className="label" style={{ color: "var(--ink-faint)" }}>Nearest {near.length}</span>
                  <ul className="flex flex-col gap-1 pl-2">
                    {near.map((n) =>
                      byId.has(n.id) ? (
                        <li key={n.id}>
                          {pointLink(n.id, <span className="font-mono text-[12px] ml-auto shrink-0" style={{ color: "var(--ink-faint)" }}>{num(n.similarity)}</span>)}
                        </li>
                      ) : null,
                    )}
                  </ul>
                </li>
              );
            })}
          </ol>
          <p className="px-3 py-2 text-[12px]" style={{ color: "var(--ink-faint)" }}>
            Cosine similarity and distance between unit-length {what}. Distance runs from 0 (same) to 2 (opposite).
          </p>
        </>
      )}
    </section>
  );
}
