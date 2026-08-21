# Rule-based bucket classifier: frontmatter tags > folder path > inline hashtags > default "other".
# ponytail: no LLM fallback yet — that's step 2 (provider-key store). Untagged/unfoldered notes land in "other".

from pathlib import Path

RULES = {
    "uni": {"uni", "university", "school", "class", "course"},
    "work": {"work", "job", "clarify"},
    "business": {"business", "biz", "startup"},
}


def _match(keywords: set[str]) -> str | None:
    for bucket, terms in RULES.items():
        if keywords & terms:
            return bucket
    return None


def classify(path: str, tags: list[str], hashtags: list[str]) -> str:
    tag_bucket = _match({t.lower() for t in tags})
    if tag_bucket:
        return tag_bucket

    path_parts = {p.lower() for p in Path(path).parts}
    path_bucket = _match(path_parts)
    if path_bucket:
        return path_bucket

    hashtag_bucket = _match({h.lower() for h in hashtags})
    if hashtag_bucket:
        return hashtag_bucket

    return "other"
