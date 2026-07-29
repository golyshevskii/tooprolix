# CLI and exit-code contract

Everything the command line guarantees: how a finding is addressed, which stream it lands on, what
the exit code means, and the shape of the JSON document.

For the rules themselves — thresholds, suppression markers, `pyproject.toml` — see
[rules-and-configuration.md](rules-and-configuration.md).

## The four invocations

```console
$ tooprolix check <path> [--format text|json]
$ tooprolix --help
$ tooprolix --version
$ tooprolix --rules
```

`--help`, `--version` and `--rules` all exit **0** and write to stdout. An unknown subcommand or an
unknown option exits **2** and points at `tooprolix --help`.

### `--version`

```console
$ tooprolix --version
tooprolix 0.3.1 (2026-07-28)
```

`-V` is the same thing, spelled ruff's way. The version comes from `Cargo.toml` — the one owner of
that number, which is why `pyproject.toml` declares `dynamic = ["version"]`.

The date in brackets is the **date of the commit the binary was built from, not the date it was
built**. Two builds of one commit, hours apart, print the same line; if that were the wall clock the
binary would not be reproducible and the string could not identify what is running. A build from a
tree with no git history and no `SOURCE_DATE_EPOCH` prints `unknown` rather than substituting
today's date — `unknown` is a true answer and a guess is not.

`SOURCE_DATE_EPOCH`, when set, wins over git. That is the [reproducible-builds
convention](https://reproducible-builds.org/docs/source-date-epoch/), and it is the only way a
package built from an archive rather than a checkout can carry a real date.

### `--rules`

```console
$ tooprolix --rules
TPX001  Implemented  A comment run longer than its word limit
TPX002  Implemented  A docstring longer than its word limit
TPX003  Implemented  One explanation repeated across comments and docstrings, reported once with every place it appears
TPX004  Reserved     Comments that restate the following code
```

Three columns — code, status, description — separated by spaces, one rule per line, nothing else on
stdout. `--help` embeds the identical lines, and both come from one array in `src/rules.rs`, so the
CLI cannot describe a rule differently from [rules-and-configuration.md](rules-and-configuration.md).

`TPX004` is listed as `Reserved` and is **not** an accepted code: `ignore = ["TPX004"]` is still a
fatal configuration error and `# !TPX004` still suppresses nothing. Being documented and being
accepted are different things, deliberately.

Each of the three flags takes no other argument — `tooprolix --version --rules` is an error, not a
ranking, for the same reason `--format` given twice is. `--help` is the exception and ignores what
follows it, which is behaviour from 0.1.0.

## Finding addresses

Every finding points to `path:start-end`:

```console
src/config.py:1-26: TPX002 docstring is 243 words long, over the 200-word limit — shorten it, or mark it with `# !TPX002` on the line above it
```

The end line is the size of the problem, and it is the half a reader cannot infer. "243 words" does
not say whether that is 26 lines or 3; the address does, so you know where to stop before opening
the file.

Every reported block spans at least two lines, so in practice every address carries a range.
One-line prose is never a finding at all — a block must span at least two physical lines to be
reported, which is why a single-line `path:line` form is not something you will meet in output.

### Migrating from 0.3.0

`path:start-end` is a **break** from 0.3.0's `path:line` for anything that parses the address
strictly. A tool that reads the leading `path:line` and stops at the first number is unaffected; one
that splits on `:` and requires an integer is not. Taken deliberately while nothing is published and
there are no consumers.

## Reading TPX003 clusters

A cluster names its weakest link by name, not only by score:

```console
src/client.py:14-31: TPX003 same explanation in 3 places: src/poller.py:38-52, src/worker.py:91-104 (weakest src/client.py:14-31 ~ src/worker.py:91-104, similarity 0.812)
```

Grouping asserts a transitivity the similarity measure does not have, so `weakest a ~ b` is the pair
to read first — on a cluster of twenty addresses that is the difference between a finding and a hint.

A cluster lists at most ten other addresses before folding the rest into a count. The JSON document
always carries every address, so nothing is lost, only folded.

## Output channels and ordering

Findings go to **stdout** and diagnostics to **stderr**, sorted by address and byte-identical between
runs. Piping stdout somewhere never loses a warning, and never mixes one into the data.

## Clean output and colour

When the tree was read whole and there is nothing to report:

```console
$ tooprolix check .
All checks passed!
```

Green on a terminal, plain everywhere else — in a pipe, a file, or with `NO_COLOR` set to anything
other than an empty string. It is printed **only** when the tree was read whole and the exit code is
0: a run that could not read part of the tree exits 1 and stays quiet, because the sentence would be
claiming a completeness the run does not have. `--format json` never prints it.

## Exit codes

| state | code |
| --- | --- |
| the tree was read whole, nothing to report | **0** |
| the tree was read whole, findings were printed | **1** |
| part of the tree could not be read — with or without findings | **1** |
| the run could not start: a bad path, a broken `[tool.tooprolix]` | **2** |

A file that does not parse, or that cannot be opened, does not fail the run: it is named on stderr
with the reason, the rest of the tree is still checked, and the findings that were reachable are
printed.

**A tree that was not read whole never exits 0** — that guarantee is what makes the rest safe, so
"no findings" and "not fully measured" are never the same answer.

### Symlinked sources

A `*.py` symlink is not followed. It is reported as skipped, which makes the document incomplete and
the run exit 1:

```console
warning: 1 file(s) skipped:
  vendor/alias.py: symlinks are not followed, so this file was not measured
```

Naming one directly (`tooprolix check alias.py`) still measures it — that is a request about one
file, not a claim about a tree.

## JSON contract

`--format json` writes one versioned document, including on a clean run, so a consumer never has to
tell an empty result from a crash. Since exit 1 no longer distinguishes "the prose is bad" from "the
measurement is incomplete", this is the only channel that carries completeness — all five keys are
present on **every** document, `schema_version` is `"2"`, and a v1 consumer fails loudly on it rather
than silently reading a partial result as a whole one:

```json
{
  "schema_version": "2",
  "complete": false,
  "skipped": [{"path": "vendor/fixture.py", "reason": "could not parse Python source: …"}],
  "excluded": ["tests/fixtures"],
  "findings": []
}
```

### Completeness

`skipped` is a **refusal** — the tool tried to read the file and could not — and it is the only thing
that sets `complete` to `false`. `excluded` is a **boundary**: `exclude` says a tree was never in
scope, so inside that scope the measurement really is whole and the text output stays silent about
it. A pruned directory appears as one path, not as the subtree behind it.

> [!IMPORTANT]
> On a `complete: false` document, `TPX003` clusters are a **different graph** — not the same
> duplicates minus a file. `TPX003` is cross-file by construction, so a missing block may have been
> the only bridge between two components: clusters that were one become two, and a cluster that falls
> below two members disappears entirely. Do not diff the findings of a partial run against a whole
> one and read the difference as churn in the repository. It is churn in the input set.
