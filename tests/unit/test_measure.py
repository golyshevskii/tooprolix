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

import logging
import re
import sys
import tomllib
from pathlib import Path

import measure
import pytest

# corpus/fixtures/sample.py, reached from tests/unit/
FIXTURE: Path = Path(__file__).parents[2] / "corpus" / "fixtures" / "sample.py"
#: The second oracle, and the only one that can tell the interpreters apart. See its own test.
FSTRING_FIXTURE: Path = Path(__file__).parents[2] / "corpus" / "fixtures" / "fstring_identifiers.py"
PYPROJECT: Path = Path(__file__).parents[2] / "pyproject.toml"
CI_WORKFLOW: Path = Path(__file__).parents[2] / ".github" / "workflows" / "ci.yml"

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


def test_the_corpus_floor_stays_above_the_floor_the_distribution_promises() -> None:
    """
    The two floors are SEPARATE contracts and must not be collapsed back into one value.

    `requires-python` is what an installer reads: the wheel is `py3-none-<platform>`, carrying a
    native executable and no Python at all, so the distribution runs wherever it is installed.
    `MIN_INTERPRETER` is a MEASURED floor for this script alone — PEP 701 changed what `tokenize`
    reports inside f-strings, so a 3.11 run emits different constants into REPORT.md. Raising
    `requires-python` back to the corpus floor would lock installers out of an interpreter the
    distribution supports; lowering `MIN_INTERPRETER` to the distribution floor would let the
    research oracle answer with numbers no downstream constant was derived from.

    The exact spelling is asserted before it is parsed, and both halves are load-bearing. AC5
    promises `>=3.11` and nothing narrower OR broader: `>=3.10` would satisfy the separation below
    while advertising an interpreter nothing installs on, and `>=3.11,<4` or `~=3.11` would mean
    something this test's parser cannot read. Pinning the string makes the parse total.
    """
    declared: str = tomllib.loads(PYPROJECT.read_text(encoding="utf-8"))["project"]["requires-python"]

    assert declared == ">=3.11"
    distribution_floor: tuple[int, ...] = tuple(int(part) for part in declared.removeprefix(">=").split("."))
    assert distribution_floor < measure.MIN_INTERPRETER


def test_ci_runs_above_the_corpus_floor_so_the_open_upper_bound_is_exercised() -> None:
    """
    `requires-python = ">=3.11"` has no upper bound, and an unexercised upper bound is a claim.

    CI must therefore run STRICTLY ABOVE the corpus floor, not on it: pinned to the floor, every job
    in the repository sits at the bottom of a range the package advertises as open, and a break on
    any newer interpreter passes everything. This is what makes "the upper bound is open" a measured
    statement rather than a comment.

    Reading the `env:` value alone would assert the first regex match rather than the workflow: a
    job or step setting `python-version: "3.12"` directly leaves the global value untouched and runs
    on the floor anyway. So every `python-version:` in the file is required to route through
    `${{ env.PYTHON_VERSION }}`, and the file must declare that variable exactly once — absent or
    duplicated, this fails rather than picking one.

    `build-artifacts.yml` is deliberately NOT covered: its 3.11 pin is the distribution floor, a
    different contract, exercised on purpose.
    """
    text: str = CI_WORKFLOW.read_text(encoding="utf-8")

    declared: list[tuple[str, str]] = re.findall(r'^\s*PYTHON_VERSION:\s*"(\d+)\.(\d+)"\s*$', text, re.MULTILINE)
    assert len(declared) == 1, f"ci.yml must declare PYTHON_VERSION exactly once, found {declared}"

    literal: list[str] = [
        value
        for value in re.findall(r"^\s*python-version:\s*(.+?)\s*$", text, re.MULTILINE)
        if value != "${{ env.PYTHON_VERSION }}"
    ]
    assert literal == [], f"ci.yml pins python-version outside PYTHON_VERSION: {literal}"

    ci_interpreter: tuple[int, int] = (int(declared[0][0]), int(declared[0][1]))
    assert ci_interpreter > measure.MIN_INTERPRETER


def test_the_corpus_tool_refuses_an_interpreter_below_its_own_floor(
    monkeypatch: pytest.MonkeyPatch, caplog: pytest.LogCaptureFixture, tmp_path: Path
) -> None:
    """
    On 3.11 the script must REFUSE before measuring, and say which floor it wants.

    The message is asserted, not only the exit code: every other early failure in `main` also
    returns 2, so a test reading the code alone would stay green with the guard deleted — which is
    precisely the fail-open shape the guard exists to prevent. The lock is deliberately absent, so
    a run that got past the guard reports `cannot read` instead and reddens here.
    """
    monkeypatch.setattr(sys, "version_info", (3, 11, 9, "final", 0))

    with caplog.at_level(logging.ERROR, logger="measure"):
        code = measure.main(["--lock", str(tmp_path / "corpus.lock")])

    assert code == 2
    assert "need CPython >= 3.12, got 3.11" in caplog.text


def test_identifiers_interpolated_in_an_fstring_are_seen_by_the_restatement_probe() -> None:
    """
    The PEP 701 difference itself, which every other test in this suite is blind to.

    `MIN_INTERPRETER`, the CI pin and the whole two-floor split rest on one measured fact: before
    3.12 an f-string is ONE STRING token, so `counter` and `limit` inside
    `f"{counter} of {limit}"` are invisible to the probe and the comment above it scores 0 instead
    of 1.0. `sample.py` contains no interpolated identifier, so the entire suite passes identically
    on 3.11 and 3.14 — and a developer whose venv is 3.11 (buildable at all only because the
    distribution floor was lowered) gets a green run that agrees with them.

    So this test is DESIGNED to go red on 3.11. That is not a flaw in it: it is the only thing in
    the repository that tells such a developer the interpreter is wrong instead of quietly
    producing different numbers. Hand-counted on the 6-line fixture: the block on line 5 is the one
    hit, and its target is the f-string on line 6.
    """
    stats: measure.FileStats = measure.measure_file(FSTRING_FIXTURE)

    assert [hit.line for hit in stats.restatement_hits] == [5]
    assert stats.restatement_hits[0].code_line == 'message = f"{counter} of {limit}"'


def test_rendering_the_report_below_the_corpus_floor_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    `render` is the single emitter, so it is the last place a wrong interpreter can be stopped.

    Guarding `measure_repo` alone still lets an importer assemble `RepoStats` out of unguarded
    `measure_file` calls and render them: measured, that route shifts the counts (OpenHands
    1249 -> 1255, crewAI 147 -> 149, langgraph 482 -> 485, agents 595 -> 597). Numbers that look
    right and are wrong is precisely what `MIN_INTERPRETER` exists to prevent, so the refusal has to
    sit on the path every REPORT line goes through, not only on the path most callers take.
    """
    monkeypatch.setattr(sys, "version_info", (3, 11, 9, "final", 0))

    with pytest.raises(RuntimeError, match=r"3\.12"):
        measure.render([], {})


def test_measuring_a_repo_below_the_corpus_floor_raises_instead_of_emitting_numbers(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """
    The floor belongs on the EMITTING path, not only on the CLI.

    `main`'s check protects `uv run python3 corpus/measure.py`. It protects nothing that imports
    `measure` — and lowering `requires-python` to the distribution floor is exactly what made a 3.11
    environment buildable in the first place. `measure_repo` is where every counted repository goes
    through, so a wrong-interpreter run stops there rather than returning numbers that look right.

    `measure_file` is deliberately NOT guarded: it is the unit under test above, called directly on
    `corpus/fixtures/` by the hand-counted tests.
    """
    monkeypatch.setattr(sys, "version_info", (3, 11, 9, "final", 0))

    with pytest.raises(RuntimeError, match=r"3\.12"):
        measure.measure_repo("https://example.invalid/repo", "0" * 40, tmp_path)


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
