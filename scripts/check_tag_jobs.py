"""
Refuse to approve publication of a `v*` tag unless every expected prerequisite of that tag ran and
RAW-concluded `success`.

The defect this closes is grading a self-report: a run's `head_sha` is a label the run attaches to
itself, and an expected-job list derived from the run agrees with whatever the run did. Run
30691798694 reports `head_sha=292d8af4` while `actions/checkout` materialised tree `3a1f67fd`. So
the expectation is pinned in a tracked file (`.github/expected-tag-jobs.txt`) that a human edits in
a reviewable diff, and the observation is each job's RAW `conclusion` — `cancelled` and `skipped`
are not `success`, and reading them as "not failed" is how PR #37's four cancelled artifact checks
looked green.

Three ways a weaker version passes while the guarantee is broken, all refused here:

  * a conclusion flipped to `cancelled` — the raw value is compared to `success`, not to `failure`;
  * an expected job absent from the run — the comparison is SET EQUALITY after excluding exactly
    the environment-gated `Build artifacts / Publish to PyPI` job, never "contains", so any other
    superset fails as loudly as a subset;
  * the expectation reduced to `[]` — refused by its own assertion, because a set-equality check
    over an empty expectation passes having verified nothing and no populated-set test can see it.

It also binds the jobs to the tag and to the workflows a tag push fires. Without that, a payload
naming every expected job with no `head_sha` at all exited 0, and so did one whose `workflow_name`
was a foreign workflow dispatched on `main`. Every job must now report the tag's commit and the tag
as `head_branch`, the expectation is `(workflow name, job name)` pairs, and one payload must be one
run and one attempt so a stitched-together file is refused rather than averaged.

WHAT THIS CANNOT ESTABLISH:

  * `head_sha` is still a label the RUN attaches to itself. It proves the run claims the tag, not
    that `actions/checkout` materialised it. What proves that is the last step of every job that
    checks out, in `ci.yml` and `build-artifacts.yml`, asserting `git rev-parse HEAD == $GITHUB_SHA`
    after its gates have run — unreadable from here, which is why it fails the job instead.
  * the run's EVENT is not in this payload, and `workflow_name` is only a display label.
    `actions/runs/<id>/jobs` carries `workflow_name`, `head_branch`, `head_sha`, `run_id` and
    `run_attempt`, but `event` lives on `actions/runs/<id>`. The release-manifest and publish jobs
    therefore independently require `push` both in their job conditions and shipped shell guards;
    a dispatch at the tag ref cannot produce the required manifest job.
  * the payload is trusted to have come from `gh api`. This reads a file; it authenticates nothing.
  * nothing here checks that the run executed the workflow file that is on the tag.

Usage, once per tag, before the `pypi` environment is approved. The publication runbook copies this verbatim, and
`test_the_documented_runbook_command_parses` executes the argument list parsed out of it:

    gh api repos/golyshevskii/tooprolix/actions/runs/<ci-run>/jobs --paginate > ci.json
    gh api repos/golyshevskii/tooprolix/actions/runs/<artifact-run>/jobs --paginate > artifacts.json
    python scripts/check_tag_jobs.py --tag-sha $(git rev-parse v0.4.8^{commit}) --tag-name v0.4.8 ci.json artifacts.json
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

#: The tracked expectation. Read from disk rather than embedded here so that shrinking it shows up
#: as a diff to a data file, next to the workflows it describes.
DEFAULT_MANIFEST: Path = Path(__file__).parents[1] / ".github" / "expected-tag-jobs.txt"

#: The only conclusion that means the job did its work. Everything else — `failure`, `cancelled`,
#: `skipped`, `timed_out`, `action_required`, `neutral`, or a null for a job still running — blocks.
SUCCESS: str = "success"

#: The only job outside the preapproval proof. It is waiting on the approval this script informs,
#: so requiring its raw success here would make the gate circular. All other observed jobs remain
#: under set equality and raw-success checks.
PREAPPROVAL_EXCLUDED_JOBS: frozenset[tuple[str, str]] = frozenset({("Build artifacts", "Publish to PyPI")})


class TagJobsError(AssertionError):
    """The tag's jobs are not the jobs that were promised, or did not all succeed."""


def read_manifest(path: Path) -> set[tuple[str, str]]:
    """
    Read the expected `(workflow name, job name)` pairs, ignoring blank lines and `#` comments.

    The WORKFLOW half stops a run of some other workflow satisfying the expectation by carrying the
    right job names. An empty manifest is refused here rather than at the comparison, so the message
    names the file that is wrong.
    """
    if not path.is_file():
        raise TagJobsError(f"no expected-job manifest at {path}")
    expected: set[tuple[str, str]] = set()
    workflow: str | None = None
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if section := re.fullmatch(r"\[(?P<workflow>.+)\]", stripped):
            workflow = section.group("workflow")
            continue
        if workflow is None:
            raise TagJobsError(f"{path} lists {stripped!r} before any [workflow name] header")
        expected.add((workflow, stripped))
    if not expected:
        raise TagJobsError(f"{path} expects no jobs at all, so it would be satisfied by a run that did nothing")
    return expected


def read_jobs(paths: list[Path]) -> list[dict[str, Any]]:
    """
    Collect the job objects out of one or more `actions/runs/<id>/jobs` payloads.

    Two shapes are accepted because they are the two `gh api` produces: one response object, or a
    list of them from `--paginate --slurp`. Anything else is refused rather than guessed at —
    `{"message": "Not Found"}` is what a wrong run id returns, and it is valid JSON with no jobs in
    it. The two refusals are worded differently on purpose, so a test can tell which one fired.
    """
    if not paths:
        raise TagJobsError("no run payloads given, so there is nothing to grade")
    collected: list[dict[str, Any]] = []
    for path in paths:
        if not path.is_file():
            raise TagJobsError(f"no such run payload: {path}")
        document: Any = json.loads(path.read_text(encoding="utf-8"))
        pages: list[Any] = document if isinstance(document, list) else [document]
        from_this_file: list[dict[str, Any]] = []
        for page in pages:
            if not isinstance(page, dict) or not isinstance(page.get("jobs"), list):
                raise TagJobsError(f"{path} is not an `actions/runs/<id>/jobs` response")
            for job in page["jobs"]:
                if not isinstance(job, dict):
                    raise TagJobsError(f"{path} holds a job entry that is not an object: {job!r}")
                from_this_file.append(job)
        # ONE FILE IS ONE RUN ATTEMPT. `--paginate` walks the pages of a single run, so jobs that
        # disagree about `run_id`/`run_attempt` were stitched together — and pooling them would let
        # a green job from another run stand in for a missing one.
        identities = {(job.get("run_id"), job.get("run_attempt")) for job in from_this_file}
        if len(identities) > 1:
            raise TagJobsError(f"{path} mixes {len(identities)} different run/attempt pairs: {sorted(identities)}")
        if identities and None in next(iter(identities)):
            raise TagJobsError(f"{path} reports no run_id/run_attempt, so its jobs cannot be tied to one run")
        collected.extend(from_this_file)
    if not collected:
        raise TagJobsError(f"no job was read from {[str(path) for path in paths]}")
    return collected


def failures(expected: set[tuple[str, str]], observed: list[dict[str, Any]], tag_sha: str, tag_name: str) -> list[str]:
    """Return every reason the tag must not be published, or an empty list when there is none."""
    reasons: list[str] = []

    # THE BINDING TO THE TAG: without it this graded a SHAPE, which any two old green runs have.
    # The SHA says which commit; the ref says the run was of the tag and not of a branch pointing at
    # the same commit.
    strangers = sorted({str(job.get("head_sha")) for job in observed if job.get("head_sha") != tag_sha})
    if strangers:
        reasons.append(f"job(s) belong to a run of {strangers}, not of the tag {tag_sha}")

    elsewhere = sorted({str(job.get("head_branch")) for job in observed if job.get("head_branch") != tag_name})
    if elsewhere:
        reasons.append(f"job(s) belong to a run of ref {elsewhere}, not of {tag_name}")

    prerequisites = [
        job
        for job in observed
        if (str(job.get("workflow_name")), str(job.get("name"))) not in PREAPPROVAL_EXCLUDED_JOBS
    ]
    malformed = [
        job
        for job in prerequisites
        if not isinstance(job.get("name"), str) or not isinstance(job.get("conclusion"), str)
    ]
    if malformed:
        reasons.append(
            f"{len(malformed)} job(s) report no name or no conclusion (a job still running has a null "
            f"conclusion, and 'not yet failed' is not 'succeeded')"
        )

    ran = {(str(job.get("workflow_name")), job["name"]) for job in prerequisites if isinstance(job.get("name"), str)}
    if missing := sorted(expected - ran):
        reasons.append(f"expected job(s) that did not run at all: {missing}")
    if unexpected := sorted(ran - expected):
        reasons.append(f"job(s) the manifest does not expect: {unexpected}")

    unsuccessful = sorted(
        f"{job['name']}={job['conclusion']}"
        for job in prerequisites
        if isinstance(job.get("name"), str) and isinstance(job.get("conclusion"), str) and job["conclusion"] != SUCCESS
    )
    if unsuccessful:
        reasons.append(f"job(s) that did not conclude {SUCCESS}: {unsuccessful}")

    return reasons


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("payload", nargs="+", type=Path, help="`gh api .../actions/runs/<id>/jobs` output")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST, help="the tracked expected-job list")
    parser.add_argument("--tag-sha", required=True, help="the tag's target commit, `git rev-parse 'v0.4.8^{commit}'`")
    parser.add_argument("--tag-name", required=True, help="the tag itself, e.g. v0.4.8")
    parsed = parser.parse_args(argv)

    try:
        # A 40-hex commit or nothing: an abbreviated `--tag-sha` matches no `head_sha`, and would
        # match many if the comparison were ever loosened.
        if not re.fullmatch(r"[0-9a-f]{40}", parsed.tag_sha):
            raise TagJobsError(f"--tag-sha {parsed.tag_sha!r} is not a full 40-character commit SHA")
        # `v*` is the pattern both workflows trigger on, so anything else is not a tag they ran for.
        if not re.fullmatch(r"v\S+", parsed.tag_name):
            raise TagJobsError(f"--tag-name {parsed.tag_name!r} is not a v* tag")
        expected = read_manifest(parsed.manifest)
        observed = read_jobs(parsed.payload)
    except TagJobsError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1

    if reasons := failures(expected, observed, parsed.tag_sha, parsed.tag_name):
        for reason in reasons:
            print(f"FAIL: {reason}", file=sys.stderr)
        return 1

    runs = sorted({str(job.get("run_id")) for job in observed})
    print(
        f"OK: all {len(expected)} expected job(s) of {parsed.tag_name} ({parsed.tag_sha}) concluded {SUCCESS} (runs {runs})."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
