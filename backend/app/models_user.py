"""User accounts: registration, password auth, personal API tokens.

Passwords are hashed with bcrypt. Personal API tokens are random strings;
only their SHA-256 hash is ever stored (``user.api_token_hash``), matching
the plan's "generated once, shown once, stored hashed" model.
"""

import hashlib
import secrets
from dataclasses import dataclass

import bcrypt
from surrealdb import RecordID

from app.db import db as get_connection


@dataclass
class User:
    id: RecordID
    email: str


def _row_to_user(row: dict) -> User:
    return User(id=row["id"], email=row["email"])


async def register_user(email: str, password: str) -> User:
    """Create a new `user` row. Raises on duplicate email (unique index)."""
    conn = get_connection()
    password_hash = bcrypt.hashpw(password.encode(), bcrypt.gensalt()).decode()
    rows = await conn.query(
        "CREATE user SET email = $email, password_hash = $password_hash RETURN AFTER",
        {"email": email, "password_hash": password_hash},
    )
    return _row_to_user(rows[0])


async def authenticate(email: str, password: str) -> User | None:
    conn = get_connection()
    rows = await conn.query("SELECT * FROM user WHERE email = $email LIMIT 1", {"email": email})
    if not rows:
        return None
    row = rows[0]
    if not bcrypt.checkpw(password.encode(), row["password_hash"].encode()):
        return None
    return _row_to_user(row)


def _hash_token(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


async def create_api_token(owner: RecordID) -> str:
    """Generate a new personal API token for `owner`, store its hash, and
    return the plaintext -- the only time it is ever available."""
    conn = get_connection()
    token = secrets.token_urlsafe(32)
    await conn.query(
        "UPDATE $id SET api_token_hash = $hash", {"id": owner, "hash": _hash_token(token)}
    )
    return token


async def verify_api_token(token: str) -> User | None:
    conn = get_connection()
    rows = await conn.query(
        "SELECT * FROM user WHERE api_token_hash = $hash LIMIT 1", {"hash": _hash_token(token)}
    )
    return _row_to_user(rows[0]) if rows else None
