"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { BookOpen, Building2, MapPin, CornerDownLeft, FileText, FolderGit2, LayoutDashboard, MessageSquare, Plug, Search, Settings, Share2, User, Vault } from "lucide-react";
import type { EntityKind } from "@/lib/types";
import { useSources } from "@/lib/queries/sources";
import { usePaletteSearch } from "@/lib/queries/tools";

type IconType = React.ComponentType<{ size?: number; color?: string; strokeWidth?: number }>;
type Item = { key: string; label: string; sub?: string; icon: IconType; go: () => void };
type Group = { title: string; items: Item[] };

const OPEN_EVENT = "eunomia:open-palette";
/** Opens the palette from anywhere (the sidebar search button, page CTAs). */
export function openCommandPalette() {
  window.dispatchEvent(new Event(OPEN_EVENT));
}

// organisation and location read as a building and a pin, not a person
const ENTITY_ICON: Record<string, IconType> = { person: User, organisation: Building2, location: MapPin, repository: FolderGit2, file: FileText };

const STATIC_ITEMS: { label: string; href: string; icon: IconType }[] = [
  { label: "Dashboard", href: "/", icon: LayoutDashboard },
  { label: "Chat", href: "/chat", icon: MessageSquare },
  { label: "Entities", href: "/entities", icon: Share2 },
  { label: "Code", href: "/code", icon: FolderGit2 },
  { label: "Vaults", href: "/vaults", icon: Vault },
  { label: "Connectors", href: "/connectors", icon: Plug },
  { label: "Docs", href: "/docs", icon: BookOpen },
  { label: "Settings", href: "/settings", icon: Settings },
];

const CODE_KINDS = new Set<EntityKind>(["repository", "file", "symbol"]);
const SEARCH_DEBOUNCE_MS = 250;
const SEARCH_MIN_LENGTH = 2;
const SEARCH_LIMIT = 5;

export default function CommandPalette() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setOpen((v) => !v);
      } else if (e.key === "Escape") {
        setOpen(false);
      }
    }
    const onOpen = () => setOpen(true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener(OPEN_EVENT, onOpen);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener(OPEN_EVENT, onOpen);
    };
  }, []);

  // Mounted only while open, so every open starts from fresh state. No
  // open/close animation: this is opened from the keyboard many times a day.
  return open ? <Palette onClose={() => setOpen(false)} /> : null;
}

function Palette({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const rows = useSources().data ?? [];
  const [debounced, setDebounced] = useState("");
  const router = useRouter();

  // Data search, debounced -- only fires once the nav/connector match is
  // worth supplementing, i.e. the query is long enough to be meaningful.
  const q = query.trim();
  const searching = q.length >= SEARCH_MIN_LENGTH;
  useEffect(() => {
    if (!searching) return;
    const timer = setTimeout(() => setDebounced(q), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [q, searching]);
  const { entities: entityHits, records: recordHits } = usePaletteSearch(debounced, SEARCH_LIMIT, debounced !== "");

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

  const entityItems: Item[] = (searching ? entityHits : []).map((e) => ({
    key: `entity:${e.id}`,
    label: e.name,
    sub: e.kind,
    icon: ENTITY_ICON[e.kind] ?? User,
    go: () => router.push(CODE_KINDS.has(e.kind) ? "/code" : "/entities"),
  }));
  const recordItems: Item[] = (searching ? recordHits : []).map((h) => ({
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

  const groups: Group[] = [
    { title: "Pages", items: navItems },
    { title: "Entities", items: entityItems },
    { title: "Records", items: recordItems },
  ].filter((g) => g.items.length > 0);
  const items = groups.flatMap((g) => g.items);

  function choose(item: Item) {
    item.go();
    onClose();
  }

  let flat = -1;
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center px-4 pt-[14vh]" style={{ background: "var(--scrim)" }} onClick={onClose}>
      <div
        role="dialog"
        aria-label="Command palette"
        className="w-full max-w-[560px] panel overflow-hidden"
        style={{ boxShadow: "var(--shadow-pop)" }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2.5 px-4 h-12" style={{ borderBottom: "var(--hair) solid var(--border)" }}>
          <Search size={16} color="var(--ink-faint)" />
          <input
            autoFocus
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
            aria-label="Search pages, entities and records"
            className="flex-1 h-full bg-transparent text-[15px] outline-none"
            style={{ color: "var(--ink)" }}
          />
          <span className="kbd">esc</span>
        </div>
        <div className="max-h-[min(420px,60vh)] overflow-y-auto py-1.5">
          {groups.map((g) => (
            <div key={g.title} className="py-1">
              <div className="label px-4 h-7 flex items-center">{g.title}</div>
              <ul>
                {g.items.map((item) => {
                  flat += 1;
                  const i = flat;
                  const Icon = item.icon;
                  const active = i === index;
                  return (
                    <li key={item.key} className="px-1.5">
                      <button
                        onClick={() => choose(item)}
                        onMouseMove={() => setIndex(i)}
                        className="w-full flex items-center gap-2.5 px-2.5 h-9 text-[13.5px] text-left rounded-[7px]"
                        style={{ background: active ? "var(--surface-raised)" : "transparent", color: "var(--ink)" }}
                      >
                        <Icon size={15} color={active ? "var(--accent-text)" : "var(--ink-faint)"} />
                        <span className="flex-1 truncate">{item.label}</span>
                        {item.sub && (
                          <span className="text-[12px] shrink-0" style={{ color: "var(--ink-faint)" }}>
                            {item.sub}
                          </span>
                        )}
                        {active && <CornerDownLeft size={13} color="var(--ink-faint)" />}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
          {items.length === 0 && (
            <div className="px-4 py-8 text-center text-[13px]" style={{ color: "var(--ink-faint)" }}>
              Nothing matches &ldquo;{query}&rdquo;.
            </div>
          )}
        </div>
        <div
          className="flex items-center gap-4 px-4 h-9 text-[11.5px]"
          style={{ borderTop: "var(--hair) solid var(--border)", color: "var(--ink-faint)" }}
        >
          <span className="flex items-center gap-1.5"><span className="kbd">↑</span><span className="kbd">↓</span> move</span>
          <span className="flex items-center gap-1.5"><span className="kbd">↵</span> open</span>
          <span className="ml-auto">Type 2+ letters to search memory</span>
        </div>
      </div>
    </div>
  );
}
