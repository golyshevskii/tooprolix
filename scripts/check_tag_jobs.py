"""
Refuse to publish a `v*` tag unless every expected job of that tag ran and RAW-concluded `success`.

**The defect this closes is grading a self-report.** A run's `head_sha` is a label the run attaches
to itself, and an expected-job list derived from the run itself agrees with whatever the run did.
Measured in this repository: run 30691798694 reports `head_sha=292d8af4` while `actions/checkout`
materialised tree `3a1f67fd`. So the expectation is pinned in a tracked file
(`.github/expected-tag-jobs.txt`) that a human has to edit in a reviewable diff, and the observation
is each job's RAW `conclusion` — `cancelled` and `skipped` are not `success`, and reading them as
"not failed" is exactly how PR #37's four cancelled artifact checks looked green.

Three ways a weaker version of this passes while the guarantee is broken, all refused here:

  * a job flipped to `cancelled` — refused, because the raw conclusion is compared to `success`
    rather than to `failure`;
  * an expected job absent from the run entirely — refused, because the comparison is SET EQUALITY
    and never "contains"; a superset fails as loudly as a subset;
  * the expectation reduced to `[]` — refused separately, because a set-equality check over an empty
    expectation passes having verified nothing. Shrink the workflow and the manifest together and
    every mutation fixture over a populated set still passes. This is the one that needs its own
    assertion.

**And it binds the jobs to the TAG, which an earlier version did not.** Given a payload carrying
every expected name and no `head_sha` at all, that version exited 0 — so two old green
feature-branch runs would have certified a tag nothing tested. Every job must now report the tag's
own commit, and the jobs of one payload must all belong to one run and one attempt, so a file
stitched together from several runs is refused rather than averaged.

**And it binds the jobs to the WORKFLOWS a tag push fires**, because the same commit does not
establish that those workflows ran. Measured: a payload carrying every expected job name with
`workflow_name: Some other workflow`, dispatched on `main`, exited 0 before the manifest was
sectioned by workflow. The expectation is now `(workflow name, job name)` pairs, and every job's
`head_branch` must be the tag itself — on a real tag-push run it is (`v0.4.7`, run 30703436113).

WHAT THIS CANNOT ESTABLISH, said plainly because the whole point is not to grade a self-report:

  * `head_sha` is still a label the RUN attaches to itself. It proves the run claims the tag; it
    does not prove `actions/checkout` materialised it. What proves that is the last step of every
    job that checks out, in `ci.yml` AND `build-artifacts.yml`, which asserts
    `git rev-parse HEAD == $GITHUB_SHA` after its gates have run. That assertion cannot be read from
    here — job outputs and step logs are not returned by this REST endpoint — which is exactly why
    it fails the job rather than reporting to anyone.
  * **the run's EVENT is not in this payload, and `workflow_name` is only a display label.**
    `actions/runs/<id>/jobs` carries `workflow_name`, `head_branch`, `head_sha`, `run_id` and
    `run_attempt`, but not `event` — that lives on `actions/runs/<id>`. Two consequences, both
    accepted as residuals rather than closed here:
      - a `workflow_dispatch` launched against the tag ref is indistinguishable from the tag push;
      - a DECOY workflow file whose `name:` is `CI` or `Build artifacts`, dispatched at the tag,
        satisfies both the section names and `head_branch`. `workflow_name` is what the workflow
        calls itself, not which file ran.
    Closing either means a second API call for the run object and grading its `event` and
    `path`. That is the publication task's design problem, and it is recorded as such.
  * the payload is trusted to have come from `gh api`. This reads a file; it does not authenticate
    the API.
  * nothing here checks that the run executed the workflow file that is on the tag.

Usage, once per tag, before anything is uploaded. The publication runbook copies this verbatim, so
`test_the_documented_runbook_command_parses` executes the argument list parsed out of this docstring
— an earlier revision of it omitted `--tag-name` and exited 2 on `error: the following arguments are
required`, which is a defect rather than a typo when a runbook is the only reader:

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


class TagJobsError(AssertionError):
    """The tag's jobs are not the jobs that were promised, or did not all succeed."""


def read_manifest(path: Path) -> set[tuple[str, str]]:
    """
    Read the expected `(workflow name, job name)` pairs, ignoring blank lines and `#` comments.

    The WORKFLOW half is what stops a run of some other workflow — or a hand-dispatched one —
    satisfying the expectation just by carrying the right job names. Measured before it existed: a
    payload whose `workflow_name` was `Some other workflow`, dispatched on `main`, exited 0.

    Empties are NOT tolerated here rather than at the comparison: a manifest that reads as the empty
    set is an unusable expectation whatever the run did, and saying so at the point of reading names
    the file that is wrong.
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

    Two shapes are accepted because those are the two `gh api` produces: a single response object,
    and a list of them when `--paginate --slurp` returned several pages. Anything else is refused
    instead of guessed at — a checker that reads an unexpected document as "no jobs, nothing wrong"
    is the fail-open this exists to prevent. `{"message": "Not Found"}` is what a wrong run id
    returns, and it is a perfectly valid JSON document with no jobs in it.

    The two refusals are worded differently on purpose: a malformed document and a document that
    genuinely contained no jobs are different faults, and a checker whose failures are all spelled
    the same cannot be shown to be grading the thing it claims.
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
        # ONE FILE IS ONE RUN ATTEMPT. `--paginate` walks the pages of a single run, so a file whose
        # jobs disagree about `run_id`/`run_attempt` was stitched together from several runs — and a
        # grader that pooled them would let a green job from any other run stand in for a missing
        # one. Refused rather than merged.
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

    # THE BINDING TO THE TAG. Without it this graded a SHAPE — the right names with the right
    # conclusions — and any two old green runs satisfied it. The SHA says which commit; the ref says
    # the run was of the tag rather than of a branch that happened to point at the same commit.
    strangers = sorted({str(job.get("head_sha")) for job in observed if job.get("head_sha") != tag_sha})
    if strangers:
        reasons.append(f"job(s) belong to a run of {strangers}, not of the tag {tag_sha}")

    elsewhere = sorted({str(job.get("head_branch")) for job in observed if job.get("head_branch") != tag_name})
    if elsewhere:
        reasons.append(f"job(s) belong to a run of ref {elsewhere}, not of {tag_name}")

    malformed = [
        job for job in observed if not isinstance(job.get("name"), str) or not isinstance(job.get("conclusion"), str)
    ]
    if malformed:
        reasons.append(
            f"{len(malformed)} job(s) report no name or no conclusion (a job still running has a null "
            f"conclusion, and 'not yet failed' is not 'succeeded')"
        )

    ran = {(str(job.get("workflow_name")), job["name"]) for job in observed if isinstance(job.get("name"), str)}
    if missing := sorted(expected - ran):
        reasons.append(f"expected job(s) that did not run at all: {missing}")
    if unexpected := sorted(ran - expected):
        reasons.append(f"job(s) the manifest does not expect: {unexpected}")

    unsuccessful = sorted(
        f"{job['name']}={job['conclusion']}"
        for job in observed
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
        # A 40-hex commit or nothing. An abbreviated or empty `--tag-sha` would silently match no
        # job's `head_sha`, or — worse, if the comparison were ever loosened — match many.
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
