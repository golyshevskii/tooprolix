//! The finding as the user and the machine see it: an owned type, and the JSON schema is its shape.
//!
//! # Why the detectors' own types are not this type
//!
//! [`crate::detect::volume::Overrun`] and [`crate::detect::duplicate::Cluster`] **borrow**: their
//! fields are `&'a ProseBlock`. Two consequences, and both of them land here rather than there:
//!
//! * `#[pyclass]` requires `'static`, so neither can ever be handed to Python. The type that freezes
//!   into `tooprolix.check(path) -> list[Finding]` has to own its data, and it is this one;
//! * serialising a borrowed cluster inlines [`crate::extract::ProseBlock::raw`] once per member. A
//!   licence header shared by 2 000 files is one cluster with 2 000 members, which would be 2 000
//!   copies of the same paragraph in the JSON.
//!
//! The answer to the second is not a smarter representation of the prose — it is that **a finding
//! carries addresses and numbers and no prose at all**. Nothing downstream needs the text: the text
//! is in the file, at the address the finding names. That also makes the type trivially `'static`,
//! so the first answer comes for free.
//!
//! # Two shapes, deliberately not collapsed into one
//!
//! | shape | codes | what it carries |
//! |---|---|---|
//! | [`Detail::Duplicate`] | `TPX003` | every member's address, plus the weakest edge and its score |
//! | [`Detail::Volume`] | `TPX001`, `TPX002` | the measured size and the limit it passed |
//!
//! One struct with `locations`, `words` and `max_volume` all optional would be smaller to write and
//! would put a permanently-`null` field in every finding of both kinds. What the two shapes share
//! is the *contract* — a rule code, an address, deterministic order, an opt-out — not a
//! representation, so that is what is shared.
//!
//! Findings are **never** deduplicated across rule codes. One block can be both an overrun and a
//! member of a cluster, and both findings must reach the output; a naive `(path, line)` dedup would
//! silently eat one of them.
//!
//! # What is deliberately not in the schema
//!
//! * **No spanning tree.** `duplicates` merges edges in arrival order, so the minimum over a
//!   spanning tree is not the minimum over the component — measured on 5 of 6 corpus repositories,
//!   worst overstatement 0.059. [`Weakest`] is the real minimum and its two real ends.
//! # Two things a consumer will otherwise assume, and both are wrong
//!
//! * **A `TPX003` finding's top-level `prose_kind` is the ANCHOR member's kind, not the cluster's.**
//!   A cluster has no single kind — the same rationale copied from a docstring into a comment is one
//!   finding spanning both — and the top-level fields describe the address the finding is *reported
//!   at*, which is `locations[0]`. Nothing is lost: every entry in `locations` carries its own
//!   `prose_kind`. A consumer grouping by the top-level one is grouping by the anchor.
//! * **`path` is rendered with [`std::path::Path::display`], which is lossy.** A filename that is
//!   not valid UTF-8 reaches the JSON with replacement characters, so the string may not name a file
//!   that can be opened. Known limit, not fixed here: the honest alternatives are a `bytes` field
//!   nobody wants in JSON or refusing such files outright, and no corpus repository has one. If it
//!   ever matters, the fix belongs in this type, which is the single place a path becomes a string.
//!
//! * **No provenance flag on a cluster.** A cluster cannot say whether it came from identical text
//!   or from a near match, and `weakest_score == 1.0` does not answer it either: two blocks with
//!   different normalised texts can own the same shingle set and score exactly 1.0. Deriving a
//!   field from a number that does not determine it would be a lie in a versioned schema.

use std::fmt;

use serde::{Serialize, Serializer};

use crate::detect::duplicate::Cluster;
use crate::detect::volume::Overrun;
use crate::extract::{ProseBlock, ProseKind};
use crate::rules::Rule;

/// The version of the JSON document produced by `--format json`.
///
/// A string rather than a number so that `"1.1"` remains expressible without changing the type of
/// the field, which is the one change a consumer cannot absorb.
pub const SCHEMA_VERSION: &str = "1";

/// How many *other* addresses a duplicate finding prints before it summarises the rest.
///
/// One cluster can name every file in a repository — a licence header shared by 2 000 files is one
/// finding with 2 000 addresses — and 2 000 paths on one line is not a diagnostic, it is a denial
/// of service against the reader. The count is always exact and the JSON always carries every
/// address, so nothing is lost, only folded.
pub const MAX_RENDERED_LOCATIONS: usize = 10;

/// Where one block is.
///
/// `#[non_exhaustive]` like every other type in this schema: an address is the thing most likely to
/// grow a field — a column is the obvious next one — and this type appears in the JSON three times
/// over, so a struct literal in a consumer is the most expensive thing to break.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
pub struct Location {
    /// The file, exactly as the walk reached it, so a relative invocation reports relative paths.
    pub path: String,
    /// First physical line of the block, 1-based.
    pub line: usize,
    /// Last physical line of the block, 1-based and inclusive.
    pub end_line: usize,
    /// `comment` or `docstring`.
    ///
    /// Named the same in Rust and in JSON. It was `kind` here and `prose_kind` there, through a
    /// `#[serde(rename)]` — two names for one field on the most-serialised type in the schema, and
    /// the kind of split that outlives everyone who remembers why.
    #[serde(serialize_with = "serialize_kind")]
    pub prose_kind: ProseKind,
}

impl Location {
    /// The address of `block`, rendered from the path the walk used.
    #[must_use]
    pub fn of(block: &ProseBlock) -> Self {
        Self {
            path: block.path.display().to_string(),
            line: block.line_start,
            end_line: block.line_end,
            prose_kind: block.kind,
        }
    }
}

impl fmt::Display for Location {
    /// `path:line` — the form every editor and every CI annotation parser already understands.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.path, self.line)
    }
}

/// A [`ProseKind`] is its lower-case name in JSON.
///
/// Written here rather than as a `Serialize` derive on [`ProseKind`] itself: serialisation is this
/// module's contract, and `extract` has no reason to depend on serde.
///
/// By reference against `trivially_copy_pass_by_ref`, because serde's `serialize_with` fixes this
/// signature: it is `fn(&T, S)` and a by-value version does not type-check.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_kind<S: Serializer>(kind: &ProseKind, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(kind.as_str())
}

/// The weakest edge of a duplicate cluster: the pair that holds it together least well.
///
/// It is in the output because a connected component asserts a transitivity Jaccard does not have —
/// measured, 12 of 646 corpus components are not cliques — so without it a loose cluster and a set
/// of identical paragraphs look the same.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Weakest {
    /// One end of the weakest edge; always one of the finding's own locations.
    pub first: Location,
    /// The other end; always a *different* address from [`Self::first`].
    pub second: Location,
    /// The minimum Jaccard similarity over every edge of the cluster.
    pub similarity: f64,
}

/// The half of a finding that depends on which rule produced it.
///
/// `#[serde(untagged)]` with `#[serde(flatten)]` at the use site, so the fields sit beside `code`
/// and `path` in the JSON rather than under a wrapper key. The rule code already says which shape
/// it is; a second discriminator would be a second source of truth.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Detail {
    /// One block that is longer than its kind is allowed to be.
    ///
    /// `#[non_exhaustive]` on the VARIANT, not only on the enum. The attribute on an enum blocks
    /// new *variants*; a consumer destructuring `Detail::Volume { words, max_volume }` compiles
    /// today and breaks the day a field arrives. Both are needed, and this module's own header
    /// already names a field the schema is missing.
    #[non_exhaustive]
    Volume {
        /// The measured size, in normalised words.
        words: usize,
        /// The limit it passed, in normalised words.
        max_volume: usize,
    },
    /// Two or more blocks that say the same thing. `#[non_exhaustive]` per variant, as above.
    #[non_exhaustive]
    Duplicate {
        /// Every member of the cluster, in order, including the one the finding is addressed to.
        locations: Vec<Location>,
        /// The cluster's weakest edge.
        weakest: Weakest,
    },
}

/// One finding, owned, ordered and serialisable.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Finding {
    /// Which rule fired.
    pub code: Rule,
    /// Where the finding is addressed: for a cluster, its smallest member.
    #[serde(flatten)]
    pub at: Location,
    /// The one-line human rendering, so a JSON consumer does not have to rebuild the sentence.
    pub message: String,
    /// The rule-specific half.
    #[serde(flatten)]
    pub detail: Detail,
}

impl Finding {
    /// The total order every ordered output uses: address, then rule code, then the rendering.
    ///
    /// The code is a genuine tie-breaker rather than decoration: one block can be both an overrun
    /// and a cluster member, so two findings can share an address and nothing else separates them.
    ///
    /// # Why the message is in the key
    ///
    /// It was `(address, code)` alone, and `sort_by` is **stable**, so two findings equal on both
    /// kept whatever order they arrived in — the output would not have been a function of its input.
    /// That is the same defect this crate has now shipped three times in three different modules,
    /// and the resolution is the one task 4 arrived at: put the thing that distinguishes them into
    /// the key, rather than tie-breaking on a float or trusting the caller's order.
    ///
    /// The message is enough, and *totality* is the claim, so here is the argument rather than the
    /// assertion. Two findings that tie on all three are byte-identical in **both** output formats:
    ///
    /// * two [`Detail::Volume`] findings — the detail is `words` and `max_volume`, and the message
    ///   renders both, so an equal message implies an equal detail and identical JSON;
    /// * two [`Detail::Duplicate`] findings cannot tie on the address at all: clusters are disjoint
    ///   connected components, so no two of them share a smallest member. (That matters, because a
    ///   cluster message *folds* its addresses at [`MAX_RENDERED_LOCATIONS`] and so does not render
    ///   the whole detail — the fold is safe only because this case is unreachable);
    /// * one of each is separated by the code.
    ///
    /// No `f64` enters the ordering, which is what keeps `weakest.similarity` out of it.
    #[must_use]
    pub fn sort_key(&self) -> (&Location, Rule, &str) {
        (&self.at, self.code, &self.message)
    }

    /// Builds the finding for one volume [`Overrun`].
    #[must_use]
    pub fn from_overrun(overrun: &Overrun<'_>) -> Self {
        let at = Location::of(overrun.block);
        let code = Rule::volume_for(overrun.block.kind);
        // "is N words long, over the M-word limit" and NOT "is N words over a limit of M". The
        // second reads in English as "N words IN EXCESS OF M" — a reader sees 356 and 200 and
        // computes a 556-word docstring. The JSON fields are unambiguous; this string is what a
        // human acts on, and it was wrong about its own number.
        //
        // "on the line above it" is not padding. The remedy is useless without the *placement*, and
        // the placement is the non-obvious half for a docstring: the marker goes between `def` and
        // the literal, inside the body, not above the `def`. Naming the marker at all is a
        // requirement — without it the cheapest way to go green is to split one docstring into
        // three, which is the behaviour the rule exists to prevent.
        let message = format!(
            "{at}: {} {} is {} words long, over the {}-word limit \u{2014} shorten it, or mark \
             it with `# !{}` on the line above it",
            code.code(),
            overrun.block.kind.as_str(),
            overrun.words,
            overrun.max_volume,
            code.code(),
        );

        Self {
            code,
            at,
            message,
            detail: Detail::Volume {
                words: overrun.words,
                max_volume: overrun.max_volume,
            },
        }
    }

    /// Builds the finding for one duplicate [`Cluster`].
    ///
    /// The finding is addressed to the cluster's first member, which `duplicates` has already
    /// ordered, and the message names the others.
    ///
    /// # Panics
    ///
    /// If `cluster` has no members at all, which `duplicates` cannot produce: a component of fewer
    /// than two distinct addresses is discarded there by construction. The `expect` is left in
    /// rather than silently rendered as an empty address, so that a change to that invariant says
    /// which invariant moved.
    #[must_use]
    pub fn from_cluster(cluster: &Cluster<'_>) -> Self {
        let locations: Vec<Location> = cluster.members.iter().map(|m| Location::of(m)).collect();
        // `duplicates` never yields a cluster of fewer than two members, so both indices exist. An
        // `expect` rather than an `unwrap`: if that invariant ever moves, the panic says which one.
        let at = locations
            .first()
            .expect("a cluster has at least two members")
            .clone();

        let weakest = Weakest {
            first: Location::of(cluster.weakest.0),
            second: Location::of(cluster.weakest.1),
            similarity: cluster.weakest_score,
        };
        // The two ENDS of the weakest edge, not only its score. A connected component asserts a
        // transitivity Jaccard does not have — measured, 12 of 646 corpus components are not
        // cliques, the loosest 6 blocks held together by 6 of 15 possible edges — and the score
        // alone tells a reader that a cluster is loose without telling them *where*. On a cluster of
        // twenty addresses that is the difference between an actionable finding and a hint. It is
        // also the only reason `Cluster::weakest` carries two blocks instead of an `f64`, so a
        // rendering that dropped them threw the field's whole purpose away.
        let message = format!(
            "{at}: {} same explanation in {} places: {} (weakest {} ~ {}, similarity {:.3})",
            Rule::DuplicateProse.code(),
            locations.len(),
            render_others(&locations[1..]),
            weakest.first,
            weakest.second,
            weakest.similarity,
        );

        Self {
            code: Rule::DuplicateProse,
            at,
            message,
            detail: Detail::Duplicate { locations, weakest },
        }
    }
}

impl fmt::Display for Finding {
    /// The user-facing line, and the single owner of it.
    ///
    /// The detectors' own `Display` implementations are documented as diagnostics rather than as
    /// user output, and they stay that way: they carry no rule code, no opt-out advice and no unit
    /// on the duplicate side. This is the string `README.md` documents.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// The addresses after the first, folded at [`MAX_RENDERED_LOCATIONS`].
fn render_others(others: &[Location]) -> String {
    let shown: Vec<String> = others
        .iter()
        .take(MAX_RENDERED_LOCATIONS)
        .map(ToString::to_string)
        .collect();
    let hidden = others.len().saturating_sub(shown.len());

    if hidden == 0 {
        shown.join(", ")
    } else {
        format!("{} \u{2026} and {hidden} more", shown.join(", "))
    }
}

/// The whole `--format json` document: a version and the findings.
///
/// A named struct rather than an ad-hoc `serde_json::json!`, because the schema is meant to be a
/// Rust type that a change has to go through.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Report {
    /// [`SCHEMA_VERSION`].
    pub schema_version: &'static str,
    /// Every finding, in the order stdout prints them.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Wraps `findings` in the versioned envelope.
    #[must_use]
    pub fn new(findings: Vec<Finding>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            findings,
        }
    }

    /// The document, pretty-printed with a trailing newline.
    ///
    /// # Panics
    ///
    /// It cannot, and the reason recorded here used to be wrong. It said the only failure mode was
    /// a non-finite `f64`; **measured**, `serde_json::to_string(&f64::NAN)` returns `Ok("null")`
    /// rather than an error, so that was never a failure mode at all. What actually holds is
    /// stronger: every field of this type serialises through a derive over `String`, `usize`,
    /// `f64`, `Vec` and `&'static str`, none of which can fail, and there is no map with
    /// non-string keys anywhere in the schema. The `expect` documents an impossibility rather than
    /// handling a risk.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut rendered =
            serde_json::to_string_pretty(self).expect("a finding contains no non-finite float");
        rendered.push('\n');
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::{Finding, Location, MAX_RENDERED_LOCATIONS, Report};
    use crate::detect::duplicate::duplicates;
    use crate::detect::volume::{Limits, volume};
    use crate::extract::{ProseKind, extract};
    use crate::rules::Rule;
    use std::path::Path;

    fn blocks_of(path: &str, source: &str) -> Vec<crate::extract::ProseBlock> {
        extract(Path::new(path), source).expect("the fixture is valid Python")
    }

    /// The volume line must name the code, the size, the limit, the unit and the marker.
    ///
    /// Every one of those five is load-bearing: without the unit the number is meaningless, and
    /// without the marker the cheapest way to go green is to split one docstring into three — the
    /// exact behaviour the rule exists to prevent.
    #[test]
    fn a_volume_finding_names_the_unit_and_the_way_out() {
        let source = format!("\"\"\"Overview.\n{}\"\"\"\n", "word ".repeat(231));
        let blocks = blocks_of("api.py", &source);

        let report = volume(&blocks, Limits::default());
        let finding = Finding::from_overrun(&report.overruns[0]);

        assert_eq!(
            finding.to_string(),
            "api.py:1: TPX002 docstring is 232 words long, over the 200-word limit \u{2014} \
             shorten it, or mark it with `# !TPX002` on the line above it"
        );
        assert_eq!(finding.code, Rule::DocstringVolume);
        assert_eq!(finding.at.prose_kind, ProseKind::Docstring);
        assert_eq!(finding.at.end_line, 2);
    }

    /// A cluster line carries every address and the weakest edge, because a component asserts a
    /// transitivity Jaccard does not have.
    #[test]
    fn a_duplicate_finding_names_every_address_and_the_weakest_edge() {
        let left = "# The retry budget here is deliberately small, and that matters because\n\
                    # the upstream service rate limits us on every fourth call.\n";
        let reworded = "# The retry budget here is deliberately small, and that matters because\n\
                        # the upstream service rate limits us on every fourth request.\n";
        let mut blocks = blocks_of("client.py", left);
        blocks.extend(blocks_of("worker.py", reworded));

        let report = duplicates(&blocks);
        let finding = Finding::from_cluster(&report.clusters[0]);

        assert_eq!(
            finding.to_string(),
            "client.py:1: TPX003 same explanation in 2 places: worker.py:1 \
             (weakest client.py:1 ~ worker.py:1, similarity 0.900)"
        );
        assert_eq!(finding.code, Rule::DuplicateProse);
    }

    /// The weakest edge names **which pair** to look at, and that pair is not always the obvious one.
    ///
    /// Clustering asserts a transitivity Jaccard does not have — measured, 12 of 646 corpus
    /// components are not cliques, the loosest being 6 blocks held together by 6 of 15 possible
    /// edges. The score alone says a cluster is loose; only the endpoints say where. That is the
    /// whole reason `Cluster::weakest` carries two blocks instead of a number, so a rendering that
    /// drops them throws away the field's reason to exist.
    ///
    /// The fixture is built so the weakest edge is **not** the first two members: `a.py` and `b.py`
    /// are identical, and `c.py` is the reworded one, so the printed pair must involve `c.py`.
    #[test]
    fn the_text_output_names_the_two_ends_of_the_weakest_edge() {
        let original = "# The retry budget here is deliberately small, and that matters because\n\
                        # the upstream service rate limits us on every fourth call.\n";
        let reworded = "# The retry budget here is deliberately small, and that matters because\n\
                        # the upstream service rate limits us on every fourth request.\n";
        let mut blocks = blocks_of("a.py", original);
        blocks.extend(blocks_of("b.py", original));
        blocks.extend(blocks_of("c.py", reworded));

        let report = duplicates(&blocks);
        let finding = Finding::from_cluster(&report.clusters[0]);
        let rendered = finding.to_string();

        assert_eq!(
            rendered,
            "a.py:1: TPX003 same explanation in 3 places: b.py:1, c.py:1 \
             (weakest a.py:1 ~ c.py:1, similarity 0.900)"
        );
        assert!(
            rendered.contains("c.py:1,"),
            "the loose member is missing from the address list: {rendered}"
        );
    }

    /// A cluster that names every file in a repository must fold, and the count must stay exact.
    ///
    /// The number is the thing that cannot be wrong: a reader who sees "and 5 more" and counts six
    /// has been told the wrong size of the problem.
    #[test]
    fn a_very_large_cluster_folds_its_addresses_but_not_its_count() {
        let paragraph = "# One rationale, copied verbatim into every module of the project,\n\
                         # which is what a licence header or a policy note looks like.\n";
        let mut blocks = Vec::new();
        for index in 0..40 {
            blocks.extend(blocks_of(&format!("pkg/m{index:03}.py"), paragraph));
        }

        let report = duplicates(&blocks);
        let finding = Finding::from_cluster(&report.clusters[0]);

        assert!(
            finding
                .to_string()
                .contains("same explanation in 40 places"),
            "{finding}"
        );
        assert!(
            finding.to_string().contains("\u{2026} and 29 more"),
            "40 addresses were printed in full: {finding}"
        );
        assert_eq!(
            finding.to_string().matches(".py:").count(),
            MAX_RENDERED_LOCATIONS + 3,
            "the fold printed a different number of addresses than it claims: the anchor, \
             {MAX_RENDERED_LOCATIONS} others, and the two ends of the weakest edge — which are \
             never folded away, because they are the pair the reader is being sent to"
        );

        // ... and the machine-readable form loses nothing.
        let json = Report::new(vec![finding]).to_json();
        assert_eq!(json.matches("\"path\"").count(), 40 + 2 + 1);
    }

    /// The schema is a contract: a field that disappears or is renamed breaks every consumer, and
    /// the two shapes must stay two shapes.
    #[test]
    fn the_json_document_carries_its_version_and_both_finding_shapes() {
        let long = format!("\"\"\"Overview.\n{}\"\"\"\n", "word ".repeat(231));
        let long_blocks = blocks_of("api.py", &long);
        let overruns = volume(&long_blocks, Limits::default());

        let left = "# The retry budget here is deliberately small, and that matters because\n\
                    # the upstream service rate limits us on every fourth call.\n";
        let mut blocks = blocks_of("client.py", left);
        blocks.extend(blocks_of("worker.py", left));
        let clusters = duplicates(&blocks);

        let report = Report::new(vec![
            Finding::from_overrun(&overruns.overruns[0]),
            Finding::from_cluster(&clusters.clusters[0]),
        ]);
        let json = report.to_json();
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("the renderer emits valid JSON");

        assert_eq!(parsed["schema_version"], "1");
        let volume_finding = &parsed["findings"][0];
        assert_eq!(volume_finding["code"], "TPX002");
        assert_eq!(volume_finding["path"], "api.py");
        assert_eq!(volume_finding["line"], 1);
        assert_eq!(volume_finding["end_line"], 2);
        assert_eq!(volume_finding["prose_kind"], "docstring");
        assert_eq!(volume_finding["words"], 232);
        assert_eq!(volume_finding["max_volume"], 200);
        assert!(
            volume_finding.get("locations").is_none(),
            "a volume finding carries a dead cluster field"
        );

        let duplicate_finding = &parsed["findings"][1];
        assert_eq!(duplicate_finding["code"], "TPX003");
        assert_eq!(
            duplicate_finding["locations"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(duplicate_finding["weakest"]["similarity"], 1.0);
        assert_eq!(duplicate_finding["weakest"]["first"]["path"], "client.py");
        assert!(
            duplicate_finding.get("words").is_none(),
            "a duplicate finding carries a dead volume field"
        );
    }

    /// The sort key separates two findings that share one address, which is the case a
    /// coordinate-only key silently collapses.
    #[test]
    fn two_findings_at_one_address_are_ordered_by_their_rule_code() {
        let at = Location {
            path: "a.py".to_owned(),
            line: 1,
            end_line: 4,
            prose_kind: ProseKind::Comment,
        };
        let volume_finding = Finding {
            code: Rule::CommentVolume,
            at: at.clone(),
            message: String::new(),
            detail: super::Detail::Volume {
                words: 1,
                max_volume: 0,
            },
        };
        let duplicate_finding = Finding {
            code: Rule::DuplicateProse,
            at,
            message: String::new(),
            detail: super::Detail::Duplicate {
                locations: Vec::new(),
                weakest: super::Weakest {
                    first: volume_finding.at.clone(),
                    second: volume_finding.at.clone(),
                    similarity: 1.0,
                },
            },
        };

        assert!(volume_finding.sort_key() < duplicate_finding.sort_key());
    }

    /// The ordering key is **total** over everything two findings can differ in.
    ///
    /// It was `(address, code)`, so two findings equal on both compared equal and `sort_by` — which
    /// is stable — left them in arrival order. Nothing in the CLI can reach that today, because
    /// `extract` cannot emit two blocks with the same path, span and kind; `findings` is `pub` and
    /// this is the API freeze, so a caller can. It is also the third time this crate has shipped an
    /// ordering key with an untested half, and every previous time the fix was the same: put the
    /// thing that distinguishes them into the key, so no tie is ever broken by arrival.
    #[test]
    fn the_ordering_key_is_total_for_two_findings_at_one_address_under_one_code() {
        let at = Location {
            path: "a.py".to_owned(),
            line: 1,
            end_line: 4,
            prose_kind: ProseKind::Comment,
        };
        let smaller = Finding {
            code: Rule::CommentVolume,
            at: at.clone(),
            message: "a.py:1: TPX001 comment is 151 words long, over the 150-word limit".to_owned(),
            detail: super::Detail::Volume {
                words: 151,
                max_volume: 150,
            },
        };
        let larger = Finding {
            code: Rule::CommentVolume,
            at,
            message: "a.py:1: TPX001 comment is 900 words long, over the 150-word limit".to_owned(),
            detail: super::Detail::Volume {
                words: 900,
                max_volume: 150,
            },
        };

        assert_ne!(
            smaller.sort_key(),
            larger.sort_key(),
            "two findings that render differently compared equal, so their order is arrival order"
        );
        assert!(smaller.sort_key() < larger.sort_key());
    }
}
