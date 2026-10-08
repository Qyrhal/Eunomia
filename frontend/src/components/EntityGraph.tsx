"use client";

import { useEffect, useMemo, useState, type ReactNode } from "react";
import { forceCenter, forceLink, forceManyBody, forceSimulation } from "d3-force-3d";
import { ArrowLeft, ArrowRight, Pencil, Plus, Trash2, X } from "lucide-react";
import Scene3D from "./Scene3D";
import AuthorTag from "./AuthorTag";
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

const KIND_SIZE: Record<EntityKind, number> = { person: 1, organisation: 1.15, location: 1, repository: 1.4, file: 0.85, symbol: 0.7 };

// Static 3D force layout, run to completion once per graph (KISS: orbit the
// result rather than simulate live).
function layout3d(graph: EntityGraphData): Map<string, { x: number; y: number; z: number }> {
  const nodes = graph.nodes.map((n) => ({ id: n.id }));
  const ids = new Set(nodes.map((n) => n.id));
  const links = graph.edges.filter((e) => ids.has(e.source) && ids.has(e.target)).map((e) => ({ source: e.source, target: e.target }));
  forceSimulation(nodes, 3)
    .force("link", forceLink<{ id: string }>(links).id((n) => n.id).distance(40))
    .force("charge", forceManyBody().strength(-60))
    .force("center", forceCenter())
    .stop()
    .tick(300);
  return new Map(nodes.map((n: { id: string; x?: number; y?: number; z?: number }) => [n.id, { x: n.x ?? 0, y: n.y ?? 0, z: n.z ?? 0 }]));
}

// Compact age for a timestamp: "now", "4m", "3h", "2d", then a date.
function age(iso?: string): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  const s = Math.max(0, (Date.now() - t) / 1000);
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  if (s < 86400 * 30) return `${Math.floor(s / 86400)}d`;
  return new Date(t).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

// The real author of a row, if attribution was recorded. `null` means it
// predates attribution tracking, so no tag is shown.
function Author({ email, me }: { email: string | null; me: string | null }) {
  if (!email) return null;
  return <AuthorTag name={email} title={email === me ? `${email} (you)` : email} />;
}

const ICON = { size: 14, strokeWidth: 1.75 } as const;

export default function EntityGraph({ kinds, header }: { kinds?: EntityKind[]; header?: ReactNode } = {}) {
  const shownKinds = kinds ?? ALL_KINDS;
  const [graph, setGraph] = useState<EntityGraphData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [myEmail, setMyEmail] = useState<string | null>(null);
  const [myVaults, setMyVaults] = useState<Vault[]>([]);
  const [vaultId, setVaultId] = useState<string | undefined>(undefined);
  const [selected, setSelected] = useState<EntityDetail | null>(null);
  const [visibleKinds, setVisibleKinds] = useState<Set<EntityKind>>(new Set(shownKinds));
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

  useEffect(() => {
    if (!selected && !showCreate) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (showCreate) setShowCreate(false);
      else if (!editing) setSelected(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, showCreate, editing]);

  const positions = useMemo(() => (graph ? layout3d(graph) : new Map()), [graph]);

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

  // Everything floats over one full-bleed canvas: header and toolbar top left,
  // inspector top right, zoom bottom right (inside Scene3D).
  const frame = (toolbar: ReactNode, content: ReactNode, inspector?: ReactNode) => (
    <div className="absolute inset-0 overflow-hidden">
      <div className="absolute inset-0">{content}</div>
      <div className={`absolute left-0 top-0 right-0 md:right-auto p-4 md:p-5 flex flex-col gap-3 items-start pointer-events-none max-w-full [&>*]:pointer-events-auto ${inspector ? "md:max-w-[calc(100%-24rem)]" : ""}`}>
        {header}
        {toolbar}
      </div>
      {inspector}
    </div>
  );

  if (error) {
    return frame(
      null,
      <div className="h-full flex items-center justify-center p-4">
        <div
          className="max-w-sm rounded-[10px] px-4 py-3 text-[13px] flex flex-col gap-2"
          style={{ background: "var(--critical-soft)", color: "var(--critical)" }}
          role="alert"
        >
          <span>Could not load the entity graph: {error}</span>
          <button type="button" className="btn btn-sm self-start" onClick={() => window.location.reload()}>
            Try again
          </button>
        </div>
      </div>
    );
  }

  if (!graph) {
    return frame(
      <div className="panel p-1.5 flex gap-1.5" aria-busy="true" aria-label="Loading entity graph">
        {[64, 92, 80, 112].map((w) => (
          <span key={w} className="skeleton h-6" style={{ width: w }} />
        ))}
      </div>,
      <div className="h-full flex items-center justify-center" aria-hidden>
        <div className="relative w-48 h-48">
          {[
            [20, 30],
            [70, 12],
            [55, 60],
            [15, 75],
            [82, 70],
          ].map(([x, y]) => (
            <span key={`${x}${y}`} className="skeleton absolute w-3 h-3 rounded-full" style={{ left: `${x}%`, top: `${y}%`, borderRadius: 999 }} />
          ))}
        </div>
      </div>
    );
  }

  const counts = new Map<EntityKind, number>();
  graph.nodes.forEach((n) => counts.set(n.kind, (counts.get(n.kind) ?? 0) + 1));
  const nameOf = (id: string) => graph.nodes.find((n) => n.id === id)?.name ?? id;

  const createModal = showCreate && (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 fade-in"
      style={{ background: "var(--scrim)" }}
      onClick={() => setShowCreate(false)}
    >
      <form
        onSubmit={createEntity}
        onClick={(e) => e.stopPropagation()}
        className="panel pop-in p-5 w-full max-w-sm flex flex-col gap-3.5"
        style={{ boxShadow: "var(--shadow-pop)" }}
        role="dialog"
        aria-modal="true"
        aria-labelledby="new-entity-title"
      >
        <div className="flex items-center justify-between">
          <h2 id="new-entity-title" className="section-title">
            New entity
          </h2>
          <button type="button" className="btn btn-ghost btn-sm btn-icon w-[26px]" onClick={() => setShowCreate(false)} aria-label="Close">
            <X {...ICON} />
          </button>
        </div>
        <label className="label flex flex-col gap-1.5">
          Kind
          <select
            className="field h-8 px-2.5 text-[13px]"
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
        <label className="label flex flex-col gap-1.5">
          Name
          <input
            autoFocus
            className="field h-8 px-2.5 text-[13px]"
            value={createForm.name}
            onChange={(e) => setCreateForm((f) => ({ ...f, name: e.target.value }))}
            placeholder="e.g. Jordan Blake"
          />
        </label>
        <label className="label flex flex-col gap-1.5">
          Aliases (optional, comma-separated)
          <input
            className="field h-8 px-2.5 text-[13px]"
            value={createForm.aliases}
            onChange={(e) => setCreateForm((f) => ({ ...f, aliases: e.target.value }))}
            placeholder="e.g. JB, Jordy"
          />
        </label>
        {createError && (
          <p className="text-[12px] rounded-[7px] px-2.5 py-1.5" style={{ color: "var(--critical)", background: "var(--critical-soft)" }}>
            {createError}
          </p>
        )}
        <div className="flex justify-end gap-2 pt-1">
          <button type="button" className="btn btn-ghost" onClick={() => setShowCreate(false)}>
            Cancel
          </button>
          <button type="submit" disabled={creating || !createForm.name.trim()} className="btn btn-primary">
            {creating ? "Creating…" : "Create"}
          </button>
        </div>
      </form>
    </div>
  );

  const vaultSwitcher = myVaults.length > 1 && (
    <select
      className="field h-6 px-2 text-[12px]"
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
    <div className="panel p-1.5 flex flex-wrap items-center gap-1.5 max-w-full">
      {vaultSwitcher}
      {vaultSwitcher && <span className="w-px h-4 shrink-0" style={{ background: "var(--border)" }} aria-hidden />}
      <div className="flex flex-wrap gap-1 items-center" role="group" aria-label="Entity kinds">
        {shownKinds.map((kind) => {
          const active = visibleKinds.has(kind);
          return (
            <button key={kind} type="button" className="pill shrink-0" aria-pressed={active} onClick={() => toggleKind(kind)}>
              <span
                className="w-2 h-2 rounded-full shrink-0"
                style={{ background: KIND_COLOR[kind], opacity: active ? 1 : 0.35 }}
                aria-hidden
              />
              {KIND_LABEL[kind]}
              <span className="font-mono" style={{ color: "var(--ink-faint)" }}>
                {counts.get(kind) ?? 0}
              </span>
            </button>
          );
        })}
      </div>
      <span className="w-px h-4 shrink-0" style={{ background: "var(--border)" }} aria-hidden />
      {graph.nodes.length > 0 && (
        <form
          className="shrink-0"
          onSubmit={(e) => {
            e.preventDefault();
            const input = e.currentTarget.elements.namedItem("find") as HTMLInputElement;
            const q = input.value.trim().toLowerCase();
            const hit = graph.nodes.find((n) => n.name.toLowerCase() === q) ?? graph.nodes.find((n) => n.name.toLowerCase().includes(q));
            if (q && hit) {
              setVisibleKinds((prev) => new Set(prev).add(hit.kind));
              selectNode(hit.id);
              input.value = "";
            }
          }}
        >
          <input
            name="find"
            list="entity-names"
            className="field h-6 w-36 px-2 text-[12px]"
            placeholder="Find entity…"
            aria-label="Find entity"
            autoComplete="off"
          />
          <datalist id="entity-names">
            {graph.nodes.map((n) => (
              <option key={n.id} value={n.name} />
            ))}
          </datalist>
        </form>
      )}
      <button type="button" className="btn btn-sm shrink-0" onClick={() => setShowCreate(true)}>
        <Plus size={13} strokeWidth={1.75} />
        New entity
      </button>
    </div>
  );

  if (graph.nodes.length === 0) {
    return (
      <>
        {frame(
          toolbar,
          <div className="h-full flex items-center justify-center p-6">
            <div className="flex flex-col items-center gap-3 text-center max-w-xs">
              <p className="text-[13px]" style={{ color: "var(--ink-dim)" }}>
                No entities yet. They appear as sources sync and agents write memory, or add one yourself.
              </p>
              <button type="button" className="btn btn-primary btn-sm" onClick={() => setShowCreate(true)}>
                <Plus size={13} strokeWidth={1.75} />
                New entity
              </button>
            </div>
          </div>
        )}
        {createModal}
      </>
    );
  }

  const points = graph.nodes
    .filter((n) => visibleKinds.has(n.kind))
    .map((n) => ({
      id: n.id,
      ...(positions.get(n.id) ?? { x: 0, y: 0, z: 0 }),
      color: KIND_COLOR[n.kind],
      size: KIND_SIZE[n.kind],
      label: n.name,
    }));

  const lastTouched = selected
    ? selected.memory.reduce<string | undefined>((a, m) => (m.created_at && (!a || m.created_at > a) ? m.created_at : a), undefined)
    : undefined;

  const inspector = selected && (
    <aside
      aria-label={`${selected.name} details`}
      className="panel frame-selected pop-in absolute z-10 flex flex-col inset-x-2 bottom-2 max-h-[62%] md:inset-x-auto md:bottom-auto md:right-4 md:top-4 md:w-[22rem] md:max-h-[calc(100%-4.5rem)]"
      style={{ transformOrigin: "top right" }}
    >
      <div className="flex items-start justify-between gap-2 p-4 pb-3">
        <div className="flex items-center gap-3 min-w-0">
          {selected.kind === "person" && <Blobatar name={selected.name || selected.id} animate="hover" size={36} background="circle" />}
          <div className="min-w-0">
            <span className="label inline-flex items-center gap-1.5">
              <span className="w-2 h-2 rounded-full" style={{ background: KIND_COLOR[selected.kind] }} aria-hidden />
              {KIND_LABEL[selected.kind]}
            </span>
            <h2 className="text-[16px] font-semibold tracking-[-0.015em] mt-0.5 truncate">{selected.name}</h2>
          </div>
        </div>
        <div className="flex items-center gap-0.5 shrink-0 -mr-1">
          <button className="btn btn-ghost btn-sm btn-icon w-[26px]" onClick={startEditing} aria-label="Edit" title="Edit">
            <Pencil {...ICON} />
          </button>
          <button className="btn btn-ghost btn-sm btn-icon w-[26px]" onClick={deleteSelected} aria-label="Delete" title="Delete" disabled={busy}>
            <Trash2 {...ICON} />
          </button>
          <button className="btn btn-ghost btn-sm btn-icon w-[26px]" onClick={() => setSelected(null)} aria-label="Close" title="Close (Esc)">
            <X {...ICON} />
          </button>
        </div>
      </div>

      <div className="overflow-y-auto px-4 pb-4 flex flex-col gap-6">
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-[13px] items-center">
          <dt className="label">Added by</dt>
          <dd className="min-w-0">
            {selected.owner_email ? <Author email={selected.owner_email} me={myEmail} /> : <span style={{ color: "var(--ink-faint)" }}>Not recorded</span>}
          </dd>
          <dt className="label">Last memory</dt>
          <dd className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>
            {lastTouched ? `${age(lastTouched)} ago`.replace("now ago", "just now") : "None yet"}
          </dd>
          {selected.aliases.length > 0 && (
            <>
              <dt className="label">Aliases</dt>
              <dd className="truncate" style={{ color: "var(--ink-dim)" }}>
                {selected.aliases.join(", ")}
              </dd>
            </>
          )}
        </dl>

        {actionError && (
          <p className="text-[12px] rounded-[7px] px-2.5 py-1.5" style={{ color: "var(--critical)", background: "var(--critical-soft)" }} role="alert">
            {actionError}
          </p>
        )}

        {editing ? (
          <form onSubmit={saveEdit} className="flex flex-col gap-2.5">
            <label className="label flex flex-col gap-1">
              Name
              <input className="field h-8 px-2.5 text-[13px]" value={editForm.name} onChange={(e) => setEditForm((f) => ({ ...f, name: e.target.value }))} />
            </label>
            <label className="label flex flex-col gap-1">
              Aliases (comma-separated)
              <input className="field h-8 px-2.5 text-[13px]" value={editForm.aliases} onChange={(e) => setEditForm((f) => ({ ...f, aliases: e.target.value }))} />
            </label>
            <label className="label flex flex-col gap-1">
              Summary
              <textarea
                className="field px-2.5 py-1.5 text-[13px]"
                rows={3}
                value={editForm.summary}
                onChange={(e) => setEditForm((f) => ({ ...f, summary: e.target.value }))}
              />
            </label>
            <div className="flex gap-2">
              <button type="submit" disabled={busy} className="btn btn-primary btn-sm">
                Save
              </button>
              <button type="button" className="btn btn-ghost btn-sm" onClick={() => setEditing(false)}>
                Cancel
              </button>
            </div>
          </form>
        ) : (
          selected.summary && (
            <p className="text-[13px] leading-relaxed -mt-2" style={{ color: "var(--ink-dim)" }}>
              {selected.summary}
            </p>
          )
        )}

        <section className="flex flex-col gap-2">
          <h3 className="section-title flex items-baseline gap-2">
            Memory
            <span className="font-mono text-[12px] font-normal" style={{ color: "var(--ink-faint)" }}>
              {selected.memory.length}
            </span>
          </h3>
          {selected.memory.length === 0 ? (
            <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
              Nothing recorded yet. Add the first fact below.
            </p>
          ) : (
            <ul className="hairline-rows">
              {selected.memory.map((m) => (
                <li key={m.id} className="group py-2.5 flex items-start justify-between gap-2">
                  <div className="min-w-0 flex flex-col gap-1.5">
                    <span className="text-[13px] leading-snug">{m.text}</span>
                    {(m.owner_email || m.created_at) && (
                      <div className="flex items-center gap-2 min-w-0">
                        <Author email={m.owner_email} me={myEmail} />
                        {age(m.created_at) && (
                          <time className="font-mono text-[11.5px]" style={{ color: "var(--ink-faint)" }} dateTime={m.created_at} title={m.created_at}>
                            {age(m.created_at)}
                          </time>
                        )}
                      </div>
                    )}
                  </div>
                  <button
                    onClick={() => deleteMemory(m.id)}
                    aria-label="Delete memory"
                    title="Delete memory"
                    disabled={busy}
                    className="btn btn-ghost btn-sm btn-icon w-[24px] h-[24px] shrink-0 -mr-1"
                  >
                    <X size={13} strokeWidth={1.75} />
                  </button>
                </li>
              ))}
            </ul>
          )}
          <form onSubmit={submitMemory} className="flex flex-col gap-2">
            <textarea
              className="field px-2.5 py-1.5 text-[13px]"
              rows={2}
              placeholder="Add a memory about this entity…"
              value={memoryText}
              onChange={(e) => setMemoryText(e.target.value)}
            />
            <button type="submit" disabled={busy || !memoryText.trim()} className="btn btn-sm self-start">
              Add memory
            </button>
          </form>
        </section>

        <section className="flex flex-col gap-2">
          <h3 className="section-title flex items-baseline gap-2">
            Relations
            <span className="font-mono text-[12px] font-normal" style={{ color: "var(--ink-faint)" }}>
              {selected.relations.length}
            </span>
          </h3>
          {selected.relations.length === 0 ? (
            <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
              No known relations.
            </p>
          ) : (
            <ul className="hairline-rows">
              {selected.relations.map((r) => {
                const other = r.direction === "out" ? r.out : r.in;
                return (
                  <li key={r.id} className="py-2 flex items-center gap-2 min-w-0">
                    {r.direction === "out" ? (
                      <ArrowRight size={13} strokeWidth={1.75} className="shrink-0" style={{ color: "var(--ink-faint)" }} aria-label="outgoing" />
                    ) : (
                      <ArrowLeft size={13} strokeWidth={1.75} className="shrink-0" style={{ color: "var(--ink-faint)" }} aria-label="incoming" />
                    )}
                    <span className="font-mono text-[11.5px] shrink-0" style={{ color: "var(--ink-faint)" }}>
                      {r.label}
                    </span>
                    <button
                      type="button"
                      className="text-[13px] truncate text-left min-w-0 hover:underline"
                      style={{ color: "var(--accent-text)" }}
                      onClick={() => selectNode(other)}
                    >
                      {nameOf(other)}
                    </button>
                    <span className="ml-auto shrink-0">
                      <Author email={r.owner_email} me={myEmail} />
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
          <form onSubmit={submitRelation} className="flex flex-col gap-2">
            <select className="field h-8 px-2.5 text-[13px]" value={relForm.to} onChange={(e) => setRelForm((f) => ({ ...f, to: e.target.value }))} aria-label="Relate to">
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
              className="field h-8 px-2.5 text-[13px]"
              placeholder="Relation label, e.g. works_at"
              aria-label="Relation label"
              value={relForm.label}
              onChange={(e) => setRelForm((f) => ({ ...f, label: e.target.value }))}
            />
            <button type="submit" disabled={busy || !relForm.to || !relForm.label.trim()} className="btn btn-sm self-start">
              Add relation
            </button>
          </form>
        </section>
      </div>
    </aside>
  );

  return (
    <>
      {frame(
        toolbar,
        <>
          <Scene3D points={points} links={graph.edges} labels zoomControls selectedId={selected?.id} onSelect={selectNode} ariaLabel="Entity relationship graph" />
          <p className="label absolute left-5 bottom-4 hidden md:block pointer-events-none">Drag to orbit, scroll to zoom, click a node to open it</p>
        </>,
        inspector
      )}
      {createModal}
    </>
  );
}
