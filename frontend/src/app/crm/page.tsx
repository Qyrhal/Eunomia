"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Building2, Users, DollarSign, CheckSquare, FileText, ArrowRight } from "lucide-react";
import { api } from "@/lib/api";

type Summary = {
  people: number;
  companies: number;
  deals: number;
  tasks: number;
  notes: number;
};

const RESOURCE_LINKS = [
  { href: "/crm/people", label: "People", icon: Users, color: "var(--series-4)" },
  { href: "/crm/companies", label: "Companies", icon: Building2, color: "var(--series-1)" },
  { href: "/crm/deals", label: "Deals", icon: DollarSign, color: "var(--series-3)" },
  { href: "/crm/tasks", label: "Tasks", icon: CheckSquare, color: "var(--series-2)" },
  { href: "/crm/notes", label: "Notes", icon: FileText, color: "var(--text-muted)" },
];

export default function CrmPage() {
  const [summary, setSummary] = useState<Summary | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .get<Summary>("/api/connectors/twenty/snapshot")
      .then((s) => setSummary(s))
      .catch((e) => setError(e.message));
  }, []);

  if (error) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">CRM</div>
        <h1 className="font-display text-3xl mb-6">CRM</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--text-secondary)" }}>
            Twenty CRM isn&apos;t connected — nothing to show yet.
          </p>
          <Link href="/connectors" className="px-4 py-2 text-[13px] font-medium text-white inline-block" style={{ background: "var(--accent)" }}>
            Connect Twenty CRM
          </Link>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-7 max-w-3xl">
      <div>
        <div className="eyebrow mb-2">CRM</div>
        <h1 className="font-display text-3xl">CRM</h1>
      </div>

      {summary && (
        <div className="grid grid-cols-2 sm:grid-cols-5 gap-3">
          {RESOURCE_LINKS.map(({ label, icon: Icon, color }) => {
            const key = label.toLowerCase() as keyof Summary;
            const count = summary[key];
            const isError = count === -1;
            return (
              <Link
                key={label}
                href={`/crm/${label.toLowerCase()}`}
                className="ledger p-4 flex items-center gap-3 text-left"
                style={{ borderColor: isError ? "var(--critical)" : "var(--border)" }}
              >
                <div className="w-9 h-9 flex items-center justify-center rounded-full" style={{ background: isError ? "var(--critical)" : color, opacity: 0.85 }}>
                  <Icon size={15} color="var(--surface)" strokeWidth={2.25} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-[13px] font-medium truncate">{label}</div>
                  <div className="font-mono text-[18px] leading-none mt-0.5" style={{ color: isError ? "var(--critical)" : "var(--text-primary)" }}>
                    {isError ? "err" : count ?? "–"}
                  </div>
                </div>
                <ArrowRight size={13} style={{ color: "var(--text-muted)", flexShrink: 0 }} />
              </Link>
            );
          })}
        </div>
      )}

      {!summary && !error && (
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--text-secondary)" }}>
            Loading CRM data…
          </p>
        </div>
      )}
    </div>
  );
}
