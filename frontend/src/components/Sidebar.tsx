"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { BarChart3, CheckSquare, MessageCircle, Plug, Settings, Wallet } from "lucide-react";
import HourRing from "./HourRing";

const NAV = [
  { href: "/", label: "Dashboard", icon: BarChart3 },
  { href: "/tasks", label: "Tasks", icon: CheckSquare },
  { href: "/finance", label: "Finance", icon: Wallet },
  { href: "/connectors", label: "Connectors", icon: Plug },
  { href: "/chat", label: "Assistant", icon: MessageCircle },
  { href: "/settings", label: "Settings", icon: Settings },
];

export default function Sidebar() {
  const pathname = usePathname();
  return (
    <nav
      className="w-60 shrink-0 flex flex-col gap-1 py-7"
      style={{ background: "var(--surface)", borderRight: "1px solid var(--border)" }}
    >
      <div className="px-7 pb-8 flex items-center gap-2.5">
        <HourRing size={22} color="var(--accent)" />
        <span className="font-display text-[19px]" style={{ color: "var(--text-primary)" }}>
          Eunomia
        </span>
      </div>
      <div className="px-4 flex flex-col gap-0.5">
        {NAV.map(({ href, label, icon: Icon }) => {
          const active = pathname === href;
          return (
            <Link
              key={href}
              href={href}
              className="flex items-center gap-2.5 px-3 py-2 text-[13.5px] relative"
              style={{
                color: active ? "var(--text-primary)" : "var(--text-secondary)",
                fontWeight: active ? 600 : 400,
              }}
            >
              <span
                className="absolute left-0 top-1.5 bottom-1.5 w-[2px]"
                style={{ background: active ? "var(--accent)" : "transparent" }}
              />
              <Icon size={15} strokeWidth={active ? 2.25 : 1.75} />
              {label}
            </Link>
          );
        })}
      </div>
      <div className="mt-auto px-7 pt-6 pb-10 eyebrow truncate">Register of Order</div>
    </nav>
  );
}
