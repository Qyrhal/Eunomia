"""The de-tokenization boundary (#22).

The ONLY place outside :mod:`masking.vault` allowed to turn tokens back into
real values. Two directions:

- :func:`resolve_tool_input` — the MCP/REST dispatcher calls this on tool-call
  arguments *before* running the tool, so an agent can act on a token it can't read.
- :func:`resolve_outbound` — a source's HTTP client calls this on the fully
  assembled request pieces immediately before hitting the external API.

Every crossing is written to the audit log (#36) — token strings + actor, never
values. Tool *return values* are never passed through here. A grep test enforces
that ``detokenize`` is called nowhere else.
"""

from . import audit
from .models import AuditEvent
from .vault import TOKEN_RE, detokenize


def _walk(obj, seen: set):
    if isinstance(obj, str):
        def _rep(m):
            tok = m.group(0)
            seen.add(tok)
            return detokenize(tok) or tok

        return TOKEN_RE.sub(_rep, obj)
    if isinstance(obj, dict):
        return {k: _walk(v, seen) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return type(obj)(_walk(v, seen) for v in obj)
    return obj


def resolve_tool_input(args, actor: str = ""):
    """Deep-replace every token in a tool-call argument structure."""
    seen: set = set()
    out = _walk(args, seen)
    audit.record(AuditEvent.KIND_TOOL_INPUT, actor, seen)
    return out


def resolve_outbound(payload, actor: str = ""):
    """Deep-replace every token in an outbound request payload."""
    seen: set = set()
    out = _walk(payload, seen)
    audit.record(AuditEvent.KIND_OUTBOUND, actor, seen)
    return out
