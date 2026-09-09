"use client";

import { useEffect, useState } from "react";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { api } from "@/lib/api";

type Note = {
  id: string;
  title?: string;
  content?: string | null;
  author?: { id: string; name?: string } | null;
  updated_at?: string;
  created_at?: string;
};

type Envelope = {
  data?: Note[];
  items?: Note[];
};

export default function NotesPage() {
  const [notes, setNotes] = useState<Note[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const load = () => {
    setBusy(true);
    setErr(null);
    api
      .get<Envelope>("/api/connectors/twenty/notes?limit=200")
      .then((r) => setNotes(r.data || r.items || []))
      .catch((e) => setErr(e.message))
      .finally(() => setBusy(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const title = (n: Note) => (n.title || "").trim() || (n.content || "").slice(0, 60) || "<no title>";

  if (err && notes.length === 0) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Notes</div>
        <h1 className="font-display text-3xl mb-6">Notes</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--text-secondary)" }}>
            Couldn&apos;t load notes from Twenty CRM.
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
          <h1 className="font-display text-3xl">Notes</h1>
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
        {notes.length === 0 && !busy && (
          <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
            No notes yet.
          </p>
        )}
        {notes.length > 0 && (
          <ul className="hairline-rows">
            {notes.map((n) => (
              <li key={n.id} className="py-3 text-[13.5px]">
                <div className="flex items-baseline justify-between gap-3">
                  <div>
                    <div className="font-medium truncate max-w-[520px]">{title(n)}</div>
                    <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "var(--text-muted)" }}>
                      {(n.content || "").slice(0, 120) || "no content"}
                    </div>
                  </div>
                  <div className="text-[11.5px] font-mono shrink-0" style={{ color: "var(--text-muted)" }}>
                    {n.id}
                  </div>
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
