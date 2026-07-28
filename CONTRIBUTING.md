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
- change rule statuses in `README.md` and `docs/rules-and-configuration.md` from `Implemented` to
  `Released` — and only after reference-corpus validation records the result;
- verify every install and output example against the **published wheel**, not a local build.

## Run the gates before you push

CI runs all six of these on every pull request:

```bash
make lint.check       # ruff format --check + ruff check   -> CI job "lint"
make type             # ty                                 -> CI job "type"
make test             # pytest, tests/unit/                 -> CI job "test"
make rust.fmt.check   # cargo fmt --check                   -> CI job "cargo-fmt"
make rust.lint        # cargo clippy -D warnings            -> CI job "cargo-clippy"
make rust.test        # cargo test                          -> CI job "cargo-test"
```

**Only three of them are enforced by branch protection today: `lint`, `type` and `test`.**
`cargo-fmt`, `cargo-clippy` and `cargo-test` run and report, but they are not yet registered as
required status checks — that is a repository-settings change only the repo owner can make. Until
they are registered, a pull request with red Rust gates is still mergeable, so **read the Rust job
results yourself before merging**; do not treat a green merge button as a green build.

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
