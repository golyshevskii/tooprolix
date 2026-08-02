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
`src/rules.rs`; this table is written by hand, and `the_rules_listing_agrees_with_every_documented_table`
in `tests/cli.rs` requires the rows above to carry the same codes, statuses, descriptions and order
the binary printed, so the two cannot drift apart unnoticed.

### Volume boundaries

Volume is measured in **words** rather than lines or characters, and the limit is the last size still
allowed — a block of exactly the limit is silent, one word over is a finding.

A block must also span at least two physical lines to be considered at all. That threshold is why
one-line prose never produces a finding: on the reference corpus, one-line blocks like
`"""Initialize the class."""` are 98.83%–99.99% of all exact duplicate pairs (`corpus/REPORT.md`,
*Minimum prose-block size*), and counting them would bury every real result.

### What `TPX003` compares: narrative prose, not the reference scaffolding

`TPX003` does not compare the whole block. It compares the **narrative** — the block with its
API-reference scaffolding discarded first — because a finding says *"this explanation is written
twice, delete or merge one copy"*, and for a parameter table that advice cannot be taken. `help(post)`
and `help(put)` each need their own table, so merging them can only make the reference worse.

Measured on a hand-annotated sample (`corpus/annotations.md` §1.2–1.3): the templated
`requests.post`/`put`/`patch` docstrings, which repeat byte for byte apart from the HTTP verb, score
**0.898**, while a cluster annotated as a genuine finding goes down to **0.750**. The two classes
overlap, so no similarity threshold separates them — which is why the rule compares less rather than
demanding more.

Discarded, and nothing else is:

| Style | What is discarded |
| --- | --- |
| Google | a line that is exactly `Args:`, `Attributes:`, `Keyword Args:`, `Raises:`, `Returns:` or `Yields:`, plus the blank and deeper-indented lines under it |
| Sphinx / reST | a line opening `:param …:`, `:type …:`, `:return:`, `:returns:`, `:rtype:`, `:raise…:`, `:key…:`, `:var…:`, `:except…:` or `:yield…:`, plus its deeper-indented continuation lines. The closing colon must be followed by whitespace or the end of the line, and the six names that take no argument (`:return:`, `:returns:`, `:rtype:`, `:vartype:`, `:yield:`, `:yields:`) must stand alone — `` :return:`x` `` is a role and `:return policy:` is an unlisted field, and both stay narrative |
| NumPy | `Parameters`, `Returns`, `Raises`, `Yields`, `Attributes` or `Keyword Arguments` **underlined by a row of dashes exactly as long as the name** at the same indentation, plus its body up to the next underlined header. A mismatched underline is a malformed docstring, not a section, and the whole thing stays narrative |
| Examples | a fenced ` ``` ` block through its closing fence, and a `>>>` doctest run together with the `>>>`/`...` lines that continue it. A prompt must be followed by whitespace or end the line — `>>>text` is what `doctest` itself refuses to parse, so it stays narrative. The example's **output** is narrative and is kept |

Everything else stays narrative, on purpose and in this direction on purpose:

- a section header outside those lists — `Example:`, `Note:`, `Notes:`, `See Also` — is prose about
  the callable, and so is its body;
- an *inline* reST role such as ``:class:`Response` `` opening a line is a cross-reference inside a
  sentence, not a field entry;
- a `Returns` line with no dashes under it is an English word, not a NumPy section;
- an unterminated fence is not a fence;
- a comment run is narrative in full — the grammar reads a line's trimmed text, and `# Args:` is not
  `Args:`.

Every one of those is a case the parser does **not** recognise, and each one keeps more text in the
comparison. That bias is deliberate: an unrecognised construct can only preserve a finding, never
invent one, whereas the opposite bias would delete findings through a parser bug silently.

A block whose narrative is shorter than **three words** — a docstring that is nothing but a parameter
table, or one left with a bare `Send.` — takes no part in `TPX003` at all: once the table is
discarded there is not enough explanation left to have been said twice. Three is the shingle width,
so it is also the point below which the two comparison paths could not agree: a shorter text produces
no shingle at all. Such a block still counts in full towards `TPX001` and `TPX002`, which measure
volume rather than repetition.

**What this floor does not do.** Two unrelated callables whose narratives are the *same* templated
summary of three words or more are still reported as one finding. That is the templated-summary
class described in `corpus/annotations.md` §1.5, and §1.5 records the measurement showing it cannot
be separated by raising the similarity threshold either — a genuine finding sits at the same score.
Closing it needs a rule about templated summary lines, which is a judgement rather than a grammar.

The grammar lives in one function, `extract::narrative`, and its unit tests carry one case per style
plus the fail-safe cases above.

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

### `exclude` path shapes

At **most one** `pyproject.toml` is used per run: the nearest one at or above the path being
checked. If the search reaches the filesystem root without finding one, none is read and the
defaults apply. Every glob in the file that *is* found resolves against that file's own directory,
not against the working directory — so one rule means the same thing run from the project root and
run from a package inside it. Configuration is not hierarchical: a `pyproject.toml` that sits
*below* the checked path is never read, and its `exclude` does not apply to a run started above it.

The shape of the entry decides the depth:

| entry | what it selects |
|---|---|
| `vendor` | every `vendor` at any depth |
| `vendor/` | only the `vendor` **directory** beside this file |
| `./vendor` | only the `vendor` beside this file, file or directory |
| `/vendor` | **refused**, exit 2 — write `./vendor` |

Measured against ruff 0.16.0 on a tree holding `vendor/a.py` and `sub/vendor/b.py`: the first three
rows match it exactly. The fourth is a deliberate divergence, and the reason is that ruff has no one
behaviour to copy — a leading `/` means opposite things there depending on the shape that follows
it. Measured on that same tree: `exclude = ["/vendor"]` excludes **nothing**, while
`exclude = ["/*.py"]` and `exclude = ["/**/*.py"]` exclude **everything**. Picking either reading
would silently disagree with ruff for the other, so the whole class is refused instead.

The refusal quotes the entry back as written, minus the leading slash, so `/vendor/` is told to
write `./vendor/` — keeping the trailing slash, which is a narrower rule than `./vendor`. No
any-depth alternative is suggested: a glob containing a `/` is matched as one complete path relative
to the configuration file, so `vendor/generated` selects only the root one and never matches at any
depth.
