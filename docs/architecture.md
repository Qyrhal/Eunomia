# How Eunomia works

Vaults are Eunomia's core memory boundary. Each vault contains its own
entities, facts, observations and relationships, with access checked against
active membership. Agents use one tool registry to work in these vaults and
recall their knowledge alongside the caller's own synced app records. The
architecture below describes the implementation in this repository.

![Eunomia component architecture](diagrams/architecture.svg)

[Interactive version](diagrams/architecture.html) — download and open the
HTML locally to explore components, switch themes and export the diagram.

## Vaults define the memory boundary

Every user has a personal vault. Project/team vaults use kind `org`, with
owner and member roles. Active membership is checked on reads and writes;
a pending invitation gives no access. Relations and entity merges stay
inside their vault.

By default, `recall`, `reflect` and `entities_search` read the caller's personal
vault plus the org vaults they belong to. Another user's personal vault must
be requested explicitly, even when shared. Other tools default to the caller's
personal vault. Raw connector records remain owner-scoped, so vault membership
does not grant access to another user's synced records.

Vault names resolve only among the caller's accessible vaults. An entity's
name is unique per kind within its vault, so the same name in another vault
represents a separate entity. Entity relations and merges cannot cross that
boundary.

Cloning copies entities, memories and relations into a new vault owned by the
caller. Merging also creates a new vault: matching entity kinds and names are
folded together, aliases are combined, and duplicate facts and relations are
removed. Both input vaults retain their contents. Memberships are not copied
into the new vault; sharing it is a separate decision.

Sources: [membership, default scopes, clone and merge](../backend/src/vaults/service.rs),
[vault name resolution](../backend/src/tools/registry.rs),
[vault isolation tests](../frontend/tests/vault-isolation.spec.ts).

## One backend, several clients

The Next.js frontend proxies `/api/*` and `/mcp` to the Rust/Axum backend.
Browser requests authenticate with a signed session cookie. MCP requests
require a personal bearer token; a browser cookie is not accepted there.
REST tool calls can also use bearer tokens.

REST (`POST /api/tools/:name`), MCP (`/mcp`) and the built-in chat agent
execute the same registered tools. The registry resolves vault names,
checks access, and logs mutating calls. Chat streams its conversation and
tool activity over SSE. MCP is stateless Streamable HTTP: JSON-RPC POST
requests receive JSON replies, with no server-initiated stream.

Sources: [frontend proxy](../frontend/next.config.ts),
[backend assembly](../backend/src/main.rs),
[authentication](../backend/src/auth.rs),
[tool registry](../backend/src/tools/registry.rs),
[MCP transport](../backend/src/routers/mcp.rs),
[chat loop](../backend/src/chat/service.rs).

## From an app record to useful memory

```mermaid
flowchart TD
    A[Scheduled or manual sync] --> B[Fetch with saved cursor]
    W[Verified Up Bank webhook] --> C[Map to common envelopes]
    B --> C
    C --> D[Upsert owner-scoped records and links]
    D --> E[Embed live records when available]
    E --> F[Extract entities and facts from changed records]
    F --> G[Consolidate touched entities into observations]
```

Each source implements a common adapter interface. The registry contains
Up Bank, HeyPocket, GitHub, Slack, Notion, Linear, Gmail, Google Calendar,
Discord, Spotify, Todoist and Stripe. The demo adapter is resolved on demand
and is excluded from scheduled sources.

A background loop wakes every minute. It checks enabled connectors for each
user and syncs sources whose interval has elapsed. Defaults are 15 minutes,
except HeyPocket at 24 hours; per-user settings can override them. Failed
syncs use backoff. An in-process guard and a database lease prevent overlapping
manual and scheduled runs for the same user/source.

The pipeline maps raw data to `Envelope` values and idempotently upserts
`cache_record` rows and `linked_to` relations. Mapping or storage failures
are recorded while the rest of the batch continues; a partially failed run
keeps its cursor for retry. Embeddings and extraction are best-effort
additions to successfully stored records. A live record without an embedding
can retry embedding when replayed unchanged. Entity extraction runs on changed
records when a chat model is available; consolidation runs once per batch
for the affected subjects.

Sources: [adapter contract](../backend/src/sources/base.rs),
[source registry](../backend/src/sources/registry.rs),
[scheduler and leases](../backend/src/sources/scheduler.rs),
[ingestion](../backend/src/cache/ingest.rs),
[webhook verification and dispatch](../backend/src/routers/sources.rs).

## What is stored

| Layer | Storage | Meaning |
|---|---|---|
| Synced records | `cache_record`, `linked_to` | Owner-scoped app data, text, payloads, dates, vectors and record links |
| Entities | `person`, `organisation`, `location`, `repository`, `file`, `symbol` | Vault-scoped subjects with names, aliases and summaries |
| Memories | `memory` | Facts (`world`), events (`experience`) and consolidated beliefs (`observation`) |
| Entity relations | `relates_to` | Labelled links such as `works_at`, `imports` or `calls` |
| Access and operations | Users, tokens, sessions, vault membership, connectors, sync status, audit log | Identity, permissions, encrypted credentials and operational state |

SurrealDB holds graph relations, BM25 full-text indexes and a cosine MTREE
vector index in the same schema. Compose pins SurrealDB 2.3 because this
schema uses the 2.x MTREE syntax. The embedding index expects 1,536 dimensions;
the embedding service rejects wrong-sized or non-finite vectors.

Code entities are created by agent calls to `code_entity_upsert` and linked
with `code_relate`. The code graph reflects what agents record about a
codebase; it is not an automatic static-analysis index.

Sources: [schema and indexes](../backend/src/db.rs),
[record operations](../backend/src/cache/search.rs),
[embedding validation and cache](../backend/src/embeddings/service.rs),
[entity tools](../backend/src/entities/tools.rs).

## How recall ranks context

```mermaid
flowchart TD
    Q[Query and permitted vaults] --> S[Semantic: synced record vectors]
    Q --> K[Keyword: synced record text]
    Q --> G[Graph: named entities and their facts]
    Q --> T[Temporal: explicit date range]
    Q --> M[Memory text: remembered facts]
    S --> R[Reciprocal Rank Fusion]
    K --> R
    G --> R
    T --> R
    M --> R
    R --> H[Hydrate and recheck scope / freshness]
    H --> B[Boost by recency and retrieval agreement]
    B --> O[Sort, limit and fit optional token budget]
```

The semantic arm runs concurrently with the local database work. The four
database arms execute sequentially to avoid measured database contention.
Every arm has a four-second deadline. Timeouts yield no candidates for that
arm; a semantic provider error also drops only the semantic arm. Other
database errors can still fail the request. Four seconds is an arm deadline,
not a total request deadline.

Semantic and record-keyword searches only include the caller's own records
alongside their personal vault. Memory-text search works across readable
vaults without embeddings. The graph arm finds exact entity names and aliases
from phrases in the query, follows their memories, and includes eligible
source records and linked neighbours. The temporal arm requires an explicit
`time_range`; it does not parse phrases such as “last week”.

Reciprocal Rank Fusion sums `1 / (60 + rank)` across the ranked candidate
lists, with rank starting at 1. Hydration rechecks record ownership and vault scope and excludes stale
observations and superseded facts. Agreement adds 5% for each additional arm
that found the same item. Recency decreases linearly from 1.0 to a floor of
0.5 over 365 days; undated items receive a neutral multiplier. Scores are
normalised relative to the best returned hit. “Proof” here means retrieval
agreement, not independent evidence that a fact is true.

Results retain their IDs, source, date, vault and number of matching arms.
An optional token budget estimates one token per four characters and trims
individual texts; clients can use the ID to fetch full content. There is no
cross-encoder reranking step.

Sources: [recall orchestration and ranking](../backend/src/cache/recall.rs),
[full-text, vector search and RRF](../backend/src/cache/search.rs).

## Keeping memories current

```mermaid
flowchart LR
    A[Fact added, edited or deleted] --> B[Observation marked stale]
    B --> C[Consolidate surviving facts]
    C --> D[Update one observation and its lineage]
    N[New fact contradicts an earlier fact] --> S[Earlier fact marked superseded]
    S --> R[Recall excludes superseded facts]
    B --> X[Recall excludes stale observations]
```

An observation is an evolving synthesis for one entity, with source-memory
lineage, proof count and versioning. Changing raw facts invalidates that
synthesis. Consolidation rebuilds it from surviving facts, and a configured
chat model can mark contradicted earlier facts as superseded. Those facts
remain inspectable through entity retrieval. Without a chat model,
contradiction detection does not run.

A source tombstone hides the raw record and its record links from retrieval.
Facts previously derived from that record remain independent assertions;
forgetting them requires memory deletion. The explicit “delete source data”
operation also removes facts and relations derived from that source.

Sources: [consolidation](../backend/src/entities/consolidate.rs),
[supersession](../backend/src/entities/supersede.rs),
[memory edits and deletion](../backend/src/entities/service.rs),
[source data deletion](../backend/src/sources/registry.rs).

## Model boundaries

| Capability | Without a server model | With a suitable configured endpoint |
|---|---|---|
| Agent memory writes, vaults and graphs | Available | Available |
| Raw connector sync and text/graph recall | Available with source credentials | Available |
| Semantic record retrieval | Unavailable | Requires an embeddings endpoint with 1,536-dimensional output |
| Automatic entity extraction and consolidation | Unavailable | Requires chat completions |
| `reflect` | Returns recalled context and instructions for the calling agent | Synthesises an answer with citations; falls back to context if synthesis fails |
| In-app chat | Unavailable | Requires chat completions and tool calling |
| Vector cloud | Lexical projection | Can use semantic embeddings |

Model providers are resolved per user. A server API key is used only for its
configured base URL; a user selecting a different URL supplies their own
credentials. Configuring a remote model means relevant record text and
memories can be sent to that endpoint for embedding or synthesis. Connector
secrets use AES-256-GCM at rest; this does not encrypt every database record.

Sources: [vault membership and defaults](../backend/src/vaults/service.rs),
[provider resolution](../backend/src/embeddings/provider.rs),
[reflection and fallback](../backend/src/cache/reflect.rs),
[vector cloud](../backend/src/cache/cloud.rs),
[credential encryption](../backend/src/connectors/crypto.rs).

## Where to start in the code

| Question | Entry point |
|---|---|
| How do vault access, cloning and merging work? | [`vaults/service.rs`](../backend/src/vaults/service.rs) |
| How does the server start? | [`main.rs`](../backend/src/main.rs) |
| How is the database defined? | [`db.rs`](../backend/src/db.rs) |
| How does a tool reach shared logic? | [`tools/registry.rs`](../backend/src/tools/registry.rs) |
| How does source data become memory? | [`cache/ingest.rs`](../backend/src/cache/ingest.rs) |
| How does a query find relevant context? | [`cache/recall.rs`](../backend/src/cache/recall.rs) |
| How do clients call the API? | [`frontend/src/lib/api.ts`](../frontend/src/lib/api.ts) |
| How is vault isolation exercised? | [`vault-isolation.spec.ts`](../frontend/tests/vault-isolation.spec.ts) |
| How is retrieval evaluated? | [`recall-eval.spec.ts`](../frontend/tests/recall-eval.spec.ts) |

For operational setup, use [Deployment](deployment.md). For the user-facing
memory model and edge cases, use [Concepts](concepts.md).
