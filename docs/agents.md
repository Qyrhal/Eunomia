# AI agents & MCP

Eunomia is an MCP server (Streamable HTTP) at `http://localhost:8001/mcp`
(also proxied at `http://localhost:3000/mcp`). It exposes the same tools the
in-app chat uses. Every call is scoped to the token's user and logged in the
audit log if it changes anything.

## Connecting agents

The installer does this for you. To do it again, or for an agent you install
later:

```bash
cd eunomia
./scripts/connect-agents.sh --email you@example.com --password '…'          # every agent
./scripts/connect-agents.sh --token <token> --agents claude,codex           # an existing token
```

| Agent | Config written |
|---|---|
| Claude Code | `~/.claude.json` → `mcpServers.eunomia` |
| Codex | `~/.codex/config.toml` → `[mcp_servers.eunomia]` |
| Hermes | `~/.hermes/config.yaml` → `mcp_servers.eunomia` |
| Gemini CLI | `~/.gemini/settings.json` |
| Cursor | `~/.cursor/mcp.json` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` |
| OpenCode | `~/.config/opencode/opencode.json` |
| VS Code | `…/Code/User/mcp.json` → `servers.eunomia` |
| Claude Desktop | `…/Claude/claude_desktop_config.json` (through `npx mcp-remote`, needs Node) |

- Each agent gets its own token (Settings → API tokens, named after it), so
  you can revoke one without touching the others. Re-running replaces the
  entry and revokes the old token.
- Configs for agents you haven't installed yet are written too, so they're
  connected the first time they run (`--detected-only` turns that off).
- Other settings in those files are preserved. A file the script can't
  parse, or one with its own hand-written `eunomia` entry, is left untouched
  and reported.
- ChatGPT and Claude.ai (web/desktop) need a public `https://` URL. Add it
  by hand under their Connectors settings.

## The memory skill

The **Memory skill** page (sidebar) holds the instructions that tell agents
when to recall and what to remember. You can view it, edit it, or reset it to the
built-in default. It reaches agents three ways:

- **Every MCP client** gets it as the server's connection instructions, so an
  edit applies the next time the agent connects.
- **Claude Code, Codex, Hermes** get a `skills/eunomia-memory/SKILL.md`
  (re-run `connect-agents.sh` to refresh the file copies).
- **Claude Code** also gets two hooks in `~/.claude/settings.json`:
  `SessionStart` injects your current skill, and `UserPromptSubmit` calls
  `recall` with each prompt and injects the relevant memories before Claude
  sees it. The hooks are silent and never block a prompt if Eunomia is down.

By hand, for any MCP client: URL `http://localhost:8001/mcp`, header
`Authorization: Bearer <token>`. Create a token in Settings → API tokens.

## Do I need an OpenAI key?

No. The connected agent is the model.

| Without a key | With a key (recommended to save tokens) |
|---|---|
| `recall`: keyword + full-text over memories + graph + recency | adds semantic (embedding) search |
| `reflect` returns the numbered memories; your agent writes the answer | Eunomia writes the cited answer |
| `consolidate_observations`: your agent writes the belief with `memory_write` type `observation` | Eunomia synthesizes it |
| Vector cloud uses a lexical (word) space | Real embedding space |
| In-app Chat is off | In-app Chat works |

## Tools

| Tool | Does |
|---|---|
| `docs` | Read these docs. No args lists them, `topic` reads one |
| `recall` | Best memories and records for a question, within a token budget |
| `reflect` | Cited answer from recalled memories (or the memories, without a key) |
| `search` / `get` / `list` / `links` | Raw synced records from connectors |
| `memory_write` | Remember a fact (`world`, `experience`) or set an entity's `observation` |
| `memory_update` / `memory_delete` | Edit or delete one memory |
| `entities_search` / `entities_get` / `entities_graph` | Find and read people, orgs, places, code |
| `entity_update` / `entity_merge` / `entity_delete` | Edit, de-duplicate, delete entities |
| `code_entity_upsert` / `code_relate` | Map repositories, files, symbols and their relations |
| `consolidate_observations` | Fold new facts into each entity's belief |
| `vault_list` / `vault_create` / `vault_clone` / `vault_merge` | Manage vaults; merge two into a new one |
| `vault_invite` / `vault_members` / `vault_remove_member` / `vault_leave` / `vault_rename` / `vault_delete` | Sharing |

Destructive tools are flagged `destructiveHint` so clients can ask first.
