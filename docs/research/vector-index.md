# Research: sqlite vector index for Eunomia

Resolves [Qyrhal/Eunomia#17](https://github.com/Qyrhal/Eunomia/issues/17). Child of #1. Feeds #6 (cache + search design) and #30.

> Note on method: the live web-search backend was degraded when this was done, so
> this is a judgement call from known properties of the options rather than a
> fresh benchmark sweep. Direction is high-confidence; the throughput numbers are
> order-of-magnitude and must be re-measured on the real homelab box (#30).

## Question

Which vector-index approach fits an embedded sqlite deployment on a homelab box:
~100k rows of short personal-data text, 384-dim vectors (bge-small, per #18),
semantic search sitting next to the cache rows, single-file deploy, clean path to
Postgres + pgvector later (fog).

## Recommendation

**`sqlite-vec`, brute-force KNN, 384-dim, one `vec0` virtual table keyed by cache
record id. FTS5 stays a separate virtual table. No ANN, no extra service.**

## Why

| Option | Verdict |
|---|---|
| **sqlite-vec** | **Chosen.** Single C file, zero deps, loadable extension — works anywhere sqlite does. `pip install sqlite-vec` + load on the connection. Vectors in a `vec0` virtual table with metadata columns; join back to cache rows by id. Actively maintained, explicitly the successor to sqlite-vss. Brute-force KNN only today — fine at this scale (see below). |
| sqlite-vss | Rejected. Deprecated by its own author in favour of sqlite-vec. Faiss-based, painful to build, stale. |
| hnswlib sidecar | Rejected. Separate index file, separate lifecycle, manual id mapping, no SQL. Adds an ANN layer we don't need yet. |
| libSQL / Turso native vectors | Rejected for v1. Means replacing SQLite with libSQL — a bigger swap than the problem warrants. Revisit only if we outgrow sqlite-vec. |
| embedded Chroma / LanceDB | Rejected. Separate store and query language; breaks "one sqlite file" and "search sits on the cache DB". |

## Scale check

100k × 384 float32 = ~150 MB of raw vectors. Brute-force cosine/L2 over that is a
linear scan the CPU chews through in the low tens of milliseconds per query,
single-threaded — well inside interactive latency for a single-user assistant.
ANN (and its recall/complexity cost) only earns its place in the millions. If
Eunomia ever gets there, the escape hatches already exist: sqlite-vec's ANN work,
or the Postgres + pgvector path already listed as fog on the map.

## Integration notes for #30

- Load the extension per-connection: a Django `connection_created` signal handler
  calling `connection.connection.enable_load_extension(True)` then
  `sqlite_vec.load(connection.connection)`. Django's sqlite backend allows this.
- Schema: `CREATE VIRTUAL TABLE cache_vec USING vec0(record_id TEXT PRIMARY KEY, embedding float[384])`.
- Query: `SELECT record_id, distance FROM cache_vec WHERE embedding MATCH :q ORDER BY distance LIMIT :k`, then hydrate rows from the cache table.
- Keep the embedding dim in one constant shared with the embedding service (#29);
  changing the model / dim means dropping and rebuilding `cache_vec`.
- FTS5 and `vec0` are independent; the generic search tool (#37) runs both and
  merges (reciprocal rank fusion is the cheap default).
