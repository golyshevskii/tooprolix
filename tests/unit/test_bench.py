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
        """
        binary = fake(tmp_path, "false", "exit 127")
        monkeypatch.setenv("CORPUS_ROOT", str(tmp_path))

        code = bench.main([str(binary)])

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

        assert bench.binary_from([]) == tmp_path / "substituted"

    def test_the_release_build_is_the_default_when_the_variable_is_unset(self, monkeypatch: pytest.MonkeyPatch) -> None:
        """The fallback is the shell scripts' fallback, so an unset variable means the same tree."""
        monkeypatch.delenv("TOOPROLIX_BIN", raising=False)

        assert bench.binary_from([]) == REPO_ROOT / "target/release/tooprolix"

    def test_an_explicit_argument_outranks_the_variable(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """
        Naming a binary on the command line is more specific than exporting one, so it wins.

        The shell scripts take no positional binary at all, so this precedence is `bench.py`'s
        alone and cannot disagree with them — but it is asserted rather than assumed, because
        silently preferring the environment would make `bench.py <path>` measure something else.
        """
        monkeypatch.setenv("TOOPROLIX_BIN", str(tmp_path / "from-the-environment"))

        assert bench.binary_from([str(tmp_path / "from-the-argument")]) == tmp_path / "from-the-argument"

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
        Graded on the script's own bytes, which is the only artifact there is: neither shell script
        is executed by any test or any CI job, so nothing else in this repository would notice one
        of them being rewritten to hard-code a path.
        """
        assert 'readonly BINARY="${TOOPROLIX_BIN:-$REPO_ROOT/target/release/tooprolix}"' in script.read_text(
            encoding="utf-8"
        )

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
