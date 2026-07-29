"""
Guards for `scripts/pr_title_gate.py`, the CI job that grades a pull request's TITLE.

The defect this exists for is on `main` and cost a version boundary. PR #17 was titled
`Audit the Rust code with /rust-skills, and close what it found` and its branch carried
`fix!: stop on a closed pipe, and refuse a backslash in exclude`. This repository
squash-merges, so the squashed subject is the PR title and the branch's own subjects become body
prose — release-plz read a non-conventional subject and cut **v0.3.4**, a patch, for a breaking
change. Both facts are captured verbatim below in `PR_17_TITLE` / `PR_17_COMMITS`, read out of
`gh api repos/golyshevskii/tooprolix/pulls/17/commits` on 2026-07-29, so this suite is pinned to
what actually happened rather than to a plausible reconstruction of it.

Two properties, and the second is the one a narrower gate would miss:

  1. **the title parses as a Conventional Commit with a known type.** `fix: …` yes,
     `Audit the Rust code …` no. This alone catches #17, but only by accident — #17's title
     happened to have no type at all.
  2. **the title's bump is not smaller than the branch's.** A branch carrying `!` or a
     `BREAKING CHANGE:` footer under a title without one is exactly #17 wearing a valid type, and
     property 1 passes it. This is the property that actually failed.

Everything here grades the ARTIFACT: the real title string and the real commit messages. Nothing in
the gate reads a value the author or the workflow summarises for it — that is the recurring defect
of this epic, and `TestTheGateReadsTheArtifactAndFailsClosed` is where it is held.

`TestTheGuardIsWiredIntoTheEntryPoint` runs the script the way `.github/workflows/ci.yml` runs it,
because a guard reachable only through a function call can be deleted from `main` with every other
test in this file still passing.

Run: make test
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest
from pr_title_gate import (
    API_COMMIT_CAP,
    KNOWN_TYPES,
    Subject,
    bump_for,
    grade_merged_commit,
    grade_pull_request,
    message_is_breaking,
    parse_subject,
)

REPO_ROOT = Path(__file__).resolve().parents[2]

# PR #17, verbatim. `gh api repos/golyshevskii/tooprolix/pulls/17/commits --paginate --slurp |
# jq 'add'` on 2026-07-29 returned three commits; the middle one is the breaking change that
# shipped as a patch. Only the subjects are reproduced — none of the three carries a
# `BREAKING CHANGE:` footer (`grep -c BREAKING` over the full messages returned 0), so the subject
# is the whole of the signal in this case. The footer path has its own test below.
PR_17_TITLE = "Audit the Rust code with /rust-skills, and close what it found"
PR_17_COMMITS = [
    "fix: normalise an exclude glob before judging it, and gate rustdoc",
    "fix!: stop on a closed pipe, and refuse a backslash in `exclude`",
    "fix: report an unwritable stdout instead of exiting in silence",
]


class TestTheTitleParsesAsAConventionalCommit:
    """
    The subject grammar, which is what release-plz reads and — under squash — is the PR title.

    Parametrised over both directions on purpose. A gate proved only on good input is a gate whose
    regex could be `.*`.
    """

    @pytest.mark.parametrize(
        ("title", "expected"),
        [
            ("fix: stop on a closed pipe", Subject(type="fix", breaking=False)),
            ("feat!: replace the opt-out marker", Subject(type="feat", breaking=True)),
            ("ci(coverage): measure both languages", Subject(type="ci", breaking=False)),
            ("feat(cli)!: narrow what exclude accepts", Subject(type="feat", breaking=True)),
            # Leading/trailing whitespace is normalised BEFORE the grammar is applied — GitHub does
            # not trim a pasted title, and comparing an untrimmed string against the grammar would
            # reject a title release-plz accepts.
            ("  fix: trim me  ", Subject(type="fix", breaking=False)),
        ],
    )
    def test_a_conventional_subject_is_parsed(self, title: str, expected: Subject) -> None:
        assert parse_subject(title) == expected

    @pytest.mark.parametrize(
        "title",
        [
            # PR #17's actual title. No type, no colon in type position.
            PR_17_TITLE,
            "",
            "   ",
            "fix stop on a closed pipe",  # no colon
            # `fix:no space after the colon` was here, asserting it does NOT parse. That was my
            # assumption, and measuring release-plz 0.3.160 disproved it — it answers `0.3.4 ->
            # 0.3.5` under `### Fixed`. The case now lives in
            # `TestTheGrammarMatchesWhatReleasePlzActuallyParses` asserting the opposite. Left as a
            # note because a test asserting a guess is worse than no test.
            "feat()!: an empty scope voids the bang",  # MEASURED -> patch, so it must not parse
            "fix: ",  # no summary
            "(cli): a scope with no type",
            "fix(unclosed: a scope that never closes",
        ],
    )
    def test_a_non_conventional_subject_does_not_parse(self, title: str) -> None:
        assert parse_subject(title) is None

    def test_an_unknown_type_parses_but_is_not_accepted(self) -> None:
        """
        `parse_subject` grades the GRAMMAR; the known-type set is a separate judgement, so a typo
        can be reported as a typo rather than as "this is not a conventional commit at all".
        """
        parsed = parse_subject("feet: a typo for feat")

        assert parsed == Subject(type="feet", breaking=False)
        assert parsed.type not in KNOWN_TYPES


class TestTheKnownTypesAreTheOnesContributingDeclares:
    """
    The gate's type set and the set `CONTRIBUTING.md` tells a contributor to use must be the same
    list, or the document and the gate disagree about what a valid PR title is — and the
    contributor believes the document until CI says otherwise.

    Read out of the file rather than restated here, for the same reason
    `test_coverage_report.py` walks the filesystem instead of listing today's filenames: a second
    hand-written copy agrees with itself, not with the thing it is meant to pin.
    """

    def test_the_gate_accepts_exactly_the_documented_types(self) -> None:
        line = next(
            line
            for line in (REPO_ROOT / "CONTRIBUTING.md").read_text(encoding="utf-8").splitlines()
            if line.startswith("Types in use:")
        )
        documented = set(re.findall(r"`([a-z]+)`", line))

        assert documented, "CONTRIBUTING.md's 'Types in use:' line lists no types"
        assert documented == KNOWN_TYPES


class TestABreakingChangeIsRecognisedWhereverItIsWritten:
    """
    Both spellings, because release-plz honours both and the gate must too.

    A gate that knew only the `!` would wave through a branch declaring itself breaking in a
    footer — half a guarantee, and the invisible half.
    """

    @pytest.mark.parametrize(
        "message",
        [
            "fix!: stop on a closed pipe",
            "feat(cli)!: narrow what exclude accepts",
            "refactor: move the walker\n\nBREAKING CHANGE: the exit codes changed",
            "refactor: move the walker\n\nBREAKING-CHANGE: the exit codes changed",
        ],
    )
    def test_a_breaking_message_is_breaking(self, message: str) -> None:
        assert message_is_breaking(message) is True

    @pytest.mark.parametrize(
        "message",
        [
            "fix: stop on a closed pipe",
            "feat: add the exclude key",
            # Prose ABOUT a breaking change is not a footer. The spec's footer token is uppercase
            # at the start of a line, and matching it case-insensitively anywhere would turn every
            # commit body discussing the subject into a breaking change — a gate that fires on
            # prose is a gate people learn to route around.
            "fix: tidy up\n\nThis is not a breaking change: the exit codes are unchanged.",
            "fix: tidy up\n\nno breaking change: here",
        ],
    )
    def test_a_non_breaking_message_is_not_breaking(self, message: str) -> None:
        assert message_is_breaking(message) is False


class TestTheBranchMayNotOutrankTheTitle:
    """
    Decision 1 of this task, and the property whose absence cost `v0.4.0`.

    A title that parses is not enough: under squash it is the ONLY thing release-plz sees, so a
    branch carrying `!` under a title without one silently downgrades a minor to a patch. Every
    case here is graded against the real commit messages, never against a summary of them.
    """

    def test_the_real_pr_17_is_rejected(self) -> None:
        failures = grade_pull_request(PR_17_TITLE, PR_17_COMMITS)

        assert failures, "PR #17 released a breaking change as a patch and this gate let it through"

    def test_a_valid_non_breaking_title_over_a_breaking_branch_is_rejected(self) -> None:
        """
        #17 wearing a valid type. A gate that only checks the grammar passes this and ships the
        same patch — which is why the grammar check alone is a weaker property than the one that
        failed.
        """
        title = "fix: audit the Rust code with /rust-skills"

        assert parse_subject(title) == Subject(type="fix", breaking=False)
        assert grade_pull_request(title, PR_17_COMMITS)

    def test_a_breaking_title_over_the_same_breaking_branch_is_accepted(self) -> None:
        assert grade_pull_request("fix!: audit the Rust code with /rust-skills", PR_17_COMMITS) == []

    def test_a_breaking_change_footer_in_the_title_also_satisfies_it(self) -> None:
        title = "fix: audit the Rust code\n\nBREAKING CHANGE: exclude no longer accepts a backslash"

        assert grade_pull_request(title, PR_17_COMMITS) == []

    def test_a_breaking_change_footer_in_a_commit_body_also_demands_it(self) -> None:
        commits = ["refactor: move the walker\n\nBREAKING CHANGE: the exit codes changed"]

        assert grade_pull_request("refactor: move the walker", commits)
        assert grade_pull_request("refactor!: move the walker", commits) == []

    def test_a_non_breaking_branch_under_a_valid_title_is_accepted(self) -> None:
        assert grade_pull_request("ci: gate the PR title", ["ci: gate the PR title"]) == []

    def test_a_breaking_title_over_a_non_breaking_branch_is_accepted(self) -> None:
        """The title may outrank the branch — over-declaring costs a minor, never a lost boundary."""
        assert grade_pull_request("feat!: a deliberate break", ["feat: the work"]) == []


class TestTheFailureMessageNamesTheBump:
    """
    AC2. "does not match regex" gets re-run; "this title cuts a PATCH and you wanted a MINOR" gets
    read. The bump names are asserted in the message text, not merely returned by `bump_for`,
    because the message is the artifact a human acts on.
    """

    def test_bump_for_follows_the_measured_zero_x_table(self) -> None:
        assert bump_for(breaking=False) == "patch"
        assert bump_for(breaking=True) == "minor"

    def test_the_crate_is_still_zero_x(self) -> None:
        """
        The gate's messages state the `0.x` rule — where `feat:` is a PATCH — because that is the
        half of the bump table this repository measured. The day `Cargo.toml` reaches `1.0.0` that
        sentence becomes wrong, and this is what says so instead of letting it rot.
        """
        version = next(
            line.split('"')[1]
            for line in (REPO_ROOT / "Cargo.toml").read_text(encoding="utf-8").splitlines()
            if line.startswith("version = ")
        )

        assert version.startswith("0."), (
            f"the crate is now {version}: the `0.x` wording in scripts/pr_title_gate.py and the bump "
            f"table in CONTRIBUTING.md describe a rule that no longer applies"
        )

    def test_a_title_with_no_type_names_both_bumps(self) -> None:
        (failure,) = grade_pull_request(PR_17_TITLE, ["fix: the work"])

        assert "patch" in failure.lower()
        assert PR_17_TITLE in failure, "the message must quote the title it graded"

    def test_a_downgraded_breaking_change_names_both_bumps_and_the_commit(self) -> None:
        (failure,) = grade_pull_request("fix: audit the Rust code", PR_17_COMMITS)

        assert "patch" in failure.lower()
        assert "minor" in failure.lower()
        assert "fix!: stop on a closed pipe" in failure, "the message must name the breaking commit"


class TestTheGateReadsTheArtifactAndFailsClosed:
    """
    An input shape the gate did not expect must make it RED, never green-with-a-warning and never
    skipped. A gate that a surprising input disables is not a gate.
    """

    @pytest.mark.parametrize("title", ["", "   ", "\n"])
    def test_an_empty_title_is_rejected_rather_than_waved_through(self, title: str) -> None:
        assert grade_pull_request(title, ["fix: the work"])

    def test_an_empty_commit_list_is_rejected(self) -> None:
        """
        Every pull request has at least one commit, so zero means the gate read the wrong thing —
        a bad API response, a wrong PR number, an empty `jq` filter. Passing on it would mean the
        breaking-change half of the gate silently disables itself whenever the fetch misfires.
        """
        assert grade_pull_request("fix: a valid title", [])


class TestTheGuardIsWiredIntoTheEntryPoint:
    """
    Everything above tests the guard as a function. This tests that the CI job still CALLS it.

    In this epic a guard has already been deleted from a `main()` with 142 tests still green, so
    the ordering is deliberate: these run the script exactly as `.github/workflows/ci.yml` does —
    the title through the `PR_TITLE` environment variable, the commits through the JSON file `gh
    api` writes — and assert on the process's exit code and stderr, which is all CI can see.
    """

    def run_script(self, title: str | None, commits: list[str], tmp_path: Path) -> subprocess.CompletedProcess[str]:
        commits_path = tmp_path / "commits.json"
        # The shape `gh api repos/<owner>/<repo>/pulls/<n>/commits` returns, reduced to the one
        # field the gate reads.
        commits_path.write_text(json.dumps([{"commit": {"message": m}} for m in commits]), encoding="utf-8")
        env = {"PATH": "/usr/bin:/bin"}
        if title is not None:
            env["PR_TITLE"] = title
        return subprocess.run(
            [
                sys.executable,
                str(REPO_ROOT / "scripts" / "pr_title_gate.py"),
                "--event",
                "pull_request",
                "--commits",
                str(commits_path),
            ],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
            env=env,
            check=False,
        )

    def test_a_good_title_exits_zero(self, tmp_path: Path) -> None:
        result = self.run_script("ci: gate the PR title", ["ci: gate the PR title"], tmp_path)

        assert result.returncode == 0, result.stderr

    def test_the_real_pr_17_exits_non_zero(self, tmp_path: Path) -> None:
        result = self.run_script(PR_17_TITLE, PR_17_COMMITS, tmp_path)

        assert result.returncode != 0, "the script accepted the exact input that cost v0.4.0"
        assert "patch" in result.stderr.lower()

    def test_a_valid_title_over_a_breaking_branch_exits_non_zero(self, tmp_path: Path) -> None:
        result = self.run_script("fix: audit the Rust code", PR_17_COMMITS, tmp_path)

        assert result.returncode != 0
        assert "fix!: stop on a closed pipe" in result.stderr

    def test_an_unset_pr_title_exits_non_zero(self, tmp_path: Path) -> None:
        """`PR_TITLE` unset means the workflow wiring broke, not that the title is fine."""
        result = self.run_script(None, ["fix: the work"], tmp_path)

        assert result.returncode != 0
        assert "PR_TITLE" in result.stderr

    def test_an_unreadable_commits_file_exits_non_zero(self, tmp_path: Path) -> None:
        result = subprocess.run(
            [
                sys.executable,
                str(REPO_ROOT / "scripts" / "pr_title_gate.py"),
                "--event",
                "pull_request",
                "--commits",
                str(tmp_path / "does-not-exist.json"),
            ],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
            env={"PATH": "/usr/bin:/bin", "PR_TITLE": "fix: a valid title"},
            check=False,
        )

        assert result.returncode != 0

    def test_a_commits_payload_of_the_wrong_shape_exits_non_zero(self, tmp_path: Path) -> None:
        """
        `gh api --paginate --slurp` returns an array of PAGES, not of commits; piping it through
        `jq 'add'` is what flattens it. If that pipe is ever dropped the gate must go red rather
        than quietly grade a list of lists as a list of commits with no `!` in it.

        The message is asserted, not only the exit code, and that is not decoration. Deleting the
        shape check leaves this input STILL failing — at `entry["commit"]` with `TypeError: list
        indices must be integers` — so an exit-code-only assertion comes back green on a mutation
        that removed the guard entirely. The named diagnostic is what isolates it, and it is also
        the difference between a maintainer reading "the `jq 'add'` flatten is missing" and reading
        a traceback.
        """
        commits_path = tmp_path / "commits.json"
        commits_path.write_text(json.dumps([[{"commit": {"message": "fix!: a break"}}]]), encoding="utf-8")

        result = subprocess.run(
            [
                sys.executable,
                str(REPO_ROOT / "scripts" / "pr_title_gate.py"),
                "--event",
                "pull_request",
                "--commits",
                str(commits_path),
            ],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
            env={"PATH": "/usr/bin:/bin", "PR_TITLE": "fix: a valid title"},
            check=False,
        )

        assert result.returncode != 0
        assert "flatten" in result.stderr, "the refusal must name the cause, not fail by accident"
        assert "list indices" not in result.stderr, (
            "this is the accidental failure the deleted guard leaves behind, not the guard firing"
        )


# The squashed merge commit that PR #17 actually produced, read out of `git log -1 033ceeb` on
# 2026-07-29. The subject carries GitHub's ` (#17)` suffix, and the branch's three commit subjects
# survive as `* ` bullets in the body — including the `fix!:` at body line 77, which is the marker
# that existed, survived the merge, and was read by nothing. Reproduced verbatim rather than fetched
# with `git` at test time: `actions/checkout` clones to depth 1, so this commit does not exist in
# CI's checkout and a test that shelled out to git would fail there for the wrong reason.
MERGED_17_SUBJECT = "Audit the Rust code with /rust-skills, and close what it found (#17)"
MERGED_17_BODY = """\
* fix: normalise an exclude glob before judging it, and gate rustdoc

The /rust-skills audit of `src/**/*.rs`, `build.rs` and `tests/*.rs` against the
skill's 265 rules across 26 categories.

* fix!: stop on a closed pipe, and refuse a backslash in `exclude`

This is a fix rather than a break: the entry was never honoured, so no working
configuration changes meaning.

* fix: report an unwritable stdout instead of exiting in silence
"""


class TestTheGrammarMatchesWhatReleasePlzActuallyParses:
    """
    The gate stands in for release-plz's parser, so a divergence is a defect in one of two
    directions — and only one of them is loud.

    If the gate is STRICTER it blocks a title release-plz would have accepted: annoying, visible,
    fixed by editing the title. If the gate is LOOSER it passes a title release-plz then misprices:
    silent, and it is the exact shape of the defect this whole job exists for. Both directions are
    pinned here.

    Every case below was MEASURED against release-plz 0.3.160 on 2026-07-29, on a throwaway clone
    of this repository at the live `v0.3.4` tag, one commit per run, reading both the version answer
    and the CHANGELOG section the entry landed in. The commands and their output are recorded in
    CONTRIBUTING.md under "How the bump table was measured".
    """

    def test_an_empty_scope_is_not_a_conventional_subject(self) -> None:
        """
        MEASURED: `feat()!: break the API` -> `0.3.4 -> 0.3.5`, a PATCH, and the CHANGELOG entry is
        the raw subject with no `[**breaking**]` marker. Compare `feat(cli)!:` -> `0.3.4 -> 0.4.0`.

        So the `!` is silently ignored when the parens are empty. The gate used to accept this as
        breaking — LOOSER than the parser, which is the dangerous direction: a PR titled this way
        over a breaking branch passed the gate and would still have shipped a patch.
        """
        assert parse_subject("feat()!: break the API") is None

    def test_a_scope_of_only_whitespace_is_accepted_because_release_plz_accepts_it(self) -> None:
        """
        MEASURED: `feat( )!: break the API` -> `0.3.4 -> 0.4.0`, and the CHANGELOG reads
        `*( )* [**breaking**]`. So the rule is non-EMPTY parens, not non-BLANK ones. Rejecting this
        would make the gate stricter than the parser for no safety gain.
        """
        assert parse_subject("feat( )!: break the API") == Subject(type="feat", breaking=True)

    def test_no_space_after_the_colon_is_accepted_because_release_plz_accepts_it(self) -> None:
        """
        MEASURED: `fix:no space after the colon` -> `0.3.4 -> 0.3.5` filed under `### Fixed` with
        the summary `no space after the colon` — so release-plz parsed it as a `fix`, type and all.

        The gate used to reject it. That errs SAFE (a false red, never a mispriced release), but a
        gate that reddens a title the release tool accepts is a gate people learn to route around.
        """
        assert parse_subject("fix:no space after the colon") == Subject(type="fix", breaking=False)


class TestTheBreakingFooterMatchesWhatReleasePlzActuallyParses:
    """
    The `token #value` footer form, which the gate used to miss entirely.

    Same measurement conditions as the class above. The miss was the dangerous direction: a commit
    whose only breaking declaration was `BREAKING CHANGE #123` read as non-breaking, so a title
    without `!` sailed through and the release would have been a patch.
    """

    @pytest.mark.parametrize(
        "footer",
        [
            "BREAKING CHANGE: the exit codes changed",  # MEASURED -> 0.4.0
            "BREAKING-CHANGE: the exit codes changed",  # MEASURED -> 0.4.0
            "BREAKING CHANGE #123",  # MEASURED -> 0.4.0
            "BREAKING-CHANGE #123",  # MEASURED -> 0.4.0
        ],
    )
    def test_a_form_release_plz_treats_as_breaking_is_breaking_here_too(self, footer: str) -> None:
        assert message_is_breaking(f"refactor: a change\n\n{footer}") is True

    @pytest.mark.parametrize(
        "footer",
        [
            "BREAKING CHANGE#123",  # MEASURED -> 0.3.5: no space before the hash, not a footer
            "breaking change: the exit codes changed",  # MEASURED -> 0.3.5: the token is uppercase
            "BREAKING CHANGES: the exit codes changed",  # MEASURED -> 0.3.5: singular, not plural
        ],
    )
    def test_a_form_release_plz_ignores_is_not_breaking_here_either(self, footer: str) -> None:
        """
        Matching these would make the gate stricter than the parser. That errs safe, but each one
        is a title the author would be told to change for no reason — and `BREAKING CHANGES:` in
        particular is a plausible thing to write in prose.
        """
        assert message_is_breaking(f"refactor: a change\n\n{footer}") is False


class TestATruncatedCommitListIsRefused:
    """
    F4. `repos/{owner}/{repo}/pulls/{n}/commits` returns **at most 250** commits, and `--paginate`
    cannot lift it — the cap is on the endpoint, not on the page size (GitHub REST documentation,
    "Lists a maximum of 250 commits for a pull request").

    So a 251-commit pull request whose only `!` sits at position 251 comes back looking clean. The
    gate must refuse to grade a list it can see is truncated rather than return a verdict about the
    half it was given. It will never fire on this repository — the largest pull request here has
    three commits — and that is fine: a guard that silently grades half its input is precisely the
    fail-open shape this epic keeps shipping.
    """

    def test_a_list_at_the_cap_is_refused_even_when_every_commit_is_clean(self) -> None:
        failures = grade_pull_request("fix: a valid title", ["fix: clean"] * API_COMMIT_CAP)

        assert failures
        assert str(API_COMMIT_CAP) in failures[0]

    def test_a_list_one_short_of_the_cap_is_graded_normally(self) -> None:
        assert grade_pull_request("fix: a valid title", ["fix: clean"] * (API_COMMIT_CAP - 1)) == []


class TestTheSubjectThatActuallyLandedOnMainIsGraded:
    """
    F1.2, and it is the difference between grading a proxy and grading the artifact.

    A pre-merge title check grades what the title WAS. GitHub's squash dialog lets the merger edit
    the subject at the moment of merge, and no branch protection exists here to stop them, so the
    string release-plz finally parses can differ from every string this gate ever saw. The only
    thing that grades what release-plz will actually read is the subject that landed on `main`.

    It fires after the merge but BEFORE the Release PR is merged — a separate, manual step — so a
    boundary caught here is still correctable.

    After a squash the branch's own commit subjects survive as `* ` bullets in the body. That is
    where `033ceeb` kept its `fix!:`, and reading them back out is what makes this the same
    comparison the pull-request path makes.
    """

    def test_the_real_merged_033ceeb_is_rejected(self) -> None:
        failures = grade_merged_commit(MERGED_17_SUBJECT, MERGED_17_BODY)

        assert failures, "the commit that actually shipped v0.3.4 as a patch was graded as fine"

    def test_a_valid_subject_over_the_same_body_is_still_rejected(self) -> None:
        """The squash-dialog exploit: a title that passed the pre-merge gate, edited at merge."""
        assert grade_merged_commit("fix: audit the Rust code (#17)", MERGED_17_BODY)

    def test_declaring_the_break_in_the_landed_subject_is_accepted(self) -> None:
        assert grade_merged_commit("fix!: audit the Rust code (#17)", MERGED_17_BODY) == []

    def test_a_release_commit_is_accepted(self) -> None:
        """`chore: release v0.3.4 (#18)` is what release-plz's own merge looks like."""
        assert grade_merged_commit("chore: release v0.3.4 (#18)", "") == []

    def test_prose_about_breaking_changes_is_not_a_breaking_marker(self) -> None:
        """
        The false-positive case, and it is not hypothetical: the body of `d6e7561` — this gate's
        own commit — contains three lines mentioning `fix!:` or `BREAKING CHANGE:` while describing
        the defect. A substring scan fires three times on it. Markers are read only from `* ` bullet
        subjects and line-anchored footers, which is why this is silent.
        """
        body = (
            "found` over a branch carrying `fix!: stop on a closed pipe, and refuse a\n"
            "change. Re-measured on the live tag: the same change under a `fix!:` title\n"
            "and that no commit on the branch carries a `!` or a `BREAKING CHANGE:` footer\n"
        )

        assert grade_merged_commit("ci: gate the PR title (#19)", body) == []


class TestThePushPathIsWiredIntoTheEntryPoint:
    """
    The `push: main` half of the entry point, run the way the workflow runs it: the commit message
    arrives in a file written by `git log -1 --format=%B`, and the event is named explicitly.
    """

    def run_script(self, message: str, tmp_path: Path) -> subprocess.CompletedProcess[str]:
        message_path = tmp_path / "merged.txt"
        message_path.write_text(message, encoding="utf-8")
        return subprocess.run(
            [
                sys.executable,
                str(REPO_ROOT / "scripts" / "pr_title_gate.py"),
                "--event",
                "push",
                "--merged-message",
                str(message_path),
            ],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
            env={"PATH": "/usr/bin:/bin"},
            check=False,
        )

    def test_the_real_merged_033ceeb_exits_non_zero(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            result = self.run_script(f"{MERGED_17_SUBJECT}\n\n{MERGED_17_BODY}", Path(tmp))

        assert result.returncode != 0, "the commit that actually shipped v0.3.4 as a patch was accepted"
        assert "fix!: stop on a closed pipe" in result.stderr

    def test_a_clean_merge_exits_zero(self, tmp_path: Path) -> None:
        result = self.run_script("ci: gate the PR title (#19)\n\nSome prose.\n", tmp_path)

        assert result.returncode == 0, result.stderr

    def test_a_missing_merged_message_exits_non_zero(self, tmp_path: Path) -> None:
        result = subprocess.run(
            [sys.executable, str(REPO_ROOT / "scripts" / "pr_title_gate.py"), "--event", "push"],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
            env={"PATH": "/usr/bin:/bin"},
            check=False,
        )

        assert result.returncode != 0

    @pytest.mark.parametrize("event", ["merge_group", "workflow_dispatch", "schedule", ""])
    def test_an_unrecognised_event_is_refused(self, event: str, tmp_path: Path) -> None:
        """
        F5, and it is defect class 2 — a guard switched off by an input shape nobody anticipated.

        The previous wiring said "if this is not a `pull_request`, exit 0", so a `merge_group` event
        — or any trigger a later task adds — reported SUCCESS having graded nothing at all. The
        allow-list inverts that: the two known events are handled, everything else is refused.
        """
        message_path = tmp_path / "merged.txt"
        message_path.write_text("fix: whatever\n", encoding="utf-8")
        result = subprocess.run(
            [
                sys.executable,
                str(REPO_ROOT / "scripts" / "pr_title_gate.py"),
                "--event",
                event,
                "--merged-message",
                str(message_path),
            ],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
            env={"PATH": "/usr/bin:/bin"},
            check=False,
        )

        assert result.returncode != 0, f"the event {event!r} was accepted and graded nothing"


class TestTheWorkflowCannotDisableItsOwnGate:
    """
    F3. Every assertion above runs the SCRIPT; none of them can see the workflow that calls it, and
    a gate is only as real as its invocation.

    Deleting the invocation is caught by the last test here, but deletion was never the cheap
    attack. `|| true` appended, `set -o pipefail` dropped, `if: false` added to the job — each one
    leaves the script perfect, the workflow syntactically valid, and the gate dead. Those are the
    four this class pins, and every one of them is mutation-proved.

    Parsed as YAML, not substring-matched. That is why `pyyaml` is now in the `test` dependency
    group: the previous substring assertion looked for `scripts/pr_title_gate.py`, which also
    appears in a `name:` and in three comment lines, so replacing the command with `echo` left the
    test GREEN — satisfied by prose about the guard it no longer had. `scripts/coverage_report.py`
    set this precedent: parse the artifact, do not pattern-match near it.
    """

    GATE_WORKFLOW = "release-contract.yml"
    # The workflows whose jobs are gates. `release-plz.yml` is excluded deliberately: it is a
    # release mechanism, not a check, and its jobs legitimately carry conditions.
    GATE_WORKFLOWS = ("ci.yml", "release-contract.yml")

    def workflow(self, name: str) -> dict[Any, Any]:
        import yaml

        loaded: Any = yaml.safe_load((REPO_ROOT / ".github" / "workflows" / name).read_text(encoding="utf-8"))
        assert isinstance(loaded, dict)
        return loaded

    def triggers(self, name: str) -> dict[str, Any]:
        """
        Return a workflow's `on:` block.

        Looked up under `True`, not `"on"`. YAML 1.1 — which is what PyYAML implements —
        resolves the unquoted plain scalar `on` to the BOOLEAN true, so `workflow["on"]` raises
        `KeyError` on a file that plainly reads `on:`. Both spellings are accepted here so a
        future quoted `"on":` does not silently skip these assertions.
        """
        loaded = self.workflow(name)
        block: Any = loaded[True] if True in loaded else loaded["on"]
        assert isinstance(block, dict)
        return block

    def gate_steps(self) -> list[dict[str, Any]]:
        jobs: Any = self.workflow(self.GATE_WORKFLOW)["jobs"]
        return [step for job in jobs.values() for step in job["steps"]]

    def invocations(self) -> list[str]:
        """Return the shell bodies that actually run the gate."""
        return [step["run"] for step in self.gate_steps() if "scripts/pr_title_gate.py" in str(step.get("run", ""))]

    @pytest.mark.parametrize("name", GATE_WORKFLOWS)
    def test_no_gate_job_carries_a_condition_or_a_path_filter(self, name: str) -> None:
        """
        AC3, as a test rather than as a parse someone ran by hand once.

        A SKIPPED check does not satisfy a required status check — it stays pending — so a job-level
        `if:` or `paths:` wedges the pull request at "Expected — waiting for status to be reported"
        on the day branch protection is enabled, with CI entirely green.
        """
        jobs: Any = self.workflow(name)["jobs"]

        assert jobs, f"{name} declares no jobs"
        for job_name, job in jobs.items():
            for key in ("if", "paths", "paths-ignore"):
                assert key not in job, f"{name}: job `{job_name}` carries `{key}:`"

    def test_both_paths_are_actually_invoked(self) -> None:
        """
        BOTH, named individually. Asserting only that *an* invocation exists is not enough: this
        workflow runs the gate twice, so deleting the whole `pull_request` branch left the `push`
        branch satisfying the assertion and the mutation came back GREEN. The two are separate
        guarantees — one grades the proxy before the merge, the other the artifact after it — and a
        test that cannot tell them apart protects neither.
        """
        joined = "\n".join(self.invocations())

        assert joined, "no step in the gate workflow runs scripts/pr_title_gate.py"
        assert "--event pull_request" in joined, "the pre-merge title check is no longer invoked"
        assert "--event push" in joined, "the merged-subject check is no longer invoked"

    def test_every_invocation_runs_under_pipefail(self) -> None:
        """
        Without `pipefail` the `gh api … | jq …` pipe reports the exit status of `jq`, so an API
        failure leaves an empty-but-valid commit list and the shell carries on. The script's own
        empty-list refusal catches that one — but `set -e` is what stops the step at the first
        failure at all, and dropping either is a one-word edit.
        """
        for body in self.invocations():
            assert "set -euo pipefail" in body, f"an invocation lost `set -euo pipefail`:\n{body}"

    def test_no_invocation_has_an_escape_hatch(self) -> None:
        """
        `|| true`, `|| :`, `|| exit 0` — any trailing disjunction turns a red gate green while
        leaving every other test in this file passing. This is the cheapest possible way to disable
        the gate and the hardest to notice in review.

        ⚠️ Backslash continuations are joined FIRST, and that is not tidiness. The first version of
        this test scanned raw lines and asked whether a line mentioning `pr_title_gate.py` carried
        `||` — and the real invocation spans two physical lines, so appending `|| true` to the
        continuation put the escape hatch on a line that names neither the script nor `gh api`. The
        mutation came back GREEN. A shell guard has to be read in shell's units, not in YAML's.
        """
        for body in self.invocations():
            logical = body.replace("\\\n", " ")
            for line in logical.splitlines():
                if "pr_title_gate.py" in line or "gh api" in line:
                    assert "||" not in line, f"an invocation carries an escape hatch:\n{line}"

    def test_the_title_still_comes_from_the_event_payload(self) -> None:
        joined = (REPO_ROOT / ".github" / "workflows" / self.GATE_WORKFLOW).read_text(encoding="utf-8")

        assert "PR_TITLE: ${{ github.event.pull_request.title }}" in joined, (
            "the gate must read the REAL title from the event payload, never a value a workflow "
            "input or a job summary supplies"
        )

    def test_the_gate_fires_when_a_title_is_edited(self) -> None:
        """
        F1.1, and it is the finding that made this fix round necessary.

        `on: pull_request:` with no `types:` fires on `opened, synchronize, reopened` ONLY. Editing
        a title fires nothing, so the check keeps reporting the verdict it reached about the
        PREVIOUS title — proved live on PR #19, where changing the title produced no new run at all.
        Reversed, that is the exploit: pass the gate with a good title, edit it to a mispricing one,
        merge on a stale green.
        """
        on = self.triggers(self.GATE_WORKFLOW)

        assert "edited" in on["pull_request"]["types"], (
            "a title edit must re-run the gate, or its verdict describes a title that no longer exists"
        )

    def test_the_gate_also_grades_what_landed_on_main(self) -> None:
        """F1.2 — the pre-merge check is a proxy; the squash subject on `main` is the artifact."""
        on = self.triggers(self.GATE_WORKFLOW)

        assert on["push"]["branches"] == ["main"]
