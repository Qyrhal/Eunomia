"""App settings, read from environment / .env (pydantic-settings)."""

import logging
import secrets
from typing import Literal

from pydantic_settings import BaseSettings, SettingsConfigDict

log = logging.getLogger("eunomia.config")


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    # Signs browser-session JWTs (app/auth.py). If unset, a random secret is
    # generated at startup -- fine for a single process / local dev, but it
    # means existing sessions won't survive a restart and multi-process
    # deployments must set this explicitly so every process agrees.
    JWT_SECRET: str = ""

    SURREAL_URL: str = "ws://localhost:8000/rpc"
    SURREAL_USER: str = "root"
    SURREAL_PASS: str = "root"
    SURREAL_NS: str = "eunomia"
    SURREAL_DB: str = "eunomia"

    OPENAI_API_KEY: str | None = None

    # Fernet key for connector credential encryption. Falls back to a static
    # (non-secret) key when unset so tests / fresh local dev can still
    # round-trip values; real deployments must set a real key.
    ENCRYPTION_KEY: str = ""

    # The ONLY place the stub embeddings backend is selectable -- never a
    # DB-backed user setting.
    EMBEDDINGS_BACKEND: Literal["openai", "stub"] = "openai"


settings = Settings()

if not settings.JWT_SECRET:
    settings.JWT_SECRET = secrets.token_urlsafe(32)
    log.warning(
        "JWT_SECRET not set -- generated a random one for this process. "
        "Sessions won't survive a restart; set JWT_SECRET explicitly in production."
    )
