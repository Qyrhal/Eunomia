# Swarm contract: Multiplayer Canvas

Every worker in the design pass reads this file, `PRODUCT.md`, and the direction contract at the top of `frontend/src/app/layout.tsx` before touching code. The approved reference image is `.impeccable/mocks/decision/assigned.png` (Codex-generated; it shows a blue accent, but ours is felt green). Match its craft, density and grammar, not its exact layout.

## The world in one paragraph

Eunomia is a multiplayer canvas. A team's agents and people all write to one memory, and the UI always shows who wrote what, when, and how fresh it is. Think Figma and tldraw, run by a San Francisco dev-tool team: near-black ground (white in light theme) with a faint dot grid, floating hairline panels, small radii, dense tabular data, Geist type, one green accent.

## Hard rules

1. **Tokens only.** Use the CSS variables and classes in `frontend/src/app/globals.css`. No raw hex values, no new colours. If you truly need a new token, list it in your report; do not add it.
2. **Do not edit shared files:** `globals.css`, `app/layout.tsx`, `app/(app)/layout.tsx`, `Sidebar.tsx`, `CommandPalette.tsx`, `EunomiaMark.tsx`, `AuthorTag.tsx`, `ThemeToggle.tsx`, `AuthGuard.tsx`, `lib/api.ts`. Read them; import from them.
3. **Accent = actionable or selected only.** `--accent` / `.btn-primary` / `.pill[aria-pressed]` / `.frame-selected` / `--accent-text` for links. Never decoration, never section headings, never icons at rest.
4. **Author colours mark authorship only.** Use `<AuthorTag name=... />` from `components/AuthorTag.tsx` with a real author: `owner_email`, a source key, or an agent/client name the API actually returns. Never invent an author or an agent name. If the data has no author, show none.
5. **Both themes.** Every screen must look right in dark (default) and light (`data-theme="light"` on `<html>`, toggle in the sidebar footer). Never hard-code `rgba(0,0,0,...)` or white; use `--scrim`, `--shadow-panel`, `--shadow-pop`.
6. **Keep behaviour and tests.** Keep every route, feature, API call, accessible name, label text, heading text, `.ledger` class usage that tests select on, and `data-*` hook. Playwright specs in `frontend/tests/` must keep passing. Grep the specs for your pages before you rename anything.
7. **No backend changes.** If the UI wants data the API does not return, design with what exists and note the gap in your report.
8. **Next.js 16.** Read `frontend/node_modules/next/dist/docs/` before using any framework API you are unsure of.
9. **No em dashes** in any copy, comment or file you write. Use commas, colons, full stops or parentheses.

## Vocabulary (use these, do not invent parallels)

| Need | Use |
|---|---|
| Page title | `<h1 className="page-title">` top left. One per page. |
| Section heading | `<h2 className="section-title">`. Never a small label above a heading (kickers are banned). |
| Field / column / stat label | `.label` |
| Container | `.ledger` (or `.surface`) for in-flow blocks. `.panel` for floating things (inspector, popover, palette). Never nest cards in cards. |
| Selected item | `.frame-selected` (green outline + 4 square handles), Figma style. |
| Buttons | `.btn`, `.btn-primary` (one per view, the main action), `.btn-ghost`, `.btn-danger`, sizes `.btn-sm`, `.btn-icon`. |
| Inputs | `.field` on inputs/textareas/selects. Height 32px for single-line. |
| Filters / toggles | `button.pill` with `aria-pressed`. |
| Shortcuts | `.kbd` |
| Tables | `<table className="data-table">` with tabular numerals. Prefer dense tables and hairline rows over card grids for lists of things. |
| Status | `.dot` with `--good` / `--warning` / `--critical`. |
| Loading | `.skeleton` blocks shaped like the content, never a bare "Loading…" line for a main region. |
| Empty | One sentence saying what is missing plus the one action that fixes it. |
| Errors | Name the problem and the recovery, `--critical` text, `--critical-soft` fill. |
| Numbers, ids, code, timestamps | `.font-mono` (tabular). Not for prose or labels. |
| Icons | `lucide-react`, size 14-16, `strokeWidth` 1.75. No emoji or unicode as icons. |

Scale: body 13-14px, labels 12px, page title 22px. Radii: cards 10px, fields and buttons 7px, tags 4px. Spacing: tight groups (6-8px), clear separation between sections (32-40px), more space above a heading than below.

## Motion (Emil Kowalski rules, already wired into the classes)

- Buttons and pills press to `scale(0.97)` (built in). Every new pressable thing gets the same.
- Popovers, menus, inspectors: `.pop-in` (opacity + `scale(0.96)` via `@starting-style`), with `transform-origin` set toward the trigger. Modals stay centred.
- Durations under 250ms. Use `--ease-out` for enter, `--ease-in-out` for moving on screen. Never `ease-in`, never `transition: all`, never `scale(0)`.
- No animation on keyboard-driven or very frequent actions (palette, typing, list keyboard nav, tab switches).
- Hover effects only inside `@media (hover: hover) and (pointer: fine)`.
- Live data updates in place: values change, layout never jumps (fixed columns, tabular numerals, reserved space).
- Respect `prefers-reduced-motion` (global rule already strips movement).
- One authored moment per surface at most. No scattered entrance animations.

## Craft floor (impeccable)

Body text ≥4.5:1 contrast in both themes. No gradient text, no decorative glass, no coloured side-stripe borders thicker than 1px, no same-size icon+heading+text card grids as page structure, no hero-metric template, no section numbers, no monospace as costume. Theme the details: selection, focus rings, scrollbars, placeholders (already global). Hover, focus, disabled, loading, empty and error states for everything you touch.

## Verify before reporting

From your worktree's `frontend/`:

1. `bun install` (once), then `bunx tsc --noEmit`, `bun run lint`.
2. Start `BACKEND_INTERNAL_URL=http://localhost:8101 bun run dev -p <your port>` in the background. The backend on :8101 is an isolated test instance shared by all workers (in-memory DB, already seeded with demo data and a few memories).
3. Sign in inside the browser page with a JS `fetch` POST to `/api/auth/login` using `design@example.test` and the default password from `frontend/tests/helpers.ts`. It is a local test account; never type credentials anywhere else.
4. Screenshot every page you own at 1440x900 and 375x812, in dark and light. Save to `.impeccable/review/<slice>/` in your worktree. Open each file and confirm it shows what its name says.
5. Run the Playwright specs that cover your pages: `E2E_BASE_URL=http://localhost:<your port> bunx playwright test tests/<spec>.ts`. Fix failures you caused.
6. `node /Users/avi/.claude/skills/impeccable/scripts/detect.mjs --json <your changed files>` once; fix mechanical findings.
7. Commit on your branch with a plain message (no em dashes).

## Report

`PASS`, `ISSUES` or `BLOCKED`, then: files changed, screenshot paths, spec results, detector result, token requests, data gaps, anything another slice must know.
