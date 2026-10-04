"""App settings, read from environment / .env (pydantic-settings)."""

from typing import Literal

from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    EUNOMIA_API_TOKEN: str | None = None

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
