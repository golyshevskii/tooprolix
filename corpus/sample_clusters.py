"""
Draw the AC1 annotation sample of `TPX003` clusters and print it with the prose to be judged.

AC1 is "precision >= 0.8 on a hand-annotated sample of at least 20 `TPX003` findings", and this
script owns the two decisions that make that number mean anything. Both are recorded decisions of
the epic, not choices made here:

* **Near clusters only.** A cluster whose weakest edge is exactly 1.0 holds definitionally
  identical text; asking whether one of two identical explanations should be merged says nothing
  about a detector tuned around a 0.75 Jaccard threshold. `weakest_score < 1.0` is the only
  operational signal available — `Cluster` carries no provenance field — and it is *conservative*:
  a near edge that happens to score exactly 1.0 is counted as exact and dropped. Measured on the
  runs in `corpus/runs/`, the population is large enough that the conservatism costs nothing.
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
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

#: Runs that are not part of the AC1 population: `crewAI-full` is the same checkout as `crewAI` at
#: a root that cannot be measured at all (exit 2).
EXCLUDED_RUNS: frozenset[str] = frozenset({"crewAI-full"})

#: AC1's floor: "precision on a sample of at least 20 findings".
MINIMUM_SAMPLE: int = 20


class SampleTooSmall(RuntimeError):
    """The corpus could not supply the sample AC1 requires."""


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


def _member(raw: Mapping[str, Any]) -> Member:
    return Member(path=str(raw["path"]), line=int(raw["line"]), end_line=int(raw["end_line"]))


def near_clusters(repo: str, report: Mapping[str, Any]) -> list[Cluster]:
    """
    Return every `TPX003` finding of `report` whose weakest edge is inexact, ordered by address.

    Findings without a `weakest` field are volume findings (`TPX001`/`TPX002`) and are skipped
    rather than defaulted: a default would put a block that no similarity was ever computed for
    into a precision measurement about similarity.
    """
    clusters: list[Cluster] = []
    for raw in report.get("findings", []):
        if raw.get("code") != "TPX003":
            continue
        weakest = raw.get("weakest")
        if weakest is None:
            continue
        similarity = float(weakest["similarity"])
        if similarity >= 1.0:
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


def round_robin(pools: Mapping[str, Sequence[Cluster]], per_repo: int, minimum: int = MINIMUM_SAMPLE) -> list[Cluster]:
    """
    Return the first `per_repo` clusters of each repository, repositories interleaved by name.

    Interleaving rather than concatenating matters for reading, not for the set: a reviewer who
    stops early has still seen every repository.

    # Raises
    `SampleTooSmall` if fewer than `minimum` clusters come out. Pools smaller than `per_repo` shrink
    the sample silently, and AC1's "at least 20 findings" would then live only in prose with no red
    path in the tool that draws it.
    """
    ordered = sorted(pools)
    sampled: list[Cluster] = []
    for index in range(per_repo):
        for repo in ordered:
            pool = pools[repo]
            if index < len(pool):
                sampled.append(pool[index])
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


def _load_runs(runs_dir: Path) -> dict[str, list[Cluster]]:
    pools: dict[str, list[Cluster]] = {}
    for path in sorted(runs_dir.glob("*.json")):
        repo = path.stem
        if repo in EXCLUDED_RUNS or path.stat().st_size == 0:
            continue
        report = json.loads(path.read_text(encoding="utf-8"))
        pool = near_clusters(repo, report)
        if pool:
            pools[repo] = pool
    return pools


def main(argv: Sequence[str] | None = None) -> int:
    """Print the sample as Markdown. Returns a process exit code."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--per-repo", type=int, default=4, help="clusters drawn per repository")
    parser.add_argument(
        "--runs", type=Path, default=Path(__file__).resolve().parent / "runs", help="directory of run_all.sh output"
    )
    args = parser.parse_args(argv)

    corpus_root = os.environ.get("CORPUS_ROOT")
    if not corpus_root:
        print("error: set CORPUS_ROOT to the directory the runs were produced from", file=sys.stderr)
        return 2
    root = Path(corpus_root)

    pools = _load_runs(args.runs)
    if not pools:
        print(f"error: no near clusters in {args.runs}; run corpus/run_all.sh first", file=sys.stderr)
        return 2

    try:
        sampled = round_robin(pools, args.per_repo)
    except SampleTooSmall as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"# AC1 sample — {len(sampled)} near clusters, {args.per_repo} per repository\n")
    print("| repo | near clusters available |")
    print("|---|---|")
    for repo in sorted(pools):
        print(f"| {repo} | {len(pools[repo])} |")
    print()
    for index, cluster in enumerate(sampled, start=1):
        print(f"## {index}. `{cluster.repo}` — weakest edge {cluster.weakest:.3f}\n")
        for member in cluster.members:
            print(f"`{member.path}:{member.line}-{member.end_line}`\n")
            print(_quote(root, member))
            print()
        print("**Verdict:** \n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
