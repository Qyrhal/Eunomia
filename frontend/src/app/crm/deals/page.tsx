"use client";

import { useEffect, useState } from "react";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { api } from "@/lib/api";

type Deal = {
  id: string;
  name?: string;
  amount?: string | null;
  stage?: string | null;
  company?: { id: string; name?: string } | null;
  updated_at?: string;
  created_at?: string;
};

type Envelope = {
  data?: Deal[];
  items?: Deal[];
};

export default function DealsPage() {
  const [deals, setDeals] = useState<Deal[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const load = () => {
    setBusy(true);
    setErr(null);
    api
      .get<Envelope>("/api/connectors/twenty/deals?limit=200")
      .then((r) => setDeals(r.data || r.items || []))
      .catch((e) => setErr(e.message))
      .finally(() => setBusy(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const name = (d: Deal) => (d.name || "").trim() || "<no name>";
  const amount = (d: Deal) => d.amount || "–";

  if (err && deals.length === 0) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Deals</div>
        <h1 className="font-display text-3xl mb-6">Deals</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--text-secondary)" }}>
            Couldn&apos;t load deals from Twenty CRM.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-4xl">
      <div className="flex items-baseline justify-between mb-6">
        <div>
          <Link href="/crm" className="eyebrow inline-block mb-2">
            <ArrowLeft size={11} style={{ marginRight: 4, verticalAlign: "middle" }} />
            CRM
          </Link>
          <h1 className="font-display text-3xl">Deals</h1>
        </div>
        <button
          onClick={load}
          disabled={busy}
          className="px-4 py-2 text-[13px] font-medium disabled:opacity-40"
          style={{ background: "var(--accent)", color: "var(--surface)" }}
        >
          {busy ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      <div className="ledger p-6">
        {deals.length === 0 && !busy && (
          <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
            No deals yet.
          </p>
        )}
        {deals.length > 0 && (
          <ul className="hairline-rows">
            {deals.map((d) => (
              <li key={d.id} className="flex items-baseline justify-between py-3 text-[13.5px]">
                <div>
                  <div className="font-medium">{name(d)}</div>
                  <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "var(--text-muted)" }}>
                    {d.stage || "no stage"}
                    {d.company ? ` · ${d.company.name || ""}` : ""}
                  </div>
                </div>
                <div className="font-mono font-medium" style={{ color: d.amount && d.amount.startsWith("-") ? "var(--text-muted)" : "var(--text-primary)" }}>
                  {amount(d)}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
