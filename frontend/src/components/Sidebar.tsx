"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { LayoutDashboard, Mic, Plug, Settings, Wallet } from "lucide-react";
import HourRing from "./HourRing";
import { sources, type SyncStatus } from "@/lib/api";

const NAV = [
  { href: "/", label: "Dashboard", icon: LayoutDashboard, sourceKey: null },
  { href: "/finance", label: "Finance", icon: Wallet, sourceKey: "up_bank" },
  { href: "/meetings", label: "Meetings", icon: Mic, sourceKey: "heypocket" },
  { href: "/connectors", label: "Connectors", icon: Plug, sourceKey: null },
  { href: "/settings", label: "Settings", icon: Settings, sourceKey: null },
];

function healthColor(status: SyncStatus | undefined): string | null {
  if (!status) return null;
  if (status.consecutive_failures === 0) return "var(--good)";
  if (status.consecutive_failures <= 2) return "var(--warning)";
  return "var(--critical)";
}

export default function Sidebar() {
  const pathname = usePathname();
  const [statuses, setStatuses] = useState<Record<string, SyncStatus>>({});

  useEffect(() => {
    sources
      .status()
      .then(setStatuses)
      .catch(() => {});
  }, []);

  return (
    <nav
      className="w-60 shrink-0 flex flex-col gap-1 py-7"
      style={{ background: "var(--surface)", borderRight: "1px solid var(--border)" }}
    >
      <div className="px-7 pb-8 flex items-center gap-2.5">
        <HourRing size={22} color="var(--ink)" />
        <span className="font-display text-[19px]" style={{ color: "var(--ink)" }}>
          Eunomia
        </span>
      </div>
      <div className="px-4 flex flex-col gap-0.5">
        {NAV.map(({ href, label, icon: Icon, sourceKey }) => {
          const active = pathname === href;
          const dot = sourceKey ? healthColor(statuses[sourceKey]) : null;
          return (
            <Link
              key={href}
              href={href}
              className="flex items-center gap-2.5 px-3 py-2 text-[13.5px] relative"
              style={{
                color: active ? "var(--ink)" : "var(--ink-dim)",
                fontWeight: active ? 600 : 400,
              }}
            >
              <span
                className="absolute left-0 top-1.5 bottom-1.5 w-[2px]"
                style={{ background: active ? "var(--ink)" : "transparent" }}
              />
              <Icon size={15} strokeWidth={active ? 2.25 : 1.75} />
              <span className="flex-1">{label}</span>
              {dot && <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: dot }} aria-hidden />}
            </Link>
          );
        })}
      </div>
      <div className="mt-auto px-7 pt-6 pb-10 eyebrow truncate">Register of Order</div>
    </nav>
  );
}
