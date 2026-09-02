# Research: Google Workspace push-notification capability (Gmail, Calendar, Drive)

Ticket: [Qyrhal/Eunomia#20](https://github.com/Qyrhal/Eunomia/issues/20) · Child of #1 · Feeds #11, #25, #35
Branch: `research/google-push`
Date: 2026-09-02

## TL;DR

- **Gmail** push = `users.watch` → **Cloud Pub/Sub topic** (mandatory). Delta via `users.history.list` + `historyId`. Channel life **7 days**, renew daily.
- **Calendar** push = `events.watch` → **HTTPS webhook channel** (`type: web_hook`). Delta via `events.list` + `syncToken`. Channel TTL default/max **604800 s (7 days)**, no auto-renew.
- **Drive** push = `changes.watch` → **HTTPS webhook channel**. Delta via `changes.list` + `pageToken` / `changes.getStartPageToken`. Channel max **604800 s (7 days)** for Changes (86400 s / 1 day for `files.watch`), default 3600 s, no auto-renew.
- **A publicly reachable HTTPS endpoint with a CA-valid TLS cert is mandatory for Calendar and Drive push.** There is no pull transport for them. Self-signed / untrusted certs are explicitly rejected.
- **Gmail is the exception:** its notifications land in Pub/Sub, and a Pub/Sub **pull** subscription needs no inbound endpoint — the subscriber dials out. So Gmail push works on a tailscale-only box; Calendar and Drive push do not.
- For a tailscale/netbird-only deployment with **no public HTTPS**: either (a) run a tiny public HTTPS relay that forwards `POST`s onto the tailnet (unavoidable if you want Calendar/Drive push), or (b) **poll** Calendar and Drive on an interval and use Gmail Pub/Sub pull. Polling is the realistic zero-public-surface path.

---

## 1. Gmail — `users.watch` + Cloud Pub/Sub

Source: <https://developers.google.com/workspace/gmail/api/guides/push>

- The Gmail API delivers push **only through the Cloud Pub/Sub API**. You must:
  1. Create a Pub/Sub topic (`projects/<proj>/topics/<topic>`).
  2. Grant `roles/pubsub.publisher` on that topic to **`gmail-api-push@system.gserviceaccount.com`**.
  3. Create a subscription on the topic — **push (webhook)** *or* **pull (your app initiates)**.
- Start delivery: `POST https://gmail.googleapis.com/gmail/v1/users/me/watch` with body `{ topicName, labelIds, labelFilterBehavior }`.
  Response: `{ historyId, expiration }` (`expiration` is a ms timestamp).
  Ref: <https://developers.google.com/workspace/gmail/api/reference/rest/v1/users/watch>
- **Renewal:** "You must re-call `watch` at least every 7 days or else you will stop receiving updates … We recommend calling `watch` once per day." (guide, *Renewing mailbox watch*)
- **Notification payload:** a `PubsubMessage` whose `message.data` is Base64URL-encoded JSON `{"emailAddress": "...", "historyId": "..."}`. It carries **no message content** — it is only a pointer to a new `historyId`.
- **Max notification rate:** "one event per second per user"; excess notifications are dropped (guide, *Limitations*).
- **Reliability:** "usually … within a few seconds" but "in some rare situations notifications may be dropped … periodically call `history.list`" as a backstop (guide, *Limitations*).
- **Stop:** `users.stop`.

### Delta retrieval — `historyId`

Source: <https://developers.google.com/workspace/gmail/api/guides/sync> (Partial synchronization)

- `users.history.list?startHistoryId=<last known>` returns every history record newer than that id: added/deleted messages, label changes, etc.
- History records are "typically available for at least a week and often longer", but the window "may be significantly shorter" in rare cases.
- If `startHistoryId` is too old / out of range, the API returns **HTTP 404** → client must do a **full sync** (`messages.list` + `messages.get`, then store the newest `historyId`).
- After processing a push, persist the returned `historyId` as the new checkpoint.

---

## 2. Calendar — `events.watch` (push channels) + sync tokens

Sources:
- Push guide: <https://developers.google.com/workspace/calendar/api/guides/push>
- `events.watch` reference: <https://developers.google.com/workspace/calendar/api/v3/reference/events/watch>
- Sync guide: <https://developers.google.com/workspace/calendar/api/guides/sync>

- Watchable resources: **ACL, CalendarList, Events, Settings** (each has a `watch` method).
- Create a channel: `POST .../calendars/{calendarId}/events/watch` with body:
  ```json
  {
    "id": "<uuid>",
    "type": "web_hook",
    "address": "https://your.host/notifications",
    "token": "opaque-verification-string",
    "params": { "ttl": "604800" }
  }
  ```
- **The receiver requirements are strict** (push guide, *Required properties*):
  - `address` **must use HTTPS**.
  - "Google Calendar API can send notifications to this HTTPS address only if there is a valid SSL certificate installed on your web server." Invalid = **self-signed, signed by an untrusted source, revoked, or subject/hostname mismatch**.
  - i.e. the endpoint must be **publicly reachable by Google and present a CA-trusted cert**. A tailscale/netbird-internal host cannot receive these.
- **Channel TTL / expiration** (`events.watch` reference, `params.ttl`): "The time-to-live in seconds for the notification channel. **Default is 604800 seconds**" (7 days). The push guide adds that the effective expiration is "the more restrictive value" of your request vs. Google's internal limits/defaults.
- **Renewal:** "there is currently no automatic way to renew a notification channel. When a channel is close to its expiration, you must create a new one by calling the `watch` method" with a fresh unique `id`. Expect a brief overlap where both channels deliver.
- **Notification content:** headers only (`X-Goog-Channel-ID`, `X-Goog-Channel-Token`, `X-Goog-Resource-ID`, `X-Goog-Resource-State`, `X-Goog-Message-Number`, `X-Goog-Channel-Expiration`). Body is empty. States: `sync` (sent once on channel creation, `X-Goog-Message-Number: 1`, safe to ignore) and `exists` (something changed — go call the API).
- **Ack:** respond `200/201/202/204/102`. `500/502/503/504` → Google retries with exponential backoff; any other code = delivery failure.
- **Stop:** `POST https://www.googleapis.com/calendar/v3/channels/stop` with `{ id, resourceId }`.

### Delta retrieval — `syncToken`

Source: sync guide (above).

- **Initial full sync:** `events.list` (optionally date-bounded with `timeMin`; `singleEvents=true` recommended). The response's last page carries `nextSyncToken` — persist it.
- **Incremental sync:** `events.list?syncToken=<stored>`. Returns only resources changed since last sync, **including deleted entries** (status `cancelled`) so the client can purge them.
- If the changed set is large the response paginates: you get `nextPageToken` (not `nextSyncToken`) — keep the same `syncToken`, append `pageToken`, page until a `nextSyncToken` appears on the final page.
- **Token invalidation:** server may expire a sync token (token age or ACL change) → responds **HTTP 410 Gone** → wipe local store, do a fresh full sync.
- Sync tokens are incompatible with most `list` filters (date range is the main allowed narrowing on full sync).

---

## 3. Drive — `changes.watch` + `changes` API

Sources:
- Push guide: <https://developers.google.com/workspace/drive/api/guides/push>
- `changes.watch` reference: <https://developers.google.com/workspace/drive/api/reference/rest/v3/changes/watch>
- Manage-changes guide: <https://developers.google.com/workspace/drive/api/guides/manage-changes>

- Watchable: **`changes`** (whole-account changelog) and **`files`** (single file). For Eunomia's "did anything change" use case, `changes.watch` is the one.
- Create a channel: `POST https://www.googleapis.com/drive/v3/changes/watch` with a `Channel` body — same shape as Calendar (`id`, `type: web_hook`, `address`, `token`, `expiration`).
  The request also takes the same query params as `changes.list` (`pageToken`, `includeRemoved`, `restrictToMyDrive`, `includeItemsFromAllDrives`, `spaces`, `pageSize`, …).
- **Receiver requirements: identical to Calendar** — `address` must be HTTPS with a **valid, CA-trusted** cert; self-signed / untrusted / revoked / hostname-mismatch certs are rejected (push guide, *Required properties*). Publicly reachable endpoint mandatory.
- **Channel TTL / expiration:**
  - Push guide: expiration is "determined either by your request or by … Drive API internal limits or defaults (the more restrictive value is used)"; **default 3600 s** if `expiration` is omitted.
  - The long-standing documented cap: **max 86400 s (1 day) for `files` resources, 604800 s (1 week) for `changes`** — quoted verbatim from the Drive push docs in multiple Google-forum / Stack Overflow answers, e.g. <https://stackoverflow.com/questions/70673398/google-drive-api-watch-channel-is-only-24h> and <https://stackoverflow.com/questions/45279261/push-notifications-and-channels-lifecycle>. Treat 7 days as the ceiling for a `changes` channel; verify the exact number from the returned `expiration` at runtime.
  - **Renewal:** same as Calendar — "no automatic way to renew"; call `watch` again with a new `id` before expiry; expect overlap.
- **Notification content:** headers only, **empty body** (push guide explicitly: "Notification messages for both `files` … and `changes` resources are always empty"). `X-Goog-Resource-State: sync` on creation, then `change` when changelog items are added. `files` watches also emit `add/remove/update/trash/untrash` with an optional `X-Goog-Changed: content,properties,parents,children,permissions`.
- **Ack / retry / stop:** same as Calendar. Stop: `POST https://www.googleapis.com/drive/v3/channels/stop` with `{ id, resourceId }`.

### Delta retrieval — `pageToken` / `startPageToken`

Source: manage-changes guide (above).

- **Bootstrap:** `changes.getStartPageToken` → returns `startPageToken`; store it. This marks "now".
- **Poll / drain:** `changes.list?pageToken=<token>`.
  - `pageSize` default 100, max 1000.
  - Entries are chronological (oldest first).
  - Response has **either** `nextPageToken` (more pages — keep going) **or** `newStartPageToken` (you're caught up — store it as the next checkpoint).
- The push notification carries no detail; it is purely a signal to run the `changes.list` drain loop above. Polling `changes.list` on a timer is the exact same code path with no webhook.

---

## 4. Is a public HTTPS webhook mandatory? — Yes, for Calendar and Drive

**Plainly: yes.** Calendar `*.watch` and Drive `*.watch` only support `type: "web_hook"` with an `address` that is (a) HTTPS, (b) publicly resolvable and reachable from Google's servers, and (c) presenting a certificate chained to a public CA. Google documents that it will **not** deliver to self-signed or untrusted certs. There is no pull, long-poll, or Pub/Sub transport for Calendar or Drive push.

**Gmail is different.** Gmail push is published to a **Cloud Pub/Sub topic**. A Pub/Sub **pull** subscription (`projects.subscriptions.pull` / `StreamingPull`) is initiated by the subscriber — the Django box dials out to `pubsub.googleapis.com` over 443 and needs **no inbound port, no public DNS, no public cert**. So real-time Gmail notifications are fully compatible with a tailscale/netbird-only deployment.

### What this means for a tailscale-only Eunomia

| Service | Real-time push without a public endpoint? | Path |
| --- | --- | --- |
| Gmail | **Yes** | `users.watch` → Pub/Sub topic → **pull** subscription from Django. No relay. |
| Calendar | **No** | Needs public HTTPS webhook. Options below. |
| Drive | **No** | Needs public HTTPS webhook. Options below. |

For Calendar + Drive you have two honest choices:

1. **Small public relay (unavoidable if you want push):** a minimal always-on HTTPS endpoint on the public internet (Cloud Run, a Fly/Render/Cloudflare Worker, a 5-line nginx on a cheap VPS with a Let's Encrypt cert, or a Cloudflare Tunnel whose public hostname terminates TLS at Cloudflare). It does nothing but authenticate the `X-Goog-Channel-Token`, then forward the notification headers onto the tailnet (HTTP POST to the Django host's tailscale IP, or drop a row/enqueue a job). This keeps Django itself off the public internet; only a dumb forwarder is exposed. A Cloudflare Tunnel is the least-infrastructure version — no inbound firewall rule, the tunnel daemon dials out from the tailnet-side host, Cloudflare provides the public cert.
2. **Polling only (zero public surface):** don't call `events.watch` / `changes.watch` at all. Poll `events.list?syncToken=…` and `changes.list?pageToken=…` on a schedule. Simplest, no extra infra, at the cost of latency (minutes instead of seconds).

**Recommendation for Eunomia:** Gmail via Pub/Sub **pull** (no relay, near-real-time). Calendar + Drive via **polling** to start (sync-token / page-token delta, cheap — see §5). Add the tiny public relay for Calendar/Drive push later only if minute-scale latency proves too slow; it's an additive change (swap the cron trigger for a webhook trigger, same drain code).

---

## 5. Polling fallback design

The delta mechanism is identical whether triggered by a push or a timer, so a poller is the push handler minus the webhook.

### Gmail (fallback if not using Pub/Sub pull, or as a safety net)

- Store the last `historyId`. On each tick: `users.history.list?startHistoryId=<last>`; walk pages; update checkpoint. On 404 → full sync.
- **Interval:** the push guide itself recommends "periodically call `history.list`" as a backstop after a quiet period. **Every 5–15 min** is ample for a personal-assistant app. Even if you rely on Pub/Sub, run a **1×/hour** reconciliation poll to catch dropped notifications.
- **Quota:** `history.list` = **2 quota units**; `watch` = 100; `messages.get` = 20; `messages.list` = 5. Project budget is **1,200,000 units/min**, **6,000 units/min/user**, soft daily threshold 80,000,000 units/project (billing "later in 2026, 90 days' notice"). Ref: <https://developers.google.com/workspace/gmail/api/reference/quota>. A 5-min `history.list` poll = ~576 units/user/day — negligible.

### Calendar

- Store `nextSyncToken` per calendar. On each tick: `events.list?syncToken=<stored>`; page via `pageToken` until a new `nextSyncToken`; store it. On **410** → drop store, full sync.
- **Interval:** no minimum is documented. **5–15 min** is a good default; tighten to ~2 min for calendars that drive time-sensitive reminders. An empty incremental sync is one cheap request.
- **Quota:** Calendar API default courtesy limit is **1,000,000 queries/day** per project and **600 queries/min/user** (Google Cloud console "Calendar API" quotas; <https://developers.google.com/workspace/calendar/api/guides/quota>). Each poll is 1 request + 1 per extra page. 5-min poll ≈ 288 requests/calendar/day — trivial.

### Drive

- Bootstrap once with `changes.getStartPageToken`. On each tick: `changes.list?pageToken=<token>` loop until `newStartPageToken`; store it.
- **Interval:** no documented minimum. **5–15 min**; `pageSize=1000` to drain bursts in one call.
- **Quota:** Drive API default is **12,000 queries/min per user** and a large per-project ceiling (Google Cloud console "Google Drive API" quotas; <https://developers.google.com/workspace/drive/api/guides/limits>). `changes.list` is 1 request per page. 5-min poll ≈ 288 requests/user/day — trivial.

### General

- Jitter the schedule; use truncated exponential backoff on 403/429/5xx (Gmail quota guide documents the algorithm).
- Persist every checkpoint (`historyId`, `syncToken`, `startPageToken`) transactionally with the data it commits, so a crash mid-drain re-processes rather than skips.
- Poll intervals are a tuning knob, not a constant — expose them in config.

---

## Sources (all primary Google docs unless noted)

- Gmail push: <https://developers.google.com/workspace/gmail/api/guides/push>
- Gmail `users.watch`: <https://developers.google.com/workspace/gmail/api/reference/rest/v1/users/watch>
- Gmail sync / `history.list`: <https://developers.google.com/workspace/gmail/api/guides/sync>
- Gmail usage limits / quota units: <https://developers.google.com/workspace/gmail/api/reference/quota>
- Calendar push: <https://developers.google.com/workspace/calendar/api/guides/push>
- Calendar `events.watch` (`params.ttl` default 604800): <https://developers.google.com/workspace/calendar/api/v3/reference/events/watch>
- Calendar sync / `syncToken` / 410: <https://developers.google.com/workspace/calendar/api/guides/sync>
- Drive push: <https://developers.google.com/workspace/drive/api/guides/push>
- Drive `changes.watch`: <https://developers.google.com/workspace/drive/api/reference/rest/v3/changes/watch>
- Drive manage-changes / `pageToken` / `getStartPageToken`: <https://developers.google.com/workspace/drive/api/guides/manage-changes>
- Drive channel TTL cap (docs note quoted 2nd-hand): <https://stackoverflow.com/questions/70673398/google-drive-api-watch-channel-is-only-24h>, <https://stackoverflow.com/questions/45279261/push-notifications-and-channels-lifecycle>
- Pub/Sub pull subscriptions (no inbound endpoint): <https://cloud.google.com/pubsub/docs/pull>
- Pub/Sub pricing (usage-based, first 10 GiB/mo free tier): <https://cloud.google.com/pubsub/pricing>
