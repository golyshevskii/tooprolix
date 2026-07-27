//! The opt-out marker, put in front of the tool it has to share a comment syntax with.
//!
//! This file is a measurement kept executable. The epic's own default grammar for 0.2.0 was
//! `# noqa TPX00N`, and it was **killed by running ruff over it** rather than by discussion:
//!
//! | grammar | ruff codes | survives `ruff check --fix` |
//! |---|---|---|
//! | `# !TPX002` | — | yes |
//! | `# noqa TPX002` | RUF100 | **no — the whole line is deleted** |
//! | `# noqa: TPX002` | RUF102 | **no — the whole line is deleted** |
//!
//! `pyproject.toml` enables RUF100 in this very repository, so the rejected spelling would have had
//! our own lint job delete our own markers. The collision is closed *by construction* — a grammar
//! without the word `noqa` is invisible to ruff and flake8 — and the two assertions below are what
//! stop that from being re-opened by a "harmless" grammar tweak.
//!
//! ruff is invoked `--isolated` with an explicit `--select`, so the result is a property of the
//! grammar and not of this repository's configuration.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The two unused imports are the neighbouring violations: whatever the marker line does, ruff must
/// still catch them, on the line before it and on the line after.
fn fixture(marker: &str) -> String {
    format!(
        "import os\n\
         {marker}\n\
         import sys\n\
         # A comment run that earns its length and is what the marker is there for.\n"
    )
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Writes `source` into a scratch file of its own and returns its path.
fn scratch(name: &str, source: &str) -> PathBuf {
    let directory = std::fs::canonicalize(std::env::temp_dir())
        .expect("the system temporary directory exists")
        .join(format!("tooprolix-ruff-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a scratch directory is creatable");
    let path = directory.join(name);
    std::fs::write(&path, source).expect("a scratch file is writable");
    path
}

/// `uv run ruff`, the same ruff `make lint.check` uses, on one file and with one explicit rule set.
fn ruff(arguments: &[&str], path: &Path) -> Output {
    Command::new("uv")
        .args(["run", "--only-group", "lint", "ruff", "check", "--isolated"])
        .args([
            "--select",
            "F401,RUF100,RUF102",
            "--output-format",
            "concise",
        ])
        .args(arguments)
        .arg(path)
        .current_dir(repository_root())
        .output()
        .expect("uv run ruff is available in this environment")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("ruff writes UTF-8")
}

/// AC3 — our marker is invisible to ruff, and ruff's own fixer leaves it alone.
///
/// The rejected spelling is asserted beside it on purpose. "No RUF100 was reported" passes on a
/// ruff invocation that reported nothing at all — an `--isolated` run with the wrong `--select`, a
/// path that resolved to nothing, a ruff that failed to start — so the same command has to be shown
/// producing RUF100 for the grammar that earns it.
#[test]
fn the_marker_fires_no_ruff_rule_and_survives_ruff_check_fix() {
    // Arrange
    let ours = scratch("marker_ours.py", &fixture("# !TPX002"));
    let rejected = scratch("marker_noqa.py", &fixture("# noqa TPX002"));

    // Act
    let ours_report = stdout_of(&ruff(&[], &ours));
    let rejected_report = stdout_of(&ruff(&[], &rejected));

    // Assert — the neighbouring violations are still caught, on both sides of the marker line.
    assert!(
        ours_report.contains("marker_ours.py:1:8: F401")
            && ours_report.contains("marker_ours.py:3:8: F401"),
        "the marker silenced ruff on a neighbouring line: {ours_report}"
    );
    // ... and the marker line itself is not a ruff finding of any kind.
    assert!(
        !ours_report.contains("RUF100") && !ours_report.contains("RUF102"),
        "the marker fired a ruff rule: {ours_report}"
    );
    assert_eq!(
        ours_report
            .lines()
            .filter(|line| line.contains(": F401"))
            .count(),
        2,
        "expected exactly the two unused imports: {ours_report}"
    );

    // The control: the same run, on the grammar this one replaced, does fire RUF100.
    assert!(
        rejected_report.contains("RUF100"),
        "the rejected `# noqa` grammar stopped firing RUF100 — this test can no longer tell the \
         two apart: {rejected_report}"
    );

    // Act — the fixer, which is the half that actually deletes a line.
    ruff(&["--fix"], &ours);
    ruff(&["--fix"], &rejected);

    // Assert
    let ours_after = std::fs::read_to_string(&ours).expect("the fixture survived");
    let rejected_after = std::fs::read_to_string(&rejected).expect("the fixture survived");
    assert!(
        ours_after.contains("# !TPX002"),
        "`ruff check --fix` deleted our own opt-out marker: {ours_after:?}"
    );
    assert!(
        !rejected_after.contains("# noqa TPX002"),
        "`ruff check --fix` no longer deletes the rejected grammar's marker line, so the reason \
         that grammar was rejected is no longer being measured: {rejected_after:?}"
    );
}
