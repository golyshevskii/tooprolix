# CLI and exit-code contract

Everything the command line guarantees: how a finding is addressed, which stream it lands on, what
the exit code means, and the shape of the JSON document.

For the rules themselves — thresholds, suppression markers, `pyproject.toml` — see
[rules-and-configuration.md](rules-and-configuration.md).

## The four invocations

```console
$ tooprolix check <path>... [--format text|json]
$ tooprolix --help
$ tooprolix --version
$ tooprolix --rules
```

`--help`, `--version` and `--rules` all exit **0** and write to stdout. An unknown subcommand or an
unknown option exits **2** and points at `tooprolix --help`.

`check` requires one or more explicit Python files or directories. Paths may appear before, between
or after the two `--format` forms. Every selected path feeds one combined report and one TPX003
input set; there is one summary or one schema-v2 JSON document, not one output per root.

Repeated and overlapping targets are canonicalised for identity only. Each physical file is
measured once, while findings retain the winning typed spelling: an explicitly named file wins over
the same file found by a directory walk, and the first explicit spelling wins between repeats. A
missing or unsupported explicit target stops the whole invocation before any report is written.

The nearest `pyproject.toml` is resolved independently from every target before sources are read.
All targets must resolve to the same configuration source; targets with no file share the default
context. Conflicting sources are an exit-2 startup error rather than an order-dependent choice.

### `--version`

One line, `tooprolix <version> (<commit date>)`. `-V` is the same thing, spelled ruff's way. No
worked example is frozen here: the string moves at every release, and nothing in the suite would go
red when the copy went stale.

The version comes from `Cargo.toml` — the one owner of that number, which is why `pyproject.toml`
declares `dynamic = ["version"]`. The date in brackets is the **date of the commit the binary was
built from, not the date it was built**, so two builds of one commit print the same line and the
string identifies what is running.

A build whose source tree has no git history **of its own**, and no `SOURCE_DATE_EPOCH`, prints
`unknown` rather than substituting today's date. "Of its own" is the load-bearing part: git's
discovery walks upward, so an unpacked sdist or a `cargo vendor` directory sitting inside some
unrelated checkout would otherwise report **that** repository's commit date as though it were this
package's. The build script requires the repository git finds to be rooted exactly at this package.

`SOURCE_DATE_EPOCH`, when set, wins over git. That is the [reproducible-builds
convention](https://reproducible-builds.org/docs/source-date-epoch/), and it is the only way a
package built from an archive rather than a checkout can carry a real date.

### `--rules`

Three columns — code, status, description — one rule per line, nothing else on stdout. The rows are
not repeated here: their one documented owner is the table in
[rules-and-configuration.md](rules-and-configuration.md), and
`the_rules_listing_agrees_with_every_documented_table` in `tests/cli.rs` requires that table to carry
the same codes, statuses, descriptions and order the binary prints. `--help` embeds the same lines,
and both come from one array in `src/rules.rs`.

Each of the three flags takes no other argument — `tooprolix --version --rules` is an error, not a
ranking, for the same reason `--format` given twice is. `--help` is the exception and ignores what
follows it.

## Finding addresses

Every finding points to `path:start-end`:

```console
src/config.py:1-26: TPX002 docstring is 243 words long, over the 200-word limit — shorten it, or mark it with `# !TPX002` on the line above it
```

The end line is the size of the problem, and it is the half a reader cannot infer. "243 words" does
not say whether that is 26 lines or 3; the address does, so you know where to stop before opening
the file.

Every address carries a range, because a block must span at least two physical lines to be reported
at all (see [rules-and-configuration.md](rules-and-configuration.md)). A single-line `path:line`
form is not something you will meet in output.

## Reading TPX003 clusters

A cluster names its weakest link by name, not only by score:

```console
src/client.py:14-31: TPX003 same explanation in 3 places: src/poller.py:38-52, src/worker.py:91-104 (weakest src/client.py:14-31 ~ src/worker.py:91-104, similarity 0.812)
```

Grouping asserts a transitivity the similarity measure does not have, so `weakest a ~ b` is the pair
to read first — on a cluster of twenty addresses that is the difference between a finding and a hint.

Past `MAX_RENDERED_LOCATIONS` (`src/finding.rs`, the one owner of that number) a cluster folds its
remaining addresses into a count. The JSON document always carries every address, so nothing is
lost, only folded.

## Output channels and ordering

Findings and their final summary go to **stdout**; diagnostics go to **stderr**. Finding lines are
sorted by address and byte-identical between runs. Piping stdout somewhere never loses a warning,
and never mixes one into the data.

A complete non-clean run ends with the total and each non-zero code count, in code order:

```text
Found 3 findings (TPX001: 1, TPX002: 1, TPX003: 1).
```

An incomplete run carries its skipped count in that final stdout line as well as keeping each
detailed reason on stderr:

```text
Found 1 findings (TPX001: 1); check incomplete: 1 file skipped.
No findings; check incomplete: 1 file skipped.
```

## Clean output and colour

When the tree was read whole and there is nothing to report:

```console
$ tooprolix check .
All checks passed!
```

Green on a terminal, plain everywhere else — in a pipe, a file, or with `NO_COLOR` set to anything
other than an empty string. It is printed **only** when the tree was read whole and the exit code is
0. A run that could not read part of the tree exits 1 and prints the explicit incomplete summary
above, never this success sentence. `--format json` never prints either text summary.

## Exit codes

| state | code |
| --- | --- |
| the tree was read whole, nothing to report | **0** |
| the tree was read whole, findings were printed | **1** |
| part of the tree could not be read — with or without findings | **1** |
| **the consumer stopped reading (a closed pipe)** | **0** |
| the run could not start: a bad path, a broken `[tool.tooprolix]` | **2** |
| the output could not be written for any other reason (a full disk) | **2** |

A file that does not parse, or that cannot be opened, does not fail the run: it is named on stderr
with the reason, the rest of the tree is still checked, and the findings that were reachable are
printed.

**A tree that was not read whole never exits 0** — that guarantee is what makes the rest safe, so
"no findings" and "not fully measured" are never the same answer.

### A consumer that stops reading

`tooprolix check . | head -5` exits **0** and prints nothing on stderr. The run itself succeeded —
it measured the tree and produced the answer — and a reader that closed the pipe has already
decided it has what it needs. It is deliberately **not** an exception to the guarantee above:
nothing went unmeasured, only undelivered.

Only a closed pipe is treated this way. Any other failure to write is exit **2** with the reason on
stderr, because there the answer genuinely did not reach its destination. Both cases are measured,
not asserted (2026-07-29, macOS/APFS, `aarch64-apple-darwin`):

| stdout is | exit | stderr |
|---|---|---|
| a filesystem with no space left (a 2 MB image filled to capacity) | **2** | `error: could not write to stdout: No space left on device (os error 28)` |
| a fd opened read-only, or a fd pointing at a directory | **2** | `error: could not write to stdout: Bad file descriptor (os error 9)` |

Both rows hold for the text format and for `--format json`, and the second holds for `--version`,
`--rules` and `--help` as well.

**The read-only-fd row is unix-only, and the reason is a standard-library behaviour worth knowing
about.** `std::io::stdout()` silently discards `EBADF`: writing to an unwritable descriptor through
it returns `Ok(())` while the bytes go nowhere. `tooprolix` therefore writes through a safely
duplicated descriptor. That duplication is not available in safe code off unix, so on a non-unix
platform an `EBADF`-shaped failure remains silent. It is specific to `EBADF` — a closed pipe and a
full disk both travel through the same code correctly.

**`--format json` piped to a reader that stops early yields a TRUNCATED document**, not a smaller
valid one. The JSON is a single document larger than any pipe buffer, so it cannot be written
atomically; if you stop reading, what you have is a prefix and parsing it is an error. Read the
whole document, or use the text format for streaming.

### Symlinked sources

A `*.py` symlink is not followed. It is reported as skipped, which makes the document incomplete and
the run exit 1. With no reachable findings, stdout is `No findings; check incomplete: 1 file
skipped.` and stderr carries the detail:

```console
warning: 1 file(s) skipped:
  vendor/alias.py: symlinks are not followed, so this file was not measured
```

Naming one directly (`tooprolix check alias.py`) still measures it — that is a request about one
file, not a claim about a tree.

## JSON contract

`--format json` writes one versioned document, including on a clean run, so a consumer never has to
tell an empty result from a crash. Exit 1 does not distinguish "the prose is bad" from "the
measurement is incomplete", so the document carries machine-readable completeness. All five keys
are present on **every** document, `schema_version` is `"2"`, and a v1 consumer fails loudly on it
rather than silently reading a partial result as a whole one:

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
