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

fn stderr_of(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("the CLI writes UTF-8")
}

/// The exit contract, all three codes, on three fixtures that are each capable of the other two
/// answers — which is what stops this from being three tests that all pass on a CLI that always
/// exits 0.
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

    // Assert
    assert_eq!(clean.status.code(), Some(0), "clean: {clean:?}");
    assert_eq!(
        stdout_of(&clean),
        "",
        "a clean tree prints nothing to stdout"
    );

    assert_eq!(findings.status.code(), Some(1), "findings: {findings:?}");
    assert!(
        !stdout_of(&findings).is_empty(),
        "exit 1 with an empty stdout is a finding nobody can act on"
    );

    assert_eq!(broken.status.code(), Some(2), "broken: {broken:?}");
    assert_eq!(
        stdout_of(&broken),
        "",
        "a run that could not read the tree must not print findings: {:?}",
        stdout_of(&broken)
    );
    assert!(
        stderr_of(&broken).contains("syntax_error.py"),
        "the reason must name the file: {:?}",
        stderr_of(&broken)
    );
}

/// The broken tree holds a file that *would* be a finding, and the tool must withhold it.
///
/// Without this the AC2 fixture could pass on a CLI that reports findings and merely adds an exit
/// code, which is the "a parser error looks like clean" defect one layer out: the user would read
/// a partial list as the state of the repository.
#[test]
fn a_parse_failure_withholds_the_findings_of_the_files_that_did_parse() {
    // Arrange — prove the withheld finding is real by asking for it on its own.
    let reachable = tooprolix(&["check", "tests/fixtures/broken/long_docstring.py"]);
    assert_eq!(reachable.status.code(), Some(1), "{reachable:?}");
    assert!(
        stdout_of(&reachable).contains("TPX002"),
        "the fixture cannot demonstrate withholding if it has nothing to withhold: {:?}",
        stdout_of(&reachable)
    );

    // Act
    let whole_tree = tooprolix(&["check", "tests/fixtures/broken"]);

    // Assert
    assert_eq!(whole_tree.status.code(), Some(2), "{whole_tree:?}");
    assert!(
        !stdout_of(&whole_tree).contains("TPX002"),
        "a finding survived a failed run: {:?}",
        stdout_of(&whole_tree)
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

/// All three shipping codes render, sorted by address, and the run is byte-identical.
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
            "tests/fixtures/dup-corpus/client.py:2: TPX003 same explanation in 3 places: \
             tests/fixtures/dup-corpus/poller.py:2, tests/fixtures/dup-corpus/worker.py:2 \
             (weakest tests/fixtures/dup-corpus/client.py:2 ~ \
             tests/fixtures/dup-corpus/poller.py:2, similarity 0.900)",
            "tests/fixtures/dup-corpus/config.py:1: TPX002 docstring is 244 words long, over \
             the 200-word limit \u{2014} shorten it, or mark it with \
             `# tooprolix: noqa TPX002` on the line above it",
            "tests/fixtures/dup-corpus/legacy.py:2: TPX001 comment is 238 words long, over the \
             150-word limit \u{2014} shorten it, or mark it with `# tooprolix: noqa TPX001` \
             on the line above it",
        ]
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
    assert_eq!(stdout_of(&single), "");

    assert_eq!(help.status.code(), Some(0), "{help:?}");
    assert!(
        stdout_of(&help).contains("only finds duplicates inside that file"),
        "--help does not warn that a single-file run is not a verdict on the repository: {}",
        stdout_of(&help)
    );
    assert!(
        stdout_of(&help).contains("words"),
        "--help does not name the unit the limits are measured in: {}",
        stdout_of(&help)
    );
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
        stdout_of(&output).starts_with("tests/fixtures/dup-corpus/config.py:1: TPX002"),
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
    assert_eq!(stdout_of(&output), "");
    assert!(
        stderr_of(&output).contains("no Python files"),
        "a walk that measured nothing reported success in silence: {:?}",
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

    assert_eq!(document["schema_version"], "1");
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

    // Every finding renders the same sentence the text format prints, so the two cannot drift.
    // The length is asserted BEFORE the zip: `zip` stops at the shorter side, so zero text lines
    // would make the loop run zero times and the claim hold vacuously.
    let text = tooprolix(&["check", "tests/fixtures/dup-corpus"]);
    let lines: Vec<&str> = stdout_of(&text).lines().collect();
    assert_eq!(lines.len(), findings.len(), "{lines:#?}");
    for (finding, line) in findings.iter().zip(lines) {
        assert_eq!(finding["message"], line);
    }
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
            "import json,sys; d=json.load(sys.stdin); print(d['schema_version'], len(d['findings']))",
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
    assert_eq!(stdout_of(&python).trim(), "1 3");
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
    let flagged: Vec<&str> = stdout_of(&output)
        .lines()
        .map(|line| {
            line.strip_prefix("tests/fixtures/optout/")
                .expect("every finding is inside the fixture")
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
        stderr_of(&output).contains("`TPX999` in a tooprolix marker is not a rule code"),
        "an unknown code in a marker was swallowed: {:?}",
        stderr_of(&output)
    );
    assert!(
        stdout_of(&output).contains("comment_mistyped.py:6: TPX001"),
        "an unknown code in a marker silenced a real rule: {}",
        stdout_of(&output)
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
        "",
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
    assert_eq!(
        stdout_of(&output).lines().count(),
        1,
        "{}",
        stdout_of(&output)
    );
    assert!(
        stdout_of(&output).contains("in 2 places"),
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
        "",
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
        stdout_of(&prefixed).contains("./-weird.py:1: TPX001"),
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
    assert_eq!(stdout_of(&defaults).lines().count(), 1, "{defaults:?}");
    assert!(stdout_of(&defaults).contains("TPX003"));
    assert!(
        !stdout_of(&defaults).contains("TPX001"),
        "the rationale is under the default limit and must not fire: {}",
        stdout_of(&defaults)
    );

    assert_eq!(tightened.status.code(), Some(1));
    assert_eq!(
        stdout_of(&tightened)
            .lines()
            .filter(|line| line.contains("TPX001"))
            .count(),
        2,
        "a lowered comment limit did not reach the detector: {}",
        stdout_of(&tightened)
    );

    assert_eq!(silenced.status.code(), Some(0), "{silenced:?}");
    assert_eq!(stdout_of(&silenced), "");
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
        ("[tool.tooprolix]\nexclude = [\"vendor\"]\n", "exclude"),
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

/// A clean run is silent in text and is still a document in JSON.
///
/// Zero bytes on a successful `--format json` is a parse error at the consumer's end that is
/// indistinguishable from a crash, so the empty case has to be written. The text half of the same
/// contract is the opposite and is asserted here beside it, so neither can be changed alone.
#[test]
fn a_clean_run_is_silent_in_text_and_an_empty_document_in_json() {
    // Act
    let text = tooprolix(&["check", "tests/fixtures/clean"]);
    let json = tooprolix(&["check", "tests/fixtures/clean", "--format", "json"]);

    // Assert
    assert_eq!(text.status.code(), Some(0), "{text:?}");
    assert_eq!(stdout_of(&text), "");

    assert_eq!(json.status.code(), Some(0), "{json:?}");
    let document: serde_json::Value =
        serde_json::from_str(stdout_of(&json)).expect("a clean run still emits one JSON document");
    assert_eq!(document["schema_version"], "1");
    assert_eq!(
        document["findings"].as_array().map(std::vec::Vec::len),
        Some(0)
    );
}
