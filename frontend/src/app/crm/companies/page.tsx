"use client";

import { useEffect, useState } from "react";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { api } from "@/lib/api";

type Company = {
  id: string;
  name?: string;
  industry?: string | null;
  website?: string | null;
  phone?: string | null;
  updated_at?: string;
  created_at?: string;
};

type Envelope = {
  data?: Company[];
  items?: Company[];
};

export default function CompaniesPage() {
  const [companies, setCompanies] = useState<Company[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const load = () => {
    setBusy(true);
    setErr(null);
    api
      .get<Envelope>("/api/connectors/twenty/companies?limit=200")
      .then((r) => setCompanies(r.data || r.items || []))
      .catch((e) => setErr(e.message))
      .finally(() => setBusy(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const name = (c: Company) => (c.name || "").trim() || "<no name>";

  if (err && companies.length === 0) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Companies</div>
        <h1 className="font-display text-3xl mb-6">Companies</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--text-secondary)" }}>
            Couldn&apos;t load companies from Twenty CRM.
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
          <h1 className="font-display text-3xl">Companies</h1>
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
        {companies.length === 0 && !busy && (
          <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
            No companies yet.
          </p>
        )}
        {companies.length > 0 && (
          <ul className="hairline-rows">
            {companies.map((c) => (
              <li key={c.id} className="flex items-baseline justify-between py-3 text-[13.5px]">
                <div>
                  <div className="font-medium">{name(c)}</div>
                  <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "var(--text-muted)" }}>
                    {c.industry || c.website || "no details"}
                  </div>
                </div>
                <div className="text-[11.5px] font-mono" style={{ color: "var(--text-muted)" }}>
                  {c.id}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
