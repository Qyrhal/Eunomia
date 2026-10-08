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
import { Maximize2, Minus, Pencil, Plus, RotateCcw, Trash2, X } from "lucide-react";
import { Blobatar } from "@blobatar/react";
import {
  auth,
  entities,
  vaults as vaultsApi,
  type EntityDetail,
  type EntityGraph as EntityGraphData,
  type EntityKind,
  type Vault,
} from "@/lib/api";

const DEFAULT_WIDTH = 640;
const DEFAULT_HEIGHT = 420;
const DRAG_THRESHOLD = 4; // px of movement before a pointerdown counts as a drag, not a click
const ZOOM_MIN = 0.5;
const ZOOM_MAX = 2.5;
const ZOOM_STEP = 0.2;

const KIND_COLOR: Record<EntityKind, string> = {
  person: "var(--kind-person)",
  organisation: "var(--kind-organisation)",
  location: "var(--kind-location)",
  repository: "var(--kind-repository)",
  file: "var(--kind-file)",
  symbol: "var(--kind-symbol)",
};

const KIND_LABEL: Record<EntityKind, string> = {
  person: "Person",
  organisation: "Organisation",
  location: "Location",
  repository: "Repository",
  file: "File",
  symbol: "Symbol",
};

const ALL_KINDS: EntityKind[] = ["person", "organisation", "location", "repository", "file", "symbol"];

type LaidOutNode = SimulationNodeDatum & { id: string; kind: EntityKind; name: string; owner_email: string | null };
type LaidOutLink = { source: LaidOutNode; target: LaidOutNode; label: string; owner_email: string | null };

const FIT_PADDING = 50;

// zoom/pan that centers and fits `nodeList`'s bounding box into a `w`x`h`
// viewport -- shared by the auto-fit-on-load effect and the manual "Fit"
// button, so they can't drift out of sync with each other.
function computeFit(nodeList: { x?: number; y?: number }[], w: number, h: number): { zoom: number; x: number; y: number } {
  const pts = nodeList.filter((n): n is { x: number; y: number } => typeof n.x === "number" && typeof n.y === "number");
  if (pts.length === 0) return { zoom: 1, x: 0, y: 0 };
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (const n of pts) {
    minX = Math.min(minX, n.x);
    maxX = Math.max(maxX, n.x);
    minY = Math.min(minY, n.y);
    maxY = Math.max(maxY, n.y);
  }
  const bw = Math.max(maxX - minX, 1);
  const bh = Math.max(maxY - minY, 1);
  const zoomFit = Math.min((w - 2 * FIT_PADDING) / bw, (h - 2 * FIT_PADDING) / bh);
  const zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoomFit));
  return { zoom, x: w / 2 - (minX + maxX) / 2, y: h / 2 - (minY + maxY) / 2 };
}

// "you"/email for whoever wrote a row -- `null` means it predates attribution
// tracking (relations created before `relates_to.owner` existed).
function attribution(ownerEmail: string | null, myEmail: string | null): string {
  if (!ownerEmail) return "unknown";
  return myEmail && ownerEmail === myEmail ? "you" : ownerEmail;
}

export default function EntityGraph({ kinds }: { kinds?: EntityKind[] } = {}) {
  const shownKinds = kinds ?? ALL_KINDS;
  const [graph, setGraph] = useState<EntityGraphData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [myEmail, setMyEmail] = useState<string | null>(null);
  const [myVaults, setMyVaults] = useState<Vault[]>([]);
  const [vaultId, setVaultId] = useState<string | undefined>(undefined);
  const [selected, setSelected] = useState<EntityDetail | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [visibleKinds, setVisibleKinds] = useState<Set<EntityKind>>(new Set(shownKinds));
  const [nodes, setNodes] = useState<LaidOutNode[]>([]);
  const [view, setView] = useState({ zoom: 1, x: 0, y: 0 });
  const [dims, setDims] = useState({ w: DEFAULT_WIDTH, h: DEFAULT_HEIGHT });
  const [panning, setPanning] = useState(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);

  const [showCreate, setShowCreate] = useState(false);
  const [createForm, setCreateForm] = useState({ kind: shownKinds[0], name: "", aliases: "" });
  const [createError, setCreateError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const [editing, setEditing] = useState(false);
  const [editForm, setEditForm] = useState({ name: "", aliases: "", summary: "" });
  const [memoryText, setMemoryText] = useState("");
  const [relForm, setRelForm] = useState({ to: "", label: "" });
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const svgRef = useRef<SVGSVGElement | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const simRef = useRef<Simulation<LaidOutNode, undefined> | null>(null);
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
    setPanning(true);
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
    setPanning(false);
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
    auth
      .me()
      .then((me) => setMyEmail(me.email))
      .catch(() => {});
    vaultsApi
      .list()
      .then((r) => setMyVaults(r.results))
      .catch(() => setMyVaults([]));
  }, []);

  useEffect(() => {
    entities
      .graph(kinds ? { kinds, vaultId } : { vaultId })
      .then(setGraph)
      .catch((e) => setError(e instanceof Error ? e.message : "Could not load the entity graph."));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vaultId]);

  // Fill whatever height the page gives the canvas, tracked live so the
  // simulation's center force and the viewBox stay in sync on resize.
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const box = entries[0]?.contentRect;
      if (!box || box.width < 10 || box.height < 10) return;
      setDims({ w: Math.round(box.width), h: Math.round(box.height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
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
        return source && target ? { source, target, label: e.label, owner_email: e.owner_email } : null;
      })
      .filter((l): l is LaidOutLink => l !== null);

    const sim = forceSimulation(simNodes)
      .force("link", forceLink(links).distance(90).strength(0.5))
      .force("charge", forceManyBody().strength(-160))
      .force("center", forceCenter(dims.w / 2, dims.h / 2))
      .force("collide", forceCollide(26))
      .on("tick", () => setNodes([...sim.nodes()]))
      // auto-fit once the layout settles, so a reload/vault-switch never
      // leaves nodes scattered outside the viewport
      .on("end", () => setView(computeFit(sim.nodes(), dims.w, dims.h)));

    simRef.current = sim;
    return () => {
      sim.stop();
      simRef.current = null;
    };
    // dims intentionally excluded: resizing re-centers via the effect below
    // rather than rebuilding the whole simulation (which would reset drag state).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graph]);

  // Recenter (without rebuilding) when the canvas size changes.
  useEffect(() => {
    simRef.current?.force("center", forceCenter(dims.w / 2, dims.h / 2)).alpha(0.3).restart();
  }, [dims.w, dims.h]);

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
    setDraggingId(n.id);
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
    setDraggingId(null);
    if (!drag.moved) selectNode(n.id);
  }

  async function selectNode(id: string) {
    setEditing(false);
    setMemoryText("");
    setRelForm({ to: "", label: "" });
    setActionError(null);
    try {
      setSelected(await entities.get(id));
    } catch {
      setSelected(null);
    }
  }

  async function refreshGraph() {
    const g = await entities.graph(kinds ? { kinds, vaultId } : { vaultId }).catch(() => null);
    if (g) setGraph(g);
  }

  async function createEntity(e: React.FormEvent) {
    e.preventDefault();
    const name = createForm.name.trim();
    if (!name) return;
    setCreating(true);
    setCreateError(null);
    try {
      const aliases = createForm.aliases
        .split(",")
        .map((a) => a.trim())
        .filter(Boolean);
      const created = await entities.create({
        kind: createForm.kind,
        name,
        aliases: aliases.length ? aliases : undefined,
        vault_id: vaultId,
      });
      setShowCreate(false);
      setCreateForm({ kind: shownKinds[0], name: "", aliases: "" });
      await refreshGraph();
      await selectNode(created.id);
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : "Could not create the entity.");
    } finally {
      setCreating(false);
    }
  }

  function startEditing() {
    if (!selected) return;
    setEditForm({ name: selected.name, aliases: selected.aliases.join(", "), summary: selected.summary });
    setActionError(null);
    setEditing(true);
  }

  async function saveEdit(e: React.FormEvent) {
    e.preventDefault();
    if (!selected) return;
    setBusy(true);
    setActionError(null);
    try {
      const aliases = editForm.aliases
        .split(",")
        .map((a) => a.trim())
        .filter(Boolean);
      await entities.update(selected.id, { name: editForm.name.trim(), aliases, summary: editForm.summary.trim() });
      setEditing(false);
      await refreshGraph();
      await selectNode(selected.id);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Could not save changes.");
    } finally {
      setBusy(false);
    }
  }

  async function deleteSelected() {
    if (!selected) return;
    if (!window.confirm(`Delete ${selected.name}? This also removes its memory and relations.`)) return;
    setBusy(true);
    setActionError(null);
    try {
      await entities.delete(selected.id);
      setSelected(null);
      await refreshGraph();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Could not delete this entity.");
      setBusy(false);
    }
  }

  async function submitMemory(e: React.FormEvent) {
    e.preventDefault();
    if (!selected || !memoryText.trim()) return;
    setBusy(true);
    setActionError(null);
    try {
      await entities.addMemory(selected.id, { text: memoryText.trim() });
      setMemoryText("");
      await selectNode(selected.id);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Could not add that memory.");
    } finally {
      setBusy(false);
    }
  }

  async function deleteMemory(memoryId: string) {
    if (!selected) return;
    if (!window.confirm("Delete this memory?")) return;
    setBusy(true);
    setActionError(null);
    try {
      await entities.deleteMemory(memoryId);
      await selectNode(selected.id);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Could not delete that memory.");
    } finally {
      setBusy(false);
    }
  }

  async function submitRelation(e: React.FormEvent) {
    e.preventDefault();
    if (!selected || !relForm.to || !relForm.label.trim()) return;
    setBusy(true);
    setActionError(null);
    try {
      await entities.addRelation(selected.id, { to_id: relForm.to, label: relForm.label.trim() });
      setRelForm({ to: "", label: "" });
      await refreshGraph();
      await selectNode(selected.id);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Could not add that relation.");
    } finally {
      setBusy(false);
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

  const createModal = showCreate && (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0,0,0,0.45)" }}
      onClick={() => setShowCreate(false)}
    >
      <form
        onSubmit={createEntity}
        onClick={(e) => e.stopPropagation()}
        className="ledger p-5 w-full max-w-sm flex flex-col gap-3.5"
      >
        <div className="flex items-center justify-between">
          <span className="eyebrow">New entity</span>
          <button type="button" onClick={() => setShowCreate(false)} aria-label="Close" style={{ color: "var(--ink-faint)" }}>
            <X size={15} />
          </button>
        </div>
        <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
          Kind
          <select
            className="field px-3 py-2 text-[13px]"
            value={createForm.kind}
            onChange={(e) => setCreateForm((f) => ({ ...f, kind: e.target.value as EntityKind }))}
          >
            {shownKinds.map((k) => (
              <option key={k} value={k}>
                {KIND_LABEL[k]}
              </option>
            ))}
          </select>
        </label>
        <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
          Name
          <input
            autoFocus
            className="field px-3 py-2 text-[13px]"
            value={createForm.name}
            onChange={(e) => setCreateForm((f) => ({ ...f, name: e.target.value }))}
            placeholder="e.g. Jordan Blake"
          />
        </label>
        <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
          Aliases (optional, comma-separated)
          <input
            className="field px-3 py-2 text-[13px]"
            value={createForm.aliases}
            onChange={(e) => setCreateForm((f) => ({ ...f, aliases: e.target.value }))}
            placeholder="e.g. JB, Jordy"
          />
        </label>
        {createError && (
          <p className="text-[12px]" style={{ color: "var(--critical)" }}>
            {createError}
          </p>
        )}
        <button
          type="submit"
          disabled={creating || !createForm.name.trim()}
          className="self-start px-4 py-2 text-[13px] font-medium rounded-xl disabled:opacity-50"
          style={{ background: "var(--felt)", color: "var(--canvas)" }}
        >
          {creating ? "Creating…" : "Create"}
        </button>
      </form>
    </div>
  );

  const vaultSwitcher = myVaults.length > 1 && (
    <select
      className="field px-2.5 py-1.5 text-[12.5px]"
      value={vaultId ?? ""}
      onChange={(e) => {
        setVaultId(e.target.value || undefined);
        setSelected(null);
      }}
      aria-label="Vault"
    >
      {myVaults.map((v) => (
        <option key={v.id} value={v.kind === "personal" ? "" : v.id}>
          {v.kind === "personal" ? "Personal" : v.name}
        </option>
      ))}
    </select>
  );

  const toolbar = (
    <div className="flex items-center justify-between gap-2 flex-wrap">
      <div className="flex gap-2 flex-wrap items-center">
        {vaultSwitcher}
        {shownKinds.map((kind) => {
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
      <button type="button" className="pill" onClick={() => setShowCreate(true)}>
        <Plus size={12} />
        New entity
      </button>
    </div>
  );

  if (graph.nodes.length === 0) {
    return (
      <div className="flex flex-col gap-3 flex-1 min-h-0">
        {toolbar}
        {createModal}
        <div className="ledger p-10 text-center text-[13px] flex-1" style={{ color: "var(--ink-faint)" }}>
          No entities yet — they accumulate automatically as sources sync and get extracted, or add one yourself.
        </div>
      </div>
    );
  }

  const visibleNodes = nodes.filter((n) => visibleKinds.has(n.kind));
  // Built from `nodes` (the simulation's own node objects), so link
  // endpoints move with the nodes on every tick.
  const nodeById = new Map(visibleNodes.map((n) => [n.id, n]));
  const visibleLinks = (graph?.edges ?? []).flatMap((e) => {
    const source = nodeById.get(e.source);
    const target = nodeById.get(e.target);
    return source && target ? [{ source, target }] : [];
  });

  return (
    <div className="flex flex-col gap-3 flex-1 min-h-0">
      {toolbar}
      {createModal}
      <span className="text-[11px] -mt-2" style={{ color: "var(--ink-faint)" }}>
        Drag to rearrange
      </span>

      <div className="flex flex-col md:flex-row gap-4 flex-1 min-h-0">
        <div ref={wrapRef} className="relative flex-1 min-h-0" style={{ minHeight: 420 }}>
          <svg
            ref={svgRef}
            viewBox={`0 0 ${dims.w} ${dims.h}`}
            width="100%"
            height="100%"
            role="img"
            aria-label="Entity relationship graph"
            className="ledger"
            style={{ touchAction: "none", userSelect: "none", WebkitUserSelect: "none", cursor: panning ? "grabbing" : "default" }}
            onWheel={onCanvasWheel}
            onPointerDown={onCanvasPointerDown}
            onPointerMove={onCanvasPointerMove}
            onPointerUp={onCanvasPointerUp}
            onPointerCancel={onCanvasPointerUp}
          >
          <g transform={`translate(${dims.w / 2},${dims.h / 2}) scale(${view.zoom}) translate(${-dims.w / 2 + view.x},${-dims.h / 2 + view.y})`}>
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
            const isDragging = draggingId === n.id;
            const r = isHovered || isSelected || isDragging ? 16 : 14;
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
                <title>{`${n.name} — added by ${attribution(n.owner_email, myEmail)}`}</title>
                {n.kind === "person" ? (
                  <>
                    {(isHovered || isDragging) && (
                      <circle
                        r={r + 1}
                        fill="none"
                        stroke="var(--felt)"
                        strokeWidth={2}
                      />
                    )}
                    <foreignObject x={-r} y={-r} width={r * 2} height={r * 2} style={{ overflow: "visible" }}>
                      <Blobatar name={n.name || n.id} animate="hover" size={r * 2} />
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
                ) : n.kind === "location" ? (
                  <path
                    d={`M0,${-r * 1.15} C${r * 0.75},${-r * 1.15} ${r * 0.95},${-r * 0.2} 0,${r * 1.05}
                        C${-r * 0.95},${-r * 0.2} ${-r * 0.75},${-r * 1.15} 0,${-r * 1.15} Z`}
                    fill={KIND_COLOR[n.kind]}
                    stroke={isHovered || isDragging ? "var(--felt)" : "none"}
                    strokeWidth={isHovered || isDragging ? 2 : 0}
                    opacity={isSelected || isHovered || isDragging ? 1 : 0.85}
                  />
                ) : n.kind === "repository" ? (
                  // a larger rounded square -- the "container" of a file/symbol tree.
                  <rect
                    x={-r * 0.95}
                    y={-r * 0.95}
                    width={r * 1.9}
                    height={r * 1.9}
                    rx={6}
                    fill={KIND_COLOR[n.kind]}
                    stroke={isHovered || isDragging ? "var(--felt)" : "none"}
                    strokeWidth={isHovered || isDragging ? 2 : 0}
                    opacity={isSelected || isHovered || isDragging ? 1 : 0.85}
                  />
                ) : n.kind === "file" ? (
                  // a small plain square -- one level down from a repository.
                  <rect
                    x={-r * 0.6}
                    y={-r * 0.6}
                    width={r * 1.2}
                    height={r * 1.2}
                    rx={1.5}
                    fill={KIND_COLOR[n.kind]}
                    stroke={isHovered || isDragging ? "var(--felt)" : "none"}
                    strokeWidth={isHovered || isDragging ? 2 : 0}
                    opacity={isSelected || isHovered || isDragging ? 1 : 0.85}
                  />
                ) : (
                  // symbol -- a small diamond, one level down from a file.
                  <rect
                    x={-r * 0.62}
                    y={-r * 0.62}
                    width={r * 1.24}
                    height={r * 1.24}
                    fill={KIND_COLOR[n.kind]}
                    stroke={isHovered || isDragging ? "var(--felt)" : "none"}
                    strokeWidth={isHovered || isDragging ? 2 : 0}
                    opacity={isSelected || isHovered || isDragging ? 1 : 0.85}
                    transform="rotate(45)"
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
            <button
              type="button"
              className="pill"
              aria-label="Fit to view"
              title="Fit to view"
              onClick={() => setView(computeFit(visibleNodes, dims.w, dims.h))}
            >
              <Maximize2 size={13} />
            </button>
            <button type="button" className="pill" aria-label="Reset view" onClick={resetView}>
              <RotateCcw size={13} />
            </button>
          </div>
        </div>

        {selected && (
          <div
            className="ledger p-5 w-full md:w-72 shrink-0 flex flex-col gap-4"
            style={{ maxHeight: dims.h, overflowY: "auto" }}
          >
            <div className="flex items-start justify-between gap-2">
              <div className="flex items-center gap-3 min-w-0">
                {selected.kind === "person" && (
                  <Blobatar name={selected.name || selected.id} animate="hover" size={36} background="circle" />
                )}
                <div className="min-w-0">
                  <span className="eyebrow" style={{ color: KIND_COLOR[selected.kind] }}>
                    {selected.kind}
                  </span>
                  <div className="text-[14.5px] font-medium mt-1 truncate">{selected.name}</div>
                  <div className="text-[11px] mt-0.5" style={{ color: "var(--ink-faint)" }}>
                    added by {attribution(selected.owner_email, myEmail)}
                  </div>
                </div>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <button onClick={startEditing} aria-label="Edit" style={{ color: "var(--ink-faint)" }}>
                  <Pencil size={14} />
                </button>
                <button onClick={deleteSelected} aria-label="Delete" disabled={busy} style={{ color: "var(--ink-faint)" }}>
                  <Trash2 size={14} />
                </button>
                <button onClick={() => setSelected(null)} aria-label="Close" style={{ color: "var(--ink-faint)" }}>
                  <X size={15} />
                </button>
              </div>
            </div>

            {actionError && (
              <p className="text-[12px]" style={{ color: "var(--critical)" }}>
                {actionError}
              </p>
            )}

            {editing ? (
              <form onSubmit={saveEdit} className="flex flex-col gap-2.5">
                <label className="text-[11.5px] flex flex-col gap-1" style={{ color: "var(--ink-dim)" }}>
                  Name
                  <input
                    className="field px-2.5 py-1.5 text-[12.5px]"
                    value={editForm.name}
                    onChange={(e) => setEditForm((f) => ({ ...f, name: e.target.value }))}
                  />
                </label>
                <label className="text-[11.5px] flex flex-col gap-1" style={{ color: "var(--ink-dim)" }}>
                  Aliases (comma-separated)
                  <input
                    className="field px-2.5 py-1.5 text-[12.5px]"
                    value={editForm.aliases}
                    onChange={(e) => setEditForm((f) => ({ ...f, aliases: e.target.value }))}
                  />
                </label>
                <label className="text-[11.5px] flex flex-col gap-1" style={{ color: "var(--ink-dim)" }}>
                  Summary
                  <textarea
                    className="field px-2.5 py-1.5 text-[12.5px]"
                    rows={3}
                    value={editForm.summary}
                    onChange={(e) => setEditForm((f) => ({ ...f, summary: e.target.value }))}
                  />
                </label>
                <div className="flex gap-2">
                  <button
                    type="submit"
                    disabled={busy}
                    className="px-3 py-1.5 text-[12px] font-medium rounded-lg disabled:opacity-50"
                    style={{ background: "var(--felt)", color: "var(--canvas)" }}
                  >
                    Save
                  </button>
                  <button type="button" className="pill" onClick={() => setEditing(false)}>
                    Cancel
                  </button>
                </div>
              </form>
            ) : (
              selected.summary && (
                <p className="text-[12.5px]" style={{ color: "var(--ink-dim)" }}>
                  {selected.summary}
                </p>
              )
            )}

            <div>
              <div className="eyebrow mb-2">Memory</div>
              {selected.memory.length === 0 && (
                <p className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
                  Nothing recorded yet.
                </p>
              )}
              <ul className="hairline-rows">
                {selected.memory.map((m) => (
                  <li key={m.id} className="py-2 text-[12.5px] flex items-start justify-between gap-2">
                    <div>
                      <span>{m.text}</span>
                      <div className="text-[10.5px] mt-0.5" style={{ color: "var(--ink-faint)" }}>
                        added by {attribution(m.owner_email, myEmail)}
                      </div>
                    </div>
                    <button
                      onClick={() => deleteMemory(m.id)}
                      aria-label="Delete memory"
                      disabled={busy}
                      className="shrink-0"
                      style={{ color: "var(--ink-faint)" }}
                    >
                      <X size={12} />
                    </button>
                  </li>
                ))}
              </ul>
              <form onSubmit={submitMemory} className="flex flex-col gap-2 mt-2.5">
                <textarea
                  className="field px-2.5 py-1.5 text-[12.5px]"
                  rows={2}
                  placeholder="Add a memory about this entity…"
                  value={memoryText}
                  onChange={(e) => setMemoryText(e.target.value)}
                />
                <button
                  type="submit"
                  disabled={busy || !memoryText.trim()}
                  className="self-start pill disabled:opacity-50"
                >
                  Add memory
                </button>
              </form>
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
                  <li key={r.id} className="py-2 text-[12px]">
                    <span className="font-mono" style={{ color: "var(--ink-dim)" }}>
                      {r.direction === "out" ? `→ ${r.label} → ${r.out}` : `← ${r.label} ← ${r.in}`}
                    </span>
                    <div className="text-[10.5px] mt-0.5" style={{ color: "var(--ink-faint)" }}>
                      added by {attribution(r.owner_email, myEmail)}
                    </div>
                  </li>
                ))}
              </ul>
              <form onSubmit={submitRelation} className="flex flex-col gap-2 mt-2.5">
                <select
                  className="field px-2.5 py-1.5 text-[12.5px]"
                  value={relForm.to}
                  onChange={(e) => setRelForm((f) => ({ ...f, to: e.target.value }))}
                >
                  <option value="">Relate to…</option>
                  {graph.nodes
                    .filter((n) => n.id !== selected.id)
                    .map((n) => (
                      <option key={n.id} value={n.id}>
                        {KIND_LABEL[n.kind]}: {n.name}
                      </option>
                    ))}
                </select>
                <input
                  className="field px-2.5 py-1.5 text-[12.5px]"
                  placeholder="Relation label, e.g. works_at"
                  value={relForm.label}
                  onChange={(e) => setRelForm((f) => ({ ...f, label: e.target.value }))}
                />
                <button
                  type="submit"
                  disabled={busy || !relForm.to || !relForm.label.trim()}
                  className="self-start pill disabled:opacity-50"
                >
                  Add relation
                </button>
              </form>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
