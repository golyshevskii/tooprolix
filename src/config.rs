//! `[tool.tooprolix]` in `pyproject.toml`: four keys, and every way of getting them wrong.
//!
//! ```toml
//! [tool.tooprolix]
//! ignore = ["TPX003"]
//! exclude = ["tests/fixtures", "vendor"]
//! comment-max-volume = 150
//! docstring-max-volume = 200
//! ```
//!
//! That is the whole surface. `select`/`extend-select`, per-file settings and any other
//! per-repository calibration remain out of scope; the scope guard is lifted for these four keys and
//! for nothing else. Without a configuration file the behaviour is exactly
//! [`Limits::default`] with nothing ignored and nothing excluded, and that is pinned by a test
//! rather than left to coincidence.
//!
//! # `exclude` is a measurement boundary, and that is why it exists at all
//!
//! It is not configurability for its own sake, and its job outlived the strict exit contract it was
//! introduced under. A repository that *legitimately* contains invalid Python — this crate's own
//! `tests/fixtures/broken/`, or the pinned ruff checkout with its 374 deliberately-unparsable parser
//! fixtures — is the case neither `.gitignore` (which does not cover committed files) nor an opt-out
//! marker (which cannot save a file that never parses far enough for its comments to be read) can
//! reach. `exclude` is the only lever that does.
//!
//! It is deliberately **not** graceful handling of unreadable files, and the two have stayed
//! distinct now that both exist: `exclude` says a path was never part of the measurement, graceful
//! says the measurement met something it could not read. That is why they land in different fields
//! of [`crate::finding::Report`] and why only the second one sets `complete: false` — see the
//! exit contract in [`crate::cli`]. Without `exclude`, the ruff checkout is a run that reports 374
//! skipped files on every invocation; with it, the tree is measured whole inside a boundary the
//! project drew on purpose, and the text output stays silent.
//!
//! # Where the file is looked for, and the answer is one answer
//!
//! **The nearest `pyproject.toml` at or above the checked path**, searching upwards to the
//! filesystem root, first match wins. Not "the current directory": with a cwd rule,
//! `tooprolix check src/api.py` and `cd src && tooprolix check api.py` would apply different
//! limits to the same file, and a CI job that changed directory would silently change the
//! thresholds it was enforcing.
//!
//! **The path is canonicalised before anything is compared or walked.** `..` components and
//! symlinks defeat every lexical answer to "which directory is this in", and that is a recorded
//! defect of this project rather than a hypothesis. [`std::fs::canonicalize`] resolves both, which
//! is why the search starts from its result and not from the string the user typed. (The *output*
//! still uses the path as typed — see [`crate::cli`] — because a user who wrote a relative path
//! wants relative findings.)
//!
//! # Every decision this module had to make, and what it decided
//!
//! | situation | behaviour | why not the other thing |
//! |---|---|---|
//! | no `pyproject.toml` anywhere | defaults, silently | a missing config is the normal case, not an error |
//! | `pyproject.toml` with no `[tool.tooprolix]` | defaults, silently | same |
//! | `tool` or `tool.tooprolix` present but **not a table** | **exit 2**, naming the key and what was found | it failed open — the defaults were silently restored and a project's whole `ignore` list vanished with no diagnostic. Same class as an unknown key, one level out |
//! | a key the tool does not know | **exit 2**, naming the key | a key that does nothing looks exactly like a key that works, which is the whole reason `ty` rejects unknown keys too |
//! | a code in `ignore` that no rule answers to | **exit 2**, naming the code | a gate switched off by a typo. Fatal here and merely loud in a marker, because this file belongs to the tool and there is one of it |
//! | an `exclude` entry that is empty, blank, starts with `!`, or is not a glob | **exit 2**, naming the entry | measured against the walker: `""` and `"   "` build a matcher that excludes **nothing**, and `"!vendor"` cancels the exclusion into a no-op. All three look exactly like a rule that works, and the second class silently un-excludes the tree the project meant to put out of scope |
//! | an `exclude` entry starting with `/` | **exit 2**, naming `./x` and `x` | measured against ruff 0.16.0: `/vendor` excludes neither the root nor a nested `vendor` there. Accepting it would ship a private extension that silently disagrees with the tool this one is compared against |
//! | a limit that is not an integer, or is negative | **exit 2**, naming the key and what was found | `docstring-max-volume = "200"` silently falling back to the default is the same defect one type further out |
//! | a limit of `0` | **accepted**: every block of that kind is a finding | `0` is the literal meaning of the key — "no words allowed" — and the core is already fail-closed there. `ignore` is how a rule is switched off; a limit that quietly meant "off" would be the trap |
//! | `ignore` naming every shipping code | **accepted**, and [`crate::cli`] prints a diagnostic | the exit code is honestly 0 — there really are no findings — but a run that measured nothing must not be silent about it |
//!
//! There is deliberately no way to *enable* a rule that `ignore` disabled. See [`crate::rules`] for
//! the marker-versus-`ignore` precedence that follows from it.

use std::path::{Path, PathBuf};

use ignore::overrides::{Override, OverrideBuilder};
use thiserror::Error as ThisError;

use crate::detect::volume::Limits;
use crate::rules::{Rule, shipping_codes};

/// The file the configuration is read from.
///
/// A constant rather than a literal in three places, and `pyproject.toml` rather than a file of our
/// own: a Python project already has one, and a second dotfile per linter is the thing every Python
/// developer complains about.
pub const CONFIG_FILE: &str = "pyproject.toml";

/// The table inside [`CONFIG_FILE`], as a path of keys.
const TABLE_PATH: [&str; 2] = ["tool", "tooprolix"];

/// Every key `[tool.tooprolix]` understands.
///
/// The `match` in [`from_document`] is the real definition and this is the list the error message
/// reads, so the two *are* two places — which is exactly how a known-key list drifts until the
/// newest key is reported as unknown. `every_known_key_is_actually_accepted` walks this array
/// through the parser and closes that by test rather than by hope.
const KNOWN_KEYS: [&str; 4] = [
    "ignore",
    "exclude",
    "comment-max-volume",
    "docstring-max-volume",
];

/// Everything `[tool.tooprolix]` can say.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Config {
    /// The word limits handed to [`crate::detect::volume::volume`].
    pub limits: Limits,
    /// Rules switched off for the whole project, in the order the shipping registry lists them.
    pub ignore: Vec<Rule>,
    /// Gitignore-syntax globs whose matches are never walked, relative to [`Config::source`]'s
    /// own directory.
    ///
    /// Kept as the strings the file wrote, in file order, rather than as a built matcher: the
    /// matcher is not comparable, and `Config` is compared. `exclude_matcher` turns these into
    /// the one the walk uses, and is the *only* thing that does — so the validation [`load`]
    /// performs and the filtering [`crate::cli`] applies can never be built from different rules.
    pub exclude: Vec<String>,
    /// The file these settings came from, or `None` when nothing was found and the defaults apply.
    pub source: Option<PathBuf>,
}

impl Default for Config {
    /// The corpus-measured limits, nothing ignored, nothing excluded, no file.
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            ignore: Vec::new(),
            exclude: Vec::new(),
            source: None,
        }
    }
}

impl Config {
    /// Whether `rule` is switched off for the whole project.
    #[must_use]
    pub fn ignores(&self, rule: Rule) -> bool {
        self.ignore.contains(&rule)
    }

    /// Whether every shipping rule is switched off, so the run cannot produce a finding.
    ///
    /// Read by [`crate::cli`], which has to say so on stderr: an exit 0 that was reached by
    /// measuring nothing is indistinguishable from an exit 0 that was earned.
    #[must_use]
    pub fn ignores_everything(&self) -> bool {
        Rule::ALL.iter().all(|rule| self.ignores(*rule))
    }
}

/// Everything that can go wrong reading a configuration file.
///
/// Every variant names the file, because "unknown key" without a path is unactionable when the file
/// was found by walking upwards from somewhere else.
#[derive(Debug, ThisError)]
#[non_exhaustive]
pub enum Error {
    /// The file was found but could not be read.
    #[error("could not read {}: {source}", path.display())]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },

    /// The file is not valid TOML.
    #[error("could not parse {}: {message}", path.display())]
    Parse {
        /// The file that could not be parsed.
        path: PathBuf,
        /// The parser's own complaint.
        message: String,
    },

    /// `[tool.tooprolix]` holds a key this version does not know.
    #[error(
        "unknown key `{key}` in [tool.tooprolix] of {} (known keys: {known})",
        path.display()
    )]
    UnknownKey {
        /// The file the key was found in.
        path: PathBuf,
        /// The key nobody answers to.
        key: String,
        /// The keys that are understood, comma separated.
        known: String,
    },

    /// A value has the wrong type or an impossible value.
    #[error("`{key}` in [tool.tooprolix] of {}: {problem}", path.display())]
    BadValue {
        /// The file the value was found in.
        path: PathBuf,
        /// The key whose value is wrong.
        key: String,
        /// What is wrong with it.
        problem: String,
    },

    /// `ignore` names a code no rule answers to.
    #[error(
        "`ignore` in [tool.tooprolix] of {} names `{code}`, which is not a tooprolix rule \
         (shipping rules: {known})",
        path.display()
    )]
    UnknownCode {
        /// The file the code was found in.
        path: PathBuf,
        /// The code nobody answers to.
        code: String,
        /// The codes that do exist, comma separated.
        known: String,
    },
}

/// Loads the configuration that applies to `target`, or the defaults if there is none.
///
/// `target` is the path the user asked to check; it does not have to exist, and a path that cannot
/// be canonicalised falls back to the defaults rather than failing — a missing target is
/// [`crate::cli`]'s error to report, and reporting it as a *configuration* failure would name the
/// wrong problem.
///
/// # Errors
///
/// Every variant of [`Error`]: a configuration file that exists and is wrong is fatal, because a
/// setting that silently does nothing is a gate that is silently off.
pub fn load(target: &Path) -> Result<Config, Error> {
    let Some(path) = discover(target) else {
        return Ok(Config::default());
    };

    let text = std::fs::read_to_string(&path).map_err(|source| Error::Read {
        path: path.clone(),
        source,
    })?;
    // `to_string()`, not `message()`. `message()` is the bare complaint; the `Display` impl adds
    // `TOML parse error at line N, column M` and a caret snippet. A tool that tells a user their
    // configuration is broken and then refuses to say where has done half a job — and the test
    // could not see it, because `stderr.contains("parse")` is satisfied by the literal word in the
    // format string below whatever `message` holds.
    let document: toml::Table = text
        .parse()
        .map_err(|error: toml::de::Error| Error::Parse {
            path: path.clone(),
            message: error.to_string(),
        })?;

    from_document(&document, path)
}

/// The nearest [`CONFIG_FILE`] at or above `target`, canonicalising first.
///
/// Returns `None` when `target` cannot be resolved on disk at all, which is deliberately not an
/// error here.
fn discover(target: &Path) -> Option<PathBuf> {
    let resolved = std::fs::canonicalize(target).ok()?;
    // A file's configuration is its directory's configuration; a directory is its own.
    let mut directory: &Path = if resolved.is_dir() {
        &resolved
    } else {
        resolved.parent()?
    };

    loop {
        let candidate = directory.join(CONFIG_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = directory.parent()?;
    }
}

/// Reads `[tool.tooprolix]` out of an already-parsed document.
///
/// Separate from [`load`] so the table can be tested without a filesystem, and so the walk up the
/// tree is tested without a table.
fn from_document(document: &toml::Table, path: PathBuf) -> Result<Config, Error> {
    let mut config = Config {
        source: Some(path.clone()),
        ..Config::default()
    };

    // "Absent" and "present but not a table" are two different answers, and collapsing them is how
    // this failed OPEN: `tooprolix = "disabled"` used to restore the measured defaults in silence,
    // so a project whose table had the wrong shape lost its whole `ignore` list without being told.
    // Absence is the normal case and stays silent; a wrong type is the same class as an unknown
    // key, which is already fatal, one level further out.
    let mut table = document;
    for key in TABLE_PATH {
        match table.get(key) {
            Some(value) => {
                table = value.as_table().ok_or_else(|| Error::BadValue {
                    path: path.clone(),
                    key: key.to_owned(),
                    problem: format!("expected a table, found {}", value.type_str()),
                })?;
            }
            // No `[tool]`, or no `[tool.tooprolix]` inside it: the defaults, and no complaint.
            None => {
                return Ok(Config {
                    source: None,
                    ..config
                });
            }
        }
    }

    for (key, value) in table {
        // The advertised list GATES the dispatch, so the two cannot be separate lists in the
        // direction that used to be untested: an arm added below without a matching entry here is
        // rejected as unknown the first time it is used, instead of quietly working while the
        // `known keys:` line kept telling users it does not exist. The reverse direction — a key
        // advertised that no arm handles — falls through to the same error and is caught by
        // `every_advertised_key_is_actually_accepted`.
        if !KNOWN_KEYS.contains(&key.as_str()) {
            return Err(Error::UnknownKey {
                path,
                key: key.clone(),
                known: KNOWN_KEYS.join(", "),
            });
        }
        match key.as_str() {
            "ignore" => config.ignore = read_ignore(value, &path)?,
            "exclude" => config.exclude = read_exclude(value, &path)?,
            "comment-max-volume" => {
                config.limits.comment_max_volume = read_limit(value, key, &path)?;
            }
            "docstring-max-volume" => {
                config.limits.docstring_max_volume = read_limit(value, key, &path)?;
            }
            _ => {
                return Err(Error::UnknownKey {
                    path,
                    key: key.clone(),
                    known: KNOWN_KEYS.join(", "),
                });
            }
        }
    }

    // The globs are compiled HERE, while the file that wrote them is still in hand, and the result
    // is thrown away. A glob that only fails when a walk happens to reach it would be reported by
    // `crate::cli` as "could not walk <root>" — naming the tree instead of the typo, at a moment
    // when the configuration is no longer on screen. Everything else in this module is fatal at
    // load time for the same reason, so this is the convention and not a new rule.
    exclude_matcher(&config)?;

    Ok(config)
}

/// Reads `ignore = ["TPX003"]`, in registry order rather than in the order the file listed.
///
/// The order is normalised so that two files saying the same thing produce the same [`Config`],
/// which is what lets a test compare configurations rather than compare renderings of them.
fn read_ignore(value: &toml::Value, path: &Path) -> Result<Vec<Rule>, Error> {
    let entries = value.as_array().ok_or_else(|| Error::BadValue {
        path: path.to_path_buf(),
        key: "ignore".to_owned(),
        problem: format!(
            "expected an array of rule codes, found {}",
            value.type_str()
        ),
    })?;

    let mut ignored = Vec::new();
    for entry in entries {
        let code = entry.as_str().ok_or_else(|| Error::BadValue {
            path: path.to_path_buf(),
            key: "ignore".to_owned(),
            problem: format!("expected a rule code string, found {}", entry.type_str()),
        })?;
        let rule = Rule::from_code(code).ok_or_else(|| Error::UnknownCode {
            path: path.to_path_buf(),
            code: code.to_owned(),
            known: shipping_codes(),
        })?;
        ignored.push(rule);
    }

    ignored.sort_unstable();
    ignored.dedup();
    Ok(ignored)
}

/// Reads `exclude = ["tests/fixtures", "vendor"]`, in the order the file wrote them.
///
/// Order is preserved rather than sorted, unlike [`read_ignore`]: gitignore globs are evaluated
/// last-match-wins, so reordering them is not guaranteed to be a no-op the way reordering a set of
/// rule codes is.
///
/// # Every rejection here closes a measured fail-open, not a hypothetical one
///
/// [`exclude_matcher`] spells each entry `!{entry}`, because [`OverrideBuilder`] is a *whitelist*
/// by default — the plain form means "walk only this" — and that inversion is what makes these
/// three inputs dangerous rather than merely useless:
///
/// * `""` and any blank string become the bare `!`, which was measured to exclude the **entire
///   tree** — a silent exit 0 on a repository nobody looked at.
/// * a leading `!` becomes `!!…`, which the walker accepts and which excludes **nothing** — the
///   user's negation cancels ours and the gate is off with no diagnostic.
///
/// Both are the "a guard that can be switched off by a typo in its own configuration" class.
///
/// # Entries are trimmed before they are judged, and that is a decision
///
/// The negation guard originally read position zero of the raw string, so `" !vendor"` — one
/// leading space — walked past it and became the useless `"! !vendor"`. The same space applied to
/// a *valid* glob, `" vendor"`, was equally a silent no-op: it excluded a directory whose name
/// begins with a space, which is to say nothing. Trimming first fixes both, and every check below
/// reads the trimmed value.
///
/// The price is that a path which genuinely begins or ends with a space cannot be named by
/// `exclude`. That is accepted: such a path is pathological, gitignore itself strips trailing
/// whitespace from its patterns, and the alternative — honouring it — means every ordinary typo
/// stays a silent no-op. Refusing padded entries outright was the other candidate and was not
/// taken, because `" vendor"` has exactly one plausible reading and rejecting it would be pedantry
/// where the guard's whole purpose is to stop *silence*, not to police formatting.
///
/// # An entry naming a path that is ABSENT is silent; one naming a present path never is
///
/// The distinction matters and an earlier version of this comment blurred it. A configuration
/// shared across repositories legitimately names paths that are missing from any one of them, so
/// warning about those would be a false alarm most of the time — unlike the near-miss diagnostic
/// on opt-out *markers* ([`crate::rules`]), which measured zero collisions on the corpus. That
/// case stays silent.
///
/// What is **not** allowed to be silent is an entry naming a path that is right there, in a
/// spelling the matcher does not recognise. `./broken` was exactly that, and [`normalise_glob`]
/// now gives it the anchored form; anything that provably cannot match is refused outright.
///
/// # Two residuals, recorded so they are re-examined rather than rediscovered
///
/// * **Unicode normalisation.** A directory created with NFD bytes and an entry written in NFC do
///   not match, so the exclusion misses and the run exits 2. Left alone: it fails loud rather than
///   silently, it is macOS-specific, and ruff matches byte-wise too.
/// * **An explicitly named directory can be emptied by a child-matching glob.**
///   `exclude = ["vendor/**"]` with `tooprolix check vendor` keeps the root — depth 0 is exempt
///   from filtering — but removes every child, giving an empty walk. Matches ruff, and it is loud.
fn read_exclude(value: &toml::Value, path: &Path) -> Result<Vec<String>, Error> {
    let entries = value.as_array().ok_or_else(|| Error::BadValue {
        path: path.to_path_buf(),
        key: "exclude".to_owned(),
        problem: format!("expected an array of globs, found {}", value.type_str()),
    })?;

    let mut excluded = Vec::with_capacity(entries.len());
    for entry in entries {
        let raw = entry.as_str().ok_or_else(|| Error::BadValue {
            path: path.to_path_buf(),
            key: "exclude".to_owned(),
            problem: format!("expected a glob string, found {}", entry.type_str()),
        })?;

        // NORMALISE FIRST, then judge the NORMALISED form, then keep it. The order is the whole
        // guard, and the previous revision only claimed it: it judged `raw.trim()` and normalised
        // afterwards, so every check read a string the walk would never see.
        //
        // Whitespace was never the only thing that could sit in front of the `!`. A `./` hop is a
        // path component to nobody and is stripped by `normalise_glob`, so `./!vendor` walked past
        // a guard reading position zero and reached `exclude_matcher` as the bare `!vendor` it
        // exists to refuse — which that function then spells `!!vendor`.
        //
        // What `!!vendor` actually means is worth stating exactly, because the obvious reading is
        // wrong. It is NOT a double negation that cancels out. `ignore-0.4.31`'s
        // `Gitignore::add_line` strips a leading `!` with an `if`, not a `while`: the first `!` is
        // consumed as the whitelist marker and the remainder, `!vendor`, stays as a **literal**
        // pattern. So the entry becomes "exclude a file whose name is literally `!vendor`" — a
        // rule that is well-formed, applied, and matches nothing any real tree contains.
        //
        // The observable outcome is the same either way, which is why the wrong description
        // survived a round: measured on the built binary over a tree with one excluded finding,
        // `"!vendor"` gave exit 2, and `"./!vendor"` and `".//!vendor"` gave **exit 1 with the
        // finding still reported**. The config looked applied and did nothing, which is precisely
        // the silent no-op the message below names.
        //
        // Comparing before normalising is a recorded defect class of this project, not a
        // hypothesis; it is the same one `crate::cli` and `discover` both canonicalise against.
        let glob = raw.trim();

        if glob.is_empty() {
            return Err(Error::BadValue {
                path: path.to_path_buf(),
                key: "exclude".to_owned(),
                problem: "an empty glob would exclude the whole tree; remove the entry instead"
                    .to_owned(),
            });
        }
        let normalised = normalise_glob(glob).map_err(|problem| Error::BadValue {
            path: path.to_path_buf(),
            key: "exclude".to_owned(),
            problem: format!("`{raw}` {problem}"),
        })?;
        // Read past the anchor `normalise_glob` may have prepended, not byte zero. Measured: once
        // `./x` started emitting `/x`, a byte-zero guard let `./!vendor` through as `/!vendor` —
        // the third time this project has read position zero of the wrong representation.
        if normalised.trim_start_matches('/').starts_with('!') {
            return Err(Error::BadValue {
                path: path.to_path_buf(),
                key: "exclude".to_owned(),
                problem: format!(
                    "`{raw}` starts with `!`, which would negate the exclusion into a no-op; \
                     `exclude` entries are exclusions already, and re-inclusion is not supported"
                ),
            });
        }
        // The same class as the two guards above, one separator further out. `normalise_glob`
        // splits on `/` and nothing else, but `OverrideBuilder` reads `\` as an ESCAPE — so
        // `vendor\**` is not a two-component path, it is the single literal name `vendor**`, which
        // matches nothing. Measured on the built binary before this guard, on a tree with one
        // excluded finding: exit 1 with the finding still reported. On Windows `vendor\**` is the
        // natural way to write `vendor/**`, so this is the spelling most likely to be typed by
        // someone whose tree it then silently fails to exclude.
        //
        // REFUSED, not rewritten into `/`. A backslash is a legal filename character on POSIX, so
        // rewriting guesses intent, and it would break the one backslash spelling that is correct
        // today: `\!vendor` escapes the leading `!` to name a file literally called `!vendor`.
        // Refusing the class costs that spelling — deliberately, and the message says so — because
        // a guard that silently changes what a glob means is the failure this whole function
        // exists to prevent. Fail loud beats guess right.
        if normalised.contains('\\') {
            return Err(Error::BadValue {
                path: path.to_path_buf(),
                key: "exclude".to_owned(),
                problem: format!(
                    "`{raw}` contains a backslash, which is not a path separator here: globs are \
                     matched with `/` on every platform, and `\\` is read as an escape, so this \
                     entry would silently exclude nothing. Write `/` instead. (This also refuses \
                     `\\!name`, the escape for a file literally called `!name`; name it with a \
                     glob that does not lead with the escape.)"
                ),
            });
        }
        excluded.push(normalised);
    }

    Ok(excluded)
}

/// Turns an `exclude` entry into the one glob that says what it means, or refuses it.
///
/// Separator noise carries no meaning and collapses (`.//x`, `././x`, `x/./y`, `x//y`); the depth an
/// entry names does carry meaning and survives. Left raw, `./broken` is a glob for a directory
/// literally called `.` and matches nothing at all.
///
/// # What each spelling means, measured against ruff 0.16.0
///
/// * `x` matches **at any depth**, and stays a bare name.
/// * `x/` and `./x` select **only the entry beside the configuration file**, and are emitted with a
///   leading `/`. That is not cosmetic: gitignore anchors a pattern whose `/` is not merely
///   trailing, so the leading slash is what turns `vendor/` from "every `vendor` at any depth" into
///   "the root one". A trailing `/` is still kept on top of it, because it means "directory only".
/// * `/x` is **refused**. It reads as an anchor and ruff excludes neither depth for it, so
///   accepting it would ship a private extension that silently disagrees with ruff.
/// * A `..` component is refused: it walks out of the tree the base names, so nothing the walk can
///   yield will match it — measured as a no-op.
/// * An entry that normalises away to nothing (`.`, `./`, `/`) is refused. This one is load-bearing:
///   it would otherwise reach [`exclude_matcher`] as the bare `!`, measured to exclude the **entire
///   tree**.
fn normalise_glob(glob: &str) -> Result<String, String> {
    // Anchoring is decided on the raw spelling, and only here, so there is one representation to
    // reason about: a `.` hop before any real component, or a trailing `/`.
    let mut leading_dot_hop = false;
    let mut segments = Vec::new();
    for segment in glob.split('/') {
        match segment {
            // A separator run or a `.` hop: no path component, so nothing to keep.
            "" => {}
            "." => leading_dot_hop |= segments.is_empty(),
            ".." => {
                return Err(
                    "escapes the directory of the configuration file with `..`, so no path this walk can reach will ever match it".to_owned(),
                );
            }
            component => segments.push(component),
        }
    }

    if segments.is_empty() {
        return Err("names no path at all; an empty glob would exclude the whole tree".to_owned());
    }
    // Refused rather than mapped onto `./x`: a leading `/` looks like the root-anchored form and
    // ruff matches nothing at all for it, so guessing would be the silent disagreement this whole
    // function exists to prevent.
    if glob.starts_with('/') {
        let name = segments.join("/");
        return Err(format!(
            "is anchored with a leading `/`, which ruff does not honour and this tool does not \
             accept; write `./{name}` for the one beside this file, or `{name}` to match it at any \
             depth"
        ));
    }

    let mut normalised = String::with_capacity(glob.len() + 1);
    if leading_dot_hop || glob.ends_with('/') {
        normalised.push('/');
    }
    normalised.push_str(&segments.join("/"));
    if glob.ends_with('/') {
        normalised.push('/');
    }
    Ok(normalised)
}

/// The walk filter for [`Config::exclude`], with the configuration file's directory as its base.
///
/// **The base is the configuration file's own directory** — ruff's rule — and not the working
/// directory and not the walk root. One project, one file, one meaning for `vendor/`, whether CI
/// runs `tooprolix check .` at the root or `tooprolix check .` inside a package. [`crate::cli`] is
/// what makes that reachable, by matching against paths rooted at the same canonical tree.
///
/// Returns a matcher that is [`Override::is_empty`] when nothing is excluded, which is the signal
/// [`crate::cli`] uses to keep the untouched walk untouched.
///
/// # Errors
///
/// [`Error::BadValue`], naming the entry and the file, for a glob the walker cannot compile.
pub(crate) fn exclude_matcher(config: &Config) -> Result<Override, Error> {
    if config.exclude.is_empty() {
        return Ok(Override::empty());
    }
    // `exclude` is non-empty, so it came from a file and `source` is `Some`; the fallback keeps
    // this total rather than asserting that, since a panic here would be a worse answer than a
    // matcher based at the working directory.
    let source = config.source.as_deref().unwrap_or(Path::new(CONFIG_FILE));
    let base = source.parent().unwrap_or(Path::new("."));

    let mut builder = OverrideBuilder::new(base);
    for glob in &config.exclude {
        // The `!` is load-bearing and inverted from the intuition: `OverrideBuilder` is ripgrep's
        // `--include`, so a bare `vendor` means "walk ONLY vendor" — measured, and it produced an
        // empty walk on a tree full of Python. `!vendor` is the exclusion, and it leaves every
        // non-matching path at `Match::None`, which is what lets the `.gitignore` layer underneath
        // still have its say instead of being replaced.
        builder
            .add(&format!("!{glob}"))
            .map_err(|error| Error::BadValue {
                path: source.to_path_buf(),
                key: "exclude".to_owned(),
                problem: format!("`{glob}` is not a valid glob: {error}"),
            })?;
    }

    builder.build().map_err(|error| Error::BadValue {
        path: source.to_path_buf(),
        key: "exclude".to_owned(),
        problem: error.to_string(),
    })
}

/// Reads one of the two `*-max-volume` keys, **in words**.
///
/// `0` is accepted and means what it says: no prose of that kind may carry any words at all, so
/// every block of that kind is a finding. Negative and non-integer values are errors — a limit that
/// fell back to the default because it was quoted would be a threshold nobody could see was not
/// applied.
fn read_limit(value: &toml::Value, key: &str, path: &Path) -> Result<usize, Error> {
    let number = value.as_integer().ok_or_else(|| Error::BadValue {
        path: path.to_path_buf(),
        key: key.to_owned(),
        problem: format!(
            "expected a whole number of words, found {}",
            value.type_str()
        ),
    })?;

    usize::try_from(number).map_err(|_| Error::BadValue {
        path: path.to_path_buf(),
        key: key.to_owned(),
        problem: format!("expected a whole number of words, found {number}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::volume::{DEFAULT_COMMENT_MAX_VOLUME, DEFAULT_DOCSTRING_MAX_VOLUME};

    fn parse(toml_text: &str) -> Result<Config, Error> {
        let document: toml::Table = toml_text.parse().expect("the fixture is valid TOML");
        from_document(&document, PathBuf::from("pyproject.toml"))
    }

    /// The whole point of the defaults: a project that says nothing gets exactly the calibrated
    /// numbers, and this is what would break silently if a key were read into the wrong field.
    #[test]
    fn a_project_with_no_table_gets_the_measured_defaults() {
        for text in [
            "",
            "[project]\nname = \"x\"\n",
            "[tool.ruff]\nline-length = 120\n",
        ] {
            let config = parse(text).expect("no table is not an error");

            assert_eq!(config.limits, Limits::default());
            assert_eq!(config.limits.comment_max_volume, DEFAULT_COMMENT_MAX_VOLUME);
            assert_eq!(
                config.limits.docstring_max_volume,
                DEFAULT_DOCSTRING_MAX_VOLUME
            );
            assert!(config.ignore.is_empty());
            assert_eq!(config.source, None, "no table means no configuration file");
        }
    }

    /// A `[tool.tooprolix]` that is present but is **not a table** is an error, not "no config".
    ///
    /// This failed OPEN: the lookup was `get(key).and_then(Value::as_table)`, so a wrong type was
    /// indistinguishable from an absent key and the measured defaults were silently restored. A
    /// user whose table had the wrong shape lost their entire `ignore` list and was told nothing —
    /// the same class as an unknown key, which this module already calls fatal, one level out.
    #[test]
    fn a_table_of_the_wrong_type_is_an_error_and_not_a_silent_default() {
        for (text, what) in [
            (
                "[tool]\ntooprolix = \"disabled\"\n",
                "a string where the table goes",
            ),
            ("[tool]\ntooprolix = []\n", "an array where the table goes"),
            ("tool = \"everything\"\n", "a string where [tool] goes"),
            ("tool = 3\n", "a number where [tool] goes"),
        ] {
            let Err(error) = parse(text) else {
                panic!("{what} was read as no configuration at all");
            };
            let rendered = error.to_string();

            assert!(
                rendered.contains("tooprolix") || rendered.contains("tool"),
                "{what}: the message names neither key: {rendered}"
            );
            assert!(
                rendered.contains("pyproject.toml"),
                "{what}: the message does not name the file: {rendered}"
            );
        }
    }

    /// Each key reaches its own field. A test that set both limits to the same number could not see
    /// them swapped, which is why they differ here and differ from the defaults.
    #[test]
    fn each_key_reaches_its_own_field() {
        let config = parse(
            "[tool.tooprolix]\n\
             ignore = [\"TPX003\"]\n\
             comment-max-volume = 40\n\
             docstring-max-volume = 90\n",
        )
        .expect("the fixture is a valid configuration");

        assert_eq!(config.limits.comment_max_volume, 40);
        assert_eq!(config.limits.docstring_max_volume, 90);
        assert_eq!(config.ignore, vec![Rule::DuplicateProse]);
        assert!(config.ignores(Rule::DuplicateProse));
        assert!(!config.ignores(Rule::CommentVolume));
        assert!(!config.ignores_everything());
    }

    /// A configuration that switches every rule off is legal and must be *visible* — the caller
    /// asks this question in order to say so on stderr.
    #[test]
    fn ignoring_every_code_is_legal_and_detectable() {
        let config = parse("[tool.tooprolix]\nignore = [\"TPX001\", \"TPX002\", \"TPX003\"]\n")
            .expect("ignoring everything is a legal configuration");

        assert!(config.ignores_everything());
        for rule in Rule::ALL {
            assert!(config.ignores(rule));
        }
    }

    /// A limit of zero is the literal meaning of the key, not a way of switching the rule off.
    #[test]
    fn a_zero_limit_is_accepted_and_means_no_words_are_allowed() {
        let config = parse("[tool.tooprolix]\ncomment-max-volume = 0\n").expect("zero is legal");

        assert_eq!(config.limits.comment_max_volume, 0);
        assert!(
            !config.ignores(Rule::CommentVolume),
            "zero must not be a back door to disabling the rule"
        );
    }

    /// Every key this module advertises as known is a key it actually accepts.
    ///
    /// [`KNOWN_KEYS`] feeds the "unknown key" message while the `match` in [`from_document`] is
    /// what really decides, so they are two lists that must agree. The failure mode is not
    /// theoretical and it is exactly backwards from the obvious one: a key added to the `match`
    /// but not to [`KNOWN_KEYS`] leaves users a message that omits it, and a key added to
    /// [`KNOWN_KEYS`] but not to the `match` is *advertised and then rejected*. This walks the
    /// advertised list through the parser, so the second cannot ship.
    #[test]
    fn every_advertised_key_is_actually_accepted() {
        for key in KNOWN_KEYS {
            // A value of the right type for each, so only the KEY is under test here.
            let value = match key {
                "ignore" => "[\"TPX003\"]",
                "exclude" => "[\"vendor\"]",
                _ => "10",
            };

            let result = parse(&format!("[tool.tooprolix]\n{key} = {value}\n"));

            assert!(
                result.is_ok(),
                "`{key}` is advertised as a known key and then rejected: {:?}",
                result.err().map(|error| error.to_string())
            );
        }
    }

    /// `exclude` is read verbatim and in file order, and nothing else moves.
    ///
    /// Order is asserted because gitignore globs are last-match-wins, so a normalising sort would
    /// be a silent change of meaning rather than the harmless one it is for `ignore`.
    #[test]
    fn exclude_is_read_in_file_order_and_touches_no_other_field() {
        let config = parse(
            "[tool.tooprolix]\nexclude = [\"vendor\", \"tests/fixtures\", \"*.generated.py\"]\n",
        )
        .expect("the fixture is a valid configuration");

        assert_eq!(
            config.exclude,
            vec!["vendor", "tests/fixtures", "*.generated.py"]
        );
        // Stored trimmed, so the walk matches the glob the user meant rather than one behind a
        // space — which matched nothing and said nothing.
        assert_eq!(
            parse("[tool.tooprolix]\nexclude = [\"  vendor  \"]\n")
                .expect("padding is not an error")
                .exclude,
            vec!["vendor"]
        );
        assert_eq!(config.limits, Limits::default());
        assert!(config.ignore.is_empty());
        assert!(
            !config.ignores_everything(),
            "excluding paths must not read as disabling rules"
        );
    }

    /// Separator noise collapses; the spellings that carry a DEPTH become the anchored form.
    ///
    /// The exact emitted string is what no end-to-end test can see: `./broken` and `broken/` both
    /// mean the root one, and the only way to say that to gitignore is a leading `/`. Expected
    /// strings are written out rather than derived, so a normaliser that mangled every case
    /// identically could not pass.
    #[test]
    fn normalisation_collapses_redundant_separators_and_anchors_the_root_only_spellings() {
        for (written, expected) in [
            // A bare name, at any depth, and unchanged.
            ("broken", "broken"),
            ("crates/*/resources", "crates/*/resources"),
            ("a/./b", "a/b"),
            ("a//b", "a/b"),
            // A `./` hop means the root one, however many separators it is written with.
            ("./broken", "/broken"),
            (".//broken", "/broken"),
            ("././broken", "/broken"),
            // A trailing `/` means the root one AND directory-only, so both slashes survive.
            ("broken/", "/broken/"),
            ("./broken/", "/broken/"),
        ] {
            let config = parse(&format!("[tool.tooprolix]\nexclude = [\"{written}\"]\n"))
                .unwrap_or_else(|error| panic!("`{written}` was rejected: {error}"));

            assert_eq!(
                config.exclude,
                vec![expected.to_owned()],
                "`{written}` did not normalise to `{expected}`"
            );
        }
    }

    /// The entries that the walker accepts and then silently does nothing useful with.
    ///
    /// Each of these compiles cleanly one layer down — that is the whole problem. `""` and a blank
    /// string become the bare `!` and were measured to exclude the **entire tree**; a leading `!`
    /// becomes `!!…` and excludes **nothing**. A configuration that reads as a working gate and is
    /// not one is the defect this project keeps closing, so all three are refused at the door.
    #[test]
    fn an_exclude_entry_that_would_silently_do_the_wrong_thing_is_refused() {
        for (text, expected, what) in [
            (
                "[tool.tooprolix]\nexclude = [\"\"]\n",
                "empty",
                "an empty glob, which excludes everything",
            ),
            (
                "[tool.tooprolix]\nexclude = [\"   \"]\n",
                "empty",
                "a blank glob, which excludes everything",
            ),
            (
                "[tool.tooprolix]\nexclude = [\"!vendor\"]\n",
                "!",
                "a negated glob, which excludes nothing",
            ),
            // The guard used to read byte zero of the raw string, so one leading space walked past
            // it into the same silent no-op it exists to prevent.
            (
                "[tool.tooprolix]\nexclude = [\" !vendor\"]\n",
                "!",
                "a negated glob behind a leading space",
            ),
            (
                "[tool.tooprolix]\nexclude = [\"\\t!vendor\"]\n",
                "!",
                "a negated glob behind a tab",
            ),
            // The same defect one normalisation further out, and the reason the guard moved below
            // `normalise_glob`. Whitespace was not the only thing that could sit in front of the
            // `!`: a `./` hop is stripped by normalisation, so the entry reaches the matcher as the
            // bare `!vendor` this guard exists to refuse. Measured on the built binary before the
            // fix, on a tree of one excluded finding: `"!vendor"` was exit 2, and `"./!vendor"` was
            // **exit 1 with the finding still reported** — the config silently excluded nothing,
            // which is precisely the no-op the message below describes.
            (
                "[tool.tooprolix]\nexclude = [\"./!vendor\"]\n",
                "!",
                "a negated glob behind a `./` hop",
            ),
            (
                "[tool.tooprolix]\nexclude = [\".//!vendor\"]\n",
                "!",
                "a negated glob behind a separator run",
            ),
            // The SECOND compare-before-normalise instance, and the reason `\` is refused outright
            // rather than rewritten. `normalise_glob` splits on `/` and nothing else, while the
            // `ignore` crate's `OverrideBuilder` reads `\` as an ESCAPE — so a Windows-shaped
            // `vendor\**` is not a path with two components, it is the single literal name `vendor**`
            // that matches nothing. Measured on the built binary before this guard: exit 1 with the
            // finding still reported, i.e. the tree the user excluded was measured anyway.
            //
            // Rewriting `\` into `/` was rejected: on POSIX a backslash is a legal filename
            // character, so the rewrite would guess intent and break the one spelling that is
            // currently CORRECT — `\!vendor`, which escapes a leading `!` to name a file literally
            // called `!vendor`. Refusing the whole class costs that spelling, and that is the
            // intended consequence rather than a regression: the message says so, and a file named
            // `!vendor` can still be reached by a glob that does not lead with the escape.
            //
            // Note both of these are TOML *literal* strings (single quotes). In a basic string
            // `"vendor\**"` is a TOML parse error, which is exactly what hid this for so long.
            (
                "[tool.tooprolix]\nexclude = ['vendor\\**']\n",
                "backslash",
                "a Windows-shaped separator, which the matcher reads as an escape",
            ),
            (
                "[tool.tooprolix]\nexclude = ['\\!vendor']\n",
                "backslash",
                "an escaped `!`, refused with the rest of the backslash class",
            ),
            (
                "[tool.tooprolix]\nexclude = [\"a[\"]\n",
                "a[",
                "a glob that does not compile",
            ),
            (
                "[tool.tooprolix]\nexclude = \"vendor\"\n",
                "string",
                "a bare string instead of an array",
            ),
            (
                "[tool.tooprolix]\nexclude = [3]\n",
                "integer",
                "a non-string entry",
            ),
        ] {
            let error = parse(text).expect_err(&format!("{what} was accepted"));
            let rendered = error.to_string();

            assert!(
                rendered.contains(expected),
                "{what}: the message does not name `{expected}`: {rendered}"
            );
            assert!(
                rendered.contains("exclude") && rendered.contains("pyproject.toml"),
                "{what}: the message names neither the key nor the file: {rendered}"
            );
        }
    }

    /// Every way of getting the table wrong is fatal, and each error names what was wrong.
    ///
    /// Without this each of these would fall back to a default: a threshold that is not applied and
    /// cannot be seen not to be applied.
    #[test]
    fn every_malformed_setting_is_an_error_that_names_itself() {
        let cases = [
            (
                "[tool.tooprolix]\nignore-me = [\"TPX001\"]\n",
                "ignore-me",
                "an unknown key",
            ),
            (
                "[tool.tooprolix]\nignore = [\"TPX999\"]\n",
                "TPX999",
                "an unknown code",
            ),
            (
                "[tool.tooprolix]\nignore = [\"TPX004\"]\n",
                "TPX004",
                "a reserved code with no rule behind it",
            ),
            (
                "[tool.tooprolix]\ndocstring-max-volume = -1\n",
                "-1",
                "a negative limit",
            ),
            (
                "[tool.tooprolix]\ndocstring-max-volume = \"200\"\n",
                "string",
                "a quoted limit",
            ),
            (
                "[tool.tooprolix]\ndocstring-max-volume = 1.5\n",
                "float",
                "a fractional limit",
            ),
            (
                "[tool.tooprolix]\nignore = \"TPX001\"\n",
                "string",
                "a bare string ignore",
            ),
            (
                "[tool.tooprolix]\nignore = [1]\n",
                "integer",
                "a non-string code",
            ),
        ];

        for (text, expected_detail, what) in cases {
            let error = parse(text).expect_err(&format!("{what} was accepted"));
            let rendered = error.to_string();

            assert!(
                rendered.contains(expected_detail),
                "{what}: the message does not name `{expected_detail}`: {rendered}"
            );
            assert!(
                rendered.contains("pyproject.toml"),
                "{what}: the message does not name the file: {rendered}"
            );
        }
    }
}
