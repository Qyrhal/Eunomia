"""Pattern-based PII detection for the ingest path (#28).

`scan(text)` returns non-overlapping `(start, end, type, confidence)` spans.
Detectors are regex + checksum only — no model. Names / street addresses need
NER and are deliberately out of scope (fog on the map).

Add a detector: append to `DETECTORS`. Each is `(type, fn)` where
`fn(text) -> Iterable[(start, end, confidence)]`.
"""

import math
import re

Span = tuple[int, int, str, float]

_EMAIL = re.compile(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b")
_PHONE = re.compile(
    r"""(?x)
    (?<![\w.])                         # not mid-token / mid-decimal
    (?:
        \+\d{1,3}[\s.\-]?\(?\d{1,4}\)?(?:[\s.\-]?\d{2,4}){2,4}   # +61 4 1234 5678
      | \(0[2-9]\)[\s.\-]?\d{4}[\s.\-]?\d{4}                     # (02) 1234 5678
      | \b0[2-9]\d{0,2}[\s.\-]?\d{3,4}[\s.\-]?\d{3,4}\b          # 0412 345 678
    )
    (?![\w.])
    """
)
_CARD = re.compile(r"(?<!\d)(?:\d[ -]?){13,19}(?<![ -])")
_BANK = re.compile(r"\b\d{3}[ -]?\d{3}\b[\s:]*\d{6,10}\b")   # AU BSB + account
_TOKENISH = re.compile(r"(?<![A-Za-z0-9_\-])[A-Za-z0-9_\-]{24,}(?![A-Za-z0-9_\-])")


def _luhn(digits: str) -> bool:
    total, alt = 0, False
    for ch in reversed(digits):
        d = ord(ch) - 48
        if alt:
            d *= 2
            if d > 9:
                d -= 9
        total += d
        alt = not alt
    return total % 10 == 0


def _entropy(s: str) -> float:
    if not s:
        return 0.0
    counts = {c: s.count(c) for c in set(s)}
    n = len(s)
    return -sum((c / n) * math.log2(c / n) for c in counts.values())


def _find_email(text):
    for m in _EMAIL.finditer(text):
        yield m.start(), m.end(), 0.99


def _find_phone(text):
    for m in _PHONE.finditer(text):
        digits = re.sub(r"\D", "", m.group())
        if 8 <= len(digits) <= 15:
            yield m.start(), m.end(), 0.8


def _find_card(text):
    for m in _CARD.finditer(text):
        digits = re.sub(r"\D", "", m.group())
        if 13 <= len(digits) <= 19 and _luhn(digits):
            yield m.start(), m.end(), 0.95


def _find_bank(text):
    for m in _BANK.finditer(text):
        yield m.start(), m.end(), 0.85


def _find_tokenish(text):
    for m in _TOKENISH.finditer(text):
        s = m.group()
        if not (re.search(r"[A-Za-z]", s) and re.search(r"\d", s)):
            continue  # plain long word or all-digits — not a key
        if _entropy(s) < 3.5:
            continue
        yield m.start(), m.end(), 0.6


DETECTORS: list[tuple[str, object]] = [
    ("email", _find_email),
    ("card", _find_card),
    ("bank_acct", _find_bank),
    ("phone", _find_phone),
    ("key_shaped", _find_tokenish),
]


def scan(text: str) -> list[Span]:
    """All PII spans in `text`, de-overlapped (higher confidence, then longer, wins)."""
    raw: list[Span] = []
    for type_, fn in DETECTORS:
        for start, end, conf in fn(text):
            raw.append((start, end, type_, conf))

    raw.sort(key=lambda s: (-s[3], -(s[1] - s[0]), s[0]))
    kept: list[Span] = []
    for span in raw:
        if any(span[0] < k[1] and k[0] < span[1] for k in kept):
            continue
        kept.append(span)
    kept.sort(key=lambda s: s[0])
    return kept
