"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { X } from "lucide-react";
import { auth, settings, type Me, type AppSettings } from "@/lib/api";

const STEP_LABELS = ["Welcome", "OpenAI key"];

export default function OnboardingPage() {
  const router = useRouter();
  const [me, setMe] = useState<Me | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
  const [step, setStep] = useState(0);
  const [baseUrlInput, setBaseUrlInput] = useState("");
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
    settings
      .get()
      .then((s) => {
        setAppSettings(s);
        setBaseUrlInput(s.openai_base_url);
      })
      .catch(() => {});
  }, [router]);

  // The 2nd step is pointless if an OpenAI key is already configured
  // server-wide (env var) or was already set earlier by this user.
  const skipOpenAiStep = Boolean(appSettings?.openai_api_key_set);
  const steps = skipOpenAiStep ? STEP_LABELS.slice(0, 1) : STEP_LABELS;

  async function finish() {
    await settings.completeOnboarding();
    router.replace("/");
  }

  async function saveApiKey() {
    setBusy(true);
    setError(null);
    try {
      const url = baseUrlInput.trim();
      const changes: Record<string, string> = {};
      if (url && url !== appSettings?.openai_base_url) changes.openai_base_url = url;
      if (apiKeyInput) changes.openai_api_key = apiKeyInput;
      if (Object.keys(changes).length) await settings.update(changes);
      await finish();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not save key.");
    } finally {
      setBusy(false);
    }
  }

  async function skip() {
    setBusy(true);
    try {
      await finish();
    } catch {
      // Finishing the wizard shouldn't trap the user even if the server
      // call fails -- let them into the app and they can set things up
      // from Settings/Connectors later.
      router.replace("/");
    } finally {
      setBusy(false);
    }
  }

  if (!me) return null;

  return (
    <div className="min-h-screen w-full flex items-center justify-center p-6">
      <div className="surface w-full max-w-md p-8 flex flex-col gap-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="eyebrow mb-2">
              Step {step + 1} of {steps.length}
            </div>
            <h1 className="font-display text-2xl">{steps[step]}</h1>
          </div>
          <button
            onClick={skip}
            disabled={busy}
            aria-label="Skip setup"
            className="shrink-0 p-1.5 rounded-lg"
            style={{ color: "var(--ink-faint)" }}
          >
            <X size={16} />
          </button>
        </div>

        {step === 0 && (
          <div className="flex flex-col gap-5">
            <p className="text-[13.5px]" style={{ color: "var(--ink-dim)" }}>
              You&apos;re signed in as <strong>{me.email}</strong>. Let&apos;s get you set up — one quick
              step, then you&apos;re in. You can connect a data source any time from Connectors.
            </p>
            <button
              onClick={() => (skipOpenAiStep ? finish() : setStep(1))}
              className="self-start px-4 py-2.5 text-[13px] font-medium rounded-xl"
              style={{ background: "var(--felt)", color: "var(--canvas)" }}
            >
              Let&apos;s go
            </button>
          </div>
        )}

        {step === 1 && !skipOpenAiStep && (
          <div className="flex flex-col gap-4">
            <p className="text-[13px]" style={{ color: "var(--ink-dim)" }}>
              Optional. A connected AI agent (Claude, Codex, …) can recall and write memory using its own
              model. Adding an OpenAI-compatible endpoint lets Eunomia do embeddings and answer synthesis
              itself, which saves your agent&apos;s tokens. Skip it and add it later from Settings if you like.
            </p>
            <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
              Base URL
              <input
                className="field px-3 py-2 text-[13px] font-mono"
                placeholder="https://api.openai.com/v1"
                value={baseUrlInput}
                onChange={(e) => setBaseUrlInput(e.target.value)}
              />
            </label>
            <label className="text-[12px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
              API key
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
                className="px-4 py-2.5 text-[13px] font-medium rounded-xl disabled:opacity-50"
                style={{ background: "var(--felt)", color: "var(--canvas)" }}
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
