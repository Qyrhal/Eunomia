from eunomia.classifier import classify, rules_from_buckets
from eunomia.models import Bucket

RULES = {
    "uni": {"uni", "university", "school", "class", "course"},
    "work": {"work", "job", "clarify"},
    "business": {"business", "biz", "startup"},
}


def test_frontmatter_tag_wins():
    assert classify("random/path.md", ["work"], [], RULES) == "work"


def test_folder_path_used_when_no_tag():
    assert classify("Uni/lecture.md", [], [], RULES) == "uni"


def test_hashtag_used_when_no_tag_or_folder():
    assert classify("notes.md", [], ["business"], RULES) == "business"


def test_defaults_to_other():
    assert classify("notes.md", [], [], RULES) == "other"


def test_tag_beats_conflicting_folder():
    assert classify("Uni/note.md", ["business"], [], RULES) == "business"


def test_case_insensitive():
    assert classify("path.md", ["WORK"], [], RULES) == "work"


def test_rules_from_buckets_includes_key_and_keywords():
    buckets = [Bucket(id=1, key="side-project", label="Side Project", keywords=["startup-x"])]
    rules = rules_from_buckets(buckets)
    assert rules == {"side-project": {"side-project", "startup-x"}}


def test_classify_matches_custom_bucket_by_its_own_key():
    buckets = [Bucket(id=1, key="side-project", label="Side Project", keywords=[])]
    rules = rules_from_buckets(buckets)
    assert classify("notes.md", ["side-project"], [], rules) == "side-project"
