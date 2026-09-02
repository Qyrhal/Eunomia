"""The source plug-in contract (#5).

A source is a package under ``sources/`` exposing a module-level ``SOURCE`` that
is a :class:`Source` instance. The registry (:mod:`sources.registry`) discovers
it on startup and wires it into sync, masking, and the tool surface.
"""

import abc
from dataclasses import dataclass, field


@dataclass
class SyncResult:
    records: list[dict] = field(default_factory=list)   # raw origin dicts
    cursor: str | None = None                           # opaque, fed back next sync


@dataclass
class ToolSpec:
    name: str
    schema: dict          # JSON-schema for the arguments
    impl: object          # callable(**args) -> jsonable


class Source(abc.ABC):
    #: stable identifier, e.g. "up_bank"
    key: str = ""
    #: human label for the admin UI
    label: str = ""
    #: envelope `type` values this source emits, e.g. ["up.transaction", "up.account"]
    record_types: list[str] = []
    #: "oauth" | "api_key" | "token"
    auth_kind: str = "token"
    #: dotted paths into the stored credentials dict that must always be vault-masked
    secret_fields: list[str] = []

    @abc.abstractmethod
    def sync(self, mode: str, cursor: str | None = None) -> SyncResult:
        """Fetch raw records. `mode` in {"poll", "webhook", "backfill"}."""

    @abc.abstractmethod
    def map(self, raw: dict) -> dict | None:
        """One raw origin dict -> a cache envelope (#21), or None to skip."""

    def tools(self) -> list[ToolSpec]:
        """Per-source MCP/REST tools beyond the generic ones (#7)."""
        return []

    def webhook(self, request) -> list[dict] | None:
        """Verify + translate a provider push into raw dicts (or None)."""
        return None
