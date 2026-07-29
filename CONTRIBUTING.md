# Contributing to tooprolix

## Conventional Commits are mandatory

Every commit message must follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <summary in the imperative>
```

Types in use: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `chore`, `ci`, `build`.

A breaking change is marked with `!` (`feat!: …`) or a `BREAKING CHANGE:` footer.

This is not a style preference — it is load-bearing, and it is load-bearing **on the pull request
title**, not on the individual commits. This paragraph used to say that release-plz reads *these
messages* to compute the version. Under this repository's squash merge that is **false**, and it is
the sentence that made losing a version boundary feel safe. The next section is what actually
happens; read it before opening a pull request.

Commit-level discipline still matters — the reviewer reads it, and it is what the gate below
compares your title against — but nothing downstream of the merge button ever parses it.

## What decides the version number

**The pull request TITLE is the release contract.** This repository squash-merges, so the squashed
commit's *subject* is the pull request title, and conventional-commits parsing reads the subject.
Your branch's own commit subjects are demoted into the body of that commit, as prose, where nothing
parses them. release-plz never sees them.

So:

- the PR title must itself be a Conventional Commit — `<type>(<optional scope>)!: <summary>`;
- a **breaking change must carry `!` in the PR TITLE**. A `!` on a branch commit is not enough;
- if the two disagree, the title wins, silently, and the release is mispriced.

**The worked example is `033ceeb`, and it is on `main`.** PR #17 was titled `Audit the Rust code
with /rust-skills, and close what it found` — no type, no `!` — over a branch containing
`fix!: stop on a closed pipe, and refuse a backslash in \`exclude\``. The marker survived: it is in
the body of `033ceeb` at **line 77**, where nothing reads it. release-plz saw a non-conventional
subject and cut **`v0.3.4`**, a patch, for a breaking change; `CHANGELOG.md` files it under
"Other". Re-measured 2026-07-29 on the live `v0.3.4` tag: the same change under a `fix!:` title
answers `0.3.4 -> 0.4.0`. The boundary was lost at the merge dialog, not in the code.

Proved in both directions by this repository's own history: #5 `feat!:` → **v0.2.0**, #9 `feat!:` →
**v0.3.0**, #7 `feat:` → v0.2.1, #15 `ci:` → v0.3.3, **#17 no type → v0.3.4 ❌**.

**A CI job now grades this**, because "remember to title the PR correctly" depends on attention at
exactly the moment attention is scarce. `.github/workflows/release-contract.yml` runs
`scripts/pr_title_gate.py` on `pull_request` — including **`edited`**, so editing a title re-grades
it — reading the real title from the event payload and the real commits from the API, and failing
when either the title does not parse or the branch carries a `!` / `BREAKING CHANGE:` the title does
not.

The failure message names the bump your title would produce and the bump you probably wanted. It
fails closed: an unreadable title, an unreachable API, a nested or empty commit payload, and a
commit list at the API's 250-commit cap are all red, never a skip.

**What a red actually does, stated plainly: nothing, yet.** It cannot block a merge, because branch
protection is impossible on this repository — private without GitHub Pro, so
`branches/main/protection` is 404 and rulesets are 403. It is a red X a human has to read. It
becomes a real barrier when `flip-public-and-publish-to-pypi` makes the repository public and
registers the required set.

⚠️ **One case where the PR title is NOT what release-plz reads**, measured 2026-07-29 and worth
knowing because the sentence above is otherwise stated flatly. This repository is set to
`squash_merge_commit_title: COMMIT_OR_PR_TITLE`, which uses the **commit's** subject when the pull
request has **exactly one commit**, and falls back to the PR title only when there is more than one.
Measured across every **merged** pull request on this repository: #5/#7/#9/#15 have 4 commits, #11
has 6, #13 and **#17** have 3 — never one, so the fallback to the PR title fired every time,
including the one that mispriced. (Counted on merged PRs deliberately: an open PR's commit count
still moves, so it is not evidence.)
On a single-commit pull request the gate grades the title while release-plz reads the commit
subject, so the gate is **over-strict** there: a false red is possible, a false green is not. It
errs safe, which is why it is documented rather than fixed.

**Accepted residuals — recorded, not overlooked:**

- **Nothing tests the workflow YAML.** A `|| true` appended to the invocation, a job- or step-level
  `if:`, or a dropped `set -o pipefail` disables the gate and no test in this repository notices. A
  YAML-parsing suite that caught all of those existed and was **deliberately removed** to keep this
  feature small, along with the `pyyaml` dependency it needed. The mitigation is that
  `release-contract.yml` is ~100 lines and changes to it are visible in a pull request diff — read
  it at review time.
- **There is no post-merge check.** One was built, on `push: main`, grading the subject that
  actually landed — because GitHub's squash dialog lets whoever merges edit the subject after the
  gate has passed. It was removed: it could not **block** anything (no branch protection, and
  `release-plz.yml` fires on the same `push: main` independently), so its red was advice arriving
  after the release was already priced. **The residual is real**: a subject edited in the merge
  dialog is graded by nothing. Check the squash subject before you confirm the merge.

**Two things it deliberately cannot do**, so nobody reads more into a green than is there:

- **It cannot detect a breaking change nobody declared.** If the code breaks the API while every
  commit and the title both say `fix:`, this job is satisfied and the release is a patch. The gate
  compares *declarations*; it does not diff the API. `cargo-semver-checks` is the tool that would,
  and it is a candidate for the publication task, not something this job approximates.
- **It runs the pull request's own code**, like every other job here — a pull request may edit the
  gate to `exit 0` in the same commit that breaks the contract. That is a property of CI, not of
  this gate; the mitigations are branch protection and reading the diff, and branch protection is
  measured impossible on this repository today. Owned by `flip-public-and-publish-to-pypi`.

### The bump table — measured, both halves

Measured on throwaway clones with release-plz 0.3.160, one commit type per run, each with a real
change to a packaged file. The `0.x` column was first measured 2026-07-27 on a real `v0.1.0` tag and
**re-measured 2026-07-29 on the live `v0.3.4` tag**; the `1.x` column was measured 2026-07-29 on a
throwaway clone tagged `v1.0.0`. Neither column is an assumption about semver.

| commit type | while the crate is `0.x` (from `0.3.4`) | after `1.0.0` (from `1.0.0`) |
|---|---|---|
| `fix:` | `0.3.5` — patch | `1.0.1` — patch |
| `feat:` | 🔴 `0.3.5` — **patch** | `1.1.0` — minor |
| `perf:` / `docs:` / `chore:` / `refactor:` | `0.3.5` — patch | `1.0.1` — patch |
| `feat!:` / `fix!:` / a `BREAKING CHANGE:` footer | **`0.4.0` — minor** | **`2.0.0` — major** |
| anything, changing no packaged file | no release at all | no release at all |

Three consequences, each of which surprises somebody:

1. 🔴 **While the crate is `0.x`, `feat:` does NOT give a minor.** Only `!` / `BREAKING CHANGE`
   moves the middle number. Anyone reasoning from ordinary semver gets this wrong — and the `1.x`
   column shows the rule changing under them the day `1.0.0` ships.
2. **There is no `major` today.** On `0.x` the largest bump reachable is a minor. The first `1.0.0`
   is a deliberate act, not something a commit type triggers, and it belongs to the publication
   task.
3. **Even `docs:` and `chore:` produce a release**, as long as the commit changes a packaged file.
   The type chooses the CHANGELOG *section*, not whether a bump happens. The only thing producing no
   release is a commit that leaves the packaged content byte-identical — an `--allow-empty` `feat!:`
   answers `already up to date`.

<details>
<summary><strong>How the bump table was measured</strong> — the commands and their output</summary>

Recorded so the table is checkable rather than trusted. **release-plz 0.3.160**, throwaway clones
outside any checkout, one commit per run, each appending a comment to `src/main.rs` so a packaged
file genuinely changes.

⚠️ **The harness has one non-obvious trap, and it produced a wrong answer first.** release-plz reads
the commits up to the branch's **upstream ref**, not up to local `HEAD`. Without the
`git update-ref` line below, every single case answers `already up to date` — including the known
`feat!:` control, which is how the bug was caught rather than published.

```bash
git clone /path/to/tooprolix clone && cd clone
git checkout -B measure a21881f && git branch --set-upstream-to=origin/main measure
# for each case:
git reset --hard a21881f && git clean -fd
printf '\n// measurement\n' >> src/main.rs
git add -A && git commit -m "$SUBJECT"
git update-ref refs/remotes/origin/main HEAD   # ← without this, everything reads "already up to date"
release-plz update            # then read the version line and `git diff CHANGELOG.md`
```

`0.x`, on the live `v0.3.4` tag (2026-07-29):

```
fix: a fix                   -> 0.3.4 -> 0.3.5      ### Fixed
feat: a feature              -> 0.3.4 -> 0.3.5      ### Added
chore: a chore               -> 0.3.4 -> 0.3.5
docs: a doc change           -> 0.3.4 -> 0.3.5
feat!: a breaking feature    -> 0.3.4 -> 0.4.0      ### Added   - [**breaking**] …
fix!: a breaking fix         -> 0.3.4 -> 0.4.0
feat!: but --allow-empty     -> already up to date
Audit the Rust code with …   -> 0.3.4 -> 0.3.5      ### Other   ← PR #17's title, reproduced
```

`1.x`, on a throwaway clone tagged `v1.0.0` with `Cargo.toml` set to `1.0.0` (2026-07-29):

```
fix: a fix                   -> 1.0.0 -> 1.0.1
feat: a feature              -> 1.0.0 -> 1.1.0
chore: / docs: / perf: / refactor:  -> 1.0.0 -> 1.0.1
feat!: a breaking feature    -> 1.0.0 -> 2.0.0
fix!: a breaking fix         -> 1.0.0 -> 2.0.0
BREAKING CHANGE: footer      -> 1.0.0 -> 2.0.0
feat!: but --allow-empty     -> already up to date
```

The same harness measured the **grammar** boundaries that `scripts/pr_title_gate.py` implements —
each one is cited in the docstring of the test that pins it:

```
feat()!: break the API       -> 0.3.4 -> 0.3.5   ← an EMPTY scope silently voids the `!`
feat( )!: break the API      -> 0.3.4 -> 0.4.0   ← a scope of one space IS a scope
feat(cli)!: break the API    -> 0.3.4 -> 0.4.0
fix:no space after the colon -> 0.3.4 -> 0.3.5   ### Fixed  ← the space is not required
BREAKING CHANGE: x           -> 0.3.4 -> 0.4.0     BREAKING CHANGE #123  -> 0.4.0
BREAKING-CHANGE: x           -> 0.3.4 -> 0.4.0     BREAKING-CHANGE #123  -> 0.4.0
BREAKING CHANGE#123          -> 0.3.4 -> 0.3.5     breaking change: x    -> 0.3.5
BREAKING CHANGES: x          -> 0.3.4 -> 0.3.5
```

</details>

⚠️ **None of this is a property of release-plz in general — it is a property of `git_only = true`
in `release-plz.toml`.** The version comes from the git tag, not from a registry, because this crate
is never published. Without that line "the next version is the current version, forever", measured.
Read that file's comment before changing anything here.

## `v0.3.4` shipped a breaking change as a patch, and is knowingly left as is

Recorded so it is not rediscovered as a bug. `v0.3.4` should have been `v0.4.0`. The tag, the GitHub
release and the CHANGELOG entry are live and all say patch.

**It is deliberately not being fixed.** It is not retagged, the release is not deleted, and neither
`[package] version` in `Cargo.toml` nor the `## [0.3.4]` heading in `CHANGELOG.md` is hand-edited —
the rule above forbids exactly that, and the rule stands. Nothing is user-facing: the repository is
private and PyPI returns 404 for this project, so the entire cost is bookkeeping.

**Owner: the `flip-public-and-publish-to-pypi` task**, which chooses the first *published* version
deliberately. Until then this is a known wart with a named owner, not an open question.

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

An **eighth** job in `ci.yml`, `coverage`, runs `make cov` — see below. **It is deliberately not a
required check**: it protects the measuring instrument (that the coverage toolchain still resolves
and that the report grader still accepts a real run), not the shipped artifact.

A ninth check, `release-contract`, lives in its **own workflow file** rather than in `ci.yml` — see
"What decides the version number" above. It needs a `pull_request: types: [… edited]` trigger, and
`edited` also fires on pull-request *body* edits, so putting it on `ci.yml`'s shared trigger would
re-run all eight jobs — including the 25-minute `coverage` — every time somebody fixes a typo in a
description. The gate's logic is unit tested by `tests/unit/test_pr_title_gate.py` under
`make test`; **the workflow file itself is not** — see "Accepted residuals" above.

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

The same is true of `release-contract`, and it is the other thing the merge button does not tell
you: a red `release-contract` is a warning today and becomes a barrier the moment protection is
registered. Until then, a mispriced release is one ignored red X away — which is precisely how
`v0.3.4` happened, when there was no X at all. Nothing fires after the merge; see "Accepted
residuals" above.

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
