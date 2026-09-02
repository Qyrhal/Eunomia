# Research: Up Bank API — real-time + delta-sync capability

Ticket: [Qyrhal/Eunomia#19](https://github.com/Qyrhal/Eunomia/issues/19) (child of #1). Feeds #12, #25, #35.

**Question:** What real-time and delta-sync capability does the Up Bank API actually
offer, for keeping a local transaction cache fresh?

## Primary sources

- Docs (single-page reference): <https://developer.up.com.au/>
- OpenAPI 3.0 spec (authoritative, versioned): <https://github.com/up-banking/api> —
  raw: <https://raw.githubusercontent.com/up-banking/api/master/v1/openapi.json>
  (`info.version: v1`, `servers[0].url: https://api.up.com.au/api/v1`)

Every claim below is from one of those two. The rendered docs page is large; the
webhook callback contract quoted here is taken verbatim from the OpenAPI spec
(`paths./webhooks.post.callbacks.Event.{webhookURL}.post`).

---

## 1. Webhooks

### Event types (`WebhookEventTypeEnum`)

| eventType | Trigger (spec, verbatim intent) | `transaction` relationship |
|---|---|---|
| `PING` | Manually triggered by the webhook `ping` endpoint. Testing/debugging only. | no |
| `TRANSACTION_CREATED` | "Triggered whenever a new transaction is created in Up." | yes, with `links.related` |
| `TRANSACTION_SETTLED` | "Triggered whenever a transaction transitions from the `HELD` status to the `SETTLED` status." | yes, with `links.related` |
| `TRANSACTION_DELETED` | "Triggered whenever a `HELD` transaction is deleted from Up" (e.g. a hotel deposit returned). | yes, but **no link** (resource no longer exists) |

There are **no** account, category, tag, or balance webhook events. Only these four.

Important caveat on `TRANSACTION_SETTLED` (spec, verbatim): *"Due to external factors
in banking processes, on rare occasions this event may not be triggered. Separate
`TRANSACTION_DELETED` and `TRANSACTION_CREATED` events will be received in its
place."* So a consumer must treat "held txn disappears + new settled txn appears"
as an equivalent settle path.

### Payload shape (`WebhookEventCallback` → `WebhookEventResource`)

`POST` to your URL, `Content-Type: application/json`:

```json
{
  "data": {
    "type": "webhook-events",
    "id": "32e730bd-752d-4ae8-a8e2-bab115c72b6d",
    "attributes": {
      "eventType": "TRANSACTION_CREATED",
      "createdAt": "2024-08-06T12:19:02+10:00"
    },
    "relationships": {
      "webhook": {
        "data": { "type": "webhooks", "id": "135017b5-..." },
        "links": { "related": "https://api.up.com.au/api/v1/webhooks/135017b5-..." }
      },
      "transaction": {
        "data": { "type": "transactions", "id": "516f5342-..." },
        "links": { "related": "https://api.up.com.au/api/v1/transactions/516f5342-..." }
      }
    }
  }
}
```

- The payload carries **only IDs + links, never the transaction body**. You must
  `GET` the `transaction.links.related` URL to get the actual data. (Spec: "This
  link should be used to retrieve the complete transaction data.")
- `data.id` (the event id) "will remain constant across delivery retries" — usable
  as an idempotency key.
- `attributes.createdAt` is when the event was generated (RFC-3339 with TZ offset).

### Registering a webhook — `POST /webhooks`

Request (`CreateWebhookRequest` / `WebhookInputResource`):

```json
{ "data": { "attributes": {
  "url": "https://your.app/callbacks/up",       // valid HTTP/HTTPS, <= 300 chars
  "description": "optional, <= 64 chars"
}}}
```

Response `201` (`WebhookResource`) includes a **`secretKey`** attribute. Spec,
verbatim: *"This field is returned only once, upon the initial creation of the
webhook. If lost, create a new webhook and delete this webhook."* It is not
returned by `GET /webhooks` or `GET /webhooks/{id}`.

- Limit: **10 webhooks** at any one time (spec text on `POST /webhooks` says 10;
  once reached you must delete before creating). Delete via `DELETE /webhooks/{id}`.
- After creating, send a test event with `POST /webhooks/{webhookId}/ping` (`201`,
  returns the `PING` event data in the response body; delivered asynchronously).

### Signature verification (`X-Up-Authenticity-Signature`)

Every incoming webhook `POST` carries header `X-Up-Authenticity-Signature`. Spec
verification process, verbatim:

1. Take the **raw, unparsed** webhook event request body.
2. Compute the **SHA-256 HMAC** of that body using the shared `secretKey`.
3. Compare (constant-time) against the `X-Up-Authenticity-Signature` header value.

Header value is the **hex digest** (example in spec:
`317c0a8ea81df3f53c1d2aef5dcbf60492d0df557197b2990e71daa4a0693364`). Reference
implementations given for Ruby (`OpenSSL::HMAC.hexdigest('SHA256', ...)` +
`Rack::Utils.secure_compare`), PHP (`hash_hmac('sha256', $raw_body, $secret)` +
`hash_equals`), and Go (`hmac.New(sha256.New, secretKey)` + `hmac.Equal`).
Python equivalent: `hmac.new(secret, raw_body, hashlib.sha256).hexdigest()` +
`hmac.compare_digest`.

### Delivery guarantees & retries

- Your URL **must** respond `200`. Any non-`200`, unreachable host, or timeout →
  **retried with exponential backoff** (spec, on both `POST /webhooks` and the
  callback definition).
- **Response timeout is currently 30s.** Spec advises: do no heavy processing
  inline; ack fast and hand off to a queue/broker.
- Delivery is **at-least-once** (retries + stable event `id` ⇒ you must dedupe).
- No ordering guarantee stated. No signed timestamp / replay window beyond the
  HMAC.

### Delivery logs — `GET /webhooks/{webhookId}/logs`

Paginated (`page[size]`), newest-first. Each `WebhookDeliveryLogResource` has
`attributes.request.body`, `attributes.response.statusCode` + `.body` (nullable),
`attributes.deliveryStatus`, `attributes.createdAt`, and a `webhookEvent`
relationship. `WebhookDeliveryStatusEnum`:

- `DELIVERED` — 200 received.
- `UNDELIVERABLE` — URL unreachable or timed out.
- `BAD_RESPONSE_CODE` — delivered but non-200 response.

Spec: *"Logs may be automatically purged after a period of time."* — not a durable
audit store.

---

## 2. Rate limits

The docs only say (status-code table, entry `429`):

> **Too many requests**: You have been rate limited—try later, ideally with
> exponential backoff. The `X-RateLimit-Remaining` response header shows the
> number remaining.

Source: <https://developer.up.com.au/> (HTTP status codes section).

- No published numeric limit, window, or `X-RateLimit-Limit` / `Retry-After` in
  either the docs or the OpenAPI spec.
- Practical takeaway: read `X-RateLimit-Remaining` on every response, back off on
  `429` with exponential backoff + jitter. Treat webhooks (not polling) as the
  primary freshness mechanism to stay well clear of the limit.

---

## 3. Delta pulls

### `filter[since]` / `filter[until]`

On `GET /transactions` and `GET /accounts/{accountId}/transactions`:

- `filter[since]` — start date-time, **RFC-3339 with timezone offset**, e.g.
  `2020-01-01T01:02:03+10:00`.
- `filter[until]` — end date-time, same format.
- Both filter on the transaction's **`createdAt`** (the "date-time at which this
  transaction was first encountered"), ordered **newest-first to oldest-last**.
- Spec + docs both state, verbatim: *"These filter parameters **should not** be
  used for pagination."* Use them to bound a window; use cursor links to walk it.
- `filter[status]` = `HELD` | `SETTLED` — narrow to one status.
- `filter[category]` (parent or child id; invalid id → `404`) and `filter[tag]`
  (unknown tag → empty success) also available.

### Cursor pagination

- Opaque cursors. Response has top-level `links: { prev, next }` with
  `page[before]=` / `page[after]=` cursors baked in.
- Walk forward by following `next` until it is `null`; backward via `prev` until
  `null`. Do not construct cursors yourself.
- `links` present ⇒ endpoint is paginated; `data` is the resource array.

### `page[size]`

- Positive integer, **upper limit generally 100** ("individual endpoints may
  impose different constraints"). Spec example value `30`.
- Each endpoint has its own sensible default when omitted.

### Recommended delta-sync loop

1. Persist a high-water mark = max `createdAt` (or settle time) seen last run.
2. `GET /transactions?filter[since]=<mark>&page[size]=100`, follow `next` to the
   end.
3. Upsert by transaction `id`. Advance the mark.
4. Because `filter[since]` keys on `createdAt`, a **held→settled transition does
   not change `createdAt`** and can fall *behind* your mark — see next section.

---

## 4. Transaction status lifecycle (HELD → SETTLED)

`TransactionStatusEnum` = `HELD` | `SETTLED` (spec). *"When a transaction is held,
its account's `availableBalance` is affected. When settled, its account's
`currentBalance` is affected."*

`TransactionResource.attributes` fields (spec) — full list:
`amount, cardPurchaseMethod, cashback, createdAt, description, foreignAmount,
holdInfo, isCategorizable, message, note, performingCustomer, rawText, roundUp,
settledAt, status, transactionType`.

- **There is NO `updatedAt` field.** The only timestamps are:
  - `createdAt` — "date-time at which this transaction was first encountered" (set
    once, at HELD creation; does not move on settle).
  - `settledAt` — nullable; "`null` for transactions that are currently in the
    `HELD` status"; populated when it settles.
- `holdInfo` (`HoldInfoObject`) preserves `amount` / `foreignAmount` as they were
  while HELD — present if the txn "is currently in the `HELD` status, or was ever
  in the `HELD` status". So `holdInfo != null && status == SETTLED` marks a txn
  that went through the hold path. Final `amount` can differ from `holdInfo.amount`
  (e.g. tips, FX).
- Not every transaction is held first — card purchases are typically HELD then
  SETTLED; many transfers/direct-credits appear SETTLED immediately.

### How a poller catches the transition

There is no "changed since" query. A pure poller must:

1. Keep a local set of transaction ids where `status == HELD`.
2. On each poll, **re-fetch those specific held transactions** — either
   `GET /transactions/{id}` per id, or `GET /transactions?filter[status]=HELD` and
   diff: any id that was in your HELD set but is no longer returned has either
   settled or been deleted; fetch it by id to find out (`status` now `SETTLED`
   with `settledAt` set, or `404`/gone ⇒ deleted).
3. `filter[since]` alone will **miss** the transition, because `createdAt` is
   unchanged and is likely older than your high-water mark.

Webhooks make this clean: `TRANSACTION_SETTLED` (or the
`TRANSACTION_DELETED` + `TRANSACTION_CREATED` fallback pair) fires on the
transition, so the cache only needs to re-`GET` the referenced transaction. A
belt-and-braces poll of `filter[status]=HELD` on a slow cadence covers missed
events.

---

## 5. Account / category / tag change signals

- **No webhooks** for accounts, categories, or tags (enum has only the 4 txn/ping
  types).
- `AccountResource.attributes` = `accountType, balance, createdAt, displayName,
  ownershipType` — **no `updatedAt`**. Detect new/removed accounts by periodically
  listing `GET /accounts` and diffing ids; detect balance movement from
  `balance` (or infer from transactions).
- Categories: `GET /categories` returns the fixed Up category tree (`filter[parent]`
  for children). Effectively static; no change signal. A transaction's category is
  mutable via `PATCH /transactions/{id}/relationships/category` but that change is
  not broadcast — you'd re-fetch the transaction (no event, no `updatedAt`).
- Tags: `GET /tags`, and add/remove via
  `/transactions/{transactionId}/relationships/tags`. No change signal.

Bottom line: for anything other than transaction create/settle/delete, the only
mechanism is **periodic list-and-diff**.

---

## 6. `util/ping` and the PAT auth model

### Auth model

- **Bearer token only.** `Authorization: Bearer <token>` on every request
  (`components.securitySchemes.bearer_auth: { type: http, scheme: bearer }`,
  global `security: [{ bearer_auth: [] }]`).
- The only credential type today is a **Personal Access Token (PAT)**, obtained in
  the Up app (swipe right → Data sharing → Personal Access Token → Generate) or at
  <https://api.up.com.au/>. Token lifetime is chosen at generation time.
- Docs, verbatim: *"Only one personal access token can be used at a time."*
  Generating a new one or revoking invalidates the previous. No OAuth, no refresh
  tokens, no per-scope tokens in v1 (beta).
- Token is full-access to that user's data and "highly sensitive" — store
  encrypted, treat rotation as user-initiated only.
- Missing / malformed / invalid `Authorization` ⇒ `401` error response
  (`ErrorResponse` / `ErrorObject`).

### `GET /util/ping`

- Purpose (spec): *"Make a basic ping request to the API... to verify that
  authentication is functioning correctly. On authentication success an HTTP `200`
  status is returned. On failure an HTTP `401` error response is returned."*
- Response (`PingResponse`):

  ```json
  { "meta": { "id": "f8178615-7fd7-47a6-9a6e-cf62b3313848", "statusEmoji": "⚡️" } }
  ```

- Use it as the "is this stored PAT still valid?" health check before/around sync
  runs; a `401` here means the user must re-issue their token.

---

## Implications for Eunomia's transaction cache

1. **Primary freshness = webhooks.** Register one webhook per user PAT (or one
   global endpoint keyed by webhook id → user), verify `X-Up-Authenticity-Signature`
   (SHA-256 HMAC of raw body, constant-time compare), ack within 30s, enqueue the
   `transaction.links.related` fetch. Dedupe on event `data.id`.
2. **Handle all four cases:** `TRANSACTION_CREATED` (upsert), `TRANSACTION_SETTLED`
   (re-fetch, update status/`settledAt`/`amount`), `TRANSACTION_DELETED` (soft-
   delete the held row), and the rare `DELETED`+`CREATED` pair that substitutes for
   `SETTLED`.
3. **Backfill / reconciliation poll** (webhooks are at-least-once and can be
   missed): on a schedule, `GET /transactions?filter[since]=<last_createdAt>` walking
   `next`, plus a separate sweep of `filter[status]=HELD` to catch settle/delete
   transitions that `filter[since]` structurally cannot see (no `updatedAt`).
4. **Store `secretKey` at creation** — it is unrecoverable; rotation = create new +
   delete old.
5. **Respect `X-RateLimit-Remaining`**, exponential backoff + jitter on `429`. No
   published quota, so keep polling coarse and lean on webhooks.
6. **Accounts/categories/tags:** list-and-diff on a slow cadence; there is no
   push and no `updatedAt`.
7. **PAT health:** call `GET /util/ping` before a sync batch; on `401`, mark the
   connection as needing user re-auth.

## Source index

- Up API reference (auth, PAT, `util/ping`, pagination, `page[size]` <=100, `429` +
  `X-RateLimit-Remaining`, status codes): <https://developer.up.com.au/>
- Up OpenAPI v1 spec (all schemas, webhook event enum + payload, callback contract
  with 30s timeout + exponential backoff + HMAC verification steps + language
  examples, `filter[*]` params, `TransactionResource` field list confirming no
  `updatedAt`, `WebhookDeliveryStatusEnum`, 10-webhook limit, `secretKey`
  once-only): <https://raw.githubusercontent.com/up-banking/api/master/v1/openapi.json>
  (repo <https://github.com/up-banking/api>)
