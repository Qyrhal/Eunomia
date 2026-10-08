# React Bits pass: synthesized plan

Arena: 6 candidate plans (2 each on Opus, Fable, Sonnet), cross-judged on Fable. Source plans live in the session scratchpad (`arena-reactbits/candidate-{1..6}/plan.md`).

## Synthesis record

- **Base: candidate 1** ("motion lives where memory changes hands"). Picked by me and, independently, by the cross-judge (scores: C1 30, C4 28, C2 27, C3 25, C5 24, C6 24). Reason: no weak criterion, the most complete test and accessibility safety (real text always in the DOM, scramble and roll are `aria-hidden` overlays, entrances start at opacity 0.001), zero dependencies, licence risk surfaced.
- **Convergence (shared by most or all candidates, shipped as is):** zero new packages; token decrypt on mint; change-only digit roll (never count up on load); one check-draw grammar for "it finished"; author-coloured arrival of new rows from the 60s poll (never on first load); blur-in only on the onboarding heading; View Transitions circular theme wipe; login dot grid that reacts to the pointer; no motion on palette, sidebar, tabs or keyboard navigation; no custom cursors, WebGL backgrounds, magnets on buttons or gradient text.
- **Grafts:**
  - C2: Figma-style scrub on the graph zoom readout; warm tooltip group; dashed-to-solid invitation join.
  - C3: author-colour wash on freshly arrived rows; graph hover reticle (corner brackets); a `data-input` pointer/keyboard flag on `<html>`.
  - C4: hold-to-delete as an accelerator (the normal click still confirms); a breathing "Thinking" line with a real elapsed timer instead of a shimmer (avoids the gradient-text ban).
  - C5: emphasise the selected node's edges; sparks fire on success, not on press; the answer settles into focus with a 2px blur.
  - C6: the logo's points resolve into one (played on the onboarding welcome, so it never delays navigation); check draw after merge, clone and invite succeed.
- **Rejected:** C1's hold-only vault delete (breaks keyboard parity), C1/C3/C6 shimmer on "Thinking" (gradient-text ban), C3 count-up once per session and sidebar plate slide, C5 magnet on dashboard cursors (seen all day), C4/C6 sparks on every copy, C6 navigation delay and `navigator.webdriver` branch, C4/C5 near-verbatim vendoring (licence).

## Licence rule (binding)

React Bits is MIT + Commons Clause; Eunomia is MIT. **Do not copy React Bits source.** Re-implement each idea from scratch. Each file in `frontend/src/components/bits/` starts with `// Inspired by React Bits <Name> (reactbits.dev). Original implementation for Eunomia.` No em dashes.

## Rules for every placement

Emil Kowalski's rules, already in `.impeccable/swarm-contract.md`: purpose first; frequency gating (100+/day gets nothing); `--ease-out` for enter, `--ease-in-out` for movement; UI motion under 300ms (rare first-run moments may run longer and say so); never animate keyboard-driven actions; hover only on fine pointers; transform, opacity, clip-path, stroke-dashoffset and blur ≤4px only; reduced motion renders the final state. Real text is always in the DOM at its final value from the first frame. Colours come from CSS variables (canvas reads them at fire time). Accent only on pressable or selected; author colours only for real authors.

## Phase 1: the bits library (one worker, before anything else)

`frontend/src/components/bits/`:

| File | API | Notes |
|---|---|---|
| `motion.ts` | `useReducedMotion()`, `useFinePointer()` (useSyncExternalStore over matchMedia, server snapshot = reduced/false), `cssVar(name)`, `useInputMode()` + `InputModeTracker` component that sets `data-input="pointer"|"keyboard"` on `<html>` | Tracker mounted once from `app/(app)/layout.tsx` and the auth/onboarding pages. |
| `bits.css` | All keyframes, `@starting-style` rules, `.bits-*` classes, reduced-motion blocks, in `@layer components` | Imported by the bits components that need it. |
| `SyncMark.tsx` | `<SyncMark status="idle"|"running"|"done"|"failed" size={14} />` | Running: 68% arc rotating 800ms linear. Done: check draws 200ms `--ease-out` in `--good`. Failed: cross in `--critical`. `aria-hidden`. Shared "it finished" grammar. |
| `CopyButton.tsx` | `<CopyButton value label? size? />` | One copy button for the whole app: Copy icon to drawn check (180ms), "Copied" for 1.5s, same accessible names as today ("Copy", "Copied"). Replaces the per-page copies. |
| `DigitRoll.tsx` | `<DigitRoll value={n} format? />` | Renders the real formatted number as text; on a later change only the changed digits roll (240ms, 30ms stagger right to left) in an `aria-hidden` overlay. Never on mount. |
| `DecryptReveal.tsx` | `<DecryptReveal text maxMs={480} />` | Real text in the DOM (transparent during the reveal), scramble overlay `aria-hidden`, left to right, mono, `--ink-faint` to `--ink-dim`. Mount only. |
| `HoldButton.tsx` | wraps a `.btn-danger` button: `holdMs`, `onConfirm` | Click still confirms. Pointer hold also confirms after `holdMs` with a clip-path fill (linear) that snaps back in 200ms on release. Keyboard: Enter/Space confirm instantly, no fill. |
| `Spark.ts` | `spark(el, { count, radius, color: "--accent" })` | DOM spans + WAAPI (no canvas), fired from success handlers only, pointer-initiated only. |
| `BlurWords.tsx` | `<BlurWords text as="h1" className />` | Word spans inside one element; accessible name unchanged; once on mount. |
| `Tooltip.tsx` | `<Tooltip label shortcut?>{trigger}</Tooltip>` + `TooltipGroup` | 400ms first delay, 300ms warm window (instant, no animation, after one opened), 140ms pop from `scale(0.96)` toward the trigger. Shows on focus-visible too. |

Shared-file changes in phase 1 (owner = this worker): `ThemeToggle.tsx` + `globals.css` theme wipe (View Transitions circle from the button, 280ms `--ease-out`, pointer only, instant swap for keyboard or no support) and the Sun/Moon blur crossfade; `.frame-selected` handle snap under `html[data-input="pointer"]` (handles from `scale(0.6)`, 120ms) in `bits.css`.

## Phase 2: placements (six workers in parallel, file-disjoint)

**W1 Dashboard** (`app/(app)/page.tsx`, `Sidebar.tsx` tooltips only)
1. Live memory arrival (signature): rows new since the previous poll (never first load) fade in 200ms with a 1.4s author-colour wash (`color-mix(in oklab, var(--author) 16%, transparent)`) and the author tag shows its cursor glyph for 2.4s, then fades it. Max 3 cursors per refresh.
2. Stat strip and record counts use `DigitRoll`.
3. McpCard token: `DecryptReveal` on the token only (never the `claude mcp add` command), `Spark` on successful generate.
4. "Sync now" buttons use `SyncMark` (done holds 1.2s, labels unchanged).
5. Copy fields use `CopyButton`.
6. Sidebar: `Tooltip` on the icon-only footer buttons (theme, log out).

**W2 Chat** (`app/(app)/chat/page.tsx`)
1. Tool rows use `SyncMark` (running to done draw; history renders done without animation); live rows enter with `@starting-style` 180ms.
2. "Thinking…" / "Using tool…" breathes (opacity 1 to 0.6, 1.6s `--ease-in-out`) with a real elapsed timer in mono; on the first token it settles into "Thought for 2.4s".
3. The streamed answer settles in from `blur(2px)` + opacity, 200ms, once per reply.
4. Transcript scroller gets a static top/bottom mask fade only when scrolled.
5. `Tooltip` on icon-only buttons (new chat, delete thread, stop, send).

**W3 Graph** (`components/Scene3D.tsx`, `components/EntityGraph.tsx`, `app/(app)/entities/page.tsx`, `app/(app)/code/page.tsx`)
1. Pointer pick ring: one 1px `--accent` ring at the picked node, `scale(1)` to `2.2` and fade, 260ms (pointer only).
2. Hover reticle: four corner brackets lock onto the hovered node's label box (enter 140ms from `scale(1.5)`; moves between nodes 120ms `--ease-in-out`); fine pointer only.
3. Selected node's incident edges brighten (200ms), others dim.
4. Zoom readout becomes a scrub control (drag left/right, Shift coarse, Alt fine, rubber band at limits, `aria-label="Zoom level, drag to change"`); +/- buttons stay for keyboard.
5. `Tooltip` on inspector icon buttons; check draw after "Add memory" succeeds; the new memory row blurs in.

**W4 Settings and Vaults** (`app/(app)/settings/page.tsx`, `app/(app)/vaults/page.tsx`)
1. Token mint (signature): `DecryptReveal` on the token, `Spark` on create success, `CopyButton`.
2. Check for updates uses `SyncMark`.
3. Delete vault confirm uses `HoldButton` (900ms) as an accelerator; click still works.
4. Merge: on success the two vault chips glide together (260ms `--ease-in-out`) and resolve into the new name (blur crossfade 180ms); `onMerged` is never delayed. Clone and invite success show a drawn check.
5. Invitation Join: dashed border turns solid with an `--accent-soft` flash (180ms), then the new vault row arrives (200ms). `Spark` on Join success.
6. Revoke token / session: `HoldButton` 650ms accelerator.

**W5 Auth and onboarding** (`app/login/*`, `app/register/*`, `app/onboarding/*`)
1. "The canvas writes itself" (signature): in the illustration, one cursor glides to the empty row and its text types in (28ms/char, caret in the author colour), the rows slide down a slot, the next author takes the empty row. Uses only the existing illustrative rows; pauses when hidden or offscreen.
2. Dot spotlight: a second dot layer revealed by a `mask-image` radial gradient at the pointer (120px), fine pointer only.
3. Onboarding welcome: the Eunomia mark's three points resolve into one (260ms, 50ms stagger) above the heading; heading uses `BlurWords`.
4. Progress bars fill with `scaleX` (260ms) instead of a colour flip.
5. `Spark` on finish success; navigation is never delayed.

**W6 Connectors, docs and skill** (`app/(app)/connectors/**`, `app/(app)/docs/page.tsx`, `app/(app)/skill/page.tsx`, `lib/connectorMeta.tsx`)
1. Sync now and Test connection use `SyncMark`; records count uses `DigitRoll`.
2. Every copy button in docs, skill and setup becomes `CopyButton` (removes the duplicated copy implementations).
3. Skill "Saved" gets the drawn check.
4. `Tooltip` on icon-only buttons.

## Verify

Per worker: `bunx tsc --noEmit`, `bun run lint`, the specs for its pages against the dev server on :3417, screenshots in both themes, and a reduced-motion check (final state renders instantly). Parent: full suite, finish review, then `DESIGN.md` from the shipped build.
