<p align="center">
  <img src="assets/tooprolix.gif" width="72" height="72" alt="#TPX">
</p>

<h1 align="center">tooprolix</h1>

<p align="center">
  <strong>Less narration. More signal.</strong>
  <br>
  Deterministic control over comments and docstrings in Python repositories.
</p>

<p align="center">
  <img src="https://img.shields.io/pypi/v/tooprolix?color=CCCCCC&labelColor=12130f" alt="PyPI">
  <img src="https://img.shields.io/badge/Rust-powered-CCCCCC.svg?logo=rust&labelColor=12130f" alt="Powered by Rust">
  <img src="https://img.shields.io/pypi/pyversions/tooprolix?color=CCCCCC&logo=python&labelColor=12130f" alt="Python">
</p>

Coding agents over-explain. Comments grow into essays, docstrings restate the code, and the same
rationale quietly spreads across five files. `tooprolix` turns that into `TPX` diagnostics you can
act on. It reads only comments and docstrings, never rewrites your **why**, and leaves the call to
you.

## Quick start

Install `tooprolix` with [**uv**](https://docs.astral.sh/uv/):

```console
$ uv add tooprolix
```

Then:

```console
$ tooprolix check .
src/config.py:1-26: TPX002 docstring is 243 words long, over the 200-word limit — shorten it, or mark it with `# !TPX002` on the line above it
src/client.py:14-31: TPX003 same explanation in 3 places: src/poller.py:38-52, src/worker.py:91-104 (weakest src/client.py:14-31 ~ src/worker.py:91-104, similarity 0.812)
```

Each finding points to `path:start-end`, so you know how big the block is before you open it.

And when the whole tree came back clean:

```console
$ tooprolix check .
All checks passed!
```

Exit codes are the contract: `0` clean, `1` findings, `2` could not start. A tree that was not read
whole **never** exits 0. See the [CLI contract](docs/cli-contract.md).

## Rules at a glance

| Code | Detects | Status |
| --- | --- | --- |
| `TPX001` | A comment run longer than its word limit | Released |
| `TPX002` | A docstring longer than its word limit | Released |
| `TPX003` | One explanation repeated across comments and docstrings, reported once with every place it appears | Released |
| `TPX004` | Comments that restate the following code | Reserved |

`tooprolix --rules` prints this table; `tooprolix --version` prints the version and the date of the
commit it was built from.

Some prose earns its length. Put a marker on the line directly above the block:

```python
# !TPX003
# The retry contract is restated here on purpose, so this module can be read on
# its own without opening the client it talks to.
```

Several codes, blanket suppression, and repository-wide config live in
[rules and configuration](docs/rules-and-configuration.md).

## Validation

Rules are exercised on six pinned real repositories agent frameworks and mature libraries not
only synthetic fixtures. Every threshold is read off that corpus rather than chosen by taste, and
critical guards are mutation-proved: a test that stays green when its guarantee is broken is deleted,
not kept for the count.

## Why tooprolix

- **Repository-wide.** It finds one explanation repeated across otherwise unrelated files.
- **Deterministic.** The same source produces the same ordered findings, byte for byte.
- **Local.** No model calls, no network, no probabilistic output.

## Documentation

| | |
| --- | --- |
| [CLI and exit-code contract](docs/cli-contract.md) | Addresses, output channels, colour, exit codes, JSON schema |
| [Rules and configuration](docs/rules-and-configuration.md) | Thresholds, suppression markers, `pyproject.toml` |
| [`corpus/REPORT.md`](corpus/REPORT.md) | The corpus, the measurements, the open questions |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Toolchain, gates, commit and release conventions |

## Contributing

Run `make help` for the task list, and see [CONTRIBUTING.md](CONTRIBUTING.md) for the toolchain and
the gates CI requires.
