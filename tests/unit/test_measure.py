"""
Hand-counted regression tests for `corpus/measure.py`.

Every expected number here was counted by eye on `corpus/fixtures/sample.py` (19 physical
lines, verified with `cat -n`) BEFORE `measure.py` computed anything. The fixture is the
oracle: if the code and this file disagree, the code is what changed.

Why the numbers matter (intent, not behaviour): four hard constants in epic tasks 3/4/5/7 are
read off `measure.py`'s output. The issue records what a miscount costs — `radon multi`
reported 79 docstrings where 94 existed — and a shifted denominator moves every downstream
constant silently. So these tests pin the four places a prose counter is easiest to get wrong:

  1. a MODULE docstring is prose (a walk over def/class only loses it),
  2. a CLASS docstring is prose (same failure, different node type),
  3. a TRAILING comment is NOT a standalone comment block (folding it in inflates the block
     count and drags the median block size toward 1 — the exact statistic the min-block-size
     decision reads),
  4. the prose-share denominator is ALL physical lines, which is what reproduces the issue's
     published 48/53/42% and therefore keeps REPORT.md comparable with the issue.

The fixture is deliberately excluded from ruff/ty (`extend-exclude`): `ruff format` rewrote it
once, changed its line count from 19 to 22, and turned three of these assertions red. Its byte
layout is test data, not code.

Run: make test    (or: uv run --only-group test pytest)
"""

from __future__ import annotations

from pathlib import Path

import measure
import pytest

# corpus/fixtures/sample.py, reached from tests/unit/
FIXTURE: Path = Path(__file__).parents[2] / "corpus" / "fixtures" / "sample.py"

# --- hand counts on corpus/fixtures/sample.py ------------------------------------------
# docstrings: lines 1-4 (module), 12 (class), 15-17 (method)
EXPECTED_DOCSTRING_SIZES: list[int] = [1, 3, 4]
# comment blocks: lines 6-7 glue into one 2-line block; line 18 is a 1-line block
EXPECTED_COMMENT_BLOCK_SIZES: list[int] = [1, 2]
EXPECTED_TRAILING_COMMENTS: int = 1  # line 19: `# trailing comment, not a block`
EXPECTED_PROSE_LINES: int = 11  # 4 + 1 + 3 docstring lines, 2 + 1 comment lines
EXPECTED_BLANK_LINES: int = 4  # lines 5, 9, 10, 13 — 2 and 16 are blank INSIDE docstrings
EXPECTED_CODE_LINES: int = 4  # lines 8, 11, 14, 19
EXPECTED_TOTAL_LINES: int = 19
EXPECTED_BLOCK_COUNT: int = 5  # 3 docstrings + 2 comment blocks


@pytest.fixture
def stats() -> measure.FileStats:
    """Measure the hand-counted fixture once per test."""
    return measure.measure_file(FIXTURE)


def test_docstring_sizes_include_module_and_class_docstrings(stats: measure.FileStats) -> None:
    """
    Module (4 lines), class (1) and method (3) docstrings must all be counted.

    Guard: a counter visiting only FunctionDef drops the module and class entries,
    which is the `radon multi` undercount the issue hit.
    """
    assert sorted(stats.docstring_sizes) == EXPECTED_DOCSTRING_SIZES


def test_comment_blocks_glue_runs_and_exclude_trailing_comments(stats: measure.FileStats) -> None:
    """Adjacent own-line comments form one block; a comment beside code is not a block."""
    assert sorted(stats.comment_block_sizes) == EXPECTED_COMMENT_BLOCK_SIZES
    assert stats.trailing_comments == EXPECTED_TRAILING_COMMENTS


def test_line_buckets_partition_the_file_exactly(stats: measure.FileStats) -> None:
    """
    Prose + blank + code must equal every physical line, with no line counted twice.

    This sum is the denominator of every ratio in REPORT.md. If it does not come to 19,
    the reported prose share is meaningless.
    """
    assert stats.prose_lines == EXPECTED_PROSE_LINES
    assert stats.blank_lines == EXPECTED_BLANK_LINES
    assert stats.code_lines == EXPECTED_CODE_LINES
    assert stats.total_lines == EXPECTED_TOTAL_LINES
    assert stats.prose_lines + stats.blank_lines + stats.code_lines == EXPECTED_TOTAL_LINES


def test_prose_share_uses_all_physical_lines_as_denominator(stats: measure.FileStats) -> None:
    """
    `prose_share` is 11/19, not 11/15.

    The choice is not cosmetic: prose/total is the denominator that reproduces the issue's
    published 48/53/42% for the three files it names
    (measured: 47.4/53.0/41.7%). Switching to the non-blank denominator would silently
    inflate every figure in REPORT.md against the issue it must be comparable with.
    """
    assert stats.prose_share == pytest.approx(11 / 19)
    assert stats.prose_share_nonblank == pytest.approx(11 / 15)


def test_blocks_are_sorted_by_position_and_normalised(stats: measure.FileStats) -> None:
    """Block order is a sorted function of position (AC4), and text is normalised."""
    assert [b.start_line for b in stats.blocks] == sorted(b.start_line for b in stats.blocks)
    assert len(stats.blocks) == EXPECTED_BLOCK_COUNT

    module_doc: measure.ProseBlock = stats.blocks[0]
    assert module_doc.kind == "docstring"
    assert module_doc.size_lines == 4
    # lowercased, whitespace collapsed, quotes and punctuation gone
    assert module_doc.normalised == "module docstring line one line three still the module docstring"
    assert module_doc.size_words == 10


@pytest.mark.parametrize(
    "series,percent,expected",
    [
        # Nearest-rank: rank = ceil(p/100 * n), 1-indexed. Hand-checked on 1..10:
        # ceil(0.5*10) = 5 -> 5th smallest = 5; ceil(0.9*10) = 9 -> 9th smallest = 9.
        # NOT 10, and not numpy's interpolated 9.1 — REPORT.md quotes whole lines, so the
        # discrete definition is the contract and has to be pinned.
        ([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 50, 5),
        ([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 90, 9),
        ([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 99, 10),
        ([7], 90, 7),
        ([], 90, 0),
    ],
)
def test_percentile_uses_nearest_rank(series: list[int], percent: int, expected: int) -> None:
    """Percentiles are discrete nearest-rank, so REPORT.md can quote whole lines."""
    assert measure.percentile(series, percent) == expected


@pytest.mark.parametrize(
    "raw,expected",
    [
        # Commented-out code — ruff ERA001's territory, out of scope per the issue, so every
        # one of these is a FALSE POSITIVE for detector 2. This split is what the detector
        # priority decision in REPORT.md rests on (pydantic: 420 of 514 candidates).
        (" print(df, file_path)", True),
        (" return filtered_defs", True),
        (" x = 1", True),
        (" if lock:", True),
        # A bare attribute access is a disabled statement, not prose (review finding A4):
        # `# config.retry_limit` above `if config.retry_limit:` is dead code.
        (" config.retry_limit", True),
        (" lock.acquired", True),
        # Real prose that FAILS to parse — these only ever exercise the SyntaxError path,
        # which is why they could not guard the exemption branch below.
        (" increment counter", False),
        (" release lock", False),
        (" sorted, not set: compared across processes", False),
        ("", False),
        # Real prose that PARSES. Before these cases the exemption branch was unguarded:
        # replacing its condition with `if False:` left the entire suite green.
        (" counter", False),
        (" TODO", False),
        (" 42", False),
        # A slash- or dash-joined phrase parses as an operator expression but is prose
        # (review finding A4): `# source / destination` above `copy(source, destination)` is a
        # restatement, and scoring it as code inflated the commented-out share.
        (" source / destination", False),
        (" first - second", False),
        (" width * height", False),
        # Prose that merely CONTAINS a bracket. Nothing here is a statement — every one fails to
        # parse — but each carries the punctuation a shape-based heuristic reaches for first, and
        # a loosening to `"(" in body -> True` is what moves the 117 of the no-ship verdict to 111
        # (`tests/unit/test_restatement_verdict.py`). Before these rows that loosening left the
        # whole suite green.
        (" release the lock (the caller owns it)", False),
        (" retry up to MAX_RETRIES (see the table above)", False),
        (" the rows [and their headers] are copied verbatim", False),
        # Prose with a bracket that DOES parse: a bare parenthesised name is still a phrase, so
        # this row reaches the exemption branch rather than the SyntaxError path above it.
        (" (deprecated)", False),
    ],
)
def test_looks_like_code_separates_commented_out_code_from_prose(raw: str, expected: bool) -> None:
    """
    Disabled statements must be told apart from prose that merely parses.

    `# increment counter` is a restatement; `# print(df, file_path)` is dead code. Counting
    the second as a restatement candidate would credit detector 2 with finds that belong to
    ruff ERA001 and inflate its apparent value.
    """
    assert measure.looks_like_code(raw) is expected
