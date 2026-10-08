# Debugging a failure

The loop an agent (or you) follows from "it broke" to a failing test.

## 1. Get the trace id

Every error carries one. In the UI it is in the error toast. In API responses it is `trace_id` in the problem+json body and the `x-trace-id` header. MCP tool errors return it as `trace_id` in the tool result. It is 32 lowercase hex characters.

## 2. Look at the trace

Open SigNoz and filter `trace_id = <id>` (see [observability.md](observability.md)). The spans show the route, the registry tool, each store query and its timing. The failing span has the error. Without SigNoz, search the JSON logs for the same `trace_id`.

## 3. Read the error code

Look the `code` up in [errors.md](errors.md): it says what the code means, whether retrying helps, and which module emits it.

## 4. Read the capsule

When a route answers 5xx, or a registry tool fails with anything but `validation.invalid`, the backend stores a **failure capsule** under that trace id: the time, the user, the route or tool name, the arguments, the error code, the raw error text and the backend version (`APP_VERSION`). Secrets are masked before storage: any key containing `token`, `secret`, `password`, `key`, `authorization` or `cookie` becomes `***`, and `Bearer ...` strings and token-like runs of 40 or more characters are masked in every string. A capsule is capped at 16 KB (a larger one is stored cut and cannot be replayed). Capsules are kept for 7 days or the newest 1000, pruned by the `prune_capsules` job.

Fetch one over HTTP as an instance admin (the first user, or an email in `EUNOMIA_ADMIN_EMAILS`, comma separated):

```bash
curl -H "Authorization: Bearer $TOKEN" https://your-host/api/debug/capsules/<trace_id>
```

## 5. Replay it

On the machine that has the database (same `SURREAL_*` environment as the backend):

```bash
eunomia-backend replay <trace_id>
```

The inside of the container is `docker compose exec backend eunomia-backend replay <trace_id>`. It reads the capsule and the capsule user's data from the real database (read only), copies that data into a throwaway in-memory database, re-runs the same tool call or route as that user, and prints the original outcome next to the replayed one. Exit code 0 means the same error code came back, 1 means it did not (the data or the code differs from when it failed), 2 means it could not run.

Limits: the replay uses the user's personal vault only, so a call that names another vault will not find it. Calls that depend on outside services (LLM, connectors) fail or skip the same way they do without keys.

## 6. Write the failing test

```bash
eunomia-backend replay <trace_id> --emit-test > backend/tests/replay_<short>.rs
```

This prints a Rust integration test skeleton using the harness in `backend/tests/common/`. Seed the data the failure needs (the replay output shows the shape), then run `cargo test --test replay_<short>`. The skeleton asserts that the call no longer fails with the captured code, so it fails while the bug exists and passes once you fix it. Rename it for what it covers and keep it.
