# Configuration preparation: before/after evidence

Measured 2026-11-07 against `78c11f437e106a4359342469125eea096015ad19`.
The CLI now reads/validates each discovered configuration once per invocation and
reuses its exclusion matcher. Public `config::load` still reads and validates afresh.
No dependencies, detector changes, public signatures or JSON schema changes.

## Whole-process results

Apple M4 Pro, macOS arm64, Rust 1.97.0; unchanged release settings (fat LTO,
one codegen unit). Two warmups, 21 paired measurements, alternating A/B order;
AC power before and after. Wall time includes process startup and captured output.
RSS is the median of three separate `/usr/bin/time -l` runs per binary.
[Raw samples, hashes and descriptive paired bootstrap intervals](CONFIG_REUSE.json).

| Workload | Files | Before ms | After ms | Time saved | RSS MiB before / after |
|---|---:|---:|---:|---:|---:|
| OpenHands-directory | 914 | 196.872 | 196.890 | -0.01% | 41.61 / 41.45 |
| OpenHands-explicit | 914 | 266.140 | 232.705 | 12.56% | 41.73 / 41.36 |
| crewAI-full-directory | 1269 | 222.982 | 223.015 | -0.01% | 33.67 / 33.89 |
| crewAI-directory | 754 | 113.991 | 114.332 | -0.30% | 24.52 / 24.33 |
| crewAI-explicit | 754 | 168.802 | 149.205 | 11.61% | 25.45 / 25.25 |
| langgraph-directory | 445 | 114.985 | 115.592 | -0.53% | 32.92 / 32.39 |
| langgraph-explicit | 445 | 143.743 | 133.868 | 6.87% | 33.11 / 32.67 |
| openai-agents-python-directory | 834 | 137.201 | 137.150 | 0.04% | 23.56 / 23.80 |
| openai-agents-python-explicit | 834 | 206.565 | 172.102 | 16.68% | 24.80 / 24.62 |
| pydantic-directory | 404 | 100.649 | 100.724 | -0.07% | 23.06 / 22.69 |
| pydantic-explicit | 404 | 136.020 | 117.513 | 13.61% | 22.69 / 22.81 |
| requests-directory | 37 | 11.168 | 11.232 | -0.57% | 8.42 / 8.47 |
| requests-explicit | 37 | 13.379 | 12.553 | 6.17% | 8.23 / 8.48 |
| synthetic-1-paths-0-globs | 1 | 2.053 | 2.093 | -1.94% | 3.12 / 3.12 |
| synthetic-100-paths-0-globs | 100 | 9.789 | 8.838 | 9.72% | 4.81 / 4.78 |
| synthetic-1-paths-100-globs | 1 | 2.428 | 2.433 | -0.24% | 3.47 / 3.45 |
| synthetic-100-paths-100-globs | 100 | 27.621 | 10.324 | 62.62% | 5.31 / 4.98 |

Positive time saved means faster. All six successful real-corpus explicit-file
runs improved by 6.2–16.7%; their descriptive paired 95% intervals for milliseconds
saved exclude zero. All seven directory-run intervals include zero; their median
changes are -0.57% to +0.04% saved. This is evidence on these workloads, not proof
of identical speed on every machine. Corpus median RSS shifts range from -0.53 to
+0.25 MiB; three samples do not establish a memory optimization. No CPU-temperature
control or Linux instruction-count comparison was performed.
The stripped release executable grew from 3,015,632 to 3,032,160 bytes
(+16,528 bytes, +0.55%); this is the measured code-size cost of the change.

## E2E equivalence and population integrity

Six fresh checkouts match every SHA in `corpus.lock`, with clean worktrees and no
ancestor `.gitignore`. Seven run roots have the expected 914 / 1269 / 754 / 445 /
834 / 404 / 37 Python files. Directory and explicit-path modes are compared
separately; they are not assumed equivalent to each other.

For all seven roots, JSON and text stdout, stderr and exit code match byte for byte
between baseline and candidate, including each timed run. Complete/skipped/excluded,
all findings, locations and weakest edges therefore match as well. Full crewAI
retains its five skipped templates. Every successful scan returns findings (exit 1);
this is not a benchmark of empty output or a crash. Synthetic runs produce exactly
one TPX001 finding per input file; TPX003 is disabled there to isolate config cost.

The full crewAI **explicit-file** case is intentionally not timed: both binaries
return exit 2 on the same template `pyproject.toml` containing `{{folder_name}}`.
Its JSON/text stdout, stderr and exit match. Reporting a fast startup failure as a
successful scan would be misleading. The other six explicit-file scans succeed.

Twenty additional configuration-sensitive process scenarios also match baseline,
including effective defaults, symlink config identity/exclusion base and diagnostic
ordering. The committed process tests cover these compatibility boundaries.

## Regression checks

- TDD: actual `prepare_walks` on three files first failed with `(reads, builds) =
  (3, 6)` against expected `(1, 1)`; the same test now passes with `(1, 1)` and
  verifies the returned configuration and file list. Counters are test-only.
- `cargo fmt --all -- --check`, clippy all targets with warnings denied, and rustdoc
  private items with warnings denied passed.
- `cargo test --locked --workspace`: 243 passed, 2 normally ignored.
- Both ignored tests were explicitly run in release and passed: reference corpus
  and adversarial line-rate budget. The 2000-file adversarial median was 1.713 s
  against a 5 s budget; this is not an A/B claim for the unchanged detector.
- `UV_LOCKED=1 uv run --only-group test pytest -q`: 557 passed.

One intentional clarification: an invocation uses the first validated snapshot of
each discovered config path. Later invocations and public `load` calls observe file
changes; a unit test checks this with a newly invalid glob. No live config reload
is promised while a single invocation runs. The snapshot test was added after the
implementation; the read/build-count test was the RED that preceded it.

## Reproduce

Build the baseline in a detached worktree at the commit above and build this branch
using `cargo build --release --locked --message-format=json`. Copy the executables
named by Cargo into a measurement directory as `baseline` and `candidate`.
Clone each URL at the exact SHA from `corpus.lock` into an external corpus directory
(no ancestor `.gitignore`). Write that directory to `corpus-root.txt` beside the
binaries. In the same directory write `corpus-integrity.json` with this structure,
one entry for each of the six pins:

```json
{"clones": [{"path": "/path/to/corpus/requests", "expected_sha": "69f84847045bef7a849cc994a26fe7ba8a169e95"}]}
```

The harness verifies each recorded checkout and all seven expected file counts.
Run it without concurrent builds/tests. Save the following harness (formatting normalized; Python AST verified identical to the executed version)
as `compare.py` beside the binaries and manifests, then run `python3 compare.py`.
It emits all raw samples and saves both formats of output for both binaries.
The pinned paths/counts are deliberate independent corpus expectations.

<details>
<summary>Standalone reproduction harness (stdlib only, macOS RSS)</summary>

```python
"""Compare immutable release executables on pinned corpus and config-heavy CLI inputs."""

import collections
import hashlib
import json
import random
import re
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

OUT = Path(__file__).resolve().parent
ROOT = Path((OUT / "corpus-root.txt").read_text().strip())
BINS = {name: OUT / name for name in ("baseline", "candidate")}
SUBJECTS = {
    "OpenHands": ("OpenHands", 914, 0),
    "crewAI-full": ("crewAI", 1269, 5),
    "crewAI": ("crewAI/lib/crewai", 754, 0),
    "langgraph": ("langgraph", 445, 0),
    "openai-agents-python": ("openai-agents-python", 834, 0),
    "pydantic": ("pydantic", 404, 0),
    "requests": ("requests", 37, 0),
}


def run(label, paths, fmt="json"):
    start = time.perf_counter_ns()
    result = subprocess.run(
        [str(BINS[label]), "check", *paths, "--format", fmt], cwd=ROOT, capture_output=True, timeout=120
    )
    elapsed = (time.perf_counter_ns() - start) / 1e6
    return (result.returncode, result.stdout, result.stderr), elapsed


def summarize(samples):
    before, after = samples["baseline"], samples["candidate"]
    deltas = [a - b for a, b in zip(before, after, strict=True)]
    rng = random.Random(761)
    boot = sorted(statistics.median(rng.choices(deltas, k=len(deltas))) for _ in range(4000))
    return {
        "samples_ms": samples,
        "median_ms": {k: statistics.median(v) for k, v in samples.items()},
        "min_max_ms": {k: [min(v), max(v)] for k, v in samples.items()},
        "paired_saved_ms_ci95": [boot[100], boot[3899]],
        "median_speedup": statistics.median(before) / statistics.median(after),
    }


def measure(name, paths, repeats=21):
    expected, _ = run("baseline", paths)
    if name == "crewAI-full-explicit":
        assert expected[0] == 2 and not expected[1]
        assert b"templates/crew/pyproject.toml" in expected[2] and b"TOML parse error" in expected[2]
        for fmt in ("json", "text"):
            reference, _ = run("baseline", paths, fmt)
            actual, _ = run("candidate", paths, fmt)
            assert actual == reference
        return {
            "name": name,
            "paths": paths,
            "exit": 2,
            "identical_json_text_stderr_exit": True,
            "startup_error": expected[2].decode(),
            "timing_omitted": "The old CLI already refuses template pyproject TOML; not a successful scan.",
        }
    assert expected[0] == 1, (name, expected)
    document = json.loads(expected[1])
    assert document["findings"], name
    assert document["schema_version"] == "2" and document["excluded"] == [], name
    outputs = OUT / "outputs"
    outputs.mkdir(exist_ok=True)
    for fmt in ("json", "text"):
        reference, _ = run("baseline", paths, fmt)
        actual, _ = run("candidate", paths, fmt)
        assert actual == reference, (name, fmt, "output/exit mismatch")
        for label, result in (("baseline", reference), ("candidate", actual)):
            (outputs / f"{name}.{label}.{fmt}").write_bytes(result[1])
            (outputs / f"{name}.{label}.{fmt}.stderr").write_bytes(result[2])
    samples = {key: [] for key in BINS}
    for iteration in range(repeats + 2):
        order = list(BINS) if iteration % 2 else list(reversed(BINS))
        for label in order:
            result, elapsed = run(label, paths)
            assert result == expected, (name, label, "non-deterministic or changed output")
            if iteration >= 2:
                samples[label].append(elapsed)
    rss = {key: [] for key in BINS}
    for iteration in range(3):
        for label in list(BINS) if iteration % 2 else list(reversed(BINS)):
            proc = subprocess.run(
                ["/usr/bin/time", "-l", str(BINS[label]), "check", *paths, "--format", "json"],
                cwd=ROOT,
                capture_output=True,
                timeout=120,
            )
            assert proc.returncode == expected[0] and proc.stdout == expected[1]
            match = re.search(rb"(\d+)\s+maximum resident set size", proc.stderr)
            assert match, proc.stderr
            rss[label].append(int(match[1]))
    row = {
        "name": name,
        "paths": paths,
        "exit": expected[0],
        "findings": dict(collections.Counter(f["code"] for f in document["findings"])),
        "complete": document["complete"],
        "skipped": len(document["skipped"]),
        "excluded": len(document["excluded"]),
        "identical_json_text_stderr_exit": True,
        "rss_bytes": rss,
        **summarize(samples),
    }
    print(json.dumps({k: row[k] for k in ("name", "median_ms", "paired_saved_ms_ci95", "median_speedup")}), flush=True)
    return row


def main():
    for entry in json.loads((OUT / "corpus-integrity.json").read_text())["clones"]:
        checkout = Path(entry["path"])
        assert (
            subprocess.check_output(["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True).strip()
            == entry["expected_sha"]
        )
        assert not subprocess.check_output(["git", "-C", str(checkout), "status", "--porcelain"])
    assert not any((p / ".gitignore").exists() for p in [ROOT, *ROOT.parents])
    results = {
        "binary_sha256": {k: hashlib.sha256(p.read_bytes()).hexdigest() for k, p in BINS.items()},
        "power_before": subprocess.check_output(["pmset", "-g", "batt"], text=True),
        "protocol": "21 paired samples after 2 warmups; alternating order; stdout/stderr/exit equality every timed run; RSS 3 runs separately verifies stdout/exit; CI paired bootstrap median saved ms, descriptive only",
        "cases": [],
    }
    for name, (subject, count, skipped) in SUBJECTS.items():
        files = sorted(
            subprocess.check_output(
                ["rg", "--no-require-git", "--files", "--glob", "*.py", subject], cwd=ROOT, text=True
            ).splitlines()
        )
        assert len(files) == count, (name, len(files), count)
        (OUT / f"{name}.files").write_text("\n".join(files) + "\n")
        for mode, paths in (("directory", [subject]), ("explicit", files)):
            row = measure(f"{name}-{mode}", paths)
            if row["exit"] == 1:
                assert row["skipped"] == skipped and row["complete"] == (skipped == 0), row
            row["input_files"] = count
            results["cases"].append(row)
        (OUT / "comparison.json").write_text(json.dumps(results, indent=2) + "\n")
    with tempfile.TemporaryDirectory(prefix="tooprolix-config-bench-") as directory:
        fixture = Path(directory)
        files = []
        for i in range(100):
            target = fixture / f"f{i:03}.py"
            target.write_text("# " + " ".join([f"word{i}"] * 151) + "\n")
            files.append(str(target))
        for globs in (0, 100):
            (fixture / "pyproject.toml").write_text(
                '[tool.tooprolix]\nignore = ["TPX003"]\nexclude = '
                + json.dumps([f"vendor-{i}/" for i in range(globs)])
                + "\n"
            )
            for n in (1, 100):
                row = measure(f"synthetic-{n}-paths-{globs}-globs", files[:n])
                assert row["findings"] == {"TPX001": n} and row["complete"]
                results["cases"].append(row)
    results["power_after"] = subprocess.check_output(["pmset", "-g", "batt"], text=True)
    (OUT / "comparison.json").write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    assert summarize({"baseline": [10.0] * 21, "candidate": [5.0] * 21})["paired_saved_ms_ci95"] == [5.0, 5.0]
    main()
```

</details>
