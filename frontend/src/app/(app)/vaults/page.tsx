"use client";

import { useEffect, useState } from "react";
import { Check, ChevronDown, ChevronRight, Copy, LogOut, Mail, Plus, Trash2, UserPlus, Users, X } from "lucide-react";
import { vaults as vaultsApi, type Vault, type VaultInvitation, type VaultMember, type VaultRole } from "@/lib/api";

function Badge({ children, tone = "dim" }: { children: React.ReactNode; tone?: "dim" | "accent" }) {
  return (
    <span
      className="text-[11px] px-2 py-0.5 rounded-full font-medium"
      style={{
        background: tone === "accent" ? "var(--surface-raised)" : "transparent",
        color: tone === "accent" ? "var(--ink)" : "var(--ink-faint)",
        border: "1px solid var(--border)",
      }}
    >
      {children}
    </span>
  );
}

function MembersPanel({ vault, onChanged }: { vault: Vault; onChanged: () => void }) {
  const [members, setMembers] = useState<VaultMember[] | null>(null);
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<VaultRole>("member");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const isOwner = vault.role === "owner";

  const load = () =>
    vaultsApi
      .members(vault.id)
      .then((r) => setMembers(r.results))
      .catch(() => setMembers([]));

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vault.id]);

  async function invite() {
    const trimmed = email.trim();
    if (!trimmed) return;
    setBusy(true);
    setError(null);
    try {
      await vaultsApi.invite(vault.id, trimmed, role);
      setEmail("");
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not invite that person.");
    } finally {
      setBusy(false);
    }
  }

  async function remove(targetEmail: string) {
    setError(null);
    try {
      await vaultsApi.removeMember(vault.id, targetEmail);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not remove that member.");
    }
  }

  async function leave() {
    setError(null);
    try {
      await vaultsApi.leave(vault.id);
      onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not leave this vault.");
    }
  }

  return (
    <div className="px-5 pb-5 flex flex-col gap-3" style={{ borderTop: "1px solid var(--border)" }}>
      <div className="flex flex-col gap-2 pt-4">
        {members === null && (
          <div className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
            Loading…
          </div>
        )}
        {members?.map((m) => (
          <div key={m.email} className="field flex items-center justify-between px-3 py-2">
            <div className="flex items-center gap-2">
              <span className="text-[13px]">{m.email}</span>
              <Badge tone={m.role === "owner" ? "accent" : "dim"}>{m.role}</Badge>
            </div>
            {isOwner && (
              <button onClick={() => remove(m.email)} aria-label="Remove member" style={{ color: "var(--critical)" }}>
                <Trash2 size={13} />
              </button>
            )}
          </div>
        ))}
      </div>

      {isOwner && (
        <div className="flex items-center gap-2">
          <input
            className="field flex-1 px-3 py-2 text-[13px]"
            placeholder="person@example.com"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && invite()}
          />
          <select
            className="field px-2 py-2 text-[12.5px]"
            value={role}
            onChange={(e) => setRole(e.target.value as VaultRole)}
          >
            <option value="member">member</option>
            <option value="owner">owner</option>
          </select>
          <button
            onClick={invite}
            disabled={busy || !email.trim()}
            className="px-3 py-2 text-[13px] font-medium rounded-xl disabled:opacity-50 flex items-center gap-1.5"
            style={{ background: "var(--felt)", color: "var(--canvas)" }}
          >
            <UserPlus size={13} />
            Invite
          </button>
        </div>
      )}

      {error && (
        <p className="text-[12px]" style={{ color: "var(--critical)" }}>
          {error}
        </p>
      )}

      {vault.kind !== "personal" && (
        <button
          onClick={leave}
          className="self-start text-[12px] flex items-center gap-1.5"
          style={{ color: "var(--ink-faint)" }}
        >
          <LogOut size={12} />
          Leave this vault
        </button>
      )}
    </div>
  );
}

function InvitationsPanel({ onChanged }: { onChanged: () => void }) {
  const [invitations, setInvitations] = useState<VaultInvitation[] | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () =>
    vaultsApi
      .invitations()
      .then((r) => setInvitations(r.results))
      .catch(() => setInvitations([]));

  useEffect(() => {
    load();
  }, []);

  async function accept(vaultId: string) {
    setBusyId(vaultId);
    setError(null);
    try {
      await vaultsApi.acceptInvitation(vaultId);
      await load();
      onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not accept this invitation.");
    } finally {
      setBusyId(null);
    }
  }

  async function decline(vaultId: string) {
    setBusyId(vaultId);
    setError(null);
    try {
      await vaultsApi.declineInvitation(vaultId);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not decline this invitation.");
    } finally {
      setBusyId(null);
    }
  }

  if (!invitations || invitations.length === 0) return null;

  return (
    <section className="flex flex-col gap-3">
      <div className="eyebrow flex items-center gap-1.5">
        <Mail size={12} /> Invitations
      </div>
      {invitations.map((inv) => (
        <div key={inv.vault_id} className="ledger flex items-center justify-between px-5 py-4">
          <div className="flex items-center gap-2">
            <span className="text-[14px] font-medium">{inv.vault_name}</span>
            <Badge tone="accent">{inv.vault_kind}</Badge>
            <Badge>{inv.role}</Badge>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => accept(inv.vault_id)}
              disabled={busyId === inv.vault_id}
              className="px-3 py-1.5 text-[12.5px] font-medium rounded-xl disabled:opacity-50 flex items-center gap-1.5"
              style={{ background: "var(--felt)", color: "var(--canvas)" }}
            >
              <Check size={13} />
              Join
            </button>
            <button
              onClick={() => decline(inv.vault_id)}
              disabled={busyId === inv.vault_id}
              aria-label="Decline invitation"
              className="px-3 py-1.5 text-[12.5px] rounded-xl disabled:opacity-50 flex items-center gap-1.5"
              style={{ color: "var(--ink-faint)", border: "1px solid var(--border)" }}
            >
              <X size={13} />
              Decline
            </button>
          </div>
        </div>
      ))}
      {error && (
        <p className="text-[12px]" style={{ color: "var(--critical)" }}>
          {error}
        </p>
      )}
    </section>
  );
}

function VaultCard({ vault, onChanged }: { vault: Vault; onChanged: () => void }) {
  const [open, setOpen] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [cloning, setCloning] = useState(false);
  const [cloneName, setCloneName] = useState(`${vault.name} (copy)`);
  const [cloneBusy, setCloneBusy] = useState(false);
  const [cloneError, setCloneError] = useState<string | null>(null);

  async function doDelete() {
    await vaultsApi.delete(vault.id);
    onChanged();
  }

  async function doClone() {
    setCloneBusy(true);
    setCloneError(null);
    try {
      await vaultsApi.clone(vault.id, cloneName.trim() || undefined);
      setCloning(false);
      onChanged();
    } catch (e) {
      setCloneError(e instanceof Error ? e.message : "Could not clone this vault.");
    } finally {
      setCloneBusy(false);
    }
  }

  return (
    <div className="ledger overflow-hidden">
      <button
        onClick={() => setOpen((o) => !o)}
        className="w-full flex items-center gap-3 px-5 py-4 text-left"
      >
        {open ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[14px] font-medium">{vault.name}</span>
            <Badge tone="accent">{vault.kind}</Badge>
            <Badge>{vault.role}</Badge>
          </div>
        </div>
        {!confirmingDelete && !cloning && (
          <span
            role="button"
            onClick={(e) => {
              e.stopPropagation();
              setCloneName(`${vault.name} (copy)`);
              setCloning(true);
            }}
            aria-label="Clone vault"
            title="Clone into a new vault"
            style={{ color: "var(--ink-faint)" }}
          >
            <Copy size={14} />
          </span>
        )}
        {vault.kind === "org" && vault.role === "owner" && !confirmingDelete && !cloning && (
          <span
            role="button"
            onClick={(e) => {
              e.stopPropagation();
              setConfirmingDelete(true);
            }}
            aria-label="Delete vault"
            style={{ color: "var(--ink-faint)" }}
          >
            <Trash2 size={14} />
          </span>
        )}
        {confirmingDelete && (
          <div className="flex items-center gap-2" onClick={(e) => e.stopPropagation()}>
            <span className="text-[12px]" style={{ color: "var(--critical)" }}>
              Delete {vault.name}?
            </span>
            <button
              onClick={doDelete}
              className="text-[12px] font-medium px-2 py-1 rounded-lg"
              style={{ background: "var(--critical)", color: "var(--canvas)" }}
            >
              Confirm
            </button>
            <button onClick={() => setConfirmingDelete(false)} className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
              Cancel
            </button>
          </div>
        )}
      </button>
      {cloning && (
        <div className="px-5 pb-4 flex items-center gap-2" style={{ borderTop: "1px solid var(--border)" }}>
          <input
            autoFocus
            className="field flex-1 px-3 py-2 text-[13px] mt-4"
            value={cloneName}
            onChange={(e) => setCloneName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && doClone()}
          />
          <button
            onClick={doClone}
            disabled={cloneBusy}
            className="mt-4 px-4 py-2 text-[13px] font-medium rounded-xl disabled:opacity-50"
            style={{ background: "var(--felt)", color: "var(--canvas)" }}
          >
            {cloneBusy ? "Cloning…" : "Clone"}
          </button>
          <button onClick={() => setCloning(false)} className="mt-4 text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
            Cancel
          </button>
          {cloneError && (
            <p className="text-[12px] mt-4" style={{ color: "var(--critical)" }}>
              {cloneError}
            </p>
          )}
        </div>
      )}
      {open && <MembersPanel vault={vault} onChanged={onChanged} />}
    </div>
  );
}

function CreateVaultCard({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  async function create() {
    const trimmed = name.trim();
    if (!trimmed) return;
    setBusy(true);
    try {
      await vaultsApi.create(trimmed, "org");
      setName("");
      setOpen(false);
      onCreated();
    } finally {
      setBusy(false);
    }
  }

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="ledger px-5 py-4 flex items-center gap-2.5 text-[13.5px] font-medium"
        style={{ color: "var(--ink-dim)" }}
      >
        <Plus size={15} />
        New org vault
      </button>
    );
  }

  return (
    <div className="ledger px-5 py-4 flex items-center gap-2">
      <input
        autoFocus
        className="field flex-1 px-3 py-2 text-[13px]"
        placeholder="Vault name (e.g. Acme Team)"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && create()}
      />
      <button
        onClick={create}
        disabled={busy || !name.trim()}
        className="px-4 py-2 text-[13px] font-medium rounded-xl disabled:opacity-50"
        style={{ background: "var(--felt)", color: "var(--canvas)" }}
      >
        Create
      </button>
      <button onClick={() => setOpen(false)} className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
        Cancel
      </button>
    </div>
  );
}

export default function VaultsPage() {
  const [vaults, setVaults] = useState<Vault[] | null>(null);

  const load = () =>
    vaultsApi
      .list()
      .then((r) => setVaults(r.results))
      .catch(() => setVaults([]));

  useEffect(() => {
    load();
  }, []);

  if (!vaults) return null;

  const personal = vaults.filter((v) => v.kind === "personal");
  const org = vaults.filter((v) => v.kind !== "personal");

  return (
    <div className="max-w-2xl flex flex-col gap-8">
      <div>
        <div className="eyebrow mb-2">Vaults</div>
        <h1 className="font-display text-3xl">Who sees what</h1>
        <p className="text-[13px] mt-2" style={{ color: "var(--ink-faint)" }}>
          A vault is a scope of shared entities and memory. Your personal vault is always
          private; invite someone to an org vault to share knowledge with them.
        </p>
      </div>

      <InvitationsPanel onChanged={load} />

      <section className="flex flex-col gap-3">
        <div className="eyebrow flex items-center gap-1.5">
          <Users size={12} /> Personal
        </div>
        {personal.map((v) => (
          <VaultCard key={v.id} vault={v} onChanged={load} />
        ))}
      </section>

      <section className="flex flex-col gap-3">
        <div className="eyebrow">Shared &amp; organisation</div>
        {org.length === 0 && (
          <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
            No shared vaults yet — create one, or wait for an invite.
          </p>
        )}
        {org.map((v) => (
          <VaultCard key={v.id} vault={v} onChanged={load} />
        ))}
        <CreateVaultCard onCreated={load} />
      </section>
    </div>
  );
}
