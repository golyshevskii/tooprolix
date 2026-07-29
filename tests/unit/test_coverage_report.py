"""
Guards for `scripts/coverage_report.py`, which reads a coverage report and refuses to believe it if
it did not measure what it claims.

The percentage it prints is what tasks 9-11 audit against, so the three ways it can lie are the
three things tested here:

  1. **it can overstate.** The percentage is truncated toward zero at one decimal, never rounded to
     nearest — 99.96% reads `99.9%`, not `100.0%`. A figure that says 100 on a code base with an
     uncovered line is the worst outcome, so the rounding rule is pinned by cases that a
     round-half-up implementation gets wrong.
  2. **it can disagree with the tool that measured it.** The percentage is never typed by hand: it
     is read out of the machine-readable report the coverage tool itself wrote. The two extractors
     are tested against report fragments captured verbatim from `cargo llvm-cov --json` and
     `coverage json` on this repository, so a test cannot pass by agreeing with the script's own
     arithmetic.
  3. **it can be a true number about the wrong denominator.** This is the one that leaves no trace:
     drop `branch = true`, or let a source file fall out of discovery, and the percentage RISES
     while measuring less. `verify_report_measured_the_source_tree` is the guard, and
     `TestTheReportIsGradedNotJustRead` below is its test — the file set is computed by walking the
     filesystem, never typed out as a list of today's filenames, because a literal list is the same
     self-report defect one level up: it would agree with a report that missed a file sitting right
     beside the ones it names.

`TestTheGuardIsWiredIntoTheEntryPoint` is what stops all of the above from being unreachable prose:
it runs the script the way the Makefile runs it, so a guard deleted from `main` fails a test rather
than nothing at all.

Run: make test
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest
from coverage_report import (
    PYTHON_REPORT_FORMAT,
    RUST_REPORT_FORMAT,
    format_percent,
    measurable_source_files,
    percent_from_report,
    verify_report_measured_the_source_tree,
)

REPO_ROOT = Path(__file__).resolve().parents[2]


def python_report(files: set[str], *, branch: bool = True) -> dict[str, Any]:
    """Build a coverage.py report shell that claims to have measured exactly `files`."""
    return {
        "meta": {"version": "7.15.2", "branch_coverage": branch},
        "files": {name: {} for name in sorted(files)},
        "totals": {"percent_covered": 52.37203495630462},
    }


def rust_report(files: set[str], root: Path = REPO_ROOT) -> dict[str, Any]:
    """Build a cargo-llvm-cov report shell that claims to have measured exactly `files`."""
    return {
        "data": [
            {
                "files": [{"filename": str(root / name)} for name in sorted(files)],
                "totals": {"lines": {"percent": 98.07146908678389}},
            }
        ],
        "type": "llvm.coverage.json.export",
        "version": "3.1.0",
    }


def fake_crate(root: Path, sources: dict[str, str]) -> None:
    """Lay out a throwaway `src/` tree so the guard can be tested away from this repository."""
    for name, body in sources.items():
        path = root / "src" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")


class TestThePercentageIsNeverRoundedUp:
    """
    Truncation toward zero at one decimal. Every case below is one a round-half-up implementation
    would get wrong, so replacing the rounding rule turns this class red.
    """

    @pytest.mark.parametrize(
        "measured,printed",
        [
            # `cargo llvm-cov` on this repo, 2026-07-29 — 98.07% of lines. Half-up agrees here, so
            # this row alone would not catch a rounding change; the rows below are the guard.
            (98.07, "98.0%"),
            # coverage.py hands back a full-precision float; half-up would print `83.9%`.
            (83.87096774193549, "83.8%"),
            # The case that matters most: nothing but genuine full coverage may print `100.0%`.
            (99.96, "99.9%"),
            (100.0, "100.0%"),
            # A value whose float representation sits just BELOW the decimal it prints
            # (83.9 * 10 == 838.9999999999999 in IEEE 754), so an int(pct * 10) implementation
            # renders `83.8%` and loses a tenth the tool actually measured.
            (83.9, "83.9%"),
            (0.0, "0.0%"),
        ],
    )
    def test_the_printed_percentage(self, measured: float, printed: str) -> None:
        assert format_percent(measured) == printed


class TestThePercentageComesFromTheCoverageTool:
    """
    The extractors are fed report fragments captured from the real tools, so these tests fail if the
    generator starts reading the wrong field — the failure mode where the badge is a plausible
    number that measures something other than what it claims.
    """

    def test_it_reads_total_line_coverage_from_a_cargo_llvm_cov_report(self, tmp_path: Path) -> None:
        # Captured from `cargo llvm-cov --locked --features python --summary-only --json` via
        # `make rust.cov` on this repository, 2026-07-29 — the totals block verbatim, with the
        # per-file `files` list dropped. `lines.percent` is the figure the badge shows; every
        # sibling here is a plausible wrong answer that is NOT it — `regions` 97.95, `functions`
        # 96.33, `instantiations` 83.06 — and `branches` reads 0/0 because llvm-cov reports no
        # branch data on the pinned stable toolchain, so reading that field would render `0.0%`.
        report = tmp_path / "llvm-cov.json"
        report.write_text(
            json.dumps(
                {
                    "data": [
                        {
                            "totals": {
                                "branches": {"count": 0, "covered": 0, "notcovered": 0, "percent": 0},
                                "functions": {"count": 354, "covered": 341, "percent": 96.32768361581921},
                                "instantiations": {"count": 555, "covered": 461, "percent": 83.06306306306305},
                                "lines": {"count": 3526, "covered": 3458, "percent": 98.07146908678389},
                                "mcdc": {"count": 0, "covered": 0, "notcovered": 0, "percent": 0},
                                "regions": {
                                    "count": 5612,
                                    "covered": 5497,
                                    "notcovered": 115,
                                    "percent": 97.95081967213115,
                                },
                            }
                        }
                    ],
                    "type": "llvm.coverage.json.export",
                    "version": "3.1.0",
                }
            ),
            encoding="utf-8",
        )

        assert percent_from_report(report, RUST_REPORT_FORMAT) == pytest.approx(98.07146908678389)

    def test_it_reads_the_total_percentage_from_a_coverage_py_report(self, tmp_path: Path) -> None:
        # Captured from `coverage json` (coverage 7.15.2) via `make py.cov` on this repository,
        # 2026-07-29 — the totals block verbatim, with the per-file `files` map dropped.
        # `percent_covered` (52.37) already folds branches in, because `branch = true`. Every
        # neighbour in this block is a plausible wrong answer and none of them is 52.37:
        # `percent_covered_display` is a pre-rounded STRING that drops the decimal,
        # `percent_statements_covered` ignores branches, `percent_branches_covered` ignores lines.
        report = tmp_path / "coverage.json"
        report.write_text(
            json.dumps(
                {
                    "meta": {"version": "7.15.2", "branch_coverage": True},
                    "files": {},
                    "totals": {
                        "covered_lines": 615,
                        "num_statements": 1156,
                        "percent_covered": 52.37203495630462,
                        "percent_covered_display": "52",
                        "missing_lines": 541,
                        "excluded_lines": 0,
                        "percent_statements_covered": 53.200692041522494,
                        "percent_statements_covered_display": "53",
                        "num_branches": 446,
                        "num_partial_branches": 20,
                        "covered_branches": 224,
                        "missing_branches": 222,
                        "percent_branches_covered": 50.224215246636774,
                        "percent_branches_covered_display": "50",
                    },
                }
            ),
            encoding="utf-8",
        )

        assert percent_from_report(report, PYTHON_REPORT_FORMAT) == pytest.approx(52.37203495630462)

    def test_a_report_without_the_totals_it_needs_is_fatal(self, tmp_path: Path) -> None:
        # Fail closed. A tool that changed its schema must stop the badge, not render 0% — a badge
        # reading 0% is a claim about the code, and this would be a claim about the parser.
        report = tmp_path / "coverage.json"
        report.write_text(json.dumps({"totals": {}}), encoding="utf-8")

        with pytest.raises(KeyError):
            percent_from_report(report, PYTHON_REPORT_FORMAT)


class TestTheReportIsGradedNotJustRead:
    """
    The percentage is only meaningful if the report behind it measured the tree it claims to.
    Every case here is a report that is internally consistent and carries a perfectly plausible
    number — the kind that produces a fresh percentage, a passing `make cov`, and a lie.

    The expected file set is WALKED, never listed. A test that named `bench/measure/sample_clusters/
    units` would keep passing on the day someone adds `corpus/pkg/thing.py` that coverage.py never
    discovers, which is exactly the hole being closed.
    """

    def test_the_walked_set_is_the_corpus_sources_and_excludes_the_data_trees(self) -> None:
        walked = measurable_source_files(REPO_ROOT, PYTHON_REPORT_FORMAT)

        # Every entry is real, under corpus/, and outside the two data trees. Asserted as
        # properties rather than as names, so the assertion survives a new source file.
        assert walked, "walked no Python sources at all"
        for name in walked:
            assert (REPO_ROOT / name).is_file()
            assert name.startswith("corpus/")
            assert not name.startswith(("corpus/checkouts/", "corpus/fixtures/"))
        # corpus/fixtures/sample.py exists and is test DATA, so its absence is the exclusion working
        # rather than the walk finding nothing.
        assert (REPO_ROOT / "corpus/fixtures/sample.py").is_file()
        assert "corpus/fixtures/sample.py" not in walked

    def test_a_report_that_measured_the_whole_tree_is_accepted(self) -> None:
        walked = measurable_source_files(REPO_ROOT, PYTHON_REPORT_FORMAT)

        verify_report_measured_the_source_tree(python_report(walked), PYTHON_REPORT_FORMAT, REPO_ROOT)

    def test_a_source_file_missing_from_the_report_is_fatal(self) -> None:
        # The namespace-package hole: a production file coverage.py never discovered leaves the
        # denominator silently and the percentage goes UP.
        walked = measurable_source_files(REPO_ROOT, PYTHON_REPORT_FORMAT)
        dropped = min(walked)

        with pytest.raises(ValueError, match=r"never measured.*" + dropped):
            verify_report_measured_the_source_tree(python_report(walked - {dropped}), PYTHON_REPORT_FORMAT, REPO_ROOT)

    def test_a_report_measuring_the_tests_themselves_is_fatal(self) -> None:
        # AC1 in one assertion: put `tests/unit` in `[tool.coverage.run] source` and the badge
        # starts climbing whenever someone writes test code.
        walked = measurable_source_files(REPO_ROOT, PYTHON_REPORT_FORMAT)

        with pytest.raises(ValueError, match=r"measured files outside.*tests/unit/test_measure\.py"):
            verify_report_measured_the_source_tree(
                python_report(walked | {"tests/unit/test_measure.py"}), PYTHON_REPORT_FORMAT, REPO_ROOT
            )

    def test_branch_coverage_switched_off_is_fatal(self) -> None:
        # Deleting `branch = true` measures less and reports MORE: 52.37% -> 53.20% on this repo,
        # with nothing in the badge to show that the question changed.
        walked = measurable_source_files(REPO_ROOT, PYTHON_REPORT_FORMAT)

        with pytest.raises(ValueError, match="branch coverage"):
            verify_report_measured_the_source_tree(python_report(walked, branch=False), PYTHON_REPORT_FORMAT, REPO_ROOT)

    def test_a_rust_report_measuring_something_outside_src_is_fatal(self) -> None:
        # The half of the guard that holds for llvm-cov: nothing but the crate's own sources may sit
        # in the denominator. A dependency or a test binary leaking in would move the percentage
        # without moving the code.
        walked = measurable_source_files(REPO_ROOT, RUST_REPORT_FORMAT)

        assert walked, "walked no Rust sources at all"
        verify_report_measured_the_source_tree(rust_report(walked), RUST_REPORT_FORMAT, REPO_ROOT)

        with pytest.raises(ValueError, match=r"measured files outside.*tests/cli\.rs"):
            verify_report_measured_the_source_tree(
                rust_report(walked | {"tests/cli.rs"}), RUST_REPORT_FORMAT, REPO_ROOT
            )

    def test_a_rust_source_file_with_no_instrumentable_code_may_be_absent(self) -> None:
        """
        The other half deliberately does NOT hold for llvm-cov, and this test is the reason.

        `src/detect.rs` is 26 lines of module documentation and two `pub mod` declarations — no
        functions, nothing to instrument — so `cargo llvm-cov` emits no entry for it and is right
        not to. Requiring every `.rs` on disk to appear made `make rust.cov` fail on the real
        repository: `never measured 1 source file(s) that exist on disk: src/detect.rs`.

        coverage.py differs in kind: it walks `source` and reports files it never executed at 0%, so
        there the same requirement IS sound and stays enforced — see
        `test_a_source_file_missing_from_the_report_is_fatal`. The asymmetry is a property of the
        two tools' discovery models, not a relaxation of the guard.
        """
        detect = (REPO_ROOT / "src/detect.rs").read_text(encoding="utf-8")

        assert "pub mod " in detect
        assert "fn " not in detect

        walked = measurable_source_files(REPO_ROOT, RUST_REPORT_FORMAT)
        assert "src/detect.rs" in walked

        verify_report_measured_the_source_tree(rust_report(walked - {"src/detect.rs"}), RUST_REPORT_FORMAT, REPO_ROOT)


class TestAMissingRustFileIsOnlyExcusedWhenThereIsNothingToInstrument:
    """
    `cargo llvm-cov` reports what the compiler instrumented, so a `.rs` file with no functions is
    legitimately absent from the report. A file that DOES define functions is a different thing
    entirely: it is orphaned from the module tree, or gated behind a feature this run does not
    enable. Either way it left the denominator in silence while every gate stayed green — `cargo
    fmt`, `clippy` and `cargo test` all walk the module tree and never see it.

    Tested on a throwaway crate rather than on this repository, so the case survives `src/detect.rs`
    one day gaining a function.
    """

    def test_a_file_with_no_functions_may_be_absent(self, tmp_path: Path) -> None:
        fake_crate(tmp_path, {"lib.rs": "pub mod wiring;\n", "wiring.rs": "pub mod a;\npub mod b;\n"})
        walked = measurable_source_files(tmp_path, RUST_REPORT_FORMAT)

        assert walked == {"src/lib.rs", "src/wiring.rs"}

        verify_report_measured_the_source_tree(rust_report({"src/lib.rs"}, root=tmp_path), RUST_REPORT_FORMAT, tmp_path)

    def test_a_file_that_defines_functions_may_not_be_absent(self, tmp_path: Path) -> None:
        fake_crate(tmp_path, {"lib.rs": "pub mod a;\n", "orphan.rs": "pub fn one() {}\npub fn two() {}\n"})

        with pytest.raises(ValueError, match=r"never measured.*src/orphan\.rs"):
            verify_report_measured_the_source_tree(
                rust_report({"src/lib.rs"}, root=tmp_path), RUST_REPORT_FORMAT, tmp_path
            )


class TestTheGuardIsWiredIntoTheEntryPoint:
    """
    Everything above tests the guard as a function. This tests that the production path still CALLS
    it, which nothing else does — the Makefile invokes the script, and a script whose `main` no
    longer verifies would leave every one of those tests passing and `make cov` green while printing
    a percentage from a report that measured the wrong thing.

    These two run the script exactly as the Makefile does, and they pin the ORDERING as well as the
    outcome. **The printed percentage is the artifact**, so "nothing was emitted for a report we
    refused" is the guarantee that used to read "the badge file was not written": a number on stdout
    is a number somebody quotes into an audit, and a non-zero exit afterwards does not un-print it.
    The second assertion of the rejection test exists for exactly that, and for nothing else.
    """

    def run_script(self, report: dict[str, Any], tmp_path: Path) -> subprocess.CompletedProcess[str]:
        report_path = tmp_path / "report.json"
        report_path.write_text(json.dumps(report), encoding="utf-8")
        return subprocess.run(
            [
                sys.executable,
                str(REPO_ROOT / "scripts" / "coverage_report.py"),
                "--report",
                str(report_path),
                "--format",
                PYTHON_REPORT_FORMAT,
            ],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
            check=False,
        )

    def test_an_honest_report_is_accepted_and_the_percentage_is_printed(self, tmp_path: Path) -> None:
        walked = measurable_source_files(REPO_ROOT, PYTHON_REPORT_FORMAT)

        result = self.run_script(python_report(walked), tmp_path)

        assert result.returncode == 0, result.stderr
        assert result.stdout.strip() == "python coverage: 52.3%"

    def test_a_rejected_report_exits_non_zero_and_prints_nothing(self, tmp_path: Path) -> None:
        walked = measurable_source_files(REPO_ROOT, PYTHON_REPORT_FORMAT)

        result = self.run_script(python_report(walked, branch=False), tmp_path)

        assert result.returncode != 0
        assert "branch coverage" in result.stderr
        assert result.stdout == "", "a percentage was printed from a report the guard rejected"


class TestTheTwoStatementsOfTheExclusionContractHaveTheSameShape:
    """
    The denominator is declared twice on purpose — `[tool.coverage.run]` in `pyproject.toml` and
    `_SOURCE_TREES` here — because a check that reads the same configuration it checks agrees with
    any edit to it. Two statements only work as a cross-check while they mean the same thing.

    `omit` names `corpus/checkouts/*` and `corpus/fixtures/*`: the two data trees at the TOP of
    `corpus/`. A walker that instead skipped any directory called `fixtures` at any depth would
    disagree the day production code lands in `corpus/pkg/fixtures/`: coverage.py would measure it,
    the walker would not expect it, and the guard would refuse the run while reporting that the
    denominator "is meant to be production code only" — about a file that is production code.
    Fail-closed, but with the wrong diagnosis, which is worse than no message.
    """

    def test_only_the_top_level_data_trees_are_excluded(self, tmp_path: Path) -> None:
        corpus = tmp_path / "corpus"
        for name in ["real.py", "fixtures/oracle.py", "checkouts/vendor/mod.py", "pkg/fixtures/helper.py"]:
            path = corpus / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("x = 1\n", encoding="utf-8")

        walked = measurable_source_files(tmp_path, PYTHON_REPORT_FORMAT)

        assert walked == {"corpus/real.py", "corpus/pkg/fixtures/helper.py"}
