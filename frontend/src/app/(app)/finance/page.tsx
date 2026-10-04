"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { connectors, type FinanceSummary } from "@/lib/api";

const CATEGORY_COLORS = ["var(--series-1)", "var(--series-2)", "var(--series-3)", "var(--series-4)"];

export default function FinancePage() {
  const [summary, setSummary] = useState<FinanceSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    connectors
      .upBankFinanceSummary(30)
      .then(setSummary)
      .catch((e) => setError(e.message));
  }, []);

  if (error) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Finance</div>
        <h1 className="font-display text-3xl mb-6">Ledger</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--ink-dim)" }}>
            Up Bank isn&apos;t connected — nothing to enter in the ledger yet.
          </p>
          <Link href="/connectors" className="px-4 py-2 text-[13px] font-medium rounded-xl inline-block" style={{ background: "var(--felt)", color: "var(--canvas)" }}>
            Connect Up Bank
          </Link>
        </div>
      </div>
    );
  }

  const maxSpend = Math.max(1, ...(summary?.spend_by_category.map((c) => c.amount) ?? [0]));

  return (
    <div className="flex flex-col gap-7 max-w-4xl">
      <div>
        <div className="eyebrow mb-2">Finance</div>
        <h1 className="font-display text-3xl">Ledger</h1>
      </div>

      <div className="grid md:grid-cols-2 gap-5">
        <div className="ledger p-6" style={{ background: "var(--ink)", borderColor: "var(--ink)" }}>
          <div className="eyebrow" style={{ color: "var(--canvas)", opacity: 0.65 }}>
            Balance across accounts
          </div>
          <div className="font-mono text-4xl my-2" style={{ color: "var(--canvas)" }}>
            {summary ? `$${summary.balance.toFixed(2)}` : "–"}
          </div>
          <div className="flex flex-col gap-1.5 mt-4 pt-4" style={{ borderTop: "1px solid rgba(255,255,255,0.14)" }}>
            {summary?.accounts.map((a) => (
              <div key={a.name} className="flex justify-between text-[12.5px] font-mono" style={{ color: "var(--canvas)", opacity: 0.75 }}>
                <span className="font-sans">{a.name}</span>
                <span>${a.balance}</span>
              </div>
            ))}
          </div>
        </div>

        <div className="ledger p-6">
          <div className="eyebrow mb-4">Spend by category — 30 days</div>
          <div className="flex flex-col gap-2.5">
            {summary?.spend_by_category.slice(0, 6).map((c, i) => (
              <div key={c.category} className="flex items-center gap-3">
                <span className="w-24 text-[12.5px] truncate" style={{ color: "var(--ink-dim)" }}>
                  {c.category}
                </span>
                <div className="flex-1 h-1.5" style={{ background: "var(--surface-raised)" }}>
                  <div
                    className="h-1.5"
                    style={{
                      width: `${Math.round((c.amount / maxSpend) * 100)}%`,
                      background: CATEGORY_COLORS[i % CATEGORY_COLORS.length],
                    }}
                  />
                </div>
                <span className="font-mono text-[12.5px] font-medium w-16 text-right">${c.amount.toFixed(2)}</span>
              </div>
            ))}
            {summary && summary.spend_by_category.length === 0 && (
              <span className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
                No spending in this period.
              </span>
            )}
          </div>
        </div>
      </div>

      <div className="ledger p-6">
        <div className="eyebrow mb-3">Recent transactions</div>
        <ul className="hairline-rows">
          {summary?.recent_transactions.map((t, i) => (
            <li key={i} className="flex items-center justify-between py-2.5 text-[13.5px]">
              <div>
                <div>{t.description}</div>
                <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "var(--ink-faint)" }}>
                  {new Date(t.created_at).toLocaleDateString()}
                </div>
              </div>
              <span className="font-mono font-medium" style={{ color: t.amount.startsWith("-") ? "var(--ink)" : "var(--good)" }}>
                {t.amount}
              </span>
            </li>
          ))}
          {summary && summary.recent_transactions.length === 0 && (
            <li className="text-[12.5px] py-6 text-center" style={{ color: "var(--ink-faint)" }}>
              No recent transactions.
            </li>
          )}
        </ul>
      </div>
    </div>
  );
}
