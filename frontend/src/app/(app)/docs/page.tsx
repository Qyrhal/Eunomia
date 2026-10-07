"use client";

import { useEffect, useState } from "react";
import Markdown from "@/components/Markdown";
import { docs } from "@/lib/api";

// The repo's docs/ folder, served by the same `docs` tool agents call over MCP.
export default function DocsPage() {
  const [topics, setTopics] = useState<{ topic: string; title: string }[]>([]);
  const [topic, setTopic] = useState("quickstart");
  const [body, setBody] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    docs.list().then((r) => setTopics(r.docs)).catch(() => setError("Could not load the docs."));
  }, []);

  useEffect(() => {
    docs
      .get(topic)
      .then((d) => setBody(d.markdown))
      .catch(() => setError("Could not load that doc."));
  }, [topic]);

  return (
    <div className="flex flex-col md:flex-row gap-6 max-w-4xl">
      <nav className="flex md:flex-col gap-1 md:w-48 shrink-0 flex-wrap" aria-label="Docs">
        <div className="eyebrow mb-1 hidden md:block">Docs</div>
        {topics.map((t) => (
          <button
            key={t.topic}
            onClick={() => setTopic(t.topic)}
            aria-current={t.topic === topic ? "page" : undefined}
            className="text-left px-3 py-2 rounded-lg text-[13px]"
            style={{
              background: t.topic === topic ? "var(--surface-raised)" : "transparent",
              color: t.topic === topic ? "var(--ink)" : "var(--ink-dim)",
              fontWeight: t.topic === topic ? 600 : 400,
            }}
          >
            {t.title}
          </button>
        ))}
      </nav>
      <article className="ledger p-6 md:p-8 flex-1 min-w-0 text-[14px] leading-relaxed docs">
        {error ? <p style={{ color: "var(--critical)" }}>{error}</p> : body === null ? <p style={{ color: "var(--ink-faint)" }}>Loading…</p> : <Markdown text={body} onDocLink={setTopic} />}
      </article>
    </div>
  );
}
