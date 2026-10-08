---
name: Eunomia
description: Shared long-term memory for your team's AI agents, drawn as a multiplayer canvas.
colors:
  canvas: "#0b0c0e"
  surface: "#121316"
  surface-raised: "#1a1b1f"
  surface-hover: "#212328"
  ink: "#ededef"
  ink-dim: "#a3a7ae"
  ink-faint: "#80858e"
  border: "rgba(237, 237, 239, 0.08)"
  border-strong: "rgba(237, 237, 239, 0.15)"
  grid-dot: "rgba(237, 237, 239, 0.07)"
  felt-green: "#3fa873"
  felt-green-text: "#52bd87"
  felt-green-soft: "rgba(63, 168, 115, 0.14)"
  on-accent: "#04140b"
  good: "#3fa873"
  warning: "#d0a651"
  critical: "#d4705f"
  critical-soft: "rgba(212, 112, 95, 0.12)"
  scrim: "rgba(4, 5, 6, 0.6)"
  author-1: "#f0815e"
  author-2: "#5fa6f2"
  author-3: "#b48cf0"
  author-4: "#e2b04f"
  author-5: "#e77fa9"
  author-6: "#4fc1c7"
  on-author: "#0b0c0e"
  kind-person: "#6f9bc9"
  kind-organisation: "#a692c9"
  kind-location: "#c98a5a"
  kind-repository: "#5a9e8f"
  kind-file: "#9e9150"
  kind-symbol: "#9e5a7a"
  canvas-light: "#f5f6f7"
  surface-light: "#ffffff"
  surface-raised-light: "#f0f1f3"
  surface-hover-light: "#e8e9ec"
  ink-light: "#111214"
  ink-dim-light: "#4f545c"
  ink-faint-light: "#676c75"
  border-light: "rgba(17, 18, 20, 0.1)"
  border-strong-light: "rgba(17, 18, 20, 0.18)"
  felt-green-light: "#2b7d53"
  felt-green-text-light: "#23744b"
  felt-green-soft-light: "rgba(43, 125, 83, 0.1)"
  on-accent-light: "#ffffff"
  warning-light: "#9a6b12"
  critical-light: "#b4432f"
  critical-soft-light: "rgba(180, 67, 47, 0.08)"
  scrim-light: "rgba(17, 18, 20, 0.28)"
  author-1-light: "#e0623c"
  author-2-light: "#2f7fd8"
  author-3-light: "#8a5ad8"
  author-4-light: "#c48a14"
  author-5-light: "#cf4f84"
  author-6-light: "#1f9aa1"
  on-author-light: "#ffffff"
typography:
  display:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "22px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.025em"
  headline:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 600
    letterSpacing: "-0.015em"
  title:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 600
    letterSpacing: "-0.005em"
  body:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    fontFeature: "\"ss01\", \"cv11\""
  body-root:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    fontFeature: "\"ss01\", \"cv11\""
  label:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.3
  stat:
    fontFamily: "Geist Mono, ui-monospace, monospace"
    fontSize: "26px"
    lineHeight: 1
    letterSpacing: "-0.02em"
    fontFeature: "\"tnum\""
  mono:
    fontFamily: "Geist Mono, ui-monospace, monospace"
    fontSize: "12.5px"
    fontWeight: 400
    fontFeature: "\"tnum\""
  tag:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "11.5px"
    fontWeight: 500
    lineHeight: 1
rounded:
  tag: "4px"
  pill: "6px"
  control: "7px"
  field: "7px"
  code: "8px"
  card: "10px"
  full: "999px"
spacing:
  hair: "1px"
  "1": "4px"
  "1.5": "6px"
  "2": "8px"
  "3": "12px"
  "4": "16px"
  "5": "20px"
  "6": "24px"
  "8": "32px"
  "10": "40px"
components:
  button:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "30px"
  button-hover:
    backgroundColor: "{colors.surface-hover}"
  button-primary:
    backgroundColor: "{colors.felt-green}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "30px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "30px"
  button-ghost-hover:
    textColor: "{colors.ink}"
  button-danger:
    backgroundColor: "transparent"
    textColor: "{colors.critical}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "30px"
  button-danger-hover:
    backgroundColor: "{colors.critical-soft}"
  button-sm:
    padding: "0 9px"
    height: "26px"
  button-icon:
    width: "30px"
    padding: "0"
  field:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.field}"
    padding: "0 12px"
    height: "32px"
  pill:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.pill}"
    padding: "0 9px"
    height: "24px"
  pill-pressed:
    backgroundColor: "{colors.surface-hover}"
    textColor: "{colors.ink}"
  pill-selected:
    backgroundColor: "{colors.felt-green-soft}"
    textColor: "{colors.ink}"
  author-tag:
    textColor: "{colors.on-author}"
    typography: "{typography.tag}"
    padding: "0 6px"
    height: "20px"
  kbd:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink-faint}"
    rounded: "{rounded.tag}"
    padding: "0 4px"
    height: "18px"
  nav-link:
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.control}"
    padding: "0 10px"
    height: "32px"
  nav-link-active:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.ink}"
  panel:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.card}"
  ledger:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.card}"
  tooltip:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.pill}"
    padding: "4px 8px"
  table-header:
    textColor: "{colors.ink-faint}"
    typography: "{typography.label}"
    padding: "0 12px"
    height: "32px"
  table-cell:
    typography: "{typography.body}"
    padding: "0 12px"
    height: "40px"
---

# Design System: Eunomia

## Overview

**Creative North Star: "The Multiplayer Canvas"**

Eunomia is drawn as the shared canvas a team's agents and people write to, in the grammar of Figma and tldraw. A near-black ground (paper-white in the light theme) carries a faint 20px dot grid; on it float hairline panels with small radii, dense tabular data, and Geist type. Every memory says who wrote it through a Figma-style author tag, and the selected thing wears a green outline with four square handles. Dark is the default; `data-theme="light"` on `<html>` flips every token, and a saved light theme is applied before first paint.

Density is high and quiet. The page is a slim 232px sidebar plus a dot-grid canvas holding one page title, hairline ledgers and tables, and (on the dashboard) a floating inspector for the selected item. Lists of things are tables with hairline rows, not card grids. Colour is rationed into three separate vocabularies that never borrow from each other: one felt-green accent for what you can press or have selected, a six-colour presence palette for authorship, and muted kind and connector hues for categorising entities and sources.

Motion is small, fast and earned. Press feedback, popover entrances and a handful of authored moments (live memory arrival, token decrypt, digit roll, the theme wipe) are gated by frequency, by input device, and by reduced-motion preference. Real text is always in the DOM at its final value from the first frame.

**Key Characteristics:**
- Dot-grid canvas ground, hairline (1px, 0.5px on 2x screens) borders everywhere.
- One accent, felt green, only on pressable or selected things.
- Author tags in a separate presence palette mark who wrote what, and nothing else.
- Figma selection frame: 1px accent outline plus four 6px square handles.
- Dense tables with tabular numerals; mono only for numbers, ids, code and timestamps.
- Motion under 300ms for UI, pointer-only flourishes, nothing on keyboard or high-frequency actions.

## Colors

A near-neutral graphite (or paper) ground with a single felt-green accent and a separate, louder presence palette reserved for authorship.

### Primary
- **Felt Green** (`felt-green`, `--accent`): fills of the primary button, the selection frame and its handles, focus rings (2px, offset 2px), the field focus border, text selection, caret, the active nav icon, selected tabs (`pill[aria-selected]`), the brand mark tile. Light theme uses the deeper `felt-green-light`.
- **Felt Green Text** (`felt-green-text`, `--accent-text`): the green used as text on the ground: links, "Connect one" calls to action, the active nav icon, the update-available notice.
- **Felt Green Wash** (`felt-green-soft`, `--accent-soft`): the fill behind a selected table row and a selected tab pill, and the update notice background.
- **On Felt** (`on-accent`): ink that sits on a green fill.

### Secondary: the presence palette
- **Author 1 to 6** (`author-1` coral, `author-2` sky, `author-3` lilac, `author-4` ochre, `author-5` rose, `author-6` teal): the background of author tags, the author cursor glyph, the account avatar, and the 16% arrival wash on newly arrived rows. Assigned by `authorColor(name)`: known agents get fixed slots (Claude and Claude Code 1, Gemini 2, Cursor 3, Windsurf 4, Eunomia 5, Codex 6); any other name hashes stably into one of the six. Text on a tag is `on-author` (canvas ink in dark, white in light).

### Tertiary: categorical hues
- **Entity kinds** (`kind-person`, `kind-organisation`, `kind-location`, `kind-repository`, `kind-file`, `kind-symbol`): muted, desaturated hues for the kind dot beside an entity and the nodes of the entity graph.
- **Connector and series hues** (`--connector-*`, `--series-1..4` in globals.css): per-source identity dots and chart series. Same muted register as the kinds; never as fills behind text.

### Status
- **Good** (`good`): healthy status dots and the drawn "done" check. Shares the felt-green value but carries status meaning, not action.
- **Warning** (`warning`) and **Critical** (`critical`, with `critical-soft` wash): status dots, error text, danger buttons, the hold-to-delete fill.

### Neutral
- **Canvas** (`canvas`): the page ground under the dot grid, and the outline stroke around the author cursor.
- **Surface** (`surface`): ledgers, panels, the sidebar and mobile top bar.
- **Surface Raised** (`surface-raised`): buttons, fields, pills, tooltips, table-row hover, the active nav row, code blocks.
- **Surface Hover** (`surface-hover`): button hover, pressed filter pills, the skeleton shimmer peak.
- **Ink / Ink Dim / Ink Faint** (`ink`, `ink-dim`, `ink-faint`): primary text; secondary text and descriptions; labels, placeholders, table headers, kbd glyphs, idle nav icons.
- **Border / Border Strong** (`border`, `border-strong`): hairline rules and container edges; button and kbd edges and the scrollbar thumb.
- **Grid Dot** (`grid-dot`): the 1px dots of the canvas grid.
- **Scrim** (`scrim`): behind the command palette and the mobile drawer.

### Named Rules
**The Felt Rule.** Felt green appears only on something you can press or something that is selected (primary button, focus ring, selection frame, selected tab, link). Never on headings, decoration, or icons at rest.

**The Presence Rule.** Author colours mark who wrote something and nothing else. Never use them for actions, status or decoration, and never invent an author: if the data has none, show no tag.

**The Quiet Filter Rule.** Multi-select filters that are on (`aria-pressed="true"`) go neutral (raised fill, strong border, full ink), so a row of them never becomes a wall of green. Only single-choice selection (`aria-selected`) takes the accent.

**The Two Theme Rule.** Every colour is a variable with a dark and a light value. Never hard-code black, white or rgba shadows; use `scrim`, `--shadow-panel`, `--shadow-pop`.

## Typography

**Display Font:** Geist (with system-ui, sans-serif)
**Body Font:** Geist (with system-ui, sans-serif), stylistic sets `ss01` and `cv11` on
**Label/Mono Font:** Geist Mono (with ui-monospace, monospace), tabular numerals, ligature features reset

**Character:** One neo-grotesque family at small, dense sizes, tightened at headline weights; its mono sibling carries every number, id, path and timestamp so columns align.

### Hierarchy
- **Display** (600, 22px, 1.2, -0.025em, balanced wrap): the page title, one per page, top left of the canvas. The onboarding welcome heading uses the same style.
- **Headline** (600, 17px, -0.015em): pane and detail headings inside a page (chat thread title, vault detail).
- **Title** (600, 13px, -0.005em): section headings inside a page ("Live memory", "Connected sources").
- **Body** (400, 13px; root is 14px): table cells, nav rows, descriptions. Helper paragraphs hold to 62ch; long-form docs hold to 70ch at line-height 1.6.
- **Label** (400, 12px, 1.3, `ink-faint`): field labels, column headers, stat labels, palette group names. Sentence case, no tracking.
- **Stat** (Geist Mono, 26px, line-height 1, -0.02em): numeric stat values on the dashboard strip; non-numeric stat values use Geist 500 at the same size.
- **Mono** (Geist Mono, 10.5 to 13px): ids, tokens, URLs, commands, record counts, timestamps, the version tag.
- **Tag** (500, 11.5px): author tags; kbd glyphs are mono 500 at 11px.

### Named Rules
**The No Kicker Rule.** Headings carry their own weight. Never set a small label above a heading; `label` is for fields, columns and stats only.

**The Honest Mono Rule.** Mono is for data that must align or be copied exactly (numbers, ids, code, timestamps). Never for prose, labels or decoration.

## Layout

The app shell is a flex row: a sticky full-height sidebar (232px, `surface`, right hairline) and a `main` canvas with the dot grid (20px pitch) padded 24px by 16px on mobile and 32px by 40px from `md` (768px) up. Below `md` the sidebar becomes a 48px sticky top bar (menu, brand, search) and a 264px slide-in drawer over the scrim.

Pages stack vertically with 24 to 36px between sections (`gap-6` to `gap-9`) and cap their width (dashboard 1240px, vaults 72rem, settings 64rem). Two-column pages put the main ledger on the left and a fixed side column on the right from `lg` (1024px): 320px for the dashboard inspector (sticky at 32px), 380px for vault detail. Settings uses a 180px tab rail beside its panel from `md`, with rows laid out as a 180px label column beside the control. The entities page breaks out of the canvas padding to fill the viewport with its graph.

Spacing is a 4px-based rhythm with a heavy middle: 6 to 8px inside tight groups (icon and label, tag and text), 12px for table cell and button padding, 16 to 20px inside ledgers and panels, 24 to 40px between sections. More space sits above a heading than below it. The command palette opens 14vh from the top, 560px wide.

## Elevation & Depth

Hybrid and restrained. In-flow containers (ledgers, surfaces) are flat: a hairline border on `surface` against the dot-grid canvas. Only floating things get a shadow, and every shadow starts with a 1px ring so the edge reads as a hairline first. Depth otherwise comes from tonal steps (`canvas` to `surface` to `surface-raised` to `surface-hover`).

### Shadow Vocabulary
- **Panel** (`--shadow-panel`, dark: `0 0 0 1px var(--border), 0 1px 1px rgba(0,0,0,0.25), 0 12px 32px -12px rgba(0,0,0,0.6)`): floating panels: the dashboard inspector, the command palette, onboarding and auth cards.
- **Pop** (`--shadow-pop`, dark: `0 0 0 1px var(--border-strong), 0 4px 12px -2px rgba(0,0,0,0.4), 0 24px 64px -16px rgba(0,0,0,0.7)`): tooltips, menus and popovers that sit above panels.
- Light theme swaps both to ink-tinted shadows at a quarter of the opacity (see the sidecar).

### Named Rules
**The Float Only Rule.** In-flow blocks never cast a shadow; a shadow means the thing floats over the canvas. Never nest a card inside a card.

## Shapes

Small, consistent radii and one-device-pixel lines. Tags and kbd keys 4px (the author tag is 4px with a 1px bottom-left corner, so it points at its cursor like a Figma name tag); pills and tooltips 6px; buttons, fields and nav rows 7px; code blocks 8px; ledgers, panels and the inspector 10px; status dots and avatars fully round. Every border and rule is `--hair` (1px, 0.5px on 2x displays). The selection frame is the one recurring silhouette: a 1px accent outline inset by 1px with four 6px square handles at the corners, 3px outside the box. The brand mark is a 6px-radius green tile where three scattered points resolve into one off-centre node.

## Components

### Buttons
Compact and tactile, with a press you can feel.
- **Shape:** gently squared (7px radius), 30px tall, 12px side padding, 13px weight 500, 6px icon gap, hairline `border-strong` edge.
- **Default:** `surface-raised` fill, `ink` text. Hover (fine pointers only) moves to `surface-hover`.
- **Primary:** felt-green fill, no border, `on-accent` text; hover mixes 12% ink into the green. One per view, the main action.
- **Ghost:** transparent, `ink-dim` text, hover to `ink`. Used for icon buttons in the sidebar footer, mobile bar and toolbars.
- **Danger:** transparent with `critical` text and a strong hairline; hover fills `critical-soft`. Destructive vault and token actions use the HoldButton variant (below).
- **Sizes:** small 26px tall, 9px padding, 12px text; icon-only is a 30px (or 26px small) square.
- **States:** press scales to 0.97 over 140ms `--ease-out`; disabled is 50% opacity with a not-allowed cursor; focus is the global 2px accent ring at 2px offset.

### Chips (pills)
- **Style:** 24px tall, 6px radius, 9px padding, 12px `ink-dim` text on `surface-raised` with a hairline border. Pressable pills share the 0.97 press.
- **State:** filter off (`aria-pressed="false"`) drops to 70% opacity; filter on goes neutral (strong border, `surface-hover`, `ink`); single-choice selected (`aria-selected="true"`) takes the accent border and `felt-green-soft` fill.

### Author Tag (signature)
The multiplayer presence grammar.
- **Tag:** 20px tall, 6px padding, radius 4px 4px 4px 1px, 11.5px weight 500, author colour fill with `on-author` text, truncates inside its column; email authors show the part before the @, the full name sits in the title.
- **Cursor:** an optional arrow glyph in the author colour with a `canvas` outline, pinned to the tag's top-left corner. Shown on API-token tags in the dashboard header and, briefly, on freshly arrived memory rows.
- **Avatar:** the account avatar in the sidebar footer is a 24px circle in the user's author colour with their initial.
- **Arrival:** rows new since the previous 60s poll (never on first load) fade in over 200ms and hold a 16% author-colour wash that clears over 1.4s; up to three of them show the author cursor for 2.4s. Under reduced motion the wash (movement-free) stays and the fade is dropped.

### Selection Frame (signature)
- **In-flow and floating items:** 1px accent outline at -1px offset plus four 6px square accent handles 3px outside the corners. The dashboard inspector wears it to show what is selected.
- **Table rows:** outline plus `felt-green-soft` row fill, no handles (a pseudo-element would become an extra cell).
- **Motion:** when a pointer picked the frame, the handles grow from 3.6px to 6px and fade in over 120ms `--ease-out`; keyboard selection shows them at once.

### Cards / Containers
- **Ledger / Surface:** `surface`, hairline `border`, 10px radius, no shadow. Sections inside separate with hairline rows rather than gaps.
- **Panel:** `surface`, 10px radius, `--shadow-panel` (ring included). Inspector padding 16px; onboarding card 28px.
- **Stat strip:** one ledger split into two (mobile) or four (lg) hairline cells, each a label over a 26px value.

### Inputs / Fields
- **Style:** `surface-raised`, hairline border, 7px radius, 32px tall single-line, 12px padding, 13px text (mono for URLs, keys and tokens), `ink-faint` placeholder.
- **Focus:** the border turns felt green; no glow, no outline. Border and fill transition over 120ms.
- **Errors:** `critical` text naming the problem and the recovery, on a `critical-soft` wash.

### Navigation
- **Sidebar:** brand row (20px mark, "Eunomia" in display 15px, mono version), a search field that opens the palette with a ⌘K kbd, nav rows, a "Connected" label over live sources (status dot, name, mono record count), and an account footer above a hairline.
- **Nav row:** 32px tall, 7px radius, 13px `ink-dim` text with a 15px icon in `ink-faint` (stroke 1.75). Hover (fine pointers) and active both use `surface-raised` with `ink`; active adds weight 500 and turns the icon felt green. No motion beyond the 120ms colour fade.
- **Mobile:** 48px top bar; the drawer slides from the left over 260ms `--ease-drawer` with the scrim fading in 180ms, and closes on navigation.

### Command Palette
A 560px floating panel 14vh from the top over the scrim: a 48px search row (15px input, esc kbd), grouped results (label headings, 36px rows at 13.5px, 7px radius), and a 36px hint footer of kbd keys. It never animates: it is keyboard-first and opened all day.

### Data Tables
Full width, 13px, tabular numerals. Headers 32px tall in 12px weight-500 `ink-faint`; cells 40px tall with 12px side padding; hairline row rules, none after the last row; row hover (fine pointers) fills `surface-raised`. Secondary columns hide below `sm` or `md` rather than wrapping.

### Tooltip
`surface-raised`, 6px radius, 4px by 8px padding, 12px text, `--shadow-pop`, optional compact kbd. Opens after 400ms of pointer hover, immediately on keyboard focus, and instantly with no animation for 300ms after a neighbour in the same group closed. Enters over 140ms from scale 0.96 toward its trigger. Visual only: the trigger keeps its own accessible name.

### Status, Loading, Empty
- **Dot:** 6px circle in `good`, `warning` or `critical` (or a kind or connector hue for identity).
- **Skeleton:** 5px radius blocks shaped like the content, a `surface-raised` to `surface-hover` shimmer over 1.2s linear.
- **Empty:** one sentence naming what is missing plus the one action that fixes it, in `ink-faint` with a felt-green link.

### Kbd
18px square minimum, 4px radius, strong hairline on `surface`, mono 11px weight 500 in `ink-faint`.

### Motion system and the bits library
Tokens: `--ease-out` cubic-bezier(0.23, 1, 0.32, 1) for entrances and presses; `--ease-in-out` cubic-bezier(0.77, 0, 0.175, 1) for things moving or breathing on screen; `--ease-drawer` cubic-bezier(0.32, 0.72, 0, 1) for the drawer. Durations: hover 120ms, press 140ms, pop 180ms, drawer 260ms. Popovers enter with `@starting-style` from scale 0.96 and opacity 0, never from nothing.

Gating, enforced in code:
- **Frequency:** nothing animates on the palette, sidebar navigation, tab switches, typing or list keyboard navigation.
- **Pointer versus keyboard:** an `InputModeTracker` writes `data-input="pointer"|"keyboard"` on `<html>` (a lone modifier key still counts as pointer). Pointer-only flourishes (handle snap, theme icon crossfade, theme wipe, sparks, check draws, hold fill) check it or the click's `detail`; keyboard actions swap state instantly.
- **Hover:** hover styles live only inside `(hover: hover) and (pointer: fine)`.
- **Reduced motion:** a global rule strips transitions to opacity and colour and shortens animations to near zero; the bits stop their keyframes and render final states; server rendering assumes reduced motion so nothing animates before hydration.
- **Honest text:** the final text is in the DOM from the first frame; scrambles and rolls are `aria-hidden` overlays, entrances start at opacity 0.001.

The bits components (`frontend/src/components/bits/`, styles in `bits.css`):
- **SyncMark:** refresh icon at rest; a 68% arc spinning at 800ms linear while running; a check (in `good`) or cross (in `critical`) that draws over 200ms only when the status changes after mount. Used by every "Sync now", test and run button (dashboard, connectors, connector setup, chat tool rows, skill, settings, vaults, entity graph).
- **CopyButton:** the one copy control; the icon draws to a check over 180ms on pointer copy, the name reads "Copied" for 1.5s; on a primary button the check uses the button ink. Dashboard, settings, skill, docs, connector setup.
- **DigitRoll:** only changed digits roll (240ms, 30ms stagger) on a later value change, never on mount. Dashboard stat strip and record counts, connector detail.
- **DecryptReveal:** a mono scramble resolving left to right within 480ms, mount only, for single-line secrets that appear after a click. Freshly minted tokens on the dashboard and in settings.
- **HoldButton:** a danger button where click, Enter or Space confirm at once and a pointer hold of 900ms also confirms, with a `critical-soft` wash filling left to right after a 150ms delay and snapping back over 200ms. Vault deletion and settings token revocation.
- **Spark:** eight short accent lines thrown 18px from the element edge over 280ms, from success handlers started by a pointer only. Vault invite and clone success, onboarding completion.
- **BlurWords:** words settle from a 4px blur over 420ms with a 45ms stagger, once. Only the onboarding "Welcome to Eunomia" heading; it is a first-run moment, so it runs longer than UI motion.
- **Tooltip / TooltipGroup:** see Tooltip above. Sidebar footer, icon-only toolbar buttons across chat, connectors, vaults, settings, docs, skill, entity graph and 3D scenes.
- **Theme toggle:** Sun and Moon share a cell and blur-crossfade over 200ms; a pointer click grows the new theme as a circle from the button over 280ms via View Transitions; keyboard, reduced motion or no support swap instantly.

**The Licence Rule.** Files in `bits/` are original implementations inspired by React Bits (reactbits.dev). React Bits is MIT plus Commons Clause and Eunomia is MIT, so no React Bits source is copied; each file opens with an "Inspired by React Bits <Name> (reactbits.dev). Original implementation for Eunomia." header, colours come only from globals.css tokens, and no new dependency is added.

**The One Moment Rule.** At most one authored motion moment per surface; everything else is press feedback and pop-in entrances under 300ms.

## Do's and Don'ts

### Do:
- **Do** build from the tokens and classes in `globals.css` (`.ledger`, `.panel`, `.btn`, `.field`, `.pill`, `.data-table`, `.label`, `.page-title`, `.section-title`, `.frame-selected`); add a token rather than a raw hex.
- **Do** show authorship with the author tag and a real author (owner email, source key, or an agent name the API returns).
- **Do** mark the selected item with the selection frame (outline plus four 6px handles), or the outline plus green wash on a table row.
- **Do** put lists of things in dense tables with hairline rows, tabular numerals and columns that hide rather than wrap on small screens.
- **Do** give every pressable thing the 0.97 press over 140ms `--ease-out`, and every floating thing the pop-in from scale 0.96.
- **Do** check every screen in both themes; dark is the default.
- **Do** keep real text in the DOM at its final value from the first frame, and render final states under reduced motion.
- **Do** use lucide-react icons at 14 to 16px with stroke 1.75.

### Don't:
- **Don't** put felt green on headings, decoration or idle icons; it means "press me" or "selected".
- **Don't** use author colours for actions, status or decoration, or invent an author.
- **Don't** set a small label or kicker above a heading.
- **Don't** give in-flow blocks a shadow or nest a card inside a card.
- **Don't** animate keyboard-driven or high-frequency actions (palette, nav, tabs, typing), use `transition: all`, `ease-in`, or scale from 0.
- **Don't** add hover styles outside `(hover: hover) and (pointer: fine)`.
- **Don't** use mono for prose or labels, gradient text, or emoji and unicode glyphs as icons.
- **Don't** copy React Bits source into `bits/`.
