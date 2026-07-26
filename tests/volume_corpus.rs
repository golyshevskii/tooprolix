//! The volume detector measured against the pinned corpus checkouts, not against a fixture.
//!
//! The corpus test is `#[ignore]`d: it needs `corpus/checkouts/` on disk, which CI does not have,
//! and it reads thousands of files. It is the AC3 instrument — the numbers in
//! `src/detect/volume.rs`'s documentation are what it prints, produced by the shipped
//! `extract()` + `volume()` path rather than by the Python calibration script. The other test,
//! `the_walk_accepts_the_same_files_extract_does`, is **not** ignored: it guards the walk itself
//! against a fixture it builds, and a guard that only runs where the corpus is checked out is a
//! guard CI never runs. Run the corpus one with:
//!
//! ```text
//! PYO3_PYTHON="$(uv python find)" cargo test --locked --test volume_corpus -- --ignored --nocapture
//! ```
//!
//! # They count the same file set the rule reads, and that is shared rather than re-stated
//!
//! The walk below accepts a file iff [`is_python_source`] does — the predicate `extract()` itself
//! dispatches on, called rather than re-spelled. It *was* re-spelled, as `extension == "py"`, and
//! the difference was invisible: a file named `LOUD.PY` is prose to the rule and was absent from
//! every number here. Measured after the fix, no checkout at the pinned SHAs holds a file whose
//! extension differs from `py` only in case (`find -name '*.py'` and `find -iname '*.py'` return
//! the same count for all eight), so the figures below are unchanged by it — the bug was latent,
//! not active.
//!
//! # These still count a different file set from `corpus/measure.py`
//!
//! `measure.py` additionally drops virtualenvs, build trees, vendored and third-party directories,
//! migrations, protobuf stubs and machine-generated files (`corpus/REPORT.md`, "Exclusions").
//! Measured, the two agree exactly on five of the six checkouts; they part on `OpenHands`, where
//! this walk sees 915 files against `measure.py`'s 774 and finds one more `TPX002` at the default
//! limit. Which exclusions the real run applies is
//! `build-cli-with-exit-contract-and-rule-codes`'s decision, not this crate's.

use std::fs;
use std::path::{Path, PathBuf};

use tooprolix::detect::volume::{Limits, volume};
use tooprolix::extract::{ProseKind, extract, is_python_source};

/// The checkouts `corpus/corpus.lock` pins, by name.
///
/// Asserted as a set rather than as a bare count, so a clone that did not materialise is *named*
/// in the failure instead of quietly shrinking the corpus a floor assertion is measured over.
/// Re-pinning the corpus moves SHAs, not names, so this does not carry the brittleness a
/// per-repository finding count would.
const PINNED_CHECKOUTS: [&str; 6] = [
    "OpenHands",
    "crewAI",
    "langgraph",
    "openai-agents-python",
    "pydantic",
    "requests",
];

/// Files the parser rejects, corpus-wide, at the pinned SHAs.
///
/// Not zero, and the non-zero value is the point of asserting it rather than printing it: `crewAI`
/// ships Jinja templates named `.py`. Measured, never estimated — the figure is whatever
/// `volume_finds_something_on_the_corpus` prints, and a re-pin may legitimately move it. Moving it
/// is a corpus change that has to be seen and re-recorded, which is the opposite of the counter
/// being computed and then discarded.
const EXPECTED_UNPARSEABLE: usize = 5;

/// Every Python file under `root`, recursively, skipping symlinks so a linked tree is not counted
/// twice.
///
/// "Python" is [`is_python_source`], the same predicate [`extract`] dispatches on, rather than a
/// second spelling of it — see that function's documentation for the `LOUD.PY` file the second
/// spelling used to lose.
///
/// # Errors
///
/// Any [`fs::read_dir`] failure, propagated rather than swallowed. A directory this walk cannot
/// read is a corpus it cannot measure: swallowing the error turned an unreadable checkout into
/// "zero findings here", which every assertion downstream then read as a fact.
fn python_files(root: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            python_files(&path, found)?;
        } else if is_python_source(&path) {
            found.push(path);
        }
    }
    Ok(())
}

/// What one checkout scan produced.
struct Scan {
    /// `TPX002` findings.
    docstrings: usize,
    /// `TPX001` findings.
    comments: usize,
    /// Python files the walk reached.
    files: usize,
    /// Files the walk reached and the *parser* rejected — counted, never silently dropped.
    ///
    /// A file that cannot be **read** is not counted here and is not tolerated at all: it panics
    /// out of [`scan`]. Read failure is a broken environment and parse failure is a fact about the
    /// corpus, and folding the two together is how a corpus that stopped being readable kept
    /// reporting a plausible number.
    unparseable: usize,
}

/// Findings for one checkout.
///
/// # Panics
///
/// If the checkout cannot be walked or one of its files cannot be read.
fn scan(repo: &Path, limits: Limits) -> Scan {
    let mut files = Vec::new();
    python_files(repo, &mut files)
        .unwrap_or_else(|error| panic!("{} could not be walked: {error}", repo.display()));
    files.sort();

    let mut scan = Scan {
        docstrings: 0,
        comments: 0,
        files: files.len(),
        unparseable: 0,
    };
    for file in &files {
        let source = fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("{} could not be read: {error}", file.display()));
        let Ok(blocks) = extract(file, &source) else {
            scan.unparseable += 1;
            continue;
        };
        for overrun in volume(&blocks, limits).overruns {
            match overrun.block.kind {
                ProseKind::Docstring => scan.docstrings += 1,
                ProseKind::Comment => scan.comments += 1,
            }
        }
    }
    scan
}

/// The directory holding the pinned checkouts, or a failure naming what is missing.
fn checkouts() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/checkouts");
    assert!(
        root.is_dir(),
        "{} does not exist; run `make corpus.measure` to materialise the pinned checkouts",
        root.display()
    );
    root
}

/// The walk and [`extract`] must accept exactly the same files, or AC5 counts the wrong set.
///
/// Not `#[ignore]`d and not corpus-backed: it builds three files in `CARGO_TARGET_TMPDIR`, so it
/// runs in CI beside everything else. `LOUD.PY` is the whole point — [`extract`] compares the
/// extension with `eq_ignore_ascii_case`, so that file is prose to the shipped rule, and a walker
/// that compares it with `== "py"` measures a *different* file set from the one the linter reads
/// while reporting the difference as nothing at all.
#[test]
fn the_walk_accepts_the_same_files_extract_does() {
    // Arrange — one lower-case, one upper-case, one that is not Python at all.
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("walk_case");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("pkg")).expect("the temp tree is writable");
    for name in ["pkg/quiet.py", "pkg/LOUD.PY", "pkg/notes.txt"] {
        fs::write(root.join(name), "x = 1\n").expect("the fixture file is writable");
    }

    // Act
    let mut found = Vec::new();
    python_files(&root, &mut found).expect("the temp tree is readable");
    found.sort();

    // Assert
    assert_eq!(
        found,
        vec![root.join("pkg/LOUD.PY"), root.join("pkg/quiet.py")],
        "the walk must accept every path extract() accepts, and only those"
    );
    for file in &found {
        extract(file, "x = 1\n").expect("a file the walk accepted must be one extract() reads");
    }
}

/// **AC3, the floor.** Findings per corpus repository, so a limit that fires nowhere cannot pass.
///
/// The *finding* assertion is deliberately weak — a per-repository number would pin eight checkouts
/// instead of one and would redden on every corpus re-pin — but it is not absent: a set of limits
/// that finds nothing anywhere is the "detector that does not detect" the epic names as a failure
/// mode, and this is what refuses it. The exact per-repository figures are printed, and are the
/// table in `src/detect/volume.rs`.
///
/// # A weak assertion over an unknown input set is not weak, it is empty
///
/// So the *input* is pinned even though the finding counts are not, and that is the difference
/// between this version and the one review rejected. Measured on the shipped code: `chmod 000` on
/// `corpus/checkouts/requests` left the walk reporting "0 files, 0 findings" for it, six other
/// repositories still firing, and this test **green** — a floor assertion satisfied by a corpus it
/// could not read. Three things now stand in the way, none of which pins a finding count:
///
/// * [`PINNED_CHECKOUTS`] — the corpus is the one `corpus/corpus.lock` names, by name;
/// * [`python_files`] propagates its [`fs::read_dir`] errors, so an unreadable checkout is an error
///   rather than a zero;
/// * [`EXPECTED_UNPARSEABLE`] — the counter [`scan`] fills is asserted rather than discarded.
#[test]
#[ignore = "needs corpus/checkouts on disk"]
fn volume_finds_something_on_the_corpus() {
    // Arrange
    let mut repos: Vec<PathBuf> = fs::read_dir(checkouts())
        .expect("the checkouts directory was just asserted to exist")
        .map(|entry| {
            entry
                .expect("a checkouts directory entry is readable")
                .path()
        })
        .filter(|path| path.is_dir())
        .collect();
    repos.sort();
    let names: Vec<String> = repos
        .iter()
        .map(|repo| {
            repo.file_name()
                .expect("a directory entry has a name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        names, PINNED_CHECKOUTS,
        "the corpus on disk is not the corpus corpus.lock pins; run `make corpus.measure`"
    );

    // Act
    let mut firing = 0;
    let (mut total_docstrings, mut total_comments, mut total_unparseable) = (0, 0, 0);
    println!(
        "{:<28} {:>6} {:>8} {:>8} {:>12}",
        "repo", "files", "TPX002", "TPX001", "unparseable"
    );
    for (repo, name) in repos.iter().zip(&names) {
        let found = scan(repo, Limits::default());
        println!(
            "{name:<28} {:>6} {:>8} {:>8} {:>12}",
            found.files, found.docstrings, found.comments, found.unparseable
        );
        assert!(
            found.files > 0,
            "{name} yielded no Python files at all — that is a clone that did not materialise, \
             not a measurement"
        );
        total_docstrings += found.docstrings;
        total_comments += found.comments;
        total_unparseable += found.unparseable;
        if found.docstrings + found.comments > 0 {
            firing += 1;
        }
    }

    // Assert
    assert_eq!(
        total_unparseable, EXPECTED_UNPARSEABLE,
        "the number of files the parser rejects moved; re-record it rather than discarding it"
    );
    assert!(
        firing >= 4,
        "the default limits fire on only {firing} of {} checkouts — a rule that finds nothing is a \
         failed choice, not a safe one",
        repos.len()
    );
    assert!(
        total_docstrings > 0 && total_comments > 0,
        "one of the two codes never fires"
    );
}
