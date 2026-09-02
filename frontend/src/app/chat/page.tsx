"use client";

import { useState } from "react";
import { Send } from "lucide-react";
import HourRing from "@/components/HourRing";
import { API_BASE, ChatMessage } from "@/lib/api";

type ToolCall = { tool: string; args: Record<string, unknown>; result: unknown };
type StreamEvent =
  | { type: "token"; content: string }
  | { type: "tool"; tool: string; args: Record<string, unknown>; result: unknown }
  | { type: "error"; detail: string }
  | { type: "done" };

export default function ChatPage() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastTools, setLastTools] = useState<ToolCall[]>([]);

  async function send() {
    if (!input.trim() || busy) return;
    const next = [...messages, { role: "user", content: input } as ChatMessage];
    setMessages([...next, { role: "assistant", content: "" }]);
    setInput("");
    setBusy(true);
    setError(null);
    setLastTools([]);

    function appendToken(chunk: string) {
      setMessages((cur) => {
        const copy = [...cur];
        copy[copy.length - 1] = { ...copy[copy.length - 1], content: copy[copy.length - 1].content + chunk };
        return copy;
      });
    }

    try {
      const res = await fetch(`${API_BASE}/api/ai/chat/stream`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ messages: next }),
      });
      if (!res.ok || !res.body) {
        setError("Could not reach the backend.");
        setMessages(next);
        return;
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const frames = buffer.split("\n\n");
        buffer = frames.pop() || "";
        for (const frame of frames) {
          if (!frame.startsWith("data: ")) continue;
          const event = JSON.parse(frame.slice("data: ".length)) as StreamEvent;
          if (event.type === "token") appendToken(event.content);
          else if (event.type === "tool") setLastTools((cur) => [...cur, event]);
          else if (event.type === "error") setError(event.detail);
        }
      }
    } catch {
      setError("Could not reach the backend.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="max-w-3xl flex flex-col h-[calc(100vh-5rem)]">
      <div className="eyebrow mb-2">Assistant</div>
      <h1 className="font-display text-3xl mb-6">Eunomia is listening</h1>

      <div className="flex-1 overflow-y-auto flex flex-col gap-3 pr-1">
        {messages.map((m, i) => (
          <div
            key={i}
            className="px-3.5 py-2.5 text-[13.5px] max-w-[85%]"
            style={{
              alignSelf: m.role === "user" ? "flex-end" : "flex-start",
              background: m.role === "user" ? "var(--accent)" : "var(--surface)",
              color: m.role === "user" ? "#fff" : "var(--text-primary)",
              border: m.role === "user" ? "none" : "1px solid var(--border)",
              borderRadius: "4px",
            }}
          >
            {m.content || (busy && i === messages.length - 1 ? "…" : "")}
          </div>
        ))}
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center flex-1 text-center gap-3 py-16">
            <HourRing size={32} color="var(--border-strong)" />
            <div className="text-[13px] max-w-xs" style={{ color: "var(--text-muted)" }}>
              Ask it to enter tasks, check your calendar, or draft follow-ups from recent spending.
            </div>
          </div>
        )}
      </div>

      {lastTools.length > 0 && (
        <div className="text-[11px] font-mono mb-2 flex flex-wrap gap-2">
          {lastTools.map((t, i) => (
            <span key={i} className="px-2 py-1" style={{ background: "var(--surface-2)", color: "var(--text-muted)" }}>
              {t.tool}()
            </span>
          ))}
        </div>
      )}

      {error && (
        <div className="text-[12.5px] mb-2" style={{ color: "var(--critical)" }}>
          {error}
        </div>
      )}

      <div className="flex gap-2">
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && send()}
          placeholder="Message Eunomia…"
          disabled={busy}
          className="field flex-1 px-3.5 py-2.5 text-[13.5px]"
        />
        <button onClick={send} disabled={busy} className="field px-3.5" style={{ color: "var(--accent)" }}>
          <Send size={16} />
        </button>
      </div>
    </div>
  );
}
