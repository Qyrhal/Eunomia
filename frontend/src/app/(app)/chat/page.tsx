"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { ArrowUp, Plus, Trash2 } from "lucide-react";
import Markdown from "@/components/Markdown";
import { chat, type ChatMessage, type ChatThread } from "@/lib/api";

function Bubble({ message }: { message: ChatMessage }) {
  const isUser = message.role === "user";
  if (message.role === "tool") {
    return (
      <div className="self-start max-w-[75%] px-3.5 py-2 rounded-xl text-[12px] font-mono" style={{ background: "var(--surface)", color: "var(--ink-faint)", border: "1px solid var(--border)" }}>
        {message.content}
      </div>
    );
  }
  return (
    <div
      className={`max-w-[75%] px-4 py-2.5 rounded-xl text-[13.5px] ${isUser ? "whitespace-pre-wrap" : ""}`}
      style={{
        alignSelf: isUser ? "flex-end" : "flex-start",
        background: isUser ? "var(--surface-raised)" : "var(--surface)",
        border: "1px solid var(--border)",
        color: "var(--ink)",
      }}
    >
      {isUser ? message.content : <Markdown text={message.content} />}
      {message.tool_calls && message.tool_calls.length > 0 && (
        <div className="mt-1.5 text-[11px]" style={{ color: "var(--ink-faint)" }}>
          used: {message.tool_calls.map((tc) => tc.function.name).join(", ")}
        </div>
      )}
    </div>
  );
}

function ThreadList({
  threads,
  activeId,
  onSelect,
  onCreate,
  onDelete,
}: {
  threads: ChatThread[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
}) {
  return (
    <div className="surface flex flex-col gap-1 p-2.5 w-56 shrink-0 h-full overflow-y-auto">
      <button
        onClick={onCreate}
        className="flex items-center gap-1.5 px-2.5 py-2 rounded-lg text-[12.5px] mb-1 shrink-0"
        style={{ background: "var(--felt)", color: "var(--canvas)" }}
      >
        <Plus size={14} /> New chat
      </button>
      {threads.map((t) => (
        <div
          key={t.id}
          className="group flex items-center gap-1 rounded-lg"
          style={{ background: t.id === activeId ? "var(--surface-raised)" : "transparent" }}
        >
          <button
            onClick={() => onSelect(t.id)}
            className="flex-1 text-left px-2.5 py-2 text-[12.5px] truncate"
            style={{ color: t.id === activeId ? "var(--ink)" : "var(--ink-dim)" }}
          >
            {t.title}
          </button>
          <button
            onClick={() => onDelete(t.id)}
            aria-label="Delete thread"
            className="p-1.5 mr-1 rounded opacity-0 group-hover:opacity-100 shrink-0"
            style={{ color: "var(--ink-faint)" }}
          >
            <Trash2 size={13} />
          </button>
        </div>
      ))}
      {threads.length === 0 && (
        <div className="px-2.5 py-2 text-[12px]" style={{ color: "var(--ink-faint)" }}>
          No threads yet.
        </div>
      )}
    </div>
  );
}

export default function ChatPage() {
  const [threads, setThreads] = useState<ChatThread[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[] | null>(null);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [streamingText, setStreamingText] = useState("");
  const [activeTool, setActiveTool] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [needsKey, setNeedsKey] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  const loadHistory = useCallback((threadId: string) => {
    setMessages(null);
    chat
      .history(threadId)
      .then(setMessages)
      .catch(() => setMessages([]));
  }, []);

  useEffect(() => {
    chat.threads.list().then(async (list) => {
      if (list.length === 0) {
        const created = await chat.threads.create();
        list = [created];
      }
      setThreads(list);
      setActiveId(list[0].id);
      loadHistory(list[0].id);
    });
  }, [loadHistory]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, sending, streamingText]);

  function selectThread(id: string) {
    if (id === activeId) return;
    setActiveId(id);
    setError(null);
    setNeedsKey(false);
    loadHistory(id);
  }

  async function createThread() {
    const created = await chat.threads.create();
    setThreads((prev) => [created, ...prev]);
    setActiveId(created.id);
    setMessages([]);
  }

  async function deleteThread(id: string) {
    await chat.threads.delete(id);
    const remaining = threads.filter((t) => t.id !== id);
    setThreads(remaining);
    if (id !== activeId) return;
    if (remaining.length > 0) {
      setActiveId(remaining[0].id);
      loadHistory(remaining[0].id);
    } else {
      const created = await chat.threads.create();
      setThreads([created]);
      setActiveId(created.id);
      setMessages([]);
    }
  }

  async function send() {
    const text = input.trim();
    if (!text || sending || !activeId) return;
    const threadId = activeId;
    setInput("");
    setError(null);
    setNeedsKey(false);
    setMessages((prev) => [...(prev ?? []), { role: "user", content: text }]);
    setSending(true);
    setStreamingText("");
    setActiveTool(null);
    try {
      let reply = "";
      await chat.send(threadId, text, (event) => {
        if (event.type === "text") {
          reply += event.delta;
          setStreamingText(reply);
        } else if (event.type === "tool_call") {
          setActiveTool(event.name);
        } else if (event.type === "tool_result") {
          setActiveTool(null);
        } else if (event.type === "error") {
          setError(event.message);
          setNeedsKey(/OpenAI API key/i.test(event.message));
        }
      });
      setMessages((prev) => [...(prev ?? []), { role: "assistant", content: reply }]);
      chat.threads.list().then(setThreads);
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Something went wrong.";
      setError(msg);
      setNeedsKey(/OpenAI API key/i.test(msg));
    } finally {
      setSending(false);
      setStreamingText("");
      setActiveTool(null);
    }
  }

  return (
    <div className="flex flex-col gap-5 h-[calc(100vh-11rem)] md:h-[calc(100vh-7.5rem)]">
      <div>
        <h1 className="font-display text-xl" style={{ color: "var(--ink)" }}>
          Chat
        </h1>
        <p className="text-[12.5px] mt-1" style={{ color: "var(--ink-dim)" }}>
          Ask about people, projects, or code you&apos;ve mapped. The assistant can recall, search, and write to your
          memory graph.
        </p>
      </div>

      <div className="flex-1 min-h-0 flex gap-4">
        <ThreadList threads={threads} activeId={activeId} onSelect={selectThread} onCreate={createThread} onDelete={deleteThread} />

        <div className="flex-1 min-h-0 flex flex-col gap-5">
          <div className="surface flex-1 min-h-0 flex flex-col p-5 overflow-y-auto gap-3">
            {messages === null && <div className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>Loading…</div>}
            {messages !== null && messages.length === 0 && !sending && (
              <div className="flex-1 flex items-center justify-center text-center text-[13px]" style={{ color: "var(--ink-faint)" }}>
                Ask about people, projects, or code you&apos;ve mapped.
              </div>
            )}
            {messages?.map((m, i) => <Bubble key={i} message={m} />)}
            {sending && (
              <div
                className="max-w-[75%] px-4 py-2.5 rounded-xl text-[13.5px]"
                style={{ background: "var(--surface)", border: "1px solid var(--border)", color: streamingText ? "var(--ink)" : "var(--ink-faint)" }}
              >
                {activeTool && (
                  <div className="text-[12px] mb-1" style={{ color: "var(--ink-faint)" }}>
                    Using {activeTool}…
                  </div>
                )}
                {streamingText ? <Markdown text={streamingText} /> : activeTool ? "" : "Thinking…"}
              </div>
            )}
            <div ref={bottomRef} />
          </div>

          {error && (
            <div className="text-[12.5px]" style={{ color: "var(--critical)" }}>
              {error}
              {needsKey && (
                <>
                  {" "}
                  <Link href="/settings" className="underline" style={{ color: "var(--ink)" }}>
                    Add one in Settings
                  </Link>
                  .
                </>
              )}
            </div>
          )}

          <div className="flex items-center gap-2.5">
            <input
              className="field flex-1 px-3.5 py-2.5 text-[13.5px]"
              placeholder="Ask something…"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  send();
                }
              }}
              disabled={sending}
            />
            <button
              onClick={send}
              disabled={sending || !input.trim()}
              aria-label="Send"
              className="p-2.5 rounded-xl shrink-0"
              style={{ background: "var(--felt)", color: "var(--canvas)", opacity: sending || !input.trim() ? 0.5 : 1 }}
            >
              <ArrowUp size={16} />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
