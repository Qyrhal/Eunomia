"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Landmark, Mic, Plug, Settings } from "lucide-react";

type Item = { label: string; hint?: string; go: () => void };

const PAGE_LABELS = ["Finance", "Meetings", "Connectors", "Settings"];
const PAGE_ICONS = [Landmark, Mic, Plug, Settings];
const PAGE_HREFS = ["/finance", "/meetings", "/connectors", "/settings"];

export default function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const router = useRouter();

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setOpen((v) => !v);
      } else if (e.key === "Escape") {
        setOpen(false);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setIndex(0);
    setTimeout(() => inputRef.current?.focus(), 0);
  }, [open]);

  if (!open) return null;

  const items: Item[] = [
    ...PAGE_LABELS.map((label, i) => ({ label, go: () => router.push(PAGE_HREFS[i]) })),
  ].filter((item) => item.label.toLowerCase().includes(query.toLowerCase()));

  function choose(item: Item) {
    item.go();
    setOpen(false);
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh]"
      style={{ background: "rgba(0,0,0,0.35)" }}
      onClick={() => setOpen(false)}
    >
      <div
        className="w-full max-w-md ledger overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setIndex(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setIndex((i) => Math.min(i + 1, items.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setIndex((i) => Math.max(i - 1, 0));
            } else if (e.key === "Enter" && items[index]) {
              choose(items[index]);
            }
          }}
          placeholder="Jump to a page…"
          className="w-full px-4 py-3 text-[14px]"
          style={{ background: "transparent", borderBottom: "1px solid var(--border)" }}
        />
        <ul className="max-h-72 overflow-y-auto">
          {items.map((item, i) => {
            const Icon = i < PAGE_LABELS.length ? PAGE_ICONS[i] : null;
            return (
              <li key={item.label + i}>
                <button
                  onClick={() => choose(item)}
                  className="w-full flex items-center gap-2.5 px-4 py-2.5 text-[13px] text-left"
                  style={{
                    background: i === index ? "var(--surface-2)" : "transparent",
                    color: "var(--text-primary)",
                  }}
                >
                  {Icon && <Icon size={14} color="var(--text-muted)" />}
                  <span className="flex-1 truncate">{item.label}</span>
                  {item.hint && (
                    <span className="eyebrow" style={{ color: "var(--text-muted)" }}>
                      {item.hint}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
          {items.length === 0 && (
            <li className="px-4 py-6 text-center text-[12.5px]" style={{ color: "var(--text-muted)" }}>
              Nothing matches.
            </li>
          )}
        </ul>
      </div>
    </div>
  );
}
