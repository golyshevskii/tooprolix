"""
Read a coverage report, refuse to believe it if it did not measure what it claims, and print the
percentage.

The refusing is the point. A coverage percentage is trivially raised by measuring less, and every
way of doing that leaves a report which is internally consistent, freshly generated and completely
plausible — drop `branch = true`, let a source file fall out of discovery, orphan a module from the
`mod` tree. Nothing downstream can catch any of it, because there is nothing wrong with the number
itself; the denominator is what moved. So the report is graded before the number is read out of it,
and the run fails rather than printing a figure that flatters the code.

There is no badge: the repository is private until the PyPI flip, so no badge host can read it. The
number is printed for a human and written to `target/coverage/`.

Three properties, each tested in `tests/unit/test_coverage_report.py` as a function AND — in
`TestTheGuardIsWiredIntoTheEntryPoint` — by running this script the way the Makefile runs it,
because a guard reachable only through a function call can be unwired from `main` with every other
test still passing:

  1. **the number is never typed by a human.** `percent_from_report` reads it out of the report the
     coverage tool wrote, so the printed figure and the tool cannot disagree by transcription.
  2. **the number never overstates.** `format_percent` truncates toward zero, so 99.96% reads
     `99.9%` and only a genuine 100% reads `100.0%`.
  3. **the report is graded before the number is believed.** `verify_report_measured_the_source_tree`
     refuses a report that measured less than the whole source tree, or measured it with branch
     coverage off, and nothing is printed when it does.

Usage (this is the exact invocation in the Makefile's `rust.cov` recipe):

    python3 scripts/coverage_report.py --report target/coverage/llvm-cov.json --format llvm-cov

Run: make rust.cov / make py.cov / make cov
"""

from __future__ import annotations

import argparse
import json
from decimal import ROUND_DOWN, Decimal
from pathlib import Path
from typing import Any

# The two machine-readable report formats this repository's two coverage tools emit.
RUST_REPORT_FORMAT = "llvm-cov"
PYTHON_REPORT_FORMAT = "coverage.py"

# What each format's percentage is called when printed. Derived from `--format` rather than passed
# in, so the label cannot end up describing the wrong report.
_LABELS = {RUST_REPORT_FORMAT: "rust coverage", PYTHON_REPORT_FORMAT: "python coverage"}


def format_percent(percent: float) -> str:
    """
    Format a coverage percentage as one decimal, truncated toward zero.

    Truncation rather than round-to-nearest, deliberately: the figure must never claim coverage the
    run did not measure, so 99.96% reads `99.9%` and only a genuine 100% reads `100.0%`. `Decimal`
    on the number's `repr` rather than integer arithmetic on `percent * 10`, because in IEEE 754
    `83.9 * 10` is `838.9999999999999` and truncating that would lose a tenth the tool did measure.
    """
    truncated = Decimal(repr(float(percent))).quantize(Decimal("0.1"), rounding=ROUND_DOWN)
    return f"{truncated}%"


def percent_from_report(report: Path, report_format: str) -> float:
    """
    Read the total percentage out of a coverage tool's JSON report.

    Both lookups fail with `KeyError` on a report that does not carry the field, rather than
    defaulting to 0.0: a printed `0.0%` is a claim about the code, and a schema change is a fact
    about the parser. The two must not look the same.

    The two numbers are NOT the same measure, and they are reported separately for exactly this
    reason: the Rust figure is line coverage, while coverage.py's `percent_covered` folds branches
    in (`branch = true`). Do not add them together or describe them as comparable.
    """
    data: Any = json.loads(report.read_text(encoding="utf-8"))

    if report_format == RUST_REPORT_FORMAT:
        # `cargo llvm-cov --json --summary-only`. LINE coverage, because it is the only whole-crate
        # measure llvm-cov offers here: `branches` reads 0/0 on the pinned stable toolchain, and
        # `regions`/`functions`/`instantiations` are different questions again.
        return float(data["data"][0]["totals"]["lines"]["percent"])

    if report_format == PYTHON_REPORT_FORMAT:
        # `coverage json`. `percent_covered` already folds branches in when `branch = true`;
        # `percent_covered_display` is a pre-rounded string and would silently drop the decimal.
        return float(data["totals"]["percent_covered"])

    raise ValueError(f"unknown report format: {report_format!r}")


# The measurement contract, per language: (source directory, extension, subdirectories that are test
# DATA rather than code). Deliberately NOT read out of `[tool.coverage.run]` — a check that reads the
# same configuration as the thing it checks agrees with any edit to that configuration, which is the
# whole defect. This is the second, independent statement of the denominator, and
# `verify_report_measured_the_source_tree` is where the two are made to agree.
_SOURCE_TREES: dict[str, tuple[str, str, tuple[str, ...]]] = {
    RUST_REPORT_FORMAT: ("src", ".rs", ()),
    PYTHON_REPORT_FORMAT: ("corpus", ".py", ("checkouts", "fixtures")),
}

# Formats whose reports list source files that were never executed, so "every file on disk must
# appear" is unconditional. coverage.py does this — it walks `source` and reports an untouched
# module at 0%, which is exactly how a file dropping out of discovery becomes detectable.
#
# `cargo llvm-cov` does not: it reports what the compiler instrumented. A missing `.rs` is therefore
# excused there, but only when the file defines no functions — see the check below, which applies
# that condition instead of exempting the whole format. Both formats are checked in the other
# direction (nothing outside the source tree may be in the denominator) unconditionally.
_FORMATS_THAT_REPORT_UNEXECUTED_FILES = {PYTHON_REPORT_FORMAT}


def measurable_source_files(repo_root: Path, report_format: str) -> set[str]:
    """
    Walk the filesystem for the source files a coverage run of `report_format` must measure.

    Walked, not listed. A hard-coded list of today's four corpus modules would keep agreeing with a
    report that never discovered a fifth file added beside them — and coverage.py genuinely does not
    discover unexecuted files inside a directory that is not an importable package, so the file
    would leave the denominator and push the percentage UP with nothing to show for it.
    """
    root_name, suffix, excluded = _SOURCE_TREES[report_format]
    root = repo_root / root_name
    return {
        path.relative_to(repo_root).as_posix()
        for path in root.rglob(f"*{suffix}")
        # `parts[:1]` — the TOP-level directory only, matching the shape of the `omit` globs in
        # pyproject.toml (`corpus/checkouts/*`, `corpus/fixtures/*`). Skipping any directory named
        # `fixtures` at any depth would make the two statements of the contract disagree the day
        # production code lands in `corpus/pkg/fixtures/`, and the guard would then reject the run
        # with a message about test data, about a file that is not test data.
        if path.is_file() and not set(path.relative_to(root).parts[:1]) & set(excluded)
    }


def _measured_files(data: Any, report_format: str, repo_root: Path) -> set[str]:
    if report_format == RUST_REPORT_FORMAT:
        names = [entry["filename"] for entry in data["data"][0]["files"]]
    else:
        names = list(data["files"])
    measured = set()
    for name in names:
        path = Path(name)
        # llvm-cov writes absolute paths; coverage.py writes repository-relative ones. A path that
        # is not under the repository is kept verbatim so it shows up as unexpected rather than
        # raising something unreadable here.
        measured.add(
            path.relative_to(repo_root).as_posix()
            if path.is_absolute() and path.is_relative_to(repo_root)
            else path.as_posix()
        )
    return measured


def verify_report_measured_the_source_tree(data: Any, report_format: str, repo_root: Path) -> None:
    """
    Refuse a report that measured something other than the whole source tree.

    Reading the percentage out of the report grades the number; this grades the report. Every
    failure below produces a report that is internally consistent and carries a plausible, FRESH
    percentage — and in each case the percentage rises because less was measured:

      - `branch = true` removed: 52.37% becomes 53.20% on this repository, measuring statements only;
      - a source file that fell out of discovery: gone from the denominator entirely;
      - `tests/unit` added to `source`: the figure then climbs whenever someone writes test code,
        which measures the opposite of what a coverage number is for.

    None of those leave a trace in the percentage, so none of them can be caught downstream.
    """
    if report_format == PYTHON_REPORT_FORMAT and not data["meta"]["branch_coverage"]:
        raise ValueError(
            "the coverage report says branch coverage was OFF, so this percentage counts statements "
            "only and is not the measure it is read as; restore `branch = true` under "
            "[tool.coverage.run] in pyproject.toml"
        )

    expected = measurable_source_files(repo_root, report_format)
    measured = _measured_files(data, report_format, repo_root)

    missing = expected - measured
    if missing and report_format not in _FORMATS_THAT_REPORT_UNEXECUTED_FILES:
        # llvm-cov reports what the compiler instrumented, so a source file with nothing to
        # instrument is legitimately absent — `src/detect.rs` is 26 lines of module documentation
        # and two `pub mod` declarations. That is the ONLY excuse, and it is checked rather than
        # assumed: a file that defines functions and is still missing has left the denominator
        # silently, because it was orphaned from the module tree or gated behind a feature this run
        # does not enable. `cargo fmt`, `clippy` and `cargo test` all walk the module tree, so none
        # of them would notice either.
        #
        # `fn ` in the file text is the whole test. Its known ceiling is a `fn` appearing only
        # inside a doc-comment example, which would demand an entry llvm-cov will never produce —
        # that fails loudly and is fixed by wiring the module up or saying why it has no code.
        missing = {name for name in missing if "fn " in (repo_root / name).read_text(encoding="utf-8")}
    if missing:
        raise ValueError(
            f"the coverage report never measured {len(missing)} source file(s) that exist on disk: "
            f"{', '.join(sorted(missing))} — they are outside the denominator, so the percentage is "
            f"higher than the truth"
        )
    if extra := measured - expected:
        raise ValueError(
            f"the coverage report measured files outside the source tree: {', '.join(sorted(extra))} "
            f"— the denominator is meant to be production code only, and one that contains the tests "
            f"grows whenever more test code is written"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True, help="JSON report written by the coverage tool")
    parser.add_argument("--format", required=True, choices=[RUST_REPORT_FORMAT, PYTHON_REPORT_FORMAT])
    args = parser.parse_args()

    data: Any = json.loads(args.report.read_text(encoding="utf-8"))
    # Grade the report BEFORE emitting anything. The printed percentage is the artifact now, so a
    # rejected report must leave no percentage behind at all — a number on stdout is a number
    # somebody quotes, and `make cov` exiting non-zero after printing one would still have said it.
    # This ordering is pinned by `test_a_rejected_report_exits_non_zero_and_prints_nothing`, whose
    # second assertion exists solely to catch a verify that moved below the print.
    verify_report_measured_the_source_tree(data, args.format, Path(__file__).resolve().parents[1])

    percent = percent_from_report(args.report, args.format)
    print(f"{_LABELS[args.format]}: {format_percent(percent)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
