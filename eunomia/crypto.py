# Encrypts credentials at rest. Key comes from EUNOMIA_MASTER_KEY (a Fernet key —
# generate one with: python -c "from cryptography.fernet import Fernet; print(Fernet.generate_key().decode())"
# ponytail: lookup fails loud and only when a credential is actually touched, not at import time,
# so the rest of the app works fine without it configured.

import os

from cryptography.fernet import Fernet


def _fernet() -> Fernet:
    key = os.environ.get("EUNOMIA_MASTER_KEY")
    if not key:
        raise RuntimeError(
            "EUNOMIA_MASTER_KEY is not set. Generate one with: "
            "python -c \"from cryptography.fernet import Fernet; print(Fernet.generate_key().decode())\""
        )
    return Fernet(key.encode())


def encrypt(value: str) -> str:
    return _fernet().encrypt(value.encode()).decode()


def decrypt(token: str) -> str:
    return _fernet().decrypt(token.encode()).decode()
