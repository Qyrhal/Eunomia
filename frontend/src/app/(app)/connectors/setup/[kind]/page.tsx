"use client";

import { use, useEffect, useState } from "react";
import Link from "next/link";
import { ArrowRight, Check, ChevronRight, Copy, Eye, EyeOff } from "lucide-react";
import { apiOrigin, auth as authApi, connectors as connectorsApi, settings as settingsApi, type AppSettings, type Connector, type ConnectorKind } from "@/lib/api";
import { CONNECTOR_META, CONNECTOR_ORDER, ConnectorTile, connectorStatus, type FieldDef } from "@/lib/connectorMeta";

// Presets for the sync-interval select, in seconds.
const SYNC_INTERVAL_PRESETS = [
  { value: 300, label: "5 minutes" },
  { value: 900, label: "15 minutes" },
  { value: 1800, label: "30 minutes" },
  { value: 3600, label: "1 hour" },
  { value: 21600, label: "6 hours" },
  { value: 86400, label: "24 hours" },
];

// Fallback interval a source runs at when the user hasn't set an override,
// per the backend scheduler (heypocket defaults to 24h there; every other
// source falls back to the scheduler's generic 900s/15min default).
const SOURCE_DEFAULT_INTERVAL: Record<string, number> = {
  up_bank: 900,
  heypocket: 86400,
};

function isConnectorKind(kind: string): kind is ConnectorKind {
  return (CONNECTOR_ORDER as string[]).includes(kind);
}

/** Webhook signing secrets are optional: polling works without them. */
const isOptional = (f: FieldDef) => f.key.includes("webhook");

/** Names the problem and the fix for one field value, or null when it is fine. */
function validate(f: FieldDef, value: string, required: boolean, connector: string): string | null {
  const v = value.trim();
  if (!v) return required && f.secret && !isOptional(f) ? `Enter your ${f.label.toLowerCase()} to connect ${connector}.` : null;
  if (f.key === "base_url" && !/^https?:\/\/[^\s/]+/i.test(v)) {
    return `${f.label} must be a full address starting with http:// or https://, for example ${f.placeholder}.`;
  }
  if (f.key === "channel_id" && !/^\d{5,}$/.test(v)) {
    return "Channel ID is a long number. In Discord, right-click the channel and choose Copy Channel ID.";
  }
  return null;
}

function Breadcrumb({ label }: { label: string }) {
  return (
    <nav aria-label="Breadcrumb" className="flex items-center gap-1.5 text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
      <Link href="/connectors" className="hover:underline" style={{ color: "var(--ink-dim)" }}>
        Connectors
      </Link>
      <ChevronRight size={12} strokeWidth={1.75} aria-hidden />
      <span aria-current="page">{label}</span>
    </nav>
  );
}

export default function ConnectorSetupPage({ params }: { params: Promise<{ kind: string }> }) {
  const { kind } = use(params);

  if (!isConnectorKind(kind)) {
    return (
      <div className="max-w-2xl flex flex-col gap-6">
        <Breadcrumb label={kind} />
        <h1 className="page-title">Unknown connector</h1>
        <div className="ledger px-5 py-4 flex items-center justify-between gap-4 flex-wrap text-[13px]">
          <span style={{ color: "var(--ink-dim)" }}>There is no connector called &ldquo;{kind}&rdquo;.</span>
          <Link href="/connectors" className="btn btn-sm">
            Browse connectors
          </Link>
        </div>
      </div>
    );
  }

  return <ConnectorSetup kind={kind} />;
}

function SecretToggle({ shown, onToggle, label }: { shown: boolean; onToggle: () => void; label: string }) {
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-label={shown ? `Hide ${label}` : `Show ${label}`}
      aria-pressed={shown}
      className="btn btn-ghost btn-sm w-6 px-0 -mr-1.5"
    >
      {shown ? <EyeOff size={14} strokeWidth={1.75} /> : <Eye size={14} strokeWidth={1.75} />}
    </button>
  );
}

function ConnectorSetup({ kind }: { kind: ConnectorKind }) {
  const meta = CONNECTOR_META[kind];
  const [connector, setConnector] = useState<Connector | undefined | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [shown, setShown] = useState<Record<string, boolean>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [savedFlash, setSavedFlash] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; error?: string } | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettings | undefined>(undefined);
  const [intervalState, setIntervalState] = useState<"idle" | "saved" | "error">("idle");
  const [webhookUrl, setWebhookUrl] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);

  const load = () =>
    connectorsApi
      .list()
      .then((list) => setConnector(list.find((c) => c.kind === kind)))
      .catch((e: Error) => setLoadError(e.message));
  useEffect(() => {
    load();
    if (meta.sourceKey) settingsApi.get().then(setAppSettings).catch(() => undefined);
    if (meta.webhooks)
      authApi
        .me()
        .then((me) => setWebhookUrl(`${apiOrigin()}/api/sources/${kind}/webhook/${me.id}`))
        .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kind]);

  const status = connectorStatus(connector ?? undefined);
  const isDemo = status === "demo";
  const connected = status === "connected";

  function setField(key: string, value: string) {
    setValues((s) => ({ ...s, [key]: value }));
    if (errors[key]) setErrors((s) => ({ ...s, [key]: "" }));
  }

  async function test() {
    setTesting(true);
    setTestResult(null);
    try {
      setTestResult(await connectorsApi.test(kind));
    } catch (e) {
      setTestResult({ ok: false, error: (e as Error).message });
    } finally {
      setTesting(false);
    }
  }

  async function save(e: React.FormEvent) {
    e.preventDefault();
    const nextErrors: Record<string, string> = {};
    for (const f of meta.fields) {
      const msg = validate(f, values[f.key] ?? "", !connected, meta.label);
      if (msg) nextErrors[f.key] = msg;
    }
    setErrors(nextErrors);
    const firstBad = meta.fields.find((f) => nextErrors[f.key]);
    if (firstBad) {
      document.getElementById(`field-${firstBad.key}`)?.focus();
      return;
    }

    const credentials: Record<string, string> = {};
    const config: Record<string, string> = {};
    for (const f of meta.fields) {
      const v = values[f.key]?.trim();
      if (!v) continue;
      if (f.secret || f.key === "client_id") credentials[f.key] = v;
      else config[f.key] = v;
    }
    const body: Record<string, unknown> = { enabled: true };
    if (Object.keys(credentials).length) body.credentials = credentials;
    if (Object.keys(config).length) body.config = config;

    setSaving(true);
    setSaveError(null);
    try {
      await connectorsApi.update(kind, body);
    } catch (err) {
      setSaveError((err as Error).message);
      return;
    } finally {
      setSaving(false);
    }
    setValues({});
    setShown({});
    setSavedFlash(true);
    setTimeout(() => setSavedFlash(false), 1400);
    await load();
    // Saving is the moment people want to know it works: check it for them.
    test();
  }

  async function saveInterval(seconds: number) {
    if (!meta.sourceKey) return;
    try {
      const updated = await settingsApi.update({
        sync_intervals: { ...(appSettings?.sync_intervals ?? {}), [meta.sourceKey]: seconds },
      });
      setAppSettings(updated);
      setIntervalState("saved");
      setTimeout(() => setIntervalState("idle"), 1400);
    } catch {
      setIntervalState("error");
    }
  }

  async function copyWebhook() {
    if (!webhookUrl) return;
    try {
      await navigator.clipboard.writeText(webhookUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 1400);
    } catch {
      document.getElementById("field-webhook-url")?.focus();
      (document.getElementById("field-webhook-url") as HTMLInputElement | null)?.select();
    }
  }

  async function disconnect() {
    setDisconnecting(true);
    setSaveError(null);
    try {
      await connectorsApi.update(kind, { enabled: false });
      setConfirmDisconnect(false);
      setTestResult(null);
      await load();
    } catch (err) {
      setSaveError(`Could not disconnect: ${(err as Error).message}. Try again.`);
    } finally {
      setDisconnecting(false);
    }
  }

  const statusText = connector === null ? "Checking…" : isDemo ? "Demo data" : connected ? "Connected" : "Not connected";
  const statusTone = isDemo ? "var(--warning)" : connected ? "var(--good)" : "var(--ink-faint)";
  const sourceKey = meta.sourceKey;
  const override = sourceKey ? appSettings?.sync_intervals?.[sourceKey] : undefined;
  const defaultInterval = sourceKey ? SOURCE_DEFAULT_INTERVAL[sourceKey] ?? 900 : 900;

  return (
    <div className="max-w-2xl flex flex-col gap-8">
      <header className="flex flex-col gap-3">
        <Breadcrumb label={meta.label} />
        <div className="flex items-center gap-3 flex-wrap">
          <ConnectorTile kind={kind} size={40} />
          <div className="flex flex-col gap-0.5 min-w-0 flex-1">
            <h1 className="page-title">{meta.label}</h1>
            <span className="text-[12.5px] flex items-center gap-1.5" style={{ color: "var(--ink-dim)" }}>
              {connector === null ? <span className="skeleton w-1.5 h-1.5" /> : <span className="dot" style={{ background: statusTone }} aria-hidden />}
              {statusText}
            </span>
          </div>
          {(connected || isDemo) && sourceKey && (
            <Link href={`/connectors/${sourceKey}`} className="btn">
              View synced data <ArrowRight size={14} strokeWidth={1.75} aria-hidden />
            </Link>
          )}
        </div>
        <p className="text-[13px]" style={{ color: "var(--ink-dim)" }}>
          {meta.description}
        </p>
      </header>

      {loadError && (
        <div className="rounded-[10px] px-4 py-3 text-[13px]" style={{ background: "var(--critical-soft)", color: "var(--critical)" }} role="alert">
          Could not load this connector ({loadError}). Reload the page once the backend is reachable.
        </div>
      )}

      {isDemo && (
        <p className="text-[13px] rounded-[10px] px-4 py-3" style={{ background: "var(--surface-raised)", color: "var(--ink-dim)" }}>
          Running on demo data seeded from Settings. Save a real token below to switch over, or clear the demo data from Settings.
        </p>
      )}

      <form onSubmit={save} noValidate className="ledger flex flex-col">
        <div className="p-5 flex flex-col gap-5">
          <div className="flex flex-col gap-1">
            <h2 className="section-title">Credentials</h2>
            {meta.help && (
              <p className="text-[12.5px]" style={{ color: "var(--ink-dim)" }}>
                {meta.help}
              </p>
            )}
          </div>

          {meta.fields.map((f) => {
            const err = errors[f.key];
            const reveal = shown[f.key];
            const describedBy = err ? `err-${f.key}` : undefined;
            return (
              <div key={f.key} className="flex flex-col gap-1.5">
                <label htmlFor={`field-${f.key}`} className="text-[12.5px] font-medium flex items-baseline gap-2">
                  {f.label}
                  {isOptional(f) && <span className="label font-normal">Optional</span>}
                </label>
                <div className="field flex items-center gap-2 h-8 px-2.5" style={err ? { borderColor: "var(--critical)" } : undefined}>
                  <input
                    id={`field-${f.key}`}
                    type={f.secret && !reveal ? "password" : "text"}
                    autoComplete="off"
                    spellCheck={false}
                    className="flex-1 min-w-0 bg-transparent outline-none text-[13px] font-mono"
                    placeholder={connected && f.secret ? "Saved. Paste a new value to replace it" : f.placeholder}
                    value={values[f.key] || ""}
                    onChange={(e) => setField(f.key, e.target.value)}
                    aria-invalid={err ? true : undefined}
                    aria-describedby={describedBy}
                  />
                  {f.secret && <SecretToggle shown={!!reveal} label={f.label} onToggle={() => setShown((s) => ({ ...s, [f.key]: !s[f.key] }))} />}
                </div>
                {err && (
                  <p id={describedBy} className="text-[12px]" style={{ color: "var(--critical)" }}>
                    {err}
                  </p>
                )}
              </div>
            );
          })}

          {meta.webhooks && webhookUrl !== null && (
            <div className="flex flex-col gap-1.5">
              <label htmlFor="field-webhook-url" className="text-[12.5px] font-medium">
                Webhook URL
              </label>
              <div className="flex items-center gap-2">
                <input
                  id="field-webhook-url"
                  type="text"
                  spellCheck={false}
                  className="field flex-1 min-w-0 h-8 px-2.5 text-[12.5px] font-mono"
                  value={webhookUrl}
                  onChange={(e) => setWebhookUrl(e.target.value)}
                  aria-describedby="webhook-help"
                />
                <button type="button" className="btn btn-sm h-8 w-[84px]" onClick={copyWebhook} aria-live="polite">
                  {copied ? <Check size={13} strokeWidth={1.75} aria-hidden /> : <Copy size={13} strokeWidth={1.75} aria-hidden />}
                  {copied ? "Copied" : "Copy"}
                </button>
              </div>
              <p id="webhook-help" className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
                Register this with {meta.label} so updates arrive instantly instead of on the next poll. If {meta.label} needs a public address, replace{" "}
                <span className="font-mono">{new URL(apiOrigin()).host}</span> with your tunnel host before copying.
              </p>
            </div>
          )}
        </div>

        {saveError && (
          <p className="mx-5 mb-4 rounded-[7px] px-3 py-2 text-[12.5px]" style={{ background: "var(--critical-soft)", color: "var(--critical)" }} role="alert">
            {saveError.startsWith("Could not") ? saveError : `Could not save: ${saveError}. Check the values and try again.`}
          </p>
        )}

        <div className="px-5 py-3 flex items-center gap-2 flex-wrap border-t">
          <button type="submit" disabled={saving} className="btn btn-primary min-w-[72px]">
            {savedFlash ? "Saved" : saving ? "Saving…" : "Save"}
          </button>
          <button type="button" onClick={test} disabled={testing || !(connected || isDemo)} className="btn" title={connected || isDemo ? undefined : "Save credentials first"}>
            {testing ? "Testing…" : "Test connection"}
          </button>
          <span className="text-[12.5px] flex items-center gap-1.5 min-w-0" aria-live="polite">
            {testResult?.ok && (
              <>
                <span className="dot" style={{ background: "var(--good)" }} aria-hidden />
                Connection works
              </>
            )}
          </span>
        </div>

        {testResult && !testResult.ok && (
          <div className="mx-5 mb-4 rounded-[7px] px-3 py-2 text-[12.5px] flex flex-col gap-0.5" style={{ background: "var(--critical-soft)", color: "var(--critical)" }} role="alert">
            <span className="font-medium">{meta.label} rejected the connection.</span>
            <span className="font-mono text-[12px] break-words">{testResult.error || "The provider gave no reason."}</span>
            <span style={{ color: "var(--ink-dim)" }}>Check the token is current and has the scope described above, then save it again.</span>
          </div>
        )}
      </form>

      {sourceKey && (
        <section className="flex flex-col gap-3" aria-labelledby="sync-heading">
          <h2 id="sync-heading" className="section-title">
            Sync
          </h2>
          <div className="ledger p-5 flex items-center justify-between gap-4 flex-wrap">
            <label htmlFor="sync-interval" className="flex flex-col gap-0.5 text-[13px]">
              Sync interval
              <span className="label">How often Eunomia pulls new records from {meta.label}.</span>
            </label>
            <div className="flex items-center gap-2">
              <span className="text-[12px] w-12 text-right" aria-live="polite" style={{ color: intervalState === "error" ? "var(--critical)" : "var(--good)" }}>
                {intervalState === "saved" ? "Saved" : intervalState === "error" ? "Not saved" : ""}
              </span>
              <select id="sync-interval" className="field h-8 px-2.5 text-[13px]" value={override ?? defaultInterval} onChange={(e) => saveInterval(Number(e.target.value))}>
                {SYNC_INTERVAL_PRESETS.map((p) => (
                  <option key={p.value} value={p.value}>
                    {p.label}
                    {override === undefined && p.value === defaultInterval ? " (default)" : ""}
                  </option>
                ))}
              </select>
            </div>
          </div>
        </section>
      )}

      {connected && (
        <section className="flex flex-col gap-3" aria-labelledby="danger-heading">
          <h2 id="danger-heading" className="section-title">
            Disconnect
          </h2>
          <div className="ledger p-5 flex items-center justify-between gap-4 flex-wrap text-[13px]">
            <span style={{ color: "var(--ink-dim)" }}>
              {confirmDisconnect ? `Stop syncing ${meta.label}? Records already synced stay in memory.` : `Stops syncing. Records already synced stay in memory.`}
            </span>
            <div className="flex items-center gap-2">
              {confirmDisconnect && (
                <button type="button" className="btn btn-ghost btn-sm" onClick={() => setConfirmDisconnect(false)}>
                  Cancel
                </button>
              )}
              <button
                type="button"
                className="btn btn-danger btn-sm"
                disabled={disconnecting}
                onClick={() => (confirmDisconnect ? disconnect() : setConfirmDisconnect(true))}
              >
                {disconnecting ? "Disconnecting…" : confirmDisconnect ? `Disconnect ${meta.label}` : "Disconnect"}
              </button>
            </div>
          </div>
        </section>
      )}
    </div>
  );
}
