# Hermes-agent integration: tool transport + notification channel

Research resolution for [Qyrhal/Eunomia#2](https://github.com/Qyrhal/Eunomia/issues/2).
Grounded in the `NousResearch/hermes-agent` repo and its docs at
`hermes-agent.nousresearch.com/docs` (fetched 2026-09-02). All non-obvious claims cite a
source URL or repo file path.

---

## TL;DR recommendation

- **Expose Eunomia's tools as an MCP server over streamable HTTP.** Hermes is a first-class
  MCP *client* that speaks both stdio and remote HTTP, auto-discovers tools at startup, and
  hot-reloads them on `notifications/tools/list_changed`. This is the cleanest and
  best-supported contract. Auth: static `Authorization: Bearer` header (simplest) or OAuth 2.1.
- **Deliver notifications by POSTing to Hermes's inbound webhook adapter.** Hermes has **no
  generic "subscribe to my event stream" push channel**. The supported inbound direction is:
  an external system POSTs JSON to an HTTP endpoint the Hermes **gateway** exposes
  (`POST http://<host>:8644/webhooks/<route>`), authenticated with an HMAC-SHA256 signature.
  Use `deliver_only: true` routes for plain notifications (zero LLM cost) or default routes to
  wake an agent run.
- Confidence: **high** for both. The docs are unusually detailed (full config reference,
  security model, response codes, source file references). Remaining uncertainty is noted inline.

---

## 1. Tool transport Hermes expects

**Hermes is an MCP client.** MCP is explicitly "usually the cleanest way" to give Hermes a
tool that lives outside its own codebase.
Source: <https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp>

Supported MCP transports (same `mcp_servers` block in `~/.hermes/config.yaml`):

| Kind | Config shape | Notes |
|---|---|---|
| **stdio** | `command`, `args`, `env` | local subprocess, stdin/stdout |
| **HTTP** (remote) | `url`, `headers` | `url: "https://mcp.example.com/mcp"`; `headers: { Authorization: "Bearer ***" }` |
| **HTTP + OAuth 2.1** | `url`, `auth: oauth`, optional `oauth: { client_id, client_secret, redirect_uri, ... }` | Hermes does discovery, PKCE, token exchange/refresh, DCR or Client-ID-Metadata-Document, via the MCP Python SDK. Tokens cached at `~/.hermes/mcp-tokens/<server>.json`. |

Source: "Two kinds of MCP servers" / "HTTP servers" / "OAuth-authenticated HTTP servers" in
<https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp>

Extra HTTP-server knobs relevant to Eunomia (a multi-user Django backend):

- `identity_header: { name: "X-User-Id", value_from: static|profile, value: ... }` — sent on
  every request so a multi-tenant server can attribute the caller.
- `client_cert` / `client_key` — mutual TLS.
- `tools.include` / `tools.exclude` (exact names or globs) — per-server tool allowlisting.
- `timeout`, `connect_timeout`, `supports_parallel_tool_calls`.
- MCP **sampling** (`sampling/createMessage`) and **elicitation** (`elicitation/create`) are
  supported and on by default — an Eunomia MCP server can ask Hermes for an LLM completion or
  for structured user input mid-tool-call.

Source: "Basic configuration reference", "Per-user identity header", "mTLS / client
certificates", "MCP Sampling Support", "MCP Elicitation Support" —
<https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp>

**Tool registration mechanics:**

- Discovery is automatic at Hermes startup; MCP tools land in the normal tool registry named
  `mcp_<server_name>_<tool_name>` (e.g. `mcp_eunomia_create_task`).
- Runtime changes: an MCP server can send `notifications/tools/list_changed` and Hermes
  re-fetches the tool list with no restart. (`prompts/list_changed` / `resources/list_changed`
  are received but not yet acted on.)
- Config edits require `/reload-mcp` (slash command) or the `reload.mcp` TUI-gateway RPC;
  the CLI also auto-reloads MCP connections with a 30s timeout when `config.yaml` changes.
- If `tools.*` filters remove every callable tool, Hermes drops the server's toolset entirely.

Source: "How Hermes registers MCP tools", "Runtime behavior" / "Dynamic Tool Discovery",
"Reloading" — <https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp>;
`reload.mcp` in <https://hermes-agent.nousresearch.com/docs/developer-guide/programmatic-integration>

**Alternatives considered and rejected as primary:**

- *Native/built-in tool* (`tools/*.py` + `toolsets.py`, `registry.register(...)`): requires
  forking Hermes core. Not viable for a separate service.
  Source: <https://hermes-agent.nousresearch.com/docs/developer-guide/adding-tools>
- *Hermes plugin* (Python package / dropped-in dir): runs in-process inside Hermes; wrong
  boundary for a network service.
- *OpenAI-compatible API server* (`/v1/chat/completions`, `/v1/runs`): that's for driving
  Hermes as a chat backend, not for contributing tools.
  Source: "OpenAI-Compatible API Server" —
  <https://hermes-agent.nousresearch.com/docs/developer-guide/programmatic-integration>
- `hermes mcp serve` makes Hermes *be* an MCP server, but **stdio-only today** and it only
  exposes Hermes's own messaging tools — not a path for Eunomia to expose anything.
  Source: "Running Hermes as an MCP server" / "Current limits" —
  <https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp>

**Verdict:** MCP over streamable HTTP is the right primary contract for Eunomia to expose.
Start with a static bearer token; add OAuth later if multi-user auth demands it.

---

## 2. Inbound push / event / notification channel

**There is no generic inbound event stream or subscription API.** Hermes does not expose a
"register a listener / push me arbitrary events" socket. What it exposes for *inbound* traffic:

### a) Webhook adapter (the intended path for "external system notifies Hermes")

Part of the messaging **gateway** (`gateway/platforms/webhook.py`). Runs an HTTP server
(default port **8644**, `WEBHOOK_PORT`), enabled with `WEBHOOK_ENABLED=true` +
`WEBHOOK_SECRET=...` or `hermes gateway setup`.

- Endpoint: `POST http://<host>:8644/webhooks/<route-name>`
  (multi-profile: `POST /p/<profile>/webhooks/<route-name>`)
- Health: `GET /health` → `{"status": "ok", "platform": "webhook"}`
- Routes are named entries under `platforms.webhook.extra.routes` in `config.yaml`, **or**
  created at runtime with `hermes webhook subscribe <name> --deliver ... --prompt ...`
  (stored in `~/.hermes/webhook_subscriptions.json`, hot-reloaded per request).
- Two modes per route:
  - **default**: payload → prompt template → agent run → response delivered somewhere
    (`deliver: telegram|discord|slack|github_comment|log|...`).
  - **`deliver_only: true`**: the rendered `prompt` template *is* the literal message; no LLM,
    sub-second, still HMAC-authed/rate-limited/idempotent. This is the "plain notification"
    mode and is exactly what Eunomia wants for e.g. "task X is due" pings. `deliver` must be a
    real target (not `log`).
- Prompt templating: `{dot.notation}` into the JSON body, plus `{__raw__}` (whole payload as
  indented JSON, truncated 4000 chars). Missing keys left as literal `{key}`.
- Response is **synchronous**: `200` once delivered, `502` if the downstream target rejected —
  so the caller can retry intelligently.

Source: <https://hermes-agent.nousresearch.com/docs/user-guide/messaging/webhooks/>
(and `gateway/platforms/webhook.py`, `gateway/platforms/msgraph_webhook.py` listed in
<https://hermes-agent.nousresearch.com/docs/developer-guide/gateway-internals>)

The docs' own "When to use direct delivery" list includes *"Inter-agent pings — Agent A
notifies Agent B's user that a long-running task finished"* and *"External service push —
Supabase/Firebase webhook fires on a database change → notify a user"* — i.e. the Eunomia use
case is a named, supported pattern.
Source: "Direct Delivery Mode" — same page.

### b) OpenAI-compatible API server (drives conversations, HTTP + SSE)

`gateway/platforms/api_server.py`. Endpoints include `POST /v1/chat/completions` (SSE),
`POST /v1/responses`, `POST /v1/runs` (→ `run_id`, `202`), `GET /v1/runs/{id}/events` (SSE
stream of lifecycle events), `POST /v1/runs/{id}/steer|stop|approval`.
Headers: `X-Hermes-Session-Id`, `X-Hermes-Session-Key`.
This is a way to *start/steer an agent run over HTTP and stream events back*, not a
notification sink. Overkill for notifications; relevant only if Eunomia later wants to fully
drive Hermes.
Source: "OpenAI-Compatible API Server" —
<https://hermes-agent.nousresearch.com/docs/developer-guide/programmatic-integration>

### c) Local-only delivery helpers (not usable from a separate host)

`hermes send <target>` CLI and cron `deliver:` targets route messages to the gateway's
configured platforms / "home channel". These run on the gateway host and are not a remote API.
Note: cron/background deliveries are **not** mirrored into gateway session history (deliberate,
to avoid message-alternation violations).
Source: "Delivery Path" — <https://hermes-agent.nousresearch.com/docs/developer-guide/gateway-internals>

### d) Relay connector (experimental, wrong direction)

`gateway/relay/` dials *out* over a WebSocket to a connector when `GATEWAY_RELAY_URL` is set;
the connector then feeds `inbound` frames. Contract:
`docs/relay-connector-contract.md` in the repo. Experimental; requires Eunomia to host a
WebSocket server Hermes connects to. Not recommended unless a persistent bidirectional channel
becomes a hard requirement.
Source: "Platform Adapters" — <https://hermes-agent.nousresearch.com/docs/developer-guide/gateway-internals>

### e) Outbound event hooks (the opposite direction, for completeness)

Hermes → world. "Outbound webhooks are the push-side mirror of the inbound webhook platform."
Not relevant to Eunomia notifying Hermes.
Source: <https://hermes-agent.nousresearch.com/docs/user-guide/features/hooks/>

**Verdict:** POST to the Hermes webhook adapter. Use `deliver_only: true` routes for
notifications; use a default (agent-run) route only when Eunomia wants Hermes to *act* on the
event.

---

## 3. Payload shape + auth for the inbound webhook channel

**Transport:** `POST` with a JSON body. Route is selected by the URL path segment
(`/webhooks/<route-name>`), not by body content.

**Body:** arbitrary JSON chosen by the sender. Hermes renders it through the route's `prompt`
template (`{dot.notation}`, `{__raw__}`); with no template the whole payload is dumped as JSON
into the prompt. Business fields in the body are treated as **untrusted** regardless of a valid
signature. Optional `filters` (declarative operators: `equals`, `contains`, `in`, `in_file`,
`regex`, `all`/`any`/`not`) and `script` (stdin JSON → stdout) run before the agent.

**Event type** is read from `X-GitHub-Event`, `X-GitLab-Event`, or an `event_type` field in
the body; matched against the route's optional `events` list.

**Auth — HMAC-SHA256, one of:**

| Scheme | Headers | Computation |
|---|---|---|
| **Generic V2 (recommended)** | `X-Webhook-Signature-V2`, `X-Webhook-Timestamp` | hex HMAC-SHA256 of `"<timestamp>.<body>"`; timestamp is Unix seconds, must be within **±300 s** of server clock (replay protection) |
| Generic V1 (legacy) | `X-Webhook-Signature` | hex HMAC-SHA256 of the raw body only; no replay protection; logs a deprecation warning |
| GitHub-style | `X-Hub-Signature-256` | `sha256=` + hex HMAC-SHA256 of body |
| GitLab-style | `X-Gitlab-Token` | plain secret string equality (not HMAC) |
| Svix-style | `svix-id`/`webhook-id`, `svix-timestamp`, `svix-signature` | base64 HMAC over `"<id>.<timestamp>.<body>"`; used by AgentMail |

Secret source: per-route `secret:` (falls back to global `WEBHOOK_SECRET`). A route with **no**
secret makes the adapter fail at startup. `secret: "INSECURE_NO_AUTH"` disables validation but
is only accepted on a loopback bind. If a secret is set but no recognized signature header is
present → rejected.

Source: "Security" / "HMAC signature validation" —
<https://hermes-agent.nousresearch.com/docs/user-guide/messaging/webhooks/>;
V2/V1/Svix branches confirmed in `gateway/platforms/webhook.py`
(`_validate_svix_signature`, `X-Webhook-Signature-V2` handling).

**Other request constraints:**

- Body size: **1 MB** default (`413` over limit); `max_body_bytes` configurable.
- Rate limit: **30 req/min per route** default (`429`); `rate_limit` configurable.
- Idempotency: `X-GitHub-Delivery` / `X-Request-ID` (timestamp fallback) cached **1 h**;
  duplicates return `200` with `{"status":"duplicate"}` and do not re-run.

**Response codes:** `200` delivered (`{"status":"delivered","route":...,"target":...,"delivery_id":...}`),
`200` duplicate, `401` bad/missing signature, `400` malformed JSON, `404` unknown route,
`413` too large, `429` rate-limited, `502` downstream target rejected.

Source: "Direct Delivery Mode" → "Response codes", "Rate limiting", "Idempotency", "Body size
limits" — <https://hermes-agent.nousresearch.com/docs/user-guide/messaging/webhooks/>

**Minimal route config Eunomia would ask a Hermes operator to add:**

```yaml
platforms:
  webhook:
    enabled: true
    extra:
      port: 8644
      routes:
        eunomia-notify:
          secret: "<shared-hmac-secret>"
          deliver_only: true
          deliver: "telegram"            # or discord/slack/...
          prompt: "{title}: {body}"      # dot-notation into Eunomia's JSON
          # deliver_extra: { chat_id: "{chat_id}" }   # else uses home channel
```

Eunomia then: `POST http://<gateway-host>:8644/webhooks/eunomia-notify` with headers
`X-Webhook-Signature-V2: <hex>` and `X-Webhook-Timestamp: <unix-seconds>`, body
`{"title": "...", "body": "..."}`.

---

## 4. Hermes + Honcho constraints on tool registration / event push

**Memory (Honcho) is a separate subsystem from tools/MCP and does not gate either.**
Provider tools (`honcho_profile`, `honcho_search`, `honcho_context`, `honcho_reasoning`,
`honcho_conclude`) are routed through `AIAgent._invoke_tool()` → `MemoryManager.handle_tool_call()`
→ `provider.handle_tool_call()`, *not* through the tool registry that MCP tools use. An Eunomia
MCP server is unaffected by whether Honcho is enabled.
Source: "Memory Provider Integration" —
<https://hermes-agent.nousresearch.com/docs/developer-guide/gateway-internals>;
<https://hermes-agent.nousresearch.com/docs/user-guide/features/memory-providers>

Constraints that *do* matter for Eunomia:

1. **Only one external memory provider active at a time** (built-in MEMORY.md/USER.md always
   also active). If Eunomia ever wanted to *be* Hermes's memory backend it would implement the
   `MemoryProvider` ABC (`agent/memory_provider.py`) as a plugin and displace whatever else is
   set — but that is a different integration than "expose tools + push notifications" and is
   not recommended here.
   Source: "Single Provider Rule" —
   <https://hermes-agent.nousresearch.com/docs/developer-guide/memory-provider-plugin>

2. **Per-message agent instances.** The gateway builds a fresh `AIAgent` per inbound message,
   keyed `agent:main:{platform}:{chat_type}:{chat_id}`. MCP tools are registered into that
   agent from the process-wide registry at startup; there is no per-request tool injection from
   outside. So Eunomia's tools must be discoverable when the gateway/agent starts (or announced
   later via `notifications/tools/list_changed`).
   Source: "Message Flow" / "Session Key Format" —
   <https://hermes-agent.nousresearch.com/docs/developer-guide/gateway-internals>

3. **Webhook-triggered runs get a constrained toolset by default**
   (`web_search`, `web_extract`, `vision_analyze`, `clarify` — **no `terminal`/`file`**),
   because payloads are untrusted. A route can widen this with a manual `toolsets: [...]` edit
   (not available via `hermes webhook subscribe`). If Eunomia expects Hermes to *use Eunomia's
   MCP tools in response to an Eunomia webhook*, the operator must grant the relevant toolset
   on that route.
   Source: "Per-route toolsets" / "Authenticated does not mean trusted" —
   <https://hermes-agent.nousresearch.com/docs/user-guide/messaging/webhooks/>

4. **`deliver_only` notifications never enter a conversation/transcript**, and cron/background
   deliveries are not mirrored into session history. A plain notification is fire-and-forget;
   it will not show up in Honcho's session context or Hermes's session search. If Eunomia needs
   the notification to become part of an ongoing conversation, use a default (agent-run) route
   instead of `deliver_only`.
   Source: "Delivery Path" —
   <https://hermes-agent.nousresearch.com/docs/developer-guide/gateway-internals>

5. **Honcho gateway identity mapping** (`pinUserPeer`, `userPeerAliases`, `runtimePeerPrefix`,
   `sessionStrategy`) only affects how the gateway attributes users/sessions to Honcho peers.
   A webhook route shows up as its own synthetic platform/session; nothing about it blocks tool
   or webhook registration. Minor, noted for completeness.
   Source: "Honcho" config table —
   <https://hermes-agent.nousresearch.com/docs/user-guide/features/memory-providers>

---

## Recommended Eunomia design

| Concern | Choice |
|---|---|
| **Tool transport** | Eunomia runs an MCP server over streamable HTTP. Hermes operator adds `mcp_servers.eunomia: { url: "https://<eunomia>/mcp", headers: { Authorization: "Bearer <token>" } }` to `config.yaml`. Announce catalog changes with `notifications/tools/list_changed`. Send `identity_header` if per-user attribution is needed. |
| **Notifications → Hermes** | Eunomia's trigger subsystem (#9) `POST`s JSON to the Hermes gateway webhook adapter at `/webhooks/eunomia-<route>`, signed with generic **V2** HMAC (`X-Webhook-Signature-V2` = hex HMAC-SHA256 of `"<ts>.<body>"`, `X-Webhook-Timestamp`). Use `deliver_only: true` routes for plain pushes; a default route when Hermes should act. |
| **Shared secret** | One HMAC secret per route, provisioned out of band, stored by Eunomia; keep body ≤ 1 MB; include an idempotency id header (`X-Request-ID`); handle `429`/`502` with backoff. |
| **Not recommended** | Implementing a Hermes memory provider, the experimental relay connector, or forking `tools/` for a native tool. |

### Open items / uncertainty

- The webhook adapter is bundled with the **gateway**; it only runs if the Hermes user runs
  `hermes gateway` with `WEBHOOK_ENABLED=true`. A CLI-only Hermes user has no inbound HTTP
  surface at all. Eunomia's onboarding docs must state this prerequisite.
- MCP streamable-HTTP vs. older HTTP+SSE: the docs say "remote HTTP MCP servers" and show
  `url` + `headers`; they don't pin the exact MCP HTTP sub-transport version. Build against the
  current MCP spec's streamable HTTP and test against a real Hermes install before committing.
- Exact JSON body shape is entirely Eunomia's choice; there is no schema Hermes imposes beyond
  "valid JSON, ≤ max_body_bytes". Design the payload around the route `prompt` templates you
  expect operators to write (stable top-level keys, avoid deep nesting > 2000 chars per field).
