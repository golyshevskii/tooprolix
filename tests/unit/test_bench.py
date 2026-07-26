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
from pathlib import Path

import bench
import pytest


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
