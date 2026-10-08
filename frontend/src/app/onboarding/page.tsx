"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { ArrowLeft, Loader2, X } from "lucide-react";
import { auth, settings, type Me, type AppSettings } from "@/lib/api";
import EunomiaMark from "@/components/EunomiaMark";
import ThemeToggle from "@/components/ThemeToggle";

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
      setError(err instanceof Error ? err.message : "Could not save the key. Check the URL and key, or skip for now.");
      setBusy(false);
    }
  }

  async function skip() {
    setBusy(true);
    try {
      await finish();
    } catch {
      // Finishing the wizard shouldn't trap the user even if the server
      // call fails: let them into the app and they can set things up
      // from Settings/Connectors later.
      router.replace("/");
    }
  }

  return (
    <div className="canvas-grid min-h-screen w-full flex flex-col px-6 py-6 sm:px-10">
      <header className="flex items-center justify-between">
        <span className="inline-flex items-center gap-2 text-[14px] font-semibold tracking-[-0.01em]">
          <EunomiaMark size={22} />
          Eunomia
        </span>
        <div className="flex items-center gap-1">
          <ThemeToggle />
          {me && (
            <button onClick={skip} disabled={busy} aria-label="Skip setup" title="Skip setup" className="btn btn-ghost btn-icon btn-sm" style={{ width: 26 }}>
              <X size={14} strokeWidth={1.75} />
            </button>
          )}
        </div>
      </header>

      <main className="flex-1 flex items-center justify-center py-10">
        <div className="panel w-full max-w-[440px] p-7">
          {!me ? (
            <div className="flex flex-col gap-4" aria-busy="true" aria-label="Loading">
              <div className="skeleton h-1 w-full" />
              <div className="skeleton h-6 w-2/3 mt-4" />
              <div className="skeleton h-4 w-full" />
              <div className="skeleton h-4 w-4/5" />
              <div className="skeleton h-9 w-28 mt-2" />
            </div>
          ) : (
            <>
              <ol className="flex gap-2 mb-7" aria-label="Setup progress">
                {steps.map((label, i) => (
                  <li key={label} className="flex-1" aria-current={i === step ? "step" : undefined}>
                    <span
                      className="block h-[3px] rounded-full"
                      style={{ background: i <= step ? "var(--accent)" : "var(--border-strong)", transition: "background-color 200ms ease" }}
                    />
                    <span className="mt-2 flex items-center gap-1.5 whitespace-nowrap text-[12px]" style={{ color: i === step ? "var(--ink)" : "var(--ink-faint)" }}>
                      <span className="font-mono">{i + 1}</span>
                      {label}
                      {i === 1 && <span className="hidden sm:inline" style={{ color: "var(--ink-faint)" }}>(optional)</span>}
                    </span>
                  </li>
                ))}
              </ol>

              {step === 0 && (
                <div className="flex flex-col gap-4">
                  <h1 className="page-title">Welcome to Eunomia</h1>
                  <p className="text-[14px] leading-[1.6]" style={{ color: "var(--ink-dim)" }}>
                    You&apos;re signed in as{" "}
                    <span className="font-medium" style={{ color: "var(--ink)" }}>
                      {me.email}
                    </span>
                    . {skipOpenAiStep ? "Nothing else to set up, so you can go straight in." : "One optional step, then you're in."} You can
                    connect a data source any time from Connectors.
                  </p>
                  <button
                    onClick={() => (skipOpenAiStep ? skip() : setStep(1))}
                    disabled={busy}
                    className="btn btn-primary h-9 self-start mt-2 px-4"
                  >
                    {busy && <Loader2 size={14} strokeWidth={1.75} className="animate-spin" aria-hidden />}
                    Let&apos;s go
                  </button>
                </div>
              )}

              {step === 1 && !skipOpenAiStep && (
                <div className="flex flex-col gap-4">
                  <h1 className="page-title">Add an OpenAI key</h1>
                  <p className="text-[13.5px] leading-[1.6]" style={{ color: "var(--ink-dim)" }}>
                    Connected agents like Claude Code and Codex can recall and write memory with their own model. An
                    OpenAI-compatible endpoint lets Eunomia run embeddings and answers itself, which saves your
                    agents&apos; tokens. You can add it later in Settings.
                  </p>
                  <div className="flex flex-col gap-1.5">
                    <label htmlFor="base-url" className="text-[12.5px] font-medium" style={{ color: "var(--ink-dim)" }}>
                      Base URL
                    </label>
                    <input
                      id="base-url"
                      type="url"
                      autoComplete="off"
                      spellCheck={false}
                      className="field h-9 px-3 text-[13px] font-mono"
                      placeholder="https://api.openai.com/v1"
                      value={baseUrlInput}
                      onChange={(e) => setBaseUrlInput(e.target.value)}
                    />
                  </div>
                  <div className="flex flex-col gap-1.5">
                    <label htmlFor="api-key" className="text-[12.5px] font-medium" style={{ color: "var(--ink-dim)" }}>
                      API key
                    </label>
                    <input
                      id="api-key"
                      type="password"
                      autoComplete="off"
                      spellCheck={false}
                      className="field h-9 px-3 text-[13px] font-mono"
                      placeholder="sk-…"
                      value={apiKeyInput}
                      onChange={(e) => setApiKeyInput(e.target.value)}
                    />
                  </div>

                  {error && (
                    <div role="alert" className="rounded-[7px] px-3 py-2 text-[13px]" style={{ background: "var(--critical-soft)", color: "var(--critical)" }}>
                      {error}
                    </div>
                  )}

                  <div className="flex items-center gap-2 mt-2">
                    <button onClick={() => setStep(0)} disabled={busy} className="btn btn-ghost h-9 px-2.5">
                      <ArrowLeft size={14} strokeWidth={1.75} aria-hidden />
                      Back
                    </button>
                    <div className="flex-1" />
                    <button onClick={skip} disabled={busy} className="btn btn-ghost h-9">
                      Skip &amp; finish
                    </button>
                    <button onClick={saveApiKey} disabled={busy} aria-busy={busy} className="btn btn-primary h-9 px-4">
                      {busy && <Loader2 size={14} strokeWidth={1.75} className="animate-spin" aria-hidden />}
                      {busy ? "Finishing…" : "Finish"}
                    </button>
                  </div>
                </div>
              )}
            </>
          )}
        </div>
      </main>
    </div>
  );
}
