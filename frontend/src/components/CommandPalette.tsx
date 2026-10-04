"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { LayoutDashboard, Plug, Settings } from "lucide-react";
import { sources, type SourceRow } from "@/lib/api";

type IconType = React.ComponentType<{ size?: number; color?: string; strokeWidth?: number }>;
type Item = { label: string; icon: IconType; go: () => void };

const STATIC_ITEMS: { label: string; href: string; icon: IconType }[] = [
  { label: "Dashboard", href: "/", icon: LayoutDashboard },
  { label: "Connectors", href: "/connectors", icon: Plug },
  { label: "Settings", href: "/settings", icon: Settings },
];

export default function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const [rows, setRows] = useState<SourceRow[]>([]);
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
    sources
      .list()
      .then(setRows)
      .catch(() => setRows([]));
  }, [open]);

  if (!open) return null;

  const connectorItems: Item[] = rows
    .filter((r) => r.connected)
    .map((r) => ({ label: r.label, icon: Plug, go: () => router.push(`/connectors/${r.key}`) }));
  const staticItems: Item[] = STATIC_ITEMS.map((s) => ({ label: s.label, icon: s.icon, go: () => router.push(s.href) }));

  const items = [...staticItems, ...connectorItems].filter((item) => item.label.toLowerCase().includes(query.toLowerCase()));

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
        style={{ boxShadow: "0 24px 60px rgba(0,0,0,0.5)" }}
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
            const Icon = item.icon;
            return (
              <li key={item.label}>
                <button
                  onClick={() => choose(item)}
                  className="w-full flex items-center gap-2.5 px-4 py-2.5 text-[13px] text-left"
                  style={{
                    background: i === index ? "var(--surface-raised)" : "transparent",
                    color: "var(--ink)",
                  }}
                >
                  <Icon size={14} color="var(--ink-faint)" />
                  <span className="flex-1 truncate">{item.label}</span>
                </button>
              </li>
            );
          })}
          {items.length === 0 && (
            <li className="px-4 py-6 text-center text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
              Nothing matches.
            </li>
          )}
        </ul>
      </div>
    </div>
  );
}
