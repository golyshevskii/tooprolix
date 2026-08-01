"""
Guards for promises that live in `.github/workflows/` rather than in code.

A workflow file is not documentation: it decides which commit gets built, and a wrong key in it
fails silently — as a `cancel`, which reads as "not red" and blocks nothing. This file pins the
workflow settings whose breakage would otherwise be invisible until a release.

Run: make test
"""

from __future__ import annotations

import re
import subprocess
import tempfile
import textwrap
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

import pytest

REPO: Path = Path(__file__).parents[2]
WORKFLOWS: Path = REPO / ".github" / "workflows"
BUILD_ARTIFACTS: Path = WORKFLOWS / "build-artifacts.yml"
CI: Path = WORKFLOWS / "ci.yml"
RELEASE_PLZ: Path = WORKFLOWS / "release-plz.yml"

CONCURRENCY_GROUP = re.compile(r"^concurrency:\n\s+group:\s*(?P<group>.+)$", re.MULTILINE)

#: The step whose shell IS the stale-tree guard. The tests below execute that shell rather than
#: re-implementing it, so a mutation to the workflow is what they grade.
STALE_TREE_GUARD_STEP = "Refuse to tag a tree that is not main's"

#: Every `make` target `ci.yml` ran before the eight jobs were consolidated into four. The
#: consolidation's one real risk is dropping a gate while the run still goes green, and a job count
#: cannot see that — only the target list can.
EXPECTED_MAKE_TARGETS: frozenset[str] = frozenset(
    {"lint.check", "type", "test", "rust.fmt.check", "rust.lint", "rust.build", "rust.test", "rust.doc", "cov"}
)

#: `(tag, the merge commit that landed it on main, whether invariant A holds)`, measured at
#: `bb327cc`. Invariant A: the tag's tree equals the tree of the merge commit that landed it.
#: The three FALSE rows are real releases that shipped a tree `main` never had.
RELEASE_MERGES: tuple[tuple[str, str, bool], ...] = (
    ("v0.4.2", "3ee7001", True),
    ("v0.4.3", "fbe760a", False),
    ("v0.4.4", "375e846", True),
    ("v0.4.5", "87aa67e", False),
    ("v0.4.6", "0fd27cd", False),
    ("v0.4.7", "bb327cc", True),
)


def uncommented(path: Path) -> str:
    """
    Read a workflow with its whole-line `#` comments removed.

    Every comment in these files quotes the very keys the tests below forbid — `ci.yml:54-59` spells
    out "no `paths:`, `paths-ignore:` or `if:`". A substring search over the raw text would grade the
    prose describing the rule instead of the YAML obeying it.
    """
    return "\n".join(
        line for line in path.read_text(encoding="utf-8").splitlines() if not line.lstrip().startswith("#")
    )


def top_level_block(text: str, key: str) -> str:
    """Return the indented body of the top-level mapping `key`."""
    opener = re.search(rf"^{re.escape(key)}:\s*$", text, re.MULTILINE)
    assert opener is not None, f"no top-level `{key}:` block"
    rest = text[opener.end() :]
    following = re.search(r"^\S", rest, re.MULTILINE)
    return rest[: following.start()] if following else rest


def keys_at(block: str, indent: int) -> list[str]:
    """Return the mapping keys declared at exactly `indent` spaces, in file order."""
    return re.findall(rf"^ {{{indent}}}([A-Za-z][\w-]*):", block, re.MULTILINE)


def jobs(text: str) -> dict[str, str]:
    """Split a workflow's `jobs:` block into `{job id: the job's own YAML}`."""
    split: dict[str, str] = {}
    name: str | None = None
    body: list[str] = []
    for line in top_level_block(text, "jobs").splitlines():
        header = re.match(r"^ {2}([A-Za-z][\w-]*):\s*$", line)
        if header:
            if name is not None:
                split[name] = "\n".join(body)
            name, body = header.group(1), []
        elif name is not None:
            body.append(line)
    if name is not None:
        split[name] = "\n".join(body)
    return split


def job_condition(job: str) -> str | None:
    """Return a job's own `if:` expression, or `None` when it carries none."""
    matched = re.search(r"^ {4}if:(?P<value>.*(?:\n {6,}\S.*)*)", job, re.MULTILINE)
    return matched.group("value").strip() if matched else None


def make_targets(job: str) -> set[str]:
    return set(re.findall(r"^\s+run: make (\S+)$", job, re.MULTILINE))


def stale_tree_guard() -> str:
    """Extract the guard's shell out of `release-plz.yml`, so the tests run the shipped artifact."""
    body = re.search(
        rf"- name: {re.escape(STALE_TREE_GUARD_STEP)}\n(?P<pad> +)run: \|\n(?P<script>(?:(?P=pad) +.*\n|[ \t]*\n)+)",
        RELEASE_PLZ.read_text(encoding="utf-8"),
    )
    assert body is not None, f"release-plz.yml has no `{STALE_TREE_GUARD_STEP}` step with a `run: |` script"
    return textwrap.dedent(body.group("script"))


def run_guard(cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["bash", "-c", stale_tree_guard()], cwd=cwd, capture_output=True, text=True, check=False)


@contextmanager
def checkout_of(commit: str) -> Iterator[Path]:
    """
    Yield a worktree whose HEAD is `commit`, so the guard reads a real repository state.

    `--no-checkout`: the guard only asks git plumbing questions, so materialising the files would be
    seconds of I/O per historical release for nothing.
    """
    with tempfile.TemporaryDirectory() as tmp:
        worktree = Path(tmp) / "release"
        subprocess.run(
            ["git", "-C", str(REPO), "worktree", "add", "--no-checkout", "--detach", str(worktree), commit],
            check=True,
            capture_output=True,
            text=True,
        )
        try:
            yield worktree
        finally:
            subprocess.run(
                ["git", "-C", str(REPO), "worktree", "remove", "--force", str(worktree)],
                check=False,
                capture_output=True,
            )


# ---------------------------------------------------------------------------------------------
# build-artifacts.yml — which commits get built, and how many times
# ---------------------------------------------------------------------------------------------


def test_an_artifact_build_is_only_cancelled_by_another_build_of_the_same_commit() -> None:
    """
    The artifact build must belong to the COMMIT it built, not to the branch it arrived on.

    One logical event can produce two pushes to one new ref within seconds — release-plz creates the
    branch at the base tip and pushes the release commit, and GitHub delivers those as separate
    events. Keyed on `github.ref` with `cancel-in-progress`, the later run kills the earlier one on
    the assumption that later means newer. That assumption is false here, and it lost the race for
    real: on PR #37 (`chore: release v0.4.3`) all four artifact checks came back `cancel`, and the
    only surviving build was of the base tip — a commit that PR does not propose. PR #35 raced the
    same way and happened to win, which is worse than losing, because it makes the defect look
    absent.

    Keyed on `github.sha`, a build can only be cancelled by another run of the same commit, so a
    commit that reaches a tag has artifacts built from its own tree. Duplicate events for one SHA
    still collapse, which is all `cancel-in-progress` was ever wanted for.
    """
    matched = CONCURRENCY_GROUP.search(BUILD_ARTIFACTS.read_text(encoding="utf-8"))
    assert matched is not None, "build-artifacts.yml declares no concurrency group"
    group: str = matched.group("group")

    assert "github.sha" in group, f"artifact builds must be grouped by commit, group is {group!r}"
    assert "github.ref" not in group, f"grouping by ref cancels other commits' builds, group is {group!r}"


def test_artifacts_are_built_for_main_v_tags_and_pull_requests_and_nothing_else() -> None:
    """
    A bare `push:` builds every ref, and a release cycle then pays for the same tree three times.

    Measured over the `v0.4.6` cycle: six full artifact matrices, of which at most two were
    load-bearing. The duplicates come from the BRANCH axis — GitHub delivers a branch-CREATION push
    for every release-plz branch, so the base tip is rebuilt before the release commit even exists.
    Restricting the trigger to `main`, `v*` and pull requests deletes those without touching the
    path axis, which must stay unfiltered.
    """
    triggers = top_level_block(uncommented(BUILD_ARTIFACTS), "on")

    assert set(keys_at(triggers, 2)) == {"workflow_dispatch", "pull_request", "push"}, (
        f"unexpected trigger set: {keys_at(triggers, 2)}"
    )
    push = top_level_block(triggers.replace("  push:", "push:"), "push")
    assert re.search(r"branches:\s*\[\s*main\s*\]", push), f"push trigger does not name main only: {push!r}"
    assert re.search(r"tags:\s*\[\s*'v\*'\s*\]", push), f"push trigger does not name v* tags: {push!r}"


def test_the_wheel_matrix_is_gated_on_release_events_and_the_sdist_is_not_gated_at_all() -> None:
    """
    Three wheel legs cost 29 of the 32 billed minutes an ordinary PR spends on artifacts.

    Measured per PR: sdist 3 + linux 3 + macos 20 (x10 private-repo multiplier) + windows 6. A
    cross-platform break still has to be caught BEFORE the tag, so the gate lets the wheels through
    on exactly the events that precede one — the release-plz PR, a `v*` tag, and a manual dispatch —
    and an ordinary PR keeps the sdist, which is the leg that grades the README transform and the
    archive contents.

    It is an EVENT condition, never a path one: a path filter would skip the wheels on the
    README-only change that is precisely the change able to break the transformer.
    """
    build = jobs(uncommented(BUILD_ARTIFACTS))
    assert job_condition(build["sdist"]) is None, "the sdist must run on every event this workflow accepts"

    gate = job_condition(build["wheels"])
    assert gate is not None, "the wheel matrix is ungated, so every ordinary PR pays for three wheel legs"
    assert "workflow_dispatch" in gate, gate
    assert "refs/tags/v" in gate, gate
    assert "release-plz-" in gate, gate


# ---------------------------------------------------------------------------------------------
# ci.yml — what runs, on which events, with which hard-won settings intact
# ---------------------------------------------------------------------------------------------


def test_ordinary_ci_runs_on_the_exact_v_tag() -> None:
    """
    Publication uploads from a `v*` tag, and no ordinary CI job has ever run at a tag SHA.

    Measured at `eb65e32`: five tags, five workflow runs, all of them `Build artifacts`. Zero
    lint/type/test/cargo runs on any tag, ever. The release PR's own CI does not stand in for it —
    on `pull_request` `actions/checkout` materialises `refs/pull/N/merge`, which for `v0.4.6` was
    tree `3a1f67fd` while the tag's tree was `5d0ceefb`.
    """
    triggers = top_level_block(uncommented(CI), "on")
    push = top_level_block(triggers.replace("  push:", "push:"), "push")

    assert re.search(r"tags:\s*\[\s*'v\*'\s*\]", push), f"ci.yml does not run on v* tags: {push!r}"
    assert re.search(r"branches:\s*\[\s*main\s*\]", push), (
        "post-merge CI on main stays while branch protection is absent — nothing stops a direct push"
    )


def test_ci_reports_the_four_work_jobs_and_one_aggregate() -> None:
    """
    A required check name that stops reporting stays required forever, so the names are pinned here.

    Renaming is free only while `branches/main/protection` is 404. The aggregate exists NOW, before
    protection, so `CI / ci-required` is final before it can ever be registered — the deferred path
    classifier reuses this name instead of orphaning one.
    """
    assert set(jobs(uncommented(CI))) == {"ci-python", "ci-rust", "cargo-doc", "coverage", "ci-required"}


def test_the_aggregate_is_unconditional_and_covers_every_required_job() -> None:
    """
    `if: always()` is what makes the aggregate able to see a `cancelled` or `skipped` dependency.

    Without it the aggregate is itself skipped the moment a needed job does not succeed, and a
    skipped check never reports — it wedges a protected PR at "Expected — waiting for status to be
    reported" instead of going red. `coverage` is deliberately outside `needs:`: it protects the
    measuring instrument, not the shipped artifact, and was never a required check.
    """
    aggregate = jobs(uncommented(CI))["ci-required"]

    assert job_condition(aggregate) == "always()", f"the aggregate's condition is {job_condition(aggregate)!r}"
    needs = re.search(r"^ {4}needs:\s*\[(?P<list>[^\]]*)\]", aggregate, re.MULTILINE)
    assert needs is not None, "the aggregate needs nothing, so it is green whatever CI did"
    assert {name.strip() for name in needs.group("list").split(",")} == {"ci-python", "ci-rust", "cargo-doc"}


def test_only_the_aggregate_carries_a_job_condition() -> None:
    """
    A SKIPPED check does not satisfy a required status check — it stays pending, so a conditional
    work job wedges a PR at "Expected — waiting for status to be reported" while CI is entirely
    green. The path classifier that would need such conditions is deferred; until it lands with a
    verified answer for skipped-check semantics, the aggregate is the only job allowed a condition,
    and its condition is `always()`, which can only ever add work.
    """
    conditional = {name: job_condition(job) for name, job in jobs(uncommented(CI)).items() if job_condition(job)}
    assert conditional == {"ci-required": "always()"}, conditional


@pytest.mark.parametrize("workflow", [CI, BUILD_ARTIFACTS])
def test_no_workflow_selects_jobs_by_changed_path(workflow: Path) -> None:
    """
    The old five-file allowlist on `build-artifacts.yml` skipped a README-only change — precisely
    the change that breaks the README transformer — so not one of its checks could go red on it.
    Replacing a stale allowlist with a longer one only moves the boundary; the next file nobody
    thought of has the same effect, and the list rots silently because nothing tells you it is short.
    """
    offending = re.findall(r"^\s*paths(?:-ignore)?:.*$", uncommented(workflow), re.MULTILINE)
    assert offending == [], f"{workflow.name} filters by path: {offending}"


def test_every_make_target_that_ran_before_the_consolidation_still_runs() -> None:
    """
    Consolidating eight jobs into four is a green-to-green refactor, which is exactly the shape that
    can silently delete a gate: fewer red Xs looks like progress. The job count cannot see it. This
    grades the work the run actually performs.
    """
    ran = set().union(*(make_targets(job) for job in jobs(uncommented(CI)).values()))
    assert ran == EXPECTED_MAKE_TARGETS, (
        f"missing {EXPECTED_MAKE_TARGETS - ran}, unexpected {ran - EXPECTED_MAKE_TARGETS}"
    )


def test_every_job_that_runs_the_python_tests_checks_out_the_full_history() -> None:
    """
    `corpus/classification.py` resolves the commit each measurement names with `git cat-file` rather
    than trusting the string, so under the default shallow checkout that lookup fails on a commit
    that genuinely exists and ~20 tests go red for an environment reason. Deliberately not fixed by
    letting the check pass when the repository is shallow — that would be a guard disabled by the
    checkout depth of whoever runs it.

    EVERY job that runs those tests needs it, not just the obvious one. `coverage` runs them too,
    through `make cov` -> `py.cov`, and it went red for the identical reason after the first job was
    fixed alone. This asks which jobs run them rather than naming the jobs, so a fifth one inherits
    the requirement.
    """
    for name, job in jobs(uncommented(CI)).items():
        if make_targets(job) & {"test", "cov"}:
            assert "fetch-depth: 0" in job, f"{name} runs the Python tests on a shallow checkout"


def test_the_job_that_runs_the_rust_tests_installs_uv() -> None:
    """
    Two Rust tests SHELL OUT to `uv`, for reasons unrelated to the deleted pyo3 boundary:
    `tests/cli.rs` proves the JSON document is consumable by a Python consumer, and
    `tests/ruff_compatibility.rs` proves the marker grammar stays invisible to ruff. Deleting the uv
    setup was reasoned from the build ("nothing compiles against an interpreter") and went red on
    PR #34 with `Os { code: 2, kind: NotFound }`.

    The rule is "does anything it EXECUTES shell out to uv?", not "does anything it builds need an
    interpreter?" — so the reason is asserted from the Rust sources, not from a job name.
    """
    shelling_out = [
        path.name
        for path in sorted((REPO / "tests").glob("*.rs"))
        if 'Command::new("uv")' in path.read_text(encoding="utf-8")
    ]
    assert shelling_out, "no Rust test shells out to uv any more — this contract has changed, re-derive it"

    for name, job in jobs(uncommented(CI)).items():
        if make_targets(job) & {"rust.test", "cov"}:
            assert "astral-sh/setup-uv@" in job, f"{name} runs {shelling_out} with no uv on PATH"


def test_no_ci_job_may_write_a_snapshot() -> None:
    """
    An insta snapshot that is NEW or CHANGED must FAIL the job, never be written. Without
    `INSTA_UPDATE: "no"` insta stores a `.snap.new` beside the source, which is one `git add .` away
    from an unreviewed snapshot committed as intent.
    """
    for name, job in jobs(uncommented(CI)).items():
        if make_targets(job) & {"rust.test", "cov"}:
            assert 'INSTA_UPDATE: "no"' in job, f"{name} runs the snapshot tests and may rewrite them"


def test_the_consolidation_kept_every_hardening_setting() -> None:
    """
    Four jobs' worth of steps were merged into two, and each of these settings is one line that a
    merge can drop while the run stays green: an unpinned action is a supply-chain hole, a persisted
    credential outlives the checkout in `.git/config`, and a cache written from every branch lets a
    PR poison the entry `main` reads.
    """
    text = uncommented(CI)

    unpinned = [
        ref for ref in re.findall(r"^\s+uses: (\S+)$", text, re.MULTILINE) if not re.search(r"@[0-9a-f]{40}$", ref)
    ]
    assert unpinned == [], f"actions not pinned to a commit SHA: {unpinned}"
    assert text.count("actions/checkout@") == text.count("persist-credentials: false"), (
        "every checkout must keep the token out of .git/config"
    )
    assert text.count("Swatinem/rust-cache@") == text.count("save-if: ${{ github.ref == 'refs/heads/main' }}"), (
        "only main may write the Rust cache, or a PR can poison the entry every other PR reads"
    )


def test_a_ci_run_emits_the_commit_and_tree_it_actually_checked_out() -> None:
    """
    The run's `head_sha` is a label the run reports ABOUT ITSELF, and it is not evidence.

    Measured: run `30691798694` reports `head_sha=292d8af4` while `actions/checkout` materialised
    tree `3a1f67fd`. A workflow triggered at tag `T` but checking out `main` yields `head_sha=T`,
    `name=CI`, `conclusion=success` — and a publish precondition reading that field passes over a tag
    no ordinary CI ever exercised. An emitted `git rev-parse HEAD` is read off the real working tree
    at the moment the gates run, so it is the artifact; the Actions metadata is the story about it.
    """
    emitting = {
        name: job
        for name, job in jobs(uncommented(CI)).items()
        if "git rev-parse HEAD" in job and "$GITHUB_OUTPUT" in job
    }
    assert emitting, "no job emits what it checked out, so the tag evidence is Actions metadata alone"

    for name, job in emitting.items():
        assert "HEAD^{tree}" in job, f"{name} emits a commit but not the tree that was graded"
        assert make_targets(job), f"{name} emits a tree it never tested, which is the defect being closed"


def test_the_expected_tag_job_manifest_names_every_ci_job() -> None:
    """
    The manifest is the publish precondition's expectation, and it is graded by SET EQUALITY, so a
    job added to `ci.yml` without being added here turns every future release red at the tag —
    after the tag exists. This moves that discovery to the pull request that adds the job.

    Only `ci.yml`'s names are checked from this direction: `build-artifacts.yml`'s wheel legs are
    matrix-expanded, so their reported names are not readable out of the YAML.
    """
    expected = {
        stripped
        for line in (REPO / ".github" / "expected-tag-jobs.txt").read_text(encoding="utf-8").splitlines()
        if (stripped := line.strip()) and not stripped.startswith("#")
    }
    assert expected, "the manifest expects nothing, so it would be satisfied by a run that did nothing"
    assert set(jobs(uncommented(CI))) <= expected, f"missing from the manifest: {set(jobs(uncommented(CI))) - expected}"


# ---------------------------------------------------------------------------------------------
# release-plz.yml — the tag may not point at a tree main never had
# ---------------------------------------------------------------------------------------------


def test_the_release_job_checks_the_tree_before_release_plz_can_tag() -> None:
    """
    A guard that runs after the tagging action has nothing left to refuse: the tag is not a proposal
    and deleting one is a published-artifact problem, not a CI one.
    """
    release = jobs(uncommented(RELEASE_PLZ))["release"]

    assert STALE_TREE_GUARD_STEP in release, "the release job tags with no tree check at all"
    assert release.index(STALE_TREE_GUARD_STEP) < release.index("command: release"), (
        "the tree check runs after the tag already exists"
    )


def test_the_release_pr_is_opened_from_the_current_main_tip() -> None:
    """
    Configuration is not a guard, but it is what stops the guard from ever having to fire.

    The staleness is created here, not at tag time: a `release-pr` run queued behind another checks
    out the SHA of the push that triggered it, and forks the release branch from that older tip. For
    `v0.4.6` the branch was created at `0e1b710` while `main` was already `e9af7ed`, and the tag
    inherited it. Checking out `main` by name makes the release PR fork from whatever `main` is when
    the job actually runs.
    """
    release_pr = jobs(uncommented(RELEASE_PLZ))["release-pr"]
    assert re.search(r"^\s+ref: main$", release_pr, re.MULTILINE), "the release PR may fork from a stale main tip"


@pytest.mark.parametrize(("tag", "merge", "holds"), RELEASE_MERGES)
def test_the_guard_reproduces_every_historical_release(tag: str, merge: str, holds: bool) -> None:
    """
    The guard is graded against the six releases this repository has actually cut.

    Invariant A — the tag's tree must equal the tree of the merge commit that landed it on `main` —
    is TRUE for `v0.4.2`, `v0.4.4` and `v0.4.7` and FALSE for `v0.4.3`, `v0.4.5` and `v0.4.6`, where
    the tag shipped a tree `main` never had: `v0.4.6` is missing PR #48 entirely, six files and
    +88/-83. A guard that passes all six proves nothing, which is why the FALSE rows are here.

    It is NOT an ancestry claim in either direction. With a merge-commit release the tag target is
    an ancestor of `main` on every release, good and bad alike.
    """
    with checkout_of(merge) as worktree:
        result = run_guard(worktree)

    assert (result.returncode == 0) is holds, (
        f"{tag} at {merge}: exit {result.returncode}\n{result.stdout}{result.stderr}"
    )


def test_the_guard_fails_when_it_cannot_resolve_what_it_is_grading(tmp_path: Path) -> None:
    """
    A guard that passes when its inputs do not resolve is the defect this whole review is about.

    The first version of this verifier ignored git's exit status, so two unresolvable refs produced
    two empty strings, compared EQUAL, and it printed `OK` — certifying objects that do not exist.
    """
    result = run_guard(tmp_path)

    assert result.returncode != 0, f"the guard passed outside a repository:\n{result.stdout}{result.stderr}"
    assert "OK" not in result.stdout, result.stdout
