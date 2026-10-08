"use client";

import { useRef, useSyncExternalStore } from "react";
import { flushSync } from "react-dom";
import { Moon, Sun } from "lucide-react";
import { cssVar, prefersReducedMotion } from "./bits/motion";
import "./bits/bits.css";

type Theme = "dark" | "light";

const listeners = new Set<() => void>();
function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => listeners.delete(cb);
}
function current(): Theme {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

type ViewTransitionDoc = Document & {
  startViewTransition?: (update: () => void) => { ready: Promise<void>; finished: Promise<void>; updateCallbackDone: Promise<void> };
};

export default function ThemeToggle() {
  const theme = useSyncExternalStore(subscribe, current, () => "dark" as Theme);
  const button = useRef<HTMLButtonElement>(null);

  function toggle(e: React.MouseEvent) {
    const next: Theme = theme === "dark" ? "light" : "dark";
    const root = document.documentElement;
    let applied = false;
    const apply = () => {
      if (applied) return;
      applied = true;
      if (next === "light") root.dataset.theme = "light";
      else delete root.dataset.theme;
      try {
        localStorage.setItem("eunomia-theme", next);
      } catch {}
      flushSync(() => listeners.forEach((l) => l()));
    };

    // The wipe is pointer feedback only: keyboard (detail 0), reduced motion or no support swap instantly.
    const doc = document as ViewTransitionDoc;
    if (e.detail === 0 || !doc.startViewTransition || prefersReducedMotion() || !button.current) return apply();
    try {
      const r = button.current.getBoundingClientRect();
      const x = r.left + r.width / 2;
      const y = r.top + r.height / 2;
      const radius = Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y));
      root.dataset.themeWipe = "";
      const vt = doc.startViewTransition(apply);
      vt.ready
        .then(() =>
          root.animate(
            { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${radius}px at ${x}px ${y}px)`] },
            { duration: 280, easing: cssVar("--ease-out") || "ease-out", pseudoElement: "::view-transition-new(root)" },
          ),
        )
        .catch(() => {});
      vt.updateCallbackDone.catch(apply);
      vt.finished.catch(() => {}).finally(() => delete root.dataset.themeWipe);
    } catch {
      delete root.dataset.themeWipe;
      apply();
    }
  }

  const label = theme === "dark" ? "Switch to light theme" : "Switch to dark theme";
  return (
    <button ref={button} onClick={toggle} aria-label={label} title={label} className="btn btn-ghost btn-icon btn-sm" style={{ width: 26 }}>
      <span className="bits-theme-icon" aria-hidden>
        <Sun size={14} data-on={theme === "dark" ? "" : undefined} />
        <Moon size={14} data-on={theme === "light" ? "" : undefined} />
      </span>
    </button>
  );
}
