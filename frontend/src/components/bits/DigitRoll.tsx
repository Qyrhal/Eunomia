// Inspired by React Bits Counter (reactbits.dev). Original implementation for Eunomia.
// Usage: <DigitRoll value={totalRecords} /> or <DigitRoll value={n} format={(v) => `${v}%`} />; put font-mono/tabular on the parent.
// The real number is always the text; only a later change rolls the changed digits in an aria-hidden overlay. Never on mount.
"use client";

import { useEffect, useState } from "react";
import { useReducedMotion } from "./motion";
import "./bits.css";

const STAGGER = 30;
const DURATION = 240;
const defaultFormat = (n: number) => n.toLocaleString();

type Roll = { from: string; to: string; up: boolean; id: number };

export default function DigitRoll({ value, format = defaultFormat }: { value: number; format?: (n: number) => string }) {
  const reduced = useReducedMotion();
  const text = format(value);
  const [shown, setShown] = useState({ text, value });
  const [roll, setRoll] = useState<Roll | null>(null);
  if (text !== shown.text) {
    setShown({ text, value });
    setRoll(reduced ? null : { from: shown.text, to: text, up: value >= shown.value, id: (roll?.id ?? 0) + 1 });
  }

  // Count the changed characters so the overlay leaves when the last one lands.
  let changed = 0;
  const cells = roll
    ? [...roll.to].map((ch, i) => {
        const old = roll.from[roll.from.length - roll.to.length + i] ?? " ";
        if (old === ch) return { ch, old: null, i: 0 };
        return { ch, old, i: -1 };
      })
    : [];
  for (let k = cells.length - 1; k >= 0; k--) if (cells[k].old !== null) cells[k].i = changed++;

  useEffect(() => {
    if (!roll) return;
    const t = setTimeout(() => setRoll(null), DURATION + STAGGER * Math.max(0, changed - 1) + 20);
    return () => clearTimeout(t);
  }, [roll, changed]);

  return (
    <span className="bits-roll" data-rolling={roll ? "" : undefined}>
      <span className="bits-roll-text">{text}</span>
      {roll && (
        <span key={roll.id} className="bits-roll-overlay" aria-hidden>
          {cells.map((c, k) =>
            c.old === null ? (
              <span key={k} className="bits-roll-col">
                {c.ch}
              </span>
            ) : (
              <span key={k} className="bits-roll-col">
                <span className="bits-roll-stack" data-dir={roll.up ? "up" : "down"} style={{ ["--i" as string]: c.i }}>
                  <span>{roll.up ? c.old : c.ch}</span>
                  <span>{roll.up ? c.ch : c.old}</span>
                </span>
              </span>
            ),
          )}
        </span>
      )}
    </span>
  );
}
