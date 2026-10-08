"use client";

import { useState } from "react";
import EntityGraph from "@/components/EntityGraph";
import VectorCloud from "@/components/VectorCloud";

const VIEWS = [
  { id: "graph", label: "Graph", blurb: "People, organisations and places, and how they relate." },
  { id: "cloud", label: "Vector cloud", blurb: "Every memory, projected from vector space to 3D." },
] as const;

export default function EntitiesPage() {
  const [view, setView] = useState<(typeof VIEWS)[number]["id"]>("graph");
  const current = VIEWS.find((v) => v.id === view)!;

  // Same header in both views, so switching tabs never moves it.
  const header = (
    <div className="flex flex-col gap-3 items-start">
      <div className="flex items-center gap-3 flex-wrap">
        <h1 className="page-title">Entities</h1>
        <div className="panel p-0.5 flex gap-0.5" role="tablist" aria-label="View">
          {VIEWS.map((v) => (
            <button key={v.id} role="tab" aria-selected={view === v.id} className="pill" onClick={() => setView(v.id)}>
              {v.label}
            </button>
          ))}
        </div>
      </div>
      <p className="text-[13px] -mt-1.5" style={{ color: "var(--ink-dim)" }}>
        {current.blurb}
      </p>
    </div>
  );

  return (
    <div className="relative -mx-4 -my-6 md:-mx-10 md:-my-8 h-[calc(100dvh-3rem)] md:h-dvh">
      {view === "graph" ? (
        <EntityGraph header={header} />
      ) : (
        <div className="absolute inset-0 p-4 md:p-5 flex flex-col gap-4 overflow-y-auto">
          {header}
          <VectorCloud />
        </div>
      )}
    </div>
  );
}
