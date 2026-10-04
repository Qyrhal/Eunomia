from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

import entities.tools  # noqa: F401 -- side-effect import, registers entities_* tools
from app.config import settings as app_settings
from app.db import close_db, connect_db, ensure_schema
from app.routers import auth, connectors, entities as entities_router, settings, sources, tools


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

app.add_middleware(
    CORSMiddleware,
    allow_origins=[o.strip() for o in app_settings.CORS_ALLOWED_ORIGINS.split(",") if o.strip()],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(auth.router, prefix="/api")
app.include_router(settings.router, prefix="/api")
app.include_router(connectors.router, prefix="/api")
app.include_router(connectors.snapshot_router, prefix="/api")
app.include_router(sources.router, prefix="/api")
app.include_router(tools.router, prefix="/api")
app.include_router(entities_router.router, prefix="/api")


@app.get("/healthz")
async def healthz() -> dict[str, str]:
    return {"status": "ok"}
