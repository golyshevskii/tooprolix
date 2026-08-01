"""
Draw the annotation sample of `TPX003` clusters and print it with the prose to be judged.

The precision figure the sample produces means nothing without the two decisions below, so this
script owns both:

* **Near clusters only, by default.** A cluster whose weakest edge is exactly 1.0 holds
  definitionally identical text; asking whether one of two identical explanations should be merged
  says nothing about a detector tuned around a 0.75 Jaccard threshold. `weakest_score < 1.0` is the
  only operational signal available — `Cluster` carries no provenance field — and it is
  *conservative*: a near edge that happens to score exactly 1.0 is counted as exact and dropped.
  Measured on the runs in `corpus/runs/`, the population is large enough that the conservatism costs
  nothing.

  **This is a default, not a ceiling.** `--population exact|all` exists because the exact clusters
  need measuring too — measured 2026-08-01 on `corpus/runs/` at `7757b20`, they are **456 of 619**
  (they were 457 of 617 at `v0.4.0`; the runs moved after that tag, the shape did not). The
  population actually used is printed
  in the sample's own heading, so a number can never be read under the wrong one.
* **Round-robin over repositories.** A global prefix over `(repo, path, line)` in ASCII order lies
  entirely inside `OpenHands` and never reaches `langgraph` or `pydantic`.

Input is `corpus/runs/<repo>.json`, written by `corpus/run_all.sh`. Output is Markdown on stdout:
one section per cluster, with every member's source lines quoted, ready to be annotated in
`corpus/annotations.md`. The verdict column is left blank on purpose — this script selects and
displays, it does not judge.

Stdlib only, like `corpus/measure.py`, so the sample cannot move because a dependency resolved
differently.

Usage:
    CORPUS_ROOT=/somewhere/outside uv run python3 corpus/sample_clusters.py [--per-repo 4]
    CORPUS_ROOT=/somewhere/outside uv run python3 corpus/sample_clusters.py \
        --population exact --per-repo 20 --limit 20
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal, get_args

#: The three populations of `TPX003` clusters, by the only operational signal there is.
Population = Literal["near", "exact", "all"]

#: Valid `--population` values, derived from the type so the two cannot drift apart.
POPULATIONS: tuple[str, ...] = get_args(Population)

#: Runs that are not part of the AC1 population: `crewAI-full` is the same checkout as `crewAI` at
#: a root that cannot be measured at all (exit 2).
EXCLUDED_RUNS: frozenset[str] = frozenset({"crewAI-full"})

#: AC1's floor: "precision on a sample of at least 20 findings".
MINIMUM_SAMPLE: int = 20


class SampleTooSmall(RuntimeError):
    """The corpus could not supply the sample AC1 requires."""


class MalformedFinding(RuntimeError):
    """A `TPX003` finding the sampler cannot interpret, which it refuses to skip."""


class MalformedRun(RuntimeError):
    """A run document the sampler cannot interpret, which it refuses to read findings out of."""


#: Every top-level key a run document may carry. Closed on purpose — see [`validate_run`].
RUN_KEYS: frozenset[str] = frozenset({"schema_version", "complete", "skipped", "excluded", "findings"})


def validate_run(name: str, report: Mapping[str, Any]) -> None:
    """
    Refuse a run document that is not shaped like one, naming the file.

    **The container, not one more key.** A previous round made a missing `weakest` fatal, and the
    document around it was left unvalidated — so renaming the top-level `findings` to `findings_v2`
    dropped the sampled population from **586 to 501** and made `07-klavis` vanish entirely, with no
    error, no warning and no exit code. Hardening the key that was exploited leaves its container
    open, and that is the shape this epic keeps rediscovering.

    Unknown top-level keys are refused rather than ignored: a document carrying `findings_v2`
    alongside nothing else is exactly the forgery this guards, and silently tolerating unknown keys
    is what let it through.

    # Raises
    `MalformedRun`, naming `name` and the offending key.
    """
    for key, kind in (
        ("schema_version", str),
        ("complete", bool),
        ("skipped", list),
        ("excluded", list),
        ("findings", list),
    ):
        if key not in report:
            raise MalformedRun(f"{name}: run document has no `{key}` — a renamed key is not a missing rule")
        if not isinstance(report[key], kind):
            raise MalformedRun(f"{name}: `{key}` is {type(report[key]).__name__}, expected {kind.__name__}")
    unknown = sorted(set(report) - RUN_KEYS)
    if unknown:
        raise MalformedRun(
            f"{name}: unknown top-level key(s) {', '.join(unknown)}. A document the sampler cannot "
            "fully account for is one it must not read findings out of."
        )


@dataclass(frozen=True, order=True)
class Member:
    """One block inside a cluster, at the address the CLI reported."""

    path: str
    line: int
    end_line: int


@dataclass(frozen=True)
class Cluster:
    """One `TPX003` finding, reduced to what the annotator has to see."""

    repo: str
    path: str
    line: int
    weakest: float
    members: tuple[Member, ...]

    @property
    def address(self) -> tuple[str, int]:
        """The sort key: the finding's own reported address, nothing derived."""
        return (self.path, self.line)

    @property
    def is_exact(self) -> bool:
        """Whether the cluster's weakest edge is exact, i.e. the cluster came off the exact path."""
        return self.weakest >= 1.0


def _member(raw: Mapping[str, Any]) -> Member:
    return Member(path=str(raw["path"]), line=int(raw["line"]), end_line=int(raw["end_line"]))


def tpx003_clusters(repo: str, report: Mapping[str, Any], population: Population = "near") -> list[Cluster]:
    """
    Return the `TPX003` findings of `report` in `population`, ordered by address.

    Findings whose `code` is not `TPX003` are volume findings and are not in this population at all.
    A `TPX003` finding without a readable `weakest`, however, is a **hard error**: see below.

    # Raises

    `ValueError` if `population` is not one of [`POPULATIONS`]. Falling back to the `near` default
    would report a near-only number under an `exact` heading, which is the single failure mode the
    exact population was added to prevent — so the guard fails closed.

    `MalformedFinding` if a `TPX003` finding carries no readable `weakest.similarity`.

    **This used to be `if weakest is None: continue`, and that is a guard a rename switches
    off.** Measured on a copy of the corpus: renaming `weakest` to `weakest_v2` on six findings
    dropped the sampled population from 618 to 612 with no error, no warning and no exit code — the
    clusters simply stopped existing, and every rate computed from them would have been quietly
    taken over a smaller denominator. A check that skips what it cannot interpret is a check that
    can be disabled by editing a key, so it now names the address and stops.
    """
    if population not in POPULATIONS:
        raise ValueError(f"unknown population {population!r}; expected one of {', '.join(POPULATIONS)}")
    clusters: list[Cluster] = []
    for raw in report.get("findings", []):
        if raw.get("code") != "TPX003":
            continue
        weakest = raw.get("weakest")
        where = f"{raw.get('path', '?')}:{raw.get('line', '?')}"
        if not isinstance(weakest, Mapping) or "similarity" not in weakest:
            raise MalformedFinding(
                f"{repo}: TPX003 finding at {where} has no readable `weakest.similarity`; "
                "a similarity finding that cannot state its similarity is not skippable"
            )
        similarity = float(weakest["similarity"])
        if population == "near" and similarity >= 1.0:
            continue
        if population == "exact" and similarity < 1.0:
            continue
        clusters.append(
            Cluster(
                repo=repo,
                path=str(raw["path"]),
                line=int(raw["line"]),
                weakest=similarity,
                members=tuple(sorted(_member(location) for location in raw["locations"])),
            )
        )
    clusters.sort(key=lambda cluster: cluster.address)
    return clusters


def round_robin(
    pools: Mapping[str, Sequence[Cluster]], per_repo: int, minimum: int = MINIMUM_SAMPLE, limit: int | None = None
) -> list[Cluster]:
    """
    Return the first `per_repo` clusters of each repository, repositories interleaved by name.

    Interleaving rather than concatenating matters for reading, not for the set: a reviewer who
    stops early has still seen every repository.

    `limit` truncates the **interleaved** sequence, never the individual pools. That distinction is
    the rule, not an implementation detail: cutting each pool first is the single-repository prefix
    this function exists to avoid, wearing a different name.

    # Raises
    `SampleTooSmall` if fewer than `minimum` clusters come out, counted after `limit` is applied.
    Pools smaller than `per_repo` shrink the sample silently, and AC1's "at least 20 findings" would
    then live only in prose with no red path in the tool that draws it.
    """
    ordered = sorted(pools)
    sampled: list[Cluster] = []
    for index in range(per_repo):
        for repo in ordered:
            pool = pools[repo]
            if index < len(pool):
                sampled.append(pool[index])
    if limit is not None:
        sampled = sampled[:limit]
    if len(sampled) < minimum:
        raise SampleTooSmall(
            f"{len(sampled)} clusters drawn from {len(ordered)} repositories at {per_repo} each; "
            f"AC1 needs at least {minimum}"
        )
    return sampled


def _quote(root: Path, member: Member) -> str:
    """Return the member's own source lines, indented as a Markdown code block."""
    source = (root / member.path).read_text(encoding="utf-8", errors="replace").splitlines()
    body = source[member.line - 1 : member.end_line]
    return "\n".join(f"    {line}" for line in body)


def load_runs(runs_dir: Path, population: Population = "near") -> dict[str, list[Cluster]]:
    """Return one ordered pool per run file in `runs_dir`, restricted to `population`."""
    pools: dict[str, list[Cluster]] = {}
    for path in sorted(runs_dir.glob("*.json")):
        repo = path.stem
        if repo in EXCLUDED_RUNS or path.stat().st_size == 0:
            continue
        report = json.loads(path.read_text(encoding="utf-8"))
        validate_run(repo, report)
        pool = tpx003_clusters(repo, report, population)
        if pool:
            pools[repo] = pool
    return pools


def main(argv: Sequence[str] | None = None) -> int:
    """Print the sample as Markdown. Returns a process exit code."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--per-repo", type=int, default=4, help="clusters drawn per repository")
    parser.add_argument("--population", choices=POPULATIONS, default="near", help="which TPX003 clusters are eligible")
    parser.add_argument("--limit", type=int, default=None, help="truncate the interleaved sequence to this many")
    parser.add_argument(
        "--runs", type=Path, default=Path(__file__).resolve().parent / "runs", help="directory of run_all.sh output"
    )
    args = parser.parse_args(argv)

    corpus_root = os.environ.get("CORPUS_ROOT")
    if not corpus_root:
        print("error: set CORPUS_ROOT to the directory the runs were produced from", file=sys.stderr)
        return 2
    root = Path(corpus_root)

    pools = load_runs(args.runs, args.population)
    if not pools:
        print(f"error: no {args.population} clusters in {args.runs}; run corpus/run_all.sh first", file=sys.stderr)
        return 2

    minimum = args.limit if args.limit is not None else MINIMUM_SAMPLE
    try:
        sampled = round_robin(pools, args.per_repo, minimum=minimum, limit=args.limit)
    except SampleTooSmall as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"# Sample — {len(sampled)} {args.population} clusters, {args.per_repo} per repository\n")
    print(f"| repo | {args.population} clusters available |")
    print("|---|---|")
    for repo in sorted(pools):
        print(f"| {repo} | {len(pools[repo])} |")
    print()
    for index, cluster in enumerate(sampled, start=1):
        kind = "exact" if cluster.is_exact else "near"
        print(f"## {index}. `{cluster.repo}` — {kind}, weakest edge {cluster.weakest:.3f}\n")
        for member in cluster.members:
            print(f"`{member.path}:{member.line}-{member.end_line}`\n")
            print(_quote(root, member))
            print()
        print("**Verdict:** \n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
