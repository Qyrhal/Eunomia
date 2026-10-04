import pytest
from fastapi import HTTPException

from app.auth import create_session_jwt, current_user
from app.models_user import authenticate, create_api_token, register_user, verify_api_token


async def test_register_and_authenticate(surreal_db):
    user = await register_user("alice@example.com", "s3cr3t-pw")
    assert user.email == "alice@example.com"

    ok = await authenticate("alice@example.com", "s3cr3t-pw")
    assert ok is not None
    assert ok.id == user.id

    assert await authenticate("alice@example.com", "wrong-pw") is None
    assert await authenticate("nobody@example.com", "whatever") is None


async def test_create_and_verify_api_token(surreal_db):
    user = await register_user("bob@example.com", "pw")
    token = await create_api_token(user.id)
    assert isinstance(token, str) and len(token) > 10

    verified = await verify_api_token(token)
    assert verified is not None
    assert verified.id == user.id

    assert await verify_api_token("not-a-real-token") is None


async def test_current_user_via_bearer_token(surreal_db):
    user = await register_user("carol@example.com", "pw")
    token = await create_api_token(user.id)

    resolved = await current_user(authorization=f"Bearer {token}", session=None)
    assert resolved.id == user.id


async def test_current_user_via_session_cookie(surreal_db):
    user = await register_user("dave@example.com", "pw")
    jwt_token = create_session_jwt(user)

    resolved = await current_user(authorization=None, session=jwt_token)
    assert resolved.id == user.id


async def test_current_user_raises_401_when_unauthenticated(surreal_db):
    with pytest.raises(HTTPException) as exc_info:
        await current_user(authorization=None, session=None)
    assert exc_info.value.status_code == 401

    with pytest.raises(HTTPException):
        await current_user(authorization="Bearer not-a-real-token", session=None)

    with pytest.raises(HTTPException):
        await current_user(authorization=None, session="not-a-real-jwt")
