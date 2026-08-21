import pytest
from cryptography.fernet import Fernet

from eunomia import crypto


@pytest.fixture(autouse=True)
def master_key(monkeypatch):
    monkeypatch.setenv("EUNOMIA_MASTER_KEY", Fernet.generate_key().decode())


def test_encrypt_decrypt_roundtrip():
    token = crypto.encrypt("super-secret-value")
    assert token != "super-secret-value"
    assert crypto.decrypt(token) == "super-secret-value"


def test_missing_master_key_raises(monkeypatch):
    monkeypatch.delenv("EUNOMIA_MASTER_KEY", raising=False)
    with pytest.raises(RuntimeError):
        crypto.encrypt("x")
