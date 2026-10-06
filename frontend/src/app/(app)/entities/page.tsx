"use client";

import EntityGraph from "@/components/EntityGraph";

export default function EntitiesPage() {
  return (
    <div className="flex flex-col gap-5 h-[calc(100vh-11rem)] md:h-[calc(100vh-7.5rem)]">
      <div>
        <h1 className="font-display text-xl" style={{ color: "var(--ink)" }}>
          Entities
        </h1>
        <p className="text-[12.5px] mt-1" style={{ color: "var(--ink-dim)" }}>
          People, organisations, and locations extracted from your synced sources, and how they relate.
        </p>
      </div>
      <EntityGraph />
    </div>
  );
}
