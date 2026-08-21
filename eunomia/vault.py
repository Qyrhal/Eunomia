# Scan an Obsidian vault: parse frontmatter + inline hashtags, classify each note.
# Also handles writing new notes into the vault.

import re
import unicodedata
from pathlib import Path

import yaml

from .classifier import classify
from .models import Note

HASHTAG_RE = re.compile(r"(?<!\S)#([A-Za-z0-9_-]+)")
FRONTMATTER_RE = re.compile(r"\A---\n(.*?)\n---\n", re.DOTALL)


def _parse_frontmatter(text: str) -> tuple[dict, str]:
    match = FRONTMATTER_RE.match(text)
    if not match:
        return {}, text
    body = text[match.end():]
    try:
        data = yaml.safe_load(match.group(1)) or {}
    except yaml.YAMLError:
        data = {}
    return data, body


def _extract_tags(frontmatter: dict) -> list[str]:
    tags = frontmatter.get("tags", [])
    if isinstance(tags, str):
        tags = [tags]
    return [str(t) for t in tags]


def _title_from(body: str, fallback: str) -> str:
    for line in body.splitlines():
        line = line.strip()
        if line.startswith("# "):
            return line[2:].strip()
    return fallback


def parse_note(path: Path, vault_root: Path, rules: dict[str, set[str]]) -> Note:
    text = path.read_text(encoding="utf-8")
    frontmatter, body = _parse_frontmatter(text)
    tags = _extract_tags(frontmatter)
    hashtags = HASHTAG_RE.findall(body)
    rel_path = str(path.relative_to(vault_root))
    bucket = classify(rel_path, tags, hashtags, rules)
    return Note(
        path=rel_path,
        title=_title_from(body, path.stem),
        bucket=bucket,
        tags=tags,
        mtime=path.stat().st_mtime,
    )


def scan_vault(vault_root: Path, rules: dict[str, set[str]]) -> list[Note]:
    if not vault_root.exists():
        return []
    return [parse_note(p, vault_root, rules) for p in sorted(vault_root.rglob("*.md"))]


def _slugify(title: str) -> str:
    ascii_title = unicodedata.normalize("NFKD", title).encode("ascii", "ignore").decode()
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", ascii_title).strip("-").lower()
    return slug or "note"


def create_note(vault_root: Path, title: str, bucket_key: str) -> Path:
    """Write a new .md file with the title as an H1 and the bucket as a frontmatter tag.
    Returns the created file's path. Raises FileExistsError if the slug collides."""
    slug = _slugify(title)
    path = vault_root / f"{slug}.md"
    if path.exists():
        raise FileExistsError(f"{path.name} already exists in the vault")
    path.write_text(f"---\ntags: [{bucket_key}]\n---\n# {title}\n\n", encoding="utf-8")
    return path
