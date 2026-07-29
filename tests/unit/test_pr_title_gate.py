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

import pytest
from pr_title_gate import KNOWN_TYPES, Subject, bump_for, grade_pull_request, message_is_breaking, parse_subject

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
            "fix:no space after the colon",
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
            [sys.executable, str(REPO_ROOT / "scripts" / "pr_title_gate.py"), "--commits", str(commits_path)],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
            env=env,
            check=False,
        )

    def test_ci_still_invokes_the_script(self) -> None:
        """
        The LAST call site is `.github/workflows/ci.yml`, and no test above reaches it: every one
        of them runs the script itself. Delete the invocation from the workflow and this whole file
        stays green while nothing grades a single PR title — which is the defect this epic has
        already shipped once, in task 8, where a guard was deleted from a `main()` with 142 tests
        still passing.

        A substring check, not a YAML parse: pyyaml is not a dependency of this repository and
        adding one to assert a string is present would cost more than it guards.

        ⚠️ It must match the INVOCATION, not the bare script path. The first version of this
        assertion looked for `scripts/pr_title_gate.py`, which also appears in the step's `name:`
        and three times in that job's comment — so replacing the command with `echo` left this test
        green, satisfied by prose about the guard it no longer had. Mutation-proved both ways.
        """
        workflow = (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")

        assert "uv run --no-project python3 scripts/pr_title_gate.py --commits" in workflow, (
            "the pr-title CI job no longer RUNS the gate"
        )
        assert "PR_TITLE: ${{ github.event.pull_request.title }}" in workflow, (
            "the gate must read the REAL title from the event payload, never a value a workflow "
            "input or a job summary supplies"
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
            [sys.executable, str(REPO_ROOT / "scripts" / "pr_title_gate.py"), "--commits", str(commits_path)],
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
