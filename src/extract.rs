//! Prose block extraction — the contract both detectors consume.
//!
//! This module is the single owner of the answer to "what is one prose block?". Both shipped
//! detectors — [`crate::detect::duplicate`] (`implement-duplicate-prose-detector`) and
//! [`crate::detect::volume`] (`implement-prose-volume-detectors`) — consume that answer; they do
//! not redefine it. Extraction compares nothing: no similarity, no scoring.
//!
//! The third consumer this line used to name, `implement-code-restatement-detector`, **does not
//! exist and will not**: that task closed NO-SHIP. It is named here only so the next reader does
//! not go looking for it, and so the count above reads as measured rather than as forgotten.
//!
//! # The contract
//!
//! ## Sources
//!
//! `tooprolix` reads Python source files and nothing else: the prose it checks is the comments and
//! docstrings that live beside Python code.
//!
//! * **Python** — any path whose extension is `py` (case-insensitively). That is the whole list.
//! * Anything else is [`Error::UnsupportedSource`] — **not** an empty result. An extractor that
//!   silently returns nothing for a file it cannot classify hides real prose, and it hides it in a
//!   way no green test suite can show.
//!
//! Every constant below was measured on Python sources only (`corpus/measure.py` walks
//! `rglob("*.py")`), so extending this list is a decision that needs its own measurement first.
//!
//! ## Python: docstrings
//!
//! A docstring is a string literal standing alone as the *first* statement of a module, class or
//! function body — the rule ruff applies in `crates/ruff_linter/src/docstrings/extraction.rs`.
//! Anything else in first position (an import, a bare name, `42`) means there is no docstring.
//! The whole statement tree is walked, so a `def` nested in another `def`, in a `class`, or inside
//! an `if`/`try`/`with`/`for`/`match` body is reached; a scan of the module body alone loses those
//! (this is the `radon multi` undercount recorded in `corpus/REPORT.md`).
//!
//! ## Python: comments
//!
//! Comments are read from the parsed **token stream** (`TokenKind::Comment`), the way ruff does in
//! `crates/ruff_python_index/src/indexer.rs`. They are not "only in the trivia".
//!
//! * **Own-line comments on consecutive physical lines glue into one block.** A blank line, a line
//!   of code, or an excluded comment between them ends the run, because the lines are then no
//!   longer consecutive.
//! * **A trailing comment (`x = 1  # why`) never joins a run.** It is prose about one statement,
//!   not about the lines below it. It is also, by construction, exactly one physical line — and the
//!   line half of the size conjunction below excludes every one-line block — so a trailing comment
//!   can never reach the output. It is therefore skipped rather than emitted and immediately
//!   filtered: the observable half of this rule is that it does not *glue*, and that is what
//!   `a_trailing_comment_is_not_glued_to_the_following_comment_run` pins.
//!   Own-line versus trailing is decided by `ruff_python_trivia::has_leading_content`, the same
//!   function ruff uses for the same question.
//! * **Not prose, excluded** — five machine directives: a `#!` shebang on **line 1** (keyed on the
//!   line, not on byte offset 0, so a UTF-8 BOM cannot smuggle it in), an encoding cookie on line 1
//!   or 2, `# noqa…`, `# type: …`, and our own opt-out marker, which `README.md` ships as part of
//!   the 0.1.0 contract.
//!   The exclusion is load-bearing rather than cosmetic: four glued pragma lines are 4 lines and 13
//!   normalised words, so they pass the size conjunction on their own.
//! * **Interaction with the opt-out**: only the marker LINE is excluded here, so the block it
//!   suppresses starts on the line *below* the marker. Extraction still does not **parse** the
//!   marker (`# !TPX001`) and must not — it asks [`crate::rules::is_marker`], which owns it, rather than
//!   spelling a second, nearly-identical test of its own. It did spell one (`starts_with`
//!   `"tooprolix:"`), the two disagreed in both directions, and each disagreement moved a reported
//!   word count with no diagnostic; see
//!   `a_comment_is_excluded_as_a_marker_exactly_when_the_marker_parser_accepts_it`.
//!
//! ## Normalisation
//!
//! [`normalize`] lowercases, replaces every non-alphanumeric character with a space, and collapses
//! whitespace. That kills the difference between `# Foo bar.` and `"""foo   bar"""`, which is the
//! only reason a comment and a docstring can be compared at all.
//!
//! **Deviation from the task file, deliberate.** The task prescribes
//! `" ".join(text.split()).lower()` plus explicit stripping of `# ` prefixes and docstring quotes.
//! This is `corpus/measure.py::normalise` instead, which is stricter and subsumes it: markers and
//! quotes are non-alphanumeric, so they disappear without a special case. The reason to prefer it
//! is that the two constants this module and the duplicate detector inherit — `>= 8` words here and
//! Jaccard `>= 0.75` there — were *measured* with this normaliser. Consuming them under a different
//! one would silently change what they mean (`0.1.0` is three words here, one there).
//!
//! ## Minimum block size
//!
//! [`MIN_BLOCK_LINES`] `AND` [`MIN_BLOCK_WORDS`], never either alone. See their rustdoc.

use std::fmt;
use std::path::{Path, PathBuf};

use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{self as ast, Stmt};
use ruff_python_parser::{ParseError, parse_module};
use ruff_python_trivia::has_leading_content;
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};
use thiserror::Error as ThisError;

/// Minimum physical line span of a block, in `line_end - line_start + 1`.
///
/// Measured in `corpus/REPORT.md`: **741 861 of the 741 970** exact duplicate prose pairs found in
/// the `OpenHands` checkout are one-line blocks. Without this half, the duplicate detector reports
/// hundreds of copies of `"""Initialize the class."""`.
pub const MIN_BLOCK_LINES: usize = 2;

/// Minimum number of [`normalize`]d words in a block.
///
/// Measured in `corpus/REPORT.md`: 6, 8 and 10 words yield the *same* 8 candidates on the reference
/// repository (the sets are nested), 12 yields 7 and 16 yields 6. 8 is the conservative end of that
/// measured 6–10 plateau, so moving it needs new numbers, not an opinion.
pub const MIN_BLOCK_WORDS: usize = 8;

/// Everything that can go wrong while reading prose out of a source file.
///
/// `#[non_exhaustive]`: `build-cli-with-exit-contract-and-rule-codes` adds an `Io` variant when it
/// starts reading files from disk. Before 0.1.0 is published that is one line; after, it is a semver
/// break, and release-plz is already wired to tag.
#[derive(Debug, ThisError)]
#[non_exhaustive]
pub enum Error {
    /// The source is not valid Python, so no prose could be read from it.
    ///
    /// Wrapped rather than re-exported: `ruff_python_parser` is an internal 0.0.x crate we pin
    /// exactly, and leaking its error type into our public API would make every caller depend on
    /// that pin too.
    ///
    /// This is an error and not an empty result on purpose. A syntax error must never be readable
    /// as "this file has no prose".
    ///
    /// **The text after `could not parse Python source:` comes from a `0.0.x` dependency and is NOT
    /// part of tooprolix's contract** — a patch bump may reword it. Do not build exit codes, rule
    /// codes or any parsing on top of it; match on the variant.
    #[error("could not parse Python source: {0}")]
    Parse(#[from] ParseError),

    /// The path is not a Python file.
    ///
    /// An error rather than an empty result: the caller chose the file, so a file the extractor has
    /// no rule for is a bug in the caller, not a file without prose.
    #[error("{0} is not a source tooprolix extracts prose from")]
    UnsupportedSource(PathBuf),

    /// The file could not be read from disk, or is not UTF-8.
    ///
    /// The variant this enum's `#[non_exhaustive]` was written for, added by
    /// `build-cli-with-exit-contract-and-rule-codes` together with [`read_source`], its only
    /// producer. It exists so that "I could not read this file" and "this file has no prose" cannot
    /// be the same answer — the same reason [`Self::Parse`] is an error rather than an empty
    /// result, one layer further out.
    ///
    /// Non-UTF-8 content arrives here too, as `InvalidData`. Python source is UTF-8 unless it
    /// declares otherwise, and honouring an encoding cookie is a decision with no measurement
    /// behind it yet; failing loudly on the handful of files that would need it is the honest
    /// placeholder.
    #[error("could not read {}: {source}", path.display())]
    Io {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying failure, kind intact — the pyo3 boundary maps on it.
        source: std::io::Error,
    },
}

/// Which syntax carried the prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProseKind {
    /// A module, class or function docstring.
    Docstring,
    /// One or more glued own-line `#` comments.
    Comment,
}

impl ProseKind {
    /// The lower-case name of this kind, for reports and for the Python boundary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Docstring => "docstring",
            Self::Comment => "comment",
        }
    }
}

/// Writes one block's address: `path:line`, or `path:line-end_line` when it spans more than one.
///
/// # The one owner, and why it lives here
///
/// Three `Display` impls in this crate write an address — [`crate::finding::Location`], which is
/// what a user reads, and [`crate::detect::duplicate::Cluster`] and
/// [`crate::detect::volume::Overrun`], which are the detectors' own determinism probes. All three
/// used to format `path:line` by hand, so "one owner by construction" was a property of the CLI
/// happening to route through one of them rather than of the code. It is now this function.
///
/// It is in `extract` and not in `finding` because of the layering: `finding` already depends on
/// `detect`, so putting the shared renderer there and calling it from the detectors would be a
/// cycle. `extract` is the module both layers already sit on, and [`ProseBlock`] — which owns the
/// `path`, `line_start` and `line_end` an address is made of — is defined right below this.
///
/// `&dyn fmt::Display` for the path rather than a generic or a `&Path`: the callers hold two
/// different things (a `String` in the schema, a `Path::display()` in the detectors) and both are
/// already `Display`. Writing into the caller's formatter allocates nothing.
///
/// The range is written only when the end is genuinely past the start. `end_line == line` is a
/// single-line block, and `end_line < line` cannot be built by [`extract`] — but if one ever were,
/// `path:9-4` is a nonsense address a consumer would try to parse, where `path:9` is merely
/// incomplete. The comparison fails closed.
pub(crate) fn write_address(
    formatter: &mut fmt::Formatter<'_>,
    path: &dyn fmt::Display,
    line: usize,
    end_line: usize,
) -> fmt::Result {
    write!(formatter, "{path}:{line}")?;
    if end_line > line {
        write!(formatter, "-{end_line}")?;
    }
    Ok(())
}

/// One block of prose, located in one file.
///
/// Line numbers are plain 1-based `usize` rather than `ruff_source_file::OneIndexed` so that the
/// exact pin on the `ruff_*` crates does not leak into this crate's public API — the same reason
/// [`Error::Parse`] wraps `ParseError` instead of re-exporting it.
/// `#[non_exhaustive]` for the same forward-compatibility reason as [`Error`], and it also stops an
/// external caller building a block with a struct literal — which is how `line_start > line_end`
/// would reach [`Self::size_lines`]. Construct blocks with [`extract`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProseBlock {
    /// Which syntax carried the prose.
    pub kind: ProseKind,
    /// The path exactly as it was handed to [`extract`].
    pub path: PathBuf,
    /// First physical line of the block, 1-based and inclusive.
    pub line_start: usize,
    /// Last physical line of the block, 1-based and inclusive.
    pub line_end: usize,
    /// The prose verbatim, as it appears in the file, markers and all.
    pub raw: String,
    /// [`normalize`]d form of [`Self::raw`] — the whole block, and the unit
    /// [`MIN_BLOCK_WORDS`] and the volume rules are measured in.
    pub normalized: String,
    /// [`narrative`] form of [`Self::raw`] — the block **without** its API-reference scaffolding,
    /// and the string `TPX003` compares.
    ///
    /// Equal to [`Self::normalized`] for every block that carries no `Args:`, `:param:`, fenced
    /// example or doctest — which is every comment run, and most docstrings. Where the two differ,
    /// the difference is exactly the text a duplicate finding could not have asked anyone to
    /// delete.
    ///
    /// **Empty is a meaningful value**: a docstring that is nothing but a parameter table has no
    /// explanation left to have been said twice, and [`crate::detect::duplicate::duplicates`] leaves
    /// such a block out of the rule entirely. It still counts in full towards `TPX001`/`TPX002`,
    /// which measure volume rather than repetition.
    pub narrative: String,
}

impl ProseBlock {
    /// Physical line span, the unit [`MIN_BLOCK_LINES`] was measured in.
    ///
    /// Deliberately the *span in the file*, not the number of lines of text: a three-word
    /// `Init API instance.` docstring written across three physical lines occupies three lines, and
    /// counting it as one would put it back through a line-based cutoff.
    #[must_use]
    pub const fn size_lines(&self) -> usize {
        // `saturating_sub`, not `-`: every field is public and the detectors of the next task
        // build blocks inside this crate, so an inverted span is reachable with no external caller.
        // Measured before this: `attempt to subtract with overflow` in debug, 18446744073709551614
        // in release. Across the pyo3 boundary a debug panic arrives as `PanicException`, not the
        // `ValueError` the boundary test pins.
        self.line_end.saturating_sub(self.line_start) + 1
    }

    /// Number of [`normalize`]d words, the unit [`MIN_BLOCK_WORDS`] was measured in.
    #[must_use]
    pub fn size_words(&self) -> usize {
        self.normalized.split_whitespace().count()
    }

    /// Whether the block is large enough to be worth comparing.
    ///
    /// A **conjunction**, and each half is provably necessary on the measured corpus: by lines
    /// alone the 3-word `Init API instance.` spread over three lines gets through (a real pair at
    /// Jaccard 1.000); by words alone 360 pairs of one-line copy-paste survive.
    #[must_use]
    pub fn is_large_enough(&self) -> bool {
        self.size_lines() >= MIN_BLOCK_LINES && self.size_words() >= MIN_BLOCK_WORDS
    }

    /// Where this block is: the total order every ordered output in this crate sorts by.
    ///
    /// **The single owner of that tuple.** It was spelled out once in [`extract`] and independently
    /// re-spelled in `detect::duplicate`, which is two owners for one documented contract — the
    /// review found that the two could drift with no test failing, because a fixture where
    /// `line_end == line_start + 1` cannot tell the two spellings apart. Both callers now go
    /// through here, so a change to the sort key is one edit and reaches every consumer.
    ///
    /// `kind` is last and is a genuine tie-breaker rather than decoration: nothing else separates a
    /// docstring from a comment run reported at the same span.
    pub(crate) fn coordinates(&self) -> Coordinates<'_> {
        (&self.path, self.line_start, self.line_end, self.kind)
    }
}

/// Where a block is: `(path, line_start, line_end, kind)`.
///
/// Named next to [`ProseBlock::coordinates`], which is its only producer, so that a consumer cannot
/// re-spell the tuple without noticing it already exists.
pub(crate) type Coordinates<'a> = (&'a Path, usize, usize, ProseKind);

/// Collapses `text` to the form the detectors compare: lower-case alphanumeric words, single
/// spaces, no line breaks.
///
/// Every non-alphanumeric character becomes a space, which is what removes `#` prefixes, docstring
/// quotes and all punctuation without a rule per marker. Unicode letters survive, so Russian prose
/// normalises too.
#[must_use]
pub fn normalize(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_alphanumeric() {
            folded.extend(character.to_lowercase());
        } else {
            folded.push(' ');
        }
    }
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Google-style section headers, matched against a line's trimmed text **exactly**.
///
/// Closed and measured: every one of the six occurs in the six pinned checkouts (`Args:` 2 795,
/// `Returns:` 2 261, `Raises:` 487, `Attributes:` 261, `Yields:` 39, `Keyword Args:` 31 — counted
/// 2026-07-30 over `corpus/checkouts/**/*.py`). Headers that also occur and are deliberately **not**
/// here — `Example:` 196, `Note:` 70, `Examples:` 40, `Notes:` 38 — are prose sections: what they
/// contain is the callable's own explanation, and only the fenced blocks and doctests *inside* them
/// are scaffolding, which [`narrative`] removes on their own grammar.
///
/// Documented aliases with **zero** corpus occurrences (`Arguments:`, `Keyword Arguments:`) are
/// absent on purpose. Every omission here fails safe — an unrecognised header stays narrative, so
/// the block keeps more text and the finding survives.
const GOOGLE_SECTIONS: [&str; 6] = [
    "Args:",
    "Attributes:",
    "Keyword Args:",
    "Raises:",
    "Returns:",
    "Yields:",
];

/// `NumPy`-style section names, matched against the line **above a row of dashes**.
///
/// The underline is what makes this a grammar rather than a guess, and it is required: a bare
/// `Returns` line with no dashes under it is narrative. Measured on the corpus, `NumPy` docstrings are
/// nearly absent — **three** underlined section headers in all six checkouts (`crewAI` 1,
/// `openai-agents-python` 2) — so this branch exists for the styles the rule promises to handle, not
/// because the corpus leans on it.
const NUMPY_SECTIONS: [&str; 6] = [
    "Attributes",
    "Keyword Arguments",
    "Parameters",
    "Raises",
    "Returns",
    "Yields",
];

/// reST / Sphinx info-field names, matched against the text between the first two colons.
///
/// The colon form is the whole test, which is what keeps the *inline* roles out: `:class:`,
/// `:func:`, `:meth:`, `:exc:`, `:mod:` and `:data:` all start lines in this corpus (29, 12, 5, 3,
/// 2, 1 occurrences) and none of them is a field entry — they are cross-references written inside a
/// sentence, i.e. narrative. They are absent from this list for that reason and not by oversight.
///
/// Measured field entries in the corpus: `:param:` 292, `:rtype:` 62, `:return:` 35, `:returns:` 7,
/// `:type:` 7, `:raises:` 5 — counted over the 3 913 `.py` files of the six checkouts at their
/// `corpus/corpus.lock` pins, by the rule `sphinx_field` actually applies. The rest of the list is
/// the remainder of the documented info-field set, and each is inert until a checkout uses it.
///
/// The first two figures were **293** and **8** before the review, counted with a regex that had not
/// yet learned to tell a field from a role. The one line each lost is a role, so the correction and
/// the fix in `sphinx_field` are the same discovery counted twice.
const SPHINX_FIELDS: [&str; 20] = [
    "arg",
    "argument",
    "cvar",
    "except",
    "exception",
    "ivar",
    "key",
    "keyword",
    "param",
    "parameter",
    "raise",
    "raises",
    "return",
    "returns",
    "rtype",
    "type",
    "var",
    "vartype",
    "yield",
    "yields",
];

/// The block with its API-reference scaffolding removed, [`normalize`]d — what `TPX003` compares.
///
/// # Why a block is compared on less than it says
///
/// A duplicate-prose finding says "this explanation is written twice, delete or merge one copy".
/// For a parameter table that advice cannot be taken: `help(post)` and `help(put)` each need their
/// own, and merging them can only make the reference worse. Measured on the annotated sample
/// (`corpus/annotations.md` §1.2–1.3), the templated `requests.post`/`put`/`patch` cluster scores
/// **0.898** while a cluster annotated as genuine goes down to **0.750** — the two classes overlap,
/// so no threshold separates them and the feature has to change instead.
///
/// # The grammar, which is closed and fails towards keeping findings
///
/// Removed, and nothing else is:
///
/// * **Google** — a line that is exactly one of `GOOGLE_SECTIONS`, plus every following line that
///   is blank or indented deeper than the header;
/// * **NumPy-style** — a line in `NUMPY_SECTIONS` underlined by a row of three or more dashes at the
///   same indentation, plus its body up to the next underlined header or the next line indented less
///   than the header;
/// * **Sphinx / reST** — a line beginning `:name:` or `:name arg:` where `name` is in
///   `SPHINX_FIELDS`, plus its deeper-indented continuation lines;
/// * **examples** — a fenced block (an opening ```` ``` ```` through its closing fence) and a
///   doctest run (a `>>>` line and the `>>>`/`...` lines that continue it).
///
/// **Anything the grammar does not recognise stays narrative.** That is the direction the whole
/// function is biased in: an unknown section, an unterminated fence, a header spelled with a typo —
/// each leaves *more* text in the comparison, which can only keep a finding, never invent one. The
/// opposite bias would silence findings through a parser bug, with nothing in the output to say so.
///
/// A doctest's **output** lines are narrative, deliberately: `>>>` marks input and there is no
/// marker for output, so classifying it would be a guess.
///
/// A comment run is narrative in full. The grammar is written for docstring bodies, and `# Args:`
/// does not match `Args:` — which is why the two-line rationale copied between two `OpenHands` files
/// (cluster #1, 0.885) is untouched by any of this.
///
/// # Known ceiling
///
/// A `NumPy` section is ended by the next *underlined* header, so a trailing paragraph written at the
/// section's own indentation with no underline of its own is read as part of the section and
/// removed. That is a malformed `NumPy` docstring, and with three underlined headers in the whole
/// corpus the case is unmeasurable here; it is written down rather than left to be discovered.
///
/// # Examples
///
/// ```
/// use tooprolix::extract::narrative;
///
/// let docstring = "\"\"\"Sends a POST request.\n\n    :param url: URL for the new request.\n    :rtype: requests.Response\n    \"\"\"";
/// assert_eq!(narrative(docstring), "sends a post request");
/// ```
#[must_use]
pub fn narrative(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut kept = String::with_capacity(text.len());
    let mut position = 0;
    // Indexed rather than iterated: every branch below decides how many lines to *skip*, and two of
    // them look one line ahead (the NumPy underline, the fence's partner). A cursor states that
    // directly; `perf-iter-over-index` is about walking a slice, which this is not.
    while position < lines.len() {
        // The doctest is handled here rather than in `scaffolding_at` because it is the one
        // construct with BOTH halves: its input run is scaffolding and its output is narrative, and
        // the output must be copied out **without** being offered to the section grammar. Handing it
        // back was a measured fail-open hole — a doctest printing a line that reads `Args:` turned
        // the explanation under it into a parameter table and suppressed a real finding.
        if let Some((input_end, output_end)) = doctest_run(&lines, position) {
            for line in &lines[input_end..output_end] {
                kept.push_str(line);
                kept.push('\n');
            }
            position = output_end;
            continue;
        }
        if let Some(after) = scaffolding_at(&lines, position) {
            debug_assert!(
                after > position,
                "a section must consume at least its header"
            );
            position = after;
            continue;
        }
        kept.push_str(lines[position]);
        kept.push('\n');
        position += 1;
    }
    normalize(&kept)
}

/// The doctest starting at `start`, as `(end of the input run, end of its output)`.
///
/// Both boundaries are `doctest`'s own, not invented here: a `>>>` line opens the example, `...`
/// continues it, and the expected output that follows runs until a blank line or the next `>>>`
/// (`doctest` — "How are Docstring Examples Recognized?").
///
/// The two halves are returned separately because they are treated differently — the input is
/// discarded, the output is kept as narrative. `>>>` marks input and nothing marks output, so
/// classifying the output as part of the example would be a guess, and a guess here silences
/// findings.
fn doctest_run(lines: &[&str], start: usize) -> Option<(usize, usize)> {
    if !lines[start].trim().starts_with(">>>") {
        return None;
    }
    let mut input_end = start + 1;
    while let Some(line) = lines.get(input_end) {
        let text = line.trim();
        if !(text.starts_with(">>>") || text.starts_with("...")) {
            break;
        }
        input_end += 1;
    }
    let mut output_end = input_end;
    while let Some(line) = lines.get(output_end) {
        let text = line.trim();
        if text.is_empty() || text.starts_with(">>>") {
            break;
        }
        output_end += 1;
    }
    Some((input_end, output_end))
}

/// Indentation width of a line — the depth the section grammars compare.
///
/// A tab advances to the next multiple of **eight**, which is not a choice made here: it is what
/// `str.expandtabs()` and the `CPython` tokenizer do, so it is the width the Python source itself is
/// written against.
///
/// ⚠️ **Counting characters instead was a measured defect, in the fail-closed direction.** A `\t`
/// read as one column, so a tab-indented `Args:` entry measured *shallower* than its own four-space
/// header, the section ended at its first line and the table stayed in the comparison — the
/// update/delete docstring pair scored 0.933 with a tab-indented entry against 0.800 with the
/// byte-identical eight-space one. The rule silently did not apply to tab-indented code.
fn indent_of(line: &str) -> usize {
    let mut width = 0;
    for character in line.chars() {
        match character {
            '\t' => width += 8 - width % 8,
            _ if character.is_whitespace() => width += 1,
            _ => break,
        }
    }
    width
}

/// The index just past the scaffolding section starting at `start`, or `None` when nothing does.
///
/// `None` is the fail-safe answer and every branch returns it rather than guessing: the caller then
/// keeps the line as narrative.
fn scaffolding_at(lines: &[&str], start: usize) -> Option<usize> {
    let line = lines[start];
    let trimmed = line.trim();

    if trimmed.starts_with("```") {
        // An *unterminated* fence is not a fence. Swallowing to the end of the block on a stray
        // backtick run is exactly the "parser silently ate the prose" failure this function refuses.
        let close = lines[start + 1..]
            .iter()
            .position(|following| following.trim().starts_with("```"))?;
        return Some(start + 1 + close + 1);
    }

    // Doctests are NOT here — see `doctest_run`, which [`narrative`] calls first. They are the one
    // construct whose tail has to be kept rather than skipped, and this function can only skip.

    if GOOGLE_SECTIONS.contains(&trimmed) {
        return Some(indented_body(lines, start + 1, indent_of(line)));
    }

    if let Some(name) = underlined_header(lines, start) {
        // An underlined header we do not know — `Notes`, `See Also` — is narrative, and so is
        // everything under it. Returning `None` here is what keeps that true.
        return NUMPY_SECTIONS
            .contains(&name)
            .then(|| numpy_body(lines, start + 2, indent_of(line)));
    }

    if sphinx_field(trimmed).is_some() {
        return Some(indented_body(lines, start + 1, indent_of(line)));
    }

    None
}

/// The index just past a block of lines that are blank or indented deeper than `header_indent`.
fn indented_body(lines: &[&str], from: usize, header_indent: usize) -> usize {
    let mut position = from;
    while let Some(line) = lines.get(position) {
        if !line.trim().is_empty() && indent_of(line) <= header_indent {
            break;
        }
        position += 1;
    }
    position
}

/// The index just past a `NumPy` section body, which sits at the section header's own indentation.
///
/// Ended by the next underlined header — known or not — or by a line indented less than the header.
/// See the "Known ceiling" section of [`narrative`].
fn numpy_body(lines: &[&str], from: usize, header_indent: usize) -> usize {
    let mut position = from;
    while let Some(line) = lines.get(position) {
        if !line.trim().is_empty() && indent_of(line) < header_indent {
            break;
        }
        if underlined_header(lines, position).is_some() {
            break;
        }
        position += 1;
    }
    position
}

/// The header name at `start`, if the line below it is a row of dashes **as long as the name** at
/// the same indentation.
///
/// The dashes are the grammar: without them `Returns` is an English word, and this function is the
/// only thing that stops the `NumPy` branch from eating one.
///
/// ⚠️ **The length must match, and accepting any run of three or more was a measured fail-open
/// hole.** `numpydoc` underlines a section with exactly as many dashes as the name has characters,
/// and `pydocstyle` D409 checks that; a mismatched row is a malformed docstring, not a section.
/// Under the loose test, `Parameters` over three dashes was read as a section and swallowed the
/// explanation below it — measured as a real cross-file finding disappearing, with the tool printing
/// `All checks passed!`.
fn underlined_header<'a>(lines: &[&'a str], start: usize) -> Option<&'a str> {
    let header = lines[start];
    let name = header.trim();
    if name.is_empty() {
        return None;
    }
    let underline = lines.get(start + 1)?;
    let dashes = underline.trim();
    (dashes.chars().count() == name.chars().count()
        && dashes.bytes().all(|byte| byte == b'-')
        && indent_of(underline) == indent_of(header))
    .then_some(name)
}

/// The reST info-field name a line opens with, if it opens with one.
///
/// `:param url: …` and `:rtype: …` are entries; `` :class:`Request` `` is a cross-reference inside a
/// sentence and returns `None`, because `class` is not in [`SPHINX_FIELDS`].
///
/// ⚠️ **The closed list is not enough on its own, and assuming it was is a measured fail-open
/// hole.** Six of the names here are *also* interpreted-text roles — `` :return:`the buffer` ``
/// reads as the field `return` under a name test alone, and the whole sentence after it was
/// swallowed as a field body. Measured: a shared explanation written that way scored 0.760 as plain
/// prose and vanished entirely once the role was added.
///
/// The discriminator is docutils' own: a field marker is `:name:` followed by **whitespace or the
/// end of the line**, while a role's backtick follows the closing colon immediately. That is a
/// grammar rather than a heuristic, and it declines in the safe direction — an ambiguous line stays
/// narrative.
fn sphinx_field(trimmed: &str) -> Option<&str> {
    let (inside, after) = trimmed.strip_prefix(':')?.split_once(':')?;
    if !(after.is_empty() || after.starts_with(char::is_whitespace)) {
        return None;
    }
    let name = inside.split_whitespace().next()?;
    SPHINX_FIELDS.contains(&name).then_some(name)
}

/// Whether [`extract`] reads prose from `path` — decided by the extension, and by nothing else.
///
/// **The single owner of "is this a file tooprolix reads", and public for that reason.** Every
/// caller that walks a directory has to answer this question *before* calling [`extract`], and a
/// second spelling of the answer is a file set that disagrees with the linter's while reporting
/// the difference as nothing at all. That is not hypothetical: the corpus walk behind AC5 spelled
/// it `extension == "py"` and so measured a set that excluded `LOUD.PY`, a file this function —
/// and therefore the shipped rule — calls Python. Comparison is `eq_ignore_ascii_case`, which is
/// what makes the two agree; `tests/volume_corpus.rs` pins that they still do.
///
/// Nothing is read from disk, so this answers for paths that do not exist and says `true` for a
/// *directory* named `pkg.py`. Existence and file-ness are the walker's question, not this one's.
///
/// # It stays a predicate, and that was decided at the API freeze rather than inherited
///
/// `api-parse-dont-validate` says this should be a *parser*: `PythonSource(PathBuf)` with an
/// `Option` constructor, consumed by [`extract`], which would make the disagreement this function
/// exists to prevent unrepresentable and would remove [`Error::UnsupportedSource`] from a public
/// enum. `build-cli-with-exit-contract-and-rule-codes` looked at it — the last moment it was free —
/// and **kept the predicate**. Three reasons, in order of weight:
///
/// * it moves the error rather than removing it. The pyo3 boundary takes a `&str` path from Python
///   and the CLI takes one from `argv`; both would have to turn a raw path into a `PythonSource`
///   and decide what to do when it is `None`, which is the same decision at the same two places,
///   under a different name;
/// * the drift it guards against is a *second spelling* of the rule, and there is exactly one
///   spelling because this function is public and every walker calls it. The `LOUD.PY` defect was
///   a second spelling (`extension == "py"`), not a missing type — a newtype whose constructor
///   nobody was obliged to use would not have caught it either;
/// * it is a breaking change to [`extract`]'s signature days before publication, bought against a
///   defect class that already has a test.
///
/// Recorded rather than assumed, so the second epic re-opens it with the argument in front of it
/// instead of rediscovering the question.
#[must_use]
pub fn is_python_source(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
}

/// Extracts every prose block of `source`, which is the contents of `path`.
///
/// `path` is metadata and dispatch only — nothing is read from disk, so the caller controls which
/// files are visited. Blocks smaller than the [`MIN_BLOCK_LINES`] `AND` [`MIN_BLOCK_WORDS`]
/// conjunction are dropped. The result is sorted by `(path, line_start, line_end, kind)` and is
/// therefore byte-identical between runs.
///
/// # Errors
///
/// * [`Error::Parse`] — `source` is not valid Python.
/// * [`Error::UnsupportedSource`] — `path` does not have a `py` extension.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// // Keep the Python source on ONE line: rustdoc treats a doctest line whose first non-space
/// // character sequence is `# ` as a HIDDEN line and strips that prefix before compiling, which
/// // silently turns a Python comment into a Python statement.
/// let source = "# Retries are capped at three attempts here,\n# the upstream service throttles us.\n";
/// let blocks = tooprolix::extract::extract(Path::new("client.py"), source)?;
///
/// assert_eq!(blocks.len(), 1);
/// assert_eq!(blocks[0].line_start, 1);
/// assert_eq!(blocks[0].line_end, 2);
/// # Ok::<(), tooprolix::Error>(())
/// ```
pub fn extract(path: &Path, source: &str) -> Result<Vec<ProseBlock>, Error> {
    if !is_python_source(path) {
        return Err(Error::UnsupportedSource(path.to_path_buf()));
    }
    let mut blocks = python_blocks(path, source)?;

    blocks.retain(ProseBlock::is_large_enough);
    blocks.sort_by(|left, right| left.coordinates().cmp(&right.coordinates()));
    Ok(blocks)
}

/// Reads `path` into a string, as [`extract`]'s companion for callers that have a filesystem.
///
/// The single producer of [`Error::Io`], and the reason it exists rather than every caller writing
/// its own `fs::read_to_string`: the failure has to carry the path, and it has to arrive at the
/// pyo3 boundary as the same error type the parser failures do, so that one `match` decides how a
/// Python caller sees both.
///
/// Deliberately *not* `extract_file(path) -> Vec<ProseBlock>`, which is the obvious convenience:
/// the opt-out marker is read from the physical line above a block, so the CLI needs the text as
/// well as the blocks, and a helper that returned only the blocks would force it to read every file
/// twice.
///
/// # Errors
///
/// [`Error::Io`] — the file does not exist, cannot be opened, or is not UTF-8.
pub fn read_source(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// The docstring of a body, if its first statement is a standalone string literal.
///
/// The same two checks ruff makes in `crates/ruff_linter/src/docstrings/extraction.rs`: a
/// standalone *expression* statement, whose expression is a *string literal*.
fn docstring_of(body: &[Stmt]) -> Option<TextRange> {
    let Stmt::Expr(expression) = body.first()? else {
        return None;
    };
    // Returning the RANGE rather than the literal keeps the two checks separately mutable:
    // replacing this line with `Some(expression.value.range())` drops the string-literal half in a
    // one-token edit, which is exactly the mutation a test has to be able to catch.
    expression.value.as_string_literal_expr().map(Ranged::range)
}

/// Collects the range of every class and function docstring in a statement tree.
struct DocstringRanges {
    ranges: Vec<TextRange>,
}

impl<'a> StatementVisitor<'a> for DocstringRanges {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(ast::StmtClassDef { body, .. })
        | Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) = stmt
            && let Some(range) = docstring_of(body)
        {
            self.ranges.push(range);
        }
        // Keep walking: a `def` inside a `def`, a `class` inside an `if`, a `def` in an `except`
        // handler all carry docstrings, and `walk_stmt` is what reaches them.
        walk_stmt(self, stmt);
    }
}

fn python_blocks(path: &Path, source: &str) -> Result<Vec<ProseBlock>, Error> {
    let parsed = parse_module(source)?;
    let index = LineIndex::from_source_text(source);
    let body = &parsed.syntax().body;

    let mut collector = DocstringRanges {
        ranges: docstring_of(body).into_iter().collect(),
    };
    collector.visit_body(body);

    let mut blocks: Vec<ProseBlock> = collector
        .ranges
        .into_iter()
        .map(|range| block(path, ProseKind::Docstring, source, &index, range))
        .collect();
    blocks.extend(comment_blocks(path, source, parsed.tokens(), &index));
    Ok(blocks)
}

/// Own-line comment runs, glued by consecutive physical lines.
fn comment_blocks(
    path: &Path,
    source: &str,
    tokens: &[ruff_python_ast::token::Token],
    index: &LineIndex,
) -> Vec<ProseBlock> {
    let mut blocks = Vec::new();
    // (first range of the run, last range of the run, line of the last range)
    let mut run: Option<(TextRange, TextRange, usize)> = None;

    for token in tokens
        .iter()
        .filter(|token| token.kind() == TokenKind::Comment)
    {
        let range = token.range();
        let line = index.line_index(range.start()).get();
        if is_pragma(&source[range], line) {
            continue;
        }
        // A trailing comment is prose about one statement; it must not join the run below it. It
        // is never emitted either — one physical line cannot pass MIN_BLOCK_LINES.
        if has_leading_content(range.start(), source) {
            continue;
        }

        run = match run {
            Some((first, _, previous)) if line == previous + 1 => Some((first, range, line)),
            Some((first, last, _)) => {
                blocks.push(comment_run(path, source, index, first, last));
                Some((range, range, line))
            }
            None => Some((range, range, line)),
        };
    }
    if let Some((first, last, _)) = run {
        blocks.push(comment_run(path, source, index, first, last));
    }
    blocks
}

fn comment_run(
    path: &Path,
    source: &str,
    index: &LineIndex,
    first: TextRange,
    last: TextRange,
) -> ProseBlock {
    block(
        path,
        ProseKind::Comment,
        source,
        index,
        TextRange::new(first.start(), last.end()),
    )
}

/// Whether a comment is a machine directive rather than prose.
///
/// `text` is the comment verbatim, `#` included; `line` is its 1-based line.
fn is_pragma(text: &str, line: usize) -> bool {
    // A `#!` comment is a shebang only on the FIRST LINE. Anywhere else it is an ordinary comment
    // that happens to start with an exclamation mark.
    //
    // Keyed on the line and not on byte offset 0: a UTF-8 BOM is legitimate in Python (CPython runs
    // such a file, exit 0) and pushes the shebang to byte 3, so an offset test let it through and
    // glued it onto the prose below. Measured: BOM'd -> `[(1, 3, "usr bin env python3 …")]`,
    // clean -> `[(2, 3, "the first real comment line …")]`. Same prose, two `normalized` values.
    // The cookie branch below was always immune for exactly this reason; this is the same shape.
    if line == 1 && text.starts_with("#!") {
        return true;
    }
    let body = text.trim_start_matches('#').trim_start();
    // PEP 263 puts the encoding declaration on line 1 or line 2 and nowhere else.
    if line <= 2 && has_encoding_cookie(body) {
        return true;
    }
    let lowered = body.to_ascii_lowercase();
    // OUR opt-out marker (`# !TPX001`, README.md): the marker LINE is excluded, and nothing more.
    // The question "is this line a marker?" is asked of [`crate::rules::is_marker`], which is the
    // grammar's single owner — this module still does not *parse* the marker, and must not.
    //
    // It used to be spelled here a second time as `starts_with("tooprolix:")`, and the two owners
    // disagreed in **both** directions, each one silently moving the number the volume rule
    // reports. `a_comment_is_excluded_as_a_marker_exactly_when_the_marker_parser_accepts_it` holds
    // the two measurements; the short version is that a legitimate marker had its own words counted
    // as prose, and a line that was not a marker cut a comment run in half.
    //
    // The 0.2.0 grammar change deliberately did NOT add `rules::is_near_miss` beside it, though the
    // temptation was real: the 0.1.0 marker is no longer a marker, so above a comment run it is now
    // absorbed into the prose it used to excuse. Excluding near-misses here would keep it out — and
    // would re-arm exactly the defect above, because a near-miss is by definition a line the parser
    // rejected, and excluding one in the MIDDLE of a comment run splits the run and silently halves
    // a measured volume. The diagnostic belongs where a line can be reported rather than dropped,
    // so `crate::cli` warns about both positions instead. One owner, one direction, still.
    //
    // `noqa` and `type:` stay: they name OTHER tools' pragmas (ruff, flake8, mypy) and have nothing
    // to do with our grammar. Renaming them along with the marker would start counting `# noqa:
    // F401` as human prose.
    lowered.starts_with("noqa") || lowered.starts_with("type:") || crate::rules::is_marker(text)
}

/// Whether `body` carries a PEP 263 encoding declaration.
///
/// PEP 263 spells the cookie `coding[:=]\s*([-_.a-zA-Z0-9]+)` — **a name must follow**. Testing for
/// the bare substring `coding:` instead deletes ordinary English: `# our approach to coding:` was
/// measured to make a real two-line prose block disappear with no error at all. Written out rather
/// than pulled in as a regex dependency; it is a `find` and two character tests.
fn has_encoding_cookie(body: &str) -> bool {
    let mut rest = body;
    while let Some(position) = rest.find("coding") {
        rest = &rest[position + "coding".len()..];
        let Some(after) = rest.strip_prefix([':', '=']) else {
            continue;
        };
        if after
            .trim_start_matches([' ', '\t'])
            .starts_with(|character: char| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
        {
            return true;
        }
    }
    false
}

fn block(
    path: &Path,
    kind: ProseKind,
    source: &str,
    index: &LineIndex,
    range: TextRange,
) -> ProseBlock {
    let raw = source[range].to_owned();
    ProseBlock {
        kind,
        path: path.to_path_buf(),
        line_start: index.line_index(range.start()).get(),
        line_end: index.line_index(range.end()).get(),
        normalized: normalize(&raw),
        narrative: narrative(&raw),
        raw,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use insta::assert_debug_snapshot;

    use super::{
        MIN_BLOCK_LINES, MIN_BLOCK_WORDS, ProseBlock, ProseKind, extract, narrative, normalize,
    };

    const SAMPLE_PY: &str = include_str!("../tests/fixtures/extract/sample.py");

    fn blocks(path: &str, source: &str) -> Vec<ProseBlock> {
        extract(Path::new(path), source).expect("the fixture is a supported source")
    }

    /// AC1 — the whole Python half of the contract in one reviewed artifact: module, class, method
    /// and nested-function docstrings, the glued `#`-run, and the absence of everything the
    /// contract excludes (shebang, coding cookie, `# noqa`, `# type:`, the trailing comment,
    /// `"""Init."""`, the two-line-but-short comment).
    #[test]
    fn extracts_the_python_fixture() {
        let extracted = blocks("tests/fixtures/extract/sample.py", SAMPLE_PY);

        assert_debug_snapshot!(extracted);
        // The snapshot is the artifact, and this is the one thing a reader could mistake in it:
        // `narrative` equals `normalized` for six of the seven blocks because those six carry no
        // scaffolding, not because the two fields are the same field. Exactly one block — the last
        // — has an `Args:`/`Returns:` table, and asserting the split here keeps the fixture from
        // drifting back into a corpus where the distinction is invisible.
        let split = extracted
            .iter()
            .filter(|block| block.narrative != block.normalized)
            .count();
        assert_eq!(split, 1, "got {extracted:?}");
    }

    /// AC2 — the same sentence written with different line breaks and indentation must normalise to
    /// the same string. This is the only reason the duplicate detector can compare a comment with a
    /// docstring at all. Compared directly, not by snapshot.
    #[test]
    fn line_breaks_and_indentation_do_not_change_the_normalised_text() {
        let flat = "# The retry budget here is deliberately small, and that matters.\n\
                    # It is checked twice.\n";
        let wrapped = "#   The retry budget here\n\
                       #        is deliberately small,\n\
                       #   and that matters.  It is\n\
                       #\tchecked twice.\n";

        let from_flat = blocks("a.py", flat);
        let from_wrapped = blocks("a.py", wrapped);

        assert_eq!(normalize(flat), normalize(wrapped));
        assert_eq!(from_flat.len(), 1, "got {from_flat:?}");
        assert_eq!(from_wrapped.len(), 1, "got {from_wrapped:?}");
        assert_eq!(from_flat[0].normalized, from_wrapped[0].normalized);
        assert_eq!(
            from_flat[0].normalized,
            "the retry budget here is deliberately small and that matters it is checked twice"
        );
        // ... and the raw text really does differ, so the equality above is not trivial.
        assert_ne!(from_flat[0].raw, from_wrapped[0].raw);
    }

    /// AC3(a) — `"""Init."""` fails both halves. The single most common docstring in the corpus.
    #[test]
    fn a_one_word_docstring_is_excluded() {
        let extracted = blocks("a.py", "def f():\n    \"\"\"Init.\"\"\"\n");

        assert!(extracted.is_empty(), "got {extracted:?}");
    }

    /// AC3(b) — enough LINES, too few WORDS. One of the two cases that tell `AND` from `OR`: under
    /// `OR` the line half alone would let it through.
    #[test]
    fn a_multi_line_block_with_too_few_words_is_excluded() {
        let source = "def f():\n    \"\"\"\n    Short.\n    \"\"\"\n";

        let extracted = blocks("a.py", source);

        assert!(
            extracted.is_empty(),
            "3 lines but 1 word must not pass the conjunction; got {extracted:?}"
        );
    }

    /// AC3(c) — enough WORDS, too few LINES. The other discriminating case: under `OR` the word
    /// half alone would let it through, and one-line blocks are 741 861 of the 741 970 exact
    /// duplicate pairs measured on the reference corpus.
    #[test]
    fn a_one_line_block_with_enough_words_is_excluded() {
        let source = "# one line comment with quite a lot of words in it indeed\n";

        let extracted = blocks("a.py", source);

        assert!(
            extracted.is_empty(),
            "1 line but 12 words must not pass the conjunction; got {extracted:?}"
        );
    }

    /// A block that passes both halves must actually appear — otherwise the three exclusion tests
    /// above would also pass against an extractor that returns nothing at all.
    #[test]
    fn a_block_passing_both_halves_is_kept() {
        let source = "# a comment block that spans two physical lines\n\
                      # and carries plenty of words\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 1, "got {extracted:?}");
        assert!(extracted[0].size_lines() >= MIN_BLOCK_LINES);
        assert!(extracted[0].size_words() >= MIN_BLOCK_WORDS);
    }

    /// A comment that merely *mentions* coding is prose. PEP 263 defines the cookie as
    /// `coding[:=]\s*([-_.a-zA-Z0-9]+)` — a name must follow — so matching the bare substring
    /// `coding:` deletes ordinary English, and deletes it silently: no error, just prose that is
    /// never reported as duplicated.
    #[test]
    fn a_comment_that_merely_mentions_coding_is_prose() {
        let source = "# This section explains our approach to coding:\n\
                      # every service response stays readable across old clients.\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 1, "got {extracted:?}");
        assert_eq!((extracted[0].line_start, extracted[0].line_end), (1, 2));
    }

    /// `AC1a` — the shebang branch on its own.
    ///
    /// Every one of these four tests is shaped so the leak is **>= 2 adjacent lines and >= 8
    /// words**: the pragma sits above a two-line prose run it would glue onto. Measured on
    /// `9c30b5d`: with a single pragma line the leak is one line, the size conjunction — a
    /// *neighbouring* guard — silences it, and the branch test cannot fail.
    #[test]
    fn the_shebang_branch_alone_keeps_the_shebang_out() {
        let source = "#!/usr/bin/env python3\n\
                      # the first real comment line carrying plenty of words for the size gate\n\
                      # and a second real line\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 1, "got {extracted:?}");
        assert_eq!((extracted[0].line_start, extracted[0].line_end), (2, 3));
        assert!(
            !extracted[0].normalized.contains("usr bin env"),
            "the shebang leaked into the run: {}",
            extracted[0].normalized
        );
    }

    /// `AC1a` — the shebang branch survives a UTF-8 BOM.
    ///
    /// A BOM'd file is legitimate Python (`CPython` runs it, exit 0), but the shebang then starts at
    /// byte 3, so keying the branch on "byte offset 0" let it through. The cookie branch was always
    /// immune because it keys on `line <= 2`; this is the same shape. Beyond the leak, the two files
    /// — same prose, one BOM'd — used to produce different `normalized` and different `line_start`,
    /// which is a determinism defect over *equivalent inputs* rather than repeated runs.
    #[test]
    fn the_shebang_branch_survives_a_byte_order_mark() {
        let source = "\u{feff}#!/usr/bin/env python3\n\
                      # the first real comment line carrying plenty of words for the size gate\n\
                      # and a second real line\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 1, "got {extracted:?}");
        assert_eq!((extracted[0].line_start, extracted[0].line_end), (2, 3));
        assert!(
            !extracted[0].normalized.contains("usr bin env"),
            "a BOM let the shebang into the run: {}",
            extracted[0].normalized
        );
    }

    /// The opt-out marker has ONE grammar, and extraction asks its owner rather than re-spelling it.
    ///
    /// Two directions, both measured as defects before this test existed, and each one silently
    /// changes the number the volume rule reports:
    ///
    /// * a marker with a space inside it — `# ! TPX001` — is accepted by
    ///   [`crate::rules::parse_marker`]. A second, stricter spelling here left it *inside* the
    ///   block, so the block began on the marker's own line, the CLI looked one line higher and
    ///   found nothing, and the marker's own words were counted: **303 words where the canonical
    ///   spelling gives 300 and correctly stays silent**;
    /// * a line that is not a marker — under 0.2.0, anything without the `!`, including the 0.1.0
    ///   spelling `# tooprolix: noqa` — is prose. A looser spelling here excluded it anyway and so
    ///   cut the comment run in two: **300 words reported where the same run with an ordinary
    ///   comment in the middle reports 610**. A line that is not a marker halved the measured
    ///   volume, with no diagnostic at all.
    ///
    /// The fix is not a better spelling, it is one owner. This test fails if a second one reappears
    /// in either direction — including the tempting one, `is_near_miss`, which would put the whole
    /// second case back (see `is_pragma`).
    #[test]
    fn a_comment_is_excluded_as_a_marker_exactly_when_the_marker_parser_accepts_it() {
        // Arrange — eight words per line, so every line clears MIN_BLOCK_WORDS on its own and the
        // block boundaries are the only thing under test.
        let prose = "# one two three four five six seven eight\n";
        let accepted = format!("# ! TPX001\n{prose}{prose}");
        let rejected = format!("{prose}# tooprolix: noqa\n{prose}");

        // Act
        let excluded = extract(Path::new("a.py"), &accepted).expect("valid Python");
        let kept = extract(Path::new("a.py"), &rejected).expect("valid Python");

        // Assert — the marker is not prose, so the block starts below it ...
        assert_eq!(excluded.len(), 1, "{excluded:#?}");
        assert_eq!(
            (excluded[0].line_start, excluded[0].line_end),
            (2, 3),
            "a spelling the marker parser accepts was counted as prose"
        );
        assert!(
            !excluded[0].normalized.contains("tpx001"),
            "the marker's own words entered the block it excuses: {}",
            excluded[0].normalized
        );

        // ... and a line that is NOT a marker is ordinary prose, so the run stays whole.
        assert_eq!(
            kept.len(),
            1,
            "a non-marker split one comment run: {kept:#?}"
        );
        assert_eq!(
            (kept[0].line_start, kept[0].line_end),
            (1, 3),
            "a line the marker parser rejects was excluded anyway"
        );
        assert!(
            kept[0].normalized.contains("noqa"),
            "the 0.1.0 spelling was dropped from the prose it now belongs to: {}",
            kept[0].normalized
        );
    }

    /// `AC1a` — our own opt-out marker is a machine directive, not prose.
    ///
    /// `README.md` ships `# !TPX001` as the contract. Left as prose it does three things, the third
    /// being the worst: it glues onto the block below it, it injects `tpx001` into `normalized` and
    /// so lowers the Jaccard of the *unsuppressed* twin in another file below the 0.75 threshold,
    /// and it moves `line_start` onto the marker line — the line the opt-out parser has to map onto.
    #[test]
    fn the_opt_out_marker_branch_alone_keeps_the_marker_out() {
        let source = "# !TPX001\n\
                      # the first real comment line carrying plenty of words for the size gate\n\
                      # and a second real line\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 1, "got {extracted:?}");
        assert_eq!((extracted[0].line_start, extracted[0].line_end), (2, 3));
        assert!(
            !extracted[0].normalized.contains("tpx001"),
            "the opt-out marker leaked into the run: {}",
            extracted[0].normalized
        );
    }

    /// ... and the marker must not MANUFACTURE a block either: a lone one-line comment is below the
    /// size gate, and putting a marker above it used to push the pair over the line half.
    #[test]
    fn the_opt_out_marker_does_not_manufacture_a_block() {
        let suppressed = "# !TPX001\n\
                          # one line comment with quite a lot of words in it indeed\n";

        let extracted = blocks("a.py", suppressed);

        assert!(
            extracted.is_empty(),
            "the marker turned a filtered one-line comment into a block: {extracted:?}"
        );
    }

    /// `size_lines` must not underflow on an inverted span. Every field of [`ProseBlock`] is public
    /// and the detectors of the next task build blocks inside this crate, so `line_start > line_end`
    /// is reachable without any external caller. Measured before the fix: `attempt to subtract with
    /// overflow` in debug, `18446744073709551614` in release — and across the pyo3 boundary a debug
    /// panic surfaces as `PanicException`, not the `ValueError` the boundary test pins.
    #[test]
    fn size_lines_does_not_underflow_on_an_inverted_span() {
        let block = ProseBlock {
            kind: ProseKind::Comment,
            path: PathBuf::from("a.py"),
            line_start: 5,
            line_end: 2,
            raw: String::new(),
            normalized: String::new(),
            narrative: String::new(),
        };

        assert_eq!(block.size_lines(), 1);
    }

    /// `AC1a` — the encoding-cookie branch on its own.
    #[test]
    fn the_encoding_cookie_branch_alone_keeps_the_cookie_out() {
        let source = "# -*- coding: utf-8 -*-\n\
                      # the first real comment line carrying plenty of words for the size gate\n\
                      # and a second real line\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 1, "got {extracted:?}");
        assert_eq!((extracted[0].line_start, extracted[0].line_end), (2, 3));
        assert!(
            !extracted[0].normalized.contains("coding utf 8"),
            "the encoding cookie leaked into the run: {}",
            extracted[0].normalized
        );
    }

    /// `AC1a` — the `noqa` branch on its own.
    #[test]
    fn the_noqa_branch_alone_keeps_noqa_out() {
        let source = "# noqa: E501,F401\n\
                      # the first real comment line carrying plenty of words for the size gate\n\
                      # and a second real line\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 1, "got {extracted:?}");
        assert_eq!((extracted[0].line_start, extracted[0].line_end), (2, 3));
        assert!(
            !extracted[0].normalized.contains("noqa"),
            "the noqa pragma leaked into the run: {}",
            extracted[0].normalized
        );
    }

    /// `AC1a` — the `type:` branch on its own.
    #[test]
    fn the_type_comment_branch_alone_keeps_type_comments_out() {
        let source = "# type: ignore[assignment]\n\
                      # the first real comment line carrying plenty of words for the size gate\n\
                      # and a second real line\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 1, "got {extracted:?}");
        assert_eq!((extracted[0].line_start, extracted[0].line_end), (2, 3));
        assert!(
            !extracted[0].normalized.contains("ignore assignment"),
            "the type comment leaked into the run: {}",
            extracted[0].normalized
        );
    }

    /// Red-team — a file made only of the comments the contract calls "not prose" yields nothing.
    /// The four lines are chosen so a missing exclusion LEAKS: glued they are 4 lines and 13
    /// normalised words, i.e. they pass the size conjunction on their own.
    #[test]
    fn a_file_of_only_pragma_comments_yields_no_blocks() {
        let source = "#!/usr/bin/env python3\n\
                      # -*- coding: utf-8 -*-\n\
                      # noqa: E501,F401\n\
                      # type: ignore[assignment]\n";

        let extracted = blocks("a.py", source);

        assert!(extracted.is_empty(), "got {extracted:?}");
    }

    /// A trailing comment is never glued to the own-line run below it. Without the distinction the
    /// run would start one line earlier and swallow prose written about a single statement.
    #[test]
    fn a_trailing_comment_is_not_glued_to_the_following_comment_run() {
        let source = "x = 1  # trailing prose with a good number of words in it right here\n\
                      # own line comment number one carrying plenty of words for the gate\n\
                      # own line comment number two\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 1, "got {extracted:?}");
        assert_eq!(extracted[0].line_start, 2);
        assert_eq!(extracted[0].line_end, 3);
        assert!(
            !extracted[0].normalized.contains("trailing"),
            "the trailing comment leaked into the own-line run: {}",
            extracted[0].normalized
        );
    }

    /// A statement between two comment runs ends the first one: the lines are no longer
    /// consecutive, so this is one block on each side and never one block spanning the code.
    #[test]
    fn code_between_two_comment_runs_splits_them() {
        let source = "# the first run of comment lines with enough words to be kept\n\
                      # and a second line\n\
                      x = 1\n\
                      # the second run of comment lines with enough words to be kept\n\
                      # and its own second line\n";

        let extracted = blocks("a.py", source);

        assert_eq!(extracted.len(), 2, "got {extracted:?}");
        assert_eq!((extracted[0].line_start, extracted[0].line_end), (1, 2));
        assert_eq!((extracted[1].line_start, extracted[1].line_end), (4, 5));
    }

    /// A string literal that is not in first position is not a docstring. Guards the
    /// "first statement" half of the rule: without it, any long string anywhere becomes prose.
    #[test]
    fn a_string_after_the_first_statement_is_not_a_docstring() {
        let source = "import os\n\n\"\"\"\nNot a docstring, because the import came first, and\nthis text is long enough to pass both halves.\n\"\"\"\n";

        let extracted = blocks("a.py", source);

        assert!(extracted.is_empty(), "got {extracted:?}");
    }

    /// A non-string expression in first position is a statement, not prose. Guards the
    /// "string literal" half of the same rule.
    ///
    /// The expression is deliberately **four lines and well over eight words**. With the `42` this
    /// test used to carry, a mutation that emits *any* first expression as a docstring leaks a
    /// one-line, one-word block, which the size conjunction — a neighbouring guard — silences, so
    /// the test could not fail for its own reason.
    #[test]
    fn a_non_string_first_statement_is_not_a_docstring() {
        let source = "[\n\
                      \x20   \"the first configured retry policy name\",\n\
                      \x20   \"the second configured retry policy name\",\n\
                      ]\n";

        let extracted = blocks("a.py", source);

        assert!(extracted.is_empty(), "got {extracted:?}");
    }

    /// AC4 — the output order is the documented one: ascending by line, with docstrings and
    /// comments interleaved rather than grouped by kind. **This is the AC4 guard**, together with
    /// the snapshot above, which pins the exact bytes.
    ///
    /// There is deliberately no "call it twice and compare" test. There was one, and it could not
    /// fail: `extract` is a pure function of `(path, source)`, so two calls are equal by
    /// construction — including when the order is wrong. Removing the sort left it GREEN while this
    /// test went red. Do not re-add it.
    #[test]
    fn blocks_are_ordered_by_line() {
        let extracted = blocks("tests/fixtures/extract/sample.py", SAMPLE_PY);

        assert!(
            extracted.len() > 1,
            "ordering over 0 or 1 blocks is vacuous; got {extracted:?}"
        );
        let lines: Vec<usize> = extracted.iter().map(|block| block.line_start).collect();
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        assert_eq!(lines, sorted, "got {extracted:?}");
    }

    /// Unparseable Python must fail loudly. An extractor that returned an empty list here would
    /// report "no prose" for every file it cannot read, which hides real prose.
    #[test]
    fn invalid_python_is_an_error_not_an_empty_result() {
        let error = extract(Path::new("a.py"), "def broken(:\n")
            .expect_err("the fixture is not valid Python");

        assert!(
            error.to_string().contains("could not parse Python source"),
            "got: {error}"
        );
    }

    /// Anything that is not a Python file is an error, not silence — same reason as above. The
    /// caller is told the file was not read rather than quietly handed an empty list, which is the
    /// difference between "no prose here" and "this file was never looked at".
    #[test]
    fn a_path_that_is_not_python_is_an_error() {
        for path in ["notes.txt", "Makefile", "config.json", "script.pyi"] {
            let error = extract(
                Path::new(path),
                "# a comment carrying quite a few words indeed\n",
            )
            .expect_err("only *.py is read");

            assert!(
                error.to_string().contains("not a source"),
                "{path}: got {error}"
            );
        }
    }

    /// AC1, Google — a section header from the closed list takes its indented body with it.
    ///
    /// Bytes shaped like `crewAI/lib/crewai/src/crewai/agent/core.py:766-780`, cluster #20 of
    /// `corpus/annotations.md` §1.2: three sections, one of them `Raises:` with several entries, and
    /// a summary line that has to come back untouched.
    #[test]
    fn a_google_section_takes_its_indented_body_with_it() {
        // Arrange
        let docstring = "\"\"\"Execute a task with the agent.\n\n\
                         Args:\n\
                         \x20   task: Task to execute.\n\
                         \x20   context: Context to execute the task in.\n\n\
                         Returns:\n\
                         \x20   Output of the agent\n\n\
                         Raises:\n\
                         \x20   TimeoutError: If execution exceeds the maximum execution time.\n\
                         \x20   ValueError: If the max execution time is not a positive integer.\n\
                         \"\"\"";

        // Act
        let kept = narrative(docstring);

        // Assert
        assert_eq!(kept, "execute a task with the agent");
        assert!(
            normalize(docstring).contains("timeouterror"),
            "the fixture must carry the sections the assertion says are gone"
        );
    }

    /// AC1, Sphinx/reST — a `:field:` entry takes its deeper-indented continuation lines with it,
    /// and an inline cross-reference role is **not** a field entry.
    ///
    /// Bytes from `requests/src/requests/api.py:123-132`, cluster #12 at 0.898. The `:class:` line
    /// is the distinction that matters: `:class:` opens a sentence, `:param:` opens a table row, and
    /// only the closed list in [`SPHINX_FIELDS`] separates them.
    #[test]
    fn a_sphinx_field_takes_its_continuation_lines_but_a_role_is_narrative() {
        // Arrange
        let docstring = "r\"\"\"Sends a POST request.\n\n\
                         :param url: URL for the new :class:`Request` object.\n\
                         :param data: (optional) Dictionary, list of tuples, bytes, or file-like\n\
                         \x20   object to send in the body of the :class:`Request`.\n\
                         :return: :class:`Response <Response>` object\n\
                         :rtype: requests.Response\n\
                         \"\"\"";
        let role_only = ":class:`Response` is returned whenever the server answers at all.";

        // Act & Assert
        assert_eq!(narrative(docstring), "r sends a post request");
        assert_eq!(narrative(role_only), normalize(role_only));
    }

    /// AC1, `NumPy` — a section is recognised by its row of dashes, and by nothing else.
    ///
    /// The second half is the whole point: strip the underline and `Returns` is an English word in a
    /// sentence, so the same text must come back whole. Measured, `NumPy` is nearly absent from the
    /// corpus (three underlined headers in six checkouts), so this branch is defended by this test
    /// and not by the corpus.
    #[test]
    fn a_numpy_section_is_recognised_by_its_underline_and_only_by_it() {
        // Arrange
        let underlined = "\"\"\"Summarise the run.\n\n\
                          Parameters\n\
                          ----------\n\
                          config : Mapping\n\
                          \x20   The configuration to summarise.\n\n\
                          Notes\n\
                          -----\n\
                          The summary is advisory only.\n\
                          \"\"\"";
        let unmarked = "\"\"\"Summarise the run.\n\n\
                        Parameters\n\
                        config : Mapping\n\
                        \"\"\"";

        // Act & Assert — `Notes` is not in the closed list, so it and its body stay narrative.
        assert_eq!(
            narrative(underlined),
            "summarise the run notes the summary is advisory only"
        );
        assert_eq!(narrative(unmarked), normalize(unmarked));
    }

    /// AC1, examples — a fenced block and a doctest run are scaffolding; a doctest's output is not.
    ///
    /// Bytes shaped like `pydantic-core/python/pydantic_core/core_schema.py:726-748` (cluster #11)
    /// and `langgraph/.../postgres/__init__.py:193-226` (cluster #3). Output lines stay because
    /// `>>>` marks input and nothing marks output — classifying it would be a guess, and a guess in
    /// this function silences findings.
    #[test]
    fn a_fenced_block_and_a_doctest_are_not_narrative_but_the_output_is() {
        // Arrange
        let fenced = "\"\"\"Returns a schema that matches a float value.\n\n\
                      ```py\n\
                      schema = core_schema.float_schema(le=0.8)\n\
                      ```\n\
                      \"\"\"";
        // The `...` line is a PS2 continuation and is load-bearing in this fixture: the review
        // found that deleting the `...` arm of the doctest branch reddened NOTHING, because the
        // only line here that looked like one — `CheckpointTuple(...)` — starts with a `C`. A guard
        // no test can fail is not a guard.
        let doctest = "\"\"\"Get a checkpoint tuple from the database.\n\n\
                       Examples:\n\
                       \x20   >>> config = {\n\
                       \x20   ...     \"thread_id\": \"1\",\n\
                       \x20   ... }\n\
                       \x20   >>> print(checkpoint_tuple)\n\
                       \x20   CheckpointTuple(...)\n\
                       \"\"\"";

        // 🔴 The regression the review measured: doctest output was handed back to the section
        // grammar, so an example that PRINTS something reading `Args:` turned the explanation under
        // it into a parameter table and the finding disappeared. This case cannot live in the
        // "narrative unchanged" table next door, because the `>>>` line itself is scaffolding and is
        // correctly removed — so the expectation is written out.
        let printing_a_section_header = "\"\"\"Do the thing.\n\n\
                                         >>> print_contract()\n\
                                         Args:\n\
                                         \x20   the caller keeps ownership of the buffer.\n\
                                         \"\"\"";

        // Act & Assert
        assert_eq!(
            narrative(fenced),
            "returns a schema that matches a float value"
        );
        assert_eq!(
            narrative(doctest),
            "get a checkpoint tuple from the database examples checkpointtuple"
        );
        assert_eq!(
            narrative(printing_a_section_header),
            "do the thing args the caller keeps ownership of the buffer",
            "the output of a doctest was re-parsed as a section"
        );
    }

    /// 🔴 **The invariant, stated once for every shape that could break it: a construct the grammar
    /// does not recognise — or recognises as malformed — leaves the narrative unchanged.**
    ///
    /// This is a *class* test and it is deliberately table-driven. The first review round of
    /// `exclude-reference-scaffolding-from-tpx003` found **three separate fail-open holes** in one
    /// pass, each one a different branch swallowing genuine prose and silently suppressing a real
    /// finding. Three instances of one class means the class needs a guard, not three patches, and
    /// the rows below are that guard: every new branch of [`narrative`] adds its malformed shape
    /// here.
    ///
    /// The direction is the whole point. Keeping too much text can only preserve a finding; dropping
    /// too much deletes one, and deletes it invisibly — nothing in the tool's output says "a parser
    /// ate this". Every row is a case where the parser must decline.
    #[test]
    fn a_malformed_or_unrecognised_construct_leaves_the_narrative_unchanged() {
        // Arrange — (what it is, the fixture). Rows 1-3 were green before the review; rows 4-6 are
        // the three holes it found, each one measured as a suppressed finding on the real binary.
        let cases = [
            (
                "a section header outside the closed list",
                "\"\"\"Do the thing.\n\n\
                 Note:\n\
                 \x20   The caller keeps ownership of the buffer.\n\
                 \"\"\"",
            ),
            (
                "a header that is nearly, but not, one from the list",
                "\"\"\"Do the thing.\n\n\
                 Arguments:\n\
                 \x20   buffer: the caller keeps ownership.\n\
                 \"\"\"",
            ),
            (
                "a fence that is never closed",
                "\"\"\"Do the thing.\n\n\
                 ```py\n\
                 never_closed = True\n\
                 \"\"\"",
            ),
            (
                "a NumPy underline shorter than its header",
                "\"\"\"Do the thing.\n\n\
                 Parameters\n\
                 ---\n\
                 The caller keeps ownership of the buffer, which is the whole contract here.\n\
                 \"\"\"",
            ),
            (
                "a NumPy underline longer than its header",
                "\"\"\"Do the thing.\n\n\
                 Returns\n\
                 ------------\n\
                 The caller keeps ownership of the buffer, which is the whole contract here.\n\
                 \"\"\"",
            ),
            (
                "a reST role on one line, which is a cross-reference and not a field entry",
                "\"\"\"Do the thing.\n\n\
                 :return:`the buffer` is handed back with ownership, which is the whole contract.\n\
                 \"\"\"",
            ),
        ];

        // Act — every row, not the first failing one. A table that stops at row four hides rows
        // five to seven, and this class was found three instances at a time.
        let swallowed: Vec<&str> = cases
            .into_iter()
            .filter(|(_, text)| narrative(text) != normalize(text))
            .map(|(what, _)| what)
            .collect();

        // Assert
        assert!(
            swallowed.is_empty(),
            "the parser swallowed narrative on {} of {} shapes: {swallowed:#?}",
            swallowed.len(),
            cases.len()
        );
    }

    /// A tab-indented section body is scaffolding, exactly like a space-indented one.
    ///
    /// Found in the same review round and it is the *other* direction — a fail-**closed** hole. A
    /// tab counted as one character of indentation, so a `\t`-indented entry measured **shallower**
    /// than its own four-space `Args:` header, the section ended at its first line, and the table
    /// stayed in the comparison. Measured on the real binary: the update/delete docstring pair
    /// scored **0.933** with a tab-indented entry against **0.800** with the byte-identical
    /// eight-space one — i.e. the rule silently did not apply to tab-indented code.
    ///
    /// The fix is Python's own rule, not an invention: a tab advances to the next multiple of eight,
    /// which is what `str.expandtabs()` and the `CPython` tokenizer do.
    #[test]
    fn a_tab_indented_section_body_is_scaffolding_like_a_space_indented_one() {
        // Arrange — the same docstring twice, differing only in how the entry is indented.
        let tabbed = "\"\"\"Update a webhook entry based on project_id or group_id.\n\n\
                      \x20   Args:\n\
                      \twebhook: object containing the updated fields and either project_id\n\
                      \tor group_id as the identifier.\n\
                      \x20   \"\"\"";
        let spaced = "\"\"\"Update a webhook entry based on project_id or group_id.\n\n\
                      \x20   Args:\n\
                      \x20       webhook: object containing the updated fields and either project_id\n\
                      \x20       or group_id as the identifier.\n\
                      \x20   \"\"\"";

        // Act & Assert
        let expected = "update a webhook entry based on project id or group id";
        assert_eq!(narrative(spaced), expected, "the control must strip");
        assert_eq!(
            narrative(tabbed),
            expected,
            "the tab-indented entry survived"
        );
    }

    /// A comment run is narrative in full — which is why cluster #1 is untouched.
    ///
    /// The grammar is written for docstring bodies and matches a line's trimmed text, so `# Args:`
    /// is not `Args:`. Stated as a test because it is load-bearing: comment runs are where the
    /// annotated sample's clearest genuine findings live, and none of them may move.
    #[test]
    fn a_comment_run_is_narrative_in_full() {
        // Arrange — every marker the grammar knows, behind a `#`.
        let run = "# Args:\n\
                   #     the retry budget here is deliberately small\n\
                   # :param timeout: and that matters, because the upstream\n\
                   # >>> service rate limits us on every fourth call\n";

        // Act & Assert
        assert_eq!(narrative(run), normalize(run));
    }
}
