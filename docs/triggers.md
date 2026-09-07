# Trigger firing path (#49)

How Eunomia pushes events back to Hermes, and how the built-in reminder/digest
triggers are turned on.

## The path

```
event source                     dispatcher                        delivery
─────────────                    ──────────                        ────────
record ingested      →  triggers.rules.evaluate_record ─┐
task/record anchor   →  triggers.scheduler.plan_schedules─┤→ triggers.delivery.fire
(cron expression due) →  triggers.scheduler.tick_crons ───┘   POST {hermes_webhook_url}/{webhook_route}
                                                             HMAC-SHA256 signed (V2 scheme)
```

- `plan_schedules` runs **hourly** on the worker (`manage.py run_worker`). A
  `schedule` trigger fires when its `anchor + offset_s` landed in the last hour:
  `spec = {anchor: "task.due_at" | "task.remind_at" | "record.occurred_at",
  filter: {...}, offset_s: int}`. `task_due_soon` uses `offset_s: -86400`
  (fire 24 h before the due time).
- `tick_crons` runs every **5 minutes** and re-reads the enabled cron triggers
  from the DB each tick, so enabling/disabling a cron trigger via
  `update_trigger` takes effect on the next tick — no worker restart. (Before
  #49 cron jobs were registered once at worker start and a newly enabled cron
  trigger stayed dead until restart.) Crons with sub-5-minute periods fire at
  most once per tick.
- `fire()` POSTs JSON to `{hermes_webhook_url}/{webhook_route}` with headers
  `X-Webhook-Timestamp` (unix seconds) and
  `X-Webhook-Signature-V2 = hex(HMAC_SHA256(hermes_webhook_secret, "{ts}.{body}"))`.
  Hermes verifies this on its gateway webhook adapter
  (`:8644/webhooks/<route>`, shared secret from AppSettings). Delivery is
  deduped per `(trigger, entity)` within `dedupe_window_s`, retried (0s/3s/10s),
  and dead-lettered into the `DeliveryLog` when the host is unreachable.

## Enabling the built-ins

`task_due_soon`, `daily_digest`, `big_transaction`, and `vip_email` are seeded
(post-migrate) **disabled**, with the default route `eunomia` attached. To turn
the reminder/digest pair on:

```jsonc
// over MCP / the tool API
{"tool": "enable_builtins", "args": {}}                     // route defaults to "eunomia"
{"tool": "enable_builtins", "args": {"webhook_route": ""}}  // deliver to bare hermes_webhook_url
```

`enable_builtins` is idempotent: it never disables anything, and it never
clobbers a route you set (an explicitly passed `webhook_route` does override).
Equivalent fine-grained control via `update_trigger`:
`{key: "task_due_soon", enabled: true}`.

`hermes_webhook_url` / `hermes_webhook_secret` come from AppSettings
(frontend Settings page or `connectors` API). Without a URL every delivery
dead-letters with detail `hermes_webhook_url not set`.

## Digest payload spec (`daily_digest`)

Cron triggers with `spec.digest == true` (or `spec.payload.kind == "digest"`)
get a live digest payload instead of the static spec payload:

```jsonc
{
  "kind": "digest",
  "generated_at": "2026-09-07T21:00:00+00:00",
  "open_total": 12,                 // all not-completed tasks
  "overdue_count": 3,               // open + due_at < now
  "due_today": [                    // open tasks with due_at inside the local day
    {"id": "task:<uuid>", "title": "pay rent", "project": "Home",
     "priority": 2, "flagged": false,
     "due_at": "2026-09-08T09:00:00+00:00", "remind_at": null}
  ],
  "reminders_today": [ /* same shape, remind_at inside the local day */ ],
  "by_project": [ {"project": "Home", "open_count": 5, "overdue_count": 2} ]
}
```

Lists are capped at 50 items each; counts are always exact. Day boundaries use
the server's local timezone (`TIME_ZONE`, UTC by default).

## Testing without Hermes

`test_trigger {key}` fires a synthetic `test:` event through the real delivery
path (useful with a local listener). `delivery_log {key, limit}` shows every
attempt, including dead letters.
