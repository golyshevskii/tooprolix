"""
Guards for `corpus/units.py`, the AC1b volume-unit comparison.

AC1b asks which unit of "prose volume" is more precise: words (shipped), characters of the
normalised text, or BPE tokens of the normalised or the raw text. The council that chose words
was unanimous and explicitly *unproven*: what had been measured was how much the units **agree**,
and agreement cannot show which one is right. This file guards the three things that would make
the comparison say nothing at all:

  1. **Equal alert volume.** Compared at their own natural thresholds the units fire different
     numbers of times, and a unit that fires twice as often trivially "finds more". Every
     alternative unit is therefore calibrated to the exact number of alerts words produces. When
     ties in the value distribution make an exact match impossible, the function has to report the
     count it actually achieved — a calibrator that silently returns a different alert volume puts
     a loudness difference into a precision number.
  2. **Per kind.** Docstrings and comments are two populations — one shared 200-word limit fires
     on 1.99% of docstring blocks against 0.076% of comment blocks, a 26x difference — so a single
     pooled threshold is necessarily wrong for one of them, and the shipped rule has two limits.
  3. **Blind order.** The annotator here is also the calibrator, so blindness has to be a
     mechanism rather than a promise: the presentation order must be a function of the block text
     alone. If it depended on which unit proposed the block, position would leak the answer.

Run: make test
"""

from __future__ import annotations

from pathlib import Path

import pytest
import units


def block(normalized: str, *, kind: str = "docstring", path: str = "a.py", line: int = 1) -> units.Block:
    """Build a block whose word count is the number of tokens in `normalized`."""
    return units.Block(
        repo="repo", path=path, line=line, end_line=line + 1, kind=kind, normalized=normalized, raw=normalized
    )


def words(count: int, *, first: str = "w") -> str:
    """Return `count` distinct words, so the text is also a usable identity."""
    return " ".join(f"{first}{index}" for index in range(count))


class TestEqualAlertVolume:
    """A unit compared at a different alert volume is compared on loudness, not precision."""

    def test_the_threshold_fires_on_exactly_the_requested_number_of_blocks(self) -> None:
        values = [10, 20, 30, 40, 50]
        threshold = units.threshold_for(values, alerts=2)
        assert [value for value in values if value > threshold] == [40, 50]

    def test_asking_for_more_alerts_than_blocks_aborts(self) -> None:
        with pytest.raises(units.CalibrationError, match="3"):
            units.threshold_for([1, 2, 3], alerts=99)

    def test_a_tie_that_makes_the_exact_count_impossible_aborts(self) -> None:
        """
        Four blocks tied at 50: no threshold fires on exactly two of them.

        The earlier version of this test asserted that the calibrator *reported* 4 and carried on,
        which codified the bug as intent: everything downstream is described as "equal alert volume"
        and would silently have been comparing 4 alerts against 2. An unreachable exact count is a
        calibration that did not happen, so it raises.
        """
        with pytest.raises(units.CalibrationError, match="4"):
            units.threshold_for([50, 50, 50, 50, 10], alerts=2)


class TestPerKind:
    """Docstrings and comments are separate populations and are calibrated separately."""

    def test_calibration_is_computed_per_kind(self) -> None:
        blocks = [
            block(words(300), kind="docstring", path="d1.py"),
            block(words(100), kind="docstring", path="d2.py"),
            block(words(90), kind="comment", path="c1.py"),
            block(words(10), kind="comment", path="c2.py"),
        ]
        calibrated = units.calibrate(blocks, unit=units.count_words, alerts_by_kind={"docstring": 1, "comment": 1})
        assert set(calibrated) == {"docstring", "comment"}
        assert calibrated["docstring"] == 100
        assert calibrated["comment"] == 10

    def test_a_kind_with_no_blocks_is_absent_rather_than_zero(self) -> None:
        """A zero threshold would fire on everything of a kind that was never measured."""
        blocks = [block(words(300), kind="docstring")]
        calibrated = units.calibrate(blocks, unit=units.count_words, alerts_by_kind={"docstring": 1, "comment": 4})
        assert "comment" not in calibrated


class TestDisagreement:
    """Only the blocks the two units decide differently about carry information."""

    def test_blocks_both_units_fire_on_are_not_in_the_disagreement_set(self) -> None:
        agreed = block(words(300), path="both.py")
        only_a = block(words(200), path="a_only.py")
        assert units.disagreement({agreed, only_a}, {agreed}) == {only_a}

    def test_the_set_is_symmetric(self) -> None:
        left, right = block(words(10), path="l.py"), block(words(10), path="r.py")
        assert units.disagreement({left}, {right}) == {left, right}


class TestBlindOrder:
    """Blindness is a mechanism: order must be a function of the text and of nothing else."""

    def test_the_order_does_not_change_when_the_input_order_does(self) -> None:
        blocks = [block(words(5, first=letter), path=f"{letter}.py") for letter in "abcdef"]
        assert units.blind_order(blocks) == units.blind_order(list(reversed(blocks)))

    def test_the_order_is_not_the_input_order(self) -> None:
        """If the hash order coincided with insertion order the mechanism would prove nothing."""
        blocks = [block(words(5, first=letter), path=f"{letter}.py") for letter in "abcdef"]
        assert units.blind_order(blocks) != blocks

    def test_identical_prose_at_two_addresses_gets_two_distinct_keys(self) -> None:
        """
        The property that makes an address tie-break unnecessary, tested instead of it.

        This replaces `test_two_blocks_with_the_same_text_sort_by_address_not_by_arrival`, which
        **could not fail**: `digest` already hashes path and line, so the two fixtures never tied on
        the primary key and the tie-break was never reached — deleting it left the test green. The
        tie-break is gone; what is asserted now is why it was never needed, and this one *can* fail:
        narrow `digest` to the text alone and identical prose collides, at which point `blind_order`
        falls back to arrival order and the blinding leaks.
        """
        here, there = block(words(5), path="a.py", line=1), block(words(5), path="z.py", line=9)
        assert here.normalized == there.normalized
        assert here.digest != there.digest
        assert units.blind_order([there, here]) == units.blind_order([here, there])


class TestTheLabelRoundTrip:
    """`--emit` prints an identifier and `--join` looks one up; they must be the same identifier."""

    def test_the_emitted_identifier_is_the_key_join_looks_up(self) -> None:
        one, two = block(words(9), path="a.py"), block(words(9), path="b.py")
        emitted = [block.label for block in units.blind_order([one, two])]
        verdicts = dict.fromkeys(emitted, "yes")

        table = units.precision_table({one, two}, {"words": {one}}, verdicts)

        assert table["words"] == (1, 1)

    def test_only_the_labelled_blocks_count_as_true(self) -> None:
        one, two = block(words(9), path="a.py"), block(words(9), path="b.py")
        verdicts = {one.label: "yes", two.label: "no"}

        table = units.precision_table({one, two}, {"words": {one, two}}, verdicts)

        assert table["words"] == (2, 1)


class TestTheJoinFailsClosed:
    """A label set that does not describe the disagreement set is an error, never a silent zero."""

    def test_a_missing_label_is_named_and_fatal(self) -> None:
        one, two = block(words(9), path="a.py"), block(words(9), path="b.py")
        with pytest.raises(units.LabelError, match=two.label):
            units.precision_table({one, two}, {"words": {one}}, {one.label: "yes"})

    def test_an_extra_label_is_named_and_fatal(self) -> None:
        """A digest that matches nothing is a typo in the label file, or a stale label set."""
        one = block(words(9), path="a.py")
        with pytest.raises(units.LabelError, match="deadbeefdead"):
            units.precision_table({one}, {"words": {one}}, {one.label: "yes", "deadbeefdead": "no"})

    def test_a_verdict_that_is_neither_yes_nor_no_is_named_and_fatal(self) -> None:
        """`yes`/`no` are the protocol; anything else silently counted as `no` moves a number."""
        one = block(words(9), path="a.py")
        with pytest.raises(units.LabelError, match="maybe"):
            units.precision_table({one}, {"words": {one}}, {one.label: "maybe"})


class TestOneOwnerForTheRunTable:
    """`corpus/run_all.sh` owns the run table; the Python copies may not drift from it."""

    def test_the_run_table_agrees_with_run_all_sh(self) -> None:
        """
        `run_all.sh` is the only place that verifies the pins, the worktrees and the walked counts,
        so every Python restatement of "which root, how many files" has to answer to it. Two owners
        for one fact is the defect this epic keeps paying for; this is the machine check that keeps
        it to one owner plus mirrors.

        The row shape is pinned by its separator count, and that is load-bearing rather than
        incidental: the columns are
        `name|root|checkout|exit|files|skipped|TPX001|TPX002|TPX003|near|exact`, so **ten** pipes.
        It was seven until `make-check-graceful-on-unreadable-files` added the `skipped` column, and
        eight until `close-anti-fp-gate-with-public-reference` added the near/exact split — and each
        time the stale count made this test fail loudly with an empty parse instead of silently
        agreeing with nothing, which is exactly what the `assert table` line below is for. Keep
        both: a parser that quietly matches zero rows would turn this whole check into a no-op.
        """
        import bench

        script = (Path(__file__).resolve().parents[2] / "corpus/run_all.sh").read_text()
        table = {
            row[0]: (row[1], int(row[4]))
            for row in (
                line.strip().strip('"').split("|")
                for line in script.splitlines()
                if line.strip().startswith('"') and line.count("|") == 10
            )
        }
        assert table, "the EXPECTED table in run_all.sh could not be parsed"

        for name, (root, files) in units.RUNS.items():
            assert table[name] == (root, files), f"units.RUNS disagrees with run_all.sh for {name}"
        for name, root in bench.ROOTS:
            assert table[name][0] == root, f"bench.ROOTS disagrees with run_all.sh for {name}"


class TestThePopulationIsPinned:
    """`--verify` compares finding addresses, which leaves 97% of the blocks unconstrained."""

    def test_a_walk_that_lost_a_finding_free_file_is_caught(self) -> None:
        """
        Executed by review: dropping `requests/src/requests/models.py` (44 blocks, no finding) took
        the population from 12 113 to 12 069 while `--verify` still printed "183 volume findings
        reproduced exactly" and exited 0 — and every calibrated threshold and the whole 58-block
        disagreement set are computed over that population.

        The file count is not a new number: it is the column `run_all.sh` already asserts.
        """
        with pytest.raises(units.PopulationError, match="requests"):
            units.check_population({"requests": 36})

    def test_the_recorded_counts_pass(self) -> None:
        counts = {name: files for name, (_, files) in units.RUNS.items()}
        units.check_population(counts)


class TestCalibrationTolerance:
    """
    An exact match is required; a drift of one alert is allowed only when ties make exact impossible.

    Words fires on 165 docstring blocks in the corpus, and `tokens_raw` cannot fire on exactly 165 of
    the 7 980 docstring blocks — the achievable counts step 164 -> 166. Refusing outright would drop a
    unit the AC names; drifting silently is the defect review already closed. So the drift is capped
    at one alert, chosen deterministically (nearest, ties toward FEWER alerts, so no unit is handed
    more chances than words had), and the table prints the count actually measured, not the one
    requested.
    """

    def test_an_exact_match_is_preferred_over_a_tolerated_one(self) -> None:
        blocks = [block(words(size), path=f"{size}.py") for size in (10, 20, 30, 40, 50)]
        assert units.calibrate(blocks, units.count_words, {"docstring": 2}) == {"docstring": 30}

    def test_a_one_alert_drift_is_tolerated_when_ties_make_exact_impossible(self) -> None:
        """Counts reachable on 10/20/20/30 are {0, 1, 3, 4}; 2 is not, and 1 is the nearest below."""
        blocks = [block(words(size), path=f"{index}.py") for index, size in enumerate((10, 20, 20, 30))]
        assert units.calibrate(blocks, units.count_words, {"docstring": 2}) == {"docstring": 20}

    def test_a_drift_larger_than_the_cap_still_aborts(self) -> None:
        """Four blocks tied at 50: the nearest reachable counts to 2 are 0 and 4, a drift of two."""
        blocks = [block(words(50), path=f"{index}.py") for index in range(4)]
        blocks.append(block(words(10), path="small.py"))
        with pytest.raises(units.CalibrationError):
            units.calibrate(blocks, units.count_words, {"docstring": 2})


class TestTheEntryPointRefusesWithoutASubject:
    """
    `main`'s one pre-flight refusal, and the only part of it reachable without the checkouts.

    Everything below this line in `main` needs the compiled `tooprolix` extension and 773 MB of
    corpus; this check needs neither, and it is what stops a comparison being run against whatever
    directory the shell happened to be in.
    """

    def test_an_unset_corpus_root_exits_two_before_anything_is_loaded(
        self, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
    ) -> None:
        monkeypatch.delenv("CORPUS_ROOT", raising=False)

        assert units.main([]) == 2
        captured = capsys.readouterr()
        assert "CORPUS_ROOT" in captured.err
        assert captured.out == "", "a calibration table was printed for a run with no population"
