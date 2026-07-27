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
//!   `TPX001TPX002` matches no code and is reported as unknown. That is the loud direction: the
//!   marker suppresses nothing and the user is told which token was not understood;
//! * **trailing prose does not make a marker blanket.** ruff reads `# noqa this is fine` as a
//!   blanket suppression, because its marker sits *after code* where a trailing comment is the norm.
//!   Ours owns its whole line, so text after the `!` is far more likely to be a mistyped code than a
//!   remark — and reading it as blanket is the one mistake that can make a marker silence *more*
//!   than it says. Here the blanket form is one literal token, `TPX*`, and nothing else reaches it.
//!   See `collect_codes` (private) for the defect this replaced.
//!
//! **`TPX*` is a literal token and not a glob.** `TPX0*`, `TP*` and `*` are unrecognised codes: they
//! warn and silence nothing. A glob engine has no consumer, and every star form that is not the one
//! blanket token has to fail *closed* — the alternative is the defect below in a new costume.
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
/// grammar requires is missing, and `# !` on its own, which names nothing. Neither suppresses
/// anything, which is the closed direction; [`is_near_miss`] is what makes them audible.
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
    let after_hash = line.trim_start().strip_prefix('#')?;
    let directive = after_hash
        .strip_prefix(char::is_whitespace)?
        .trim_start()
        .strip_prefix('!')?
        .trim();

    // `# !` alone. It is deliberately NOT the blanket form — that one is named by a token of its
    // own — and it is not a marker either, so it cannot silently silence nothing: it falls through
    // to the near-miss diagnostic instead.
    if directive.is_empty() {
        return None;
    }

    Some(collect_codes(directive))
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
/// Two shapes count, and they are drawn where they are for measured reasons:
///
/// * the comment body starts with `!` **and** a space separates it from the `#` — the marker's own
///   shape. `#!/usr/bin/env python` is deliberately excluded, or 11 files in the pinned corpus
///   would each report their shebang;
/// * the body names something of the form `TPX<digits>` in any case — which is every 0.1.0 marker,
///   every misspelt keyword in front of a real code, and `#!TPX002` with the space forgotten.
///
/// It costs a false positive on a comment like `# TPX002 needs fixing` written directly above a
/// block. That is the accepted price: the diagnostic changes no exit code and silences nothing, so
/// the cost of the false positive is one line of stderr, while the cost of the silence it replaces
/// is a suppression the author believes is in place and is not.
///
/// ```
/// use tooprolix::rules::is_near_miss;
///
/// assert!(is_near_miss("# tooprolix: noqa TPX002"));  // the 0.1.0 marker
/// assert!(is_near_miss("# TPX002"));                  // the `!` forgotten
/// assert!(is_near_miss("#!TPX002"));                  // the space forgotten
/// assert!(!is_near_miss("# !TPX002"));                // ... this one works
/// assert!(!is_near_miss("#!/usr/bin/env python"));
/// assert!(!is_near_miss("# an ordinary comment"));
/// ```
#[must_use]
pub fn is_near_miss(line: &str) -> bool {
    if is_marker(line) {
        return false;
    }
    let Some(after_hash) = line.trim_start().strip_prefix('#') else {
        return false;
    };
    if after_hash
        .strip_prefix(char::is_whitespace)
        .is_some_and(|body| body.trim_start().starts_with('!'))
    {
        return true;
    }

    // `get`, not a slice: `to_ascii_lowercase` leaves every non-ASCII byte where it was, so the
    // byte after a match is not guaranteed to be a char boundary and must never be sliced at.
    let lowered = after_hash.to_ascii_lowercase();
    lowered.match_indices("tpx").any(|(at, _)| {
        lowered
            .as_bytes()
            .get(at + "tpx".len())
            .is_some_and(u8::is_ascii_digit)
    })
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
/// A **literal**, compared with `==`. It is not a glob and there is no glob: `TPX0*`, `TP*` and `*`
/// are unrecognised codes that warn and silence nothing. Widening this into a pattern language would
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
/// # Where the code list ends
///
/// At the first token that is neither [`BLANKET`] nor `[A-Z]+[0-9]+`. What happens to that token
/// depends on whether anything was read before it, and that is the *only* thing it depends on:
///
/// * **a code, or the blanket token, was already read** — the rest is the author's reason, and is
///   ignored in silence. `# !TPX001 because the table below is the contract` is legal, and ruff
///   stops in the same place for the same reason;
/// * **nothing was read yet** — the marker was trying to name something and failed, so the token is
///   recorded as unknown. The marker silences nothing and [`crate::cli`] says which token it was.
fn collect_codes(directive: &str) -> Suppression {
    let mut suppression = Suppression::default();

    for token in directive
        .split([',', ' ', '\t'])
        .filter(|token| !token.is_empty())
    {
        if token == BLANKET {
            suppression.all = true;
        } else if is_code_shaped(token) {
            match Rule::from_code(token) {
                Some(rule) => suppression.codes.push(rule),
                None => suppression.unknown.push(token.to_owned()),
            }
        } else {
            // `!all` belongs in this condition: without it `# !TPX* because the table is the
            // contract` would report `because` as a mistyped code.
            if !suppression.all && suppression.codes.is_empty() && suppression.unknown.is_empty() {
                suppression.unknown.push(token.to_owned());
            }
            break;
        }
    }

    suppression
}

/// `[A-Z]+[0-9]+` and nothing else — ruff's own shape for a rule code.
///
/// `bytes()`, not `chars()`, so `letters` is a byte offset **by construction** and the slice below
/// cannot land inside a character. It was `chars().take_while(char::is_ascii_uppercase)`, which was
/// also correct — an ASCII upper-case char is one byte, so the char count equalled the byte offset —
/// but correct by a two-step argument that a change of predicate would quietly break. `parse_marker`
/// above shipped a panic from exactly that kind of reasoning; this is the same class made structural
/// rather than argued. Verified before the change with `# !AB日`: no panic.
fn is_code_shaped(token: &str) -> bool {
    let letters = token.bytes().take_while(u8::is_ascii_uppercase).count();
    letters > 0
        && token.len() > letters
        && token.as_bytes()[letters..].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::{Rule, parse_marker};
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

    /// A squashed code list is reported as unknown rather than split, and it silences nothing.
    #[test]
    fn squashed_codes_are_not_silently_recovered() {
        let marker = parse_marker("# !TPX001TPX002").expect("marker");

        assert!(!marker.silences(Rule::CommentVolume));
        assert!(!marker.silences(Rule::DocstringVolume));
        assert_eq!(marker.unknown_codes(), ["TPX001TPX002"]);
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
    #[test]
    fn a_marker_that_names_something_unrecognised_never_widens_to_a_blanket() {
        for line in [
            "# !tpx001",
            "# !TPX",
            "# !TPX999",
            "# !TPX001TPX002",
            "# !this one is on purpose",
            "# !001",
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
            let marker = parse_marker(line).unwrap_or_else(|| panic!("not parsed: {line}"));

            for rule in Rule::ALL {
                assert!(
                    !marker.silences(rule),
                    "{line:?} silenced {rule:?} \u{2014} a mistyped marker must fail closed"
                );
            }
            assert!(
                !marker.unknown_codes().is_empty(),
                "{line:?} silenced nothing and said nothing about why"
            );
        }
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

    /// An explanation *after* a code that was recognised is prose, not a second code.
    ///
    /// This is why the two cases are told apart by what came before them rather than by what the
    /// token looks like: `because` after `TPX001` is a reason, and `because` on its own is a marker
    /// that failed to name anything.
    #[test]
    fn an_explanation_after_a_recognised_code_is_not_reported_as_a_typo() {
        let marker = parse_marker("# !TPX001 because the table is the contract").expect("marker");

        assert!(marker.silences(Rule::CommentVolume));
        assert!(!marker.silences(Rule::DuplicateProse));
        assert!(
            marker.unknown_codes().is_empty(),
            "an explanation was reported as a mistyped code: {:?}",
            marker.unknown_codes()
        );
    }

    /// A `# tooprolix:` line whose text is not `noqa` must be **rejected**, in any script.
    ///
    /// This closes a class, not an input. `parse_marker` byte-indexed `rest[..4]` after checking
    /// only `rest.len() >= 4`, and length is not a char boundary: a 3-byte lead character puts byte
    /// 4 *inside* it and the slice panics. Reproduced on the built binary —
    /// `# tooprolix: 忽略这个块` gave `end byte index 4 is not a char boundary` and **exit 101**,
    /// which is outside this task's whole deliverable of 0/1/2, and which crosses the pyo3
    /// boundary as a `PanicException` that does not inherit `Exception`.
    ///
    /// **Why 129 tests missed it, which is the part worth keeping:** the panic needs a **3-byte**
    /// lead. 2-byte (Cyrillic, Latin-1) and 4-byte (most emoji) characters both land byte 4 on a
    /// boundary and pass. So CJK, Devanagari and most of the BMP above U+0800 crashed while
    /// `привет` and `🙂` did not — and every string in this module's tests was ASCII. The cases
    /// below therefore span all three widths deliberately: a fixture set that is "some non-ASCII"
    /// would have missed it too.
    ///
    /// **These are 0.1.0 spellings and are kept unchanged on purpose.** The 0.2.0 grammar has no
    /// keyword to split, so they now fail one step earlier — at the missing `!` — and this test is a
    /// regression net rather than the live guard. The live one is the pair below it, which drives
    /// the same three character widths through the *current* parser.
    #[test]
    fn a_non_ascii_directive_is_rejected_rather_than_panicking() {
        for line in [
            // 2-byte lead — never panicked, here so the set cannot silently narrow to one width.
            "# tooprolix: привет",
            // 3-byte lead — the panic.
            "# tooprolix: 忽略这个块",
            "# tooprolix: 日本",
            "#tooprolix:日",
            "# tooprolix: नमस्ते",
            // 4-byte lead.
            "# tooprolix: 🙂 this block is deliberate",
            // Shorter than the keyword, in a script where that is fewer than four bytes.
            "# tooprolix: 日",
            "# tooprolix: ы",
        ] {
            assert_eq!(
                parse_marker(line),
                None,
                "{line:?} is not a directive and must not be read as one"
            );
        }
    }

    /// The keyword matches case-insensitively, and a non-ASCII **tail** still parses.
    ///
    /// The pair to the test above: it would pass on an implementation that gave up on every
    /// non-ASCII line, which would silently stop excluding legitimate markers written by a team
    /// that comments in another script.
    ///
    /// The three cases are the three states, and the middle one is not an accident of encoding:
    /// bare prose after the `!` silences nothing **in any script**, by the same rule that makes
    /// `# !this one is on purpose` silence nothing. A marker that means "everything" says `TPX*`.
    ///
    /// The unrecognised tokens span 2-, 3- and 4-byte lead characters, because that is the width
    /// that decided whether the 0.1.0 panic fired.
    #[test]
    fn a_marker_with_a_non_ascii_tail_is_still_a_marker() {
        let blanket = parse_marker("# !TPX* 這個區塊是故意的").expect("marker");
        let coded = parse_marker("# !TPX002 這是合約").expect("marker");
        let unnamed = parse_marker("# !這個區塊是故意的").expect("marker");

        for rule in Rule::ALL {
            assert!(
                blanket.silences(rule),
                "a non-ASCII reason unmade the blanket token: {rule:?}"
            );
            assert!(
                !unnamed.silences(rule),
                "bare prose widened a marker: {rule:?}"
            );
        }
        assert!(coded.silences(Rule::DocstringVolume));
        assert!(
            !coded.silences(Rule::DuplicateProse),
            "a non-ASCII explanation after a code was read as a second code"
        );
        assert!(coded.unknown_codes().is_empty());
        assert!(blanket.unknown_codes().is_empty());
        assert_eq!(unnamed.unknown_codes(), ["這個區塊是故意的"]);

        // All three widths through the live parser: 2-byte, 3-byte and 4-byte lead characters, each
        // an unrecognised code rather than a panic and each silencing nothing.
        for line in ["# !ы", "# !日", "# !🙂", "# !AB日", "# !नमस्ते"] {
            let marker = parse_marker(line).unwrap_or_else(|| panic!("not parsed: {line}"));
            for rule in Rule::ALL {
                assert!(!marker.silences(rule), "{line:?} silenced {rule:?}");
            }
            assert!(!marker.unknown_codes().is_empty(), "{line:?} said nothing");
        }
    }

    /// A trailing marker on a line of code is still a marker for the block below it — the parser
    /// only ever sees whole lines, and this pins that it does not accidentally accept one.
    #[test]
    fn the_marker_must_own_its_line() {
        assert_eq!(parse_marker("value = 1  # !TPX001"), None);
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
            "# toprolix: noqa TPX002",
            "# tooprolix noqa TPX002",
            "# tooprolix: noqua TPX002",
            "# noqa TPX002",
            // The `!` forgotten, and the space forgotten.
            "# TPX002",
            "#!TPX002",
            "#!tpx002",
            // The marker's own shape, naming nothing at all.
            "# !",
            "# ! ",
            "    # !",
        ] {
            assert!(super::is_near_miss(line), "went unreported: {line:?}");
        }

        for line in [
            // Working markers are not near-misses, whatever they go on to say about their codes:
            // `# !TPX999` already warns through `unknown_codes`, and warning twice is noise.
            "# !TPX002",
            "# !TPX*",
            "# !TPX999",
            "# !tpx002",
            "# !TPX0*",
            "# !TPX",
            "  # !nonsense",
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
