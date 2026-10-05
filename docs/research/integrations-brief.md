# Research: personal-life integrations (Apple Reminders, multi-Gmail, Spotify, Moodle)

Date: 2026-09-06 · Status: proposal · North star: Hermes+Eunomia = the one front-end for
Avi's tasks, reminders, calendar, email, meetings (pasted Granola notes), and uni work.
No wall-of-text UX: the agent acts, Eunomia stores, things just happen.

## TL;DR

| Integration | Approach | Write-back | Effort |
|---|---|---|---|
| Apple Reminders | Shortcuts CLI over SSH/NetBird to the Mac (EventKit on-device) | Yes (create/complete) | M |
| Multi-account Gmail | N Google OAuth tokens, one per account, keyed per Connector row | Send via existing Google push work (#20) | S-M |
| Spotify | Web API + OAuth PKCE, read-only (playlists, recently played, saved tracks) | Playback only via Hermes-side plugin (already exists) | S |
| Moodle | Web services token → core_calendar_get_calendar_events + mod/assign WS | Read-only → Eunomia tasks | S |
| Granola | None needed: paste notes into Hermes chat → agent creates tasks | n/a | 0 |

Build order: Moodle (token in hand, instant value) → multi-Gmail → Apple Reminders → Spotify.

---

## 1. Apple Reminders (priority)

Options evaluated:

- **AppleScript/osascript directly**: iterating reminders is notoriously slow
  (community reports ~18s for 180 items) and macOS TCC blocks automation of
  Reminders for sshd sessions unless explicitly granted; headless reliability is poor.
- **iCloud CalDAV (VTODO)**: reminders do appear as VTODOs on
  `caldav.icloud.com`, but Apple requires **app-specific passwords** (iCloud
  account settings) for third-party CalDAV clients, sync latency is unbounded
  (reminders propagate to iCloud lazily), and two-way completion state is
  flaky across clients.
- **Shortcuts CLI on the Mac (recommended)**: `shortcuts run "name" <input>`
  is officially supported from Terminal and over SSH (Apple: "Run shortcuts
  from the command line", support.apple.com/en-au/guide/shortcuts-mac/apd455c82f02/mac).
  A tiny Eunomia-sync shortcut uses EventKit locally (fast, native TCC grant
  once per account), reads all reminders, emits JSON; write-back shortcuts
  ("eunomia-create", "eunomia-complete") call EventKit create/complete.
  Transport: SSH from Venus to the Mac over NetBird (`100.x` address), key
  auth already the homelab pattern.

Mapping to Eunomia source surface: `auth` = Mac reachable + shortcut inventory
probe; `sync` = run read shortcut → ingest as cache records + mirror to tasks
with `source=apple_reminders` tag; `tools` = create_reminder / complete_reminder
(wrapping write shortcuts); `webhook` = none (poll every sync_interval, default
15 min). Gotchas: Mac must be awake (set `caffeinate`/power settings; NetBird
wake-on-LAN optional), first-run TCC prompt needs one manual approval, shortcut
names must be stable contracts.

## 2. Multi-account Gmail

The Connector model stores credentials per kind (google = Calendar+Gmail
single row today). For N accounts: keep one Connector row **per Gmail
account** (kind=`google`, `config.label` = account nickname, `config.email`
= address), each holding its own encrypted OAuth token blob. Token refresh is
standard `grant_type=refresh_token` against `accounts.google.com/token`
(refresh tokens are per-user and long-lived; see developers.google.com/identity
docs). Ingestion tags every cached email with the account label so the
email page can group/filter; Hermes tools accept `account=` or default to
"all accounts, newest first". Gmail API scopes: `gmail.readonly` for
ingestion; `gmail.send`/`gmail.modify` only if agent sending is wanted
(scope choice reference: developers.google.com/workspace/gmail/api/auth/scopes).
Watch-push (#20 research doc) can later fan Pub/Sub notifications per account.
Gotcha: OAuth consent app must be "In production" or refresh tokens die in
7 days (known, already handled on the Hermes side).

## 3. Spotify (read source)

Web API with OAuth (PKCE or client-credentials for non-owner data; PKCE user
auth for his own library): playlists
(`GET /users/{id}/playlists`, developer.spotify.com/documentation/web-api/
reference/get-list-users-playlists), recently played, saved tracks, currently
playing. Cache as records (playlist/track/artist) with `spotify` source tag;
triggers like "new saved track" or "new playlist" are natural Eunomia trigger
rules. Playback control stays with the Hermes-side Spotify plugin (already
working) — Eunomia is the memory, Hermes is the remote. Rate limits are
generous (rolling 30s window); token refresh hourly.

## 4. Moodle / uni assignments

Web services token (Avi provides): `core_calendar_get_calendar_events`
(available since 2.5, see docs.moodle.org/dev/Web_service_API_functions)
returns assignment due-date events; `mod_assign_*` WS functions give
assignment detail/submission status. Sync every 1-6h → upsert Eunomia tasks
in project "Uni", due_at from the event, title = assignment name, course as
tag. Idempotent upsert keyed by Moodle event/assign id stored in task props.
Gotcha: Monash may restrict which WS functions the token can call — probe
with `core_webservice_get_site_info` first; fall back to the existing
Moodle-calendar-ICS flow (google-workspace skill has an import pattern) if
token scope is too narrow.

## 5. Granola meetings

Zero integration: Avi pastes notes/transcript into Hermes chat. Agent flow:
extract action items → `create_task` per item (project from context, due dates
normalized via the new user-timezone handling) → store meeting summary as a
cache record. No API, no sync, no code.

## Sources (verified via web search during research)

- Apple: https://support.apple.com/en-au/guide/shortcuts-mac/apd455c82f02/mac (shortcuts CLI)
- Gmail scopes: https://developers.google.com/workspace/gmail/api/auth/scopes
- Google OAuth token endpoint: https://developers.google.com/identity/protocols/oauth2
- Spotify playlists: https://developer.spotify.com/documentation/web-api/reference/get-list-users-playlists
- Moodle WS functions index: https://docs.moodle.org/dev/Web_service_API_functions
- (Local ecosystem reports on AppleScript Reminders latency: apple.stackexchange.com/questions/417866)
