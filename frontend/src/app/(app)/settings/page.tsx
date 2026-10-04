"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Plug } from "lucide-react";
import { settings as settingsApi, type AppSettings } from "@/lib/api";

export default function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    settingsApi.get().then(setSettings);
  }, []);

  async function saveSettings() {
    if (!settings) return;
    const payload: Record<string, unknown> = { theme: settings.theme };
    if (apiKeyInput) payload.openai_api_key = apiKeyInput;
    const updated = await settingsApi.update(payload);
    setSettings(updated);
    setApiKeyInput("");
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  }

  if (!settings) return null;

  return (
    <div className="max-w-2xl flex flex-col gap-8">
      <div>
        <div className="eyebrow mb-2">Settings</div>
        <h1 className="font-display text-3xl">The instrument</h1>
      </div>

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
        <div className="eyebrow">OpenAI</div>
        <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
          Used for embeddings and entity-memory extraction.
        </p>
        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
          API key {settings.openai_api_key_set && <span style={{ color: "var(--good)" }}>· set</span>}
          <input
            type="password"
            className="field px-3 py-2.5 text-[13.5px] font-mono"
            value={apiKeyInput}
            onChange={(e) => setApiKeyInput(e.target.value)}
            placeholder={settings.openai_api_key_set ? "leave blank to keep" : "sk-…"}
          />
        </label>
      </section>

      <button onClick={saveSettings} className="self-start px-5 py-2.5 text-[13px] font-medium rounded-xl" style={{ background: "var(--felt)", color: "var(--canvas)" }}>
        {saved ? "Saved" : "Save settings"}
      </button>
    </div>
  );
}
