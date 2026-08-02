"""
Guards for promises that live in `.github/workflows/` rather than in code.

A workflow file is not documentation: it decides which commit gets built, and a wrong key in it
fails silently — as a `cancel`, which reads as "not red" and blocks nothing. This file pins the
workflow settings whose breakage would otherwise be invisible until a release.

Run: make test
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import tempfile
import textwrap
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
from typing import Any

import pytest
from check_tag_jobs import read_manifest

REPO: Path = Path(__file__).parents[2]
WORKFLOWS: Path = REPO / ".github" / "workflows"
BUILD_ARTIFACTS: Path = WORKFLOWS / "build-artifacts.yml"
CI: Path = WORKFLOWS / "ci.yml"
RELEASE_PLZ: Path = WORKFLOWS / "release-plz.yml"
CONTRIBUTING: Path = REPO / "CONTRIBUTING.md"
GITATTRIBUTES: Path = REPO / ".gitattributes"
README: Path = REPO / "README.md"

CONCURRENCY_GROUP = re.compile(r"^concurrency:\n\s+group:\s*(?P<group>.+)$", re.MULTILINE)

#: The steps whose shell IS the guard. The tests below execute that shell rather than
#: re-implementing it, so a mutation to the workflow is what they grade.
STALE_TREE_GUARD_STEP = "Refuse to tag a tree that is not main's"
AGGREGATE_STEP = "Every required job must have concluded success"
COMMIT_ASSERTION_STEP = "The gates above ran on the commit this event names"
ASSEMBLE_RELEASE_STEP = "Assemble the immutable PyPI release candidate"
VERIFY_RELEASE_STEP = "Verify the approved PyPI release candidate"
PYPI_PUBLISH_ACTION = "pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33"

#: Every `make` target `ci.yml` ran before the eight jobs were consolidated into four. The
#: consolidation's one real risk is dropping a gate while the run still goes green, and a job count
#: cannot see that — only the target list can.
EXPECTED_MAKE_TARGETS: frozenset[str] = frozenset(
    {"lint.check", "type", "test", "rust.fmt.check", "rust.lint", "rust.build", "rust.test", "rust.doc", "cov"}
)

#: An ordinary (non-release) PR's manifest: a readable version that never moves.
VERSIONED_MANIFEST: str = '[package]\nname = "fixture"\nversion = "0.3.4"\n'

#: `(tag, the merge commit that landed it on main, whether invariant A holds)`. Invariant A: the
#: tag's tree equals that merge commit's tree. The FALSE rows are real releases that shipped a tree
#: `main` never had.
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

    The comments in these files quote the very keys the tests below forbid, so a substring search
    over the raw text would grade the prose describing a rule instead of the YAML obeying it.
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


def step_scripts(workflow: Path, step: str) -> list[str]:
    """
    Extract a step's `run: |` shell, so the tests EXECUTE the shipped artifact instead of matching it.

    A token assertion cannot tell `!= "success"` from `== "success"`, and that inversion turns the
    aggregate into "fail if any job passed" — measured, it left all 25 workflow tests green.
    """
    # `(?: +[^\n]*\n)*?` skips `id:`/`env:` between the name and the script; non-greedy and
    # anchored on the first `run: |`, so it cannot run past this step.
    found = [
        textwrap.dedent(body.group("script"))
        for body in re.finditer(
            rf"- name: {re.escape(step)}\n(?: +[^\n]*\n)*?(?P<pad> +)run: \|\n(?P<script>(?:(?P=pad) +.*\n|[ \t]*\n)+)",
            workflow.read_text(encoding="utf-8"),
        )
    ]
    assert found, f"{workflow.name} has no `{step}` step with a `run: |` script"
    return found


def step_script(workflow: Path, step: str) -> str:
    return step_scripts(workflow, step)[0]


def run_guard(cwd: Path) -> subprocess.CompletedProcess[str]:
    script = step_script(RELEASE_PLZ, STALE_TREE_GUARD_STEP)
    return subprocess.run(["bash", "-c", script], cwd=cwd, capture_output=True, text=True, check=False)


def run_aggregate(needs: dict[str, dict[str, Any]]) -> subprocess.CompletedProcess[str]:
    """Execute `ci-required`'s shipped shell against a `needs` context, exactly as Actions would."""
    return subprocess.run(
        ["bash", "-c", step_script(CI, AGGREGATE_STEP)],
        env={**os.environ, "RESULTS": json.dumps(needs)},
        capture_output=True,
        text=True,
        check=False,
    )


def run_commit_assertion(cwd: Path, event_sha: str) -> subprocess.CompletedProcess[str]:
    """Execute the shipped per-job commit assertion in a real checkout, as Actions would."""
    return subprocess.run(
        ["bash", "-c", step_script(CI, COMMIT_ASSERTION_STEP)],
        cwd=cwd,
        env={**os.environ, "GITHUB_SHA": event_sha},
        capture_output=True,
        text=True,
        check=False,
    )


def run_build_artifact_step(step: str, cwd: Path, environment: dict[str, str]) -> subprocess.CompletedProcess[str]:
    """Execute one shipped build-artifacts shell step against a local release fixture."""
    return subprocess.run(
        ["bash", "-c", step_script(BUILD_ARTIFACTS, step)],
        cwd=cwd,
        env={**os.environ, **environment},
        capture_output=True,
        text=True,
        check=False,
    )


def release_artifact_names(version: str) -> tuple[str, ...]:
    """Return the four files the supported tag matrix publishes."""
    return (
        f"tooprolix-{version}.tar.gz",
        f"tooprolix-{version}-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        f"tooprolix-{version}-py3-none-macosx_11_0_arm64.whl",
        f"tooprolix-{version}-py3-none-win_amd64.whl",
    )


def release_fixture(tmp_path: Path) -> tuple[Path, dict[str, str]]:
    """Create the exact four downloaded build artifacts and the tag checkout that owns them."""
    root = tmp_path / "release"
    root.mkdir()
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.email", "fixture@example.com"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.name", "fixture"], cwd=root, check=True)
    (root / "Cargo.toml").write_text('[package]\nname = "tooprolix"\nversion = "0.5.1"\n', encoding="utf-8")
    subprocess.run(["git", "add", "Cargo.toml"], cwd=root, check=True)
    subprocess.run(["git", "commit", "-qm", "release: v0.5.1"], cwd=root, check=True)
    sha = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=root, check=True, capture_output=True, text=True
    ).stdout.strip()

    artifact_names = release_artifact_names("0.5.1")
    artifact_ids = ("sdist", "linux-x86_64", "macos-arm64", "windows-x86_64")
    for artifact_id, filename in zip(artifact_ids, artifact_names, strict=True):
        directory = root / "downloaded" / f"artifacts-{artifact_id}"
        directory.mkdir(parents=True)
        (directory / filename).write_bytes(f"artifact:{filename}\n".encode())

    return root, {
        "GITHUB_REF": "refs/tags/v0.5.1",
        "GITHUB_REF_NAME": "v0.5.1",
        "GITHUB_RUN_ID": "424242",
        "GITHUB_SHA": sha,
    }


def succeeded(*jobs: str) -> dict[str, dict[str, Any]]:
    return {job: {"result": "success"} for job in jobs}


def evaluate(expression: str, event_name: str, ref: str, head_ref: str) -> bool:
    """
    Evaluate a SHIPPED GitHub `if:` expression against a context.

    Asserting the string merely contains `refs/tags/v` cannot distinguish `||` from `&&`, and
    flipping the first `||` — which stops the tag matrix running — left every wheel assertion green.
    The accepted subset is whitelisted below, so an expression growing a construct this cannot
    translate fails loudly instead of being mistranslated into a passing answer.
    """
    # `if: >-` is a folded scalar; YAML folds its newlines to single spaces, and so does this.
    expression = " ".join(expression.split())

    # `''` in a single-quoted GitHub literal is ONE escaped apostrophe; the regex below reads it as
    # two adjacent strings. Refused rather than mistranslated — no shipped expression uses one.
    assert "''" not in expression, f"escaped apostrophes are not translated: {expression!r}"

    context = {"github.event_name": event_name, "github.ref": ref, "github.head_ref": head_ref}
    skeleton = re.sub(r"'[^']*'", "''", expression)
    used = set(re.findall(r"[A-Za-z_][\w.]*", skeleton))
    assert used <= {"startsWith", *context}, (
        f"expression uses {used - {'startsWith', *context}}, which is not translated"
    )

    # GITHUB COMPARES STRINGS CASE-INSENSITIVELY — `==`, `!=`, `startsWith`, `endsWith`, `contains`.
    # Python does not, so translating to Python's operators was PERMISSIVELY wrong:
    # `|| startsWith(github.ref, 'REFS/HEADS/MAIN')` runs the wheel matrix on every push to `main`
    # in real Actions and every fixture below passed. A translator that errs towards "the gate is
    # fine" makes a green test certify a broken gate. Case-folding both sides restores GitHub's
    # semantics for this subset, whose only operations are equality and prefix tests. `||`/`&&`
    # become `or`/`and`, matching GitHub's precedence and short-circuit order.
    translated = re.sub(r"'([^']*)'", lambda literal: repr(literal.group(1).casefold()), expression)
    translated = translated.replace("||", " or ").replace("&&", " and ")
    for name, value in context.items():
        translated = translated.replace(name, repr(value.casefold()))
    return bool(eval(translated, {"__builtins__": {}}, {"startsWith": lambda text, prefix: text.startswith(prefix)}))


@contextmanager
def checkout_of(commit: str) -> Iterator[Path]:
    """
    Yield a worktree whose HEAD is `commit`, so the guard reads a real repository state.

    `--no-checkout`: the guard only asks git plumbing questions, so materialising files would cost
    seconds per historical release for nothing.
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

    Keyed on `github.ref` with `cancel-in-progress`, a later push to the same ref kills the earlier
    run on the assumption that later means newer. On PR #37 all four artifact checks came back
    `cancel` and the only surviving build was of the base tip, a commit that PR does not propose.
    Keyed on `github.sha`, a build can only be cancelled by another run of the same commit, and
    duplicate events for one SHA still collapse.
    """
    matched = CONCURRENCY_GROUP.search(BUILD_ARTIFACTS.read_text(encoding="utf-8"))
    assert matched is not None, "build-artifacts.yml declares no concurrency group"
    group: str = matched.group("group")

    assert "github.sha" in group, f"artifact builds must be grouped by commit, group is {group!r}"
    assert "github.ref" not in group, f"grouping by ref cancels other commits' builds, group is {group!r}"


def test_artifacts_are_built_for_main_v_tags_and_pull_requests_and_nothing_else() -> None:
    """
    A bare `push:` builds every ref: the `v0.4.6` cycle ran six full artifact matrices of which at
    most two were load-bearing. The duplicates come from the BRANCH axis — GitHub delivers a
    branch-CREATION push for every release-plz branch. The path axis must stay unfiltered.
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
    The three wheel legs cost 29 of the 32 billed minutes an ordinary PR spends on artifacts, so
    they run only on the events that precede a tag. The sdist is ungated: it grades the README
    transform and the archive contents, which any code change can break.
    """
    build = jobs(uncommented(BUILD_ARTIFACTS))
    assert job_condition(build["sdist"]) is None, "the sdist must run on every event this workflow accepts"

    gate = job_condition(build["wheels"])
    assert gate is not None, "the wheel matrix is ungated, so every ordinary PR pays for three wheel legs"


def test_every_wheel_waits_for_the_sdist_freshness_gate() -> None:
    """No wheel may upload if the independently generated third-party bundle is stale."""
    wheel = jobs(uncommented(BUILD_ARTIFACTS))["wheels"]

    assert re.search(r"^ {4}needs:\s+sdist\s*$", wheel, re.MULTILINE), (
        "the wheel matrix can build and upload even when the sdist freshness gate failed"
    )


def test_required_licence_files_are_pinned_to_lf_in_every_checkout() -> None:
    """Archive byte checks need canonical Git blobs on Windows as well as Unix runners."""
    expected = ("LICENSE text eol=lf", "THIRD-PARTY-LICENSES.html text eol=lf")
    rules = tuple(line for line in GITATTRIBUTES.read_text(encoding="utf-8").splitlines() if line)

    assert rules == expected


def test_the_expression_translator_refuses_an_escaped_apostrophe() -> None:
    """
    `'it''s'` is ONE GitHub literal containing an apostrophe; the literal regex reads it as two. A
    silent mistranslation is the failure mode this function exists to avoid, so it stops the test.
    """
    with pytest.raises(AssertionError, match="escaped apostrophes"):
        evaluate("startsWith(github.ref, 'it''s')", "push", "refs/tags/v0.4.8", "")


def test_the_expression_translator_folds_case_on_the_context_too() -> None:
    """
    Case-insensitivity has two sides: with the context left unfolded, every literal-side fixture
    below still passed. An upper-cased ref must match a lower-case prefix, and still fail another.
    """
    assert evaluate("startsWith(github.ref, 'refs/tags/v')", "PUSH", "REFS/TAGS/V0.4.8", "") is True
    assert evaluate("github.event_name == 'push'", "PUSH", "REFS/TAGS/V0.4.8", "") is True
    assert evaluate("startsWith(github.ref, 'refs/heads/')", "PUSH", "REFS/TAGS/V0.4.8", "") is False


@pytest.mark.parametrize(
    ("expression", "holds"),
    [
        # Case-insensitively: `|| startsWith(github.ref, 'REFS/HEADS/MAIN')` runs the wheel matrix
        # on every push to main in real Actions, and every fixture below still passed.
        ("startsWith(github.ref, 'refs/tags/v')", True),
        ("startsWith(github.ref, 'REFS/TAGS/V')", True),
        ("github.event_name == 'push'", True),
        ("github.event_name == 'PUSH'", True),
        ("startsWith(github.head_ref, 'release-plz-')", False),
        # `&&` binds tighter than `||`, as `and` does over `or`. Under the wrong grouping —
        # `A && (B || C)` — this would be false rather than true.
        (
            "github.event_name == 'pull_request' && startsWith(github.ref, 'refs/') || startsWith(github.ref, 'refs/tags/v')",
            True,
        ),
    ],
)
def test_the_expression_translator_reproduces_github_semantics(expression: str, holds: bool) -> None:
    """
    The translator is itself a guard, so its semantics are pinned rather than assumed: the gate
    below is only as trustworthy as this function.
    """
    assert evaluate(expression, event_name="push", ref="refs/tags/v0.4.8", head_ref="") is holds


@pytest.mark.parametrize(
    ("event", "event_name", "ref", "head_ref", "builds_wheels"),
    [
        ("an ordinary pull request", "pull_request", "refs/pull/52/merge", "feat/something", False),
        ("a release-plz pull request", "pull_request", "refs/pull/53/merge", "release-plz-2026-08-01T09-00-00Z", True),
        ("a v* tag push", "push", "refs/tags/v0.4.8", "", True),
        ("a push to main", "push", "refs/heads/main", "", False),
        ("a manual dispatch", "workflow_dispatch", "refs/heads/main", "", True),
    ],
)
def test_the_shipped_wheel_gate_selects_the_release_events(
    event: str, event_name: str, ref: str, head_ref: str, builds_wheels: bool
) -> None:
    """
    The gate's SEMANTICS, by evaluating the shipped expression rather than reading tokens out of it.
    No substring assertion can check that `startsWith` on an empty `head_ref` keeps the release-plz
    clause out of push events, nor tell `||` from `&&`.
    """
    gate = job_condition(jobs(uncommented(BUILD_ARTIFACTS))["wheels"])
    assert gate is not None
    expression = gate.removeprefix(">-").strip()

    assert evaluate(expression, event_name, ref, head_ref) is builds_wheels, f"{event} selected the wrong matrix"


def test_manifest_and_publish_are_tag_only_and_wait_for_every_build() -> None:
    """Ordinary PR/main runs stay reversible; a tag waits for the full authoritative matrix."""
    build = jobs(uncommented(BUILD_ARTIFACTS))
    manifest = build["release-manifest"]
    publish = build["publish-pypi"]

    assert re.search(r"^ {4}needs: \[sdist, wheels\]$", manifest, re.MULTILINE)
    assert re.search(r"^ {4}needs: \[sdist, wheels, release-manifest\]$", publish, re.MULTILINE)
    for job in (manifest, publish):
        condition = job_condition(job)
        assert condition is not None
        expression = condition.removeprefix(">").removeprefix("-").strip()
        assert evaluate(expression, "push", "refs/tags/v0.5.1", "") is True
        assert evaluate(expression, "push", "refs/heads/main", "") is False
        assert evaluate(expression, "pull_request", "refs/pull/56/merge", "release-plz-0.5.1") is False


def test_only_the_environment_gated_publish_job_can_request_an_oidc_token() -> None:
    """The reviewable manifest exists before the only job able to identify itself to PyPI."""
    build = jobs(uncommented(BUILD_ARTIFACTS))
    manifest = build["release-manifest"]
    publish = build["publish-pypi"]

    assert "id-token" not in manifest
    assert re.search(r"^ {4}environment:\s*$\n {6}name: pypi$", publish, re.MULTILINE)
    assert re.search(r"^ {4}permissions:\s*$\n {6}contents: read$\n {6}id-token: write$", publish, re.MULTILINE)
    assert uncommented(BUILD_ARTIFACTS).count("id-token: write") == 1
    assert "secrets." not in publish
    assert "name: release-manifest-${{ github.run_id }}" in manifest
    assert "path: release-manifest.txt" in manifest
    assert "name: release-candidate-${{ github.run_id }}" in manifest


def test_build_artifacts_uploads_only_the_publishable_source_from_each_builder() -> None:
    build = jobs(uncommented(BUILD_ARTIFACTS))

    assert re.search(r"name: artifacts-sdist\n\s+path: dist/\*\.tar\.gz", build["sdist"])
    assert "path: dist/*.whl" in build["wheels"]


def test_every_build_artifact_action_is_pinned_to_a_full_commit() -> None:
    used = re.findall(r"^\s+uses:\s+(\S+)", uncommented(BUILD_ARTIFACTS), re.MULTILINE)

    assert used
    assert [reference for reference in used if not re.search(r"@[0-9a-f]{40}$", reference)] == []
    assert {reference for reference in used if reference.startswith("actions/download-artifact@")} == {
        "actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c"
    }
    assert {reference for reference in used if reference.startswith("actions/upload-artifact@")} == {
        "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"
    }


def test_publish_uses_the_full_sha_pinned_official_action_on_only_the_verified_directory() -> None:
    publish = jobs(uncommented(BUILD_ARTIFACTS))["publish-pypi"]

    assert f"uses: {PYPI_PUBLISH_ACTION}" in publish
    assert "packages-dir: dist" in publish
    assert "password:" not in publish
    assert "repository-url:" not in publish


def test_the_shipped_manifest_assembles_exactly_the_supported_artifacts(tmp_path: Path) -> None:
    root, environment = release_fixture(tmp_path)

    result = run_build_artifact_step(ASSEMBLE_RELEASE_STEP, root, environment)

    assert result.returncode == 0, f"{result.stdout}{result.stderr}"
    assert {path.name for path in (root / "dist").iterdir()} == set(release_artifact_names("0.5.1"))
    manifest = (root / "release-manifest.txt").read_text(encoding="utf-8")
    assert f"tag-target-sha: {environment['GITHUB_SHA']}" in manifest
    assert "tree-sha: " in manifest
    assert "run-id: 424242" in manifest
    for filename in release_artifact_names("0.5.1"):
        assert f"\t{filename}\t" in manifest


@pytest.mark.parametrize(
    "mutation", ["unknown-artifact", "extra-file", "missing-file", "wrong-name", "wrong-sha", "wrong-tag"]
)
def test_the_shipped_manifest_fails_closed_on_an_inconsistent_release(tmp_path: Path, mutation: str) -> None:
    root, environment = release_fixture(tmp_path)
    sdist = root / "downloaded" / "artifacts-sdist" / release_artifact_names("0.5.1")[0]
    if mutation == "unknown-artifact":
        unknown = root / "downloaded" / "artifacts-other"
        unknown.mkdir()
        (unknown / "foreign.whl").write_bytes(b"foreign")
    elif mutation == "extra-file":
        (sdist.parent / "duplicate.tar.gz").write_bytes(sdist.read_bytes())
    elif mutation == "missing-file":
        sdist.unlink()
    elif mutation == "wrong-name":
        sdist.rename(sdist.with_name("tooprolix-9.9.9.tar.gz"))
    elif mutation == "wrong-sha":
        environment["GITHUB_SHA"] = "0" * 40
    else:
        environment["GITHUB_REF_NAME"] = "v9.9.9"
        environment["GITHUB_REF"] = "refs/tags/v9.9.9"

    result = run_build_artifact_step(ASSEMBLE_RELEASE_STEP, root, environment)

    assert result.returncode != 0, f"{mutation} passed:\n{result.stdout}{result.stderr}"


@pytest.mark.parametrize("mutation", ["changed-bytes", "extra-file", "missing-file", "manifest-hash", "wrong-sha"])
def test_the_shipped_publish_verification_refuses_any_change_after_the_manifest(tmp_path: Path, mutation: str) -> None:
    root, environment = release_fixture(tmp_path)
    assembled = run_build_artifact_step(ASSEMBLE_RELEASE_STEP, root, environment)
    assert assembled.returncode == 0, f"{assembled.stdout}{assembled.stderr}"
    candidate = root / "dist" / release_artifact_names("0.5.1")[0]
    if mutation == "changed-bytes":
        candidate.write_bytes(candidate.read_bytes() + b"changed")
    elif mutation == "extra-file":
        (root / "dist" / "foreign.whl").write_bytes(b"foreign")
    elif mutation == "missing-file":
        candidate.unlink()
    elif mutation == "manifest-hash":
        manifest = root / "release-manifest.txt"
        manifest.write_text(
            re.sub(r"\t[0-9a-f]{64}$", f"\t{'0' * 64}", manifest.read_text(), count=1, flags=re.MULTILINE)
        )
    else:
        environment["GITHUB_SHA"] = "0" * 40

    result = run_build_artifact_step(VERIFY_RELEASE_STEP, root, environment)

    assert result.returncode != 0, f"{mutation} passed:\n{result.stdout}{result.stderr}"


def test_the_shipped_publish_verification_accepts_the_unchanged_candidate(tmp_path: Path) -> None:
    root, environment = release_fixture(tmp_path)
    assembled = run_build_artifact_step(ASSEMBLE_RELEASE_STEP, root, environment)
    assert assembled.returncode == 0, f"{assembled.stdout}{assembled.stderr}"

    verified = run_build_artifact_step(VERIFY_RELEASE_STEP, root, environment)

    assert verified.returncode == 0, f"{verified.stdout}{verified.stderr}"


def test_the_release_day_readme_no_longer_claims_the_project_is_unpublished() -> None:
    readme = README.read_text(encoding="utf-8")

    assert "img.shields.io/pypi/v/tooprolix" in readme
    assert "img.shields.io/pypi/pyversions/tooprolix" in readme
    assert "status-pre--release" not in readme
    assert "Publication is still gated by labelled-corpus validation" not in readme


# ---------------------------------------------------------------------------------------------
# ci.yml — what runs, on which events, with which hard-won settings intact
# ---------------------------------------------------------------------------------------------


def test_ordinary_ci_runs_on_the_exact_v_tag() -> None:
    """
    Publication uploads from a `v*` tag; before this, five tags produced five runs and every one was
    `Build artifacts`. The release PR's CI does not stand in for it — `pull_request` checks out
    `refs/pull/N/merge`, tree `3a1f67fd` for `v0.4.6`, while the tag's was `5d0ceefb`.
    """
    triggers = top_level_block(uncommented(CI), "on")
    push = top_level_block(triggers.replace("  push:", "push:"), "push")

    assert re.search(r"tags:\s*\[\s*'v\*'\s*\]", push), f"ci.yml does not run on v* tags: {push!r}"
    assert re.search(r"branches:\s*\[\s*main\s*\]", push), (
        "post-merge CI on main must grade the exact commit that actually landed"
    )


def test_ci_reports_the_four_work_jobs_and_one_aggregate() -> None:
    """
    A required check name that stops reporting stays required forever, so the names are pinned here.
    Protection requires `ci-required`, so changing that name needs an atomic rules migration.
    """
    assert set(jobs(uncommented(CI))) == {"ci-python", "ci-rust", "cargo-doc", "coverage", "ci-required"}


def test_the_aggregate_is_unconditional_and_covers_every_required_job() -> None:
    """
    Without `if: always()` the aggregate is itself skipped the moment a needed job does not succeed,
    and protection accepts a skipped required job. `always()` makes the aggregate turn that result
    into a real failure. `coverage` is outside `needs:` deliberately: it protects the measuring
    instrument, not the shipped artifact.
    """
    aggregate = jobs(uncommented(CI))["ci-required"]

    assert job_condition(aggregate) == "always()", f"the aggregate's condition is {job_condition(aggregate)!r}"
    needs = re.search(r"^ {4}needs:\s*\[(?P<list>[^\]]*)\]", aggregate, re.MULTILINE)
    assert needs is not None, "the aggregate needs nothing, so it is green whatever CI did"
    assert {name.strip() for name in needs.group("list").split(",")} == {"ci-python", "ci-rust", "cargo-doc"}


def test_the_shipped_aggregate_passes_only_a_wholly_successful_run() -> None:
    """The baseline the refusals below break; without it a gate that always fails looks correct."""
    result = run_aggregate(succeeded("ci-python", "ci-rust", "cargo-doc"))
    assert result.returncode == 0, f"{result.stdout}{result.stderr}"


@pytest.mark.parametrize("result", ["failure", "cancelled", "skipped", "neutral"])
def test_the_shipped_aggregate_refuses_any_result_that_is_not_success(result: str) -> None:
    """
    Only `success` passes. `cancelled` and `skipped` are the ones that matter: reading them as "not
    failed" is how PR #37's four cancelled artifact checks looked green. Executed, not matched — no
    token assertion can tell `!= "success"` from `== "success"`.
    """
    needs = succeeded("ci-python", "ci-rust", "cargo-doc")
    needs["ci-rust"]["result"] = result

    assert run_aggregate(needs).returncode == 1


def test_the_shipped_aggregate_refuses_an_empty_needs_list() -> None:
    """
    Deleting `needs:` would leave a job reporting success unconditionally, and a required check that
    grades nothing is worse than none, because it is counted.
    """
    assert run_aggregate({}).returncode == 1


@pytest.mark.parametrize("workflow", [CI, BUILD_ARTIFACTS])
def test_every_job_that_checks_out_proves_which_commit_it_graded(workflow: Path) -> None:
    """
    The binding must reach the ARTIFACT workflow, where the publishable binaries are built: point
    its checkout at `ref: main` and every leg builds the wrong tree while every REST job still
    self-reports `head_sha=<tag>`. An aggregate cannot cover it — `needs:` does not cross workflows
    and job outputs are not returned by the jobs REST endpoint.

    LAST STEP, not first: recorded right after the checkout, a second checkout inserted afterwards
    would change what the gates graded while the record stayed correct.
    """
    for name, job in jobs(uncommented(workflow)).items():
        if "actions/checkout@" not in job:
            continue
        # EVERY step, named or not. Matching `- name:` lines made "last" mean "last NAMED", and a
        # bare `- run: git checkout main` after the assertion is the exact window it exists to
        # close — measured: with one appended, this test still passed.
        steps = re.findall(r"^ {6}- (\S.*)$", job, re.MULTILINE)
        assert steps[-1] == f"name: {COMMIT_ASSERTION_STEP}", (
            f"{name}'s last step is {steps[-1]!r}, so something can run after its checkout is graded"
        )


def test_the_commit_assertion_is_one_script_and_not_seven_drifting_copies() -> None:
    """
    Seven jobs carry it, and the per-job test above checks placement, not content — so a fix applied
    to one copy and not the others would leave a hole nothing else sees.
    """
    copies = step_scripts(CI, COMMIT_ASSERTION_STEP) + step_scripts(BUILD_ARTIFACTS, COMMIT_ASSERTION_STEP)

    assert len(copies) == 7, f"expected the assertion in all seven checkout-carrying jobs, found {len(copies)}"
    # Stripped: the extractor swallows the blank line that separates a step from the next block, so
    # a copy at the end of a job differs from one in the middle by trailing whitespace alone.
    assert len({copy.strip() for copy in copies}) == 1, "the copies have drifted apart"


def test_the_shipped_commit_assertion_accepts_only_the_commit_the_event_names() -> None:
    """
    Executed in a real checkout against the shipped shell: a job that graded another commit must go
    red, and so must one whose event names nothing, rather than mismatching an empty string by luck.
    """
    with checkout_of("bb327cc") as worktree:
        graded = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=worktree, capture_output=True, text=True, check=True
        ).stdout.strip()

        assert run_commit_assertion(worktree, graded).returncode == 0
        assert run_commit_assertion(worktree, "f0" * 20).returncode == 1
        assert run_commit_assertion(worktree, "").returncode == 1


def test_only_the_aggregate_carries_a_job_condition() -> None:
    """
    The current aggregate rejects every skipped dependency, so no work job is conditional yet. The
    path classifier that would define legitimate skips is deferred. The aggregate is the only job
    allowed a condition, and `always()` can only ever add work.
    """
    conditional = {name: job_condition(job) for name, job in jobs(uncommented(CI)).items() if job_condition(job)}
    assert conditional == {"ci-required": "always()"}, conditional


def test_ci_explains_the_two_different_skipped_check_semantics() -> None:
    """Keep the routing warning precise: job skips pass protection; workflow skips never report."""
    text = " ".join(
        line.lstrip().removeprefix("#").strip()
        for line in CI.read_text(encoding="utf-8").splitlines()
        if line.lstrip().startswith("#")
    )

    assert "A job skipped by a job-level `if:` reports `skipped`, which satisfies protection." in text
    assert "A workflow skipped by a workflow-level filter never reports its checks" in text
    workflow_comments = " ".join(path.read_text(encoding="utf-8") for path in (CI, BUILD_ARTIFACTS)).casefold()
    assert "a skipped check does not satisfy a required" not in workflow_comments


@pytest.mark.parametrize("workflow", [CI, BUILD_ARTIFACTS])
def test_no_workflow_selects_jobs_by_changed_path(workflow: Path) -> None:
    """
    The old five-file allowlist on `build-artifacts.yml` skipped a README-only change — precisely
    the change that breaks the README transformer. A longer allowlist only moves the boundary: the
    next file nobody thought of has the same effect, and nothing tells you the list is short.
    """
    offending = re.findall(r"^\s*paths(?:-ignore)?:.*$", uncommented(workflow), re.MULTILINE)
    assert offending == [], f"{workflow.name} filters by path: {offending}"


def test_every_make_target_that_ran_before_the_consolidation_still_runs() -> None:
    """
    Consolidating eight jobs into four is a green-to-green refactor, the shape that can silently
    delete a gate: fewer red Xs looks like progress and the job count cannot see it.
    """
    ran = set().union(*(make_targets(job) for job in jobs(uncommented(CI)).values()))
    assert ran == EXPECTED_MAKE_TARGETS, (
        f"missing {EXPECTED_MAKE_TARGETS - ran}, unexpected {ran - EXPECTED_MAKE_TARGETS}"
    )


def test_contributing_names_the_job_that_actually_runs_each_documented_gate() -> None:
    """A contributor should be able to map a local failure to the check they see on the PR."""
    documented = re.findall(
        r'^make (?P<target>\S+).*-> CI job "(?P<job>[^"]+)"$', CONTRIBUTING.read_text(encoding="utf-8"), re.MULTILINE
    )
    assert documented, "CONTRIBUTING.md documents no local-gate to CI-job mappings"

    workflow_jobs = jobs(uncommented(CI))
    for target, documented_job in documented:
        actual_jobs = {name for name, body in workflow_jobs.items() if target in make_targets(body)}
        assert actual_jobs == {documented_job}, (
            f"make {target} is documented as {documented_job!r}, but runs in {sorted(actual_jobs)}"
        )


def test_every_job_that_runs_the_python_tests_checks_out_the_full_history() -> None:
    """
    `corpus/classification.py` resolves the commit each measurement names with `git cat-file`, and
    a shallow checkout fails that lookup on a commit that genuinely exists: 16 tests go red for an
    environment reason. NOT fixed by passing the check when the repository is shallow — that guard
    would be disabled by the checkout depth of whoever ran it.

    EVERY job running those tests needs it; `coverage` does too, and went red for the same reason
    after the first job was fixed alone. This asks which jobs run them rather than naming them.
    """
    for name, job in jobs(uncommented(CI)).items():
        if make_targets(job) & {"test", "cov"}:
            assert "fetch-depth: 0" in job, f"{name} runs the Python tests on a shallow checkout"


def test_the_job_that_runs_the_rust_tests_installs_uv() -> None:
    """
    Two Rust tests SHELL OUT to `uv`, unrelated to the deleted pyo3 boundary. Removing the uv setup
    was reasoned from the build ("nothing compiles against an interpreter") and went red on PR #34
    with `Os { code: 2, kind: NotFound }`. The rule is "does anything it EXECUTES shell out to uv?",
    so the requirement is derived from the Rust sources rather than from a job name.
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
    Four jobs' worth of steps were merged into two, and each of these settings is one line a merge
    can drop while the run stays green: an unpinned action is a supply-chain hole, a persisted
    credential outlives the checkout in `.git/config`, and a cache written from every branch lets a
    PR poison the entry `main` reads.
    """
    text = uncommented(CI)

    # NOT anchored with `$`: every shipped `uses:` line carries a trailing ` # v7.0.1`, which
    # `uncommented()` does not strip, so an anchored pattern matched zero of the 11 actions and
    # `unpinned` was unconditionally empty. Measured: all four checkout pins replaced with
    # `@v7 # v7.0.1` and this test still reported `1 passed`.
    used = re.findall(r"^\s+uses:\s+(\S+)", text, re.MULTILINE)
    assert len(used) >= 11, f"only {len(used)} `uses:` lines found — the matcher has stopped seeing them"
    unpinned = [ref for ref in used if not re.search(r"@[0-9a-f]{40}$", ref)]
    assert unpinned == [], f"actions not pinned to a commit SHA: {unpinned}"
    assert text.count("actions/checkout@") == text.count("persist-credentials: false"), (
        "every checkout must keep the token out of .git/config"
    )
    assert text.count("Swatinem/rust-cache@") == text.count("save-if: ${{ github.ref == 'refs/heads/main' }}"), (
        "only main may write the Rust cache, or a PR can poison the entry every other PR reads"
    )


def test_the_expected_tag_job_manifest_names_every_ci_job() -> None:
    """
    Graded by SET EQUALITY over `(workflow name, job name)` pairs, so a job added to `ci.yml` and
    not here turns every future release red at the tag. This moves that discovery to the PR.

    Read with the SHIPPED parser: a duplicate reader cannot see a `[workflow]` header going missing,
    which silently reassigns four artifact jobs to `CI` — measured, it left this test green.

    Only `ci.yml`'s names are checked from this direction; the wheel legs are matrix-expanded, so
    their reported names are not readable out of the YAML.
    """
    expected = read_manifest(REPO / ".github" / "expected-tag-jobs.txt")

    assert {workflow for workflow, _ in expected} == {"CI", "Build artifacts"}, (
        "a `v*` tag fires exactly these two workflows; a missing section reassigns its jobs to another"
    )
    in_ci = {job for workflow, job in expected if workflow == "CI"}
    assert set(jobs(uncommented(CI))) <= in_ci, f"missing from the manifest: {set(jobs(uncommented(CI))) - in_ci}"
    in_artifacts = {job for workflow, job in expected if workflow == "Build artifacts"}
    assert in_artifacts == {
        "sdist (+ the wheel built from it)",
        "wheel linux-x86_64",
        "wheel macos-arm64",
        "wheel windows-x86_64",
        "PyPI release manifest",
        "Publish to PyPI",
    }


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
    The staleness is created here, not at tag time: a `release-pr` run queued behind another checks
    out the SHA of the push that triggered it and forks from that older tip. For `v0.4.6` the branch
    was created at `0e1b710` while `main` was already `e9af7ed`. Configuration is not a guard, but
    it is what stops the guard in the `release` job from having to fire.
    """
    release_pr = jobs(uncommented(RELEASE_PLZ))["release-pr"]
    assert re.search(r"^\s+ref: main$", release_pr, re.MULTILINE), "the release PR may fork from a stale main tip"


@pytest.mark.parametrize(("tag", "merge", "holds"), RELEASE_MERGES)
def test_the_guard_reproduces_every_historical_release(tag: str, merge: str, holds: bool) -> None:
    """
    Graded against the six releases this repository has actually cut. Invariant A holds for
    `v0.4.2`, `v0.4.4` and `v0.4.7` and fails for `v0.4.3`, `v0.4.5` and `v0.4.6`, where the tag
    shipped a tree `main` never had — `v0.4.6` misses PR #48 entirely, six files and +88/-83. A
    guard that passes all six would prove nothing, which is why the FALSE rows are here.

    It is NOT an ancestry claim: with a merge-commit release the tag target is an ancestor of `main`
    on every release, good and bad alike.
    """
    with checkout_of(merge) as worktree:
        result = run_guard(worktree)

    assert (result.returncode == 0) is holds, (
        f"{tag} at {merge}: exit {result.returncode}\n{result.stdout}{result.stderr}"
    )


def a_merge_that_is_not_a_release(root: Path, *, manifest: str | None, release_manifest: str | None = None) -> Path:
    """
    Fork at A, `main` advances to B, land the PR as a merge commit.

    `HEAD^{tree}` then holds both changes and `HEAD^2^{tree}` lacks B, so the trees differ exactly
    as a stale release's do — while release-plz, with `release_always = false`, would tag nothing.
    """
    repo = root / "repo"
    repo.mkdir()

    def git(*args: str) -> str:
        done = subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True, text=True)
        return done.stdout.strip()

    git("init", "-q", "-b", "main")
    git("config", "user.email", "fixture@example.com")
    git("config", "user.name", "fixture")
    (repo / "base.txt").write_text("base\n", encoding="utf-8")
    if manifest is not None:
        # The guard identifies a release from the version in Cargo.toml, not from the commit
        # subject, so the fixture carries one and never bumps it — an ordinary PR's shape.
        (repo / "Cargo.toml").write_text(manifest, encoding="utf-8")
    git("add", "-A")
    git("commit", "-qm", "chore: base")
    git("checkout", "-qb", "feature")
    (repo / "feature.txt").write_text("feature\n", encoding="utf-8")
    if release_manifest is not None:
        # Makes the branch commit a genuine RELEASE commit: the package version really moves.
        (repo / "Cargo.toml").write_text(release_manifest, encoding="utf-8")
    git("add", "-A")
    git("commit", "-qm", "feat: the feature this PR proposes")
    git("checkout", "-q", "main")
    (repo / "landed-first.txt").write_text("someone else got there first\n", encoding="utf-8")
    git("add", "-A")
    git("commit", "-qm", "fix: an unrelated change that landed first")
    git("merge", "--no-ff", "-q", "-m", "Merge pull request #7 from golyshevskii/feature", "feature")

    assert git("rev-parse", "HEAD^{tree}") != git("rev-parse", "HEAD^2^{tree}"), (
        "the fixture's trees agree, so it cannot show the guard distinguishing a merge from a release"
    )
    return repo


def test_a_non_release_merge_does_not_redden_main(tmp_path: Path) -> None:
    """
    The guard must refuse RELEASES, not refuse MERGES: comparing unconditionally paints `main` red
    for every merge-commit PR, none of which release-plz would have tagged. The release is
    identified from the version in `Cargo.toml` — what release-plz acts on — rather than from a
    commit subject it owns and could reword.
    """
    result = run_guard(a_merge_that_is_not_a_release(tmp_path, manifest=VERSIONED_MANIFEST))

    assert result.returncode == 0, f"a non-release merge reddened main:\n{result.stdout}{result.stderr}"


@pytest.mark.parametrize(
    ("manifest", "shape"),
    [
        (None, "no Cargo.toml at all"),
        # A manifest that EXISTS and declares no readable `version` is the case `set -o pipefail`
        # does NOT catch: `git show` succeeds and `sed` simply prints nothing. Measured — with the
        # emptiness check deleted, the missing-file fixture alone still went red and proved nothing.
        ('[package]\nname = "fixture"\nversion.workspace = true\n', "a version this guard cannot read"),
    ],
)
def test_a_merge_whose_package_version_cannot_be_read_is_refused(
    tmp_path: Path, manifest: str | None, shape: str
) -> None:
    """
    The classification itself must fail CLOSED: "I could not tell whether this is a release" has to
    mean refuse, never "not a release, so nothing to check".
    """
    result = run_guard(a_merge_that_is_not_a_release(tmp_path, manifest=manifest))

    assert result.returncode != 0, f"{shape} passed:\n{result.stdout}{result.stderr}"
    assert "OK" not in result.stdout, result.stdout


def test_an_ambiguous_package_version_is_refused(tmp_path: Path) -> None:
    """
    The fail-open the lexical read had: a decoy `version = "…"` above `[package]`. The fixture is a
    GENUINE release — version 0.3.4 -> 0.4.0, tree stale — carrying a `[workspace.package]` table
    whose version never changes. Reading the first match graded 9.9.9 on both sides and skipped the
    tree comparison: measured, exit 0 with `OK: … leaves the package version at 9.9.9`.

    THE DIAGNOSIS IS ASSERTED, NOT ONLY THE EXIT CODE. This fixture's tree is also stale, so with
    the ambiguity refusal disabled the guard still reaches the TREE comparison and exits 1 anyway —
    measured, the exit-code-only version of this test reported `1 passed`. Only this guard can
    produce the counts in the message.
    """
    decoy = '[workspace.package]\nversion = "9.9.9"\n\n[package]\nname = "fixture"\nversion = "0.3.4"\n'
    released = decoy.replace('version = "0.3.4"', 'version = "0.4.0"')

    result = run_guard(a_merge_that_is_not_a_release(tmp_path, manifest=decoy, release_manifest=released))

    assert result.returncode != 0, f"an ambiguous manifest passed:\n{result.stdout}{result.stderr}"
    assert "OK" not in result.stdout, result.stdout
    assert "exactly one top-of-line" in result.stderr, f"refused for some other reason:\n{result.stderr}"
    assert "found 2 at" in result.stderr, f"the refusal did not report the ambiguity it found:\n{result.stderr}"


def test_the_guard_fails_when_it_cannot_resolve_what_it_is_grading(tmp_path: Path) -> None:
    """
    The first version of this verifier ignored git's exit status, so two unresolvable refs produced
    two empty strings, compared EQUAL, and it printed `OK` — certifying objects that do not exist.
    """
    result = run_guard(tmp_path)

    assert result.returncode != 0, f"the guard passed outside a repository:\n{result.stdout}{result.stderr}"
    assert "OK" not in result.stdout, result.stdout
