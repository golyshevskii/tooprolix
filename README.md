<p align="center">
  <img src="assets/tooprolix.gif" width="128" height="128" alt="Animated tooprolix cube artifact">
</p>

<h1 align="center">tooprolix</h1>

<p align="center">
  <strong>Less narration. More signal.</strong>
  <br>
  Deterministic control over comments and docstrings in Python repositories.
</p>

<p align="center">
  <a href="https://github.com/golyshevskii/tooprolix/actions/workflows/ci.yml"><img src="https://github.com/golyshevskii/tooprolix/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-12130f.svg" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/status-pre--release-f5c2c8.svg?labelColor=12130f" alt="Pre-release">
  <img src="https://img.shields.io/badge/Rust-powered-12130f.svg?logo=rust&logoColor=e4dfda" alt="Powered by Rust">
</p>

Coding agents can over-explain: comments grow into essays, docstrings repeat implementation details,
and the same rationale spreads across files. `tooprolix` turns that noise into explicit `TPX`
diagnostics for oversized and duplicated prose. It checks only comments and docstrings
in `*.py` files, never rewrites the useful **why**, and leaves the final decision to you.

## See the signal

```console
$ tooprolix check .
src/client.py:14: TPX003 same explanation in 3 places: src/poller.py:38, src/worker.py:91 (weakest src/client.py:14 ~ src/worker.py:91, similarity 0.812)
src/config.py:1: TPX002 docstring is 243 words long, over the 200-word limit — shorten it, or mark it with `# !TPX002` on the line above it
```

A cluster names its weakest link by name, not only by score. Grouping asserts a transitivity the
similarity measure does not have, so `weakest a ~ b` is the pair to read first — on a cluster of
twenty addresses that is the difference between a finding and a hint.

Findings go to stdout and diagnostics to stderr, sorted by address and byte-identical between runs.
`--format json` writes one versioned document — `{"schema_version", "findings"}` — including on a
clean run, so a consumer never has to tell an empty result from a crash.

The exit code is the contract: **0** nothing to report, **1** findings were printed, **2** the tree
could not be read — a bad path, a file that does not parse, or a broken `[tool.tooprolix]`. A failed
run prints *no* findings at all: a partial list from a tree that was never fully measured reads as a
verdict on it.

> [!NOTE]
> `tooprolix` is under active development. The prose extractor, all three shipping detectors
> (`TPX001`, `TPX002`, `TPX003`) and the command line above are implemented and exercised as a
> process. There is no PyPI release yet, so the command is not on PATH until
> `cargo run -- check <path>` becomes `tooprolix check <path>` at packaging time.

## Rules

| Code | Detects | Status |
| --- | --- | --- |
| `TPX001` | A comment run longer than its word limit | Shipping |
| `TPX002` | A docstring longer than its word limit | Shipping |
| `TPX003` | One explanation repeated across comments and docstrings, reported once with every place it appears | Shipping |
| `TPX004` | Comments that restate the following code | Reserved; not in 0.1.0 |

Volume is measured in words rather than lines or characters, and the limit is the last size still
allowed — a block of exactly the limit is silent, one word over is a finding. `TPX004` is reserved
rather than disabled: evaluation on the reference corpus could not find a setting that both flagged
the intended case and stayed quiet on hand-cleaned code, so no rule ships under that number.

Suppress deliberate repetition or a long block that earns its length, without disabling the rule for
a file. The marker goes on the physical line **directly above the block** — one rule for comments and
docstrings alike:

```python
# !TPX003
# The retry contract is restated here on purpose, so this module can be read on
# its own without opening the client it talks to.


def settle(batch):
    # !TPX002
    """Fold a batch into the ledger.

    The long explanation of why settlement waits for the nightly cut-off lives
    here rather than in the wiki, because it is the reason this function exists
    and it is the first thing anyone changing it needs to know.
    """
    return batch
```

For a docstring that means *inside* the body, between `def`/`class` and the literal — not above the
`def` line. Several codes are comma-separated, anything after them is your reason, and `# !TPX*`
silences every rule for that block — `TPX*` is a literal token and not a glob, so `TPX0*` is simply
an unrecognised code.

Turn a rule off repository-wide, or move a limit, in `pyproject.toml`. The nearest one at or above
the checked path is used, and a rule listed in `ignore` cannot be switched back on by a marker:

```toml
[tool.tooprolix]
ignore = ["TPX003"]
exclude = ["tests/fixtures", "vendor", "*_pb2.py"]
comment-max-volume = 150
docstring-max-volume = 200
```

## How it is tested

- Rules are exercised on real repositories, not only synthetic fixtures. The pinned corpus spans
  agent frameworks (`openai-agents-python`, CrewAI, LangGraph, OpenHands) and mature libraries
  (Pydantic, Requests) — six repositories, all measurable Python.
- Every `*.py` file is parsed for module, class, function, and async-function docstrings, plus runs
  of own-line comments. Generated, vendored, build, and environment files are excluded explicitly.
  Every threshold and word limit is read off that corpus rather than chosen by taste, and each is
  recorded next to the constant it sets, together with what it costs in opt-out markers.
- Guards are proved by mutation, not by passing: a threshold shifted by one, an ordering key
  replaced, or an unreadable checkout must turn a named test red. Tests that stayed green under such
  a mutation have been removed rather than kept for the count.
- `TPX003` is release-gated on at least 20 labelled findings and precision of at least 0.80. The
  volume limits are calibrated from the corpus against the cost of opting out, and their true-positive
  rate is not yet labelled — that measurement gates publication, not the detector.

See [`corpus/REPORT.md`](corpus/REPORT.md) for the pins, measurements, and unresolved evidence.

## Why tooprolix

- **Repository-wide.** It can find one explanation repeated across otherwise unrelated files.
- **Deterministic.** The same source produces the same ordered findings.
- **Local.** No model calls, network requests, or probabilistic output.

`tooprolix` complements [Ruff](https://github.com/astral-sh/ruff): Ruff owns Python code quality;
`tooprolix` focuses on the prose beside the code and uses Ruff's parser as its syntax foundation.

## Development

The project follows RED → GREEN → REFACTOR and keeps every gate explicit:

```console
make lint.check
make type
make test
make rust.fmt.check
make rust.lint
make rust.test
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the toolchain, release, and commit conventions.

<!--
Release-day cleanup:
- remove the pre-release notice;
- replace the status badge with PyPI version and supported-Python badges;
- update test counts or replace them with a generated badge;
- change rule statuses only after reference-corpus validation records the result;
- verify every install and output example against the published wheel.
-->
