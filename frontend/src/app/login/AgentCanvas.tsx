"use client";

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import AuthorTag, { CursorGlyph, authorColor } from "@/components/AuthorTag";
import { cssVar, useFinePointer, useReducedMotion } from "@/components/bits/motion";

// Scene coordinates are fixed (600 x 460) so the SVG edges line up with the
// absolutely placed chips, the memory panel and the cursors.
const ROWS = [
  { author: "Claude Code", text: "Prefers Bun over npm for frontend work" },
  { author: "Codex", text: "Payments service owns the Stripe webhook" },
  { author: "Cursor", text: "Shared components live in packages/ui" },
];
// Age by slot: the top slot is the row being written.
const AGES = ["now", "2m", "4m", "7m"];
const ROW_H = 40;

const NODES = [
  { label: "packages/ui", kind: "file", x: 404, y: 40 },
  { label: "Platform team", kind: "organisation", x: 6, y: 372 },
  { label: "payments", kind: "repository", x: 452, y: 392 },
];

const CURSORS = [
  { name: "Claude Code", x: 36, y: 72, anim: "auth-drift-a", dur: "13s" },
  { name: "Codex", x: 516, y: 262, anim: "auth-drift-b", dur: "16s" },
  { name: "Cursor", x: 236, y: 352, anim: "auth-drift-c", dur: "11s" },
];
// Where the writing cursor's tip rests: on the panel edge beside the top row, so its tag never covers a row.
const WRITE_AT = { x: 504, y: 172 };

const CHAR_MS = 28;
const GLIDE_MS = 700;
const HOLD_MS = 1800;
const SLIDE_MS = 260;

/** The illustration on /login and /register. One agent's cursor glides to the
 * empty top row and types one of the existing illustrative rows, the rows
 * slide down a slot, then the next agent takes the empty row. It is a
 * demonstration of the mechanism, not data. Pauses while the tab is hidden or
 * the aside is offscreen (it is display:none below lg), and renders the
 * finished rows under reduced motion. */
export default function AgentCanvas() {
  const reduced = useReducedMotion();
  const fine = useFinePointer();
  const aside = useRef<HTMLElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [n, setN] = useState(0); // rows written so far; ROWS[n % 3] is being written
  const [typed, setTyped] = useState(0);
  const [onScreen, setOnScreen] = useState(false);
  const [tabVisible, setTabVisible] = useState(true);
  const running = !reduced && onScreen && tabVisible;

  useEffect(() => {
    const el = aside.current;
    if (!el) return;
    const io = new IntersectionObserver(([e]) => setOnScreen(e.isIntersecting));
    io.observe(el);
    const onVis = () => setTabVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", onVis);
    return () => {
      io.disconnect();
      document.removeEventListener("visibilitychange", onVis);
    };
  }, []);

  const active = ROWS[n % ROWS.length];
  const len = active.text.length;

  // One step per timeout, so pausing simply stops scheduling and resumes where it left off.
  useEffect(() => {
    if (!running) return;
    const t =
      typed < len
        ? setTimeout(() => setTyped(typed + 1), typed === 0 ? GLIDE_MS : CHAR_MS)
        : setTimeout(() => {
            setN(n + 1);
            setTyped(0);
          }, HOLD_MS);
    return () => clearTimeout(t);
  }, [running, typed, len, n]);

  // A new empty row arrived on top: start the list one slot up and slide it down.
  useLayoutEffect(() => {
    if (n === 0 || !list.current) return;
    list.current.animate([{ transform: `translateY(-${ROW_H}px)` }, { transform: "none" }], {
      duration: SLIDE_MS,
      easing: cssVar("--ease-in-out") || "ease-in-out",
    });
  }, [n]);

  // Dot spotlight: the pointer position lives on the mask layer only (no children to restyle).
  const spot = useRef<HTMLDivElement>(null);
  function track(e: React.PointerEvent) {
    const s = spot.current;
    if (!s || e.pointerType !== "mouse") return;
    const r = e.currentTarget.getBoundingClientRect();
    s.style.setProperty("--mx", `${e.clientX - r.left}px`);
    s.style.setProperty("--my", `${e.clientY - r.top}px`);
    s.dataset.on = "";
  }

  // Under reduced motion (and on the server) the top row shows finished text.
  const shown = reduced ? len : typed;
  const writing = !reduced && typed < len;

  return (
    <aside
      ref={aside}
      className="canvas-grid auth-canvas relative isolate hidden lg:flex flex-col justify-between overflow-hidden border-l p-12"
      onPointerMove={fine ? track : undefined}
      onPointerLeave={() => spot.current && delete spot.current.dataset.on}
    >
      <style>{`
        @keyframes auth-drift-a { 0% { transform: translate(0, 0); } 50% { transform: translate(28px, 16px); } 100% { transform: translate(12px, 34px); } }
        @keyframes auth-drift-b { 0% { transform: translate(0, 0); } 50% { transform: translate(-26px, 10px); } 100% { transform: translate(-12px, -18px); } }
        @keyframes auth-drift-c { 0% { transform: translate(0, 0); } 50% { transform: translate(14px, -18px); } 100% { transform: translate(-10px, -8px); } }
        @keyframes auth-breathe { 0%, 100% { opacity: 0.35; } 50% { opacity: 0.8; } }
        .auth-drift { animation-timing-function: var(--ease-in-out); animation-iteration-count: infinite; animation-direction: alternate; will-change: transform; }
        .auth-breathe { animation: auth-breathe 6s var(--ease-in-out) infinite; }
        .auth-glide { transition: transform ${GLIDE_MS}ms var(--ease-in-out); }
        .auth-spot {
          background-image: radial-gradient(circle at 1px 1px, var(--ink-faint) 1.1px, transparent 0);
          background-size: 20px 20px;
          -webkit-mask-image: radial-gradient(120px circle at var(--mx, -200px) var(--my, -200px), var(--ink), transparent);
          mask-image: radial-gradient(120px circle at var(--mx, -200px) var(--my, -200px), var(--ink), transparent);
          opacity: 0;
          transition: opacity 200ms var(--ease-out);
        }
        .auth-spot[data-on] { opacity: 0.7; }
        @media (prefers-reduced-motion: reduce) { .auth-drift, .auth-breathe { animation: none !important; } .auth-glide { transition: none; } }
      `}</style>
      {/* The second dot layer, revealed only around a fine pointer. */}
      {fine && <div ref={spot} className="auth-spot pointer-events-none absolute inset-0 -z-10" aria-hidden />}

      <div className="max-w-[440px]">
        <p className="font-display text-[26px] leading-[1.2] text-balance">Every agent on your team, one shared memory.</p>
        <p className="mt-3 text-[14px] leading-[1.55]" style={{ color: "var(--ink-dim)" }}>
          Your agents read and write the same memory over MCP, and every memory shows who wrote it and when.
        </p>
      </div>

      <div className="flex justify-center" aria-hidden>
        <div className="relative h-[460px] w-[600px] shrink-0 scale-[0.72] xl:scale-90 2xl:scale-100">
          <svg className="absolute inset-0" width="600" height="460" viewBox="0 0 600 460" fill="none">
            <g stroke="var(--border-strong)" strokeWidth="1" strokeDasharray="3 4">
              <line x1="452" y1="64" x2="430" y2="130" />
              <line x1="70" y1="372" x2="150" y2="326" />
              <line x1="496" y1="392" x2="470" y2="326" />
            </g>
          </svg>

          {NODES.map((nd) => (
            <span
              key={nd.label}
              className="absolute inline-flex items-center gap-1.5 rounded-[6px] px-2 h-6 text-[11.5px]"
              style={{ left: nd.x, top: nd.y, background: "var(--surface)", boxShadow: "var(--shadow-panel)", color: "var(--ink-dim)" }}
            >
              <span className="dot" style={{ background: `var(--kind-${nd.kind})` }} />
              <span className={nd.kind === "organisation" ? "" : "font-mono"}>{nd.label}</span>
            </span>
          ))}

          <div className="panel absolute left-[110px] top-[130px] w-[390px] overflow-hidden">
            <div className="flex items-center justify-between px-3 h-9 border-b">
              <span className="section-title">Team memory</span>
              <span className="inline-flex items-center gap-1.5 label">
                <span className="dot auth-breathe" style={{ background: "var(--good)" }} />
                Live
              </span>
            </div>
            {/* Three slots visible; a fourth (the oldest) waits below the clip so the slide has somewhere to go. */}
            <div className="overflow-hidden" style={{ height: ROW_H * 3 }}>
              <div ref={list} className="hairline-rows">
                {AGES.map((age, slot) => {
                  const r = ROWS[(((n - slot) % ROWS.length) + ROWS.length) % ROWS.length];
                  const top = slot === 0;
                  return (
                    <div key={n - slot} className="flex items-center gap-3 px-3" style={{ height: ROW_H }}>
                      <span className="w-[84px] shrink-0">
                        <AuthorTag name={r.author} />
                      </span>
                      <span className="flex-1 min-w-0 truncate text-[12.5px]" style={{ color: "var(--ink)" }}>
                        {top && shown === 0 ? (
                          <span className="skeleton block h-2.5 w-3/5" />
                        ) : (
                          <>
                            {top ? r.text.slice(0, shown) : r.text}
                            {top && writing && (
                              <span
                                className="inline-block align-[-2px] ml-px h-[13px] w-[1.5px]"
                                style={{ background: authorColor(r.author) }}
                              />
                            )}
                          </>
                        )}
                      </span>
                      <span className="w-8 text-right font-mono text-[11.5px]" style={{ color: "var(--ink-faint)" }}>
                        {age}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>

          {CURSORS.map((c) => {
            const isWriter = !reduced && c.name === active.author;
            return (
              <span
                key={c.name}
                className="auth-glide absolute"
                style={{ left: c.x, top: c.y, transform: isWriter ? `translate(${WRITE_AT.x - c.x}px, ${WRITE_AT.y - c.y}px)` : undefined }}
              >
                <span
                  className="auth-drift inline-flex items-start gap-0.5"
                  style={{ animationName: c.anim, animationDuration: c.dur, animationPlayState: isWriter ? "paused" : undefined }}
                >
                  <CursorGlyph color={authorColor(c.name)} size={20} />
                  <span className="mt-4">
                    <AuthorTag name={c.name} />
                  </span>
                </span>
              </span>
            );
          })}
        </div>
      </div>

      <p className="label">Illustration of agents writing to a shared vault.</p>
    </aside>
  );
}
