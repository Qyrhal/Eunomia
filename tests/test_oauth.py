import pytest

from eunomia import oauth


def test_is_configured_false_when_env_missing(monkeypatch):
    monkeypatch.delenv("EUNOMIA_GITHUB_CLIENT_ID", raising=False)
    monkeypatch.delenv("EUNOMIA_GITHUB_CLIENT_SECRET", raising=False)
    assert oauth.is_configured("github") is False


def test_is_configured_true_when_env_set(monkeypatch):
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_ID", "id123")
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_SECRET", "secret123")
    assert oauth.is_configured("github") is True


def test_authorize_url_includes_client_id_and_state(monkeypatch):
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_ID", "id123")
    url = oauth.authorize_url("github", "http://localhost/oauth/github/callback", "state-abc")
    assert url.startswith("https://github.com/login/oauth/authorize?")
    assert "client_id=id123" in url
    assert "state=state-abc" in url


def test_exchange_code_for_token_success(monkeypatch):
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_ID", "id123")
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_SECRET", "sec123")

    class FakeResponse:
        def raise_for_status(self):
            pass

        def json(self):
            return {"access_token": "gho_faketoken"}

    monkeypatch.setattr(oauth.httpx, "post", lambda *a, **kw: FakeResponse())

    token = oauth.exchange_code_for_token("github", "code123", "http://localhost/callback")
    assert token == "gho_faketoken"


def test_exchange_code_for_token_missing_access_token_raises(monkeypatch):
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_ID", "id123")
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_SECRET", "sec123")

    class FakeResponse:
        def raise_for_status(self):
            pass

        def json(self):
            return {"error": "bad_verification_code"}

    monkeypatch.setattr(oauth.httpx, "post", lambda *a, **kw: FakeResponse())

    with pytest.raises(RuntimeError):
        oauth.exchange_code_for_token("github", "code123", "http://localhost/callback")
