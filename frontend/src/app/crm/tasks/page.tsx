"use client";

import { useEffect, useState } from "react";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { api } from "@/lib/api";

type Task = {
  id: string;
  title?: string;
  status?: string | null;
  due_date?: string | null;
  assigned_to?: { id: string; name?: string } | null;
  updated_at?: string;
  created_at?: string;
};

type Envelope = {
  data?: Task[];
  items?: Task[];
};

export default function TasksPage() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const load = () => {
    setBusy(true);
    setErr(null);
    api
      .get<Envelope>("/api/connectors/twenty/tasks?limit=200")
      .then((r) => setTasks(r.data || r.items || []))
      .catch((e) => setErr(e.message))
      .finally(() => setBusy(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const title = (t: Task) => (t.title || "").trim() || "<no title>";

  if (err && tasks.length === 0) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Tasks</div>
        <h1 className="font-display text-3xl mb-6">Tasks</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--text-secondary)" }}>
            Couldn&apos;t load tasks from Twenty CRM.
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
          <h1 className="font-display text-3xl">Tasks</h1>
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
        {tasks.length === 0 && !busy && (
          <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
            No tasks yet.
          </p>
        )}
        {tasks.length > 0 && (
          <ul className="hairline-rows">
            {tasks.map((t) => (
              <li key={t.id} className="flex items-baseline justify-between py-3 text-[13.5px]">
                <div>
                  <div className="font-medium">{title(t)}</div>
                  <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "var(--text-muted)" }}>
                    {t.status || "no status"}
                    {t.due_date ? ` · due ${t.due_date.slice(0, 10)}` : ""}
                  </div>
                </div>
                <div className="text-[11.5px] font-mono" style={{ color: "var(--text-muted)" }}>
                  {t.id}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
