//! The `tooprolix` command line, exercised as a process.
//!
//! Everything here spawns the real binary and reads the real bytes, because the contract this
//! task ships is a *process* contract — an exit code, a stream split, and an ordered stdout — and
//! none of those three can be observed from inside the library.
//!
//! `env!("CARGO_BIN_EXE_tooprolix")` rather than `assert_cmd`. The task file names `assert_cmd`,
//! and this is a deliberate, recorded divergence: cargo already exports the absolute path of every
//! binary target to its integration tests, so the crate would buy nothing here except six more
//! entries in a `Cargo.lock` that `--locked` makes load-bearing.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Runs the CLI from the repository root, so every path in the output is repository-relative.
fn tooprolix(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tooprolix"))
        .args(arguments)
        .current_dir(repository_root())
        .output()
        .expect("the binary cargo just built is executable")
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn stdout_of(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("the CLI writes UTF-8")
}

/// Everything a run that read the whole tree and found nothing writes to stdout.
///
/// Until 0.3.0 this was `""`, and a dozen assertions here spelled "nothing was found" as an empty
/// string. Naming it is not tidying: an empty stdout is also what a crash, a walk that visited
/// nothing and a killed process produce, so those assertions were passing on three outcomes and
/// meaning one. They now pin the sentence, which only a successful complete run can print.
///
/// Colourless because `Command::output` hands the child a pipe — see
/// `a_clean_full_run_says_so_and_a_pipe_receives_no_escape_codes`, which is where that half of the
/// rule is asserted rather than assumed.
const CLEAN_STDOUT: &str = "All checks passed!\n";

fn stderr_of(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("the CLI writes UTF-8")
}

/// The exit contract, all three codes, on fixtures that are each capable of the other two answers —
/// which is what stops this from being three tests that all pass on a CLI that always exits 0.
///
/// The `broken` tree is the row that changed: it used to be the exit-2 case and is now the partial
/// one, so exit 2 is proved on the only thing left that produces it — a run that cannot start at
/// all. Keeping `broken` here, in the same test, is what makes the narrowing visible rather than
/// merely asserted somewhere else.
#[test]
fn the_exit_code_says_which_of_the_three_outcomes_happened() {
    // Arrange — the fixtures exist, and a missing one must fail here rather than silently make
    // "no findings" true for the wrong reason.
    for fixture in ["clean", "broken", "dup-corpus"] {
        let path = repository_root().join("tests/fixtures").join(fixture);
        assert!(path.is_dir(), "fixture tree is missing: {}", path.display());
    }

    // Act
    let clean = tooprolix(&["check", "tests/fixtures/clean"]);
    let findings = tooprolix(&["check", "tests/fixtures/dup-corpus"]);
    let broken = tooprolix(&["check", "tests/fixtures/broken"]);
    let cannot_start = tooprolix(&["check", "tests/fixtures/does-not-exist"]);

    // Assert
    assert_eq!(clean.status.code(), Some(0), "clean: {clean:?}");
    assert_eq!(
        stdout_of(&clean),
        CLEAN_STDOUT,
        "a clean tree must SAY it is clean; silence is what a crash looks like"
    );

    assert_eq!(findings.status.code(), Some(1), "findings: {findings:?}");
    assert!(
        !stdout_of(&findings).is_empty(),
        "exit 1 with an empty stdout is a finding nobody can act on"
    );

    // A tree holding one unparsable file is measured as far as it goes and is never 0 ...
    assert_eq!(broken.status.code(), Some(1), "broken: {broken:?}");
    assert!(
        stdout_of(&broken).contains("TPX002"),
        "the finding of the file that parsed is still being withheld: {:?}",
        stdout_of(&broken)
    );
    assert!(
        stderr_of(&broken).contains("syntax_error.py"),
        "the reason must name the file: {:?}",
        stderr_of(&broken)
    );

    // ... and 2 is left meaning only that the run never got started.
    assert_eq!(
        cannot_start.status.code(),
        Some(2),
        "cannot start: {cannot_start:?}"
    );
    assert_eq!(stdout_of(&cannot_start), "");
}

/// The broken tree holds a file that *would* be a finding, and the tool must now report it.
///
/// **This test used to assert the exact opposite**, under the name
/// `a_parse_failure_withholds_the_findings_of_the_files_that_did_parse`, and it was right to: 0.1.0
/// withheld everything on the reasoning that a partial list reads as the state of the repository.
/// The reversal is deliberate, was reserved from the start, and is kept as an inversion rather than
/// a deletion so the contract that changed is visible in the suite instead of merely absent from it.
///
/// The withheld finding is proved real by asking for it on its own first — otherwise this passes on
/// a build where the fixture never had anything to report.
#[test]
fn a_parse_failure_no_longer_withholds_the_findings_of_the_files_that_did_parse() {
    // Arrange
    let reachable = tooprolix(&["check", "tests/fixtures/broken/long_docstring.py"]);
    assert_eq!(reachable.status.code(), Some(1), "{reachable:?}");
    assert!(
        stdout_of(&reachable).contains("TPX002"),
        "the fixture has nothing to withhold or to report: {:?}",
        stdout_of(&reachable)
    );

    // Act
    let whole_tree = tooprolix(&["check", "tests/fixtures/broken"]);

    // Assert
    assert_eq!(whole_tree.status.code(), Some(1), "{whole_tree:?}");
    assert!(
        stdout_of(&whole_tree).contains("TPX002"),
        "the finding of the file that parsed did not survive its broken sibling: {:?}",
        stdout_of(&whole_tree)
    );
    assert!(
        stderr_of(&whole_tree).contains("syntax_error.py"),
        "the run reported findings without saying part of the tree went unread: {:?}",
        stderr_of(&whole_tree)
    );
}

/// A scratch tree outside any git repository, canonicalised so that macOS's `/var` symlink cannot
/// make a walk look like it followed a link.
///
/// Built at run time rather than committed, for two reasons that are not stylistic: a fixture whose
/// own `.gitignore` hides half of it cannot be committed without `git add -f`, and a committed tree
/// of deliberate duplicates would change what `tooprolix check .` reports for this repository.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .expect("the system temporary directory exists")
            .join(format!("tooprolix-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch tree is creatable");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("a scratch directory is creatable");
        std::fs::write(&path, contents).expect("a scratch file is writable");
        path
    }

    fn check(&self, extra: &[&str]) -> Output {
        let mut arguments = vec!["check", "."];
        arguments.extend_from_slice(extra);
        Command::new(env!("CARGO_BIN_EXE_tooprolix"))
            .args(arguments)
            .current_dir(&self.root)
            .output()
            .expect("the binary cargo just built is executable")
    }

    /// `tooprolix check <target>` with the working directory `relative` *inside* the scratch tree.
    ///
    /// The working directory is a parameter and not a constant because `exclude` is resolved
    /// against the **configuration file's** directory: a rule written once at the root of a
    /// project has to mean the same thing to `tooprolix check .` run from the root and to the same
    /// command run from a package inside it. Nothing else here can vary the two independently.
    fn check_from(&self, relative: &str, target: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tooprolix"))
            .args(["check", target])
            .current_dir(self.root.join(relative))
            .output()
            .expect("the binary cargo just built is executable")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// One rationale, long enough for `TPX001` and repeated verbatim, which is what a licence header or
/// a policy note looks like.
const SHARED_RATIONALE: &str = "\
# The retry budget is small because the upstream service rate limits us.
# The retry budget is small because the upstream service rate limits us.
";

/// A path that does not exist is a tool error, not an empty repository.
#[test]
fn an_unreadable_path_is_an_error_and_not_a_clean_tree() {
    // Act
    let output = tooprolix(&["check", "tests/fixtures/does-not-exist"]);

    // Assert
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(stdout_of(&output), "");
    assert!(
        stderr_of(&output).contains("does-not-exist"),
        "{:?}",
        stderr_of(&output)
    );
}

/// All three shipping codes render, sorted by address, with an ordered aggregate, and the run is
/// byte-identical.
#[test]
fn the_findings_are_ordered_and_the_run_is_reproducible() {
    // Act — both formats, twice each. The JSON half is not decoration: the text line carries the
    // weakest edge's two ends but nothing else about a cluster, so a choice between two
    // equal-scoring edges that depended on arrival order would flap `weakest` and `locations` in
    // JSON while every text comparison stayed green. That is the FOURTH shape of one defect in this
    // crate — the finding not being a function of its input, hidden by a rendering that omitted the
    // field that moved (task 4: the score; task 4a: the weakest edge's referents; task 4a round 2:
    // which of two equal twins survived the dedup). Compare the bytes that carry every field.
    let first = tooprolix(&["check", "tests/fixtures/dup-corpus"]);
    let second = tooprolix(&["check", "tests/fixtures/dup-corpus"]);
    let first_json = tooprolix(&["check", "tests/fixtures/dup-corpus", "--format", "json"]);
    let second_json = tooprolix(&["check", "tests/fixtures/dup-corpus", "--format", "json"]);

    // Assert
    assert_eq!(
        stdout_of(&first),
        stdout_of(&second),
        "two runs over one unchanged tree disagreed"
    );
    assert_eq!(
        stdout_of(&first_json),
        stdout_of(&second_json),
        "two JSON runs over one unchanged tree disagreed"
    );
    assert!(
        stdout_of(&first_json).contains("\"weakest\""),
        "the reproducibility probe is not looking at the weakest edge at all"
    );

    let lines: Vec<&str> = stdout_of(&first).lines().collect();
    assert_eq!(
        lines,
        vec![
            "tests/fixtures/dup-corpus/client.py:2-4: TPX003 same explanation in 3 places: \
             tests/fixtures/dup-corpus/poller.py:2-3, tests/fixtures/dup-corpus/worker.py:2-3 \
             (weakest tests/fixtures/dup-corpus/client.py:2-4 ~ \
             tests/fixtures/dup-corpus/poller.py:2-3, similarity 0.900)",
            "tests/fixtures/dup-corpus/config.py:1-25: TPX002 docstring is 244 words long, over \
             the 200-word limit \u{2014} shorten it, or mark it with `# !TPX002` on the line \
             above it",
            "tests/fixtures/dup-corpus/legacy.py:2-20: TPX001 comment is 238 words long, over \
             the 150-word limit \u{2014} shorten it, or mark it with `# !TPX001` on the line \
             above it",
            "Found 3 findings (TPX001: 1, TPX002: 1, TPX003: 1).",
        ]
    );
}

/// A finding says where the block **ends**, not only where it starts.
///
/// Asserted on a real 25-line docstring through the real process, because the number that matters
/// is the one the extractor measured: a renderer that printed `line` twice would satisfy any
/// assertion written as `starts_with(path)` and most written as `contains(":1-")`. The end line is
/// checked against the JSON's own `end_line` for the same finding, so this cannot pass by agreeing
/// with itself.
#[test]
fn a_finding_addresses_the_whole_block_and_not_only_its_first_line() {
    // Arrange — the document is the independent witness of where the block ends.
    let json = tooprolix(&[
        "check",
        "tests/fixtures/dup-corpus/config.py",
        "--format",
        "json",
    ]);
    let document: serde_json::Value =
        serde_json::from_str(stdout_of(&json)).expect("stdout is one JSON document");
    assert_eq!(document["findings"][0]["line"], 1);
    assert_eq!(document["findings"][0]["end_line"], 25);

    // Act
    let text = tooprolix(&["check", "tests/fixtures/dup-corpus/config.py"]);

    // Assert
    assert!(
        stdout_of(&text).starts_with("tests/fixtures/dup-corpus/config.py:1-25: TPX002"),
        "the address stops at the first line of a 25-line block: {:?}",
        stdout_of(&text)
    );
}

/// Every address on a `TPX003` line carries the range, not only the one the finding is filed under.
///
/// This is what "one owner" costs and buys: `Location::Display` is reached five times on a cluster
/// line — the anchor, each rendered other, and both ends of the weakest edge — so a second renderer
/// for the secondary addresses is exactly the divergence the single owner exists to prevent. The
/// whole line is pinned rather than a substring, because a fold that dropped the range from the
/// `weakest` pair alone would pass every `contains` written against the head of the line.
#[test]
fn every_address_on_a_cluster_line_carries_the_range() {
    // Act
    let output = tooprolix(&["check", "tests/fixtures/dup-corpus"]);

    // Assert
    let first = stdout_of(&output)
        .lines()
        .next()
        .expect("the fixture reports a cluster");
    assert_eq!(
        first,
        "tests/fixtures/dup-corpus/client.py:2-4: TPX003 same explanation in 3 places: \
         tests/fixtures/dup-corpus/poller.py:2-3, tests/fixtures/dup-corpus/worker.py:2-3 \
         (weakest tests/fixtures/dup-corpus/client.py:2-4 ~ \
         tests/fixtures/dup-corpus/poller.py:2-3, similarity 0.900)"
    );
}

/// A run that read the whole tree and found nothing says so, and says it in plain bytes.
///
/// The exact bytes are the assertion. `Command::output` gives the child a pipe, which is the
/// not-a-terminal half of the colour rule, so an escape sequence here would be a real defect
/// reaching a real consumer — a log file, a CI annotation, a `| grep`. Checked as a byte
/// comparison **and** as an explicit scan for `ESC`, so a future line that legitimately changes
/// the wording cannot quietly take the colour rule with it.
#[test]
fn a_clean_full_run_says_so_and_a_pipe_receives_no_escape_codes() {
    // Act
    let output = tooprolix(&["check", "tests/fixtures/clean"]);

    // Assert
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        output.stdout,
        b"All checks passed!\n",
        "stdout was {:?}",
        stdout_of(&output)
    );
    assert!(
        !output.stdout.contains(&0x1b),
        "an ANSI escape reached a pipe: {:?}",
        output.stdout
    );
}

/// A run that could not read part of the tree says no findings were reachable without claiming the
/// tree passed.
///
/// This is the success line seen from the side that makes it dangerous. The exit code is already
/// 1 here (task 5's guarantee), so the only thing left that could call this tree clean is a line
/// of text. The summary must therefore carry the incomplete state in the same stdout answer.
#[test]
fn a_partial_run_with_nothing_to_report_prints_no_success_line() {
    // Arrange — one file, unparsable, and nothing else in the tree to find.
    let scratch = Scratch::new("partial-no-success-line");
    scratch.write("broken.py", "def (:\n");

    // Act
    let output = scratch.check(&[]);

    // Assert
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        stdout_of(&output),
        "No findings; check incomplete: 1 file skipped.\n",
        "a tree that was not read whole lost its incomplete summary"
    );
    assert!(
        stderr_of(&output).contains("broken.py"),
        "the skip was not even reported: {:?}",
        stderr_of(&output)
    );
}

/// A single file is a legal target, and `--help` has to say what it can and cannot find.
#[test]
fn a_single_file_is_checked_and_the_help_says_what_that_misses() {
    // Act
    let single = tooprolix(&["check", "tests/fixtures/dup-corpus/client.py"]);
    let help = tooprolix(&["--help"]);

    // Assert — the retry-budget comment is a duplicate of two blocks in sibling files, and it is
    // NOT reported here, because the detector only ever sees the blocks it was handed.
    assert_eq!(single.status.code(), Some(0), "{single:?}");
    assert_eq!(stdout_of(&single), CLEAN_STDOUT);

    assert_eq!(help.status.code(), Some(0), "{help:?}");
    assert!(
        stdout_of(&help).contains("only finds duplicates inside that file"),
        "--help does not warn that a single-file run is not a verdict on the repository: {}",
        stdout_of(&help)
    );
    for required in [
        "check <path>...",
        "one combined report",
        "one TPX003 input set",
        "same configuration source",
    ] {
        assert!(
            stdout_of(&help).contains(required),
            "--help is missing `{required}`: {}",
            stdout_of(&help)
        );
    }
    assert!(
        stdout_of(&help).contains("words"),
        "--help does not name the unit the limits are measured in: {}",
        stdout_of(&help)
    );
}

/// Several explicit files form one report, so cross-file TPX003 still sees every block.
#[test]
fn multiple_explicit_paths() {
    // Act
    let text = tooprolix(&[
        "check",
        "tests/fixtures/dup-corpus/client.py",
        "tests/fixtures/dup-corpus/poller.py",
        "tests/fixtures/dup-corpus/worker.py",
    ]);
    let missing = tooprolix(&[
        "check",
        "tests/fixtures/dup-corpus/client.py",
        "tests/fixtures/does-not-exist",
    ]);
    let json = tooprolix(&[
        "check",
        "tests/fixtures/dup-corpus/client.py",
        "--format",
        "json",
        "tests/fixtures/dup-corpus/poller.py",
        "tests/fixtures/dup-corpus/worker.py",
    ]);

    // Assert
    assert_eq!(text.status.code(), Some(1), "{text:?}");
    let lines: Vec<&str> = stdout_of(&text).lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "stdout is not one finding plus one summary: {lines:#?}"
    );
    assert!(
        lines[0].starts_with("tests/fixtures/dup-corpus/client.py:2-4: TPX003 ")
            && lines[0].contains("poller.py:2-3")
            && lines[0].contains("worker.py:2-3")
            && lines[0].contains("same explanation in 3 places"),
        "the three explicit files did not form one TPX003 input set: {lines:#?}"
    );
    assert_eq!(lines[1], "Found 1 findings (TPX003: 1).");
    assert_eq!(stderr_of(&text), "");

    assert_eq!(json.status.code(), Some(1), "{json:?}");
    assert_eq!(stderr_of(&json), "");
    let document: serde_json::Value =
        serde_json::from_str(stdout_of(&json)).expect("stdout is one JSON document");
    assert_eq!(document["schema_version"], "2");
    assert_eq!(document["complete"], true);
    assert_eq!(document["findings"].as_array().map(Vec::len), Some(1));
    assert_eq!(document["findings"][0]["code"], "TPX003");
    assert_eq!(
        document["findings"][0]["locations"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );

    assert_eq!(missing.status.code(), Some(2), "{missing:?}");
    assert_eq!(stdout_of(&missing), "", "{missing:?}");
    assert!(stderr_of(&missing).contains("does-not-exist"));
}

/// An explicitly named file wins over every spelling of the same file reached by a walk.
#[test]
#[cfg(unix)]
fn explicit_path_wins_over_walked_duplicate() {
    // Arrange
    let scratch = Scratch::new("explicit-wins");
    scratch.write("visible.py", &long_comment("visible retry policy"));
    scratch.write("excluded.py", &long_comment("excluded retry policy"));
    scratch.write("sub/.keep", "");
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"excluded.py\"]\n",
    );
    std::os::unix::fs::symlink(
        scratch.root.join("visible.py"),
        scratch.root.join("alias.py"),
    )
    .expect("a symlink is creatable");

    // Act — the directory contributes a walked copy and a skipped symlink; the first explicit
    // spelling must replace both, and the explicitly named excluded file must leave `excluded`.
    let output = scratch.check(&[
        "visible.py",
        "./visible.py",
        "alias.py",
        "sub/../visible.py",
        "excluded.py",
        "--format",
        "json",
    ]);

    // Assert
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(stderr_of(&output), "", "{output:?}");
    let document: serde_json::Value =
        serde_json::from_str(stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(document["complete"], true, "{document:#}");
    assert_eq!(document["skipped"].as_array().map(Vec::len), Some(0));
    assert_eq!(document["excluded"].as_array().map(Vec::len), Some(0));
    let paths: Vec<&str> = document["findings"]
        .as_array()
        .expect("findings is an array")
        .iter()
        .filter(|finding| finding["code"] == "TPX001")
        .map(|finding| finding["path"].as_str().expect("a finding path"))
        .collect();
    assert_eq!(paths, vec!["excluded.py", "visible.py"]);
}

/// Several targets cannot make CLI order choose between different project configurations.
#[test]
fn conflicting_explicit_path_configurations() {
    // Arrange
    let scratch = Scratch::new("conflicting-configs");
    scratch.write("one/a.py", &long_comment("first project retry policy"));
    scratch.write("two/b.py", &long_comment("second project retry policy"));
    scratch.write(
        "one/pyproject.toml",
        "[tool.tooprolix]\ncomment-max-volume = 5\n",
    );
    scratch.write(
        "two/pyproject.toml",
        "[tool.tooprolix]\ncomment-max-volume = 6\n",
    );

    // Act
    let forward = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
        .args(["check", "one/a.py", "two/b.py"])
        .current_dir(&scratch.root)
        .output()
        .expect("the binary cargo just built is executable");
    let reverse = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
        .args(["check", "two/b.py", "one/a.py"])
        .current_dir(&scratch.root)
        .output()
        .expect("the binary cargo just built is executable");

    // Assert
    for output in [&forward, &reverse] {
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert_eq!(stdout_of(output), "", "{output:?}");
        for expected in [
            "one/a.py",
            "two/b.py",
            "one/pyproject.toml",
            "two/pyproject.toml",
        ] {
            assert!(
                stderr_of(output).contains(expected),
                "the conflict does not name {expected}: {}",
                stderr_of(output)
            );
        }
    }
}

/// `.` is one of the three path forms the user confirmed, and it must not be special-cased.
#[test]
fn a_directory_and_a_dot_are_the_same_walk() {
    // Act
    let dot = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
        .args(["check", "."])
        .current_dir(repository_root().join("tests/fixtures/dup-corpus"))
        .output()
        .expect("the binary cargo just built is executable");
    let named = tooprolix(&["check", "tests/fixtures/dup-corpus"]);

    // Assert — same findings, same order; only the path prefix differs.
    assert_eq!(dot.status.code(), named.status.code());
    let rebased = stdout_of(&named).replace("tests/fixtures/dup-corpus/", "./");
    assert_eq!(stdout_of(&dot), rebased);
}

/// Nothing outside the fixture trees leaks into a run: paths are printed as they were reached.
#[test]
fn the_reported_path_is_the_one_the_user_typed() {
    let output = tooprolix(&["check", "tests/fixtures/dup-corpus/config.py"]);

    assert!(
        stdout_of(&output).starts_with("tests/fixtures/dup-corpus/config.py:1-25: TPX002"),
        "{:?}",
        stdout_of(&output)
    );
}

/// A file that is not Python is a tool error rather than a quiet zero.
#[test]
fn a_non_python_file_named_directly_is_an_error() {
    let output = tooprolix(&["check", "README.md"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        stderr_of(&output).contains("README.md"),
        "{:?}",
        stderr_of(&output)
    );
}

/// Guards the one thing a directory walk can silently get wrong: reporting nothing because it
/// visited nothing.
#[test]
fn a_directory_with_no_python_is_reported_rather_than_scored_clean() {
    let empty = repository_root().join("target/tests/empty-tree");
    std::fs::create_dir_all(&empty).expect("the scratch tree is creatable");
    let _ = std::fs::write(empty.join("notes.txt"), "not python\n");

    let output = tooprolix(&["check", "target/tests/empty-tree"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    // The success line DOES print here, and the warning beside it is what keeps that honest. The
    // tree was read whole — there was nothing in it to read — so the run is `Success` by every
    // part of the definition, and suppressing the line would put a second, quieter rule in the
    // renderer for a case the exit code already calls clean. Measured against the reference:
    // `ruff check --isolated <dir with no .py>` prints `warning: No Python files found under the
    // given path(s)` on stderr and `All checks passed!` on stdout, exit 0 — the same pair.
    assert_eq!(stdout_of(&output), CLEAN_STDOUT);
    assert!(
        stderr_of(&output).contains("no Python files"),
        "a walk that measured nothing reported success without saying it measured nothing: {:?}",
        stderr_of(&output)
    );
}

/// AC3 — the machine-readable form parses, carries its version, and keeps both finding shapes.
///
/// The document is checked as *parsed JSON* rather than as bytes: a snapshot of the text would go
/// red for a whitespace change and would say nothing about whether the shape is still the shape.
/// What is pinned is every field a consumer would read.
#[test]
fn the_json_document_is_valid_versioned_and_carries_both_shapes() {
    // Act
    let output = tooprolix(&["check", "tests/fixtures/dup-corpus", "--format", "json"]);

    // Assert — it is exit 1 like the text form, and stdout is the document and nothing else.
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: serde_json::Value =
        serde_json::from_str(stdout_of(&output)).expect("stdout is one JSON document");

    let mut keys: Vec<&str> = document
        .as_object()
        .expect("the document is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "complete",
            "excluded",
            "findings",
            "schema_version",
            "skipped"
        ],
        "the schema-v2 top-level shape changed: {document:#}"
    );
    assert_eq!(document["schema_version"], "2");
    let findings = document["findings"]
        .as_array()
        .expect("findings is an array");
    assert_eq!(findings.len(), 3, "{document:#}");

    let cluster = &findings[0];
    assert_eq!(cluster["code"], "TPX003");
    assert_eq!(cluster["path"], "tests/fixtures/dup-corpus/client.py");
    assert_eq!(cluster["line"], 2);
    assert_eq!(cluster["end_line"], 4);
    assert_eq!(cluster["prose_kind"], "comment");
    assert_eq!(
        cluster["locations"].as_array().map(std::vec::Vec::len),
        Some(3),
        "the cluster lost members on the way into JSON"
    );
    assert_eq!(
        cluster["weakest"]["first"]["path"],
        "tests/fixtures/dup-corpus/client.py"
    );
    assert_eq!(
        cluster["weakest"]["second"]["path"],
        "tests/fixtures/dup-corpus/poller.py"
    );
    assert!(
        cluster["weakest"]["similarity"].as_f64().expect("a score") < 1.0,
        "a reworded copy scored as an exact one"
    );
    assert!(
        cluster.get("words").is_none() && cluster.get("max_volume").is_none(),
        "the cluster shape carries dead volume fields: {cluster:#}"
    );

    let docstring = &findings[1];
    assert_eq!(docstring["code"], "TPX002");
    assert_eq!(docstring["prose_kind"], "docstring");
    assert_eq!(docstring["words"], 244);
    assert_eq!(docstring["max_volume"], 200);
    assert!(
        docstring.get("locations").is_none(),
        "the single-block shape carries a dead cluster field: {docstring:#}"
    );
    assert_eq!(findings[2]["code"], "TPX001");

    // Every finding renders the same sentence the text format prints, so the two cannot drift. The
    // final line is the human-only aggregate and is deliberately absent from the JSON document.
    // The length is asserted BEFORE the zip: `zip` stops at the shorter side, so zero text lines
    // would make the loop run zero times and the claim hold vacuously.
    let text = tooprolix(&["check", "tests/fixtures/dup-corpus"]);
    let lines: Vec<&str> = stdout_of(&text).lines().collect();
    assert_eq!(lines.len(), findings.len() + 1, "{lines:#?}");
    for (finding, line) in findings.iter().zip(&lines) {
        assert_eq!(finding["message"], *line);
    }
    assert_eq!(
        lines.last(),
        Some(&"Found 3 findings (TPX001: 1, TPX002: 1, TPX003: 1).")
    );
}

/// AC3's own gate, run the way the acceptance criterion words it: through a real Python parser.
#[test]
fn the_json_document_parses_in_python() {
    let output = tooprolix(&["check", "tests/fixtures/dup-corpus", "--format", "json"]);

    let python = Command::new("uv")
        .args([
            "run",
            "python3",
            "-c",
            "import json,sys; d=json.load(sys.stdin); print(d['schema_version'], len(d['findings']), d['complete'])",
        ])
        .current_dir(repository_root())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write as _;
            child
                .stdin
                .take()
                .expect("stdin was piped")
                .write_all(&output.stdout)?;
            child.wait_with_output()
        })
        .expect("uv run python3 is available in this environment");

    assert!(python.status.success(), "{python:?}");
    // `complete` is read by the same key lookup as the other two, so a document that lost the field
    // raises `KeyError` in the consumer rather than reading as fully measured — which is exactly
    // what the schema bump exists to make happen on the far end.
    assert_eq!(stdout_of(&python).trim(), "2 3 True");
}

/// AC4 — the paired opt-out, end to end.
///
/// Both halves are asserted in one test on purpose. "The marked block is not flagged" passes on a
/// rule that never fires, and "the unmarked block is flagged" passes on a marker that is never
/// read; only the pair rules out both.
#[test]
fn a_marker_silences_its_own_block_and_only_its_own_rule() {
    // Act
    let output = tooprolix(&["check", "tests/fixtures/optout"]);

    // Assert
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let lines: Vec<&str> = stdout_of(&output).lines().collect();
    let (summary, findings) = lines.split_last().expect("findings plus a summary");
    assert_eq!(
        *summary, "Found 4 findings (TPX001: 3, TPX002: 1).",
        "the final line is not the expected summary: {lines:#?}"
    );
    let flagged: Vec<&str> = findings
        .iter()
        .map(|line| {
            line.strip_prefix("tests/fixtures/optout/")
                .expect("every line before the summary is a fixture finding")
        })
        .collect();

    assert_eq!(
        flagged
            .iter()
            .map(|line| line.split(':').next().expect("a file name"))
            .collect::<Vec<_>>(),
        vec![
            // marked: silenced. blanket: silenced. docstring_marked: silenced.
            "comment_mistyped.py",
            "comment_unmarked.py",
            "comment_wrong_code.py",
            "docstring_unmarked.py",
        ],
        "got {flagged:#?}"
    );

    // The red-team half: a marker naming a code that does not exist warns and silences nothing.
    assert!(
        stderr_of(&output).contains("`TPX999` in an opt-out marker is not a rule code"),
        "an unknown code in a marker was swallowed: {:?}",
        stderr_of(&output)
    );
    assert!(
        stdout_of(&output).contains("comment_mistyped.py:6-19: TPX001"),
        "an unknown code in a marker silenced a real rule: {}",
        stdout_of(&output)
    );
}

/// **Prose is never eaten by a comment that merely looks marker-shaped.**
///
/// The worst outcome this tool has, measured on the committed 0.2.0 parser: a comment a human wrote
/// without ever having heard of tooprolix — `# !important: never cache this response` — parsed as a
/// marker naming an unrecognised code. `extract` then dropped the line as a directive, the remainder
/// fell under the two-line block gate, the finding disappeared, and **no warning fired at all**,
/// because no block survived to carry the unknown-code diagnostic. Silent suppression by English.
///
/// The BOM half is the same defect from the other side: a marker that stops working because a
/// legitimate byte sits in front of it, again in silence.
///
/// Every case is paired with a control that must produce the finding, so none of the assertions can
/// pass on a run that measured nothing.
#[test]
fn ordinary_prose_is_not_swallowed_by_a_marker_shaped_comment() {
    // Arrange — ONE comment line of 160 words, which is what makes the defect visible rather than
    // merely renumbered: a block needs two lines, so dropping the line above this one takes the
    // whole block below the gate and the finding stops existing.
    let body = format!("# {}\n", "word ".repeat(160));
    // Two trees, one file each: same prose in both, so a shared tree would report them as a TPX003
    // cluster and the two runs would stop being comparable on TPX001 alone.
    let control = Scratch::new("prose-control");
    control.write(
        "control.py",
        &format!("x = 1\n# a plain first line\n{body}"),
    );
    let scratch = Scratch::new("prose-not-eaten");
    scratch.write(
        "exclamation.py",
        &format!("x = 1\n# !important: never cache this response\n{body}"),
    );
    // The same defect in somebody else's namespace, which is how it survived the first fix: the
    // gate tested the general `[A-Z]+[0-9]+` shape used to *report* an unknown code, and `HTTP2`,
    // `UTF8`, `SHA256`, `RFC2119` and `TLS13` all match it.
    let foreign = Scratch::new("prose-not-eaten-foreign");
    foreign.write(
        "http2.py",
        &format!("x = 1\n# !HTTP2 is mandatory for this endpoint\n{body}"),
    );

    // The BOM trio, on docstrings so the marker sits on its own line above the block. Each one says
    // something different, or the three identical docstrings would be a TPX003 cluster and the
    // assertions below could not tell a suppressed TPX002 from a cluster naming the same file.
    let docstring = |subject: &str| {
        format!(
            "\"\"\"Overview.\n{}\"\"\"\n",
            format!(
                "The {subject} is read once per batch because two reads straddle a boundary.\n"
            )
            .repeat(30)
        )
    };
    let bom = Scratch::new("prose-not-eaten-bom");
    bom.write("capable.py", &docstring("clock"));
    bom.write("marked.py", &format!("# !TPX002\n{}", docstring("ledger")));
    bom.write(
        "bom_marked.py",
        &format!("\u{feff}# !TPX002\n{}", docstring("cursor")),
    );

    // Act
    let control_output = control.check(&[]);
    let output = scratch.check(&[]);
    let foreign_output = foreign.check(&[]);
    let bom_output = bom.check(&[]);

    // Assert — the control proves the fixture can fire ...
    assert!(
        stdout_of(&control_output).contains("TPX001"),
        "the fixture cannot demonstrate a suppression it never triggers: {}",
        stdout_of(&control_output)
    );
    // ... and the `!` comment must not have removed the same finding from the same prose.
    assert!(
        stdout_of(&output).contains("TPX001"),
        "an ordinary English comment beginning with `!` silenced a whole block: {:?} / {:?}",
        stdout_of(&output),
        stderr_of(&output)
    );
    // ... and neither did the same comment in a namespace that is not ours.
    assert!(
        stdout_of(&foreign_output).contains("TPX001"),
        "a comment about HTTP/2 silenced a whole block: {:?} / {:?}",
        stdout_of(&foreign_output),
        stderr_of(&foreign_output)
    );
    assert_eq!(
        stderr_of(&foreign_output),
        "",
        "a comment about HTTP/2 was reported as a mistyped marker: {:?}",
        stderr_of(&foreign_output)
    );

    // The BOM: `capable.py` proves the docstring is a finding, `marked.py` proves the marker works,
    // and `bom_marked.py` must behave exactly like `marked.py` and not like `capable.py`.
    assert!(
        stdout_of(&bom_output).contains("capable.py:1-32: TPX002"),
        "the BOM fixture cannot demonstrate anything: {}",
        stdout_of(&bom_output)
    );
    // `./` prefixes, not bare names: `marked.py` is a substring of `bom_marked.py`, so the plain
    // assertion below would answer for the BOM one too and the pair would stop being a pair.
    assert!(
        !stdout_of(&bom_output).contains("./marked.py"),
        "the plain marker stopped working: {}",
        stdout_of(&bom_output)
    );
    assert!(
        !stdout_of(&bom_output).contains("./bom_marked.py"),
        "a UTF-8 byte order mark silently defeated a correct marker: {}",
        stdout_of(&bom_output)
    );
}

/// **A word in the reason cannot silence a rule the marker never named.**
///
/// The same seam as `ordinary_prose_is_not_swallowed_by_a_marker_shaped_comment`, one position
/// further along: the code list and the reason were separated by nothing, because the tokeniser
/// split on comma and space alike. A sentence written after a code the author really did mean was
/// parsed as more codes — including the blanket literal, which is the one token that must never be
/// reachable except by being written on purpose.
///
/// The fixture is a four-line, 180-word comment run, chosen so that it survives losing its marker
/// line. Without that, "no finding" would be ambiguous between "suppressed" and "the block
/// collapsed under the two-line gate", and the test would pass for the wrong reason.
#[test]
fn a_word_in_the_reason_cannot_silence_a_rule_the_marker_never_named() {
    // Arrange — 180 words over four lines, over the 150-word default and still four lines after the
    // marker line is taken off it, which is what keeps "no finding" unambiguous.
    let body = format!("# {}\n", "word ".repeat(45)).repeat(4);
    let capable = Scratch::new("reason-capable");
    capable.write("c0.py", &body);
    // A reason that could never be mistaken for a code — the control for "the marker still works".
    let plain = Scratch::new("reason-plain");
    plain.write(
        "c1.py",
        &format!("# !TPX002 blanket would be overkill here\n{body}"),
    );
    // The blanket literal in the reason position.
    let blanket = Scratch::new("reason-blanket");
    blanket.write(
        "c2.py",
        &format!("# !TPX002 TPX* would be overkill here\n{body}"),
    );
    // A real code in the reason position.
    let code = Scratch::new("reason-code");
    code.write(
        "c4.py",
        &format!("# !TPX002 TPX001 was fixed above\n{body}"),
    );
    // A starred form INSIDE the comma list, which must warn — Decisions #9.
    let starred = Scratch::new("reason-starred");
    starred.write("c3.py", &format!("# !TPX002,TPX0*\n{body}"));

    // Act
    let c0 = capable.check(&[]);
    let c1 = plain.check(&[]);
    let c2 = blanket.check(&[]);
    let c4 = code.check(&[]);
    let c3 = starred.check(&[]);

    // Assert — the fixture is capable, and it stays capable with a marker for another rule on it.
    assert!(
        stdout_of(&c0).contains("TPX001"),
        "the fixture cannot demonstrate a suppression it never triggers: {}",
        stdout_of(&c0)
    );
    assert!(
        stdout_of(&c1).contains("TPX001"),
        "a TPX002 marker silenced TPX001: {}",
        stdout_of(&c1)
    );

    // ... and neither reason may take the finding away, silently or otherwise.
    assert!(
        stdout_of(&c2).contains("TPX001"),
        "`TPX*` in the reason position set the blanket: {:?} / {:?}",
        stdout_of(&c2),
        stderr_of(&c2)
    );
    assert!(
        stdout_of(&c4).contains("TPX001"),
        "a code named in the reason position silenced its rule: {:?} / {:?}",
        stdout_of(&c4),
        stderr_of(&c4)
    );

    // The comma list keeps the other half of the contract: an unrecognised token there warns.
    assert!(
        stdout_of(&c3).contains("TPX001"),
        "`TPX0*` suppressed a rule: {}",
        stdout_of(&c3)
    );
    assert!(
        stderr_of(&c3).contains("`TPX0*`"),
        "a starred form inside a comma list said nothing: {:?}",
        stderr_of(&c3)
    );
}

/// A comment that was aiming at a marker and missed is reported — and reported is all it is.
///
/// The class this closes was measured on the 0.1.0 binary: a typo in the **code** was already loud
/// (`TPX999` names its file and line), but a typo in the **directive** was completely silent, and
/// the 0.2.0 grammar makes that worse rather than better — a forgotten `!` leaves `# TPX002`, one
/// character from a working marker.
///
/// Four properties in one test, because each of the first three passes on a tool that always warns
/// and the fourth passes on a tool that never does:
///
/// 1. the 0.1.0 marker warns, in both positions it can occupy;
/// 2. it silences nothing — the finding it used to remove is back;
/// 3. it does not change the exit code, in either direction;
/// 4. a **working** marker produces no warning at all, so the diagnostic is not noise.
#[test]
fn a_comment_that_was_aiming_at_a_marker_is_reported_without_changing_the_outcome() {
    // Arrange — one long comment run and one long docstring, each over its default limit.
    let long_comment =
        "# The cache is warmed at start because a cold read costs whole seconds.\n".repeat(20);
    let long_docstring = format!(
        "def process(batch):\n{}    \"\"\"Overview.\n{}    \"\"\"\n",
        "",
        "    Clocks are read once per batch because two reads straddle a boundary.\n".repeat(30)
    );

    // The 0.1.0 spelling, in the two positions a dead marker can land in: above a comment run it
    // is absorbed into the prose it used to excuse and becomes the run's FIRST line, while above a
    // docstring it stays a comment on the line above.
    let loud = Scratch::new("near-miss-loud");
    loud.write(
        "comment.py",
        &format!("# tooprolix: noqa TPX001\n{long_comment}"),
    );
    loud.write(
        "docstring.py",
        &long_docstring.replace(
            "def process(batch):\n",
            "def process(batch):\n    # tooprolix: noqa TPX002\n",
        ),
    );

    // The same two files with the 0.2.0 marker: silenced, and silent.
    let quiet = Scratch::new("near-miss-quiet");
    quiet.write("comment.py", &format!("# !TPX001\n{long_comment}"));
    quiet.write(
        "docstring.py",
        &long_docstring.replace(
            "def process(batch):\n",
            "def process(batch):\n    # !TPX002\n",
        ),
    );

    // A near-miss over a block that is not a finding at all — the case that proves the warning
    // cannot move the exit code, because there is no finding for it to hide behind.
    let clean = Scratch::new("near-miss-clean");
    clean.write(
        "short.py",
        "# TPX001 remember to shorten this one day\n\
         # one two three four five six seven eight\n\
         # nine ten eleven twelve thirteen fourteen fifteen sixteen\n",
    );

    // The 0.1.0 BLANKET marker, which carries no code and so cannot be found by a code-shaped
    // search. Its upgrade goes wrong twice: it stops suppressing, and its own two words are counted
    // as prose — measured, a 149-word run becomes 151 and reports TPX001 that the same run without
    // the dead marker never reported. Both halves are asserted, and both were silent before.
    let legacy = Scratch::new("near-miss-legacy");
    legacy.write(
        "blanket.py",
        &format!("x = 1\n# tooprolix: noqa\n# {}\n", "word ".repeat(149)),
    );
    let legacy_control = Scratch::new("near-miss-legacy-control");
    legacy_control.write("blanket.py", &format!("x = 1\n# {}\n", "word ".repeat(149)));

    // Act
    let loud_output = loud.check(&[]);
    let quiet_output = quiet.check(&[]);
    let clean_output = clean.check(&[]);
    let legacy_output = legacy.check(&[]);
    let legacy_control_output = legacy_control.check(&[]);

    // Assert — (1) both positions warn, and the warning names the file, the line and the form.
    assert!(
        stderr_of(&loud_output).contains("comment.py:1: this is not an opt-out marker"),
        "a dead marker absorbed into a comment run went unreported: {:?}",
        stderr_of(&loud_output)
    );
    assert!(
        stderr_of(&loud_output).contains("docstring.py:2: this is not an opt-out marker"),
        "a dead marker above a docstring went unreported: {:?}",
        stderr_of(&loud_output)
    );
    assert!(
        stderr_of(&loud_output).contains("`# !TPX001`"),
        "the warning does not say what to write instead: {:?}",
        stderr_of(&loud_output)
    );

    // (2) and (3) — the findings are back and the exit code is the ordinary one for findings.
    assert_eq!(loud_output.status.code(), Some(1), "{loud_output:?}");
    assert!(
        stdout_of(&loud_output).contains("TPX001") && stdout_of(&loud_output).contains("TPX002"),
        "the 0.1.0 marker still suppressed something: {}",
        stdout_of(&loud_output)
    );

    // (3) again, on the half that cannot hide behind a finding: a warning, and still exit 0.
    assert_eq!(clean_output.status.code(), Some(0), "{clean_output:?}");
    assert_eq!(
        stdout_of(&clean_output),
        CLEAN_STDOUT,
        "the near-miss manufactured a finding: {}",
        stdout_of(&clean_output)
    );
    assert!(
        stderr_of(&clean_output).contains("short.py:1: this is not an opt-out marker"),
        "a near-miss over a clean block said nothing: {:?}",
        stderr_of(&clean_output)
    );

    // (4) — the control. Working markers suppress and say nothing, or the warning is just noise.
    assert_eq!(quiet_output.status.code(), Some(0), "{quiet_output:?}");
    assert_eq!(stdout_of(&quiet_output), CLEAN_STDOUT);
    assert_eq!(
        stderr_of(&quiet_output),
        "",
        "a working marker was reported as a near-miss: {:?}",
        stderr_of(&quiet_output)
    );

    // (5) — the 0.1.0 blanket, which carries no code at all. The control shows the same 149-word
    // run is silent without it, so the finding below is one the dead marker's own words created;
    // the warning is the only thing that connects the two for whoever is upgrading.
    assert_eq!(
        stdout_of(&legacy_control_output),
        CLEAN_STDOUT,
        "the control fires on its own, so it cannot show what the dead marker added: {}",
        stdout_of(&legacy_control_output)
    );
    assert!(
        stderr_of(&legacy_output).contains("blanket.py:2: this is not an opt-out marker"),
        "the 0.1.0 blanket marker went completely unreported: {:?}",
        stderr_of(&legacy_output)
    );
}

/// AC5 — `.gitignore` is respected, and the fixture is proved capable of producing the finding it
/// is supposed to suppress.
///
/// Without the second half this passes on a walk that finds nothing at all, which is the shape of
/// fail-open this project has already shipped once.
#[test]
fn the_walk_respects_gitignore_and_the_fixture_can_prove_it() {
    // Arrange
    let scratch = Scratch::new("gitignore");
    scratch.write("visible.py", SHARED_RATIONALE);
    let hidden = scratch.write("vendor/copy.py", SHARED_RATIONALE);
    assert!(
        hidden.is_file(),
        "the ignored file must exist to be ignored"
    );

    // Act — first without a .gitignore, to show the pair really is a finding.
    let uncovered = scratch.check(&[]);
    scratch.write(".gitignore", "vendor/\n");
    let covered = scratch.check(&[]);

    // Assert
    assert_eq!(uncovered.status.code(), Some(1), "{uncovered:?}");
    assert!(
        stdout_of(&uncovered).contains("TPX003"),
        "the fixture cannot demonstrate an exclusion it never triggers: {}",
        stdout_of(&uncovered)
    );
    assert_eq!(covered.status.code(), Some(0), "{covered:?}");
    assert_eq!(
        stdout_of(&covered),
        CLEAN_STDOUT,
        "a .gitignore'd file was scanned: {}",
        stdout_of(&covered)
    );
}

/// The walk does not follow symlinks, so one file behind two names is one file.
///
/// Measured on pydantic 2026-07-25: following them takes 343 findings to 559, because the same
/// sources are reached twice through `tests/pydantic_core`.
#[test]
#[cfg(unix)]
fn the_walk_does_not_follow_symlinks() {
    // Arrange — `real/` holds a duplicate pair; `link` points at it, so a following walk sees four
    // copies of one rationale instead of two and reports the same cluster twice as large.
    let scratch = Scratch::new("symlink");
    scratch.write("real/a.py", SHARED_RATIONALE);
    scratch.write("real/b.py", SHARED_RATIONALE);
    std::os::unix::fs::symlink(scratch.root.join("real"), scratch.root.join("link"))
        .expect("a symlink is creatable in the scratch tree");
    assert!(
        scratch.root.join("link/a.py").is_file(),
        "the symlink must resolve, or this test proves nothing"
    );

    // Act
    let output = scratch.check(&[]);

    // Assert
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let lines: Vec<&str> = stdout_of(&output).lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "stdout must be exactly one finding and its summary: {lines:#?}"
    );
    assert_eq!(lines[1], "Found 1 findings (TPX003: 1).");
    assert!(
        lines[0].contains(": TPX003 ") && lines[0].contains("in 2 places"),
        "the same file was counted twice through a symlink: {}",
        stdout_of(&output)
    );
    assert!(
        !stdout_of(&output).contains("link/"),
        "the walk descended into a symlink: {}",
        stdout_of(&output)
    );
}

/// Hidden entries are skipped, which is 0.1.0's only defence against linting a virtualenv.
///
/// The module documentation calls it exactly that and nothing asserted it. It is also the one walk
/// property that is a **divergence from ruff** — ruff sets `hidden(false)` and compensates with a
/// default `exclude` list we deliberately do not ship — so it is the property most likely to be
/// "corrected" back by someone reading ruff, and the one that would then silently start reporting
/// findings from `.venv`.
///
/// Both halves again: the same tree with the directory un-hidden must produce the finding, or this
/// passes on a walk that found nothing for some other reason.
#[test]
fn the_walk_skips_hidden_entries() {
    // Arrange
    let scratch = Scratch::new("hidden");
    scratch.write("visible.py", SHARED_RATIONALE);
    scratch.write(".venv/lib/copy.py", SHARED_RATIONALE);

    // Act
    let hidden = scratch.check(&[]);
    // ... and the control: the identical tree with the directory not hidden.
    let control = Scratch::new("hidden-control");
    control.write("visible.py", SHARED_RATIONALE);
    control.write("venv/lib/copy.py", SHARED_RATIONALE);
    let exposed = control.check(&[]);

    // Assert
    assert_eq!(hidden.status.code(), Some(0), "{hidden:?}");
    assert_eq!(
        stdout_of(&hidden),
        CLEAN_STDOUT,
        "a dot-directory was walked: {}",
        stdout_of(&hidden)
    );
    assert_eq!(exposed.status.code(), Some(1), "{exposed:?}");
    assert!(
        stdout_of(&exposed).contains("TPX003"),
        "the fixture cannot demonstrate a skip it never triggers: {}",
        stdout_of(&exposed)
    );
}

/// There is no `--` end-of-options marker, and `--help` says so — this pins that it is true.
///
/// A documented workaround that does not work is worse than the missing feature. `--` was skipped
/// deliberately: `./-name.py` is the universal idiom, it costs the user four characters, and the
/// alternative is a parser branch for a filename nobody has. If that ever stops being true, this
/// test is where the claim lives.
#[test]
fn a_path_beginning_with_a_dash_is_reachable_only_as_documented() {
    // Arrange
    let scratch = Scratch::new("dash");
    // Long enough to fire TPX001 on its own: TPX003 is cross-file, so a single-file run cannot
    // demonstrate that the file was reached.
    scratch.write("-weird.py", &SHARED_RATIONALE.repeat(10));

    // Act
    let bare = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
        .args(["check", "-weird.py"])
        .current_dir(&scratch.root)
        .output()
        .expect("the binary cargo just built is executable");
    let prefixed = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
        .args(["check", "./-weird.py"])
        .current_dir(&scratch.root)
        .output()
        .expect("the binary cargo just built is executable");
    let help = tooprolix(&["--help"]);

    // Assert
    assert_eq!(bare.status.code(), Some(2), "{bare:?}");
    assert!(
        stderr_of(&bare).contains("unknown option"),
        "{:?}",
        stderr_of(&bare)
    );
    assert_eq!(prefixed.status.code(), Some(1), "{prefixed:?}");
    assert!(
        stdout_of(&prefixed).contains("./-weird.py:1-20: TPX001"),
        "the documented workaround does not work: {}",
        stdout_of(&prefixed)
    );
    assert!(
        stdout_of(&help).contains("./-name.py"),
        "--help does not tell the user the workaround: {}",
        stdout_of(&help)
    );
}

/// `[tool.tooprolix]` reaches the detectors, and each key reaches its own rule.
#[test]
fn the_project_configuration_changes_what_is_reported() {
    // Arrange
    let scratch = Scratch::new("config");
    scratch.write("a.py", SHARED_RATIONALE);
    scratch.write("b.py", SHARED_RATIONALE);

    // Act — three configurations over one unchanged tree.
    let defaults = scratch.check(&[]);
    scratch.write(
        "pyproject.toml",
        "[project]\nname = \"scratch\"\n\n[tool.tooprolix]\ncomment-max-volume = 5\n",
    );
    let tightened = scratch.check(&[]);
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nignore = [\"TPX001\", \"TPX002\", \"TPX003\"]\n",
    );
    let silenced = scratch.check(&[]);

    // Assert
    let default_lines: Vec<&str> = stdout_of(&defaults).lines().collect();
    assert_eq!(
        default_lines.len(),
        2,
        "stdout must be one finding and one summary: {defaults:?}"
    );
    assert!(
        default_lines[0].starts_with("./") && default_lines[0].contains(": TPX003 "),
        "the line before the summary is not the expected finding: {default_lines:#?}"
    );
    assert_eq!(default_lines[1], "Found 1 findings (TPX003: 1).");
    assert!(
        !stdout_of(&defaults).contains("TPX001"),
        "the rationale is under the default limit and must not fire: {}",
        stdout_of(&defaults)
    );

    assert_eq!(tightened.status.code(), Some(1));
    let tightened_lines: Vec<&str> = stdout_of(&tightened).lines().collect();
    let (tightened_summary, tightened_findings) = tightened_lines
        .split_last()
        .expect("findings plus a summary");
    assert_eq!(
        *tightened_summary,
        "Found 3 findings (TPX001: 2, TPX003: 1)."
    );
    assert!(
        tightened_findings
            .iter()
            .all(|line| line.starts_with("./") && line.contains(": TPX")),
        "an unexpected stdout line preceded the summary: {tightened_lines:#?}"
    );
    assert_eq!(
        tightened_findings
            .iter()
            .filter(|line| line.contains(": TPX001 "))
            .count(),
        2,
        "a lowered comment limit did not reach the detector: {}",
        stdout_of(&tightened)
    );

    assert_eq!(silenced.status.code(), Some(0), "{silenced:?}");
    // Clean and complete, so the line prints — and the stderr warning beside it is the whole
    // reason that is not a lie: the run really did find nothing, and the warning is what says it
    // could not have. Same pairing as the empty-tree case above, and as ruff's.
    assert_eq!(stdout_of(&silenced), CLEAN_STDOUT);
    assert!(
        stderr_of(&silenced).contains("every rule (TPX001, TPX002, TPX003) is disabled"),
        "a run that could not report anything said nothing about it: {:?}",
        stderr_of(&silenced)
    );
}

/// A broken configuration is exit 2, and the message names the file and the mistake.
///
/// Every one of these would otherwise fall back to a default: a threshold that is not applied, and
/// cannot be seen not to be applied.
#[test]
fn a_broken_configuration_is_a_tool_error_and_not_a_default() {
    let scratch = Scratch::new("badconfig");
    scratch.write("a.py", SHARED_RATIONALE);
    scratch.write("b.py", SHARED_RATIONALE);

    for (table, expected) in [
        ("[tool.tooprolix]\nignore = [\"TPX999\"]\n", "TPX999"),
        // Was `exclude = ["vendor"]`, when `exclude` was the stand-in for "a key we do not have".
        // It is a shipping key now, so the row would assert the opposite of the truth. A near-miss
        // spelling keeps the case it was written for — an unknown key is fatal — and
        // `an_unknown_key_is_still_fatal_and_the_known_list_now_carries_exclude` covers the half
        // this row can no longer see.
        (
            "[tool.tooprolix]\nexclude-vendor = [\"vendor\"]\n",
            "exclude-vendor",
        ),
        ("[tool.tooprolix]\ncomment-max-volume = -3\n", "-3"),
        ("[tool.tooprolix]\ncomment-max-volume = \"150\"\n", "string"),
        // Not `"parse"` — that word is in our own format string and was satisfied whatever the
        // parser said. The line number can only come from the parser's own `Display`.
        ("[tool.tooprolix]\n\nthis is not toml\n", "line 3"),
    ] {
        scratch.write("pyproject.toml", table);

        let output = scratch.check(&[]);

        assert_eq!(output.status.code(), Some(2), "{table:?} gave {output:?}");
        assert_eq!(
            stdout_of(&output),
            "",
            "a broken configuration still printed findings: {table:?}"
        );
        assert!(
            stderr_of(&output).contains(expected),
            "{table:?}: the message does not name `{expected}`: {:?}",
            stderr_of(&output)
        );
        assert!(
            stderr_of(&output).contains("pyproject.toml"),
            "{table:?}: the message does not name the file: {:?}",
            stderr_of(&output)
        );
    }
}

/// The configuration is found from the checked path, not from the working directory.
///
/// With a cwd rule, `tooprolix check src/api.py` and `cd src && tooprolix check api.py` would apply
/// different limits to the same file — and a CI job that changed directory would silently change
/// the thresholds it enforces.
#[test]
fn the_configuration_is_found_relative_to_the_checked_path() {
    let scratch = Scratch::new("discovery");
    scratch.write("project/pkg/a.py", SHARED_RATIONALE);
    scratch.write(
        "project/pyproject.toml",
        "[tool.tooprolix]\ncomment-max-volume = 5\n",
    );

    // Run from OUTSIDE the project, and with a `..` in the path that only canonicalisation removes.
    let from_outside = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
        .args(["check", "project/pkg/../pkg/a.py"])
        .current_dir(&scratch.root)
        .output()
        .expect("the binary cargo just built is executable");

    assert_eq!(from_outside.status.code(), Some(1), "{from_outside:?}");
    assert!(
        stdout_of(&from_outside).contains("TPX001"),
        "the project's own limit was not applied from outside it: {}",
        stdout_of(&from_outside)
    );
    assert!(
        stdout_of(&from_outside).contains("project/pkg/../pkg/a.py"),
        "the path was reported canonicalised rather than as typed: {}",
        stdout_of(&from_outside)
    );
}

/// A clean run says so in text, and is still exactly one document — and nothing else — in JSON.
///
/// The two halves are asserted together so neither can be changed alone. Zero bytes on a
/// successful `--format json` is a parse error at the consumer's end that is indistinguishable
/// from a crash, so the empty document has to be written; and the sentence the text format gained
/// must not follow it there. `serde_json::from_str` over the **whole** of stdout is what enforces
/// that: a success line appended to a document is trailing input, and parsing fails. The explicit
/// checks below say the same thing in the error message a maintainer will actually read.
#[test]
fn a_clean_run_says_so_in_text_and_is_only_a_document_in_json() {
    // Act
    let text = tooprolix(&["check", "tests/fixtures/clean"]);
    let json = tooprolix(&["check", "tests/fixtures/clean", "--format", "json"]);

    // Assert
    assert_eq!(text.status.code(), Some(0), "{text:?}");
    assert_eq!(stdout_of(&text), CLEAN_STDOUT);

    assert_eq!(json.status.code(), Some(0), "{json:?}");
    assert!(
        !stdout_of(&json).contains("All checks passed"),
        "the success line leaked into the machine-readable format: {:?}",
        stdout_of(&json)
    );
    assert!(
        !json.stdout.contains(&0x1b),
        "an ANSI escape reached the JSON document: {:?}",
        stdout_of(&json)
    );
    let document: serde_json::Value =
        serde_json::from_str(stdout_of(&json)).expect("a clean run still emits one JSON document");
    assert_eq!(document["schema_version"], "2");
    assert_eq!(
        document["findings"].as_array().map(std::vec::Vec::len),
        Some(0)
    );
}

// ---------------------------------------------------------------------------------------------
// `exclude` — the measurement boundary.
//
// `exclude` is not configurability for its own sake. It is the one lever that makes the strict
// exit contract (`crate::cli`, exit 2: "a file that does not parse must never be reportable as
// clean") applicable to a repository that legitimately contains invalid Python — a parser's own
// test corpus, a vendored tree, a fixture directory. `.gitignore` cannot express it, because those
// files are deliberately committed, and an opt-out marker cannot save a file that does not parse
// far enough to have comments read out of it.
//
// It is deliberately NOT graceful handling of unreadable files. That is a different contract:
// `exclude` says "this was never part of the measurement", graceful says "the measurement hit
// something it could not read". This half must never be able to swallow the second.
// ---------------------------------------------------------------------------------------------

/// A long comment run, over the default 150-word limit, so a single file can carry `TPX001`.
fn long_comment(subject: &str) -> String {
    format!("# The {subject} is described here at length and entirely on purpose.\n").repeat(20)
}

/// The headline: a file the project excluded is neither checked nor a reason to fail.
///
/// Both halves are asserted and neither alone is worth anything. Without the "before" run this
/// passes on a build where the fixture never failed in the first place; without the "after" run it
/// passes on an `exclude` that swallows the *whole* tree. The unparsable file is the point — it is
/// the case `.gitignore` and the opt-out marker both structurally cannot reach.
#[test]
fn an_excluded_unparsable_file_is_not_a_measurement_failure() {
    // Arrange
    let scratch = Scratch::new("exclude-broken");
    scratch.write("vendor/parser_fixture.py", "def f(:\n    pass\n");
    scratch.write("app.py", "\"\"\"A short docstring.\"\"\"\n");

    // Act — first WITHOUT the key, to prove the tree really does fail.
    let before = scratch.check(&[]);
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor\"]\n",
    );
    let after = scratch.check(&[]);

    // Assert — the "before" run is exit 1 and not 2 since the graceful change, but the half this
    // test needs is unchanged: the unparsable file is visible in the run's answer, so excluding it
    // is demonstrably a change and not a no-op.
    assert_eq!(
        before.status.code(),
        Some(1),
        "the fixture cannot demonstrate an exclusion it never triggers: {before:?}"
    );
    assert!(
        stderr_of(&before).contains("parser_fixture.py"),
        "the unparsable file went unreported, so there is nothing for `exclude` to remove: {:?}",
        stderr_of(&before)
    );

    assert_eq!(
        after.status.code(),
        Some(0),
        "an excluded unparsable file still failed the run: {:?}",
        stderr_of(&after)
    );
    assert_eq!(stdout_of(&after), CLEAN_STDOUT);
    assert_eq!(
        stderr_of(&after),
        "",
        "an excluded file is not a diagnostic either — it was never part of the measurement: {:?}",
        stderr_of(&after)
    );
}

/// AC3 — the glob is matched against the path **relative to the configuration file**, not against
/// the basename and not against the working directory.
///
/// This is the test the recorded mutation has to redden: matching on `file_name()` instead of the
/// relative path. The fixture is built so that mutation is *observable*, which a two-file tree
/// cannot do — with only `vendor/copy.py` and `keep/copy.py`, excluding both leaves one block, and
/// excluding one leaves one block, and `TPX003` needs two members either way, so both answers are
/// exit 0 and the test is blind. The third file is what gives the two outcomes different shapes:
///
/// | matcher | excluded | surviving blocks | reported |
/// |---|---|---|---|
/// | relative path (correct) | `vendor/copy.py` | `keep/copy.py`, `anchor.py` | `TPX003 in 2 places` |
/// | basename (the mutation) | both `copy.py` | `anchor.py` | nothing, exit 0 |
#[test]
fn exclude_is_matched_on_the_path_relative_to_the_configuration_file_not_on_the_basename() {
    // Arrange
    let scratch = Scratch::new("exclude-relative");
    scratch.write("vendor/copy.py", SHARED_RATIONALE);
    scratch.write("keep/copy.py", SHARED_RATIONALE);
    scratch.write("anchor.py", SHARED_RATIONALE);

    // Act — without the key first, so the fixture is proved capable of the finding it must keep.
    let before = scratch.check(&[]);
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor/copy.py\"]\n",
    );
    let after = scratch.check(&[]);

    // Assert
    assert_eq!(before.status.code(), Some(1), "{before:?}");
    assert!(
        stdout_of(&before).contains("in 3 places"),
        "the fixture must start as a three-member cluster: {}",
        stdout_of(&before)
    );

    assert_eq!(
        after.status.code(),
        Some(1),
        "the surviving pair stopped being a finding, so the glob matched by basename and took \
         `keep/copy.py` with it: {after:?}"
    );
    assert!(
        stdout_of(&after).contains("in 2 places"),
        "expected the two survivors to still cluster: {}",
        stdout_of(&after)
    );
    assert!(
        stdout_of(&after).contains("keep/copy.py"),
        "a file sharing only its BASENAME with the excluded path was excluded too: {}",
        stdout_of(&after)
    );
    assert!(
        !stdout_of(&after).contains("vendor"),
        "the excluded file is still being measured: {}",
        stdout_of(&after)
    );
}

/// AC3 — the relative base is the configuration file's directory even when the walk starts below
/// it, which is the case a lexical answer gets wrong.
///
/// `cd pkg && tooprolix check .` walks a tree whose entries the walker names `./generated/g.py`.
/// Relative to the *configuration file* that same file is `pkg/generated/g.py`, and that is the
/// name the glob is written against — one project, one rule, whatever directory CI happens to be
/// standing in. Nothing normalises `./generated/g.py` into `pkg/generated/g.py` by accident.
#[test]
fn exclude_means_the_same_thing_from_a_subdirectory_as_from_the_project_root() {
    // Arrange
    let scratch = Scratch::new("exclude-subdir");
    scratch.write("pkg/generated/g.py", SHARED_RATIONALE);
    scratch.write("pkg/keep.py", SHARED_RATIONALE);
    scratch.write("pkg/anchor.py", SHARED_RATIONALE);
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"pkg/generated\"]\n",
    );

    // Act — the same rule, reached from two different working directories.
    let from_root = scratch.check(&[]);
    let from_pkg = scratch.check_from("pkg", ".");

    // Assert
    assert_eq!(from_root.status.code(), Some(1), "{from_root:?}");
    assert!(
        stdout_of(&from_root).contains("in 2 places")
            && !stdout_of(&from_root).contains("generated"),
        "the rule does not even hold from the project root: {}",
        stdout_of(&from_root)
    );

    assert_eq!(from_pkg.status.code(), Some(1), "{from_pkg:?}");
    assert!(
        !stdout_of(&from_pkg).contains("generated"),
        "the same rule stopped applying one directory down, so the glob was resolved against the \
         working directory rather than against the configuration file: {}",
        stdout_of(&from_pkg)
    );
    assert!(
        stdout_of(&from_pkg).contains("in 2 places"),
        "expected the two survivors to still cluster from the subdirectory: {}",
        stdout_of(&from_pkg)
    );
}

/// The anchor `./x` emits is relative to the CONFIGURATION FILE, not to wherever the walk started.
///
/// The sibling test above uses `pkg/generated`, whose internal `/` passes through normalisation
/// untouched, so it never exercises the leading `/` that `./x` and `x/` now produce. This is the
/// invariant that breaks first if `exclude_matcher`'s base is ever rebased onto the walk root:
/// `./generated` would then start eating `pkg/generated` the moment CI ran from `pkg`.
#[test]
fn an_anchored_entry_stays_anchored_to_the_configuration_file_from_a_subdirectory() {
    // Arrange — the same name at the root and inside the package, so a rebased anchor is visible.
    let scratch = Scratch::new("exclude-anchor-subdir");
    scratch.write("generated/top.py", &long_comment("top level codegen"));
    scratch.write("pkg/generated/g.py", &long_comment("package codegen"));
    scratch.write("pkg/keep.py", &long_comment("retry policy"));

    for entry in ["./generated", "generated/"] {
        scratch.write(
            "pyproject.toml",
            &format!("[tool.tooprolix]\nexclude = [\"{entry}\"]\n"),
        );

        // Act — the same rule, reached from the root and from inside the package.
        let from_root = scratch.check(&[]);
        let from_pkg = scratch.check_from("pkg", ".");

        // Assert — from the root the anchor selects the top-level one and nothing deeper.
        assert!(
            !stdout_of(&from_root).contains("top.py")
                && stdout_of(&from_root).contains("g.py")
                && stdout_of(&from_root).contains("keep.py"),
            "`{entry}` from the root did not select exactly the top-level `generated`: {}",
            stdout_of(&from_root)
        );
        // ... and starting the walk BELOW the base does not re-anchor it there.
        assert!(
            stdout_of(&from_pkg).contains("g.py") && stdout_of(&from_pkg).contains("keep.py"),
            "`{entry}` swallowed `pkg/generated` when the walk started at `pkg`, so the anchor was \
             resolved against the walk root instead of the configuration file: {}",
            stdout_of(&from_pkg)
        );
    }
}

/// AC3 — `exclude` is a second layer over `.gitignore`, not a replacement for it.
///
/// The one-line version of the defect this forbids: switching the walk over to an "include only
/// what the overrides allow" matcher, which is what the underlying crate does by default and which
/// would make a `.gitignore`'d file reappear the moment a project set `exclude`. Both files are
/// asserted absent and the tree is proved capable of reporting them.
#[test]
fn exclude_adds_to_gitignore_rather_than_replacing_it() {
    // Arrange
    let scratch = Scratch::new("exclude-gitignore");
    scratch.write("ignored/copy.py", SHARED_RATIONALE);
    scratch.write("vendor/copy.py", SHARED_RATIONALE);
    scratch.write("anchor.py", SHARED_RATIONALE);

    // Act
    let bare = scratch.check(&[]);
    scratch.write(".gitignore", "ignored/\n");
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor\"]\n",
    );
    let both_layers = scratch.check(&[]);

    // Assert
    assert!(
        stdout_of(&bare).contains("in 3 places"),
        "the fixture cannot demonstrate two exclusions it never triggers: {}",
        stdout_of(&bare)
    );

    assert_eq!(both_layers.status.code(), Some(0), "{both_layers:?}");
    assert!(
        !stdout_of(&both_layers).contains("ignored/"),
        "setting `exclude` resurrected a .gitignore'd file — the override REPLACED the gitignore \
         layer instead of adding to it: {}",
        stdout_of(&both_layers)
    );
    assert!(
        !stdout_of(&both_layers).contains("vendor"),
        "the excluded file was still measured: {}",
        stdout_of(&both_layers)
    );
}

/// AC3 — a path named **directly** on the command line is checked even when a glob
/// excludes it, which is ruff's own default.
///
/// The reasoning is that `exclude` describes what a *walk* should not wander into, and a user who
/// types a path has already answered the question the walk was asking. The alternative — silently
/// exiting 0 on a file the user explicitly asked about — is the "measured nothing, said nothing"
/// class this project keeps closing.
#[test]
fn an_explicitly_named_path_is_checked_even_when_it_is_excluded() {
    // Arrange
    let scratch = Scratch::new("exclude-explicit");
    scratch.write("vendor/loud.py", &long_comment("vendored retry policy"));
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor\"]\n",
    );

    // Act
    let walked = scratch.check(&[]);
    let named_file = scratch.check_from("", "vendor/loud.py");
    let named_directory = scratch.check_from("", "vendor");

    // Assert — the walk skips it ...
    assert_eq!(walked.status.code(), Some(0), "{walked:?}");
    assert!(
        stderr_of(&walked).contains("excluded"),
        "a walk that measured nothing must say why: {:?}",
        stderr_of(&walked)
    );

    // ... and naming it does not.
    assert_eq!(
        named_file.status.code(),
        Some(1),
        "an explicitly named file was silently skipped: {named_file:?}"
    );
    assert!(stdout_of(&named_file).contains("TPX001"));
    assert_eq!(
        named_directory.status.code(),
        Some(1),
        "an explicitly named directory was silently skipped: {named_directory:?}"
    );
    assert!(stdout_of(&named_directory).contains("TPX001"));
}

/// AC3 — `exclude` does not switch symlink following back on.
///
/// Measured on pydantic in epic 1: following links takes 343 findings to 559, because the same
/// sources are reached twice. That invariant belongs to the walk, and adding a second filtering
/// layer to the walk is exactly the kind of change that quietly rebuilds it with different options.
#[test]
#[cfg(unix)]
fn exclude_does_not_make_the_walk_follow_symlinks() {
    // Arrange
    let scratch = Scratch::new("exclude-symlink");
    scratch.write("pkg/a.py", &long_comment("retry policy"));
    scratch.write("vendor/skipped.py", SHARED_RATIONALE);
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor\"]\n",
    );
    std::os::unix::fs::symlink(scratch.root.join("pkg"), scratch.root.join("alias"))
        .expect("a symlink is creatable");

    // Act
    let output = scratch.check(&[]);

    // Assert — one file behind two names is still one file.
    let lines: Vec<&str> = stdout_of(&output).lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "stdout must be exactly one finding and one summary: {lines:#?}"
    );
    assert!(lines[0].starts_with("./pkg/a.py") && lines[0].contains(": TPX001 "));
    assert_eq!(lines[1], "Found 1 findings (TPX001: 1).");
    assert!(
        !stdout_of(&output).contains("alias"),
        "the walk descended through a symlink: {}",
        stdout_of(&output)
    );
}

/// Red team — a glob that matches the WHOLE tree must not be a silent green.
///
/// This is the defect class epic 1 shipped once already: a run that measured nothing scores every
/// repository clean, and exit 0 is indistinguishable from an earned one. The exit code stays 0
/// because it is honest — there really are no findings — but the run has to say on stderr that it
/// measured nothing, and it has to name `exclude` as the reason rather than blaming an absence of
/// Python files that is not what happened.
#[test]
fn excluding_the_whole_tree_says_so_rather_than_scoring_it_clean() {
    // Arrange
    let scratch = Scratch::new("exclude-everything");
    scratch.write("a.py", &long_comment("retry policy"));
    scratch.write("b.py", SHARED_RATIONALE);

    // Act
    let before = scratch.check(&[]);
    scratch.write("pyproject.toml", "[tool.tooprolix]\nexclude = [\"*\"]\n");
    let after = scratch.check(&[]);

    // Assert
    assert_eq!(
        before.status.code(),
        Some(1),
        "the fixture has nothing to lose: {before:?}"
    );

    // The success line prints here too, and the loud stderr below is what stops it being a claim
    // about a tree nobody looked at. `exclude` is a boundary the project drew (EPIC Decisions
    // #15), so inside it the measurement really is whole and `complete` is `true` — the run is
    // `Success` by the same definition as any other. Ruff answers identically on a fully-excluded
    // tree: the warning on stderr, `All checks passed!` on stdout, exit 0.
    assert_eq!(stdout_of(&after), CLEAN_STDOUT);
    assert!(
        !stderr_of(&after).is_empty(),
        "every file in the tree was excluded and the run said nothing at all"
    );
    // Not merely "stderr mentions exclude" — that was satisfied by a clause driven off the
    // configuration's own say-so, which said the same thing for a glob that matched nothing. The
    // claim under test is the one only the walk can make: how many measurable paths it removed.
    // The fixture has exactly two files and `*` takes both, so the number is checked and not just
    // its presence — a hardcoded 1, or a count of globs rather than of removals, is visible here.
    assert!(
        stderr_of(&after).contains("removed 2 path(s) that could have been measured"),
        "the diagnostic blames something other than the `exclude` that actually caused it, or \
         reports the rule's mere presence rather than its measured effect: {:?}",
        stderr_of(&after)
    );
    assert!(
        stderr_of(&after).contains("pyproject.toml"),
        "the diagnostic does not name the file the rule came from: {:?}",
        stderr_of(&after)
    );
}

/// An `exclude` entry the tool cannot act on is fatal, and never a silent "excluded nothing".
///
/// Measured against the underlying crate, and this is why the list is not decorative:
///
/// | entry | what the crate does with it, unguarded |
/// |---|---|
/// | `""` | accepted, builds an **empty** matcher — excludes nothing, silently |
/// | `" "` | the same |
/// | `"!vendor"` | accepted, and excludes **nothing** — the negation cancels ours |
/// | `"a["` | a parse error, which is the only one of the four that was already loud |
///
/// Two of those fail open and one is meaningless, and all three look exactly like a rule that
/// works. A gate that can be switched off by a typo in its own configuration is not a gate.
#[test]
fn a_malformed_exclude_entry_is_fatal_rather_than_quietly_doing_nothing() {
    let scratch = Scratch::new("exclude-malformed");
    scratch.write("a.py", &long_comment("retry policy"));

    for (table, expected) in [
        ("[tool.tooprolix]\nexclude = [\"\"]\n", "empty"),
        ("[tool.tooprolix]\nexclude = [\"   \"]\n", "empty"),
        ("[tool.tooprolix]\nexclude = [\"!vendor\"]\n", "!"),
        ("[tool.tooprolix]\nexclude = [\"a[\"]\n", "a["),
        ("[tool.tooprolix]\nexclude = \"vendor\"\n", "string"),
        ("[tool.tooprolix]\nexclude = [3]\n", "integer"),
    ] {
        scratch.write("pyproject.toml", table);

        let output = scratch.check(&[]);

        assert_eq!(
            output.status.code(),
            Some(2),
            "{table:?} was accepted and excluded nothing: {output:?}"
        );
        assert_eq!(stdout_of(&output), "", "{table:?} still printed findings");
        assert!(
            stderr_of(&output).contains(expected),
            "{table:?}: the message does not name `{expected}`: {:?}",
            stderr_of(&output)
        );
        assert!(
            stderr_of(&output).contains("exclude") && stderr_of(&output).contains("pyproject.toml"),
            "{table:?}: the message names neither the key nor the file: {:?}",
            stderr_of(&output)
        );
    }
}

/// AC4 — an unknown key is still fatal, and the message still lists what *is* known.
///
/// The regression this guards is specific: adding `exclude` grows the known-key list, and a list
/// written down in a second place is a list that drifts. If the rejection ever consults a
/// hardcoded set that nobody updated, `exclude` itself becomes the unknown key — so the test
/// asserts both directions on one run.
#[test]
fn an_unknown_key_is_still_fatal_and_the_known_list_now_carries_exclude() {
    // Arrange
    let scratch = Scratch::new("exclude-unknown-key");
    scratch.write("a.py", SHARED_RATIONALE);
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexcludes = [\"vendor\"]\n",
    );

    // Act
    let output = scratch.check(&[]);

    // Assert
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(stdout_of(&output), "");
    assert!(
        stderr_of(&output).contains("unknown key") && stderr_of(&output).contains("excludes"),
        "a near-miss key was accepted or not named: {:?}",
        stderr_of(&output)
    );
    assert!(
        stderr_of(&output).contains("exclude,") || stderr_of(&output).contains(", exclude"),
        "the known-key list does not advertise `exclude`, so the two lists have drifted: {:?}",
        stderr_of(&output)
    );
}

/// The negation guard has to look at the entry the way the *walker* will, not at position zero.
///
/// `starts_with('!')` inspected byte 0 only, so ` !vendor` walked straight past it, became
/// `! !vendor` one layer down, and excluded a literal space-prefixed path — which is to say
/// nothing at all, silently. That is the exact no-op the guard exists to prevent, reached by
/// typing one space. It is also the epic's "enumerate positions, not just inputs" lesson landing
/// inside a guard written *because* of that lesson: the original probe only ever tested position 0.
///
/// The decision this pins is that an entry is **trimmed and then judged**, so surrounding
/// whitespace can neither smuggle a `!` past the guard nor turn a working glob into a silent
/// no-op. The second half is the one users actually hit, and it is asserted here too: ` vendor`
/// must exclude `vendor`, not a directory whose name begins with a space.
#[test]
fn surrounding_whitespace_cannot_smuggle_a_negation_past_the_guard() {
    let scratch = Scratch::new("exclude-whitespace");
    scratch.write("app.py", "\"\"\"Short.\"\"\"\n");
    scratch.write("vendor/fat.py", &long_comment("vendored retry policy"));

    // Every spelling of a negated entry is refused, wherever the whitespace sits.
    for entry in [" !vendor", "!vendor ", "\t!vendor", "  !vendor  "] {
        scratch.write(
            "pyproject.toml",
            &format!("[tool.tooprolix]\nexclude = [\"{entry}\"]\n"),
        );

        let output = scratch.check(&[]);

        assert_eq!(
            output.status.code(),
            Some(2),
            "`{entry}` slipped past the negation guard and excluded nothing, silently: {output:?}"
        );
        assert!(
            stderr_of(&output).contains('!') && stderr_of(&output).contains("exclude"),
            "`{entry}`: the message does not name the problem: {:?}",
            stderr_of(&output)
        );
    }

    // ... and a stray space around a REAL glob is still that glob, not a no-op.
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"  vendor  \"]\n",
    );
    let padded = scratch.check(&[]);

    assert_eq!(
        padded.status.code(),
        Some(0),
        "a glob with surrounding whitespace excluded nothing: {padded:?}"
    );
    assert_eq!(
        stdout_of(&padded),
        CLEAN_STDOUT,
        "the padded glob did not reach the walk: {}",
        stdout_of(&padded)
    );
}

/// The empty-walk diagnostic must report what the **walk** did, not what the config *says*.
///
/// The clause was driven by `!config.exclude.is_empty()` — a field of the configuration — and
/// announced as a fact about the walk. An empty directory whose config excludes a `vendor` that
/// does not exist anywhere therefore blamed `exclude` for an emptiness it had no part in, sending
/// the reader hunting for excluded files that were never there. That is the same wrong-cause
/// failure the clause was added to fix, one level in: the original "no Python files" message
/// blamed an absence, and this blamed a rule that never fired.
///
/// Both directions are asserted on one fixture, because either alone passes on a build that always
/// prints the clause or never does.
#[test]
fn the_empty_walk_diagnostic_blames_exclude_only_when_exclude_actually_excluded_something() {
    // Arrange — one tree, one glob, and the only difference is whether anything matches it.
    let scratch = Scratch::new("exclude-blame");
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor\"]\n",
    );

    // Act — nothing named `vendor` exists, so the walk is empty for a reason `exclude` did not
    // cause ...
    let nothing_matched = scratch.check(&[]);
    // ... and now it does exist, and is the whole of the tree's Python.
    scratch.write("vendor/fat.py", &long_comment("vendored retry policy"));
    let everything_matched = scratch.check(&[]);

    // Assert
    assert_eq!(
        nothing_matched.status.code(),
        Some(0),
        "{nothing_matched:?}"
    );
    assert!(
        stderr_of(&nothing_matched).contains("no Python files"),
        "an empty walk must still say it measured nothing: {:?}",
        stderr_of(&nothing_matched)
    );
    assert!(
        !stderr_of(&nothing_matched).contains("exclude"),
        "`exclude` was blamed for an emptiness it had no part in — nothing in this tree ever \
         matched `vendor`: {:?}",
        stderr_of(&nothing_matched)
    );

    assert_eq!(
        everything_matched.status.code(),
        Some(0),
        "{everything_matched:?}"
    );
    assert!(
        stderr_of(&everything_matched).contains("exclude"),
        "`exclude` really did empty this walk and the run did not say so: {:?}",
        stderr_of(&everything_matched)
    );
}

/// The empty-walk diagnostic may name only what the walk itself observed — third pass over one
/// sentence, and the defect has changed shape twice.
///
/// Version one gated on `config.exclude` being non-empty. Version two moved the *gate* onto the
/// walk but left the parenthesised body as `config.exclude.iter()` — every configured glob, firing
/// or not — so a tree whose only removal was `vendor` still announced `absent-glob` as having
/// removed paths. The gate became honest and the sentence did not.
///
/// Two removals are asserted here and they are deliberately different: a glob that fires and one
/// that cannot, in the same run, so a message built from the configuration and a message built
/// from the walk produce visibly different strings. A body driven by the configuration also
/// reddens `an_absent_glob_is_never_named`, below, which is the half that isolates it.
#[test]
fn the_empty_walk_diagnostic_names_nothing_the_walk_did_not_observe() {
    // Arrange — `vendor/` holds the tree's only Python; `absent-glob` matches nothing at all.
    let scratch = Scratch::new("exclude-attribution");
    scratch.write("vendor/fat.py", &long_comment("vendored retry policy"));
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"absent-glob\", \"vendor\"]\n",
    );

    // Act
    let output = scratch.check(&[]);

    // Assert — the run still says `exclude` emptied it ...
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        stderr_of(&output).contains("exclude"),
        "`exclude` really did empty this walk and the run did not say so: {:?}",
        stderr_of(&output)
    );
    // ... and does not credit a glob that never matched anything.
    assert!(
        !stderr_of(&output).contains("absent-glob"),
        "a glob that matched nothing is named as having removed paths — the sentence is built \
         from the configuration rather than from the walk: {:?}",
        stderr_of(&output)
    );
}

/// A removal that could never have been measured is not a reason to say the tree was not measured.
///
/// The flag observed "a path was removed"; the sentence claimed "measurement is incomplete". Those
/// are different facts, and `README.md` is where they come apart: excluding it removes a path the
/// tool would never have read, so nothing measurable was lost and there is nothing to warn about.
/// `is_python_source` already answers this for a file. A pruned *directory* is the case where the
/// answer is genuinely unknowable without descending — that is the pruning the walk must keep — so
/// a directory stays conservative and still counts.
#[test]
fn removing_something_that_was_never_measurable_is_not_an_incomplete_measurement() {
    // Arrange — no Python anywhere, so the walk is empty either way and only the REASON differs.
    let scratch = Scratch::new("exclude-unmeasurable");
    scratch.write("notes.txt", "not python\n");
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"notes.txt\"]\n",
    );

    // Act
    let unmeasurable = scratch.check(&[]);
    // ... and the directory case, which stays conservative because it cannot be known.
    scratch.write("vendor/thing.py", "\"\"\"Short.\"\"\"\n");
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor\"]\n",
    );
    let pruned_directory = scratch.check(&[]);

    // Assert
    assert_eq!(unmeasurable.status.code(), Some(0), "{unmeasurable:?}");
    assert!(
        stderr_of(&unmeasurable).contains("no Python files"),
        "an empty walk must still say it measured nothing: {:?}",
        stderr_of(&unmeasurable)
    );
    assert!(
        !stderr_of(&unmeasurable).contains("exclude"),
        "excluding a file the tool would never have read was reported as an incomplete \
         measurement: {:?}",
        stderr_of(&unmeasurable)
    );

    assert!(
        stderr_of(&pruned_directory).contains("exclude"),
        "a pruned directory might have held Python and must keep the conservative claim: {:?}",
        stderr_of(&pruned_directory)
    );
}

/// A bare name matches at any depth; a spelling that carries a `/` names the root directory only.
///
/// Measured against ruff 0.16.0, which is the contract: `vendor` excludes both depths, `vendor/`
/// and `./vendor` exclude only the directory beside the configuration file. The old version of this
/// test had one root `vendor/` and so could not tell those two apart — it asserted every spelling
/// was one rule, which the measurement disproves.
///
/// Separator noise still collapses (`.//x`, `././x`), because that was found by somebody typing it.
#[test]
fn each_exclude_spelling_selects_the_depth_it_names() {
    // Arrange — the same directory name at the root and one level down, one TPX001 in each, so the
    // two depths are distinguishable in one run.
    let scratch = Scratch::new("exclude-depth");
    scratch.write("vendor/root_only.py", &long_comment("root retry policy"));
    scratch.write(
        "sub/vendor/nested_too.py",
        &long_comment("nested retry policy"),
    );

    // Act — first WITHOUT the key, because half the table below proves exclusion by ABSENCE. On a
    // build where `long_comment` stopped producing a finding, every `root_excluded == true` row
    // would pass while the matcher did nothing.
    let baseline = scratch.check(&[]);
    for fixture in ["root_only.py", "nested_too.py"] {
        assert!(
            stdout_of(&baseline).contains(fixture),
            "`{fixture}` is not reported even with no `exclude` at all, so every absence asserted \
             below proves nothing: {:?}",
            stdout_of(&baseline)
        );
    }

    for (entry, root_excluded, nested_excluded) in [
        ("vendor", true, true),
        ("vend*r", true, true),
        ("vendor/", true, false),
        ("./vendor", true, false),
        (".//vendor", true, false),
        ("././vendor", true, false),
        ("./vendor/", true, false),
    ] {
        scratch.write(
            "pyproject.toml",
            &format!("[tool.tooprolix]\nexclude = [\"{entry}\"]\n"),
        );

        let output = scratch.check(&[]);

        assert_ne!(
            output.status.code(),
            Some(2),
            "`{entry}` is a supported spelling and was refused: {:?}",
            stderr_of(&output)
        );
        assert_eq!(
            !stdout_of(&output).contains("root_only.py"),
            root_excluded,
            "`{entry}`: the ROOT `vendor` should{} have been excluded: {:?}",
            if root_excluded { "" } else { " NOT" },
            stdout_of(&output)
        );
        assert_eq!(
            !stdout_of(&output).contains("nested_too.py"),
            nested_excluded,
            "`{entry}`: the NESTED `sub/vendor` should{} have been excluded: {:?}",
            if nested_excluded { "" } else { " NOT" },
            stdout_of(&output)
        );
    }
}

/// A leading `/` is refused, and the message hands back the spelling that works.
///
/// It reads as "anchor to the project root", and ruff 0.16.0 gives it two opposite meanings
/// depending on the shape — measured, `/vendor` excludes nothing there and `/*.py` excludes
/// everything — so there is no one reading to copy and the class is refused. It is a separate test
/// from the `..` table because it is a different verdict: `..` can never match anything, `/vendor`
/// matches fine and is refused on contract grounds, so it carries assertions the `..` entries
/// do not have.
#[test]
fn a_leading_slash_is_refused_and_names_the_spelling_that_works() {
    let scratch = Scratch::new("exclude-leading-slash");
    scratch.write("vendor/root_only.py", &long_comment("root retry policy"));

    // The advice is the user's OWN entry minus the slash, never a string rebuilt from its parts —
    // a rebuilt one dropped the trailing `/`, and `./generated.py` is a WIDER rule than
    // `./generated.py/`: it also eats the file of that name, silently shrinking the denominator.
    for (entry, advice) in [
        ("/vendor", "./vendor"),
        ("/vendor/", "./vendor/"),
        ("/./vendor", "./vendor"),
        ("/vendor/generated", "./vendor/generated"),
    ] {
        scratch.write(
            "pyproject.toml",
            &format!("[tool.tooprolix]\nexclude = [\"{entry}\"]\n"),
        );

        let output = scratch.check(&[]);

        assert_eq!(
            output.status.code(),
            Some(2),
            "`{entry}` anchors in a way ruff reads inconsistently and was accepted anyway: \
             {output:?}"
        );
        let stderr = stderr_of(&output);
        assert!(
            stderr.contains("exclude") && stderr.contains("pyproject.toml"),
            "`{entry}`: the message names neither the key nor the file: {stderr:?}"
        );
        // The whole sentence AND the end of the message. `contains("./vendor")` was also satisfied
        // by `././vendor`, so the double-hop row could not fail when the advice regressed to
        // exactly that; `ends_with` additionally refuses a second clause appended after the advice,
        // which is how the false any-depth promise got in. An assertion a longer wrong string still
        // passes is not an assertion.
        assert!(
            stderr
                .trim_end()
                .ends_with(&format!("write `{advice}` instead")),
            "`{entry}`: the message does not end by advising exactly `{advice}`, the spelling that \
             does what the user meant: {stderr:?}"
        );
        // `vendor/generated` excludes only the root one — a glob carrying a `/` is matched as a
        // whole base-relative path, so an any-depth clause is false for every multi-component name.
        assert!(
            !stderr.contains("any depth"),
            "`{entry}`: the message promises an any-depth spelling that does not match at any \
             depth: {stderr:?}"
        );
    }
}

/// Advice is only given when it can be taken: a rejected entry is never offered a rejected fix.
///
/// `/!vendor` trips the leading-slash refusal first, and the two spellings that refusal would
/// normally recommend — `./!vendor` and `!vendor` — are both refused a few lines later by the
/// negation guard. Sending the user from one exit 2 to a different exit 2 is worse than saying
/// nothing, so the concrete advice is withheld for exactly the names the later guards reject.
#[test]
fn a_refusal_never_recommends_a_spelling_that_is_also_refused() {
    let scratch = Scratch::new("exclude-unhelpable");
    scratch.write("app.py", &long_comment("retry policy"));

    // The premise: both spellings the message would otherwise name really are refused.
    for rejected in ["./!vendor", "!vendor"] {
        scratch.write(
            "pyproject.toml",
            &format!("[tool.tooprolix]\nexclude = [\"{rejected}\"]\n"),
        );
        assert_eq!(
            scratch.check(&[]).status.code(),
            Some(2),
            "`{rejected}` is accepted, so this test no longer pins anything"
        );
    }

    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"/!vendor\"]\n",
    );
    let output = scratch.check(&[]);
    let stderr = stderr_of(&output);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    for promise in ["./!vendor", "`!vendor`"] {
        assert!(
            !stderr.contains(promise),
            "the refusal tells the user to write `{promise}`, which the next guard also refuses — \
             one exit 2 handing off to another: {stderr:?}"
        );
    }
}

/// A pattern that cannot match anything under the configuration file is refused, not ignored.
///
/// `..` walks out of the tree the base names, so no path the walk can ever yield will match it —
/// measured, it is a no-op. Silence there is the same class as the empty glob and the negated one:
/// a rule that reads as working and is not. Rejecting is the only answer that cannot be mistaken
/// for a rule that fired.
#[test]
fn an_exclude_entry_that_can_never_match_is_refused() {
    let scratch = Scratch::new("exclude-unmatchable");
    scratch.write("app.py", &long_comment("retry policy"));

    for entry in ["../vendor", "vendor/../other", "..", "."] {
        scratch.write(
            "pyproject.toml",
            &format!("[tool.tooprolix]\nexclude = [\"{entry}\"]\n"),
        );

        let output = scratch.check(&[]);

        assert_eq!(
            output.status.code(),
            Some(2),
            "`{entry}` can never match and was accepted anyway: {output:?}"
        );
        assert!(
            stderr_of(&output).contains("exclude") && stderr_of(&output).contains("pyproject.toml"),
            "`{entry}`: the message names neither the key nor the file: {:?}",
            stderr_of(&output)
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Graceful degradation — a file that cannot be read is a diagnostic, not a refusal.
//
// This is the ruff path, taken deliberately and against this crate's own 0.1.0 decision: a file
// that does not parse used to take the whole run down (exit 2, zero findings, the findings of every
// file that DID parse withheld). The reversal was reserved from the start — "strictness is later
// relaxed without breaking the contract, never the other way round" — and it is what makes the tool
// usable on a repository that legitimately holds invalid Python without configuring anything.
//
// The single condition under which it is acceptable, and the thing every test below exists to hold:
// **a partial run NEVER exits 0.** A tree that was not fully read must not be able to look green.
// The exit code alone cannot say WHY it is 1 — "prose is bad" and "measurement is incomplete" are
// now the same number — so completeness moves into the machine-readable channel, which is why the
// JSON schema went to version 2 in the same change.
//
// `skipped` and `excluded` are two channels and never one: `skipped` is a REFUSAL (the tool tried to
// read the file and could not), `excluded` is a BOUNDARY (the project said not to look). Only the
// first makes a run incomplete.
// ---------------------------------------------------------------------------------------------

/// The unparsable file this section uses everywhere, and it must be unparsable for a *syntax*
/// reason rather than because it is missing — otherwise the walk never yields it at all.
const UNPARSABLE: &str = "def settle(:\n    pass\n";

/// Parses the JSON document on stdout, failing loudly with the bytes if it is not one.
fn document_of(output: &Output) -> serde_json::Value {
    serde_json::from_str(stdout_of(output)).unwrap_or_else(|error| {
        panic!(
            "stdout is not one JSON document ({error}): {:?} / stderr {:?}",
            stdout_of(output),
            stderr_of(output)
        )
    })
}

/// AC1 — the finding of a file that parsed survives a sibling that did not.
///
/// This is the exact inversion of what 0.1.0 guaranteed, and both halves are asserted in both
/// formats because either one alone is satisfiable by the wrong build: printing the finding without
/// naming the broken file is silently partial, and naming the broken file without printing the
/// finding is the old behaviour with a friendlier message.
#[test]
fn a_broken_file_no_longer_hides_the_findings_of_the_files_that_parsed() {
    // Arrange
    let scratch = Scratch::new("graceful-ac1");
    scratch.write("broken.py", UNPARSABLE);
    scratch.write("fat.py", &long_comment("retry policy"));

    // Act
    let text = scratch.check(&[]);
    let json = scratch.check(&["--format", "json"]);

    // Assert — the finding is printed ...
    assert_eq!(text.status.code(), Some(1), "{text:?}");
    assert!(
        stdout_of(&text).contains("fat.py:1-20: TPX001"),
        "the finding of the file that parsed was withheld: {:?}",
        stdout_of(&text)
    );
    assert!(
        stdout_of(&text)
            .ends_with("Found 1 findings (TPX001: 1); check incomplete: 1 file skipped.\n"),
        "the partial finding aggregate lost its count or incomplete suffix: {:?}",
        stdout_of(&text)
    );
    // ... and the file that did not parse is named, with the reason.
    assert!(
        stderr_of(&text).contains("1 file(s) skipped:") && stderr_of(&text).contains("broken.py"),
        "the skipped file is not named: {:?}",
        stderr_of(&text)
    );
    assert!(
        stderr_of(&text).contains("could not parse Python source"),
        "the skipped file is named without a reason: {:?}",
        stderr_of(&text)
    );

    // The JSON half is the same run through the other format, not a different claim.
    assert_eq!(json.status.code(), Some(1), "{json:?}");
    let document = document_of(&json);
    assert_eq!(document["schema_version"], "2");
    assert_eq!(document["complete"], false);
    assert_eq!(document["findings"][0]["code"], "TPX001");
    assert_eq!(
        document["skipped"].as_array().map(Vec::len),
        Some(1),
        "{document}"
    );
    assert!(
        document["skipped"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("broken.py")),
        "{document}"
    );
    assert!(
        stderr_of(&json).contains("broken.py"),
        "the JSON run said nothing on stderr about the file it could not read: {:?}",
        stderr_of(&json)
    );
}

/// **The guarantee the whole reversal was accepted on: a partial run never exits 0.**
///
/// The fixture is built so that the only readable file has no findings at all. The text answer must
/// say both facts — no reachable findings and an incomplete check — while the exit code remains the
/// load-bearing guarantee.
///
/// Asserted on the code and not merely on `!= 0`: 2 would mean the reversal never happened.
#[test]
fn a_partial_run_never_exits_zero_even_with_nothing_to_report() {
    // Arrange
    let scratch = Scratch::new("graceful-never-zero");
    scratch.write("broken.py", UNPARSABLE);
    scratch.write("clean.py", "\"\"\"Short.\"\"\"\n");

    // Act
    let output = scratch.check(&[]);
    let json = scratch.check(&["--format", "json"]);

    // Assert
    assert_eq!(
        output.status.code(),
        Some(1),
        "a tree that was not fully read reported itself measured and clean: {output:?}"
    );
    assert_eq!(
        stdout_of(&output),
        "No findings; check incomplete: 1 file skipped.\n",
        "the incomplete no-findings summary drifted: {:?}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("broken.py"),
        "exit 1 with no finding and no reason is unreadable: {:?}",
        stderr_of(&output)
    );

    assert_eq!(json.status.code(), Some(1), "{json:?}");
    let document = document_of(&json);
    assert_eq!(
        document["complete"], false,
        "the machine-readable completeness channel says the tree was complete: \
         {document}"
    );
    assert_eq!(document["findings"].as_array().map(Vec::len), Some(0));
}

/// 0 of N files readable is the extreme of the same case, and it must not look successful.
///
/// A run where *nothing* was read has the strongest claim to being reported as unmeasured, and it is
/// also the shape most likely to fall out of a loop that only reports when it has something.
#[test]
fn a_tree_where_no_file_could_be_read_does_not_look_successful() {
    // Arrange
    let scratch = Scratch::new("graceful-none-readable");
    scratch.write("a.py", UNPARSABLE);
    scratch.write("b.py", UNPARSABLE);

    // Act
    let output = scratch.check(&[]);
    let json = scratch.check(&["--format", "json"]);

    // Assert
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        stdout_of(&output),
        "No findings; check incomplete: 2 files skipped.\n"
    );
    assert!(
        stderr_of(&output).contains("2 file(s) skipped:"),
        "{:?}",
        stderr_of(&output)
    );
    for name in ["a.py", "b.py"] {
        assert!(
            stderr_of(&output).contains(name),
            "{name} was skipped in silence: {:?}",
            stderr_of(&output)
        );
    }

    let document = document_of(&json);
    assert_eq!(document["complete"], false);
    assert_eq!(document["skipped"].as_array().map(Vec::len), Some(2));
}

/// AC3 — `complete` answers in both directions, and all three fields are present either way.
///
/// A field that only appears when it is `false` is worse than no field: its absence becomes
/// indistinguishable from "fully measured" for every consumer that does not know to look.
#[test]
fn the_json_document_carries_completeness_in_both_directions() {
    // Arrange
    let scratch = Scratch::new("graceful-complete");
    scratch.write("fat.py", &long_comment("retry policy"));

    // Act — the complete run first, so the partial one is a change and not a constant.
    let complete = scratch.check(&["--format", "json"]);
    scratch.write("broken.py", UNPARSABLE);
    let partial = scratch.check(&["--format", "json"]);

    // Assert
    let whole = document_of(&complete);
    assert_eq!(complete.status.code(), Some(1), "{complete:?}");
    assert_eq!(whole["schema_version"], "2");
    assert_eq!(whole["complete"], true);
    assert_eq!(
        whole["skipped"].as_array().map(Vec::len),
        Some(0),
        "`skipped` must be present and empty on a complete run, never absent: {whole}"
    );
    assert_eq!(
        whole["excluded"].as_array().map(Vec::len),
        Some(0),
        "`excluded` must be present and empty when nothing is excluded, never absent: {whole}"
    );
    assert_eq!(whole["findings"].as_array().map(Vec::len), Some(1));

    let part = document_of(&partial);
    assert_eq!(part["complete"], false);
    assert_eq!(part["skipped"].as_array().map(Vec::len), Some(1));
    assert!(
        part["skipped"][0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("could not parse")),
        "the machine-readable channel carries a path with no reason: {part}"
    );
    assert_eq!(
        part["findings"].as_array().map(Vec::len),
        Some(1),
        "the finding of the readable file vanished from the document: {part}"
    );
}

/// AC3 — a partial run is byte-identical across two invocations, in both formats.
///
/// The lists are the new risk: `skipped` and `excluded` are filled from a filesystem walk, whose
/// order is not a guarantee on any filesystem, so an unsorted list would flap between runs on a tree
/// large enough for the directory order to differ. Compared on NON-EMPTY output, because two empty
/// strings are byte-identical for the wrong reason.
#[test]
fn a_partial_run_is_reproducible_byte_for_byte() {
    // Arrange — enough files, broken and excluded alike, that an unsorted list can visibly disagree.
    let scratch = Scratch::new("graceful-determinism");
    for name in ["zulu", "alpha", "mike", "bravo", "yankee", "charlie"] {
        scratch.write(&format!("{name}.py"), UNPARSABLE);
        scratch.write(&format!("vendor/{name}.py"), UNPARSABLE);
    }
    scratch.write("fat.py", &long_comment("retry policy"));
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor/*.py\"]\n",
    );

    // Act
    let text = (scratch.check(&[]), scratch.check(&[]));
    let json = (
        scratch.check(&["--format", "json"]),
        scratch.check(&["--format", "json"]),
    );

    // Assert — the outputs are non-empty first, or "identical" is worth nothing.
    assert!(!stdout_of(&text.0).is_empty() && !stderr_of(&text.0).is_empty());
    assert_eq!(stdout_of(&text.0), stdout_of(&text.1));
    assert_eq!(stderr_of(&text.0), stderr_of(&text.1));

    let document = document_of(&json.0);
    assert_eq!(document["skipped"].as_array().map(Vec::len), Some(6));
    assert_eq!(document["excluded"].as_array().map(Vec::len), Some(6));
    assert_eq!(stdout_of(&json.0), stdout_of(&json.1));
    assert_eq!(stderr_of(&json.0), stderr_of(&json.1));

    // ... and identical between runs is not the same as ordered, which is what makes it identical.
    let skipped: Vec<&str> = document["skipped"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|entry| entry["path"].as_str().expect("a path"))
        .collect();
    let mut sorted = skipped.clone();
    sorted.sort_unstable();
    assert_eq!(skipped, sorted, "`skipped` is in walk order: {document}");
    let excluded: Vec<&str> = document["excluded"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|entry| entry.as_str().expect("a path"))
        .collect();
    let mut sorted = excluded.clone();
    sorted.sort_unstable();
    assert_eq!(excluded, sorted, "`excluded` is in walk order: {document}");
}

/// AC5 — a refusal and a boundary are two channels, asserted on ONE run that has both.
///
/// Checking only one field cannot tell "landed in the wrong list" from "landed in both": a build
/// that put every removed path into `skipped` passes an `excluded`-only test, and one that dropped
/// the distinction entirely passes a `skipped`-only test. So the run holds an unreadable file and an
/// excluded file at once and both lists are pinned to exactly one entry each.
///
/// `complete` is the load-bearing consequence: the excluded file must NOT make the run incomplete —
/// `exclude` is a boundary the project drew on purpose, and inside it the tree really was measured
/// whole. Only the refusal moves it. And the text output stays silent about the exclusion, which is
/// the recorded decision the alternative was measured against: a warning on every real exclusion
/// fires on every run of this repository and of the ruff checkout.
#[test]
fn a_skipped_file_and_an_excluded_file_are_two_different_channels() {
    // Arrange
    let scratch = Scratch::new("graceful-two-channels");
    scratch.write("broken.py", UNPARSABLE);
    scratch.write("vendor/generated.py", &long_comment("generated policy"));
    scratch.write("fat.py", &long_comment("retry policy"));
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor\"]\n",
    );

    // Act
    let json = scratch.check(&["--format", "json"]);
    let text = scratch.check(&[]);

    // Assert
    let document = document_of(&json);
    assert_eq!(
        document["skipped"].as_array().map(Vec::len),
        Some(1),
        "{document}"
    );
    assert!(
        document["skipped"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("broken.py")),
        "the refusal channel does not hold the file that was refused: {document}"
    );
    assert_eq!(
        document["excluded"].as_array().map(|paths| paths
            .iter()
            .map(|path| path.as_str().expect("a path"))
            .collect::<Vec<_>>()),
        // `./vendor` and not `vendor`: the field carries the path as the WALK reached it, the same
        // spelling `findings[].path` uses for the same run, so the two can be joined by a consumer.
        Some(vec!["./vendor"]),
        "the boundary channel does not hold exactly the excluded path: {document}"
    );
    assert_eq!(
        document["complete"], false,
        "an unreadable file did not make the run incomplete: {document}"
    );
    assert!(
        !stdout_of(&json).contains("generated.py"),
        "an excluded file was measured: {document}"
    );

    // The text output names the refusal and says nothing at all about the boundary.
    assert!(
        stderr_of(&text).contains("broken.py"),
        "{:?}",
        stderr_of(&text)
    );
    assert!(
        !stderr_of(&text).contains("vendor") && !stderr_of(&text).contains("exclude"),
        "the text output warned about a file the project excluded on purpose: {:?}",
        stderr_of(&text)
    );
}

/// The whole tree excluded is still a COMPLETE run: nothing refused, so nothing incomplete.
///
/// This is the direction the `complete` field is easiest to get wrong in — "the walk lost paths" and
/// "the tool could not read a file" look alike from far enough away, and conflating them would mark
/// every configured repository permanently incomplete.
#[test]
fn an_excluded_tree_is_a_complete_measurement_of_what_was_in_scope() {
    // Arrange
    let scratch = Scratch::new("graceful-excluded-complete");
    scratch.write("vendor/fat.py", &long_comment("vendored retry policy"));
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nexclude = [\"vendor\"]\n",
    );

    // Act
    let output = scratch.check(&["--format", "json"]);

    // Assert
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let document = document_of(&output);
    assert_eq!(
        document["complete"], true,
        "a deliberate boundary was reported as a failed measurement: {document}"
    );
    assert_eq!(document["skipped"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        document["excluded"].as_array().map(|paths| paths
            .iter()
            .map(|path| path.as_str().expect("a path"))
            .collect::<Vec<_>>()),
        Some(vec!["./vendor"]),
        "{document}"
    );
}

/// Red team — unreadable by PERMISSIONS is an io failure, not a parse failure, and the same channel.
///
/// The parse path and the open path are two different call sites, and a fix written against the one
/// the ticket named leaves the other still fatal. The file is valid Python, so nothing but the mode
/// bits can be what stops it being read.
#[test]
#[cfg(unix)]
fn a_file_unreadable_by_permissions_is_skipped_and_not_a_refusal_to_run() {
    use std::os::unix::fs::PermissionsExt as _;

    // Arrange
    let scratch = Scratch::new("graceful-permissions");
    let locked = scratch.write("locked.py", "\"\"\"Perfectly valid Python.\"\"\"\n");
    scratch.write("fat.py", &long_comment("retry policy"));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
        .expect("the mode bits are settable");

    // Act
    let output = scratch.check(&["--format", "json"]);
    let readable = std::fs::read_to_string(&locked).is_ok();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644))
        .expect("the mode bits are restorable");

    // Assert — the fixture only means anything if the file really is unreadable to this user.
    assert!(
        !readable,
        "the test user can read a 0o000 file (running as root?), so nothing was proved"
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document = document_of(&output);
    assert_eq!(document["complete"], false, "{document}");
    assert_eq!(
        document["skipped"].as_array().map(Vec::len),
        Some(1),
        "an io failure took a different path from a parse failure: {document}"
    );
    assert!(
        document["skipped"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("locked.py")),
        "{document}"
    );
    assert_eq!(
        document["findings"][0]["code"], "TPX001",
        "the readable file's finding was withheld: {document}"
    );
}

/// Red team — a skipped file that WOULD have been a cluster member changes the graph, and the run
/// has to say so.
///
/// `TPX003` is cross-file by construction, so a subset of the input does not give a smaller true
/// answer — it gives a different one. The fixture proves the claim rather than asserting it: the
/// same three files are checked whole (one cluster of three) and then with one member corrupted
/// (one cluster of *two*, a different finding), so the warning is attached to a graph that visibly
/// changed rather than to a file that merely failed.
#[test]
fn a_skipped_cluster_member_makes_the_run_warn_that_the_graph_is_incomplete() {
    // Arrange
    let scratch = Scratch::new("graceful-cluster");
    scratch.write("a.py", SHARED_RATIONALE);
    scratch.write("b.py", SHARED_RATIONALE);
    scratch.write("c.py", SHARED_RATIONALE);

    // Act
    let whole = scratch.check(&[]);
    scratch.write("c.py", &format!("{SHARED_RATIONALE}{UNPARSABLE}"));
    let partial = scratch.check(&[]);

    // Assert — the member really was in the cluster ...
    assert!(
        stdout_of(&whole).contains("in 3 places"),
        "the fixture never had the member it is about to lose: {}",
        stdout_of(&whole)
    );
    assert!(
        !stderr_of(&whole).contains("incomplete"),
        "a complete run warned about an incomplete graph: {:?}",
        stderr_of(&whole)
    );

    // ... and losing it produces a DIFFERENT finding, announced as such.
    assert_eq!(partial.status.code(), Some(1), "{partial:?}");
    assert!(
        stdout_of(&partial).contains("in 2 places"),
        "the cluster did not change, so there is nothing for the warning to be about: {}",
        stdout_of(&partial)
    );
    assert!(
        stderr_of(&partial).contains("TPX003") && stderr_of(&partial).contains("incomplete"),
        "the cluster graph was computed over a subset and the run did not say so: {:?}",
        stderr_of(&partial)
    );
}

/// ... and it does not claim it when `TPX003` was never computed at all.
///
/// A warning about a rule the configuration switched off is the same defect as a diagnostic built
/// from the configuration's say-so rather than from the run: it describes something that did not
/// happen. The skipped block itself must still be there — that half is about the files, not the rule.
#[test]
fn the_incomplete_graph_warning_is_absent_when_tpx003_never_ran() {
    // Arrange
    let scratch = Scratch::new("graceful-cluster-ignored");
    scratch.write("a.py", SHARED_RATIONALE);
    scratch.write("broken.py", UNPARSABLE);
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nignore = [\"TPX003\"]\n",
    );

    // Act
    let output = scratch.check(&[]);

    // Assert
    assert!(
        stderr_of(&output).contains("1 file(s) skipped:"),
        "the skipped file stopped being reported because a rule was off: {:?}",
        stderr_of(&output)
    );
    assert!(
        !stderr_of(&output).contains("TPX003"),
        "the run warned that a disabled rule was computed over a subset: {:?}",
        stderr_of(&output)
    );
}

/// Exit 2 is now only "the tool could not start", and the narrowing is the breaking half.
///
/// Enumerated rather than sampled, because the value of the code is that it still means something:
/// every remaining member is a failure BEFORE any file is read, and the one case that left the set —
/// an unparsable file — is asserted as having left it, in the same test. Without that last row this
/// passes on a build where 2 never happens at all.
#[test]
fn exit_two_now_means_only_that_the_run_could_not_start() {
    // Arrange
    let scratch = Scratch::new("graceful-exit-two");
    scratch.write("broken.py", UNPARSABLE);
    scratch.write("notes.txt", "not Python at all\n");

    // Act — the three ways to fail before reading anything ...
    let missing_path = scratch.check_from("", "nowhere");
    let not_python = scratch.check_from("", "notes.txt");
    scratch.write(
        "pyproject.toml",
        "[tool.tooprolix]\nignore = [\"TPX999\"]\n",
    );
    let broken_config = scratch.check(&[]);
    // ... and the case that no longer belongs to them.
    std::fs::remove_file(scratch.root.join("pyproject.toml")).expect("the file is removable");
    let unparsable_file = scratch.check(&[]);

    // Assert
    for (name, output) in [
        ("a path that does not exist", &missing_path),
        ("a non-Python file named directly", &not_python),
        ("an unknown rule code in the configuration", &broken_config),
    ] {
        assert_eq!(
            output.status.code(),
            Some(2),
            "{name} stopped being a tool error: {output:?}"
        );
    }
    assert_eq!(
        unparsable_file.status.code(),
        Some(1),
        "an unparsable file still refuses to run, so exit 2 was never narrowed: {unparsable_file:?}"
    );
}

/// The walk has its own way to fail, and it must not be the all-or-nothing contract in disguise.
///
/// The read channel was made graceful; this is the channel one screen up. A directory the walker
/// cannot enter is a *part of the tree that could not be read*, which the settled table numbers 1 —
/// not "the run could not start", which is the only thing left that numbers 2.
///
/// Both positions are probed in one test, because only the pair is a contract: a fix that turns
/// **every** walk error into a skip would delete exit 2 altogether, and a fix that turns none of
/// them into a skip is the defect. The discriminator is depth — the root is depth 0 — so the root
/// case is asserted at the same time, on the same kind of io failure, with the same mode bits.
#[test]
#[cfg(unix)]
fn an_unreadable_directory_inside_the_tree_is_skipped_rather_than_fatal() {
    use std::os::unix::fs::PermissionsExt as _;

    // Arrange — a real finding is at stake, so "no findings" cannot be mistaken for "nothing there".
    let scratch = Scratch::new("walk-unreadable");
    scratch.write("fat.py", &long_comment("retry policy"));
    let locked = scratch.root.join("locked");
    std::fs::create_dir(&locked).expect("a scratch directory is creatable");
    std::fs::write(locked.join("hidden.py"), SHARED_RATIONALE).expect("writable");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    // The mode bits are the whole fixture, and root ignores them. Without this the test does not
    // fail as root — it silently stops testing anything, walks a perfectly readable directory and
    // passes every assertion for the wrong reason. Sampled here rather than after the run, because
    // by then the permissions are already restored.
    let root_ignores_the_mode_bits = std::fs::read_dir(&locked).is_ok();

    // Act
    let inside = scratch.check(&["--format", "json"]);
    let inside_text = scratch.check(&[]);
    // ... and the same failure applied to the ROOT of the walk, which really cannot start.
    std::fs::set_permissions(&scratch.root, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let at_the_root = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
        .args(["check", scratch.root.to_str().expect("utf-8")])
        .current_dir(repository_root())
        .output()
        .expect("the binary cargo just built is executable");

    // Restore before asserting, or a failure leaves an undeletable tree behind.
    std::fs::set_permissions(&scratch.root, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    // Assert — the fixture only means anything if `chmod 000` actually stopped someone.
    assert!(
        !root_ignores_the_mode_bits,
        "a `chmod 000` directory is still readable to this user (running as root?), so neither \
         half of this test proves anything"
    );

    // Mid-walk: the run continues, reports what it read, and never exits 0 ...
    assert_eq!(
        inside.status.code(),
        Some(1),
        "an unreadable directory inside the tree still takes the whole run down: {inside:?}"
    );
    let document = document_of(&inside);
    assert_eq!(
        document["complete"], false,
        "a directory the walk could not enter left the run marked whole: {document}"
    );
    assert_eq!(
        document["findings"][0]["code"], "TPX001",
        "the finding of the readable file was thrown away with the walk error: {document}"
    );
    assert!(
        document["skipped"]
            .as_array()
            .is_some_and(|entries| entries.len() == 1
                && entries[0]["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("locked"))),
        "the unreadable directory is in neither channel: {document}"
    );
    assert!(
        stdout_of(&inside_text).contains("TPX001") && stderr_of(&inside_text).contains("locked"),
        "text: {:?} / {:?}",
        stdout_of(&inside_text),
        stderr_of(&inside_text)
    );

    // ... and at the root, where nothing could be read at all, 2 still means what it says.
    assert_eq!(
        at_the_root.status.code(),
        Some(2),
        "an unreadable ROOT was reported as a partial measurement, so exit 2 is now unreachable \
         and the guard fails open: {at_the_root:?}"
    );
    assert_eq!(stdout_of(&at_the_root), "");
}

/// A path named `*.py` that was never opened must not be counted as measured.
///
/// A FIFO passes the extension test and fails `is_file()`, so it was dropped from the walk into
/// neither channel — and the document then said `complete: true` about a tree holding an unread
/// `.py`. The drop predates this contract; the false claim does not, which is what makes it a defect
/// now rather than a quirk.
///
/// **Four positions, one run, one exact-set assertion.** Every one of them is now reported, and the
/// set is asserted exactly rather than by `contains`, so a rule that stopped covering any single
/// position is red here.
///
/// | entry | reported? | why |
/// |---|---|---|
/// | `probe.py`, a FIFO | **yes** | a `.py` nobody opened, and reading it would block forever |
/// | `alias.py`, a symlink into this tree | **yes** | not followed, therefore not measured *as this entry* |
/// | `outsider.py`, a symlink out of this tree | **yes** | same |
/// | `dead.py`, a symlink that dangles | **yes** | same |
///
/// # The middle two rows used to read **no**, and deleting that exception is the point
///
/// This table was a discrimination three times over, and each version was wrong in a way the next
/// one only narrowed:
///
/// 1. `alias.py` silent whenever `exists()` — broken by a target **outside** the walked tree, which
///    resolves just as well and is measured nowhere: a green `All checks passed!` over an unread
///    `TPX001`;
/// 2. silent when the target canonicalised **under the walk root** — broken by a sibling directory
///    whose name is a mere string prefix of the root, restoring the same false green;
/// 3. silent when the target is under the root — broken by a target that is under the root and
///    still never walked: a non-`.py` name (`./notes.txt`), a hidden directory, a gitignored path,
///    an `exclude`d path. All four exited **0** in silence.
///
/// The root cause never moved. The guard asked *"where does the target live"* while the invariant
/// is *"was the target measured in this run"*, and each round answered the wrong question more
/// precisely. So the exception is gone rather than guarded a fourth time: **a `*.py`-named symlink
/// is skipped, always**, and there is no longer a question to get wrong. `complete: false` and
/// exit 1 follow by construction instead of from a predicate that has to be right.
///
/// The dangling case is now **subsumed** rather than special: `dead.py` and `alias.py` take one
/// path and carry one reason. The `exists()`-versus-`symlink_metadata` reasoning task 5 recorded
/// here existed only to separate them, and it is deleted with the distinction it served.
///
/// What this costs is measured and it is nothing: symlinks named `*.py` number **zero** across all
/// six pinned checkouts (crewAI 0, langgraph 0, openai-agents-python 0, `OpenHands` 0, pydantic 0,
/// requests 0) and zero in this repository.
///
/// Naming a symlink directly — `tooprolix check alias.py` — still **measures** it, and that is
/// not an inconsistency. An explicit argument is an instruction about one file, not a claim about a
/// tree's completeness; ruff resolves explicit arguments past its own exclusions for the same
/// reason. `a_single_file_is_checked_and_the_help_says_what_that_misses` and the direct-FIFO half
/// of this test hold that end.
#[test]
#[cfg(unix)]
fn a_path_named_python_that_is_not_a_regular_file_is_not_counted_as_measured() {
    // Arrange
    let scratch = Scratch::new("walk-not-a-file");
    scratch.write("real.py", SHARED_RATIONALE);
    scratch.write("pkg/dup.py", SHARED_RATIONALE);
    std::os::unix::fs::symlink(scratch.root.join("real.py"), scratch.root.join("alias.py"))
        .expect("a symlink is creatable");
    // A second tree, so `outsider.py` points at a real, readable `.py` that this walk will never
    // reach. Held in a binding for the whole test: `Scratch` deletes its tree on drop, and a
    // dropped target would turn this row into a second copy of the `dead.py` case.
    let elsewhere = Scratch::new("walk-not-a-file-elsewhere");
    let outside_target = elsewhere.write("out_of_tree.py", SHARED_RATIONALE);
    std::os::unix::fs::symlink(&outside_target, scratch.root.join("outsider.py"))
        .expect("a symlink is creatable");
    assert!(
        std::fs::metadata(scratch.root.join("outsider.py")).is_ok_and(|meta| meta.is_file())
            && !outside_target.starts_with(&scratch.root),
        "`outsider.py` must RESOLVE and resolve OUTSIDE the walked tree, or it proves nothing"
    );
    let dangling = scratch.root.join("dead.py");
    std::os::unix::fs::symlink(scratch.root.join("nothing-is-here.py"), &dangling)
        .expect("a symlink is creatable");
    // The three links still have to differ from each other on disk, or the fixture is one case
    // written three times — but they no longer have to differ in the OUTCOME, and that is the
    // change. `dead.py` must dangle and the other two must resolve; the run treats all three
    // identically, which is exactly what makes the contract impossible to get wrong.
    assert!(
        std::fs::symlink_metadata(&dangling)
            .expect("the link itself exists")
            .is_symlink()
            && std::fs::metadata(&dangling).is_err(),
        "`dead.py` resolves, so the fixture no longer covers the dangling case at all"
    );
    assert!(
        std::fs::metadata(scratch.root.join("alias.py")).is_ok_and(|meta| meta.is_file()),
        "`alias.py` does not resolve, so the fixture no longer covers the resolving case at all"
    );
    // `mkfifo(1)` rather than `mkfifo(2)`: the crate denies `unsafe`, and pulling in `libc` for one
    // call would put a new entry in a `Cargo.lock` that `--locked` makes load-bearing. POSIX.
    let fifo = scratch.root.join("probe.py");
    let made = Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo(1) is a POSIX utility");
    assert!(
        made.success(),
        "the fixture needs a real FIFO to prove anything"
    );
    assert!(
        !fifo.is_file() && fifo.exists(),
        "the fixture is an ordinary file, so it proves nothing"
    );

    // Act
    let walked = scratch.check(&["--format", "json"]);
    let named = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
        .args(["check", "probe.py", "--format", "json"])
        .current_dir(&scratch.root)
        .output()
        .expect("the binary cargo just built is executable");

    // Assert — the FIFO is named as unmeasured, and the run says so ...
    let document = document_of(&walked);
    assert_eq!(
        document["complete"], false,
        "a tree holding an unread `.py` reported itself fully measured: {document}"
    );
    let skipped: Vec<&str> = document["skipped"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|entry| entry["path"].as_str().expect("a path"))
        .collect();
    // The exact set, and that is what makes one assertion cover both failure directions: dropping
    // `dead.py` is an under-reach, adding `alias.py` is an over-reach, and neither can hide behind
    // a length check or a `contains`.
    assert_eq!(
        skipped,
        vec!["./alias.py", "./dead.py", "./outsider.py", "./probe.py"],
        "a `.py`-named entry the walk never read is missing from the report: {document}"
    );
    assert_eq!(walked.status.code(), Some(1), "{walked:?}");
    // ... every symlink says the same thing, and it is the reason that is now true of all of them:
    // the walk did not follow it. This assertion used to require `alias.py` to be ABSENT from
    // stderr and `dead.py` to say "resolve" — two outcomes for two cases the tool could not
    // reliably tell apart. One reason for one rule.
    for link in ["./alias.py", "./dead.py", "./outsider.py"] {
        assert!(
            document["skipped"]
                .as_array()
                .expect("an array")
                .iter()
                .any(|entry| entry["path"] == link
                    && entry["reason"]
                        .as_str()
                        .is_some_and(|reason| reason.contains("symlink"))),
            "{link} is missing, or is reported without saying it was a link the walk did not \
             follow: {document}"
        );
    }
    assert!(
        stdout_of(&walked).contains("in 2 places"),
        "the two real sources stopped being a cluster, so the walk changed shape: {}",
        stdout_of(&walked)
    );

    // ... and naming it directly is not a silent success either.
    assert_eq!(
        named.status.code(),
        Some(1),
        "a FIFO named directly exited 0 with `no Python files`: {named:?}"
    );
    assert_eq!(document_of(&named)["complete"], false);
    // ... and it does not ALSO claim the path holds no Python. The skip already said what happened;
    // "no Python files under probe.py" is a second, contradicting answer about the same run, which
    // is the defect class this whole round is about one more time.
    assert!(
        !stderr_of(&named).contains("no Python files"),
        "the run blamed an absence of Python for a `.py` it had just reported as unread: {:?}",
        stderr_of(&named)
    );
}

/// A tree holding a symlinked source must not be told it passed — and the link points **inside**.
///
/// The sharp end of the rule, isolated from the exact-set test so the failure is legible alone: an
/// otherwise-clean tree, one link, and `All checks passed!` with exit **0** over a `TPX001` nobody
/// had read.
///
/// **The link deliberately resolves to a file inside this very tree**, which is the case that
/// was silent through all three previous versions of the guard and the one the `fable` review
/// finally broke: the target is `notes.txt`, under the walk root, resolvable — and never walked,
/// because it is not named `*.py`. A hidden directory, a gitignored path and an `exclude`d path all
/// reproduce it identically. No containment test can answer this, because "under the root" was
/// never the same question as "measured by this run". The rule is now positional and total: a
/// `*.py`-named symlink is skipped, so the tree cannot be called whole whatever the target is.
///
/// The target is proved to be a real finding **first**, through the direct-argument path. Without
/// that, this passes on a build where the link points at something that would have been clean
/// anyway, and the assertion would be about nothing.
#[test]
#[cfg(unix)]
fn a_tree_holding_a_symlinked_source_is_not_reported_as_a_clean_measurement() {
    // Arrange — the target is INSIDE the tree and is not a `*.py`, so the walk never reaches it.
    let scratch = Scratch::new("link-inside-tree");
    scratch.write("clean.py", "\"\"\"Short.\"\"\"\n");
    scratch.write("notes.txt", &SHARED_RATIONALE.repeat(10));
    std::os::unix::fs::symlink(
        scratch.root.join("notes.txt"),
        scratch.root.join("alias.py"),
    )
    .expect("a symlink is creatable");
    let named_directly = scratch.check_from("", "alias.py");
    assert_eq!(
        named_directly.status.code(),
        Some(1),
        "the target is not a finding, so this test asserts nothing: {named_directly:?}"
    );

    // Act
    let output = scratch.check(&[]);
    let json = scratch.check(&["--format", "json"]);

    // Assert — every channel says the tree was not measured whole, and none of them says it passed.
    assert_eq!(
        stdout_of(&output),
        "No findings; check incomplete: 1 file skipped.\n",
        "a tree holding an unmeasured `.py` lost its incomplete summary"
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document = document_of(&json);
    assert_eq!(
        document["complete"], false,
        "the document claimed to be whole over a source it never opened: {document}"
    );
    assert!(
        document["skipped"]
            .as_array()
            .expect("an array")
            .iter()
            .any(|entry| entry["path"] == "./alias.py"),
        "the link is not named as unread: {document}"
    );
    // ... and the very same file, named directly, IS measured. Asserted here beside the silence so
    // the distinction cannot be read as an inconsistency: an explicit argument is an instruction
    // about one file, not a claim about a tree's completeness.
    assert!(
        stdout_of(&named_directly).contains("TPX001"),
        "naming the link directly stopped measuring it: {:?}",
        stdout_of(&named_directly)
    );
}

/// Every marker diagnostic comes out in a deterministic order, and the proof is not a re-run.
///
/// `report_skipped` and `Report::new` both sort; these two did not, and rode the walk order
/// instead. **Re-running twice on one filesystem cannot show it** — the directory order is stable
/// on a given disk, so an order-dependent output is byte-identical to itself all day. The property
/// is therefore asserted directly: the emitted sequence must be sorted, on input whose walk order
/// is demonstrably not.
///
/// Both channels are checked, because they are separate call sites one screen apart, and the whole
/// class here is a guard applied in one place and missed in its sibling.
#[test]
fn the_marker_diagnostics_are_emitted_in_a_deterministic_order() {
    // Arrange — names chosen so alphabetical order is not the order they are written in, and one
    // of them is nested, so a walk that descends last cannot accidentally agree with the sort.
    let block = |name: &str| {
        format!(
            "# The {name} path is described here because the reason is not obvious from the code.\n\
             # It matters on retry, where the caller expects steady progress on every attempt.\n"
        )
    };
    let names = ["middle", "zebra", "beta", "alpha", "nested/deep"];

    let unknown = Scratch::new("order-unknown-code");
    let near_miss = Scratch::new("order-near-miss");
    for name in names {
        unknown.write(
            &format!("{name}.py"),
            &format!("# !TPX999\n{}", block(name)),
        );
        near_miss.write(
            &format!("{name}.py"),
            &format!("# !nonsense TPX001\n{}", block(name)),
        );
    }

    // Act
    let unknown_output = unknown.check(&[]);
    let near_miss_output = near_miss.check(&[]);

    // Assert
    for (label, output, needle) in [
        ("unknown code", &unknown_output, "is not a rule code"),
        ("near miss", &near_miss_output, "is not an opt-out marker"),
    ] {
        let lines: Vec<&str> = stderr_of(output)
            .lines()
            .filter(|line| line.contains(needle))
            .collect();
        assert_eq!(
            lines.len(),
            names.len(),
            "{label}: the fixture did not produce one diagnostic per file, so ordering is \
             untestable: {:?}",
            stderr_of(output)
        );
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        assert_eq!(
            lines, sorted,
            "{label}: the diagnostics are in walk order, so the run is only reproducible on a \
             filesystem that happens to enumerate in this order"
        );
    }
}

/// AC1 — the version the binary prints is the one in `Cargo.toml`, not a literal it carries.
///
/// The comparison is between two **artifacts**: the bytes the real binary wrote and the real
/// `Cargo.toml` read off disk. Comparing against `env!("CARGO_PKG_VERSION")` from inside this test
/// would have been shorter and would have graded a self-report — the same constant the CLI reads,
/// so a CLI that printed a literal `0.0.0` could still be made to pass by editing one place. It is
/// also the invariant `pyproject.toml`'s `dynamic = ["version"]` depends on: one owner of the
/// number, no manual copies.
///
/// **The date gets exactly the same treatment, and git is its oracle.** It used to be pinned by
/// *shape* — ten characters of digits and dashes — which graded nothing: a `commit_date()` that
/// ignored git and returned the literal `"2024-03-01"` passed the whole suite, and so did a stale
/// build whose date was two years wrong. Both were reproduced. `git log -1 --format=%cs` is an
/// artifact this crate does not produce, so comparing the binary's bytes to it kills the literal,
/// a fake `git` on `PATH` answering `2026-99-99`, an invalid civil date, and a build script that
/// did not re-run — with one assertion instead of a two-build diff nobody runs in CI.
///
/// The three branches are the three states the build script can be in, and only the first is not
/// an equality — with `SOURCE_DATE_EPOCH` set there is no second artifact to ask, because the
/// environment variable *is* the answer. Nothing here returns early: a skipped branch that reports
/// success is the fail-open guard this epic keeps finding.
#[test]
fn the_version_is_the_one_in_cargo_toml_and_carries_a_build_date() {
    // Arrange
    let manifest = std::fs::read_to_string(repository_root().join("Cargo.toml"))
        .expect("the manifest is next to the tests");
    let declared = manifest
        .lines()
        .take_while(|line| !line.starts_with("[lib]"))
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.split('"').next())
        .expect("[package] declares a version");

    // Act
    let output = tooprolix(&["--version"]);
    let printed = stdout_of(&output);

    // Assert
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let date = printed
        .strip_prefix(&format!("tooprolix {declared} ("))
        .and_then(|rest| rest.strip_suffix(")\n"))
        .unwrap_or_else(|| {
            panic!("`--version` is not `tooprolix {declared} (<date>)`: {printed:?}")
        });
    match (
        std::env::var_os("SOURCE_DATE_EPOCH"),
        our_git(&["log", "-1", "--format=%cs"]),
    ) {
        (Some(epoch), _) => assert!(
            date.len() == 10
                && date.split('-').count() == 3
                && date.chars().all(|c| c.is_ascii_digit() || c == '-'),
            "SOURCE_DATE_EPOCH={epoch:?} was set, so the date is that epoch and there is no second \
             artifact to check it against — but it is not even an ISO date: {date:?}"
        ),
        (None, Some(committed)) => assert_eq!(
            date, committed,
            "`--version` does not print the commit date git reports. Either the build script is \
             not reading git, or cargo did not re-run it and the binary is carrying the date of an \
             older commit."
        ),
        (None, None) => assert_eq!(
            date, "unknown",
            "this package has no git history of its own and SOURCE_DATE_EPOCH is unset, so the \
             only honest answer is `unknown` — a date here comes from somewhere that is not this \
             package, which is what Decisions #14 forbids"
        ),
    }
}

/// No cargo profile may abort on panic, because the exit contract is 0/1/2 and abort is neither.
///
/// `[profile.release]` was added by `dry-run-packaging-matrix` after an A/B, and `panic` is the one
/// key that measurement is **not** allowed to reach for. With `panic = "abort"` a panic is
/// `SIGABRT`: the process dies on a signal, `std::process::ExitCode` never gets to carry the number
/// `src/main.rs` returns, destructors do not run, and whatever `cli::emit`'s `BufWriter` was holding
/// is lost. Exit 101 from an unwinding panic is already outside the documented contract and is a
/// bug when it happens — but it is a *number*, on stderr, with the panic message beside it, which
/// is the difference between a diagnosable failure and a signal.
///
/// **Two things this test deliberately does NOT do.**
///
/// It does not assert `cfg!(panic = "unwind")`. Cargo forces unwinding for test harnesses and warns
/// that `panic` is ignored for the test profile, so that assertion is true no matter what
/// `[profile.release]` says — a test that cannot fail for the thing it names, which is the exact
/// shape this epic keeps finding. Measured: it stays green with `panic = "abort"` in place.
///
/// It does not check `[profile.release]` alone. A guard aimed at one section is disabled by moving
/// the key: `[profile.bench]` and a `[profile.release.package.*]` override are both real places to
/// put it. Every `[profile…]` table in the manifest is scanned instead, so the document is
/// validated rather than the one spelling that was thought of.
///
/// Mutation-proved: with `panic = "abort"` appended to `[profile.release]`, this test fails.
#[test]
fn no_cargo_profile_aborts_on_panic() {
    // Arrange
    let manifest = std::fs::read_to_string(repository_root().join("Cargo.toml"))
        .expect("the manifest is next to the tests");

    // Act — every `[profile…]` table, each with the keys that belong to it and nothing else.
    let mut section = String::new();
    let offenders: Vec<String> = manifest
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                section = name.to_owned();
                return None;
            }
            let value = line.strip_prefix("panic")?.trim_start().strip_prefix('=')?;
            section
                .starts_with("profile")
                .then(|| format!("[{section}] panic ={value}"))
        })
        .collect();

    // Assert
    assert!(
        offenders.is_empty(),
        "a cargo profile sets `panic`, and the only value the 0/1/2 exit contract survives is the \
         default `unwind`: {offenders:?}"
    );
}

/// A panic in a **release** build stays an exit code and does not take buffered output with it.
///
/// The runtime half of the `[profile.release]` guarantee. `no_cargo_profile_aborts_on_panic` above
/// reads the manifest; this one runs a binary compiled under the profile and watches it die.
///
/// **Both halves are needed, and the reason is narrower than it first looks.** The manifest check
/// asserts that no profile *sets* `panic` — which is only a guarantee if the DEFAULT is `unwind`.
/// It trusts that. This one verifies it, by watching a binary compiled under the profile die.
///
/// It does **not** cover `RUSTFLAGS`, and an earlier draft of this comment claimed it did. That
/// claim was measured and is false: `RUSTFLAGS="-C panic=abort" cargo test` does not produce an
/// aborting test binary, it refuses to build at all —
/// `error: building tests with panic=abort is not supported without -Zpanic_abort_tests`. So that
/// path is loud rather than silent, and neither test has to cover it. Corrected here rather than
/// left standing, because an unmeasured sentence in a guard's own documentation is how a guard
/// comes to be trusted for something it never did.
///
/// The two assertions are the two things `abort` destroys, measured on this repository 2026-07-31:
///
/// | `[profile.release]` | exit | stdout |
/// |---|---|---|
/// | as committed (`unwind`) | **101** | `buffered-before-the-panic` |
/// | `+ panic = "abort"`     | **134** (SIGABRT) | **empty** |
///
/// The stdout half is the one worth spelling out: the example leaves 25 bytes inside an unflushed
/// `BufWriter` and lets the panic unwind through it, so the flush happens in `Drop` during
/// unwinding. `abort` runs no destructor, so the bytes never leave the process — the same way a
/// real run would lose whatever `cli::emit` had buffered. 101 is *itself* outside the documented
/// 0/1/2 contract and is a bug wherever it happens; the point is that it is a number, on a stream,
/// beside a message, rather than a signal.
///
/// It shells out to cargo because the profile under test is not the one this test is compiled
/// with — cargo forces unwinding for test harnesses, so nothing observable from inside this process
/// says anything about `[profile.release]`.
#[test]
fn a_panic_in_a_release_build_stays_a_code_and_keeps_its_output() {
    // Arrange / Act
    let output = std::process::Command::new(env!("CARGO"))
        .args([
            "run",
            "--release",
            "--locked",
            "--quiet",
            "--example",
            "panic_is_a_controlled_exit",
        ])
        .current_dir(repository_root())
        // `cargo llvm-cov` does NOT put `-Cinstrument-coverage` in `RUSTFLAGS` — measured with
        // `cargo llvm-cov show-env`, it sets `RUSTC_WRAPPER` plus `__CARGO_LLVM_COV_RUSTC_WRAPPER_*`
        // and injects the flags inside the wrapper, where cargo's fingerprint cannot see them. Its
        // whole isolation is therefore the `--target-dir target/llvm-cov-target` FLAG, and a flag
        // is exactly what this nested cargo does not inherit. Left alone it builds an instrumented
        // release example into the shared `target/release/examples/`, which cargo then treats as
        // fresh forever, so the next plain `cargo test` reuses it.
        .env_remove("RUSTC_WRAPPER")
        // With the profile path gone too, an instrumented child has nowhere to write but its
        // current directory — the repository root. That is what turns the assertion at the end of
        // this test from an ordering-dependent accident into one that fails in the coverage job as
        // well as here: measured 2026-08-01, deleting the `RUSTC_WRAPPER` line above and running
        // `make rust.cov` on a cold `target/release/examples/` exits 2 on that assertion.
        //
        // Its ceiling, for the same fingerprint reason: cargo reuses whichever build landed first,
        // so a warm example built the other way is reused and hides the change until something
        // else invalidates it. `cargo clean --release -p tooprolix` is what makes a run decisive.
        .env_remove("LLVM_PROFILE_FILE")
        .output()
        .expect("cargo is on PATH: this test is running under it");

    // Assert
    assert_eq!(
        output.status.code(),
        Some(101),
        "a panic must end in an exit CODE; `None` here means a signal, i.e. panic = \"abort\". \
         stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "buffered-before-the-panic",
        "output buffered when the panic happened was lost, which is what abort does"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("panicked at"),
        "the panic message has to reach stderr, not just the exit code"
    );

    // …and it leaves no coverage artifact in the repository root. Graded on the directory listing
    // rather than on anything the child reported about itself, and NOT deleted when it fires: a
    // guard that tidies away its own evidence turns a real leak into a green second run.
    //
    // Scope, so nothing reads more into a green run than it earns: ONE snapshot of ONE directory,
    // taken inside a suite that runs in parallel. It proves the route traced above and nothing
    // wider — a `tests/*.profraw`, or a root file written after this enumeration, passes it. The
    // whole-tree postcondition is `CHECK_NO_PROFRAW` in the Makefile, which runs after
    // `make rust.test` and `make cov` have finished.
    let leaked: Vec<PathBuf> = std::fs::read_dir(repository_root())
        .expect("the repository root is readable")
        .map(|entry| {
            entry
                .expect("the repository root stays readable while it is walked")
                .path()
        })
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "profraw")
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "the nested cargo ran an instrumented binary in the repository root, so the LLVM profile \
         runtime wrote {leaked:?} next to Cargo.toml — untracked, ungitignored and one `git add .` \
         from being committed. Delete them, then keep the instrumentation out of the child build."
    );
}

/// [`git`], but only when the repository git discovers is **this package's own**.
///
/// Without this the oracle is circular in the one layout that matters. Git's discovery walks
/// *upward*, so an extracted sdist or a `cargo vendor` tree sitting inside an unrelated checkout
/// answers with the **host** repository's commit date — and this test then computed its expected
/// value from that same foreign repository, so the wrong date graded as correct. Reproduced: a
/// tree with no `.git` of its own printed `2020-01-01` from the repository above it, and this test
/// passed. The containment has to be here as well as in `build.rs`; checking only the build script
/// would leave a test that cannot fail for the thing it exists to check.
///
/// **Both sides are canonicalised before they are compared.** On macOS `/tmp` is a symlink to
/// `/private/tmp`, so `--show-toplevel` and `CARGO_MANIFEST_DIR` name the same directory with
/// different strings whenever the checkout is reached through one — including in this epic's own
/// scratch directories. A string comparison would fail closed on a perfectly good repository and
/// turn every `--version` into `unknown`, which is worse than the bug being fixed.
fn our_git(arguments: &[&str]) -> Option<String> {
    let canonical = |path: &str| std::fs::canonicalize(path).ok();
    let toplevel = git(&["rev-parse", "--show-toplevel"])?;
    if canonical(&toplevel)? != canonical(&repository_root().display().to_string())? {
        return None;
    }
    git(arguments)
}

/// One `git` invocation from the repository root, or `None` when git cannot answer.
///
/// The same collapse-to-`None` as `build.rs`: no git on `PATH`, not a checkout, and a repository
/// with no commits are all "git has no answer", and the caller treats all three alike. Callers
/// wanting the date want [`our_git`] instead — this one will happily answer about a repository
/// that merely encloses the package.
fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository_root())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// AC3 — one registry feeds `--rules` and the Rules block of `--help`, so the text cannot drift.
///
/// The assertion is containment of the **rendered bytes**: every line `--rules` printed appears in
/// `--help` indented by two spaces. A test that only checked both mentioned `TPX001` would survive
/// exactly the mutation this exists to catch — a description hardcoded in the help text next to a
/// different one in the registry.
///
/// `TPX004` is listed and is deliberately **not** a [`Rule`]: it is a number that has been spoken
/// for, not a detector that runs, and `Rule::ALL` stays three long so `ignore = ["TPX004"]` and
/// `# !TPX004` keep being refused.
#[test]
fn the_rules_listing_and_the_help_render_the_same_registry() {
    // Act
    let rules = tooprolix(&["--rules"]);
    let help = tooprolix(&["--help"]);

    // Assert
    assert_eq!(rules.status.code(), Some(0), "{rules:?}");
    assert_eq!(help.status.code(), Some(0), "{help:?}");

    let listed: Vec<&str> = stdout_of(&rules).lines().collect();
    assert_eq!(
        listed.len(),
        4,
        "`--rules` does not list the three shipping rules plus reserved TPX004: {:?}",
        stdout_of(&rules)
    );
    for line in &listed {
        assert!(
            stdout_of(&help).contains(&format!("  {line}\n")),
            "`--help` does not carry the line `--rules` printed, so the two have separate copies \
             of the text: {line:?} not in {:?}",
            stdout_of(&help)
        );
    }
    for code in ["TPX001", "TPX002", "TPX003", "TPX004"] {
        assert!(
            listed.iter().any(|line| line.starts_with(code)),
            "`--rules` does not list {code}: {:?}",
            stdout_of(&rules)
        );
    }
    assert!(
        listed[3].contains("Reserved"),
        "TPX004 is not marked reserved, so `--rules` claims a detector that does not run: {:?}",
        listed[3]
    );
}

/// AC3 — the binary and every documented rule table say the same thing, so there is no fourth owner.
///
/// This grades the real Markdown files against the real stdout. It is also the guard on the
/// release-day flip: when `Implemented` becomes `Released` in one file and not the others, this
/// reddens instead of shipping three answers to "is this rule available?".
#[test]
fn the_rules_listing_agrees_with_every_documented_table() {
    // Arrange
    let rules = tooprolix(&["--rules"]);
    let documents = ["README.md", "docs/rules-and-configuration.md"];

    // Assert — the loop below iterates over `--rules` output, so an empty one would make every
    // assertion in it vacuous. Measured: without these two lines this test PASSED on a binary that
    // did not have `--rules` at all.
    assert_eq!(rules.status.code(), Some(0), "{rules:?}");
    assert_eq!(
        stdout_of(&rules).lines().count(),
        4,
        "`--rules` printed nothing to compare the tables against: {:?}",
        stdout_of(&rules)
    );
    let expected: Vec<String> = stdout_of(&rules)
        .lines()
        .map(|line| {
            let (code, rest) = line.split_at(6);
            let (status, description) = rest
                .trim_start()
                .split_once(char::is_whitespace)
                .expect("every catalogue line is `code status description`");
            format!("| `{code}` | {} | {status} |", description.trim_start())
        })
        .collect();

    // Equality of the whole set of `TPX` rows, not a `contains` per row. A whole-file `contains`
    // passed while the visible table was stale and the right row sat in a fenced code block or an
    // HTML comment; narrowing it to "some line starting with `|`" does not fix that, because a
    // fenced row starts with `|` too. Requiring the document's rows to BE the binary's rows, in
    // order, closes the stale row, the hidden duplicate and the extra row at once.
    for document in documents {
        let text = std::fs::read_to_string(repository_root().join(document))
            .unwrap_or_else(|error| panic!("{document} is readable: {error}"));
        let rows: Vec<String> = text
            .lines()
            .map(|line| line.trim_end().to_owned())
            .filter(|line| line.starts_with("| `TPX"))
            .collect();
        assert_eq!(
            rows, expected,
            "the rule rows in {document} are not the ones `--rules` printed"
        );
    }
}

/// AC4 — the discovery surface, pinned as a regression: it was already right and must stay right.
///
/// The two new flags are documented by the same text that documents the old ones, which is the only
/// reason a user who ran `--help` before this change would find them.
#[test]
fn the_discovery_surface_names_every_flag_and_an_unknown_command_points_at_it() {
    // Act
    let help = tooprolix(&["--help"]);
    let unknown = tooprolix(&["badcommand"]);
    let short = tooprolix(&["-V"]);

    // Assert
    assert_eq!(help.status.code(), Some(0), "{help:?}");
    for flag in ["--help", "--version", "--rules", "--format"] {
        assert!(
            stdout_of(&help).contains(flag),
            "`--help` does not document {flag}: {:?}",
            stdout_of(&help)
        );
    }

    assert_eq!(unknown.status.code(), Some(2), "{unknown:?}");
    assert!(
        stderr_of(&unknown).contains("unknown subcommand `badcommand`")
            && stderr_of(&unknown).contains("Run `tooprolix --help`"),
        "an unknown command no longer points at the help: {:?}",
        stderr_of(&unknown)
    );

    // ruff's own convention, checked against ruff 0.16.0: `-V` and `--version` both work.
    assert_eq!(short.status.code(), Some(0), "{short:?}");
    assert_eq!(stdout_of(&short), stdout_of(&tooprolix(&["--version"])));
}

/// A flag that reports and exits takes nothing else, so `--version --rules` cannot silently pick.
///
/// The precedent is `--format` given twice: this parser refuses an ambiguous command line rather
/// than letting one side win. `--help` is deliberately left alone — it already ignores everything
/// after it, that is shipped behaviour, and tightening it would change an exit code this task
/// promised not to touch.
#[test]
fn two_reporting_flags_at_once_are_refused_rather_than_ranked() {
    for arguments in [
        vec!["--version", "--rules"],
        vec!["--rules", "--version"],
        vec!["--version", "check", "."],
        vec!["--rules", "src"],
    ] {
        let output = tooprolix(&arguments);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{arguments:?} was not refused: {output:?}"
        );
        assert!(
            stderr_of(&output).contains("takes no other arguments"),
            "{arguments:?} was refused without saying why: {:?}",
            stderr_of(&output)
        );
    }
}

/// A consumer that stops reading gets exit 0 and no output error, without losing skip diagnostics.
///
/// `tooprolix check big/ | head -5` used to exit **101** and print
/// `thread 'main' panicked at ... failed printing to stdout: Broken pipe (os error 32)`, because
/// Rust's `println!` panics on a write error and nothing caught it. 101 is outside the documented
/// 0/1/2 contract altogether, and `| head` is an ordinary thing to do with a linter. ruff answers
/// the identical pipeline with **0**, which is the parity this pins.
///
/// The reader is closed *before* the child can write, so the very first write fails and the test
/// has no race in it. Both formats are covered because the JSON path is a single large `write_all`
/// and the text path is a loop — different code, same contract.
///
/// The stderr assertions are not decoration: exiting 0 while printing a panic, or swallowing a
/// partial run's skip reason, would satisfy an exit-code-only test and still be a defect. So is the
/// finding count — a handler that swallowed every io error would make `| cat` green too, which is why
/// `a_readable_pipe_still_reports_findings` sits beside this and asserts exit 1.
#[test]
fn a_consumer_that_stops_reading_is_not_an_error() {
    // Arrange — enough findings that the writer is still writing when the reader is gone.
    let scratch = Scratch::new("broken-pipe");
    for name in ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"] {
        scratch.write(
            &format!("{name}.py"),
            &long_comment(&format!("{name} budget")),
        );
    }

    for format in [&[][..], &["--format", "json"][..]] {
        // Act — spawn, then drop the read end before reading a single byte.
        let mut arguments = vec!["check", "."];
        arguments.extend_from_slice(format);
        let mut child = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
            .args(&arguments)
            .current_dir(&scratch.root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the binary cargo just built is executable");
        drop(child.stdout.take().expect("stdout was piped"));

        let output = child
            .wait_with_output()
            .expect("the child is waitable after its pipe is closed");
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        // Assert
        assert_eq!(
            output.status.code(),
            Some(0),
            "{arguments:?}: a closed pipe must be exit 0, got {:?} with stderr {stderr:?}",
            output.status.code()
        );
        assert!(
            !stderr.contains("panicked") && !stderr.contains("Broken pipe"),
            "{arguments:?}: the panic reached the user: {stderr:?}"
        );
    }

    for path in [
        "tests/fixtures/broken/syntax_error.py",
        "tests/fixtures/broken",
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
            .args(["check", path])
            .current_dir(repository_root())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the binary cargo just built is executable");
        drop(child.stdout.take().expect("stdout was piped"));

        let output = child
            .wait_with_output()
            .expect("the partial child is waitable after its pipe is closed");
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert_eq!(output.status.code(), Some(0), "{path}: {output:?}");
        assert!(
            stderr.contains("syntax_error.py") && stderr.contains("could not parse Python source"),
            "{path}: a closed stdout swallowed the skip detail: {stderr:?}"
        );
    }
}

/// ... and the clean stop must not swallow a real find: a reader that DOES read still gets exit 1.
///
/// The guard in `status` keys on `ErrorKind::BrokenPipe` alone. Widening it to any write failure
/// would make this test's tree — six files, all with findings — report success, which is the
/// fail-open shape of the fix rather than the fix.
#[test]
fn a_readable_pipe_still_reports_findings() {
    let scratch = Scratch::new("broken-pipe-readable");
    for name in ["alpha", "bravo", "charlie"] {
        scratch.write(
            &format!("{name}.py"),
            &long_comment(&format!("{name} budget")),
        );
    }

    let output = scratch.check(&[]);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a fully read run with findings is exit 1: {:?}",
        stderr_of(&output)
    );
    assert!(
        stdout_of(&output).contains("TPX001"),
        "the findings themselves went missing: {:?}",
        stdout_of(&output)
    );
}

/// A write failure that is NOT a closed pipe is reported: exit 2, with the reason on stderr.
///
/// The injection is a **read-only fd as stdout** — `File::open` gives a fd opened for reading, and
/// `Stdio::from` hands it to the child as its fd 1. Writing to it fails with `EBADF`. That is a
/// real failure and not a synthetic one: `sh -c 'echo hi'`, `head` and `/bin/echo` all report
/// `Bad file descriptor` and exit 1 on the identical fd.
///
/// It is the other half of `a_consumer_that_stops_reading_is_not_an_error`. That test pins the one
/// write failure that is FORGIVEN; this one pins that forgiveness is narrow. Without it, widening
/// the guard to "any write failure is a clean stop" is invisible end-to-end — the unit table
/// `only_a_closed_pipe_is_a_clean_stop` catches it, but nothing proves the wiring reaches a real
/// process with a real broken fd.
///
/// This case reached exit 2 only after `emit` stopped writing through `std::io::stdout()`.
/// Measured on `4ac17da`: `EBADF` was swallowed by std — `write_all` and `flush` both returned
/// `Ok(())` while the data went nowhere — so `check` exited **1** and the discovery commands
/// exited **0**, both in complete silence. See `emit`'s documentation for the mechanism.
#[cfg(unix)]
#[test]
fn a_write_failure_that_is_not_a_closed_pipe_is_reported() {
    let scratch = Scratch::new("readonly-stdout");
    scratch.write("fat.py", &long_comment("retry budget"));
    let sink = scratch.write("sink.txt", "");

    // `check` (findings on stdout) and the three discovery commands all go through `emit`.
    for arguments in [
        vec!["check", "."],
        vec!["check", ".", "--format", "json"],
        vec!["--version"],
        vec!["--rules"],
    ] {
        let readonly = std::fs::File::open(&sink).expect("the sink is openable for reading");
        let output = Command::new(env!("CARGO_BIN_EXE_tooprolix"))
            .args(&arguments)
            .current_dir(&scratch.root)
            .stdout(std::process::Stdio::from(readonly))
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("the binary cargo just built is executable");
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        assert_eq!(
            output.status.code(),
            Some(2),
            "{arguments:?}: an unwritable stdout must be exit 2, got {:?} with stderr {stderr:?}",
            output.status.code()
        );
        assert!(
            stderr.contains("could not write to stdout"),
            "{arguments:?}: the failure was not named on stderr: {stderr:?}"
        );
    }
}
