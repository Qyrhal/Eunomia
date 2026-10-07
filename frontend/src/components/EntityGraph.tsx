"use client";

import { useEffect, useMemo, useState } from "react";
import { forceCenter, forceLink, forceManyBody, forceSimulation } from "d3-force-3d";
import { Pencil, Plus, Trash2, X } from "lucide-react";
import Scene3D from "./Scene3D";
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

  const points = graph.nodes
    .filter((n) => visibleKinds.has(n.kind))
    .map((n) => ({
      id: n.id,
      ...(positions.get(n.id) ?? { x: 0, y: 0, z: 0 }),
      color: KIND_COLOR[n.kind],
      size: KIND_SIZE[n.kind],
      label: n.name,
    }));

  return (
    <div className="flex flex-col gap-3 flex-1 min-h-0">
      {toolbar}
      {createModal}
      <span className="text-[11px] -mt-2" style={{ color: "var(--ink-faint)" }}>
        Drag to orbit · scroll to zoom · click a node to open it
      </span>

      <div className="flex flex-col md:flex-row gap-4 flex-1 min-h-0">
        <div className="ledger relative flex-1 min-h-0" style={{ minHeight: 420 }}>
          <Scene3D
            points={points}
            links={graph.edges}
            labels
            selectedId={selected?.id}
            onSelect={selectNode}
            ariaLabel="Entity relationship graph"
          />
        </div>

        {selected && (
          <div
            className="ledger p-5 w-full md:w-72 shrink-0 flex flex-col gap-4"
            style={{ maxHeight: 640, overflowY: "auto" }}
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
