from eunomia.vault import scan_vault


def test_scan_vault_classifies_each_note(vault):
    notes = scan_vault(vault)
    by_path = {n.path: n for n in notes}

    assert by_path["tagged.md"].bucket == "work"
    assert by_path["tagged.md"].tags == ["work"]
    assert by_path["tagged.md"].title == "Tagged Note"

    assert by_path["Uni/lecture.md"].bucket == "uni"
    assert by_path["hashtag.md"].bucket == "business"
    assert by_path["plain.md"].bucket == "other"


def test_broken_frontmatter_does_not_crash(vault):
    notes = scan_vault(vault)
    by_path = {n.path: n for n in notes}
    assert by_path["bad_frontmatter.md"].title == "Broken"


def test_missing_vault_returns_empty(tmp_path):
    missing = tmp_path / "does-not-exist"
    assert scan_vault(missing) == []
