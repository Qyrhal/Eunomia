from app.auth import token_ok
from app.config import settings


def test_open_when_no_token_configured(monkeypatch):
    monkeypatch.setattr(settings, "EUNOMIA_API_TOKEN", "")
    assert token_ok(None) is True
    assert token_ok("Bearer anything") is True


def test_requires_matching_bearer_when_configured(monkeypatch):
    monkeypatch.setattr(settings, "EUNOMIA_API_TOKEN", "s3cr3t")
    assert token_ok(None) is False
    assert token_ok("s3cr3t") is False
    assert token_ok("Bearer wrong") is False
    assert token_ok("Basic s3cr3t") is False
    assert token_ok("Bearer s3cr3t") is True
    assert token_ok("bearer s3cr3t") is True
