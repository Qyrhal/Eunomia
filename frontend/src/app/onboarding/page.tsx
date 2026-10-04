"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Landmark, Mic, Plug2 } from "lucide-react";
import { auth, connectors, settings, type ConnectorKind, type Me, type AppSettings } from "@/lib/api";

type FieldDef = { key: string; label: string; placeholder: string; secret: boolean };

const CONNECTOR_META: Record<ConnectorKind, { label: string; icon: React.ReactNode; fields: FieldDef[] }> = {
  up_bank: {
    label: "Up Bank",
    icon: <Landmark size={16} />,
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
  open_connector: {
    label: "Open Connector",
    icon: <Plug2 size={16} />,
    fields: [
      { key: "base_url", label: "Base URL", placeholder: "http://localhost:3000", secret: false },
      { key: "api_key", label: "Runtime token", placeholder: "…", secret: true },
    ],
  },
};

const STEP_LABELS = ["Welcome", "Connect a source", "OpenAI key"];

export default function OnboardingPage() {
  const router = useRouter();
  const [me, setMe] = useState<Me | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
  const [step, setStep] = useState(0);
  const [kind, setKind] = useState<ConnectorKind>("up_bank");
  const [fields, setFields] = useState<Record<string, string>>({});
  const [connectSaved, setConnectSaved] = useState(false);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    auth
      .me()
      .then((m) => {
        if (m.onboarded) {
          router.replace("/");
          return;
        }
        setMe(m);
      })
      .catch(() => router.replace("/login"));
    settings.get().then(setAppSettings).catch(() => {});
  }, [router]);

  // The 3rd step is pointless if an OpenAI key is already configured
  // server-wide (env var) or was already set earlier by this user.
  const skipOpenAiStep = Boolean(appSettings?.openai_api_key_set);
  const steps = skipOpenAiStep ? STEP_LABELS.slice(0, 2) : STEP_LABELS;

  async function saveConnector() {
    setBusy(true);
    setError(null);
    try {
      const meta = CONNECTOR_META[kind];
      const credentials: Record<string, string> = {};
      const config: Record<string, string> = {};
      for (const f of meta.fields) {
        const v = fields[f.key];
        if (!v) continue;
        if (f.secret) credentials[f.key] = v;
        else config[f.key] = v;
      }
      await connectors.update(kind, { enabled: true, credentials, config });
      setConnectSaved(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not save connector.");
    } finally {
      setBusy(false);
    }
  }

  async function saveApiKey() {
    setBusy(true);
    setError(null);
    try {
      if (apiKeyInput) await settings.update({ openai_api_key: apiKeyInput });
      await finish();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not save key.");
    } finally {
      setBusy(false);
    }
  }

  async function finish() {
    await settings.completeOnboarding();
    router.replace("/");
  }

  if (!me) return null;

  return (
    <div className="min-h-screen w-full flex items-center justify-center p-6">
      <div className="surface w-full max-w-md p-8 flex flex-col gap-6">
        <div>
          <div className="eyebrow mb-2">
            Step {step + 1} of {steps.length}
          </div>
          <h1 className="font-display text-2xl">{steps[step]}</h1>
        </div>

        {step === 0 && (
          <div className="flex flex-col gap-5">
            <p className="text-[13.5px]" style={{ color: "var(--ink-dim)" }}>
              You&apos;re signed in as <strong>{me.email}</strong>. Let&apos;s get you set up — two quick
              steps, then you&apos;re in.
            </p>
            <button
              onClick={() => setStep(1)}
              className="self-start px-4 py-2.5 text-[13px] font-medium text-white"
              style={{ background: "var(--accent)" }}
            >
              Let&apos;s go
            </button>
          </div>
        )}

        {step === 1 && (
          <div className="flex flex-col gap-4">
            <p className="text-[13px]" style={{ color: "var(--ink-dim)" }}>
              Connect one data source to start. You can add the others later from Connectors.
            </p>
            <div className="flex gap-2">
              {(Object.keys(CONNECTOR_META) as ConnectorKind[]).map((k) => (
                <button
                  key={k}
                  onClick={() => {
                    setKind(k);
                    setFields({});
                    setConnectSaved(false);
                  }}
                  className="flex-1 field px-3 py-2 text-[12.5px] flex items-center justify-center gap-1.5"
                  style={{
                    borderColor: kind === k ? "var(--accent)" : "var(--border)",
                    color: kind === k ? "var(--accent)" : "var(--ink-dim)",
                  }}
                >
                  {CONNECTOR_META[k].icon}
                  {CONNECTOR_META[k].label}
                </button>
              ))}
            </div>

            <div className="flex flex-col gap-3">
              {CONNECTOR_META[kind].fields.map((f) => (
                <label key={f.key} className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
                  {f.label}
                  <input
                    type={f.secret ? "password" : "text"}
                    className="field px-3 py-2 text-[13px] font-mono"
                    placeholder={f.placeholder}
                    value={fields[f.key] || ""}
                    onChange={(e) => setFields((s) => ({ ...s, [f.key]: e.target.value }))}
                  />
                </label>
              ))}
            </div>

            {error && (
              <p className="text-[12.5px]" style={{ color: "var(--critical)" }}>
                {error}
              </p>
            )}

            <div className="flex items-center gap-3">
              <button
                onClick={saveConnector}
                disabled={busy}
                className="px-4 py-2.5 text-[13px] font-medium text-white disabled:opacity-50"
                style={{ background: "var(--accent)" }}
              >
                {connectSaved ? "Saved" : busy ? "Saving…" : "Save & continue"}
              </button>
              <button
                onClick={() => (skipOpenAiStep ? finish() : setStep(2))}
                className="px-4 py-2 text-[13px]"
                style={{ color: "var(--ink-faint)" }}
              >
                {connectSaved ? "Continue" : "Skip for now"}
              </button>
            </div>
          </div>
        )}

        {step === 2 && !skipOpenAiStep && (
          <div className="flex flex-col gap-4">
            <p className="text-[13px]" style={{ color: "var(--ink-dim)" }}>
              Eunomia uses OpenAI for embeddings and entity-memory extraction. Paste a key now, or skip
              and add it later from Settings.
            </p>
            <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
              OpenAI API key
              <input
                type="password"
                className="field px-3 py-2 text-[13px] font-mono"
                placeholder="sk-…"
                value={apiKeyInput}
                onChange={(e) => setApiKeyInput(e.target.value)}
              />
            </label>

            {error && (
              <p className="text-[12.5px]" style={{ color: "var(--critical)" }}>
                {error}
              </p>
            )}

            <div className="flex items-center gap-3">
              <button
                onClick={saveApiKey}
                disabled={busy}
                className="px-4 py-2.5 text-[13px] font-medium text-white disabled:opacity-50"
                style={{ background: "var(--accent)" }}
              >
                {busy ? "Finishing…" : "Finish"}
              </button>
              <button onClick={finish} className="px-4 py-2 text-[13px]" style={{ color: "var(--ink-faint)" }}>
                Skip & finish
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
