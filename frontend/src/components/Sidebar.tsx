"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { BookOpen, Brain, FolderGit2, LayoutDashboard, LogOut, Menu, MessageSquare, Plug, Search, Settings, Share2, Vault, X } from "lucide-react";
import EunomiaMark from "./EunomiaMark";
import ThemeToggle from "./ThemeToggle";
import { authorColor } from "./AuthorTag";
import { openCommandPalette } from "./CommandPalette";
import { auth, sources, update, type Me, type SourceRow } from "@/lib/api";
import { isLiveSource, sourceHealth, sourceLabel } from "@/lib/sourceState";

const STATIC_NAV = [
  { href: "/", label: "Dashboard", icon: LayoutDashboard },
  { href: "/chat", label: "Chat", icon: MessageSquare },
  { href: "/entities", label: "Entities", icon: Share2 },
  { href: "/code", label: "Code", icon: FolderGit2 },
  { href: "/vaults", label: "Vaults", icon: Vault },
  { href: "/skill", label: "Memory skill", icon: Brain },
  { href: "/connectors", label: "Connectors", icon: Plug },
  { href: "/docs", label: "Docs", icon: BookOpen },
  { href: "/settings", label: "Settings", icon: Settings },
];

function isActive(pathname: string, href: string) {
  return href === "/" ? pathname === "/" : pathname === href || pathname.startsWith(`${href}/`);
}

function NavLink({ href, active, children }: { href: string; active: boolean; children: React.ReactNode }) {
  return (
    <Link
      href={href}
      aria-current={active ? "page" : undefined}
      className="nav-link flex items-center gap-2.5 h-8 px-2.5 text-[13px] rounded-[7px]"
      data-active={active || undefined}
    >
      {children}
    </Link>
  );
}

export default function Sidebar() {
  const pathname = usePathname();
  const router = useRouter();
  const [rows, setRows] = useState<SourceRow[] | null>(null);
  const [me, setMe] = useState<Me | null>(null);
  const [newVersion, setNewVersion] = useState<string | null>(null);
  // The drawer is open only for the page it was opened on, so navigating
  // closes it without an effect.
  const [drawerPath, setDrawerPath] = useState<string | null>(null);
  const drawerOpen = drawerPath === pathname;
  const setDrawerOpen = (open: boolean) => setDrawerPath(open ? pathname : null);

  useEffect(() => {
    sources
      .list()
      .then(setRows)
      .catch(() => setRows([]));
    auth
      .me()
      .then(setMe)
      .catch(() => {});
    update
      .status()
      .then((s) => setNewVersion(s.configured && s.update_available ? s.latest_version : null))
      .catch(() => {});
  }, []);

  const connected = rows?.filter(isLiveSource) ?? [];

  async function logout() {
    await auth.logout().catch(() => {});
    router.replace("/login");
  }

  const brand = (
    <div className="flex items-center gap-2 min-w-0">
      <EunomiaMark size={20} />
      <span className="font-display text-[15px] truncate" style={{ color: "var(--ink)" }}>
        Eunomia
      </span>
      <span className="font-mono text-[10.5px]" style={{ color: "var(--ink-faint)" }}>
        {process.env.NEXT_PUBLIC_APP_VERSION ?? "dev"}
      </span>
    </div>
  );

  // Search / nav / connected sources / account footer -- shared verbatim
  // between the desktop sidebar and the mobile drawer.
  const navBody = (
    <>
      <div className="px-3">
        <button
          onClick={openCommandPalette}
          className="field w-full h-8 px-2.5 flex items-center gap-2 text-[13px] text-left"
          style={{ color: "var(--ink-faint)" }}
        >
          <Search size={14} />
          <span className="flex-1">Search</span>
          <span className="kbd">⌘K</span>
        </button>
      </div>

      <div className="px-3 flex flex-col gap-px">
        {STATIC_NAV.map(({ href, label, icon: Icon }) => (
          <NavLink key={href} href={href} active={isActive(pathname, href)}>
            <Icon size={15} strokeWidth={1.75} className="nav-icon shrink-0" />
            <span className="flex-1">{label}</span>
          </NavLink>
        ))}
        {newVersion && (
          <Link
            href="/settings?tab=updates"
            className="mt-2 flex items-center gap-2 h-8 px-2.5 text-[12.5px] rounded-[7px]"
            style={{ color: "var(--accent-text)", background: "var(--accent-soft)" }}
          >
            <span className="dot" style={{ background: "var(--accent)" }} aria-hidden />
            Update available · {newVersion}
          </Link>
        )}
      </div>

      <div className="flex-1 min-h-0 flex flex-col gap-1 px-3">
        <div className="label px-2.5 pb-1">Connected</div>
        <div className="flex flex-col gap-px overflow-y-auto">
          {rows === null && (
            <div className="px-2.5 py-1.5 flex flex-col gap-2" aria-label="Loading sources">
              <div className="skeleton h-3 w-24" />
              <div className="skeleton h-3 w-16" />
            </div>
          )}
          {rows !== null && connected.length === 0 && (
            <Link href="/connectors" className="px-2.5 py-1.5 text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
              Nothing connected yet. <span style={{ color: "var(--accent-text)" }}>Connect one</span>
            </Link>
          )}
          {connected.map((s) => {
            const href = `/connectors/${s.key}`;
            return (
              <NavLink key={s.key} href={href} active={pathname === href}>
                <span className="dot" style={{ background: sourceHealth(s).tone }} aria-hidden />
                <span className="flex-1 truncate">{sourceLabel(s)}</span>
                <span className="font-mono text-[11px]" style={{ color: "var(--ink-faint)" }}>
                  {s.record_count.toLocaleString()}
                </span>
              </NavLink>
            );
          })}
        </div>
      </div>

      <div className="mx-3 pt-3 flex items-center gap-2" style={{ borderTop: "var(--hair) solid var(--border)" }}>
        <div
          className="w-6 h-6 rounded-full flex items-center justify-center text-[11px] font-semibold shrink-0"
          style={{ background: me?.email ? authorColor(me.email) : "var(--surface-raised)", color: "var(--on-author)" }}
          aria-hidden
        >
          {me?.email ? me.email[0].toUpperCase() : ""}
        </div>
        <span className="flex-1 min-w-0 text-[12px] truncate" style={{ color: "var(--ink-dim)" }}>
          {me?.email ?? ""}
        </span>
        <ThemeToggle />
        <button onClick={logout} aria-label="Log out" title="Log out" className="btn btn-ghost btn-icon btn-sm" style={{ width: 26 }}>
          <LogOut size={14} />
        </button>
      </div>
    </>
  );

  return (
    <>
      {/* Mobile top bar -- replaces the sidebar below `md`. */}
      <div
        className="md:hidden sticky top-0 z-40 flex items-center gap-2 px-3 h-12 shrink-0"
        style={{ background: "var(--surface)", borderBottom: "var(--hair) solid var(--border)" }}
      >
        <button onClick={() => setDrawerOpen(true)} aria-label="Open menu" className="btn btn-ghost btn-icon">
          <Menu size={18} />
        </button>
        {brand}
        <button onClick={openCommandPalette} aria-label="Search" className="btn btn-ghost btn-icon ml-auto">
          <Search size={16} />
        </button>
      </div>

      {/* Desktop sidebar */}
      <nav
        aria-label="Main"
        className="hidden md:flex w-[232px] shrink-0 flex-col gap-5 py-4 sticky top-0 h-screen"
        style={{ background: "var(--surface)", borderRight: "var(--hair) solid var(--border)" }}
      >
        <div className="px-5 h-7 flex items-center">{brand}</div>
        {navBody}
      </nav>

      {/* Mobile drawer -- an overlay, not part of the page flow. */}
      {drawerOpen && (
        <div className="md:hidden fixed inset-0 z-50 flex fade-in" style={{ background: "var(--scrim)" }} onClick={() => setDrawerOpen(false)}>
          <nav
            aria-label="Main"
            className="w-[264px] h-full flex flex-col gap-5 py-4 drawer-in"
            style={{ background: "var(--surface)", borderRight: "var(--hair) solid var(--border)" }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="pl-5 pr-3 flex items-center justify-between">
              {brand}
              <button onClick={() => setDrawerOpen(false)} aria-label="Close menu" className="btn btn-ghost btn-icon">
                <X size={16} />
              </button>
            </div>
            <div className="flex-1 min-h-0 flex flex-col gap-5">{navBody}</div>
          </nav>
        </div>
      )}
    </>
  );
}
