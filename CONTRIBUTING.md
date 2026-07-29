# Contributing to tooprolix

## Conventional Commits are mandatory

Every commit message must follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <summary in the imperative>
```

Types in use: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `chore`, `ci`, `build`.

This is not a style preference — it is load-bearing. release-plz reads these messages to compute the
next version number and to write `CHANGELOG.md`. A commit that does not parse contributes nothing to
the changelog, and a `fix:` mislabelled as `chore:` silently skips a release.

A breaking change is marked with `!` (`feat!: …`) or a `BREAKING CHANGE:` footer.

## Never edit versions or the changelog by hand

- `[package] version` in `Cargo.toml` — written by release-plz.
- `CHANGELOG.md` — written by release-plz.
- `pyproject.toml` has no version field at all: `dynamic = ["version"]` takes it from `Cargo.toml`,
  so there is exactly one number to bump and no way for the two to disagree.

Merging the Release PR that release-plz opens is what creates the tag and the GitHub release.
Publishing to PyPI and crates.io is **off** until the packaging task turns it on. There is exactly
one switch for that: `[workspace] publish = false` in `release-plz.toml`.

> **Do not add `publish = false` to `Cargo.toml`.** It reads like the obvious way to say "not
> published yet", and it silently disables release-plz entirely — no version bump, no changelog, no
> tag, no release, and `release-plz update` still exits 0. `Cargo.toml` has a comment at that spot
> saying so. Its absence there is intentional, not drift.

### Release-day checklist

The repository currently describes itself as pre-release in several places. When the first published
release goes out, all of them move together:

- replace the `status-pre--release` badge in `README.md` with PyPI version and supported-Python
  badges — that badge is currently the **only** thing marking the project unpublished, since the
  README's examples deliberately show the installed `tooprolix check` command;
- change rule statuses from `Implemented` to `Released` in **three** places — `README.md`,
  `docs/rules-and-configuration.md`, and the `status` field of `CATALOGUE` in `src/rules.rs`, which
  is what `tooprolix --rules` prints — and only after reference-corpus validation records the
  result. The three cannot be flipped separately:
  `the_rules_listing_agrees_with_every_documented_table` in `tests/cli.rs` turns each line the
  binary printed into a table row and requires the `| \`TPX…` rows of both Markdown files to be
  exactly that list, in order, so changing one of the three reddens the suite;
- refresh the worked `--version` example in `docs/cli-contract.md` (`tooprolix 0.3.1 (2026-07-28)`).
  It is a commit date, so it goes stale at every release and **nothing watches it** — the rule
  tables have a test, this line does not, and writing one would mean pinning a date that is
  supposed to move;
- verify every install and output example against the **published wheel**, not a local build.

## Run the gates before you push

CI runs all seven of these on every pull request:

```bash
make lint.check       # ruff format --check + ruff check   -> CI job "lint"
make type             # ty                                 -> CI job "type"
make test             # pytest, tests/unit/                 -> CI job "test"
make rust.fmt.check   # cargo fmt --check                   -> CI job "cargo-fmt"
make rust.lint        # cargo clippy -D warnings            -> CI job "cargo-clippy"
make rust.test        # cargo test                          -> CI job "cargo-test"
make rust.doc         # cargo doc, warnings are errors      -> CI job "cargo-doc"
```

`make rust.doc` is the rustdoc gate. `cargo doc` exits 0 on a broken intra-doc link, so before it
existed the crate carried five rustdoc diagnostics with every other job green — one of them a link
to a function that had been renamed away. `RUSTDOCFLAGS="-D warnings"` is what makes them fail.

An eighth job, `coverage`, runs `make cov` — see below. **It is deliberately not a required check**:
it protects the measuring instrument (that the coverage toolchain still resolves and that the report
grader still accepts a real run), not the shipped artifact.

**None of them is enforced by branch protection today — not one.** This paragraph used to say that
`lint`, `type` and `test` were required; that was measured false on 2026-07-29:
`gh api repos/golyshevskii/tooprolix/branches/main/protection` returns **404** and
`.../rulesets` returns **403 "Upgrade to GitHub Pro or make this repository public"**. A private
repository without Pro cannot have branch protection at all, so every job here runs and reports
and none of them can block a merge.

Consequence, and it is the reason this is written down rather than quietly corrected: **a pull
request with every Rust gate red is still mergeable.** Read the job results yourself before
merging; do not treat a green merge button as a green build. Registering the required set is part
of making the repository public, and it is owned by the `flip-public-and-publish-to-pypi` task.

`make rust.fmt` and `make lint.fix` are the fixing counterparts. `make help` lists everything.

## Toolchains

- Rust is pinned in `rust-toolchain.toml` (**1.97.0**, with `clippy` and `rustfmt`). Do not run the
  gates on `stable` — the pinned `ruff_*@0.0.6` crates require rustc ≥ 1.95, and an older `stable`
  fails before compiling any of this crate.
- Python comes from `uv` (≥ 3.12). Use `uv run python3`, never a bare `python3`.

## Working on the Rust extension from Python

`uv sync` **does not rebuild** after a Rust edit — it reports `Checked 1 package` and leaves the
previous binary installed, so a Python-side check would silently pass against stale code.

**Deleting the venv does not help either.** With `src/lib.rs` edited to raise `RuntimeError`, a full
`rm -rf .venv && uv sync` still installed a wheel that raised `ValueError`: uv's build cache is not
invalidated by the Rust source change. Always use:

```bash
make py.build         # uv sync --reinstall-package tooprolix
```

## Coverage

Coverage is measured and printed; it is **not** published. There is no badge and no committed
artifact — the repository is private until the PyPI flip so no badge host can read it, and the
projects worth comparing against publish none either (ruff, uv, tokio, serde, ripgrep, cargo,
maturin, polars, httpx, starlette). Revisit at publication.

```bash
make rust.cov         # cargo llvm-cov -> prints "rust coverage: NN.N%"
make py.cov           # pytest --cov   -> prints "python coverage: NN.N%"
make cov              # both                                        -> CI job "coverage"
```

Both write their machine-readable report to `target/coverage/` and nothing else.

`make rust.cov` needs one tool that this repository does not pin for you — `cargo llvm-cov` is a
separate cargo subcommand crate, not a rustup component:

```bash
cargo install cargo-llvm-cov --locked
```

The `llvm-tools` component it shells out to *is* pinned, in `rust-toolchain.toml`. Without the
subcommand you get `error: no such command: llvm-cov`; without the component `cargo llvm-cov` fails
looking for `llvm-profdata`.

**`scripts/coverage_report.py` will refuse a run rather than print a flattering number**, and that
is the part worth keeping in mind when you touch coverage configuration. A percentage is trivially
raised by measuring less — dropping `branch = true`, orphaning a `.rs` file from the module tree,
adding a `corpus/` subdirectory coverage.py cannot discover — and none of those leave any other
trace. The script walks the source tree itself and compares it against what the report claims to
have measured, so those edits fail the run with a message naming the file. If one fires, fix the
denominator; do not silence the check.

Do not add a `--fail-under` threshold either: picking one before the code audits would be picking a
number to match today's code.

What the two numbers do **not** include, because a percentage implies it measured everything it
could:

- **`build.rs` is invisible to the Rust number.** It is a build script, compiled and run on the host
  before the crate exists, so `cargo llvm-cov` never instruments it and it appears in no row of the
  report. Its ~190 lines are unmeasured, not covered.
- **Rust branch coverage is not reported at all** on the pinned stable toolchain — llvm-cov's
  `Branches` column reads `-`. The Rust number is **line** coverage; the Python number folds branches
  in (`branch = true`). They are two different measures, which is why they are never added together
  into a single figure and why neither is described as comparable to the other.
- **The 6 doctests are not in the Rust number.** `cargo llvm-cov` skips doctests unless
  `--doctests` is passed, and that flag needs a nightly toolchain — on the pinned 1.97.0 it fails
  with `error: 2 nightly options were parsed`. So `make rust.cov` instruments **188 of the 194**
  tests `make rust.test` runs, and code reached only by a doctest counts as uncovered.
- **The Python number measures `corpus/` only** (`[tool.coverage.run] source` in `pyproject.toml`),
  which is the throwaway research tooling — the product itself is Rust. `tests/unit` is the runner's
  input, never part of the denominator: a denominator containing the tests climbs when you write
  more tests.

## Snapshot tests

`src/snapshots/*.snap` are [insta](https://insta.rs) snapshots. **CI runs `cargo-test` with
`INSTA_UPDATE=no`**, so a new or changed snapshot is a hard failure there — it is never written for
you. Locally, a changed snapshot leaves a `.snap.new` beside it; accept it only after reading it:

```bash
cargo insta review     # interactive diff, `a` accepts
```

`cargo-insta` is a local developer tool (`cargo install cargo-insta --locked`, version matched to
the `insta` pin in `Cargo.toml`); it is not a repository dependency. If it is unavailable, read the
`.snap.new` and rename it by hand. **Never accept a snapshot you have not read** — that is how a bug
gets committed as intent.

Snapshots are taken against fixtures under `tests/fixtures/`, never against files that are edited
for reasons unrelated to the code under test — otherwise fixing a typo turns CI red.

## Dependency pins

`ruff_python_parser` and `ruff_python_ast` are pinned with `=` to an exact version. They are ruff's
internal `0.0.x` crates with no semver guarantee, so a patch bump may break the build. Bump them
deliberately, in their own commit, with the tests as the evidence.
