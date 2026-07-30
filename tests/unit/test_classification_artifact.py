"""
Guards for `corpus/classification.py` — the AC5 verification mechanism of
`close-anti-fp-gate-with-public-reference`.

AC5 asks for a baseline that is "reproducible (`--verify`-mechanism **or an analogue**) and carries
the **class** of every finding, not only its address". `tooprolix` has no `--verify` and no baseline
feature, so the analogue lives here, on the corpus side: a classification artifact plus a checker
that grades the artifact **against the run it claims to describe**.

Three properties are what make it an auditable artifact rather than a self-report, and this file is
one class per property:

  1. **The artifact is graded against external bytes.** The run JSON and its SHA-256 are computed
     from the file on disk, never read back out of a field the artifact wrote about itself. This is
     the epic's recurring defect #6 — "a validator that grades a self-report is not a validator" —
     at the annotation layer, and it is the one the artifact exists to avoid.
  2. **Coverage is exact in both directions.** A missing row and an extra row are both fatal and
     both name the offending address. A checker that only verified "every row points at a real
     finding" would pass an artifact that simply omitted every finding the annotator disliked,
     which is Decisions #17's baseline-as-input loophole with the baseline renamed.
  3. **There are exactly two classes.** Decisions #17 closed the third-class loophole by name:
     `intentional` is an *attribute* of a false positive, never a class of its own. A record whose
     class is anything but `TP` or `FP` must be rejected at parse time, and a `TP` with no named
     fix must be rejected too — Decisions #16 requires the annotation to name the proposed fix, so
     "these words are similar" cannot enter the artifact at all.

Run: make test
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import classification
import pytest

#: The dry-run artifact this repository ships, and the runs it describes.
CORPUS = Path(__file__).resolve().parents[2] / "corpus"
ARTIFACT = CORPUS / "dry_run_classification.json"
RUNS = CORPUS / "runs"
PREREGISTRATION = CORPUS / "preregistration.json"


def profile(name: str = "dry_run") -> classification.Profile:
    """Return the pre-registered profile a caller grades against — never read out of the artifact."""
    return classification.load_preregistration(PREREGISTRATION).profile(name)


def _artifact_json() -> dict[str, Any]:
    return json.loads(ARTIFACT.read_text(encoding="utf-8"))


def _write(tmp_path: Path, payload: dict[str, Any]) -> Path:
    path = tmp_path / "artifact.json"
    path.write_text(json.dumps(payload), encoding="utf-8")
    return path


class TestTheShippedArtifactDescribesTheShippedRuns:
    """The committed dry run must verify against the committed runs, or it is a transcript."""

    def test_the_dry_run_artifact_verifies_against_corpus_runs(self) -> None:
        classification.verify(classification.load(ARTIFACT), RUNS, profile())

    def test_it_covers_thirty_clusters_split_twenty_exact_and_ten_near(self) -> None:
        """
        The pre-registered shape of the dry run, asserted as a number rather than described.

        20 exact / 10 near is not arbitrary: exact is 457 of the corpus's 617 `TPX003` clusters at
        `v0.4.0` and its precision had never been measured, so the ratio is deliberately weighted
        towards the half of the population no number in this epic covers.
        """
        artifact = classification.load(ARTIFACT)

        exact = [record for record in artifact.records if record.population == "exact"]
        near = [record for record in artifact.records if record.population == "near"]

        assert (len(exact), len(near)) == (20, 10)


class TestTheArtifactIsGradedAgainstTheRunAndNotAgainstItself:
    """Every claim the artifact makes about a finding is checked against the run's own bytes."""

    def test_a_run_whose_bytes_changed_is_fatal(self, tmp_path: Path) -> None:
        """
        The hash is the link between the classification and the population it classified.

        Without it, regenerating `corpus/runs/` under a different detector would leave the artifact
        looking valid while describing findings that no longer exist — exactly the trap that made
        `corpus/runs/` cite a dead marker grammar for two releases (STATE.md, task 5).
        """
        payload = _artifact_json()
        payload["runs"][0]["sha256"] = "0" * 64

        with pytest.raises(classification.ClassificationError, match=payload["runs"][0]["name"]):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS, profile())

    def test_a_record_that_misstates_the_similarity_it_classified_is_fatal(self, tmp_path: Path) -> None:
        """A near/exact split the run does not support would silently move the AC8 numbers."""
        payload = _artifact_json()
        payload["records"][0]["weakest_similarity"] = 0.123

        with pytest.raises(classification.ClassificationError, match=payload["records"][0]["path"]):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS, profile())

    def test_a_record_that_misstates_its_members_is_fatal(self, tmp_path: Path) -> None:
        """
        The members are the finding. A record naming a different member set has classified
        something the run never emitted, and its verdict cannot be re-checked by a reader.
        """
        payload = _artifact_json()
        payload["records"][0]["members"] = ["nowhere/at/all.py:1-2"]

        with pytest.raises(classification.ClassificationError, match=payload["records"][0]["path"]):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS, profile())


class TestCoverageIsExactInBothDirections:
    """A row may not go missing and a row may not be invented."""

    def test_a_deleted_row_is_fatal_and_names_the_address_it_lost(self, tmp_path: Path) -> None:
        """
        The mutation the task asks for, kept as a permanent guard rather than a one-off transcript.

        An artifact that may quietly cover fewer findings than the sample it declares is a baseline
        taken *before* annotation wearing a different name — Decisions #17's "baseline is an output,
        not an input".
        """
        payload = _artifact_json()
        dropped = payload["records"].pop(0)

        with pytest.raises(classification.ClassificationError, match=dropped["path"]):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS, profile())

    def test_a_row_that_no_finding_backs_is_fatal(self, tmp_path: Path) -> None:
        payload = _artifact_json()
        invented = dict(payload["records"][0])
        invented["path"] = "invented/module.py"
        payload["records"].append(invented)

        with pytest.raises(classification.ClassificationError, match="invented/module.py"):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS, profile())


class TestThereAreExactlyTwoClasses:
    """Decisions #17, enforced by the parser rather than promised by the prose."""

    def test_intentional_is_rejected_as_a_class(self, tmp_path: Path) -> None:
        """
        `reason=intentional` is an attribute of an FP. As a *class* it removes the finding from the
        numerator, which turns any failed gate into a pass by renaming — the `exit 0` loophole of
        the task's first edition under a new name.
        """
        payload = _artifact_json()
        payload["records"][0]["classification"] = "intentional"

        with pytest.raises(classification.ClassificationError, match="intentional"):
            classification.load(_write(tmp_path, payload))

    def test_a_true_positive_with_no_named_fix_is_rejected(self, tmp_path: Path) -> None:
        """Decisions #16: "these words are similar" is not a basis; the annotation names the fix."""
        payload = _artifact_json()
        true_positive = next(r for r in payload["records"] if r["classification"] == "TP")
        true_positive["proposed_fix"] = ""

        with pytest.raises(classification.ClassificationError, match="proposed_fix"):
            classification.load(_write(tmp_path, payload))

    def test_a_false_positive_with_no_named_shape_is_rejected(self, tmp_path: Path) -> None:
        """An unshaped FP is an unexplained one, and the FP shapes are what the gate reports."""
        payload = _artifact_json()
        false_positive = next(r for r in payload["records"] if r["classification"] == "FP")
        false_positive["shape"] = ""

        with pytest.raises(classification.ClassificationError, match="shape"):
            classification.load(_write(tmp_path, payload))

    def test_an_intentional_attribute_keeps_the_record_a_false_positive(self, tmp_path: Path) -> None:
        """
        The other half of Decisions #17, and the one a guard could get backwards: carrying
        `intentional` must be *allowed* — it is the attribute the decision explicitly permits — as
        long as the record's class stays FP and it therefore stays in the numerator.
        """
        payload = _artifact_json()
        false_positive = next(r for r in payload["records"] if r["classification"] == "FP")
        false_positive["attributes"]["intentional"] = True

        artifact = classification.load(_write(tmp_path, payload))

        assert artifact.false_positive_count("all") == classification.load(ARTIFACT).false_positive_count("all")


class TestTheRatesAreComputedFromTheRecordsAndNotStored:
    """A stored rate is a self-report; the three AC8 numbers are derived on every read."""

    @pytest.mark.parametrize(("population", "expected"), [("exact", 20), ("near", 10), ("all", 30)])
    def test_the_denominator_is_the_clusters_emitted_in_that_population(self, population: str, expected: int) -> None:
        assert classification.load(ARTIFACT).denominator(population) == expected

    def test_an_unknown_population_is_fatal_rather_than_an_empty_denominator(self) -> None:
        """Guards fail closed: a typo'd population must not divide by an empty set and report 0.0."""
        with pytest.raises(ValueError, match="sideways"):
            classification.load(ARTIFACT).denominator("sideways")


class TestTheArtifactCannotDefineWhatItIsGradedAgainst:
    """
    Review round 1, finding B1 — the single most important guard in this file.

    `verify` used to re-draw the expected population from the **artifact's own** `draws`, so the
    thing being graded chose its own denominator. Measured: an artifact declaring
    `{"limit": 0}` with `records: []` verified clean and reported `unavailable`, i.e. the gate
    cleared with nothing labelled at all. That is this epic's defect #6 — a validator grading a
    self-report — at its seventh layer.
    """

    def test_an_artifact_that_declares_its_own_draws_is_refused(self, tmp_path: Path) -> None:
        payload = _artifact_json()
        payload["draws"] = [{"population": "exact", "per_repo": 20, "limit": 0}]

        with pytest.raises(classification.ClassificationError, match="own draws"):
            classification.load(_write(tmp_path, payload))

    def test_labelling_nothing_is_fatal_rather_than_unavailable(self, tmp_path: Path) -> None:
        """The exploit's payload: zero records against a pre-registration that draws 30."""
        payload = _artifact_json()
        payload["records"] = []

        with pytest.raises(classification.ClassificationError, match="carry no class"):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS, profile())

    def test_a_draw_that_selects_nothing_is_fatal_rather_than_an_empty_population(self, tmp_path: Path) -> None:
        """
        Found by mutation: disabling the empty-population guard left every test green.

        `test_labelling_nothing_is_fatal…` above cannot reach it — there the *expected* set is still
        30, so the coverage check fires first — and neither can `verify`, which hashes the run files
        before it draws. So the draw is exercised directly.

        ⚠️ **Why the guard exists even though `round_robin` also refuses a short draw.** That refusal
        is driven by the pre-registered `minimum`, so a profile registering `minimum: 0` slips past
        it. This is the backstop for exactly that case, and "the gate passed because it measured
        nothing" is the outcome it exists to prevent.
        """
        empty_runs = tmp_path / "runs"
        empty_runs.mkdir()
        permissive = classification.Profile(
            name="probe",
            purpose="a draw that asks for nothing and is allowed to get nothing",
            is_gate=False,
            threshold=None,
            draws=(classification.Draw(population="exact", per_repo=20, limit=20, minimum=0),),
            runs={},
        )

        with pytest.raises(classification.ClassificationError, match="drew no findings"):
            classification._expected(permissive, empty_runs)

    def test_the_profile_comes_from_the_caller_and_an_unknown_one_fails_closed(self) -> None:
        with pytest.raises(classification.ClassificationError, match="no pre-registered profile"):
            classification.load_preregistration(PREREGISTRATION).profile("whatever-passes")

    def test_a_gate_profile_with_no_numeric_threshold_refuses_to_be_used(self, tmp_path: Path) -> None:
        """
        A gate whose threshold is a placeholder cannot be failed, so offering it would be offering a
        measurement that can only pass. Refused at retrieval rather than at load, so that an
        undecided threshold does not also block a non-gate profile in the same file.

        ⚠️ **The placeholder is CONSTRUCTED here rather than read from the real file.** This test
        used to assert that the live `holdout` profile refuses — which passed only while the owner
        had not yet chosen a number, and went red the moment they did. That is a test pinned to a
        transient state of the project instead of to the guarantee, and the guarantee is what has to
        survive: *any* gate profile without a numeric threshold refuses, whatever the real file says
        today.
        """
        payload = json.loads(PREREGISTRATION.read_text())
        payload["profiles"]["holdout"]["threshold"] = "<<THRESHOLD - not yet set>>"
        placeholder = tmp_path / "preregistration.json"
        placeholder.write_text(json.dumps(payload))

        with pytest.raises(classification.ClassificationError, match="cannot\n?\\s*be failed|gate"):
            classification.load_preregistration(placeholder).profile("holdout")

    def test_the_holdout_threshold_is_frozen_as_a_number_before_the_gate_runs(self) -> None:
        """
        The other half of the guarantee above, and the one that matters now.

        The owner set 0.40 on 2026-07-30 while the holdout was still unseen. Pinning it here means a
        later edit of `preregistration.json` — the file that decides whether publication is allowed —
        cannot quietly move the bar without reddening a named test.
        """
        holdout = classification.load_preregistration(PREREGISTRATION).profile("holdout")

        assert holdout.is_gate is True
        assert holdout.threshold == 0.40


class TestTheRunItselfIsGradedBeforeItsFindingsAreRead:
    """
    Review round 1, finding B3. A hash proves a file did not change; it proves nothing about
    whether the run behind it measured the whole tree.
    """

    def test_a_truncated_walk_is_fatal_even_though_the_run_calls_itself_complete(self, tmp_path: Path) -> None:
        """
        The parent-`.gitignore` trap, which is measured and still live.

        A run under an ancestor `.gitignore` walks ~1 file per repository and still reports
        `complete: true` with an empty `skipped[]` — the finding counts simply come out small. The
        walked-file count is the only signal, it cannot live in the run JSON, and so it is written
        to a sidecar and compared against the count pinned in the pre-registration.
        """
        runs = tmp_path / "runs"
        runs.mkdir()
        for source in RUNS.iterdir():
            (runs / source.name).write_bytes(source.read_bytes())
        (runs / "crewAI.files").write_text("5\n", encoding="utf-8")

        with pytest.raises(classification.ClassificationError, match="the walk saw 5"):
            classification.verify(classification.load(ARTIFACT), runs, profile())

    def test_a_missing_walk_count_is_fatal_rather_than_skipped(self, tmp_path: Path) -> None:
        runs = tmp_path / "runs"
        runs.mkdir()
        for source in RUNS.iterdir():
            (runs / source.name).write_bytes(source.read_bytes())
        (runs / "requests.files").unlink()

        with pytest.raises(classification.ClassificationError, match="size of the walk is unknown"):
            classification.verify(classification.load(ARTIFACT), runs, profile())


class TestDetectorProvenanceIsCheckedAgainstSomethingReal:
    """
    Review round 1, finding B4. `detector_tag` was free text that changed no outcome.

    ⚠️ **Stated rather than overclaimed: none of this proves which binary produced a given JSON.**
    Only re-running it does. What it rules out is a commit that does not exist, a dirty tree that
    does not say so, and a binary on disk that is not the one the artifact names.
    """

    def test_a_commit_that_does_not_resolve_is_fatal(self, tmp_path: Path) -> None:
        payload = _artifact_json()
        payload["detector_commit"] = "0" * 40

        with pytest.raises(classification.ClassificationError, match="does not resolve"):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS, profile())

    def test_a_binary_that_does_not_match_its_recorded_hash_is_fatal(self, tmp_path: Path) -> None:
        payload = _artifact_json()
        payload["binary_sha256"] = "0" * 64
        impostor = tmp_path / "tooprolix"
        impostor.write_bytes(b"not the detector")

        with pytest.raises(classification.ClassificationError, match="hashes"):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS, profile(), binary=impostor)

    def test_the_dirty_flag_must_be_stated_and_not_omitted(self, tmp_path: Path) -> None:
        payload = _artifact_json()
        del payload["detector_dirty"]

        with pytest.raises(classification.ClassificationError, match="detector_dirty"):
            classification.load(_write(tmp_path, payload))


class TestTheBaselineIsDerivedFromTheLabels:
    """
    Review round 1, finding B5. Decisions #17's ordering, made structural instead of described.

    A baseline taken before annotation is a filter that decides what gets annotated. A function
    whose only input is the labelled records cannot be called before they exist, which is the
    difference between a rule and a sentence about a rule.
    """

    def test_the_baseline_is_exactly_the_false_positives(self) -> None:
        artifact = classification.load(ARTIFACT)

        baseline = classification.baseline_from(artifact)

        assert len(baseline) == artifact.false_positive_count("all")
        assert list(baseline) == sorted(baseline), "a baseline must be ordered, or its diff is noise"

    def test_a_true_positive_is_never_suppressed(self) -> None:
        """Suppressing a TP would suppress the tool's own value, so the derivation must exclude it."""
        artifact = classification.load(ARTIFACT)
        true_positives = {r.members[0] for r in artifact.records if r.classification == "TP"}

        assert not (set(classification.baseline_from(artifact)) & true_positives)


class TestTheGateCanActuallyFail:
    """
    The comparison the measurement exists to feed, wired now so that a red outcome is reachable
    before the owner sets the real number.

    A gate nothing compares is a gate that cannot say no, which is the `exit 0` loophole the first
    edition of this task shipped under a different name.
    """

    def _probe(self, tmp_path: Path, threshold: float) -> classification.Profile:
        payload = json.loads(PREREGISTRATION.read_text(encoding="utf-8"))
        probe = json.loads(json.dumps(payload["profiles"]["dry_run"]))
        probe["is_gate"] = True
        probe["threshold"] = threshold
        payload["profiles"]["probe"] = probe
        path = tmp_path / "preregistration.json"
        path.write_text(json.dumps(payload), encoding="utf-8")
        return classification.load_preregistration(path).profile("probe")

    def test_a_threshold_below_the_measured_share_is_red(self, tmp_path: Path) -> None:
        passed, explanation = classification.gate(classification.load(ARTIFACT), self._probe(tmp_path, 0.20))

        assert not passed
        assert "RED" in explanation

    def test_a_threshold_above_the_measured_share_passes(self, tmp_path: Path) -> None:
        passed, explanation = classification.gate(classification.load(ARTIFACT), self._probe(tmp_path, 0.50))

        assert passed
        assert "PASS" in explanation

    def test_an_unevaluated_threshold_is_not_a_pass(self) -> None:
        """`None` must never read as "green"; it reads as "no gate was evaluated"."""
        passed, explanation = classification.gate(classification.load(ARTIFACT), profile())

        assert not passed
        assert "not a pass" in explanation
