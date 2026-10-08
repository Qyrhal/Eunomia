import { ApiError } from "@/lib/api";
import CopyButton from "@/components/bits/CopyButton";

export type Failure = { message: string; code?: string; traceId?: string };

/* Turns a caught value into a Failure, keeping the server message when present. */
export function failure(e: unknown, fallback: string, suffix = ""): Failure {
  const message = (e instanceof Error && e.message ? e.message : fallback) + suffix;
  return e instanceof ApiError ? { message, code: e.code, traceId: e.traceId } : { message };
}

export default function ErrorLine({ error, children }: { error?: Failure | null; children?: React.ReactNode }) {
  return (
    <div
      role="alert"
      className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2 rounded-[7px]"
      style={{ background: "var(--critical-soft)" }}
    >
      <p className="flex-1 min-w-[200px] text-[12.5px]" style={{ color: "var(--critical)" }}>
        {children ?? error?.message}
      </p>
      {error?.code && !error.code.startsWith("http.") && (
        <code className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>{error.code}</code>
      )}
      {error?.traceId && (
        <span className="flex items-center gap-1">
          <span className="label">Trace</span>
          <code className="font-mono text-[12px]" style={{ color: "var(--ink-dim)" }}>{error.traceId}</code>
          <CopyButton value={error.traceId} label="Copy trace id" />
        </span>
      )}
    </div>
  );
}
