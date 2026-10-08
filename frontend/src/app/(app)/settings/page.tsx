"use client";

import ErrorLine, { failure, type Failure } from "@/components/ErrorLine";
import Select from "@/components/Select";
import { useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { Check, ChevronRight, Download, KeyRound, Monitor, Plug, RefreshCw, Trash2, X } from "lucide-react";
import {
  auth,
  downloadExport,
  settings as settingsApi,
  update as updateApi,
  oauth,
  type ApiToken,
  type OAuthGrant,
  type AppSettings,
  type Session,
  type UpdateStatus,
} from "@/lib/api";
import CopyButton from "@/components/bits/CopyButton";
import DecryptReveal from "@/components/bits/DecryptReveal";
import Tooltip from "@/components/bits/Tooltip";
import HoldButton from "@/components/bits/HoldButton";
import SyncMark from "@/components/bits/SyncMark";

const ICON = { size: 14, strokeWidth: 1.75 } as const;

/* reveal: a freshly minted secret decrypts in once (the token, never a command). */
function CopyField({ value, reveal = false }: { value: string; reveal?: boolean }) {
  return (
    <div className="field flex items-center gap-2 h-8 pl-3 pr-1">
      <code className="flex-1 min-w-0 truncate text-[12px] font-mono" style={{ color: "var(--ink)" }}>
        {reveal ? <DecryptReveal key={value} text={value} /> : value}
      </code>
      <CopyButton value={value} size="sm" className="btn-ghost shrink-0" />
    </div>
  );
}

function relativeTime(iso: string | null): string {
  if (!iso) return "never";
  const ms = Date.now() - new Date(iso).getTime();
  const mins = Math.round(ms / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  return `${days}d ago`;
}

/* Section header inside a tab panel: title, one line of purpose, optional action. */
function PanelHead({ title, children, action }: { title: string; children?: React.ReactNode; action?: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0 flex-1">
        <h2 className="section-title">{title}</h2>
        {children && (
          <p className="text-[13px] mt-1 max-w-[62ch]" style={{ color: "var(--ink-dim)" }}>
            {children}
          </p>
        )}
      </div>
      {action}
    </div>
  );
}

/* Inline revoke: one click arms it, the second confirms. Holding the icon for 650ms is the accelerator. */
function RevokeButton({ label, onRevoke }: { label: string; onRevoke: () => Promise<void> }) {
  const [armed, setArmed] = useState(false);
  const [busy, setBusy] = useState(false);
  async function confirm() {
    setBusy(true);
    try {
      await onRevoke();
    } finally {
      setBusy(false);
      setArmed(false);
    }
  }
  if (!armed)
    return (
      <Tooltip label={`Revoke ${label}. Hold to revoke now`}>
      <HoldButton
        holdMs={650}
        onClick={() => setArmed(true)}
        onConfirm={confirm}
        disabled={busy}
        aria-label="Revoke"
        className="btn-sm btn-icon"
        // Quiet at rest like the ghost icon it replaces; the critical fill shows while held.
        style={{ borderColor: "transparent", color: "var(--ink-dim)" }}
      >
        <Trash2 {...ICON} />
      </HoldButton>
      </Tooltip>
    );
  return (
    <span className="inline-flex items-center gap-1">
      <button
        onClick={confirm}
        disabled={busy}
        className="btn btn-danger btn-sm"
      >
        {busy ? "Revoking…" : "Revoke"}
      </button>
      <button onClick={() => setArmed(false)} aria-label="Keep" className="btn btn-ghost btn-sm btn-icon">
        <X {...ICON} />
      </button>
    </span>
  );
}

function TableSkeleton({ cols }: { cols: number }) {
  return (
    <>
      {[0, 1].map((i) => (
        <tr key={i} aria-hidden>
          {Array.from({ length: cols }, (_, c) => (
            <td key={c}>
              <span className="skeleton block h-4" style={{ width: c === 0 ? "60%" : "50%" }} />
            </td>
          ))}
        </tr>
      ))}
    </>
  );
}

function TokensSection() {
  const [tokens, setTokens] = useState<ApiToken[] | null>(null);
  const [name, setName] = useState("");
  const [minted, setMinted] = useState<{ name: string; token: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Failure | null>(null);

  const load = () => auth.tokens.list().then(setTokens).catch(() => setTokens([]));
  useEffect(() => {
    load();
  }, []);

  async function create() {
    setBusy(true);
    setError(null);
    try {
      const res = await auth.tokens.create(name.trim() || "API token");
      setMinted({ name: res.name, token: res.token });
      setName("");
      await load();
    } catch (e) {
      setError(failure(e, "Could not create a token. Check that you're still signed in, then try again."));
    } finally {
      setBusy(false);
    }
  }

  async function revoke(id: string) {
    setError(null);
    try {
      await auth.tokens.revoke(id);
      await load();
    } catch (e) {
      setError(failure(e, "Could not revoke that token. Reload and try again."));
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <PanelHead title="API tokens">
        Personal tokens for MCP clients and scripts. They act as you, in every vault you belong to. Each is shown once, at
        creation.
      </PanelHead>

      <form
        className="flex flex-col gap-1.5"
        onSubmit={(e) => {
          e.preventDefault();
          create();
        }}
      >
        <label htmlFor="token-name" className="label">
          Token name
        </label>
        <div className="flex flex-wrap items-center gap-2">
          <input
            id="token-name"
            className="field h-8 px-3 text-[13px] flex-1 min-w-[200px]"
            placeholder="Name this token (e.g. laptop, MCP)"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <button type="submit" disabled={busy} className="btn btn-primary">
            <KeyRound {...ICON} />
            {busy ? "Creating…" : "Create"}
          </button>
        </div>
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

      {error && <ErrorLine error={error} />}

      <div className="ledger overflow-x-auto">
        <table className="data-table">
          <thead>
            <tr>
              <th>Name</th>
              <th className="w-[120px]">Created</th>
              <th className="w-[120px]">Last used</th>
              <th className="w-[110px]">
                <span className="sr-only">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {tokens === null && <TableSkeleton cols={4} />}
            {tokens?.length === 0 && (
              <tr>
                <td colSpan={4} style={{ color: "var(--ink-dim)", height: 56 }}>
                  No tokens yet. Create one above to connect an agent over MCP.
                </td>
              </tr>
            )}
            {tokens?.map((t) => (
              <tr key={t.id}>
                <td className="font-medium">{t.name}</td>
                <td className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>
                  {relativeTime(t.created_at)}
                </td>
                <td className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>
                  {relativeTime(t.last_used_at)}
                </td>
                <td className="text-right">
                  <RevokeButton label={t.name} onRevoke={() => revoke(t.id)} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

const SCOPE_WORDS: Record<string, string> = {
  "memory:read": "read memory",
  "memory:write": "write memory",
  "vaults:admin": "manage vaults",
  connectors: "manage sources",
};

function ConnectedAppsSection() {
  const [grants, setGrants] = useState<OAuthGrant[] | null>(null);
  const [error, setError] = useState<Failure | null>(null);

  const load = () => oauth.grants.list().then(setGrants).catch(() => setGrants([]));
  useEffect(() => {
    load();
  }, []);

  async function revoke(id: string) {
    setError(null);
    try {
      await oauth.grants.revoke(id);
      await load();
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

function SessionsSection() {
  const [sessions, setSessions] = useState<Session[] | null>(null);
  const [error, setError] = useState<Failure | null>(null);

  const load = () => auth.sessions.list().then(setSessions).catch(() => setSessions([]));
  useEffect(() => {
    load();
  }, []);

  async function revoke(id: string) {
    setError(null);
    try {
      await auth.sessions.revoke(id);
      await load();
    } catch (e) {
      setError(failure(e, "Could not revoke that session. Reload and try again."));
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <PanelHead title="Active sessions">Browser sign-ins. Revoking one signs that browser out immediately.</PanelHead>
      {error && <ErrorLine error={error} />}
      <div className="ledger overflow-x-auto">
        <table className="data-table">
          <thead>
            <tr>
              <th>Device</th>
              <th className="w-[120px]">Signed in</th>
              <th className="w-[120px]">Last seen</th>
              <th className="w-[110px]">
                <span className="sr-only">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {sessions === null && <TableSkeleton cols={4} />}
            {sessions?.length === 0 && (
              <tr>
                <td colSpan={4} style={{ color: "var(--ink-dim)", height: 56 }}>
                  No active sessions.
                </td>
              </tr>
            )}
            {sessions?.map((s) => (
              <tr key={s.id}>
                <td>
                  <span className="flex items-center gap-2 min-w-0">
                    <Monitor {...ICON} className="shrink-0" style={{ color: "var(--ink-faint)" }} aria-hidden />
                    <span className="truncate max-w-[360px]" title={s.user_agent}>
                      {s.user_agent || "Unknown device"}
                    </span>
                  </span>
                </td>
                <td className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>
                  {relativeTime(s.created_at)}
                </td>
                <td className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>
                  {relativeTime(s.last_seen_at)}
                </td>
                <td className="text-right">
                  <RevokeButton label="this session" onRevoke={() => revoke(s.id)} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

const INSTALL_CMD = "curl -fsSL https://midhunkumar05.github.io/eunomia/install.sh | bash";

// idle -> waiting (marker dropped, updater picks it up within ~20s) -> applying
// -> restarting (API briefly unreachable) -> reload once the new version answers.
type Phase = "idle" | "waiting" | "applying" | "restarting";

function UpdateSection() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [target, setTarget] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  // How the last "Check now" ended, shown on the button for 1.2s.
  const [checkResult, setCheckResult] = useState<"done" | "failed" | null>(null);
  const [wasChecking, setWasChecking] = useState(false);
  const [stale, setStale] = useState(false);
  const [error, setError] = useState<Failure | null>(null);
  const requestedAt = useRef(0);

  const busy = phase !== "idle";
  if (checking !== wasChecking) {
    setWasChecking(checking);
    if (wasChecking && !checking && !checkResult) setCheckResult("done");
  }
  useEffect(() => {
    if (!checkResult) return;
    const t = setTimeout(() => setCheckResult(null), 1200);
    return () => clearTimeout(t);
  }, [checkResult]);

  const load = useCallback(
    () =>
      updateApi
        .status()
        .then((s) => {
          setStatus(s);
          setError(null);
          if (!s.configured) return;
          setStale(Date.now() - new Date(s.checked_at).getTime() > 30 * 60 * 1000);
          setChecking((c) => c && Date.now() - new Date(s.checked_at).getTime() > 5000);
          const fresh = new Date(s.checked_at).getTime() > requestedAt.current; // ignore an error from an older attempt
          setPhase((p) =>
            p === "idle" ? p : s.applying ? "applying" : s.error && fresh ? "idle" : p === "waiting" ? p : "restarting"
          );
        })
        .catch(() => setPhase((p) => (p === "idle" ? p : "restarting"))),
    []
  );

  useEffect(() => {
    load();
    const id = setInterval(load, busy || checking ? 3000 : 15000);
    return () => clearInterval(id);
  }, [load, busy, checking]);

  // the new release is serving: reload so the browser runs the new frontend too
  const done = Boolean(target && status?.configured && status.current_version === target && !status.applying);
  useEffect(() => {
    if (!done) return;
    const t = setTimeout(() => window.location.reload(), 1500);
    return () => clearTimeout(t);
  }, [done]);

  async function requestUpdate() {
    if (!status?.configured) return;
    setError(null);
    try {
      requestedAt.current = Date.now();
      await updateApi.request();
      setTarget(status.latest_version);
      setPhase("waiting");
    } catch (e) {
      setError(failure(e, "Could not request an update. Check that the updater container is running, then retry."));
    }
  }

  async function checkNow() {
    setChecking(true);
    setCheckResult(null);
    await updateApi.check().catch(() => {
      setChecking(false);
      setCheckResult("failed");
    });
  }

  if (!status) {
    return (
      <div className="flex flex-col gap-5" aria-hidden>
        <PanelHead title="Updates" />
        <div className="ledger p-5 flex flex-col gap-3">
          <span className="skeleton h-6 w-32" />
          <span className="skeleton h-4 w-56" />
        </div>
      </div>
    );
  }

  if (!status.configured) {
    return (
      <div className="flex flex-col gap-5">
        <PanelHead title="Updates" />
        <div className="ledger p-5 flex flex-col gap-3">
          <div className="flex items-center gap-2 text-[13px] font-medium">
            <span className="dot" style={{ background: "var(--warning)" }} aria-hidden />
            Waiting for the updater
          </div>
          <p className="text-[13px] max-w-[62ch]" style={{ color: "var(--ink-dim)" }}>
            It ships with Eunomia as the <code className="font-mono">updater</code> service and reports in within a minute of
            starting. If this message stays, this install predates it: run the installer once more from the folder that
            contains your install. It updates in place and keeps your data and settings.
          </p>
          <CopyField value={INSTALL_CMD} />
        </div>
      </div>
    );
  }

  const notes = `https://github.com/Qyrhal/Eunomia/releases/tag/${encodeURIComponent(status.latest_version)}`;
  const line = done
    ? `Updated to ${target}. Reloading…`
    : phase === "waiting"
      ? "Update requested. Starting in a few seconds…"
      : phase === "applying"
        ? `Installing ${target ?? status.latest_version}…`
        : phase === "restarting"
          ? "Restarting Eunomia…"
          : status.update_available
            ? `${status.latest_version} is available`
            : "You're on the latest release";
  const tone = busy || done ? "var(--accent)" : status.update_available ? "var(--warning)" : "var(--good)";
  const progress = done ? 1 : phase === "restarting" ? 0.8 : phase === "applying" ? 0.5 : 0.15;

  return (
    <div className="flex flex-col gap-5">
      <PanelHead
        title="Updates"
        action={
          <button onClick={checkNow} disabled={checking || busy} className="btn btn-sm shrink-0" aria-label="Check for updates">
            {checking || checkResult ? <SyncMark status={checking ? "running" : checkResult!} /> : <RefreshCw {...ICON} />}
            {checking ? "Checking…" : "Check now"}
          </button>
        }
      >
        Checked {relativeTime(status.checked_at)}. Updating takes about a minute and your data stays put.
      </PanelHead>

      <div className="ledger overflow-hidden">
        <div className="p-5 flex flex-wrap items-center justify-between gap-4">
          <div className="min-w-0 flex flex-col gap-1">
            <span className="label">Running version</span>
            <span className="text-[20px] font-mono font-medium tracking-[-0.01em]">{status.current_version}</span>
            <span className="flex items-center gap-2 text-[12.5px]" style={{ color: "var(--ink-dim)" }}>
              <span className="dot" style={{ background: tone }} aria-hidden />
              <span role="status">{line}</span>
            </span>
          </div>
          {status.update_available && !busy && !done && (
            <div className="flex items-center gap-2 shrink-0">
              <a
                href={notes}
                target="_blank"
                rel="noopener noreferrer"
                className="btn btn-ghost btn-sm"
                style={{ color: "var(--accent-text)" }}
              >
                What&apos;s new
              </a>
              <button onClick={requestUpdate} className="btn btn-primary">
                <Download {...ICON} />
                Update now
              </button>
            </div>
          )}
        </div>
        {(busy || done) && (
          <div className="h-[3px]" style={{ background: "var(--surface-raised)" }} aria-hidden>
            <div
              className="h-full origin-left"
              style={{
                background: "var(--accent)",
                transform: `scaleX(${progress})`,
                transition: "transform 700ms var(--ease-out)",
              }}
            />
          </div>
        )}
      </div>

      {stale && !busy && (
        <p className="text-[12.5px]" style={{ color: "var(--warning)" }}>
          The updater hasn&apos;t reported for a while. Check that the <code className="font-mono">updater</code> container is
          running (<code className="font-mono">docker compose ps</code>).
        </p>
      )}
      {status.error && !busy && <ErrorLine>Last update attempt failed: {status.error}. Check the updater logs, then retry.</ErrorLine>}
      {error && <ErrorLine error={error} />}
    </div>
  );
}

function GeneralSection({ settings, onSaved }: { settings: AppSettings; onSaved: (s: AppSettings) => void }) {
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [baseUrlInput, setBaseUrlInput] = useState(settings.openai_base_url);
  const [modelInput, setModelInput] = useState(settings.embedding_model);
  const [models, setModels] = useState<string[]>([]);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<Failure | null>(null);

  const urlInvalid = baseUrlInput.trim() !== "" && !/^https?:\/\/\S+$/.test(baseUrlInput.trim());
  const dirty =
    apiKeyInput !== "" || baseUrlInput.trim() !== settings.openai_base_url || modelInput.trim() !== settings.embedding_model;

  useEffect(() => {
    settingsApi
      .openaiModels()
      .then((res) => {
        setModels(res.models);
        setModelsError(res.error);
      })
      .catch(() => setModelsError("Could not reach the models endpoint."));
  }, [settings.openai_base_url, settings.openai_api_key_set]);

  async function saveSettings() {
    if (urlInvalid) return;
    const payload: Record<string, unknown> = {
      theme: settings.theme,
      openai_base_url: baseUrlInput.trim(),
      embedding_model: modelInput.trim(),
    };
    if (apiKeyInput) payload.openai_api_key = apiKeyInput;
    setSaving(true);
    setError(null);
    try {
      const updated = await settingsApi.update(payload);
      onSaved(updated);
      setApiKeyInput("");
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
    } catch (e) {
      setError(failure(e, "Could not save settings. Check the values above and try again."));
    } finally {
      setSaving(false);
    }
  }

  const row = "grid gap-1.5 md:grid-cols-[180px_minmax(0,1fr)] md:gap-6 px-5 py-4 items-start";

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col gap-3">
        <PanelHead title="Connectors" />
        <Link href="/connectors" className="ledger group px-4 py-3 flex items-center gap-3 transition-colors hover:bg-[var(--surface-raised)]">
          <Plug {...ICON} style={{ color: "var(--ink-faint)" }} aria-hidden />
          <div className="flex-1 min-w-0">
            <div className="text-[13.5px] font-medium">Connectors</div>
            <div className="label mt-0.5">Up Bank, PocketAI, Open Connector: credentials and connection status</div>
          </div>
          <span className="text-[12.5px] flex items-center gap-1" style={{ color: "var(--accent-text)" }}>
            Manage
            <ChevronRight {...ICON} aria-hidden />
          </span>
        </Link>
      </div>

      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          saveSettings();
        }}
      >
        <PanelHead title="OpenAI (optional)">
          Not required: an agent connected over MCP (Claude, Codex and others) can recall and write memory using its own
          model. Adding one saves agent tokens, since Eunomia then does embeddings and answer synthesis itself and powers the
          in-app chat. Any OpenAI-compatible endpoint works, a local model server or proxy included.
        </PanelHead>

        <div className="ledger hairline-rows">
          <div className={row}>
            <label htmlFor="base-url" className="text-[13px] font-medium md:pt-1.5">
              Base URL
            </label>
            <div className="flex flex-col gap-1">
              <input
                id="base-url"
                className="field h-8 px-3 text-[13px] font-mono"
                value={baseUrlInput}
                onChange={(e) => setBaseUrlInput(e.target.value)}
                placeholder="https://api.openai.com/v1"
                aria-invalid={urlInvalid || undefined}
                style={urlInvalid ? { borderColor: "var(--critical)" } : undefined}
              />
              {urlInvalid && (
                <span className="text-[12px]" style={{ color: "var(--critical)" }}>
                  Start the URL with http:// or https://, for example https://api.openai.com/v1.
                </span>
              )}
            </div>
          </div>

          <div className={row}>
            <label htmlFor="api-key" className="text-[13px] font-medium md:pt-1.5 flex items-center gap-2">
              API key
              {settings.openai_api_key_set && (
                <span className="label flex items-center gap-1.5">
                  <span className="dot" style={{ background: "var(--good)" }} aria-hidden />
                  set
                </span>
              )}
            </label>
            <input
              id="api-key"
              type="password"
              className="field h-8 px-3 text-[13px]"
              value={apiKeyInput}
              onChange={(e) => setApiKeyInput(e.target.value)}
              placeholder={settings.openai_api_key_set ? "Leave blank to keep the current key" : "Optional, not every base URL needs one"}
              autoComplete="off"
            />
          </div>

          <div className={row}>
            <label htmlFor="model" className="text-[13px] font-medium md:pt-1.5">
              Model
            </label>
            <div className="flex flex-col gap-1">
              {models.length > 0 ? (
                <Select
                  id="model"
                  mono
                  className="h-8 text-[13px] w-full"
                  value={modelInput}
                  onChange={setModelInput}
                  options={[...(!models.includes(modelInput) && modelInput ? [modelInput] : []), ...models].map((m) => ({ value: m, label: m }))}
                />
              ) : (
                <input
                  id="model"
                  className="field h-8 px-3 text-[13px] font-mono"
                  value={modelInput}
                  onChange={(e) => setModelInput(e.target.value)}
                  placeholder="text-embedding-3-small"
                />
              )}
              {modelsError && (
                <span className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
                  Couldn&apos;t list models from this endpoint ({modelsError}). Type a model id directly.
                </span>
              )}
            </div>
          </div>

          <div className="px-5 py-3 flex items-center justify-end gap-3" style={{ background: "var(--surface-raised)", borderRadius: "0 0 var(--radius-card) var(--radius-card)" }}>
            <span className="label mr-auto" aria-live="polite">
              {saved ? "Saved." : dirty ? "Unsaved changes" : ""}
            </span>
            <button type="submit" disabled={saving || urlInvalid} className="btn btn-primary">
              {saved && <Check {...ICON} />}
              {saving ? "Saving…" : saved ? "Saved" : "Save settings"}
            </button>
          </div>
        </div>
        {error && <ErrorLine error={error} />}
      </form>
    </div>
  );
}

function DataSection() {
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<Failure | null>(null);

  async function doExport() {
    setExporting(true);
    setError(null);
    try {
      await downloadExport();
    } catch (e) {
      setError(failure(e, "Could not prepare the export.", " Try again in a moment."));
    } finally {
      setExporting(false);
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <PanelHead title="Your data">
        Download everything Eunomia holds about you (entities, memory, relations and chat history) as one JSON file.
      </PanelHead>
      <div className="ledger px-5 py-4 flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className="text-[13px] font-medium">Export</div>
          <div className="label mt-0.5 font-mono">eunomia-export.json</div>
        </div>
        <button onClick={doExport} disabled={exporting} className="btn">
          <Download {...ICON} />
          {exporting ? "Preparing…" : "Download my data"}
        </button>
      </div>
      {error && <ErrorLine error={error} />}
    </div>
  );
}

const TABS = [
  { id: "general", label: "General" },
  { id: "updates", label: "Updates" },
  { id: "tokens", label: "API tokens" },
  { id: "apps", label: "Connected apps" },
  { id: "sessions", label: "Sessions" },
  { id: "data", label: "Your data" },
] as const;

type TabId = (typeof TABS)[number]["id"];

const isTab = (t: string | null): t is TabId => TABS.some((x) => x.id === t);

export default function SettingsPage() {
  // ?tab=updates (the sidebar's "Update available" link) opens that tab; any tab id works
  const [tab, setTab] = useState<TabId>(() => {
    if (typeof window === "undefined") return "general";
    const q = new URLSearchParams(window.location.search).get("tab");
    return isTab(q) ? q : "general";
  });
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loadError, setLoadError] = useState(false);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  useEffect(() => {
    settingsApi
      .get()
      .then(setSettings)
      .catch(() => setLoadError(true));
  }, []);

  function choose(id: TabId) {
    setTab(id);
    const url = new URL(window.location.href);
    if (id === "general") url.searchParams.delete("tab");
    else url.searchParams.set("tab", id);
    window.history.replaceState(null, "", url);
  }

  function onKey(e: React.KeyboardEvent, i: number) {
    const next = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
    if (!next) return;
    e.preventDefault();
    const j = (i + next + TABS.length) % TABS.length;
    choose(TABS[j].id);
    tabRefs.current[j]?.focus();
  }

  return (
    <div className="max-w-5xl flex flex-col gap-7">
      <h1 className="page-title">Settings</h1>

      {!settings ? (
        loadError ? (
          <ErrorLine>Could not load settings. Check that the Eunomia API is running, then reload this page.</ErrorLine>
        ) : (
          <div className="grid gap-8 md:grid-cols-[180px_minmax(0,1fr)]" aria-hidden>
            <div className="flex md:flex-col gap-2">
              {TABS.map((t) => (
                <span key={t.id} className="skeleton h-7 w-28" />
              ))}
            </div>
            <div className="flex flex-col gap-3">
              <span className="skeleton h-4 w-40" />
              <span className="skeleton h-40 w-full" />
            </div>
          </div>
        )
      ) : (
        <div className="grid gap-6 md:gap-10 md:grid-cols-[180px_minmax(0,1fr)] items-start">
          <div
            role="tablist"
            aria-label="Settings sections"
            aria-orientation="vertical"
            className="flex md:flex-col gap-0.5 overflow-x-auto -mx-4 px-4 md:mx-0 md:px-0 md:sticky md:top-6"
          >
            {TABS.map((t, i) => (
              <button
                key={t.id}
                ref={(el) => {
                  tabRefs.current[i] = el;
                }}
                role="tab"
                id={`tab-${t.id}`}
                aria-selected={tab === t.id}
                aria-controls={`panel-${t.id}`}
                tabIndex={tab === t.id ? 0 : -1}
                onClick={() => choose(t.id)}
                onKeyDown={(e) => onKey(e, i)}
                data-active={tab === t.id ? "" : undefined}
                className="nav-link shrink-0 h-8 px-2.5 rounded-[7px] text-[13px] text-left whitespace-nowrap transition-transform active:scale-[0.97]"
              >
                {t.label}
              </button>
            ))}
          </div>

          <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`} className="min-w-0 max-w-3xl">
            {tab === "general" && <GeneralSection settings={settings} onSaved={setSettings} />}
            {tab === "updates" && <UpdateSection />}
            {tab === "tokens" && <TokensSection />}
            {tab === "apps" && <ConnectedAppsSection />}
            {tab === "sessions" && <SessionsSection />}
            {tab === "data" && <DataSection />}
          </div>
        </div>
      )}
    </div>
  );
}
