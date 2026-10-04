"""Entity-memory graph: person/organisation/location + memory + relates_to.

Owner-scoped CRUD/query layer, same pattern as ``cache/search.py`` /
``connectors/service.py``. Dedup for people/orgs/locations is name-or-alias,
case-insensitive, per owner -- "Alex" and "alex" are the same person.
"""

from typing import Literal

from surrealdb import RecordID

from app.db import db as get_connection

Kind = Literal["person", "organisation", "location"]
KINDS: tuple[str, ...] = ("person", "organisation", "location")


def _cache_record_rid(owner: RecordID, record_id: str) -> RecordID:
    """The internal `cache_record` RecordID for a record's caller-facing id
    -- mirrors `cache/search.py`'s `_rid` (owner-id prefix is internal)."""
    return RecordID("cache_record", f"{owner.id}:{record_id}")


def _as_rid(entity_id) -> RecordID:
    return entity_id if isinstance(entity_id, RecordID) else RecordID.parse(str(entity_id))


def _jsonable(obj):
    """Recursively stringify `RecordID`s -- SurrealDB's Python SDK returns
    them as a `{table_name, id}` object, not a JSON-friendly `"table:id"`
    string, so presentation-facing results (REST, MCP tool output, the
    frontend graph) convert at this one point rather than each caller
    reinventing it."""
    if isinstance(obj, RecordID):
        return str(obj)
    if isinstance(obj, dict):
        return {k: _jsonable(v) for k, v in obj.items()}
    if isinstance(obj, list):
        return [_jsonable(v) for v in obj]
    return obj


async def _select_one(conn, rid: RecordID) -> dict | None:
    res = await conn.select(rid)
    if isinstance(res, list):
        return res[0] if res else None
    return res


async def upsert_entity(owner: RecordID, kind: Kind, name: str, aliases: list[str] | None = None) -> dict:
    """Find-or-create a `kind` entity for `owner`, matched case-insensitively
    against existing `name`/`aliases`. New aliases are merged onto a match
    rather than creating a duplicate row."""
    conn = get_connection()
    aliases = list(aliases or [])
    needle = name.strip().lower()

    rows = await conn.query(f"SELECT * FROM {kind} WHERE owner = $owner", {"owner": owner})
    for row in rows:
        known = {(row.get("name") or "").lower(), *((a or "").lower() for a in row.get("aliases") or [])}
        if needle in known:
            merged = sorted(set(row.get("aliases") or []) | set(aliases))
            if set(merged) != set(row.get("aliases") or []):
                updated = await conn.query(
                    "UPDATE $id SET aliases = $aliases, updated_at = time::now() RETURN AFTER",
                    {"id": row["id"], "aliases": merged},
                )
                return updated[0]
            return row

    created = await conn.query(
        f"CREATE {kind} SET owner = $owner, name = $name, aliases = $aliases RETURN AFTER",
        {"owner": owner, "name": name, "aliases": aliases},
    )
    return created[0]


async def add_memory(owner: RecordID, subject_id, text: str, source_record_id: str) -> dict:
    conn = get_connection()
    rows = await conn.query(
        "CREATE memory SET owner = $owner, subject = $subject, text = $text, source = $source RETURN AFTER",
        {
            "owner": owner,
            "subject": _as_rid(subject_id),
            "text": text,
            "source": _cache_record_rid(owner, source_record_id),
        },
    )
    return rows[0]


async def add_relation(owner: RecordID, from_id, to_id, label: str, source_record_id: str | None = None) -> dict:
    """RELATE two entities, idempotent on the (in, out, label) unique index --
    a duplicate relation is a no-op that returns the existing edge."""
    conn = get_connection()
    in_rid, out_rid = _as_rid(from_id), _as_rid(to_id)

    existing = await conn.query(
        "SELECT * FROM relates_to WHERE in = $in AND out = $out AND label = $label LIMIT 1",
        {"in": in_rid, "out": out_rid, "label": label},
    )
    if existing:
        return existing[0]

    params = {"in": in_rid, "out": out_rid, "label": label}
    set_clause = "SET label = $label"
    if source_record_id is not None:
        params["source"] = _cache_record_rid(owner, source_record_id)
        set_clause += ", source = $source"
    try:
        rows = await conn.query(f"RELATE $in->relates_to->$out {set_clause} RETURN AFTER", params)
        return rows[0]
    except Exception:
        # lost a race against another concurrent RELATE -- fetch what won.
        existing = await conn.query(
            "SELECT * FROM relates_to WHERE in = $in AND out = $out AND label = $label LIMIT 1",
            {"in": in_rid, "out": out_rid, "label": label},
        )
        return existing[0] if existing else {}


async def get_entity(owner: RecordID, entity_id) -> dict | None:
    """An entity's row plus its `memory` entries and `relates_to` edges in
    both directions, or `None` if it doesn't exist / isn't owned by `owner`."""
    conn = get_connection()
    rid = _as_rid(entity_id)
    row = await _select_one(conn, rid)
    if not row or row.get("owner") != owner:
        return None

    memories = await conn.query(
        "SELECT * FROM memory WHERE subject = $id ORDER BY created_at DESC", {"id": rid}
    )
    outgoing = await conn.query("SELECT * FROM relates_to WHERE in = $id", {"id": rid})
    incoming = await conn.query("SELECT * FROM relates_to WHERE out = $id", {"id": rid})

    return _jsonable(
        {
            "id": rid,
            "kind": rid.table_name,
            "name": row.get("name", ""),
            "aliases": row.get("aliases") or [],
            "summary": row.get("summary", ""),
            "memory": memories,
            "relations": [{**r, "direction": "out"} for r in outgoing]
            + [{**r, "direction": "in"} for r in incoming],
        }
    )


async def list_entities(owner: RecordID, kind: Kind | None = None) -> list[dict]:
    conn = get_connection()
    kinds = (kind,) if kind else KINDS
    out: list[dict] = []
    for k in kinds:
        rows = await conn.query(f"SELECT * FROM {k} WHERE owner = $owner ORDER BY name", {"owner": owner})
        for row in rows:
            out.append(
                _jsonable(
                    {
                        "id": row["id"],
                        "kind": k,
                        "name": row.get("name", ""),
                        "aliases": row.get("aliases") or [],
                        "summary": row.get("summary", ""),
                    }
                )
            )
    return out


async def graph(owner: RecordID) -> dict:
    """`{nodes: [{id, kind, name}], edges: [{source, target, label}]}` -- the
    full entity graph for the dashboard's force-directed view."""
    conn = get_connection()
    nodes = []
    ids: list[RecordID] = []
    for kind in KINDS:
        rows = await conn.query(f"SELECT * FROM {kind} WHERE owner = $owner", {"owner": owner})
        for row in rows:
            nodes.append({"id": row["id"], "kind": kind, "name": row.get("name", "")})
            ids.append(row["id"])

    edges = []
    if ids:
        # `in IN $ids` doesn't match against a union-typed `record<a|b|c>`
        # field reliably in this SurrealDB version -- `$ids CONTAINS field`
        # does.
        rows = await conn.query(
            "SELECT * FROM relates_to WHERE $ids CONTAINS in AND $ids CONTAINS out", {"ids": ids}
        )
        edges = [{"source": r["in"], "target": r["out"], "label": r.get("label", "")} for r in rows]

    return _jsonable({"nodes": nodes, "edges": edges})
