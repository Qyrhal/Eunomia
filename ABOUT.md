# About Eunomia

## What this is

Eunomia is a personal, self-hosted dashboard that pulls everything you're keeping
track of — Obsidian notes, work tickets, tasks, transcribed voice notes, reminders —
into one place, sorted automatically into the parts of your life they belong to:
**Uni, Work, Business, or Other**.

It exists because that context currently lives in five different apps that don't
talk to each other: Obsidian for notes, Slack/Linear/GitHub for work, a second
Linear/GitHub for your own business, HeyPocket for voice transcription, Apple
Reminders for todos. None of them know about the others, and none of them know
which parts of your life they belong to. Eunomia is the layer that does — one
feed, correctly bucketed, without you re-filing anything by hand.

## Why "Eunomia"

Eunomia (Εὐνομία) is the Greek personification of good order and lawful
governance — literally "good pasture," the idea that things thrive when they're
properly arranged. That's the whole premise of the app: not doing more, just
keeping what already exists in the right place, automatically.

## What it's for

- **One dashboard, four buckets.** Every note, ticket, and task lands in Uni,
  Work, Business, or Other — classified by tag, folder, or (when those don't
  give a clear signal) an LLM call, never by hand.
- **Your vault, read-only.** Eunomia reads the Obsidian vault you already sync
  (Obsidian Sync, Syncthing, iCloud — whatever keeps it on disk) and classifies
  what's there. It doesn't replace Obsidian or require a plugin.
- **Two identities per work integration.** You have a day job and a business,
  each with its own GitHub and Linear. Eunomia keeps those as separate connected
  accounts under the same service, not merged.
- **Bring your own model.** Any LLM provider's API key can be dropped in for the
  classification fallback, with automatic failover if a key stops working.
- **Voice notes become tasks.** HeyPocket transcripts flow into the same
  classification pipeline as everything else — a transcript is just another note
  that needs a bucket.
- **Todos meet you where you already look.** Tasks generated from any source can
  sync out to Apple Reminders via iCloud, so you're not checking a sixth app to
  see what's on your plate.

## What it deliberately isn't

- Not a note-taking app — Obsidian stays the place you write.
- Not a project-management tool — Linear and GitHub stay the source of truth for
  work; Eunomia surfaces it, it doesn't replace their workflows.
- Not multi-user or cloud SaaS — it's one person's dashboard, self-hosted, with
  a SQLite file as the entire backend.
- Not trying to auto-do anything irreversible — classification and surfacing are
  automatic; actions with side effects (closing tickets, deleting reminders)
  are not silently automated.

## How it's built (short version)

Single FastAPI process, single SQLite file, no build step, no external services
required to run the core dashboard. Notes are read straight off disk; classification
is rule-based first, LLM-assisted only as a fallback. Credentials (sign-in tokens
and API keys) are encrypted at rest and stored in the same database — see the
Settings page and the README's "Configure" section for how those are set up.
Full architecture and build order are captured in the project's ongoing planning
history, not duplicated here — this file is about *why* the app exists, not a
spec of every module.

## Status

Foundation (vault reading, classification, dashboard) and the credentials/settings
layer are built. Slack, Linear, GitHub, HeyPocket, and Apple Reminders sync are
not wired up yet — the settings page is ready to hold their credentials once they are.
