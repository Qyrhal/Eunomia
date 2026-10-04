from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

from fastapi import FastAPI

from app.db import close_db, connect_db, ensure_schema


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    from sources.registry import discover
    from sources.scheduler import build_scheduler

    db = await connect_db()
    await ensure_schema(db)

    discover()
    scheduler = await build_scheduler()
    scheduler.start()

    yield

    scheduler.shutdown()
    await close_db()


app = FastAPI(lifespan=lifespan)


@app.get("/healthz")
async def healthz() -> dict[str, str]:
    return {"status": "ok"}
