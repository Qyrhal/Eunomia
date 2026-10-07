"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { Check, Copy, Download, Plug, RefreshCw, Trash2 } from "lucide-react";
import {
  auth,
  downloadExport,
  settings as settingsApi,
  update as updateApi,
  type ApiToken,
  type AppSettings,
  type Session,
  type UpdateStatus,
} from "@/lib/api";

function CopyField({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    await navigator.clipboard.writeText(value).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }
  return (
    <div className="field flex items-center gap-2 px-3 py-2">
      <code className="flex-1 min-w-0 truncate text-[12px] font-mono" style={{ color: "var(--ink-dim)" }}>
        {value}
      </code>
      <button onClick={copy} aria-label="Copy" className="shrink-0" style={{ color: "var(--ink-faint)" }}>
        {copied ? <Check size={13} color="var(--good)" /> : <Copy size={13} />}
      </button>
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

function TokensSection() {
  const [tokens, setTokens] = useState<ApiToken[] | null>(null);
  const [name, setName] = useState("");
  const [minted, setMinted] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () => auth.tokens.list().then(setTokens).catch(() => setTokens([]));
  useEffect(() => {
    load();
  }, []);

  async function create() {
    setBusy(true);
    try {
      const res = await auth.tokens.create(name.trim() || "API token");
      setMinted(res.token);
      setName("");
      await load();
    } finally {
      setBusy(false);
    }
  }

  async function revoke(id: string) {
    await auth.tokens.revoke(id);
    await load();
  }

  return (
    <section className="ledger p-6 flex flex-col gap-4">
      <div>
        <div className="eyebrow">API tokens</div>
        <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
          Personal tokens for MCP clients or scripts. Each is shown once, at creation.
        </p>
      </div>

      {minted && (
        <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
          New token — store it now, it won&apos;t be shown again
          <CopyField value={minted} />
        </label>
      )}

      <div className="flex items-center gap-2">
        <input
          className="field flex-1 px-3 py-2 text-[13px]"
          placeholder="Name this token (e.g. laptop, MCP)"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button
          onClick={create}
          disabled={busy}
          className="px-4 py-2 text-[13px] font-medium rounded-xl disabled:opacity-50"
          style={{ background: "var(--felt)", color: "var(--canvas)" }}
        >
          Create
        </button>
      </div>

      <div className="flex flex-col gap-2">
        {tokens?.length === 0 && (
          <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
            No tokens yet.
          </p>
        )}
        {tokens?.map((t) => (
          <div key={t.id} className="field flex items-center justify-between px-3 py-2">
            <div>
              <div className="text-[13px]">{t.name}</div>
              <div className="text-[11px]" style={{ color: "var(--ink-faint)" }}>
                last used {relativeTime(t.last_used_at)}
              </div>
            </div>
            <button onClick={() => revoke(t.id)} aria-label="Revoke" style={{ color: "var(--critical)" }}>
              <Trash2 size={14} />
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}

function SessionsSection() {
  const [sessions, setSessions] = useState<Session[] | null>(null);

  const load = () => auth.sessions.list().then(setSessions).catch(() => setSessions([]));
  useEffect(() => {
    load();
  }, []);

  async function revoke(id: string) {
    await auth.sessions.revoke(id);
    await load();
  }

  return (
    <section className="ledger p-6 flex flex-col gap-4">
      <div>
        <div className="eyebrow">Active sessions</div>
        <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
          Browser logins. Revoking one signs that browser out immediately.
        </p>
      </div>
      <div className="flex flex-col gap-2">
        {sessions?.length === 0 && (
          <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
            No active sessions.
          </p>
        )}
        {sessions?.map((s) => (
          <div key={s.id} className="field flex items-center justify-between px-3 py-2">
            <div>
              <div className="text-[13px] truncate max-w-md">{s.user_agent || "Unknown device"}</div>
              <div className="text-[11px]" style={{ color: "var(--ink-faint)" }}>
                last seen {relativeTime(s.last_seen_at)}
              </div>
            </div>
            <button onClick={() => revoke(s.id)} aria-label="Revoke" style={{ color: "var(--critical)" }}>
              <Trash2 size={14} />
            </button>
          </div>
        ))}
      </div>
    </section>
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
  const [stale, setStale] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestedAt = useRef(0);

  const busy = phase !== "idle";

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
      setError(e instanceof Error ? e.message : "Could not request an update.");
    }
  }

  async function checkNow() {
    setChecking(true);
    await updateApi.check().catch(() => setChecking(false));
  }

  if (!status || !status.configured) {
    return (
      <section className="ledger p-6 flex flex-col gap-3">
        <div className="eyebrow">Updates</div>
        <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
          Waiting for the updater. It ships with Eunomia as the <code className="font-mono">updater</code> service and
          reports in within a minute of starting. If this message stays, this install predates it: run the installer
          once more from the folder that contains your install. It updates in place and keeps your data and settings.
        </p>
        <CopyField value={INSTALL_CMD} />
      </section>
    );
  }

  const notes = `https://github.com/Qyrhal/Eunomia/releases/tag/${encodeURIComponent(status.latest_version)}`;
  const line = done
    ? `Updated to ${target} — reloading…`
    : phase === "waiting"
      ? "Update requested — starting in a few seconds…"
      : phase === "applying"
        ? `Installing ${target ?? status.latest_version}…`
        : phase === "restarting"
          ? "Restarting Eunomia…"
          : status.update_available
            ? `${status.latest_version} is available`
            : "You're on the latest release";

  return (
    <section className="ledger p-6 flex flex-col gap-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="eyebrow">Updates</div>
          <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
            Checked {relativeTime(status.checked_at)}. Updating takes about a minute; your data stays put.
          </p>
        </div>
        <button
          onClick={checkNow}
          disabled={checking || busy}
          className="pill disabled:opacity-50 shrink-0"
          aria-label="Check for updates"
        >
          <RefreshCw size={12} className={checking ? "animate-spin" : ""} />
          {checking ? "Checking…" : "Check now"}
        </button>
      </div>
      {stale && !busy && (
        <p className="text-[12px]" style={{ color: "var(--warning)" }}>
          The updater hasn&apos;t reported for a while. Check that the <code className="font-mono">updater</code> container
          is running (<code className="font-mono">docker compose ps</code>).
        </p>
      )}

      <div className="field flex items-center justify-between gap-3 px-3 py-2.5">
        <div className="min-w-0">
          <div className="text-[13px] font-mono">{status.current_version}</div>
          <div className="text-[11.5px]" style={{ color: busy || done ? "var(--ink)" : "var(--ink-faint)" }} role="status">
            {line}
          </div>
        </div>
        {status.update_available && !busy && !done && (
          <div className="flex items-center gap-3 shrink-0">
            <a href={notes} target="_blank" rel="noopener noreferrer" className="text-[12px] underline" style={{ color: "var(--ink-dim)" }}>
              What&apos;s new
            </a>
            <button
              onClick={requestUpdate}
              className="px-3 py-1.5 text-[12.5px] font-medium rounded-lg flex items-center gap-1.5"
              style={{ background: "var(--felt)", color: "var(--canvas)" }}
            >
              <Download size={13} />
              Update now
            </button>
          </div>
        )}
      </div>
      {(busy || done) && (
        <div className="h-1 rounded-full overflow-hidden" style={{ background: "var(--surface-raised)" }} aria-hidden>
          <div
            className="h-full transition-all duration-700"
            style={{
              background: "var(--felt)",
              width: done ? "100%" : phase === "restarting" ? "80%" : phase === "applying" ? "50%" : "15%",
            }}
          />
        </div>
      )}

      {status.error && !busy && (
        <p className="text-[12px]" style={{ color: "var(--critical)" }}>
          Last update attempt failed: {status.error}
        </p>
      )}
      {error && (
        <p className="text-[12px]" style={{ color: "var(--critical)" }}>
          {error}
        </p>
      )}
    </section>
  );
}

const TABS = [
  { id: "general", label: "General" },
  { id: "updates", label: "Updates" },
  { id: "tokens", label: "API tokens" },
  { id: "sessions", label: "Sessions" },
  { id: "data", label: "Your data" },
] as const;

type TabId = (typeof TABS)[number]["id"];

export default function SettingsPage() {
  // ?tab=updates (the sidebar's "Update available" link) opens that tab
  const [tab, setTab] = useState<TabId>(() =>
    typeof window !== "undefined" && new URLSearchParams(window.location.search).get("tab") === "updates" ? "updates" : "general"
  );
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [baseUrlInput, setBaseUrlInput] = useState("");
  const [modelInput, setModelInput] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    settingsApi.get().then((s) => {
      setSettings(s);
      setBaseUrlInput(s.openai_base_url);
      setModelInput(s.embedding_model);
    });
  }, []);

  useEffect(() => {
    settingsApi
      .openaiModels()
      .then((res) => {
        setModels(res.models);
        setModelsError(res.error);
      })
      .catch(() => setModelsError("Could not reach the models endpoint."));
  }, [settings?.openai_base_url, settings?.openai_api_key_set]);

  async function saveSettings() {
    if (!settings) return;
    const payload: Record<string, unknown> = {
      theme: settings.theme,
      openai_base_url: baseUrlInput.trim(),
      embedding_model: modelInput.trim(),
    };
    if (apiKeyInput) payload.openai_api_key = apiKeyInput;
    const updated = await settingsApi.update(payload);
    setSettings(updated);
    setApiKeyInput("");
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  }

  async function doExport() {
    setExporting(true);
    try {
      await downloadExport();
    } finally {
      setExporting(false);
    }
  }

  if (!settings) return null;

  return (
    <div className="max-w-2xl flex flex-col gap-6">
      <div>
        <div className="eyebrow mb-2">Settings</div>
        <h1 className="font-display text-3xl">The instrument</h1>
      </div>

      <div className="flex items-center gap-1 border-b" style={{ borderColor: "var(--border)" }} role="tablist">
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            aria-selected={tab === t.id}
            onClick={() => setTab(t.id)}
            className="px-3.5 py-2.5 text-[13px] font-medium -mb-px border-b-2 transition-colors"
            style={{
              borderColor: tab === t.id ? "var(--felt)" : "transparent",
              color: tab === t.id ? "var(--ink)" : "var(--ink-faint)",
            }}
          >
            {t.label}
          </button>
        ))}
      </div>

      {tab === "general" && (
        <div className="flex flex-col gap-6">
          <Link href="/connectors" className="ledger p-5 flex items-center gap-3.5 hover:opacity-90">
            <div className="w-8 h-8 rounded-lg flex items-center justify-center" style={{ background: "var(--surface-raised)", color: "var(--ink-dim)" }}>
              <Plug size={16} />
            </div>
            <div className="flex-1">
              <div className="text-[13.5px] font-medium">Connectors</div>
              <div className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
                Up Bank, PocketAI, Open Connector — credentials and connection status
              </div>
            </div>
            <span className="text-[12px]" style={{ color: "var(--ink)" }}>
              Manage →
            </span>
          </Link>

          <section className="ledger p-6 flex flex-col gap-4">
            <div className="eyebrow">OpenAI · optional</div>
            <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
              Not required: an AI agent connected over MCP (Claude, Codex, …) can recall and write memory
              using its own model. Adding one is recommended to save tokens — Eunomia then does embeddings
              and answer synthesis itself, and powers the in-app chat. Point this at any OpenAI-compatible
              endpoint — a local model server or proxy included — not just api.openai.com.
            </p>

            <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
              Base URL
              <input
                className="field px-3 py-2.5 text-[13.5px] font-mono"
                value={baseUrlInput}
                onChange={(e) => setBaseUrlInput(e.target.value)}
                placeholder="https://api.openai.com/v1"
              />
            </label>

            <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
              API key {settings.openai_api_key_set && <span style={{ color: "var(--good)" }}>· set</span>}
              <input
                type="password"
                className="field px-3 py-2.5 text-[13.5px] font-mono"
                value={apiKeyInput}
                onChange={(e) => setApiKeyInput(e.target.value)}
                placeholder={settings.openai_api_key_set ? "leave blank to keep" : "optional — not every base URL needs one"}
              />
            </label>

            <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
              Model
              {models.length > 0 ? (
                <select
                  className="field px-3 py-2.5 text-[13.5px] font-mono"
                  value={modelInput}
                  onChange={(e) => setModelInput(e.target.value)}
                >
                  {!models.includes(modelInput) && modelInput && <option value={modelInput}>{modelInput}</option>}
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              ) : (
                <input
                  className="field px-3 py-2.5 text-[13.5px] font-mono"
                  value={modelInput}
                  onChange={(e) => setModelInput(e.target.value)}
                  placeholder="text-embedding-3-small"
                />
              )}
              {modelsError && (
                <span className="text-[11px]" style={{ color: "var(--ink-faint)" }}>
                  Couldn&apos;t list models from this endpoint ({modelsError}) — type a model id directly.
                </span>
              )}
            </label>
          </section>

          <button
            onClick={saveSettings}
            className="self-start px-5 py-2.5 text-[13px] font-medium rounded-xl"
            style={{ background: "var(--felt)", color: "var(--canvas)" }}
          >
            {saved ? "Saved" : "Save settings"}
          </button>
        </div>
      )}

      {tab === "updates" && <UpdateSection />}
      {tab === "tokens" && <TokensSection />}
      {tab === "sessions" && <SessionsSection />}

      {tab === "data" && (
        <section className="ledger p-6 flex flex-col gap-3">
          <div className="eyebrow">Your data</div>
          <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
            Download everything Eunomia holds about you — entities, memory, relations, and chat history —
            as one JSON file.
          </p>
          <button
            onClick={doExport}
            disabled={exporting}
            className="self-start field px-4 py-2 text-[13px] flex items-center gap-2 disabled:opacity-50"
            style={{ color: "var(--ink)" }}
          >
            <Download size={14} />
            {exporting ? "Preparing…" : "Download my data"}
          </button>
        </section>
      )}
    </div>
  );
}
