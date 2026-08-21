from eunomia.classifier import classify


def test_frontmatter_tag_wins():
    assert classify("random/path.md", ["work"], []) == "work"


def test_folder_path_used_when_no_tag():
    assert classify("Uni/lecture.md", [], []) == "uni"


def test_hashtag_used_when_no_tag_or_folder():
    assert classify("notes.md", [], ["business"]) == "business"


def test_defaults_to_other():
    assert classify("notes.md", [], []) == "other"


def test_tag_beats_conflicting_folder():
    assert classify("Uni/note.md", ["business"], []) == "business"


def test_case_insensitive():
    assert classify("path.md", ["WORK"], []) == "work"
