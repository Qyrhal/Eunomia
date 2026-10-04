from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

from fastapi import FastAPI

from app.db import close_db, connect_db, ensure_schema


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    db = await connect_db()
    await ensure_schema(db)
    yield
    await close_db()


app = FastAPI(lifespan=lifespan)


@app.get("/healthz")
async def healthz() -> dict[str, str]:
    return {"status": "ok"}
