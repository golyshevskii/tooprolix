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
import subprocess
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


class GateFailed(RuntimeError):
    """The measured false-positive share is worse than the pre-registered threshold."""


@dataclass(frozen=True)
class RunExpectation:
    """
    What the pre-registration says a run must look like before anything may be read out of it.

    🔴 **`sha256` is the authoritative one and it is checked against the file on disk.** It used to
    be pinned here and never loaded, while `verify` compared the file against
    `artifact.runs[].sha256` — another field of the very artifact being graded. Measured: rewrite a
    run, update the artifact's own hash to match, and the forged population verified clean with
    exit 0 while the pre-registered pin said something else entirely. That is this epic's defect #6
    at its eighth layer, inside the fix that closed the seventh. The pushed pre-registration is the
    authority; an artifact field may never be the sole source of any check.
    """

    schema_version: str
    complete: bool
    skipped: int
    excluded: int
    files_walked: int
    sha256: str | None = None


@dataclass(frozen=True)
class RunRequirements:
    """
    What every run of a profile must satisfy, when it cannot be predicted in advance.

    🔴 **The answer to a chicken-and-egg the first version of `RunExpectation` created.** For the
    corpus, the per-run numbers were already measured and could simply be pinned. For a holdout they
    cannot be — nobody has run it, which is the entire point — so pinning them would either be
    impossible or would have to be filled in *after* the run, which is the post-hoc self-report this
    file exists to prevent. Requirements are predictable where counts are not: a run that is not
    `complete`, or that skipped or excluded anything, is a bad run whatever it found.
    """

    schema_version: str
    complete: bool
    skipped: int
    excluded: int


@dataclass(frozen=True)
class Profile:
    """
    One pre-registered measurement: its draws, its run expectations and its threshold.

    `runs` pins per-run numbers that were already measured (the calibration corpus); `repositories`
    and `run_requirements` are amendment 2's answer for a holdout, whose per-run numbers cannot be
    predicted because nobody has run it. Fixing the repository **count** in advance is what stops the
    scan's size from depending on what the detector found.
    """

    name: str
    purpose: str
    is_gate: bool
    threshold: float | None
    draws: tuple[Draw, ...]
    runs: Mapping[str, RunExpectation]
    repositories: int | None = None
    run_requirements: RunRequirements | None = None


@dataclass(frozen=True)
class Preregistration:
    """
    The machine-readable half of `corpus/PREREGISTRATION.md`.

    Everything `verify` grades an artifact against lives here, because this file is committed and
    pushed **before** a candidate repository is ever scanned, and the artifact is written after. The
    artifact can therefore be wrong about the population; it cannot redefine it.
    """

    profiles: Mapping[str, Profile]

    def profile(self, name: str) -> Profile:
        """
        Return the named profile, or fail closed naming the ones that exist.

        A profile that **is** a gate and carries no numeric threshold is refused here rather than at
        load time, so that an undecided holdout threshold does not also block the dry run. The
        refusal is the point: a gate with no number cannot be failed, so the holdout may not be run
        until the owner sets one.
        """
        if name not in self.profiles:
            raise ClassificationError(
                f"no pre-registered profile {name!r}; this file declares {', '.join(sorted(self.profiles))}"
            )
        found = self.profiles[name]
        if found.is_gate and found.threshold is None:
            raise ClassificationError(
                f"profile {name!r} is a gate but its threshold is not a number; a gate that cannot "
                "be failed is not a gate, so this measurement is refused until the owner sets one"
            )
        return found


@dataclass(frozen=True)
class RunReference:
    """One run JSON the artifact classifies, pinned by the digest of its bytes."""

    name: str
    sha256: str


@dataclass(frozen=True)
class Draw:
    """
    One deterministic draw over a run population, in `sample_clusters`' own vocabulary.

    `limit` is the size asked for and `minimum` the size below which the draw is a failure. They
    used to be the same number, which made a short pool fatal even where the pre-registration allows
    a shortfall — and the only way out was editing the artifact's `limit` afterwards, turning the
    sample size into a post-run self-report. Both now come from the pre-registration, so a shortfall
    is recorded by the rule rather than by the person who saw the result.
    """

    population: Population
    per_repo: int
    limit: int | None = None
    minimum: int = 0


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
    """
    A whole classification: the detector it was taken on, the runs it covers, and the records.

    🔴 **It deliberately does NOT carry the draw parameters, and that is the point.** It used to,
    and `verify` re-drew the expected population from them — so the artifact defined the population
    it was then graded against. Measured: an artifact declaring `limit: 0` with zero records
    verified clean and reported `unavailable`, i.e. the gate passed with nothing labelled. The draws
    now come from [`Preregistration`], which is committed and pushed before any run exists, and the
    profile is named by the **caller** of `verify`, never read out of the artifact.
    """

    detector_tag: str
    detector_commit: str
    detector_dirty: bool
    binary_sha256: str
    protocol: str
    blind: bool
    runs: tuple[RunReference, ...]
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
    _require(
        "draws" not in raw,
        f"{path}: an artifact may not declare its own draws — they come from the pre-registration, "
        "or the artifact defines the population it is graded against",
    )
    for field_name in ("detector_commit", "binary_sha256"):
        _require(bool(raw.get(field_name)), f"{path}: {field_name} is required — AC9 binds the run to a real build")
    _require(isinstance(raw.get("detector_dirty"), bool), f"{path}: detector_dirty must be true or false, not omitted")
    return Artifact(
        detector_tag=str(raw["detector_tag"]),
        detector_commit=str(raw["detector_commit"]),
        detector_dirty=bool(raw["detector_dirty"]),
        binary_sha256=str(raw["binary_sha256"]),
        protocol=str(raw["protocol"]),
        blind=bool(raw["blind"]),
        runs=tuple(RunReference(name=str(run["name"]), sha256=str(run["sha256"])) for run in raw["runs"]),
        records=tuple(_record(record) for record in raw["records"]),
    )


def load_preregistration(path: Path) -> Preregistration:
    """
    Read `corpus/preregistration.json`, the operative half of the pre-registration.

    # Raises
    `ClassificationError` if a profile is malformed, or if a profile that **is** a gate carries no
    numeric threshold. That last refusal is deliberate and fails closed: a gate with no number
    cannot be failed, so the holdout simply may not be run until the owner sets one.
    """
    raw: Mapping[str, Any] = json.loads(path.read_text(encoding="utf-8"))
    profiles: dict[str, Profile] = {}
    for name, body in raw.get("profiles", {}).items():
        # A placeholder string rather than a number means "the owner has not decided yet", and it is
        # NOT read as 0.0 — which would be a threshold no measurement could ever meet.
        raw_threshold = body.get("threshold")
        threshold = float(raw_threshold) if isinstance(raw_threshold, (int, float)) else None
        is_gate = bool(body.get("is_gate", False))
        _require(bool(body.get("draws")), f"{path}: profile {name!r} declares no draws")
        profiles[name] = Profile(
            name=name,
            purpose=str(body.get("purpose", "")),
            is_gate=is_gate,
            threshold=threshold,
            draws=tuple(
                Draw(
                    population=str(draw["population"]),  # ty: ignore[invalid-argument-type]
                    per_repo=int(draw["per_repo"]),
                    limit=None if draw.get("limit") is None else int(draw["limit"]),
                    minimum=int(draw["minimum"]),
                )
                for draw in body["draws"]
            ),
            repositories=None if body.get("repositories") is None else int(body["repositories"]),
            run_requirements=(
                RunRequirements(
                    schema_version=str(body["run_requirements"]["schema_version"]),
                    complete=bool(body["run_requirements"]["complete"]),
                    skipped=int(body["run_requirements"]["skipped"]),
                    excluded=int(body["run_requirements"]["excluded"]),
                )
                if body.get("run_requirements")
                else None
            ),
            runs={
                run_name: RunExpectation(
                    schema_version=str(expected["schema_version"]),
                    complete=bool(expected["complete"]),
                    skipped=int(expected["skipped"]),
                    excluded=int(expected["excluded"]),
                    files_walked=int(expected["files_walked"]),
                    sha256=None if expected.get("sha256") is None else str(expected["sha256"]),
                )
                for run_name, expected in body.get("runs", {}).items()
            },
        )
    _require(bool(profiles), f"{path}: no profiles declared")
    return Preregistration(profiles=profiles)


def digest(path: Path) -> str:
    """Return the SHA-256 of a file's bytes, hex-encoded."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _expected(profile: Profile, runs_dir: Path) -> dict[tuple[str, str, int], tuple[Cluster, str]]:
    """
    Re-draw the population from the runs on disk, using the **pre-registered** draw parameters.

    Takes a [`Profile`], never an [`Artifact`]: the thing being graded does not get to say what it
    is graded against. See [`Artifact`] for the measured exploit that made this necessary.
    """
    expected: dict[tuple[str, str, int], tuple[Cluster, str]] = {}
    for draw in profile.draws:
        pools = sample_clusters.load_runs(runs_dir, draw.population)
        drawn = sample_clusters.round_robin(pools, draw.per_repo, minimum=draw.minimum, limit=draw.limit)
        for cluster in drawn:
            population = "exact" if cluster.is_exact else "near"
            expected[(cluster.repo, cluster.path, cluster.line)] = (cluster, population)
    _require(
        bool(expected),
        f"profile {profile.name!r} drew no findings at all; an empty population is a broken "
        "measurement, never a clean one",
    )
    return expected


def _check_run_health(
    name: str, path: Path, expected: RunExpectation | None, requirements: RunRequirements | None, runs_dir: Path
) -> None:
    """
    Grade a run's own metadata before any finding is read out of it.

    🔴 **The walked-file count is the whole reason this exists.** A run truncated by the parent
    `.gitignore` trap reports `complete: true` with an empty `skipped[]` and a plausible-looking
    finding list — the count is the only place it becomes visible, and the run JSON cannot carry it.
    `corpus/run_all.sh` writes it to `<name>.files`, and it is compared against the count pinned in
    the pre-registration, so the check is against an external expectation rather than a self-report.
    """
    document: Mapping[str, Any] = json.loads(path.read_text(encoding="utf-8"))
    # A profile pins per-run numbers when they were already measured (the corpus) and states
    # requirements when they cannot be predicted (a holdout nobody has run). One of the two must
    # exist: a run nothing checks is a hole, not a default.
    health: RunExpectation | RunRequirements | None = expected or requirements
    _require(
        health is not None,
        f"{name}: the profile pins neither per-run expectations nor run requirements, so nothing "
        "about this run would be checked at all",
    )
    if health is None:  # unreachable after the guard; narrows the union for the type checker
        return
    for field_name, want in (("schema_version", health.schema_version), ("complete", health.complete)):
        got = document.get(field_name)
        _require(got == want, f"{name}: {field_name} is {got!r}, the pre-registration expects {want!r}")
    for field_name, want_count in (("skipped", health.skipped), ("excluded", health.excluded)):
        got_count = len(document.get(field_name, []))
        _require(
            got_count == want_count,
            f"{name}: {got_count} {field_name} entr(ies), the pre-registration expects {want_count}",
        )

    sidecar = runs_dir / f"{name}.files"
    _require(
        sidecar.exists(),
        f"{name}: {sidecar} is missing, so the size of the walk is unknown. A run truncated by an "
        "ancestor .gitignore still reports complete=true; without this count nothing can tell.",
    )
    walked = int(sidecar.read_text(encoding="utf-8").strip())
    if expected is None:
        # A holdout run has no pre-registered count of its own — §3.0.1 records what the GitHub API
        # reported and §7 step 1 compares the walk against it by hand. The sidecar must still exist,
        # because its absence is exactly what a truncated walk looks like from here.
        return
    _require(
        walked == expected.files_walked,
        f"{name}: the walk saw {walked} .py files, the pre-registration expects "
        f"{expected.files_walked} — the run measured a different tree from the one registered",
    )


def _check_detector(artifact: Artifact, repo_root: Path, binary: Path | None) -> None:
    """
    Bind the classification to a real build rather than to a free-text string.

    `detector_commit` must resolve to a commit **in this repository**, which is the one claim in the
    artifact that an outside fact can contradict. `binary_sha256` is checked whenever the binary is
    available; when it is not, the caller says so explicitly rather than the check quietly passing.

    ⚠️ **Stated rather than overclaimed: nothing here proves which binary produced a given JSON.**
    Only re-running it does. What this rules out is a provenance string naming a commit that does
    not exist, a tree that was dirty without saying so, and a binary on disk that is not the one the
    artifact claims.
    """
    resolved = subprocess.run(
        ["git", "-C", str(repo_root), "rev-parse", "--verify", "--quiet", f"{artifact.detector_commit}^{{commit}}"],
        capture_output=True,
        text=True,
        check=False,
    )
    _require(
        resolved.returncode == 0,
        f"detector_commit {artifact.detector_commit!r} does not resolve to a commit in {repo_root}; "
        "AC9 names the commit a measurement was taken on, and a name nothing can check is not one",
    )
    if binary is None:
        return
    _require(binary.exists(), f"{binary} does not exist; pass binary=None to say so explicitly")
    actual = digest(binary)
    _require(actual == artifact.binary_sha256, f"{binary} hashes {actual}, the artifact pins {artifact.binary_sha256}")


def verify(
    artifact: Artifact, runs_dir: Path, profile: Profile, *, repo_root: Path | None = None, binary: Path | None = None
) -> None:
    """
    Fail unless the artifact describes exactly the findings the pre-registered draw selects.

    `profile` is supplied by the **caller** — the test, or `--profile` on the command line — and is
    never read out of the artifact, so the artifact cannot choose the population it is graded
    against.

    # Raises
    `ClassificationError`, naming the run or the address that disagrees. Coverage is checked in
    **both** directions on purpose: a checker that only asked "does every row point at a real
    finding" would pass an artifact that simply omitted the findings its author disliked, which is
    Decisions #17's "baseline is an output, not an input" defeated by omission.
    """
    _check_detector(artifact, repo_root or Path(__file__).resolve().parent.parent, binary)

    declared = {run.name for run in artifact.runs}
    if profile.runs:
        _require(
            declared == set(profile.runs),
            f"the artifact covers runs {sorted(declared)}, profile {profile.name!r} pre-registers "
            f"{sorted(profile.runs)} — a classification may not add or drop a run after the fact",
        )
    if profile.repositories is not None:
        _require(
            len(declared) == profile.repositories,
            f"profile {profile.name!r} fixes {profile.repositories} repositories in advance and the "
            f"artifact covers {len(declared)}. A short stratum is a reportable result; a short *scan* "
            "is not, because the number of repositories was decided before the data.",
        )

    # 🔴 The directory separation is load-bearing and is checked rather than remembered. `load_runs`
    # globs a directory, so a holdout run dropped into `corpus/runs/` silently joins the calibration
    # population — measured: five `Raven` clusters entered the *dry-run* draw that way, and the only
    # symptom was a coverage error nobody would have read as contamination. Every run the artifact
    # declares must be in the directory being graded, and nothing else may be.
    # `EXCLUDED_RUNS` rather than a second list: `sample_clusters` already owns which run files are
    # outside every population (`crewAI-full` is the same checkout at an unmeasurable root), and two
    # owners for one fact is the defect this epic keeps paying for.
    present = {
        path.stem
        for path in runs_dir.glob("*.json")
        if path.stat().st_size > 0 and path.stem not in sample_clusters.EXCLUDED_RUNS
    }
    _require(
        present == declared,
        f"{runs_dir} holds runs {sorted(present)} but the artifact declares {sorted(declared)}. "
        "Calibration and holdout runs must never share a directory: the sampler globs it, so an "
        "extra file silently enlarges the population the rates are taken over.",
    )

    for run in artifact.runs:
        path = runs_dir / f"{run.name}.json"
        _require(path.exists(), f"{run.name}: {path} is missing; the artifact classifies a run that is not here")
        actual = digest(path)
        pinned = profile.runs.get(run.name)
        authority = pinned.sha256 if pinned is not None and pinned.sha256 is not None else None
        if authority is not None:
            # The pre-registration first, because it was pushed before the run existed. The
            # artifact's own field is then required to agree with it rather than to stand in for it.
            _require(
                actual == authority,
                f"{run.name}: {path} hashes {actual}, the PRE-REGISTRATION pins {authority}. "
                "The run on disk is not the run this measurement was registered against.",
            )
            _require(
                run.sha256 == authority,
                f"{run.name}: the artifact claims {run.sha256} but the pre-registration pins "
                f"{authority}. An artifact may not restate a pinned fact differently.",
            )
        else:
            _require(
                actual == run.sha256,
                f"{run.name}: {path} hashes {actual}, the artifact pins {run.sha256}. The "
                "classification describes a different set of findings from the one on disk.",
            )
        _check_run_health(run.name, path, pinned, profile.run_requirements, runs_dir)

    expected = _expected(profile, runs_dir)
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


def baseline_from(artifact: Artifact) -> tuple[str, ...]:
    """
    Derive the baseline from the labels, so it cannot exist before them.

    🔴 **This is EPIC.md Decisions #17's ordering made structural instead of described.** The rule
    is: save and hash the first *unsuppressed* run, label every drawn finding, and only then build a
    baseline — because a baseline taken first is a filter that decides what gets annotated. Prose
    saying so is not enforcement; a function whose only input is the labelled records is, because
    there is no way to call it without them.

    The baseline is the addresses of the findings classified `FP`. A `TP` is a finding the annotator
    wants acted on, so suppressing it would be suppressing the tool's own value.
    """
    return tuple(sorted(record.members[0] for record in artifact.records if record.classification == "FP"))


def gate(artifact: Artifact, profile: Profile) -> tuple[bool, str]:
    """
    Compare the measured false-positive share against the pre-registered threshold.

    Returns `(passed, explanation)`. The comparison is on the **combined** rate, which §5.1 of the
    pre-registration names as the single primary number; the per-half rates and the intervals are
    reported alongside but are not a second criterion, because two criteria let the worse one be
    dropped after the fact.

    A population that emitted nothing is **red**, not green: a rate over an empty denominator is
    `unavailable`, and a gate that passes because it measured nothing is the `exit 0` loophole this
    task exists to close.
    """
    if profile.threshold is None:
        return False, (
            f"profile {profile.name!r} has no numeric threshold, so no gate was evaluated. This is not a pass."
        )
    rate = artifact.false_positive_rate("all")
    if rate is None:
        return False, f"no clusters were emitted, so the FP share is unavailable — RED against {profile.threshold:.3f}"
    verdict = "PASS" if rate <= profile.threshold else "RED"
    return rate <= profile.threshold, (
        f"{verdict}: FP share {rate:.3f} against the pre-registered threshold {profile.threshold:.3f} "
        f"({artifact.false_positive_count('all')} FP / {artifact.denominator('all')} clusters)"
    )


def main(argv: Sequence[str] | None = None) -> int:
    """
    Verify an artifact against its runs and its pre-registration, then evaluate the gate.

    Exit codes: `0` verified and (where a threshold exists) passed, `1` verification failed or the
    gate is red, `2` the run could not be evaluated at all.
    """
    parser = argparse.ArgumentParser(description="verify a classification artifact against its pre-registration")
    here = Path(__file__).resolve().parent
    parser.add_argument("--artifact", type=Path, default=here / "dry_run_classification.json")
    parser.add_argument("--runs", type=Path, default=here / "runs")
    parser.add_argument("--preregistration", type=Path, default=here / "preregistration.json")
    parser.add_argument("--profile", required=True, help="which pre-registered profile to grade against")
    parser.add_argument("--binary", type=Path, default=None, help="the built CLI, to check its recorded hash")
    args = parser.parse_args(argv)

    try:
        preregistration = load_preregistration(args.preregistration)
        profile = preregistration.profile(args.profile)
        artifact = load(args.artifact)
        verify(artifact, args.runs, profile, binary=args.binary)
    except ClassificationError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    except (OSError, json.JSONDecodeError) as error:
        print(f"error: could not read an input: {error}", file=sys.stderr)
        return 2

    dirty = " (DIRTY working tree)" if artifact.detector_dirty else ""
    print(f"artifact verified against {args.runs} — profile {profile.name!r}, {artifact.detector_tag}{dirty}")
    print(f"detector commit {artifact.detector_commit}, binary sha256 {artifact.binary_sha256[:16]}…")
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
    print(f"baseline derived from the labels: {len(baseline_from(artifact))} suppressed address(es)")

    passed, explanation = gate(artifact, profile)
    print(explanation)
    if profile.is_gate and not passed:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
