"""
Guards for `corpus/sample_clusters.py`, the sampling rule.

Precision over a hand-annotated sample of `TPX003` findings is worth only as much as HOW the sample
is drawn. Three ways of drawing it were measured to be worthless, and this file is one test per way:

  1. **Sampling the whole pool measures a tautology.** 398 of the corpus's 646 clusters have every
     edge at similarity 1.0 — definitionally identical text. Asking "should one of these copies be
     merged" of identical text is not a question about the detector, and the 0.75 near threshold
     the detector was tuned around never enters the answer. The blocking number is precision over
     NEAR clusters only, so an exact cluster must never reach the sample **by default**.

     The anti-false-positive gate is the exception: it measures a *false-positive* share, not
     precision, and its population includes exact clusters — at `v0.4.0` they are 457 of 617. So
     the exact population is reachable, but only when asked for by name.
     `TestTheExactPopulationIsReachable` below pins that it is opt-in, that the near default above
     is untouched, and that an unrecognised population is fatal rather than silently near.
  2. **A global prefix over `(repo, path, line)` never leaves the first repository.** In ASCII
     order the whole prefix lies inside `OpenHands` and reaches neither `langgraph` nor `pydantic`.
     Round-robin by repository is just as deterministic and covers the corpus.
  3. **A sample that depends on input order is a sample that can be tuned.** The order has to be a
     function of the finding addresses, so re-running the CLI cannot change who gets annotated.

The fixtures below are deliberately built so each of those three failures is *distinguishable*:
the exact and near clusters differ only in `weakest.similarity`, and the two repositories are
named so that one sorts entirely before the other.

Run: make test
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import pytest
import sample_clusters


def finding(path: str, line: int, similarity: float, *, code: str = "TPX003") -> dict[str, Any]:
    """One JSON finding as `--format json` renders it, trimmed to the fields the sampler reads."""
    locations = [
        {"path": path, "line": line, "end_line": line + 3, "prose_kind": "docstring"},
        {"path": path, "line": line + 100, "end_line": line + 103, "prose_kind": "docstring"},
    ]
    return {
        "code": code,
        "path": path,
        "line": line,
        "end_line": line + 3,
        "prose_kind": "docstring",
        "message": f"{path}:{line}: {code}",
        "locations": locations,
        "weakest": {"first": locations[0], "second": locations[1], "similarity": similarity},
    }


class TestNearOnly:
    """Only clusters carrying a demonstrably inexact edge belong in the blocking number."""

    def test_a_cluster_whose_weakest_edge_is_exact_is_not_sampled(self) -> None:
        report = {"findings": [finding("a.py", 1, 1.0)]}
        assert sample_clusters.tpx003_clusters("repo", report) == []

    def test_a_cluster_with_an_inexact_edge_is_sampled(self) -> None:
        report = {"findings": [finding("a.py", 1, 0.871)]}
        sampled = sample_clusters.tpx003_clusters("repo", report)
        assert [cluster.weakest for cluster in sampled] == [0.871]

    def test_volume_findings_never_enter_the_pool(self) -> None:
        """`TPX001`/`TPX002` carry no `weakest` at all; a sampler that assumed one would crash."""
        report = {
            "findings": [
                {"code": "TPX002", "path": "a.py", "line": 1, "end_line": 4, "message": "long"},
                finding("b.py", 7, 0.9),
            ]
        }
        sampled = sample_clusters.tpx003_clusters("repo", report)
        assert [cluster.path for cluster in sampled] == ["b.py"]


class TestRoundRobin:
    """Coverage of the corpus, not of whichever repository sorts first."""

    def test_every_repository_is_represented_even_when_one_sorts_entirely_first(self) -> None:
        """A global `(repo, path, line)` prefix of size 2 would return two `AAA` clusters only."""
        pools = {
            "AAA": sample_clusters.tpx003_clusters("AAA", {"findings": [finding("a.py", i, 0.8) for i in (1, 2, 3)]}),
            "zzz": sample_clusters.tpx003_clusters("zzz", {"findings": [finding("z.py", i, 0.8) for i in (1, 2, 3)]}),
        }
        sampled = sample_clusters.round_robin(pools, per_repo=1, minimum=0)
        assert sorted(cluster.repo for cluster in sampled) == ["AAA", "zzz"]

    def test_a_repository_with_fewer_clusters_than_asked_contributes_all_it_has(self) -> None:
        pools = {
            "small": sample_clusters.tpx003_clusters("small", {"findings": [finding("a.py", 1, 0.8)]}),
            "large": sample_clusters.tpx003_clusters(
                "large", {"findings": [finding("b.py", i, 0.8) for i in (1, 2, 3, 4, 5)]}
            ),
        }
        sampled = sample_clusters.round_robin(pools, per_repo=4, minimum=0)
        assert len([c for c in sampled if c.repo == "small"]) == 1
        assert len([c for c in sampled if c.repo == "large"]) == 4


class TestDeterminism:
    """The sample must be a function of the addresses, never of the order they arrived in."""

    @pytest.mark.parametrize("lines", [(1, 2, 3), (3, 1, 2), (2, 3, 1)])
    def test_the_first_two_clusters_are_the_same_whatever_order_the_input_is_in(self, lines: tuple[int, ...]) -> None:
        report = {"findings": [finding("a.py", line, 0.8) for line in lines]}
        sampled = sample_clusters.round_robin(
            {"repo": sample_clusters.tpx003_clusters("repo", report)}, per_repo=2, minimum=0
        )
        assert [cluster.line for cluster in sampled] == [1, 2]

    @pytest.mark.parametrize("arrival", [("z.py", "a.py"), ("a.py", "z.py")], ids=["reversed", "in-order"])
    def test_two_clusters_on_the_same_line_of_different_files_are_ordered_by_path(
        self, arrival: tuple[str, str]
    ) -> None:
        """
        The PATH half of the ordering key, which nothing above could see.

        Every other fixture in this file puts all its findings in one file, so `Cluster.address`
        was compared on its `line` alone: dropping the path from the key left all 218
        tests green while the sample order fell back to the order the findings arrived in — the
        precise failure claim 3 of the module docstring says must be impossible ("a sample that
        depends on input order is a sample that can be tuned"). Same class as the task-6 defect
        where a probe compared an ordering by a field its own rendering omitted.

        Both arrival orders are parametrised because only the reversed one can fail: a stable sort
        on an equal key returns the input untouched, so the in-order case passes under the broken
        key too and would be a test that agrees with the bug half the time.
        """
        report = {"findings": [finding(path, 5, 0.8) for path in arrival]}

        sampled = sample_clusters.tpx003_clusters("repo", report)

        assert [cluster.path for cluster in sampled] == ["a.py", "z.py"]


class TestTheSampleHasAFloor:
    """AC1 requires at least 20 findings; the tool must have a red path when it cannot reach it."""

    def test_a_sample_below_the_floor_is_fatal(self) -> None:
        """
        Pools smaller than `--per-repo` shrink the sample silently, and AC1's "≥ 20" then lives only
        in prose. Moot at today's 224 near clusters — which is exactly when it is cheap to add.
        """
        pools = {"one": sample_clusters.tpx003_clusters("one", {"findings": [finding("a.py", 1, 0.8)]})}
        with pytest.raises(sample_clusters.SampleTooSmall, match="1"):
            sample_clusters.round_robin(pools, per_repo=4, minimum=20)

    def test_a_sample_at_the_floor_passes(self) -> None:
        pools = {
            f"repo{index}": sample_clusters.tpx003_clusters(
                f"repo{index}", {"findings": [finding("a.py", line, 0.8) for line in range(1, 6)]}
            )
            for index in range(4)
        }
        assert len(sample_clusters.round_robin(pools, per_repo=5, minimum=20)) == 20


class TestTheExactPopulationIsReachable:
    """
    AC8 of `close-anti-fp-gate-with-public-reference`: the gate's population **includes exact
    clusters**, and its false-positive share is reported near/exact separately and combined.

    Measured on `corpus/runs/` at `v0.4.0`: **exact is 457 of 617 clusters**, and no number this
    epic owns — 0.867, 0.750, the 0.667–0.867 band — was ever drawn from one. A sampler that can
    only reach the near 160 makes the majority of what a user actually sees unmeasurable, so
    "exact is unreachable" is a defect of the measuring instrument, not a property of the corpus.

    The near-only default of §1 stands and is asserted above: it is AC1's population, and it is a
    *default*, not a ceiling.
    """

    def test_the_exact_population_selects_only_clusters_whose_weakest_edge_is_one(self) -> None:
        report = {"findings": [finding("a.py", 1, 1.0), finding("b.py", 2, 0.8)]}

        sampled = sample_clusters.tpx003_clusters("repo", report, population="exact")

        assert [cluster.path for cluster in sampled] == ["a.py"]

    def test_the_combined_population_selects_both_kinds(self) -> None:
        report = {"findings": [finding("a.py", 1, 1.0), finding("b.py", 2, 0.8)]}

        sampled = sample_clusters.tpx003_clusters("repo", report, population="all")

        assert [(cluster.path, cluster.is_exact) for cluster in sampled] == [("a.py", True), ("b.py", False)]

    def test_an_unknown_population_is_fatal_rather_than_silently_near(self) -> None:
        """
        Guards fail closed. A typo'd population that fell back to the near default would report a
        near-only number under an `exact` heading — the one failure mode AC8 exists to prevent.

        `ty` flags the literal below, correctly: a *type-checked* caller cannot write it. The runtime
        guard is for the callers that are not type-checked — the `--population` argument and the
        `population` field of a classification artifact, both of which arrive as arbitrary strings —
        so the diagnostic is suppressed rather than the test deleted.
        """
        with pytest.raises(ValueError, match="sideways"):
            sample_clusters.tpx003_clusters("repo", {"findings": []}, population="sideways")  # ty: ignore[invalid-argument-type]


class TestTheLimitTruncatesAfterInterleaving:
    """
    The sample size is fixed by pre-registration, so the sampler needs a truncation rule — and
    *where* it truncates is the whole point. Truncating each repository's pool before interleaving
    is the single-repository prefix bug of `TestRoundRobin` under a new name; truncating the
    interleaved sequence keeps every repository represented.
    """

    def test_the_limit_cuts_the_interleaved_sequence_not_each_pool(self) -> None:
        pools = {
            "AAA": sample_clusters.tpx003_clusters("AAA", {"findings": [finding("a.py", i, 0.8) for i in (1, 2, 3)]}),
            "zzz": sample_clusters.tpx003_clusters("zzz", {"findings": [finding("z.py", i, 0.8) for i in (1, 2, 3)]}),
        }

        sampled = sample_clusters.round_robin(pools, per_repo=3, minimum=0, limit=3)

        assert [(cluster.repo, cluster.line) for cluster in sampled] == [("AAA", 1), ("zzz", 1), ("AAA", 2)]

    def test_a_limit_that_cannot_be_filled_is_fatal(self) -> None:
        """The floor is checked on what came out, not on what was asked for."""
        pools = {"one": sample_clusters.tpx003_clusters("one", {"findings": [finding("a.py", 1, 0.8)]})}

        with pytest.raises(sample_clusters.SampleTooSmall, match="1"):
            sample_clusters.round_robin(pools, per_repo=20, minimum=20, limit=20)


class TestTheEntryPointRefusesRatherThanSampleNothing:
    """
    `main`'s two pre-flight refusals, both reachable without a single checkout on disk.

    They are the difference between "no sample was drawn" and "the sample is empty", and the
    sampler's whole output is Markdown that a human then annotates — an empty document is a
    plausible thing to page past. Both are exit 2 ("could not start"), distinct from the exit 1
    `SampleTooSmall` uses for "started, drew too few".
    """

    def test_an_unset_corpus_root_exits_two(
        self, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
    ) -> None:
        monkeypatch.delenv("CORPUS_ROOT", raising=False)

        assert sample_clusters.main([]) == 2
        assert "CORPUS_ROOT" in capsys.readouterr().err

    def test_a_runs_directory_holding_no_near_clusters_exits_two(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """
        An empty pool must name `run_all.sh`, not print a zero-cluster sample.

        This epic's verification policy counts a run over an empty finding set as RED, and a
        sampler that emitted `# AC1 sample — 0 near clusters` would be that run wearing a heading.
        """
        monkeypatch.setenv("CORPUS_ROOT", str(tmp_path))
        empty_runs = tmp_path / "runs"
        empty_runs.mkdir()

        assert sample_clusters.main(["--runs", str(empty_runs)]) == 2
        assert "run_all.sh" in capsys.readouterr().err


class TestAMalformedFindingIsNeverSkipped:
    """
    Review round 1, finding B2 — a guard that a key rename switched off.

    `tpx003_clusters` used to do `if weakest is None: continue`. Measured on a copy of the corpus:
    renaming `weakest` to `weakest_v2` on six findings dropped the sampled population from 618 to
    612 with no error, no warning and no exit code. Every rate computed afterwards would have been
    taken over a quietly smaller denominator — and the numbers would have looked entirely normal.

    Guards fail closed: a similarity finding that cannot state its similarity stops the run.
    """

    def test_a_tpx003_finding_without_a_weakest_is_fatal_and_names_the_address(self) -> None:
        report = {"findings": [finding("a.py", 7, 0.8)]}
        del report["findings"][0]["weakest"]

        with pytest.raises(sample_clusters.MalformedFinding, match="a.py:7"):
            sample_clusters.tpx003_clusters("repo", report, population="all")

    def test_a_renamed_weakest_key_is_fatal_rather_than_a_smaller_population(self) -> None:
        """The exploit verbatim: the key is present under a new name, so `.get` returns nothing."""
        report = {"findings": [finding("a.py", 1, 0.8), finding("b.py", 2, 0.9)]}
        report["findings"][1]["weakest_v2"] = report["findings"][1].pop("weakest")

        with pytest.raises(sample_clusters.MalformedFinding, match="b.py:2"):
            sample_clusters.tpx003_clusters("repo", report, population="all")

    def test_a_volume_finding_is_still_not_in_the_population(self) -> None:
        """The other direction: `TPX001`/`TPX002` legitimately carry no similarity and are not errors."""
        report = {
            "findings": [
                {"code": "TPX002", "path": "a.py", "line": 1, "end_line": 4, "message": "long"},
                finding("b.py", 7, 0.9),
            ]
        }

        assert [c.path for c in sample_clusters.tpx003_clusters("repo", report, population="all")] == ["b.py"]
