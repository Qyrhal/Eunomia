from dataclasses import dataclass, field

# Seed data for the buckets table: (key, label, keywords). "other" is the fallback
# every classify() call defaults to when nothing else matches, so it always exists.
DEFAULT_BUCKETS = [
    ("uni", "Uni", ["uni", "university", "school", "class", "course"]),
    ("work", "Work", ["work", "job", "clarify"]),
    ("business", "Business", ["business", "biz", "startup"]),
    ("other", "Other", []),
]


@dataclass
class Bucket:
    id: int
    key: str
    label: str
    keywords: list[str] = field(default_factory=list)
    sort_order: int = 0


@dataclass
class Note:
    path: str
    title: str
    bucket: str
    tags: list[str] = field(default_factory=list)
    mtime: float = 0.0


@dataclass
class Credential:
    id: int
    service: str
    label: str
    kind: str  # "oauth" | "api_key"
    secret_encrypted: str
    account: str | None = None
    created_at: float = 0.0
