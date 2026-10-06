"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { FileText, FolderGit2, LayoutDashboard, MessageSquare, Plug, Settings, Share2, User } from "lucide-react";
import { sources, tools, type EntityKind, type EntitySummary, type SourceRow, type ToolHit } from "@/lib/api";

type IconType = React.ComponentType<{ size?: number; color?: string; strokeWidth?: number }>;
type Item = { key: string; label: string; sub?: string; icon: IconType; go: () => void };

const STATIC_ITEMS: { label: string; href: string; icon: IconType }[] = [
  { label: "Dashboard", href: "/", icon: LayoutDashboard },
  { label: "Chat", href: "/chat", icon: MessageSquare },
  { label: "Entities", href: "/entities", icon: Share2 },
  { label: "Code", href: "/code", icon: FolderGit2 },
  { label: "Connectors", href: "/connectors", icon: Plug },
  { label: "Settings", href: "/settings", icon: Settings },
];

const CODE_KINDS = new Set<EntityKind>(["repository", "file", "symbol"]);
const SEARCH_DEBOUNCE_MS = 250;
const SEARCH_MIN_LENGTH = 2;
const SEARCH_LIMIT = 5;

export default function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const [rows, setRows] = useState<SourceRow[]>([]);
  const [entityHits, setEntityHits] = useState<EntitySummary[]>([]);
  const [recordHits, setRecordHits] = useState<ToolHit[]>([]);
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
    setEntityHits([]);
    setRecordHits([]);
    setTimeout(() => inputRef.current?.focus(), 0);
    sources
      .list()
      .then(setRows)
      .catch(() => setRows([]));
  }, [open]);

  // Data search, debounced -- only fires once the nav/connector match is
  // worth supplementing, i.e. the query is long enough to be meaningful.
  useEffect(() => {
    if (!open) return;
    const q = query.trim();
    if (q.length < SEARCH_MIN_LENGTH) {
      setEntityHits([]);
      setRecordHits([]);
      return;
    }
    const timer = setTimeout(() => {
      tools
        .call("entities_search", { query: q, limit: SEARCH_LIMIT })
        .then((res) => setEntityHits("results" in res ? (res.results as EntitySummary[]) : []))
        .catch(() => setEntityHits([]));
      tools
        .search({ query: q, limit: SEARCH_LIMIT })
        .then((res) => setRecordHits("results" in res ? res.results : []))
        .catch(() => setRecordHits([]));
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [open, query]);

  if (!open) return null;

  const connectorItems: Item[] = rows
    .filter((r) => r.connected)
    .map((r) => ({ key: `conn:${r.key}`, label: r.label, icon: Plug, go: () => router.push(`/connectors/${r.key}`) }));
  const staticItems: Item[] = STATIC_ITEMS.map((s) => ({
    key: `nav:${s.href}`,
    label: s.label,
    icon: s.icon,
    go: () => router.push(s.href),
  }));
  const navItems = [...staticItems, ...connectorItems].filter((item) =>
    item.label.toLowerCase().includes(query.toLowerCase())
  );

  const entityItems: Item[] = entityHits.map((e) => ({
    key: `entity:${e.id}`,
    label: e.name,
    sub: e.kind,
    icon: User,
    go: () => router.push(CODE_KINDS.has(e.kind) ? "/code" : "/entities"),
  }));
  const recordItems: Item[] = recordHits.map((h) => ({
    key: `record:${h.id}`,
    label: h.title || h.snippet,
    sub: h.source,
    icon: FileText,
    go: () => {
      if (h.url) {
        window.open(h.url, "_blank", "noopener,noreferrer");
      } else {
        router.push(`/connectors/${h.source}`);
      }
    },
  }));

  const items = [...navItems, ...entityItems, ...recordItems];

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
        <ul className="max-h-80 overflow-y-auto">
          {items.map((item, i) => {
            const Icon = item.icon;
            return (
              <li key={item.key}>
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
                  {item.sub && (
                    <span className="text-[11px] shrink-0" style={{ color: "var(--ink-faint)" }}>
                      {item.sub}
                    </span>
                  )}
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
