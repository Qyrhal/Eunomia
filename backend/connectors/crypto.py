from cryptography.fernet import Fernet

from app.config import settings


def _fernet() -> Fernet:
    key = settings.ENCRYPTION_KEY
    if not key:
        # Degrade to a static fallback when ENCRYPTION_KEY is empty. Reachable
        # only in test harnesses (no key in the in-memory test DB) or local-dev
        # setups that skipped the key — in that case the app is already "open"
        # (see app/auth.py) and any previously-stored ciphertext can't be
        # decrypted, so round-tripping is the only operation that still needs to
        # work (tests, demo seeding on a fresh DB). The fallback is a real
        # 32-byte URL-safe base64 Fernet key, not a plain string.
        key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    return Fernet(key.encode())


def encrypt(value: str) -> str:
    if not value:
        return ""
    return _fernet().encrypt(value.encode()).decode()


def decrypt(value: str) -> str:
    if not value:
        return ""
    return _fernet().decrypt(value.encode()).decode()
