//! The rule-code namespace and the per-block opt-out marker that switches one off.
//!
//! # The codes
//!
//! | code | rule | shape of the finding |
//! |---|---|---|
//! | `TPX001` | a comment run longer than its word limit | one block |
//! | `TPX002` | a docstring longer than its word limit | one block |
//! | `TPX003` | one explanation repeated across blocks | a cluster of blocks |
//!
//! `TPX004` is **reserved** for the code-restatement rule and there is deliberately **no variant
//! for it**. That rule was evaluated on the reference corpus and closed NO-SHIP, so a variant would
//! be a render branch, a match arm and a JSON case that no test could ever reach. Forward
//! compatibility is bought with `#[non_exhaustive]` instead, which is what that attribute is for.
//! The number is held so that the second epic renames nothing.
//!
//! Duplicate prose is `TPX003` and not `TPX001`. Volume is the base of the library — the tool is
//! called *tooprolix* — so the first numbers belong to it. Any place that still reads `TPX001` as
//! "duplicates" predates 2026-07-26. The codes freeze into the JSON schema and into every opt-out
//! marker a user writes, so this is the last moment they can be chosen for free.
//!
//! # The opt-out marker
//!
//! ```python
//! # !TPX002
//! """A docstring that earns its length."""
//! ```
//!
//! **The marker is a comment on the physical line immediately above the block it silences.** One
//! rule, both kinds of block, no per-kind branch — and it is legal Python in front of a module,
//! class or function docstring alike, because comments are trivia and the string is still the first
//! *statement* of the body.
//!
//! The issue's contract offered a second form for docstrings — a marker inside the docstring's own
//! first line, or a comment above the `def`/`class` line. **Both are rejected here**, and for a
//! reason worth keeping: a marker inside the docstring would be counted as part of the prose it
//! excuses (`extract` normalises the whole literal, so `# !TPX002` would add words to
//! the very number the rule is measuring), and a marker above `def` is a different distance from the
//! block depending on how many decorators sit in between. Neither is wrong; both need a second rule
//! where one suffices.
//!
//! Only the line *immediately* above counts, so two stacked markers are one marker and a comment.
//!
//! ```python
//! # !TPX002                          silence one rule
//! # !TPX001,TPX002                   several
//! # !TPX*                            every rule on this block
//! # !TPX002 the table below is the contract   a reason, after the codes
//! ```
//!
//! ## Why the marker does not say `noqa`, which is what 0.1.0 shipped
//!
//! `# tooprolix: noqa TPX002` was the 0.1.0 form and **0.2.0 does not accept it at all** — it is not
//! a marker, it is a comment, and [`is_near_miss`] reports it so the migration is loud rather than a
//! silently reinstated finding. The replacement is not a shortening for its own sake. Measured on
//! ruff 0.16.0 and flake8 7.3.0, with the marker on its own line above a docstring:
//!
//! | grammar | ruff codes | survives `ruff check --fix` | blanket-suppresses ruff on a code line |
//! |---|---|---|---|
//! | `# tooprolix: noqa TPX001` (0.1.0) | — | yes | no |
//! | `# noqa TPX001` | RUF100 | **no — the line is deleted** | **yes, ruff and flake8** |
//! | `# noqa: TPX001` | RUF102 | **no — the line is deleted** | no |
//! | `# !TPX001` | — | yes | no |
//!
//! Every namespace collision was created by the word `noqa` itself, so dropping it closes the
//! conflict by construction rather than by mitigation — and `pyproject.toml` enables RUF100 in this
//! very repository, so a `noqa` spelling would have had `ruff check --fix` delete our own markers.
//! `tests/ruff_compatibility.rs` holds that measurement as a test rather than as a paragraph.
//!
//! ## The space after `#` is part of the grammar
//!
//! `# !TPX002` is a marker and **`#!TPX002` is not**. The reason is measured, not stylistic: without
//! the rule, the shebang `#!/usr/bin/env python` sitting above a module docstring parses as a marker
//! naming an unrecognised code. Measured over the pinned corpus as it stands (6 repositories, 3913
//! `.py` files): **25** files open with a shebang, **11** of them reach the parser as the line above
//! a prose block — verified by building the rule out and counting the diagnostics — and **0** lines
//! anywhere match `# !…`, which is why that shape was free to take.
//!
//! ## What is copied from ruff, and where this deliberately differs
//!
//! The shape of the parser is `crates/ruff_linter/src/noqa.rs` at the pinned reference
//! (`NoqaLexer::lex_file_exemption`): eat the `#`, allow whitespace anywhere a human would, and
//! treat anything after the codes as a comment rather than as an error. Three divergences, all
//! deliberate:
//!
//! * **the directive is `!`, not a keyword**, for the measured reason above;
//! * **squashed codes are not split.** ruff recovers `F401F841` into two codes with a warning; here
//!   `TPX001TPX002` is the shape of no code at all, so the line is not a marker — and it is still
//!   audible, because [`is_near_miss`] reports it. What must never happen is the recovery: a marker
//!   silencing two rules the author did not separately write;
//! * **trailing prose is a reason, and cannot be anything else.** ruff reads `# noqa this is fine`
//!   as a blanket suppression, because its marker sits *after code* where a trailing comment is the
//!   norm. Ours owns its whole line, so **the code list stops at the first space** and everything
//!   past it is the author's reason — whatever it is shaped like. That sentence stood here while it
//!   was false in the way that matters: the tokeniser split on whitespace as well as commas, so
//!   `# !TPX002 TPX* would be overkill here` reached the blanket state out of an English clause,
//!   silencing every rule on the block without a word. Reading trailing prose as codes is the one
//!   mistake that can make a marker silence *more* than it says, and a comma is now the only thing
//!   that can extend a code list. See `collect_codes` (private).
//!
//! ## A marker has to name a code in OUR namespace, or English silences a block
//!
//! **After `# !` the directive must begin with `TPX*` or `TPX` followed by digits.** Anything else
//! is an ordinary comment, and stays one.
//!
//! This is not tidiness, and it took two attempts. Without any such rule,
//! `# !important: never cache this response` — written by somebody who has never heard of this tool
//! — was a marker naming an unrecognised code, so [`crate::extract`] dropped the line as a
//! directive, the prose left under it fell below the two-line block gate, and the finding vanished
//! **in silence**: no block survived to carry the unknown-code warning. The first fix then tested
//! the *general* code shape `[A-Z]+[0-9]+`, which is ruff's shape for anybody's rule code, and the
//! same defect walked straight back in through `HTTP2`, `UTF8`, `SHA256`, `RFC2119`, `ISO8601` and
//! `TLS13`. Measured: `# !HTTP2 is mandatory` removed a whole comment run, silently. The question
//! "should this token be reported as an unknown code?" and the question "may this line silence a
//! rule?" have different answers, and only the second one may ever be answered by a general shape.
//!
//! The price is taken deliberately and is the smaller one: `# !nonsense` is not reported as a
//! mistyped code, because it is not a marker at all. No shape test can separate a typo from a
//! sentence, and only one of the two directions fails closed.
//!
//! **`TPX*` is a literal token and not a glob.** `TPX0*`, `TPX00*`, `TP*` and `TPX` are
//! unrecognised: not markers, and each one **reported** by [`is_near_miss`], because a token that
//! begins with our namespace was aiming at a code. `# !*` and `# !HTTP2 …` begin with somebody
//! else's, so they are simply comments and are left alone. Every one of them silences nothing, which
//! is the property that matters — a glob engine has no consumer, and every star form that is not the
//! one blanket token has to fail *closed*, or the defect below returns in a new costume.
//!
//! ## An unknown code in a marker warns; an unknown code in the config is fatal
//!
//! `# !TPX999` silences **nothing** and prints a diagnostic naming the file, the line
//! and the token. It does not stop the run, and that asymmetry with [`crate::config`] — where an
//! unknown code exits 2 — is a decision, not an oversight. The config is one file that belongs to
//! the tool, so a typo there is cheap to fix and expensive to ignore. Markers are scattered through
//! somebody else's source, and a hard failure would turn a typo in one comment into a red build on
//! an unrelated file. Both are loud; only one is fatal.
//!
//! A typo in the *keyword* used to be the silent half of the same class — `# toprolix: noqa TPX002`
//! left the finding in place and said nothing — and [`is_near_miss`] is what closes it: a comment
//! above a block that was clearly aiming at a marker and missed is reported, whatever it misspelt.
//!
//! **Either way the typo fails closed**: the rule stays on and the finding still appears, so a
//! mistyped marker cannot be a gate switched off by accident. That sentence stood here while it was
//! **false** — `# tooprolix: noqa tpx001` silenced every rule on its block without a word, and,
//! because `TPX003` suppression is applied before clustering, removed the block from a cluster
//! nobody had marked. It is true now only because `collect_codes` (private) was rebuilt so the blanket form
//! cannot be reached by *failing* to recognise something. Read that function before changing this
//! one; the invariant is **no token a human meant as a code may widen a marker's scope**, and it is
//! pinned by `a_marker_that_names_something_unrecognised_never_widens_to_a_blanket`.

use crate::extract::ProseKind;

/// One shipping rule, and the single owner of its `TPX` code.
///
/// `#[non_exhaustive]`: `TPX004` and whatever the second epic adds are new variants, and a consumer
/// that matches on this must keep compiling across that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Rule {
    /// `TPX001` — a comment run longer than [`crate::detect::volume::Limits::comment_max_volume`].
    CommentVolume,
    /// `TPX002` — a docstring longer than [`crate::detect::volume::Limits::docstring_max_volume`].
    DocstringVolume,
    /// `TPX003` — one explanation repeated across two or more blocks.
    DuplicateProse,
}

impl Rule {
    /// Every rule that ships, in code order.
    ///
    /// The single owner of "which codes exist": [`Self::from_code`] and the "everything is ignored"
    /// diagnostic both read it, so a fourth rule is one line here rather than three edits that can
    /// disagree.
    pub const ALL: [Self; 3] = [
        Self::CommentVolume,
        Self::DocstringVolume,
        Self::DuplicateProse,
    ];

    /// The `TPX` code, as it appears in output, in `ignore` and in a marker.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::CommentVolume => "TPX001",
            Self::DocstringVolume => "TPX002",
            Self::DuplicateProse => "TPX003",
        }
    }

    /// The rule named by `code`, or `None` if no rule has that code.
    ///
    /// Exact match, upper case: the codes are printed upper case everywhere and accepting
    /// `tpx001` would make the same configuration file mean two things in two versions of the tool
    /// the day case folding is reconsidered.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|rule| rule.code() == code)
    }

    /// The volume rule that applies to a block of `kind`.
    ///
    /// The single owner of the kind-to-code mapping, next to
    /// [`crate::detect::volume::Limits::max_volume`], which owns the kind-to-*limit* half. A second
    /// `match` elsewhere is how a docstring quietly starts being reported as `TPX001`.
    #[must_use]
    pub const fn volume_for(kind: ProseKind) -> Self {
        match kind {
            ProseKind::Comment => Self::CommentVolume,
            ProseKind::Docstring => Self::DocstringVolume,
        }
    }
}

/// One row of the catalogue `tooprolix --rules` prints and `--help` embeds.
///
/// The fields are the three columns of the table in `docs/rules-and-configuration.md` and in the
/// README, in that order, because those two files and this array now say the same sentence and a
/// test compares them byte for byte.
///
/// `status` is a `&'static str` and not an enum on purpose, against `type-no-stringly`: it is a
/// *label*, not a decision anything branches on — nothing in this crate reads it, three documents
/// render it, and `CONTRIBUTING.md`'s release-day checklist flips every one of them from
/// `Implemented` to `Released` on publication day. An enum would turn that one-word edit into a
/// variant rename plus a `match`, and would still not stop the label being wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Documented {
    /// The `TPX` code, as it appears in output and in a marker.
    pub code: &'static str,
    /// One line saying what the rule detects.
    pub description: &'static str,
    /// Whether the rule ships today.
    pub status: &'static str,
}

/// Every `TPX` number that has been spoken for, shipping or not, with its one-line description.
///
/// 🔴 **This is not the registry of rules that exist — [`Rule::ALL`] is, and it stays three long.**
/// The distinction is load-bearing: `Rule::ALL` answers "which codes does this tool accept", and
/// three separate consumers read it for that ([`Rule::from_code`], `ignore` validation in
/// [`crate::config`], and marker parsing below). Adding a `TPX004` variant so that `--rules` could
/// print it would have made `ignore = ["TPX004"]` and `# !TPX004` start being *accepted* — a
/// configuration silently switching off a detector that does not exist, and a marker silently
/// claiming to suppress one. `TPX004` is documented here and refused there, which is the honest
/// pair, and `the_registry_is_the_single_owner_of_the_codes` pins both halves.
///
/// The single owner of the **text**. Before this existed the same sentence was written three times
/// — in the help string, in the README table, and in `docs/rules-and-configuration.md` — and two of
/// the three had already drifted apart. The wording here is the documentation's, because a user
/// reads the table far more often than the help.
pub const CATALOGUE: [Documented; 4] = [
    Documented {
        code: "TPX001",
        description: "A comment run longer than its word limit",
        status: "Implemented",
    },
    Documented {
        code: "TPX002",
        description: "A docstring longer than its word limit",
        status: "Implemented",
    },
    Documented {
        code: "TPX003",
        description: "One explanation repeated across comments and docstrings, reported once with \
                      every place it appears",
        status: "Implemented",
    },
    Documented {
        code: "TPX004",
        description: "Comments that restate the following code",
        status: "Reserved",
    },
];

impl serde::Serialize for Rule {
    /// A rule is its code in JSON, never its Rust variant name.
    ///
    /// Hand-written rather than `#[derive(Serialize)]` with a rename per variant, because the code
    /// is already spelled once in [`Self::code`] and a `#[serde(rename)]` next to it would be a
    /// second spelling that a rename could silently desynchronise.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// What one opt-out marker silences for the block below it.
///
/// The empty value silences nothing, which is what a block with no marker gets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suppression {
    /// A marker with no codes at all: every rule is silenced for this block.
    all: bool,
    /// The rules the marker named, in the order it named them.
    codes: Vec<Rule>,
    /// Tokens that looked like a code and match no rule. Kept, not dropped, so the caller can say
    /// which token it was.
    unknown: Vec<String>,
}

impl Suppression {
    /// Whether this marker silences `rule`.
    #[must_use]
    pub fn silences(&self, rule: Rule) -> bool {
        self.all || self.codes.contains(&rule)
    }

    /// Codes the marker named that no rule answers to.
    #[must_use]
    pub fn unknown_codes(&self) -> &[String] {
        &self.unknown
    }
}

/// Reads the opt-out marker out of one physical source line, if that line is one.
///
/// Returns `None` when the line is not a marker at all — including `#!TPX002`, where the space the
/// grammar requires is missing, `# !` on its own, and `# !anything that is not a code` — a marker
/// has to NAME something of code shape. None of them suppresses anything, which is the closed
/// direction; [`is_near_miss`] is what makes the ones that were aiming at a marker audible.
///
/// # Examples
///
/// ```
/// use tooprolix::rules::{Rule, parse_marker};
///
/// let marker = parse_marker("    # !TPX002").expect("this is a marker");
/// assert!(marker.silences(Rule::DocstringVolume));
/// assert!(!marker.silences(Rule::DuplicateProse));
///
/// // `TPX*` is the one blanket token, and it is a literal — not a glob.
/// assert!(parse_marker("# !TPX*").expect("blanket marker").silences(Rule::CommentVolume));
///
/// assert_eq!(parse_marker("# an ordinary comment"), None);
/// assert_eq!(parse_marker("#!/usr/bin/env python"), None);
///
/// // A directive that names nothing of code shape is prose, not a marker that silences nothing.
/// assert_eq!(parse_marker("# !important: never cache this response"), None);
/// ```
#[must_use]
pub fn parse_marker(line: &str) -> Option<Suppression> {
    // The whitespace between `#` and `!` is part of the contract, not slack in it. Without it the
    // shebang `#!/usr/bin/env python` above a module docstring is a marker naming an unrecognised
    // code, and 11 of them land above a prose block in the pinned corpus, against 0 lines of the
    // form `# !…` anywhere in it.
    //
    // `strip_prefix` with a `char` predicate, never a byte index: this function shipped
    // `rest[..4]` once, and a length is not a char boundary — a 3-byte lead character put byte 4
    // inside it and the slice panicked with exit 101, outside the 0/1/2 contract, crossing the pyo3
    // boundary as a `PanicException` that does not inherit `Exception`. Nothing here indexes bytes.
    let directive = comment_body(line)?
        .strip_prefix(char::is_whitespace)?
        .trim_start()
        .strip_prefix('!')?;

    // **The code list is everything up to the first space; the rest is the author's reason.** The
    // comma is the only separator, which is what `README.md` has always documented and what the
    // parser did not do: it split on comma and whitespace alike, so a word standing after a code
    // the author really meant was read as another code. Measured — `# !TPX002 TPX* would be
    // overkill here` silenced **every** rule on the block and said nothing, and
    // `# !TPX002 TPX001 was fixed above` silenced `TPX001`. Silent and suppressing, reached by an
    // English sentence, which is the third time this seam has been re-armed.
    let list = code_list(directive)?;

    // **The directive has to begin with a token of code shape, or this is not a marker.** Measured
    // defect: without this test `# !important: never cache this response` — English, written by
    // somebody who has never heard of this tool — parsed as a marker naming an unrecognised code,
    // `crate::extract` dropped the line as a directive, the remaining prose fell under the two-line
    // block gate, and the finding disappeared **with no warning at all**, because no block survived
    // to carry one. Silent suppression by prose, reachable without ever writing `TPX*`.
    //
    // The price is real and is taken deliberately: `# !nonsense` is no longer reported as a mistyped
    // code, because it is no longer a marker. A shape test cannot tell a typo from a sentence, and
    // between eating prose and missing a typo, only one of the two can fail closed.
    let first = tokens(list).next()?;
    if first != BLANKET && !is_code_shaped(first) {
        return None;
    }

    Some(collect_codes(list))
}

/// The text of `line` after its `#`, or `None` when `line` is not a comment.
///
/// **The byte order mark is stripped here, and that is not cosmetic.** A BOM is legitimate in
/// Python — `CPython` runs such a file — and it sits on byte 0 of the first physical line, exactly
/// where a module-level marker lives. `str::trim_start` does *not* remove U+FEFF (it is `Cf`, not
/// whitespace), so `\u{feff}# !TPX002` was "not a comment" to this module while [`crate::extract`],
/// reading the tokenizer's output, had already excluded the same line as a directive. One line, two
/// owners, opposite answers — and the visible half was a correct marker that silently stopped
/// working.
fn comment_body(line: &str) -> Option<&str> {
    line.trim_start_matches('\u{feff}')
        .trim_start()
        .strip_prefix('#')
}

/// The code list at the head of `directive`: everything before the first whitespace.
///
/// The single owner of **where the codes stop and the reason starts**, and there has to be exactly
/// one, because the answer was previously "nowhere" — see `collect_codes` (private).
fn code_list(directive: &str) -> Option<&str> {
    directive.split_whitespace().next()
}

/// The code list split into tokens, empties dropped. **A comma, and nothing else, separates codes.**
fn tokens(list: &str) -> impl Iterator<Item = &str> {
    list.split(',').filter(|token| !token.is_empty())
}

/// Whether `line` was *trying* to be an opt-out marker and failed.
///
/// The answer is only ever `true` for a line [`parse_marker`] rejected, and it exists because the
/// loud half of the contract had a hole in it. A typo in a **code** has always been loud
/// (`# !TPX999` warns and names the token); a typo in the **directive** was silent — measured on
/// the 0.1.0 binary, `# toprolix: noqa TPX002`, `# tooprolix noqa TPX002` and
/// `# tooprolix: noqua TPX002` each left the finding in place and said nothing at all. The 0.2.0
/// grammar makes that hole *wider*, not narrower: a forgotten `!` leaves `# TPX002`, one character
/// away from a working marker and with less redundancy than two misspellable words had.
///
/// **It keys on our namespace, never on a bare `!`.** Three clauses:
///
/// * the line is `# !…` and its **first token** begins with `tp` in any case — a directive that was
///   aiming at a code of ours and missed: `# !TP*`, `# !TPX`, `# !TPX_001`, `# !TPX0*`.
///   the test is `starts_with`, never `contains`, because `HTTP2` contains `tp`;
/// * or the body names something of the form `TPX` followed by a digit or a star, in any case —
///   which covers every 0.1.0 marker that carried a code, every misspelt keyword in front of a real
///   one, `#!TPX002` and `#!TPX*` with the space forgotten, and `# TPX002` with the `!` forgotten;
/// * or the body is the 0.1.0 **blanket** spelling, which carries no code at all and is therefore
///   invisible to both clauses above.
///
/// Keying on `!` instead is what an earlier version did, and it was the same defect as the parser's:
/// `# !` is a shape ordinary English reaches by accident. `#!/usr/bin/env python` is excluded for
/// free now — it names no code — where before it needed a rule of its own.
///
/// It costs a false positive on a comment like `# TPX002 needs fixing` written directly above a
/// block. That is the accepted price: the diagnostic changes no exit code and silences nothing, so
/// the cost of the false positive is one line of stderr, while the cost of the silence it replaces
/// is a suppression the author believes is in place and is not. Measured over the 6 pinned corpus
/// repositories: **0** lines fire it.
///
/// ```
/// use tooprolix::rules::is_near_miss;
///
/// assert!(is_near_miss("# tooprolix: noqa TPX002"));  // the 0.1.0 marker
/// assert!(is_near_miss("# tooprolix: noqa"));         // ... and its code-less blanket form
/// assert!(is_near_miss("# TPX002"));                  // the `!` forgotten
/// assert!(is_near_miss("#!TPX002"));                  // the space forgotten
/// assert!(is_near_miss("# !TP*"));                    // aimed at our namespace, missed
/// assert!(!is_near_miss("# !TPX002"));                // ... this one works
/// assert!(!is_near_miss("#!/usr/bin/env python"));
/// assert!(!is_near_miss("# an ordinary comment"));
/// assert!(!is_near_miss("# !important: never cache this response"));  // prose, and left alone
/// assert!(!is_near_miss("# !HTTP2 is mandatory"));    // somebody else's namespace
/// ```
#[must_use]
pub fn is_near_miss(line: &str) -> bool {
    if is_marker(line) {
        return false;
    }
    let Some(body) = comment_body(line) else {
        return false;
    };

    // A `# !…` line whose FIRST token was aiming at our namespace and is not a code: `# !TP*`,
    // `# !TPX`, `# !TPX_001`. `EPIC.md` Decisions #9 and #10 both name the starred forms as
    // warnings, and this is what makes them one — while leaving `# !HTTP2 is mandatory` alone,
    // which is the whole reason the test is `starts_with` and not `contains`.
    if body
        .strip_prefix(char::is_whitespace)
        .map(str::trim_start)
        .and_then(|rest| rest.strip_prefix('!'))
        .and_then(code_list)
        .and_then(|list| tokens(list).next())
        .is_some_and(aims_at_our_namespace)
    {
        return true;
    }

    // ... and a line that names a code without being a directive at all: the `!` forgotten
    // (`# TPX002`), the space forgotten (`#!TPX002`, `#!TPX*`), or a 0.1.0 marker with a code in it.
    //
    // `get`, not a slice: `to_ascii_lowercase` leaves every non-ASCII byte where it was, so the
    // byte after a match is not guaranteed to be a char boundary and must never be sliced at.
    let lowered = body.to_ascii_lowercase();
    let names_a_code = lowered.match_indices("tpx").any(|(at, _)| {
        lowered
            .as_bytes()
            .get(at + NAMESPACE.len())
            .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'*')
    });

    // The 0.1.0 blanket marker is the one legacy spelling that carries no code at all, so neither
    // clause above can see it — and it is the spelling whose upgrade goes wrong twice over: it stops
    // suppressing, and its own two words are then counted as prose. Measured: a 149-word comment run
    // became 151 words and started reporting `TPX001` that the same run without the dead marker
    // never reported. Both halves are silent without this clause.
    names_a_code || is_legacy_marker(body)
}

/// Whether `line` is an opt-out marker at all, without asking what it silences.
///
/// The grammar's single public question, and it exists for [`crate::extract`]: extraction has to
/// *exclude* a marker line from the prose it measures, and it must not answer "is this a marker?"
/// with a spelling of its own. It had one — `starts_with("tooprolix:")` — and the two disagreed in
/// **both** directions, each disagreement silently moving a reported word count. One grammar, one
/// owner, and this is how the other side asks.
///
/// ```
/// use tooprolix::rules::is_marker;
///
/// assert!(is_marker("    # !TPX001"));
/// assert!(!is_marker("# tooprolix: noqa TPX001"));
/// assert!(!is_marker("# tooprolix is a linter"));
/// ```
#[must_use]
pub fn is_marker(line: &str) -> bool {
    parse_marker(line).is_some()
}

/// The one token that means "every rule on this block".
///
/// A **literal**, compared with `==`. It is not a glob and there is no glob: `TPX0*` and `TP*` are
/// unrecognised, warn through [`is_near_miss`] and silence nothing. Widening this into a pattern
/// language would
/// re-open the defect below by a different door — the blanket state would once again be reachable by
/// a token nobody spelled out — and no consumer has asked for it.
const BLANKET: &str = "TPX*";

/// Reads the code list of a marker: `TPX001, TPX003`, `TPX001 TPX003`, or `TPX*`.
///
/// # The blanket form is reachable only by naming it, never by failing to parse
///
/// **A marker silences every rule if and only if it names [`BLANKET`].** Every other marker silences
/// exactly the codes it named and nothing more, even when it named none successfully.
///
/// That "if and only if" is the fix for a shipped defect, and it is worth stating why the earlier
/// shape was wrong rather than merely different. The first version reached the blanket state
/// whenever both lists came out empty, and guessed which unrecognised tokens were *code attempts*
/// with a heuristic ("upper-case first letter and a digit"). Everything the heuristic missed became
/// the blanket form: measured, `# tooprolix: noqa tpx001` and `# tooprolix: noqa TPX` silenced
/// **every** rule on the block with no diagnostic, and — because `TPX003` suppression is applied
/// before clustering — quietly removed the block from a cluster its author had never marked, which
/// then reported "2 places" instead of 3. A typo widened a marker's scope. The heuristic is gone
/// rather than widened: no guess about a token's intent can decide how much a marker silences.
///
/// The 0.2.0 grammar tightened the same invariant one turn further. Under 0.1.0 the blanket form was
/// *absence* — a directive with no text — so a parser that stopped reading early still landed on it.
/// Now absence is not a marker at all ([`parse_marker`] rejects `# !`), and the only road to `all`
/// is the `==` below.
///
/// # Where the code list ends, and why this function never has to decide
///
/// It does not end here. `list` is **already** only the code list — [`parse_marker`] cuts it at the
/// first whitespace via `code_list` (private) — so every token reaching this loop is something the
/// author wrote between commas, and every one of them is a code attempt. There is no "is this still
/// a code, or has the reason started?" question left to get wrong, and getting it wrong is the
/// entire history of this function.
///
/// Two earlier answers, both shipped, both defects:
///
/// * *"the list ends at the first token that is not code shaped, and if nothing was read yet that
///   token is recorded as unknown"* — the second clause made `# !important: never cache this` a
///   marker naming an unrecognised code, which then removed the block below it;
/// * *"the list ends at the first token that is not code shaped, full stop"* — that reads
///   `# !TPX002 TPX* would be overkill here` as **two** tokens, because the tokeniser split on
///   whitespace as well as commas. Measured: every rule on the block silenced, no warning.
///
/// # An unrecognised token warns here, and prose does not
///
/// That asymmetry is what the comma buys. `# !TPX002,TPX0*` warns about `TPX0*`, because a token
/// written inside the list can only have been meant as a code — `EPIC.md` Decisions #9 requires
/// exactly that of every starred form. `# !TPX002 TPX0* was rejected` says nothing, because after
/// the space the author is writing English. Same token, different position, and the position is
/// unambiguous rather than inferred.
fn collect_codes(list: &str) -> Suppression {
    let mut suppression = Suppression::default();

    for token in tokens(list) {
        if token == BLANKET {
            suppression.all = true;
        } else if let Some(rule) = Rule::from_code(token) {
            suppression.codes.push(rule);
        } else {
            // Code shaped or not, a token inside the comma list was meant as a code and answers to
            // no rule. `crate::cli` names it.
            suppression.unknown.push(token.to_owned());
        }
    }

    suppression
}

/// `TPX` and then digits — the shape of a code in **our** namespace, and nothing else.
///
/// This used to be the general `[A-Z]+[0-9]+`, ruff's shape for anybody's rule code, and the
/// difference is not academic. That predicate is the right answer to "should this token be reported
/// as an unknown code?" and the **wrong** answer to "does this line silence a rule?", because it
/// accepts `HTTP2`, `UTF8`, `SHA256`, `RFC2119`, `ISO8601` and `TLS13`. Measured on the built
/// binary: `# !HTTP2 is mandatory` above a comment run removed the whole block, in silence — a
/// comment about a wire protocol switching off a lint. One namespace, one question, one predicate.
///
/// Case-insensitive on the prefix so that `tpx001` is still *a code*, and therefore still reported
/// as one nobody answers to. [`Rule::from_code`] is the case-sensitive half, deliberately: a code is
/// upper case, and getting the case wrong is a typo the tool names rather than a second spelling it
/// accepts.
///
/// `str::get`, never a slice: it returns `None` rather than panicking when byte 3 falls inside a
/// character, which `# !AB日` does. `parse_marker` shipped exit 101 from exactly that reasoning
/// once; nothing here can repeat it.
fn is_code_shaped(token: &str) -> bool {
    token
        .get(..NAMESPACE.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(NAMESPACE))
        && token.len() > NAMESPACE.len()
        && token.as_bytes()[NAMESPACE.len()..]
            .iter()
            .all(u8::is_ascii_digit)
}

/// Our namespace, and the first thing every code in it spells.
const NAMESPACE: &str = "TPX";

/// Whether `token` was **aiming** at our namespace, however badly.
///
/// The line between a mistyped marker that must be reported and a sentence that must be left alone,
/// and the only thing that can draw it: `# !TP*` is somebody reaching for `TPX*`, `# !HTTP2 is
/// mandatory` is somebody talking about HTTP/2. Both are `# !` followed by a token that is not a
/// code, so nothing else about their shape separates them.
///
/// **`starts_with`, never `contains`** — `HTTP2` contains `tp` and must stay silent. Two characters
/// rather than three, so that `# !TP*` and `# !tp` are caught as well as `TPX`-something.
fn aims_at_our_namespace(token: &str) -> bool {
    token
        .as_bytes()
        .get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"tp"))
}

/// Whether `body` — the text after a `#` — is the **0.1.0** marker, `tooprolix : noqa …`.
///
/// The 0.1.0 grammar, walked the way 0.1.0 walked it, and not a search for its words. Asking whether
/// the body merely *contained* `tooprolix` and `noqa` reported `# tooprolix deliberately avoids noqa
/// semantics` — a sentence about the tool, above a block — as a dead marker.
fn is_legacy_marker(body: &str) -> bool {
    let Some(rest) = body.trim_start().strip_prefix("tooprolix") else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix(':') else {
        return false;
    };
    rest.trim_start()
        .get(.."noqa".len())
        .is_some_and(|keyword| keyword.eq_ignore_ascii_case("noqa"))
}

#[cfg(test)]
mod tests {
    use super::{CATALOGUE, Rule, parse_marker};
    use crate::extract::ProseKind;

    /// The code namespace is a published contract: a rename after 0.1.0 breaks the JSON schema, the
    /// Python API and every marker a user has written. Pinning the strings here makes that a red
    /// test rather than a silent break.
    #[test]
    fn each_rule_owns_exactly_one_code_and_the_codes_are_the_shipped_ones() {
        assert_eq!(Rule::CommentVolume.code(), "TPX001");
        assert_eq!(Rule::DocstringVolume.code(), "TPX002");
        assert_eq!(Rule::DuplicateProse.code(), "TPX003");
        assert_eq!(Rule::ALL.len(), 3, "the registry is three rules, not four");

        for rule in Rule::ALL {
            assert_eq!(Rule::from_code(rule.code()), Some(rule));
        }
        assert_eq!(
            Rule::from_code("TPX004"),
            None,
            "TPX004 is a reserved number and must not resolve to a rule"
        );
        assert_eq!(Rule::from_code("TPX999"), None);
        assert_eq!(Rule::from_code("tpx001"), None, "codes are upper case");
    }

    /// The catalogue documents every number; `Rule::ALL` decides which ones the tool *accepts*.
    ///
    /// Two things go wrong if these drift, and each has its own assertion below. Renaming a code in
    /// one place leaves `--rules` describing `TPX003` under a heading nothing answers to. Promoting
    /// the catalogue into the acceptance path — the tempting way to make `--rules` list `TPX004` —
    /// makes `ignore = ["TPX004"]` and `# !TPX004` start being accepted for a detector that does
    /// not exist, which is the pair `each_rule_owns_exactly_one_code…` pins from the other side.
    #[test]
    fn the_catalogue_documents_the_rules_that_exist_and_one_that_deliberately_does_not() {
        for (rule, documented) in Rule::ALL.into_iter().zip(CATALOGUE) {
            assert_eq!(
                rule.code(),
                documented.code,
                "the catalogue and the rule registry disagree about the codes, in order"
            );
            assert_eq!(documented.status, "Implemented");
        }

        // Codes and statuses lining up is not enough: swapping the TPX001 and TPX002 *descriptions*
        // and updating both Markdown copies to match left all six new tests passing while the tool
        // documented the comment detector as the docstring one. The oracle is the kind-to-code
        // mapping this module already owns, so a description is checked against the detector that
        // actually runs under that code rather than against nothing.
        for (kind, own, other) in [
            (ProseKind::Comment, "comment", "docstring"),
            (ProseKind::Docstring, "docstring", "comment"),
        ] {
            let code = Rule::volume_for(kind).code();
            let documented = CATALOGUE
                .iter()
                .find(|entry| entry.code == code)
                .unwrap_or_else(|| panic!("{code} runs but is not documented"));
            let description = documented.description.to_lowercase();
            assert!(
                description.contains(own) && !description.contains(other),
                "{code} detects {kind:?} blocks, but its description is about the other kind: {:?}",
                documented.description
            );
        }

        let reserved = CATALOGUE[Rule::ALL.len()];
        assert_eq!(reserved.code, "TPX004");
        assert_eq!(reserved.status, "Reserved");
        assert_eq!(
            Rule::from_code(reserved.code),
            None,
            "a documented code became an accepted one: `ignore` and markers would now take TPX004"
        );
        assert_eq!(
            CATALOGUE.len(),
            Rule::ALL.len() + 1,
            "the catalogue grew without the rules doing so, or the reverse"
        );
    }

    /// A docstring must never be reported under the comment code, and the reverse.
    #[test]
    fn the_volume_code_follows_the_kind_of_the_block() {
        assert_eq!(Rule::volume_for(ProseKind::Comment), Rule::CommentVolume);
        assert_eq!(
            Rule::volume_for(ProseKind::Docstring),
            Rule::DocstringVolume
        );
    }

    /// The documented form from `README.md`, plus every spelling a human would expect to work.
    #[test]
    fn a_marker_names_the_rules_it_silences() {
        for line in [
            "# !TPX002",
            "    # !TPX002",
            "#  !TPX002",
            "#\t!TPX002",
            "# ! TPX002",
            "# !TPX002 because the table is the contract",
            "# !TPX002,TPX004",
        ] {
            let marker = parse_marker(line).unwrap_or_else(|| panic!("not parsed: {line}"));
            assert!(
                marker.silences(Rule::DocstringVolume),
                "did not silence its own code: {line}"
            );
            assert!(
                !marker.silences(Rule::DuplicateProse),
                "silenced a rule it never named: {line}"
            );
        }
    }

    /// `TPX*` is the blanket form; a marker with only unknown codes is not.
    ///
    /// The second half is the one that matters: if an unrecognised code collapsed to "silence
    /// everything", `# !TPX999` would be the most dangerous typo in the tool.
    #[test]
    fn the_blanket_token_silences_everything_and_a_mistyped_marker_silences_nothing() {
        let blanket = parse_marker("# !TPX*").expect("blanket marker");
        let mistyped = parse_marker("# !TPX999").expect("marker");

        for rule in Rule::ALL {
            assert!(blanket.silences(rule), "blanket marker missed {rule:?}");
            assert!(
                !mistyped.silences(rule),
                "an unknown code silenced {rule:?} — a typo must fail closed"
            );
        }
        assert_eq!(mistyped.unknown_codes(), ["TPX999"]);
        assert!(blanket.unknown_codes().is_empty());
    }

    /// The space after `#` is part of the grammar, and the shebang is why.
    ///
    /// Both halves in one test on purpose: "`#!TPX002` is not a marker" passes on a parser that
    /// rejects everything, so the working spelling is asserted beside it. `#!/usr/bin/env python`
    /// is the case that pays for the rule — 25 files in the pinned corpus open with one, and a
    /// module docstring on line 2 puts it exactly where a marker would be read from. Measured by
    /// deleting the space requirement and re-running the corpus: **11** of them then parse as a
    /// marker naming an unrecognised code, and print a diagnostic apiece.
    #[test]
    fn the_space_after_the_hash_is_part_of_the_grammar() {
        assert!(
            parse_marker("# !TPX002").is_some(),
            "the documented spelling stopped parsing"
        );

        for line in [
            "#!TPX002",
            "#!TPX*",
            "#!/usr/bin/env python",
            "#!/usr/bin/env python3",
        ] {
            assert_eq!(
                parse_marker(line),
                None,
                "a `#!` line was read as a marker: {line:?}"
            );
        }
    }

    /// A squashed code list is never split into the codes it looks like, and it silences nothing.
    ///
    /// ruff recovers `F401F841` into two codes with a warning. Here `TPX001TPX002` matches the shape
    /// of no code, so it is not a marker at all — and it is still audible, through `is_near_miss`,
    /// because it names `TPX` followed by a digit. What must never happen is the recovery: a marker
    /// that silences two rules the author did not separately write.
    #[test]
    fn squashed_codes_are_not_silently_recovered() {
        assert_eq!(parse_marker("# !TPX001TPX002"), None);
        assert!(super::is_near_miss("# !TPX001TPX002"));
    }

    /// Lines that are near-misses must not be markers, or a comment about the tool would silence
    /// the block under it.
    #[test]
    fn a_line_that_is_not_a_marker_is_not_read_as_one() {
        for line in [
            "# an ordinary comment",
            "# tooprolix is a linter",
            // The 0.1.0 marker. 0.2.0 replaced the grammar outright — there is no alias period —
            // so this line is a comment, and `is_near_miss` is what keeps that from being silent.
            "# tooprolix: noqa TPX001",
            "# tooprolix: noqa",
            // The `!` forgotten, which is the whole redundancy the grammar has.
            "# TPX001",
            "# noqa: TPX001",
            "# ruff: noqa",
            "x = 1  # !TPX001",
            // A marker that names nothing is not the blanket form and not a marker.
            "# !",
            "# !   ",
            "",
        ] {
            assert_eq!(parse_marker(line), None, "read as a marker: {line:?}");
        }
    }

    /// **No marker may ever widen its own scope by being mistyped.**
    ///
    /// Every line here names *something* and gets it wrong, and every one of them must silence
    /// nothing at all. The failure this pins is not theoretical: a token that missed the
    /// "code attempt" heuristic used to leave both lists empty, which collapsed to the blanket
    /// form — so `# tooprolix: noqa tpx001` silenced every rule on the block, with no warning, and
    /// quietly removed it from a cluster the author never marked.
    ///
    /// The starred forms are the 0.2.0 half of the same guarantee. `TPX*` is a **literal** token,
    /// so every other star spelling is an unrecognised code — and the one thing none of them may
    /// ever do is fall through to the blanket state, which would be that defect rebuilt on purpose.
    ///
    /// A line here fails closed in one of two ways, and the test accepts either: it is a marker that
    /// named only codes nobody answers to, or it is not a marker at all. What it may never be is a
    /// marker that silences a rule. The second way is the newer one — a directive that names nothing
    /// of code shape stopped being a marker, because reading one as "a marker that silences nothing"
    /// still handed the line to `extract` as a directive and ate the block below it.
    #[test]
    fn a_marker_that_names_something_unrecognised_never_widens_to_a_blanket() {
        for line in [
            "# !tpx001",
            "# !TPX",
            "# !TPX999",
            "# !TPX004",
            "# !TPX001TPX002",
            "# !this one is on purpose",
            "# !001",
            "# !important: never cache this response",
            // Not a glob engine: every star form except the literal blanket token is a typo.
            "# !TPX0*",
            "# !TPX00*",
            "# !TP*",
            "# !T*",
            "# !*",
            "# !tpx*",
            "# !TPX**",
            "# !*TPX001",
        ] {
            let Some(marker) = parse_marker(line) else {
                continue; // Not a marker: it silences nothing by construction.
            };

            for rule in Rule::ALL {
                assert!(
                    !marker.silences(rule),
                    "{line:?} silenced {rule:?} \u{2014} a mistyped marker must fail closed"
                );
            }
            assert!(
                !marker.unknown_codes().is_empty(),
                "{line:?} was read as a marker, silenced nothing and said nothing about why"
            );
        }

        // The `continue` above must not be how every case passes: the two that ARE markers have to
        // stay markers, or this test would go green on a parser that rejects the entire grammar.
        assert!(parse_marker("# !TPX999").is_some());
        assert!(parse_marker("# !TPX004").is_some());
    }

    /// The blanket form is reachable **only** by naming `TPX*`.
    ///
    /// The pair to the test above: together they say the set of blanket markers is exactly the set
    /// of markers carrying that one literal token, so no third state can leak into it. Under 0.1.0
    /// the pair read "exactly the markers with no directive text"; naming the state out loud is
    /// strictly stronger, because a parser that gives up early now lands on *nothing* rather than
    /// on *everything*.
    #[test]
    fn only_the_literal_blanket_token_is_a_blanket() {
        for line in [
            "# !TPX*",
            "    # !TPX*",
            "# ! TPX*",
            "# !TPX*   ",
            "# !TPX* this whole block is deliberate",
            "# !TPX*  # and a second comment",
            "# !TPX*,TPX001",
        ] {
            let marker = parse_marker(line).unwrap_or_else(|| panic!("not parsed: {line}"));

            for rule in Rule::ALL {
                assert!(marker.silences(rule), "{line:?} missed {rule:?}");
            }
            assert!(marker.unknown_codes().is_empty(), "{line:?} warned");
        }
    }

    /// An explanation *after* a code that was recognised is prose — **whatever it looks like**.
    ///
    /// The test used to say only this much with a `because…` sentence, which is the one explanation
    /// that could never be mistaken for a code. Its own name promised more than that, and the
    /// guarantee behind the name was broken: a reason word shaped like a code was read as one.
    #[test]
    fn an_explanation_after_a_recognised_code_is_not_reported_as_a_typo() {
        for line in [
            "# !TPX001 because the table is the contract",
            // Every one of these is a reason whose first word is code-shaped, and none of them may
            // be read as a second code.
            "# !TPX001 TPX002 is handled by the schema",
            "# !TPX001 E501 lives in ruff, not here",
            "# !TPX001 HTTP2 framing is described below",
            "# !TPX001 TPX0* was considered and rejected",
        ] {
            let marker = parse_marker(line).unwrap_or_else(|| panic!("not parsed: {line}"));

            assert!(marker.silences(Rule::CommentVolume), "{line:?}");
            assert!(
                !marker.silences(Rule::DocstringVolume) && !marker.silences(Rule::DuplicateProse),
                "a word in the reason silenced a rule the marker never named: {line:?}"
            );
            assert!(
                marker.unknown_codes().is_empty(),
                "an explanation was reported as a mistyped code: {line:?} -> {:?}",
                marker.unknown_codes()
            );
        }
    }

    /// **The code list ends at the first space. Everything after it is the author's reason.**
    ///
    /// The third re-arming of one seam, and the first to hide in the *reason* position. `tokens`
    /// split on comma and whitespace alike, so a word standing after a perfectly good code was
    /// parsed as another code — including the blanket literal. Reproduced on the binary, on a
    /// comment run capable of surviving the loss of its marker line:
    ///
    /// ```text
    /// # !TPX002 blanket would be overkill here  → TPX001 reported  (correct)
    /// # !TPX002 TPX* would be overkill here     → nothing, and no warning
    /// # !TPX002 TPX001 was fixed above          → nothing, and no warning
    /// ```
    ///
    /// Silent **and** suppressing — the combination the whole grammar exists to make unreachable —
    /// and reached this time by an ordinary English sentence written after a code the author really
    /// did mean. `README.md` had already shipped the narrower contract ("Several codes are
    /// comma-separated, anything after them is your reason"); the parser was simply wider than its
    /// own documentation, and the width was the defect.
    ///
    /// The asymmetry below is the point of the comma. Inside a comma list every token is a code
    /// attempt, so an unrecognised one **warns**; after a space nothing warns, because it is prose.
    /// The comma is what makes the author's intent unambiguous.
    #[test]
    fn the_code_list_ends_at_the_first_space_and_the_rest_is_the_reason() {
        // A comma list is still a list.
        let list = parse_marker("# !TPX001,TPX002").expect("marker");
        assert!(list.silences(Rule::CommentVolume) && list.silences(Rule::DocstringVolume));
        assert!(list.unknown_codes().is_empty());

        // ... and a space is no longer a separator, so only the first code is named.
        let spaced = parse_marker("# !TPX001 TPX002").expect("marker");
        assert!(spaced.silences(Rule::CommentVolume));
        assert!(
            !spaced.silences(Rule::DocstringVolume),
            "a space separated two codes: the second was silenced without being written in the list"
        );

        // The blanket literal in the reason position must NOT widen the marker. This is the case
        // that was both silent and suppressing.
        let reasoned_blanket = parse_marker("# !TPX002 TPX* would be overkill here").expect("m");
        assert!(reasoned_blanket.silences(Rule::DocstringVolume));
        for rule in [Rule::CommentVolume, Rule::DuplicateProse] {
            assert!(
                !reasoned_blanket.silences(rule),
                "the blanket token was reached from the reason position: {rule:?}"
            );
        }
        assert!(reasoned_blanket.unknown_codes().is_empty());

        // A real code in the reason position is prose too.
        let reasoned_code = parse_marker("# !TPX002 TPX001 was fixed above").expect("marker");
        assert!(reasoned_code.silences(Rule::DocstringVolume));
        assert!(
            !reasoned_code.silences(Rule::CommentVolume),
            "a word in the reason silenced a second rule"
        );

        // ... but inside the comma list, an unrecognised token is a code attempt and must warn.
        let starred = parse_marker("# !TPX002,TPX0*").expect("marker");
        assert!(starred.silences(Rule::DocstringVolume));
        assert!(
            !starred.silences(Rule::CommentVolume) && !starred.silences(Rule::DuplicateProse),
            "a starred form set the blanket"
        );
        assert_eq!(
            starred.unknown_codes(),
            ["TPX0*"],
            "a starred form inside a comma list said nothing — EPIC.md Decisions #9 requires every \
             starred form to warn and suppress nothing"
        );

        // The blanket itself is untouched, in first position where it belongs.
        let blanket = parse_marker("# !TPX* the whole block is deliberate").expect("marker");
        for rule in Rule::ALL {
            assert!(blanket.silences(rule));
        }
        assert!(blanket.unknown_codes().is_empty());
    }

    /// A directive whose text is not a code must be **rejected**, in any script, without panicking.
    ///
    /// This closes a class, not an input. `parse_marker` byte-indexed `rest[..4]` after checking
    /// only `rest.len() >= 4`, and length is not a char boundary: a 3-byte lead character puts byte
    /// 4 *inside* it and the slice panics. Reproduced on the built binary —
    /// `# tooprolix: 忽略这个块` gave `end byte index 4 is not a char boundary` and **exit 101**,
    /// which is outside this crate's whole deliverable of 0/1/2, and which crosses the pyo3
    /// boundary as a `PanicException` that does not inherit `Exception`.
    ///
    /// **Why 129 tests missed it, which is the part worth keeping:** the panic needs a **3-byte**
    /// lead. 2-byte (Cyrillic, Latin-1) and 4-byte (most emoji) characters both land byte 4 on a
    /// boundary and pass. So CJK, Devanagari and most of the BMP above U+0800 crashed while
    /// `привет` and `🙂` did not — and every string in this module's tests was ASCII. The cases
    /// below therefore span all three widths deliberately: a fixture set that is "some non-ASCII"
    /// would have missed it too.
    ///
    /// **The cases drive the LIVE grammar, and that is a correction.** They were kept in their 0.1.0
    /// spelling for one revision, which meant every one of them exited at the missing `!` before
    /// reaching anything this parser does today — so re-introducing an unchecked index *after* the
    /// `!` left this test, the one named for the class, green. Each width now appears in both
    /// positions the current parser can index at: right after the `!`, and inside a token.
    #[test]
    fn a_non_ascii_directive_is_rejected_rather_than_panicking() {
        for line in [
            // Straight after the `!`, where the directive is read. 2-byte lead first — it never
            // panicked, and it is here so the set cannot silently narrow to the one width that did.
            "# !привет",
            "# !ы",
            // 3-byte lead — the width that panicked.
            "# !忽略这个块",
            "# !日本",
            "# !日",
            "# !नमस्ते",
            // 4-byte lead.
            "# !🙂 this block is deliberate",
            "# !🙂",
            // Inside a token that starts ASCII, which is where `is_code_shaped` does its indexing:
            // the byte after the upper-case run is the one that must not be sliced at.
            "# !AB日",
            "# !TPX日",
            "# !TPX00日",
            "# !TPXы",
            "# !TPX🙂",
            "# !TPX*日",
            // And the 0.1.0 spellings, which now fail one step earlier still.
            "# tooprolix: 忽略这个块",
            "#tooprolix:日",
            "# tooprolix: ы",
        ] {
            assert_eq!(
                parse_marker(line),
                None,
                "{line:?} is not a directive and must not be read as one"
            );
        }
    }

    /// A marker whose **reason** is not ASCII is still a marker.
    ///
    /// The pair to the test above: that one would pass on an implementation that gave up on every
    /// non-ASCII line, which would silently stop excluding legitimate markers written by a team
    /// that comments in another script. Here the code is ASCII and everything after it is not, in
    /// all three lead widths — 2-byte, 3-byte and 4-byte — because the width is what decided whether
    /// the 0.1.0 panic fired.
    #[test]
    fn a_marker_with_a_non_ascii_tail_is_still_a_marker() {
        let blanket = parse_marker("# !TPX* 這個區塊是故意的").expect("marker");
        let coded = parse_marker("# !TPX002 這是合約").expect("marker");
        let cyrillic = parse_marker("# !TPX002 таблица ниже — контракт").expect("marker");
        let emoji = parse_marker("# !TPX002 🙂 deliberate").expect("marker");

        for rule in Rule::ALL {
            assert!(
                blanket.silences(rule),
                "a non-ASCII reason unmade the blanket token: {rule:?}"
            );
        }
        for marker in [&coded, &cyrillic, &emoji] {
            assert!(marker.silences(Rule::DocstringVolume));
            assert!(
                !marker.silences(Rule::DuplicateProse),
                "a non-ASCII explanation after a code was read as a second code"
            );
            assert!(marker.unknown_codes().is_empty());
        }
        assert!(blanket.unknown_codes().is_empty());
    }

    /// A trailing marker on a line of code is still a marker for the block below it — the parser
    /// only ever sees whole lines, and this pins that it does not accidentally accept one.
    #[test]
    fn the_marker_must_own_its_line() {
        assert_eq!(parse_marker("value = 1  # !TPX001"), None);
    }

    /// **A marker has to NAME something, or ordinary English silences a block.**
    ///
    /// `# !important: never cache this response` is a comment a human writes without ever having
    /// heard of this tool. Reproduced on the committed 0.2.0 parser: it parsed as a marker naming an
    /// unrecognised code, `extract` dropped the line as a directive, the two-line remainder fell
    /// under the block gate, the `TPX001` finding vanished — and no unknown-code warning fired,
    /// because no block survived to carry one. Silent suppression by prose, reachable without ever
    /// writing `TPX*`.
    ///
    /// The rule that closes it: **after `# !` the directive must begin with a token of code shape**,
    /// `TPX*` or `TPX` followed by digits. The price is named rather than worked around — a mistyped
    /// code that is not code-shaped (`# !nonsense`) stops being reported at all, because the only
    /// alternative is to keep eating real prose.
    #[test]
    fn a_marker_must_name_a_code_shaped_token_or_it_is_not_a_marker() {
        for line in [
            // The measured defect: ordinary English that happens to open with `!`.
            "# !important: never cache this response",
            "# !!! do not reorder these two calls",
            "# !this one is on purpose",
            "# !001",
            // Named nothing at all: an empty directive, and one that is only separators.
            "# !",
            "# !   ",
            "# !,,,",
            "# !, ,\t,",
            // Aimed at a code and missed the SHAPE. These stay audible through `is_near_miss`,
            // which is asserted separately — here the only claim is that they silence nothing.
            // `# !tpx001` is NOT among them: the wrong case is still our namespace and still a
            // code, so it stays a marker and is reported as a code no rule answers to.
            "# !TPX",
            "# !TPX001TPX002",
            "# !TPX0*",
        ] {
            assert_eq!(
                parse_marker(line),
                None,
                "a directive that names no code was read as a marker: {line:?}"
            );
        }

        // The pair, or the assertions above pass on a parser that rejects everything.
        for line in ["# !TPX001", "# !TPX*", "# !TPX999", "# ! TPX001,TPX002"] {
            assert!(parse_marker(line).is_some(), "stopped parsing: {line:?}");
        }
    }

    /// **Only OUR namespace can make a line a marker.** Anything else is prose, and stays prose.
    ///
    /// The first version of the "name a code" rule tested `[A-Z]+[0-9]+` — the general shape used to
    /// *report* an unknown code — and that is not the same question. It accepts `HTTP2`, `UTF8`,
    /// `SHA256`, `RFC2119`, `ISO8601`, `TLS13`: ordinary technical prose. Reproduced on the binary:
    /// `# !HTTP2 is mandatory` above a comment run took the whole block out, silently, exactly as
    /// `# !important: never cache this response` had done one revision earlier. Same defect, same
    /// seam, smaller class.
    ///
    /// Both halves are asserted for every line. Not being a marker is what stops the block being
    /// eaten; not being a near-miss either is what stops the tool shouting about somebody's comment
    /// on TLS.
    #[test]
    fn only_our_own_namespace_can_make_a_line_a_marker() {
        for line in [
            "# !HTTP2 is mandatory",
            "# !UTF8 everywhere, no exceptions",
            "# !SHA256 only",
            "# !RFC2119 language throughout",
            "# !ISO8601 timestamps",
            "# !TLS13 required",
            "# !A1",
            "# !important: never cache this response",
            "# !nonsense",
        ] {
            assert_eq!(
                parse_marker(line),
                None,
                "prose in somebody else's namespace was read as a marker: {line:?}"
            );
            assert!(
                !super::is_near_miss(line),
                "prose in somebody else's namespace was reported as a near-miss: {line:?}"
            );
        }
    }

    /// A directive that aims at our namespace and misses is **reported**, never silent.
    ///
    /// `EPIC.md` Decisions #9 and #10 both say it in as many words: `TPX0*`, `TP*` and every other
    /// starred form warn and suppress zero rules. The measured gap was the ones that carry neither a
    /// digit nor a star after `TPX` — `# !TP*`, `# !TPX`, `# !TPX_001` were silent.
    ///
    /// The signal that separates these from the test above is **whether the token was aiming at our
    /// namespace at all**, and it is `starts_with`, never `contains`: `HTTP2` contains `tp` and must
    /// stay silent, while `TP*` begins with it and must not.
    #[test]
    fn a_directive_aimed_at_our_namespace_that_misses_is_reported() {
        for line in [
            "# !TP*",
            "# !TPX",
            "# !TPX_001",
            "# !TPXOO1",
            "# !TPX0*",
            "# !TPX00*",
            "# !tpx*",
            "# !TPX**",
            "# !TPX001TPX002",
            "# !tp",
        ] {
            assert!(
                super::is_near_miss(line),
                "a directive aimed at our namespace went unreported: {line:?}"
            );
            assert_eq!(
                parse_marker(line),
                None,
                "a token that is not a code was read as a marker: {line:?}"
            );
        }
    }

    /// The legacy clause matches the 0.1.0 **grammar**, not two words in the same sentence.
    ///
    /// `# tooprolix deliberately avoids noqa semantics` is a sentence about the tool, above a block,
    /// and it warned — because the clause asked whether the body contained both words anywhere.
    #[test]
    fn only_the_actual_0_1_0_spelling_counts_as_a_legacy_marker() {
        for line in [
            "# tooprolix: noqa",
            "#tooprolix:noqa",
            "# tooprolix : NOQA",
            "# tooprolix:  noqa  # deliberate",
        ] {
            assert!(super::is_near_miss(line), "went unreported: {line:?}");
        }

        for line in [
            "# tooprolix deliberately avoids noqa semantics",
            "# tooprolix no longer uses a noqa keyword",
            "# unlike noqa, tooprolix marks the block above",
            "# tooprolix is a linter",
            "# noqa: F401",
        ] {
            assert!(
                !super::is_near_miss(line),
                "prose about the tool was reported as a legacy marker: {line:?}"
            );
        }
    }

    /// A UTF-8 BOM must not decide whether a marker is a marker.
    ///
    /// A BOM is legitimate in Python — `CPython` runs such a file — and it lands on byte 0 of the
    /// first physical line, which is exactly where a module-level marker lives. `str::trim_start`
    /// does **not** remove U+FEFF (it is `Cf`, not whitespace), so the marker parser saw
    /// `\u{feff}# !TPX002` and said "not a comment", while `extract` — reading the tokenizer's
    /// output — had already excluded the same line as a directive. Two owners disagreeing about one
    /// line again, and the visible half was a marker that silently stopped working.
    #[test]
    fn a_byte_order_mark_does_not_unmake_a_marker() {
        let marker = parse_marker("\u{feff}# !TPX002").expect("a BOM'd marker is a marker");
        assert!(marker.silences(Rule::DocstringVolume));
        assert!(!marker.silences(Rule::DuplicateProse));

        assert!(parse_marker("\u{feff}# !TPX*").is_some_and(|m| m.silences(Rule::CommentVolume)));
        assert!(
            super::is_near_miss("\u{feff}# tooprolix: noqa TPX002"),
            "a BOM hid a near-miss as well"
        );
        // ... and a BOM does not turn a shebang into one, which is the pair to the rule above.
        assert_eq!(parse_marker("\u{feff}#!/usr/bin/env python"), None);
    }

    /// A comment that was aiming at a marker and missed is reported, and one that was not is not.
    ///
    /// This is the class the rename would otherwise have made *worse*: a typo in the code is loud,
    /// a typo in the directive was silent, and `!` is one character of redundancy where 0.1.0 had
    /// two words. Both directions are asserted, because a predicate that answers `true` to
    /// everything closes the class by drowning it.
    #[test]
    fn a_comment_that_was_aiming_at_a_marker_is_reported() {
        for line in [
            // Every 0.1.0 spelling, including the ones measured silent on the 0.1.0 binary.
            "# tooprolix: noqa TPX002",
            // The 0.1.0 BLANKET, which carries no code at all. A predicate keyed only on the code
            // shape cannot see it, and it is the one 0.1.0 spelling whose upgrade is doubly wrong:
            // it stops suppressing AND its own two words are counted as prose, which was measured
            // to push a 149-word run over the 150-word limit. Recognising the literal legacy
            // spelling is the migration aid Decisions #10 promised; it matches 0 lines of the
            // pinned corpus, which contains the word `tooprolix` nowhere.
            "# tooprolix: noqa",
            "#tooprolix:noqa",
            "# tooprolix : NOQA",
            "# toprolix: noqa TPX002",
            "# tooprolix noqa TPX002",
            "# tooprolix: noqua TPX002",
            "# noqa TPX002",
            // The `!` forgotten, and the space forgotten — including the star form, which needs the
            // fallback to accept a `*` after `TPX` and not only a digit.
            "# TPX002",
            "# TPX*",
            "#!TPX002",
            "#!tpx002",
            "#!TPX*",
            // Aimed at a code and missed its SHAPE. None of these is a marker any more, so the
            // near-miss clause is the only thing left that can report them.
            "# !TPX001TPX002",
            "# !TPX0*",
            "# !tpx*",
            "# !*TPX001",
            // A byte order mark is a legitimate first byte and must not hide any of it.
            "\u{feff}# tooprolix: noqa TPX002",
            "\u{feff}# TPX002",
        ] {
            assert!(super::is_near_miss(line), "went unreported: {line:?}");
        }

        for line in [
            // Working markers are not near-misses, whatever they go on to say about their codes:
            // `# !TPX999` already warns through `unknown_codes`, and warning twice is noise.
            "# !TPX002",
            "# !TPX*",
            "# !TPX999",
            // The wrong case is our namespace with a typo in it, so it stays a marker and warns
            // through `unknown_codes`. Warning twice about one line is noise.
            "# !tpx001",
            "\u{feff}# !TPX002",
            // **Prose that merely begins with `!`.** This is the half the predicate got wrong by
            // keying on the `!`: a comment nobody wrote as a directive must be neither a marker nor
            // a diagnostic, or every English sentence opening with an exclamation reports itself.
            "# !important: never cache this response",
            "# !!! do not reorder these two calls",
            "  # !nonsense",
            "# !",
            "# ! ",
            // A star that never names our namespace is not aiming at a code either. `# !TP*` is the
            // opposite case and lives in the reported list above: it begins with our namespace.
            "# !*",
            "# !HTTP2 is mandatory",
            // The shebang. 25 files in the pinned corpus open with one and 11 of those sit directly
            // above a module docstring — the difference between no diagnostic and 11 of them.
            "#!/usr/bin/env python",
            "#!/usr/bin/env python3",
            "#!/bin/sh",
            // Ordinary comments, other tools' pragmas, and prose that merely mentions the tool.
            "# an ordinary comment",
            "# tooprolix is a linter",
            "# type: ignore",
            "# noqa: F401",
            "# ruff: noqa",
            "value = 1",
            "",
        ] {
            assert!(!super::is_near_miss(line), "falsely reported: {line:?}");
        }
    }
}
