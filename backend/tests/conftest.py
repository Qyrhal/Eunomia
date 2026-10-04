"""Shared pytest fixtures.

Ephemeral SurrealDB for tests: the `surreal` binary isn't installed locally
(and this repo's SurrealDB image is Docker-only, pinned to v2.3), so rather
than spin up Docker for unit tests, we use the `surrealdb` Python SDK's
embedded engine directly -- `AsyncSurreal("mem://")` runs an in-memory
SurrealDB instance inside the test process itself, no subprocess/binary/
Docker dependency, no auth (there's no root user in the embedded engine, so
tests bypass `connect_db()`'s signin and populate `app.db._db` directly).
This is the fastest, lowest-friction option and exercises the exact same
SurrealQL (including the BM25/MTREE indexes) that the real server runs.
"""

import pytest_asyncio
from surrealdb import AsyncSurreal

import app.db as db_module
from app.db import SCHEMA_STATEMENTS


@pytest_asyncio.fixture
async def surreal_db():
    """A fresh in-memory SurrealDB instance with the schema applied, wired up
    as the module-level connection `cache`/`embeddings` services read via
    `app.db.db()`."""
    conn = AsyncSurreal("mem://")
    await conn.connect()
    await conn.use("test", "test")
    for statement in SCHEMA_STATEMENTS:
        await conn.query(statement)

    previous = db_module._db
    db_module._db = conn
    try:
        yield conn
    finally:
        db_module._db = previous
        await conn.close()


@pytest_asyncio.fixture
async def owner(surreal_db):
    """A `user` row's RecordID -- every per-user table now requires one."""
    rows = await surreal_db.query(
        "CREATE user SET email = $email, password_hash = 'x' RETURN AFTER",
        {"email": "owner@example.com"},
    )
    return rows[0]["id"]
