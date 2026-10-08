"use client";

import { useState } from "react";
import { Trash2, X } from "lucide-react";
import CopyButton from "@/components/bits/CopyButton";
import DecryptReveal from "@/components/bits/DecryptReveal";
import Tooltip from "@/components/bits/Tooltip";
import HoldButton from "@/components/bits/HoldButton";

export const ICON = { size: 14, strokeWidth: 1.75 } as const;

/* reveal: a freshly minted secret decrypts in once (the token, never a command). */
export function CopyField({ value, reveal = false }: { value: string; reveal?: boolean }) {
  return (
    <div className="field flex items-center gap-2 h-8 pl-3 pr-1">
      <code className="flex-1 min-w-0 truncate text-[12px] font-mono" style={{ color: "var(--ink)" }}>
        {reveal ? <DecryptReveal key={value} text={value} /> : value}
      </code>
      <CopyButton value={value} size="sm" className="btn-ghost shrink-0" />
    </div>
  );
}

export function relativeTime(iso: string | null): string {
  if (!iso) return "never";
  const ms = Date.now() - new Date(iso).getTime();
  const mins = Math.round(ms / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  return `${days}d ago`;
}

/** "in 12d" / "Expired": when a token stops working. */
export function expiryLabel(iso: string | null): { text: string; expired: boolean } {
  if (!iso) return { text: "Never", expired: false };
  const ms = new Date(iso).getTime() - Date.now();
  if (ms <= 0) return { text: "Expired", expired: true };
  const days = Math.ceil(ms / 86_400_000);
  return { text: days >= 2 ? `in ${days}d` : "in <1d", expired: false };
}

/* Section header inside a tab panel: title, one line of purpose, optional action. */
export function PanelHead({ title, children, action }: { title: string; children?: React.ReactNode; action?: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0 flex-1">
        <h2 className="section-title">{title}</h2>
        {children && (
          <p className="text-[13px] mt-1 max-w-[62ch]" style={{ color: "var(--ink-dim)" }}>
            {children}
          </p>
        )}
      </div>
      {action}
    </div>
  );
}

/* Inline revoke: one click arms it, the second confirms. Holding the icon for 650ms is the accelerator. */
export function RevokeButton({ label, onRevoke }: { label: string; onRevoke: () => Promise<void> }) {
  const [armed, setArmed] = useState(false);
  const [busy, setBusy] = useState(false);
  async function confirm() {
    setBusy(true);
    try {
      await onRevoke();
    } finally {
      setBusy(false);
      setArmed(false);
    }
  }
  if (!armed)
    return (
      <Tooltip label={`Revoke ${label}. Hold to revoke now`}>
      <HoldButton
        holdMs={650}
        onClick={() => setArmed(true)}
        onConfirm={confirm}
        disabled={busy}
        aria-label="Revoke"
        className="btn-sm btn-icon"
        // Quiet at rest like the ghost icon it replaces; the critical fill shows while held.
        style={{ borderColor: "transparent", color: "var(--ink-dim)" }}
      >
        <Trash2 {...ICON} />
      </HoldButton>
      </Tooltip>
    );
  return (
    <span className="inline-flex items-center gap-1">
      <button
        onClick={confirm}
        disabled={busy}
        className="btn btn-danger btn-sm"
      >
        {busy ? "Revoking…" : "Revoke"}
      </button>
      <button onClick={() => setArmed(false)} aria-label="Keep" className="btn btn-ghost btn-sm btn-icon">
        <X {...ICON} />
      </button>
    </span>
  );
}

export function TableSkeleton({ cols }: { cols: number }) {
  return (
    <>
      {[0, 1].map((i) => (
        <tr key={i} aria-hidden>
          {Array.from({ length: cols }, (_, c) => (
            <td key={c}>
              <span className="skeleton block h-4" style={{ width: c === 0 ? "60%" : "50%" }} />
            </td>
          ))}
        </tr>
      ))}
    </>
  );
}
