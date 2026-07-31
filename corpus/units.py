"""
Compare the four candidate units of "prose volume" at an equal alert volume.

The unit that ships is **words of the normalised text**. What the earlier comparison measured was
how far the units *agree* with one another, and agreement is structurally incapable of showing which
one is more precise. This script produces the material for annotating a true positive instead, and
it is built around four constraints:

1. **The blocks come from the shipped extractor.** `load_blocks` calls
   `tooprolix.prose_blocks(path, source)` — the same function the rule reads, including its
   `>= 2 lines AND >= 8 words` filter. Nothing here re-implements block finding; a probe that
   re-implements the thing it is measuring measures the probe.

   **THIS SCRIPT DOES NOT CURRENTLY RUN.** `prose_blocks` was a pyo3 export, and the pyo3 boundary
   is gone: `pyproject.toml` ships `bindings = "bin"` and `scripts/install-smoke.sh` asserts that
   `import tooprolix` MUST raise. So `load_blocks` dies at its `import_module("tooprolix")` —
   measured, `CORPUS_ROOT=/tmp python corpus/units.py --verify` →
   `ModuleNotFoundError: No module named 'tooprolix'`. The numbers this file produced are recorded
   in `corpus/REPORT.md`; reproducing them needs the extraction re-routed through the CLI first.
2. **The word count is checked against the CLI before anything is built on it.** `--verify` replays
   the shipped limits (200 docstring / 150 comment, strictly greater) over these blocks and
   compares the resulting addresses with the `TPX001`/`TPX002` findings in `corpus/runs/`. If they
   disagree the run aborts: every number below would then be about a different quantity than the
   rule's.
3. **Equal alert volume.** Each alternative unit gets the threshold that fires on the same number
   of blocks, per kind, as words does. Otherwise the comparison is between loudnesses.
4. **Blind presentation.** The disagreement set is written with the proposing unit stripped and
   ordered by the SHA-256 of the normalised text, so the order is a function of the block content
   and of nothing else. Labels are joined back afterwards by `--join`.

`tokens_raw` needs the *raw* text, which `prose_blocks` does not return; it is sliced out of the
source with the block's own `line_start`/`line_end`, so the slice is the extractor's coordinates
rather than a second opinion about where the block is.

    # ponytail: tiktoken is imported dynamically so that this file stays inside the repository's
    # stdlib-only Python surface for `ty` and needs no entry in `[dependency-groups]` for one
    # measurement. If tokens ever become more than a one-off comparison, declare the dependency.

Usage:
    CORPUS_ROOT=/somewhere/outside uv run --with tiktoken python3 corpus/units.py --verify
    CORPUS_ROOT=/somewhere/outside uv run --with tiktoken python3 corpus/units.py --emit  > blind.md
    CORPUS_ROOT=/somewhere/outside uv run --with tiktoken python3 corpus/units.py --join labels.tsv
"""

from __future__ import annotations

import argparse
import hashlib
import importlib
import json
import os
import subprocess
import sys
from collections.abc import Callable, Iterable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

#: The shipped defaults, from `src/detect/volume.rs`. The limit is the last size still allowed.
SHIPPED_LIMITS: dict[str, int] = {"docstring": 200, "comment": 150}

#: How far a calibrated alert count may sit from the requested one when ties make an exact match
#: impossible. One alert, and never silently: the printed table shows the count measured from the
#: firing set, not the count asked for.
MAX_ALERT_DRIFT: int = 1

#: The BPE the earlier corpus measurement used. Named, because the answer moves with it: on
#: Cyrillic prose cl100k charges 2.14 tokens per word against 1.01 on English.
ENCODING: str = "cl100k_base"

#: Runs whose blocks are not part of the AC1b population: `crewAI-full` is the same checkout as
#: `crewAI` at a root that cannot be measured at all (exit 2).
EXCLUDED_RUNS: frozenset[str] = frozenset({"crewAI-full"})


@dataclass(frozen=True)
class Block:
    """One prose block, as the shipped extractor reported it."""

    repo: str
    path: str
    line: int
    end_line: int
    kind: str
    normalized: str
    raw: str

    @property
    def address(self) -> tuple[str, int]:
        """The `(path, line)` pair the CLI prints, used to join with `corpus/runs/`."""
        return (self.path, self.line)

    @property
    def label(self) -> str:
        """
        Return the identifier `--emit` prints and `--join` looks up — one spelling, used by both.

        It was spelled `digest[:12]` at both ends. Two spellings of one key is how an emitted
        identifier and a looked-up identifier drift apart without any test noticing.
        """
        return self.digest[:12]

    @property
    def digest(self) -> str:
        """A stable identity for the block, from its text and address only."""
        payload = f"{self.normalized}\0{self.path}\0{self.line}".encode()
        return hashlib.sha256(payload).hexdigest()


Unit = Callable[[Block], int]


class CalibrationError(RuntimeError):
    """A unit could not be calibrated to the requested alert volume."""


class LabelError(RuntimeError):
    """The blind label set does not describe the disagreement set."""


class PopulationError(RuntimeError):
    """The walk did not produce the file set `corpus/run_all.sh` recorded."""


def count_words(block: Block) -> int:
    """Return the shipped unit: whitespace-separated words of the normalised text."""
    return len(block.normalized.split())


def count_chars_norm(block: Block) -> int:
    """Return the characters of the normalised text."""
    return len(block.normalized)


def _encoder() -> Any:
    """
    Return a loaded `tiktoken` encoder.

    `Any` is the honest type at a dynamic-import boundary and is confined to this one function:
    `tiktoken` is imported by name so the file stays inside the repository's stdlib-only Python
    surface, which means no static type is available for what comes back.
    """
    tiktoken = importlib.import_module("tiktoken")
    return tiktoken.get_encoding(ENCODING)


def token_counters() -> tuple[Unit, Unit]:
    """Return `(tokens_norm, tokens_raw)`, sharing one loaded encoder."""
    encoding = _encoder()

    def tokens_norm(block: Block) -> int:
        return len(encoding.encode(block.normalized))

    def tokens_raw(block: Block) -> int:
        return len(encoding.encode(block.raw))

    return tokens_norm, tokens_raw


def threshold_for(values: Sequence[int], alerts: int) -> int:
    """
    Return the threshold firing on exactly `alerts` of `values`, under `value > threshold`.

    # Raises
    `CalibrationError` if no threshold fires on exactly `alerts` blocks — because a run of blocks
    tied on one value cannot be split, or because `alerts` exceeds the population.

    **This used to return the closest achievable count and carry on.** Everything downstream is
    described as a comparison at "equal alert volume", so a silent 4-against-2 would have been
    published as one. On the run this task recorded every unit calibrated exactly, so aborting costs
    nothing today and refuses the failure a future re-pin could otherwise hide.
    """
    if not values:
        raise CalibrationError(f"cannot calibrate to {alerts} alerts over an empty population")
    candidates = sorted({*values, min(values) - 1}, reverse=True)
    for candidate in candidates:
        if sum(1 for value in values if value > candidate) == alerts:
            return candidate
    achievable = sorted({sum(1 for value in values if value > c) for c in candidates})
    raise CalibrationError(
        f"no threshold fires on exactly {alerts} of {len(values)} blocks; ties allow only {achievable}"
    )


def calibrate(blocks: Iterable[Block], unit: Unit, alerts_by_kind: Mapping[str, int]) -> dict[str, int]:
    """
    Return `{kind: threshold}`, one calibration per prose kind.

    A kind with no blocks is absent from the result rather than mapped to a threshold: a zero
    threshold would fire on every block of a population that was never measured.

    # Raises
    `CalibrationError` if any kind cannot be calibrated within [`MAX_ALERT_DRIFT`].
    """
    by_kind: dict[str, list[int]] = {}
    for block in blocks:
        by_kind.setdefault(block.kind, []).append(unit(block))
    return {kind: _calibrate_one(values, alerts_by_kind.get(kind, 0)) for kind, values in by_kind.items() if values}


def _calibrate_one(values: Sequence[int], alerts: int) -> int:
    """
    Return the threshold for `alerts`, drifting by at most [`MAX_ALERT_DRIFT`] when ties force it.

    Exact is always preferred. Some value distributions have no threshold that fires on exactly
    `alerts` blocks — `tokens_raw` over the docstring population steps 164 -> 166 with 165 asked for
    — and there the nearest reachable count within the cap is used, ties broken toward **fewer**
    alerts so no unit is handed more chances than words had. Beyond the cap it raises: at that point
    the units are being compared on loudness rather than on precision.
    """
    try:
        return threshold_for(values, alerts)
    except CalibrationError:
        pass
    for drift in range(1, MAX_ALERT_DRIFT + 1):
        for candidate in (alerts - drift, alerts + drift):
            if candidate < 0:
                continue
            try:
                return threshold_for(values, candidate)
            except CalibrationError:
                continue
    raise CalibrationError(f"no threshold fires on {alerts} of {len(values)} blocks, or within {MAX_ALERT_DRIFT} of it")


def firing(blocks: Iterable[Block], unit: Unit, thresholds: Mapping[str, int]) -> set[Block]:
    """Return the blocks `unit` flags at `thresholds`."""
    return {block for block in blocks if block.kind in thresholds and unit(block) > thresholds[block.kind]}


def precision_table(
    union: Iterable[Block], fired: Mapping[str, set[Block]], verdicts: Mapping[str, str]
) -> dict[str, tuple[int, int]]:
    """
    Return `{unit: (proposed, true)}` over the disagreement set.

    # Raises
    `LabelError` if `verdicts` does not hold **exactly one** `yes`/`no` for every block of `union`.
    It used to be a `dict.get(..., "")` compared against `"yes"`, so a missing, misspelled or
    invalid label silently became `no` and the run still exited 0 — one typo in the label file
    quietly moving a published precision number. Extra labels are equally fatal: they mean the
    labels were written against a different disagreement set from the one being scored.
    """
    blocks = list(union)
    expected = {block.label for block in blocks}
    missing = sorted(expected - set(verdicts))
    if missing:
        raise LabelError(f"no verdict for {len(missing)} block(s): {', '.join(missing)}")
    extra = sorted(set(verdicts) - expected)
    if extra:
        raise LabelError(f"{len(extra)} verdict(s) match no block: {', '.join(extra)}")
    invalid = sorted(
        f"{label}={verdict!r}" for label, verdict in verdicts.items() if verdict.strip().lower() not in {"yes", "no"}
    )
    if invalid:
        raise LabelError(f"verdicts must be yes or no: {', '.join(invalid)}")

    table: dict[str, tuple[int, int]] = {}
    for name, proposed in fired.items():
        selected = [block for block in blocks if block in proposed]
        true = [b for b in selected if verdicts[b.label].strip().lower() == "yes"]
        table[name] = (len(selected), len(true))
    return table


def disagreement(left: set[Block], right: set[Block]) -> set[Block]:
    """Return the symmetric difference: only blocks the two units decide differently about."""
    return left ^ right


def blind_order(blocks: Iterable[Block]) -> list[Block]:
    """
    Return the blocks ordered by the digest of their text and address.

    This is the blindness mechanism. It guarantees the presentation order carries no information
    about which unit proposed a block. It does **not** make the annotator unable to guess a unit
    from the block itself — a 900-word docstring is recognisably long whatever proposed it — and
    that limit is stated in `corpus/annotations.md` rather than papered over.

    There is no tie-break on the address, because `Block.digest` already hashes the address: two
    distinct blocks cannot collide on the key. A tie-break was here and was unreachable — dead code
    with a test that could not fail, which is worse than neither.
    """
    return sorted(blocks, key=lambda block: block.digest)


def walked_files(corpus_root: Path, root: str) -> list[str]:
    """
    Return the `.py` files under `root` that the CLI's walk reaches, relative to `corpus_root`.

    Produced by `rg --no-require-git --files --glob '*.py'`, which is **the same `ignore` crate
    with the same settings** the CLI builds its walk from: ancestor `.gitignore` files collected
    regardless of any git repository, hidden entries skipped, symlinks not followed. It is used
    rather than a hand-written walk because a probe that re-implements the thing it measures
    measures the probe — and rather than the CLI itself because the CLI reports findings, not the
    file set.

    That it is the *right* file set is not assumed: `--verify` replays the shipped limits over the
    blocks these files yield and requires the result to equal the CLI's own `TPX001`/`TPX002`
    findings exactly. A missing file loses a finding and a spurious one adds a finding, so both
    directions are caught by that equality — measured, the two agree on all 1290 volume blocks.

    Two independent points already pin the agreement: under such an ancestor this
    command reports 5 files for crewAI, which is exactly the truncation the CLI shows there; and
    on pydantic it reports 404 against `find`'s 405, the missing file being the hidden-directory
    one that accounts for the whole `120` vs `121` cluster discrepancy.
    """
    result = subprocess.run(
        ["rg", "--no-require-git", "--files", "--glob", "*.py", root],
        cwd=corpus_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode not in (0, 1):
        raise RuntimeError(f"rg failed on {root}: {result.stderr.strip()}")
    return sorted(line for line in result.stdout.splitlines() if line)


#: `run name -> (walk root, walked .py files)`.
#:
#: `corpus/run_all.sh` owns these — it is the only place that checks the pinned SHA, the clean
#: worktree and the walked count — and `tests/unit/test_units.py` fails if this copy drifts. The
#: file count is here because `verify` needs to pin the **population**, not only the findings.
RUNS: dict[str, tuple[str, int]] = {
    "OpenHands": ("OpenHands", 914),
    "crewAI": ("crewAI/lib/crewai", 754),
    "langgraph": ("langgraph", 445),
    "openai-agents-python": ("openai-agents-python", 834),
    "pydantic": ("pydantic", 404),
    "requests": ("requests", 37),
}


def check_population(counts: Mapping[str, int]) -> None:
    """
    Check the walked file counts against what `corpus/run_all.sh` recorded.

    # Raises
    `PopulationError`, naming every run that differs.

    Without this, `verify` constrains only the ~3% of blocks that carry a volume finding: dropping a
    finding-free file changes every calibrated threshold and the disagreement set while `verify`
    still reports "reproduced exactly". Measured by review on `requests/src/requests/models.py`
    (44 blocks, no finding): population 12 113 -> 12 069, `--verify` green.
    """
    drift = [
        f"{name}: walked {counts.get(name, 0)} .py files, run_all.sh recorded {files}"
        for name, (_, files) in RUNS.items()
        if counts.get(name, 0) != files
    ]
    if drift:
        raise PopulationError("; ".join(drift))


def load_blocks(corpus_root: Path, runs_dir: Path) -> list[Block]:
    """
    Extract every prose block of every measured run through the `tooprolix` Python export.

    **Raises `ModuleNotFoundError` as it stands**: that export was the pyo3 extension module, which
    the distribution no longer carries (`bindings = "bin"`). See the module docstring.
    """
    # The import is by name because a compiled extension's exports are invisible to a static
    # checker; the boundary is typed `Any` here and nowhere else.
    tooprolix: Any = importlib.import_module("tooprolix")
    blocks: list[Block] = []
    counts: dict[str, int] = {}
    for run in sorted(runs_dir.glob("*.json")):
        entry = RUNS.get(run.stem)
        if entry is None or run.stat().st_size == 0:
            continue
        root, _ = entry
        walked = walked_files(corpus_root, root)
        counts[run.stem] = len(walked)
        for relative in walked:
            source = (corpus_root / relative).read_text(encoding="utf-8", errors="replace")
            lines = source.splitlines()
            for kind, start, end, normalized in tooprolix.prose_blocks(relative, source):
                blocks.append(
                    Block(
                        repo=run.stem,
                        path=relative,
                        line=start,
                        end_line=end,
                        kind=kind,
                        normalized=normalized,
                        raw="\n".join(lines[start - 1 : end]),
                    )
                )
    check_population(counts)
    return blocks


def _cli_volume_addresses(runs_dir: Path) -> set[tuple[str, int]]:
    """Return the `(path, line)` of every `TPX001`/`TPX002` finding the CLI recorded."""
    addresses: set[tuple[str, int]] = set()
    for run in sorted(runs_dir.glob("*.json")):
        if run.stem in EXCLUDED_RUNS or run.stat().st_size == 0:
            continue
        report = json.loads(run.read_text(encoding="utf-8"))
        for finding in report["findings"]:
            if finding["code"] in {"TPX001", "TPX002"}:
                addresses.add((str(finding["path"]), int(finding["line"])))
    return addresses


def verify(blocks: Sequence[Block], runs_dir: Path) -> bool:
    """
    Replay the shipped limits over `blocks` and compare with what the CLI reported.

    Prints the mismatch and returns `False` if the two disagree. Everything downstream reads the
    word count off these blocks, so a silent disagreement would mean the comparison is about a
    different quantity from the one the rule uses.

    **What this does and does not constrain.** It compares *finding addresses*, so it pins the
    ~3% of blocks that carry a volume finding in both directions — a lost one disappears from the
    replay, an invented one appears. It says **nothing** about the other ~97%: dropping a
    finding-free file leaves this green while moving every calibrated threshold and the whole
    disagreement set. `check_population`, called from `load_blocks`, is what closes that, by pinning
    the walked file count against the column `corpus/run_all.sh` records.
    """
    replayed = {block.address for block in blocks if count_words(block) > SHIPPED_LIMITS.get(block.kind, 10**9)}
    reported = _cli_volume_addresses(runs_dir)
    if replayed == reported:
        print(f"verify: {len(reported)} volume findings reproduced exactly from the extractor")
        return True
    print(f"verify: MISMATCH — replayed {len(replayed)}, CLI reported {len(reported)}", file=sys.stderr)
    for address in sorted(replayed - reported):
        print(f"  only in replay: {address[0]}:{address[1]}", file=sys.stderr)
    for address in sorted(reported - replayed):
        print(f"  only in CLI:    {address[0]}:{address[1]}", file=sys.stderr)
    return False


def _units(blocks: Sequence[Block]) -> dict[str, Unit]:
    tokens_norm, tokens_raw = token_counters()
    return {"words": count_words, "chars_norm": count_chars_norm, "tokens_norm": tokens_norm, "tokens_raw": tokens_raw}


def _report_calibration(blocks: Sequence[Block]) -> tuple[dict[str, set[Block]], dict[str, Unit]]:
    units = _units(blocks)
    words_firing = {block for block in blocks if count_words(block) > SHIPPED_LIMITS.get(block.kind, 10**9)}
    alerts_by_kind: dict[str, int] = {}
    for block in words_firing:
        alerts_by_kind[block.kind] = alerts_by_kind.get(block.kind, 0) + 1

    print("| unit | kind | threshold | alerts | words alerts |")
    print("|---|---|---|---|---|")
    fired: dict[str, set[Block]] = {"words": words_firing}
    for name, unit in units.items():
        if name == "words":
            for kind, limit in sorted(SHIPPED_LIMITS.items()):
                print(f"| words | {kind} | {limit} | {alerts_by_kind.get(kind, 0)} | {alerts_by_kind.get(kind, 0)} |")
            continue
        thresholds = calibrate(blocks, unit, alerts_by_kind)
        fired[name] = firing(blocks, unit, thresholds)
        for kind in sorted(thresholds):
            # Counted from the firing set, not echoed back from `alerts_by_kind`. Both columns used
            # to print the same variable, so the table's "matched exactly" was a tautology — the
            # property is enforced by `CalibrationError`, but the printed evidence proved nothing.
            measured = sum(1 for block in fired[name] if block.kind == kind)
            print(f"| {name} | {kind} | {thresholds[kind]} | {measured} | {alerts_by_kind.get(kind, 0)} |")
    return fired, units


def main(argv: Sequence[str] | None = None) -> int:
    """Run the requested mode. Returns a process exit code."""
    parser = argparse.ArgumentParser(description="AC1b volume-unit comparison")
    parser.add_argument("--verify", action="store_true", help="replay the shipped limits and stop")
    parser.add_argument("--emit", action="store_true", help="write the blind disagreement set")
    parser.add_argument("--join", type=Path, help="join a TSV of `12-char digest<TAB>verdict` back to units")
    parser.add_argument("--runs", type=Path, default=Path(__file__).resolve().parent / "runs", help="run_all.sh output")
    args = parser.parse_args(argv)

    corpus_root = os.environ.get("CORPUS_ROOT")
    if not corpus_root:
        print("error: set CORPUS_ROOT to the directory the runs were produced from", file=sys.stderr)
        return 2

    try:
        blocks = load_blocks(Path(corpus_root), args.runs)
    except PopulationError as error:
        print(f"error: the walk is not the one run_all.sh recorded — {error}", file=sys.stderr)
        return 1
    print(f"blocks: {len(blocks)} from {len({block.repo for block in blocks})} runs", file=sys.stderr)

    if not verify(blocks, args.runs):
        return 1
    if args.verify:
        return 0

    fired, _ = _report_calibration(blocks)
    words_firing = fired["words"]
    union: set[Block] = set()
    print()
    print("| unit | fires | agrees with words | disagrees |")
    print("|---|---|---|---|")
    for name in ("chars_norm", "tokens_norm", "tokens_raw"):
        differing = disagreement(words_firing, fired[name])
        union |= differing
        print(f"| {name} | {len(fired[name])} | {len(words_firing & fired[name])} | {len(differing)} |")
    print(f"\nunion of symmetric differences: **{len(union)}** blocks\n")

    if args.join:
        verdicts = dict(line.split("\t", 1) for line in args.join.read_text().splitlines() if "\t" in line)
        try:
            table = precision_table(union, fired, verdicts)
        except LabelError as error:
            print(f"error: {error}", file=sys.stderr)
            return 1
        print("| unit | proposed | true | precision |")
        print("|---|---|---|---|")
        for name in ("words", "chars_norm", "tokens_norm", "tokens_raw"):
            proposed, true = table[name]
            share = f"{true / proposed:.3f}" if proposed else "n/a"
            print(f"| {name} | {proposed} | {true} | {share} |")
        return 0

    if args.emit:
        print("<!-- blind: the proposing unit is stripped; order is sha256 of the block text -->")
        for index, block in enumerate(blind_order(union), start=1):
            print(f"\n## {index}. `{block.label}` ({block.kind})\n")
            print("\n".join(f"    {line}" for line in block.raw.splitlines()))
            print("\n**Verbose prose? (yes/no):** \n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
