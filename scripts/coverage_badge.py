"""
Render a coverage percentage as a deterministic SVG badge, committed to `assets/`.

Why a file in the repository instead of a shields.io or codecov URL: this repository is private
until the PyPI flip, so a hosted badge endpoint has nothing to read and a gist endpoint would tie
the README to one person's account. The SVG is generated from the coverage tool's own JSON report,
committed, and CI regenerates it and fails on `git diff` — the drift gate is the whole design.

Two properties this script exists to hold, both of them tested in `tests/unit/test_coverage_badge.py`:

  1. **the number is never typed by a human.** `percent_from_report` reads it out of the report the
     coverage tool wrote, so the badge and the tool cannot disagree by transcription.
  2. **the output depends on nothing but the number.** No clock, no hostname, no run id, no network.
     Same percentage in, byte-identical file out — otherwise the CI drift gate would fire on every
     run and be turned off within a week.
  3. **the report is graded before the number is believed.** `verify_report_measured_the_source_tree`
     refuses a report that measured less than the whole source tree, or measured it with branch
     coverage off. Those failures all RAISE the percentage, so nothing downstream can catch them.

Usage (this is the exact invocation in the Makefile's `rust.cov` recipe):

    python3 scripts/coverage_badge.py --report target/coverage/llvm-cov.json --format llvm-cov \
        --label "rust coverage" --out assets/coverage-rust.svg

Run: make rust.cov / make py.cov
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

# Layout constants. Verdana at font-size 11 is roughly 6.5px per character; 7 is rounded up so a
# wide string is never clipped by its box, which is the only failure mode that matters here.
_PX_PER_CHAR = 7
_BOX_PADDING = 12
_HEIGHT = 20
_BASELINE = 14

# The README's existing badge row: `12130f` label box, `f5c2c8` accent, `e4dfda` light text.
_LABEL_BG = "#12130f"
_VALUE_BG = "#f5c2c8"
_LABEL_FG = "#e4dfda"
_VALUE_FG = "#12130f"


def _box_width(text: str) -> int:
    return _PX_PER_CHAR * len(text) + _BOX_PADDING


def format_percent(percent: float) -> str:
    """
    Format a coverage percentage as one decimal, truncated toward zero.

    Truncation rather than round-to-nearest, deliberately: the badge must never claim coverage the
    run did not measure, so 99.96% reads `99.9%` and only a genuine 100% reads `100.0%`. `Decimal`
    on the number's `repr` rather than integer arithmetic on `percent * 10`, because in IEEE 754
    `83.9 * 10` is `838.9999999999999` and truncating that would lose a tenth the tool did measure.
    """
    truncated = Decimal(repr(float(percent))).quantize(Decimal("0.1"), rounding=ROUND_DOWN)
    return f"{truncated}%"


def render_badge(label: str, percent: float) -> str:
    """Render the badge SVG for `percent` under `label`, as a complete document ending in a newline."""
    value = format_percent(percent)
    label_width = _box_width(label)
    value_width = _box_width(value)
    total = label_width + value_width
    title = f"{label}: {value}"

    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{total}" height="{_HEIGHT}" role="img" '
        f'aria-label="{title}">'
        f"<title>{title}</title>"
        f'<rect width="{label_width}" height="{_HEIGHT}" fill="{_LABEL_BG}"/>'
        f'<rect x="{label_width}" width="{value_width}" height="{_HEIGHT}" fill="{_VALUE_BG}"/>'
        f'<g font-family="Verdana,DejaVu Sans,sans-serif" font-size="11" text-anchor="middle">'
        f'<text x="{label_width // 2}" y="{_BASELINE}" fill="{_LABEL_FG}">{label}</text>'
        f'<text x="{label_width + value_width // 2}" y="{_BASELINE}" fill="{_VALUE_FG}">{value}</text>'
        f"</g>"
        f"</svg>\n"
    )


def percent_from_report(report: Path, report_format: str) -> float:
    """
    Read the total percentage out of a coverage tool's JSON report.

    Both lookups fail with `KeyError` on a report that does not carry the field, rather than
    defaulting to 0.0: a badge reading `0.0%` is a claim about the code, and a schema change is a
    fact about the parser. The two must not look the same.

    ⚠️ The two numbers are NOT the same measure, and the badges are labelled separately for exactly
    this reason: the Rust figure is line coverage, while coverage.py's `percent_covered` folds
    branches in (`branch = true`). Do not add them together or describe them as comparable.
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

# Formats whose reports list source files that were never executed, and for which "every file on
# disk must appear" is therefore a sound requirement. coverage.py does this — it walks `source` and
# reports an untouched module at 0%, which is exactly how a file dropping out of discovery becomes
# detectable. `cargo llvm-cov` does not: it reports what the compiler instrumented, so a file with
# no instrumentable items is legitimately absent. Measured, not assumed — `src/detect.rs` is 26
# lines of module docs plus two `pub mod` declarations, and requiring it made `make rust.cov` fail
# on the real repository. Both formats are still checked in the other direction (nothing outside the
# source tree may be in the denominator), which is sound for both.
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
        if path.is_file() and not set(path.relative_to(root).parts[:-1]) & set(excluded)
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
      - `tests/unit` added to `source`: the badge then climbs whenever someone writes test code,
        which is the one thing this task's AC1 forbids in writing.

    None of those leave a trace in the badge, so none of them can be caught downstream.
    """
    if report_format == PYTHON_REPORT_FORMAT and not data["meta"]["branch_coverage"]:
        raise ValueError(
            "the coverage report says branch coverage was OFF, so this percentage counts statements "
            "only and is not the measure the badge claims; restore `branch = true` under "
            "[tool.coverage.run] in pyproject.toml"
        )

    expected = measurable_source_files(repo_root, report_format)
    measured = _measured_files(data, report_format, repo_root)

    if (missing := expected - measured) and report_format in _FORMATS_THAT_REPORT_UNEXECUTED_FILES:
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
    parser.add_argument("--label", required=True, help="badge label, e.g. 'rust coverage'")
    parser.add_argument("--out", type=Path, required=True, help="SVG file to write")
    args = parser.parse_args()

    data: Any = json.loads(args.report.read_text(encoding="utf-8"))
    # Grade the report BEFORE writing anything. A rejected report leaves the committed badge
    # untouched, so `make cov.check` then fails on a stale badge rather than blessing a fresh wrong
    # one — the guard is on the path every `make rust.cov` / `make py.cov` takes, not on a test that
    # only runs when the report happens to be lying around.
    verify_report_measured_the_source_tree(data, args.format, Path(__file__).resolve().parents[1])

    percent = percent_from_report(args.report, args.format)
    args.out.write_text(render_badge(args.label, percent), encoding="utf-8")
    # The printed number and the number in the SVG come from the same call, so AC1's "prints the
    # percentage" and AC2's "the SVG matches the target output" cannot drift apart.
    print(f"{args.label}: {format_percent(percent)}  ->  {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
