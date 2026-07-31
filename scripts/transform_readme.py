"""
Rewrite the relative addresses in README.md into absolute GitHub URLs, for the PyPI project page.

`readme = "README.md"` in `[project]` makes this file the description PyPI renders. README.md is
written for GitHub, where `docs/cli-contract.md` resolves against the repository; on the project
page there is no repository to resolve against and every such address is a 404. Measured on this
README 2026-07-31: **10** of them — 8 relative markdown links, `assets/tooprolix.gif`, and the
`<a href="LICENSE">` behind the licence badge. `twine check` sees none of this: it validates the
METADATA envelope, never the links inside the description.

The technique is ruff's (`scripts/transform_readme.py` at a2635fd8): a committed transformer run
before every build, which **raises** rather than warns when the input is not the shape it expects.
The logic is not ruff's — their script swaps one `<picture>` block for one `<img>` and knows nothing
about links.

# What makes this a guard rather than a best effort

Three refusals, and the third is the one that matters:

  1. a relative address naming a file that is not in the checkout is a build failure — the link is
     already broken on GitHub, and rewriting it would only move the 404;
  2. a relative address that escapes the repository (`../`) is a build failure — it resolves on the
     author's machine and nowhere else. **The check is on the resolved path, never on the text**:
     `./`, `..` and symlinks defeat any lexical prefix test;
  3. a README with **no** relative address at all is a build failure. That is the silent case: the
     document was restructured, this script now does nothing, and without this refusal every gate
     stays green while the transformer has quietly stopped being wired to anything.

🔴 **And the verification does not share the rewriter's eyes.** It used to: the check at the end of
`transform` re-ran the rewriting regex, so it was blind exactly where the rewriter was blind and
could only confirm that the rewriter had done what the rewriter could see. Measured on a real tree
2026-07-31 — a reference-style `[g]: docs/cli-contract.md` definition and a single-quoted
`href='docs/rules-and-configuration.md'` BOTH survived a run that printed `rewrote 11 relative
addresses` and exited **0**, while a link inside a fenced block was wrongly rewritten.

The output is now verified by `rendered_addresses`, which renders the markdown with the library PyPI
itself uses and reads the `href`/`src` of the resulting HTML. It shares no pattern with the rewriter,
so a syntax `ADDRESS` has never heard of still stops the build. `ADDRESS` is the rewriter's reach;
the renderer is the judge.

Usage (the workflow step, run from the repository root, before maturin reads the README):

    python scripts/transform_readme.py [README.md]
"""

from __future__ import annotations

import argparse
import re
import sys
from html.parser import HTMLParser
from pathlib import Path

REPOSITORY = "https://github.com/golyshevskii/tooprolix"
# `main` and not the release tag: the sdist is built before the tag exists (this is the dry-run
# matrix; the tag is created by release-plz afterwards), so a tag URL would 404 on the one page it
# is meant to serve. The cost is that the links track the branch rather than the release.
BLOB = f"{REPOSITORY}/blob/main/"
# `raw.githubusercontent.com` for images specifically: GitHub serves a `blob/` address as an HTML
# page, so an <img> pointing at it renders as a broken image.
RAW = "https://raw.githubusercontent.com/golyshevskii/tooprolix/main/"

IMAGE_SUFFIXES = frozenset({".gif", ".jpeg", ".jpg", ".png", ".svg", ".webp"})

# Fenced blocks and inline code spans, which are LITERAL TEXT and must not be rewritten. Measured
# before this existed: ```` ```markdown\n[x](docs/cli-contract.md)\n``` ```` came out with an
# absolute URL inside it, i.e. the transformer changed what the document says. That is the opposite
# failure from a dead link and just as wrong.
CODE = re.compile(r"(?ms)^```.*?^```|`[^`\n]*`")

# The three address syntaxes the rewriter knows:
#   * markdown link/image targets  `](x)`
#   * HTML attributes              `src="x"` / `href='x'` — EITHER quote. Single quotes were missed
#     by the first version and survived a run that reported success.
#   * reference-style definitions  `[g]: x` on its own line, whose target lives nowhere near a `](`.
#
# ⚠️ This list is not the guard, and must never be treated as one. It is the rewriter's reach;
# [`rendered_addresses`] is what decides whether the reach was enough, and it shares nothing with
# this pattern. A fourth syntax (an unquoted attribute, say) reaches the verifier and fails the
# build — which is what `test_a_shape_the_rewriter_misses_is_a_build_failure_not_a_silent_pass`
# pins, deliberately leaving one shape unhandled so the split is provable rather than asserted.
ADDRESS = re.compile(
    r"\]\((?P<markdown>[^)\s]+)\)"
    r"|(?P<attribute>src|href)=(?P<quote>[\"'])(?P<html>[^\"']+)(?P=quote)"
    r"|^(?P<label>\[[^\]]+\]:[ \t]+)(?P<reference>\S+)",
    re.MULTILINE,
)


class ReadmeNotInExpectedFormatError(ValueError):
    """The README is not the document this transformer knows how to make PyPI-safe."""


def _is_relative(address: str) -> bool:
    """Whether `address` needs a repository to resolve against."""
    # Anchors and `mailto:` resolve identically everywhere; anything with a scheme is already
    # absolute. Rewriting either would turn a working link into a 404 — the opposite failure.
    return not (address.startswith("#") or ":" in address.split("/", 1)[0])


def _prose_spans(text: str) -> list[tuple[int, int]]:
    """Return the spans of `text` that are outside every fenced block and inline code span."""
    spans: list[tuple[int, int]] = []
    cursor = 0
    for code in CODE.finditer(text):
        spans.append((cursor, code.start()))
        cursor = code.end()
    spans.append((cursor, len(text)))
    return spans


def relative_addresses(text: str) -> list[str]:
    """
    Every address the REWRITER can see that only resolves inside a checkout, in document order.

    ⚠️ **Not the verifier.** This is what [`transform`] rewrites, so it is blind to any syntax
    [`ADDRESS`] does not carry — which is precisely why the check at the end of [`transform`] is
    [`rendered_addresses`] instead. Kept public because "did the shape of the README change?" is a
    question about the rewriter's reach, and the tests ask it directly.
    """
    found = [
        match["markdown"] or match["html"] or match["reference"]
        for start, end in _prose_spans(text)
        for match in ADDRESS.finditer(text, start, end)
    ]
    return [address for address in found if _is_relative(address)]


class _Addresses(HTMLParser):
    """Collects the value of every `href`/`src` attribute in a rendered document."""

    def __init__(self) -> None:
        super().__init__()
        self.found: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.found.extend(value for name, value in attrs if name in {"href", "src"} and value)

    # `<img src=...>` is void and arrives here in some documents rather than at `handle_starttag`.
    def handle_startendtag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.handle_starttag(tag, attrs)


def rendered_addresses(text: str) -> list[str]:
    """
    Every `href`/`src` the PyPI renderer produces for `text`, in document order.

    🔴 **This is the independent half of the guard and it must stay independent.** Verifying with
    [`relative_addresses`] — the rewriter's own regex — is what the first version of this script did,
    and it meant the verifier was blind exactly where the rewriter was blind: it graded its own
    output. Measured on a real tree 2026-07-31, before this function existed: a reference-style
    definition and a single-quoted `href` both survived a run that reported
    `rewrote 11 relative addresses` and exited **0**.

    So the question is asked of the ARTIFACT instead — `readme_renderer` is the library PyPI itself
    renders descriptions with (and the one `twine check` already pulls in), so this reads the very
    HTML the project page will show. Three things fall out that no regex of ours has to know about:
    reference-style links are resolved, quoting is normalised, and fenced blocks and inline code
    become text rather than links.
    """
    try:
        import readme_renderer.markdown
    except ImportError as error:  # pragma: no cover - exercised by the workflow, not by a unit test
        # Fail CLOSED. Skipping the verification when the library is absent would turn the one guard
        # that cannot be fooled into a guard that is simply off on the machine that lacks it.
        message = (
            "readme_renderer is required to verify the transformed README (it is what PyPI renders "
            "with). Install it: uv run --with 'readme_renderer[md]' ..."
        )
        raise ReadmeNotInExpectedFormatError(message) from error

    html = readme_renderer.markdown.render(text)
    if html is None:
        message = "readme_renderer refused to render the README; PyPI would show it as plain text"
        raise ReadmeNotInExpectedFormatError(message)

    parser = _Addresses()
    parser.feed(html)
    parser.close()
    return parser.found


def _absolute(address: str, root: Path) -> str:
    """Make `address` absolute, or raise if it is not a file inside this repository."""
    resolved_root = root.resolve()
    target = (resolved_root / address).resolve()
    # Resolved on both sides before comparing: `docs/../../secrets` and a symlink out of the tree
    # both pass a lexical `startswith` on the raw text.
    if not target.is_relative_to(resolved_root):
        message = f"README address {address!r} points outside the repository, to {target}"
        raise ReadmeNotInExpectedFormatError(message)
    if not target.exists():
        message = f"README address {address!r} names no file in the repository ({target} is missing)"
        raise ReadmeNotInExpectedFormatError(message)

    base = RAW if target.suffix.lower() in IMAGE_SUFFIXES else BLOB
    return f"{base}{address}"


def transform(text: str, root: Path) -> str:
    """`text` with every relative address rewritten against `root`'s canonical GitHub URLs."""
    if not relative_addresses(text):
        message = (
            "README.md is not in the expected format: no relative address found. Either the "
            "document was restructured and this transformer no longer applies, or it has already "
            "been transformed — both mean the build is about to publish something nobody checked."
        )
        raise ReadmeNotInExpectedFormatError(message)

    def replace(match: re.Match[str]) -> str:
        address = match["markdown"] or match["html"] or match["reference"]
        if not _is_relative(address):
            return match[0]
        absolute = _absolute(address, root)
        if match["markdown"]:
            return f"]({absolute})"
        if match["reference"]:
            return f"{match['label']}{absolute}"
        # The original quote character is kept rather than normalised to `"`: this script's job is
        # the addresses, and rewriting punctuation it was not asked about is how a diff stops being
        # reviewable.
        return f"{match['attribute']}={match['quote']}{absolute}{match['quote']}"

    # Spliced from matches found only in the prose spans, so fenced blocks and inline code pass
    # through untouched. Deliberately NOT `ADDRESS.sub` over `text[start:end]`: a slice that begins
    # mid-line would let `^` in the reference-definition branch match where no line starts.
    # `finditer(text, start, end)` keeps the real line boundaries.
    pieces: list[str] = []
    cursor = 0
    for start, end in _prose_spans(text):
        for match in ADDRESS.finditer(text, start, end):
            replacement = replace(match)
            if replacement != match[0]:
                pieces.append(text[cursor : match.start()])
                pieces.append(replacement)
                cursor = match.end()
    pieces.append(text[cursor:])
    rewritten = "".join(pieces)

    # 🔴 GRADE THE ARTIFACT, AND WITH DIFFERENT EYES. This used to re-run `relative_addresses` — the
    # rewriter's own regex — so it could only ever confirm that the rewriter had done what the
    # rewriter could see. `rendered_addresses` renders the markdown the way PyPI does and reads the
    # resulting `href`/`src`, so a syntax `ADDRESS` has never heard of still stops the build here.
    if survivors := [address for address in rendered_addresses(rewritten) if _is_relative(address)]:
        message = f"relative addresses survived the transformation: {survivors}"
        raise ReadmeNotInExpectedFormatError(message)
    return rewritten


def main(argv: list[str] | None = None) -> int:
    """Rewrite the README in place; return 0, or 1 with the reason on stderr."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[1] if __doc__ else None)
    parser.add_argument(
        "readme", nargs="?", default="README.md", type=Path, help="the README to rewrite in place (default: README.md)"
    )
    arguments = parser.parse_args(argv)

    readme: Path = arguments.readme
    try:
        text = readme.read_text(encoding="utf-8")
    except OSError as error:
        print(f"error: could not read {readme}: {error}", file=sys.stderr)
        return 1

    try:
        # The README's own directory is the repository root — addresses in it are written relative
        # to the file, which is what GitHub resolves them against.
        rewritten = transform(text, root=readme.parent)
    except ReadmeNotInExpectedFormatError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    readme.write_text(rewritten, encoding="utf-8")
    print(f"transform_readme: rewrote {len(relative_addresses(text))} relative addresses in {readme}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
