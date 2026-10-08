"use client";

import { useEffect, useState, useSyncExternalStore } from "react";
import {
  Check,
  ChevronRight,
  Combine,
  Copy,
  Lock,
  LogOut,
  Mail,
  Plus,
  Trash2,
  UserPlus,
  Users,
  X,
} from "lucide-react";
import AuthorTag from "@/components/AuthorTag";
import {
  auth,
  vaults as vaultsApi,
  type Vault,
  type VaultInvitation,
  type VaultMember,
  type VaultRole,
} from "@/lib/api";

const ICON = { size: 14, strokeWidth: 1.75 } as const;
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

function errText(e: unknown, fallback: string) {
  return e instanceof Error && e.message ? e.message : fallback;
}

function ago(iso: string | null): string | null {
  if (!iso) return null;
  const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

function RoleChip({ role }: { role: VaultRole }) {
  return (
    <span
      className="inline-flex items-center h-5 px-1.5 rounded-[4px] text-[11.5px] font-medium shrink-0"
      style={{
        border: "var(--hair) solid var(--border-strong)",
        color: role === "owner" ? "var(--ink)" : "var(--ink-dim)",
      }}
    >
      {role === "owner" ? "Owner" : "Member"}
    </span>
  );
}

function ErrorLine({ children }: { children: React.ReactNode }) {
  return (
    <p
      role="alert"
      className="text-[12.5px] px-3 py-2 rounded-[7px]"
      style={{ color: "var(--critical)", background: "var(--critical-soft)" }}
    >
      {children}
    </p>
  );
}

/* Overlapping identity tags, Figma-cursor style. Real member emails only. */
function MemberStack({ members }: { members: VaultMember[] | null }) {
  if (!members)
    return <span className="skeleton hidden sm:block h-5 w-20" aria-hidden />;
  const shown = members.slice(0, 3);
  const rest = members.length - shown.length;
  return (
    <span className="hidden sm:flex items-center gap-1 min-w-0" aria-hidden>
      {shown.map((m) => (
        <span key={m.email} className="max-w-[110px] min-w-0 flex">
          <AuthorTag name={m.email} />
        </span>
      ))}
      {rest > 0 && <span className="label font-mono">+{rest}</span>}
    </span>
  );
}

/* The inspector: everything you can do to one vault, Figma-style on the right. */
function VaultInspector({
  vault,
  me,
  rev,
  inline = false,
  onMembersChanged,
  onVaultsChanged,
}: {
  vault: Vault;
  me: string | null;
  rev: number;
  inline?: boolean;
  onMembersChanged: () => void;
  onVaultsChanged: () => void;
}) {
  const [members, setMembers] = useState<VaultMember[] | null>(null);
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<VaultRole>("member");
  const [touched, setTouched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<"leave" | "delete" | null>(null);
  const [cloning, setCloning] = useState(false);
  const [cloneName, setCloneName] = useState(`${vault.name} (copy)`);
  const [cloneBusy, setCloneBusy] = useState(false);
  const [sent, setSent] = useState<string | null>(null);

  useEffect(() => {
    vaultsApi
      .members(vault.id)
      .then((r) => setMembers(r.results))
      .catch(() => setMembers([]));
  }, [vault.id, rev]);

  const isOwner = vault.role === "owner";
  const isOrg = vault.kind !== "personal";
  const trimmed = email.trim();
  const invalid = touched && trimmed !== "" && !EMAIL_RE.test(trimmed);
  const already = members?.some(
    (m) => m.email.toLowerCase() === trimmed.toLowerCase(),
  );
  const created = ago(vault.created_at);

  async function invite() {
    setTouched(true);
    if (!trimmed || !EMAIL_RE.test(trimmed)) return;
    setBusy(true);
    setError(null);
    setSent(null);
    try {
      await vaultsApi.invite(vault.id, trimmed, role);
      setSent(
        `Invite sent. It shows up on their Vaults page until they join or decline.`,
      );
      setEmail("");
      setTouched(false);
      onMembersChanged();
    } catch (e) {
      setError(
        `${errText(e, "Could not send that invite.")} Check the address and try again.`,
      );
    } finally {
      setBusy(false);
    }
  }

  async function run(
    action: () => Promise<unknown>,
    fallback: string,
    after: () => void,
  ) {
    setError(null);
    try {
      await action();
      after();
    } catch (e) {
      setError(errText(e, fallback));
    }
  }

  async function doClone() {
    setCloneBusy(true);
    setError(null);
    try {
      await vaultsApi.clone(vault.id, cloneName.trim() || undefined);
      setCloning(false);
      onVaultsChanged();
    } catch (e) {
      setError(errText(e, "Could not clone this vault. Try a different name."));
    } finally {
      setCloneBusy(false);
    }
  }

  return (
    <aside
      className={inline ? "flex flex-col" : "panel flex flex-col"}
      style={
        inline ? { borderTop: "var(--hair) solid var(--border)" } : undefined
      }
      aria-label={`${vault.name} details`}
    >
      <div className="px-5 pt-4 pb-4 flex flex-col gap-1">
        <span className="label">{isOrg ? "Org vault" : "Personal vault"}</span>
        <div className="flex items-center gap-2 min-w-0">
          <h2 className="text-[17px] font-semibold tracking-[-0.015em] truncate">
            {vault.name}
          </h2>
          <RoleChip role={vault.role} />
        </div>
        <span className="label">
          {isOrg
            ? "Shared with every member below"
            : "Private to you. Nobody else can be added."}
          {created ? ` · created ${created}` : ""}
        </span>
      </div>

      <div
        className="px-5 pb-4 flex flex-col gap-2"
        style={{ borderTop: "var(--hair) solid var(--border)" }}
      >
        <h3 className="label pt-4 flex items-center gap-1.5">
          Members
          {members && <span className="font-mono">{members.length}</span>}
        </h3>
        <ul className="hairline-rows">
          {members === null &&
            [0, 1].map((i) => (
              <li key={i} className="flex items-center gap-2 h-10">
                <span className="skeleton h-5 w-16" />
                <span className="skeleton h-4 flex-1" />
              </li>
            ))}
          {members?.map((m) => {
            const isMe =
              me !== null && m.email.toLowerCase() === me.toLowerCase();
            return (
              <li
                key={m.email}
                className="flex items-center gap-2 min-h-10 py-1.5"
              >
                <span className="flex max-w-[96px] min-w-0 shrink-0">
                  <AuthorTag name={m.email} />
                </span>
                <span
                  className="flex-1 min-w-0 truncate text-[12.5px]"
                  style={{ color: "var(--ink-dim)" }}
                >
                  {m.email}
                  {isMe && (
                    <span style={{ color: "var(--ink-faint)" }}> (you)</span>
                  )}
                </span>
                {removing === m.email ? (
                  <span className="inline-flex items-center gap-1 shrink-0">
                    <button
                      onClick={() =>
                        run(
                          () => vaultsApi.removeMember(vault.id, m.email),
                          "Could not remove that member. Reload and try again.",
                          () => {
                            setRemoving(null);
                            onMembersChanged();
                          },
                        )
                      }
                      className="btn btn-danger btn-sm"
                    >
                      Remove
                    </button>
                    <button
                      onClick={() => setRemoving(null)}
                      className="btn btn-ghost btn-sm btn-icon"
                      aria-label="Keep member"
                    >
                      <X {...ICON} />
                    </button>
                  </span>
                ) : (
                  <>
                    <RoleChip role={m.role} />
                    {isOwner && !isMe && (
                      <button
                        onClick={() => setRemoving(m.email)}
                        aria-label="Remove member"
                        title={`Remove ${m.email}`}
                        className="btn btn-ghost btn-sm btn-icon shrink-0"
                      >
                        <Trash2 {...ICON} />
                      </button>
                    )}
                  </>
                )}
              </li>
            );
          })}
        </ul>

        {isOwner && isOrg && (
          <form
            className="flex flex-col gap-1.5 pt-2"
            onSubmit={(e) => {
              e.preventDefault();
              invite();
            }}
            noValidate
          >
            <label htmlFor={`invite-${vault.id}`} className="label">
              Invite by email
            </label>
            <div className="flex items-center gap-1.5">
              <input
                id={`invite-${vault.id}`}
                type="email"
                className="field h-8 px-3 text-[13px] flex-1 min-w-0"
                placeholder="person@example.com"
                value={email}
                aria-invalid={invalid || undefined}
                aria-describedby={`invite-help-${vault.id}`}
                onChange={(e) => {
                  setEmail(e.target.value);
                  setSent(null);
                }}
                onBlur={() => setTouched(true)}
                style={invalid ? { borderColor: "var(--critical)" } : undefined}
              />
              <select
                className="field h-8 px-1.5 text-[12.5px]"
                value={role}
                aria-label="Role for invite"
                onChange={(e) => setRole(e.target.value as VaultRole)}
              >
                <option value="member">Member</option>
                <option value="owner">Owner</option>
              </select>
            </div>
            <div className="flex items-start justify-between gap-2">
              <span
                id={`invite-help-${vault.id}`}
                className="text-[12px] pt-1.5"
                style={{
                  color: invalid ? "var(--critical)" : "var(--ink-faint)",
                }}
              >
                {invalid
                  ? "That isn't an email address. Use name@company.com."
                  : already
                    ? "Already a member."
                    : (sent ??
                      "They join once they accept. Owners can invite and remove.")}
              </span>
              <button
                type="submit"
                disabled={busy || !trimmed}
                className="btn btn-sm shrink-0"
              >
                <UserPlus {...ICON} />
                {busy ? "Inviting…" : "Invite"}
              </button>
            </div>
          </form>
        )}
      </div>

      <div
        className="px-5 py-4 flex flex-col gap-2"
        style={{ borderTop: "var(--hair) solid var(--border)" }}
      >
        {cloning ? (
          <form
            className="flex flex-col gap-1.5"
            onSubmit={(e) => {
              e.preventDefault();
              doClone();
            }}
          >
            <label htmlFor={`clone-${vault.id}`} className="label">
              Name for the copy. {vault.name} stays exactly as it is.
            </label>
            <input
              id={`clone-${vault.id}`}
              autoFocus
              className="field h-8 px-3 text-[13px]"
              value={cloneName}
              onChange={(e) => setCloneName(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setCloning(false)}
            />
            <div className="flex justify-end gap-1.5">
              <button
                type="button"
                onClick={() => setCloning(false)}
                className="btn btn-ghost btn-sm"
              >
                Cancel
              </button>
              <button type="submit" disabled={cloneBusy} className="btn btn-sm">
                {cloneBusy ? "Cloning…" : "Clone"}
              </button>
            </div>
          </form>
        ) : confirm ? (
          <div
            className="flex flex-col gap-2 rounded-[7px] p-3"
            style={{ background: "var(--critical-soft)" }}
          >
            <p className="text-[12.5px]" style={{ color: "var(--ink)" }}>
              {confirm === "delete"
                ? `Delete ${vault.name}? Its entities and memory are removed for every member. This can't be undone.`
                : `Leave ${vault.name}? You lose access until an owner invites you again.`}
            </p>
            <div className="flex justify-end gap-1.5">
              <button
                onClick={() => setConfirm(null)}
                className="btn btn-ghost btn-sm"
              >
                Cancel
              </button>
              <button
                onClick={() =>
                  confirm === "delete"
                    ? run(
                        () => vaultsApi.delete(vault.id),
                        "Could not delete this vault. Reload and try again.",
                        onVaultsChanged,
                      )
                    : run(
                        () => vaultsApi.leave(vault.id),
                        "Could not leave this vault. It needs an owner, so make someone else owner first.",
                        onVaultsChanged,
                      )
                }
                className="btn btn-danger btn-sm"
              >
                {confirm === "delete" ? (
                  <Trash2 {...ICON} />
                ) : (
                  <LogOut {...ICON} />
                )}
                {confirm === "delete" ? "Delete vault" : "Leave"}
              </button>
            </div>
          </div>
        ) : (
          <div className="flex flex-wrap items-center gap-1.5">
            <button
              onClick={() => {
                setCloneName(`${vault.name} (copy)`);
                setCloning(true);
              }}
              className="btn btn-sm"
              title="Clone into a new vault"
            >
              <Copy {...ICON} />
              Clone vault
            </button>
            <span className="flex-1" />
            {isOrg && (
              <button
                onClick={() => setConfirm("leave")}
                className="btn btn-ghost btn-sm"
              >
                <LogOut {...ICON} />
                Leave this vault
              </button>
            )}
            {isOrg && isOwner && (
              <button
                onClick={() => setConfirm("delete")}
                className="btn btn-danger btn-sm"
                aria-label="Delete vault"
              >
                <Trash2 {...ICON} />
                Delete
              </button>
            )}
          </div>
        )}
        {error && <ErrorLine>{error}</ErrorLine>}
      </div>
    </aside>
  );
}

function InvitationsPanel({ onChanged }: { onChanged: () => void }) {
  const [invitations, setInvitations] = useState<VaultInvitation[] | null>(
    null,
  );
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
      setError(
        errText(
          e,
          "Could not accept this invitation. It may have been withdrawn, so reload to check.",
        ),
      );
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
      setError(
        errText(e, "Could not decline this invitation. Reload and try again."),
      );
    } finally {
      setBusyId(null);
    }
  }

  if (!invitations || invitations.length === 0) return null;

  return (
    <section className="flex flex-col gap-2.5" aria-labelledby="invites-title">
      <h2 id="invites-title" className="section-title flex items-center gap-2">
        <Mail {...ICON} aria-hidden style={{ color: "var(--ink-faint)" }} />
        Pending invitations
        <span className="label font-mono">{invitations.length}</span>
      </h2>
      {invitations.map((inv) => (
        <div
          key={inv.vault_id}
          className="ledger flex flex-wrap items-center gap-3 px-4 py-3"
          style={{ borderStyle: "dashed", borderColor: "var(--border-strong)" }}
        >
          <div className="flex-1 min-w-[160px]">
            <div className="text-[13.5px] font-medium truncate">
              {inv.vault_name}
            </div>
            <div className="label mt-0.5">
              Invited as {inv.role === "owner" ? "an owner" : "a member"}
              {inv.vault_kind === "org" ? " of an org vault" : ""}
              {ago(inv.created_at) ? ` · ${ago(inv.created_at)}` : ""}
            </div>
          </div>
          <div className="flex items-center gap-1.5">
            <button
              onClick={() => decline(inv.vault_id)}
              disabled={busyId === inv.vault_id}
              aria-label="Decline invitation"
              className="btn btn-ghost btn-sm"
            >
              <X {...ICON} />
              Decline
            </button>
            <button
              onClick={() => accept(inv.vault_id)}
              disabled={busyId === inv.vault_id}
              className="btn btn-primary btn-sm"
            >
              <Check {...ICON} />
              Join
            </button>
          </div>
        </div>
      ))}
      {error && <ErrorLine>{error}</ErrorLine>}
    </section>
  );
}

function VaultRow({
  vault,
  rev,
  selected,
  onSelect,
}: {
  vault: Vault;
  rev: number;
  selected: boolean;
  onSelect: () => void;
}) {
  const [members, setMembers] = useState<VaultMember[] | null>(null);

  useEffect(() => {
    vaultsApi
      .members(vault.id)
      .then((r) => setMembers(r.results))
      .catch(() => setMembers([]));
  }, [vault.id, rev]);

  return (
    <button
      onClick={onSelect}
      aria-current={selected || undefined}
      className={`w-full first:rounded-t-[10px] last:rounded-b-[10px] flex items-center gap-3 px-4 py-3 text-left transition-[background-color,transform] duration-[120ms] active:scale-[0.995] hover:bg-[var(--surface-raised)] ${selected ? "frame-selected" : ""}`}
      style={selected ? { background: "var(--accent-soft)" } : undefined}
    >
      <span className="flex-1 min-w-0">
        <span className="flex items-center gap-2 min-w-0">
          <span className="text-[13.5px] font-medium truncate">
            {vault.name}
          </span>
          <RoleChip role={vault.role} />
        </span>
        <span className="label block mt-0.5 truncate">
          {vault.kind === "personal" ? "Private to you" : "Org vault"}
          {members
            ? ` · ${members.length} ${members.length === 1 ? "member" : "members"}`
            : ""}
        </span>
      </span>
      <MemberStack members={members} />
      <ChevronRight
        {...ICON}
        aria-hidden
        className="shrink-0"
        style={{ color: selected ? "var(--accent-text)" : "var(--ink-faint)" }}
      />
    </button>
  );
}

function CreateVaultForm({
  onCreated,
  onCancel,
}: {
  onCreated: () => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function create() {
    const trimmed = name.trim();
    if (!trimmed) return;
    setBusy(true);
    setError(null);
    try {
      await vaultsApi.create(trimmed, "org");
      setName("");
      onCreated();
    } catch (e) {
      setError(
        errText(e, "Could not create that vault. Try a different name."),
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      className="ledger pop-in flex flex-col gap-1.5 px-4 py-3"
      style={{ transformOrigin: "top right" }}
      onSubmit={(e) => {
        e.preventDefault();
        create();
      }}
    >
      <label htmlFor="new-vault-name" className="label">
        Name the vault. You become its owner and can invite people next.
      </label>
      <div className="flex flex-wrap items-center gap-2">
        <input
          id="new-vault-name"
          autoFocus
          className="field h-8 px-3 text-[13px] flex-1 min-w-[180px]"
          placeholder="Vault name (e.g. Acme Team)"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && onCancel()}
        />
        <button
          type="button"
          onClick={onCancel}
          className="btn btn-ghost btn-sm"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={busy || !name.trim()}
          className="btn btn-sm"
        >
          {busy ? "Creating…" : "Create"}
        </button>
      </div>
      {error && <ErrorLine>{error}</ErrorLine>}
    </form>
  );
}

const vaultLabel = (v: Vault) => (v.kind === "personal" ? "Personal" : v.name);

function MergeVaultsCard({
  vaults,
  onMerged,
}: {
  vaults: Vault[];
  onMerged: () => void;
}) {
  const [a, setA] = useState(vaults[0]?.id ?? "");
  const [b, setB] = useState(vaults[1]?.id ?? "");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function merge() {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const merged = await vaultsApi.merge(a, b, name.trim());
      setResult(`Created “${merged.name}” with ${merged.entities} entities.`);
      setName("");
      onMerged();
    } catch (e) {
      setError(
        errText(
          e,
          "Could not merge those vaults. Check you are a member of both and try again.",
        ),
      );
    } finally {
      setBusy(false);
    }
  }

  const select = (value: string, set: (v: string) => void, label: string) => (
    <label className="flex flex-col gap-1.5 flex-1 min-w-[140px]">
      <span className="label">{label}</span>
      <select
        className="field h-8 px-2 text-[13px]"
        value={value}
        onChange={(e) => set(e.target.value)}
        aria-label={label}
      >
        {vaults.map((v) => (
          <option key={v.id} value={v.id}>
            {vaultLabel(v)}
          </option>
        ))}
      </select>
    </label>
  );

  return (
    <section className="flex flex-col gap-2.5" aria-labelledby="merge-title">
      <div>
        <h2 id="merge-title" className="section-title">
          Merge two vaults
        </h2>
        <p
          className="text-[13px] mt-1 max-w-[62ch]"
          style={{ color: "var(--ink-dim)" }}
        >
          Copies both into a brand new vault. The originals are never changed.
          Matching entities (same kind and name) are combined, and duplicate
          facts are kept once.
        </p>
      </div>
      <div className="ledger p-4 flex flex-col gap-3">
        <div className="flex items-end gap-2 flex-wrap">
          {select(a, setA, "First vault")}
          <Plus
            {...ICON}
            aria-hidden
            className="mb-2 hidden sm:block"
            style={{ color: "var(--ink-faint)" }}
          />
          {select(b, setB, "Second vault")}
        </div>
        <div className="flex items-end gap-2 flex-wrap">
          <label className="flex flex-col gap-1.5 flex-1 min-w-[180px]">
            <span className="label">New vault name</span>
            <input
              className="field h-8 px-3 text-[13px]"
              placeholder="New vault name (optional)"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <button
            onClick={merge}
            disabled={busy || !a || a === b}
            className="btn"
          >
            <Combine {...ICON} />
            {busy ? "Merging…" : "Merge"}
          </button>
        </div>
        {a === b && (
          <p className="label">Pick two different vaults to merge.</p>
        )}
        {result && (
          <p
            role="status"
            className="text-[12.5px] flex items-center gap-2"
            style={{ color: "var(--ink)" }}
          >
            <span
              className="dot"
              style={{ background: "var(--good)" }}
              aria-hidden
            />
            {result}
          </p>
        )}
        {error && <ErrorLine>{error}</ErrorLine>}
      </div>
    </section>
  );
}

function SkeletonRows() {
  return (
    <div className="ledger hairline-rows" aria-hidden>
      {[0, 1].map((i) => (
        <div key={i} className="flex items-center gap-3 px-4 py-3">
          <div className="flex-1 flex flex-col gap-1.5">
            <span className="skeleton h-4 w-40" />
            <span className="skeleton h-3 w-56 max-w-full" />
          </div>
          <span className="skeleton h-5 w-24" />
        </div>
      ))}
    </div>
  );
}

// The inspector floats on the right on wide screens and opens under its row on narrow ones.
const WIDE = "(min-width: 1024px)";
function useWide() {
  return useSyncExternalStore(
    (cb) => {
      const m = window.matchMedia(WIDE);
      m.addEventListener("change", cb);
      return () => m.removeEventListener("change", cb);
    },
    () => window.matchMedia(WIDE).matches,
    () => true,
  );
}

export default function VaultsPage() {
  const [vaults, setVaults] = useState<Vault[] | null>(null);
  const [me, setMe] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [rev, setRev] = useState(0);

  const load = () =>
    vaultsApi
      .list()
      .then((r) => setVaults(r.results))
      .catch(() => setVaults([]));

  useEffect(() => {
    load();
    auth
      .me()
      .then((m) => setMe(m.email))
      .catch(() => {});
  }, []);

  const personal = vaults?.filter((v) => v.kind === "personal") ?? [];
  const org = vaults?.filter((v) => v.kind !== "personal") ?? [];
  // Open on the first team vault: that is where the people are.
  const selected =
    vaults?.find((v) => v.id === selectedId) ?? org[0] ?? personal[0] ?? null;

  const wide = useWide();
  const inspectorFor = (inline: boolean) =>
    selected && (
      <VaultInspector
        key={selected.id}
        inline={inline}
        vault={selected}
        me={me}
        rev={rev}
        onMembersChanged={() => setRev((r) => r + 1)}
        onVaultsChanged={() => {
          setSelectedId(null);
          load();
        }}
      />
    );

  const list = (items: Vault[]) => (
    <div className="ledger hairline-rows">
      {items.map((v) => (
        <div key={v.id}>
          <VaultRow
            vault={v}
            rev={rev}
            selected={selected?.id === v.id}
            onSelect={() => setSelectedId(v.id)}
          />
          {!wide && selected?.id === v.id && inspectorFor(true)}
        </div>
      ))}
    </div>
  );

  return (
    <div className="max-w-6xl flex flex-col gap-9">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <h1 className="page-title">Who sees what</h1>
          <p
            className="text-[13px] mt-1.5 max-w-[62ch]"
            style={{ color: "var(--ink-dim)" }}
          >
            A vault is a scope of shared entities and memory. Your personal
            vault is always private. Invite people to an org vault and every
            agent they connect reads and writes the same memory.
          </p>
        </div>
        <button
          onClick={() => setCreating(true)}
          disabled={creating}
          className="btn btn-primary"
        >
          <Plus {...ICON} />
          New org vault
        </button>
      </header>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_380px] items-start">
        <div className="flex flex-col gap-9 min-w-0">
          <InvitationsPanel onChanged={load} />

          <section
            className="flex flex-col gap-2.5"
            aria-labelledby="org-title"
          >
            <h2
              id="org-title"
              className="section-title flex items-center gap-2"
            >
              <Users
                {...ICON}
                aria-hidden
                style={{ color: "var(--ink-faint)" }}
              />
              Shared and organisation
              {vaults && <span className="label font-mono">{org.length}</span>}
            </h2>
            {creating && (
              <CreateVaultForm
                onCancel={() => setCreating(false)}
                onCreated={() => {
                  setCreating(false);
                  load();
                }}
              />
            )}
            {!vaults ? (
              <SkeletonRows />
            ) : org.length === 0 ? (
              !creating && (
                <div className="ledger px-4 py-4 flex flex-wrap items-center justify-between gap-3">
                  <p
                    className="text-[13px]"
                    style={{ color: "var(--ink-dim)" }}
                  >
                    No shared vaults yet. Create one to share memory with your
                    team, or accept an invite.
                  </p>
                  <button
                    onClick={() => setCreating(true)}
                    className="btn btn-sm"
                  >
                    <Plus {...ICON} />
                    Create a vault
                  </button>
                </div>
              )
            ) : (
              list(org)
            )}
          </section>

          <section
            className="flex flex-col gap-2.5"
            aria-labelledby="personal-title"
          >
            <h2
              id="personal-title"
              className="section-title flex items-center gap-2"
            >
              <Lock
                {...ICON}
                aria-hidden
                style={{ color: "var(--ink-faint)" }}
              />
              Personal
            </h2>
            {!vaults ? <SkeletonRows /> : list(personal)}
          </section>

          {vaults && vaults.length > 1 && (
            <MergeVaultsCard vaults={vaults} onMerged={load} />
          )}
        </div>

        {wide && (
          <div className="sticky top-6">
            {inspectorFor(false) ?? (
              <div className="panel p-5 flex flex-col gap-3" aria-hidden>
                <span className="skeleton h-4 w-24" />
                <span className="skeleton h-5 w-40" />
                <span className="skeleton h-24 w-full" />
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
