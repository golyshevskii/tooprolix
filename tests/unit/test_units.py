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

import importlib.abc
import importlib.machinery
import sys
import types
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


def run_all_table() -> dict[str, tuple[str, int]]:
    """
    Return `{run: (walk root, walked .py files)}` parsed out of `corpus/run_all.sh`.

    `run_all.sh` is the only place that verifies the pins, the worktrees and the walked counts, so
    it is the independent source of truth for both users below: the drift assertion, and the
    population that `check_population` must accept.

    The row shape is pinned by its separator count, and that is load-bearing rather than
    incidental: the columns are
    `name|root|checkout|exit|files|skipped|TPX001|TPX002|TPX003|near|exact`, so **ten** pipes. It
    was seven until `make-check-graceful-on-unreadable-files` added the `skipped` column, and eight
    until `close-anti-fp-gate-with-public-reference` added the near/exact split — and each time the
    stale count made the caller fail loudly with an empty parse instead of silently agreeing with
    nothing, which is what each caller's `assert table` is for. A parser that quietly matched zero
    rows would turn both checks into no-ops.
    """
    script = (Path(__file__).resolve().parents[2] / "corpus/run_all.sh").read_text()
    return {
        row[0]: (row[1], int(row[4]))
        for row in (
            line.strip().strip('"').split("|")
            for line in script.splitlines()
            if line.strip().startswith('"') and line.count("|") == 10
        )
    }


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
        """
        import bench

        table = run_all_table()
        assert table, "the EXPECTED table in run_all.sh could not be parsed"

        for name, (root, files) in units.RUNS.items():
            assert table[name] == (root, files), f"units.RUNS disagrees with run_all.sh for {name}"
        for name, root in bench.ROOTS:
            assert table[name][0] == root, f"bench.ROOTS disagrees with run_all.sh for {name}"

    def test_every_run_all_sh_row_is_either_measured_or_excluded_on_the_record(self) -> None:
        """
        The drift check above runs one way only, and one way is not enough.

        Everything else here iterates `units.RUNS`, so a row that exists in `run_all.sh` and is
        **absent** from `RUNS` is invisible — and `load_blocks` independently skips run JSON whose
        stem is not in `RUNS`, so the run would vanish from every threshold, every disagreement set
        and every published number with the whole suite green. Measured 2026-08-01 at `9c660c5`:
        deleting the `pydantic` row from `RUNS` left **326 passed, exit 0**.

        Equality against `RUNS` alone would be wrong today: `crewAI-full` is a real `run_all.sh` row
        that is deliberately not in the population, and the reason is already recorded on
        `units.EXCLUDED_RUNS` — the same checkout as `crewAI` at a root that cannot be measured at
        all (exit 2, five unparsable Jinja templates; see `corpus/run_all.sh`). So the rule is
        "measured, or excluded **on the record**", which fails loudly for a genuinely new row while
        the known alternative measurement stays out with its reason attached.
        """
        table = run_all_table()
        assert table, "the EXPECTED table in run_all.sh could not be parsed"

        assert set(table) == set(units.RUNS) | units.EXCLUDED_RUNS


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

    def test_the_population_run_all_sh_recorded_is_accepted(self) -> None:
        """
        The positive control, with an input that does not come from `units.RUNS`.

        It used to build `counts` **from** `units.RUNS` and check it **against** `units.RUNS`, so it
        passed by construction whatever `RUNS` said — satisfied even by a `check_population` that
        returns unconditionally. Reading the counts out of `corpus/run_all.sh` instead makes it able
        to fail: it is now the guard that a `check_population` which rejects a correct population
        cannot ship, and the negative case below is what catches one that accepts everything.
        """
        table = run_all_table()
        assert table, "the EXPECTED table in run_all.sh could not be parsed"

        units.check_population({name: files for name, (_, files) in table.items()})


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


class TestAStaleExtensionMayNotProduceNumbers:
    """
    `import tooprolix` succeeding is itself the defect, not the happy path.

    The pyo3 boundary is gone — `pyproject.toml` ships `bindings = "bin"` and
    `scripts/install-smoke.sh` asserts that `import tooprolix` MUST raise — so the only module
    that name can resolve to on any machine is a **stale pre-removal extension**. Left to import,
    it answers `prose_blocks` out of an older extractor and every threshold, disagreement set and
    precision number below it would describe code that no longer ships, printed as current.

    That is a guard failing open: the run cannot tell that it measured the wrong thing, and
    `--verify` cannot catch it either, because a consistent old extractor reproduces its own old
    findings. So the import is refused rather than trusted.
    """

    def _stale(self, monkeypatch: pytest.MonkeyPatch) -> list[str]:
        """Put a fake pre-removal extension on the import path and record what it is asked for."""
        consulted: list[str] = []
        extension = types.ModuleType("tooprolix")
        extension.__file__ = "/stale/tooprolix.so"

        def prose_blocks(path: str, source: str) -> list[tuple[str, int, int, str]]:
            """Answer in the shape the removed export had: `(kind, start, end, normalised)`."""
            consulted.append(path)
            return [("docstring", 1, 2, "stale prose from an extractor that no longer ships")]

        extension.prose_blocks = prose_blocks  # ty: ignore[unresolved-attribute]
        monkeypatch.setitem(sys.modules, "tooprolix", extension)
        return consulted

    def test_an_importable_tooprolix_is_refused_instead_of_measured(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        """Without the refusal this returns blocks — old numbers, reported as today's."""
        consulted = self._stale(monkeypatch)
        monkeypatch.setattr(units, "RUNS", {"r": ("r", 1)})
        monkeypatch.setattr(units, "walked_files", lambda corpus_root, root: ["r/a.py"])
        (tmp_path / "r").mkdir()
        (tmp_path / "r/a.py").write_text('"""d"""\n', encoding="utf-8")
        runs = tmp_path / "runs"
        runs.mkdir()
        (runs / "r.json").write_text("{}", encoding="utf-8")

        with pytest.raises(units.StaleExtensionError):
            units.load_blocks(tmp_path, runs)

        assert consulted == [], "the stale extractor was asked for blocks before the run was refused"

    def test_the_refusal_names_the_file_to_delete(self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
        """A machine-level residue needs the path, or the reader cannot act on the message."""
        self._stale(monkeypatch)
        runs = tmp_path / "runs"
        runs.mkdir()

        with pytest.raises(units.StaleExtensionError, match="/stale/tooprolix.so"):
            units.load_blocks(tmp_path, runs)

    def test_a_module_that_only_appears_at_import_time_is_still_refused(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        """
        The guard must grade the **import**, not a prediction about the import.

        `importlib.util.find_spec` answers "what would an import find?" — a forecast. The import is
        the fact, and the two can disagree: a meta-path finder that returns `None` once and a real
        spec afterwards slips a stale extension past a `find_spec` guard and hands it to the code
        underneath. That is recurring defect #6, a validator grading a self-report, one layer down.

        Measured 2026-08-01 at `9c660c5` with exactly this finder: the name was resolved **twice**
        and `load_blocks` returned **1 block** reading `STALE OUTPUT`. Not exotic — any lazy or
        caching importer is stateful.

        Asking once is the fix, so asking once is what is asserted. Whichever answer that single
        question gets is then the fact the run acts on: `None` means `ModuleNotFoundError` and
        nothing is measured, a spec means `StaleExtensionError` and nothing is measured. There is
        no third outcome for a stateful finder to steer the run into, which is the property a
        predict-then-import pair cannot have.
        """
        consulted: list[str] = []

        class Loader(importlib.abc.Loader):
            def create_module(self, spec: importlib.machinery.ModuleSpec) -> types.ModuleType:
                module = types.ModuleType("tooprolix")
                module.__file__ = "/stale/tooprolix.so"
                module.prose_blocks = lambda path, source: (  # ty: ignore[unresolved-attribute]
                    consulted.append(path),
                    [("docstring", 1, 2, "STALE OUTPUT")],
                )[1]
                return module

            def exec_module(self, module: types.ModuleType) -> None:
                """Nothing to execute: `create_module` already produced the whole fake."""

        class AnswersDifferentlyTheSecondTime(importlib.abc.MetaPathFinder):
            """`None` for the first question asked, a real spec for every one after it."""

            asked = 0

            def find_spec(
                self, name: str, path: object = None, target: object = None
            ) -> importlib.machinery.ModuleSpec | None:
                if name != "tooprolix":
                    return None
                type(self).asked += 1
                if type(self).asked == 1:
                    return None
                return importlib.machinery.ModuleSpec("tooprolix", Loader(), origin="/stale/tooprolix.so")

        finder = AnswersDifferentlyTheSecondTime()
        AnswersDifferentlyTheSecondTime.asked = 0
        monkeypatch.setattr(sys, "meta_path", [finder, *sys.meta_path])
        monkeypatch.setattr(units, "RUNS", {"r": ("r", 1)})
        monkeypatch.setattr(units, "walked_files", lambda corpus_root, root: ["r/a.py"])
        (tmp_path / "r").mkdir()
        (tmp_path / "r/a.py").write_text('"""d"""\n', encoding="utf-8")
        runs = tmp_path / "runs"
        runs.mkdir()
        (runs / "r.json").write_text("{}", encoding="utf-8")

        with pytest.raises((units.StaleExtensionError, ModuleNotFoundError)):
            units.load_blocks(tmp_path, runs)

        assert AnswersDifferentlyTheSecondTime.asked == 1, "the name was resolved more than once"
        assert consulted == [], "the stale extractor produced blocks before the run was refused"

    def test_a_module_with_no_file_is_reported_for_what_it_is(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        """
        Not everything that resolves is a compiled left-over, and the message may not pretend it is.

        The repository directory is itself named `tooprolix`, so putting its parent on `sys.path`
        makes the name resolve as a **namespace package** — legitimate, and carrying no `__file__`.
        Telling that reader to delete a stale `.so` sends them hunting a file that does not exist.
        Measured 2026-08-01 at `9c660c5`: the message read `resolved to <unknown location>`.

        Refusing is still right — a namespace package has no `prose_blocks` either — but the
        refusal has to name what actually resolved.
        """
        package = types.ModuleType("tooprolix")
        package.__path__ = ["/somewhere/on/sys/path/tooprolix"]
        monkeypatch.setitem(sys.modules, "tooprolix", package)
        runs = tmp_path / "runs"
        runs.mkdir()

        with pytest.raises(units.StaleExtensionError, match=r"/somewhere/on/sys/path/tooprolix"):
            units.load_blocks(tmp_path, runs)

    def test_the_entry_point_turns_the_refusal_into_an_exit_code(
        self, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str], tmp_path: Path
    ) -> None:
        """
        An actionable message the CLI does not deliver is not delivered.

        `main` caught `PopulationError` alone, so the refusal escaped as a traceback — the same
        thing a reader saw before the guard existed, only longer. A stack trace is not a diagnosis.
        """
        self._stale(monkeypatch)
        monkeypatch.setenv("CORPUS_ROOT", str(tmp_path))
        runs = tmp_path / "runs"
        runs.mkdir()

        code = units.main(["--verify", "--runs", str(runs)])

        assert code == 1
        captured = capsys.readouterr()
        assert "/stale/tooprolix.so" in captured.err
        assert captured.out == "", "a table was printed for a run that never loaded a block"

    def test_with_no_extension_present_the_honest_failure_survives(self, tmp_path: Path) -> None:
        """
        The guard must not mask the ordinary dead state, which is what the module docstring documents.

        `ModuleNotFoundError` here is the script's real condition on a clean machine; turning it
        into the stale-extension error would send a reader hunting a file that is not there.
        """
        runs = tmp_path / "runs"
        runs.mkdir()

        with pytest.raises(ModuleNotFoundError):
            units.load_blocks(tmp_path, runs)


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
