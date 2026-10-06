"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { FolderGit2, LayoutDashboard, LogOut, Menu, MessageSquare, Plug, Settings, Share2, Vault, X } from "lucide-react";
import EunomiaMark from "./EunomiaMark";
import { auth, sources, type Me, type SourceRow } from "@/lib/api";

const STATIC_NAV = [
  { href: "/", label: "Dashboard", icon: LayoutDashboard },
  { href: "/chat", label: "Chat", icon: MessageSquare },
  { href: "/entities", label: "Entities", icon: Share2 },
  { href: "/code", label: "Code", icon: FolderGit2 },
  { href: "/vaults", label: "Vaults", icon: Vault },
  { href: "/connectors", label: "Connectors", icon: Plug },
  { href: "/settings", label: "Settings", icon: Settings },
];

function healthColor(failures: number): string {
  if (failures === 0) return "var(--good)";
  if (failures <= 2) return "var(--warning)";
  return "var(--critical)";
}

export default function Sidebar() {
  const pathname = usePathname();
  const router = useRouter();
  const [rows, setRows] = useState<SourceRow[] | null>(null);
  const [me, setMe] = useState<Me | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);

  useEffect(() => {
    sources
      .list()
      .then(setRows)
      .catch(() => setRows([]));
    auth
      .me()
      .then(setMe)
      .catch(() => {});
  }, []);

  // Close the mobile drawer on navigation, so a link tap doesn't leave it
  // open over the next page.
  useEffect(() => {
    setDrawerOpen(false);
  }, [pathname]);

  const connected = rows?.filter((r) => r.connected) ?? [];

  async function logout() {
    await auth.logout().catch(() => {});
    router.replace("/login");
  }

  // Nav links / connected sources / account footer -- shared verbatim
  // between the desktop sidebar and the mobile drawer, which differ only in
  // their outer chrome (a plain nav vs. an overlay with its own close button).
  const navBody = (
    <>
      <div className="px-3 flex flex-col gap-0.5">
        {STATIC_NAV.map(({ href, label, icon: Icon }) => {
          const active = pathname === href;
          return (
            <Link
              key={href}
              href={href}
              className="flex items-center gap-2.5 px-3 py-2 text-[13.5px] rounded-lg relative"
              style={{
                color: active ? "var(--ink)" : "var(--ink-dim)",
                fontWeight: active ? 600 : 400,
                background: active ? "var(--surface-raised)" : "transparent",
              }}
            >
              <Icon size={15} strokeWidth={active ? 2.25 : 1.75} />
              <span className="flex-1">{label}</span>
            </Link>
          );
        })}
      </div>

      <div className="flex-1 min-h-0 flex flex-col gap-1.5 px-3">
        <div className="eyebrow px-3">Connected</div>
        <div className="flex flex-col gap-0.5 overflow-y-auto">
          {rows === null && (
            <div className="px-3 py-1.5 text-[12px]" style={{ color: "var(--ink-faint)" }}>
              Loading…
            </div>
          )}
          {rows !== null && connected.length === 0 && (
            <div className="px-3 py-1.5 text-[12px]" style={{ color: "var(--ink-faint)" }}>
              Nothing connected yet.
            </div>
          )}
          {connected.map((s) => {
            const href = `/connectors/${s.key}`;
            const active = pathname === href;
            return (
              <Link
                key={s.key}
                href={href}
                className="flex items-center gap-2.5 px-3 py-2 text-[13.5px] rounded-lg"
                style={{
                  color: active ? "var(--ink)" : "var(--ink-dim)",
                  fontWeight: active ? 600 : 400,
                  background: active ? "var(--surface-raised)" : "transparent",
                }}
              >
                <Plug size={13} />
                <span className="flex-1 truncate">{s.label}</span>
                <span
                  className="w-1.5 h-1.5 rounded-full shrink-0"
                  style={{ background: healthColor(s.sync_status.consecutive_failures) }}
                  aria-hidden
                />
              </Link>
            );
          })}
        </div>
      </div>

      <div className="px-5 pt-4 flex items-center gap-2.5" style={{ borderTop: "1px solid var(--border)" }}>
        <div
          className="w-7 h-7 rounded-full flex items-center justify-center text-[12px] font-medium shrink-0"
          style={{ background: "var(--surface-raised)", color: "var(--ink-dim)" }}
          aria-hidden
        >
          {me?.email ? me.email[0].toUpperCase() : "?"}
        </div>
        <span className="flex-1 min-w-0 text-[12px] truncate" style={{ color: "var(--ink-dim)" }}>
          {me?.email ?? ""}
        </span>
        <button
          onClick={logout}
          aria-label="Log out"
          title="Log out"
          className="p-1.5 rounded-lg shrink-0"
          style={{ color: "var(--ink-faint)" }}
        >
          <LogOut size={14} />
        </button>
      </div>
    </>
  );

  return (
    <>
      {/* Mobile top bar -- replaces the fixed-width nav below `md`, where a
          permanent 224px sidebar would eat most of the viewport. */}
      <div
        className="md:hidden flex items-center gap-3 px-4 py-3 shrink-0"
        style={{ background: "var(--surface)", borderBottom: "1px solid var(--border)" }}
      >
        <button
          onClick={() => setDrawerOpen(true)}
          aria-label="Open menu"
          className="p-1.5 -ml-1.5 rounded-lg"
          style={{ color: "var(--ink)" }}
        >
          <Menu size={19} />
        </button>
        <EunomiaMark size={18} />
        <span className="font-display text-[15px]" style={{ color: "var(--ink)" }}>
          Eunomia
        </span>
      </div>

      {/* Desktop sidebar -- unchanged fixed-width nav at `md` and up. */}
      <nav
        className="hidden md:flex w-56 shrink-0 flex-col gap-6 py-6"
        style={{ background: "var(--surface)", borderRight: "1px solid var(--border)" }}
      >
        <div className="px-5 flex items-center gap-2.5">
          <EunomiaMark size={20} />
          <span className="font-display text-[17px]" style={{ color: "var(--ink)" }}>
            Eunomia
          </span>
          <span className="text-[11px]" style={{ color: "var(--ink-faint)" }}>
            v1.0.2
          </span>
        </div>
        {navBody}
      </nav>

      {/* Mobile drawer -- an overlay, not part of the page flow, so it never
          needs the content below to reserve space for it. */}
      {drawerOpen && (
        <div
          className="md:hidden fixed inset-0 z-50 flex"
          style={{ background: "rgba(0,0,0,0.45)" }}
          onClick={() => setDrawerOpen(false)}
        >
          <nav
            className="w-64 h-full flex flex-col gap-6 py-6"
            style={{ background: "var(--surface)", borderRight: "1px solid var(--border)" }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="px-5 flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <EunomiaMark size={20} />
                <span className="font-display text-[17px]" style={{ color: "var(--ink)" }}>
                  Eunomia
                </span>
              </div>
              <button onClick={() => setDrawerOpen(false)} aria-label="Close menu" style={{ color: "var(--ink-faint)" }}>
                <X size={17} />
              </button>
            </div>
            <div className="flex-1 min-h-0 flex flex-col gap-6">{navBody}</div>
          </nav>
        </div>
      )}
    </>
  );
}
