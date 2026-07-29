"""
AC3 — the release benchmark: a full run over a pinned repository, warm cache, well under 5 s.

**Every timed run is checked before it is timed into the median.** The harness this replaces was a
heredoc in `corpus/REPORT.md` that ran `subprocess.run(..., check=False)` and threw away the exit
code and the output; pointed at an always-failing binary it reported a green 5.356 ms. A benchmark
that times a crash is the release gate measuring nothing, so here an unexpected exit code or an
empty findings list **aborts the whole benchmark** rather than contributing a fast sample.

Every root below exits **1** — it has findings — so `EXPECT_EXIT` is 1 and not a wildcard. A clean
tree would legitimately exit 0 with no output, and that is exactly why the benchmark is not run on
one: this epic's verification policy counts a measurement taken on an empty detector output as red.

Timings are wall-clock on one machine and are **not** byte-reproducible; they are re-runnable. The
machine is recorded next to the numbers in `corpus/REPORT.md` §7.6.

Usage:
    CORPUS_ROOT=/somewhere/outside uv run python3 corpus/bench.py

`TOOPROLIX_BIN` selects the binary, exactly as it does for `corpus/run_all.sh` and
`corpus/determinism_check.sh`, and it is the ONLY way to select it — this script takes no
arguments. See `binary_from`.
"""

from __future__ import annotations

import json
import os
import statistics
import subprocess
import sys
import time
from collections.abc import Sequence
from pathlib import Path

#: `(run name, walk root)`, ordered by size. `crewAI` is the narrowed root the whole repository
#: cannot be measured at (five unparsable Jinja templates, exit 2). `corpus/run_all.sh` owns these
#: pairs; `tests/unit/test_bench.py` fails if this copy drifts from that table.
ROOTS: tuple[tuple[str, str], ...] = (
    ("requests", "requests"),
    ("pydantic", "pydantic"),
    ("langgraph", "langgraph"),
    ("crewAI", "crewAI/lib/crewai"),
    ("OpenHands", "OpenHands"),
    ("openai-agents-python", "openai-agents-python"),
)

#: The exit code every root above produces. Not a wildcard: see the module docstring.
EXPECT_EXIT: int = 1

#: Samples per root, plus one discarded run to warm the page cache.
SAMPLES: int = 10

#: Where `cargo build --release` puts the binary. The same fallback the two shell runners use.
DEFAULT_BINARY: Path = Path(__file__).resolve().parents[1] / "target/release/tooprolix"


def binary_from() -> Path:
    """
    Return the binary to measure: `$TOOPROLIX_BIN`, else the release build. One channel, no second.

    `corpus/run_all.sh` and `corpus/determinism_check.sh` both read
    `${TOOPROLIX_BIN:-$REPO_ROOT/target/release/tooprolix}`; this module used to read `argv[0]` and
    that fallback path and nothing else. So `TOOPROLIX_BIN=/somewhere/else` reached two of the three
    runners and was a silent no-op against the third — it went on timing `target/release/tooprolix`
    and reported a benchmark of a binary nobody had asked for. Substituting a binary is how this
    epic proves a harness measures what it says it measures, and a mutation one tool ignores is not
    a proof about that tool.

    There is deliberately **no** positional override. Ranking an argument above the variable put the
    divergence straight back: `TOOPROLIX_BIN=/A corpus/bench.py /B` measured B while both shells
    measured A. That the shells accept no positional is not agreement — they simply cannot express
    the choice. With the channel gone, "all three see the same substitution" is true by
    construction rather than by a precedence rule a reader has to trust.
    """
    override: str | None = os.environ.get("TOOPROLIX_BIN")
    # Resolved against the CALLER's cwd, once, before anyone runs it. The runs below pass
    # `cwd=CORPUS_ROOT` to `subprocess.run`, and a relative program path is resolved against the
    # CHILD's cwd — so a relative `TOOPROLIX_BIN` would silently mean a different file here than it
    # does to the shells' `-x` guard. `absolute()` and not `resolve()`: it matches `$PWD/$BINARY` in
    # the shells exactly, and it keeps the operator's own spelling in the error messages.
    return (Path(override) if override else DEFAULT_BINARY).absolute()


def time_run(binary: Path, root: str, *, expect_exit: int, cwd: Path | None = None) -> float:
    """
    Run `binary check <root>` once and return the wall-clock milliseconds.

    # Raises
    `RuntimeError` if the run exited with anything but `expect_exit`, or produced no output at all.
    Both are "this run measured nothing", and a benchmark that averages them in is worse than no
    benchmark, because it is believed.
    """
    started = time.perf_counter()
    try:
        done = subprocess.run([str(binary), "check", root], capture_output=True, cwd=cwd, check=False)
    except OSError as error:
        raise RuntimeError(f"{binary} could not be run: {error}") from error
    elapsed = (time.perf_counter() - started) * 1000

    if done.returncode != expect_exit:
        raise RuntimeError(
            f"{binary} check {root} gave exit {done.returncode}, expected {expect_exit}; "
            f"stderr: {done.stderr.decode(errors='replace').strip()[:200]}"
        )
    if not done.stdout.strip():
        raise RuntimeError(f"{binary} check {root} produced no output — nothing was measured")
    return elapsed


def verify_subject(binary: Path, name: str, root: str, runs_dir: Path, *, cwd: Path) -> None:
    """
    Check that `root` is the tree `corpus/runs/<name>.json` was recorded from, before timing it.

    Closing the crash direction left the subject direction open, and AC3 is a release gate: a
    substituted `CORPUS_ROOT` holding one long-docstring `x.py` per root gives every root exit 1 with
    non-empty output, so the checks in `time_run` all pass and the benchmark reports ~15 ms medians
    over a corpus that is not the corpus. A stale or dirty checkout passes the same way.

    The comparison is against the artifact `corpus/run_all.sh` produced, so the pinned SHA, the clean
    worktree and the walked-file count are verified **there** and are deliberately not restated here.
    One owner for those expectations; a second copy is the defect this epic keeps paying for.

    # Raises
    `RuntimeError` if the artifact is missing or unreadable, if the binary cannot be run at all, or
    if the findings differ from it.
    """
    recorded = runs_dir / f"{name}.json"
    if not recorded.is_file():
        raise RuntimeError(
            f"{recorded} does not exist; run corpus/run_all.sh first — timing a tree that has no "
            f"recorded measurement is timing an unknown tree"
        )
    # The same `OSError` handling `time_run` has always had. It is needed HERE too because this
    # check now runs first, so a mistyped `TOOPROLIX_BIN` reaches the binary through this call and
    # not through the timer — and `main` only catches `RuntimeError`, so without this the harness
    # died with a raw `FileNotFoundError` instead of its own abort line.
    try:
        done = subprocess.run(
            [str(binary), "check", root, "--format", "json"], capture_output=True, cwd=cwd, check=False
        )
    except OSError as error:
        raise RuntimeError(f"{binary} could not be run: {error}") from error
    try:
        measured = json.loads(done.stdout)["findings"]
    except (ValueError, KeyError) as error:
        raise RuntimeError(f"{binary} check {root} --format json did not produce a report: {error}") from error
    # The recorded artifact is read inside a `try` for the same reason the measured output is: a
    # run killed midway leaves a truncated `runs/<name>.json` and a schema bump leaves one without
    # `findings`, and `main` catches only `RuntimeError`. Without this the harness tracebacked while
    # the docstring above promised an abort for an unreadable artifact.
    try:
        expected = json.loads(recorded.read_text(encoding="utf-8"))["findings"]
    except (OSError, ValueError, KeyError) as error:
        raise RuntimeError(
            f"{recorded.name} is not a readable run artifact ({type(error).__name__}: {error}); "
            f"re-record it with corpus/run_all.sh — timing against an unreadable measurement is "
            f"timing against nothing"
        ) from error
    if measured != expected:
        raise RuntimeError(
            f"{root} does not match {recorded.name}: {len(measured)} findings measured against "
            f"{len(expected)} recorded — this is not the tree run_all.sh verified"
        )


def main(argv: Sequence[str] | None = None) -> int:
    """Print the median/min/max per root. Returns a process exit code."""
    argv = list(argv or [])
    if argv:
        # Refused rather than ignored. Ignoring it would time the default binary while the operator
        # believed they had selected the one they named — the same "measured something else" that
        # `binary_from` exists to close.
        print(
            f"error: corpus/bench.py takes no arguments; select the binary with TOOPROLIX_BIN. Got: {argv}",
            file=sys.stderr,
        )
        return 2
    corpus_root = os.environ.get("CORPUS_ROOT")
    if not corpus_root:
        print(
            "error: set CORPUS_ROOT to the checkouts directory (no ancestor .gitignore — see corpus/run_all.sh)",
            file=sys.stderr,
        )
        return 2
    binary = binary_from()

    runs_dir = Path(__file__).resolve().parent / "runs"

    print(f"{'root':<28} {'median':>10} {'min':>10} {'max':>10}")
    for name, root in ROOTS:
        try:
            verify_subject(binary, name, root, runs_dir, cwd=Path(corpus_root))
            time_run(binary, root, expect_exit=EXPECT_EXIT, cwd=Path(corpus_root))  # warm
            samples = [time_run(binary, root, expect_exit=EXPECT_EXIT, cwd=Path(corpus_root)) for _ in range(SAMPLES)]
        except RuntimeError as error:
            print(f"\nBENCHMARK ABORTED on {root}: {error}", file=sys.stderr)
            # A `continue` here re-arms the defect this module exists to close: every `time_run`
            # test stays green and the benchmark exits 0 having measured nothing.
            # `TestTheBenchmarkStopsWhenARunFails` is what refuses that.
            return 1
        print(f"{root:<28} {statistics.median(samples):9.1f}ms {min(samples):9.1f}ms {max(samples):9.1f}ms")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
