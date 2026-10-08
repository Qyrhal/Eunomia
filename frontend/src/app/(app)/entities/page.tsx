"use client";

import { useState } from "react";
import EntityGraph from "@/components/EntityGraph";
import VectorCloud from "@/components/VectorCloud";

const VIEWS = [
  { id: "graph", label: "Graph" },
  { id: "cloud", label: "Vector cloud" },
] as const;

export default function EntitiesPage() {
  const [view, setView] = useState<(typeof VIEWS)[number]["id"]>("graph");
  return (
    <div className="flex flex-col gap-5 h-[calc(100vh-11rem)] md:h-[calc(100vh-7.5rem)]">
      <div className="flex items-end justify-between gap-3 flex-wrap">
        <div>
          <h1 className="font-display text-xl" style={{ color: "var(--ink)" }}>
            Entities
          </h1>
          <p className="text-[12.5px] mt-1" style={{ color: "var(--ink-dim)" }}>
            {view === "graph"
              ? "People, organisations and locations, and how they relate."
              : "The shape of everything stored, projected from vector space to 3D."}
          </p>
        </div>
        <div className="flex gap-1" role="tablist">
          {VIEWS.map((v) => (
            <button key={v.id} role="tab" aria-selected={view === v.id} className="pill" onClick={() => setView(v.id)}>
              {v.label}
            </button>
          ))}
        </div>
      </div>
      {view === "graph" ? <EntityGraph /> : <VectorCloud />}
    </div>
  );
}
