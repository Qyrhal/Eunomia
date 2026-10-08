// Inspired by React Bits ClickSpark (reactbits.dev). Original implementation for Eunomia.
// Usage: in a success handler that a pointer click started: if (pointer) spark(buttonRef.current, { count: 8 });
// Throws short lines out from the element's centre with WAAPI, then removes them. Does nothing under reduced motion.
import { cssVar, prefersReducedMotion } from "./motion";
import "./bits.css";

type Options = { count?: number; radius?: number; color?: string };

export function spark(el: Element | null | undefined, { count = 8, radius = 18, color = "--accent" }: Options = {}) {
  if (!el || typeof window === "undefined" || prefersReducedMotion()) return;
  const r = el.getBoundingClientRect();
  if (r.width === 0 && r.height === 0) return;
  const x = r.left + r.width / 2;
  const y = r.top + r.height / 2;
  // Colour is a token name, resolved now so a theme swap is honoured.
  const fill = color.startsWith("--") ? cssVar(color) : color;
  const easing = cssVar("--ease-out") || "ease-out";

  for (let i = 0; i < count; i++) {
    const angle = (360 / count) * i;
    // Start on the element's edge along this angle, so wide buttons throw from their outline.
    const rad = (angle * Math.PI) / 180;
    const start = Math.min(r.width / 2 / Math.abs(Math.sin(rad) || 1e-6), r.height / 2 / Math.abs(Math.cos(rad) || 1e-6)) + 2;
    const s = document.createElement("span");
    s.className = "bits-spark";
    s.setAttribute("aria-hidden", "true");
    s.style.left = `${x}px`;
    s.style.top = `${y}px`;
    s.style.background = fill;
    document.body.appendChild(s);
    s.animate(
      [
        { transform: `rotate(${angle}deg) translateY(${-start}px) scaleY(1)`, opacity: 1 },
        { transform: `rotate(${angle}deg) translateY(${-(start + radius)}px) scaleY(0.2)`, opacity: 0 },
      ],
      { duration: 420, easing, fill: "forwards" },
    ).finished.finally(() => s.remove());
  }
}

export default spark;
