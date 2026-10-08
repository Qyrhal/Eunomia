# Debugging a failure

The loop an agent (or you) follows from "it broke" to a failing test.

## 1. Get the trace id

Every error carries one. In the UI it is in the error toast. In API responses it is `trace_id` in the problem+json body and the `x-trace-id` header. MCP tool errors return it as `trace_id` in the tool result. It is 32 lowercase hex characters.

## 2. Look at the trace

Open SigNoz and filter `trace_id = <id>` (see [observability.md](observability.md)). The spans show the route, the registry tool, each store query and its timing. The failing span has the error. Without SigNoz, search the JSON logs for the same `trace_id`.

## 3. Read the error code

Look the `code` up in [errors.md](errors.md): it says what the code means, whether retrying helps, and which module emits it.

## 4. Read the capsule

When a route answers 5xx, or a registry tool fails with anything but `validation.invalid`, the backend stores a **failure capsule** under that trace id: the time, the user, the route or tool name, the arguments (see below), the error code, the raw error text and the backend version (`APP_VERSION`). **By default the arguments are not stored.** `CAPSULE_ARGS=off` (the default) keeps only a sha256 hash of the arguments and their JSON shape (every value replaced by its type name, for example `{"text": "string"}`), because the text of a failed memory write is user content and the control database is readable by the instance admin across users. A capsule like that has `args.args_stored: false`; `eunomia replay` refuses it with an explanation, and `replay --emit-test` still prints a test skeleton with the shape and a note that the real values must be filled in. Set `CAPSULE_ARGS=redacted` on the server to store the arguments for later failures so they can be replayed. Secrets are masked before storage when arguments are stored: any key containing `token`, `secret`, `password`, `key`, `authorization` or `cookie` becomes `***`, and `Bearer ...` strings and token-like runs of 40 or more characters are masked in every string. A capsule is capped at 16 KB (a larger one is stored cut and cannot be replayed). Capsules are kept for 7 days or the newest 1000, pruned by the `prune_capsules` job. With `CAPSULE_ARGS=redacted` the stored `args` can contain user content (for example the text of a memory being written). It sits in the `control` database for those 7 days and any instance admin can read it, so treat such a capsule as personal data and do not paste one into a public issue. The raw error text (`source`) is stored either way, with the text inside quotes masked (database errors echo record values) and tokens scrubbed.

Capsules live in the `control` database and carry the org of the failing request. Fetch one over HTTP as an instance admin (the first user, or an email in `EUNOMIA_ADMIN_EMAILS`, comma separated). An admin sees capsules of their own org; an email in `EUNOMIA_ADMIN_EMAILS` is an operator and sees every org's:

```bash
curl -H "Authorization: Bearer $TOKEN" https://your-host/api/debug/capsules/<trace_id>
```

The answer is a JSON array, oldest first: one capsule per failure under that trace id. A batch (an MCP batch, or a request that fails in several tool calls) can leave more than one, and each keeps its own arguments and error. A route that answers 5xx because of a tool failure that was already captured adds no second capsule.

`GET /api/debug/metrics` (same admin rule) returns `queries_without_org_context`, the count of queries that ran without an org context since the process started. It must be 0; a non-zero value is logged at error level at the moment it happens (`query without org context`).

## 5. Replay it

On the machine that has the database (same `SURREAL_*` environment as the backend):

```bash
eunomia-backend replay <trace_id> [--index N]
```

`--index N` (0-based, default 0) picks one capsule when the trace has several. The inside of the container is `docker compose exec backend eunomia-backend replay <trace_id>`. It reads the capsule from the `control` database and the capsule user's data from that user's org database (read only; it attaches without provisioning or moving anything), copies that data into a throwaway in-memory install, re-runs the same tool call or route as that user, and prints the original outcome next to the replayed one. Replay seeds only the personal vault of that user, so failures that depend on a shared vault or on `cache_record` data may not reproduce. Exit code 0 means the same error code came back, 1 means it did not (the data or the code differs from when it failed), 2 means it could not run.

Limits: the replay uses the user's personal vault only, so a call that names another vault will not find it. Calls that depend on outside services (LLM, connectors) fail or skip the same way they do without keys.

## 6. Write the failing test

```bash
eunomia-backend replay <trace_id> --emit-test > backend/tests/replay_<short>.rs
```

This prints a Rust integration test skeleton using the harness in `backend/tests/common/`. Seed the data the failure needs (the replay output shows the shape), then run `cargo test --test replay_<short>`. The skeleton asserts that the call no longer fails with the captured code, so it fails while the bug exists and passes once you fix it. Rename it for what it covers and keep it.

## 7. Health, readiness and proxy warnings

- `GET /healthz` is liveness: the process is up. It never touches the database.
- `GET /readyz` is readiness: it runs `RETURN 1` on the `control` database and checks the newest control migration is applied. Healthy answers `{"ok": true}`; otherwise 503 `application/problem+json` with code `internal` and a plain detail. Point a load balancer at this one. It is public and does not leave failure capsules.
- If a request from a private-range peer that is not in `TRUSTED_PROXIES` carries `X-Forwarded-For`, the backend logs a warning once per peer per hour: `X-Forwarded-For from a peer that is not in TRUSTED_PROXIES is ignored ...`. Every client behind that peer shares one rate-limit bucket until you add the peer (or its hostname) to `TRUSTED_PROXIES`. Next's rewrite proxy adds no `X-Forwarded-For` of its own (it forwards the header it received and sets `X-Forwarded-Host`), so behind Caddy the header is what Caddy sent. If the chain you see ends with Caddy's address, list Caddy in `TRUSTED_PROXIES` too, or it is taken for the client.

## 8. The export

`GET /api/export` downloads one JSON document of the caller's **personal vault**: the six entity tables (`person`, `organisation`, `location`, `repository`, `file`, `symbol`), their `memory` and `relates_to` rows, and the caller's `chat_message` history. Its `scope` key says the same and lists what is left out: other vaults, `cache_record`, connectors and settings, `audit_log`. It runs one query per table and builds the document in memory, so a very large vault makes a large response.
