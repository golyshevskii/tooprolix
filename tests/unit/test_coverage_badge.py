"""
Guards for `scripts/coverage_badge.py`, the generator behind the two README coverage badges.

The badge is a number a human reads and acts on, so the two ways it can lie are the two things
tested here:

  1. **it can overstate.** The percentage is truncated toward zero at one decimal, never rounded to
     nearest — 99.96% renders `99.9%`, not `100.0%`. A badge that says 100 on a code base with an
     uncovered line is the worst outcome, so the rounding rule is pinned by cases that a
     round-half-up implementation gets wrong.
  2. **it can disagree with the tool that measured it.** The percentage is never typed by hand: it
     is read out of the machine-readable report the coverage tool itself wrote. The two extractors
     are tested against report fragments captured verbatim from `cargo llvm-cov --json` and
     `coverage json` on this repository, so a test cannot pass by agreeing with the generator's own
     arithmetic.

The SVG is asserted byte for byte against a literal written out by hand from the layout rules, not
against `render()`'s own output — the file that ships is the artifact, and a self-comparison would
grade the generator against itself.

Run: make test
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from coverage_badge import PYTHON_REPORT_FORMAT, RUST_REPORT_FORMAT, percent_from_report, render_badge

REPO_ROOT = Path(__file__).resolve().parents[2]


class TestThePercentageIsNeverRoundedUp:
    """
    Truncation toward zero at one decimal. Every case below is one a round-half-up implementation
    would get wrong, so replacing the rounding rule turns this class red.
    """

    @pytest.mark.parametrize(
        "measured,rendered",
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
    def test_the_rendered_percentage(self, measured: float, rendered: str) -> None:
        assert f">{rendered}<" in render_badge("python coverage", measured)


class TestTheSvgIsByteExact:
    """
    The shipped artifact is a file, so it is asserted as a file: one literal document, written by
    hand from the layout arithmetic. Comparing against `render_badge`'s own output instead would be
    a shape check wearing a test's name.
    """

    def test_a_known_percentage_renders_the_expected_document(self) -> None:
        # Written out by hand from the layout rules: a box is 7px per character plus 12px of
        # padding, so the label box ("rust coverage", 13 chars) is 7*13+12 = 103, the value box
        # ("98.0%", 5 chars) is 7*5+12 = 47, and the document is 150 wide. Text is centred at
        # 103//2 = 51 and 103+47//2 = 126.
        expected = (
            '<svg xmlns="http://www.w3.org/2000/svg" width="150" height="20" role="img" '
            'aria-label="rust coverage: 98.0%">'
            "<title>rust coverage: 98.0%</title>"
            '<rect width="103" height="20" fill="#12130f"/>'
            '<rect x="103" width="47" height="20" fill="#f5c2c8"/>'
            '<g font-family="Verdana,DejaVu Sans,sans-serif" font-size="11" text-anchor="middle">'
            '<text x="51" y="14" fill="#e4dfda">rust coverage</text>'
            '<text x="126" y="14" fill="#12130f">98.0%</text>'
            "</g>"
            "</svg>\n"
        )

        assert render_badge("rust coverage", 98.07) == expected

    def test_the_document_carries_nothing_that_varies_between_runs(self) -> None:
        # A timestamp, a hostname or a run id in the SVG would make the CI drift gate fire on every
        # run and train everyone to ignore it.
        svg = render_badge("python coverage", 50.0)

        assert svg.count("<svg") == 1
        assert "date" not in svg.lower()
        assert "generated" not in svg.lower()


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


class TestTheCommittedBadgesAreTheOnesTheReadmeShows:
    """
    The README carries no percentage of its own — it embeds the SVG by path, so the number lives in
    exactly one committed place and cannot disagree with itself. What CAN drift is the path: a
    renamed or deleted SVG leaves the README with a broken image and the CI drift gate with nothing
    to diff, which `git diff --exit-code` reports as success. These tests are that gate's floor.
    """

    @pytest.mark.parametrize("name", ["coverage-rust.svg", "coverage-python.svg"])
    def test_the_badge_file_exists_and_carries_a_percentage(self, name: str) -> None:
        svg = (REPO_ROOT / "assets" / name).read_text(encoding="utf-8")

        assert svg.startswith("<svg "), f"assets/{name} is not the SVG the generator writes"
        assert "%</text>" in svg, f"assets/{name} shows no percentage"

    @pytest.mark.parametrize("name", ["coverage-rust.svg", "coverage-python.svg"])
    def test_the_readme_embeds_it(self, name: str) -> None:
        readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")

        assert f'src="assets/{name}"' in readme, f"README.md does not embed assets/{name}"
