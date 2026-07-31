//! The AC3 wall-clock budget, measured on the input that defeats the candidate index.
//!
//! `src/detect/duplicate.rs`'s `for_each_candidate_pair` visits 2.13–3.30% of `n(n-1)/2` on real
//! repositories, and that measurement is the recorded reason this crate does **not** use
//! minhash/LSH. This file measures the case where that index buys nothing: `n` files whose prose
//! header is identical except for one token. Every block then shares almost every shingle with
//! every other block, so every bucket of the inverted index holds every block, and the candidate
//! set *is* `n(n-1)/2`. That is not a defect of the index — it is the shape of the input — and it
//! is why the budget has to be defended here rather than on the corpus, where the index already
//! removes 97% of the work.
//!
//! # The budget, and the unit it is stated in
//!
//! The contract is **`< 5 s / 100 000 lines`**. "Lines" and "files" are different quantities, and
//! stating a benchmark in files while the budget is in lines is how a gate ends up measuring
//! something the budget never priced. So the generator is sized to make the two coincide at the
//! largest case rather than to be argued about: [`LINES_PER_FILE`] is 50, so 2 000 files are
//! **exactly 100 000 lines** and the budget there is exactly 5 s. Every case is checked against
//! the *rate*, `5 s × lines / 100 000`, which is the stricter reading — the work grows with the
//! square of the file count while the allowance grows only with the line count.
//!
//! [`Manifest`] carries the line count, the byte count and a fingerprint of the exact input, so a
//! number recorded here can be tied to the tree it was measured on rather than to a file count that
//! several different trees could share.
//!
//! # Measured, and what the numbers say
//!
//! Measured 2026-07-30 on the named host below. The budget is met at every size **after** the
//! change recorded in `src/detect/duplicate.rs`'s `shingles`; it was not met before it, and both
//! sides are kept here because the "after" number means nothing without the "before".
//!
//! | files | lines | comparisons | budget | before | after | detect before | detect after |
//! |---|---|---|---|---|---|---|---|
//! | 500 | 25 000 | 124 750 | 1.250 s | 0.43–0.44 s | **0.20–0.22 s** | 0.38–0.39 s | 0.16–0.17 s |
//! | 1 000 | 50 000 | 499 500 | 2.500 s | 1.60–1.64 s | **0.70–0.73 s** | 1.53–1.57 s | 0.63–0.65 s |
//! | 2 000 | 100 000 | 1 999 000 | 5.000 s | **6.26–6.38 s** | **2.59–2.87 s** | 6.12–6.25 s | 2.46–2.71 s |
//!
//! Input fingerprints, in the same order: `3b0e8cb73922afcf`, `81dfc75f559dd74b`,
//! `02cd5bba8850271f`, pinned as constants by
//! `the_timed_trees_are_the_ones_the_recorded_numbers_were_measured_on` so this table cannot come
//! adrift from the trees it describes. Comparisons are exactly `C(n, 2)` at every size — measured
//! proof that the index removes nothing here, not a claim that it does not — and they are
//! **unchanged** across the two columns, which is what makes this a speed-up rather than a recall
//! change.
//!
//! # Which stage owns the time, and the limit of what this instrument can say
//!
//! Two stages are separated directly, because they are separate calls: **extraction** (read, parse,
//! extract) and **detection** (`duplicates`). At 2 000 files, before the change, that is 0.164 s
//! against 6.145 s — detection is **about 97%** of the run and extraction under 3%. That much is a
//! measurement, not an inference.
//!
//! Inside detection, what the instrument supports is the `ns/pair` column: `detect` divided by
//! `comparisons`. Before the change it reads **3 141 / 3 138 / 3 074 ns** at 500 / 1 000 / 2 000
//! files; after it, **1 332 / 1 302 / 1 297 ns**. Across every run the two bands are
//! **3 040–3 140 ns** and **1 230–1 350 ns**. The cost per pair is *flat across a sixteenfold
//! change in the pair count*, and that is the whole claim: a stage whose cost did not depend on the
//! number of pairs would make this column fall as `n` grew, because the fixed work would be spread
//! over quadratically more pairs. It does not fall, so detection is pair-dominated, and the change
//! recorded in `src/detect/duplicate.rs` moved exactly that per-pair cost — by **2.3x–2.5x**.
//!
//! **What it deliberately does not say is a percentage for scoring alone.** An earlier revision
//! subtracted a control input that shared no shingle and attributed the difference to scoring. That
//! is unsound and the number it produced (97.1%) has been withdrawn: at 2 000 files the two inputs
//! carry the same 156 000 gram references but the adversarial tree creates only ~2 077 distinct
//! index keys against the control's 156 000, so the control does radically different `HashMap` and
//! allocation work. Subtracting it removes work the real input never performs. Separating candidate
//! *enumeration* from *scoring* from the union-find needs a method this file does not have — they
//! are all quadratic here and all inside one call — so they are reported together as the
//! pair-dependent cost, which is what `ns/pair` measures and all it measures.
//!
//! # Deriving the break-even, from this table and nothing else
//!
//! The budget is a rate, so a run is inside it whenever
//! `lines >= seconds / 5 s * 100 000`. Dividing by the file count gives the density at which this
//! workload breaks even, and every input is on the line above:
//!
//! ```text
//! break-even lines/file = median_seconds / 5.0 * 100_000 / files
//! ```
//!
//! At 2 000 files that is `6.26 / 5 * 100000 / 2000` = **62.6–63.8 lines/file before** the change
//! and `2.59 / 5 * 100000 / 2000` = **25.9–28.7 lines/file after** it.
//! Ranges, not points, and taken across *independent* runs rather than within one: the same
//! machine reproduces a median to about 6%, which is the honest precision of any number here.
//! Below that density a tree of near-identical headers misses the budget; above it, it does not.
//!
//! # What this does NOT say, measured so it cannot be over-read
//!
//! The fixture is deliberately the densest input the budget can be stated over: 50 lines per file
//! maximises files — and therefore `n^2` — per line of allowance. Real Python is nothing like it.
//! Measured over the six pinned checkouts with
//! `find <repo> -name '*.py' -type f | wc -l` and `... -exec cat {} + | wc -l`: 3 913 files and
//! 1 207 535 lines, i.e. **309 lines per file**, with no repository below 230 — every one of them
//! at least 3.7x above even the pre-change break-even.
//!
//! End-to-end on the largest of them, with the release binary and a `CORPUS_ROOT` outside this
//! repository (`tooprolix check crewAI --format json`, five runs, median): **0.675 s** for 1 269
//! files and 292 204 lines, against a 14.6 s budget — under 5% of it. That figure comes from a
//! hand-run of the shipped binary rather than from either committed harness; the command is given
//! so it can be re-taken, because a number whose provenance is not committed is a number a reader
//! has to trust.
//!
//! So the overrun this file was written to find was real, reproducible, and reachable only by a
//! tree that is simultaneously file-dense and near-uniformly headed.
//!
//! # The reference host
//!
//! **Apple M1 Max (`MacBookPro18,2`), macOS 26.5.2, `rustc 1.97.0` pinned by
//! `rust-toolchain.toml`**, measured at `7df7e4a` (2026-07-30) under cargo's DEFAULT release
//! profile — `opt-level = 3`, no LTO, `codegen-units = 16` — because this crate declared no
//! `[profile.release]` at that commit. Naming the chip matters: "Apple M-series" spans parts that
//! differ by more than the margin being measured.
//! One discarded warm-up run per size, then [`RUNS`] timed runs, median reported.
//!
//! **The profile has since changed, so a re-run is not comparable to the table below.** `d743d8c`
//! (2026-07-31, PR #34) added `[profile.release]` with `lto = "fat"`, `codegen-units = 1` and
//! `strip = true`, and `7df7e4a` is an ancestor of it. Every number here is therefore attributable
//! — to the default profile at the commit named above — and nothing has re-measured them under the
//! profile that ships today. Re-stating the table means re-running it and saying so.
//!
//! Wall-clock numbers are not byte-reproducible across hosts; they are re-runnable on this one.
//! The spread between the printed `min` and `max` within one run is under 2% at every size; across
//! independent runs the median moves by up to 6%, which is why every figure above is a range. An earlier revision of
//! this file reported an `extract` half three to twenty times larger and varying by ±30%; that was
//! filesystem writeback from generating the tree still in flight during the timed runs, and it is
//! gone now that a second tree is written between generation and measurement. The `detect` half was
//! never affected — it measured 5.87–6.12 s under both harnesses.
//!
//! # Two gates, and they fail for different reasons
//!
//! `the_generated_headers_are_adversarial_by_construction` is **not** `#[ignore]`d: it is cheap,
//! deterministic, and it asserts the *shape* — that the fixture really does defeat the index and
//! that the detector really does find the one cluster in it. It cannot be made red by a slow
//! machine. `adversarial_headers_stay_within_the_line_rate_budget` is `#[ignore]`d because it is a
//! wall-clock measurement: it needs the release profile to mean anything and it is noisy by nature.
//! Keeping them apart is deliberate — a correctness regression and a performance regression are
//! diagnosed and fixed differently, and a single test that could be red for either reason tells you
//! neither.
//!
//! **The shape test guards one direction only, and that is on purpose.** It asserts
//! `comparisons == C(n, 2)`, so it reddens when candidate generation starts scoring *fewer* pairs —
//! the direction that would silently invalidate every timing here, and the direction a recall loss
//! arrives from. It stays green if the index is replaced by unconditional full enumeration, because
//! that scores exactly the same pairs and its claim — "the wall clock below is the wall clock of
//! `C(n, 2)` real Jaccard scores" — remains true. Catching *that* is
//! `src/detect/duplicate.rs`'s `bench_smoke_comparisons_grow_no_faster_than_the_corpus`, which
//! measures growth on an input where the index is supposed to help. Two tests, two directions,
//! neither shadowing the other.
//!
//! Run the budget gate with:
//!
//! ```text
//! cargo test --locked --release --test adversarial_bench -- --ignored --nocapture
//! ```

use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tooprolix::detect::duplicate::duplicates;
use tooprolix::extract::{ProseBlock, extract};

/// File counts the budget is measured at, smallest first.
///
/// Three sizes rather than one: a single point cannot show whether the cost is growing with the
/// files or with their square, and the whole question this file exists to answer is which.
const SIZES: [usize; 3] = [500, 1_000, 2_000];

/// Lines in every generated file.
///
/// 50 is chosen so that the largest case is exactly 100 000 lines — the denominator the budget is
/// stated over. See the module documentation; this constant is the resolution of the files-versus-
/// lines mismatch, not an arbitrary size.
const LINES_PER_FILE: usize = 50;

/// Lines of the generated file that carry the shared prose header.
const HEADER_LINES: usize = 7;

/// Seconds the budget allows per 100 000 lines. The published contract, not a local choice.
const BUDGET_SECONDS_PER_100K_LINES: f64 = 5.0;

/// Lines the budget above is stated over.
const BUDGET_LINES: f64 = 100_000.0;

/// Timed runs per size, after one discarded warm-up run.
///
/// The median of three, matching `corpus/bench.py`'s median-of-samples policy rather than
/// inventing a second one. Three and not ten because each run at the largest size reads 2 000
/// files and scores two million pairs; the gate has to stay runnable by hand.
const RUNS: usize = 3;

/// Exactly what one generated tree contains, so a timing can be tied to the input it was taken on.
///
/// The fingerprint is the part that makes the other three trustworthy: file, line and byte counts
/// can all stay identical while the *text* changes, and the text is what decides whether the input
/// is still adversarial. A tree whose header wording drifted would keep the same shape and quietly
/// stop defeating the index.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Manifest {
    /// Files written.
    files: usize,
    /// Lines across all of them — the unit the budget is stated in.
    lines: usize,
    /// Bytes across all of them.
    bytes: usize,
    /// FNV-1a 64 over every file's bytes, in ascending path order.
    fingerprint: u64,
}

impl Manifest {
    /// Seconds this input is allowed, at the contract rate.
    #[allow(
        clippy::cast_precision_loss,
        reason = "line counts here are ~10^5, exact in f64"
    )]
    fn budget(&self) -> Duration {
        Duration::from_secs_f64(BUDGET_SECONDS_PER_100K_LINES * self.lines as f64 / BUDGET_LINES)
    }
}

/// FNV-1a 64.
///
/// Hand-rolled rather than pulled in: this is a change-detector over a generated fixture, not a
/// security primitive, and a dependency added to a crate heading for publication has to earn more
/// than eight lines.
fn fingerprint(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// One generated file: a shared prose header differing from every other file's in a single token,
/// then plain statements that carry no prose at all.
///
/// The code half is deliberately free of own-line comments and docstrings, so every file yields
/// **exactly one** block. Blocks then equal files, and the comparison count is a statement about
/// the input rather than about how many blocks the generator happened to emit per file.
///
/// The distinguishing token is the last word of the header. With `SHINGLE_K = 3` it therefore
/// belongs to exactly one shingle, so two headers differ by one gram each way.
///
/// Counted against the shipped normaliser rather than eyeballed: the header is **80 normalised
/// words**, giving **78** shingles per block; two of them intersect in **77** and union to **79**,
/// so the score is `77 / 79 = 0.974684`. Far above the 0.75 threshold, and *not* 1.0, so the
/// arithmetic exact-group path is bypassed and every pair is really scored — which is the property
/// the whole benchmark depends on. (An earlier revision of this comment said `78 / 80 = 0.975`,
/// which was arithmetic written from memory rather than derived; the conclusion was unaffected but
/// the numbers were wrong.)
fn adversarial_source(index: usize) -> String {
    let mut source = String::with_capacity(LINES_PER_FILE * 80);
    source.push_str(
        "# Retry budget rationale, shared verbatim across every service in this tree.\n\
         # The upstream gateway throttles us per minute, so a fourth attempt is refused\n\
         # anyway and only makes the outage longer for every caller queued behind this\n\
         # one. Raising the cap here without also raising the quota on their side would\n\
         # move the failure exactly one layer down, where it is harder to see and much\n\
         # harder to attribute back to the decision that caused it.\n",
    );
    write!(source, "# Reviewed by team {index}\n\n").expect("writing into a String cannot fail");
    // `LINES_PER_FILE - HEADER_LINES - 1` code lines, in two-line functions, so the file is
    // exactly LINES_PER_FILE lines long and the count is arithmetic rather than eyeballed.
    let code_lines = LINES_PER_FILE - HEADER_LINES - 1;
    for step in 0..code_lines / 2 {
        write!(
            source,
            "def step_{step}(value: int) -> int:\n    return value + {step}\n"
        )
        .expect("writing into a String cannot fail");
    }
    source
}

/// What [`generate`] would produce, without touching the filesystem.
///
/// Split out so the *timed* trees can be pinned by the cheap, always-run correctness test: asserting
/// a 2 000-file fingerprint costs 2 000 string builds here, against 2 000 file writes through
/// `generate`. Without this split the only assertion affordable in the fast test was over the
/// 64-file tree, and a generator changed only for `index >= 64` would leave the committed timing
/// table describing a tree that no longer exists while every non-ignored test stayed green.
fn manifest_of(files: usize, source_of: fn(usize) -> String) -> Manifest {
    let mut manifest = Manifest {
        files,
        lines: 0,
        bytes: 0,
        fingerprint: 0xcbf2_9ce4_8422_2325,
    };
    for index in 0..files {
        let source = source_of(index);
        manifest.lines += source.lines().count();
        manifest.bytes += source.len();
        manifest.fingerprint = fingerprint(source.as_bytes(), manifest.fingerprint);
    }
    manifest
}

/// Writes `files` sources into `directory` and returns exactly what was written.
fn generate(directory: &Path, files: usize, source_of: fn(usize) -> String) -> Manifest {
    fs::create_dir_all(directory).expect("the scratch tree is creatable");
    let mut manifest = Manifest {
        files,
        lines: 0,
        bytes: 0,
        fingerprint: 0xcbf2_9ce4_8422_2325,
    };
    // Ascending index is ascending zero-padded name, so the write order below and the sorted read
    // order in `read_blocks` are the same order, and the fingerprint does not depend on which.
    for index in 0..files {
        let source = source_of(index);
        manifest.lines += source.lines().count();
        manifest.bytes += source.len();
        manifest.fingerprint = fingerprint(source.as_bytes(), manifest.fingerprint);
        fs::write(directory.join(format!("mod_{index:06}.py")), &source)
            .expect("a scratch file is writable");
    }
    manifest
}

/// Whether a measured total misses the budget.
///
/// The contract is strictly `< 5 s / 100 000 lines`, so a run landing **exactly** on the budget has
/// not met it. Extracted as a named predicate purely so that boundary can be tested: written inline
/// it was `>`, which passed a total equal to the budget, and no test could reach the case because
/// no test can make a wall clock land on an exact value.
fn over_budget(total: Duration, budget: Duration) -> bool {
    total >= budget
}

/// A scratch directory outside the repository, canonicalised so macOS's `/var` symlink cannot make
/// a path look like it was followed through a link.
///
/// Outside the repository for the reason the epic keeps paying for: a tree written under
/// `/Users/vgolyshevskii/dwh` is matched by that directory's `.gitignore`, and the CLI's walk
/// honours it. Nothing here uses that walk — the read below is this file's own — but generating
/// into a place where a later hand-run of the binary would silently see nothing is a trap left
/// armed for the next reader.
fn scratch(name: &str) -> PathBuf {
    let root = fs::canonicalize(std::env::temp_dir())
        .expect("the system temporary directory exists")
        .join(format!(
            "tooprolix-adversarial-{name}-{}",
            std::process::id()
        ));
    let _ = fs::remove_dir_all(&root);
    root
}

/// Reads every `.py` file in `directory`, in ascending path order, and extracts its prose.
///
/// Deliberately not the CLI's `python_files` walk: that walk's job is gitignore semantics and
/// directory traversal, both linear in the file count and irrelevant to a quadratic question. What
/// is timed here is read + parse + extract + detect, which is where every term that grows faster
/// than the input lives.
fn read_blocks(directory: &Path) -> Vec<ProseBlock> {
    let mut paths: Vec<PathBuf> = fs::read_dir(directory)
        .expect("the generated tree is readable")
        .map(|entry| {
            entry
                .expect("a generated directory entry is readable")
                .path()
        })
        .filter(|path| path.extension().is_some_and(|extension| extension == "py"))
        .collect();
    paths.sort();
    let mut blocks = Vec::new();
    for path in &paths {
        let source = fs::read_to_string(path).expect("a generated file is readable");
        blocks.extend(extract(path, &source).expect("the generated source is valid Python"));
    }
    blocks
}

/// The number of pairs full enumeration would visit.
fn all_pairs(n: usize) -> usize {
    n * (n - 1) / 2
}

/// The fixture is adversarial **by construction**, and this is the assertion that says so.
///
/// Not a performance test and not `#[ignore]`d: it is the correctness half of the gate, it runs on
/// every `make rust.test`, and it cannot be reddened by a slow machine. What it defends is the
/// premise the whole budget measurement rests on — that the inverted index removes *nothing* here,
/// so the wall clock at the top of this file is the wall clock of `n(n-1)/2` real Jaccard scores.
///
/// If a future change to candidate generation makes this red, the change is not automatically
/// wrong — but it has altered which pairs are scored, and that is a recall decision, which
/// `src/detect/duplicate.rs` records as belonging to the user and not to an optimisation pass.
#[test]
fn the_generated_headers_are_adversarial_by_construction() {
    // Arrange — small enough to be free, large enough that C(n, 2) is not a rounding error.
    const FILES: usize = 64;
    /// The committed snapshot of the generated input, so the numbers recorded in this file's
    /// module documentation stay attached to the tree they were measured on. File, line and byte
    /// counts can all hold while the *wording* drifts, and the wording is what decides whether the
    /// headers still collide in every shingle bucket.
    const FINGERPRINT: u64 = 0xdd84_bb40_1446_e11b;
    let directory = scratch("shape");
    let manifest = generate(&directory, FILES, adversarial_source);

    // Act
    let blocks = read_blocks(&directory);
    let report = duplicates(&blocks);

    // Assert — the input first: a fixture that drifted off 50 lines per file would silently
    // restate the budget in a different unit than the one the module documentation promises.
    assert_eq!(manifest.files, FILES);
    assert_eq!(
        manifest.lines,
        FILES * LINES_PER_FILE,
        "the generator must emit exactly {LINES_PER_FILE} lines per file — the budget is stated \
         per 100 000 LINES, and a drift here silently rescales it"
    );
    assert_eq!(
        blocks.len(),
        FILES,
        "one prose block per file, or `comparisons` stops being a statement about the input"
    );
    assert_eq!(
        manifest.fingerprint, FINGERPRINT,
        "the generated input changed; re-measure the table in this file's module documentation \
         before re-pinning this constant, because those timings describe the OLD tree"
    );
    assert!(manifest.bytes > 0, "an empty tree measures nothing");

    // Then the property that makes it adversarial: the index saved nothing.
    assert_eq!(
        report.comparisons,
        all_pairs(FILES),
        "the shared header must put every block in every bucket, so the candidate set is the \
         complete pair set; anything less means this fixture no longer measures the case the \
         budget exists to defend"
    );

    // And the detector still gets the right answer on it.
    assert_eq!(
        report.clusters.len(),
        1,
        "one header copied {FILES} times is ONE finding, not C(n, 2) of them"
    );
    assert_eq!(report.clusters[0].members.len(), FILES);

    fs::remove_dir_all(&directory).expect("the scratch tree is removable");
}

/// The three **timed** trees are pinned here, in the test that always runs.
///
/// Pinning only the 64-file tree left a hole with a name: change the generated text for
/// `index >= 64` while keeping the headers all-pairs similar, and every non-ignored test stays
/// green while the committed timing table in this file's module documentation silently describes a
/// tree that no longer exists. The `#[ignore]`d benchmark merely *prints* the new fingerprints,
/// and printed evidence a human is expected to eyeball is exactly the self-report this repository
/// keeps paying for.
///
/// [`manifest_of`] is what makes this affordable in a test that must stay cheap: it is 2 000 string
/// builds and no filesystem at all, a few milliseconds. `bytes` is pinned alongside the fingerprint
/// because it is the one field of [`Manifest`] the fingerprint cannot imply — a change that
/// preserved the hash by coincidence would still have to move the byte count.
#[test]
fn the_timed_trees_are_the_ones_the_recorded_numbers_were_measured_on() {
    // (files, lines, bytes, fingerprint) — measured 2026-07-30, the trees the module table times.
    let pinned: [(usize, usize, usize, u64); 3] = [
        (500, 25_000, 793_890, 0x3b0e_8cb7_3922_afcf),
        (1_000, 50_000, 1_587_890, 0x81df_c75f_559d_d74b),
        (2_000, 100_000, 3_176_890, 0x02cd_5bba_8850_271f),
    ];
    for (files, lines, bytes, expected) in pinned {
        let manifest = manifest_of(files, adversarial_source);
        assert_eq!(
            (manifest.files, manifest.lines, manifest.bytes),
            (files, lines, bytes),
            "the {files}-file tree changed shape"
        );
        assert_eq!(
            manifest.fingerprint, expected,
            "the {files}-file tree changed content; the timings recorded in this file's module \
             documentation describe the OLD tree and must be re-measured before re-pinning"
        );
    }
}

/// The budget is `< 5 s`, so landing exactly on it is a miss.
///
/// One character of the gate, and unreachable from the gate itself: no test can make a wall clock
/// land on an exact `Duration`. [`over_budget`] exists as a named predicate so the boundary can be
/// stated at all. Written as `>`, the middle case below passes a budget it does not meet.
#[test]
fn a_run_exactly_on_the_budget_has_not_met_it() {
    let budget = Duration::from_secs(5);
    assert!(
        !over_budget(Duration::from_millis(4_999), budget),
        "under budget is not over budget"
    );
    assert!(
        over_budget(budget, budget),
        "the contract is strictly < 5 s, so exactly 5 s is a miss"
    );
    assert!(over_budget(Duration::from_millis(5_001), budget));
}

/// AC3 — the wall-clock budget, on the input that defeats the candidate index.
///
/// `#[ignore]`d for the same reason `tests/volume_corpus.rs`'s corpus test is: it is an instrument,
/// not a unit test. It needs `--release` to mean anything (a debug build measures the profile, not
/// the algorithm) and it writes 2 000 files. The numbers it prints are the ones recorded in this
/// file's module documentation.
#[test]
#[ignore = "wall-clock instrument: needs --release and writes 2000 files"]
fn adversarial_headers_stay_within_the_line_rate_budget() {
    println!(
        "{:>6} {:>8} {:>7} {:>11} {:>8} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>12}",
        "files",
        "lines",
        "blocks",
        "comparisons",
        "clusters",
        "budget",
        "median",
        "min",
        "max",
        "extract",
        "detect",
        "ns/pair",
    );
    let mut failures: Vec<String> = Vec::new();

    for files in SIZES {
        // Arrange
        let directory = scratch(&format!("bench-{files}"));
        let manifest = generate(&directory, files, adversarial_source);
        assert_eq!(
            manifest,
            manifest_of(files, adversarial_source),
            "what reached the disk differs from what `manifest_of` pins in the always-run test"
        );

        // Act — one discarded warm-up so the page cache is warm for every timed run, exactly as
        // `corpus/bench.py` does, then RUNS timed runs.
        let mut passes: Vec<Pass> = Vec::with_capacity(RUNS);
        measure(&directory);
        for _ in 0..RUNS {
            passes.push(measure(&directory));
        }
        passes.sort_unstable_by_key(|pass| pass.total());
        let middle = passes[RUNS / 2];

        let budget = manifest.budget();
        println!(
            "{files:>6} {:>8} {:>7} {:>11} {:>8} {:>8.3}s {:>8.3}s {:>8.3}s {:>8.3}s {:>8.3}s {:>8.3}s {:>12}  fp={:016x}",
            manifest.lines,
            middle.blocks,
            middle.comparisons,
            middle.clusters,
            budget.as_secs_f64(),
            middle.total().as_secs_f64(),
            passes[0].total().as_secs_f64(),
            passes[RUNS - 1].total().as_secs_f64(),
            middle.extracting.as_secs_f64(),
            middle.detecting.as_secs_f64(),
            middle.detecting.as_nanos() / middle.comparisons as u128,
            manifest.fingerprint,
        );

        // Assert — collected rather than asserted in place, so one size over budget still reports
        // the other two. A gate that stops at the first failure hides the shape of the curve, and
        // the shape is what says whether the cost is in the files or in their square.
        if over_budget(middle.total(), budget) {
            failures.push(format!(
                "{files} files / {} lines took {:.3}s against a {:.3}s budget",
                manifest.lines,
                middle.total().as_secs_f64(),
                budget.as_secs_f64()
            ));
        }
        // Same guard as the shape test, at every size: a run that stopped being adversarial would
        // come in comfortably under budget and prove nothing at all.
        assert_eq!(
            middle.comparisons,
            all_pairs(files),
            "the fixture stopped being adversarial at {files} files"
        );
        assert_eq!(
            middle.clusters, 1,
            "one shared header is one finding, at every size"
        );
        fs::remove_dir_all(&directory).expect("the scratch tree is removable");
    }

    assert!(
        failures.is_empty(),
        "the {BUDGET_SECONDS_PER_100K_LINES} s / {BUDGET_LINES:.0} line budget was exceeded:\n  {}",
        failures.join("\n  ")
    );
}

/// What one timed pass observed.
///
/// The two halves are timed separately and not only summed: the budget is one number, but "read +
/// parse + extract" grows with the *lines* and "detect" grows with the square of the *blocks*, and
/// a single total cannot say which of the two is spending it. Anyone deciding what to do about an
/// overrun needs that split before they can choose, so the instrument reports it rather than
/// leaving it to be re-derived.
#[derive(Debug, Clone, Copy)]
struct Pass {
    /// Prose blocks the extractor produced.
    blocks: usize,
    /// Pairs a Jaccard score was computed for.
    comparisons: usize,
    /// Findings — one per connected component of two or more blocks.
    clusters: usize,
    /// Read + parse + extract.
    extracting: Duration,
    /// Candidate generation + scoring + clustering.
    detecting: Duration,
}

impl Pass {
    /// What the budget is compared against: the whole pass.
    fn total(self) -> Duration {
        self.extracting + self.detecting
    }
}

/// One timed pass: read + parse + extract, then detect.
///
/// `black_box` on both ends so the optimiser cannot hoist the work out of a call whose result is
/// only read back as a handful of integers.
fn measure(directory: &Path) -> Pass {
    let extract_started = Instant::now();
    let blocks = read_blocks(black_box(directory));
    let extracting = extract_started.elapsed();

    let detect_started = Instant::now();
    let report = duplicates(black_box(&blocks));
    let detecting = detect_started.elapsed();

    let pass = Pass {
        blocks: blocks.len(),
        comparisons: report.comparisons,
        clusters: report.clusters.len(),
        extracting,
        detecting,
    };
    black_box(&report);
    pass
}
