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

Usage, once per tag, before anything is uploaded:

    gh api repos/golyshevskii/tooprolix/actions/runs/<ci-run>/jobs --paginate > ci.json
    gh api repos/golyshevskii/tooprolix/actions/runs/<artifact-run>/jobs --paginate > artifacts.json
    python scripts/check_tag_jobs.py ci.json artifacts.json
"""

from __future__ import annotations

import argparse
import json
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


def read_manifest(path: Path) -> set[str]:
    """
    Read the expected job names, ignoring blank lines and `#` comments.

    Empties are NOT tolerated here rather than at the comparison: a manifest that reads as the empty
    set is an unusable expectation whatever the run did, and saying so at the point of reading names
    the file that is wrong.
    """
    if not path.is_file():
        raise TagJobsError(f"no expected-job manifest at {path}")
    expected = {
        stripped
        for line in path.read_text(encoding="utf-8").splitlines()
        if (stripped := line.strip()) and not stripped.startswith("#")
    }
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
        for page in pages:
            if not isinstance(page, dict) or not isinstance(page.get("jobs"), list):
                raise TagJobsError(f"{path} is not an `actions/runs/<id>/jobs` response")
            collected.extend(page["jobs"])
    if not collected:
        raise TagJobsError(f"no job was read from {[str(path) for path in paths]}")
    return collected


def failures(expected: set[str], observed: list[dict[str, Any]]) -> list[str]:
    """Return every reason the tag must not be published, or an empty list when there is none."""
    reasons: list[str] = []

    malformed = [
        job for job in observed if not isinstance(job.get("name"), str) or not isinstance(job.get("conclusion"), str)
    ]
    if malformed:
        reasons.append(
            f"{len(malformed)} job(s) report no name or no conclusion (a job still running has a null "
            f"conclusion, and 'not yet failed' is not 'succeeded')"
        )

    ran = {job["name"] for job in observed if isinstance(job.get("name"), str)}
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
    parsed = parser.parse_args(argv)

    try:
        expected = read_manifest(parsed.manifest)
        observed = read_jobs(parsed.payload)
    except TagJobsError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1

    if reasons := failures(expected, observed):
        for reason in reasons:
            print(f"FAIL: {reason}", file=sys.stderr)
        return 1

    print(f"OK: all {len(expected)} expected job(s) ran and concluded {SUCCESS}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
