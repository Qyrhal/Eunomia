# RESEARCH: Local embedding model for the homelab

Resolves [Qyrhal/Eunomia#18](https://github.com/Qyrhal/Eunomia/issues/18). Child of #1. Feeds #24 (semantic index build) and #29.

## Question

Which local text-embedding model should be Eunomia's default on a homelab box
(CPU only, no GPU assumed), for semantic search over short personal-data text
(emails, transactions, calendar, tasks, docs)? Also: the fallback API path
(OpenAI-compatible) and what switching models costs.

Context from #1: vectors live in **sqlite + sqlite-vec**; the box is a
single-host tailscale-bound Django service; Hermes queries it over MCP.

## TL;DR recommendation

**Default: `BAAI/bge-small-en-v1.5` via `sentence-transformers`.**

- Best retrieval quality per parameter in the 384-dim class: MTEB (56-task)
  average **62.17**, MTEB Retrieval (15-task) **51.68** — beats gte-small,
  e5-small-v2, ada-002, and crushes all-MiniLM-L6-v2 (retrieval 41.95).
  [BAAI model card, Evaluation table]
- 384-dim vectors keep the sqlite-vec index small (half the bytes of a 768-dim
  model) and keep brute-force/ANN scans fast.
- 33M params, 512-token context (covers a whole email / transaction memo / task,
  and most calendar entries; long Drive docs get chunked anyway).
- **MIT licence**, commercial use free of charge. [BAAI model card, License]
- Packaging is one line: `pip install sentence-transformers`, then
  `SentenceTransformer("BAAI/bge-small-en-v1.5")`. Pure PyTorch, no
  `trust_remote_code`, no task-prefix ritual. First run downloads ~130 MB.

`thenlper/gte-small` is an almost-equivalent second choice (same 384-dim, MIT,
similar footprint, MTEB avg 61.36 / retrieval 49.46) — pick bge-small unless a
task-specific eval on real Eunomia data flips it. [gte-small model card, Metrics]

**Alternative if the box already runs Ollama for chat models:**
`nomic-embed-text` (`ollama pull nomic-embed-text` → serves v1.5). Higher
ceiling — 768-dim, 8192-token context for long docs, Apache-2.0 — but ~3-5x
slower per text on CPU, 2x the vector storage, and the API forces
`search_document:` / `search_query:` prefixes you must not forget. Worth it only
if long-context single-shot embedding (whole Drive files, no chunking) turns out
to matter.

**Do NOT default to `all-MiniLM-L6-v2`.** It is the fastest and lightest, but
retrieval quality is materially worse (MTEB Retrieval 41.95 vs bge-small's
51.68) and it silently truncates input past 256 word-pieces, which clips longer
emails and docs. Keep it as an optional "min-RAM / max-speed" profile only.
[all-MiniLM-L6-v2 model card, Intended uses]

## Comparison

MTEB numbers are the authors' own published runs from the BAAI and GTE model
cards (same 56-task MTEB English suite, so directly comparable). Retrieval is
the 15-task MTEB Retrieval average — the sub-score that matters most for
semantic search.

| Model | Params | Dim | Max tokens | MTEB avg (56) | MTEB Retrieval (15) | Licence |
|---|---|---|---|---|---|---|
| `BAAI/bge-small-en-v1.5` | 33M | **384** | 512 | **62.17** | **51.68** | MIT |
| `thenlper/gte-small` | 33M | 384 | 512 | 61.36 | 49.46 | MIT |
| `nomic-ai/nomic-embed-text-v1.5` | 137M | **768** (Matryoshka → 512/256/128/64) | **8192** | 62.28 @768d, 61.04 @256d | see note | Apache-2.0 |
| `sentence-transformers/all-MiniLM-L6-v2` | 22M | 384 | **256** | 56.26 | 41.95 | Apache-2.0 |
| `text-embedding-3-small` (API fallback) | – | 1536 (reducible via `dimensions`) | 8192 | 62.3% (OpenAI's MTEB figure) | – | proprietary API |

Sources: bge-small — <https://huggingface.co/BAAI/bge-small-en-v1.5> (Evaluation
+ License). gte-small — <https://huggingface.co/thenlper/gte-small> (Metrics
table; `license:mit` in <https://huggingface.co/api/models/thenlper/gte-small>).
all-MiniLM-L6-v2 — <https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2>
(384-dim, 256-token truncation) and the MTEB rows in the bge/gte tables.
nomic v1.5 — <https://huggingface.co/nomic-ai/nomic-embed-text-v1.5> (Adjusting
Dimensionality table: 62.28 @768d → 56.10 @64d) and
<https://arxiv.org/abs/2402.01613> (Apache-2.0; "outperforms OpenAI Ada-002 and
text-embedding-3-small on short-context MTEB"). OpenAI —
<https://developers.openai.com/api/docs/guides/embeddings> (1536-dim default,
`dimensions` param, 62.3% MTEB).

> Note on nomic v1.5 retrieval: the model card publishes only the MTEB *average*
> per dimension, not a broken-out Retrieval sub-score, so the table above leaves
> that cell as a pointer. Nomic's own framing is parity-to-better vs
> text-embedding-3-small on short-context MTEB. [arxiv:2402.01613]

### Embedding dimensions

- **384** (bge-small, gte-small, all-MiniLM-L6-v2) — smallest sqlite-vec rows,
  fastest similarity scan, lowest disk. Best fit for "lots of short records".
- **768** (nomic v1.5 default) — 2x the storage and scan cost of 384. nomic's
  Matryoshka training lets you truncate to 512/256/128/64 at index-build time
  with a documented, gentle quality drop (62.28 → 61.96 → 61.04 → 59.34 →
  56.10). Truncating still means re-embedding the corpus; it is not a runtime
  toggle.
- **1536** (text-embedding-3-small default) — API only; use the `dimensions`
  request param to emit 512/768 and stay economical in sqlite-vec.

### CPU throughput (rough — benchmark on the actual box before trusting)

No first-party CPU texts/sec numbers are published for these models, so this is
an architecture-based estimate, not a measurement. Order, from the param/layer
counts and SBERT's own "all-MiniLM-L6-v2 is 5x faster than all-mpnet-base-v2"
statement (<https://sbert.net/docs/sentence_transformer/pretrained_models.html>):

- `all-MiniLM-L6-v2` (22M, 6 layers) — fastest. Ballpark a few hundred short
  texts/sec on one modern x86 core batched, ~1-2k/sec across a multi-core box.
- `bge-small-en-v1.5` / `gte-small` (33M, 12 layers) — roughly 2x slower than
  MiniLM-L6. Ballpark 100-400 short texts/sec per core.
- `nomic-embed-text-v1.5` (137M) — roughly 3-5x slower than bge-small on CPU.
  Ballpark 20-80 short texts/sec per core (Ollama uses a quantised GGUF, which
  claws some of that back).

For Eunomia's workload (incremental indexing of new emails/transactions/tasks as
connectors poll, plus one-time backfill of the existing corpus) any of these is
fine — the backfill of a personal mailbox is minutes-to-low-hours on CPU, and
steady-state is a trickle. **Ponytail: leave a batch-size / worker-count knob and
measure on the real box; do not hard-tune to these guesses.**

### RAM footprint (rough)

- `all-MiniLM-L6-v2`: ~90 MB weights (fp32), well under ~300 MB RSS with a
  PyTorch runtime.
- `bge-small-en-v1.5` / `gte-small`: ~65-135 MB weights (fp32), ~300-500 MB RSS
  with PyTorch loaded.
- `nomic-embed-text-v1.5`: ~550 MB weights (fp32) via sentence-transformers, or
  ~274 MB as the fp16 GGUF Ollama ships; ~0.6-1 GB RSS in practice. Ollama also
  keeps its own resident server process.

(Weights sizes: gte-small "Model Size 0.07 GB" and all-MiniLM-L6-v2 "0.09 GB"
from the GTE Metrics table; nomic Ollama tag is "274MB" at
<https://ollama.com/library/nomic-embed-text>.)

### Model size on disk

- bge-small-en-v1.5: ~130 MB download (fp32 safetensors).
- gte-small: ~70 MB.
- all-MiniLM-L6-v2: ~90 MB.
- nomic-embed-text-v1.5: ~550 MB via HF (fp32), 274 MB via `ollama pull`.

### Packaging effort

**`sentence-transformers` (bge-small / gte-small / all-MiniLM / nomic):**

```
uv add sentence-transformers      # pulls torch + transformers (~1-2 GB of wheels)
```

```python
from sentence_transformers import SentenceTransformer
model = SentenceTransformer("BAAI/bge-small-en-v1.5")   # ~130 MB on first run
vecs = model.encode(texts, normalize_embeddings=True, batch_size=64)
```

- In-process, no daemon, no extra port. Deterministic version pinning via uv.
- Cost: adds PyTorch to the Django image. Acceptable for a homelab box; it is the
  single biggest dependency Eunomia would take on.
- bge-small: no `trust_remote_code`, no prompt prefixes. gte-small: same.
  nomic v1.5: needs `search_document:` / `search_query:` prefixes and (on
  transformers < 5.5 / sentence-transformers < 5.3) `trust_remote_code=True`.

**`ollama pull` (nomic-embed-text, all-minilm, embeddinggemma, …):**

```
ollama pull nomic-embed-text
curl http://localhost:11434/api/embed -d '{"model":"nomic-embed-text","input":"..."}'
```

- No PyTorch in the Django image; Eunomia just makes HTTP calls. Ollama also
  exposes an OpenAI-compatible `/v1/embeddings`, so the same client code covers
  local-Ollama and the API fallback.
- Cost: a second always-on service to install, supervise, and keep updated. Only
  pays off if the box is *already* running Ollama for chat models.
- Ollama's small library does not carry `bge-small-en` or `gte-small` — only
  `bge-m3` (567M, overkill) and `all-minilm`. So "bge-small via Ollama" is not
  an option; that model only comes through sentence-transformers.

## Current Ollama-served embedding options (as of 2026)

From <https://ollama.com/search?c=embedding>:

| Model | Sizes | Notes |
|---|---|---|
| `qwen3-embedding` | 0.6b / 4b / 8b | strong MTEB, but 0.6b is already 4-8x nomic's size; CPU-heavy |
| `embeddinggemma` | 300m (622 MB) | Google, 768-dim w/ Matryoshka to 128, 2K context, Gemma licence |
| `nomic-embed-text` | 137m (274 MB) | v1.5; the sensible Ollama default here |
| `nomic-embed-text-v2-moe` | MoE | multilingual-focused |
| `mxbai-embed-large` | 335m | 1024-dim, heavier |
| `bge-m3` | 567m | multilingual/multi-function, large |
| `all-minilm` | 22m / 33m | same family as all-MiniLM-L6/L12-v2 |
| `snowflake-arctic-embed` / `-embed2` | 22m–568m | |
| `paraphrase-multilingual` | 278m | |
| `granite-embedding` | 30m / 278m | IBM, Apache-2.0 |

If Eunomia goes the Ollama route, `nomic-embed-text` is the pick (best
size/quality/licence balance); `embeddinggemma` is the runner-up but carries the
Gemma licence and is 2x the disk.

## Fallback API path (OpenAI-compatible embeddings)

Use the OpenAI `/v1/embeddings` shape as the pluggable remote backend:

```
POST {base_url}/v1/embeddings
{ "model": "text-embedding-3-small", "input": ["..."], "dimensions": 768 }
```

- **`text-embedding-3-small`** — 1536-dim default, MTEB 62.3% (OpenAI's figure),
  8192-token input, cheap (~62,500 pages/USD per OpenAI's own table).
  `text-embedding-3-large` (3072-dim, 64.6%) if quality outweighs cost.
  <https://developers.openai.com/api/docs/guides/embeddings>
- The `dimensions` param truncates 3-small/3-large to any smaller size
  (e.g. 768 or 512) server-side, so the API can be made to *match the local
  index dimension* — useful for a hybrid setup.
- The same OpenAI-compatible contract is served by Ollama (`/v1/embeddings`),
  LiteLLM, vLLM, text-embeddings-inference, and most hosted providers, so
  "OpenAI-compatible embeddings client + configurable `base_url`/`model`/
  `dimensions`" is the one abstraction to build. This also lines up with #1's
  "Not yet specified: Postgres + pgvector migration" — the client stays the same.

## Dimension implications for switching models

The vector dimension `N` is baked into the sqlite-vec virtual table
(`CREATE VIRTUAL TABLE ... USING vec0(embedding float[N])`) and into every
stored row. Consequences:

1. **Changing model = full reindex.** Different models produce different `N`
   *and* live in different vector spaces. A query vector from model A is
   meaningless against stored vectors from model B even when `N` is identical.
   You must drop/recreate the vec table and re-embed the entire corpus.
2. **Store provenance.** Persist `embedding_model` + `embedding_dim` (and ideally
   a normalization flag) next to each vector, so a model change is a detectable,
   scriptable migration rather than silent corruption.
3. **Matryoshka models don't dodge this.** nomic v1.5 / embeddinggemma /
   text-embedding-3-* can emit a smaller `N` without retraining, but choosing a
   new `N` still requires re-embedding every row — it is an index-build-time
   decision, not a runtime switch.
4. **Design implication for the default.** Picking 384-dim now (bge-small) and
   later moving to a 768-dim model is a one-time reindex migration. That is
   cheap for a personal-scale corpus (minutes-to-hours of CPU), so optimizing
   the *first* choice for storage/speed (384-dim bge-small) rather than
   future-proofing to 768 is the right call. Ship the reindex script alongside
   the indexer so a model swap is `manage.py reembed --model=...`.

## Recommendation for the build ticket (#24)

- Default: `BAAI/bge-small-en-v1.5` via `sentence-transformers`, 384-dim,
  `normalize_embeddings=True`, cosine distance in sqlite-vec.
- Config knobs: `EMBEDDING_BACKEND` (`sentence-transformers` | `openai`),
  `EMBEDDING_MODEL`, `EMBEDDING_DIM`, batch size, worker count.
- Remote fallback: OpenAI-compatible client with configurable `base_url` /
  `model` / `dimensions`, defaulting to `text-embedding-3-small` at
  `dimensions=768` (or 384 to match the local index).
- Persist `embedding_model` + `embedding_dim` per row; ship a `reembed`
  management command that recreates the vec table and rebuilds from source.
- Chunk long Drive docs before embedding (bge-small caps at 512 tokens); short
  records (emails, transactions, tasks, calendar) embed whole.

## Sources

- BAAI/bge-small-en-v1.5 model card — <https://huggingface.co/BAAI/bge-small-en-v1.5>
- thenlper/gte-small model card — <https://huggingface.co/thenlper/gte-small>
  and <https://huggingface.co/api/models/thenlper/gte-small>
- sentence-transformers/all-MiniLM-L6-v2 model card — <https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2>
- nomic-ai/nomic-embed-text-v1.5 model card — <https://huggingface.co/nomic-ai/nomic-embed-text-v1.5>
- Nomic Embed technical report — <https://arxiv.org/abs/2402.01613>
- Ollama embedding model library — <https://ollama.com/search?c=embedding>,
  <https://ollama.com/library/nomic-embed-text>, <https://ollama.com/library/embeddinggemma>
- Ollama embedding models blog — <https://ollama.com/blog/embedding-models>
- OpenAI embeddings guide — <https://developers.openai.com/api/docs/guides/embeddings>
- SBERT pretrained models — <https://sbert.net/docs/sentence_transformer/pretrained_models.html>
- MTEB leaderboard — <https://huggingface.co/spaces/mteb/leaderboard>
