//! `tooprolix check <path>`: the walk, the exit contract, and the two output formats.
//!
//! # Why all of this is in the library
//!
//! `src/main.rs` is a three-line wrapper around [`run`] and holds no logic at all, so the whole
//! command line is reachable — and testable — as a library rather than only through a process.
//!
//! # The exit contract
//!
//! | state | code |
//! |---|---|
//! | the tree was read whole, no findings | 0 |
//! | the tree was read whole, findings | 1 |
//! | part of the tree could not be read, no findings | **1** |
//! | part of the tree could not be read, findings | **1** |
//! | the run could not start: a bad path, a broken configuration | 2 |
//!
//! **A partial run never returns 0**, and that is the single guarantee the rest of this module is
//! arranged around. It is the condition the graceful path was accepted on: if a tree that was not
//! fully read could look green, none of the rest would be worth having. Everything else about the
//! change is a convenience; this is the invariant.
//!
//! ## Code 2 means only that the run could not start
//!
//! An unparsable file does **not** take the run down: it is a diagnostic on stderr, a `skipped`
//! entry in the JSON, and exit 1. Failing the whole run instead is the tempting simplification, and
//! it was measured on the pinned ruff checkout (`a2635fd8`), 374 of whose files are
//! deliberately-unparsable parser fixtures: **exit 2, 0 findings, 375 stderr lines**. An adopter
//! whose repository contains a parser corpus would have no way to run this tool over it except one
//! invocation per subdirectory, and a permanently red CI until they configured `exclude`. Ruff
//! reports a syntax error as a diagnostic and lints the rest of the tree; so does this.
//!
//! ## What that costs, and where the cost was put
//!
//! The exit code does not distinguish "the prose is bad" from "the measurement is incomplete":
//! both are 1. Completeness therefore has to live somewhere a machine can read it, and the only such
//! place is the document — [`crate::finding::Report::complete`], with the failures beside it in
//! `skipped`. That is what took the JSON schema to version 2.
//!
//! **`TPX003` over an incomplete set is a different graph, not the same clusters minus a file.**
//! It is cross-file by construction, so a missing block can be the bridge that held two components
//! together. A partial run says so on stderr, once, and the document says so by being marked
//! incomplete.
//!
//! ## `exclude` is the other half, and it is not this one
//!
//! [`crate::config`]'s `exclude` moves the **boundary** of what is measured: a tree the project
//! never claimed is out of scope rather than partially read. Inside that boundary the measurement is
//! whole, so an excluded path leaves `complete` alone and the text output says nothing about it at
//! all — a warning on every deliberate exclusion would fire on every run of any repository that
//! configured one. The paths are in the document's `excluded`, which is the whole reason that field
//! exists. `skipped` is a refusal, `excluded` is a boundary, and nothing is ever in both.
//!
//! # What the walk does and does not visit
//!
//! * `.gitignore`, `.git/info/exclude` and the global gitignore are respected, through the `ignore`
//!   crate — ruff's own choice for the same job. `require_git` is **off**, so an exported tarball
//!   with a `.gitignore` in it behaves like the checkout it came from.
//! * **`exclude` from [`crate::config`] is layered on top of that, not in place of it.** It is
//!   applied as an `ignore::overrides::Override`, which leaves every non-matching path untouched,
//!   so the gitignore layer underneath still decides those. A project that sets `exclude` does not
//!   thereby start walking what its `.gitignore` hides — which is what the whitelist reading of
//!   that type would have done.
//! * **Symlinks are not followed.** Measured 2026-07-25: following them takes pydantic from 343
//!   findings to 559, because `tests/pydantic_core` is a symlink back into the tree and every file
//!   under it is then counted twice. This is the `ignore` crate's default; the requirement is not to
//!   turn it off.
//! * **Hidden entries are skipped**, which is the `ignore` crate's default and a deliberate
//!   divergence from ruff, which sets `hidden(false)`. Ruff can afford to walk `.tox` and `.venv`
//!   because it ships a *default* `exclude` list; ours is empty unless a project writes one, so
//!   this remains the only defence a project that configures nothing has against linting a
//!   virtualenv. Naming a hidden path directly still checks it — the root of a walk is always
//!   visited, and that is also why an explicitly named path is checked even when `exclude` matches
//!   it, which is ruff's own default.
//! * Only files [`crate::extract::is_python_source`] accepts are read. Naming a non-Python file
//!   *directly* is an error rather than a silent zero, for the reason
//!   [`crate::extract::Error::UnsupportedSource`] gives: the caller chose that file.
//!
//! # Paths in the output are the paths the walk used
//!
//! `tooprolix check src` reports `src/api.py:1-9`; `tooprolix check .` reports `./api.py:1-9`. The
//! canonical form is used for finding `pyproject.toml` (see [`crate::config`]) and nowhere else,
//! because a user who typed a relative path wants a relative finding they can paste into an editor.
//!
//! # A single file is a legal target, and it measures less than it looks like it does
//!
//! `tooprolix check one_file.py` is supported and useful — a pre-commit hook over the changed files
//! is exactly this. But `TPX003` is cross-file by construction: `duplicates` compares the blocks it
//! was handed and nothing else, so a single-file run can only ever find duplicates *inside that
//! file*. A user who reads exit 0 there as a verdict on the repository has been misled by silence,
//! so [`help`] says it in as many words.

use std::ffi::{OsStr, OsString};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use ignore::WalkBuilder;
use thiserror::Error as ThisError;

use crate::config::{self, Config};
use crate::detect::duplicate::duplicates;
use crate::detect::volume::volume;
use crate::extract::{self, ProseBlock, is_python_source};
use crate::finding::{Finding, Report, Skipped};
use crate::rules::{self, Rule, Suppression, parse_marker};

/// The `--help` text, and the only place the command grammar is written down.
///
/// A function rather than a `const`, and the reason is [`rules::CATALOGUE`]: the Rules block below
/// is rendered from the same array `--rules` prints, so the two cannot say different things about
/// `TPX002`. A `const` cannot concatenate a rendered block, and writing the same three sentences
/// out here *and* in the catalogue with a test to pin them is two owners with a tripwire rather
/// than one owner. The `--help` **text** is the contract, not this signature.
#[must_use]
pub fn help() -> String {
    let mut text = String::from(HELP_BEFORE_RULES);
    for line in rules_listing().lines() {
        text.push_str("  ");
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(HELP_AFTER_RULES);
    text
}

/// The rule catalogue exactly as `tooprolix --rules` writes it: one line per code, no indent.
///
/// [`help`] embeds these same lines indented by two spaces, which is what
/// `the_rules_listing_and_the_help_render_the_same_registry` compares — the bytes, not the fact
/// that both mention `TPX001`.
///
/// The widths are the longest code plus two and the longest status plus two, so the description
/// column starts in the same place on every row.
fn rules_listing() -> String {
    use std::fmt::Write as _;

    rules::CATALOGUE
        .iter()
        .fold(String::new(), |mut text, rule| {
            writeln!(
                text,
                "{:<8}{:<13}{}",
                rule.code, rule.status, rule.description
            )
            .expect("writing into a String cannot fail");
            text
        })
}

/// `tooprolix <semver> (<date>)`, with the version from `Cargo.toml` and the date from the commit.
///
/// Neither half is written down here. The version is `CARGO_PKG_VERSION`, so `Cargo.toml` stays the
/// one owner of the number that `pyproject.toml`'s `dynamic = ["version"]` also defers to. The date
/// is `build.rs`'s answer — the **commit** date, or `unknown` — because a `--version` that moved
/// with the wall clock would differ between two builds of one commit.
fn version_line() -> String {
    format!(
        "tooprolix {} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("TOOPROLIX_COMMIT_DATE")
    )
}

const HELP_BEFORE_RULES: &str = "\
tooprolix — a prose budget linter for Python.

Usage:
  tooprolix check <path> [--format text|json]
  tooprolix --help
  tooprolix --version
  tooprolix --rules

Arguments:
  <path>    A Python file, a directory, or `.`. Directories are walked; `.gitignore`
            is respected, symlinks are not followed, and hidden entries are skipped.
            There is no `--` end-of-options marker: a path whose name begins with
            `-` is read as an option, so write it as `./-name.py`.

Options:
  --format  `text` (default) writes one line per finding to stdout.
            `json` writes a versioned document: {\"schema_version\", \"complete\",
            \"skipped\", \"excluded\", \"findings\"}, including on a clean run. All
            five are always present. Giving it twice is an error, not last-wins.
  --help    Show this text.
  --version Print `tooprolix <version> (<commit date>)`. The date is the date of
            the commit the binary was built from, not the date it was built, so
            two builds of one commit answer identically. A tree with no git
            history OF ITS OWN says `unknown` rather than guessing — a copy
            unpacked inside some other repository borrows no date from it.
  --rules   Print the rule table below and nothing else, for a script. Each of
            these three flags takes no other argument.

Rules:
";

const HELP_AFTER_RULES: &str = "
  The limits are `comment-max-volume` and `docstring-max-volume`, both in words.
  Volume is measured in WORDS, after normalisation — not lines and not characters.
  The limit is the last size still allowed: a block of exactly the limit is silent.

Opt out of one block, with the marker on the physical line DIRECTLY ABOVE it —
one rule for comments and docstrings alike:

  # !TPX001
  # A comment run that earns its length.

  def settle(batch):
      # !TPX002
      \"\"\"A docstring that earns its length.\"\"\"

  # !TPX001,TPX003 several codes, then the reason
  # !TPX*          every rule on this block, and `TPX*` is a literal, not a glob

  For a docstring that means inside the body, between `def`/`class` and the
  literal — NOT above the `def` line. The space after `#` is required, so a
  shebang is never a marker, and what follows the `!` must START with one of
  OUR codes: `# !HTTP2 is mandatory` is an ordinary comment and silences
  nothing. A code that is not recognised silences nothing and says so, and so
  does a `# !TPX…` that is not one of them.

Opt out of a rule for the whole project, in pyproject.toml:

  [tool.tooprolix]
  ignore = [\"TPX003\"]
  exclude = [\"tests/fixtures\", \"vendor\"]
  comment-max-volume = 150
  docstring-max-volume = 200

  The nearest pyproject.toml at or above the checked path is used. A rule listed in
  `ignore` cannot be switched back on by a marker.

  `exclude` takes .gitignore-syntax globs, resolved relative to the directory of
  the pyproject.toml they are written in — so one rule means the same thing from
  the project root and from a package inside it. Matches are never read, so they
  are neither checked nor able to fail the run; this is how a repository that
  legitimately contains invalid Python (a parser corpus, a fixture tree) can be
  checked at all. It ADDS to .gitignore rather than replacing it, and a path named
  directly on the command line is still checked.

Checking a single file:
  TPX003 compares only the blocks it is handed, so `tooprolix check one_file.py`
  only finds duplicates inside that file, never across the repository.
  Exit 0 on one file is not a verdict on the tree around it.

Exit codes:
  0   the tree was read whole and there are no findings
  1   findings were reported, OR part of the tree could not be read, or both
  2   the run could not start (bad path, broken configuration)

  A file that cannot be read no longer fails the run: it is named on stderr with
  the reason, the rest of the tree is still checked, and the exit code is 1 even
  when nothing was found — a tree that was not read whole never exits 0. Only
  `--format json` can tell the two apart: it carries `complete` and `skipped`.
";

/// The outcomes of a run, and the reason there are four of them rather than three numbers.
///
/// [`Incomplete`](Self::Incomplete) and [`Failure`](Self::Failure) both report **1**, so the enum
/// draws a distinction the process cannot. That is deliberate and it is where the central guarantee
/// is enforced: `Success` is the only variant that can produce 0, and it is unreachable from a run
/// that failed to read anything, so "a partial run never exits 0" is a property of one `match` arm
/// instead of a condition scattered through the caller. Collapsing the two would also make both
/// remaining doc comments false — each of them claims the tree *was read*.
///
/// `#[non_exhaustive]` because a fifth outcome is conceivable and matching on this from outside
/// the crate must keep compiling across one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExitStatus {
    /// The tree was read whole and there is nothing to report.
    Success,
    /// The tree was read whole and there are findings.
    Failure,
    /// Part of the tree could not be read — whether or not anything was found in the rest.
    Incomplete,
    /// The run could not start.
    Error,
}

impl ExitStatus {
    /// The number this outcome is reported as.
    ///
    /// There are two consumers and they need two different types, which is why the mapping is a
    /// function rather than only the `From<ExitStatus> for ExitCode` below: `src/main.rs` wants an
    /// [`ExitCode`], and the `tooprolix` console script has to hand an `int` back to `CPython`
    /// (`sys.exit(main())`), where [`ExitCode`] is opaque. One `match`, so the two cannot drift.
    ///
    /// `#[non_exhaustive]` on [`ExitStatus`] only forbids exhaustive matching from *other* crates,
    /// so a fifth outcome is still a compile error here — which is the point of doing it once. No
    /// `_` arm, for exactly that reason.
    ///
    /// The `Incomplete => 1` arm is the whole exit contract's load-bearing line: it is the one place
    /// a tree that was not read whole is given its number, and the only edit that can make such a
    /// run exit 0.
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failure | Self::Incomplete => 1,
            Self::Error => 2,
        }
    }
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        Self::from(status.code())
    }
}

/// How findings are written to stdout.
///
/// `#[non_exhaustive]`: `sarif` and `github` are the two formats every linter is eventually asked
/// for, and either would be a new variant. Free now, a major bump after publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Format {
    /// One line per finding.
    #[default]
    Text,
    /// One versioned JSON document.
    Json,
}

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq)]
enum Invocation {
    /// `tooprolix check <path> [--format …]`.
    Check {
        /// The file or directory to walk, exactly as it was typed.
        path: PathBuf,
        /// The output format.
        format: Format,
    },
    /// `tooprolix --help`.
    Help,
    /// `tooprolix --version` or `tooprolix -V`.
    Version,
    /// `tooprolix --rules`.
    Rules,
}

/// Everything that stops a run — all but one of them a failure to *start*.
///
/// There is deliberately **no** variant for a file that would not parse: that is not a reason to
/// refuse the run, and a variant for it would be a way to spell an outcome the contract does not
/// have. It is carried by [`crate::finding::Skipped`], which is a *report* and not an error.
///
/// [`Output`](Error::Output) is the one variant that is **not** a failure to start — the run did
/// everything right and could not hand the answer over. It is also the only variant that does not
/// always mean exit 2: `status` reads its [`std::io::ErrorKind`] and turns a closed pipe into a
/// silent exit 0. That decision lives there, in the single place both entry points pass through,
/// rather than here, because the *error* is the same fact either way — only the verdict differs.
#[derive(Debug, ThisError)]
#[non_exhaustive]
pub enum Error {
    /// The command line could not be understood.
    #[error("{0}\n\nRun `tooprolix --help` for the full grammar.")]
    Usage(String),

    /// The configuration file exists and is wrong.
    #[error(transparent)]
    Config(#[from] config::Error),

    /// The path to check does not exist.
    #[error("{}: no such file or directory", .0.display())]
    Missing(PathBuf),

    /// A file was named directly and is not a Python source.
    #[error("{}: not a Python source file", .0.display())]
    NotPython(PathBuf),

    /// The directory walk itself failed.
    #[error("could not walk {}: {message}", path.display())]
    #[non_exhaustive]
    Walk {
        /// The root of the walk.
        path: PathBuf,
        /// What the walker complained about.
        message: String,
    },

    /// The findings were computed and could not be written to stdout.
    ///
    /// Carries the [`std::io::Error`] rather than a rendered string, because the *kind* is what
    /// `status` has to branch on — a closed pipe is a reader that stopped reading and is not this
    /// tool's failure, while a full disk is. Flattening it to a message would have made those two
    /// indistinguishable at the one place that has to tell them apart.
    #[error("could not write to stdout: {0}")]
    Output(#[source] std::io::Error),
}

/// One block, plus whatever the marker above it silences.
///
/// The two travel together because suppression has to be applied **before** clustering, not after:
/// removing a member from a finished cluster would change which edge is the weakest one, so a
/// marker on one file would silently rewrite the score reported for another.
///
/// Deliberately **not** `#[non_exhaustive]`, unlike almost every other public type here, and for
/// the same reason [`crate::detect::volume::Limits`] is not: this is a type a caller **builds**, in
/// order to call [`findings`]. `#[non_exhaustive]` forbids the struct literal across crates, so
/// adding it would not harden the API — it would delete the only way to reach the one pure function
/// this module exposes. The price is that a third field is a breaking change, which is the honest
/// trade for an input type.
#[derive(Debug, Clone)]
pub struct Source {
    /// The block itself.
    pub block: ProseBlock,
    /// What the marker on the line above it silences, if there was one.
    pub suppressed: Suppression,
}

/// Runs the command line and returns the process exit code.
///
/// `arguments` is the command line **without the program name** — `["check", "."]`, not
/// `["tooprolix", "check", "."]`. `src/main.rs` does the `.skip(1)`; see `parse` (private) for why that
/// convention lives at the call site rather than here.
///
/// Findings go to stdout; every diagnostic goes to stderr, so
/// `tooprolix check . --format json | jq` works with warnings on screen. [`execute`] is the same
/// run with the outcome returned as a value instead of a process code.
#[must_use]
pub fn run<I: IntoIterator<Item = OsString>>(arguments: I) -> ExitCode {
    status(arguments).into()
}

/// [`run`] with the answer still an [`ExitStatus`], the failure already written to stderr.
///
/// Rendering a failure lives here, once, so that any entry point returning an exit code reports it
/// identically. [`execute`] is the same run with the failure still a value, for a caller that wants
/// to handle it rather than print it.
pub(crate) fn status<I: IntoIterator<Item = OsString>>(arguments: I) -> ExitStatus {
    execute(arguments).unwrap_or_else(|error| {
        // A reader that stopped reading is not a failure of this tool, and it is the ONE error that
        // is not reported. Uncaught, `tooprolix check big/ | head -5` exits **101** with
        // `failed printing to stdout: Broken pipe` — the Rust default, because `println!` panics on
        // a write error. That is outside the documented 0/1/2 contract entirely, and `| head` is an
        // ordinary thing to do with a linter.
        //
        // Exit **0**, which is what ruff answers to the same pipeline (measured). The findings that
        // were written are correct and the ones that were not are output nobody asked for; a
        // consumer that closed the pipe has already decided it has what it needs.
        //
        // The `kind` test is the whole guard, and widening it is the way this becomes a
        // fail-open. Every OTHER write failure falls through to the loud branch below and exits 2,
        // and both halves of that are measured rather than assumed (2026-07-29, macOS/APFS):
        //
        //   * a **full disk** — a 2 MB APFS image filled to capacity — gives exit 2 and
        //     `error: could not write to stdout: No space left on device (os error 28)`, in both
        //     the text and the JSON format;
        //   * an **unwritable stdout** — a fd opened read-only, or one pointing at a directory —
        //     gives exit 2 and `... Bad file descriptor (os error 9)`, for `check` and for all
        //     three discovery commands.
        //
        // The second of those holds only because `emit` does not write through
        // `std::io::stdout()`, which silently discards `EBADF`. See `emit` for the mechanism.
        //
        // A handler that treated any `Error::Output` as a clean stop would turn "the answer never
        // reached the file" into a green run, which is exactly the class this epic keeps finding.
        if reader_stopped_reading(&error) {
            return ExitStatus::Success;
        }
        eprintln!("error: {error}");
        ExitStatus::Error
    })
}

/// Whether `error` is a consumer that closed the pipe, rather than a failure worth reporting.
///
/// A pure function for the same reason [`use_colour`] is one: the *condition* it tests is not
/// reachable from a test — producing a real `ENOSPC` on stdout is not something a test suite can do
/// portably — so the decision is separated from the io that feeds it and pinned as a table below.
/// Without that split the narrowness of this check is untested, and an untested narrowness is how
/// it silently widens into "every write failure is a clean stop", which would turn a full disk into
/// a green run.
fn reader_stopped_reading(error: &Error) -> bool {
    matches!(error, Error::Output(source) if source.kind() == std::io::ErrorKind::BrokenPipe)
}

/// Writes to stdout, turning any write failure into [`Error::Output`].
///
/// Every byte this tool puts on stdout goes through here, and that is what makes the write
/// contract in `status` hold for **both** output formats and **both** entry points: `src/main.rs`
/// and the console script in `src/lib.rs` differ only in how they spell the exit code, and both
/// reach stdout through this function.
///
/// It takes a closure rather than a `&str` so that the findings loop can hold one buffer for the
/// whole run and stop at the **first** failed write. That matters beyond tidiness: on a closed pipe
/// the alternative keeps formatting findings nobody is going to read.
///
/// # Why this does not write through [`std::io::stdout`]
///
/// **Because `std::io::stdout()` silently discards `EBADF`, and that is measured, not inferred.**
/// On an unwritable stdout — a fd opened read-only, a fd pointing at a directory, or a fd that is
/// simply closed — `write_all` and `flush` on `Stdout` both return `Ok(())` and the bytes go
/// nowhere. Measured 2026-07-29 on rustc 1.97.0, aarch64-apple-darwin, one process holding both
/// handles on the *same* fd 1:
///
/// | fd 1 is | raw `File` on that fd | `std::io::stdout()` |
/// |---|---|---|
/// | opened read-only | `Err(EBADF, os error 9)` | **`Ok(())`** |
/// | a directory | `Err(EBADF, os error 9)` | **`Ok(())`** |
/// | closed | `Err(EBADF, os error 9)` | **`Ok(())`** |
/// | a closed pipe | `Err(BrokenPipe, os error 32)` | `Err(BrokenPipe, os error 32)` |
/// | a full disk | `Err(os error 28)` | `Err(os error 28)` |
///
/// So the swallowing is specific to `EBADF`: `EPIPE` and `ENOSPC` travel through the identical
/// path. It is not a buffering artifact either — an 8 KiB `write_all`, a write with no newline,
/// and an explicit `flush()` afterwards all return `Ok(())`. The behaviour is deliberate in the
/// standard library, so that a process spawned with its stdio closed does not fail on every write;
/// the closed-fd row above is exactly that case. Reasonable for a general program, wrong for a
/// linter whose whole output is its answer: `tooprolix check .` exited **1** in complete silence.
///
/// Duplicating the descriptor is what steps around it, and it is done with **safe** code only —
/// [`std::os::fd::BorrowedFd::try_clone_to_owned`] is safe and returns an `io::Result`, and
/// `File::from(OwnedFd)` is a safe `From`. No `unsafe`, no `libc`, no new dependency, and the
/// crate's `unsafe_code = "deny"` stands untouched. The [`std::fs::File`] owns the *duplicate*, so dropping
/// it closes the dup and never fd 1 itself.
///
/// The explicit flush is still required: [`std::io::BufWriter`] discards errors when it flushes in `Drop`,
/// so a `--format json` document could otherwise meet a broken fd with nowhere to report it.
///
/// **Unix only.** [`std::os::fd`] does not exist on Windows, and no Windows target is built or
/// tested by this repository today, so the fallback below keeps the old behaviour rather than
/// shipping a Windows path nobody has run. On such a platform an `EBADF`-shaped failure stays
/// silent, exactly as it did everywhere before this function changed.
///
/// # Errors
///
/// [`Error::Output`], carrying the [`std::io::Error`] unchanged so its `kind` survives to
/// `status`, which is the only thing that interprets it.
fn emit(write: impl FnOnce(&mut dyn Write) -> std::io::Result<()>) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd as _;

        // One `dup` per call, and there are at most a handful of calls in a run — the findings
        // loop is a single call holding a single buffer, not one per finding.
        let duplicate = std::io::stdout()
            .as_fd()
            .try_clone_to_owned()
            .map_err(Error::Output)?;
        let mut out = std::io::BufWriter::new(std::fs::File::from(duplicate));
        write(&mut out)
            .and_then(|()| out.flush())
            .map_err(Error::Output)
    }
    #[cfg(not(unix))]
    {
        let mut out = std::io::stdout().lock();
        write(&mut out)
            .and_then(|()| out.flush())
            .map_err(Error::Output)
    }
}

/// [`run`] with the outcome still typed, for a caller that is not a process.
///
/// **Public on purpose**, and the decision is worth recording because the alternative was to demote
/// [`ExitStatus`], [`Format`] and [`Error`] to `pub(crate)`. All three were public with no public
/// producer — three types frozen into 0.1.0 for zero capability — and the fix that *adds* something
/// is this one: `package-python-distribution-and-publish-0-1-0` has to run the CLI inside `CPython`
/// through a console script, where the answer has to become a Python return value rather than a
/// process exit code, and `ExitCode` cannot be inspected.
///
/// It still writes findings to stdout and diagnostics to stderr; only the exit code is returned
/// rather than taken.
///
/// # Errors
///
/// Every variant of [`Error`] — all of them are "the tree could not be read", never "it is clean".
pub fn execute<I: IntoIterator<Item = OsString>>(arguments: I) -> Result<ExitStatus, Error> {
    let (path, format) = match parse(arguments)? {
        Invocation::Help => {
            emit(|out| out.write_all(help().as_bytes()))?;
            return Ok(ExitStatus::Success);
        }
        Invocation::Version => {
            emit(|out| writeln!(out, "{}", version_line()))?;
            return Ok(ExitStatus::Success);
        }
        Invocation::Rules => {
            emit(|out| out.write_all(rules_listing().as_bytes()))?;
            return Ok(ExitStatus::Success);
        }
        Invocation::Check { path, format } => (path, format),
    };

    let config = config::load(&path)?;
    if config.ignores_everything() {
        // "Nothing was found" and "nothing could be found" are the same exit code and must not be
        // the same output. The code stays 0 because it is honest — there really are no findings.
        eprintln!(
            "warning: every rule ({}) is disabled by `ignore` in {}; this run cannot report a \
             finding",
            Rule::ALL
                .iter()
                .map(|rule| rule.code())
                .collect::<Vec<_>>()
                .join(", "),
            config.source.as_ref().map_or_else(
                || "the configuration".to_owned(),
                |p| p.display().to_string()
            ),
        );
    }

    let Walked {
        files,
        excluded,
        skipped: unwalkable,
    } = python_files(&path, &config)?;
    let excluded_measurable = excluded.len();
    // `unwalkable.is_empty()` as well: this line blames an *absence* of Python, and when the walk
    // found a `.py` it could not read, absence is not what happened. The skip is already reported,
    // with the path and the reason, so saying "no Python files" beside it is a second answer that
    // contradicts the first.
    if files.is_empty() && unwalkable.is_empty() {
        // A walk that visited nothing scores every repository clean. Saying so is the difference
        // between "no findings" and "no measurement".
        eprintln!("warning: no Python files under {}", path.display());
        // ... and when `exclude` is what emptied it, name that too. On its own the line above
        // blames an *absence* of Python, which for `exclude = ["*"]` is not what happened and
        // sends the reader looking for files that were there all along. The exit code stays 0
        // because it is honest — there really are no findings — so this line is the only thing
        // standing between an excluded tree and a tree that was measured and found clean.
        //
        // Gated on what the WALK counted, never on `!config.exclude.is_empty()`: a glob that
        // matched nothing did not empty anything, and saying it did is the same wrong-cause defect
        // in the opposite direction.
        //
        // The count is also the entire content of the sentence — and it is `excluded.len()`, the
        // length of the very list the JSON reports, never a second number derived beside it. Two
        // numbers that "should" agree is one of them going wrong alone.
        //
        // Naming the globs instead is the tempting version and it is not reachable: taken from the
        // configuration, a glob that never fired reads as having removed paths, and taking them
        // from the walk is impossible because `ignore::overrides::Glob` is an opaque newtype with a
        // private field and no accessors. The paths the walk observed are reachable; they are in
        // `excluded`.
        if excluded_measurable > 0 {
            eprintln!(
                // Says what the walk did and stops there. It must NOT read as a completeness claim
                // — that would contradict the document for the same run, where `complete` is
                // `true`, correctly: `exclude` is a boundary the project drew, and inside it the
                // tree really was measured whole. Only `skipped` moves `complete`, and no excluded
                // path is ever in `skipped`.
                "warning: `exclude` in {} removed {excluded_measurable} path(s) that could have \
                 been measured; nothing outside the excluded set was left to check",
                config.source.as_ref().map_or_else(
                    || "the configuration".to_owned(),
                    |p| p.display().to_string()
                ),
            );
        }
    }

    // The walk's refusals and the read's are one list from here on. They are the same fact — this
    // file was not measured — and the exit code, the document and the stderr block all read the one
    // list, so none of them can disagree about whether the tree was complete.
    let (sources, mut skipped, mut warnings) = read(&files);
    skipped.extend(unwalkable);
    report_unknown_marker_codes(&sources, &mut warnings);
    warn_sorted(warnings);
    let findings = findings(sources, &config);
    // The guarantee, in one expression: `Success` — the only variant that reports 0 — is
    // unreachable while anything was skipped, whatever the findings say. `skipped` is filled
    // exclusively from read attempts that actually failed, so this is a fact about the run and not
    // about the configuration's opinion of it.
    let status = if skipped.is_empty() {
        if findings.is_empty() {
            ExitStatus::Success
        } else {
            ExitStatus::Failure
        }
    } else {
        ExitStatus::Incomplete
    };

    // The empty case is written in JSON and not in text, and that asymmetry is the point. A human
    // reading a clean run wants silence; a consumer parsing stdout wants a document, and a
    // successful run that emits zero bytes is a parse error at the far end that looks exactly like
    // a crash.
    match format {
        Format::Text => {
            // One lock for the whole list, and the `?` inside stops at the first failed write
            // rather than formatting the rest of a report that is not going anywhere.
            emit(|out| {
                for finding in &findings {
                    writeln!(out, "{finding}")?;
                }
                Ok(())
            })?;
            // Gated on the OUTCOME, not on `findings.is_empty()`. `Success` is the only variant
            // that reports 0 and it is unreachable while anything was skipped, so this one
            // condition carries both halves of the rule — "no findings" and "the tree was read
            // whole" — and cannot drift from the exit code, because it *is* the exit code. A
            // partial run with nothing to report is `Incomplete`: exit 1, and silence here. The
            // line would otherwise assert a completeness the run does not have, which is the
            // outcome the whole graceful contract exists to prevent.
            if matches!(status, ExitStatus::Success) {
                success_line()?;
            }
        }
        Format::Json => {
            // `display()` and not a lossless form, for the reason `crate::finding::Location`
            // records for `path`: this is the single place a path becomes a JSON string, and the
            // whole schema is consistent about it.
            let excluded = excluded
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            // What a consumer sees if the pipe closes mid-document: a TRUNCATED one. The bytes
            // written are a prefix of valid JSON and not a valid document, so a reader that stops
            // reading must not then parse what it got — `| head -c 200 | jq` is a parse error by
            // construction, not a bug here. There is no honest alternative: the document is larger
            // than any pipe buffer (114 KB on the pinned corpus), so it cannot be written
            // atomically, and inventing a closing brace would hand over a document whose
            // `findings` array silently omitted findings the run did make. A truncated parse error
            // is loud; a well-formed lie is not.
            emit(|out| {
                out.write_all(
                    Report::new(findings, skipped.clone(), excluded)
                        .to_json()
                        .as_bytes(),
                )
            })?;
        }
    }

    // After the findings, and on stderr in BOTH formats: a document on stdout still needs its
    // diagnostics somewhere a `| jq` does not eat them, and every other warning in this file is
    // already there.
    report_skipped(&skipped, &config);
    Ok(status)
}

/// What a run that read the whole tree and found nothing says.
///
/// Worded as ruff words it, deliberately: this repository's own `make lint.check` already prints
/// exactly this sentence, and a Python developer meets it before they meet this tool. A second
/// spelling of the same fact would be a new thing to learn for no information.
const SUCCESS_LINE: &str = "All checks passed!";

/// [`SUCCESS_LINE`] on stdout, green when the terminal wants colour.
///
/// # Why a successful run is not silent
///
/// Zero bytes is what a crashed run, a walk that visited nothing and a clean repository all look
/// like: the exit code tells them apart and nothing on the screen would.
///
/// It is on **stdout**, beside the findings, because it is the answer to the question that was
/// asked — not a diagnostic about the run. That does mean `tooprolix check . | wc -l` answers 1 on
/// a clean tree; `--format json`, which is the interface for machines, never prints it at all.
///
/// # Errors
///
/// [`Error::Output`] — it writes to stdout like every other line of output, so a closed pipe here
/// is the same clean stop it is anywhere else.
fn success_line() -> Result<(), Error> {
    // Both facts are read here, once, and neither is readable from a test: `is_terminal` depends on
    // what the process was handed and `var_os` on the ambient environment. The *decision* they feed
    // is a pure function precisely so that it can be a table in the tests below.
    let no_color = std::env::var_os("NO_COLOR");
    if use_colour(std::io::stdout().is_terminal(), no_color.as_deref()) {
        // SGR 32 (green) and SGR 0 (reset). Written out rather than taken from a crate: this is the
        // only colour this tool emits anywhere, and `colored`/`owo-colors`/`anstream` would each be
        // a dependency — and a tree to audit, pin and ship to PyPI — for two escape sequences that
        // have not changed since ECMA-48 in 1976.
        emit(|out| writeln!(out, "\u{1b}[32m{SUCCESS_LINE}\u{1b}[0m"))
    } else {
        emit(|out| writeln!(out, "{SUCCESS_LINE}"))
    }
}

/// Whether the success line is coloured: a terminal is watching, and it has not asked for plain
/// text.
///
/// Both conditions can veto, and the order they are written in is not the order they matter in —
/// `NO_COLOR` is an explicit instruction and a pipe is an inference, but a pipe is the case that
/// actually breaks something. An escape sequence in a redirected file, a CI annotation or a
/// `| grep` is corruption of data a machine reads; a missing colour is only plain.
///
/// `NO_COLOR` is read as `no-color.org` defines it — **present and not an empty string**. The empty
/// value is the distinction that matters in practice: `NO_COLOR=` is how a shell script unsets an
/// inherited variable without `unset`, and treating it as "asked for no colour" would make the
/// variable impossible to switch back off.
fn use_colour(is_terminal: bool, no_color: Option<&OsStr>) -> bool {
    is_terminal && no_color.is_none_or(OsStr::is_empty)
}

/// Names every file the run could not read, and — once — what that did to `TPX003`.
///
/// Driven off the same `skipped` list that decided the exit code and fills the document, so the
/// three cannot disagree about whether anything was skipped.
fn report_skipped(skipped: &[Skipped], config: &Config) {
    if skipped.is_empty() {
        return;
    }
    // Sorted for the same reason `Report::new` sorts: the walk is deliberately unordered, so
    // without this the diagnostic is a function of the directory layout.
    let mut lines: Vec<String> = skipped
        .iter()
        .map(|entry| format!("  {}: {}", entry.path, entry.reason))
        .collect();
    lines.sort();
    eprintln!(
        "warning: {} file(s) skipped:\n{}",
        skipped.len(),
        lines.join("\n")
    );

    // Only when the rule actually ran. Announcing that a disabled detector was computed over a
    // subset describes something that did not happen — the same class as a diagnostic built from
    // the configuration's say-so rather than from the run, one level up.
    if !config.ignores(Rule::DuplicateProse) {
        eprintln!(
            "warning: TPX003 was computed over an incomplete set of files; a missing file makes a \
             different cluster graph, not the same clusters minus a file"
        );
    }
}

/// Parses `tooprolix check <path> [--format …]` and nothing else.
///
/// Hand-written, and that is a recorded decision: the grammar is one subcommand, one positional and
/// one option, so a derive-macro argument parser would be a dependency tree and a second home for
/// the help text that [`help`] already owns.
///
/// **`arguments` does NOT include the program name.** A `.skip(1)` hidden here would silently eat
/// the first argument of any caller that builds its arguments by hand — `["check", "."]` would be
/// told that `.` is an unknown subcommand. It lives in `src/main.rs` instead, where turning `argv`
/// into arguments is the caller's own convention.
fn parse<I: IntoIterator<Item = OsString>>(arguments: I) -> Result<Invocation, Error> {
    let mut arguments = arguments.into_iter().map(|argument| {
        argument
            .into_string()
            .map_err(|raw| Error::Usage(format!("argument is not valid UTF-8: {}", raw.display())))
    });

    let Some(first) = arguments.next().transpose()? else {
        return Err(Error::Usage("expected a subcommand".to_owned()));
    };
    // `--help` is deliberately NOT in the `alone` group below. It has ignored everything after it
    // since 0.1.0 — `tooprolix --help --format=yaml` exits 0 — and that is shipped behaviour this
    // additive change promised not to move. The two new flags start strict, which is the direction
    // that stays free: loosening later breaks nobody.
    if matches!(first.as_str(), "--help" | "-h") {
        return Ok(Invocation::Help);
    }
    if matches!(first.as_str(), "--version" | "-V") {
        return alone(Invocation::Version, &first, &mut arguments);
    }
    if first == "--rules" {
        return alone(Invocation::Rules, &first, &mut arguments);
    }
    if first != "check" {
        return Err(Error::Usage(format!("unknown subcommand `{first}`")));
    }

    let mut path: Option<PathBuf> = None;
    // `Option`, not `Format`, so that "given twice" is distinguishable from "given once". A silent
    // last-one-wins would make `--format json … --format text` write prose into a pipe that asked
    // for a document, which is the same class as `--format yaml` falling back to text — and every
    // other malformed command line here is an error rather than a default.
    let mut format: Option<Format> = None;
    let mut pending = arguments.collect::<Result<Vec<_>, _>>()?.into_iter();

    while let Some(argument) = pending.next() {
        match argument.as_str() {
            "--help" | "-h" => return Ok(Invocation::Help),
            "--format" => {
                let value = pending
                    .next()
                    .ok_or_else(|| Error::Usage("`--format` needs a value".to_owned()))?;
                set_format(&mut format, parse_format(&value)?)?;
            }
            _ if argument.starts_with("--format=") => {
                let value = parse_format(&argument["--format=".len()..])?;
                set_format(&mut format, value)?;
            }
            _ if argument.starts_with('-') => {
                return Err(Error::Usage(format!("unknown option `{argument}`")));
            }
            _ if path.is_some() => {
                return Err(Error::Usage(
                    "check takes exactly one path; pass a directory to check several files"
                        .to_owned(),
                ));
            }
            _ => path = Some(PathBuf::from(argument)),
        }
    }

    path.map_or_else(
        || Err(Error::Usage("check needs a path".to_owned())),
        |path| {
            Ok(Invocation::Check {
                path,
                format: format.unwrap_or_default(),
            })
        },
    )
}

/// A flag that reports and exits takes nothing else, so `--version --rules` is refused, not ranked.
///
/// Same reasoning as [`set_format`] one function down, and the same precedent: this parser answers
/// an ambiguous command line with an error rather than picking a winner. Ranking them would make
/// `tooprolix --version --rules` print one of the two things asked for and say nothing about the
/// other — a script reading the output would be silently short one answer. It also catches
/// `tooprolix --rules src`, where the user meant `check`.
///
/// # Errors
///
/// [`Error::Usage`] when anything at all follows the flag.
fn alone<I: Iterator<Item = Result<String, Error>>>(
    invocation: Invocation,
    flag: &str,
    rest: &mut I,
) -> Result<Invocation, Error> {
    match rest.next().transpose()? {
        None => Ok(invocation),
        Some(extra) => Err(Error::Usage(format!(
            "`{flag}` takes no other arguments; got `{extra}`"
        ))),
    }
}

/// Records the format, refusing a second `--format` rather than letting the last one win.
fn set_format(slot: &mut Option<Format>, format: Format) -> Result<(), Error> {
    if slot.is_some() {
        return Err(Error::Usage(
            "`--format` was given more than once; it takes exactly one value".to_owned(),
        ));
    }
    *slot = Some(format);
    Ok(())
}

/// `text` or `json`, and an error that names the alternatives rather than a silent fallback.
fn parse_format(value: &str) -> Result<Format, Error> {
    match value {
        "text" => Ok(Format::Text),
        "json" => Ok(Format::Json),
        other => Err(Error::Usage(format!(
            "unknown format `{other}`; expected `text` or `json`"
        ))),
    }
}

/// Every Python file under `root`, in whatever order the walk produced.
///
/// **Deliberately unsorted.** [`findings`] owns the ordering of the output, and a sorted file list
/// here would hand it an already-ordered input on most filesystems — which is how a missing sort
/// survives an end-to-end test. Errors are sorted at render time instead, in
/// [`report_skipped`], so the diagnostics stay reproducible without making the ordering guarantee
/// depend on the walk.
///
/// # Errors
///
/// [`Error::Missing`], [`Error::NotPython`] and [`Error::Walk`] — all three are "the tree could not
/// be read", never "the tree is clean".
fn python_files(root: &Path, config: &Config) -> Result<Walked, Error> {
    if !root.exists() {
        return Err(Error::Missing(root.to_path_buf()));
    }
    if root.is_file() && !is_python_source(root) {
        return Err(Error::NotPython(root.to_path_buf()));
    }

    let excluded = config::exclude_matcher(config)?;
    // Nothing to exclude, nothing to normalise: the walk is the one this tool has always done, on
    // the path exactly as typed. Kept as a branch rather than as a canonical walk that happens to
    // round-trip, so that a project without `exclude` cannot be affected by any of this at all.
    let walked = if excluded.is_empty() {
        root.to_path_buf()
    } else {
        // A glob is matched against the path relative to the CONFIGURATION FILE, and the walker
        // can only do that if the paths it reports are rooted in the same tree the base names.
        // `.`, `..` and a symlinked root each name the same file differently and defeat every
        // lexical answer — the recorded defect this module and `crate::config` both already
        // canonicalise against. Reported paths are put back under the typed root by `reroot`, so
        // this is invisible in the output.
        std::fs::canonicalize(root).map_err(|error| Error::Walk {
            path: root.to_path_buf(),
            message: error.to_string(),
        })?
    };

    // `require_git(false)`: a `.gitignore` means the same thing in an exported tarball as in the
    // checkout it came from, and without this the crate ignores gitignore files outside a repo.
    // Nothing here calls `follow_links`; see the module documentation for what that measured.
    let mut builder = WalkBuilder::new(&walked);
    builder.require_git(false);

    // `filter_entry` rather than `overrides`, and the reason is that the caller has to be able to
    // say whether `exclude` ACTUALLY removed anything. Read in the crate's `walk.rs`:
    // `should_skip_entry` — which is where `overrides` is consulted — runs *before* the filter
    // predicate, so with `overrides` set an excluded path is pruned before any code of ours can
    // observe it, and the only thing left to report on is the configuration's own say-so. That is
    // how the diagnostic came to blame `exclude` for emptiness it had no part in.
    //
    // The three properties that made `overrides` right are all kept, and none of them is
    // incidental to this choice:
    //   * directory pruning — the predicate returning false on a directory stops the descent, so a
    //     matched `vendor/` still costs one entry rather than a subtree, and a glob naming a
    //     directory still covers everything under it (that was always pruning's doing, never the
    //     glob matching descendants);
    //   * the root is still always visited — `skip_entry` returns early at depth 0, ahead of the
    //     filter, which is what keeps an explicitly named path checked;
    //   * `.gitignore` still has its say — its layers run first and this filter runs after, so a
    //     path is walked only if BOTH allow it. Identical to `overrides` here because we only ever
    //     build exclusions, never re-inclusions.
    // A list rather than a count, because the document has to NAME what was excluded. One value
    // serves as the gate for the stderr warning, the whole of its content and the whole of the JSON
    // field — `excluded.len()` at the call site, never a second number derived alongside it.
    //
    // `Mutex` rather than the `AtomicUsize` it replaces because a `Vec` has no atomic form;
    // `Arc<Mutex<…>>` and not a `Cell` because `filter_entry` demands `Send + Sync + 'static`.
    // ponytail: the walker is serial today (`build()`, not `build_parallel()`), so the lock is
    // never contended and its cost is a compare-and-swap per excluded path.
    let removed: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
    if !excluded.is_empty() {
        let observed = Arc::clone(&removed);
        // The reported paths are put back under the typed root, exactly as findings are, so a
        // relative invocation reports relative paths and the field can be pasted into a config.
        let (typed, canonical) = (root.to_path_buf(), walked.clone());
        builder.filter_entry(move |entry| {
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            if !excluded.matched(entry.path(), is_dir).is_ignore() {
                return true;
            }
            // Recorded only when the removal cost the run something it could have MEASURED. A
            // `README.md` the tool would never have read is a removed path but not a lost
            // measurement, and reporting it as one is the same over-claim one level in.
            //
            // A directory is recorded without knowing, and that is deliberate: finding out means
            // descending into it, which is the pruning that makes this affordable at all. So the
            // claim for a directory is conservative — it *might* have held Python — and the
            // residual is that excluding a directory of images still says paths were removed.
            if is_dir || is_python_source(entry.path()) {
                let path = reroot(&typed, &canonical, entry.path().to_path_buf());
                // A poisoned lock means a panic inside another `filter_entry` call, which cannot
                // happen here — nothing above can panic — and silently dropping the path would be
                // an under-report of exactly the kind this list exists to prevent.
                observed
                    .lock()
                    .expect("the walk is serial and nothing in this closure panics")
                    .push(path);
            }
            false
        });
    }

    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for entry in builder.build() {
        let entry = match entry {
            Ok(entry) => entry,
            // The walk is the THIRD channel a file can be lost through, after the read and the
            // parse, and it was the one still holding the all-or-nothing contract: one `?` here
            // threw away every path already collected and made the whole run exit 2. Measured on a
            // tree of one readable finding beside one `chmod 000` directory: `exit 2`, zero
            // findings, no document — the exact outcome this contract exists to delete.
            //
            // Depth is the discriminator, and it is the crate's own: an error at **depth 0** is the
            // root of the walk, so nothing was read and the run genuinely could not start (exit 2).
            // Anything deeper is part of a tree that did start. Verified against the walker rather
            // than assumed — an unreadable subdirectory reports `Some(1)`, one two levels down
            // `Some(3)`, an unreadable root `Some(0)`.
            //
            // Fails closed: an error carrying no path, or no depth at all (a malformed ignore file,
            // a symlink loop), is still fatal. Only a named entry below the root becomes a skip.
            Err(error) => {
                if let ignore::Error::WithPath { path, err } = &error
                    && err.depth().is_some_and(|depth| depth > 0)
                {
                    skipped.push(Skipped {
                        path: reroot(root, &walked, path.clone()).display().to_string(),
                        // The inner error, not `error`: the outer `WithPath` renders as
                        // `{path}: {err}` and `Skipped` already carries the path.
                        reason: err.to_string(),
                    });
                    continue;
                }
                return Err(Error::Walk {
                    path: root.to_path_buf(),
                    message: error.to_string(),
                });
            }
        };

        if !is_python_source(entry.path()) {
            continue;
        }
        match entry.file_type() {
            Some(kind) if kind.is_file() => files.push(reroot(root, &walked, entry.into_path())),
            // A directory named `thing.py` is not an unread source.
            Some(kind) if kind.is_dir() => {}
            // **Every** `*.py`-named symlink, without exception: the walk does not follow links, so
            // it did not read this entry, so the tree it belongs to was not read whole. No
            // resolution, no containment test, no question to get wrong.
            //
            // Three adversarial rounds produced three CRITICALs on one seam here, and the seam
            // was an exception that tried to decide when silence was safe:
            //
            //   1. silent whenever the target `exists()` — a target OUTSIDE the walked tree
            //      resolves just as well and is measured nowhere: a green `All checks passed!` over
            //      an unread `TPX001`;
            //   2. silent when the target canonicalised under the walk root — a sibling directory
            //      whose name is a mere string prefix of the root restored the same false green;
            //   3. silent when the target is under the root — and a target that IS under the root
            //      and still never walked (a non-`.py` name, a hidden directory, a gitignored path,
            //      an `exclude`d path) restored it again, four ways.
            //
            // The root cause never moved. The guard asked *"where does the target live"*, while the
            // invariant is *"was the target measured in this run"*, and each round answered the
            // wrong question more precisely. The fix is not a fourth guard: it is deleting the
            // exception, so `complete: false` and exit 1 follow from the shape of the code rather
            // than from a predicate that has to be right.
            //
            // A dangling link and a resolving one now take one path and carry one reason. The
            // `exists()`-versus-`symlink_metadata` reasoning recorded here existed only to separate
            // them and is gone with the distinction it served.
            //
            // The cost is measured and it is zero: symlinks named `*.py` number **zero** across all
            // six pinned corpus checkouts (crewAI 0, langgraph 0, openai-agents-python 0,
            // `OpenHands` 0, pydantic 0, requests 0) and zero in this repository. The earlier
            // "pydantic 343 -> 559" figure that once defended silence here was never about this arm
            // at all — it belongs to `follow_links` on a symlinked *directory*, which this arm never
            // sees, since `is_python_source` has already returned true.
            //
            // Naming a symlink DIRECTLY — `tooprolix check alias.py` — still measures it, and
            // that is deliberate rather than an inconsistency. This arm answers "is this tree whole",
            // where an unfollowed link is a hole; an explicit argument is an instruction about one
            // file and carries no claim about a tree. Ruff resolves explicit arguments past its own
            // exclusions for the same reason. The root of a walk is visited before this match, so
            // the two paths never meet.
            Some(kind) if kind.is_symlink() => skipped.push(Skipped {
                path: reroot(root, &walked, entry.into_path())
                    .display()
                    .to_string(),
                reason: "symlinks are not followed, so this file was not measured".to_owned(),
            }),
            // What is left is a FIFO, a socket or a device that happens to end in `.py`. It passes
            // the extension test and fails `is_file()`, so dropping it into neither channel would
            // claim `complete: true` about a tree holding a `.py` nobody opened. Reading it is not
            // the answer (a FIFO blocks forever); saying so is.
            //
            // `None` — which the crate produces only for a stdin entry this tool never asks for —
            // lands here too, because an entry whose kind cannot be established is exactly the one
            // not to claim as measured.
            _ => skipped.push(Skipped {
                path: reroot(root, &walked, entry.into_path())
                    .display()
                    .to_string(),
                reason: "not a regular file".to_owned(),
            }),
        }
    }
    // Taken out from behind the lock rather than unwrapped out of the `Arc`: the builder still owns
    // the closure that holds the second handle, so `Arc::into_inner` is `None` here — measured, and
    // it panicked on every run of this repository, whose own `exclude` makes the closure exist.
    let excluded = std::mem::take(
        &mut *removed
            .lock()
            .expect("the walk is serial and nothing in the filter panics"),
    );
    Ok(Walked {
        files,
        excluded,
        skipped,
    })
}

/// What a walk found, and what `exclude` kept it from finding.
struct Walked {
    /// Every Python file the walk yielded, unsorted — see [`python_files`].
    files: Vec<PathBuf>,
    /// Every path the exclude matcher removed that could have been measured, **observed during the
    /// walk**, unsorted.
    ///
    /// Not `config.exclude`, and not a flag beside a list read back off the configuration. This one
    /// list is the reason to print the empty-walk diagnostic, the whole of that sentence's content
    /// (as its length) *and* the whole of the document's `excluded` field, so the three cannot
    /// disagree — which the first two have, twice, in exactly this place: first a gate that read
    /// `!config.exclude.is_empty()`, then an honest gate whose sentence still listed every
    /// configured glob whether it fired or not.
    ///
    /// A pruned directory appears as **one** path, not as the subtree behind it; the walk
    /// deliberately never learns how big that subtree was, because learning it means descending
    /// into it and losing the pruning that makes `exclude` affordable at all.
    excluded: Vec<PathBuf>,
    /// Everything the **walk** could not measure, in the same shape [`read`] produces.
    ///
    /// A refusal, exactly like a read failure, and merged with those before anything looks at the
    /// list — a file lost to an unreadable parent directory and a file lost to a syntax error are
    /// the same fact for every consumer, and splitting them into two vocabularies is how one of
    /// them stays fatal after the other is fixed.
    ///
    /// Two sources: a directory the walker could not enter (below the root — at the root the run
    /// could not start and that is still [`Error::Walk`]), and an entry named `*.py` that is not a
    /// regular file.
    skipped: Vec<Skipped>,
}

/// Puts a walked path back under the root **as the user typed it**.
///
/// A no-op unless the walk was canonicalised, which is the only case where the two differ. The
/// prefix is not a guess: it is the path this walk was started from, so `strip_prefix` is exact
/// rather than a lexical comparison of two strings that might mean the same directory.
fn reroot(typed: &Path, walked: &Path, path: PathBuf) -> PathBuf {
    match path.strip_prefix(walked) {
        // The root itself, which is what `tooprolix check one_file.py` yields and nothing else.
        // `typed.join("")` would append a separator and invent a path that is not the one asked
        // about.
        Ok(rest) if rest.as_os_str().is_empty() => typed.to_path_buf(),
        Ok(rest) => typed.join(rest),
        // Unreachable — `walked` is by construction the prefix of everything this walk yields.
        // Keeping the walked path rather than dropping the file is the fail-loud half: an absolute
        // path in a finding is visible and wrong, a silently unmeasured file is neither.
        Err(_) => path,
    }
}

/// Reads and extracts every file, returning what parsed **and** what did not.
///
/// Infallible on purpose: there is nothing left here that can stop a run. Every failure is a
/// [`Skipped`] entry, and the caller turns a non-empty list into an incomplete run — never into a
/// refusal to report the files that were fine.
///
/// Plural, as the error it replaces was: reporting only the first failure makes fixing a repository
/// an exercise in re-running the tool once per broken file.
fn read(files: &[PathBuf]) -> (Vec<Source>, Vec<Skipped>, Vec<String>) {
    let mut sources = Vec::new();
    let mut skipped = Vec::new();
    // Collected rather than printed, because this loop runs in walk order and the walk is
    // deliberately unordered — see [`python_files`]. Printing from here made the diagnostics a
    // function of the directory layout; [`warn_sorted`] is where they become a function of the
    // tree. The same guard `report_skipped` and `Report::new` already apply, in the third place
    // that needed it.
    let mut warnings = Vec::new();

    for file in files {
        match extract::read_source(file).and_then(|text| {
            let blocks = extract::extract(file, &text)?;
            Ok(sources_of(blocks, &text, &mut warnings))
        }) {
            Ok(read) => sources.extend(read),
            // Both halves of the `and_then` land here, and that is the point: an io failure
            // (unreadable by permissions, vanished mid-walk) and a parse failure are the same
            // outcome for the caller — this file was not measured — and giving them two channels
            // is how one of them ends up still fatal after the other is fixed.
            Err(error) => skipped.push(Skipped {
                path: file.display().to_string(),
                reason: error.to_string(),
            }),
        }
    }

    (sources, skipped, warnings)
}

/// Prints collected diagnostics to stderr in a deterministic order.
///
/// One line per warning, sorted. Sorting the rendered strings rather than a key is deliberate and
/// enough: every line begins `warning: {path}:{line}:`, so the order is by path and the warnings
/// for one file end up adjacent — which is also what [`report_skipped`] does with its block.
fn warn_sorted(mut warnings: Vec<String>) {
    warnings.sort();
    for warning in warnings {
        eprintln!("{warning}");
    }
}

/// Pairs every block with the marker on the physical line above it, and reports the near-misses.
///
/// The near-miss diagnostic is written here rather than in [`report_unknown_marker_codes`] for one
/// reason: a near-miss is a *line*, and this is the only place that still has one. [`Source`]
/// carries what the marker silences, not the text it was written as, and giving it a third field to
/// carry a string only the warning reads would put a diagnostic channel inside the type that
/// callers of [`findings`] have to build.
///
/// Near-misses are appended to `warnings` rather than printed, so that [`warn_sorted`] can order
/// them: this function is called once per file from a loop running in walk order.
fn sources_of(blocks: Vec<ProseBlock>, text: &str, warnings: &mut Vec<String>) -> Vec<Source> {
    // Most files in a real repository yield no blocks at all, and indexing their lines to look
    // above zero of them is pure cost. The measured shape of the corpus is the argument: 663 files
    // on the reference checkout against 10 findings.
    if blocks.is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = text.lines().collect();

    blocks
        .into_iter()
        .map(|block| {
            // `line_start` is 1-based, so the line above it is index `line_start - 2`.
            let suppressed = block
                .line_start
                .checked_sub(2)
                .and_then(|index| lines.get(index))
                .and_then(|line| parse_marker(line))
                .unwrap_or_default();

            // TWO lines are checked for a near-miss, and which one carries it depends on the kind
            // of block below it. A comment marker that fails to parse stops being a directive, so
            // `extract` reads it as prose and it becomes the run's own FIRST line; a docstring
            // marker stays a comment and remains the line ABOVE. Checking only one of the two would
            // make the diagnostic depend on the block kind — the one thing the marker rule
            // deliberately does not do.
            for line_number in [block.line_start.saturating_sub(1), block.line_start] {
                if line_number > 0
                    && lines
                        .get(line_number - 1)
                        .is_some_and(|line| rules::is_near_miss(line))
                {
                    warnings.push(format!(
                        "warning: {}:{line_number}: this is not an opt-out marker and silences \
                         nothing; the form is `# !TPX001` — `# !TPX001,TPX002` for several, \
                         `# !TPX*` for all",
                        block.path.display(),
                    ));
                }
            }
            Source { block, suppressed }
        })
        .collect()
}

/// Says which markers named a code no rule answers to.
///
/// A warning and not a failure — see [`crate::rules`] for why this is loud where the configuration
/// is fatal. The finding still appears, so the typo fails closed either way.
///
/// Appends rather than prints, for the reason [`read`] does: `sources` is in walk order, so
/// emitting from here made the output depend on the filesystem. [`warn_sorted`] owns the order.
fn report_unknown_marker_codes(sources: &[Source], warnings: &mut Vec<String>) {
    for source in sources {
        for code in source.suppressed.unknown_codes() {
            warnings.push(format!(
                "warning: {}:{}: `{code}` in an opt-out marker is not a rule code; it silences \
                 nothing",
                source.block.path.display(),
                source.block.line_start.saturating_sub(1),
            ));
        }
    }
}

/// Every finding for `sources`, ordered.
///
/// Pure: no filesystem, no configuration discovery, no output. That is what makes the ordering
/// guarantee testable — hand it the same blocks in a different order and the result must be the
/// same bytes.
///
/// # Suppression happens here, and the order of the two halves matters
///
/// `TPX003` blocks are filtered **before** [`duplicates`] rather than after, because a cluster's
/// weakest edge is a property of its membership: dropping a member from a finished cluster would
/// change the similarity reported for the blocks that remain. `duplicates` already discards a
/// component that falls below two distinct members, so "a cluster of one is not a finding" comes
/// for free and is not implemented a second time.
///
/// A rule named in `ignore` is skipped entirely — a marker cannot switch it back on. That is the
/// recorded precedence: `ignore` is the wider instrument and a per-block marker is the narrower
/// one, so the narrow one can only ever *remove* findings, never add them.
#[must_use]
pub fn findings(sources: Vec<Source>, config: &Config) -> Vec<Finding> {
    let mut findings = Vec::new();

    let volume_blocks: Vec<ProseBlock> = sources
        .iter()
        .filter(|source| {
            let rule = Rule::volume_for(source.block.kind);
            !config.ignores(rule) && !source.suppressed.silences(rule)
        })
        // ponytail: one extra copy of the visible blocks, because `volume` and `duplicates` both
        // want an owned slice and only one of them can have the original. Measured scale: the
        // largest corpus checkout is 3 314 blocks. If that ever matters, the fix is for the
        // detectors to take `&[&ProseBlock]`, not a cleverer clone here.
        .map(|source| source.block.clone())
        .collect();
    findings.extend(
        volume(&volume_blocks, config.limits)
            .overruns
            .iter()
            .map(Finding::from_overrun),
    );

    if !config.ignores(Rule::DuplicateProse) {
        let duplicate_blocks: Vec<ProseBlock> = sources
            .into_iter()
            .filter(|source| !source.suppressed.silences(Rule::DuplicateProse))
            .map(|source| source.block)
            .collect();
        findings.extend(
            duplicates(&duplicate_blocks)
                .clusters
                .iter()
                .map(Finding::from_cluster),
        );
    }

    // Sorted on the way out and nowhere else. The walk hands blocks over in filesystem order, and
    // the two detectors are appended one after the other, so without this line the output is a
    // function of the directory layout rather than of the tree's contents.
    findings.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    findings
}

#[cfg(test)]
mod tests {
    use super::{
        Format, Invocation, Source, findings, parse, python_files, reader_stopped_reading,
        use_colour,
    };
    use crate::config::Config;
    use crate::detect::volume::Limits;
    use crate::extract::extract;
    use crate::rules::{Rule, parse_marker};
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};

    fn command(arguments: &[&str]) -> Result<Invocation, super::Error> {
        parse(arguments.iter().copied().map(OsString::from))
    }

    fn source(path: &str, text: &str, marker: Option<&str>) -> Vec<Source> {
        extract(Path::new(path), text)
            .expect("the fixture is valid Python")
            .into_iter()
            .map(|block| Source {
                block,
                suppressed: marker.and_then(parse_marker).unwrap_or_default(),
            })
            .collect()
    }

    /// Only a CLOSED PIPE is a clean stop. Every other write failure stays an error.
    ///
    /// This is the narrowness of the broken-pipe fix, and it is the half that end-to-end tests
    /// cannot reach: a test cannot portably fill a disk or break a redirect, so the only way to pin
    /// "`ENOSPC` is still loud" is to ask the decision directly. Without this table,
    /// `Error::Output(_) => Success` — dropping the `kind` test entirely — passes the whole suite,
    /// and a run whose findings never reached the file exits 0.
    ///
    /// `Walk` is in the table because the predicate must key on the VARIANT too: an error carrying
    /// no io at all must never be read as a closed pipe.
    #[test]
    fn only_a_closed_pipe_is_a_clean_stop() {
        use std::io::ErrorKind;

        for (kind, clean, what) in [
            (ErrorKind::BrokenPipe, true, "a reader that stopped reading"),
            (ErrorKind::StorageFull, false, "a full disk"),
            (ErrorKind::PermissionDenied, false, "a refused redirect"),
            (ErrorKind::Other, false, "an unclassified io failure"),
        ] {
            let error = super::Error::Output(std::io::Error::new(kind, "measured"));

            assert_eq!(
                reader_stopped_reading(&error),
                clean,
                "{what} ({kind:?}) was judged wrongly"
            );
        }

        assert!(
            !reader_stopped_reading(&super::Error::Walk {
                path: PathBuf::from("."),
                message: "measured".to_owned(),
            }),
            "an error carrying no io at all was read as a closed pipe"
        );
    }

    /// Colour is a decision about two facts, and both of them have to be able to veto it.
    ///
    /// A table rather than four tests, because what is being pinned is that **neither** input is
    /// ignored: a predicate that dropped the terminal check would still pass every `NO_COLOR` row,
    /// and one that dropped `NO_COLOR` would still pass every pipe row. The empty-value row is the
    /// `no-color.org` contract read literally — *present and not an empty string* — and it is the
    /// row that separates "the variable is set" from "the variable asks for no colour".
    ///
    /// The predicate is pure so that this can be a table at all: the terminal test and the
    /// environment read happen once, at the single call site, where neither is testable.
    #[test]
    fn colour_needs_a_terminal_and_the_absence_of_no_color() {
        for (terminal, no_color, expected) in [
            (true, None, true),
            (true, Some(""), true),
            (true, Some("1"), false),
            (true, Some("0"), false),
            (false, None, false),
            (false, Some("1"), false),
        ] {
            assert_eq!(
                use_colour(terminal, no_color.map(OsStr::new)),
                expected,
                "terminal={terminal}, NO_COLOR={no_color:?}"
            );
        }
    }

    /// A long comment run, as a `(path, source)` pair, sized to fire `TPX001` at the default 150.
    fn long_comment(path: &str, subject: &str) -> (String, String) {
        let line = format!("# {subject} is described here at length and on purpose.\n");
        (path.to_owned(), line.repeat(20))
    }

    #[test]
    fn the_grammar_is_the_one_the_help_documents() {
        assert_eq!(
            command(&["check", "src"]).expect("valid"),
            Invocation::Check {
                path: PathBuf::from("src"),
                format: Format::Text
            }
        );
        assert_eq!(
            command(&["check", ".", "--format", "json"]).expect("valid"),
            Invocation::Check {
                path: PathBuf::from("."),
                format: Format::Json
            }
        );
        assert_eq!(
            command(&["check", "--format=json", "a.py"]).expect("valid"),
            Invocation::Check {
                path: PathBuf::from("a.py"),
                format: Format::Json
            }
        );
        assert_eq!(command(&["--help"]).expect("valid"), Invocation::Help);
        assert_eq!(
            command(&["check", "--help"]).expect("valid"),
            Invocation::Help
        );
        // `-V` and not `-v`: the short form is ruff's, checked against ruff 0.16.0 rather than
        // guessed, and `-v` is what ruff spells `--verbose`. There is no `-r` for `--rules`,
        // because ruff has no such flag to copy and inventing one is what the task ruled out.
        assert_eq!(command(&["--version"]).expect("valid"), Invocation::Version);
        assert_eq!(command(&["-V"]).expect("valid"), Invocation::Version);
        assert_eq!(command(&["--rules"]).expect("valid"), Invocation::Rules);
    }

    /// Every way of getting the command line wrong is an error rather than a default.
    ///
    /// `--format yaml` silently falling back to text would be a machine-readable pipeline quietly
    /// receiving prose.
    #[test]
    fn a_malformed_command_line_is_never_a_silent_default() {
        for arguments in [
            vec![],
            vec!["lint", "src"],
            vec!["check"],
            vec!["check", "a", "b"],
            vec!["check", "src", "--format"],
            vec!["check", "src", "--format", "yaml"],
            vec!["check", "src", "--format=yaml"],
            // Given twice: last-one-wins would write prose into a pipe that asked for a document.
            vec!["check", "src", "--format", "json", "--format", "text"],
            vec!["check", "src", "--format=json", "--format=json"],
            vec!["check", "src", "--fix"],
            // Two reporting flags at once: ranking them would answer one of the two questions
            // asked and stay silent about the other. Same rule as `--format` twice.
            vec!["--version", "--rules"],
            vec!["--rules", "--version"],
            vec!["-V", "--help"],
            vec!["--rules", "src"],
        ] {
            assert!(
                command(&arguments).is_err(),
                "accepted a malformed command line: {arguments:?}"
            );
        }
    }

    /// The ordering guarantee, isolated from the walk.
    ///
    /// The blocks arrive in the reverse of the order the output must have, and both detectors
    /// contribute, so removing the sort in `findings` reddens exactly this test. That is the whole
    /// design of the function: the walk is unsorted precisely so this cannot pass by accident.
    #[test]
    fn the_findings_are_ordered_whatever_order_the_blocks_arrive_in() {
        let (zebra, zebra_text) = long_comment("z.py", "the zebra module");
        let (alpha, alpha_text) = long_comment("a.py", "the alpha module");
        let shared = "# One rationale, written once and copied into a second module verbatim,\n\
                      # which is what makes it a duplicate rather than a coincidence.\n";

        let mut sources = source(&zebra, &zebra_text, None);
        sources.extend(source("m.py", shared, None));
        sources.extend(source(&alpha, &alpha_text, None));
        sources.extend(source("b.py", shared, None));

        let rendered: Vec<String> = findings(sources, &Config::default())
            .iter()
            .map(ToString::to_string)
            .collect();

        assert_eq!(
            rendered
                .iter()
                .map(|line| line.split(':').next().expect("a path"))
                .collect::<Vec<_>>(),
            vec!["a.py", "b.py", "z.py"],
            "got {rendered:#?}"
        );
    }

    /// The paired opt-out: the same block, with and without a marker.
    ///
    /// One assertion without the other proves nothing — a rule that never fires passes the marked
    /// half, and a marker that is never read passes the unmarked half.
    #[test]
    fn a_marker_removes_exactly_its_own_finding() {
        let (path, text) = long_comment("api.py", "the retry policy");

        let unmarked = findings(source(&path, &text, None), &Config::default());
        let marked = findings(source(&path, &text, Some("# !TPX001")), &Config::default());
        let wrong_code = findings(source(&path, &text, Some("# !TPX002")), &Config::default());
        let mistyped = findings(source(&path, &text, Some("# !TPX999")), &Config::default());
        let blanket = findings(source(&path, &text, Some("# !TPX*")), &Config::default());
        // The 0.1.0 spelling is not a marker any more, and a hard replacement means it silences
        // nothing rather than silencing what it used to.
        let zero_one_zero = findings(
            source(&path, &text, Some("# tooprolix: noqa TPX001")),
            &Config::default(),
        );

        assert_eq!(unmarked.len(), 1, "the fixture must be able to fire");
        assert_eq!(unmarked[0].code, Rule::CommentVolume);
        assert!(marked.is_empty(), "the marker did not silence its rule");
        assert_eq!(
            wrong_code.len(),
            1,
            "a marker for TPX002 silenced TPX001: {wrong_code:#?}"
        );
        assert_eq!(
            mistyped.len(),
            1,
            "an unknown code silenced a real rule: {mistyped:#?}"
        );
        assert!(
            blanket.is_empty(),
            "the blanket token must silence everything"
        );
        assert_eq!(
            zero_one_zero.len(),
            1,
            "the 0.1.0 marker still silenced a rule — the replacement is hard, with no alias \
             period: {zero_one_zero:#?}"
        );
    }

    /// Marking enough members that fewer than two remain removes the finding altogether.
    ///
    /// This is **only** that half. It was named for the "suppress before clustering" guarantee and
    /// could not pin it: its three blocks are byte-identical, so every edge scores 1.0 and the
    /// weakest edge is 1.0 whichever members survive, and it marks two of three, so the cluster
    /// dies under a correct implementation and under an after-the-fact one alike. Measured: an
    /// implementation that removed members from a finished cluster left all 128 tests green.
    /// `a_marker_recomputes_the_weakest_edge_over_the_surviving_members` is the test that separates
    /// them; this one keeps the "fewer than two is not a finding" rule, which `duplicates` gives us
    /// for free and which nothing else asserts.
    #[test]
    fn a_cluster_that_falls_below_two_members_disappears_entirely() {
        let shared = "# One rationale, written once and copied into two more modules verbatim,\n\
                      # which is what makes this a cluster rather than a coincidence.\n";
        let three: Vec<Source> = source("a.py", shared, None)
            .into_iter()
            .chain(source("b.py", shared, None))
            .chain(source("c.py", shared, None))
            .collect();
        let two_marked: Vec<Source> = source("a.py", shared, None)
            .into_iter()
            .chain(source("b.py", shared, Some("# !TPX003")))
            .chain(source("c.py", shared, Some("# !TPX003")))
            .collect();

        let all = findings(three, &Config::default());
        let reduced = findings(two_marked, &Config::default());

        assert_eq!(all.len(), 1);
        assert!(
            all[0].to_string().contains("in 3 places"),
            "{}",
            all[0].to_string()
        );
        assert!(
            reduced.is_empty(),
            "a cluster of one survived: {reduced:#?}"
        );
    }

    /// Suppression happens **before** clustering, and the weakest edge proves it.
    ///
    /// The guarantee is not "the marked block disappears" — it is that everything derived from a
    /// cluster's membership is recomputed over the membership that survived. The weakest edge is
    /// the only observable that can tell the two apart, so the fixture is built around it:
    ///
    /// * `a.py` and `b.py` carry byte-identical prose, so the edge between them is exactly 1.0;
    /// * `c.py` is the same block with **one word** changed (`call` -> `request`), so it still
    ///   clusters — task 4 measured that one substitution needs `W >= 23` words at
    ///   `SHINGLE_K = 3`, and this is the block `tests/detect.rs` already pins at **0.900** — but
    ///   below 1.0, which makes it the component's weakest link;
    /// * exactly **one** member is marked, and it is the loose one, so the cluster survives with two
    ///   members and there is still an answer to compare.
    ///
    /// Filtering before `duplicates` therefore has to report `a.py ~ b.py` at 1.000. Removing `c.py`
    /// from a finished cluster instead reports `a.py ~ c.py` at 0.821 — an edge to a file the user
    /// just suppressed, with a score for a relationship that is no longer in the finding. Both
    /// halves are asserted, because "the count dropped" is true either way.
    #[test]
    fn a_marker_recomputes_the_weakest_edge_over_the_surviving_members() {
        // Arrange
        let exact = "# The retry budget here is deliberately small, and that matters because\n\
                     # the upstream service rate limits us on every fourth call.\n";
        let loose = "# The retry budget here is deliberately small, and that matters because\n\
                     # the upstream service rate limits us on every fourth request.\n";
        let unmarked: Vec<Source> = source("a.py", exact, None)
            .into_iter()
            .chain(source("b.py", exact, None))
            .chain(source("c.py", loose, None))
            .collect();
        let loose_member_marked: Vec<Source> = source("a.py", exact, None)
            .into_iter()
            .chain(source("b.py", exact, None))
            .chain(source("c.py", loose, Some("# !TPX003")))
            .collect();

        // Act
        let before = findings(unmarked, &Config::default());
        let after = findings(loose_member_marked, &Config::default());

        // Assert — the fixture is capable of showing the difference at all ...
        assert_eq!(before.len(), 1, "{before:#?}");
        let before_line = before[0].to_string();
        assert!(
            before_line.contains("in 3 places") && before_line.contains("c.py:1-2"),
            "the loose member never joined the cluster, so nothing is being suppressed: \
             {before_line}"
        );
        assert!(
            before_line.contains("~ c.py:1-2, similarity 0.900"),
            "the loose member is not the weakest link, so removing it would change nothing: \
             {before_line}"
        );

        // ... and after marking it, the weakest edge is the one between the SURVIVORS.
        assert_eq!(after.len(), 1, "{after:#?}");
        let after_line = after[0].to_string();
        assert_eq!(
            after_line,
            "a.py:1-2: TPX003 same explanation in 2 places: b.py:1-2 \
             (weakest a.py:1-2 ~ b.py:1-2, similarity 1.000)"
        );
        assert!(
            !after_line.contains("c.py"),
            "the finding names the file the user suppressed: {after_line}"
        );
    }

    /// `ignore` is wider than a marker, and a marker cannot undo it.
    #[test]
    fn a_rule_in_ignore_stays_off_and_no_marker_turns_it_back_on() {
        let (path, text) = long_comment("api.py", "the retry policy");
        let ignored = Config {
            ignore: vec![Rule::CommentVolume],
            ..Config::default()
        };

        assert!(findings(source(&path, &text, None), &ignored).is_empty());
        assert!(
            findings(source(&path, &text, Some("# !TPX003")), &ignored).is_empty(),
            "a marker for another rule re-enabled an ignored one"
        );
    }

    /// One block can be both an overrun and a cluster member, and both findings must survive.
    ///
    /// A `(path, line)` deduplication over rule codes would silently eat one of them, which is
    /// forbidden rather than discouraged.
    #[test]
    fn one_block_can_produce_two_findings_under_two_codes() {
        let line = "# The settlement window is described here at length and on purpose.\n";
        let long_and_shared = line.repeat(20);

        let sources: Vec<Source> = source("a.py", &long_and_shared, None)
            .into_iter()
            .chain(source("b.py", &long_and_shared, None))
            .collect();

        let found = findings(sources, &Config::default());
        let at_a: Vec<Rule> = found
            .iter()
            .filter(|finding| finding.at.path == "a.py")
            .map(|finding| finding.code)
            .collect();

        assert_eq!(at_a, vec![Rule::CommentVolume, Rule::DuplicateProse]);
    }

    /// The configured limits are the ones the detector runs with, and the boundary is `>`.
    #[test]
    fn the_configured_limit_is_the_last_size_still_allowed() {
        let text = format!("\"\"\"Overview.\n{}\"\"\"\n", "word ".repeat(9));
        let at_the_limit = Config {
            limits: Limits {
                docstring_max_volume: 10,
                ..Limits::default()
            },
            ..Config::default()
        };
        let one_lower = Config {
            limits: Limits {
                docstring_max_volume: 9,
                ..Limits::default()
            },
            ..Config::default()
        };

        assert!(
            findings(source("api.py", &text, None), &at_the_limit).is_empty(),
            "a block of exactly the limit fired"
        );
        assert_eq!(
            findings(source("api.py", &text, None), &one_lower).len(),
            1,
            "a block one word over the limit stayed silent"
        );
    }

    /// A directory walk must not be able to report "clean" because it visited nothing it should
    /// have visited.
    #[test]
    fn the_walk_finds_python_and_refuses_everything_else() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dup-corpus");

        let walked = python_files(&root, &Config::default()).expect("the fixture tree is readable");
        assert!(
            walked.excluded.is_empty(),
            "a walk with no `exclude` configured reported that it excluded something: {:?}",
            walked.excluded
        );
        let mut names: Vec<String> = walked
            .files
            .iter()
            .map(|file| {
                file.file_name()
                    .expect("a file has a name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();

        assert_eq!(
            names,
            vec![
                "client.py",
                "config.py",
                "legacy.py",
                "poller.py",
                "worker.py"
            ]
        );
        assert!(python_files(&root.join("nope"), &Config::default()).is_err());
        assert!(
            python_files(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("README.md")
                    .as_path(),
                &Config::default()
            )
            .is_err(),
            "a non-Python file named directly is not an empty result"
        );
    }
}
