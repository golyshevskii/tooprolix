"""
Guards for `scripts/transform_readme.py`, the README rewriter the wheel/sdist build runs before
maturin reads `readme = "README.md"`.

`README.md` is written for GitHub, where a relative address resolves against the repository. On the
PyPI project page there is no repository to resolve against, so every relative address 404s.
Measured on this README 2026-07-31: **10** of them — 8 relative markdown links, the
`assets/tooprolix.gif` image, and the `<a href="LICENSE">` behind the licence badge.

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
  4. 🔴 **it can grade its own output.** The verification used to re-run the rewriting regex, so it
     was blind exactly where the rewriter was blind — a reference-style definition and a
     single-quoted `href` both survived a run that reported success. `TestTheShapesTheSharedRegexWasBlindTo`
     pins those shapes and `TestTheVerifierDoesNotShareTheRewritersEyes` pins the split itself:
     the verifier renders the markdown the way PyPI does and reads the HTML, sharing no pattern
     with the rewriter.

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
    relative_addresses,
    rendered_addresses,
    transform,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "transform_readme.py"


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
        "address",
        [
            "https://github.com/astral-sh/ruff",
            "https://img.shields.io/badge/license-MIT-12130f.svg",
            "#quick-start",
            "mailto:someone@example.com",
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
        missing = [
            address
            for address in relative_addresses((REPO_ROOT / "README.md").read_text(encoding="utf-8"))
            if not (REPO_ROOT / address).exists()
        ]

        assert missing == []


class TestTheShapesTheSharedRegexWasBlindTo:
    """
    🔴 The verifier used to be `relative_addresses` — **the same regex the rewriter runs**. So it was
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
        with pytest.raises(ReadmeNotInExpectedFormatError, match="docs/moved-away.md"):
            transform("see [the contract](docs/moved-away.md).\n", root=REPO_ROOT)

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
