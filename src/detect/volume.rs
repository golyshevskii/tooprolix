//! Prose volume: one block of prose that is longer than the repository allows itself.
//!
//! `TPX001` is a comment run over [`Limits::comment_max_volume`], `TPX002` a docstring over
//! [`Limits::docstring_max_volume`]. The rule codes themselves, like every other user-facing
//! string, belong to `build-cli-with-exit-contract-and-rule-codes`; they are named here only so the
//! two limits can be told apart in prose.
//!
//! # The unit is WORDS, and neither the key nor the type name says so
//!
//! `comment-max-volume` / `docstring-max-volume` are counted in **normalised words**
//! ([`ProseBlock::size_words`]), never in physical lines. The word "volume" carries no unit, so
//! every place that renders or documents a limit has to say "words" out loud — including task 6's
//! `--help`. Words rather than lines is `corpus/REPORT.md` §4's own condition for reviving this
//! rule: `w_p90` spans 25-49 across the corpus against `line_p90` 6-19, so words
//! separate the population better. [`ProseBlock::size_lines`] survives in the finding as a
//! reference figure for output, and is never compared against a limit.
//!
//! # The boundary is `>`, because the key is named `max`
//!
//! A block at exactly the limit is **silent**: the number is the largest allowed size, not the
//! smallest flagged one. This is ruff's own reading of a `max-` setting, taken from the pinned
//! checkout rather than from memory —
//! `crates/ruff_linter/src/rules/mccabe/rules/function_is_too_complex.rs:175` is
//! `if complexity > max_complexity`, rendered as `({complexity} > {max_complexity})`.
//!
//! # Where the defaults come from, and what they do NOT claim
//!
//! **The limits were chosen by the price of false positives; the true-positive population is not
//! measured.** Neither this task nor `validate-detectors-on-reference-corpus` labelled a single
//! genuine volume finding — that task's precision AC covers `TPX003` only. So nothing below is
//! evidence that a flagged block *should* have been flagged, and a green AC5 on the reference
//! repository means "large prose there carries an explicit author's acknowledgement", **not** "the
//! limits produce no false positives". Zero on that repository is reached by opt-out markers, which
//! `build-cli-with-exit-contract-and-rule-codes` owns.
//!
//! Two numbers were measured, and a limit needs both: what it costs to silence a repository that
//! has already been audited, and whether it fires at all. A limit high enough to find nothing
//! anywhere passes every local test and is a failed choice, not a safe one.
//!
//! **Ceiling — how many blocks a limit asks an author to acknowledge.** Measured on a repository
//! whose prose had already been audited by hand, so every finding there is a block a human had
//! looked at and kept:
//!
//! | `docstring-max-volume` | `TPX002` | `comment-max-volume` | `TPX001` |
//! |---|---|---|---|
//! | 150 | 15 | 100 | 6 |
//! | **200** | **8** | **150** | **2** |
//! | 250 | 3 | 200 | 1 |
//! | 300 | 2 | 250 | 1 |
//!
//! At the defaults that is **10** markers. One of them is a long, deliberate, correct block that
//! the issue names as must-never-flag; its marker is the mechanism working, not the limit failing.
//!
//! **Floor — findings per corpus repository at the defaults**, from the same run:
//!
//! | repository | docstring blocks | `TPX002` | comment blocks | `TPX001` |
//! |---|---|---|---|---|
//! | `openai-agents-python` | 867 | 14 | 647 | 2 |
//! | `crewAI` | 2996 | 21 | 385 | 0 |
//! | `langgraph` | 1336 | 74 | 792 | 2 |
//! | `OpenHands` | 2307 | 12 | 1007 | 3 |
//! | `pydantic` | 919 | 46 | 752 | 1 |
//! | `requests` | 173 | 3 | 170 | 0 |
//!
//! `TPX002` fires on every repository in the corpus, `TPX001` on all but two — `crewAI` and
//! `requests` have no comment run over 150 words at all (their longest are 97 and 88).
//!
//! **Why the two defaults differ.** The populations do. Over the whole corpus a common limit of 200
//! flags 1.99% of the 8960 docstring blocks and 0.076% of the 3930 comment blocks — a factor of 26
//! — because a comment run is a shorter medium by nature. 150 for comments puts them at 0.25%,
//! still an order of magnitude below docstrings, and costs one extra marker on the reference
//! repository. Matching the docstring *rate* would need a comment limit near 100, which costs 6
//! markers there instead of 2 and puts the total at 14, twice the duplicate detector's price.
//!
//! **Why defaults that fire rather than defaults that are quiet.** `corpus/REPORT.md` §4 shut this
//! rule down partly because a global threshold with no per-repository calibration is indefensible.
//! `comment-max-volume` and `docstring-max-volume` are that calibration, so a repository that finds
//! the default too loud — `langgraph` at 74 `TPX002` findings is the measured worst case — raises
//! its own number instead of switching the rule off. That escape hatch is what makes a useful
//! default the right side to err on.
//!
//! # A block one physical line long is invisible here, whatever its length
//!
//! [`crate::extract::extract`] applies [`crate::extract::MIN_BLOCK_LINES`] `AND`
//! [`crate::extract::MIN_BLOCK_WORDS`] before any detector runs, and those two were measured for
//! the *duplicate* detector. The line half is the one that can hide a volume finding, and it is
//! deliberately left alone rather than forked into a second extraction path: measured over all
//! eight corpus checkouts, the one-line blocks it hides number **1 above 50 words** (in
//! `OpenHands`), **1 above 100**, and **0 above 150** — so at both defaults the population hidden
//! from this module is **empty**. One block corpus-wide is not worth two owners of one extraction
//! contract.
//!
//! # What this module does not own
//!
//! Rule codes and the `# !TPX00N` marker belong to [`crate::rules`]; the JSON document to
//! [`crate::finding`]; exit codes and `--help` to [`crate::cli`]; and the `[tool.tooprolix]` keys
//! that fill [`Limits`] to [`crate::config`], which is where `KNOWN_KEYS` and `from_document` live.
//! Nothing here reads a file or walks a directory. Two decisions belong to those modules and must
//! not be re-decided there:
//!
//! * **one block can be both an overrun and a member of a `TPX003` cluster, and both findings must
//!   reach the output.** Deduplicating by coordinate across rule codes would silently eat one of
//!   them;
//! * **the rendered finding must name the opt-out marker as the remedy, and must name the unit.**
//!   Without it the cheap way to go green is to split one docstring into three, which is the
//!   behaviour the issue exists to prevent — the rule is there to make long prose deliberate, not
//!   to make it shorter.

use std::fmt;

use crate::extract::{ProseBlock, ProseKind, write_address};

/// Default for [`Limits::docstring_max_volume`], in normalised words: `TPX002`.
///
/// Measured, not chosen — see the module documentation for the two tables behind it. In short:
/// 8 markers on an audited repository against 7 for the duplicate detector, and findings
/// on all seven corpus repositories.
pub const DEFAULT_DOCSTRING_MAX_VOLUME: usize = 200;

/// Default for [`Limits::comment_max_volume`], in normalised words: `TPX001`.
///
/// Lower than [`DEFAULT_DOCSTRING_MAX_VOLUME`] because the comment population is shorter, not
/// because comments deserve a harsher rule — at a common limit of 200 this rule would reach 0.076%
/// of the corpus's comment blocks against 1.99% of its docstrings.
pub const DEFAULT_COMMENT_MAX_VOLUME: usize = 150;

/// The largest prose block, **in normalised words**, that each kind may carry without a finding.
///
/// One field per `[tool.tooprolix]` key, named the same, so that
/// `build-cli-with-exit-contract-and-rule-configs` can map the parsed TOML onto this without a
/// translation table that could drift. Deliberately **not** `#[non_exhaustive]`: the CLI has to
/// build one from parsed configuration with a struct literal, which `#[non_exhaustive]` forbids
/// across crates. The price is that adding a third limit is a breaking change, which is the honest
/// trade for a config type.
///
/// [`Self::default`] is the corpus-measured pair, so a caller with no configuration behaves exactly
/// as the calibration says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest allowed comment run, in normalised words. Above it, `TPX001`.
    pub comment_max_volume: usize,
    /// Largest allowed docstring, in normalised words. Above it, `TPX002`.
    pub docstring_max_volume: usize,
}

impl Default for Limits {
    /// The corpus-measured defaults, and the only place they are assembled.
    fn default() -> Self {
        Self {
            comment_max_volume: DEFAULT_COMMENT_MAX_VOLUME,
            docstring_max_volume: DEFAULT_DOCSTRING_MAX_VOLUME,
        }
    }
}

impl Limits {
    /// The limit that applies to `kind`, in normalised words.
    ///
    /// The single owner of the kind-to-limit mapping: a second `match` anywhere else is how a
    /// docstring quietly starts being measured against the comment limit.
    #[must_use]
    pub const fn max_volume(self, kind: ProseKind) -> usize {
        match kind {
            ProseKind::Comment => self.comment_max_volume,
            ProseKind::Docstring => self.docstring_max_volume,
        }
    }
}

/// One finding: one block of prose longer than its kind is allowed to be.
///
/// **Not a [`crate::detect::duplicate::Cluster`], and it must not become one.** A cluster is a
/// connected component of two or more members by construction — `duplicates` drops a component that
/// collapses to a single address — so wrapping a single over-long block in one would mean either a
/// permanently dead field or a member list that lies about why the finding exists.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Overrun<'a> {
    /// The block itself: its path, its span, its `kind`, and its text.
    ///
    /// The address and the kind are read from here rather than copied out, so a finding cannot
    /// disagree with the block it came from. [`ProseBlock::size_lines`] is reachable through it as
    /// the reference figure for output — it is never what the limit was compared against.
    pub block: &'a ProseBlock,
    /// [`ProseBlock::size_words`] of [`Self::block`]: the measured size, in normalised words.
    ///
    /// Stored rather than recomputed by the consumer so that the number a finding *renders* is the
    /// same number the comparison was *made* on, and the unit cannot be reinterpreted downstream.
    pub words: usize,
    /// The limit [`Self::words`] exceeded, in normalised words, from [`Limits`].
    ///
    /// Carried per finding because task 6 renders `words > max` and would otherwise have to reach
    /// back for the configuration to know which of the two limits applied.
    pub max_volume: usize,
}

impl fmt::Display for Overrun<'_> {
    /// A one-line rendering, for tests and diagnostics.
    ///
    /// **Not the user-facing line** — the rule code, the column, JSON and the marker advice belong
    /// to `build-cli-with-exit-contract-and-rule-codes`. It carries the unit and every ordering
    /// field **except one**: the normalised text is in the sort key and is deliberately not
    /// rendered, because a finding is over-long prose by definition and inlining 300 words would
    /// make the probe unreadable for the one case it would disambiguate.
    ///
    /// That omission does not weaken the byte-for-byte determinism probe the AC4 test compares,
    /// and the reason is worth stating rather than assuming: two findings that the text separates
    /// but this rendering does not must be equal on path, span, kind, size and limit, so they
    /// render to the *same bytes* — whichever order they end up in, the output is unchanged. The
    /// probe cannot observe that half of the key, and nothing it fails to observe can move the
    /// bytes. What it does observe is pinned by `the_output_is_a_function_of_the_input_set`, whose
    /// two same-coordinate findings differ in size and therefore in rendering.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_address(
            formatter,
            &self.block.path.display(),
            self.block.line_start,
            self.block.line_end,
        )?;
        write!(
            formatter,
            ": {} prose volume ({} words > max {}, {} lines)",
            self.block.kind.as_str(),
            self.words,
            self.max_volume,
            self.block.size_lines(),
        )
    }
}

/// Everything one volume scan produced.
///
/// `Clone` and `PartialEq` are here so that
/// `build-cli-with-exit-contract-and-rule-codes` can `assert_eq!` two reports directly instead of
/// comparing renderings. Both are free — [`Overrun`] already carries them, and its `block` is a
/// shared reference, which is `Copy` and compares through to [`ProseBlock`]'s own `PartialEq`.
/// Deliberately **no** `Eq`, `Hash` or `serde` derives: serialisation is that task's decision and
/// is not pre-empted here.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Report<'a> {
    /// The findings, ordered by `ProseBlock::coordinates` and then by normalised text, and
    /// therefore byte-identical for any arrival order of the same blocks.
    ///
    /// The text is in the key for the same reason it is in `duplicate`'s: two blocks can share a
    /// coordinate when a caller labels two sources with one path, and a coordinate-only key leaves
    /// their order to the input. Nothing is deduplicated here — unlike a cluster, two findings at
    /// one address are two real findings.
    pub overruns: Vec<Overrun<'a>>,
}

/// Every block longer than `limits` allows its kind to be, sorted.
///
/// `blocks` is whatever [`crate::extract::extract`] produced, for one file or for a whole
/// repository. Nothing is re-filtered and nothing is read from disk. Pass [`Limits::default`] for
/// the corpus-measured defaults; `build-cli-with-exit-contract-and-rule-codes` passes what
/// `[tool.tooprolix]` said.
///
/// A block of exactly `max_volume` words is **not** a finding: the limit is the largest allowed
/// size.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use tooprolix::detect::volume::{Limits, volume};
/// use tooprolix::extract::extract;
///
/// // 202 normalised words in a module docstring, against a default limit of 200.
/// let source = format!("\"\"\"Overview.\n{}\"\"\"\n", "word ".repeat(201));
/// let blocks = extract(Path::new("api.py"), &source)?;
///
/// let report = volume(&blocks, Limits::default());
/// assert_eq!(report.overruns.len(), 1);
/// assert_eq!(report.overruns[0].words, 202);
/// assert_eq!(report.overruns[0].max_volume, 200);
///
/// // The same blocks under a limit this repository raised for itself: silence.
/// let relaxed = Limits { docstring_max_volume: 500, ..Limits::default() };
/// assert!(volume(&blocks, relaxed).overruns.is_empty());
/// # Ok::<(), tooprolix::Error>(())
/// ```
#[must_use]
pub fn volume<'a>(blocks: &'a [ProseBlock], limits: Limits) -> Report<'a> {
    let mut overruns: Vec<Overrun<'a>> = blocks
        .iter()
        .filter_map(|block| {
            let max_volume = limits.max_volume(block.kind);
            let words = block.size_words();
            (words > max_volume).then_some(Overrun {
                block,
                words,
                max_volume,
            })
        })
        .collect();

    // Sorted on the way out, never relying on the order `extract` happened to return: the output
    // has to be a function of the input SET. The normalised text breaks the tie that coordinates
    // alone leave open, and two findings equal on both render identically, so the bytes are stable
    // even then.
    overruns.sort_by(|left, right| {
        (left.block.coordinates(), &left.block.normalized)
            .cmp(&(right.block.coordinates(), &right.block.normalized))
    });
    Report { overruns }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        DEFAULT_COMMENT_MAX_VOLUME, DEFAULT_DOCSTRING_MAX_VOLUME, Limits, Overrun, volume,
    };
    use crate::extract::{ProseKind, extract};

    /// A module docstring of exactly `words` normalised words, across two physical lines.
    ///
    /// Two lines, not one, because [`crate::extract::MIN_BLOCK_LINES`] is 2 and a one-line block
    /// never reaches a detector at all — the limitation this module's rustdoc records.
    fn docstring_of(words: usize) -> String {
        let body: Vec<String> = (1..=words).map(|index| format!("w{index}")).collect();
        format!("\"\"\"{}\n{}\"\"\"\n", body[0], body[1..].join(" "))
    }

    /// An own-line comment run of exactly `words` normalised words, across two physical lines.
    fn comment_of(words: usize) -> String {
        let body: Vec<String> = (1..=words).map(|index| format!("w{index}")).collect();
        format!("# {}\n# {}\n", body[0], body[1..].join(" "))
    }

    /// Every block of `source`, as `extract` hands them to a detector.
    fn blocks_of(path: &str, source: &str) -> Vec<crate::extract::ProseBlock> {
        extract(Path::new(path), source).expect("the fixture is valid Python")
    }

    /// The rendered findings, which is what the determinism test compares byte for byte.
    fn rendered(found: &[Overrun<'_>]) -> String {
        found
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The defaults are the two measured constants, and `Default` is not free to disagree.
    ///
    /// This is the test that names the numbers. It is **not** the only thing standing between the
    /// crate and a silent rule, and the claim that it was is a claim that was checked: raising both
    /// constants to 10 000 and running `make rust.test` reddens six things, not one — this test,
    /// `a_docstring_one_word_over_the_limit_is_a_finding`,
    /// `a_comment_one_word_over_the_limit_is_a_finding`, `the_two_limits_are_not_interchangeable`,
    /// `the_output_is_a_function_of_the_input_set`, and the module-level doctest on [`volume`].
    /// Every one of those sizes a fixture from a literal and then asserts a *finding*, so a raised
    /// default takes the finding away.
    ///
    /// Drift the other way and the suite reacts harder still: at 100/100, seven of the nine tests
    /// in this module fail, the two silence tests among them. Only
    /// `the_limits_are_read_from_the_argument`, which passes both limits explicitly, and
    /// `a_canonical_short_docstring_is_silent`, whose fixture is far below either number, survive
    /// in both directions. So the honest statement of this test's job is not "without it the rule
    /// could go silent" — it is that it names *which* numbers, so a drift reports itself as one
    /// changed constant instead of as five fixtures that mysteriously stopped finding things.
    #[test]
    fn the_default_limits_are_the_measured_constants() {
        // Arrange / Act
        let limits = Limits::default();

        // Assert
        assert_eq!(limits.docstring_max_volume, 200);
        assert_eq!(limits.comment_max_volume, 150);
        assert_eq!(limits.docstring_max_volume, DEFAULT_DOCSTRING_MAX_VOLUME);
        assert_eq!(limits.comment_max_volume, DEFAULT_COMMENT_MAX_VOLUME);
    }

    /// A docstring of exactly the limit is silent: the number is the largest **allowed** size.
    ///
    /// The word counts are literals and not `DEFAULT_DOCSTRING_MAX_VOLUME`, deliberately. Sizing a
    /// fixture from the constant would move the fixture with the constant, and the test could then
    /// never fail for a limit that drifted — the one thing it exists to catch.
    #[test]
    fn a_docstring_of_exactly_the_limit_is_silent() {
        // Arrange
        let blocks = blocks_of("api.py", &docstring_of(200));
        assert_eq!(blocks[0].size_words(), 200, "the fixture must be exact");

        // Act
        let report = volume(&blocks, Limits::default());

        // Assert
        assert!(
            report.overruns.is_empty(),
            "200 words is the maximum ALLOWED, not the smallest flagged: {:?}",
            report.overruns
        );
    }

    /// One word over it is a finding.
    #[test]
    fn a_docstring_one_word_over_the_limit_is_a_finding() {
        // Arrange
        let blocks = blocks_of("api.py", &docstring_of(201));
        assert_eq!(blocks[0].size_words(), 201, "the fixture must be exact");

        // Act
        let report = volume(&blocks, Limits::default());

        // Assert
        assert_eq!(report.overruns.len(), 1);
        assert_eq!(report.overruns[0].block.kind, ProseKind::Docstring);
        assert_eq!(report.overruns[0].words, 201);
        assert_eq!(report.overruns[0].max_volume, 200);
    }

    /// The comment limit is its own number, pinned at its own boundary.
    #[test]
    fn a_comment_of_exactly_the_limit_is_silent() {
        // Arrange
        let blocks = blocks_of("api.py", &comment_of(150));
        assert_eq!(blocks[0].size_words(), 150, "the fixture must be exact");

        // Act
        let report = volume(&blocks, Limits::default());

        // Assert
        assert!(
            report.overruns.is_empty(),
            "150 words is the maximum ALLOWED for a comment run: {:?}",
            report.overruns
        );
    }

    /// One word over it is a finding.
    #[test]
    fn a_comment_one_word_over_the_limit_is_a_finding() {
        // Arrange
        let blocks = blocks_of("api.py", &comment_of(151));
        assert_eq!(blocks[0].size_words(), 151, "the fixture must be exact");

        // Act
        let report = volume(&blocks, Limits::default());

        // Assert
        assert_eq!(report.overruns.len(), 1);
        assert_eq!(report.overruns[0].block.kind, ProseKind::Comment);
        assert_eq!(report.overruns[0].words, 151);
        assert_eq!(report.overruns[0].max_volume, 150);
    }

    /// The two limits are not interchangeable, and this reddens if they are swapped.
    ///
    /// 175 words sits between them: over the comment limit, under the docstring one. Swapping the
    /// two fields inverts both assertions at once, which is what makes this the AC2 guard rather
    /// than a third copy of the boundary tests.
    #[test]
    fn the_two_limits_are_not_interchangeable() {
        // Arrange
        let mut blocks = blocks_of("doc.py", &docstring_of(175));
        blocks.extend(blocks_of("cmt.py", &comment_of(175)));

        // Act
        let report = volume(&blocks, Limits::default());

        // Assert
        assert_eq!(
            report.overruns.len(),
            1,
            "at 175 words only the comment is over its own limit: {:?}",
            report.overruns
        );
        assert_eq!(report.overruns[0].block.kind, ProseKind::Comment);
        assert_eq!(report.overruns[0].block.path, Path::new("cmt.py"));
    }

    /// The limits are an INPUT, not a constant the function reads behind the caller's back.
    ///
    /// Without this, `volume` could ignore its `limits` argument entirely and consult the two
    /// constants directly; every test above passes `Limits::default()`, so none of them could tell.
    /// That is the whole point of the signature the CLI has to configure through.
    #[test]
    fn the_limits_are_read_from_the_argument() {
        // Arrange — one block, and two configurations that must disagree about it.
        let blocks = blocks_of("api.py", &docstring_of(175));
        let strict = Limits {
            docstring_max_volume: 100,
            ..Limits::default()
        };
        let relaxed = Limits {
            docstring_max_volume: 1000,
            ..Limits::default()
        };

        // Act
        let under_strict = volume(&blocks, strict);
        let under_relaxed = volume(&blocks, relaxed);

        // Assert
        assert_eq!(under_strict.overruns.len(), 1, "a raised limit was ignored");
        assert_eq!(under_strict.overruns[0].max_volume, 100);
        assert!(
            under_relaxed.overruns.is_empty(),
            "a lowered limit was ignored: {:?}",
            under_relaxed.overruns
        );
    }

    /// The canonical short docstring the issue names must be silent under **both** codes.
    #[test]
    fn a_canonical_short_docstring_is_silent() {
        // Arrange — the shape the issue says a volume rule must never touch.
        let source = "\"\"\"Return the parsed config.\n\nRaises ValueError when the file is not TOML.\n\"\"\"\n";
        let blocks = blocks_of("config.py", source);
        assert!(
            !blocks.is_empty(),
            "the fixture must produce a block at all"
        );

        // Act
        let report = volume(&blocks, Limits::default());

        // Assert
        assert!(
            report.overruns.is_empty(),
            "short canonical prose is not a finding: {:?}",
            report.overruns
        );
    }

    /// The output is a function of the input **set**, and in the documented **order**.
    ///
    /// The order is asserted as one literal rather than as "sorted somehow", because every weaker
    /// form of this test has already been shown to pass on a wrong sort. The fixture is built so
    /// that path order and size order **disagree**, which is what the first version got wrong: its
    /// three files were `a`/`b`/`c` at 160/220/300 words, so sorting by size and sorting by
    /// coordinate produced the same bytes and `overruns.sort_by(|l, r| l.words.cmp(&r.words))`
    /// passed the whole suite. Three properties are pinned here, each with its own failing mutant:
    ///
    /// * `a.py` is the **largest** block and still comes first — sorting by [`Overrun::words`]
    ///   moves it to the end;
    /// * `b.py` and `c.py` carry comment runs of the **same 160 words** in different files, and
    ///   `dup.py` carries two findings at the **same coordinate** — every key coarser than the real
    ///   one leaves a tie, and a tie under a stable sort means the arrival order survives into the
    ///   output, which the reversed and rotated inputs then catch;
    /// * the two `dup.py` blocks share a path, a span and a kind, so only the normalised text
    ///   separates them — dropping that clause from the key is what they exist to redden. The
    ///   clause was documented at the sort and untested until now.
    ///
    /// The fixture is non-empty on purpose: an implementation that returned nothing would satisfy
    /// any equality between two empty renderings, so the literal is the load-bearing half.
    ///
    /// # Why there is no "call it twice and compare"
    ///
    /// The task's AC4 prescribes, in as many words, "two runs over a non-empty set → byte-identical
    /// output". The **intent** — determinism, proven on output that is not empty — is right and is
    /// exactly what this test carries. The prescribed **mechanism** cannot carry it: [`volume`] is a
    /// pure function of `(&[ProseBlock], Limits)` with no hash container anywhere in it, so two
    /// calls on one input are equal by construction — *including when the ordering is wrong*.
    ///
    /// That is measured, not argued. With `overruns.sort_by(|l, r| l.words.cmp(&r.words))` in
    /// place, an isolated `forwards == again` probe over this same six-finding fixture reported
    /// `ok` while the permutation assertions below reported `FAILED`. An assertion that stays green
    /// through the mutation it exists to catch is not a weak test, it is not a test.
    ///
    /// [`crate::extract`] learned this first and wrote it down at
    /// `blocks_are_ordered_by_line` — *"There was one, and it could not fail… Do not re-add it."* —
    /// and the same shape was re-added here one module over regardless. Hence this section rather
    /// than a silent deletion: the tension between AC4's letter and AC4's intent is named, and
    /// resolved in favour of the intent. **Do not restore the two-run comparison.** What replaces
    /// it, and what actually fails when determinism breaks, is the pair of permutation runs below
    /// — the same input reversed and rotated — plus the non-empty count.
    #[test]
    fn the_output_is_a_function_of_the_input_set() {
        // Arrange — six findings whose arrival order matches neither the expected order, nor the
        // order by size, nor the order by limit.
        let mut blocks = blocks_of("c.py", &comment_of(160));
        blocks.extend(blocks_of("a.py", &docstring_of(300)));
        blocks.extend(blocks_of("dup.py", &docstring_of(230)));
        blocks.extend(blocks_of("b.py", &comment_of(160)));
        blocks.extend(blocks_of("dup.py", &docstring_of(210)));
        blocks.extend(blocks_of("c.py", &docstring_of(220)));

        // Act
        let forwards = rendered(&volume(&blocks, Limits::default()).overruns);
        let mut reversed = blocks.clone();
        reversed.reverse();
        let backwards = rendered(&volume(&reversed, Limits::default()).overruns);
        let mut rotated_input = blocks.clone();
        rotated_input.rotate_left(1);
        let rotated = rendered(&volume(&rotated_input, Limits::default()).overruns);

        // Assert — the exact bytes, in the exact documented order.
        assert_eq!(
            forwards,
            "a.py:1-2: docstring prose volume (300 words > max 200, 2 lines)\n\
             b.py:1-2: comment prose volume (160 words > max 150, 2 lines)\n\
             c.py:1-2: docstring prose volume (220 words > max 200, 2 lines)\n\
             c.py:1-2: comment prose volume (160 words > max 150, 2 lines)\n\
             dup.py:1-2: docstring prose volume (210 words > max 200, 2 lines)\n\
             dup.py:1-2: docstring prose volume (230 words > max 200, 2 lines)",
            "the output must be ordered by coordinate and then by normalised text, and must carry \
             the unit"
        );
        assert_eq!(forwards.lines().count(), 6, "the probe must not be empty");
        assert_eq!(
            forwards, backwards,
            "reversing the input changed the output"
        );
        assert_eq!(forwards, rotated, "rotating the input changed the output");
    }
}
