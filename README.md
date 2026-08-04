<p align="center">
  <img src="assets/tooprolix.gif" width="68" height="68" alt="">
</p>

<h1 align="center">tooprolix</h1>

<p align="center">
  <strong>Less narration. More signal.</strong>
  <br>
  Deterministic control over comments and docstrings in Python repositories.
</p>

<p align="center">
  <img src="https://img.shields.io/pypi/v/tooprolix?color=CCCCCC&labelColor=12130f" alt="PyPI">
  <img src="https://img.shields.io/python/required-version-toml?tomlFilePath=https%3A%2F%2Fraw.githubusercontent.com%2Fgolyshevskii%2Ftooprolix%2Fmain%2Fpyproject.toml&color=CCCCCC&logo=python&labelColor=12130f" alt="Python">
  <img src="https://img.shields.io/badge/Rust-powered-CCCCCC.svg?logo=rust&labelColor=12130f" alt="Powered by Rust">
</p>

Coding agents over-explain. Comments grow into essays, docstrings restate the code, and the same
rationale quietly spreads across five files. `tooprolix` turns that into `TPX` diagnostics you can
act on. It reads only comments and docstrings, never rewrites your **why**, and leaves the call to
you.

## Quick start

Run `tooprolix` without installing it with [**uv**](https://docs.astral.sh/uv/):

```console
$ uvx tooprolix check .
src/config.py:1-26: TPX002 docstring is 243 words long, over the 200-word limit — shorten it, or mark it with `# !TPX002` on the line above it
Found 1 findings (TPX002: 1).
```

Each finding points to `path:start-end`, so you know how big the block is before you open it.

For an existing uv project, pin the tool as a development dependency and run it through uv:

```console
$ uv add --dev tooprolix
$ uv run tooprolix check .
```

And when the whole tree came back clean:

```console
$ uvx tooprolix check .
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
