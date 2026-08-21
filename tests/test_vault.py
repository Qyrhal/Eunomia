import pytest

from eunomia.vault import create_note, scan_vault

RULES = {
    "uni": {"uni", "university", "school", "class", "course"},
    "work": {"work", "job", "clarify"},
    "business": {"business", "biz", "startup"},
}


def test_scan_vault_classifies_each_note(vault):
    notes = scan_vault(vault, RULES)
    by_path = {n.path: n for n in notes}

    assert by_path["tagged.md"].bucket == "work"
    assert by_path["tagged.md"].tags == ["work"]
    assert by_path["tagged.md"].title == "Tagged Note"

    assert by_path["Uni/lecture.md"].bucket == "uni"
    assert by_path["hashtag.md"].bucket == "business"
    assert by_path["plain.md"].bucket == "other"


def test_broken_frontmatter_does_not_crash(vault):
    notes = scan_vault(vault, RULES)
    by_path = {n.path: n for n in notes}
    assert by_path["bad_frontmatter.md"].title == "Broken"


def test_missing_vault_returns_empty(tmp_path):
    missing = tmp_path / "does-not-exist"
    assert scan_vault(missing, RULES) == []


def test_create_note_writes_frontmatter_and_title(tmp_path):
    path = create_note(tmp_path, "My New Idea", "work")

    assert path.name == "my-new-idea.md"
    text = path.read_text()
    assert "tags: [work]" in text
    assert "# My New Idea" in text


def test_create_note_slugifies_unusual_titles(tmp_path):
    path = create_note(tmp_path, "  Q3 Roadmap: v2!! ", "work")
    assert path.name == "q3-roadmap-v2.md"


def test_create_note_raises_on_collision(tmp_path):
    create_note(tmp_path, "Duplicate", "other")
    with pytest.raises(FileExistsError):
        create_note(tmp_path, "Duplicate", "other")


def test_create_note_appears_in_scan(tmp_path):
    create_note(tmp_path, "Fresh Note", "uni")
    notes = scan_vault(tmp_path, RULES)
    assert notes[0].bucket == "uni"
    assert notes[0].title == "Fresh Note"
