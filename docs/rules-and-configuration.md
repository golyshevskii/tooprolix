# Rules, suppressions, and configuration

What each `TPX` code measures, how to silence one deliberately, and how to configure the whole
repository.

For output format, exit codes, and the JSON schema, see [cli-contract.md](cli-contract.md).

## The rules

| Code | Detects | Status |
| --- | --- | --- |
| `TPX001` | A comment run longer than its word limit | Implemented |
| `TPX002` | A docstring longer than its word limit | Implemented |
| `TPX003` | One explanation repeated across comments and docstrings, reported once with every place it appears | Implemented |
| `TPX004` | Comments that restate the following code | Reserved |

`tooprolix --rules` prints these same three columns. The CLI renders them from one array in
`src/rules.rs`; this table is written by hand, and a test (`the_rules_listing_agrees_with_every_documented_table`)
compares the binary's output against the rows above, so the two cannot drift apart unnoticed.
`Reserved` is what `TPX004` is; the paragraph below says why.

### Volume boundaries

Volume is measured in **words** rather than lines or characters, and the limit is the last size still
allowed — a block of exactly the limit is silent, one word over is a finding.

A block must also span at least two physical lines to be considered at all. That threshold is why
one-line prose never produces a finding: measured on the reference corpus, the overwhelming majority
of exact duplicate pairs are one-line blocks like `"""Initialize the class."""`, and counting them
would bury every real result.

### Reserved rules

`TPX004` is reserved rather than disabled: evaluation on the reference corpus could not find a
setting that both flagged the intended case and stayed quiet on hand-cleaned code, so no rule ships
under that number. The code stays reserved so it cannot be reused for something else.

## Suppressions

Silence deliberate repetition, or a long block that earns its length, without disabling the rule for
the whole file. The marker goes on the physical line **directly above the block** — one rule for
comments and docstrings alike:

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

For a docstring that means *inside* the body, between `def`/`class` and the literal — **not** above
the `def` line.

### Marker grammar

```python
# !TPX002                          one rule
# !TPX001,TPX002                   several, comma-separated
# !TPX*                            every rule on this block
# !TPX002 the table below is the contract   anything after the codes is your reason
```

The space after `#` is required: `#!TPX002` is not a marker. That is deliberate — without the rule, a
shebang like `#!/usr/bin/env python` sitting above a module docstring would parse as a marker with an
unrecognised code.

`TPX*` is a **literal token, not a glob**. `TPX0*`, `TP*` and any other starred form are simply
unrecognised codes: they warn and suppress nothing.

Codes are upper-case. A typo in the code is loud — `TPX02`, `TPX999` and `tpx002` each produce a
warning naming the file and line. A comment directly above a block that looks like it was meant to be
a marker but is not one also warns, so a forgotten `!` does not fail silently.

## Repository configuration

Turn a rule off repository-wide, or move a limit, in `pyproject.toml`. The nearest one at or above
the checked path is used:

```toml
[tool.tooprolix]
ignore = ["TPX003"]
exclude = ["tests/fixtures", "vendor", "*_pb2.py"]
comment-max-volume = 150
docstring-max-volume = 200
```

A rule listed in `ignore` **cannot** be switched back on by a marker — configuration wins over an
in-file annotation, so disabling a rule centrally really disables it.

`exclude` is a measurement boundary, not a filter on findings: excluded paths are never opened, they
appear in the JSON document's `excluded` list, and the text output stays silent about them. Excluding
every measurable file is legal and reported on stderr.
