"""
The publish precondition: every expected job of a `v*` tag ran, and every one RAW-concluded success.

Each test here names a way a weaker check passes while the guarantee is broken. The three the
acceptance criteria single out are a conclusion flipped to `cancelled`, an expected job absent from
the run entirely, and the expectation reduced to `[]` — the last one separately, because a mutation
over a populated set cannot detect it.

Run: make test
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest
from check_tag_jobs import main

#: A tag run in which everything the manifest names ran and succeeded.
COMPLETE: tuple[tuple[str, str], ...] = (
    ("ci-python", "success"),
    ("ci-rust", "success"),
    ("cargo-doc", "success"),
    ("ci-required", "success"),
)


def payload(path: Path, jobs: tuple[tuple[str, Any], ...]) -> Path:
    """Write a file shaped like `gh api repos/…/actions/runs/<id>/jobs`."""
    path.write_text(
        json.dumps(
            {"total_count": len(jobs), "jobs": [{"name": name, "conclusion": conclusion} for name, conclusion in jobs]}
        ),
        encoding="utf-8",
    )
    return path


def manifest(path: Path, names: tuple[str, ...]) -> Path:
    path.write_text("# the tracked expectation\n" + "".join(f"{name}\n" for name in names), encoding="utf-8")
    return path


def grade(tmp_path: Path, jobs: tuple[tuple[str, Any], ...], expected: tuple[str, ...]) -> int:
    return main(
        [str(payload(tmp_path / "jobs.json", jobs)), "--manifest", str(manifest(tmp_path / "expected.txt", expected))]
    )


def test_a_tag_whose_every_expected_job_succeeded_is_publishable(tmp_path: Path) -> None:
    """The baseline the mutations below break — without it, a check that always fails looks correct."""
    assert grade(tmp_path, COMPLETE, tuple(name for name, _ in COMPLETE)) == 0


def test_a_cancelled_job_blocks_publication(tmp_path: Path) -> None:
    """
    `cancelled` is not `success`, and treating "not failed" as "passed" is how this repository
    already shipped a green-looking release: on PR #37 all four artifact checks came back `cancel`
    and the only surviving build was of a commit that PR does not propose.
    """
    mutated = (("ci-python", "cancelled"), *COMPLETE[1:])
    assert grade(tmp_path, mutated, tuple(name for name, _ in COMPLETE)) == 1


def test_a_skipped_job_blocks_publication(tmp_path: Path) -> None:
    """
    A skipped job is the failure mode a path filter or a job `if:` introduces: the check reports a
    conclusion that is not a failure and never ran the work. Graded the same as `cancelled`.
    """
    mutated = (("ci-rust", "skipped"), *COMPLETE[1:])
    assert grade(tmp_path, mutated, tuple(name for name, _ in COMPLETE)) == 1


def test_an_expected_job_that_never_ran_blocks_publication(tmp_path: Path) -> None:
    """
    The comparison is set equality, never "contains". A job deleted from CI leaves every REMAINING
    job green, so a check that only inspects what ran cannot see the deletion at all.
    """
    assert grade(tmp_path, COMPLETE[1:], tuple(name for name, _ in COMPLETE)) == 1


def test_an_empty_expectation_blocks_publication(tmp_path: Path) -> None:
    """
    🔴 The vacuous-guard hole, and it needs its own test because a populated-set mutation cannot
    reach it: shrink the tag workflow and the manifest to nothing TOGETHER and a set-equality check
    passes having verified that no jobs equal no jobs.
    """
    assert grade(tmp_path, (), ()) == 1


def test_an_unexpected_job_blocks_publication(tmp_path: Path) -> None:
    """
    A superset must fail as loudly as a subset. A job nobody declared is either a manifest that was
    not updated or a workflow somebody else's branch added, and both are answers a human owes the
    release, not something to wave through because everything visible was green.
    """
    assert grade(tmp_path, (*COMPLETE, ("mystery", "success")), tuple(name for name, _ in COMPLETE)) == 1


def test_a_job_that_has_not_finished_blocks_publication(tmp_path: Path) -> None:
    """A running job reports `conclusion: null`, and "not yet failed" is not "succeeded"."""
    mutated = (("ci-python", None), *COMPLETE[1:])
    assert grade(tmp_path, mutated, tuple(name for name, _ in COMPLETE)) == 1


def test_a_missing_manifest_blocks_publication(tmp_path: Path) -> None:
    """A guard that cannot find its expectation must refuse, not default to expecting nothing."""
    jobs = payload(tmp_path / "jobs.json", COMPLETE)
    assert main([str(jobs), "--manifest", str(tmp_path / "absent.txt")]) == 1


@pytest.mark.parametrize(
    ("document", "diagnosis"),
    [
        ('{"message": "Not Found"}', "is not an"),
        ('{"jobs": null}', "is not an"),
        ("[1, 2]", "is not an"),
        ("[]", "no job was read"),
    ],
)
def test_a_payload_that_is_not_a_jobs_response_blocks_publication(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], document: str, diagnosis: str
) -> None:
    """
    An API error body, an empty document or a truncated page must not read as "no jobs, nothing
    wrong". `{"message": "Not Found"}` is what a wrong run id returns, and it has no `jobs` key at
    all — the exact input a lenient reader turns into a silent pass.

    The DIAGNOSIS is asserted, not only the exit code, and that is the difference between this test
    and a weaker one. A lenient reader that silently collected nothing would still exit 1 here —
    the set-equality check downstream would report every expected job as missing — so an exit-code
    assertion alone passes whether or not the payload guard exists at all. Measured: deleting the
    shape check left an exit-code-only version of this test GREEN.
    """
    broken = tmp_path / "jobs.json"
    broken.write_text(document, encoding="utf-8")

    assert main([str(broken), "--manifest", str(manifest(tmp_path / "expected.txt", ("ci-python",)))]) == 1
    assert diagnosis in capsys.readouterr().err


def test_several_runs_are_graded_together(tmp_path: Path) -> None:
    """
    A `v*` tag fires two workflows — `CI` and `Build artifacts` — so the expectation spans both and
    the check has to be given both payloads. Grading them one at a time would make each run's
    payload a superset failure against the other's names.
    """
    ci = payload(tmp_path / "ci.json", COMPLETE)
    artifacts = payload(tmp_path / "artifacts.json", (("wheel macos-arm64", "success"),))
    expected = manifest(tmp_path / "expected.txt", (*(name for name, _ in COMPLETE), "wheel macos-arm64"))

    assert main([str(ci), str(artifacts), "--manifest", str(expected)]) == 0
