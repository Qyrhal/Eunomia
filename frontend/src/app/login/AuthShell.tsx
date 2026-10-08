import Link from "next/link";
import { CircleAlert } from "lucide-react";
import EunomiaMark from "@/components/EunomiaMark";
import ThemeToggle from "@/components/ThemeToggle";
import AuthorTag from "@/components/AuthorTag";

/** Split layout shared by /login and /register: a compact form on the left,
 * and on wide screens an illustrative canvas where several agents converge
 * on one shared memory. The canvas is pure markup plus CSS (no images, no
 * JS), so it never delays the form. It is a demonstration of the mechanism,
 * not data: the agent names are clients Eunomia connects to over MCP. */
export default function AuthShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-screen w-full grid lg:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)]">
      <div className="flex flex-col px-6 py-6 sm:px-10">
        <header className="flex items-center justify-between">
          <Link href="/login" className="inline-flex items-center gap-2 text-[14px] font-semibold tracking-[-0.01em]">
            <EunomiaMark size={22} />
            Eunomia
          </Link>
          <ThemeToggle />
        </header>
        <main className="flex-1 flex items-start sm:items-center justify-center pt-16 pb-12 sm:py-12">
          <div className="w-full max-w-[340px]">{children}</div>
        </main>
        <p className="label">Self-hosted and MIT licensed.</p>
      </div>
      <AgentCanvas />
    </div>
  );
}

export function FormError({ message }: { message: string }) {
  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-[7px] px-3 py-2 text-[13px] leading-[1.45]"
      style={{ background: "var(--critical-soft)", color: "var(--critical)" }}
    >
      <CircleAlert size={14} strokeWidth={1.75} className="mt-[3px] shrink-0" aria-hidden />
      <span>{message}</span>
    </div>
  );
}

export const LABEL = "text-[12.5px] font-medium";

/** Show/hide for a password field. Named "Show"/"Hide" (not "...password")
 * so it never collides with the field's own accessible name. */
export function RevealToggle({ shown, onToggle, controls }: { shown: boolean; onToggle: () => void; controls: string }) {
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-pressed={shown}
      aria-controls={controls}
      className="btn btn-ghost btn-sm absolute right-[5px] top-[5px]"
    >
      {shown ? "Hide" : "Show"}
    </button>
  );
}

// Scene coordinates are fixed (600 x 460) so the SVG edges line up with the
// absolutely placed chips, the memory panel and the cursors.
const ROWS = [
  { author: "Claude Code", text: "Prefers Bun over npm for frontend work", age: "2m" },
  { author: "Codex", text: "Payments service owns the Stripe webhook", age: "4m" },
  { author: "Cursor", text: "Shared components live in packages/ui", age: "7m" },
];

const NODES = [
  { label: "packages/ui", kind: "file", x: 404, y: 40 },
  { label: "Platform team", kind: "organisation", x: 6, y: 372 },
  { label: "payments", kind: "repository", x: 452, y: 392 },
];

const CURSORS = [
  { name: "Claude Code", x: 36, y: 72, anim: "auth-drift-a", dur: "13s" },
  { name: "Codex", x: 508, y: 196, anim: "auth-drift-b", dur: "16s" },
  { name: "Cursor", x: 236, y: 352, anim: "auth-drift-c", dur: "11s" },
];

function AgentCanvas() {
  return (
    <aside className="canvas-grid relative hidden lg:flex flex-col justify-between overflow-hidden border-l p-12">
      <style>{`
        @keyframes auth-drift-a { 0% { transform: translate(0, 0); } 50% { transform: translate(28px, 16px); } 100% { transform: translate(12px, 34px); } }
        @keyframes auth-drift-b { 0% { transform: translate(0, 0); } 50% { transform: translate(-26px, 10px); } 100% { transform: translate(-12px, -18px); } }
        @keyframes auth-drift-c { 0% { transform: translate(0, 0); } 50% { transform: translate(14px, -18px); } 100% { transform: translate(-10px, -8px); } }
        @keyframes auth-breathe { 0%, 100% { opacity: 0.35; } 50% { opacity: 0.8; } }
        .auth-drift { animation-timing-function: var(--ease-in-out); animation-iteration-count: infinite; animation-direction: alternate; will-change: transform; }
        .auth-breathe { animation: auth-breathe 6s var(--ease-in-out) infinite; }
        @media (prefers-reduced-motion: reduce) { .auth-drift, .auth-breathe { animation: none !important; } }
      `}</style>

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

          {NODES.map((n) => (
            <span
              key={n.label}
              className="absolute inline-flex items-center gap-1.5 rounded-[6px] px-2 h-6 text-[11.5px]"
              style={{ left: n.x, top: n.y, background: "var(--surface)", boxShadow: "var(--shadow-panel)", color: "var(--ink-dim)" }}
            >
              <span className="dot" style={{ background: `var(--kind-${n.kind})` }} />
              <span className={n.kind === "organisation" ? "" : "font-mono"}>{n.label}</span>
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
            <div className="hairline-rows">
              {ROWS.map((r) => (
                <div key={r.author} className="flex items-center gap-3 px-3 h-10">
                  <span className="w-[84px] shrink-0">
                    <AuthorTag name={r.author} />
                  </span>
                  <span className="flex-1 truncate text-[12.5px]" style={{ color: "var(--ink)" }}>
                    {r.text}
                  </span>
                  <span className="w-8 text-right font-mono text-[11.5px]" style={{ color: "var(--ink-faint)" }}>
                    {r.age}
                  </span>
                </div>
              ))}
              <div className="flex items-center gap-3 px-3 h-10">
                <span className="w-[84px] shrink-0">
                  <AuthorTag name="Cursor" />
                </span>
                <span className="flex-1">
                  <span className="skeleton block h-2.5 w-3/5" />
                </span>
                <span className="w-8 text-right font-mono text-[11.5px]" style={{ color: "var(--ink-faint)" }}>
                  now
                </span>
              </div>
            </div>
          </div>

          {CURSORS.map((c) => (
            <span key={c.name} className="auth-drift absolute" style={{ left: c.x, top: c.y, animationName: c.anim, animationDuration: c.dur }}>
              <AuthorTag name={c.name} cursor />
            </span>
          ))}
        </div>
      </div>

      <p className="label">Illustration of agents writing to a shared vault.</p>
    </aside>
  );
}
