"use client";

import { useEffect, useMemo, useState } from "react";
import { forceCenter, forceCollide, forceLink, forceManyBody, forceSimulation, type SimulationNodeDatum } from "d3-force";
import { X } from "lucide-react";
import { Blobatar } from "@blobatar/react";
import { entities, type EntityDetail, type EntityGraph as EntityGraphData, type EntityKind } from "@/lib/api";

const WIDTH = 640;
const HEIGHT = 420;

const KIND_COLOR: Record<EntityKind, string> = {
  person: "var(--kind-person)",
  organisation: "var(--kind-organisation)",
  location: "var(--kind-location)",
};

const KIND_LABEL: Record<EntityKind, string> = {
  person: "Person",
  organisation: "Organisation",
  location: "Location",
};

const ALL_KINDS: EntityKind[] = ["person", "organisation", "location"];

type LaidOutNode = SimulationNodeDatum & { id: string; kind: EntityKind; name: string };
type LaidOutLink = { source: string; target: string; label: string };

function layout(graph: EntityGraphData): { nodes: LaidOutNode[]; links: LaidOutLink[] } {
  const nodes: LaidOutNode[] = graph.nodes.map((n) => ({ ...n }));
  const links: LaidOutLink[] = graph.edges.map((e) => ({ source: e.source, target: e.target, label: e.label }));

  const sim = forceSimulation(nodes)
    .force(
      "link",
      forceLink(links as unknown as { source: string; target: string }[])
        .id((d) => (d as LaidOutNode).id)
        .distance(90)
    )
    .force("charge", forceManyBody().strength(-140))
    .force("center", forceCenter(WIDTH / 2, HEIGHT / 2))
    .force("collide", forceCollide(26))
    .stop();

  for (let i = 0; i < 300; i++) sim.tick();

  return { nodes, links };
}

export default function EntityGraph() {
  const [graph, setGraph] = useState<EntityGraphData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<EntityDetail | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [visibleKinds, setVisibleKinds] = useState<Set<EntityKind>>(new Set(ALL_KINDS));

  function toggleKind(kind: EntityKind) {
    setVisibleKinds((prev) => {
      const next = new Set(prev);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });
  }

  useEffect(() => {
    entities
      .graph()
      .then(setGraph)
      .catch((e) => setError(e instanceof Error ? e.message : "Could not load the entity graph."));
  }, []);

  const laidOut = useMemo(() => (graph ? layout(graph) : null), [graph]);

  async function selectNode(id: string) {
    try {
      setSelected(await entities.get(id));
    } catch {
      setSelected(null);
    }
  }

  if (error) {
    return (
      <div className="ledger p-6 text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
        Could not load the entity graph: {error}
      </div>
    );
  }

  if (!graph) {
    return (
      <div className="ledger p-10 text-center text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
        Loading entity graph…
      </div>
    );
  }

  if (graph.nodes.length === 0) {
    return (
      <div className="ledger p-10 text-center text-[13px]" style={{ color: "var(--ink-faint)" }}>
        No entities yet — they accumulate automatically as sources sync and get extracted.
      </div>
    );
  }

  const byId = new Map(laidOut!.nodes.map((n) => [n.id, n]));
  const visibleNodes = laidOut!.nodes.filter((n) => visibleKinds.has(n.kind));
  const visibleIds = new Set(visibleNodes.map((n) => n.id));
  const visibleLinks = laidOut!.links.filter((l) => visibleIds.has(l.source) && visibleIds.has(l.target));

  return (
    <div className="flex flex-col gap-3">
      <div className="flex gap-2">
        {ALL_KINDS.map((kind) => {
          const active = visibleKinds.has(kind);
          return (
            <button
              key={kind}
              type="button"
              className="pill"
              aria-pressed={active}
              onClick={() => toggleKind(kind)}
            >
              <span
                className="w-2 h-2 rounded-full shrink-0"
                style={{ background: KIND_COLOR[kind], opacity: active ? 1 : 0.4 }}
                aria-hidden
              />
              {KIND_LABEL[kind]}
            </button>
          );
        })}
      </div>

      <div className="flex gap-4">
        <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} width="100%" height={HEIGHT} role="img" aria-label="Entity relationship graph">
          {visibleLinks.map((l, i) => {
            const a = byId.get(l.source);
            const b = byId.get(l.target);
            if (!a || !b) return null;
            return (
              <line
                key={i}
                x1={a.x}
                y1={a.y}
                x2={b.x}
                y2={b.y}
                stroke="var(--border-strong)"
                strokeWidth={1}
              />
            );
          })}
          {visibleNodes.map((n) => {
            const isHovered = hovered === n.id;
            const isSelected = selected?.id === n.id;
            const r = isHovered || isSelected ? 12 : 10;
            return (
              <g
                key={n.id}
                transform={`translate(${n.x},${n.y})`}
                style={{ cursor: "pointer" }}
                onClick={() => selectNode(n.id)}
                onMouseEnter={() => setHovered(n.id)}
                onMouseLeave={() => setHovered((h) => (h === n.id ? null : h))}
              >
                {n.kind === "person" ? (
                  <>
                    <circle
                      r={r + 1}
                      fill="var(--surface-raised)"
                      stroke={isHovered ? "var(--felt)" : "none"}
                      strokeWidth={isHovered ? 2 : 0}
                      opacity={isSelected || isHovered ? 1 : 0.9}
                    />
                    <foreignObject x={-r} y={-r} width={r * 2} height={r * 2} style={{ overflow: "visible" }}>
                      <Blobatar name={n.name || n.id} animate="hover" size={r * 2} background="circle" />
                    </foreignObject>
                  </>
                ) : (
                  <circle
                    r={r}
                    fill={KIND_COLOR[n.kind]}
                    stroke={isHovered ? "var(--felt)" : "none"}
                    strokeWidth={isHovered ? 2 : 0}
                    opacity={isSelected || isHovered ? 1 : 0.85}
                  />
                )}
                <text
                  x={0}
                  y={24}
                  textAnchor="middle"
                  fontSize={10.5}
                  fontFamily="var(--font-mono), ui-monospace, monospace"
                  fill={isHovered ? "var(--felt)" : "var(--ink-dim)"}
                >
                  {n.name.length > 16 ? `${n.name.slice(0, 15)}…` : n.name}
                </text>
              </g>
            );
          })}
        </svg>

        {selected && (
          <div className="ledger p-5 w-72 shrink-0 flex flex-col gap-4" style={{ maxHeight: HEIGHT, overflowY: "auto" }}>
            <div className="flex items-start justify-between gap-2">
              <div className="flex items-center gap-3">
                {selected.kind === "person" && (
                  <Blobatar name={selected.name || selected.id} animate="hover" size={36} background="circle" />
                )}
                <div>
                  <span className="eyebrow" style={{ color: KIND_COLOR[selected.kind] }}>
                    {selected.kind}
                  </span>
                  <div className="text-[14.5px] font-medium mt-1">{selected.name}</div>
                </div>
              </div>
              <button onClick={() => setSelected(null)} aria-label="Close" style={{ color: "var(--ink-faint)" }}>
                <X size={15} />
              </button>
            </div>

            <div>
              <div className="eyebrow mb-2">Memory</div>
              {selected.memory.length === 0 && (
                <p className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
                  Nothing recorded yet.
                </p>
              )}
              <ul className="hairline-rows">
                {selected.memory.map((m) => (
                  <li key={m.id} className="py-2 text-[12.5px]">
                    {m.text}
                  </li>
                ))}
              </ul>
            </div>

            <div>
              <div className="eyebrow mb-2">Relations</div>
              {selected.relations.length === 0 && (
                <p className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
                  No known relations.
                </p>
              )}
              <ul className="hairline-rows">
                {selected.relations.map((r) => (
                  <li key={r.id} className="py-2 text-[12px] font-mono" style={{ color: "var(--ink-dim)" }}>
                    {r.direction === "out" ? `→ ${r.label} → ${r.out}` : `← ${r.label} ← ${r.in}`}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
