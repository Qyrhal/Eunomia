// Inspired by React Bits DecryptedText (reactbits.dev). Original implementation for Eunomia.
// Usage: <DecryptReveal key={token} text={token} /> inside a font-mono parent, for single-line secrets that appear after a click; key it to replay.
// The real text is in the DOM from the first frame (transparent while revealing); the scramble is an aria-hidden overlay.
"use client";

import { useEffect, useRef, useState } from "react";
import { useReducedMotion } from "./motion";
import "./bits.css";

const NOISE = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

// Deterministic noise for the very first frame, before the rAF loop takes over.
const firstNoise = (text: string) =>
  [...text].map((c, i) => (c === " " ? " " : NOISE[(c.charCodeAt(0) * 7 + i * 13) % NOISE.length])).join("");

export default function DecryptReveal({ text, maxMs = 480 }: { text: string; maxMs?: number }) {
  const reduced = useReducedMotion();
  // Decided once at mount. During hydration the server snapshot (reduced) wins, so SSR text never scrambles.
  const [active, setActive] = useState(() => !reduced && text.length > 0);
  const done = useRef<HTMLSpanElement>(null);
  const noise = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!active) return;
    const start = performance.now();
    let raf = 0;
    let frame = 0;
    const tick = (now: number) => {
      const t = Math.min(1, (now - start) / maxMs);
      const settled = Math.floor(t * text.length);
      // Refresh the noise every other frame so it reads as flicker, not static.
      if (frame++ % 2 === 0 && done.current && noise.current) {
        done.current.textContent = text.slice(0, settled);
        let s = "";
        for (let i = settled; i < text.length; i++) s += text[i] === " " ? " " : NOISE[(Math.random() * NOISE.length) | 0];
        noise.current.textContent = s;
      }
      if (t < 1) raf = requestAnimationFrame(tick);
      else setActive(false);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [active, text, maxMs]);

  return (
    <span className="bits-decrypt" data-revealing={active ? "" : undefined}>
      <span className="bits-decrypt-text">{text}</span>
      {active && (
        <span className="bits-decrypt-overlay" aria-hidden>
          <span ref={done} />
          <span ref={noise} className="bits-decrypt-noise">
            {firstNoise(text)}
          </span>
        </span>
      )}
    </span>
  );
}
