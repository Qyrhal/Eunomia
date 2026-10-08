"use client";

import EntityGraph from "@/components/EntityGraph";

export default function CodePage() {
  return (
    <div className="relative -mx-4 -my-6 md:-mx-10 md:-my-8 h-[calc(100dvh-3rem)] md:h-dvh">
      <EntityGraph
        kinds={["repository", "file", "symbol"]}
        header={
          <div>
            <h1 className="page-title">Code</h1>
            <p className="text-[13px] mt-1" style={{ color: "var(--ink-dim)" }}>
              Repositories, files and symbols your agents have mapped.
            </p>
          </div>
        }
      />
    </div>
  );
}
