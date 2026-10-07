"use client";

import { useEffect, useState } from "react";
import Markdown from "@/components/Markdown";
import { settings as settingsApi } from "@/lib/api";

// The skill that tells connected agents when to recall and what to remember.
// Sent to every MCP client as connection instructions, injected into Claude
// Code each session by its hook, and written as SKILL.md by connect-agents.sh.
export default function SkillPage() {
  const [skill, setSkill] = useState<string | null>(null);
  const [custom, setCustom] = useState(false);
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    settingsApi
      .get()
      .then((s) => {
        setSkill(s.memory_skill);
        setCustom(s.memory_skill_custom);
      })
      .catch(() => setError("Could not load the skill."));
  }, []);

  async function save(text: string) {
    setBusy(true);
    setError(null);
    try {
      const s = await settingsApi.update({ memory_skill: text });
      setSkill(s.memory_skill);
      setCustom(s.memory_skill_custom);
      setEditing(false);
      setSaved(true);
      setTimeout(() => setSaved(false), 1800);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save the skill.");
    } finally {
      setBusy(false);
    }
  }

  if (skill === null) return error ? <p className="text-[12.5px]" style={{ color: "var(--critical)" }}>{error}</p> : null;

  const button = "px-4 py-2 text-[13px] font-medium rounded-xl disabled:opacity-50";
  return (
    <div className="max-w-3xl flex flex-col gap-5">
      <div>
        <div className="eyebrow mb-2">Memory skill</div>
        <h1 className="font-display text-3xl">How your agents use memory</h1>
        <p className="text-[13px] mt-2" style={{ color: "var(--ink-dim)" }}>
          These instructions tell every connected agent when to recall and what to remember. They&apos;re sent to MCP
          clients each time they connect, and Claude Code also gets them at the start of every session, together with
          memories recalled for each message. Changes apply to new sessions. File copies (SKILL.md for Claude Code,
          Codex and Hermes) refresh when you re-run <code className="font-mono">connect-agents.sh</code>.
        </p>
      </div>

      <div className="flex items-center gap-2 flex-wrap">
        <span className="pill" aria-label="Skill source">{custom ? "Customised" : "Built-in default"}</span>
        <div className="flex-1" />
        {editing ? (
          <>
            <button onClick={() => setEditing(false)} className="text-[12.5px] px-3" style={{ color: "var(--ink-faint)" }}>
              Cancel
            </button>
            <button onClick={() => save(draft)} disabled={busy || !draft.trim()} className={button} style={{ background: "var(--felt)", color: "var(--canvas)" }}>
              {busy ? "Saving…" : "Save"}
            </button>
          </>
        ) : (
          <>
            {custom && (
              <button
                onClick={() => window.confirm("Replace your version with the built-in skill?") && save("")}
                disabled={busy}
                className="pill"
              >
                Reset to default
              </button>
            )}
            <button
              onClick={() => {
                setDraft(skill);
                setEditing(true);
              }}
              className={button}
              style={{ background: "var(--felt)", color: "var(--canvas)" }}
            >
              Edit
            </button>
          </>
        )}
      </div>
      {saved && <p className="text-[12px]" style={{ color: "var(--good)" }} role="status">Saved. New agent sessions pick it up.</p>}
      {error && <p className="text-[12px]" style={{ color: "var(--critical)" }}>{error}</p>}

      {editing ? (
        <textarea
          aria-label="Skill markdown"
          className="field p-4 text-[12.5px] font-mono leading-relaxed min-h-[60vh]"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          spellCheck={false}
        />
      ) : (
        <article className="ledger p-6 md:p-8 text-[14px] leading-relaxed docs">
          <Markdown text={skill.replace(/^---\n[\s\S]*?\n---\n/, "")} />
        </article>
      )}
    </div>
  );
}
