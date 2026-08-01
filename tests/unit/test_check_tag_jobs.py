"""
The publish precondition: every expected job of a `v*` tag ran, and every one RAW-concluded success.

Each test here names a way a weaker check passes while the guarantee is broken. The three the
acceptance criteria single out are a conclusion flipped to `cancelled`, an expected job absent from
the run entirely, and the expectation reduced to `[]` — the last one separately, because a mutation
over a populated set cannot detect it. A fourth was found by review: a run that carries every
expected name but belongs to some OTHER commit.

Run: make test
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest
from check_tag_jobs import main

#: The tag being published, and a commit that is not it.
TAG_SHA: str = "a1" * 20
OTHER_SHA: str = "b2" * 20

#: A tag run in which everything the manifest names ran and succeeded.
COMPLETE: tuple[tuple[str, str], ...] = (
    ("ci-python", "success"),
    ("ci-rust", "success"),
    ("cargo-doc", "success"),
    ("ci-required", "success"),
)


def concluding(name: str, conclusion: Any) -> tuple[tuple[str, Any], ...]:
    """
    Return `COMPLETE` with exactly one job's conclusion replaced, and nothing else moved.

    Written by name rather than by index because doing it by hand got it wrong: a fixture spelled
    `(("ci-rust", "skipped"), *COMPLETE[1:])` PREPENDS a job and drops `ci-python`, so the run is
    both short of an expected job and holds `ci-rust` twice. That test then went red through the
    set-equality guard even with the raw-conclusion check deleted — a mutation proof that proved
    nothing.
    """
    replaced = tuple((job, conclusion if job == name else result) for job, result in COMPLETE)
    assert replaced != COMPLETE, f"{name} is not in COMPLETE, so this fixture mutates nothing"
    return replaced


def payload(
    path: Path,
    jobs: tuple[tuple[str, Any], ...],
    *,
    head_sha: str | None = TAG_SHA,
    run_id: int | None = 111,
    run_attempt: int | None = 1,
) -> Path:
    """Write a file shaped like `gh api repos/…/actions/runs/<id>/jobs`."""
    entries: list[dict[str, Any]] = []
    for name, conclusion in jobs:
        entry: dict[str, Any] = {"name": name, "conclusion": conclusion}
        if head_sha is not None:
            entry["head_sha"] = head_sha
        if run_id is not None:
            entry["run_id"] = run_id
        if run_attempt is not None:
            entry["run_attempt"] = run_attempt
        entries.append(entry)
    path.write_text(json.dumps({"total_count": len(entries), "jobs": entries}), encoding="utf-8")
    return path


def manifest(path: Path, names: tuple[str, ...]) -> Path:
    path.write_text("# the tracked expectation\n" + "".join(f"{name}\n" for name in names), encoding="utf-8")
    return path


def grade(
    tmp_path: Path,
    jobs: tuple[tuple[str, Any], ...],
    expected: tuple[str, ...],
    *,
    tag_sha: str = TAG_SHA,
    **payload_kwargs: Any,
) -> int:
    return main(
        [
            str(payload(tmp_path / "jobs.json", jobs, **payload_kwargs)),
            "--manifest",
            str(manifest(tmp_path / "expected.txt", expected)),
            "--tag-sha",
            tag_sha,
        ]
    )


def names() -> tuple[str, ...]:
    return tuple(name for name, _ in COMPLETE)


def test_a_tag_whose_every_expected_job_succeeded_is_publishable(tmp_path: Path) -> None:
    """The baseline the mutations below break — without it, a check that always fails looks correct."""
    assert grade(tmp_path, COMPLETE, names()) == 0


def test_a_cancelled_job_blocks_publication(tmp_path: Path) -> None:
    """
    `cancelled` is not `success`, and treating "not failed" as "passed" is how this repository
    already shipped a green-looking release: on PR #37 all four artifact checks came back `cancel`
    and the only surviving build was of a commit that PR does not propose.
    """
    assert grade(tmp_path, concluding("ci-python", "cancelled"), names()) == 1


def test_a_skipped_job_blocks_publication(tmp_path: Path) -> None:
    """
    A skipped job is the failure mode a path filter or a job `if:` introduces: the check reports a
    conclusion that is not a failure and never ran the work. Graded the same as `cancelled`.
    """
    assert grade(tmp_path, concluding("ci-rust", "skipped"), names()) == 1


def test_an_expected_job_that_never_ran_blocks_publication(tmp_path: Path) -> None:
    """
    The comparison is set equality, never "contains". A job deleted from CI leaves every REMAINING
    job green, so a check that only inspects what ran cannot see the deletion at all.
    """
    assert grade(tmp_path, COMPLETE[1:], names()) == 1


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
    assert grade(tmp_path, (*COMPLETE, ("mystery", "success")), names()) == 1


def test_a_job_that_has_not_finished_blocks_publication(tmp_path: Path) -> None:
    """A running job reports `conclusion: null`, and "not yet failed" is not "succeeded"."""
    assert grade(tmp_path, concluding("ci-python", None), names()) == 1


# ---------------------------------------------------------------------------------------------
# The binding to the tag. Without it the check grades a SHAPE, and any green run has that shape.
# ---------------------------------------------------------------------------------------------


def test_a_run_of_another_commit_blocks_publication(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    """
    Every expected name, every conclusion `success` — and the run is of a different commit.

    This is the bypass the first version of the script accepted: hand it two old green
    feature-branch runs and it certified a tag nothing had tested.
    """
    assert grade(tmp_path, COMPLETE, names(), head_sha=OTHER_SHA) == 1
    assert OTHER_SHA in capsys.readouterr().err


def test_a_payload_that_names_no_commit_at_all_blocks_publication(tmp_path: Path) -> None:
    """
    The exact payload that exited 0 under review: every expected job, no `head_sha` key anywhere.

    A missing field must be graded as "not the tag", never as "nothing to object to".
    """
    assert grade(tmp_path, COMPLETE, names(), head_sha=None) == 1


def test_a_payload_stitched_from_two_runs_blocks_publication(tmp_path: Path) -> None:
    """
    `--paginate` walks the pages of ONE run, so jobs disagreeing about `run_id` were assembled by
    hand. Pooling them would let a green job from any other run stand in for a missing one.
    """
    stitched = tmp_path / "jobs.json"
    stitched.write_text(
        json.dumps(
            {
                "jobs": [
                    {
                        "name": "ci-python",
                        "conclusion": "success",
                        "head_sha": TAG_SHA,
                        "run_id": 111,
                        "run_attempt": 1,
                    },
                    {"name": "ci-rust", "conclusion": "success", "head_sha": TAG_SHA, "run_id": 222, "run_attempt": 1},
                ]
            }
        ),
        encoding="utf-8",
    )
    assert (
        main(
            [
                str(stitched),
                "--manifest",
                str(manifest(tmp_path / "e.txt", ("ci-python", "ci-rust"))),
                "--tag-sha",
                TAG_SHA,
            ]
        )
        == 1
    )


def test_a_payload_that_names_no_run_blocks_publication(tmp_path: Path) -> None:
    """A job that cannot be tied to a run cannot be tied to the tag's run either."""
    assert grade(tmp_path, COMPLETE, names(), run_id=None) == 1


@pytest.mark.parametrize("given", ["", "a1a1a1a", "not-a-sha", "A1" * 20])
def test_a_tag_sha_that_is_not_a_full_commit_is_refused(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], given: str
) -> None:
    """
    An abbreviated or empty `--tag-sha` matches no `head_sha`, so it would fail for the wrong
    reason today and pass for the wrong reason the moment the comparison were loosened. Refused at
    the door instead, where the message names the real fault.

    The DIAGNOSIS is asserted, not only the exit code: deleting the shape check leaves an
    exit-code-only version of this test green, because the `head_sha` comparison then fails instead
    and blames the run for a fault that is in the argument. Measured.
    """
    assert grade(tmp_path, COMPLETE, names(), tag_sha=given) == 1
    assert "--tag-sha" in capsys.readouterr().err


def test_a_missing_manifest_blocks_publication(tmp_path: Path) -> None:
    """A guard that cannot find its expectation must refuse, not default to expecting nothing."""
    jobs = payload(tmp_path / "jobs.json", COMPLETE)
    assert main([str(jobs), "--manifest", str(tmp_path / "absent.txt"), "--tag-sha", TAG_SHA]) == 1


@pytest.mark.parametrize(
    ("document", "diagnosis"),
    [
        ('{"message": "Not Found"}', "is not an"),
        ('{"jobs": null}', "is not an"),
        ("[1, 2]", "is not an"),
        ('{"jobs": ["ci-python"]}', "not an object"),
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

    argv = [str(broken), "--manifest", str(manifest(tmp_path / "expected.txt", ("ci-python",))), "--tag-sha", TAG_SHA]
    assert main(argv) == 1
    assert diagnosis in capsys.readouterr().err


def test_the_two_workflows_a_tag_fires_are_graded_together(tmp_path: Path) -> None:
    """
    A `v*` tag fires `CI` and `Build artifacts`, so the expectation spans both runs and the check
    has to be given both payloads. Grading them one at a time would make each run's payload a
    superset failure against the other's names. Two files, two run ids, one tag.
    """
    ci = payload(tmp_path / "ci.json", COMPLETE, run_id=111)
    artifacts = payload(tmp_path / "artifacts.json", (("wheel macos-arm64", "success"),), run_id=222)
    expected = manifest(tmp_path / "expected.txt", (*names(), "wheel macos-arm64"))

    assert main([str(ci), str(artifacts), "--manifest", str(expected), "--tag-sha", TAG_SHA]) == 0
