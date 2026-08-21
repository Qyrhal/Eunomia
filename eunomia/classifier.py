# Rule-based bucket classifier: frontmatter tags > folder path > inline hashtags > default "other".
# Rules come from the buckets table (db.get_buckets), so buckets are user-managed, not hardcoded.
# ponytail: no LLM fallback yet — that's next. Unmatched notes land in "other".

from pathlib import Path

from .models import Bucket


def rules_from_buckets(buckets: list[Bucket]) -> dict[str, set[str]]:
    """Each bucket matches its own key plus any configured keywords/synonyms."""
    return {b.key: {b.key, *b.keywords} for b in buckets}


def _match(keywords: set[str], rules: dict[str, set[str]]) -> str | None:
    for bucket, terms in rules.items():
        if keywords & {t.lower() for t in terms}:
            return bucket
    return None


def classify(path: str, tags: list[str], hashtags: list[str], rules: dict[str, set[str]]) -> str:
    tag_bucket = _match({t.lower() for t in tags}, rules)
    if tag_bucket:
        return tag_bucket

    path_parts = {p.lower() for p in Path(path).parts}
    path_bucket = _match(path_parts, rules)
    if path_bucket:
        return path_bucket

    hashtag_bucket = _match({h.lower() for h in hashtags}, rules)
    if hashtag_bucket:
        return hashtag_bucket

    return "other"
