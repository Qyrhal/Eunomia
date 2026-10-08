"use client";

import ErrorLine, { failure, type Failure } from "@/components/ErrorLine";
import { useState } from "react";
import { useOauthGrants, useRevokeOauthGrant } from "@/lib/queries/oauth";
import { relativeTime, PanelHead, RevokeButton, TableSkeleton } from "./shared";

const SCOPE_WORDS: Record<string, string> = {
  "memory:read": "read memory",
  "memory:write": "write memory",
  "vaults:admin": "manage vaults",
  connectors: "manage sources",
};

export function ConnectedAppsSection() {
  const grantsQuery = useOauthGrants();
  const grants = grantsQuery.data ?? (grantsQuery.isError ? [] : null);
  const revokeGrant = useRevokeOauthGrant();
  const [error, setError] = useState<Failure | null>(null);

  async function revoke(id: string) {
    setError(null);
    try {
      await revokeGrant.mutateAsync(id);
    } catch (e) {
      setError(failure(e, "Could not disconnect that app. Reload and try again."));
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <PanelHead title="Connected apps">
        Apps like Claude Code and Cursor that you signed in to Eunomia with OAuth. Disconnecting one stops it
        immediately and it has to ask you again.
      </PanelHead>
      {grantsQuery.isError && <ErrorLine error={failure(grantsQuery.error, "Could not load your connected apps.", " Reload the page to try again.")} />}
      {error && <ErrorLine error={error} />}
      <div className="ledger overflow-x-auto">
        <table className="data-table">
          <thead>
            <tr>
              <th>App</th>
              <th>Can</th>
              <th className="w-[120px]">Connected</th>
              <th className="w-[120px]">Last used</th>
              <th className="w-[110px]">
                <span className="sr-only">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {grants === null && <TableSkeleton cols={5} />}
            {grants?.length === 0 && (
              <tr>
                <td colSpan={5} style={{ color: "var(--ink-dim)", height: 56 }}>
                  No connected apps. Add Eunomia as an MCP server in your agent and approve the sign-in.
                </td>
              </tr>
            )}
            {grants?.map((g) => (
              <tr key={g.id}>
                <td className="font-medium">
                  <span className="truncate max-w-[220px] block" title={g.client_id}>
                    {g.client_name}
                  </span>
                </td>
                <td style={{ color: "var(--ink-dim)" }}>{g.scope.map((s) => SCOPE_WORDS[s] ?? s).join(", ")}</td>
                <td className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>
                  {relativeTime(g.created_at)}
                </td>
                <td className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>
                  {relativeTime(g.last_used_at)}
                </td>
                <td className="text-right">
                  <RevokeButton label={g.client_name} onRevoke={() => revoke(g.id)} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
