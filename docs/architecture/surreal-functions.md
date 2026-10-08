# SurrealQL functions used by the backend

Scope: every SurrealQL query string in `backend/src`. Rust calls (for example `search::upsert` and `search::set_embedding` in `cache/search.rs`, or `chrono::Utc::now()`) are not counted. Counts are occurrences in SQL, found with grep on 2026-10-08. Comments are not counted.

## Functions

| Function | Occurrences | Files | Example (file:line) |
|---|---|---|---|
| `time::now()` | 48 | 10 | `backend/src/db.rs:19` |
| `string::lowercase()` | 8 | 4 | `backend/src/models_user.rs:65` |
| `string::contains()` | 3 | 2 | `backend/src/cache/search.rs:305` |
| `search::score()` | 2 | 2 | `backend/src/cache/search.rs:302` |
| `count()` (aggregate, `GROUP ALL`) | 10 | 7 | `backend/src/routers/audit.rs:69` |

Files for `time::now()`: `models_user.rs`, `db.rs`, `auth.rs`, `routers/settings.rs`, `connectors/service.rs`, `cache/search.rs`, `chat/service.rs`, `vaults/service.rs`, `entities/consolidate.rs`, `entities/service.rs`.

Files for `string::lowercase()`: `models_user.rs`, `sources/heypocket.rs` (2 calls on line 248), `vaults/service.rs`, `cache/search.rs`.

Files for `count()`: `routers/audit.rs`, `routers/auth.rs`, `routers/sources.rs`, `tools/generic.rs`, `tools/registry.rs`, `cache/search.rs`, `vaults/service.rs`.

## Related SQL syntax (operators, not functions)

- Full-text match `@1@`: `backend/src/cache/recall.rs:193`, `backend/src/cache/search.rs:303`.
- KNN `<|K|>`: `backend/src/cache/search.rs:345`.

## Function families

Families used: `time`, `string`, `search`, `count`. No `vector::`, `array::`, `math::`, `object::`, `type::`, `record::`, `rand::`, `crypto::`, `duration::`, `http::`, or `fn::` calls appear in SQL.

## Allow list for `surreal start`

Use this with `--deny-all`. The flag syntax is documented at https://surrealdb.com/docs/surrealdb/cli/start: `--allow-funcs` takes a comma-separated list, and each entry is `<family>` or `<family>::<name>`.

```
surreal start --deny-all --allow-funcs time,string,search,count [other flags]
```

Or with the env var: `SURREAL_CAPS_ALLOW_FUNC=time,string,search,count`.

Checks before you rely on it:
- The docs say a bare family name (`http`) and a wildcard (`http::*`) both include the whole family. Test that `count` alone covers `count()`. If not, add `count::*`.
- `DEFINE FIELD ... DEFAULT time::now()` in `db.rs` runs at write time, so the allow list must cover it. Test on a throwaway container.
- `docker-compose.yml` pins SurrealDB `v2.3`. These flags come from the 3.x docs. Confirm they exist in the version you run.
