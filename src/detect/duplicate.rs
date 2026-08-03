//! Duplicate prose detection: the same rationale written in several places.
//!
//! The finding is a **cluster** — a connected component of the similarity graph, whose edges are
//! "identical narrative" and "Jaccard `>=` [`SIMILARITY_THRESHOLD`]". One rationale copied
//! into a docstring, a CI-job comment and a state file is three places that rot independently, and
//! the one that gets updated is not the one you read; it is *one* problem with three addresses, and
//! a pair-shaped finding reported it three times. At corpus scale that is the difference between
//! 682 findings and 264 (`langgraph`), and on a licence header repeated across 2 000 files it is
//! the difference between 1 999 000 findings and one — measured, and the reason this shape exists.
//!
//! The pairs are never materialised, on either side of the scoring: candidates are streamed one
//! left block at a time rather than collected, and the edges go straight into a union-find as they
//! are scored. A group of `n` identical blocks costs `n - 1` unions rather than `C(n, 2)` of
//! anything, and `n` near-identical ones are still *compared* `C(n, 2)` times — that cost is
//! `validate-detectors-on-reference-corpus`'s to answer — but never *stored* that way.
//!
//! [`duplicates`] is the whole entry point; everything it consumes comes from
//! [`crate::extract`] and is not redefined here:
//!
//! * text is compared through [`crate::extract::ProseBlock::narrative`] and nothing else — the
//!   block's prose with its API-reference scaffolding discarded, still under
//!   [`crate::extract::normalize`], because [`SIMILARITY_THRESHOLD`] was measured under that
//!   normaliser. **Which text is compared is [`crate::extract`]'s decision, not this module's**, and
//!   the reason it is not the whole block is written out on [`crate::extract::narrative`]: a
//!   repeated parameter table is prose nobody can act on, and it does not separate from a genuine
//!   finding by similarity — 0.898 against 0.750, measured;
//! * a block whose narrative is **empty** takes no part in the rule. That is not a guard written
//!   here: an empty text has no shingle, so the index never proposes it as a candidate, and
//!   `exact_groups` skips it explicitly;
//! * [`duplicates`] applies [`crate::extract::ProseBlock::is_large_enough`] before both exact
//!   grouping and near-duplicate shingling. Those two dimensions count the **whole** block, so a
//!   block can pass the floor and still have no narrative left to compare;
//! * nothing here walks the filesystem, and nothing here is parallel.
//!
//! Two paths produce edges, for the reason spelled out on each: identical narrative is
//! connected arithmetically (`exact_groups`), with no Jaccard computed at all, and everything else
//! is paired through an inverted shingle index (`candidate_pairs`) and scored with `jaccard`.
//! Both feed the same `Components`, which is why an exact group and a near duplicate of it come out
//! as one cluster rather than two findings.
//!
//! Those are private, so they are named in plain backticks rather than as intra-doc links: a
//! link from public documentation to a private item does not resolve, and renders on docs.rs as an
//! inert code span pointing at nothing.
//!
//! **What this module does not own.** The rule code `TPX003`, the `# noqa` opt-out, exit codes, JSON
//! and the user-facing line format all belong to `build-cli-with-exit-contract-and-rule-codes`. The
//! `Display` impl here is a diagnostic fragment, not that line. Precision tuning belongs to
//! `validate-detectors-on-reference-corpus`, which owns the labelled corpus.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;

use crate::extract::{Coordinates, ProseBlock, write_address};

/// Width of a word shingle, in words.
///
/// `SHINGLE_K = 3` is `corpus/measure.py:90`, not a choice made here: [`SIMILARITY_THRESHOLD`]
/// was measured under this width, so changing one without re-measuring silently redefines the
/// other.
pub const SHINGLE_K: usize = 3;

/// Jaccard similarity at or above which a candidate pair is an edge of the similarity graph.
///
/// **Measured, not chosen** (`corpus/REPORT.md`): the knee is at 0.6 -> 0.75 (22 -> 8 candidates on
/// an audited repository), 0.9 finds nothing more there (8 -> 8) and costs 20.6% of the corpus
/// pool (1470 -> 1167). It is measured under [`SHINGLE_K`] and under
/// [`crate::extract::normalize`], which is why this module uses that normaliser and no other.
///
/// **It was measured over whole blocks, and it is applied to narratives** — deliberately, and this
/// is the honest statement of it. `exclude-reference-scaffolding-from-tpx003` changed *what* is
/// compared and was explicitly forbidden from re-calibrating *this*, on the grounds that a feature
/// defect and a constant defect are two decisions and mixing them makes neither measurable. Nothing
/// here claims 0.75 is still the knee on the new feature; re-measuring it is a separate, measured
/// decision that has not been taken.
pub const SIMILARITY_THRESHOLD: f64 = 0.75;

/// One finding: every block that says the same thing as at least one other block in the group.
///
/// Formally a connected component of size `>= 2` in the graph whose vertices are blocks and whose
/// edges are "identical narrative" or "Jaccard `>=` [`SIMILARITY_THRESHOLD`]".
///
/// # The transitivity this asserts, and why [`Self::weakest`] is not optional
///
/// Jaccard similarity is **not** transitive, and a connected component pretends it is: `A ~ B` and
/// `B ~ C` put `A` and `C` in one cluster even when `A !~ C`. That is a deliberate price — measured
/// over the six-repository corpus, **12 of 646 components (1.9%) are not cliques**, the loosest
/// being 6 blocks held together by 6 of the 15 possible edges. The price is paid openly rather than
/// silently: [`Self::weakest`] names the component's weakest link, so a loose cluster looks loose in
/// the output instead of looking like six identical paragraphs.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Cluster<'a> {
    /// Every block in the component, ordered by coordinate and then narrative, with no two
    /// members sharing both.
    ///
    /// Always at least two. Two blocks with the same coordinate *and* the same text are the same
    /// block seen twice — a caller that extracted one path twice — and are listed once; a component
    /// that collapses to a single address that way is not a finding at all.
    pub members: Vec<&'a ProseBlock>,
    /// The two ends of the component's weakest edge, ordered between themselves like
    /// [`Self::members`].
    ///
    /// Both are members, by construction rather than by convention: the ends are resolved against
    /// the deduplicated [`Self::members`], so this is always a pair of two *different* addresses
    /// drawn from the list above. Which pair it is, is decided by [`Self::weakest_score`] and — for
    /// a tie — by the same order [`Self::members`] uses, never by the order the edges were scored
    /// in.
    pub weakest: (&'a ProseBlock, &'a ProseBlock),
    /// The **minimum** Jaccard similarity over every edge of the component, in
    /// `[SIMILARITY_THRESHOLD, 1.0]`.
    ///
    /// A running minimum over all edges, not over the edges that happened to merge two components:
    /// union-find merges in arrival order, so the merging edge is arbitrary. Measured on the corpus,
    /// a minimum read off the spanning tree overstates the real one on 5 of 6 repositories (worst
    /// case `langgraph`, by 0.059) — i.e. it prints the cluster as tighter than it is, which is
    /// exactly the silent lie this field exists to prevent.
    ///
    /// **`1.0` does not mean the cluster came from an exact-text group.** Two blocks whose
    /// normalised texts differ can still own the same shingle set — eight repetitions of one word
    /// against nine — and they are scored on the near path and reach exactly `1.0`. Nothing in this
    /// type distinguishes the two paths, so a consumer must not infer provenance from the score.
    pub weakest_score: f64,
}

impl fmt::Display for Cluster<'_> {
    /// A one-line rendering, for tests and diagnostics.
    ///
    /// **`build-cli-with-exit-contract-and-rule-codes` owns the user-facing line** — the rule code
    /// (`TPX003` — this rule moved off `TPX001`, which is now comment volume), the column, the JSON
    /// shape and the `# noqa` opt-out all live there. This is every
    /// address, in order, plus the weakest link, which is what makes it usable as the byte-for-byte
    /// determinism probe the AC4 tests compare.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, member) in self.members.iter().enumerate() {
            if position > 0 {
                write!(formatter, ", ")?;
            }
            write_address(
                formatter,
                &member.path.display(),
                member.line_start,
                member.line_end,
            )?;
        }
        write!(formatter, ": duplicate prose (weakest ")?;
        write_address(
            formatter,
            &self.weakest.0.path.display(),
            self.weakest.0.line_start,
            self.weakest.0.line_end,
        )?;
        write!(formatter, " ~ ")?;
        write_address(
            formatter,
            &self.weakest.1.path.display(),
            self.weakest.1.line_start,
            self.weakest.1.line_end,
        )?;
        write!(formatter, ", similarity {:.3})", self.weakest_score)
    }
}

/// Everything one duplicate scan produced.
///
/// `Clone` and `PartialEq` for the same reason as [`crate::detect::volume::Report`]'s:
/// `build-cli-with-exit-contract-and-rule-codes` needs to `assert_eq!` two reports in a test, and
/// both derives are free today and non-breaking to add later. **No `Eq` and no `Hash`** — not a
/// style choice: [`Cluster::weakest_score`] is an `f64`, which is `PartialEq` but not `Eq`, so
/// neither could be derived here anyway. No `serde` either; that is the next task's decision.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Report<'a> {
    /// The findings, sorted by their smallest member's coordinate and then narrative, and
    /// byte-identical for any arrival order of the same blocks. The private `cluster_key` records
    /// why the text belongs in that order rather than the coordinate alone.
    ///
    /// **An exact-text group of `n` blocks is one entry with `n` members, never `C(n, 2)` entries.**
    /// That is the whole point of the shape: measured, 2 000 files sharing one three-line licence
    /// header used to give 1 999 000 findings and 118 MB of rendered output, and give one cluster
    /// here. `build-cli-with-exit-contract-and-rule-codes` owns how that is presented, and must not
    /// assume a cluster is small — one of them can name every file in the repository.
    pub clusters: Vec<Cluster<'a>>,
    /// How many pairs a Jaccard score was actually computed for.
    ///
    /// Exact-text groups are **not** counted here: their score is 1.0 by definition and no
    /// comparison happens. This is the instrument the growth-law test measures — the number is what
    /// distinguishes an inverted index from full enumeration, which would report exactly
    /// `n(n-1)/2` — and it is what `validate-detectors-on-reference-corpus` will need when it has a
    /// wall-clock budget to defend.
    pub comparisons: usize,
}

/// One shingle: [`SHINGLE_K`] consecutive normalised words.
type Shingle<'a> = [&'a str; SHINGLE_K];

/// Word shingles of `normalized`: every [`SHINGLE_K`] consecutive words, **sorted and without
/// duplicates**.
///
/// Duplicate-free, so a phrase repeated inside one block does not count twice — the `frozenset` of
/// `corpus/measure.py:305-310`, ported rather than reinvented. It is a sorted `Vec` rather than a
/// `HashSet`, which is a representation change and **not** a semantic one: the value is still the
/// set of grams, and [`jaccard`] still computes `|A n B| / |A u B|` over exactly that set.
///
/// # Why sorted, and why this is not the anti-pattern it resembles
///
/// The usual advice is to reach for a `HashSet` over a `Vec` for membership, and it is right when
/// the question is "is this one element present". That is not the question here. [`jaccard`] needs
/// the size of a **full intersection** of two gram sets, and the pairs are scored `C(n, 2)` times
/// on the input this detector is slowest on. A sorted merge answers that in one linear pass over
/// two contiguous allocations; a `HashSet` answers it as `min(|A|, |B|)` hash lookups scattered
/// across a table. The `Vec` is never scanned linearly *for a member* — there is no `contains` in
/// the hot path — so the cost model the advice is about does not apply.
///
/// **Measured, on the case that owns the wall-clock budget** (2 000 files whose prose header
/// differs in one token, 100 000 lines — `tests/adversarial_bench.rs`, whose module documentation
/// carries the full table these figures are read off): the detect stage went from **6.12–6.25 s to
/// 2.46–2.71 s** and the whole run from **6.26–6.38 s to 2.59–2.87 s**, taking it from *over* the
/// `< 5 s / 100 000 lines` budget to inside it. Ranges rather than points because the same machine
/// reproduces a median only to about 6%.
///
/// On the six pinned corpus checkouts, where the candidate index already removes ~97% of the
/// pairs, the same change measures **neutral** end-to-end (**0.96x–1.12x**, five runs each): the
/// pairs it makes cheaper are pairs those repositories barely score. Output is byte-identical on
/// all six — 823 findings, 1 008 216 bytes, `diff` clean — which is the gate this change had to
/// pass, and `comparisons` is unchanged at every size, which is how "the same pairs are still
/// scored" is known rather than hoped.
///
/// Sorting is `O(w log w)` **once per block** against `C(n, 2)` scorings, so it is bought back
/// immediately. A hash of the gram was rejected: it would make the comparison cheaper still and
/// introduce a collision probability, i.e. a chance of scoring two different grams as equal, which
/// is the same class of "probably right" this module already refuses in `for_each_candidate_pair`.
/// Comparing the words themselves keeps the result exact.
///
/// **One branch of `measure.py` is deliberately not ported.** It yields a single short gram for a
/// text of fewer than [`SHINGLE_K`] words; here such a text yields an *empty* set, which
/// [`jaccard`] scores 0.0. Where it would matter the two definitions agree anyway: identical short
/// texts land in the exact-text groups and score 1.0 arithmetically, and two *different* short texts
/// share no gram under either definition. Empty is also the safe side: it can only withhold a
/// finding, never invent one.
///
/// **The empty set is REACHABLE, and it is tempting to document it as impossible.** The reasoning
/// that fails is "[`crate::extract::MIN_BLOCK_WORDS`] is 8, so no extracted block can be shorter
/// than a shingle": true of the *whole* block, which is what that constant counts, and false of the
/// [`crate::extract::ProseBlock::narrative`] this function is handed. A docstring that is nothing
/// but a parameter table is 8 words or more and has an empty narrative, so it lands here and gets
/// an empty set, which is exactly how it takes no part in the rule.
fn shingles(normalized: &str) -> Vec<Shingle<'_>> {
    let words: Vec<&str> = normalized.split_whitespace().collect();
    // `collect` sizes itself exactly here, unlike the `HashSet` this replaced: `Windows` is an
    // `ExactSizeIterator`, so there is one allocation of the right length and no growth.
    let mut grams: Vec<Shingle<'_>> = words
        .windows(SHINGLE_K)
        // `from_fn` rather than `[window[0], window[1], window[2]]`: the literal spelled the
        // arity a second time, right next to the constant that owns it, so `SHINGLE_K` could
        // not be remeasured without editing two places. `windows(SHINGLE_K)` yields slices of
        // exactly that length, so every index here is in bounds by construction.
        .map(|window| std::array::from_fn(|index| window[index]))
        .collect();
    // Unstable is free here: the elements are compared by value and equal grams are
    // indistinguishable, so no order between them can be observed. `dedup` needs the sort anyway,
    // and the sort is what `jaccard` relies on.
    grams.sort_unstable();
    grams.dedup();
    grams
}

/// `|A n B| / |A u B|`, and `0.0` if either side is empty — `corpus/measure.py:313-318`.
///
/// Both sides must be sorted and duplicate-free, which is [`shingles`]'s postcondition and the
/// only way this function is ever called. The intersection is a merge of the two, so the whole
/// score costs one pass of `|A| + |B|` comparisons over contiguous memory with no hashing at all.
#[allow(
    clippy::cast_precision_loss,
    reason = "shingle counts are far below 2^53, where f64 stops being exact on integers"
)]
fn jaccard(left: &[Shingle<'_>], right: &[Shingle<'_>]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let mut left_grams = left.iter().peekable();
    let mut right_grams = right.iter().peekable();
    let mut intersection = 0usize;
    // Iterators rather than indices: no bounds check per step, and the two cursors cannot be
    // advanced out of step by an editing slip the way two `usize`s can.
    while let (Some(&&next_left), Some(&&next_right)) = (left_grams.peek(), right_grams.peek()) {
        match next_left.cmp(&next_right) {
            Ordering::Less => {
                left_grams.next();
            }
            Ordering::Greater => {
                right_grams.next();
            }
            Ordering::Equal => {
                intersection += 1;
                left_grams.next();
                right_grams.next();
            }
        }
    }
    // Inclusion-exclusion, exactly as before: both sides are duplicate-free, so this is the size
    // of the union without materialising it.
    let union = left.len() + right.len() - intersection;
    intersection as f64 / union as f64
}

/// Identity of one end of a finding: where the block is, and what it says.
///
/// The coordinate half comes from [`ProseBlock::coordinates`], which is its **single owner** — this
/// module used to re-spell the tuple, and the review proved the two spellings could drift with every
/// test green.
///
/// The text half is what makes the whole key total, and it is the answer to "when are two blocks the
/// same block?" — see [`cluster_key`].
type End<'a> = (Coordinates<'a>, &'a str);

/// Where a block is and what it says.
fn end_of(block: &ProseBlock) -> End<'_> {
    (block.coordinates(), block.narrative.as_str())
}

/// The output order of the clusters: the [`End`] of the smallest member.
///
/// # Why the text is in the key
///
/// It answers a question the first version of this module answered silently and wrongly: *two
/// blocks are the same block when they share a coordinate and a text.* Keying on coordinates alone
/// made a member a *coordinate*, and that dropped real findings — measured, with two blocks handed
/// the same `path` label and different sources, one of the two vanished from the output.
///
/// # Why this makes the order total, without comparing floats
///
/// Two clusters cannot share a member: components partition the blocks. And two blocks with the same
/// [`End`] have identical narrative, so they are exact twins, so they are in the *same*
/// component. Therefore no two clusters have the same smallest member, and this key is a total
/// order over the output — without any `f64` entering it.
///
/// The minimum is taken over the members rather than read off `members[0]`, which is the same value
/// by construction. That is deliberate: it keeps the cluster order independent of the member order,
/// so a defect in one of the two cannot be masked by the tests covering the other.
fn cluster_key<'a>(cluster: &Cluster<'a>) -> Option<End<'a>> {
    cluster.members.iter().map(|member| end_of(member)).min()
}

/// One edge of the similarity graph: its score, and the positions of its two ends, ordered by
/// [`end_of`] so that the pair does not depend on which end arrived first.
type Edge = (f64, usize, usize);

/// The weaker of two edges: the lower score, ties broken by the ends' [`End`].
///
/// The tie-break is not cosmetic. Edges arrive in the iteration order of a `HashSet`, so "the first
/// edge with the minimum score" is not a function of the input; two exact twins and a third block
/// can easily give a component whose every edge scores exactly 1.0, and then the tie-break is the
/// only thing deciding which pair the output names.
///
/// `total_cmp`, so the three arms are the whole domain and no unreachable `None` needs justifying.
/// It agrees with `partial_cmp` on every score reachable here: `jaccard` is a ratio of two positive
/// counts and `exact_groups` passes a literal `1.0`, so neither `NaN` nor `-0.0` can arrive.
fn weaker(left: Edge, right: Edge, blocks: &[ProseBlock]) -> Edge {
    match left.0.total_cmp(&right.0) {
        Ordering::Less => left,
        Ordering::Greater => right,
        Ordering::Equal => {
            let ends = |edge: Edge| (end_of(&blocks[edge.1]), end_of(&blocks[edge.2]));
            if ends(left) <= ends(right) {
                left
            } else {
                right
            }
        }
    }
}

/// The connected components of the similarity graph, built edge by edge.
///
/// A union-find, and **the** reason this module never allocates a pair: an exact group of `n`
/// identical blocks reaches it as `n - 1` calls to [`Self::connect`], and a near-duplicate family of
/// `n` as however many candidate pairs cleared the threshold — and this structure is three vectors
/// of length `n` either way, because an edge is folded in and dropped rather than kept. A list of
/// findings was `O(n^2)`: measured before the change, 2 000 files sharing a licence header allocated
/// 1 999 000 findings at 92.5 MB of resident memory, and 5 000 files 445 MB.
///
/// That bound is only the whole story because `for_each_candidate_pair` streams: it used to hand
/// this loop a `HashSet` of every candidate pair, so the peak was quadratic one function earlier,
/// where nothing here could see it.
struct Components {
    /// Union-find parent links, with path halving in [`Self::find`].
    parent: Vec<usize>,
    /// Members per root, for union by size. Valid at roots; `1` for every block that never got an
    /// edge, which is what makes a singleton cheap to skip.
    size: Vec<usize>,
    /// The weakest edge seen anywhere in the component, valid at roots.
    ///
    /// `None` only for a block no edge ever touched.
    weakest: Vec<Option<Edge>>,
}

impl Components {
    /// One component per block, with no edges yet.
    fn new(blocks: usize) -> Self {
        Self {
            parent: (0..blocks).collect(),
            size: vec![1; blocks],
            weakest: vec![None; blocks],
        }
    }

    /// The root of `position`'s component, halving the path on the way up.
    fn find(&mut self, mut position: usize) -> usize {
        while self.parent[position] != position {
            self.parent[position] = self.parent[self.parent[position]];
            position = self.parent[position];
        }
        position
    }

    /// Records one edge: merges the two ends' components, and folds the edge into the component's
    /// running minimum.
    ///
    /// **The `first == second` branch is the whole correctness of [`Cluster::weakest_score`]**, and
    /// it is the one a naive implementation drops. An edge whose ends are already in one component
    /// merges nothing — but it is still an edge of that component, and it is very often the weakest
    /// one, because the edges that *did* merge are simply whichever arrived first. Skipping the
    /// update here is exactly "read the minimum off the spanning tree", which the corpus measurement
    /// showed overstates the real minimum on 5 of 6 repositories.
    ///
    /// # An edge between two copies of one address is not an edge
    ///
    /// When both ends share an [`End`] they are the same block handed in twice — a walker that
    /// yielded one path twice. It still has to **merge**, or the copy would surface as a second
    /// cluster; but it must never be a candidate for the weakest link, because
    /// [`Cluster::members`] deduplicates by [`End`] and the pair would name one address against
    /// itself. `None` says that in the type rather than in a comment: it flows into the same fold
    /// below and simply is not there.
    fn connect(&mut self, left: usize, right: usize, score: f64, blocks: &[ProseBlock]) {
        let edge = match end_of(&blocks[left]).cmp(&end_of(&blocks[right])) {
            Ordering::Less => Some((score, left, right)),
            Ordering::Greater => Some((score, right, left)),
            Ordering::Equal => None,
        };
        let (mut first, mut second) = (self.find(left), self.find(right));

        if first == second {
            if let Some(edge) = edge {
                self.weakest[first] = Some(match self.weakest[first] {
                    Some(current) => weaker(current, edge, blocks),
                    None => edge,
                });
            }
            return;
        }

        // Union by size, so `find` stays near-constant on the 2 000-member components a licence
        // header produces. Which of the two roots survives is invisible to the output: the minimum
        // below is folded from both sides, and the members are collected by root afterwards.
        if self.size[first] < self.size[second] {
            std::mem::swap(&mut first, &mut second);
        }
        let merged = [self.weakest[first], self.weakest[second], edge]
            .into_iter()
            .flatten()
            .reduce(|carried, candidate| weaker(carried, candidate, blocks));
        self.parent[second] = first;
        self.size[first] += self.size[second];
        self.weakest[first] = merged;
    }

    /// Every component with at least two distinct members, as sorted [`Cluster`]s.
    ///
    /// # Panics
    ///
    /// Never in practice, and both `expect`s state an invariant rather than hiding a broken one
    /// behind an empty result. A component reaches two *distinct* members only along a path of
    /// edges, and at least one edge on that path joins two different addresses — [`Self::connect`]
    /// records exactly those — so the weakest edge exists and both of its ends survive the dedup
    /// below.
    fn into_clusters(mut self, blocks: &[ProseBlock]) -> Vec<Cluster<'_>> {
        let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
        for position in 0..blocks.len() {
            let root = self.find(position);
            if self.size[root] > 1 {
                by_root.entry(root).or_default().push(position);
            }
        }

        let mut clusters = Vec::with_capacity(by_root.len());
        for (root, positions) in by_root {
            let mut members: Vec<&ProseBlock> = positions
                .iter()
                .map(|&position| &blocks[position])
                .collect();
            members.sort_by(|left, right| {
                end_of(left)
                    .cmp(&end_of(right))
                    // Blocks with equal `End` are one address seen twice and only one of them
                    // survives the dedup below. `raw` decides which, so that the surviving block —
                    // and everything reachable through it — is a function of the input. `sort_by` is
                    // stable, so without this the survivor was whichever arrived first, and
                    // permuting the input changed the `raw` a consumer can read while every
                    // rendered byte stayed identical.
                    .then_with(|| left.raw.cmp(&right.raw))
            });
            // The same block handed in twice — a walker that yielded one path twice — is one
            // address, not a duplicate of itself. Keying this on the coordinate alone would instead
            // discard a genuine second block that happens to share a span, which is the defect the
            // text half of `End` exists to prevent.
            members.dedup_by(|left, right| end_of(left) == end_of(right));
            if members.len() < 2 {
                continue;
            }
            let (score, first, second) = self.weakest[root]
                .expect("a component with two members was formed by an edge, which records itself");
            // Resolve the edge's ends against the DEDUPLICATED members, so "both ends are members"
            // is true by construction. Holding the raw positions instead let the weakest link point
            // at a block the dedup had just discarded — a different object, with a different `raw`,
            // that no longer appears in the finding it is part of.
            let member_at = |position: usize| {
                let target = end_of(&blocks[position]);
                // A linear scan, not a binary search over the sorted list: it is two scans per
                // cluster, and a binary search would silently make this depend on the member
                // ordering above — two guards sharing one failure, which is how a defect in the
                // ordering hides behind a panic here.
                *members
                    .iter()
                    .find(|member| end_of(member) == target)
                    .expect("the weakest edge joins two addresses, and both are members")
            };
            let weakest = (member_at(first), member_at(second));
            clusters.push(Cluster {
                members,
                weakest,
                weakest_score: score,
            });
        }

        // Sort before output — ruff does the same at `crates/ruff/src/commands/check.rs:182`, right
        // after its parallel fan-out. Here the source of disorder is not threads but the hash
        // containers above: `HashMap`/`HashSet` iteration order differs between two instances inside
        // one process, so both the edges and the roots arrive shuffled, and the sort is the only
        // thing that makes the output a function of the input rather than of the allocator.
        clusters.sort_by(|left, right| cluster_key(left).cmp(&cluster_key(right)));
        clusters
    }
}

/// Every group of blocks that says the same thing, sorted.
///
/// `blocks` is whatever [`crate::extract::extract`] produced, for one file or for a whole
/// repository. The [`crate::extract::MIN_BLOCK_LINES`] `AND`
/// [`crate::extract::MIN_BLOCK_WORDS`] conjunction is applied before both exact grouping and
/// near-duplicate shingling, so one-line and too-small blocks allocate no shingles and do not grow
/// [`Report::comparisons`].
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use tooprolix::detect::duplicate::duplicates;
/// use tooprolix::extract::extract;
///
/// // Keep each Python source on ONE line: rustdoc strips a leading `# ` as a hidden line, which
/// // would silently turn a Python comment into a Python statement.
/// let left = "# The retry budget here is small, and that matters:\n# the upstream throttles us.\n";
/// let right = "#   The retry budget here is small,\n#   and that matters: the\n#   upstream throttles us.\n";
///
/// let mut blocks = extract(Path::new("client.py"), left)?;
/// blocks.extend(extract(Path::new("server.py"), right)?);
///
/// let report = duplicates(&blocks);
/// assert_eq!(report.clusters.len(), 1);
/// let members = &report.clusters[0].members;
/// assert_eq!(members.len(), 2);
/// assert_eq!(members[0].path, Path::new("client.py"));
/// # Ok::<(), tooprolix::Error>(())
/// ```
#[must_use]
pub fn duplicates(blocks: &[ProseBlock]) -> Report<'_> {
    let sets: Vec<Vec<Shingle<'_>>> = blocks
        .iter()
        .map(|block| {
            if block.is_large_enough() {
                shingles(&block.narrative)
            } else {
                Vec::new()
            }
        })
        .collect();

    let mut components = Components::new(blocks.len());
    let group_of = exact_groups(blocks, &mut components);

    let mut comparisons = 0;
    for_each_candidate_pair(&sets, &group_of, |left, right| {
        comparisons += 1;
        let score = jaccard(&sets[left], &sets[right]);
        if score >= SIMILARITY_THRESHOLD {
            components.connect(left, right, score, blocks);
        }
    });

    Report {
        clusters: components.into_clusters(blocks),
        comparisons,
    }
}

/// Whether `TPX003` compares this block at all: its narrative must be able to carry one shingle.
///
/// # The decision this records, and the measurement behind it
///
/// Once [`crate::extract::narrative`] discards the reference scaffolding, some blocks have little or
/// nothing left. `exclude-reference-scaffolding-from-tpx003` had to choose a floor, and the choice
/// was measured on the six pinned checkouts rather than argued:
///
/// **This section previously carried a comparison table that does not reproduce, and it is
/// removed rather than patched.** It claimed an eight-word floor yields corpus `TPX003` **543**
/// against non-empty's **617**. Re-measured by the supervisor 2026-07-30 against *this* code, by
/// changing the constant below to [`crate::extract::MIN_BLOCK_WORDS`], rebuilding, and running
/// `corpus/run_all.sh` over all six pinned checkouts: **617 either way — the floor changes nothing
/// on the corpus.** The 543 was measured when the floor sat somewhere else in the pipeline and did
/// not survive the move into this function; it was reported as current and is not.
///
/// **What is measured about this narrative floor, stated at the size it actually is.** It gates the
/// **exact** path only (this function has one caller, `exact_groups`). After the shared whole-block
/// floor, the **near** path needs no separate narrative check: fewer than [`SHINGLE_K`] narrative
/// words yield no shingle, so the inverted index never proposes the block. The two populations
/// therefore coincide *at* [`SHINGLE_K`] by construction — which is why raising this constant alone
/// moves nothing, and why raising it would have to be done in both places to mean anything.
///
/// The floor is [`SHINGLE_K`]: the smallest value that means anything at all, and the value at which
/// the two paths already agree. A larger floor is a **recall** decision that this task's Out-of-scope
/// forbids taking on a hunch, and the measurement that would justify one does not exist yet.
///
/// # Why [`SHINGLE_K`] and not "non-empty"
///
/// Non-empty leaves the two edge paths disagreeing about the population, and a measured false
/// positive follows. The **near** path already refuses a narrative of fewer than
/// [`SHINGLE_K`] words: such a text has no shingle, so the inverted index never proposes it and
/// `jaccard` never sees it. The **exact** path had no floor, so a one-word residue could only ever
/// be *matched*, never *scored* — and no threshold could reach it. Two unrelated callables both
/// summarised `"""Send.` above different `Args:` tables came out as one finding at similarity 1.000.
///
/// **This function has ONE caller — `exact_groups`.** It is NOT the one place the population is
/// decided: `grep -n 'is_compared('` returns the definition and that single call site. The near
/// path reaches the same population by a different mechanism (no shingle below [`SHINGLE_K`]
/// words), so the *effect* matches while the ownership does not. Anyone raising this floor must
/// change both places; this one will not do it alone.
///
/// **Known residual.**
/// Two unrelated callables whose narratives are the *same* templated summary of at least
/// [`SHINGLE_K`] words still form a finding at similarity 1.000 — measured on a constructed pair
/// sharing `"""Sends the prepared request now.` above different `Args:` tables. This is the
/// templated-summary class that also leaves annotated clusters #13 and #20 alive at 0.800
/// (`corpus/annotations.md` §1.5), and §1.5 records the measurement that rules out closing it with
/// the threshold: a genuine finding sits at exactly 0.800 too. Closing it needs a rule about
/// templated summary lines, which is a judgement and not this task's grammar.
fn is_compared(block: &ProseBlock) -> bool {
    block.narrative.split_whitespace().count() >= SHINGLE_K
}

/// Connects every group of blocks whose narrative is identical, and returns, per block, the
/// group it belongs to (its smallest member by [`end_of`], or `None` when it has no exact twin).
///
/// Connected arithmetically: `n - 1` edges at score 1.0, with no Jaccard computed. That is a
/// correctness fix and not a shortcut. `corpus/REPORT.md` records a group of 800 identical blocks
/// — 320 000 pairs whose score is known before anything is compared — and the same measurement
/// showed that any candidate-generation shortcut hits *these* groups hardest, because a block with
/// 800 twins puts all of its shingles in the largest buckets in the index. The content a duplicate
/// detector exists to find must not be the content its index is most likely to drop.
///
/// # Why a star, and why from the smallest member
///
/// The `C(n, 2) - (n - 1)` edges this does not record all score exactly 1.0, which is the maximum a
/// Jaccard can be, so they can never lower the component's minimum: omitting them is free rather
/// than approximate. They can only matter for the *tie-break* when a whole component scores 1.0,
/// and that is why the centre of the star is the smallest member by [`end_of`] rather than the
/// first one to arrive — the recorded edge set is then the same for any permutation of the input,
/// and so is the pair [`Cluster::weakest`] names.
fn exact_groups(blocks: &[ProseBlock], components: &mut Components) -> Vec<Option<usize>> {
    // One entry per distinct narrative, so `blocks.len()` is the exact ceiling.
    let mut by_text: HashMap<&str, Vec<usize>> = HashMap::with_capacity(blocks.len());
    for (position, block) in blocks.iter().enumerate() {
        // `is_compared` is `TPX003`'s whole answer to "what is left of a block that is mostly a
        // parameter table?" — see its rustdoc. Two such blocks are byte-identical here, so without
        // it they would be an exact group scoring 1.0 on a residue of one word.
        if block.is_large_enough() && is_compared(block) {
            by_text
                .entry(block.narrative.as_str())
                .or_default()
                .push(position);
        }
    }

    let mut group_of: Vec<Option<usize>> = vec![None; blocks.len()];
    for members in by_text.values().filter(|members| members.len() > 1) {
        // Every member has the same narrative, so they differ only in coordinate; the
        // smallest one is therefore a group identity that depends on neither hash order nor
        // arrival order. `min_by_key` cannot fail on a group of two or more.
        let Some(&representative) = members
            .iter()
            .min_by_key(|&&member| blocks[member].coordinates())
        else {
            continue;
        };
        for &member in members {
            group_of[member] = Some(representative);
            if member != representative {
                components.connect(representative, member, 1.0, blocks);
            }
        }
    }
    group_of
}

/// Calls `visit` once per pair of blocks that share at least one shingle, via an inverted index
/// over the shingles.
///
/// # Streamed, never collected
///
/// The pairs are handed to `visit` one at a time and only one left block's partners are ever alive.
/// Collecting them into a `HashSet` first — which is what this used to do — is `C(n, 2)` entries of
/// live memory *before* a single Jaccard is computed, and it dominated everything else: measured on
/// `n` files sharing a licence header that differs in one token, peak resident memory was 21.4 MB at
/// `n = 500`, 53.4 MB at 1 000, 170.9 MB at 2 000 and **619 MB** at 4 000, while the blocks
/// themselves grew only from 7.7 MB to 9.7 MB. That is quadratic storage in the exact case this
/// detector's finding shape was changed to survive, and worse in absolute terms at 2 000
/// near-identical files than the 92.5 MB the old pair model spent on 2 000 identical ones.
///
/// **The number of comparisons is deliberately unchanged**: `n(n-1)/2` of them still happen on that
/// corpus. Candidate *generation* is a separate problem and belongs to
/// `validate-detectors-on-reference-corpus`, which owns the wall-clock budget. Anything that made
/// this cheaper by visiting fewer pairs would be that task's job done wrongly here, which is why the
/// AC2 test asserts the comparison count exactly.
///
/// Chosen over `minhash`/LSH because the measurement says the exact index is already cheap enough
/// on real input, so LSH would only add a probability of *missing* a pair. Measured by the
/// supervisor on the shipped extractor's own output (blocks already `>= 2` lines `AND` `>= 8`
/// words), before this task was written:
///
/// | repository | blocks | largest shingle bucket | candidate pairs | share of `n(n-1)/2` |
/// |---|---|---|---|---|
/// | `OpenHands` | 3 314 | 286 | 117 177 | 2.13% |
/// | `pydantic` | 1 671 | 96 | 45 985 | 3.30% |
/// | `requests` | 343 | 19 | 1 138 | 1.94% |
///
/// The whole index-and-score pass over the largest of those took 0.52 s **in pure Python**.
///
/// # There is deliberately no document-frequency cap
///
/// A cap on bucket size is the obvious next move and it is measurably worthless here: `cap = 40`
/// on `OpenHands` discards 64 550 candidate pairs and changes the number of findings by **zero**.
/// It buys nothing and can only lie — and it lies in the worst direction, because the biggest
/// buckets belong to the most-duplicated blocks. The catastrophic case on record (a cap hiding
/// 737 681 exact pairs, 99.4% of them) was measured at the unfiltered `1 line / 1 word` level,
/// which the inherited size conjunction already removes before anything reaches this function.
fn for_each_candidate_pair(
    sets: &[Vec<Shingle<'_>>],
    group_of: &[Option<usize>],
    mut visit: impl FnMut(usize, usize),
) {
    // Every gram of every block gets inserted, so the sum of the set sizes is the exact ceiling
    // on distinct keys. One O(n) pass to avoid rehashing an index that reaches ~88k keys.
    let mut index: HashMap<Shingle<'_>, Vec<usize>> =
        HashMap::with_capacity(sets.iter().map(Vec::len).sum());
    for (position, set) in sets.iter().enumerate() {
        for &gram in set {
            index.entry(gram).or_default().push(position);
        }
    }

    // One left block at a time. `seen` is a dense marker indexed by position rather than a
    // `HashSet`: the near-header corpus walks tens of millions of bucket entries, and hashing each
    // one to discover it is already known is the whole cost. Both buffers are reused across the
    // outer loop, so the live candidate set is bounded by the number of blocks.
    let mut seen = vec![false; sets.len()];
    let mut partners: Vec<usize> = Vec::new();
    for left in 0..sets.len() {
        partners.clear();
        for gram in &sets[left] {
            // The key is one of `left`'s own grams, so the bucket exists and contains `left`.
            let bucket = &index[gram];
            // A bucket is filled by the ascending `enumerate` above, so it is sorted and the
            // partners with a higher position are one contiguous tail — the earlier claim that a
            // bucket is "in hash order" was simply false; only iteration ACROSS buckets is hashed.
            // Starting from that tail is what keeps each pair from being visited twice, once each
            // way round, whose only visible symptom would be a doubled `comparisons` count.
            for &right in &bucket[bucket.partition_point(|&position| position <= left)..] {
                debug_assert!(
                    left < right,
                    "buckets must be in ascending position order: {left} >= {right}"
                );
                if !seen[right] {
                    seen[right] = true;
                    partners.push(right);
                }
            }
        }
        for &right in &partners {
            seen[right] = false;
            // Exact twins were already connected arithmetically; scoring them here would both
            // duplicate the edge and spend the comparison the arithmetic saved.
            if group_of[left].is_some() && group_of[left] == group_of[right] {
                continue;
            }
            visit(left, right);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};

    use super::{
        Components, Report, SIMILARITY_THRESHOLD, Shingle, duplicates, exact_groups, jaccard,
        shingles,
    };
    use crate::extract::{ProseBlock, ProseKind, extract, narrative, normalize};

    /// A block built directly, for the tests that need a coordinate the parser cannot produce.
    ///
    /// The ordering fixtures need two blocks sharing part of a coordinate — the same span with a
    /// different `kind`, or the same start with a different end — and no Python file yields those.
    /// The text still goes through [`normalize`], so the shingles are the ones production sees.
    fn block(
        path: &str,
        line_start: usize,
        line_end: usize,
        kind: ProseKind,
        text: &str,
    ) -> ProseBlock {
        ProseBlock {
            kind,
            path: PathBuf::from(path),
            line_start,
            line_end,
            normalized: normalize(text),
            narrative: narrative(text),
            raw: text.to_owned(),
        }
    }

    /// Exact one-line twins stay outside `TPX003` even for direct public-API callers.
    #[test]
    fn exact_one_line_twins_are_not_grouped() {
        // Arrange
        let text = "one line with enough words to satisfy the measured word floor";
        let blocks = vec![
            block("left.py", 1, 1, ProseKind::Comment, text),
            block("right.py", 1, 1, ProseKind::Comment, text),
        ];

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert!(report.clusters.is_empty(), "got {:?}", report.clusters);
        assert_eq!(report.comparisons, 0, "exact twins are grouped, not scored");
    }

    /// Near one-line twins allocate no shingles and therefore create no comparison work.
    #[test]
    fn near_one_line_twins_are_not_shingled() {
        // Arrange — six shared of eight total shingles: exactly the 0.75 threshold.
        let blocks = vec![
            block(
                "left.py",
                1,
                1,
                ProseKind::Comment,
                "the retry budget here is small because upstream throttles",
            ),
            block(
                "right.py",
                1,
                1,
                ProseKind::Comment,
                "the retry budget here is small because upstream waits",
            ),
        ];

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert!(report.clusters.is_empty(), "got {:?}", report.clusters);
        assert_eq!(report.comparisons, 0, "one-line blocks allocated shingles");
    }

    /// The same rationale, written once as a module docstring and once as a comment run in
    /// another file. Two word substitutions (`refused`/`rejected`, `simply`/`just`) and a
    /// different line wrapping, so this is a paraphrase rather than a copy-paste.
    const RETRY_PY: &str = r#""""
Retries are capped at three attempts on purpose.

The upstream service rate limits us per minute, so a fourth attempt is refused anyway
and only makes the outage longer for every caller queued behind this one. Raising the
cap here without also raising the quota on their side would simply move the failure one
layer down.
"""
"#;

    const CLIENT_PY: &str = r"# Retries are capped at three attempts on purpose.
# The upstream service rate limits us per minute, so a fourth attempt is rejected anyway
# and only makes the outage longer for every caller queued behind this one. Raising the
# cap here without also raising the quota on their side would just move the failure one
# layer down.
";

    /// `requests/src/requests/api.py:120-166` verbatim, at the pin `corpus/corpus.lock` names —
    /// three public callables whose whole docstring is a reST info-field list under a one-line
    /// summary that differs in the HTTP verb. Cluster #12 of `corpus/annotations.md` §1.2, 0.898.
    ///
    /// A raw string literal with the `#` hashes, because the docstrings carry `\*\*kwargs`: without
    /// them `\*` would be an unknown Rust escape, and `\\*` would change the bytes under test.
    const REQUESTS_API_PY: &str = r#"def post(url, data=None, json=None, **kwargs):
    r"""Sends a POST request.

    :param url: URL for the new :class:`Request` object.
    :param data: (optional) Dictionary, list of tuples, bytes, or file-like
        object to send in the body of the :class:`Request`.
    :param json: (optional) A JSON serializable Python object to send in the body of the :class:`Request`.
    :param \*\*kwargs: Optional arguments that ``request`` takes.
    :return: :class:`Response <Response>` object
    :rtype: requests.Response
    """

    return request("post", url, data=data, json=json, **kwargs)


def put(url, data=None, **kwargs):
    r"""Sends a PUT request.

    :param url: URL for the new :class:`Request` object.
    :param data: (optional) Dictionary, list of tuples, bytes, or file-like
        object to send in the body of the :class:`Request`.
    :param json: (optional) A JSON serializable Python object to send in the body of the :class:`Request`.
    :param \*\*kwargs: Optional arguments that ``request`` takes.
    :return: :class:`Response <Response>` object
    :rtype: requests.Response
    """

    return request("put", url, data=data, **kwargs)


def patch(url, data=None, **kwargs):
    r"""Sends a PATCH request.

    :param url: URL for the new :class:`Request` object.
    :param data: (optional) Dictionary, list of tuples, bytes, or file-like
        object to send in the body of the :class:`Request`.
    :param json: (optional) A JSON serializable Python object to send in the body of the :class:`Request`.
    :param \*\*kwargs: Optional arguments that ``request`` takes.
    :return: :class:`Response <Response>` object
    :rtype: requests.Response
    """

    return request("patch", url, data=data, **kwargs)
"#;

    /// `OpenHands/enterprise/server/routes/org_profiles.py:101-105` verbatim — one half of cluster
    /// #1 of `corpus/annotations.md` §1.2, a `yes` verdict at 0.885.
    const ORG_PROFILES_PY: &str = r"include_secrets: bool = True
# Set when the caller has no new key (UI key field left blank), so an
# existing profile's stored key survives instead of the snapshotted one.
preserve_existing_api_key: bool = False
";

    /// `OpenHands/openhands/app_server/settings/settings_router.py:486-490` verbatim — the other
    /// half of cluster #1. One word apart (`active`), and pure narrative.
    const SETTINGS_ROUTER_PY: &str = r"include_secrets: bool = True
# Set when the caller has no new key (UI key field left blank), so an
# existing profile's stored key survives instead of the snapshotted active one.
preserve_existing_api_key: bool = False
";

    fn corpus(files: &[(&str, &str)]) -> Vec<ProseBlock> {
        let mut blocks = Vec::new();
        for (path, source) in files {
            blocks.extend(extract(Path::new(path), source).expect("the fixture is valid Python"));
        }
        blocks
    }

    /// Renders a whole report the way AC4 compares it: one cluster per line, in output order.
    fn render(report: &Report<'_>) -> String {
        let mut rendered = String::new();
        for cluster in &report.clusters {
            writeln!(rendered, "{cluster}").expect("writing into a String cannot fail");
        }
        rendered
    }

    /// A deterministic synthetic corpus: `families` planted near-duplicate families of
    /// [`FAMILY_SIZE`] members each, plus `filler` blocks of prose that repeats nothing.
    ///
    /// Blocks are built directly rather than parsed out of generated Python: the property under
    /// test is the pairing algorithm, and 10 000 parses would measure the parser instead. The
    /// words still go through [`normalize`], so the shingles are the ones the detector sees in
    /// production.
    ///
    /// The paths deliberately do **not** ascend with arrival order (`mod{i % 37}.py`), so the
    /// coordinate sort has real work to do and the AC4 permutation test is not vacuous.
    fn synthetic_corpus(families: usize, filler: usize) -> Vec<ProseBlock> {
        /// Members per planted family. `C(4, 2)` = 6 findings per family.
        const FAMILY_SIZE: usize = 4;
        /// Words a family shares. 20 shared + 1 distinct tail word puts every intra-family pair
        /// at `18 / 20` = 0.900 — a near duplicate, above the threshold and *not* exact, so it is
        /// scored rather than counted arithmetically.
        const FAMILY_WORDS: usize = 20;
        /// Words per filler block.
        const FILLER_WORDS: usize = 14;
        /// Filler vocabulary. At 4096 words a shared 3-gram between two filler blocks is a
        /// ~1-in-10^10 event, so accidental candidates stay a rounding error and the growth the
        /// test measures is the growth of the planted structure.
        const VOCABULARY: u64 = 4096;

        let mut texts: Vec<String> = Vec::with_capacity(families * FAMILY_SIZE + filler);
        for family in 0..families {
            let mut base = String::new();
            for word in 0..FAMILY_WORDS {
                write!(base, "fam{family}word{word} ").expect("writing into a String cannot fail");
            }
            for member in 0..FAMILY_SIZE {
                texts.push(format!("{base}tail{family}x{member}"));
            }
        }
        // A 64-bit LCG, not `rand`: no dependency, and the corpus is byte-identical on every
        // machine, which a determinism test has no business leaving to chance.
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        for _ in 0..filler {
            let mut words = String::new();
            for _ in 0..FILLER_WORDS {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                write!(words, "w{} ", (state >> 33) % VOCABULARY)
                    .expect("writing into a String cannot fail");
            }
            texts.push(words);
        }

        texts
            .into_iter()
            .enumerate()
            .map(|(position, text)| ProseBlock {
                kind: ProseKind::Comment,
                path: PathBuf::from(format!("pkg/mod{}.py", position % 37)),
                line_start: position * 3 + 1,
                line_end: position * 3 + 2,
                normalized: normalize(&text),
                narrative: narrative(&text),
                raw: text,
            })
            .collect()
    }

    /// AC1 — one rationale written twice in two files is one finding, and it names both blocks.
    #[test]
    fn a_rationale_written_twice_in_two_files_is_one_cluster() {
        // Arrange
        let blocks = corpus(&[("client.py", CLIENT_PY), ("retry.py", RETRY_PY)]);
        assert_eq!(blocks.len(), 2, "the fixture must yield one block each");

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(report.clusters.len(), 1, "got {:?}", report.clusters);
        let cluster = &report.clusters[0];
        assert_eq!(cluster.members.len(), 2, "got {:?}", cluster.members);
        assert_eq!(cluster.members[0].path, Path::new("client.py"));
        assert_eq!(cluster.members[1].path, Path::new("retry.py"));
        assert_eq!(cluster.members[0].kind, ProseKind::Comment);
        assert_eq!(cluster.members[1].kind, ProseKind::Docstring);
        // Pinned, not approximated: 0.800 is what the ported definitions give on this fixture,
        // measured with `corpus/measure.py`'s own `shingles`/`jaccard` before the test was
        // written. A change here means the shingle or the metric moved.
        assert!(
            (cluster.weakest_score - 0.8).abs() < 1e-9,
            "expected J=0.800, got {}",
            cluster.weakest_score
        );
    }

    /// AC2 — the same sentence wrapped differently is still one finding, and it is the
    /// **normalisation inherited from `extract`** that makes it one.
    ///
    /// This is the guard named in the task's TDD block: comparing `raw` instead of `normalized`
    /// must turn this red. The two blocks are shaped so a leak is unmissable — identical words,
    /// different line breaks, different indentation, a tab in one of them.
    #[test]
    fn the_same_sentence_wrapped_differently_is_one_cluster() {
        // Arrange
        let flat = "# The retry budget here is deliberately small, and that matters because the \
                    upstream\n\
                    # service rate limits us on every fourth call.\n";
        let wrapped = "#   The retry budget here is deliberately\n\
                       #        small, and that matters\n\
                       #   because the upstream service rate\n\
                       #\tlimits us on every fourth call.\n";
        let blocks = corpus(&[("flat.py", flat), ("wrapped.py", wrapped)]);
        assert_eq!(blocks.len(), 2, "got {blocks:?}");
        assert_ne!(blocks[0].raw, blocks[1].raw, "the raw texts must differ");

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(report.clusters.len(), 1, "got {:?}", report.clusters);
        assert_eq!(report.clusters[0].members.len(), 2);
        assert!(
            (report.clusters[0].weakest_score - 1.0).abs() < 1e-9,
            "normalisation makes these identical; got {}",
            report.clusters[0].weakest_score
        );
    }

    /// AC5 — two copies of `"""Initialize the class."""` are not a finding.
    ///
    /// The size conjunction that excludes them lives in [`crate::extract`] and is not re-applied
    /// here — but this test goes through the **detector's** entry point, because that is the only
    /// way it says anything about this module: a detector that rebuilt its own block list, or
    /// compared `raw` text it had gathered itself, would flag them.
    ///
    /// The corpus deliberately also carries one **real** duplicate, so the assertion is
    /// `exactly one finding, and it is not the short one` rather than `nothing was found` — which
    /// a detector that returns an empty report for everything would also satisfy.
    #[test]
    fn two_copies_of_a_short_docstring_are_not_a_finding() {
        // Arrange
        let left = "class Client:\n    \"\"\"Initialize the class.\"\"\"\n\n\
                    # The retry budget here is deliberately small, and that matters because\n\
                    # the upstream service rate limits us on every fourth call.\n";
        let right = "class Server:\n    \"\"\"Initialize the class.\"\"\"\n\n\
                     # The retry budget here is deliberately small, and that matters because\n\
                     # the upstream service rate limits us on every fourth call.\n";
        let blocks = corpus(&[("left.py", left), ("right.py", right)]);

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(
            report.clusters.len(),
            1,
            "only the long prose may be reported; got {:?}",
            report.clusters
        );
        for cluster in &report.clusters {
            for member in &cluster.members {
                assert!(
                    !member.normalized.contains("initialize the class"),
                    "the short docstring reached the detector: {member:?}"
                );
            }
        }
    }

    /// Red-team — two blocks quoting the same short term with different prose around it score
    /// below the threshold and are not flagged. Without this, "found the pair" would be
    /// indistinguishable from "flags everything that shares a word".
    #[test]
    fn two_blocks_sharing_one_term_are_not_a_cluster() {
        // Arrange
        let left = "# The retry budget is small because the upstream service rate limits us on\n\
                    # every fourth call, and a queue would only hide the problem.\n";
        let right = "# The retry budget is set by the operator, and every deployment reads it\n\
                     # from the environment before the first request is ever sent.\n";
        let blocks = corpus(&[("left.py", left), ("right.py", right)]);
        assert_eq!(blocks.len(), 2, "got {blocks:?}");

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert!(
            report.clusters.is_empty(),
            "shared words are not a duplicate; got {:?}",
            report.clusters
        );
        assert_eq!(report.comparisons, 1, "the pair must have been scored");
    }

    /// Three public callables whose docstrings differ only in an HTTP verb are **not** a finding.
    ///
    /// The bytes are `requests/src/requests/api.py:123-166` verbatim (`post`, `put`, `patch`), the
    /// cluster `corpus/annotations.md` §1.2 records as #12 at similarity **0.898** — the measured
    /// false positive this rule exists to remove. There is no fix a user could apply: `help(post)`
    /// and `help(put)` each need their own reference table, so "delete or merge one copy" is advice
    /// that can only make the documentation worse.
    ///
    /// Everything the three share is a reST info-field list (`:param:`, `:return:`, `:rtype:`).
    /// What remains once that is discarded is `Sends a POST request.` against
    /// `Sends a PUT request.`, which share no three-word gram at all.
    ///
    /// **Why 0.898 proves this is a feature defect and not a constant one:** the genuine finding in
    /// `a_rationale_copied_between_two_files_is_still_a_finding` scores 0.885 on the same scale, and
    /// `a_pair_exactly_on_the_threshold_is_a_cluster` pins a real one at 0.750. The classes overlap,
    /// so no threshold separates them.
    #[test]
    fn a_shared_parameter_table_is_not_duplicate_prose() {
        // Arrange
        let blocks = corpus(&[("api.py", REQUESTS_API_PY)]);
        assert_eq!(blocks.len(), 3, "the fixture must yield one block each");

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert!(
            report.clusters.is_empty(),
            "a repeated reference table is not duplicated prose; got {:?}",
            report.clusters
        );
    }

    /// One rationale copied between two files **stays** a finding — the guard against overreach.
    ///
    /// The bytes are `OpenHands/enterprise/server/routes/org_profiles.py:103-104` and
    /// `openhands/app_server/settings/settings_router.py:488-489` verbatim, cluster #1 of
    /// `corpus/annotations.md` §1.2 at similarity 0.885 and a `yes` verdict: one of the two copies
    /// should reference the other.
    ///
    /// It carries **no** scaffolding of any kind — no `Args:`, no `:param:`, no example — so the
    /// only way [`crate::extract::narrative`] can silence it is by eating narrative, which is the
    /// mutation this test exists to catch. It is green before the change and must stay green after;
    /// "exclude everything" would otherwise pass as a solution to the test above.
    #[test]
    fn a_rationale_copied_between_two_files_is_still_a_finding() {
        // Arrange
        let blocks = corpus(&[
            ("org_profiles.py", ORG_PROFILES_PY),
            ("settings_router.py", SETTINGS_ROUTER_PY),
        ]);
        assert_eq!(blocks.len(), 2, "the fixture must yield one block each");

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(report.clusters.len(), 1, "got {:?}", report.clusters);
        assert_eq!(report.clusters[0].members.len(), 2);
        assert!(
            report.clusters[0].weakest_score >= SIMILARITY_THRESHOLD,
            "got {}",
            report.clusters[0].weakest_score
        );
    }

    /// A block that is nothing **but** a parameter table takes no part in the rule at all.
    ///
    /// The third open question of `exclude-reference-scaffolding-from-tpx003`: once scaffolding is
    /// discarded there is no explanation left to have been "said twice", so the block has nothing to
    /// be a duplicate *of*. Two byte-identical copies of such a docstring are the case that decides
    /// it — under the old rule they were an exact-text group scoring 1.0.
    ///
    /// The corpus deliberately also carries one real duplicate, so the assertion is `exactly one
    /// finding, and it is not the table` rather than `nothing was found`.
    #[test]
    fn a_block_that_is_only_a_parameter_table_is_not_compared_at_all() {
        // Arrange
        let table = "def send(payload, timeout):\n    \"\"\"\n    Args:\n        payload: the body to send, already encoded by the caller.\n        timeout: how long to wait for the server, in seconds.\n    \"\"\"\n\n\n\
                     # The retry budget here is deliberately small, and that matters because\n\
                     # the upstream service rate limits us on every fourth call.\n";
        let blocks = corpus(&[("left.py", table), ("right.py", table)]);
        assert_eq!(
            blocks.len(),
            4,
            "two docstrings and two comment runs: {blocks:?}"
        );

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(
            report.clusters.len(),
            1,
            "only the prose may be reported; got {:?}",
            report.clusters
        );
        for member in &report.clusters[0].members {
            assert_eq!(member.kind, ProseKind::Comment, "got {member:?}");
        }
    }

    /// **Two unrelated callables that happen to share a one-word summary are not a finding** —
    /// the false positive this task's own first cut introduced.
    ///
    /// `send_email` and `send_packet` document different parameters, do different things and share
    /// nothing but the word `Send`. Discarding their `Args:` tables leaves each with the single word
    /// `send`, and the *exact* path keyed on that: the review measured
    /// `TPX003 same explanation in 2 places … similarity 1.000` on this input, against
    /// `All checks passed!` from the pre-change detector. A task whose purpose is removing a class
    /// of false positives had introduced one.
    ///
    /// The asymmetry that caused it is what [`is_compared`] now fixes, and it is worth naming: the
    /// **near** path already refuses a narrative shorter than [`SHINGLE_K`] words, because such a
    /// text has no shingle and the inverted index never proposes it as a candidate. The **exact**
    /// path had no floor at all, so a one-word residue could only ever be matched — never scored —
    /// and no threshold could reach it.
    #[test]
    fn two_unrelated_callables_sharing_a_one_word_summary_are_not_a_finding() {
        // Arrange
        let source = "def send_email(to, subject, body, retries):\n    \"\"\"Send.\n\n\
                      \x20   Args:\n\
                      \x20       to: the recipient address, already validated by the caller.\n\
                      \x20       subject: the subject line, trimmed to the provider's limit.\n\
                      \x20       body: the rendered message body.\n\
                      \x20       retries: how many times to retry a soft bounce.\n\
                      \x20   \"\"\"\n\n\n\
                      def send_packet(iface, payload, ttl, checksum):\n    \"\"\"Send.\n\n\
                      \x20   Args:\n\
                      \x20       iface: the interface to write the frame to.\n\
                      \x20       payload: the bytes to put on the wire.\n\
                      \x20       ttl: hop limit, decremented by every router on the path.\n\
                      \x20       checksum: the precomputed checksum, or None to compute one.\n\
                      \x20   \"\"\"\n";
        let blocks = corpus(&[("send.py", source)]);
        assert_eq!(blocks.len(), 2, "the fixture must yield one block each");

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert!(
            report.clusters.is_empty(),
            "one shared word is not one shared explanation; got {:?}",
            report.clusters
        );
    }

    /// **Two logically opposite statements are not one explanation.**
    ///
    /// The end-to-end form of the [`crate::extract::normalize`] defect that
    /// `close-anti-fp-gate-with-public-reference` measured on the pinned corpus
    /// (`corpus/annotations.md` §4.7, record 18): `requests`' `test_requests.py:2252` and `:2264`
    /// assert opposite things about a byte stream — `with size 0` against `with size > 0` — and are
    /// reported as **one** `TPX003` finding at similarity **1.000**, because the normaliser turned
    /// every non-alphanumeric character into a space and the `>` went with it.
    ///
    /// The fixture below uses `>` against `<` rather than the corpus's `0` against `> 0`, because
    /// that form cannot be argued away as a near-synonym: the two sentences state **opposite**
    /// bounds. This is the same class as
    /// [`Self::two_unrelated_callables_sharing_a_one_word_summary_are_not_a_finding`] — the detector
    /// asserting an identity that does not exist in the source — and, like that one, it was found by
    /// measurement rather than by reading the grammar.
    ///
    /// # What this pins, and what it deliberately does not
    ///
    /// **It pins that the score is no longer 1.000. It does not pin that nothing is reported.**
    /// The first draft of this test asserted an empty cluster list and failed after the fix at
    /// **0.769** — and that failure is the test being wrong, not the code. Two 24-word sentences
    /// differing in one token *are* near-duplicates by any Jaccard measure, and suppressing them is
    /// a [`SIMILARITY_THRESHOLD`] question that this task may not touch and that
    /// `corpus/annotations.md` §1.5 already measured as unreachable: genuine findings sit at 0.760,
    /// **0.769** and 0.772. The fixture below scores **0.76923…**, which is the *same* score as the
    /// genuine `list`/`alist` cluster across four checkpoint backends. Any constant that silences
    /// this silences that.
    ///
    /// So the defect being fixed is stated exactly: **the detector claimed an identity**. `1.000`
    /// comes off the exact path, which has no threshold at all — no user, no configuration and no
    /// future calibration can reach it. `0.769` comes off the near path, which is governed by a
    /// constant and is therefore a decision someone can still make. The fix moves this class from
    /// unreachable to reachable; it does not, and cannot, make it disappear.
    #[test]
    fn two_statements_differing_only_by_a_comparison_operator_are_not_an_exact_match() {
        // Arrange — two lines each, so both clear MIN_BLOCK_LINES; identical but for the operator.
        let greater = "# Ensure that a byte stream with size > 0 will not set both a Content-Length\n\
                       # and a Transfer-Encoding header on the outgoing request.\n";
        let less = "# Ensure that a byte stream with size < 0 will not set both a Content-Length\n\
                    # and a Transfer-Encoding header on the outgoing request.\n";
        let blocks = corpus(&[("greater.py", greater), ("less.py", less)]);
        assert_eq!(blocks.len(), 2, "the fixture must yield one block each");
        // `narrative`, not `normalized`: the operator survives only in the **comparison** form, and
        // `narrative` is the field this detector reads. `normalized` is the counting form the volume
        // limits were calibrated in and still erases the operator — asserting on it here would be
        // asserting against the wrong contract, and it is what the first draft of this line did.
        assert_ne!(
            blocks[0].narrative, blocks[1].narrative,
            "the operator must survive into the compared form, or nothing below can distinguish them"
        );
        assert_eq!(
            blocks[0].normalized, blocks[1].normalized,
            "the counting form must be untouched by this fix — 150/200 were measured in it"
        );

        // Act
        let report = duplicates(&blocks);

        // Assert — the claim of identity is what must be gone.
        for cluster in &report.clusters {
            assert!(
                cluster.weakest_score < 1.0,
                "`> 0` and `< 0` are opposite claims and must never score as the same text; got {cluster:?}"
            );
        }
    }

    /// A group of `n` identical blocks is **one** finding carrying all `n` addresses, and costs
    /// **zero** pairwise comparisons.
    ///
    /// This is the assertion the pair model got wrong, and the whole reason for this shape: five
    /// identical blocks used to be `C(5, 2) = 10` findings saying the same thing about the same
    /// text. What is pinned now is that the count does not grow with `n` while the *addresses* do —
    /// `members.len() == 5` is what keeps "one finding" from being achievable by throwing four
    /// blocks away.
    ///
    /// `comparisons == 0` is inherited unchanged from the pair model and is not an optimisation.
    /// `corpus/REPORT.md` records a group of 800 identical blocks whose score is 1.0 by definition;
    /// scoring them one by one buys nothing and makes the most-duplicated content — the whole point
    /// of the detector — the content most likely to be dropped by any candidate-generation shortcut.
    ///
    /// Two of the five copies are **wrapped differently**, so "identical" here means identical
    /// *normalised* text. Without that, keying the groups on `raw` would leave this test green:
    /// the blocks would still be connected through the index, only at a cost, and `comparisons == 0`
    /// is the sole observable difference between the two.
    #[test]
    fn a_group_of_identical_blocks_is_one_cluster_counted_without_comparing_them() {
        // Arrange
        let flat = "# The retry budget here is deliberately small, and that matters because\n\
                    # the upstream service rate limits us on every fourth call.\n";
        let wrapped = "#   The retry budget here is deliberately\n\
                       #   small, and that matters because the\n\
                       #\tupstream service rate limits us on every fourth call.\n";
        let blocks = corpus(&[
            ("a.py", flat),
            ("b.py", flat),
            ("c.py", flat),
            ("d.py", wrapped),
            ("e.py", wrapped),
        ]);
        assert_eq!(blocks.len(), 5, "got {blocks:?}");
        assert!(
            blocks
                .iter()
                .all(|block| block.normalized == blocks[0].normalized),
            "all five must normalise to one text: {blocks:?}"
        );

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(report.clusters.len(), 1, "got {:?}", report.clusters);
        assert_eq!(
            report.clusters[0].members.len(),
            5,
            "one finding, but all five addresses; got {:?}",
            report.clusters[0].members
        );
        assert!(
            (report.clusters[0].weakest_score - 1.0).abs() < f64::EPSILON,
            "every edge of an exact group scores 1.0 by definition"
        );
        assert_eq!(
            report.comparisons, 0,
            "an exact group needs no Jaccard computation at all"
        );
    }

    /// One address is reported once, however many times the caller handed in the same block — and
    /// a block that is only ever a duplicate **of itself** is not a finding at all.
    ///
    /// Reachable through a caller that extracts one path twice: a symlink, or a walker that yields
    /// a directory twice. Under the pair model this produced a finding reading "a.py:1 is a
    /// duplicate of a.py:1", which named one address twice and gave the reader nothing to act on;
    /// the honest answer is that one piece of prose seen twice by a broken walker is not duplicated
    /// prose. That is a **deliberate behaviour change**, so the second half of the test pins the
    /// case that must keep working: the moment a genuine second block joins, the cluster appears
    /// and names exactly two addresses rather than three.
    #[test]
    fn a_block_handed_in_twice_is_one_address_and_not_a_finding_by_itself() {
        // Arrange — the same file extracted three times, as a walker following a symlink would.
        let source = "# The retry budget here is deliberately small, and that matters because\n\
                      # the upstream service rate limits us on every fourth call.\n";
        let alone = corpus(&[("a.py", source), ("a.py", source), ("a.py", source)]);
        let with_a_real_twin = corpus(&[
            ("a.py", source),
            ("a.py", source),
            ("a.py", source),
            ("b.py", source),
        ]);
        assert_eq!((alone.len(), with_a_real_twin.len()), (3, 4));

        // Act
        let alone_report = duplicates(&alone);
        let twinned_report = duplicates(&with_a_real_twin);

        // Assert
        assert!(
            alone_report.clusters.is_empty(),
            "one address handed in three times is not duplicated prose; got {:?}",
            alone_report.clusters
        );
        assert_eq!(twinned_report.clusters.len(), 1);
        assert_eq!(
            twinned_report.clusters[0]
                .members
                .iter()
                .map(|member| member.path.as_path())
                .collect::<Vec<_>>(),
            vec![Path::new("a.py"), Path::new("b.py")],
            "the repeated block is one member, not three"
        );
    }

    /// The weakest link is never a block against **itself**.
    ///
    /// One address handed in twice — a walker that yielded one path twice — is one member, because
    /// `members` deduplicates by [`End`]. The edge between those two copies is an artifact of the
    /// caller, not an edge of the similarity graph, and naming it as the weakest link makes the
    /// cluster announce `weakest a.py:1 ~ a.py:1`: one address, printed twice, against itself.
    ///
    /// It is the other half of the decision the test above records. Suppressing the finding when a
    /// block is *only* a duplicate of itself is worth nothing if the same self-pair leaks back
    /// through `weakest` the moment a genuine twin joins — and
    /// `build-cli-with-exit-contract-and-rule-codes` renders this field into a frozen line.
    #[test]
    fn the_weakest_link_is_never_a_block_against_itself() {
        // Arrange — a.py twice, plus a genuine twin in b.py.
        let source = "# The retry budget here is deliberately small, and that matters because\n\
                      # the upstream service rate limits us on every fourth call.\n";
        let blocks = corpus(&[("a.py", source), ("a.py", source), ("b.py", source)]);

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(
            render(&report),
            "a.py:1-2, b.py:1-2: duplicate prose (weakest a.py:1-2 ~ b.py:1-2, similarity 1.000)\n"
        );
    }

    /// The weakest link's ends are the very blocks in `members`, and which blocks those are is a
    /// function of the input.
    ///
    /// Two guarantees that fail together and are invisible to every other test here, because
    /// `Display` prints a path and a line and these blocks agree on both. What differs is `raw` —
    /// and `raw` is public, so a consumer can read it.
    ///
    /// 1. `weakest` used to hold raw block positions while `members` deduplicated by [`End`], so
    ///    the weakest link could name a block that had been deduplicated away — "both ends are
    ///    members" was a comment, not an invariant.
    /// 2. Among blocks that share an [`End`], which one survives the dedup was decided by
    ///    `sort_by`'s stability, i.e. by arrival order. Permuting the input then changed the
    ///    `raw` reachable through the report while every rendered byte stayed the same, which is
    ///    exactly the kind of "deterministic" that is only deterministic in what the test looks at.
    #[test]
    fn the_report_holds_the_same_blocks_whatever_order_they_arrive_in() {
        // Arrange — two blocks that share a coordinate AND a normalised text but differ in `raw`
        // (punctuation and case are what `normalize` removes), plus a twin in another file.
        let loud = "Retry budget is SMALL, and that matters here for us!";
        let quiet = "retry budget is small and that matters here for us";
        let forward = vec![
            block("a.py", 1, 2, ProseKind::Comment, loud),
            block("a.py", 1, 2, ProseKind::Comment, quiet),
            block("b.py", 1, 2, ProseKind::Comment, quiet),
        ];
        let reversed: Vec<ProseBlock> = forward.iter().rev().cloned().collect();
        assert_eq!(forward[0].normalized, forward[1].normalized);
        assert_ne!(forward[0].raw, forward[1].raw);

        // Act
        let forward_report = duplicates(&forward);
        let reversed_report = duplicates(&reversed);

        // Assert
        for report in [&forward_report, &reversed_report] {
            let cluster = &report.clusters[0];
            for end in [cluster.weakest.0, cluster.weakest.1] {
                assert!(
                    cluster
                        .members
                        .iter()
                        .any(|member| std::ptr::eq(*member, end)),
                    "the weakest link names a block that is not a member: {end:?}"
                );
            }
        }
        let raws = |report: &Report<'_>| {
            let cluster = &report.clusters[0];
            (
                cluster
                    .members
                    .iter()
                    .map(|member| member.raw.clone())
                    .collect::<Vec<_>>(),
                cluster.weakest.0.raw.clone(),
                cluster.weakest.1.raw.clone(),
            )
        };
        assert_eq!(raws(&forward_report), raws(&reversed_report));
    }

    /// Members are ordered by where they are **and by what they say** — the coordinate alone is not
    /// a total order over them.
    ///
    /// Two blocks *can* share a coordinate: a caller that extracts one path from two sources, which
    /// is the same input shape that once dropped a real finding. On a coordinates-only key they tie,
    /// `sort_by` is stable, and their order is then the order they arrived in — so permuting the
    /// input silently permutes the members.
    ///
    /// Nothing else in this file can see that, and that is the point of writing it: `Display` prints
    /// a path and a line, these two blocks agree on both, and the assertion therefore has to look at
    /// the **normalised text**. The two texts are also arranged so that `raw` sorts the opposite way
    /// (`Zebra` before `apple` in byte order, `apple` before `zebra` after normalisation), so the
    /// tie-break that keeps duplicate addresses deterministic cannot stand in for the text half and
    /// make a coordinates-only key look correct.
    #[test]
    fn members_are_ordered_by_what_they_say_when_the_coordinate_ties() {
        // Arrange — two blocks on a.py:1-2 with different texts, plus a partner that clusters them.
        let tail = "rationale about the retry budget and the upstream rate limit here";
        let forward = vec![
            block("a.py", 1, 2, ProseKind::Comment, &format!("Zebra {tail}")),
            block("a.py", 1, 2, ProseKind::Comment, &format!("apple {tail}")),
            block("b.py", 1, 2, ProseKind::Comment, &format!("cherry {tail}")),
        ];
        let reversed: Vec<ProseBlock> = forward.iter().rev().cloned().collect();

        // Act
        let forward_report = duplicates(&forward);
        let reversed_report = duplicates(&reversed);

        // Assert
        for report in [&forward_report, &reversed_report] {
            assert_eq!(report.clusters.len(), 1, "got {:?}", report.clusters);
            let texts: Vec<&str> = report.clusters[0]
                .members
                .iter()
                .map(|member| member.normalized.split(' ').next().unwrap_or_default())
                .collect();
            assert_eq!(texts, vec!["apple", "zebra", "cherry"]);
        }
    }

    /// Clusters are ordered by what their smallest member says when its coordinate ties — the other
    /// half of the same hole, and equally invisible to every rendered byte.
    ///
    /// Eight clusters whose smallest member is the *same* coordinate, `a.py:1-2`, and a different
    /// text. On the full key they come out in text order; on a coordinates-only key all eight keys
    /// are equal, `sort_by` is stable, and the output order is the iteration order of the `HashMap`
    /// the components were collected into. Eight is not decoration: with two tied clusters a broken
    /// key still prints the right order half the time, and a mutation proof that passes half the
    /// time proves nothing. With eight it is one permutation in 40 320.
    #[test]
    fn clusters_are_ordered_by_what_their_smallest_member_says_when_the_coordinate_ties() {
        // Arrange — eight unrelated pairs, each anchored on the same a.py coordinate.
        let mut blocks = Vec::new();
        for family in 0..8 {
            let mut text = String::new();
            for word in 0..12 {
                write!(text, "t{family}word{word} ").expect("writing into a String cannot fail");
            }
            blocks.push(block("a.py", 1, 2, ProseKind::Comment, &text));
            blocks.push(block(
                &format!("z{family}.py"),
                1,
                2,
                ProseKind::Comment,
                &text,
            ));
        }

        // Act
        let rendered = render(&duplicates(&blocks));

        // Assert
        let partners: Vec<&str> = rendered
            .lines()
            .filter_map(|line| line.split(", ").nth(1))
            .filter_map(|rest| rest.split(':').next())
            .collect();
        assert_eq!(
            partners,
            vec![
                "z0.py", "z1.py", "z2.py", "z3.py", "z4.py", "z5.py", "z6.py", "z7.py",
            ]
        );
    }

    /// Members are ordered by where they are, never by when they arrived.
    ///
    /// Isolated from the cluster ordering on purpose: there is exactly **one** cluster here, so
    /// sorting the clusters is a no-op and only the member order can move these bytes. The blocks
    /// are handed in in descending path order, so a member list that kept arrival order — or that
    /// reversed the comparison — renders `c.py, b.py, a.py`.
    #[test]
    fn members_are_ordered_by_where_they_are_not_by_when_they_arrived() {
        // Arrange
        let source = "# The retry budget here is deliberately small, and that matters because\n\
                      # the upstream service rate limits us on every fourth call.\n";
        let blocks = corpus(&[("c.py", source), ("b.py", source), ("a.py", source)]);

        // Act
        let rendered = render(&duplicates(&blocks));

        // Assert
        assert_eq!(
            rendered,
            "a.py:1-2, b.py:1-2, c.py:1-2: duplicate prose \
             (weakest a.py:1-2 ~ b.py:1-2, similarity 1.000)\n"
        );
    }

    /// AC4, first half — two runs over the same corpus give byte-identical output.
    ///
    /// Not vacuous the way it would be in Python: two `HashMap`s in one Rust process iterate the
    /// same keys in different orders, so hash order leaking into the output is caught here.
    #[test]
    fn two_runs_over_the_same_corpus_give_byte_identical_output() {
        // Arrange
        let blocks = synthetic_corpus(12, 40);

        // Act
        let first = render(&duplicates(&blocks));
        let second = render(&duplicates(&blocks));

        // Assert
        assert_eq!(
            first.lines().count(),
            12,
            "AC2 of the issue: determinism is only proved on a NON-empty finding set"
        );
        assert_eq!(first, second);
    }

    /// AC4, second half — the same blocks in a different arrival order give byte-identical
    /// output. Strictly stronger than the two-run half, and it catches defects that one cannot:
    /// a member list left in arrival order, an exact group whose star is centred on whichever
    /// member happened to arrive first, or a weakest edge resolved by "the first minimum seen".
    /// All three keep the *set* of clusters identical while moving the bytes.
    #[test]
    fn permuting_the_input_gives_byte_identical_output() {
        // Arrange
        let blocks = synthetic_corpus(12, 40);
        let reversed: Vec<ProseBlock> = blocks.iter().rev().cloned().collect();

        // Act
        let forward = render(&duplicates(&blocks));
        let backward = render(&duplicates(&reversed));

        // Assert
        assert!(
            forward.lines().count() >= 10,
            "at least 10 findings, or the comparison is decoration; got {}",
            forward.lines().count()
        );
        assert_eq!(forward, backward);
    }

    /// AC4, permutation, on the half `synthetic_corpus` cannot reach: **exact** groups.
    ///
    /// Every family there is a near-duplicate family, so the exact path contributes no edge to that
    /// fixture at all. The exact path picks a star centre, and picking it by arrival order — the
    /// obvious `members[0]` — leaves the cluster set and every score identical while changing which
    /// pair the output names as the weakest link, because every edge of an exact group scores 1.0
    /// and the tie is then broken between different edge sets.
    #[test]
    fn permuting_an_exact_group_gives_byte_identical_output() {
        // Arrange — four identical blocks, handed in in two different orders.
        let source = "# The retry budget here is deliberately small, and that matters because\n\
                      # the upstream service rate limits us on every fourth call.\n";
        let forward_blocks = corpus(&[
            ("a.py", source),
            ("b.py", source),
            ("c.py", source),
            ("d.py", source),
        ]);
        let reversed: Vec<ProseBlock> = forward_blocks.iter().rev().cloned().collect();

        // Act
        let forward = render(&duplicates(&forward_blocks));
        let backward = render(&duplicates(&reversed));

        // Assert
        assert_eq!(
            forward.lines().count(),
            1,
            "one cluster, or this proves nothing"
        );
        assert_eq!(forward, backward);
    }

    /// AC3 — the **growth law**, measured, not the share of `n(n-1)/2`.
    ///
    /// The share is an artifact of the chosen `n`: it falls quadratically by construction, so a
    /// "< 1%" assertion passes or fails depending on which `n` was picked (measured on the real
    /// corpus pool: 3.32% at n=1000 down to 0.73% at n=12 890, same algorithm throughout). What
    /// distinguishes a candidate index from full enumeration is how the comparison count *grows*:
    /// doubling `n` doubles the planted structure, so the honest bound is a small multiple of 2,
    /// while full enumeration is pinned at exactly 4.
    ///
    /// The share at both sizes is printed as an observation, never asserted.
    #[test]
    fn bench_smoke_comparisons_grow_no_faster_than_the_corpus() {
        // Arrange — n and 2n, with the planted families doubled alongside the filler.
        let small = synthetic_corpus(250, 4_000);
        let large = synthetic_corpus(500, 8_000);
        assert_eq!((small.len(), large.len()), (5_000, 10_000));

        // Act
        let small_started = std::time::Instant::now();
        let small_report = duplicates(&small);
        let small_elapsed = small_started.elapsed();
        let large_started = std::time::Instant::now();
        let large_report = duplicates(&large);
        let large_elapsed = large_started.elapsed();

        // Assert
        for (n, report, elapsed) in [
            (small.len(), &small_report, small_elapsed),
            (large.len(), &large_report, large_elapsed),
        ] {
            #[allow(
                clippy::cast_precision_loss,
                reason = "an observation printed for humans, not a value anything branches on"
            )]
            let share = 100.0 * report.comparisons as f64 / (n * (n - 1) / 2) as f64;
            println!(
                "n={n}: comparisons={} ({share:.2}% of n(n-1)/2), clusters={}, {elapsed:?}",
                report.comparisons,
                report.clusters.len(),
            );
        }
        // Not merely "non-empty": every planted family, whole. One cluster per family and four
        // members in each is what a silently dropped block would break — and because a cluster
        // survives losing an edge, the count of *comparisons* carries the other half of the claim:
        // C(4, 2) = 6 candidate pairs per family must all have been generated and scored. That is
        // the failure mode a document-frequency cap or an LSH sketch buys, and the reason neither
        // is used here.
        for (families, report) in [(250, &small_report), (500, &large_report)] {
            assert_eq!(
                report.clusters.len(),
                families,
                "one cluster per planted family, and no two families merged"
            );
            assert!(
                report
                    .clusters
                    .iter()
                    .all(|cluster| cluster.members.len() == 4),
                "every planted family is whole"
            );
            assert!(
                report.comparisons >= families * 6,
                "the index dropped candidate pairs: {} < {}",
                report.comparisons,
                families * 6
            );
        }

        #[allow(
            clippy::cast_precision_loss,
            reason = "comparison counts here are ~10^3, exact in f64"
        )]
        let growth = large_report.comparisons as f64 / small_report.comparisons as f64;
        assert!(
            growth <= 2.5,
            "doubling n multiplied the comparisons by {growth:.2} \
             ({} -> {}); full enumeration is 4.0",
            small_report.comparisons,
            large_report.comparisons
        );
    }

    /// The threshold is the measured one and is not re-derived here.
    #[test]
    fn the_similarity_threshold_is_the_measured_constant() {
        assert!((SIMILARITY_THRESHOLD - 0.75).abs() < f64::EPSILON);
    }

    /// A pair sitting **exactly** on the threshold is an edge: `corpus/REPORT.md` says
    /// `Jaccard >= 0.75`, and the boundary is part of that measured contract.
    ///
    /// Nothing else in this suite can fail when `>=` becomes `>`: every other positive fixture
    /// scores 0.800 or 1.000. The boundary is not a curiosity — counted with exact rationals over
    /// six corpus repositories, **33 of 397** near-findings (8.3%) land on exactly 3/4, among them
    /// real cross-file copied docstrings (`langgraph`, 60 shared grams of 80).
    ///
    /// Nine words over two lines differing only in the final word: seven shingles each, six shared,
    /// eight in the union, so 6/8 = 0.75 exactly and no float rounding decides the comparison.
    #[test]
    fn a_pair_exactly_on_the_threshold_is_a_cluster() {
        // Arrange
        let left = "# the retry budget here is small\n# because upstream throttles\n";
        let right = "# the retry budget here is small\n# because upstream waits\n";
        let blocks = corpus(&[("left.py", left), ("right.py", right)]);
        assert_eq!(blocks.len(), 2, "got {blocks:?}");

        // Act
        let report = duplicates(&blocks);

        // Assert — the score first, so a fixture that drifts off the boundary fails loudly here
        // rather than silently turning this into a duplicate of the 0.800 test.
        assert_eq!(report.comparisons, 1, "the pair must have been scored");
        assert_eq!(report.clusters.len(), 1, "got {:?}", report.clusters);
        let score = report.clusters[0].weakest_score;
        assert!(
            (score - 0.75).abs() < 1e-12,
            "the fixture must sit exactly on the threshold, not near it; got {score}"
        );
    }

    /// Two blocks handed the same `path` and span but different text are **different blocks**, and
    /// both survive into the cluster.
    ///
    /// This is the regression test for the defect that made the output non-deterministic, carried
    /// over to the member list: the identity a member is deduplicated by is the coordinate **and**
    /// the normalised text. Keying it on the coordinate alone silently discards one of these two
    /// `a.py` blocks — under the pair model that dropped a real finding (measured: 400 runs in one
    /// process produced two different outputs, `similarity 0.769` and `similarity 0.909`), and
    /// under the cluster model it drops a real address, which is the same defect one layer down.
    ///
    /// Note what the assertion pins: **three** members from three distinct blocks.
    #[test]
    fn blocks_sharing_a_coordinate_but_not_a_text_are_distinct_members() {
        // Arrange — one path label, two different sources, so two blocks land on `a.py` lines 1-2.
        let base =
            "# the retry budget here is deliberately small\n# because upstream service rate limits";
        let blocks = corpus(&[
            ("a.py", &format!("{base} us often enough\n")),
            ("a.py", &format!("{base} us\n")),
            ("b.py", &format!("{base}\n")),
        ]);
        assert_eq!(blocks.len(), 3, "got {blocks:?}");
        assert_eq!(
            (blocks[0].line_start, blocks[0].line_end),
            (blocks[1].line_start, blocks[1].line_end),
            "the two a.py blocks must share a coordinate, or this proves nothing"
        );
        assert_ne!(blocks[0].normalized, blocks[1].normalized);

        // Act — twice, because equal keys are exactly where hash order used to leak through.
        let report = duplicates(&blocks);
        let first = render(&report);
        let second = render(&duplicates(&blocks));

        // Assert
        assert_eq!(report.clusters.len(), 1, "got {:?}", report.clusters);
        assert_eq!(
            report.clusters[0].members.len(),
            3,
            "three distinct blocks, three addresses:\n{first}"
        );
        assert_eq!(first, second);
        // 0.769 is the weakest of the three edges (the other two are 0.846 and 0.909), so the
        // cluster names it — a cluster whose weakest link came off a spanning tree or off the
        // maximum would print one of the other two.
        assert!(
            first.contains("0.769"),
            "the weakest edge of the component is 0.769:\n{first}"
        );
    }

    /// An exact group and a near duplicate of it are **one** cluster, not two findings — the exact
    /// and near paths feed one union-find, and this is the only test that proves they meet.
    ///
    /// Two halves, and each is a distinct defect. The exclusion in [`candidate_pairs`] skips a
    /// candidate only when **both** ends share one group; comparing the lower end's group with
    /// itself instead would skip every candidate whose lower end has any twin at all, dropping
    /// `c.py` out of the cluster entirely while every other test stayed green (the exact-group
    /// fixture has no non-twin in it, and the near-duplicate fixtures have no exact group). And if
    /// the two paths kept separate component sets, `a.py`/`b.py` and `c.py` would come out as two
    /// findings naming the same rationale — which is the whole defect this task exists to remove.
    #[test]
    fn an_exact_group_member_is_still_compared_with_a_non_twin() {
        // Arrange — `a.py` and `b.py` are exact twins after normalisation; `c.py` is a near
        // duplicate of both, above the threshold but not identical.
        let twin = "# the retry budget here is deliberately small, and that matters because\n\
                    # the upstream service rate limits us on every fourth call.\n";
        let rewrapped = "#   the retry budget here is deliberately small, and that matters because\n\
                         #\tthe upstream service rate limits us on every fourth call.\n";
        let near = "# the retry budget here is deliberately small, and that matters because\n\
                    # the upstream service rate limits us on every fourth request.\n";
        let blocks = corpus(&[("a.py", twin), ("b.py", rewrapped), ("c.py", near)]);
        assert_eq!(blocks.len(), 3, "got {blocks:?}");
        assert_eq!(
            blocks[0].normalized, blocks[1].normalized,
            "a.py and b.py must be an exact group"
        );
        assert_ne!(blocks[0].normalized, blocks[2].normalized);

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(
            report.clusters.len(),
            1,
            "the exact group and its near duplicate are one finding; got {:?}",
            report.clusters
        );
        assert_eq!(
            report.clusters[0]
                .members
                .iter()
                .map(|member| member.path.as_path())
                .collect::<Vec<_>>(),
            vec![Path::new("a.py"), Path::new("b.py"), Path::new("c.py")],
            "all three addresses, or the near block was dropped"
        );
        assert_eq!(
            report.comparisons, 2,
            "exactly the two mixed pairs are scored; the twin pair is arithmetic"
        );
    }

    /// `jaccard` is intersection over **union**, proved on sets of different sizes.
    ///
    /// Every other fixture compares equal-cardinality shingle sets, where reading the same side's
    /// length twice — `right.len() + right.len() - intersection` — gives the same answer. On a
    /// subset it does not: this pair would score 1.0 instead of 10/14, inventing a finding out of a
    /// block and its own prefix.
    #[test]
    fn jaccard_is_intersection_over_union_for_unequal_sets() {
        // Arrange — sixteen words and their own twelve-word prefix.
        let words: Vec<String> = (0..16).map(|index| format!("word{index}")).collect();
        let long = normalize(&words.join(" "));
        let short = normalize(&words[..12].join(" "));
        let long_set = shingles(&long);
        let short_set = shingles(&short);
        assert_eq!(
            (long_set.len(), short_set.len()),
            (14, 10),
            "14 and 10 shingles"
        );
        assert_eq!(
            short_set
                .iter()
                .filter(|gram| long_set.binary_search(gram).is_ok())
                .count(),
            10,
            "the prefix's shingles are a subset"
        );

        // Act
        let score = jaccard(&long_set, &short_set);

        // Assert — 10/14, and therefore BELOW the threshold: a false 1.0 would be a false finding.
        assert!((score - 10.0 / 14.0).abs() < 1e-12, "got {score}");
        assert!(score < SIMILARITY_THRESHOLD);
    }

    /// [`shingles`] returns its grams **sorted and duplicate-free**, which is the precondition
    /// [`jaccard`]'s merge is built on.
    ///
    /// This is the guard the representation change owes. `jaccard` no longer hashes: it walks two
    /// sequences in step and stops advancing a side when its head compares greater. Hand it an
    /// unsorted side and it silently returns an intersection that is too *small* — a lower score,
    /// a pair pushed under the threshold, a finding that quietly stops being reported.
    ///
    /// # Which half of this each mutation actually proves, measured rather than claimed
    ///
    /// The two halves are **not** equally defended, and saying so is the point of writing it down:
    ///
    /// * deleting `grams.sort_unstable()` reddens **four** tests — this one,
    ///   `jaccard_is_intersection_over_union_for_unequal_sets`,
    ///   `a_rationale_written_twice_in_two_files_is_one_cluster` and
    ///   `members_are_ordered_by_what_they_say_when_the_coordinate_ties`. So the sort is already
    ///   defended behaviourally, and the third of those is the one that matters: it is a real
    ///   cross-file finding disappearing. This test's contribution there is a *diagnosis* — it
    ///   names the cause instead of leaving a reader to infer it from a missing cluster.
    /// * deleting `grams.dedup()` reddens **only this test**. That half has no other guard at all,
    ///   and it is not cosmetic: `jaccard` computes the union as `|A| + |B| - |A n B|`, which is
    ///   the size of the union only when neither side counts a gram twice. A repeated phrase would
    ///   inflate both lengths, deflate every score containing it, and drop findings silently.
    ///
    /// That asymmetry is why this test exists as its own test rather than as an assertion bolted
    /// onto a clustering fixture.
    #[test]
    fn shingles_are_sorted_and_duplicate_free() {
        // Arrange — a text whose grams are emitted OUT of sorted order by the window walk, and one
        // phrase repeated so the dedup has something to remove.
        let text = normalize("zebra yak xray whale zebra yak xray");
        let grams = shingles(&text);

        // Assert
        assert!(
            grams.windows(2).all(|pair| pair[0] < pair[1]),
            "shingles must come back strictly ascending — sorted AND deduplicated; got {grams:?}"
        );
        // Strictly ascending already implies no duplicates, so this second assertion is about the
        // COUNT: it pins that dedup removed the repeat rather than that the repeat never existed.
        let distinct: std::collections::HashSet<Shingle<'_>> = grams.iter().copied().collect();
        assert_eq!(
            grams.len(),
            distinct.len(),
            "the repeated phrase must be carried once"
        );
        assert!(
            grams.len() < 5,
            "the fixture must actually contain a repeat, or the dedup half proves nothing; got {}",
            grams.len()
        );
    }

    /// An empty shingle set scores 0.0 against anything — `corpus/measure.py:313-318`.
    ///
    /// Tested directly, because [`duplicates`] cannot route a pair *into* it: `candidate_pairs`
    /// indexes only real shingles, so it never emits a pair with an empty side. An empty side is
    /// otherwise entirely reachable, and this test is the wrong place to say otherwise — a block
    /// whose [`crate::extract::ProseBlock::narrative`] is empty has an empty set, and
    /// [`crate::extract::MIN_BLOCK_WORDS`] cannot prevent it, because that constant counts the
    /// whole block and this function is handed the narrative.
    ///
    /// The empty branch is what makes such a block silent rather than a duplicate of every other
    /// silent block. Returning 1.0 there — the one-token mutation — would make every block with no
    /// narrative a duplicate of everything.
    #[test]
    fn jaccard_is_zero_when_either_side_has_no_shingles() {
        // Arrange — under SHINGLE_K = 3 a two-word text has no shingle at all.
        let empty = shingles("two words");
        let text = normalize("the retry budget here is deliberately small");
        let full = shingles(&text);
        assert!(
            empty.is_empty(),
            "a text shorter than SHINGLE_K has no shingles"
        );
        assert!(!full.is_empty());

        // Act & Assert — all three directions, including empty against empty.
        assert!(jaccard(&empty, &full).abs() < f64::EPSILON);
        assert!(jaccard(&full, &empty).abs() < f64::EPSILON);
        assert!(jaccard(&empty, &empty).abs() < f64::EPSILON);
    }

    /// Cluster order is decided by `line_start` — isolated, so a key that dropped it fails here.
    ///
    /// The three ordering fixtures below each vary **one** coordinate component and arrange the
    /// blocks' texts so that losing that component flips the order deterministically rather than
    /// leaving it to hash iteration. Correlated fixtures are exactly why the `line_end` mutation
    /// used to survive: comment runs generally have `line_end == line_start + 1`.
    ///
    /// These three assert on multi-line rendered output with an inline literal rather than with
    /// `assert_debug_snapshot!`, which is the crate's convention elsewhere and what
    /// `test-snapshot-testing` prescribes. The deviation is deliberate and local to ordering
    /// proofs: what is under test is *which line comes first*, and an inline literal puts the
    /// expected order directly beside the fixture that produces it, where a reader can check the
    /// two against each other. A `.snap` file moves that one indirection away and turns a review
    /// of the ordering into a review of a diff. Behavioural output stays on snapshots.
    #[test]
    fn findings_are_ordered_by_line_start() {
        // Arrange — "apple" starts later than "zebra", so text order and line order disagree.
        let late = "zebra rationale about the retry budget and the upstream rate limit here";
        let early = "apple rationale about the queue depth and the downstream batch size here";
        let blocks = vec![
            block("a.py", 5, 6, ProseKind::Comment, late),
            block("z.py", 1, 2, ProseKind::Comment, late),
            block("a.py", 1, 9, ProseKind::Comment, early),
            block("b.py", 1, 2, ProseKind::Comment, early),
        ];

        // Act
        let rendered = render(&duplicates(&blocks));

        // Assert — a.py:1 before a.py:5, whatever the texts sort like. The spans are now visible
        // in the probe (`1-9` against `5-6`), which is strictly more than it used to observe.
        assert_eq!(
            rendered,
            "a.py:1-9, b.py:1-2: duplicate prose (weakest a.py:1-9 ~ b.py:1-2, similarity 1.000)\n\
             a.py:5-6, z.py:1-2: duplicate prose (weakest a.py:5-6 ~ z.py:1-2, similarity 1.000)\n"
        );
    }

    /// Cluster order is decided by `line_end` when `line_start` ties — isolated.
    #[test]
    fn findings_are_ordered_by_line_end_when_the_start_ties() {
        // Arrange — both findings start at a.py:1; only the end separates them, and the texts sort
        // the opposite way, so a key without `line_end` puts them in the other order.
        let longer_span = "zebra rationale about the retry budget and the upstream rate limit here";
        let shorter_span =
            "apple rationale about the queue depth and the downstream batch size here";
        let blocks = vec![
            block("a.py", 1, 2, ProseKind::Comment, longer_span),
            block("z.py", 1, 2, ProseKind::Comment, longer_span),
            block("a.py", 1, 3, ProseKind::Comment, shorter_span),
            block("b.py", 1, 2, ProseKind::Comment, shorter_span),
        ];

        // Act
        let rendered = render(&duplicates(&blocks));

        // Assert — the block ending on line 2 comes first. Note what changed here: this probe
        // ordered by `line_end` while rendering only `line_start`, so both rows read `a.py:1` and
        // the field under test was invisible in the bytes being compared. `1-2` before `1-3` now
        // shows it. Sharing one address renderer made a probe strictly more discriminating.
        assert_eq!(
            rendered,
            "a.py:1-2, z.py:1-2: duplicate prose (weakest a.py:1-2 ~ z.py:1-2, similarity 1.000)\n\
             a.py:1-3, b.py:1-2: duplicate prose (weakest a.py:1-3 ~ b.py:1-2, similarity 1.000)\n"
        );
    }

    /// Cluster order is decided by `kind` when the whole span ties — isolated.
    ///
    /// Nothing else separates a docstring from a comment run reported at the same span, so without
    /// `kind` the key is not a total order over these two findings.
    #[test]
    fn findings_are_ordered_by_kind_when_the_span_ties() {
        // Arrange — same path, same span, different kind; texts again sort the opposite way.
        let doc_text = "zebra rationale about the retry budget and the upstream rate limit here";
        let comment_text =
            "apple rationale about the queue depth and the downstream batch size here";
        let blocks = vec![
            block("a.py", 1, 2, ProseKind::Docstring, doc_text),
            block("z.py", 1, 2, ProseKind::Docstring, doc_text),
            block("a.py", 1, 2, ProseKind::Comment, comment_text),
            block("b.py", 1, 2, ProseKind::Comment, comment_text),
        ];

        // Act
        let rendered = render(&duplicates(&blocks));

        // Assert — `Docstring` precedes `Comment` in the enum, so its cluster comes first.
        assert_eq!(
            rendered,
            "a.py:1-2, z.py:1-2: duplicate prose (weakest a.py:1-2 ~ z.py:1-2, similarity 1.000)\n\
             a.py:1-2, b.py:1-2: duplicate prose (weakest a.py:1-2 ~ b.py:1-2, similarity 1.000)\n"
        );
    }

    /// AC1 — the first twenty findings cover twenty different rationales, not two.
    ///
    /// This is the hermetic half of AC1, and what it reproduces is a *shape*, not a repository:
    /// output ordered by `(path, line)` puts all the pairs of one component next to each other, so
    /// the first page of a pair-shaped report is one component repeated. Measured on the real
    /// `langgraph` checkout before this task: 682 findings, of which the **first 20 cover 2**
    /// distinct normalised texts — two docstrings copied across `cache/base|memory|redis|sqlite`.
    /// That is what makes the labelled sample of `validate-detectors-on-reference-corpus` two human
    /// judgements counted twenty times.
    ///
    /// The fixture is 25 families of one rationale in five files (four byte-identical copies and one
    /// one-word variant), laid out so one family's files are adjacent in path order. On the pair
    /// model its first 20 findings covered **4** distinct texts, i.e. two families; on clusters the
    /// first 20 findings are 20 whole families. The bar is `>= 18` because that is the AC; a result
    /// sitting *on* 18 would mean something is wrong, so the assertion prints the real number.
    ///
    /// It cannot read `corpus/checkouts/` — that tree is git-ignored and absent in CI, so a test
    /// that reads it would pass by finding nothing. The real run belongs to a probe binary outside
    /// `cargo test`, and its output belongs in the task report.
    #[test]
    fn the_first_twenty_findings_cover_twenty_different_rationales() {
        // Arrange
        let families = 25;
        let blocks = family_corpus(families);

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(
            report.clusters.len(),
            families,
            "one cluster per family, or the fixture is not the shape under test"
        );
        // Every family is four identical copies plus one near variant, so a whole cluster is five
        // members carrying two distinct texts. Pinned as an equality, not as the AC's `>= 18`
        // floor: the near variant is joined to its family by the near path, and the exclusion in
        // `candidate_pairs` that keeps exact twins out of it is one comparison away from dropping
        // every variant instead. That regression leaves 20 texts — over the floor, and green —
        // which is precisely the "result sitting on the boundary" the task warns is a symptom.
        assert!(
            report
                .clusters
                .iter()
                .all(|cluster| cluster.members.len() == 5),
            "every family is one cluster of five, exact copies and near variant together"
        );
        let texts: HashSet<&str> = report
            .clusters
            .iter()
            .take(20)
            .flat_map(|cluster| {
                cluster
                    .members
                    .iter()
                    .map(|member| member.normalized.as_str())
            })
            .collect();
        assert_eq!(
            texts.len(),
            40,
            "the first 20 findings must cover 2 texts each, and the AC's floor is 18"
        );
    }

    /// AC2 — a licence header that differs by a single token across 1 000 files is **one** finding.
    ///
    /// The case that decided the shape of this module, and the one the six-repository corpus does
    /// not contain. It has **no exact group at all** — one token differs, so every pair goes down
    /// the near path — which is why collapsing only exact groups was measured and rejected: 1 000
    /// files gave 499 500 findings, 95 MB resident, and a rendered report nobody can read.
    ///
    /// What this does **not** fix, deliberately: the 499 500 Jaccard calls are still made. Candidate
    /// generation is a separate problem, recorded as a risk against the wall-clock budget of
    /// `validate-detectors-on-reference-corpus`. That cost is why this one test is essentially the
    /// whole `--lib` wall clock, and it is the honest price of covering the case at its real size.
    ///
    /// Measured 2026-08-01 at `b7c8ad9`, clean tree, macOS/arm64: **about 1.8 s** for this test and
    /// **about 1.85 s** for all 132. Two independent sets of runs spanned 1.76–1.85 s and
    /// 1.83–1.86 s, so the two are within run-to-run noise of each other on a loaded machine and a
    /// re-run landing at 1.84 s is noise, not a regression — what survives the noise is the
    /// relationship above, not the digits. The `~15 s` that stood here and the `8.68 s` banked in
    /// the rust-skills issue are both superseded, and at ~1.8 s the cost does not buy an
    /// `#[ignore]`, which would take the AC2 guard out of the default run.
    #[test]
    fn a_near_identical_header_in_a_thousand_files_is_one_finding() {
        // Arrange
        let files = 1_000;
        let blocks = near_header_corpus(files);

        // Act
        let report = duplicates(&blocks);

        // Assert
        // AC2 is written as `<= 2`, and this pins the stronger truth: exactly one. The AC's slack
        // was never reachable here — the next assertion puts all 1 000 files in `clusters[0]`, so
        // a second cluster cannot exist — and an assertion that cannot bind is one a regression
        // walks past.
        assert_eq!(
            report.clusters.len(),
            1,
            "got {} findings",
            report.clusters.len()
        );
        assert_eq!(
            report.clusters[0].members.len(),
            files,
            "one finding is only right if it names every file"
        );
        assert_eq!(
            report.comparisons,
            files * (files - 1) / 2,
            "the pairwise comparisons are NOT what this task removes; if this number moved, \
             candidate generation changed and that belongs to another task"
        );
    }

    /// AC3 — a byte-identical licence header across 2 000 files is one finding with 2 000
    /// addresses, and still costs zero comparisons.
    ///
    /// Measured on the pair model: 1 999 000 findings and 118 MB of rendered output for one licence.
    /// The rendered size is asserted too, because "one finding" is worthless if the line it prints
    /// is still quadratic.
    #[test]
    fn an_identical_header_in_two_thousand_files_is_one_finding() {
        // Arrange
        let files = 2_000;
        let blocks = exact_header_corpus(files);

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(report.clusters.len(), 1, "got {}", report.clusters.len());
        assert_eq!(report.clusters[0].members.len(), files);
        assert_eq!(
            report.comparisons, 0,
            "identical text is connected arithmetically"
        );
        assert!(
            render(&report).len() < 100_000,
            "one licence header must not render as megabytes; got {} bytes",
            render(&report).len()
        );
    }

    /// `families` families of one rationale copied into several files.
    ///
    /// The paths are `pkg/fam{family}/…` so that one family's blocks are adjacent in the
    /// `(path, line)` order the output is sorted by — which is the whole mechanism behind AC1's
    /// degeneracy. Four copies are byte-identical and the fifth differs in its last word (17 of 19
    /// shingles shared, J = 0.895), so each family exercises the exact path and the near path at
    /// once and comes out as one cluster of five.
    fn family_corpus(families: usize) -> Vec<ProseBlock> {
        const COPIES: usize = 4;
        const WORDS: usize = 20;

        let mut blocks = Vec::with_capacity(families * (COPIES + 1));
        for family in 0..families {
            let mut base = String::new();
            for word in 0..WORDS - 1 {
                write!(base, "fam{family}word{word} ").expect("writing into a String cannot fail");
            }
            let identical = format!("{base}fam{family}word{}", WORDS - 1);
            let variant = format!("{base}fam{family}variant");
            for copy in 0..COPIES {
                blocks.push(block(
                    &format!("pkg/fam{family:02}/cache{copy}.py"),
                    1,
                    3,
                    ProseKind::Comment,
                    &identical,
                ));
            }
            blocks.push(block(
                &format!("pkg/fam{family:02}/variant.py"),
                1,
                3,
                ProseKind::Comment,
                &variant,
            ));
        }
        blocks
    }

    /// `n` files carrying one licence header that differs in a single token.
    fn near_header_corpus(n: usize) -> Vec<ProseBlock> {
        (0..n)
            .map(|file| {
                block(
                    &format!("pkg/m{file}.py"),
                    1,
                    3,
                    ProseKind::Comment,
                    &format!(
                        "Copyright 2026 Example Corporation licensed under the Apache License \
                         Version two you may not use the module m{file} in this distribution \
                         except in compliance with that license and you must include a copy of it \
                         with every redistribution"
                    ),
                )
            })
            .collect()
    }

    /// `n` files carrying a byte-identical licence header.
    fn exact_header_corpus(n: usize) -> Vec<ProseBlock> {
        (0..n)
            .map(|file| {
                block(
                    &format!("pkg/m{file}.py"),
                    1,
                    3,
                    ProseKind::Comment,
                    "Copyright 2026 Example Corporation licensed under the Apache License Version \
                     two you may not use the files in this distribution except in compliance with \
                     that license and you must include a copy of it with every redistribution",
                )
            })
            .collect()
    }

    /// [`exact_groups`] labels only real twins, and centres the group on its **smallest** member —
    /// smallest by coordinate, not by arrival.
    ///
    /// The labelling half is a documented claim no behaviour test can reach, because getting it
    /// wrong is inert: a singleton labelled `Some(itself)` still differs from every other label, and
    /// a group identified by its second member is still equal within the group. The *smallest* half
    /// is not inert at all — the representative is the centre of the star of edges this records, and
    /// every one of those edges scores 1.0, so it decides which pair the `weakest` of an all-exact
    /// cluster names. Centring on the first arrival makes that output a function of the input order.
    /// The fixture therefore hands the twins in in **descending** path order, so an arrival-order
    /// centre answers `Some(0)`.
    #[test]
    fn an_exact_group_labels_only_twins_and_centres_on_the_smallest_member() {
        // Arrange — two exact twins handed in as b.py then a.py, plus one unrelated block.
        let twin = "# the retry budget here is deliberately small, and that matters because\n\
                    # the upstream service rate limits us on every fourth call.\n";
        let lone = "# the queue depth is bounded on purpose, and that matters because\n\
                    # an unbounded queue turns a slow consumer into a memory leak.\n";
        let blocks = corpus(&[("b.py", twin), ("a.py", twin), ("c.py", lone)]);
        assert_eq!(blocks.len(), 3, "got {blocks:?}");

        // Act
        let mut components = Components::new(blocks.len());
        let group_of = exact_groups(&blocks, &mut components);

        // Assert
        assert_eq!(
            group_of,
            vec![Some(1), Some(1), None],
            "twins carry the member with the SMALLEST coordinate — a.py, at position 1 — and a \
             block with no twin carries None"
        );
        assert_eq!(
            components.into_clusters(&blocks).len(),
            1,
            "the twins are connected and the lone block is not"
        );
    }

    /// AC5, first half — a cluster that is **not** a clique says so, by naming its weakest link.
    ///
    /// `a.py ~ b.py` and `b.py ~ c.py`, but `a.py !~ c.py`: three blocks, two edges of the three
    /// possible, and all three land in one finding. That is the transitivity clustering asserts and
    /// Jaccard does not have — measured at 12 of 646 components (1.9%) on the corpus — and the
    /// mitigation the user's decision made mandatory is that a loose cluster *looks* loose. The
    /// scores are pinned exactly, so a weakest link taken as the maximum (0.833) or as a mean
    /// (0.797) fails here rather than merely printing a prettier number.
    #[test]
    fn a_cluster_that_is_not_a_clique_names_its_weakest_link() {
        // Arrange — 24 distinct words, i.e. 22 shingles each. `left` differs from `middle` in words
        // 0-1 (2 shingles), `right` in words 21-23 (3 shingles), and the two edits are far enough
        // apart that no shingle carries both:
        //   left ~ middle = 20/24 = 0.833   middle ~ right = 19/25 = 0.760
        //   left ~ right  = 17/27 = 0.630 — below the threshold, so there is no third edge.
        let words: Vec<String> = (0..24).map(|index| format!("word{index}")).collect();
        let mut left = words.clone();
        left[0] = "alpha".to_owned();
        left[1] = "beta".to_owned();
        let mut right = words.clone();
        right[21] = "gamma".to_owned();
        right[22] = "delta".to_owned();
        right[23] = "epsilon".to_owned();
        let blocks = vec![
            block("a.py", 1, 2, ProseKind::Comment, &left.join(" ")),
            block("b.py", 1, 2, ProseKind::Comment, &words.join(" ")),
            block("c.py", 1, 2, ProseKind::Comment, &right.join(" ")),
        ];

        // Act
        let report = duplicates(&blocks);

        // Assert
        assert_eq!(report.clusters.len(), 1, "got {:?}", report.clusters);
        let cluster = &report.clusters[0];
        assert_eq!(cluster.members.len(), 3, "the middle block joins both ends");
        assert!(
            (cluster.weakest_score - 19.0 / 25.0).abs() < 1e-12,
            "the weaker of the two edges is 0.760, not 0.833; got {}",
            cluster.weakest_score
        );
        assert_eq!(
            (
                cluster.weakest.0.path.as_path(),
                cluster.weakest.1.path.as_path()
            ),
            (Path::new("b.py"), Path::new("c.py")),
            "the weakest link is named end to end, not merely scored"
        );
    }

    /// AC5, second half — the weakest link is the minimum over **every** edge of the component, not
    /// over the edges that happened to merge it.
    ///
    /// Union-find merges in arrival order, so the edge that joined two components is whichever came
    /// first; an implementation that keeps only those prints a cluster tighter than it is. Measured
    /// on the corpus, a spanning-tree minimum overstates the real one on 5 of 6 repositories, worst
    /// case 0.059 on `langgraph`. The half of AC5 above **cannot** catch it: `a ~ b ~ c` has two
    /// edges and every spanning tree of it contains both.
    ///
    /// So this drives `Components` directly. Through [`duplicates`] the arrival order is the
    /// iteration order of a `HashSet` and cannot be chosen, which would leave the very thing under
    /// test — *which* edge merged the component — to a coin flip. Here the two strong edges arrive
    /// first and merge all three blocks, so the weak edge lands inside an already-merged component:
    /// exactly the case a spanning-tree minimum misses.
    #[test]
    fn the_weakest_link_is_the_minimum_over_all_edges_not_over_the_merging_ones() {
        // Arrange — a triangle: strong a-b, strong a-c, weak b-c, in that order.
        let text = "the retry budget here is deliberately small and that matters a great deal";
        let blocks = vec![
            block("a.py", 1, 2, ProseKind::Comment, text),
            block("b.py", 1, 2, ProseKind::Comment, text),
            block("c.py", 1, 2, ProseKind::Comment, text),
        ];
        let mut components = Components::new(blocks.len());

        // Act
        components.connect(0, 1, 0.90, &blocks);
        components.connect(0, 2, 0.88, &blocks);
        components.connect(1, 2, 0.76, &blocks);
        let clusters = components.into_clusters(&blocks);

        // Assert
        assert_eq!(clusters.len(), 1, "all three are one component");
        assert!(
            (clusters[0].weakest_score - 0.76).abs() < 1e-12,
            "0.76 is an edge of this component; a spanning-tree minimum reports 0.88. Got {}",
            clusters[0].weakest_score
        );
        assert_eq!(
            (
                clusters[0].weakest.0.path.as_path(),
                clusters[0].weakest.1.path.as_path()
            ),
            (Path::new("b.py"), Path::new("c.py")),
            "and it names the weak edge's own ends"
        );
    }
}
