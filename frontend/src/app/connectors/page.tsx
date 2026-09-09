"use client";

import { useEffect, useState } from "react";
import { Landmark, Mic } from "lucide-react";
import { api, Connector } from "@/lib/api";

type FieldDef = { key: string; label: string; placeholder: string; secret: boolean };

const CONNECTOR_META: Record<
  Connector["kind"],
  { label: string; icon: React.ReactNode; help?: React.ReactNode; fields: FieldDef[] }
> = {
  up_bank: {
    label: "Up Bank",
    icon: <Landmark size={16} />,
    help: (
      <>
        Generate a token at{" "}
        <a href="https://api.up.com.au" target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--accent)" }}>
          api.up.com.au
        </a>
        .
      </>
    ),
    fields: [{ key: "personal_access_token", label: "Personal access token", placeholder: "up:yeah:…", secret: true }],
  },
  pocketai: {
    label: "PocketAI",
    icon: <Mic size={16} />,
    fields: [
      { key: "base_url", label: "Base URL", placeholder: "https://public.heypocketai.com/api/v1", secret: false },
      { key: "api_key", label: "API key", placeholder: "pk_…", secret: true },
    ],
  },
};

const ORDER: Connector["kind"][] = ["up_bank", "pocketai"];

export default function ConnectorsPage() {
  const [connectors, setConnectors] = useState<Connector[]>([]);
  const [inputs, setInputs] = useState<Record<string, Record<string, string>>>({});
  const [testResult, setTestResult] = useState<Record<string, string>>({});
  const [savedFlash, setSavedFlash] = useState<Record<string, boolean>>({});

  const load = () => api.get<Connector[]>("/api/connectors").then(setConnectors);
  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function setField(kind: string, key: string, value: string) {
    setInputs((s) => ({ ...s, [kind]: { ...s[kind], [key]: value } }));
  }

  async function save(kind: Connector["kind"]) {
    const meta = CONNECTOR_META[kind];
    const values = inputs[kind] || {};
    const credentials: Record<string, string> = {};
    const config: Record<string, string> = {};
    for (const f of meta.fields) {
      const v = values[f.key];
      if (!v) continue;
      if (f.secret) credentials[f.key] = v;
      else config[f.key] = v;
    }
    const body: Record<string, unknown> = { enabled: true };
    if (Object.keys(credentials).length) body.credentials = credentials;
    if (Object.keys(config).length) body.config = config;
    await api.patch(`/api/connectors/${kind}`, body);
    setInputs((s) => ({ ...s, [kind]: {} }));
    setSavedFlash((s) => ({ ...s, [kind]: true }));
    setTimeout(() => setSavedFlash((s) => ({ ...s, [kind]: false })), 1400);
    load();
  }

  async function test(kind: string) {
    const res = await api.post<{ ok: boolean; error?: string }>(`/api/connectors/${kind}/test`);
    setTestResult((r) => ({ ...r, [kind]: res.ok ? "connected" : res.error || "failed" }));
  }

  const byKind = Object.fromEntries(connectors.map((c) => [c.kind, c]));

  return (
    <div className="max-w-3xl flex flex-col gap-7">
      <div>
        <div className="eyebrow mb-2">Connectors</div>
        <h1 className="font-display text-3xl">Sealed accounts</h1>
        <p className="text-[13px] mt-2" style={{ color: "var(--text-secondary)" }}>
          Every credential here is encrypted at rest and only ever used by your own instance —
          never sent anywhere but the provider it belongs to.
        </p>
      </div>

      <div className="flex flex-col gap-5">
        {ORDER.map((kind) => {
          const meta = CONNECTOR_META[kind];
          const c = byKind[kind];
          if (!c) return null;
          const isDemo = Boolean(c.config?.demo);
          const connected = c.enabled && c.credentials_set;
          return (
            <div key={kind} className="ledger p-6 flex flex-col gap-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2.5">
                  <div className="w-8 h-8 flex items-center justify-center" style={{ background: "var(--surface-2)", color: "var(--text-secondary)" }}>
                    {meta.icon}
                  </div>
                  <div className="text-[13.5px] font-medium">{meta.label}</div>
                </div>
                <span className="eyebrow" style={{ color: isDemo ? "var(--accent)" : connected ? "var(--good)" : "var(--text-muted)" }}>
                  {isDemo ? "Demo data" : connected ? "Connected" : "Not connected"}
                </span>
              </div>

              {isDemo && (
                <p className="text-[12px]" style={{ color: "var(--text-muted)" }}>
                  Running on fake data seeded from Settings. Save a real token below to switch over,
                  or clear the demo data from Settings.
                </p>
              )}

              {meta.help && (
                <p className="text-[12px]" style={{ color: "var(--text-muted)" }}>
                  {meta.help}
                </p>
              )}

              <div className="grid sm:grid-cols-2 gap-3">
                {meta.fields.map((f) => (
                  <label key={f.key} className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--text-secondary)" }}>
                    {f.label}
                    <input
                      type={f.secret ? "password" : "text"}
                      className="field px-3 py-2 text-[13px] font-mono"
                      placeholder={f.placeholder}
                      value={inputs[kind]?.[f.key] || ""}
                      onChange={(e) => setField(kind, f.key, e.target.value)}
                    />
                  </label>
                ))}
              </div>

              <div className="flex items-center gap-4 pt-1" style={{ borderTop: "1px solid var(--border)" }}>
                <div className="flex items-center gap-2 pt-4">
                  <button onClick={() => save(kind)} className="px-4 py-2 text-[13px] font-medium text-white" style={{ background: "var(--accent)" }}>
                    {savedFlash[kind] ? "Saved" : "Save"}
                  </button>
                  <button onClick={() => test(kind)} className="px-4 py-2 text-[13px]" style={{ color: "var(--accent)" }}>
                    Test connection
                  </button>
                  {testResult[kind] && (
                    <span className="text-[12px] font-mono" style={{ color: "var(--text-muted)" }}>
                      {testResult[kind]}
                    </span>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
