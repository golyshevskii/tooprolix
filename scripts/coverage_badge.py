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

Usage:

    python3 scripts/coverage_badge.py --report target/llvm-cov/cov.json --format llvm-cov \
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
    """
    data: Any = json.loads(report.read_text(encoding="utf-8"))

    if report_format == RUST_REPORT_FORMAT:
        # `cargo llvm-cov --json --summary-only`. LINE coverage, matching what coverage.py reports
        # for the Python side; llvm-cov's `regions` and `functions` totals are different measures
        # and mixing them across the two badges would make the pair incomparable.
        return float(data["data"][0]["totals"]["lines"]["percent"])

    if report_format == PYTHON_REPORT_FORMAT:
        # `coverage json`. `percent_covered` already folds branches in when `branch = true`;
        # `percent_covered_display` is a pre-rounded string and would silently drop the decimal.
        return float(data["totals"]["percent_covered"])

    raise ValueError(f"unknown report format: {report_format!r}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True, help="JSON report written by the coverage tool")
    parser.add_argument("--format", required=True, choices=[RUST_REPORT_FORMAT, PYTHON_REPORT_FORMAT])
    parser.add_argument("--label", required=True, help="badge label, e.g. 'rust coverage'")
    parser.add_argument("--out", type=Path, required=True, help="SVG file to write")
    args = parser.parse_args()

    percent = percent_from_report(args.report, args.format)
    args.out.write_text(render_badge(args.label, percent), encoding="utf-8")
    # The printed number and the number in the SVG come from the same call, so AC1's "prints the
    # percentage" and AC2's "the SVG matches the target output" cannot drift apart.
    print(f"{args.label}: {format_percent(percent)}  ->  {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
