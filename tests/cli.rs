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
             the 200-word limit \u{2014} shorten it, or mark it with `# !TPX002` on the line \
             above it",
            "tests/fixtures/dup-corpus/legacy.py:2: TPX001 comment is 238 words long, over the \
             150-word limit \u{2014} shorten it, or mark it with `# !TPX001` on the line above \
             it",
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
        stderr_of(&output).contains("`TPX999` in an opt-out marker is not a rule code"),
        "an unknown code in a marker was swallowed: {:?}",
        stderr_of(&output)
    );
    assert!(
        stdout_of(&output).contains("comment_mistyped.py:6: TPX001"),
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
        stdout_of(&bom_output).contains("capable.py:1: TPX002"),
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
        "",
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
    assert_eq!(stdout_of(&quiet_output), "");
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
        "",
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

    // Assert
    assert_eq!(
        before.status.code(),
        Some(2),
        "the fixture cannot demonstrate an exclusion it never triggers: {before:?}"
    );
    assert!(
        stderr_of(&before).contains("parser_fixture.py"),
        "the unparsable file is not the reason the run failed: {:?}",
        stderr_of(&before)
    );

    assert_eq!(
        after.status.code(),
        Some(0),
        "an excluded unparsable file still failed the run: {:?}",
        stderr_of(&after)
    );
    assert_eq!(stdout_of(&after), "");
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
    assert_eq!(
        stdout_of(&output).lines().count(),
        1,
        "the same source was counted twice, so the walk followed a symlink: {}",
        stdout_of(&output)
    );
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

    assert_eq!(stdout_of(&after), "");
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
        "",
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

/// `./vendor` and `vendor` are one rule, and so is every other spelling of the same path.
///
/// This overturns a residual: "an entry that matches nothing is silent" was accepted because a
/// shared configuration legitimately names paths absent from any one repository. `./vendor` is not
/// an absent path — it is a *present* one in a natural spelling, and the task pins ruff semantics,
/// where it works. Unnormalised it is a glob for a directory literally named `.`, which matches
/// nothing, so the run fails on a tree the user believed they had excluded.
///
/// The whole class is swept rather than the one instance that was reported, because `./x` was
/// found by someone typing it and the next one will be found the same way.
#[test]
fn every_spelling_of_the_same_relative_path_excludes_the_same_tree() {
    // Arrange — an unparsable file, so a failed exclusion is exit 2 and cannot be mistaken for a
    // clean run that happened to find nothing.
    let scratch = Scratch::new("exclude-spellings");
    scratch.write("broken/parser_fixture.py", "def f(:\n    pass\n");
    scratch.write("app.py", "\"\"\"A short docstring.\"\"\"\n");

    for spelling in [
        "broken",
        "./broken",
        ".//broken",
        "././broken",
        "broken/",
        "./broken/",
        "brok*n",
    ] {
        scratch.write(
            "pyproject.toml",
            &format!("[tool.tooprolix]\nexclude = [\"{spelling}\"]\n"),
        );

        let output = scratch.check(&[]);

        assert_eq!(
            output.status.code(),
            Some(0),
            "`{spelling}` silently excluded nothing, so the unparsable file still failed the \
             run: {:?}",
            stderr_of(&output)
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
