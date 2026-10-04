"""Integration-test fixtures: a real ASGI client against `app.main.app`,
backed by the same ephemeral in-memory SurrealDB as the unit tests.

`ASGITransport` never sends ASGI lifespan events, so `app.main`'s lifespan
(which opens its own DB connection, starts the scheduler, etc.) never runs --
we wire up the DB (`surreal_db`) and source discovery ourselves instead.
"""

import pytest_asyncio
from httpx import ASGITransport, AsyncClient

from app.main import app
from sources.registry import discover


@pytest_asyncio.fixture
async def client(surreal_db):
    discover()
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as c:
        yield c
