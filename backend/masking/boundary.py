"""The de-tokenization boundary (#22).

The ONLY place outside :mod:`masking.vault` allowed to turn tokens back into
real values. Two directions:

- :func:`resolve_tool_input` — the MCP/REST dispatcher calls this on tool-call
  arguments *before* running the tool, so an agent can act on a token it can't read.
- :func:`resolve_outbound` — a source's HTTP client calls this on the fully
  assembled request pieces immediately before hitting the external API.

Tool *return values* are never passed through here. Neither is anything a logger,
serializer, or the cache touches. A grep test enforces that.
"""

from .vault import TOKEN_RE, detokenize


def _resolve_str(s: str) -> str:
    return TOKEN_RE.sub(lambda m: detokenize(m.group(0)) or m.group(0), s)


def _walk(obj):
    if isinstance(obj, str):
        return _resolve_str(obj)
    if isinstance(obj, dict):
        return {k: _walk(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return type(obj)(_walk(v) for v in obj)
    return obj


def resolve_tool_input(args):
    """Deep-replace every token in a tool-call argument structure. audit: #36."""
    return _walk(args)


def resolve_outbound(payload):
    """Deep-replace every token in an outbound request payload. audit: #36."""
    return _walk(payload)
