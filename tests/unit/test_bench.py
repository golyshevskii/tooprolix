"""
Guards for `corpus/bench.py`, the AC3 benchmark harness.

The harness this replaces lived as a heredoc in `corpus/REPORT.md` and ran every invocation with
`check=False`, discarding the exit code, stdout and stderr. Review executed it against an
**always-failing binary** and got a green **5.356 ms "benchmark"** — the release gate for
"< 5 s on ~100k lines" measuring nothing while looking green, which the epic's verification policy
calls RED, not green.

So the one thing worth testing here is that a run which did not measure anything **stops the
benchmark** instead of being timed. Three ways a run can fail to measure, one test each:

  1. the binary exits with a code that is not the expected one (a crash, or exit 2 meaning the tree
     could not be read);
  2. the binary produces no findings at all, so the median describes an empty output — the same
     "passed because the output was empty" class;
  3. the binary is not there.

Run: make test
"""

from __future__ import annotations

import stat
import subprocess
from pathlib import Path

import bench
import pytest

REPO_ROOT: Path = Path(__file__).resolve().parents[2]

#: The three things that run the shipped binary over the corpus. `run_all.sh` produces the
#: artifacts, `determinism_check.sh` re-runs them, `bench.py` times them — so a substituted binary
#: has to reach all three or the substitution proves nothing about the third.
CORPUS_RUNNERS: tuple[Path, ...] = (REPO_ROOT / "corpus/run_all.sh", REPO_ROOT / "corpus/determinism_check.sh")


def fake(tmp_path: Path, name: str, body: str) -> Path:
    """Write an executable shell stub and return its path."""
    script = tmp_path / name
    script.write_text(f"#!/bin/sh\n{body}\n", encoding="utf-8")
    script.chmod(script.stat().st_mode | stat.S_IEXEC)
    return script


class TestARunThatDidNotMeasureIsFatal:
    """A run that measured nothing must stop the benchmark, not be timed into the median."""

    def test_a_working_binary_is_timed(self, tmp_path: Path) -> None:
        binary = fake(tmp_path, "ok", "echo 'a.py:1: TPX002 …'\nexit 1")
        assert bench.time_run(binary, "root", expect_exit=1) >= 0.0

    def test_an_unexpected_exit_code_aborts(self, tmp_path: Path) -> None:
        binary = fake(tmp_path, "boom", "echo 'a.py:1: TPX002 …'\nexit 2")
        with pytest.raises(RuntimeError, match="exit 2"):
            bench.time_run(binary, "root", expect_exit=1)

    def test_an_always_failing_binary_aborts(self, tmp_path: Path) -> None:
        """The exact mutation review executed: a binary that only fails, timed at 5.356 ms."""
        binary = fake(tmp_path, "false", "exit 127")
        with pytest.raises(RuntimeError, match="exit 127"):
            bench.time_run(binary, "root", expect_exit=1)

    def test_empty_output_aborts(self, tmp_path: Path) -> None:
        binary = fake(tmp_path, "silent", "exit 1")
        with pytest.raises(RuntimeError, match="no output"):
            bench.time_run(binary, "root", expect_exit=1)

    def test_a_missing_binary_aborts(self, tmp_path: Path) -> None:
        with pytest.raises(RuntimeError):
            bench.time_run(tmp_path / "absent", "root", expect_exit=1)


class TestTheBenchmarkStopsWhenARunFails:
    """A raise that does not stop the benchmark is the same defect one frame up."""

    def test_main_returns_non_zero_and_prints_no_timing_row(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """
        The five tests above pin `time_run`; this one pins `main`.

        Review re-armed the original defect by changing `main`'s `return 1` to `continue`: every
        `time_run` test stayed green and the benchmark exited 0 on an always-failing binary. The
        header always prints, so the distinguishing evidence is the *absence of a data row* — with
        `continue` both assertions below flip.

        **The binary arrives through `TOOPROLIX_BIN`, and that is load-bearing.** It used to be
        passed as `main([str(binary)])`, and deleting the positional channel turned this test
        VACUOUS without turning it red: `main` began returning 2 before printing anything, so
        `code != 0` and `"ms" not in out` both held for a reason that has nothing to do with the
        defect. Caught by re-running the `continue` mutation and finding that **this** test stayed
        green while two others failed — the guard for a defect must be the thing that falls for it.
        """
        binary = fake(tmp_path, "false", "exit 127")
        monkeypatch.setenv("CORPUS_ROOT", str(tmp_path))
        monkeypatch.setenv("TOOPROLIX_BIN", str(binary))

        code = bench.main([])

        assert code != 0
        out = capsys.readouterr().out
        assert "ms" not in out, out


class TestTheBenchmarkVerifiesItsSubject:
    """AC3 is a release gate; it must not pass on a tree that is not the corpus."""

    def test_a_subject_whose_findings_differ_from_the_committed_run_aborts(self, tmp_path: Path) -> None:
        """Fable's exploit: a substituted CORPUS_ROOT of trivial files, every root exit 1."""
        runs = tmp_path / "runs"
        runs.mkdir()
        (runs / "requests.json").write_text(
            '{"schema_version":"1","findings":[{"code":"TPX002","path":"requests/a.py","line":1}]}'
        )
        binary = fake(tmp_path, "other", 'echo \'{"schema_version":"1","findings":[]}\'\nexit 1')

        with pytest.raises(RuntimeError, match="does not match"):
            bench.verify_subject(binary, "requests", "requests", runs, cwd=tmp_path)

    def test_a_subject_matching_the_committed_run_passes(self, tmp_path: Path) -> None:
        document = '{"schema_version":"1","findings":[{"code":"TPX002","path":"requests/a.py"}]}'
        runs = tmp_path / "runs"
        runs.mkdir()
        (runs / "requests.json").write_text(document)
        binary = fake(tmp_path, "same", f"echo '{document}'\nexit 1")

        bench.verify_subject(binary, "requests", "requests", runs, cwd=tmp_path)

    def test_a_missing_run_artifact_aborts(self, tmp_path: Path) -> None:
        """Timing against no recorded measurement is timing against nothing."""
        runs = tmp_path / "runs"
        runs.mkdir()
        binary = fake(tmp_path, "x", "echo '{}'\nexit 1")

        with pytest.raises(RuntimeError, match="run_all.sh"):
            bench.verify_subject(binary, "requests", "requests", runs, cwd=tmp_path)

    @pytest.mark.parametrize(
        ("recorded", "why"),
        [("{not json at all", "truncated-or-corrupt"), ('{"schema_version":"1"}', "valid-json-without-findings")],
        ids=["malformed", "no-findings-key"],
    )
    def test_an_unreadable_run_artifact_aborts_instead_of_tracebacking(
        self, tmp_path: Path, recorded: str, why: str
    ) -> None:
        """
        The `# Raises` block promises `RuntimeError` when the artifact is "missing or unreadable".
        Only the missing half was true.

        `is_file()` covers missing; the read itself —
        `json.loads(recorded.read_text(...))["findings"]` — sat outside every `try`, so a truncated
        or schema-shifted `corpus/runs/<name>.json` raised `JSONDecodeError`/`KeyError`, `main`
        catches only `RuntimeError`, and the harness tracebacked. A docstring promising behaviour
        the code does not have is this epic's recurring defect, and this one was written by the
        commit being reviewed — so the code moves to meet the docstring, not the other way round.

        Both shapes are real: a run killed midway leaves the first, and a schema bump leaves the
        second. `{why}` names which is which when one of them regresses alone.
        """
        runs = tmp_path / "runs"
        runs.mkdir()
        (runs / "requests.json").write_text(recorded, encoding="utf-8")
        binary = fake(tmp_path, "ok", "echo '{\"findings\":[]}'\nexit 1")

        with pytest.raises(RuntimeError, match="requests.json"):
            bench.verify_subject(binary, "requests", "requests", runs, cwd=tmp_path)


class TestAllThreeRunnersSelectTheBinaryTheSameWay:
    """
    One mechanism for "which binary is being measured", or a substitution proves nothing.

    `run_all.sh` and `determinism_check.sh` have read `${TOOPROLIX_BIN:-…}` since they were
    written. `bench.py` did not: it took the binary from `argv[0]` with a hard-coded fallback, so
    `TOOPROLIX_BIN=/somewhere/else corpus/bench.py` was a **no-op** — it went on timing
    `target/release/tooprolix` and reported a clean benchmark of a binary nobody had pointed it at.
    A mutation that a tool ignores is not a proof about that tool, and AC3 of this task is exactly
    the demand that the same mutation be visible to all three.
    """

    def test_tooprolix_bin_selects_the_binary_when_no_argument_is_given(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setenv("TOOPROLIX_BIN", str(tmp_path / "substituted"))

        assert bench.binary_from() == tmp_path / "substituted"

    def test_the_release_build_is_the_default_when_the_variable_is_unset(self, monkeypatch: pytest.MonkeyPatch) -> None:
        """The fallback is the shell scripts' fallback, so an unset variable means the same tree."""
        monkeypatch.delenv("TOOPROLIX_BIN", raising=False)

        assert bench.binary_from() == REPO_ROOT / "target/release/tooprolix"

    def test_a_relative_tooprolix_bin_is_resolved_against_the_callers_directory(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """
        The Python half of the guard-vs-run divergence the two shells had.

        `main` passes `cwd=CORPUS_ROOT` to `subprocess.run`, and a relative program path resolves
        against the CHILD's cwd — so an unresolved relative `TOOPROLIX_BIN` would mean
        `$CORPUS_ROOT/bin/tooprolix` here while the shells' `-x` guard judged
        `$PWD/bin/tooprolix`. Resolving once, up front, against the caller's directory is what puts
        all three runners on one file.
        """
        monkeypatch.chdir(tmp_path)
        monkeypatch.setenv("TOOPROLIX_BIN", "bin/tooprolix")

        assert bench.binary_from() == tmp_path / "bin/tooprolix"

    def test_there_is_no_second_channel_for_choosing_the_binary(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """
        A positional argument is REFUSED, so `TOOPROLIX_BIN` is the only way to choose the binary.

        This replaces `test_an_explicit_argument_outranks_the_variable`, which codified the
        divergence AC3 exists to close: `TOOPROLIX_BIN=/A corpus/bench.py /B` measured B while both
        shell runners measured A. The docstring defending it argued the shells "take no positional
        binary, so nothing disagrees" — but they cannot *express* the choice, and one runner having
        a channel the others lack is not agreement. Nothing in the repository passes a positional
        (`git grep bench.py` → `corpus/REPORT.md:550` and the module docstring, both without one),
        so the channel is deleted rather than re-ranked and AC3 holds by construction.

        Refused, not ignored: silently dropping `bench.py /B` would time the default while the
        operator believed they had selected B — the same "measured something else" this closes.
        It also removes the `argv=[""]` → `Path("")` → `Path(".")` footgun the old branch carried.


        `CORPUS_ROOT` is set deliberately: without it `main` returns 2 for a different reason
        entirely, and the exit code alone would agree with a build that ignored the positional
        silently.
        """
        monkeypatch.setenv("CORPUS_ROOT", str(tmp_path))
        monkeypatch.setenv("TOOPROLIX_BIN", str(tmp_path / "from-the-environment"))

        assert bench.main([str(tmp_path / "from-the-argument")]) == 2
        error = capsys.readouterr().err
        assert "takes no arguments" in error, "the refusal must say the positional is not accepted"
        assert "TOOPROLIX_BIN" in error, "the refusal must name the one channel that does work"

    def test_main_itself_measures_the_substituted_binary(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """
        The three tests above grade `binary_from`; this one grades the CALL SITE.

        Re-pointing `main` back at `Path(argv[0]) if argv else DEFAULT_BINARY` leaves every one of
        them green while `TOOPROLIX_BIN` goes back to being the no-op this task exists to fix — a
        guard connected to production by nothing, which is this epic's most-repeated defect. So the
        evidence here is the abort message naming the SUBSTITUTED path: before the fix the same run
        named `target/release/tooprolix` instead.

        No checkouts are involved. The stub is never asked to walk anything; it is asked for a JSON
        report and gives none, which is what `verify_subject` refuses.
        """
        stub = fake(tmp_path, "substituted", "exit 9")
        monkeypatch.setenv("CORPUS_ROOT", str(tmp_path))
        monkeypatch.setenv("TOOPROLIX_BIN", str(stub))

        assert bench.main([]) != 0
        assert str(stub) in capsys.readouterr().err

    def test_a_tooprolix_bin_that_does_not_exist_aborts_legibly(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """
        A typo in `TOOPROLIX_BIN` must produce the harness's own abort, not a traceback.

        `time_run` has caught `OSError` since it was written, and `test_a_missing_binary_aborts`
        proves it — but `verify_subject` was added in front of it later and did not, so `main`
        reached the missing binary through the checker rather than through the timer and died with
        a raw `FileNotFoundError`. The existing test could not see it: it calls `time_run`
        directly, which is no longer the first thing to touch the binary.

        Loud either way, so this is legibility rather than safety — but making the variable a
        first-class way to choose the binary is what turns "you typed the path wrong" into a
        routine input, and this module's whole argument is that an aborted run must say why.
        """
        monkeypatch.setenv("CORPUS_ROOT", str(tmp_path))
        monkeypatch.setenv("TOOPROLIX_BIN", str(tmp_path / "typo-in-the-path"))

        assert bench.main([]) != 0
        error = capsys.readouterr().err
        assert "BENCHMARK ABORTED" in error
        assert "typo-in-the-path" in error

    @pytest.mark.parametrize("script", CORPUS_RUNNERS, ids=lambda path: path.name)
    def test_the_shell_runners_read_the_same_variable_with_the_same_fallback(self, script: Path) -> None:
        """
        Graded on the script's own bytes: it pins the FALLBACK, which no execution test can reach
        without a built release binary at that exact path.

        It is deliberately no longer the only evidence — see
        `TestTheShellRunnersAreProvedByRunningThem`. A rewrite can keep this line and route the
        call sites through a different variable, and this assertion would not notice.
        """
        assert 'BINARY="${TOOPROLIX_BIN:-$REPO_ROOT/target/release/tooprolix}"' in script.read_text(encoding="utf-8")

    @pytest.mark.parametrize("script", CORPUS_RUNNERS, ids=lambda path: path.name)
    def test_the_shell_runners_still_parse(self, script: Path) -> None:
        """
        `bash -n` on the two scripts CI never runs.

        They need `corpus/checkouts/` (773 MB) to do anything, so they are manual by design and no
        job executes them — which means a syntax error introduced by an edit stays invisible until
        someone runs the corpus by hand, months later. Parsing is not correctness, but it is the
        difference between a broken script found now and one found at release time.
        """
        assert subprocess.run(["bash", "-n", str(script)], capture_output=True, check=False).returncode == 0


class TestTheShellRunnersAreProvedByRunningThem:
    """
    AC3 asks for a proof by RUNNING with a substituted binary, and a byte assertion is not one.

    `test_the_shell_runners_read_the_same_variable_with_the_same_fallback` grades a line's presence.
    Review's counter-example: keep that exact line, add `ACTUAL_BINARY=/tmp/B`, swap the call sites
    to `"$ACTUAL_BINARY"` — the byte assertion passes, `bash -n` passes, and `TOOPROLIX_BIN` is
    dead. So the variable is exercised here by executing each script.

    Cheap, and that was checked rather than hoped: in BOTH scripts the `-x` guard is reached before
    any checkout work, any `jq`/`git`/`rg` probe and any `mktemp`. No corpus is involved.
    """

    @staticmethod
    def run_script(script: Path, cwd: Path, **env: str) -> subprocess.CompletedProcess[str]:
        """Execute a runner with a minimal environment; `PWD` is left for bash to derive from cwd."""
        return subprocess.run(
            ["bash", str(script)],
            capture_output=True,
            text=True,
            cwd=cwd,
            env={"PATH": "/usr/bin:/bin", **env},
            check=False,
        )

    @pytest.mark.parametrize("script", CORPUS_RUNNERS, ids=lambda path: path.name)
    def test_a_relative_tooprolix_bin_is_resolved_before_it_is_judged(self, script: Path, tmp_path: Path) -> None:
        """
        The guard must judge the SAME file the run executes, and today it does not.

        Reproduced before fixing, with one stub at the caller's cwd and another at `$CORPUS_ROOT`,
        both named `bin/tooprolix`:

            GUARD PASSED for: bin/tooprolix     <- blessed the caller-cwd copy
            I AM B (corpus-root copy)           <- ran the other one

        `[[ -x "$BINARY" ]]` runs in the caller's cwd; the run happens inside
        `(cd "$CORPUS_ROOT" && "$BINARY" …)`, and a relative program path resolves against the
        child's cwd. So the guard blesses one binary and the measurement times another, silently,
        and the numbers land in `corpus/runs/` and `REPORT.md`.

        The discriminating assertion is that the refusal names an ABSOLUTE path: that can only be
        true if the resolution happened before the guard. Today the error echoes `bin/tooprolix`
        verbatim.
        """
        result = self.run_script(script, cwd=tmp_path, CORPUS_ROOT=str(tmp_path), TOOPROLIX_BIN="bin/tooprolix")

        assert result.returncode == 2, result.stderr
        assert str(tmp_path / "bin/tooprolix") in result.stderr, result.stderr

    @pytest.mark.parametrize("script", CORPUS_RUNNERS, ids=lambda path: path.name)
    def test_a_substituted_binary_that_does_not_exist_is_refused_by_name(self, script: Path, tmp_path: Path) -> None:
        """Executing the script proves the variable reaches the guard, not merely that a line exists."""
        absent = tmp_path / "no-such-tooprolix"

        result = self.run_script(script, cwd=tmp_path, CORPUS_ROOT=str(tmp_path), TOOPROLIX_BIN=str(absent))

        assert result.returncode == 2, result.stderr
        assert str(absent) in result.stderr, result.stderr

    def test_determinism_check_executes_the_substituted_binary_and_not_another_one(self, tmp_path: Path) -> None:
        """
        The variable reaches the RUN, not just the guard — which is what kills the `ACTUAL_BINARY`
        rewrite the byte assertion cannot see.

        `determinism_check.sh` is the only one of the two that can be proved this far without the
        corpus: its five-pass loop invokes the binary immediately after `mktemp`. `run_all.sh`
        cannot — it verifies every pin in `corpus.lock` against a real git checkout first and exits
        1 long before the binary is touched.

        **That gap is measured, not assumed, and it is an accepted residual.** Applying this
        exact rewrite to each script: on `determinism_check.sh` **this test fails**; on
        `run_all.sh` the suite comes back **229 passed** — nothing notices. So `run_all.sh`'s
        reachable proof stops at the guard, and closing it needs the 773 MB corpus, which is out of
        scope for the fast Python gates. Written down here because a residual nobody wrote down is
        indistinguishable from one nobody found.
        """
        marker = tmp_path / "the-substituted-binary-ran"
        stub = fake(tmp_path, "stub", f'touch "{marker}"\nexit 1')

        self.run_script(
            REPO_ROOT / "corpus/determinism_check.sh", cwd=tmp_path, CORPUS_ROOT=str(tmp_path), TOOPROLIX_BIN=str(stub)
        )

        assert marker.is_file(), "the script got past the guard without ever running TOOPROLIX_BIN"


class TestTheHarnessRefusesToRunWithoutASubject:
    """
    The one path through `main` that needs nothing on disk, and it is the one that decides whether
    a corpus run happens at all.

    `CORPUS_ROOT` must point outside any ancestor `.gitignore` — under this repository's parent,
    whose `.gitignore` line 17 is `lib/`, the walk sees 5 of crewAI's 1269 files and still exits 1
    with plausible output. So an unset variable is refused rather than defaulted to the working
    directory, and the refusal is exit 2 ("could not start"), not exit 1 ("measured, found
    something"). Nothing here needs the 773 MB of checkouts.
    """

    def test_an_unset_corpus_root_exits_two_and_times_nothing(
        self, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
    ) -> None:
        monkeypatch.delenv("CORPUS_ROOT", raising=False)

        assert bench.main([]) == 2
        captured = capsys.readouterr()
        assert "CORPUS_ROOT" in captured.err
        assert captured.out == "", "a timing table was printed for a run that never had a subject"
