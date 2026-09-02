"""The vault: reversible masking of secrets and PII.

Token shape: ``[eunomia:<type>:<n>]`` — ASCII, bracketed, survives JSON and
verbatim LLM copy-back. `TOKEN_RE` finds them anywhere in a string.

Only :mod:`masking.boundary` (and this module) may call :func:`detokenize`.
"""

import hashlib
import hmac
import re

from django.conf import settings
from django.db import IntegrityError, transaction
from django.utils import timezone

from connectors.crypto import decrypt, encrypt

from .models import VaultSecret

TOKEN_RE = re.compile(r"\[eunomia:([a-z0-9_]+):(\d+)\]")


def _hmac(type_: str, value: str) -> str:
    key = (getattr(settings, "ENCRYPTION_KEY", "") or "").encode()
    if not key:
        raise RuntimeError("ENCRYPTION_KEY is not set; the vault cannot operate.")
    return hmac.new(key, f"{type_}:{value}".encode(), hashlib.sha256).hexdigest()


def _normalize(type_: str, value: str) -> str:
    if type_ == "email":
        return value.strip().lower()
    if type_ in {"phone", "card", "bank_acct"}:
        return re.sub(r"\D", "", value)
    return value


def tokenize(value: str, type_: str, *, kind: str = VaultSecret.KIND_PII, source: str = "") -> str:
    """Return the stable token for ``value``, minting one on first sight."""
    if not value:
        return value
    digest = _hmac(type_, _normalize(type_, value))
    existing = VaultSecret.objects.filter(value_hmac=digest).first()
    if existing:
        VaultSecret.objects.filter(pk=existing.pk).update(last_used=timezone.now())
        return existing.token

    with transaction.atomic():
        used = (
            VaultSecret.objects.select_for_update()
            .filter(type=type_)
            .values_list("token", flat=True)
        )
        nums = [int(m.group(2)) for t in used if (m := TOKEN_RE.fullmatch(t))]
        n = (max(nums) + 1) if nums else 1
        token = f"[eunomia:{type_}:{n}]"
        try:
            VaultSecret.objects.create(
                token=token,
                value_encrypted=encrypt(value),
                value_hmac=digest,
                kind=kind,
                type=type_,
                source=source,
            )
        except IntegrityError:
            # racing writer inserted the same value between our check and create
            return VaultSecret.objects.get(value_hmac=digest).token
    return token


def detokenize(token: str) -> str | None:
    """Real value behind a token, or None if unknown. Boundary use only."""
    row = VaultSecret.objects.filter(token=token).first()
    if not row:
        return None
    VaultSecret.objects.filter(pk=row.pk).update(last_used=timezone.now())
    return decrypt(row.value_encrypted)


def rotate(token: str, new_value: str) -> None:
    """Point an existing token at new bytes (e.g. an OAuth token refresh)."""
    row = VaultSecret.objects.get(token=token)
    row.value_encrypted = encrypt(new_value)
    row.value_hmac = _hmac(row.type, _normalize(row.type, new_value))
    row.last_used = timezone.now()
    row.save(update_fields=["value_encrypted", "value_hmac", "last_used"])


def tokenize_text(text: str, source: str = "") -> str:
    """Replace every detected PII span in ``text`` with its token.

    The detector registry lives in :mod:`masking.pii` (#28); until it ships this
    is a no-op passthrough, so callers can wire it in now.
    """
    if not text:
        return text
    try:
        from .pii import scan
    except ImportError:
        return text
    spans = sorted(scan(text), key=lambda s: s[0], reverse=True)
    out = text
    for start, end, type_, _conf in spans:
        tok = tokenize(out[start:end], type_, kind=VaultSecret.KIND_PII, source=source)
        out = out[:start] + tok + out[end:]
    return out
