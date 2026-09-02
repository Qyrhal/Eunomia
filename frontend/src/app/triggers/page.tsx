"use client";

import { useCallback, useEffect, useState } from "react";
import { Play, Trash2 } from "lucide-react";
import {
  Delivery,
  Trigger,
  createTrigger,
  deleteTrigger,
  deliveryLog,
  listTriggers,
  testTrigger,
  updateTrigger,
} from "@/lib/admin";

const KINDS = ["record_rule", "schedule", "cron"] as const;
const SAMPLE: Record<string, string> = {
  record_rule: '{\n  "types": ["up.transaction"],\n  "match": [["payload.amount_cents", "lt", -20000]]\n}',
  schedule: '{\n  "anchor": "task.due_at",\n  "offset_s": -86400\n}',
  cron: '{\n  "cron": "0 8 * * *",\n  "payload": {"kind": "digest"}\n}',
};

export default function TriggersPage() {
  const [triggers, setTriggers] = useState<Trigger[] | null>(null);
  const [deliveries, setDeliveries] = useState<Delivery[]>([]);
  const [key, setKey] = useState("");
  const [kind, setKind] = useState<(typeof KINDS)[number]>("record_rule");
  const [spec, setSpec] = useState(SAMPLE.record_rule);
  const [err, setErr] = useState<string | null>(null);

  const load = useCallback(() => {
    listTriggers().then((r) => setTriggers(r.triggers)).catch(() => setTriggers([]));
    deliveryLog().then((r) => setDeliveries(r.deliveries)).catch(() => {});
  }, []);
  useEffect(() => {
    load();
  }, [load]);

  async function add(e: React.FormEvent) {
    e.preventDefault();
    setErr(null);
    let parsed: unknown;
    try {
      parsed = JSON.parse(spec);
    } catch {
      setErr("Spec is not valid JSON.");
      return;
    }
    try {
      await createTrigger({ key: key.trim(), kind, spec: parsed });
      setKey("");
      load();
    } catch (e2) {
      setErr(e2 instanceof Error ? e2.message : "Could not create the trigger.");
    }
  }

  return (
    <div className="max-w-4xl">
      <h1 className="font-display text-3xl mb-1">Triggers</h1>
      <p className="text-[13px] mb-6" style={{ color: "var(--text-secondary)" }}>
        Watches that fire a signed webhook to Hermes. Built-ins ship disabled.
      </p>

      <ul className="ledger overflow-hidden hairline-rows mb-8">
        {triggers?.length === 0 && (
          <li className="px-4 py-3 text-[13px]" style={{ color: "var(--text-muted)" }}>
            No triggers.
          </li>
        )}
        {triggers?.map((t) => (
          <li key={t.key} className="px-4 py-3 flex items-center gap-3">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={t.enabled}
                onChange={() => updateTrigger({ key: t.key, enabled: !t.enabled }).then(load)}
                className="accent-[var(--accent)]"
              />
              <span className="text-[13.5px] font-medium">{t.key}</span>
            </label>
            <span className="eyebrow">{t.kind}</span>
            <span className="text-[11.5px] font-mono" style={{ color: "var(--text-muted)" }}>
              fired {t.fire_count}×
            </span>
            <div className="ml-auto flex items-center gap-3">
              <button
                onClick={() => testTrigger(t.key).then(load)}
                className="text-[12px] inline-flex items-center gap-1"
                style={{ color: "var(--accent)" }}
              >
                <Play size={12} /> test
              </button>
              <button
                onClick={() => deleteTrigger(t.key).then(load)}
                aria-label={`Delete ${t.key}`}
              >
                <Trash2 size={13} style={{ color: "var(--text-muted)" }} />
              </button>
            </div>
          </li>
        ))}
      </ul>

      <form onSubmit={add} className="ledger p-5 mb-8 flex flex-col gap-3">
        <div className="eyebrow">New trigger</div>
        <div className="flex gap-2">
          <input
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder="key, e.g. rent_paid"
            className="field flex-1 px-3 py-2 text-[13px] font-mono"
            required
          />
          <select
            value={kind}
            onChange={(e) => {
              const k = e.target.value as (typeof KINDS)[number];
              setKind(k);
              setSpec(SAMPLE[k]);
            }}
            className="field px-2.5 py-2 text-[12.5px]"
          >
            {KINDS.map((k) => (
              <option key={k}>{k}</option>
            ))}
          </select>
        </div>
        <textarea
          value={spec}
          onChange={(e) => setSpec(e.target.value)}
          rows={5}
          spellCheck={false}
          className="field px-3 py-2 text-[12px] font-mono"
        />
        {err && (
          <div className="text-[12px]" style={{ color: "var(--critical)" }}>
            {err}
          </div>
        )}
        <button
          type="submit"
          className="self-start px-4 py-2 text-[13px] font-medium"
          style={{ background: "var(--accent)", color: "var(--surface)", borderRadius: 4 }}
        >
          Create
        </button>
      </form>

      <div className="eyebrow mb-2">Delivery log</div>
      <ul className="ledger overflow-hidden hairline-rows">
        {deliveries.length === 0 && (
          <li className="px-4 py-3 text-[13px]" style={{ color: "var(--text-muted)" }}>
            Nothing delivered yet.
          </li>
        )}
        {deliveries.map((d, i) => (
          <li key={i} className="px-4 py-2 flex items-baseline gap-3 text-[12px]">
            <span
              className="w-1.5 h-1.5 rounded-full shrink-0"
              style={{ background: d.ok ? "var(--good)" : d.dead ? "var(--critical)" : "var(--warning)" }}
            />
            <span className="font-medium">{d.trigger}</span>
            <span className="font-mono truncate" style={{ color: "var(--text-secondary)" }}>
              {d.entity}
            </span>
            <span className="font-mono" style={{ color: "var(--text-muted)" }}>
              {d.status ?? d.detail} · try {d.attempt}
            </span>
            <span className="ml-auto font-mono shrink-0" style={{ color: "var(--text-muted)" }}>
              {new Date(d.at).toLocaleTimeString()}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
