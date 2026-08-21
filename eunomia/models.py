from dataclasses import dataclass, field

BUCKETS = ("uni", "work", "business", "other")


@dataclass
class Note:
    path: str
    title: str
    bucket: str
    tags: list[str] = field(default_factory=list)
    mtime: float = 0.0
