---
name: eunomia-memory
description: Use Eunomia, the user's long-term memory, on every task. Recall what's relevant before answering or acting, and save durable new facts, preferences and decisions as you learn them.
---

# Eunomia memory

You're connected to Eunomia (MCP server `eunomia`), the user's long-term memory.
Use it so the user never has to repeat themselves.

## Before you answer or act

- If the request touches people, projects, organisations, places, preferences,
  past decisions, or code you've worked on, call `recall` with the request (or
  the specific names) first, and use what comes back.
- If nothing relevant comes back, carry on normally. Don't mention it.

## Remember as you go

- Call `memory_write` when the user states a preference, makes a decision,
  corrects you, or tells you a lasting fact about a person, organisation,
  project or place. Save one fact per call, in plain words, about the right
  entity.
- Types: `world` for facts, `experience` for things that happened,
  `observation` for an entity's overall belief (it revises in place).
- Wrong or outdated memory: `memory_update` it. Delete only when asked
  (`memory_delete`).
- Never store secrets, credentials, or one-off small talk.

## Code

When you learn how a codebase fits together, map it with
`code_entity_upsert` (repository, file, symbol) and `code_relate` (`calls`,
`imports`, …).

## Answering from memory

`reflect` gives a cited answer. If it returns `mode: "recall_only"`, write the
answer yourself from the numbered memories and cite them like [1].

## Scope: separate vaults for separate areas

Everything defaults to the user's personal vault. If the user keeps
separate vaults (`vault_list` shows them), for example for different
projects, services, clients, teams or their homelab, keep each area's memory in its vault:

- When the work clearly belongs to one of those areas, pass `vault_id` with
  the vault's **name** (e.g. `vault_id: "Acme"`) to `recall`, `memory_write`,
  `entities_search`, etc.
- Don't create vaults on your own. Use `vault_create` when the user asks for
  a new area or agrees to one.
- Don't mix one vault's facts into another or into the personal vault.

For anything else, call `docs`.
