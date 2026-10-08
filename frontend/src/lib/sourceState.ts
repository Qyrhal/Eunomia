import type { SourceRow } from "@/lib/api";
import { CONNECTOR_META, kindForSource } from "@/lib/connectorMeta";

/** Failures in a row before a source counts as failing rather than retrying.
 * The scheduler's backoff reaches a full hour by index 2, so 3 means it needs attention. */
export const FAILURE_ALERT_THRESHOLD = 3;

/** A source is live when it is enabled or already holds records (seeded demo data has no connector but is real data). */
export const isLiveSource = (s: SourceRow) => s.connected || s.record_count > 0;

/** Backend reference stubs that exist for developers, not users. */
export const isStubSource = (s: SourceRow) => s.key === "example";

/** The name users know a source by: the connector label when one exists ("heypocket" reads "PocketAI"). */
export function sourceLabel(s: SourceRow): string {
  const kind = kindForSource(s.key);
  return kind ? CONNECTOR_META[kind].label : s.label;
}

export type SourceHealth = { label: "Healthy" | "Retrying" | "Failing" | "Never synced" | "Connected"; tone: string };

/** One source's sync health. No row means a connector with nothing to sync (tool calls only).
 * A connected source that has not synced yet reads "Connected", since there is no health to report. */
export function sourceHealth(s: SourceRow | undefined): SourceHealth {
  if (!s) return { label: "Connected", tone: "var(--good)" };
  const { consecutive_failures: fails, last_ok } = s.sync_status;
  if (fails >= FAILURE_ALERT_THRESHOLD) return { label: "Failing", tone: "var(--critical)" };
  if (fails > 0) return { label: "Retrying", tone: "var(--warning)" };
  if (last_ok) return { label: "Healthy", tone: "var(--good)" };
  return { label: s.connected ? "Connected" : "Never synced", tone: "var(--ink-faint)" };
}
