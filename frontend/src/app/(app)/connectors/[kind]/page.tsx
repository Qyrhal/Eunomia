"use client";

import { use, useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { ArrowRight, ChevronDown, ChevronRight, Search, Trash2 } from "lucide-react";
import { connectors, sources, tools, type SourceRow, type ToolHit, type ToolRecord } from "@/lib/api";
import SyncStatusCard from "@/components/SyncStatusCard";

const DATE_KEY = /(_at|date|time)$/i;
const MONEY_KEY = /(amount|balance|price|cost|total|spent|fee)/i;
const DURATION_KEY = /duration/i;
const ISO_DATE = /^\d{4}-\d{2}-\d{2}T/;

function formatKey(key: string): string {
  return key
    .replace(/_/g, " ")
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function formatPrimitive(key: string, value: unknown): string {
  if (value === null || value === undefined || value === "") return "—";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "number") {
    if (MONEY_KEY.test(key)) return `$${value.toFixed(2)}`;
    if (DURATION_KEY.test(key)) return `${value} min`;
    return value.toLocaleString();
  }
  if (typeof value === "string") {
    if (DATE_KEY.test(key) || ISO_DATE.test(value)) {
      const d = new Date(value);
      if (!Number.isNaN(d.getTime())) return d.toLocaleString();
    }
    return value;
  }
  return String(value);
}

/** Renders one payload value, formatted by simple heuristics on its key/type
 * (dates, currency-looking numbers) -- never a raw JSON dump. */
function PayloadValue({ k, v }: { k: string; v: unknown }) {
  if (typeof v === "string" && /^https?:\/\//i.test(v)) {
    return (
      <a href={v} target="_blank" rel="noreferrer" className="underline break-all" style={{ color: "var(--ink)" }}>
        {v}
      </a>
    );
  }
  if (Array.isArray(v)) {
    if (v.length === 0) return <span style={{ color: "var(--ink-faint)" }}>—</span>;
    if (v.every((item) => item === null || typeof item !== "object")) {
      return <span>{v.map((item) => formatPrimitive(k, item)).join(", ")}</span>;
    }
    return <span style={{ color: "var(--ink-dim)" }}>{v.length} items</span>;
  }
  if (v && typeof v === "object") {
    const entries = Object.entries(v as Record<string, unknown>);
    if (entries.length === 0) return <span style={{ color: "var(--ink-faint)" }}>—</span>;
    return (
      <div className="flex flex-col gap-1 mt-1 pl-3" style={{ borderLeft: "1px solid var(--border)" }}>
        {entries.map(([nk, nv]) => (
          <div key={nk} className="flex items-baseline gap-2 text-[12px]">
            <span className="shrink-0" style={{ color: "var(--ink-faint)" }}>
              {formatKey(nk)}
            </span>
            <span style={{ color: "var(--ink-dim)" }}>{typeof nv === "object" && nv !== null ? JSON.stringify(nv) : formatPrimitive(nk, nv)}</span>
          </div>
        ))}
      </div>
    );
  }
  return <span>{formatPrimitive(k, v)}</span>;
}

function RecordRow({ hit }: { hit: ToolHit }) {
  const [open, setOpen] = useState(false);
  const [detail, setDetail] = useState<ToolRecord | null>(null);
  const [loading, setLoading] = useState(false);

  async function toggle() {
    if (!open && !detail) {
      setLoading(true);
      const res = await tools.get(hit.id).catch(() => null);
      setLoading(false);
      if (res && !("error" in res)) setDetail(res);
    }
    setOpen((o) => !o);
  }

  return (
    <li>
      <button onClick={toggle} className="w-full flex items-center gap-3 px-4 py-3 text-left">
        {open ? <ChevronDown size={14} color="var(--ink-faint)" /> : <ChevronRight size={14} color="var(--ink-faint)" />}
        <div className="flex-1 min-w-0">
          <div className="text-[13.5px] font-medium truncate">{hit.title || "Untitled"}</div>
          <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "var(--ink-faint)" }}>
            {hit.occurred_at ? new Date(hit.occurred_at).toLocaleString() : "no date"}
          </div>
        </div>
        <span className="pill shrink-0">{hit.type}</span>
      </button>
      {open && (
        <div className="px-4 pb-4 pl-10">
          {loading && (
            <p className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
              Loading…
            </p>
          )}
          {!loading && detail && (
            <div className="flex flex-col gap-2">
              {Object.entries(detail.payload || {}).map(([k, v]) => (
                <div key={k} className="flex items-start gap-3 text-[12.5px]">
                  <span className="w-32 shrink-0" style={{ color: "var(--ink-faint)" }}>
                    {formatKey(k)}
                  </span>
                  <div className="flex-1 min-w-0" style={{ color: "var(--ink)" }}>
                    <PayloadValue k={k} v={v} />
                  </div>
                </div>
              ))}
              {Object.keys(detail.payload || {}).length === 0 && (
                <p className="text-[12px]" style={{ color: "var(--ink-faint)" }}>
                  No payload fields.
                </p>
              )}
              {detail.recording?.transcript && (
                <details className="text-[12.5px]">
                  <summary className="cursor-pointer" style={{ color: "var(--ink-dim)" }}>
                    Full transcript
                  </summary>
                  <pre className="mt-2 whitespace-pre-wrap font-sans max-h-96 overflow-auto" style={{ color: "var(--ink)" }}>
                    {detail.recording.transcript}
                  </pre>
                </details>
              )}
            </div>
          )}
          {!loading && !detail && (
            <p className="text-[12px]" style={{ color: "var(--critical)" }}>
              Could not load this record.
            </p>
          )}
        </div>
      )}
    </li>
  );
}

/** "Delete all data" with a two-step confirmation: what goes, then type
 * the connector's name. */
function DeleteData({ row, onDeleted }: { row: SourceRow; onDeleted: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [step, setStep] = useState<1 | 2>(1);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function open() {
    setStep(1);
    setTyped("");
    setError(null);
    dialog.current?.showModal();
  }

  async function confirm() {
    setBusy(true);
    try {
      await connectors.deleteData(row.provider);
      dialog.current?.close();
      onDeleted();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <button
        onClick={open}
        className="self-start px-4 py-2 text-[13px] rounded-xl inline-flex items-center gap-1.5"
        style={{ border: "1px solid var(--critical)", color: "var(--critical)" }}
      >
        <Trash2 size={13} /> Delete all {row.label} data
      </button>
      <dialog ref={dialog} aria-labelledby="delete-data-title" className="ledger p-6 max-w-md w-[calc(100%-32px)] m-auto backdrop:bg-black/50" style={{ color: "var(--ink)" }}>
        <h2 id="delete-data-title" className="font-display text-xl mb-3">
          {step === 1 ? `Delete all ${row.label} data?` : "Are you sure?"}
        </h2>
        {step === 1 ? (
          <p className="text-[13px] mb-5" style={{ color: "var(--ink-dim)" }}>
            This permanently deletes the {row.record_count.toLocaleString()} {row.label} records Eunomia has synced, their
            search index, and every fact and relation extracted from them. The connection itself stays, so later syncs
            bring in new data only.
          </p>
        ) : (
          <label className="text-[13px] flex flex-col gap-1.5 mb-5" style={{ color: "var(--ink-dim)" }}>
            <span>
              Type <span className="font-mono" style={{ color: "var(--ink)" }}>{row.label}</span> to confirm. This can&apos;t be undone.
            </span>
            <input autoFocus className="field px-3 py-2.5 text-[13.5px] font-mono" value={typed} onChange={(e) => setTyped(e.target.value)} />
          </label>
        )}
        {error && (
          <p role="alert" className="text-[12.5px] mb-3" style={{ color: "var(--critical)" }}>
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <button onClick={() => dialog.current?.close()} className="field px-4 py-2 text-[13px]">
            Cancel
          </button>
          {step === 1 ? (
            <button onClick={() => setStep(2)} className="px-4 py-2 text-[13px] font-medium rounded-xl" style={{ background: "var(--critical)", color: "var(--canvas)" }}>
              Continue
            </button>
          ) : (
            <button
              onClick={confirm}
              disabled={typed !== row.label || busy}
              className="px-4 py-2 text-[13px] font-medium rounded-xl disabled:opacity-40"
              style={{ background: "var(--critical)", color: "var(--canvas)" }}
            >
              {busy ? "Deleting…" : "Delete everything"}
            </button>
          )}
        </div>
      </dialog>
    </>
  );
}

export default function ConnectorWorkspacePage({ params }: { params: Promise<{ kind: string }> }) {
  const { kind } = use(params);
  const [row, setRow] = useState<SourceRow | null | undefined>(undefined);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ToolHit[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [syncing, setSyncing] = useState(false);

  const loadRow = useCallback(
    () =>
      sources
        .list()
        .then((rows) => setRow(rows.find((r) => r.key === kind) ?? null))
        .catch(() => setRow(null)),
    [kind]
  );

  const loadRecords = useCallback(() => {
    tools
      .list({ filters: { source: kind }, sort: "-occurred_at", limit: 50 })
      .then((res) => setResults("results" in res ? res.results : []))
      .catch(() => setResults([]));
  }, [kind]);

  useEffect(() => {
    loadRow();
    loadRecords();
  }, [loadRow, loadRecords]);

  async function runSearch(e: React.FormEvent) {
    e.preventDefault();
    if (!query.trim()) {
      loadRecords();
      return;
    }
    setSearching(true);
    const res = await tools.search({ query, sources: [kind], limit: 50 }).catch(() => null);
    setSearching(false);
    setResults(res && "results" in res ? res.results : []);
  }

  async function sync() {
    setSyncing(true);
    try {
      await sources.sync(kind);
      await loadRow();
      loadRecords();
    } finally {
      setSyncing(false);
    }
  }

  if (row === undefined) return null;

  if (row === null) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Connector</div>
        <h1 className="font-display text-3xl mb-6">Unknown source</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--ink-dim)" }}>
            There&apos;s no source called &ldquo;{kind}&rdquo;.
          </p>
          <Link href="/connectors" className="field px-4 py-2 text-[13px] inline-flex items-center gap-1.5" style={{ color: "var(--ink)" }}>
            Back to connectors <ArrowRight size={13} />
          </Link>
        </div>
      </div>
    );
  }

  if (!row.connected) {
    return (
      <div className="max-w-2xl">
        <div className="eyebrow mb-2">Connector</div>
        <h1 className="font-display text-3xl mb-6">{row.label}</h1>
        <div className="ledger p-10 text-center">
          <p className="text-[13.5px] mb-5" style={{ color: "var(--ink-dim)" }}>
            {row.label} isn&apos;t connected yet — nothing to show here.
          </p>
          <Link href="/connectors" className="px-4 py-2 text-[13px] font-medium rounded-xl inline-block" style={{ background: "var(--felt)", color: "var(--canvas)" }}>
            Connect {row.label}
          </Link>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-7 max-w-4xl">
      <div>
        <div className="eyebrow mb-2">Connector</div>
        <h1 className="font-display text-3xl">{row.label}</h1>
      </div>

      <SyncStatusCard source={row} onSync={sync} syncing={syncing} />

      <form onSubmit={runSearch} className="flex items-center gap-2">
        <div className="field flex-1 flex items-center gap-2 px-3 py-2">
          <Search size={14} color="var(--ink-faint)" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={`Search ${row.label}…`}
            className="flex-1 text-[13.5px] bg-transparent outline-none"
            style={{ color: "var(--ink)" }}
          />
        </div>
        <button type="submit" className="px-4 py-2.5 text-[13px] font-medium rounded-xl" style={{ background: "var(--felt)", color: "var(--canvas)" }}>
          {searching ? "Searching…" : "Search"}
        </button>
      </form>

      <ul className="ledger overflow-hidden hairline-rows">
        {results === null && (
          <li className="px-4 py-6 text-center text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
            Loading…
          </li>
        )}
        {results?.map((hit) => (
          <RecordRow key={hit.id} hit={hit} />
        ))}
        {results !== null && results.length === 0 && (
          <li className="px-4 py-6 text-center text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
            No records.
          </li>
        )}
      </ul>

      <DeleteData
        row={row}
        onDeleted={() => {
          loadRow();
          loadRecords();
        }}
      />
    </div>
  );
}
