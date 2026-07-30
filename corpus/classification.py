"""
The classification artifact of the anti-false-positive gate, and the checker that grades it.

AC5 of `close-anti-fp-gate-with-public-reference` asks for a baseline that is reproducible — "a
`--verify` mechanism **or an analogue**" — and that carries the **class** of every finding rather
than only its address. `tooprolix` has neither a `--verify` flag nor a baseline feature, and adding
one is a linter feature, not a measurement. So the analogue is built here, on the corpus side: a
JSON artifact of one record per classified finding, plus `verify`, which re-reads the run the
artifact claims to describe and fails if the two disagree.

# What makes this an artifact rather than a self-report

Everything `verify` checks comes from **outside** the artifact:

* the run file's SHA-256 is computed from its bytes on disk, never read back out of the artifact;
* the population the artifact must cover is re-drawn by `sample_clusters`, the same deterministic
  sampler that drew it, from those same bytes;
* each record's similarity, near/exact half and member list are compared against the finding the
  run actually emitted.

This is the epic's recurring defect #6 — *a validator that grades a self-report is not a validator*
— at the annotation layer. The one number `verify` cannot check is whether a verdict is *right*;
that is what the named proposed fix in each record is for, and why a `TP` without one is refused.

# Exactly two classes

EPIC.md Decisions #17: a finding that was rejected while its prose stayed in place is a **false
positive**. It may carry `intentional` in `attributes`, but it may not become a third class, because
a third class removes it from the numerator and turns every failed gate into a pass by renaming.
[`Record`] therefore accepts `TP` and `FP` and nothing else, and it refuses a `TP` that names no fix
— Decisions #16's "these words are similar is not a basis" enforced at parse time rather than
promised in prose.

# Stdlib only, and that is a deliberate deviation

`AGENTS.md` asks for Pydantic v2 models for data structures. This module does not use them, for the
same reason `corpus/measure.py`, `corpus/units.py` and `corpus/sample_clusters.py` do not: the whole
corpus tooling is stdlib-only so that a measurement cannot move because a dependency resolved
differently, and `make test` runs `uv run --only-group test pytest`, a group that carries pytest and
nothing else. Adding Pydantic would put a resolver between the artifact and the number it produces.
The conflict is recorded rather than averaged; validation is explicit instead, and every failure
path is tested.

Usage:
    uv run python3 corpus/classification.py --artifact corpus/dry_run_classification.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal, get_args

import sample_clusters
from sample_clusters import POPULATIONS, Cluster, Population

#: The two classes, and there is no third. See the module docstring and EPIC.md Decisions #17.
Classification = Literal["TP", "FP"]

#: Valid `classification` values, derived from the type so the two cannot drift apart.
CLASSIFICATIONS: tuple[str, ...] = get_args(Classification)

#: Populations a rate may be reported for. `all` is the combined figure AC8 asks for alongside the
#: two halves, and it is a *view* over the records, never a third draw.
RATE_POPULATIONS: tuple[str, ...] = (*POPULATIONS,)

#: What a single record may be. `all` is a view and never the half a finding belongs to, so a
#: record carrying it would be counted in `all` twice and in neither half — a silently wrong split.
RECORD_POPULATIONS: tuple[str, ...] = ("near", "exact")

#: 97.5th percentile of the standard normal, i.e. a two-sided 95% interval.
Z_95: float = 1.959963984540054


class ClassificationError(RuntimeError):
    """The artifact does not describe the run it claims to describe, or is not well formed."""


@dataclass(frozen=True)
class RunReference:
    """One run JSON the artifact classifies, pinned by the digest of its bytes."""

    name: str
    sha256: str


@dataclass(frozen=True)
class Draw:
    """One deterministic draw over a run population, in `sample_clusters`' own vocabulary."""

    population: Population
    per_repo: int
    limit: int | None = None


@dataclass(frozen=True)
class Record:
    """
    One classified finding: its address, its class, why, and the fix that class implies.

    Every field here is either checked against the run by [`verify`] — `repo`, `path`, `line`,
    `population`, `weakest_similarity`, `members` — or a human judgement that only a reader can
    check: `classification`, `reason`, `proposed_fix`, `shape`, `attributes`. There is deliberately
    no third kind. A finding's `end_line` used to be stored here and was removed: it duplicates
    `members[0]`, which *is* checked, and an unchecked copy of a checked value is a field that can
    drift while every guard stays green.
    """

    repo: str
    path: str
    line: int
    population: Population
    weakest_similarity: float
    members: tuple[str, ...]
    classification: Classification
    reason: str
    proposed_fix: str = ""
    shape: str = ""
    attributes: Mapping[str, Any] = field(default_factory=dict)

    @property
    def address(self) -> tuple[str, str, int]:
        """The key a record is joined to a finding by: run, path and the finding's own line."""
        return (self.repo, self.path, self.line)


@dataclass(frozen=True)
class Artifact:
    """A whole classification: the detector it was taken on, the runs, the draws, the records."""

    detector_tag: str
    detector_commit: str
    protocol: str
    blind: bool
    runs: tuple[RunReference, ...]
    draws: tuple[Draw, ...]
    records: tuple[Record, ...]

    def _in(self, population: str) -> list[Record]:
        if population not in RATE_POPULATIONS:
            raise ValueError(f"unknown population {population!r}; expected one of {', '.join(RATE_POPULATIONS)}")
        if population == "all":
            return list(self.records)
        return [record for record in self.records if record.population == population]

    def denominator(self, population: str = "all") -> int:
        """Return the gate's primary denominator: `TPX003` clusters emitted in `population`."""
        return len(self._in(population))

    def false_positive_count(self, population: str = "all") -> int:
        """Return the numerator: every record classed `FP`, `intentional` ones included (#17)."""
        return sum(1 for record in self._in(population) if record.classification == "FP")

    def false_positive_rate(self, population: str = "all") -> float | None:
        """`FP / clusters emitted`, or `None` when nothing was emitted — never `0.0`."""
        total = self.denominator(population)
        if total == 0:
            return None
        return self.false_positive_count(population) / total

    def wilson_interval(self, population: str = "all", z: float = Z_95) -> tuple[float, float] | None:
        """Return the score interval of the false-positive share, or `None` on an empty set."""
        total = self.denominator(population)
        if total == 0:
            return None
        share = self.false_positive_count(population) / total
        denominator = 1 + z * z / total
        centre = (share + z * z / (2 * total)) / denominator
        half = z / denominator * math.sqrt(share * (1 - share) / total + z * z / (4 * total * total))
        return (max(0.0, centre - half), min(1.0, centre + half))


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ClassificationError(message)


def _record(raw: Mapping[str, Any]) -> Record:
    """Parse and validate one record. Every refusal names the field and the address."""
    where = f"{raw.get('path', '?')}:{raw.get('line', '?')}"
    classification = raw.get("classification")
    _require(
        classification in CLASSIFICATIONS,
        f"{where}: classification {classification!r} is not one of {', '.join(CLASSIFICATIONS)}; "
        "EPIC.md Decisions #17 allows exactly two classes and `intentional` is an attribute of an FP",
    )
    population = raw.get("population")
    _require(
        population in RECORD_POPULATIONS,
        f"{where}: population {population!r} is not one of {', '.join(RECORD_POPULATIONS)}",
    )
    _require(bool(str(raw.get("reason", "")).strip()), f"{where}: reason is empty")
    if classification == "TP":
        _require(
            bool(str(raw.get("proposed_fix", "")).strip()),
            f"{where}: a TP must name its proposed_fix — EPIC.md Decisions #16 rules out "
            '"these words are similar" as a basis',
        )
        _require(not str(raw.get("shape", "")).strip(), f"{where}: a TP has no false-positive shape")
    else:
        _require(bool(str(raw.get("shape", "")).strip()), f"{where}: an FP must name its shape")
        _require(not str(raw.get("proposed_fix", "")).strip(), f"{where}: an FP has no proposed_fix")
    members = raw.get("members", [])
    _require(len(members) >= 2, f"{where}: a cluster has at least two members, got {len(members)}")
    return Record(
        repo=str(raw["repo"]),
        path=str(raw["path"]),
        line=int(raw["line"]),
        population=str(population),  # ty: ignore[invalid-argument-type]
        weakest_similarity=float(raw["weakest_similarity"]),
        members=tuple(str(member) for member in members),
        classification=str(classification),  # ty: ignore[invalid-argument-type]
        reason=str(raw["reason"]),
        proposed_fix=str(raw.get("proposed_fix", "")),
        shape=str(raw.get("shape", "")),
        attributes=dict(raw.get("attributes", {})),
    )


def load(path: Path) -> Artifact:
    """
    Read and validate a classification artifact.

    # Raises
    `ClassificationError` on anything malformed. There is no lenient path: a record this function
    could not interpret is a record whose class nothing knows, and a guard that skips what it cannot
    read is a guard that a renamed key switches off.
    """
    raw: Mapping[str, Any] = json.loads(path.read_text(encoding="utf-8"))
    _require(bool(raw.get("detector_tag")), f"{path}: detector_tag is required — AC9 names the tag measured on")
    _require(bool(raw.get("runs")), f"{path}: at least one run must be referenced")
    _require(bool(raw.get("draws")), f"{path}: at least one draw must be declared")
    return Artifact(
        detector_tag=str(raw["detector_tag"]),
        detector_commit=str(raw["detector_commit"]),
        protocol=str(raw["protocol"]),
        blind=bool(raw["blind"]),
        runs=tuple(RunReference(name=str(run["name"]), sha256=str(run["sha256"])) for run in raw["runs"]),
        draws=tuple(
            Draw(
                population=str(draw["population"]),  # ty: ignore[invalid-argument-type]
                per_repo=int(draw["per_repo"]),
                limit=None if draw.get("limit") is None else int(draw["limit"]),
            )
            for draw in raw["draws"]
        ),
        records=tuple(_record(record) for record in raw["records"]),
    )


def digest(path: Path) -> str:
    """Return the SHA-256 of a file's bytes, hex-encoded."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _expected(artifact: Artifact, runs_dir: Path) -> dict[tuple[str, str, int], tuple[Cluster, str]]:
    """Re-draw the population the artifact claims to cover, from the runs on disk."""
    expected: dict[tuple[str, str, int], tuple[Cluster, str]] = {}
    for draw in artifact.draws:
        pools = sample_clusters.load_runs(runs_dir, draw.population)
        minimum = draw.limit if draw.limit is not None else 0
        drawn = sample_clusters.round_robin(pools, draw.per_repo, minimum=minimum, limit=draw.limit)
        for cluster in drawn:
            population = "exact" if cluster.is_exact else "near"
            expected[(cluster.repo, cluster.path, cluster.line)] = (cluster, population)
    return expected


def verify(artifact: Artifact, runs_dir: Path) -> None:
    """
    Fail unless the artifact describes exactly the findings the runs on disk emit.

    # Raises
    `ClassificationError`, naming the run or the address that disagrees. Coverage is checked in
    **both** directions on purpose: a checker that only asked "does every row point at a real
    finding" would pass an artifact that simply omitted the findings its author disliked, which is
    Decisions #17's "baseline is an output, not an input" defeated by omission.
    """
    for run in artifact.runs:
        path = runs_dir / f"{run.name}.json"
        _require(path.exists(), f"{run.name}: {path} is missing; the artifact classifies a run that is not here")
        actual = digest(path)
        _require(
            actual == run.sha256,
            f"{run.name}: {path} hashes {actual}, the artifact pins {run.sha256}. The classification "
            "describes a different set of findings from the one on disk.",
        )

    expected = _expected(artifact, runs_dir)
    by_address = {record.address: record for record in artifact.records}
    _require(
        len(by_address) == len(artifact.records), "two records share one address; a finding carries exactly one class"
    )

    missing = sorted(address for address in expected if address not in by_address)
    _require(
        not missing,
        f"{len(missing)} drawn finding(s) carry no class, first {missing[0] if missing else ''}: "
        "every raw finding gets exactly one label before any baseline is built",
    )
    extra = sorted(address for address in by_address if address not in expected)
    _require(not extra, f"{len(extra)} record(s) match no drawn finding, first {extra[0] if extra else ''}")

    for address, (cluster, population) in expected.items():
        record = by_address[address]
        _require(
            record.population == population,
            f"{record.path}:{record.line}: recorded as {record.population}, the run says {population}",
        )
        _require(
            record.weakest_similarity == cluster.weakest,
            f"{record.path}:{record.line}: recorded similarity {record.weakest_similarity}, "
            f"the run says {cluster.weakest}",
        )
        members = tuple(f"{member.path}:{member.line}-{member.end_line}" for member in cluster.members)
        _require(
            record.members == members,
            f"{record.path}:{record.line}: recorded members {list(record.members)} != {list(members)}",
        )


def main(argv: Sequence[str] | None = None) -> int:
    """Verify an artifact against a runs directory and print the rates. Returns an exit code."""
    parser = argparse.ArgumentParser(description="verify a classification artifact against its runs")
    here = Path(__file__).resolve().parent
    parser.add_argument("--artifact", type=Path, default=here / "dry_run_classification.json")
    parser.add_argument("--runs", type=Path, default=here / "runs")
    args = parser.parse_args(argv)

    try:
        artifact = load(args.artifact)
        verify(artifact, args.runs)
    except ClassificationError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    print(f"artifact verified against {args.runs} — detector {artifact.detector_tag} ({artifact.detector_commit})")
    print("| population | FP | clusters | FP rate | 95% Wilson |")
    print("|---|---|---|---|---|")
    for population in ("exact", "near", "all"):
        rate = artifact.false_positive_rate(population)
        interval = artifact.wilson_interval(population)
        if rate is None or interval is None:
            print(f"| {population} | 0 | 0 | unavailable (no clusters emitted) | unavailable |")
            continue
        print(
            f"| {population} | {artifact.false_positive_count(population)} | "
            f"{artifact.denominator(population)} | {rate:.3f} | [{interval[0]:.3f}, {interval[1]:.3f}] |"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
