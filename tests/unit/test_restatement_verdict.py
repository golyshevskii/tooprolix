"""
The evidence behind the no-ship verdict on detector 2 (code restatement).

The heuristic was gated on both mandatory fixtures passing deterministically. They DO — and the
detector still does not ship, because the setting that passes them is the same setting that fires
117 times on the repository every shipped detector is required to be silent on.

That verdict is only worth the numbers under it, and one of those numbers is a boundary nobody
would notice by reading: the positive fixture scores EXACTLY the shipped threshold and carries
EXACTLY the shipped minimum token count. So tightening — the only direction in which 117 falls —
loses the fixture the detector exists to pass.

WHAT THESE TESTS DO AND DO NOT PIN. They pin the fixture verdicts, both constants, the stemmer, and
the score that separates the red-team fixture from the threshold. They do **not** pin the 117
itself, which needs `corpus/checkouts/` (git-ignored). Two knobs move that count without moving any
fixture verdict, and both were found by review rather than by writing the tests. **Both are now
pinned:**

* `_stem` replaced by the identity function -> 117 becomes 100 (see the stemmer test);
* `looks_like_code` loosened, e.g. `or "(" in raw` -> 117 becomes 111 — the filter that produces
  the subtracted 20. Pinning only its two extremes leaves that loosening green (measured by
  inserting `if "(" in body: return True`), so
  `test_a_prose_restatement_carrying_parentheses_stays_in_the_117` is the middle case that closes
  it, together with the bracket rows of
  `test_measure.py::test_looks_like_code_separates_commented_out_code_from_prose`.

So a green suite means "the setting the verdict was argued from is still that setting", not "the
reference count is still 117". The count is reproduced by `make corpus.measure` and recorded in
`corpus/REPORT.md`: on an audited repository restate=137, commented=20, i.e. 117 after the
commented-out-code filter.

Run: make test
"""

from __future__ import annotations

from pathlib import Path

import measure
import pytest

# `# increment counter` over `counter += 1` — the issue's ToDo 3 positive fixture, which the
# detector must flag or it detects nothing anyone asked for.
POSITIVE = "# increment counter\ncounter += 1\n"

# The issue's red-team fixture, verbatim from its TDD block: a comment that quotes the very
# identifiers it is arguing AGAINST. Worst possible input for token overlap.
RED_TEAM = "def key(payload):\n    # sorted, not set: сравнивается между процессами\n    return sorted(set(payload))\n"

# The same argument written in English. It exists to separate two explanations for the skip: the
# score, or the Cyrillic. Only this one can tell them apart, and it is the one the guards use.
RED_TEAM_ENGLISH = (
    "def key(payload):\n    # sorted, not set: compared across processes\n    return sorted(set(payload))\n"
)

# Out of scope by the issue's own wording, kept as a documenting fixture: a Russian restatement
# is invisible to overlap with English identifiers.
RUSSIAN = "# увеличиваем счётчик\ncounter += 1\n"

# `ruff ERA001`'s territory, which the issue puts Out of scope — so this hit is a false positive
# by construction, and 420 of pydantic's 514 candidates are this class.
COMMENTED_OUT_CODE = "# print(df, file_path)\nprint(df, file_path)\n"

# A real restatement whose comment carries a parenthesis. It scores 2 of 4 stems against
# `lock.release()`, so it is a candidate, and it is PROSE — the brackets are an aside, not a call.
# This is the fixture that separates "is a restatement" from "is commented-out code" on the one
# input where a shape-based filter gets it wrong.
PROSE_WITH_PARENTHESES = "# release the lock (see above)\nlock.release()\n"


def hits(tmp_path: Path, source: str) -> list[measure.RestatementHit]:
    """Restatement candidates `corpus/measure.py` finds in `source`."""
    path = tmp_path / "fixture.py"
    path.write_text(source, encoding="utf-8")
    return measure.measure_file(path, rel_path="fixture.py").restatement_hits


def test_the_verdict_rests_on_these_exact_constants() -> None:
    """
    Pin the two numbers the 117 was measured at, because no fixture can pin them.

    Found by review: lowering the threshold to 0.45 or the token floor to 1 leaves EVERY other
    test in this file green — the positive fixture still clears the bar, the red-team fixture
    still misses it — while the reference-repo count moves 117 -> 120 and 117 -> 123. A verdict
    quoting a number measured at a setting nothing pins is a verdict quoting a story.
    """
    assert measure.RESTATEMENT_OVERLAP == 0.5
    assert measure.RESTATEMENT_MIN_TOKENS == 2


@pytest.mark.parametrize(
    ("word", "stem"),
    [
        ("sorted", "sort"),
        ("compared", "compar"),
        ("processes", "process"),
        ("across", "acros"),
        ("counter", "counter"),
        ("increment", "increment"),
    ],
)
def test_the_stemmer_is_part_of_the_setting_the_count_was_measured_at(word: str, stem: str) -> None:
    """
    The third knob, and the one no fixture reaches.

    Review's finding: replacing `_stem` with the identity function leaves every fixture verdict
    in this file unchanged — the positive fixture still scores 1 of 2, the red-team fixture still
    scores 2 of 5 — while the reference-repo count moves 117 -> 100. So the stemmer changes the
    number the verdict quotes without changing anything the fixtures can see, which is precisely
    the shape of a constant that drifts unnoticed. `across -> acros` is deliberate: it is the
    crude suffix strip doing something a real stemmer would not, and swapping in a real one is
    the change this test exists to make visible.
    """
    assert measure._stem(word) == stem


def test_the_positive_fixture_is_flagged(tmp_path: Path) -> None:
    found = hits(tmp_path, POSITIVE)

    assert len(found) == 1
    assert found[0].code_line == "counter += 1"
    assert found[0].code_like is False


def test_the_positive_fixture_scores_exactly_the_shipped_threshold(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """
    Raising the threshold by any amount loses the positive fixture.

    This is the load-bearing half of the verdict: 117 hits on an audited repository cannot be
    reduced by tightening, because the fixture is already sitting on the line. `increment` is
    absent from `counter += 1`, so the score is 1 of 2 tokens and no rewording changes that.
    """
    monkeypatch.setattr(measure, "RESTATEMENT_OVERLAP", 0.51)

    assert hits(tmp_path, POSITIVE) == []


def test_the_positive_fixture_carries_exactly_the_minimum_token_count(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The other tightening knob, and it is on its stop too: two distinct non-stopword stems."""
    monkeypatch.setattr(measure, "RESTATEMENT_MIN_TOKENS", 3)

    assert hits(tmp_path, POSITIVE) == []


@pytest.mark.parametrize("source", [RED_TEAM, RED_TEAM_ENGLISH], ids=["cyrillic", "english"])
def test_the_red_team_fixture_is_skipped(tmp_path: Path, source: str) -> None:
    """
    Both spellings skip — which is what says the metric did it, not the Cyrillic.

    The English twin is not decoration. The task file proposed an explanation-marker rule
    (punctuation, `not`, `because`) to produce this skip, and the issue's fixture happens to
    contain three Russian words — so a detector could pass the mandatory fixture through a
    non-ASCII bail-out while the rule it credited did nothing at all.
    """
    assert hits(tmp_path, source) == []


@pytest.mark.parametrize(
    ("source", "threshold"),
    [(RED_TEAM, 0.4), (RED_TEAM_ENGLISH, 0.4), (RUSSIAN, 0.0)],
    ids=["cyrillic", "english", "russian-blindness"],
)
def test_every_skipped_fixture_is_reachable_at_its_own_score(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, source: str, threshold: float
) -> None:
    """
    Each skip is a scoring decision, not a fixture the probe never saw.

    Every `== []` assertion above passes just as well against a probe that returns nothing for
    everything, so on its own it cannot tell "correctly skipped" from "never extracted". Drop the
    threshold to the fixture's own score and it must appear.
    """
    monkeypatch.setattr(measure, "RESTATEMENT_OVERLAP", threshold)

    assert len(hits(tmp_path, source)) == 1


def test_the_red_team_score_is_bracketed_below_the_threshold(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Name what does the work: dividing by the COMMENT's tokens, not by the matched ones.

    `sorted` and `set` both appear in the code, so a metric dividing by the CODE's tokens scores
    this comment 2/4 = 0.50 and flags it. Dividing by the comment's five tokens scores it 0.40.
    Bracketing — present at 0.40, absent at 0.41 — is what distinguishes the two, and it is the
    part a single lowered-threshold assertion misses: at 0.40 alone, the 0.50 impostor also
    passes. Review found exactly that hole in the previous version of this test.
    """
    monkeypatch.setattr(measure, "RESTATEMENT_OVERLAP", 0.41)

    assert hits(tmp_path, RED_TEAM_ENGLISH) == []


def test_the_russian_restatement_is_invisible(tmp_path: Path) -> None:
    """Out of scope by the issue's wording; pinned so the limitation is a decision, not a surprise."""
    assert hits(tmp_path, RUSSIAN) == []


def test_commented_out_code_is_flagged_and_carries_its_own_marker(tmp_path: Path) -> None:
    """
    The false-positive class the detector cannot see without `looks_like_code`.

    A disabled statement restates the code below it perfectly — score 1.0 — and is `ERA001`'s
    job, not ours. Nothing in the overlap metric separates the two; only the parse does.
    """
    found = hits(tmp_path, COMMENTED_OUT_CODE)

    assert len(found) == 1
    assert found[0].code_like is True


def test_a_prose_restatement_carrying_parentheses_stays_in_the_117(tmp_path: Path) -> None:
    """
    The knob the module docstring used to record as NOT pinned, and the reason it mattered.

    `looks_like_code` is the filter that subtracts the 20 commented-out hits from the 137
    candidates, so every hit it misclassifies moves the 117 the no-ship verdict is argued from.
    Loosening it — `or "(" in raw`, the review's own example — takes 117 to 111, and before this
    test every fixture in this file survived that: the positive fixture has no bracket, and the
    commented-out fixture is code with or without the filter. Both extremes, neither middle.

    This is the middle. `# release the lock (see above)` is a restatement of `lock.release()` —
    it must be counted (`len == 1`) AND counted as prose (`code_like is False`). The two
    assertions are not the same one twice: a filter that swallowed the whole hit would fail the
    first, and a filter that reclassified it as ERA001's would fail the second.
    """
    found = hits(tmp_path, PROSE_WITH_PARENTHESES)

    assert len(found) == 1
    assert found[0].code_like is False
