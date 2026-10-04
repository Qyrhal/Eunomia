"""Auth routes: register/login issue the session cookie, `me`/`token` require
it (or a Bearer API token, via `current_user`). No router-level auth
dependency here -- register/login/logout/bootstrap must be reachable
unauthenticated."""

from fastapi import APIRouter, Depends, HTTPException, Response
from pydantic import BaseModel
from surrealdb.errors import InternalError

from app.auth import SESSION_COOKIE, create_session_jwt, current_user
from app.db import db as get_connection
from app.models_user import User, authenticate, create_api_token, register_user

router = APIRouter(prefix="/auth", tags=["auth"])


class Credentials(BaseModel):
    email: str
    password: str


async def _onboarded(user: User) -> bool:
    conn = get_connection()
    row = await conn.select(user.id)
    if isinstance(row, list):
        row = row[0] if row else None
    return bool(row and row.get("onboarded_at"))


def _set_session_cookie(response: Response, user: User) -> None:
    response.set_cookie(
        SESSION_COOKIE,
        create_session_jwt(user),
        httponly=True,
        samesite="lax",
    )


@router.post("/register")
async def register(body: Credentials, response: Response) -> dict:
    try:
        user = await register_user(body.email, body.password)
    except InternalError:
        raise HTTPException(status_code=409, detail="A user with that email already exists.")
    _set_session_cookie(response, user)
    return {"id": str(user.id), "email": user.email, "onboarded": False}


@router.post("/login")
async def login(body: Credentials, response: Response) -> dict:
    user = await authenticate(body.email, body.password)
    if user is None:
        raise HTTPException(status_code=401, detail="Invalid email or password.")
    _set_session_cookie(response, user)
    return {"id": str(user.id), "email": user.email, "onboarded": await _onboarded(user)}


@router.post("/logout")
async def logout(response: Response) -> dict:
    response.delete_cookie(SESSION_COOKIE)
    return {"ok": True}


@router.get("/me")
async def me(user: User = Depends(current_user)) -> dict:
    return {"id": str(user.id), "email": user.email, "onboarded": await _onboarded(user)}


@router.post("/token")
async def token(user: User = Depends(current_user)) -> dict:
    return {"token": await create_api_token(user.id)}


@router.get("/bootstrap")
async def bootstrap() -> dict:
    conn = get_connection()
    rows = await conn.query("SELECT count() FROM user GROUP ALL")
    has_users = bool(rows and rows[0].get("count"))
    return {"has_users": has_users}
