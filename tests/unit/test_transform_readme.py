"""
Guards for `scripts/transform_readme.py`, the README rewriter the wheel/sdist build runs before
maturin reads `readme = "README.md"`.

`README.md` is written for GitHub, where a relative address resolves against the repository. On the
PyPI project page there is no repository to resolve against, so every relative address 404s.

The four ways this script can leave the shop window broken are the four things tested here:

  1. **it can miss an address.** So the guard is not "the four links we thought of were rewritten":
     `TestTheRealReadmeIsFullyResolved` transforms the repository's own README and asserts that
     **zero** relative addresses survive, computed by re-scanning the output rather than by
     comparing against a list of today's filenames. A list would agree with a README that grew an
     eleventh link.
  2. **it can rewrite an address to somewhere that does not exist.** Every relative address is
     required to name a file that is actually in the checkout, so a typo or a moved document fails
     the build instead of shipping a link that 404s on GitHub too.
  3. **it can pass silently on a README that no longer has the expected shape.** This is the one
     `twine check` cannot see and the reason ruff's own transformer raises rather than warns: a
     README with no relative addresses at all means the document was restructured and this script
     has stopped doing anything. That is a build failure, not a no-op.
  4. **it can grade its own output.** A verification that re-runs the rewriting regex is blind
     exactly where the rewriter is blind — a reference-style definition and a single-quoted `href`
     both survive a run that reports success. `TestTheShapesTheSharedRegexWasBlindTo` pins those
     shapes and `TestTheVerifierDoesNotShareTheRewritersEyes` pins the split itself: the verifier
     renders the markdown the way PyPI does and reads the HTML, sharing no pattern with the
     rewriter.

`TestTheGuardIsWiredIntoTheEntryPoint` runs the script the way the workflow runs it, so a check
deleted from `main()` fails a test rather than nothing at all.

Run: make test
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest
from transform_readme import (
    BLOB,
    RAW,
    ReadmeNotInExpectedFormatError,
    _absolute,
    code_content,
    relative_addresses,
    rendered_addresses,
    transform,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "transform_readme.py"


def unresolvable_addresses(text: str) -> list[str]:
    """
    Return the relative addresses of `text` that do not name something in this repository.

    **One owner for the resolution, and it is the script's own.** This check used to spell its own
    predicate — `(REPO_ROOT / address).exists()` on the RAW address — which is the exact defect
    `_absolute` was fixed for: a `docs/cli-contract.md#exit-codes` link is accepted by the script
    and called missing here, so the artifact test would have blocked the release on a README the
    transformer is happy with. A second predicate for one question is how the fix gets applied in
    one place and its copy left behind.
    """
    missing: list[str] = []
    for address in relative_addresses(text):
        try:
            _absolute(address, REPO_ROOT)
        except ReadmeNotInExpectedFormatError as error:
            missing.append(f"{address}: {error}")
    return missing


class TestRelativeAddressesAreMadeAbsolute:
    """
    One case per address *syntax* the README actually uses, because the rewrite has to happen in
    markdown link targets and in HTML attributes alike — a regex that only knows `](...)` silently
    leaves the `<img src>` and the `<a href>` behind, which is how the image would still 404.
    """

    def test_a_relative_markdown_link_becomes_a_blob_url(self) -> None:
        text = "see the [CLI contract](docs/cli-contract.md).\n"

        assert transform(text, root=REPO_ROOT) == f"see the [CLI contract]({BLOB}docs/cli-contract.md).\n"

    def test_a_relative_image_source_becomes_a_raw_url(self) -> None:
        # Not the blob URL: GitHub serves `blob/` as an HTML page, so an <img> pointing at it
        # renders as a broken image on PyPI rather than as the cube.
        text = '<img src="assets/tooprolix.gif" width="128">\n'

        assert transform(text, root=REPO_ROOT) == f'<img src="{RAW}assets/tooprolix.gif" width="128">\n'

    def test_a_relative_html_href_becomes_a_blob_url(self) -> None:
        text = '<a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-12130f.svg" alt="MIT"></a>\n'

        assert transform(text, root=REPO_ROOT) == (
            f'<a href="{BLOB}LICENSE"><img src="https://img.shields.io/badge/license-MIT-12130f.svg" alt="MIT"></a>\n'
        )

    @pytest.mark.parametrize(
        ("address", "target"),
        [
            ("docs/cli-contract.md#exit-codes", "docs/cli-contract.md"),
            ("docs/rules-and-configuration.md?plain=1", "docs/rules-and-configuration.md"),
            ("docs/cli-contract.md?plain=1#exit-codes", "docs/cli-contract.md"),
        ],
    )
    def test_a_link_to_a_section_of_a_document_resolves_against_the_document(self, address: str, target: str) -> None:
        """
        One address, two parsers, and the second one decides — the compare-before-normalise shape.

        `_is_relative` is URL-aware (`urlsplit`, which is the whole argument of its docstring), so a
        `#fragment` or `?query` is correctly split off and the address is judged relative. `_absolute`
        then took the **raw** string and asked the filesystem for it, so `docs/cli-contract.md#x`
        was looked up as a file literally named `cli-contract.md#x`, found missing, and the build was
        stopped with a message blaming a file that is present.

        Measured 2026-08-01 through the real entry path on a copy of the repository README with one
        anchor added: `scripts/transform_readme.py` exited **1** with *"names no file in the
        repository"*. A section link is the most ordinary thing a README grows, and this script
        gates the README that goes to PyPI, so the false positive stops a release.

        The rewritten URL must keep the fragment: it is the reader's destination, and only the
        filesystem lookup has any business ignoring it.
        """
        text = f"see the [contract]({address}).\n"

        assert transform(text, root=REPO_ROOT) == f"see the [contract]({BLOB}{address}).\n"
        assert (REPO_ROOT / target).is_file(), "the fixture must name a document that really exists"

    @pytest.mark.parametrize(
        "address",
        [
            "https://github.com/astral-sh/ruff",
            "https://img.shields.io/badge/license-MIT-12130f.svg",
            "#quick-start",
            "mailto:someone@example.com",
            # Protocol-relative. It resolves against the PAGE's scheme, not the repository, so
            # rewriting it would break a working URL. `":" in ...` judged it relative — the same
            # broken test that judged `1:missing.md` absolute, in the other direction.
            "//img.shields.io/badge/x.svg",
        ],
    )
    def test_an_address_that_already_resolves_from_anywhere_is_left_alone(self, address: str) -> None:
        # Absolute URLs, in-page anchors and mailto: all resolve identically on PyPI and on GitHub.
        # Rewriting them would be the opposite failure — a working link turned into a 404.
        #
        # One relative address rides along on purpose: a document with none at all is refused
        # outright (`TestItFailsLoudRatherThanSilently`), so the untouched-ness of the absolute one
        # has to be asserted inside a document the transformer accepts.
        text = f'[x]({address}) and <a href="{address}">y</a> and [real](LICENSE)\n'

        transformed = transform(text, root=REPO_ROOT)

        assert transformed == text.replace("(LICENSE)", f"({BLOB}LICENSE)")
        assert transformed.count(address) == 2


class TestTheRealReadmeIsFullyResolved:
    """The artifact test: it grades the transformed bytes, not the script's intentions."""

    def test_the_repository_readme_carries_the_relative_addresses_this_script_exists_for(self) -> None:
        # RED-side of the guard: if this ever drops to zero the README was restructured and the
        # count below stops meaning anything, which is exactly what `transform` refuses to do
        # quietly.
        found = relative_addresses((REPO_ROOT / "README.md").read_text(encoding="utf-8"))

        assert len(found) >= 9, f"expected the measured relative addresses, found {found}"

    def test_no_relative_address_survives_the_transformation(self) -> None:
        transformed = transform((REPO_ROOT / "README.md").read_text(encoding="utf-8"), root=REPO_ROOT)

        assert relative_addresses(transformed) == []

    def test_every_relative_address_in_the_readme_names_a_file_that_exists(self) -> None:
        # Not a list of today's filenames: the set is read out of the README and checked against the
        # filesystem, so a link added tomorrow is covered by the same assertion.
        assert unresolvable_addresses((REPO_ROOT / "README.md").read_text(encoding="utf-8")) == []

    def test_a_section_link_would_pass_this_check_too(self) -> None:
        """
        The artifact check must accept exactly what the transformer accepts.

        `README.md` carries no fragment link today, so nothing here is red on the real file — and
        that is the point: the moment someone adds `[contract](docs/cli-contract.md#exit-codes)`,
        `transform` accepts it and a check spelling its own raw-path predicate would fail the
        release on a document that is present. Measured 2026-08-01 at `9c660c5`:
        `(REPO_ROOT / 'docs/cli-contract.md#exit-codes').exists()` is **False** while `_absolute`
        returns the blob URL.
        """
        assert unresolvable_addresses("see the [contract](docs/cli-contract.md#exit-codes).\n") == []


class TestTheShapesTheSharedRegexWasBlindTo:
    """
    The verifier used to be `relative_addresses` — **the same regex the rewriter runs**. So it was
    blind exactly where the rewriter was blind and reported success on its own output: the defect
    this epic keeps finding, one layer up.

    Reproduced on a real tree 2026-07-31, before the fix: appending a reference-style link, a
    single-quoted `href` and a fenced code block gave `rc=0`, `rewrote 11 relative addresses`, and
    left two 404s in the shipped description while corrupting the code block.

    Each shape below is one of those, and they are asserted through `transform` — which now verifies
    by rendering the markdown the way PyPI does, so a shape the rewriter misses is a build failure
    rather than a silent pass.
    """

    def test_a_reference_style_link_definition_is_rewritten(self) -> None:
        text = "See the [guide][g] for more.\n\n[g]: docs/cli-contract.md\n"

        assert transform(text, root=REPO_ROOT) == (f"See the [guide][g] for more.\n\n[g]: {BLOB}docs/cli-contract.md\n")

    def test_a_single_quoted_html_attribute_is_rewritten(self) -> None:
        text = "<a href='docs/rules-and-configuration.md'>rules</a>\n"

        assert transform(text, root=REPO_ROOT) == f"<a href='{BLOB}docs/rules-and-configuration.md'>rules</a>\n"

    def test_an_address_inside_a_fenced_code_block_is_left_alone(self) -> None:
        # A fenced block is literal text: rewriting it changes what the document SAYS, which is the
        # opposite failure from a dead link and just as wrong.
        text = "[real](LICENSE)\n\n```markdown\n[an example](docs/cli-contract.md)\n```\n"

        transformed = transform(text, root=REPO_ROOT)

        assert "```markdown\n[an example](docs/cli-contract.md)\n```" in transformed
        assert transformed.count(BLOB) == 1

    def test_an_address_inside_an_inline_code_span_is_left_alone(self) -> None:
        text = "[real](LICENSE) and `see [x](docs/cli-contract.md)` verbatim.\n"

        transformed = transform(text, root=REPO_ROOT)

        assert "`see [x](docs/cli-contract.md)`" in transformed
        assert transformed.count(BLOB) == 1

    def test_a_shape_the_rewriter_misses_is_a_build_failure_not_a_silent_pass(self) -> None:
        # The guard that makes the four cases above more than a list of shapes somebody thought of.
        # `<a href=docs/cli-contract.md>` is UNQUOTED — the rewriter does not handle it, on purpose,
        # because this test pins what happens then: the render-based verifier still sees the address
        # and the build stops. A new shape costs a red build, never a broken project page.
        with pytest.raises(ReadmeNotInExpectedFormatError, match="survived"):
            transform("[real](LICENSE) <a href=docs/cli-contract.md>x</a>\n", root=REPO_ROOT)


class TestTheTransformNeverChangesCode:
    """
    The direction the F1 split does NOT cover, and the reason it needed its own judge.

    `rendered_addresses` catches addresses LEFT BEHIND. It is structurally incapable of catching
    code the rewriter CORRUPTED: a link inside a fence renders as text, not as a link, so a
    rewritten one and an untouched one look identical to it — no `href` either way.

    Measured before the fix: a `~~~markdown` fence containing `[x](docs/cli-contract.md)` came out
    with an absolute URL inside it and `transform` returned normally. The code-span regex knew only
    backticks, and that surface had already been widened by one shape twice.

    The guard is therefore not "also know about `~~~`". It renders the document BEFORE and AFTER and
    requires every code block and code span to be byte-identical. That is blind to which fence
    syntax was used, so `~~~`, indented blocks and anything else fall out without being enumerated.

    **The cost, named:** `CODE` is deliberately NOT widened, so a README that grows a `~~~` fence
    or an indented block holding a repo-relative address FAILS THE BUILD until somebody widens it.
    That is friction, and it is the right direction of failure — the alternative this replaced was
    silently publishing corrupted documentation. The guard is also what makes widening safe when it
    is wanted: change `CODE`, and this check confirms the change was right.
    """

    def test_a_tilde_fence_stops_the_build_instead_of_being_corrupted(self) -> None:
        # `CODE` still knows only backticks, on purpose — see the class docstring. What changed is
        # that the corruption is now LOUD. Before the guard this returned normally with an absolute
        # URL inside the fence and every other check passing.
        text = "[real](LICENSE)\n\n~~~markdown\n[x](docs/cli-contract.md)\n~~~\n"

        with pytest.raises(ReadmeNotInExpectedFormatError, match="changed code content"):
            transform(text, root=REPO_ROOT)

    def test_an_indented_code_block_stops_the_build_too(self) -> None:
        # Never enumerated anywhere in this script — it falls out of comparing rendered code, which
        # is the whole point: shapes nobody listed are covered.
        text = "[real](LICENSE)\n\nAn example:\n\n    [x](docs/cli-contract.md)\n"

        with pytest.raises(ReadmeNotInExpectedFormatError, match="changed code content"):
            transform(text, root=REPO_ROOT)

    def test_code_content_is_read_off_the_render_not_off_the_source(self) -> None:
        # The mechanism itself: same code, three fence syntaxes, and the extractor sees the text of
        # each without knowing any of them.
        assert code_content("```\nA\n```\n") == code_content("~~~\nA\n~~~\n") == ["A\n"]


class TestTheVerifierDoesNotShareTheRewritersEyes:
    """
    `rendered_addresses` is the independent half: it runs the markdown through the SAME renderer
    PyPI uses and reads the `href`/`src` of the resulting HTML. It shares no regex with the rewriter,
    so it can see shapes the rewriter cannot.
    """

    def test_it_sees_an_address_the_rewriting_regex_cannot(self) -> None:
        # An UNQUOTED HTML attribute. The rewriter does not handle it and this test is what says so
        # out loud: `relative_addresses` returns nothing, the renderer returns the address, and the
        # gap between the two lists is the whole reason the verifier is a separate mechanism.
        source = "<a href=docs/cli-contract.md>bare</a>\n"

        assert relative_addresses(source) == []
        assert rendered_addresses(source) == ["docs/cli-contract.md"]

    def test_it_does_not_see_addresses_inside_code(self) -> None:
        source = "```\n[x](docs/cli-contract.md)\n```\n\n`[y](LICENSE)`\n"

        assert rendered_addresses(source) == []

    def test_it_reports_the_absolute_urls_it_finds_so_the_caller_can_judge_them(self) -> None:
        assert rendered_addresses(f"[x]({BLOB}LICENSE)\n") == [f"{BLOB}LICENSE"]


class TestItFailsLoudRatherThanSilently:
    """
    A transformer that shrugs at input it does not understand leaves the package page broken and
    every gate green — `twine check` validates the METADATA envelope, never the links inside it.
    """

    def test_a_readme_with_no_relative_address_is_a_build_failure(self) -> None:
        with pytest.raises(ReadmeNotInExpectedFormatError, match="no relative address"):
            transform("# tooprolix\n\nAll [absolute](https://example.com/x).\n", root=REPO_ROOT)

    def test_a_relative_address_naming_a_missing_file_is_a_build_failure(self) -> None:
        # The other half of the reason wording: an address that really is absent must still be
        # called missing. Pinning only the directory branch would let both cases collapse onto one
        # word again, in whichever direction the next edit happens to push.
        with pytest.raises(ReadmeNotInExpectedFormatError, match="docs/moved-away.md.* is missing"):
            transform("see [the contract](docs/moved-away.md).\n", root=REPO_ROOT)

    @pytest.mark.parametrize("address", ["docs", "docs/", ".", "./", "docs/..", "docs/.", "LICENSE/..#missing"])
    def test_an_address_that_resolves_to_a_directory_is_a_build_failure(self, address: str) -> None:
        """
        `_absolute` says "or raise if it is not a **file**", and its error says "names no **file**".

        It asked `Path.exists()`, which is true of directories, so all seven of these were accepted
        and rewritten into blob URLs pointing at a directory listing or at the repository root.
        `LICENSE/..#missing` is the one this task introduced: before the fragment split the raw
        path carried the `#` and was refused, and splitting it normalised the address into the root.
        The other six are pre-existing, and they die on the same word.

        Measured 2026-08-01 at `9c660c5`: all seven returned a `blob/main/…` URL, e.g.
        `.` → `…/blob/main/.`.

        **The message is asserted, not only the exception type.** The first version of this fix
        rejected directories with the wording it had used for absent files — `docs is missing`,
        `<repo root> is missing` — which is a message asserting a fact the code never determined,
        and it sends a reader looking for a path that is right there. Matching on the reason is
        what stops that drifting back.
        """
        with pytest.raises(ReadmeNotInExpectedFormatError, match="names no file .* is a directory"):
            transform(f"[real](LICENSE) [bad]({address})\n", root=REPO_ROOT)

    @pytest.mark.parametrize("address", ["?plain=1", "?raw=true"])
    def test_a_query_with_no_document_in_front_of_it_is_a_build_failure(self, address: str) -> None:
        """
        The one address the fragment/query split must NOT wave through.

        Splitting the path off for the filesystem lookup means an address that is *only* a query has
        an empty path, and `root / ""` is the repository root — which exists, so it would resolve
        and be rewritten to a `blob/main/?plain=1` URL that points at nothing. Reachable, not
        hypothetical: `relative_addresses('[x](?plain=1) …')` returns `['?plain=1', 'LICENSE']`
        (measured 2026-08-01), so the rewriter really does hand this to the resolver.
        """
        with pytest.raises(ReadmeNotInExpectedFormatError, match="carries no path"):
            transform(f"[real](LICENSE) [bad]({address})\n", root=REPO_ROOT)

    @pytest.mark.parametrize("address", ["1:missing.md", "2024:notes.md", "9:x/y.md"])
    def test_a_colon_that_is_not_a_url_scheme_is_still_a_relative_address(self, address: str) -> None:
        # A URL scheme must start with a LETTER (RFC 3986). `1:missing.md` has a colon and no
        # scheme, so a browser resolves it against the project page — it is a 404 in waiting, and
        # `":" in address.split("/")[0]` waved it through as absolute.
        with pytest.raises(ReadmeNotInExpectedFormatError, match="names no file"):
            transform(f"[real](LICENSE) [bad]({address})\n", root=REPO_ROOT)

    def test_an_address_that_escapes_the_repository_is_a_build_failure(self) -> None:
        # `../` resolves to a file that exists on this machine and to nothing on GitHub. Compare
        # after normalising, never before: a lexical prefix check passes `docs/../../secrets`.
        with pytest.raises(ReadmeNotInExpectedFormatError, match="outside the repository"):
            transform("see [up there](../AGENTS.md).\n", root=REPO_ROOT)


class TestTheGuardIsWiredIntoTheEntryPoint:
    """
    The checks above are unreachable prose unless `main()` runs them and unless a failure reaches
    the shell as a non-zero exit code — the workflow step is `python scripts/transform_readme.py`,
    and a script that printed a warning and exited 0 would let the build carry on.
    """

    def _run(self, readme: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, str(SCRIPT), str(readme)], capture_output=True, text=True, check=False)

    def test_it_rewrites_the_file_in_place_and_exits_zero(self, tmp_path: Path) -> None:
        # The addresses are resolved against the README's own directory, so the referenced file has
        # to be here too — which is the point of the existence check, not an artefact of the test.
        (tmp_path / "docs").mkdir()
        (tmp_path / "docs" / "cli-contract.md").write_text("x\n", encoding="utf-8")
        readme = tmp_path / "README.md"
        readme.write_text("see [the contract](docs/cli-contract.md).\n", encoding="utf-8")

        result = self._run(readme)

        assert result.returncode == 0, result.stderr
        assert readme.read_text(encoding="utf-8") == f"see [the contract]({BLOB}docs/cli-contract.md).\n"

    def test_a_corrupted_readme_exits_non_zero_and_says_why(self, tmp_path: Path) -> None:
        readme = tmp_path / "README.md"
        readme.write_text("# tooprolix\n\nNothing relative in here.\n", encoding="utf-8")

        result = self._run(readme)

        assert result.returncode != 0
        assert "no relative address" in result.stderr
