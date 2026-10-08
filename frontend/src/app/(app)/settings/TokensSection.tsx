"use client";

import ErrorLine, { failure, type Failure } from "@/components/ErrorLine";
import Select from "@/components/Select";
import { useState } from "react";
import { KeyRound, X } from "lucide-react";
import { useCreateToken, useRevokeToken, useTokens } from "@/lib/queries/auth";
import { useVaults } from "@/lib/queries/vaults";
import type { Scope, Vault } from "@/lib/types";
import Tooltip from "@/components/bits/Tooltip";
import { ICON, CopyField, relativeTime, expiryLabel, PanelHead, RevokeButton, TableSkeleton } from "./shared";

const SCOPES: { id: Scope; label: string; hint: string }[] = [
  { id: "memory:read", label: "Read memories", hint: "Recall, search and read entities and vaults" },
  { id: "memory:write", label: "Write memories", hint: "Add, edit and delete memories and entities" },
  { id: "vaults:admin", label: "Manage vaults", hint: "Create, rename, share and delete vaults; settings, tokens and chat" },
  { id: "connectors", label: "Connectors", hint: "View and sync connected sources" },
];

const EXPIRY_OPTIONS = [
  { value: "", label: "Never expires" },
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
  { value: "365", label: "1 year" },
];

export function TokensSection() {
  const tokensQuery = useTokens();
  const tokens = tokensQuery.data ?? (tokensQuery.isError ? [] : null);
  const createToken = useCreateToken();
  const revokeToken = useRevokeToken();
  const vaultList: Vault[] = useVaults().data ?? [];
  const [name, setName] = useState("");
  const [scopes, setScopes] = useState<Scope[]>(SCOPES.map((s) => s.id));
  const [vaultId, setVaultId] = useState("");
  const [expiryDays, setExpiryDays] = useState("");
  const [minted, setMinted] = useState<{ name: string; token: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Failure | null>(null);

  const vaultName = (id: string) => vaultList.find((v) => v.id === id)?.name ?? id;

  async function create() {
    setBusy(true);
    setError(null);
    try {
      const res = await createToken.mutateAsync({
        name: name.trim() || "API token",
        scopes,
        vault_id: vaultId || null,
        expires_at: expiryDays ? new Date(Date.now() + Number(expiryDays) * 86_400_000).toISOString() : null,
      });
      setMinted({ name: res.name, token: res.token });
      setName("");
    } catch (e) {
      setError(failure(e, "Could not create a token. Check that you're still signed in, then try again."));
    } finally {
      setBusy(false);
    }
  }

  async function revoke(id: string) {
    setError(null);
    try {
      await revokeToken.mutateAsync(id);
    } catch (e) {
      setError(failure(e, "Could not revoke that token. Reload and try again."));
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <PanelHead title="API tokens">
        Personal tokens for MCP clients and scripts. They act as you; narrow one to the scopes it needs, to a single vault,
        or give it an expiry. Each is shown once, at creation.
      </PanelHead>

      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          create();
        }}
      >
        <div className="flex flex-col gap-1.5">
          <label htmlFor="token-name" className="label">
            Token name
          </label>
          <input
            id="token-name"
            className="field h-8 px-3 text-[13px] w-full"
            placeholder="Name this token (e.g. laptop, MCP)"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </div>

        <div className="flex flex-col gap-1.5">
          <span id="token-scopes" className="label">
            Scopes
          </span>
          <div className="flex flex-wrap gap-2" role="group" aria-labelledby="token-scopes">
            {SCOPES.map((s) => {
              const on = scopes.includes(s.id);
              return (
                <Tooltip key={s.id} label={s.hint}>
                  <button
                    type="button"
                    className="pill"
                    aria-pressed={on}
                    onClick={() => setScopes((cur) => (on ? cur.filter((x) => x !== s.id) : [...cur, s.id]))}
                  >
                    {s.label}
                  </button>
                </Tooltip>
              );
            })}
          </div>
          {scopes.length === 0 && (
            <span className="text-[12px]" style={{ color: "var(--critical)" }}>
              Choose at least one scope.
            </span>
          )}
        </div>

        <div className="flex flex-wrap items-end gap-3">
          <div className="flex flex-col gap-1.5 w-full sm:w-[220px]">
            <span className="label">
              Vault
            </span>
            <Select
              aria-label="Restrict to vault"
              value={vaultId}
              onChange={setVaultId}
              options={[
                { value: "", label: "All my vaults" },
                ...vaultList.map((v) => ({ value: v.id, label: v.name })),
              ]}
              className="h-8 text-[13px] w-full"
            />
          </div>
          <div className="flex flex-col gap-1.5 w-full sm:w-[160px]">
            <span className="label">
              Expiry
            </span>
            <Select
              aria-label="Expiry"
              value={expiryDays}
              onChange={setExpiryDays}
              options={EXPIRY_OPTIONS}
              className="h-8 text-[13px] w-full"
            />
          </div>
          <button type="submit" disabled={busy || scopes.length === 0} className="btn btn-primary">
            <KeyRound {...ICON} />
            {busy ? "Creating…" : "Create"}
          </button>
        </div>
        {vaultId && (
          <p className="text-[12px] -mt-1" style={{ color: "var(--ink-faint)" }}>
            A vault-restricted token works only in that vault. It can&apos;t create vaults or change account settings.
          </p>
        )}
      </form>

      {minted && (
        <div className="panel pop-in p-4 flex flex-col gap-2" style={{ transformOrigin: "top center" }}>
          <div className="flex items-start justify-between gap-3">
            <div>
              <div className="text-[13px] font-medium">Token “{minted.name}” created</div>
              <p className="text-[12.5px] mt-0.5" style={{ color: "var(--ink-dim)" }}>
                Copy it now and store it somewhere safe. It won&apos;t be shown again.
              </p>
            </div>
            <button onClick={() => setMinted(null)} aria-label="Dismiss" className="btn btn-ghost btn-sm btn-icon">
              <X {...ICON} />
            </button>
          </div>
          <CopyField value={minted.token} reveal />
        </div>
      )}

      {tokensQuery.isError && <ErrorLine error={failure(tokensQuery.error, "Could not load your tokens.", " Reload the page to try again.")} />}
      {error && <ErrorLine error={error} />}

      <div className="ledger overflow-x-auto">
        <table className="data-table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Access</th>
              <th className="w-[100px]">Last used</th>
              <th className="w-[90px]">Expires</th>
              <th className="w-[110px]">
                <span className="sr-only">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {tokens === null && <TableSkeleton cols={5} />}
            {tokens?.length === 0 && (
              <tr>
                <td colSpan={5} style={{ color: "var(--ink-dim)", height: 56 }}>
                  No tokens yet. Create one above to connect an agent over MCP.
                </td>
              </tr>
            )}
            {tokens?.map((t) => {
              const exp = expiryLabel(t.expires_at);
              const full = SCOPES.every((s) => t.scopes.includes(s.id));
              return (
                <tr key={t.id}>
                  <td className="font-medium">{t.name}</td>
                  <td>
                    <span className="flex flex-col gap-0.5 py-1.5 min-w-0">
                      {full ? (
                        <span>Full access</span>
                      ) : (
                        <span className="font-mono text-[12px]">{t.scopes.join(" ")}</span>
                      )}
                      <span className="text-[12px]" style={{ color: "var(--ink-dim)" }}>
                        {t.vault_id ? `Vault: ${vaultName(t.vault_id)}` : "All vaults"}
                      </span>
                    </span>
                  </td>
                  <td className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>
                    {relativeTime(t.last_used_at)}
                  </td>
                  <td
                    className="font-mono text-[12px]"
                    style={{ color: exp.expired ? "var(--critical)" : "var(--ink-dim)" }}
                    title={t.expires_at ? new Date(t.expires_at).toLocaleString() : undefined}
                  >
                    {exp.text}
                  </td>
                  <td className="text-right">
                    <RevokeButton label={t.name} onRevoke={() => revoke(t.id)} />
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
