# Contributing to tooprolix

## Pull request titles and release versions

Every ordinary pull request title must follow
[Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<optional scope>)!: <imperative summary>
```

The `!` is optional and marks a breaking change. Allowed types are `feat`, `fix`, `perf`,
`refactor`, `test`, `docs`, `chore`, `ci` and `build`.

release-plz derives the next version from the commits that land on `main`. The repository uses this
small merge policy:

- ordinary contributor PRs are **squash-merged**, so the validated PR title is the landed commit;
- `release-plz-*` Release PRs use a **merge commit**, preserving the exact reviewed release commit;
- rebase merge is disabled.

GitHub cannot select a merge method by PR kind, so the maintainer applies the Release PR exception.
Do not edit `Cargo.toml` versions or `CHANGELOG.md` by hand; release-plz owns both.

The version effect is release-plz's native Cargo SemVer behaviour, measured with release-plz
0.3.160:

| landed commit | while the crate is `0.x` | after `1.0.0` |
|---|---|---|
| `fix:` and other non-breaking types | patch | patch |
| `feat:` | patch | minor |
| `feat!:` / `fix!:` / `BREAKING CHANGE:` | minor | major |

While the crate is `0.x`, a feature is therefore still a patch and a breaking change is a minor.
There is no automatic major release before `1.0.0`; choosing `1.0.0` is a deliberate API-stability
decision. Do not enable `features_always_increment_minor`: release-plz documents that it violates
Cargo SemVer for `0.x`.

## `v0.3.4` shipped a breaking change as a patch, and is knowingly left as is

Recorded so it is not rediscovered as a bug. `v0.3.4` should have been `v0.4.0`; the tag, the GitHub
release and the CHANGELOG entry are live and all say patch. It is deliberately **not** retagged and
neither `Cargo.toml` nor `CHANGELOG.md` is hand-edited — the rule above forbids exactly that, and the
rule stands. The cost is bookkeeping only for as long as the repository is **private** — the tag and
its GitHub release become user-facing the moment visibility flips, which is before anything is
published. **Owner: the `flip-public-and-publish-to-pypi` task**, which chooses the first *published*
version deliberately.

## Never edit versions or the changelog by hand

- `[package] version` in `Cargo.toml` — written by release-plz.
- `CHANGELOG.md` — written by release-plz.
- `pyproject.toml` has no version field at all: `dynamic = ["version"]` takes it from `Cargo.toml`,
  so there is exactly one number to bump and no way for the two to disagree.

Merging the Release PR that release-plz opens is what creates the tag and the GitHub release.
Neither registry receives anything today, and **the two are off for different reasons**:

- **crates.io is off by a switch** — `[workspace] publish = false` in `release-plz.toml` keeps
  release-plz to version, changelog, tag and GitHub release, never `cargo publish`;
- **PyPI is off because nothing uploads to it** — there is no switch. `build-artifacts.yml` builds,
  checks and attaches artifacts to the run, and opens with "THERE IS NO PUBLISH STEP IN THIS FILE,
  AND THAT IS THE POINT OF IT"; the publishing task adds that job.

Neither is a one-line flip: `release-plz.yml` also withholds `CARGO_REGISTRY_TOKEN` as a backstop,
and PyPI has no publish job to enable.

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
- verify every install and output example against the **published wheel**, not a local build.

Nothing on this list is a version or date string copied into prose: `docs/` deliberately carries no
frozen `--version` example, because nothing would go red when a hand-refreshed copy went stale.

## Run the gates before you push

CI runs all eight of these on every pull request:

```bash
make lint.check       # ruff format --check + ruff check   -> CI job "ci-python"
make type             # ty                                 -> CI job "ci-python"
make test             # pytest, tests/unit/                -> CI job "ci-python"
make rust.fmt.check   # cargo fmt --check                  -> CI job "ci-rust"
make rust.lint        # cargo clippy -D warnings           -> CI job "ci-rust"
make rust.build       # cargo build                        -> CI job "ci-rust"
make rust.test        # cargo test                         -> CI job "ci-rust"
make rust.doc         # cargo doc, warnings are errors      -> CI job "cargo-doc"
```

`make rust.doc` is the rustdoc gate. `cargo doc` exits 0 on a broken intra-doc link — a link to a
renamed function passes it — so `RUSTDOCFLAGS="-D warnings"` is what makes those diagnostics fail.

Those targets are grouped into `ci-python`, `ci-rust` and `cargo-doc`. A fourth work job,
`coverage`, runs `make cov` — see below. The `ci-required` aggregate runs with `if: always()` and
fails unless `ci-python`, `ci-rust` and `cargo-doc` all succeed. `coverage` is deliberately outside
that aggregate: it protects the measuring instrument, not the shipped artifact.

A separate `release-contract` check validates the PR title on open, edit, reopen and new pushes.
It reads the trusted workflow from the base branch and never checks out or executes PR code. Keeping
it separate avoids rerunning `ci.yml` when only the PR title or body changes.

Branch protection requires exactly `ci-required` and `release-contract`, with the branch required
to be up to date. The aggregate makes the three work jobs merge-blocking without coupling protection
to their individual names; `coverage` remains advisory. CI still runs after a merge on `main`, so
the exact commit that landed is graded as well as the proposed merge.

`make rust.fmt` and `make lint.fix` are the fixing counterparts. `make help` lists everything.

## Toolchains

- Rust is pinned in `rust-toolchain.toml` (**1.97.0**, with `clippy` and `rustfmt`). Do not run the
  gates on `stable` — the pinned `ruff_*@0.0.6` crates require rustc ≥ 1.95, and an older `stable`
  fails before compiling any of this crate.
- Python comes from `uv`. **Two floors, and they are not the same number:**
  - the **distribution** installs and runs on **≥ 3.11** (`requires-python`). It is a
    `py3-none-<platform>` wheel carrying a native executable and no Python, so nothing in it cares
    which interpreter installs it — `.github/workflows/build-artifacts.yml` smokes every artifact on
    3.11, and `scripts/check_artifact.py` fails a build whose `Requires-Python` header says anything
    narrower.
  - **development** needs **≥ 3.12**: `measure_file` in `corpus/measure.py` raises below it
    (`MIN_INTERPRETER`, PEP 701 — pre-3.12 `tokenize` hides identifiers inside f-strings and lowers
    the restatement counts), so **`make test` itself fails on 3.11** rather than measuring
    differently in silence. `ci.yml` runs **3.14** — above that floor, so `requires-python`'s open
    upper bound is actually exercised. `tests/unit/test_measure.py` reddens if either floor swallows
    the other, if CI drops back onto the floor, or if `ci.yml` declares `PYTHON_VERSION` anywhere but
    once at the top (a job-level `env:` counts, in any quoting).

  ruff's `target-version` and ty's `python-version` are **inferred** from `requires-python`, i.e.
  3.11 — deliberately the lower of the two, because `build-artifacts.yml` runs `scripts/*.py` under
  3.11 and a higher target would greenlight a construct that only breaks during the release build.

  Use `uv run python3`, never a bare `python3`.

## Building and smoking the distribution

There is no Rust extension module any more and no `make py.build`. The wheel carries the compiled
executable (`[tool.maturin] bindings = "bin"`), so `import tooprolix` raises `ModuleNotFoundError`
by design and there is nothing to rebuild into `.venv`.

```bash
make rust.build       # cargo build, then otool/ldd that binary -> CI job "ci-rust"
uvx maturin==1.14.1 build --release --locked --out dist   # a wheel for this machine
# install it and run the COMMAND. The date is REQUIRED and is the oracle the check compares
# against — without one it could only assert the SHAPE of a date, which accepted a binary built
# from any other commit. Use `unknown` for an sdist built with no SOURCE_DATE_EPOCH.
scripts/install-smoke.sh dist/tooprolix-*.whl "$(git log -1 --format=%cs)"
# ... and grade the archive itself: MIT metadata, a physical LICENSE, docs/ in the sdist, and a
# description that is the TRANSFORMED README rather than whatever happens to be on disk.
uv run --no-project python scripts/check_artifact.py dist/tooprolix-*.whl README.md
```

`make rust.build` is **not** a guard on the shipped wheels: it inspects the binary cargo just linked
on the machine running it, not the release artifacts `build-artifacts.yml` produces.

`scripts/install-smoke.sh` is the guard that replaced the pyo3 boundary tests: it installs the
artifact into a throwaway project and asserts the command exists, prints exactly the expected
version and date, that `import tooprolix` raises `ModuleNotFoundError` (the wheel ships an
executable, not a module), and that it exits 1 with `TPX002` on a file with a finding and 0 on a
clean one. `.github/workflows/build-artifacts.yml`
runs the same script on every artifact it builds.

## Coverage

Coverage is measured and printed; it is **not** published. There is no badge and no committed
artifact — while the repository is private, no badge host can read one. Revisit when it goes public.

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
have measured, and fails the run with a message naming the file. Its one ceiling is stated in the
script at the line that implements it: a missing Rust file is reported only if its text contains a
literal `fn `. That is what lets `src/detect.rs` — module documentation and `pub mod` declarations,
no functions — be legitimately absent, which
`test_a_rust_source_file_with_no_instrumentable_code_may_be_absent` holds in place; it also means a
module whose functions all arrive from a macro expansion clears the filter and leaves the
denominator without a word. If the check fires, fix the denominator; do not silence it.

Do not add a `--fail-under` threshold either: picking one before the code audits would be picking a
number to match today's code.

What the two numbers do **not** include, because a percentage implies it measured everything it
could:

- **`build.rs` is invisible to the Rust number.** It is a build script, compiled and run on the host
  before the crate exists, so `cargo llvm-cov` never instruments it and it appears in no row of the
  report. It is unmeasured, not covered.
- **Rust branch coverage is not reported at all** on the pinned stable toolchain — llvm-cov's
  `Branches` column reads `-`. The Rust number is **line** coverage; the Python number folds branches
  in (`branch = true`). They are two different measures, which is why they are never added together
  into a single figure and why neither is described as comparable to the other.
- **The doctests are not in the Rust number.** `cargo llvm-cov` skips doctests unless `--doctests`
  is passed, and that flag needs a nightly toolchain — on the pinned 1.97.0 it fails with
  `error: 2 nightly options were parsed`. So the `Doc-tests tooprolix` target is absent from
  `make rust.cov` entirely and code reached only by a doctest counts as uncovered. No test counts
  are written down here, on purpose: nothing would fail when they drifted. Run `make rust.test`.
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
