"use client";

import { use, useEffect, useState } from "react";
import Link from "next/link";
import { ArrowLeft, ArrowRight } from "lucide-react";
import { apiOrigin, auth as authApi, connectors as connectorsApi, settings as settingsApi, type AppSettings, type Connector, type ConnectorKind } from "@/lib/api";
import { CONNECTOR_META, connectorStatus } from "@/lib/connectorMeta";

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
// per backend/sources/scheduler.py (heypocket defaults to 24h there; every
// other source falls back to the scheduler's generic 900s/15min default).
const SOURCE_DEFAULT_INTERVAL: Record<string, number> = {
  up_bank: 900,
  heypocket: 86400,
};

const KNOWN_KINDS: ConnectorKind[] = ["up_bank", "pocketai", "open_connector"];

function isConnectorKind(kind: string): kind is ConnectorKind {
  return (KNOWN_KINDS as string[]).includes(kind);
}

export default function ConnectorSetupPage({ params }: { params: Promise<{ kind: string }> }) {
  const { kind } = use(params);

  if (!isConnectorKind(kind)) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Connector</div>
        <h1 className="font-display text-3xl mb-6">Unknown connector</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--ink-dim)" }}>
            There&apos;s no connector called &ldquo;{kind}&rdquo;.
          </p>
          <Link href="/connectors" className="field px-4 py-2 text-[13px] inline-flex items-center gap-1.5" style={{ color: "var(--ink)" }}>
            Back to connectors <ArrowRight size={13} />
          </Link>
        </div>
      </div>
    );
  }

  return <ConnectorSetup kind={kind} />;
}

function ConnectorSetup({ kind }: { kind: ConnectorKind }) {
  const meta = CONNECTOR_META[kind];
  const [connector, setConnector] = useState<Connector | undefined>(undefined);
  const [values, setValues] = useState<Record<string, string>>({});
  const [testResult, setTestResult] = useState<string | null>(null);
  const [savedFlash, setSavedFlash] = useState(false);
  const [appSettings, setAppSettings] = useState<AppSettings | undefined>(undefined);
  const [intervalSavedFlash, setIntervalSavedFlash] = useState(false);
  const [ownerId, setOwnerId] = useState<string | null>(null);
  const [webhookUrl, setWebhookUrl] = useState<string | null>(null);

  const load = () => connectorsApi.list().then((list) => setConnector(list.find((c) => c.kind === kind)));
  useEffect(() => {
    load();
    if (meta.sourceKey) settingsApi.get().then(setAppSettings);
    if (meta.webhooks)
      authApi.me().then((me) => {
        setOwnerId(me.id);
        setWebhookUrl(`${apiOrigin()}/api/sources/${kind}/webhook/${me.id}`);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kind]);

  function setField(key: string, value: string) {
    setValues((s) => ({ ...s, [key]: value }));
  }

  async function save() {
    const credentials: Record<string, string> = {};
    const config: Record<string, string> = {};
    for (const f of meta.fields) {
      const v = values[f.key];
      if (!v) continue;
      if (f.secret || f.key === "client_id") credentials[f.key] = v;
      else config[f.key] = v;
    }
    const body: Record<string, unknown> = { enabled: true };
    if (Object.keys(credentials).length) body.credentials = credentials;
    if (Object.keys(config).length) body.config = config;
    await connectorsApi.update(kind, body);
    setValues({});
    setSavedFlash(true);
    setTimeout(() => setSavedFlash(false), 1400);
    load();
  }

  async function test() {
    const res = await connectorsApi.test(kind);
    setTestResult(res.ok ? "connected" : res.error || "failed");
  }

  async function saveInterval(seconds: number) {
    if (!meta.sourceKey) return;
    const updated = await settingsApi.update({
      sync_intervals: { ...(appSettings?.sync_intervals ?? {}), [meta.sourceKey]: seconds },
    });
    setAppSettings(updated);
    setIntervalSavedFlash(true);
    setTimeout(() => setIntervalSavedFlash(false), 1400);
  }

  const status = connectorStatus(connector);
  const isDemo = status === "demo";
  const connected = status === "connected";

  return (
    <div className="max-w-2xl flex flex-col gap-6">
      <Link href="/connectors" className="text-[12.5px] inline-flex items-center gap-1.5" style={{ color: "var(--ink-faint)" }}>
        <ArrowLeft size={13} /> Back to connectors
      </Link>

      <div className="ledger overflow-hidden flex flex-col">
        <div
          className="h-36 flex items-center justify-center"
          style={{
            background: `color-mix(in srgb, ${meta.tint} 14%, var(--surface))`,
            borderBottom: "1px solid var(--border)",
          }}
        >
          <div
            className="w-20 h-20 rounded-2xl flex items-center justify-center"
            style={{
              background: `color-mix(in srgb, ${meta.tint} 22%, var(--surface))`,
              border: `1px solid color-mix(in srgb, ${meta.tint} 40%, var(--border))`,
              color: meta.tint,
            }}
          >
            {/* icon is sized 18 in CONNECTOR_META for the grid tile; scale it up for this larger hero */}
            <span style={{ display: "inline-flex", transform: "scale(1.8)" }}>{meta.icon}</span>
          </div>
        </div>

        <div className="p-6 flex flex-col gap-5">
          <div className="flex items-center justify-between gap-3">
            <h1 className="font-display text-2xl">{meta.label}</h1>
            <span
              className="eyebrow shrink-0"
              style={{ color: isDemo ? "var(--warning)" : connected ? "var(--good)" : "var(--ink-faint)" }}
            >
              {isDemo ? "Demo data" : connected ? "Connected" : "Not connected"}
            </span>
          </div>

          <p className="text-[13px]" style={{ color: "var(--ink-dim)" }}>
            {meta.description}
          </p>

          {connected && meta.sourceKey && (
            <Link
              href={`/connectors/${meta.sourceKey}`}
              className="text-[12.5px] inline-flex items-center gap-1.5 self-start"
              style={{ color: "var(--good)" }}
            >
              View synced data <ArrowRight size={13} />
            </Link>
          )}

          {isDemo && (
            <p className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
              Running on fake data seeded from Settings. Save a real token below to switch over,
              or clear the demo data from Settings.
            </p>
          )}

          {meta.help && (
            <p className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
              {meta.help}
            </p>
          )}

          <div className="grid sm:grid-cols-2 gap-3">
            {meta.fields.map((f) => (
              <label key={f.key} className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
                {f.label}
                <input
                  type={f.secret ? "password" : "text"}
                  className="field px-3 py-2 text-[13px] font-mono"
                  placeholder={f.placeholder}
                  value={values[f.key] || ""}
                  onChange={(e) => setField(f.key, e.target.value)}
                />
              </label>
            ))}
          </div>

          {meta.webhooks && ownerId && webhookUrl !== null && (
            <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
              Webhook URL — register this with the provider to push updates here instead of waiting for the next poll.
              Defaults to this app&apos;s address; edit the host if the provider needs a publicly reachable URL
              (e.g. a tunnel) instead of {new URL(apiOrigin()).host}.
              <input
                type="text"
                className="field px-3 py-2 text-[13px] font-mono"
                value={webhookUrl}
                onChange={(e) => setWebhookUrl(e.target.value)}
              />
            </label>
          )}

          {meta.sourceKey &&
            (() => {
              const sourceKey = meta.sourceKey as string;
              const override = appSettings?.sync_intervals?.[sourceKey];
              const defaultInterval = SOURCE_DEFAULT_INTERVAL[sourceKey];
              return (
                <label className="text-[12px] flex flex-col gap-1.5 pt-1" style={{ color: "var(--ink-dim)", borderTop: "1px solid var(--border)" }}>
                  <span className="pt-4">Sync interval</span>
                  <div className="flex items-center gap-3">
                    <select
                      className="field px-3 py-2 text-[13px]"
                      value={override ?? defaultInterval}
                      onChange={(e) => saveInterval(Number(e.target.value))}
                    >
                      {SYNC_INTERVAL_PRESETS.map((p) => (
                        <option key={p.value} value={p.value}>
                          {p.label}
                          {override === undefined && p.value === defaultInterval ? " (default)" : ""}
                        </option>
                      ))}
                    </select>
                    {intervalSavedFlash && (
                      <span className="text-[12px]" style={{ color: "var(--good)" }}>
                        Saved
                      </span>
                    )}
                  </div>
                </label>
              );
            })()}

          <div className="flex items-center gap-4 pt-1" style={{ borderTop: "1px solid var(--border)" }}>
            <div className="flex items-center gap-2 pt-4">
              <button onClick={save} className="px-4 py-2 text-[13px] font-medium rounded-xl" style={{ background: "var(--felt)", color: "var(--canvas)" }}>
                {savedFlash ? "Saved" : "Save"}
              </button>
              <button onClick={test} className="px-4 py-2 text-[13px]" style={{ color: "var(--ink)" }}>
                Test connection
              </button>
              {testResult && (
                <span className="text-[12px] font-mono" style={{ color: "var(--ink-faint)" }}>
                  {testResult}
                </span>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
