"""
Grade a pull request's TITLE, because under squash merge the title is the release contract.

This repository squash-merges. The squashed commit's SUBJECT is the pull request title, and
conventional-commits parsing reads the subject — so the branch's own commit subjects are demoted
into the body, where nothing parses them. release-plz therefore never sees per-commit discipline;
it sees one string, chosen in the merge dialog.

That is not a theory. PR #17 was titled `Audit the Rust code with /rust-skills, and close what it
found` while its branch carried `fix!: stop on a closed pipe, and refuse a backslash in exclude`.
The `!` survived into the body of `033ceeb` at line 77 and released as **v0.3.4**, a patch, for a
breaking change. The tag, the GitHub release and the CHANGELOG entry are all live and all say
patch.

Two things are graded here, and the second is the one that actually failed:

  1. **the title parses as a Conventional Commit with a known type.** This catches #17 — but only
     because #17's title had no type at all. `fix: audit the Rust code` would pass it and ship the
     identical patch.
  2. **the title's bump is not smaller than the branch's.** If any commit carries `!` or a
     `BREAKING CHANGE:` footer while the title carries neither, the release would be a patch for a
     breaking change, and that is refused.

Both read the ARTIFACT: the title comes from `github.event.pull_request.title` through the
environment, the commit messages from `gh api repos/<owner>/<repo>/pulls/<n>/commits`. Nothing
here reads a value the author or the workflow summarised — a validator grading a self-report is
this epic's most repeated defect.

It fails CLOSED. A missing `PR_TITLE`, an unreadable commit list, a payload of an unexpected shape
and an empty commit list are all failures, not skips: a gate that a surprising input switches off
is not a gate.

⚠️ The bump names below are the **measured `0.x`** table: while the crate is `0.x`, ONLY `!` /
`BREAKING CHANGE` moves the middle number, so `feat:` is a patch too. That stops being true at
`1.0.0`, where `feat:` becomes a minor and `!` becomes a major — both halves are measured and
tabulated in `CONTRIBUTING.md`. `test_the_crate_is_still_zero_x` in
`tests/unit/test_pr_title_gate.py` is what goes red when `Cargo.toml` reaches `1.0.0` and this
wording needs revisiting.

Usage (this is the exact invocation in `.github/workflows/ci.yml`):

    PR_TITLE="$TITLE" uv run --no-project python3 scripts/pr_title_gate.py --commits commits.json

Run: make test (the guards) — the gate itself runs only in CI, on `pull_request` events.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

# The types a title may use. Deliberately the list `CONTRIBUTING.md` declares — a superset of the
# five this repository's merged history actually contains (`feat`, `fix`, `ci`, `chore`, `docs`) —
# rather than a third list invented here. `TestTheKnownTypesAreTheOnesContributingDeclares` reads
# the document and fails if the two ever drift, so the gate cannot start refusing a type the
# contributing guide tells people to use.
#
# ⚠️ Reviewed and REJECTED, so it is not re-raised: deriving this set from `git log --format=%s` on
# `main` instead. It sounds like the empirical choice and is the wrong one — history contains the
# types that have been NEEDED so far, not the types that are allowed, so the first pull request to
# legitimately want `docs:` or `build:` would be refused by a gate citing a list nobody wrote down.
# The drift test is a check against the DOCUMENT on purpose; it is not, and is not meant to be, a
# check against reality.
KNOWN_TYPES = frozenset({"feat", "fix", "perf", "refactor", "test", "docs", "chore", "ci", "build"})

# `<type>(<optional scope>)!: <summary>`.
#
# Every boundary here was MEASURED against release-plz 0.3.160 rather than read off the spec,
# because the only grammar that matters is the one the release tool actually implements. A gate
# STRICTER than the parser blocks a legitimate title — loud, and fixed by editing it. A gate LOOSER
# than the parser passes a title the parser then misprices — silent, and the whole reason this job
# exists. The two that were measured wrong, and are now right:
#
#   `feat()!: x`  -> release-plz answers 0.3.5, a PATCH, and files the raw subject with no
#                    `[**breaking**]` marker. An EMPTY scope silently voids the `!`. So the scope
#                    is `[^()]+`, not `[^()]*` — this was the dangerous direction.
#   `feat( )!: x` -> 0.4.0. A scope of one space IS a scope, so the rule is non-empty, not
#                    non-blank, and tightening further would be stricter than the parser.
#   `fix:no space`-> 0.3.5 under `### Fixed` with the summary parsed. release-plz does not require
#                    the space, so neither does this: `[ \t]*`, not `[ \t]+`.
#   `feat!:<NBSP>x`-> 0.4.0 under `### Added`, fully parsed, so U+00A0 is accepted too. Listed
#                    literally rather than widened to `\s`, which would also accept separators
#                    nobody has measured — extend this class only with a measurement.
_SUBJECT = re.compile(r"^(?P<type>[A-Za-z]+)(?:\((?P<scope>[^()]+)\))?(?P<bang>!)?:[ \t\u00a0]*(?P<summary>\S.*)$")

# The footer form of a breaking change. Case-SENSITIVE and anchored to the start of a line, which
# is the spec's own rule: matched loosely, every commit body that discusses a breaking change would
# become one, and a gate that fires on prose is a gate people learn to route around.
#
# Both separators, because Conventional Commits allows `token: value` AND `token #value`, and
# release-plz implements both — MEASURED, one commit per run at the live `v0.3.4` tag:
#
#   BREAKING CHANGE: x   -> 0.4.0     BREAKING CHANGE #123  -> 0.4.0
#   BREAKING-CHANGE: x   -> 0.4.0     BREAKING-CHANGE #123  -> 0.4.0
#   BREAKING CHANGE#123  -> 0.3.5     breaking change: x    -> 0.3.5
#   BREAKING CHANGES: x  -> 0.3.5
#
# The hash form was missed entirely before this, which is the dangerous direction: a commit whose
# only declaration was `BREAKING CHANGE #123` read as non-breaking here and as breaking there. The
# three that answer 0.3.5 are deliberately NOT matched — `BREAKING CHANGES:` especially, which is a
# plausible thing to write in prose.
_BREAKING_FOOTER = re.compile(r"^BREAKING[ -]CHANGE(?::| #)", re.MULTILINE)

# `repos/{owner}/{repo}/pulls/{n}/commits` returns AT MOST 250 commits and `--paginate` cannot lift
# it — the cap is on the endpoint, not the page size ("Lists a maximum of 250 commits for a pull
# request", GitHub REST documentation). A 251-commit pull request whose only `!` sits past the cap
# comes back looking clean, so a list that reaches this length is refused rather than graded. It
# will never fire here; a guard that silently grades half its input is the fail-open shape this
# epic keeps shipping.
API_COMMIT_CAP = 250


@dataclass(frozen=True)
class Subject:
    """The two things about a conventional subject that decide a release: its type and its `!`."""

    type: str
    breaking: bool


def parse_subject(subject: str) -> Subject | None:
    """
    Parse the first line of a commit message, or `None` if it is not a conventional subject.

    Normalise first, then match. Whitespace is stripped before the grammar is applied because
    GitHub does not trim a pasted title, and judging an untrimmed string would reject a title
    release-plz accepts. The type is NOT lowercased: an unknown-type failure naming `Feat` is more
    useful than silently accepting a spelling this repository never uses.

    Grammar only. Whether the type is one this repository uses is a separate judgement, so a typo
    can be reported as a typo instead of as "this is not a conventional commit at all".
    """
    match = _SUBJECT.match(subject.strip().splitlines()[0] if subject.strip() else "")
    if match is None:
        return None
    return Subject(type=match["type"], breaking=match["bang"] is not None)


def message_is_breaking(message: str) -> bool:
    """
    Report whether this whole commit message is breaking — by its subject's `!` or by a footer.

    Both forms, because release-plz honours both and a gate that knew only one would let the other
    through. The footer is looked for in the whole message, the `!` only in the subject line.
    """
    parsed = parse_subject(message)
    return (parsed is not None and parsed.breaking) or _BREAKING_FOOTER.search(message) is not None


def bump_for(*, breaking: bool) -> str:
    """
    Return the bump a subject produces, from the measured `0.x` table.

    Measured, not assumed. First on the real `v0.1.0` tag (2026-07-27) and re-measured on the live
    `v0.3.4` tag (2026-07-29), one commit type per run in a throwaway clone: `fix:`, `feat:`,
    `perf:`, `docs:`, `chore:` and `refactor:` all answered `0.3.4 -> 0.3.5`, and only `feat!:` /
    `fix!:` / a `BREAKING CHANGE:` footer answered `0.3.4 -> 0.4.0`. So on `0.x` there are exactly
    two answers, and the type chooses the CHANGELOG section rather than the number.

    ⚠️ Two answers is a `0.x` fact. Measured the same day on a throwaway clone tagged `v1.0.0`,
    there are three: `fix:` -> 1.0.1, `feat:` -> 1.1.0, `feat!:` -> 2.0.0. Whoever ships `1.0.0`
    owns turning this function into the three-way version; `test_the_crate_is_still_zero_x` is what
    makes that day loud instead of silent.
    """
    return "minor" if breaking else "patch"


_SQUASH_NOTE = (
    "This repository squash-merges, so the PR title becomes the commit subject and is the ONLY "
    "thing release-plz parses — the branch's own subjects become body prose. See CONTRIBUTING.md, "
    "'What decides the version number'."
)


def grade_pull_request(title: str, commit_messages: Sequence[str]) -> list[str]:
    """
    Grade a pull request's title against its own commits. An empty list means it may merge.

    Returns messages rather than raising, so every problem with a title is reported in one CI run
    instead of one per push.
    """
    failures: list[str] = []

    parsed = parse_subject(title)
    if parsed is None:
        failures.append(
            f"the PR title is not a Conventional Commit: {title.strip()!r}\n"
            f"  release-plz would read it as a non-conventional subject and cut a "
            f"{bump_for(breaking=False).upper()} release; only a `!` (or a `BREAKING CHANGE:` "
            f"footer) cuts a {bump_for(breaking=True).upper()} while the crate is 0.x.\n"
            f"  Expected `<type>(<optional scope>)!: <summary>` with a type from: "
            f"{', '.join(sorted(KNOWN_TYPES))}.\n"
            f"  {_SQUASH_NOTE}"
        )
    elif parsed.type not in KNOWN_TYPES:
        failures.append(
            f"the PR title uses an unknown type {parsed.type!r}: {title.strip()!r}\n"
            f"  release-plz would cut a {bump_for(breaking=parsed.breaking).upper()} release from it.\n"
            f"  Valid types: {', '.join(sorted(KNOWN_TYPES))}.\n"
            f"  {_SQUASH_NOTE}"
        )

    if len(commit_messages) >= API_COMMIT_CAP:
        # Refuse rather than grade a truncation. See API_COMMIT_CAP: past 250 the endpoint stops
        # listing, so "no `!` in these commits" stops being a statement about the branch. Returning
        # early, because every judgement below this line would be made about a partial branch.
        truncated = (
            f"this pull request has at least {API_COMMIT_CAP} commits, which is the maximum the "
            f"GitHub API lists for a pull request — `--paginate` cannot lift it. The commit list is "
            f"therefore possibly truncated, and a breaking change past that point would be "
            f"invisible here. Refusing to grade a branch this gate can only half-see: split the "
            f"pull request, or declare the bump in the title deliberately."
        )
        return [*failures, truncated]

    if not commit_messages:
        # Every pull request has at least one commit, so zero means this gate read the wrong thing:
        # a failed API call, the wrong PR number, a `jq` filter that selected nothing. Passing here
        # would silently disable the breaking-change half of the gate whenever the fetch misfired.
        failures.append(
            "the commit list for this pull request is empty, which cannot be true — the gate could "
            "not read the branch's commits, so it refuses rather than grading half the contract."
        )
        return failures

    breaking_commits = [m for m in commit_messages if message_is_breaking(m)]
    title_is_breaking = parsed is not None and parsed.breaking or _BREAKING_FOOTER.search(title) is not None
    if breaking_commits and not title_is_breaking:
        subjects = "\n".join(f"    {m.strip().splitlines()[0]}" for m in breaking_commits)
        failures.append(
            f"this branch contains a breaking change but the PR title does not declare one.\n"
            f"  PR title: {title.strip()!r}\n"
            f"  Breaking commit(s):\n{subjects}\n"
            f"  As written the release would be a {bump_for(breaking=False).upper()}; a breaking "
            f"change must be a {bump_for(breaking=True).upper()}.\n"
            f"  Add `!` before the colon in the PR TITLE (or a `BREAKING CHANGE:` footer to it) — "
            f"the `!` in a branch commit is not enough.\n"
            f"  {_SQUASH_NOTE}\n"
            f"  This is exactly how PR #17 released v0.3.4 — a patch — for a breaking change."
        )

    return failures


def grade_merged_commit(subject: str, body: str, branch_commits: Sequence[str] | None) -> list[str]:
    """
    Grade the commit that actually landed on `main` — the artifact, not the proxy.

    The pull-request check above grades what the title WAS. GitHub's squash dialog lets whoever
    merges edit the subject at that moment, and no branch protection exists on this repository to
    stop them, so the string release-plz finally parses can differ from every string the pre-merge
    gate ever saw. This is the only check that reads what release-plz will read.

    It fires on `push: main`, which is after the merge but BEFORE the Release PR is merged — a
    separate, manual step. So a boundary caught here is still correctable, which is the entire
    reason it is worth running at all.

    `branch_commits` is the pull request's REAL commits, fetched by resolving the landed SHA
    through `repos/{owner}/{repo}/commits/{sha}/pulls` and then `pulls/{n}/commits` — the same
    source the pull-request path already trusts, and the reason this check is worth anything.

    ⚠️ It used to reconstruct them from the `* ` bullets the squash leaves in the BODY, and that was
    the identical defect one box over: the merge dialog edits the body and the subject together, so
    deleting the bullets — or emptying the body — disarmed the check in the same keystroke.
    Measured on the real merge: bullets present rc=1, bullets deleted rc=0, empty body rc=0. No
    bullet parser survives here; a cleverer one would only move the seam again.

    `None` means the landed commit has no associated pull request — a direct push to `main`. That
    is not an API failure and not a refusal: there is no branch to compare against, so the commit
    is graded on its own, which is the entire artifact in that case. The body still goes in whole,
    so a line-anchored `BREAKING CHANGE:` footer in it demands a `!` in the subject.

    ⚠️ HEAD only, deliberately. A push carrying several commits is graded by its tip, which is the
    shape every squash merge and every Release PR merge takes. Grading the whole pushed range would
    mean reading `github.event.before`, which is all-zeros on branch creation. Recorded as an
    accepted residual in CONTRIBUTING.md rather than left to be rediscovered.
    """
    if branch_commits is None:
        return grade_pull_request(subject, [f"{subject}\n{body}"])
    # The body rides along as a second, free signal: a `BREAKING CHANGE:` footer that a merger
    # typed straight into the squash message belongs to no branch commit and would otherwise be
    # invisible. It cannot produce a false positive that the bullets used to, because nothing here
    # reads bullets any more — only `_BREAKING_FOOTER`, which is anchored to the start of a line.
    return grade_pull_request(subject, [*branch_commits, body])


def commit_messages_from(payload: Path) -> list[str]:
    """
    Read the commit messages out of what `gh api .../pulls/<n>/commits` wrote.

    Every shape that is not a flat list of commit objects is fatal. `gh api --paginate --slurp`
    returns an array of PAGES, and it is the `jq 'add'` in the workflow that flattens it; if that
    pipe is ever dropped this must go red rather than grade a list of lists as a list of commits
    with no `!` anywhere in it.
    """
    data: Any = json.loads(payload.read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise TypeError(f"{payload}: expected a JSON array of commits, got {type(data).__name__}")

    messages: list[str] = []
    for index, entry in enumerate(data):
        # Bound rather than re-subscripted, so the `isinstance` below is what the type checker sees
        # narrowing the value that is actually read — two lookups can disagree, one cannot.
        commit: Any = entry.get("commit") if isinstance(entry, dict) else None
        if not isinstance(commit, dict):
            raise TypeError(f"{payload}: entry {index} is not a commit object — is the `jq 'add'` flatten missing?")
        message: Any = commit.get("message")
        if not isinstance(message, str):
            raise TypeError(f"{payload}: entry {index} carries no commit message")
        messages.append(message)
    return messages


def main() -> int:
    parser = argparse.ArgumentParser(description="Grade the release contract of a pull request or a merged commit.")
    # `choices` is the event ALLOW-LIST, and it is the point of passing the event at all. The
    # previous shape refused only what it recognised as not-a-pull-request and let everything else
    # through: a `merge_group` event — or any trigger added later — reported success with no title
    # and no commits, which is an unexpected input shape switching the guard off. argparse now
    # refuses an unknown event before any grading happens, and the workflow's `case` refuses it a
    # second time. Two layers, both loud.
    parser.add_argument("--event", required=True, choices=("pull_request", "push"))
    parser.add_argument("--commits", type=Path, help="JSON from `gh api .../pulls/<n>/commits` for either event")
    parser.add_argument("--merged-message", type=Path, help="`push`: the full message of the commit that landed")
    args = parser.parse_args()

    if args.event == "pull_request":
        if args.commits is None:
            print("pr-title-gate: --commits is required for a pull_request event — refusing.", file=sys.stderr)
            return 1
        # From the event payload through the environment, never from an argument a workflow could
        # summarise or a caller could supply by hand. Unset is broken wiring, not a valid title.
        title = os.environ.get("PR_TITLE")
        if title is None:
            print("pr-title-gate: PR_TITLE is not set, so there is no title to grade — refusing.", file=sys.stderr)
            return 1
        failures = grade_pull_request(title, commit_messages_from(args.commits))
    else:
        if args.merged_message is None:
            print("pr-title-gate: --merged-message is required for a push event — refusing.", file=sys.stderr)
            return 1
        # `git log -1 --format=%B`, written by git, read here. The subject is its first line; the
        # rest is the body the squash left behind.
        message = args.merged_message.read_text(encoding="utf-8")
        subject, _, body = message.partition("\n")
        # `--commits` present = the landed SHA resolved to a pull request, and these are its REAL
        # commits. Absent = no associated pull request, i.e. a direct push, which is graded on its
        # own. The workflow only omits the flag when the resolution genuinely returned nothing; an
        # API call that FAILS stops the step before this line, because it runs under `pipefail`.
        branch_commits = commit_messages_from(args.commits) if args.commits is not None else None
        failures = grade_merged_commit(subject, body, branch_commits)

    if failures:
        for failure in failures:
            print(f"pr-title-gate: {failure}", file=sys.stderr)
        return 1

    print("pr-title-gate: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
