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

**What is NOT refused, said here because this is the part that gets read first.** Everything above
is about a report measuring LESS. A report can also measure MORE than the build produced: llvm-cov
merges the coverage mappings of every object it is handed, including objects no current target
builds, and the extra records arrive under real source filenames. That inflates the denominator and
DEFLATES the percentage — it cost this repository 20 points on 2026-08-01 and nobody noticed for
three days. Only one shape of it is rejected (a phantom under a file that defines no function); a
phantom under a function-bearing file is accepted, measured. Denominator integrity is therefore
partial, and the comment on that check in `verify_report_measured_the_source_tree` is the honest
account of what closing it would take.

Usage (this is the exact invocation in the Makefile's `rust.cov` recipe):

    python3 scripts/coverage_report.py --report target/coverage/llvm-cov.json --format llvm-cov

Run: make rust.cov / make py.cov / make cov
"""

from __future__ import annotations

import argparse
import json
import re
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


# The measurement contract, per language: (source directories, extension, subdirectories that are
# test DATA rather than code). Deliberately NOT read out of `[tool.coverage.run]` — a check that
# reads the same configuration as the thing it checks agrees with any edit to that configuration,
# which is the whole defect. This is the second, independent statement of the denominator, and
# `verify_report_measured_the_source_tree` is where the two are made to agree.
#
# `scripts/` joined `corpus/` on 2026-08-01. It is the release-critical Python — it grades the built
# archive and rewrites the README that goes to PyPI — and it was outside the denominator while
# `corpus/`, which the Makefile calls throwaway research tooling, was the whole of it. Measured
# 2026-08-01: the three scripts are 88% / 84% / 87% covered in-process by `tests/unit/`, so they are
# attributable; the total moved 61.4% -> 65.7%.
#
# TWO enforcers, each catching a different edit. Both measured 2026-08-01 by making the edit; do
# not name only one, and do not name the wrong one.
#
#   - narrow `[tool.coverage.run] source` back to `["corpus"]`, leave this list alone:
#     `test_the_release_scripts_are_in_the_denominator` stays GREEN (23 passed) — it reads this
#     list, not pyproject. `make py.cov` exits 2 from
#     `verify_report_measured_the_source_tree`: "never measured 3 source file(s) that exist on
#     disk: scripts/…".
#   - narrow THIS list, leave pyproject alone: the test goes RED (1 failed, 22 passed), and
#     `make py.cov` exits 2 as well — first because pytest runs that test, and independently
#     because the script then rejects the report from the other side: "measured files outside the
#     source tree: scripts/…".
#
# So the pair is what holds the denominator, and neither statement alone is the enforcer.
_SOURCE_TREES: dict[str, tuple[tuple[str, ...], str, tuple[str, ...]]] = {
    RUST_REPORT_FORMAT: (("src",), ".rs", ()),
    PYTHON_REPORT_FORMAT: (("corpus", "scripts"), ".py", ("checkouts", "fixtures")),
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
    root_names, suffix, excluded = _SOURCE_TREES[report_format]
    return {
        path.relative_to(repo_root).as_posix()
        for root_name in root_names
        for path in (repo_root / root_name).rglob(f"*{suffix}")
        # `parts[:1]` — the TOP-level directory only, matching the shape of the `omit` globs in
        # pyproject.toml (`corpus/checkouts/*`, `corpus/fixtures/*`). Skipping any directory named
        # `fixtures` at any depth would make the two statements of the contract disagree the day
        # production code lands in `corpus/pkg/fixtures/`, and the guard would then reject the run
        # with a message about test data, about a file that is not test data.
        if path.is_file() and not set(path.relative_to(repo_root / root_name).parts[:1]) & set(excluded)
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


def _defines_functions(repo_root: Path, name: str) -> bool:
    r"""
    Whether a source file's own text defines something `cargo llvm-cov` could have instrumented.

    One fact, used in both directions by `verify_report_measured_the_source_tree`: a file with no
    function is legitimately absent from an llvm-cov report, and is illegitimately present in one.

    `\bfn\s` rather than the literal `"fn "` this used to be. `pub fn\nrelease_gate() {}` and
    `fn\trelease_gate()` are both valid Rust that the substring missed, and the direction that
    matters is the excusing one: a file missed here is EXCUSED for being absent, so the substring
    let an orphaned module leave the denominator in silence. Rustfmt would join those lines — but
    an orphaned file is outside the module tree, so `cargo fmt` does not walk it either, and the
    file most in need of this check is the one least likely to have been normalised.

    Known ceiling, unchanged and deliberate: a module whose functions arrive from a macro expansion
    has instrumentable code and no `fn` token of its own. It is not closed in code because no
    `macro_rules!` or `#[proc_macro]` exists in `src/` (measured 2026-08-01), and a guard for a
    defect that cannot occur is cost without cover.

    **Neither direction handles that module correctly, and an earlier revision of this docstring
    claimed both fail closed. That was false.** Measured 2026-08-01 on a file containing only
    `make_fn!(release_gate);`, which returns False here:

      - ABSENT from the report: EXCUSED, so it leaves the denominator in silence. Fail-OPEN, in the
        exact direction the old sentence promised was closed.
      - PRESENT in the report: rejected — but by the phantom check, whose message blames a stale
        object. Fail-closed on the outcome, wrong on the cause, so a human debugging it is sent to
        the target directory to look for something that is not there.

    Lexical blindness is a separate and PRE-EXISTING property of any substring predicate: `fn ` in
    a doc comment, a line comment or a string literal matches, and matched under the literal
    `"fn "` too. This regex neither introduced nor widened it — measured over ten cases, the only
    behavioural change is the two real-function shapes above.
    """
    return re.search(r"\bfn\s", (repo_root / name).read_text(encoding="utf-8")) is not None


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
    # The same function fact read in the other direction. It caught a real 20-point error and it is
    # NOT a denominator-integrity guarantee — read the next paragraph before trusting it.
    #
    # What happened: `cargo llvm-cov` hands `llvm-cov export` the objects it finds in the target
    # directory, including ones no current target produces. A `libtooprolix.dylib` from a removed
    # pyo3 build survived from 2026-07-29, carried a coverage mapping of an older source tree, and
    # added 1 024 phantom lines at zero coverage under the real filename `src/lib.rs`. `make cov`
    # printed 78.6% instead of the 98.2% the same profraw gives without it, exited 0, and nothing
    # noticed: the phantom is inside the source tree so the `extra` check cannot see it, and
    # `src/lib.rs` was not missing so the check after this one could not either.
    #
    # WHAT THIS CATCHES: a phantom landing under a file whose text the compiler cannot have
    # instrumented. WHAT IT DOES NOT CATCH: a phantom landing under any function-bearing file.
    # Measured 2026-08-01 — 1 024 phantom lines injected under `src/cli.rs` into the real report:
    # this function ACCEPTED it and the number read 78.6%, the same wrong figure. So 98.2% is
    # trustworthy today partly because of which filename the stale object happened to carry.
    #
    # The shape — a denominator carrying records no current build produced — is not closed, and the
    # two cheap invariants were measured and rejected rather than assumed: instrumented lines vs
    # physical lines of the file (`src/cli.rs` is 1 883 physical against 716 instrumented, so the
    # phantom fits underneath), and any line-NUMBER bound, which needs per-line `segments` that the
    # `--summary-only` report the Makefile grades does not carry at all.
    # ponytail: closing it means grading the report against the `-object` list `llvm-cov export`
    # was given, cross-referenced with what cargo currently builds. That is a feature, not a guard;
    # build it when a phantom lands under a function-bearing file, not before.
    if report_format == RUST_REPORT_FORMAT and (
        phantom := {name for name in measured & expected if not _defines_functions(repo_root, name)}
    ):
        raise ValueError(
            f"the coverage report measured {len(phantom)} source file(s) that define no "
            f"function: {', '.join(sorted(phantom))} — llvm-cov cannot have instrumented that "
            f"text, so the denominator carries records from a stale object; clear the target "
            f"directory of artifacts no current target builds and measure again"
        )
    if missing and report_format not in _FORMATS_THAT_REPORT_UNEXECUTED_FILES:
        # llvm-cov reports what the compiler instrumented, so a source file with nothing to
        # instrument is legitimately absent — `src/detect.rs` is module documentation and two
        # `pub mod` declarations, nothing else. That is the ONLY excuse, and it is checked rather than
        # assumed: a file that defines functions and is still missing has left the denominator
        # silently, because it was orphaned from the module tree or gated behind a feature this run
        # does not enable. `cargo fmt`, `clippy` and `cargo test` all walk the module tree, so none
        # of them would notice either.
        #
        # `fn ` in the file text is the whole test. Its known ceiling is a `fn` appearing only
        # inside a doc-comment example, which would demand an entry llvm-cov will never produce —
        # that fails loudly and is fixed by wiring the module up or saying why it has no code.
        missing = {name for name in missing if _defines_functions(repo_root, name)}
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
