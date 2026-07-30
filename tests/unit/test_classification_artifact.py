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


def _artifact_json() -> dict[str, Any]:
    return json.loads(ARTIFACT.read_text(encoding="utf-8"))


def _write(tmp_path: Path, payload: dict[str, Any]) -> Path:
    path = tmp_path / "artifact.json"
    path.write_text(json.dumps(payload), encoding="utf-8")
    return path


class TestTheShippedArtifactDescribesTheShippedRuns:
    """The committed dry run must verify against the committed runs, or it is a transcript."""

    def test_the_dry_run_artifact_verifies_against_corpus_runs(self) -> None:
        classification.verify(classification.load(ARTIFACT), RUNS)

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
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS)

    def test_a_record_that_misstates_the_similarity_it_classified_is_fatal(self, tmp_path: Path) -> None:
        """A near/exact split the run does not support would silently move the AC8 numbers."""
        payload = _artifact_json()
        payload["records"][0]["weakest_similarity"] = 0.123

        with pytest.raises(classification.ClassificationError, match=payload["records"][0]["path"]):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS)

    def test_a_record_that_misstates_its_members_is_fatal(self, tmp_path: Path) -> None:
        """
        The members are the finding. A record naming a different member set has classified
        something the run never emitted, and its verdict cannot be re-checked by a reader.
        """
        payload = _artifact_json()
        payload["records"][0]["members"] = ["nowhere/at/all.py:1-2"]

        with pytest.raises(classification.ClassificationError, match=payload["records"][0]["path"]):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS)


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
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS)

    def test_a_row_that_no_finding_backs_is_fatal(self, tmp_path: Path) -> None:
        payload = _artifact_json()
        invented = dict(payload["records"][0])
        invented["path"] = "invented/module.py"
        payload["records"].append(invented)

        with pytest.raises(classification.ClassificationError, match="invented/module.py"):
            classification.verify(classification.load(_write(tmp_path, payload)), RUNS)


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
