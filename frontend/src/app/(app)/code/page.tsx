"use client";

import Link from "next/link";
import EntityGraph from "@/components/EntityGraph";

export default function CodePage() {
  return (
    <div className="relative -mx-4 -my-6 md:-mx-10 md:-my-8 h-[calc(100dvh-3rem)] md:h-dvh">
      <EntityGraph
        kinds={["repository", "file", "symbol"]}
        guide={
          <>
            <p>
              Agents map code as they work, with <code className="font-mono text-[12px]">code_entity_upsert</code> and{" "}
              <code className="font-mono text-[12px]">code_relate</code>.
            </p>
            <Link href="/skill" className="btn btn-sm">
              Open the Memory skill
            </Link>
          </>
        }
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
