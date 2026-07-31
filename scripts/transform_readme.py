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

The output is verified, not assumed: `main` re-scans what it wrote and refuses to leave a relative
address behind, so the artifact is graded rather than the intention.

Usage (the workflow step, run from the repository root, before maturin reads the README):

    python scripts/transform_readme.py [README.md]
"""

from __future__ import annotations

import argparse
import re
import sys
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

# Markdown link targets `](x)` and the two HTML attributes this README uses. Both syntaxes are
# needed: a regex that only knows `](...)` leaves `<img src="assets/tooprolix.gif">` behind, which
# is the address that renders as a broken image rather than as a dead link.
ADDRESS = re.compile(r"\]\((?P<markdown>[^)\s]+)\)|(?P<attribute>src|href)=\"(?P<html>[^\"]+)\"")


class ReadmeNotInExpectedFormatError(ValueError):
    """The README is not the document this transformer knows how to make PyPI-safe."""


def _is_relative(address: str) -> bool:
    """Whether `address` needs a repository to resolve against."""
    # Anchors and `mailto:` resolve identically everywhere; anything with a scheme is already
    # absolute. Rewriting either would turn a working link into a 404 — the opposite failure.
    return not (address.startswith("#") or ":" in address.split("/", 1)[0])


def relative_addresses(text: str) -> list[str]:
    """Every address in `text` that only resolves inside a checkout, in the order they appear."""
    found = [match["markdown"] or match["html"] for match in ADDRESS.finditer(text)]
    return [address for address in found if _is_relative(address)]


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
        address = match["markdown"] or match["html"]
        if not _is_relative(address):
            return match[0]
        absolute = _absolute(address, root)
        if match["markdown"]:
            return f"]({absolute})"
        return f'{match["attribute"]}="{absolute}"'

    rewritten = ADDRESS.sub(replace, text)

    # Grade the output, not the plan. If the substitution missed a syntax the scanner can see, the
    # build fails here rather than on the project page.
    if survivors := relative_addresses(rewritten):
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
