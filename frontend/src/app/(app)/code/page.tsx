"use client";

import EntityGraph from "@/components/EntityGraph";

export default function CodePage() {
  return (
    <div className="flex flex-col gap-5 h-[calc(100vh-11rem)] md:h-[calc(100vh-7.5rem)]">
      <div>
        <h1 className="font-display text-xl" style={{ color: "var(--ink)" }}>
          Code
        </h1>
        <p className="text-[12.5px] mt-1" style={{ color: "var(--ink-dim)" }}>
          Repositories, files, and symbols an agent has mapped while working in your codebases.
        </p>
      </div>
      <EntityGraph kinds={["repository", "file", "symbol"]} />
    </div>
  );
}
