"""
The publish precondition: every expected job of a `v*` tag ran, and every one RAW-concluded success.

Each test names a way a weaker check passes while the guarantee is broken — a conclusion flipped to
`cancelled`, an expected job absent from the run, the expectation reduced to `[]`, and a run that
carries every expected name but belongs to another commit.

Run: make test
"""

from __future__ import annotations

import json
import shlex
from pathlib import Path
from typing import Any

import check_tag_jobs
import pytest
from check_tag_jobs import main

#: The tag being published, and a commit that is not it.
TAG_SHA: str = "a1" * 20
OTHER_SHA: str = "b2" * 20
TAG_NAME: str = "v0.4.8"

#: The workflow the fixture jobs belong to. A tag fires two; `CI` is the one most tests need.
WORKFLOW: str = "CI"

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

    By name, not by index: `(("ci-rust", "skipped"), *COMPLETE[1:])` prepends a job and drops
    `ci-python`, so the fixture is short one expected job and the test then goes red through the
    set-equality guard even with the raw-conclusion check deleted.
    """
    replaced = tuple((job, conclusion if job == name else result) for job, result in COMPLETE)
    assert replaced != COMPLETE, f"{name} is not in COMPLETE, so this fixture mutates nothing"
    return replaced


def payload(
    path: Path,
    jobs: tuple[tuple[str, Any], ...],
    *,
    head_sha: str | None = TAG_SHA,
    head_branch: str | None = TAG_NAME,
    workflow_name: str = WORKFLOW,
    run_id: int | None = 111,
    run_attempt: int | None = 1,
) -> Path:
    """Write a file shaped like `gh api repos/…/actions/runs/<id>/jobs` — the real field set."""
    entries: list[dict[str, Any]] = []
    for name, conclusion in jobs:
        entry: dict[str, Any] = {"name": name, "conclusion": conclusion, "workflow_name": workflow_name}
        if head_sha is not None:
            entry["head_sha"] = head_sha
        if head_branch is not None:
            entry["head_branch"] = head_branch
        if run_id is not None:
            entry["run_id"] = run_id
        if run_attempt is not None:
            entry["run_attempt"] = run_attempt
        entries.append(entry)
    path.write_text(json.dumps({"total_count": len(entries), "jobs": entries}), encoding="utf-8")
    return path


def manifest(path: Path, names: tuple[str, ...], workflow: str = WORKFLOW) -> Path:
    """Write the sectioned expectation: a `[workflow name]` header, then its job names."""
    body = f"[{workflow}]\n" + "".join(f"{name}\n" for name in names) if names else ""
    path.write_text("# the tracked expectation\n" + body, encoding="utf-8")
    return path


def grade(
    tmp_path: Path,
    jobs: tuple[tuple[str, Any], ...],
    expected: tuple[str, ...],
    *,
    tag_sha: str = TAG_SHA,
    tag_name: str = TAG_NAME,
    expected_workflow: str = WORKFLOW,
    **payload_kwargs: Any,
) -> int:
    return main(
        [
            str(payload(tmp_path / "jobs.json", jobs, **payload_kwargs)),
            "--manifest",
            str(manifest(tmp_path / "expected.txt", expected, expected_workflow)),
            "--tag-sha",
            tag_sha,
            "--tag-name",
            tag_name,
        ]
    )


def names() -> tuple[str, ...]:
    return tuple(name for name, _ in COMPLETE)


def test_a_tag_whose_every_expected_job_succeeded_is_publishable(tmp_path: Path) -> None:
    """The baseline the mutations below break — without it, a check that always fails looks correct."""
    assert grade(tmp_path, COMPLETE, names()) == 0


def test_a_cancelled_job_blocks_publication(tmp_path: Path) -> None:
    """
    Treating "not failed" as "passed" is how PR #37 shipped a green-looking release: all four
    artifact checks came back `cancel` and the only surviving build was of a commit that PR does not
    propose.
    """
    assert grade(tmp_path, concluding("ci-python", "cancelled"), names()) == 1


def test_a_skipped_job_blocks_publication(tmp_path: Path) -> None:
    """
    The failure mode a path filter or a job `if:` introduces: a conclusion that is not a failure
    over work that never ran. Graded the same as `cancelled`.
    """
    assert grade(tmp_path, concluding("ci-rust", "skipped"), names()) == 1


def test_an_expected_job_that_never_ran_blocks_publication(tmp_path: Path) -> None:
    """
    Set equality, never "contains": a job deleted from CI leaves every remaining job green, so a
    check that only inspects what ran cannot see the deletion.
    """
    assert grade(tmp_path, COMPLETE[1:], names()) == 1


def test_an_empty_expectation_blocks_publication(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    """
    The vacuous-guard hole: shrink the workflow and the manifest together and a set-equality check
    passes having verified that no jobs equal no jobs. No populated-set mutation can reach it.

    The DIAGNOSIS is asserted because the payload reader also refuses a run with no jobs, so an
    exit-code-only assertion stayed green with the manifest check deleted.
    """
    assert grade(tmp_path, (), ()) == 1
    assert "expects no jobs at all" in capsys.readouterr().err


def test_an_unexpected_job_blocks_publication(tmp_path: Path) -> None:
    """
    A superset must fail as loudly as a subset: a job nobody declared is either a stale manifest or
    a workflow someone else added, and both are answers a human owes the release.
    """
    assert grade(tmp_path, (*COMPLETE, ("mystery", "success")), names()) == 1


def test_the_environment_gated_publish_job_is_not_a_circular_preapproval_requirement(tmp_path: Path) -> None:
    """The manifest can be approved while Publish is waiting and therefore has no conclusion."""
    ci = payload(tmp_path / "ci.json", COMPLETE, run_id=111)
    artifact_names = (
        "sdist (+ the wheel built from it)",
        "wheel linux-x86_64",
        "wheel macos-arm64",
        "wheel windows-x86_64",
        "PyPI release manifest",
    )
    artifacts = payload(
        tmp_path / "artifacts.json",
        (*((name, "success") for name in artifact_names), ("Publish to PyPI", None)),
        workflow_name="Build artifacts",
        run_id=222,
    )
    expectation = tmp_path / "expected.txt"
    expectation.write_text(
        "[CI]\n"
        + "".join(f"{name}\n" for name in names())
        + "[Build artifacts]\n"
        + "".join(f"{name}\n" for name in artifact_names),
        encoding="utf-8",
    )

    assert (
        main([str(ci), str(artifacts), "--manifest", str(expectation), "--tag-sha", TAG_SHA, "--tag-name", TAG_NAME])
        == 0
    )


def test_only_the_named_publish_job_is_exempt_from_preapproval_success(tmp_path: Path) -> None:
    """Any other pending or unexpected job still fails closed."""
    assert grade(tmp_path, (*COMPLETE, ("publish-lookalike", None)), names()) == 1


def test_a_job_that_has_not_finished_blocks_publication(tmp_path: Path) -> None:
    """A running job reports `conclusion: null`, and "not yet failed" is not "succeeded"."""
    assert grade(tmp_path, concluding("ci-python", None), names()) == 1


# ---------------------------------------------------------------------------------------------
# The binding to the tag. Without it the check grades a SHAPE, and any green run has that shape.
# ---------------------------------------------------------------------------------------------


def test_a_run_of_another_commit_blocks_publication(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    """
    Every expected name, every conclusion `success`, and the run is of a different commit — the
    bypass the first version accepted, certifying a tag from two old green feature-branch runs.
    """
    assert grade(tmp_path, COMPLETE, names(), head_sha=OTHER_SHA) == 1
    assert OTHER_SHA in capsys.readouterr().err


def test_a_payload_that_names_no_commit_at_all_blocks_publication(tmp_path: Path) -> None:
    """
    Every expected job, no `head_sha` key anywhere — the exact payload that exited 0 under review.
    A missing field is "not the tag", never "nothing to object to".
    """
    assert grade(tmp_path, COMPLETE, names(), head_sha=None) == 1


def test_a_payload_stitched_from_two_runs_blocks_publication(tmp_path: Path) -> None:
    """
    `--paginate` walks the pages of ONE run, so jobs disagreeing about `run_id` were assembled by
    hand — and pooling them lets a green job from another run stand in for a missing one.
    """

    def job(name: str, run_id: int) -> dict[str, Any]:
        return {
            "name": name,
            "conclusion": "success",
            "workflow_name": WORKFLOW,
            "head_sha": TAG_SHA,
            "head_branch": TAG_NAME,
            "run_id": run_id,
            "run_attempt": 1,
        }

    stitched = tmp_path / "jobs.json"
    stitched.write_text(json.dumps({"jobs": [job("ci-python", 111), job("ci-rust", 222)]}), encoding="utf-8")
    expectation = manifest(tmp_path / "e.txt", ("ci-python", "ci-rust"))

    assert main([str(stitched), "--manifest", str(expectation), "--tag-sha", TAG_SHA, "--tag-name", TAG_NAME]) == 1


def test_a_run_of_another_workflow_blocks_publication(tmp_path: Path) -> None:
    """
    The same commit does not establish that the TAG-PUSH workflows ran. Before the manifest carried
    workflow names, a payload with `workflow_name: Some other workflow` dispatched on `main` and
    every expected job name exited 0.
    """
    assert grade(tmp_path, COMPLETE, names(), workflow_name="Some other workflow") == 1


def test_a_run_of_a_branch_at_the_same_commit_blocks_publication(tmp_path: Path) -> None:
    """
    A tag and a branch can point at the same commit, and a run of the branch is not a run of the
    tag. On a real tag-push run `head_branch` is the tag: `v0.4.7`, run 30703436113.
    """
    assert grade(tmp_path, COMPLETE, names(), head_branch="main") == 1


@pytest.mark.parametrize("given", ["", "0.4.8", "release-v0.4.8"])
def test_a_tag_name_that_is_not_a_v_tag_is_refused(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], given: str
) -> None:
    """`v*` is the pattern both workflows trigger on, so anything else is not a tag they ran for."""
    assert grade(tmp_path, COMPLETE, names(), tag_name=given) == 1
    assert "--tag-name" in capsys.readouterr().err


def test_a_manifest_listing_a_job_before_any_workflow_header_is_refused(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """
    A job with no workflow above it has no binding at all, so it must not be attached to whatever
    section follows. The DIAGNOSIS is asserted: a version that quietly attached the orphan to `CI`
    still exited 1 through the set-equality check downstream and left this test green.
    """
    broken = tmp_path / "expected.txt"
    broken.write_text("ci-python\n[CI]\nci-rust\n", encoding="utf-8")
    jobs = payload(tmp_path / "jobs.json", COMPLETE)

    assert main([str(jobs), "--manifest", str(broken), "--tag-sha", TAG_SHA, "--tag-name", TAG_NAME]) == 1
    assert "before any [workflow name] header" in capsys.readouterr().err


def test_a_payload_that_names_no_run_blocks_publication(tmp_path: Path) -> None:
    """A job that cannot be tied to a run cannot be tied to the tag's run either."""
    assert grade(tmp_path, COMPLETE, names(), run_id=None) == 1


@pytest.mark.parametrize("given", ["", "a1a1a1a", "not-a-sha", "A1" * 20])
def test_a_tag_sha_that_is_not_a_full_commit_is_refused(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], given: str
) -> None:
    """
    An abbreviated `--tag-sha` matches no `head_sha`, so it fails for the wrong reason today and
    would pass for the wrong one if the comparison were loosened. The DIAGNOSIS is asserted:
    without the shape check the `head_sha` comparison fails instead, blaming the run for a fault
    that is in the argument.
    """
    assert grade(tmp_path, COMPLETE, names(), tag_sha=given) == 1
    assert "--tag-sha" in capsys.readouterr().err


def test_the_documented_runbook_command_parses(tmp_path: Path) -> None:
    """
    The docstring IS the publication runbook, so it must be executable, not illustrative: before
    `--tag-name` was added to it the documented line exited 2 on
    `error: the following arguments are required`. The command is parsed out of the module docstring
    so the two cannot drift. `$(…)` is substituted with a real SHA because this grades the ARGUMENT
    LIST: exit 1 means argparse accepted it and the missing payloads were refused; exit 2 would mean
    the documented arguments are wrong.
    """
    documented = next(
        line.strip() for line in (check_tag_jobs.__doc__ or "").splitlines() if "check_tag_jobs.py --" in line
    )
    argv = [
        TAG_SHA if token.startswith("$(") else token
        for token in shlex.split(documented.replace("$(git rev-parse v0.4.8^{commit})", "$(sha)"))
    ][2:]

    assert main(argv) == 1, "the documented command must be refused on its missing payloads, not on its arguments"


def test_a_missing_manifest_blocks_publication(tmp_path: Path) -> None:
    """A guard that cannot find its expectation must refuse, not default to expecting nothing."""
    jobs = payload(tmp_path / "jobs.json", COMPLETE)
    assert (
        main([str(jobs), "--manifest", str(tmp_path / "absent.txt"), "--tag-sha", TAG_SHA, "--tag-name", TAG_NAME]) == 1
    )


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
    An API error body or an empty document must not read as "no jobs, nothing wrong":
    `{"message": "Not Found"}` is what a wrong run id returns, valid JSON with no `jobs` key.

    The DIAGNOSIS is asserted because a lenient reader that collected nothing would still exit 1
    through the set-equality check downstream — measured, an exit-code-only version stayed GREEN
    with the shape check deleted.
    """
    broken = tmp_path / "jobs.json"
    broken.write_text(document, encoding="utf-8")

    argv = [
        str(broken),
        "--manifest",
        str(manifest(tmp_path / "expected.txt", ("ci-python",))),
        "--tag-sha",
        TAG_SHA,
        "--tag-name",
        TAG_NAME,
    ]
    assert main(argv) == 1
    assert diagnosis in capsys.readouterr().err


def test_the_two_workflows_a_tag_fires_are_graded_together(tmp_path: Path) -> None:
    """
    A `v*` tag fires `CI` and `Build artifacts`, so the expectation spans both runs and both
    payloads must be given at once — one at a time, each is a superset failure against the other.
    """
    ci = payload(tmp_path / "ci.json", COMPLETE, run_id=111)
    artifacts = payload(tmp_path / "artifacts.json", (("wheel macos-arm64", "success"),), run_id=222)
    expected = manifest(tmp_path / "expected.txt", (*names(), "wheel macos-arm64"))

    assert (
        main([str(ci), str(artifacts), "--manifest", str(expected), "--tag-sha", TAG_SHA, "--tag-name", TAG_NAME]) == 0
    )
