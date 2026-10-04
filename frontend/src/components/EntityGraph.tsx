"use client";

import { useEffect, useRef, useState } from "react";
import {
  forceCenter,
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  type Simulation,
  type SimulationNodeDatum,
} from "d3-force";
import { Minus, Plus, RotateCcw, X } from "lucide-react";
import { Blobatar } from "@blobatar/react";
import { entities, type EntityDetail, type EntityGraph as EntityGraphData, type EntityKind } from "@/lib/api";

const WIDTH = 640;
const HEIGHT = 420;
const DRAG_THRESHOLD = 4; // px of movement before a pointerdown counts as a drag, not a click
const ZOOM_MIN = 0.5;
const ZOOM_MAX = 2.5;
const ZOOM_STEP = 0.2;

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
type LaidOutLink = { source: LaidOutNode; target: LaidOutNode; label: string };

export default function EntityGraph() {
  const [graph, setGraph] = useState<EntityGraphData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<EntityDetail | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [visibleKinds, setVisibleKinds] = useState<Set<EntityKind>>(new Set(ALL_KINDS));
  const [nodes, setNodes] = useState<LaidOutNode[]>([]);
  const [view, setView] = useState({ zoom: 1, x: 0, y: 0 });

  const svgRef = useRef<SVGSVGElement | null>(null);
  const simRef = useRef<Simulation<LaidOutNode, undefined> | null>(null);
  const linksRef = useRef<LaidOutLink[]>([]);
  const draggingRef = useRef<{ id: string; moved: boolean } | null>(null);
  const panRef = useRef<{ startX: number; startY: number; originX: number; originY: number } | null>(null);

  function zoomBy(factor: number) {
    setView((v) => ({ ...v, zoom: Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, v.zoom * factor)) }));
  }

  function resetView() {
    setView({ zoom: 1, x: 0, y: 0 });
  }

  function onCanvasWheel(e: React.WheelEvent) {
    e.preventDefault();
    zoomBy(e.deltaY < 0 ? 1.1 : 0.9);
  }

  function onCanvasPointerDown(e: React.PointerEvent<SVGSVGElement>) {
    if (e.target !== e.currentTarget) return; // a node/link handled its own pointerdown
    (e.target as SVGSVGElement).setPointerCapture(e.pointerId);
    panRef.current = { startX: e.clientX, startY: e.clientY, originX: view.x, originY: view.y };
  }

  function onCanvasPointerMove(e: React.PointerEvent<SVGSVGElement>) {
    const pan = panRef.current;
    if (!pan) return;
    setView((v) => ({
      ...v,
      x: pan.originX + (e.clientX - pan.startX) / v.zoom,
      y: pan.originY + (e.clientY - pan.startY) / v.zoom,
    }));
  }

  function onCanvasPointerUp() {
    panRef.current = null;
  }

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

  // Live force simulation: runs continuously (not a one-shot layout) so
  // dragging a node and releasing it lets physics settle it back in.
  useEffect(() => {
    if (!graph) return;

    const simNodes: LaidOutNode[] = graph.nodes.map((n) => ({ ...n }));
    const byId = new Map(simNodes.map((n) => [n.id, n]));
    const links: LaidOutLink[] = graph.edges
      .map((e) => {
        const source = byId.get(e.source);
        const target = byId.get(e.target);
        return source && target ? { source, target, label: e.label } : null;
      })
      .filter((l): l is LaidOutLink => l !== null);
    linksRef.current = links;

    const sim = forceSimulation(simNodes)
      .force("link", forceLink(links).distance(90).strength(0.5))
      .force("charge", forceManyBody().strength(-160))
      .force("center", forceCenter(WIDTH / 2, HEIGHT / 2))
      .force("collide", forceCollide(26))
      .on("tick", () => setNodes([...sim.nodes()]));

    simRef.current = sim;
    return () => {
      sim.stop();
      simRef.current = null;
    };
  }, [graph]);

  function svgPoint(e: React.PointerEvent): { x: number; y: number } {
    const svg = svgRef.current;
    const ctm = svg?.getScreenCTM();
    if (!svg || !ctm) return { x: 0, y: 0 };
    const pt = svg.createSVGPoint();
    pt.x = e.clientX;
    pt.y = e.clientY;
    const p = pt.matrixTransform(ctm.inverse());
    return { x: p.x, y: p.y };
  }

  function onNodePointerDown(e: React.PointerEvent, n: LaidOutNode) {
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    draggingRef.current = { id: n.id, moved: false };
    n.fx = n.x;
    n.fy = n.y;
    simRef.current?.alphaTarget(0.3).restart();
  }

  function onNodePointerMove(e: React.PointerEvent, n: LaidOutNode) {
    const drag = draggingRef.current;
    if (!drag || drag.id !== n.id) return;
    const { x, y } = svgPoint(e);
    if (!drag.moved && (Math.abs(x - (n.fx ?? x)) > DRAG_THRESHOLD || Math.abs(y - (n.fy ?? y)) > DRAG_THRESHOLD)) {
      drag.moved = true;
    }
    n.fx = x;
    n.fy = y;
    setNodes((prev) => [...prev]);
  }

  function onNodePointerUp(e: React.PointerEvent, n: LaidOutNode) {
    const drag = draggingRef.current;
    if (!drag || drag.id !== n.id) return;
    n.fx = null;
    n.fy = null;
    simRef.current?.alphaTarget(0);
    draggingRef.current = null;
    if (!drag.moved) selectNode(n.id);
  }

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

  const visibleNodes = nodes.filter((n) => visibleKinds.has(n.kind));
  const visibleIds = new Set(visibleNodes.map((n) => n.id));
  const visibleLinks = linksRef.current.filter((l) => visibleIds.has(l.source.id) && visibleIds.has(l.target.id));

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
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
        <span className="text-[11px]" style={{ color: "var(--ink-faint)" }}>
          Drag to rearrange
        </span>
      </div>

      <div className="flex gap-4">
        <div className="relative" style={{ width: "100%" }}>
          <svg
            ref={svgRef}
            viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
            width="100%"
            height={HEIGHT}
            role="img"
            aria-label="Entity relationship graph"
            className="ledger"
            style={{ touchAction: "none", userSelect: "none", WebkitUserSelect: "none", cursor: panRef.current ? "grabbing" : "default" }}
            onWheel={onCanvasWheel}
            onPointerDown={onCanvasPointerDown}
            onPointerMove={onCanvasPointerMove}
            onPointerUp={onCanvasPointerUp}
            onPointerCancel={onCanvasPointerUp}
          >
          <g transform={`translate(${WIDTH / 2},${HEIGHT / 2}) scale(${view.zoom}) translate(${-WIDTH / 2 + view.x},${-HEIGHT / 2 + view.y})`}>
          {visibleLinks.map((l, i) => (
            <line
              key={i}
              x1={l.source.x}
              y1={l.source.y}
              x2={l.target.x}
              y2={l.target.y}
              stroke="var(--border-strong)"
              strokeWidth={1}
            />
          ))}
          {visibleNodes.map((n) => {
            const isHovered = hovered === n.id;
            const isSelected = selected?.id === n.id;
            const isDragging = draggingRef.current?.id === n.id;
            const r = isHovered || isSelected || isDragging ? 12 : 10;
            return (
              <g
                key={n.id}
                transform={`translate(${n.x},${n.y})`}
                style={{ cursor: isDragging ? "grabbing" : "grab", touchAction: "none" }}
                onPointerDown={(e) => onNodePointerDown(e, n)}
                onPointerMove={(e) => onNodePointerMove(e, n)}
                onPointerUp={(e) => onNodePointerUp(e, n)}
                onPointerCancel={(e) => onNodePointerUp(e, n)}
                onMouseEnter={() => setHovered(n.id)}
                onMouseLeave={() => setHovered((h) => (h === n.id ? null : h))}
              >
                {n.kind === "person" ? (
                  <>
                    <circle
                      r={r + 1}
                      fill="var(--surface-raised)"
                      stroke={isHovered || isDragging ? "var(--felt)" : "none"}
                      strokeWidth={isHovered || isDragging ? 2 : 0}
                      opacity={isSelected || isHovered || isDragging ? 1 : 0.9}
                    />
                    <foreignObject x={-r} y={-r} width={r * 2} height={r * 2} style={{ overflow: "visible" }}>
                      <Blobatar name={n.name || n.id} animate="hover" size={r * 2} background="circle" />
                    </foreignObject>
                  </>
                ) : n.kind === "organisation" ? (
                  <rect
                    x={-r * 0.82}
                    y={-r * 0.82}
                    width={r * 1.64}
                    height={r * 1.64}
                    rx={4}
                    fill={KIND_COLOR[n.kind]}
                    stroke={isHovered || isDragging ? "var(--felt)" : "none"}
                    strokeWidth={isHovered || isDragging ? 2 : 0}
                    opacity={isSelected || isHovered || isDragging ? 1 : 0.85}
                  />
                ) : (
                  <path
                    d={`M0,${-r * 1.15} C${r * 0.75},${-r * 1.15} ${r * 0.95},${-r * 0.2} 0,${r * 1.05}
                        C${-r * 0.95},${-r * 0.2} ${-r * 0.75},${-r * 1.15} 0,${-r * 1.15} Z`}
                    fill={KIND_COLOR[n.kind]}
                    stroke={isHovered || isDragging ? "var(--felt)" : "none"}
                    strokeWidth={isHovered || isDragging ? 2 : 0}
                    opacity={isSelected || isHovered || isDragging ? 1 : 0.85}
                  />
                )}
                <text
                  x={0}
                  y={24}
                  textAnchor="middle"
                  fontSize={10.5}
                  fontFamily="var(--font-mono), ui-monospace, monospace"
                  fill={isHovered || isDragging ? "var(--felt)" : "var(--ink-dim)"}
                >
                  {n.name.length > 16 ? `${n.name.slice(0, 15)}…` : n.name}
                </text>
              </g>
            );
          })}
          </g>
          </svg>

          <div className="absolute bottom-3 right-3 flex gap-1">
            <button type="button" className="pill" aria-label="Zoom in" onClick={() => zoomBy(1 + ZOOM_STEP)}>
              <Plus size={13} />
            </button>
            <button type="button" className="pill" aria-label="Zoom out" onClick={() => zoomBy(1 - ZOOM_STEP)}>
              <Minus size={13} />
            </button>
            <button type="button" className="pill" aria-label="Reset view" onClick={resetView}>
              <RotateCcw size={13} />
            </button>
          </div>
        </div>

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
