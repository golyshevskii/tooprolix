//! `tooprolix check <path>`: the walk, the exit contract, and the two output formats.
//!
//! # Why all of this is in the library
//!
//! `src/main.rs` is a three-line wrapper around [`run`] and holds no logic at all. That is a
//! requirement rather than a style preference: how this tool is *delivered* — a standalone binary,
//! a pyo3 feature gate, or a console script that runs the CLI inside `CPython` — is a decision
//! deferred to `package-python-distribution-and-publish-0-1-0`, and with the logic in the library
//! this file is byte-identical under all three. `proj-lib-main-split` in rust-skills says the same
//! thing for the same reason.
//!
//! # The exit contract
//!
//! | code | meaning |
//! |---|---|
//! | 0 | the tree was read and has no findings |
//! | 1 | the tree was read and has findings |
//! | 2 | the tree could not be read: a bad path, a file that does not parse, a broken configuration |
//!
//! The three codes are ruff's (`crates/ruff/src/lib.rs`, `ExitStatus`), and the third is the whole
//! point of having one: **a file that does not parse must never be reportable as "clean"**.
//!
//! # ⚠️ A failed run prints no findings at all — and that part is OURS, not ruff's
//!
//! Not the findings of the files that happened to parse: **none**. A partial list is read by a
//! human as the state of the repository, and a repository that was never fully measured has no
//! state to report. [`Error::Unreadable`] carries every failure and stdout stays empty.
//!
//! **This is a decision, not an inheritance, and the reason recorded here used to say otherwise.**
//! It cited ruff's `ExitStatus` as though ruff behaved this way. It does not. Executed against the
//! pinned reference, on a directory holding one unparsable file and one lintable one:
//!
//! ```text
//! $ uvx ruff@0.16.0 check --isolated --select F <dir>
//! invalid-syntax: Expected a parameter or the end of the parameter list  --> bad.py:1:12
//! F401 [*] `os` imported but unused                                      --> good.py:1:8
//! Found 3 errors.
//! ```
//!
//! Ruff reports the syntax error **as a diagnostic** and lints the rest of the tree. It takes the
//! opposite trade-off deliberately: a linter people run on every save has to stay useful while one
//! file is mid-edit. We take this one because a prose *budget* is a claim about a whole tree —
//! `TPX003` is cross-file by construction, so a cluster computed over a subset is not a smaller
//! true answer, it is a different one.
//!
//! ## What that cost was, and the half of it that is now paid
//!
//! Measured on the pinned ruff checkout (`a2635fd8`) before this key existed:
//!
//! | | |
//! |---|---|
//! | `tooprolix check /path/to/ruff` | **exit 2**, **0** findings, **375** stderr lines |
//! | why | 374 of its files are deliberately-unparsable parser fixtures |
//!
//! A first adopter whose repository contains a parser test-corpus had no way to run this tool over
//! it except one invocation per subdirectory. [`crate::config`]'s `exclude` is the answer, and it
//! is deliberately the *narrow* one: it moves the boundary of what is measured, so a tree the
//! project never claimed is silently out of scope rather than partially read. It does **not**
//! soften what happens inside that boundary — an unparsable file that nobody excluded is still
//! exit 2, and making that graceful is a separate, breaking change.
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
//! `tooprolix check src` reports `src/api.py:1`; `tooprolix check .` reports `./api.py:1`. The
//! canonical form is used for finding `pyproject.toml` (see [`crate::config`]) and nowhere else,
//! because a user who typed a relative path wants a relative finding they can paste into an editor.
//!
//! # A single file is a legal target, and it measures less than it looks like it does
//!
//! `tooprolix check one_file.py` is supported and useful — a pre-commit hook over the changed files
//! is exactly this. But `TPX003` is cross-file by construction: `duplicates` compares the blocks it
//! was handed and nothing else, so a single-file run can only ever find duplicates *inside that
//! file*. A user who reads exit 0 there as a verdict on the repository has been misled by silence,
//! so [`HELP`] says it in as many words.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ignore::WalkBuilder;
use thiserror::Error as ThisError;

use crate::config::{self, Config};
use crate::detect::duplicate::duplicates;
use crate::detect::volume::volume;
use crate::extract::{self, ProseBlock, is_python_source};
use crate::finding::{Finding, Report};
use crate::rules::{self, Rule, Suppression, parse_marker};

/// The `--help` text, and the only place the command grammar is written down.
pub const HELP: &str = "\
tooprolix — a prose budget linter for Python.

Usage:
  tooprolix check <path> [--format text|json]
  tooprolix --help

Arguments:
  <path>    A Python file, a directory, or `.`. Directories are walked; `.gitignore`
            is respected, symlinks are not followed, and hidden entries are skipped.
            There is no `--` end-of-options marker: a path whose name begins with
            `-` is read as an option, so write it as `./-name.py`.

Options:
  --format  `text` (default) writes one line per finding to stdout.
            `json` writes a versioned document: {\"schema_version\", \"findings\"},
            including on a clean run. Giving it twice is an error, not last-wins.
  --help    Show this text.

Rules:
  TPX001    A comment run longer than `comment-max-volume` words.
  TPX002    A docstring longer than `docstring-max-volume` words.
  TPX003    One explanation repeated across two or more blocks.

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
  0   no findings
  1   findings were reported
  2   the tree could not be read (bad path, unparsable file, broken configuration);
      no findings are printed, because a partial list is not a measurement
";

/// The three outcomes of a run — ruff's `ExitStatus`, and the reason it has three and not two.
///
/// `#[non_exhaustive]` because a fourth outcome is conceivable and matching on this from outside
/// the crate must keep compiling across one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExitStatus {
    /// The tree was read and there is nothing to report.
    Success,
    /// The tree was read and there are findings.
    Failure,
    /// The tree could not be read.
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
    /// so a fourth outcome is still a compile error here — which is the point of doing it once.
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failure => 1,
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
}

/// Everything that makes a run exit 2.
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

    /// One or more files could not be read or parsed.
    ///
    /// Plural on purpose: reporting only the first failure makes fixing a repository an exercise in
    /// re-running the tool once per broken file.
    #[error("{}", render_failures(.0))]
    Unreadable(Vec<(PathBuf, extract::Error)>),
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
/// The two entry points — `src/main.rs` and the `tooprolix` console script in `src/lib.rs` — differ
/// only in the *type* of the exit code they return, never in how a failure is reported. Rendering
/// it in each of them would have been two lines duplicated and one place for the wording to drift,
/// so the rendering lives here and both go through it. [`execute`] is the same run with the failure
/// still a value, for a caller that wants to handle it rather than print it.
pub(crate) fn status<I: IntoIterator<Item = OsString>>(arguments: I) -> ExitStatus {
    execute(arguments).unwrap_or_else(|error| {
        eprintln!("error: {error}");
        ExitStatus::Error
    })
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
            print!("{HELP}");
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
        excluded_any,
    } = python_files(&path, &config)?;
    if files.is_empty() {
        // A walk that visited nothing scores every repository clean. Saying so is the difference
        // between "no findings" and "no measurement".
        eprintln!("warning: no Python files under {}", path.display());
        // ... and when `exclude` is what emptied it, name that too. On its own the line above
        // blames an *absence* of Python, which for `exclude = ["*"]` is not what happened and
        // sends the reader looking for files that were there all along. The exit code stays 0
        // because it is honest — there really are no findings — so this line is the only thing
        // standing between an excluded tree and a tree that was measured and found clean.
        //
        // Gated on what the WALK observed, never on `!config.exclude.is_empty()`: a glob that
        // matched nothing did not empty anything, and saying it did is the same wrong-cause defect
        // in the opposite direction.
        if excluded_any {
            eprintln!(
                "warning: `exclude` in {} removed paths from this walk ({}); an excluded tree is \
                 not a measured one",
                config.source.as_ref().map_or_else(
                    || "the configuration".to_owned(),
                    |p| p.display().to_string()
                ),
                config
                    .exclude
                    .iter()
                    .map(|glob| format!("`{glob}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }

    let sources = read(&files)?;
    report_unknown_marker_codes(&sources);
    let findings = findings(sources, &config);
    let status = if findings.is_empty() {
        ExitStatus::Success
    } else {
        ExitStatus::Failure
    };

    // The empty case is written in JSON and not in text, and that asymmetry is the point. A human
    // reading a clean run wants silence; a consumer parsing stdout wants a document, and a
    // successful run that emits zero bytes is a parse error at the far end that looks exactly like
    // a crash. The one thing neither format ever prints is a *partial* list — that path returned
    // above, as `Error`.
    match format {
        Format::Text => {
            for finding in &findings {
                println!("{finding}");
            }
        }
        Format::Json => print!("{}", Report::new(findings).to_json()),
    }
    Ok(status)
}

/// Parses `tooprolix check <path> [--format …]` and nothing else.
///
/// Hand-written, and that is a recorded decision: the grammar is one subcommand, one positional and
/// one option, so a derive-macro argument parser would be a dependency tree and a second home for
/// the help text that [`HELP`] already owns.
///
/// **`arguments` does NOT include the program name.** It used to, silently — this function opened
/// with `.skip(1)` while [`run`]'s parameter was documented as "the command line", so a caller
/// building the arguments by hand (which the packaging task will) would pass `["check", "."]` and
/// be told that `.` is an unknown subcommand. The `.skip(1)` now lives in `src/main.rs`, where
/// turning `argv` into arguments is the caller's own convention rather than a hidden one here.
fn parse<I: IntoIterator<Item = OsString>>(arguments: I) -> Result<Invocation, Error> {
    let mut arguments = arguments.into_iter().map(|argument| {
        argument
            .into_string()
            .map_err(|raw| Error::Usage(format!("argument is not valid UTF-8: {}", raw.display())))
    });

    let Some(first) = arguments.next().transpose()? else {
        return Err(Error::Usage("expected a subcommand".to_owned()));
    };
    if matches!(first.as_str(), "--help" | "-h") {
        return Ok(Invocation::Help);
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
/// [`render_failures`], so the diagnostics stay reproducible without making the ordering guarantee
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
    let observed = Arc::new(AtomicBool::new(false));
    if !excluded.is_empty() {
        let seen = Arc::clone(&observed);
        builder.filter_entry(move |entry| {
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            if excluded.matched(entry.path(), is_dir).is_ignore() {
                seen.store(true, Ordering::Relaxed);
                return false;
            }
            true
        });
    }

    let mut files = Vec::new();
    for entry in builder.build() {
        let entry = entry.map_err(|error| Error::Walk {
            path: root.to_path_buf(),
            message: error.to_string(),
        })?;
        if entry.file_type().is_some_and(|kind| kind.is_file()) && is_python_source(entry.path()) {
            files.push(reroot(root, &walked, entry.into_path()));
        }
    }
    Ok(Walked {
        files,
        excluded_any: observed.load(Ordering::Relaxed),
    })
}

/// What a walk found, and whether `exclude` is the reason it did not find more.
struct Walked {
    /// Every Python file the walk yielded, unsorted — see [`python_files`].
    files: Vec<PathBuf>,
    /// Set when the exclude matcher really did remove a path, **observed during the walk**.
    ///
    /// Not `!config.exclude.is_empty()`. A configured glob that matches nothing is not an
    /// exclusion, and reporting it as one blames a rule that never fired for an empty tree it had
    /// no part in emptying. The distinction is only knowable by watching the walk, which is why
    /// this rides back with the files instead of being re-derived from the configuration.
    excluded_any: bool,
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

/// Reads and extracts every file, or fails with **all** of the failures.
///
/// Nothing is returned when anything failed, and that is the fail-loud half of the exit contract:
/// findings from the files that did parse would look like a measurement of a tree that was never
/// measured.
fn read(files: &[PathBuf]) -> Result<Vec<Source>, Error> {
    let mut sources = Vec::new();
    let mut failures = Vec::new();

    for file in files {
        match extract::read_source(file).and_then(|text| {
            let blocks = extract::extract(file, &text)?;
            Ok(sources_of(blocks, &text))
        }) {
            Ok(read) => sources.extend(read),
            Err(error) => failures.push((file.clone(), error)),
        }
    }

    if failures.is_empty() {
        Ok(sources)
    } else {
        Err(Error::Unreadable(failures))
    }
}

/// Pairs every block with the marker on the physical line above it, and reports the near-misses.
///
/// The near-miss diagnostic is written here rather than in [`report_unknown_marker_codes`] for one
/// reason: a near-miss is a *line*, and this is the only place that still has one. [`Source`]
/// carries what the marker silences, not the text it was written as, and giving it a third field to
/// carry a string only the warning reads would put a diagnostic channel inside the type that
/// callers of [`findings`] have to build.
fn sources_of(blocks: Vec<ProseBlock>, text: &str) -> Vec<Source> {
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
                    eprintln!(
                        "warning: {}:{line_number}: this is not an opt-out marker and silences \
                         nothing; the form is `# !TPX001` — `# !TPX001,TPX002` for several, \
                         `# !TPX*` for all",
                        block.path.display(),
                    );
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
fn report_unknown_marker_codes(sources: &[Source]) {
    for source in sources {
        for code in source.suppressed.unknown_codes() {
            eprintln!(
                "warning: {}:{}: `{code}` in an opt-out marker is not a rule code; it silences \
                 nothing",
                source.block.path.display(),
                source.block.line_start.saturating_sub(1),
            );
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

/// Renders every read failure, one per line, sorted by path.
///
/// Sorted here rather than in the walk, so that the walk can stay unordered and keep
/// [`findings`]'s sort load-bearing.
fn render_failures(failures: &[(PathBuf, extract::Error)]) -> String {
    let mut lines: Vec<String> = failures
        .iter()
        .map(|(path, error)| format!("{}: {error}", path.display()))
        .collect();
    lines.sort();
    format!(
        "{}\n{} file(s) could not be read; no findings are reported for a tree that was not fully \
         measured",
        lines.join("\n"),
        failures.len()
    )
}

#[cfg(test)]
mod tests {
    use super::{Format, Invocation, Source, findings, parse, python_files};
    use crate::config::Config;
    use crate::detect::volume::Limits;
    use crate::extract::extract;
    use crate::rules::{Rule, parse_marker};
    use std::ffi::OsString;
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
            before_line.contains("in 3 places") && before_line.contains("c.py:1"),
            "the loose member never joined the cluster, so nothing is being suppressed: \
             {before_line}"
        );
        assert!(
            before_line.contains("~ c.py:1, similarity 0.900"),
            "the loose member is not the weakest link, so removing it would change nothing: \
             {before_line}"
        );

        // ... and after marking it, the weakest edge is the one between the SURVIVORS.
        assert_eq!(after.len(), 1, "{after:#?}");
        let after_line = after[0].to_string();
        assert_eq!(
            after_line,
            "a.py:1: TPX003 same explanation in 2 places: b.py:1 \
             (weakest a.py:1 ~ b.py:1, similarity 1.000)"
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
            !walked.excluded_any,
            "a walk with no `exclude` configured reported that it excluded something"
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
