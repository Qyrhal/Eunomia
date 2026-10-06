"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { connectors as connectorsApi, type Connector } from "@/lib/api";
import { CONNECTOR_META, CONNECTOR_ORDER, connectorStatus } from "@/lib/connectorMeta";

type View = "mine" | "discover";
type StatusFilter = "all" | "connected" | "disconnected";

export default function ConnectorsPage() {
  const [connectors, setConnectors] = useState<Connector[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [view, setView] = useState<View | null>(null);
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");

  useEffect(() => {
    connectorsApi.list().then((list) => {
      setConnectors(list);
      setLoaded(true);
    });
  }, []);

  const byKind = Object.fromEntries(connectors.map((c) => [c.kind, c]));
  const isConnected = (kind: Connector["kind"]) => connectorStatus(byKind[kind]) === "connected";

  // Default to "My Connectors" once we know the user has at least one
  // connection; otherwise there's nothing to show there, so land on the
  // full marketplace instead. Resolved after the initial fetch so we don't
  // flash the wrong tab.
  const effectiveView: View = view ?? (loaded && CONNECTOR_ORDER.some(isConnected) ? "mine" : "discover");

  const q = query.trim().toLowerCase();
  let kinds = CONNECTOR_ORDER.filter((kind) => {
    if (effectiveView === "mine" && !isConnected(kind)) return false;
    if (q) {
      const meta = CONNECTOR_META[kind];
      if (!meta.label.toLowerCase().includes(q) && !meta.description.toLowerCase().includes(q)) return false;
    }
    if (effectiveView === "discover" && statusFilter !== "all") {
      const connected = isConnected(kind);
      if (statusFilter === "connected" && !connected) return false;
      if (statusFilter === "disconnected" && connected) return false;
    }
    return true;
  });
  if (effectiveView === "discover") {
    kinds = [...kinds].sort((a, b) => Number(isConnected(b)) - Number(isConnected(a)));
  }

  return (
    <div className="max-w-4xl flex flex-col gap-7">
      <div>
        <div className="eyebrow mb-2">Connectors</div>
        <h1 className="font-display text-3xl">Sealed accounts</h1>
        <p className="text-[13px] mt-2" style={{ color: "var(--ink-dim)" }}>
          Every credential here is encrypted at rest and only ever used by your own instance —
          never sent anywhere but the provider it belongs to.
        </p>
      </div>

      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div className="flex gap-2">
          <button
            type="button"
            className="pill"
            aria-pressed={effectiveView === "mine"}
            onClick={() => setView("mine")}
          >
            My Connectors
          </button>
          <button
            type="button"
            className="pill"
            aria-pressed={effectiveView === "discover"}
            onClick={() => setView("discover")}
          >
            Discover
          </button>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <input
            type="text"
            className="field px-3 py-2 text-[13px]"
            placeholder="Search connectors…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {effectiveView === "discover" && (
            <div className="flex gap-2">
              <button type="button" className="pill" aria-pressed={statusFilter === "all"} onClick={() => setStatusFilter("all")}>
                All
              </button>
              <button
                type="button"
                className="pill"
                aria-pressed={statusFilter === "connected"}
                onClick={() => setStatusFilter("connected")}
              >
                Connected
              </button>
              <button
                type="button"
                className="pill"
                aria-pressed={statusFilter === "disconnected"}
                onClick={() => setStatusFilter("disconnected")}
              >
                Not connected
              </button>
            </div>
          )}
        </div>
      </div>

      {kinds.length === 0 ? (
        <div className="ledger p-10 text-center text-[13px]" style={{ color: "var(--ink-faint)" }}>
          {effectiveView === "mine"
            ? "No connectors set up yet — switch to Discover to connect one."
            : "No connectors match your search."}
        </div>
      ) : (
      <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-5">
        {kinds.map((kind) => {
          const meta = CONNECTOR_META[kind];
          const status = connectorStatus(byKind[kind]);
          return (
            <Link
              key={kind}
              href={`/connectors/setup/${kind}`}
              className="ledger overflow-hidden flex flex-col transition-colors hover:border-[var(--border-strong)]"
            >
              <div
                className="h-24 flex items-center justify-center"
                style={{
                  background: `color-mix(in srgb, ${meta.tint} 14%, var(--surface))`,
                  borderBottom: "1px solid var(--border)",
                }}
              >
                <div
                  className="w-12 h-12 rounded-xl flex items-center justify-center"
                  style={{
                    background: `color-mix(in srgb, ${meta.tint} 22%, var(--surface))`,
                    border: `1px solid color-mix(in srgb, ${meta.tint} 40%, var(--border))`,
                    color: meta.tint,
                  }}
                >
                  {meta.icon}
                </div>
              </div>
              <div className="p-5 flex flex-col gap-2 flex-1">
                <div className="flex items-center justify-between gap-2">
                  <div className="text-[14px] font-medium">{meta.label}</div>
                  <span
                    className="eyebrow shrink-0"
                    style={{ color: status === "demo" ? "var(--warning)" : status === "connected" ? "var(--good)" : "var(--ink-faint)" }}
                  >
                    {status === "demo" ? "Demo data" : status === "connected" ? "Connected" : "Not connected"}
                  </span>
                </div>
                <p className="text-[12.5px]" style={{ color: "var(--ink-dim)" }}>
                  {meta.description}
                </p>
              </div>
            </Link>
          );
        })}
      </div>
      )}
    </div>
  );
}
