//! `[tool.tooprolix]` in `pyproject.toml`: three keys, and every way of getting them wrong.
//!
//! ```toml
//! [tool.tooprolix]
//! ignore = ["TPX003"]
//! comment-max-volume = 150
//! docstring-max-volume = 200
//! ```
//!
//! That is the whole surface. `exclude`, `select`/`extend-select`, per-file settings and any other
//! per-repository calibration are a second epic; the scope guard is lifted for these three keys and
//! for nothing else. Without a configuration file the behaviour is exactly
//! [`Limits::default`] with nothing ignored, and that is pinned by a test rather than left to
//! coincidence.
//!
//! # Where the file is looked for, and the answer is one answer
//!
//! **The nearest `pyproject.toml` at or above the checked path**, searching upwards to the
//! filesystem root, first match wins. Not "the current directory": with a cwd rule,
//! `tooprolix check src/api.py` and `cd src && tooprolix check api.py` would apply different
//! limits to the same file, and a CI job that changed directory would silently change the
//! thresholds it was enforcing.
//!
//! ⚠️ **The path is canonicalised before anything is compared or walked.** `..` components and
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
//! | a limit that is not an integer, or is negative | **exit 2**, naming the key and what was found | `docstring-max-volume = "200"` silently falling back to the default is the same defect one type further out |
//! | a limit of `0` | **accepted**: every block of that kind is a finding | `0` is the literal meaning of the key — "no words allowed" — and the core is already fail-closed there. `ignore` is how a rule is switched off; a limit that quietly meant "off" would be the trap |
//! | `ignore` naming every shipping code | **accepted**, and [`crate::cli`] prints a diagnostic | the exit code is honestly 0 — there really are no findings — but a run that measured nothing must not be silent about it |
//!
//! There is deliberately no way to *enable* a rule that `ignore` disabled. See [`crate::rules`] for
//! the marker-versus-`ignore` precedence that follows from it.

use std::path::{Path, PathBuf};

use thiserror::Error as ThisError;

use crate::detect::volume::Limits;
use crate::rules::Rule;

/// The file the configuration is read from.
///
/// A constant rather than a literal in three places, and `pyproject.toml` rather than a file of our
/// own: a Python project already has one, and a second dotfile per linter is the thing every Python
/// developer complains about.
pub const CONFIG_FILE: &str = "pyproject.toml";

/// The table inside [`CONFIG_FILE`], as a path of keys.
const TABLE_PATH: [&str; 2] = ["tool", "tooprolix"];

/// Everything `[tool.tooprolix]` can say.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Config {
    /// The word limits handed to [`crate::detect::volume::volume`].
    pub limits: Limits,
    /// Rules switched off for the whole project, in the order the shipping registry lists them.
    pub ignore: Vec<Rule>,
    /// The file these settings came from, or `None` when nothing was found and the defaults apply.
    pub source: Option<PathBuf>,
}

impl Default for Config {
    /// The corpus-measured limits, nothing ignored, no file.
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            ignore: Vec::new(),
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
        match key.as_str() {
            "ignore" => config.ignore = read_ignore(value, &path)?,
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
                    known: "ignore, comment-max-volume, docstring-max-volume".to_owned(),
                });
            }
        }
    }

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

/// Every shipping code, comma separated, for an error message.
fn shipping_codes() -> String {
    Rule::ALL
        .iter()
        .map(|rule| rule.code())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{Config, Error, from_document};
    use crate::detect::volume::{DEFAULT_COMMENT_MAX_VOLUME, DEFAULT_DOCSTRING_MAX_VOLUME, Limits};
    use crate::rules::Rule;
    use std::path::PathBuf;

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
