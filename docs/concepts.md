# Concepts

## Vaults

A vault is a separate scope for entities and memories. Everyone has a
**personal vault** (the default for every tool). Make more for anything you
want kept apart, like a project or service, a client, a team or your homelab. Invite people to
share a vault, and they accept from the Vaults page. Roles are `owner` and `member`.

Agents can name a vault instead of using its id: every tool's `vault_id`
takes a name, e.g. `memory_write` with `vault_id: "Acme"`. The memory skill
tells agents to keep each area's memory in its own vault, and to create
vaults only when you ask.

- **Clone** copies a vault into a new one you own.
- **Merge** takes two vaults and creates a **new** vault from copies of
  both. The originals are never changed. Entities with the same kind and name
  are folded together: aliases are unioned, identical facts are kept once,
  relations are de-duplicated, and two observations are joined. Use it from
  the Vaults page or the `vault_merge` tool.

**Isolation guarantees.** Every read and write checks that you are an
active member of the vault right now: a pending invitation gives no access,
and removing someone or leaving takes effect on the next call. A vault name
only matches vaults you belong to. Relations and entity merges stay inside one
vault. Tools without a `vault_id` (and the export) always use your own
personal vault, even if you've joined someone else's. Synced records belong
to you, not to a vault: they show up only in your personal vault's recall and
vector cloud, and other members never see them.

## Entities and memories

Entities are people, organisations, locations, repositories, files and
symbols. Each one has **memories**:

| Type | Meaning |
|---|---|
| `world` | an objective fact |
| `experience` | something that happened |
| `observation` | the entity's consolidated belief, one per entity, revised in place |

Memories support full create, read, update and delete (`memory_write`,
`entities_get`/`recall`, `memory_update`, `memory_delete`). Editing or adding
a fact marks the entity's observation *stale* until it's consolidated again.
Relations (`works_at`, `calls`, …) link entities, and an edge with the same
label is stored only once.

## Synced records

Connectors (Up Bank, GitHub, Gmail, …) sync raw records into a cache. With
a model key, records are embedded for semantic search. Use `search`/`get`
for these, and `recall` to search records and memories together.

## Recall

`recall` runs five arms and fuses them (reciprocal rank fusion), then boosts
results that several arms agree on and recent ones:

1. **semantic**: embeddings over records (needs a model key)
2. **keyword**: full-text over records
3. **memory text**: full-text (BM25) over memories, so a fact is found by what it says
4. **graph**: entities named in the question, then their memories and linked records
5. **temporal**: an explicit `time_range`

## The graph

Entities → Graph shows the entity graph. Drag nodes to rearrange them,
scroll or use the buttons to zoom, and click a node to open it. The Code
page shows the code graph the same way.

## The vector cloud

Entities → Cloud shows the *shape of what's stored*: every memory (and, for
your personal vault, every synced record) as a point. The full vectors are
projected to 3D with PCA, so the axes are the three directions of greatest
spread (PC1–PC3), not entities. Pick several vaults to layer them as
coloured clouds in the **same** space. A vault and its merge, or two teams'
vaults, are directly comparable. Without a model key the space is lexical
(points that share words sit together) rather than semantic.
