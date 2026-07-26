"""
Guards for the duplicate machinery in `corpus/measure.py`.

Two of the four constants in `corpus/REPORT.md` (the similarity threshold and the
min-block-size conjunction) are read off `duplicate_stats()`. Before this file existed the
whole function was untested: replacing its body with `return DuplicateStats()` left the
suite green while every threshold table went to zero. So each expected value below is
hand-computed from the block literals in the test, never recomputed the way the code
computes it.

Run: make test
"""

from __future__ import annotations

import measure
import pytest


def block(text: str, *, lines: int = 1, path: str = "a.py", start: int = 1) -> measure.ProseBlock:
    """Build a normalised block spanning `lines` physical lines."""
    return measure.ProseBlock("comment", path, start, start + lines - 1, text, text)


# 8 words, so it clears the decided 8-word half of the cutoff on its own.
EIGHT_WORDS: str = "alpha beta gamma delta epsilon zeta eta theta"


class TestExactGrouping:
    """Exact-duplicate grouping is the uncapped, precise half of the pass."""

    def test_three_identical_blocks_form_one_group_of_three_pairs(self) -> None:
        """C(3,2) = 3 pairs in 1 group — hand-counted, not derived from n*(n-1)/2 here."""
        stats = measure.duplicate_stats([block(EIGHT_WORDS, path=f"f{i}.py") for i in range(3)])
        assert stats.exact_groups == 1
        assert stats.exact_pairs == 3

    def test_distinct_texts_produce_no_exact_group(self) -> None:
        """Different prose is not a duplicate, however similar the shape."""
        stats = measure.duplicate_stats([block("alpha beta gamma"), block("delta epsilon zeta")])
        assert stats.exact_groups == 0
        assert stats.exact_pairs == 0

    def test_exact_pairs_are_bucketed_by_the_duplicated_block_size(self) -> None:
        """
        A 2-line duplicate lands in the 2-line bucket, not the 1-line one.

        REPORT.md's "one-line blocks are where the noise lives" table is this dict; if the
        size key were wrong, the min-block-size argument would be reading the wrong row.
        """
        stats = measure.duplicate_stats([block(EIGHT_WORDS, lines=2, path=f"f{i}.py") for i in range(2)])
        assert stats.exact_pairs_by_size == {2: 1}


class TestCandidateGeneration:
    """Near-duplicate candidates come from the inverted shingle index."""

    def test_blocks_sharing_a_shingle_become_a_candidate_pair(self) -> None:
        """`alpha beta gamma` is a shared 3-gram, so the pair is generated."""
        stats = measure.duplicate_stats([block("alpha beta gamma delta"), block("alpha beta gamma epsilon")])
        assert stats.candidate_pairs == 1

    def test_blocks_sharing_no_shingle_are_never_paired(self) -> None:
        """No shared 3-gram means the pair is never scored — the index is the filter."""
        stats = measure.duplicate_stats([block("alpha beta gamma"), block("delta epsilon zeta")])
        assert stats.candidate_pairs == 0


class TestJaccard:
    """The similarity metric the 0.75 decision is stated in."""

    def test_jaccard_on_a_hand_computed_pair(self) -> None:
        """
        Hand-computed: shingles(k=3) of the two texts share 1 of 3 distinct grams.

        A = {(alpha,beta,gamma), (beta,gamma,delta)}
        B = {(alpha,beta,gamma), (beta,gamma,epsilon)}
        intersection 1, union 3 -> 1/3.
        """
        a = measure.shingles("alpha beta gamma delta")
        b = measure.shingles("alpha beta gamma epsilon")
        assert measure.jaccard(a, b) == pytest.approx(1 / 3)

    def test_identical_texts_score_exactly_one(self) -> None:
        """An exact copy must be 1.0, or no threshold in the grid means what it says."""
        a = measure.shingles(EIGHT_WORDS)
        assert measure.jaccard(a, a) == pytest.approx(1.0)

    def test_empty_side_scores_zero_rather_than_dividing_by_zero(self) -> None:
        """Empty prose is not similar to anything."""
        assert measure.jaccard(frozenset(), measure.shingles(EIGHT_WORDS)) == 0.0


class TestConjunctionCutoff:
    """The decided min-block-size: >= 2 physical lines AND >= 8 normalised words."""

    def test_pair_meeting_both_halves_is_counted(self) -> None:
        """2 lines and 8 words at J=1.0 clears the cutoff."""
        stats = measure.duplicate_stats([block(EIGHT_WORDS, lines=2, path=f"f{i}.py") for i in range(2)])
        assert stats.conjunction[(2, 8, 1.0)] == 1

    def test_one_line_pair_is_excluded_however_many_words(self) -> None:
        """The line half must bite: 8 words on ONE line is the copy-paste noise class."""
        stats = measure.duplicate_stats([block(EIGHT_WORDS, lines=1, path=f"f{i}.py") for i in range(2)])
        assert stats.conjunction[(2, 8, 1.0)] == 0
        assert stats.conjunction[(1, 1, 1.0)] == 1

    def test_short_pair_is_excluded_however_many_lines(self) -> None:
        """The word half must bite: this is the `Init API instance.` class, 3 words / 3 lines."""
        stats = measure.duplicate_stats([block("init api instance", lines=3, path=f"f{i}.py") for i in range(2)])
        assert stats.conjunction[(2, 8, 1.0)] == 0
        assert stats.conjunction[(1, 1, 1.0)] == 1


class TestDocumentFrequencyCapDoesNotHideDuplicates:
    """
    A cap you cannot see is a cap that lies (review finding A2).

    With a document-frequency cap of 40, 41 identical blocks put every one of their shingles
    in an over-cap bucket, so every bucket was skipped and the threshold tables reported ZERO
    near-duplicate pairs while exact grouping reported 820. The most-duplicated content — the
    highest-value signal a duplicate detector exists to find — was the content most certain to
    vanish. Exact groups must therefore reach the tables regardless of the cap.
    """

    def test_forty_one_identical_blocks_reach_the_threshold_tables(self) -> None:
        """C(41,2) = 820 pairs, hand-computed, must appear at the decided cutoff."""
        blocks = [block(EIGHT_WORDS, lines=2, path=f"f{i}.py") for i in range(41)]
        stats = measure.duplicate_stats(blocks)
        assert stats.exact_pairs == 820
        assert stats.conjunction[(2, 8, 1.0)] == 820
        assert stats.conjunction[(2, 8, 0.75)] == 820

    def test_capped_buckets_and_their_block_mass_are_reported(self) -> None:
        """Whatever the cap still drops has to be visible next to the tables."""
        blocks = [block(EIGHT_WORDS, lines=2, path=f"f{i}.py") for i in range(41)]
        stats = measure.duplicate_stats(blocks)
        assert stats.capped_shingles > 0
        assert stats.capped_blocks == 41

    def test_exact_pairs_are_not_double_counted_when_under_the_cap(self) -> None:
        """Two identical blocks are ONE pair, not two: the analytic and index paths must not overlap."""
        stats = measure.duplicate_stats([block(EIGHT_WORDS, lines=2, path=f"f{i}.py") for i in range(2)])
        assert stats.exact_pairs == 1
        assert stats.conjunction[(2, 8, 1.0)] == 1
        assert stats.jaccard_grid[(2, 1.0)] == 1


class TestDifflibSampleIsUniform:
    """The metric-agreement sample must not be a lexicographic prefix (review finding B5)."""

    def test_the_sample_spans_the_whole_sorted_pair_list(self) -> None:
        """
        The sample must reach the LAST pair, not just the first DIFFLIB_SAMPLE of them.

        Built as 3 groups of 40 blocks (group = index % 3), each group sharing a group-specific
        opening 3-gram. Bucket size 40 sits exactly on the cap, so each group contributes
        C(40,2) = 780 index pairs: 2340 in total, above DIFFLIB_SAMPLE = 2000.

        Hand-derived expectation: pairs are (left, right) with left < right, both in one group.
        Group 2 holds indices 2, 5, ... 119, so its largest possible left is 116 (paired with
        119); groups 0 and 1 top out at 114 and 115. The global maximum left is therefore 116,
        and a sample that truly spans the list must reach it. A prefix cannot.
        """
        blocks = [
            block(f"phrase{i % 3} alpha beta gamma tail {i}", lines=2, path=f"{i:03d}_mod.py") for i in range(120)
        ]
        stats = measure.duplicate_stats(blocks)
        assert stats.candidate_pairs == 2340
        assert stats.difflib_sampled == measure.DIFFLIB_SAMPLE
        assert stats.sampled_max_left == 116


# Two texts differing only in their last word: 8 shingles each, 7 shared, 9 in the union, so
# J = 7/9 = 0.7778 — strictly between the 0.75 cutoff and 1.0. Verified by
# TestNearDuplicateConjunction::test_the_pair_really_is_a_near_duplicate below.
NEAR_A: str = "alpha beta gamma delta epsilon zeta eta theta iota kappa"
NEAR_B: str = "alpha beta gamma delta epsilon zeta eta theta iota lambda"

# Different texts with IDENTICAL shingle sets, because the phrase repeats: 6 words vs 5 words
# both reduce to {(alpha,beta,gamma), (beta,gamma,alpha), (gamma,alpha,beta)}. J = 1.0 while the
# texts differ, which is the only way to get a high score on a SHORT block — a single changed
# word in a 5-word text costs too many shingles to stay above 0.75.
SHORT_A: str = "alpha beta gamma alpha beta gamma"
SHORT_B: str = "alpha beta gamma alpha beta"


class TestNearDuplicateConjunction:
    """
    The conjunction on the NEAR-duplicate path, which no other test reaches.

    Every other conjunction test in this file builds *identical* blocks. Identical blocks form
    an exact group and are counted through the separate arithmetic path, so the near-duplicate
    loop contributed to no assertion at all: changing its `and` to `or` left all 57 tests green.
    That loop is not a corner case — 4 of an audited repository's 8 findings (the J=0.932 and
    J=0.941 pairs) and the whole 28/22/8 knee row flow through it, not through the exact path.

    Every test here asserts `exact_groups == 0`, which is what proves the pair travelled the
    near-duplicate loop rather than the arithmetic shortcut.
    """

    def test_the_pair_really_is_a_near_duplicate(self) -> None:
        """
        Guard the guard: these fixtures must be near-identical, not identical.

        If a future edit made them equal, every test below would silently start exercising the
        exact-group path again and stop testing the loop it exists to test.
        """
        assert NEAR_A != NEAR_B
        score = measure.jaccard(measure.shingles(NEAR_A), measure.shingles(NEAR_B))
        assert score == pytest.approx(7 / 9)
        assert measure.CALIBRATION_THRESHOLD < score < 1.0
        assert SHORT_A != SHORT_B
        assert measure.jaccard(measure.shingles(SHORT_A), measure.shingles(SHORT_B)) == pytest.approx(1.0)

    def test_near_duplicate_meeting_both_halves_is_counted(self) -> None:
        """2 lines and 10 words at J=0.778 clears `2ln/8w` — the positive control."""
        stats = measure.duplicate_stats([block(NEAR_A, lines=2, path="a.py"), block(NEAR_B, lines=2, path="b.py")])
        assert stats.exact_groups == 0
        assert stats.conjunction[(2, 8, 0.75)] == 1

    def test_near_duplicate_on_one_line_is_excluded_despite_having_the_words(self) -> None:
        """
        10 words but ONE line must not count at `2ln/8w`.

        This is the case that distinguishes `and` from `or`: the word half passes, the line half
        fails, so only a conjunction rejects it. With `or` this returns 1.
        """
        stats = measure.duplicate_stats([block(NEAR_A, lines=1, path="a.py"), block(NEAR_B, lines=1, path="b.py")])
        assert stats.exact_groups == 0
        assert stats.conjunction[(2, 8, 0.75)] == 0
        assert stats.conjunction[(1, 1, 0.75)] == 1

    def test_near_duplicate_that_is_too_short_is_excluded_despite_having_the_lines(self) -> None:
        """
        3 lines but 5 words must not count at `2ln/8w`.

        The mirror case: the line half passes, the word half fails. This is the
        `Init API instance.` shape — short prose spread over several lines — and again only a
        conjunction rejects it. With `or` this returns 1.
        """
        stats = measure.duplicate_stats([block(SHORT_A, lines=3, path="a.py"), block(SHORT_B, lines=3, path="b.py")])
        assert stats.exact_groups == 0
        assert stats.conjunction[(2, 8, 0.75)] == 0
        assert stats.conjunction[(1, 1, 0.75)] == 1

    def test_near_duplicate_below_the_threshold_is_not_counted_at_all(self) -> None:
        """
        A pair at J=0.778 must vanish from the >=0.9 column but survive at >=0.75.

        Pins that the near-duplicate loop reads the threshold too, not only the size cutoffs.
        """
        stats = measure.duplicate_stats([block(NEAR_A, lines=2, path="a.py"), block(NEAR_B, lines=2, path="b.py")])
        assert stats.conjunction[(2, 8, 0.75)] == 1
        assert stats.conjunction[(2, 8, 0.9)] == 0
