"use client";

import { useEffect, useState } from "react";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { api } from "@/lib/api";

type Person = {
  id: string;
  name?: string;
  email?: string | null;
  phone?: string | null;
  company?: { id: string; name?: string } | null;
  updated_at?: string;
  created_at?: string;
};

type PeopleEnvelope = {
  data?: Person[];
  items?: Person[];
};

export default function PeoplePage() {
  const [people, setPeople] = useState<Person[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const load = () => {
    setBusy(true);
    setErr(null);
    api
      .get<PeopleEnvelope>("/api/connectors/twenty/people?limit=200")
      .then((r) => setPeople(r.data || r.items || []))
      .catch((e) => setErr(e.message))
      .finally(() => setBusy(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const name = (p: Person) => (p.name || "").trim() || "<no name>";

  if (err && people.length === 0) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">People</div>
        <h1 className="font-display text-3xl mb-6">People</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--text-secondary)" }}>
            Couldn&apos;t load people from Twenty CRM.
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
          <h1 className="font-display text-3xl">People</h1>
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
        {people.length === 0 && !busy && (
          <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
            No people yet.
          </p>
        )}
        {people.length > 0 && (
          <ul className="hairline-rows">
            {people.map((p) => (
              <li key={p.id} className="flex items-baseline justify-between py-3 text-[13.5px]">
                <div>
                  <div className="font-medium">{name(p)}</div>
                  <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "var(--text-muted)" }}>
                    {p.email || "no email"}
                    {p.phone ? ` · ${p.phone}` : ""}
                  </div>
                </div>
                <div className="text-[11.5px] font-mono" style={{ color: "var(--text-muted)" }}>
                  {p.id}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
