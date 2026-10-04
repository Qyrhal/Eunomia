"""Authentication: a browser JWT cookie or a personal API token header, both
resolving to the same :func:`current_user` dependency.

- Browser sessions: ``POST /api/auth/login`` issues a signed JWT (HS256,
  ``settings.JWT_SECRET``) stored as an httpOnly cookie.
- Agent/MCP sessions: a long-lived personal API token (``app/models_user.py``)
  presented as ``Authorization: Bearer <token>``.

Service/tool code never needs to know which method was used -- both paths
hand back the same :class:`~app.models_user.User`.
"""

import jwt
from fastapi import Cookie, Header, HTTPException
from surrealdb import RecordID

from app.config import settings
from app.models_user import User, verify_api_token

SESSION_COOKIE = "eunomia_session"
_JWT_ALG = "HS256"


def create_session_jwt(user: User) -> str:
    return jwt.encode({"sub": str(user.id), "email": user.email}, settings.JWT_SECRET, algorithm=_JWT_ALG)


def _user_from_jwt(token: str) -> User | None:
    try:
        payload = jwt.decode(token, settings.JWT_SECRET, algorithms=[_JWT_ALG])
    except jwt.PyJWTError:
        return None
    sub = payload.get("sub")
    if not sub:
        return None
    try:
        rid = RecordID.parse(sub)
    except Exception:
        return None
    return User(id=rid, email=payload.get("email", ""))


async def current_user(
    authorization: str | None = Header(default=None),
    session: str | None = Cookie(default=None, alias=SESSION_COOKIE),
) -> User:
    """Resolve the authenticated user from a Bearer API token or the session
    cookie. Raises 401 if neither is present/valid."""
    if authorization:
        scheme, _, value = authorization.partition(" ")
        if scheme.lower() == "bearer" and value:
            user = await verify_api_token(value)
            if user:
                return user

    if session:
        user = _user_from_jwt(session)
        if user:
            return user

    raise HTTPException(status_code=401, detail="Not authenticated.")
