"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Plug, Sparkles, Trash2 } from "lucide-react";
import { api, AppSettings } from "@/lib/api";

export default function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [saved, setSaved] = useState(false);
  const [demoBusy, setDemoBusy] = useState(false);
  const [demoMessage, setDemoMessage] = useState<string | null>(null);

  useEffect(() => {
    api.get<AppSettings>("/api/settings").then(setSettings);
  }, []);

  async function seedDemoData() {
    setDemoBusy(true);
    setDemoMessage(null);
    try {
      const res = await api.post<{
        projects: number;
        tasks: number;
        transactions: number;
        calendar_events: number;
        emails: number;
        recordings: number;
      }>("/api/demo-data");
      setDemoMessage(
        `Seeded ${res.projects} projects, ${res.tasks} tasks, ${res.transactions} Up Bank transactions, ` +
          `${res.calendar_events} calendar events, ${res.emails} emails, and ${res.recordings} PocketAI recordings. ` +
          `Refresh the dashboard to see it.`
      );
    } catch {
      setDemoMessage("Could not seed demo data.");
    } finally {
      setDemoBusy(false);
    }
  }

  async function clearDemoData() {
    setDemoBusy(true);
    setDemoMessage(null);
    try {
      const res = await api.del<{
        projects_removed: number;
        transactions_removed: number;
        calendar_events_removed: number;
        emails_removed: number;
        recordings_removed: number;
      }>("/api/demo-data");
      setDemoMessage(
        `Removed ${res.projects_removed} demo project(s), ${res.transactions_removed} transaction(s), ` +
          `${res.calendar_events_removed} event(s), ${res.emails_removed} email(s), and ${res.recordings_removed} recording(s).`
      );
    } catch {
      setDemoMessage("Could not clear demo data.");
    } finally {
      setDemoBusy(false);
    }
  }

  async function saveSettings() {
    if (!settings) return;
    const payload: Record<string, unknown> = {
      embedding_backend: settings.embedding_backend,
      embedding_model: settings.embedding_model,
      llm_base_url: settings.llm_base_url,
      theme: settings.theme,
    };
    if (apiKeyInput) payload.llm_api_key = apiKeyInput;
    const updated = await api.patch<AppSettings>("/api/settings", payload);
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
        <div className="w-8 h-8 flex items-center justify-center" style={{ background: "var(--surface-2)", color: "var(--text-secondary)" }}>
          <Plug size={16} />
        </div>
        <div className="flex-1">
          <div className="text-[13.5px] font-medium">Connectors</div>
          <div className="text-[12px]" style={{ color: "var(--text-muted)" }}>
            Up Bank, PocketAI, Open Connector — credentials and connection status
          </div>
        </div>
        <span className="text-[12px]" style={{ color: "var(--accent)" }}>
          Manage →
        </span>
      </Link>

      <section className="ledger p-6 flex flex-col gap-4">
        <div className="eyebrow">Embeddings</div>
        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--text-secondary)" }}>
          Backend
          <select
            className="field px-3 py-2.5 text-[13.5px]"
            value={settings.embedding_backend}
            onChange={(e) => setSettings({ ...settings, embedding_backend: e.target.value as AppSettings["embedding_backend"] })}
          >
            <option value="api">OpenAI-compatible API</option>
            <option value="local">Local (sentence-transformers)</option>
            <option value="stub">Stub (offline)</option>
          </select>
        </label>
        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--text-secondary)" }}>
          API base URL
          <input
            className="field px-3 py-2.5 text-[13.5px] font-mono"
            value={settings.llm_base_url}
            onChange={(e) => setSettings({ ...settings, llm_base_url: e.target.value })}
            placeholder="http://127.0.0.1:11434/v1"
          />
        </label>
        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--text-secondary)" }}>
          Model
          <input
            className="field px-3 py-2.5 text-[13.5px] font-mono"
            value={settings.embedding_model}
            onChange={(e) => setSettings({ ...settings, embedding_model: e.target.value })}
            placeholder="nomic-embed-text"
          />
        </label>
        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--text-secondary)" }}>
          API key {settings.llm_api_key_set && <span style={{ color: "var(--good)" }}>· set</span>}
          <input
            type="password"
            className="field px-3 py-2.5 text-[13.5px] font-mono"
            value={apiKeyInput}
            onChange={(e) => setApiKeyInput(e.target.value)}
            placeholder={settings.llm_api_key_set ? "leave blank to keep" : "sk-…"}
          />
        </label>
      </section>

      <section className="ledger p-6 flex flex-col gap-4">
        <div className="eyebrow">Theme</div>
        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--text-secondary)" }}>
          Mode
          <select
            className="field px-3 py-2.5 text-[13.5px]"
            value={settings.theme.mode || "system"}
            onChange={(e) => setSettings({ ...settings, theme: { ...settings.theme, mode: e.target.value as "light" | "dark" } })}
          >
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </label>
        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--text-secondary)" }}>
          Accent color
          <input
            type="color"
            className="w-16 h-9 field p-1"
            value={settings.theme.accent || "#a8752f"}
            onChange={(e) => setSettings({ ...settings, theme: { ...settings.theme, accent: e.target.value } })}
          />
        </label>
      </section>

      <button onClick={saveSettings} className="self-start px-5 py-2.5 text-[13px] font-medium text-white" style={{ background: "var(--accent)" }}>
        {saved ? "Saved" : "Save settings"}
      </button>

      <section className="ledger p-6 flex flex-col gap-3">
        <div className="eyebrow">Demo data</div>
        <p className="text-[13px]" style={{ color: "var(--text-secondary)" }}>
          Populate ~50 realistic fake tasks across 4 demo projects (prefixed &ldquo;Demo — &rdquo;), plus
          fake Up Bank transactions and PocketAI recordings —
          Finance, the dashboard, Connectors, and the assistant&apos;s tools all switch to it
          automatically, no real credentials needed. Re-seeding replaces the previous batch; clearing
          puts every connector back to however it was before — never touches a real connection.
        </p>
        <div className="flex items-center gap-2">
          <button
            onClick={seedDemoData}
            disabled={demoBusy}
            className="field px-4 py-2 text-[13px] flex items-center gap-1.5 disabled:opacity-40"
            style={{ color: "var(--accent)" }}
          >
            <Sparkles size={14} /> Seed demo data
          </button>
          <button
            onClick={clearDemoData}
            disabled={demoBusy}
            className="field px-4 py-2 text-[13px] flex items-center gap-1.5 disabled:opacity-40"
            style={{ color: "var(--critical)" }}
          >
            <Trash2 size={14} /> Clear demo data
          </button>
        </div>
        {demoMessage && (
          <div className="text-[12px] font-mono" style={{ color: "var(--text-muted)" }}>
            {demoMessage}
          </div>
        )}
      </section>
    </div>
  );
}
