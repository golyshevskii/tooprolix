"""
Guards for promises that live in `.github/workflows/` rather than in code.

A workflow file is not documentation: it decides which commit gets built, and a wrong key in it
fails silently — as a `cancel`, which reads as "not red" and blocks nothing. This file pins the
workflow settings whose breakage would otherwise be invisible until a release.

Run: make test
"""

from __future__ import annotations

import re
from pathlib import Path

BUILD_ARTIFACTS: Path = Path(__file__).parents[2] / ".github" / "workflows" / "build-artifacts.yml"

CONCURRENCY_GROUP = re.compile(r"^concurrency:\n\s+group:\s*(?P<group>.+)$", re.MULTILINE)


def test_an_artifact_build_is_only_cancelled_by_another_build_of_the_same_commit() -> None:
    """
    The artifact build must belong to the COMMIT it built, not to the branch it arrived on.

    `build-artifacts.yml` triggers on every push with no branch filter, so one logical event can
    produce two pushes to one new ref within seconds — release-plz creates the branch at the base
    tip and pushes the release commit, and GitHub delivers those as separate events. Keyed on
    `github.ref` with `cancel-in-progress`, the later run kills the earlier one on the assumption
    that later means newer. That assumption is false here, and it lost the race for real: on
    PR #37 (`chore: release v0.4.3`) all four artifact checks came back `cancel`, and the only
    surviving build was of the base tip — a commit that PR does not propose. PR #35 raced the same
    way and happened to win, which is worse than losing, because it makes the defect look absent.

    Keyed on `github.sha`, a build can only be cancelled by another run of the same commit, so a
    commit that reaches a tag has artifacts built from its own tree. Duplicate events for one SHA
    still collapse, which is all `cancel-in-progress` was ever wanted for.
    """
    matched = CONCURRENCY_GROUP.search(BUILD_ARTIFACTS.read_text(encoding="utf-8"))
    assert matched is not None, "build-artifacts.yml declares no concurrency group"
    group: str = matched.group("group")

    assert "github.sha" in group, f"artifact builds must be grouped by commit, group is {group!r}"
    assert "github.ref" not in group, f"grouping by ref cancels other commits' builds, group is {group!r}"
