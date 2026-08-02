"""
Grade a built wheel or sdist by opening it, not by re-reading the configuration that produced it.

**This exists because three acceptance criteria were true only because somebody looked once.**
AC3 (MIT metadata, physical project/third-party licence files, `docs/` in the sdist) and AC8 (the
shipped description is the *transformed* README) were verified by hand and written into a report, and nothing in the
repository asserted any of it. `twine check --strict` validates the METADATA envelope and the
description's markup; it never asks what the licence says or which file the description came from.

Measured 2026-07-31, all three exploits green under every gate that existed:

  * `license-files = []` in `pyproject.toml` -> a wheel with **no** `License-File` header and **no**
    `LICENSE` inside it. `twine check --strict`: PASSED. `install-smoke.sh`: OK.
    The obvious exploit, *deleting* `license-files`, does NOT reproduce — maturin 1.14.1 finds
    `LICENSE` on its own. The empty list is what actually strips it, which is precisely why the
    check has to grade the archive rather than trust a key's presence in a manifest.
  * `exclude = ["corpus/", "docs/"]` in `Cargo.toml` -> an sdist with 0 files under `docs/`.
    `twine check --strict`: PASSED.
  * an inline `readme = {text = "..."}` -> `scripts/transform_readme.py` still verifies README.md
    happily while a completely different description ships.

Usage, once per built artifact, after the README has been transformed and before anything is
uploaded:

    python scripts/check_artifact.py dist/tooprolix-0.4.1-py3-none-macosx_11_0_arm64.whl README.md
"""

from __future__ import annotations

import argparse
import email
import sys
import tarfile
import zipfile
from collections.abc import Iterator
from pathlib import Path

#: Metadata the project promises and PyPI shows. PEP 639 spellings, which is what maturin emits.
#: `Requires-Python` is the distribution floor, not `MIN_INTERPRETER`; test_measure.py keeps them apart.
REQUIRED_HEADERS: dict[str, str] = {"License-Expression": "MIT", "Requires-Python": ">=3.11"}

#: PEP 639 paths that must be declared and physically present in both distribution archives.
REQUIRED_LICENSE_FILES: tuple[str, ...] = ("LICENSE", "THIRD-PARTY-LICENSES.html")

#: Documents `README.md` links to that must travel with the source. The README's own links are
#: rewritten to GitHub URLs for the project page, so the sdist is the only place a consumer without
#: network access can read them.
REQUIRED_SDIST_DOCUMENTS: tuple[str, ...] = ("docs/cli-contract.md", "docs/rules-and-configuration.md")


class ArtifactError(AssertionError):
    """The built artifact does not carry what the acceptance criteria promise."""


def tag_set(compressed: str) -> frozenset[str]:
    """
    Expand a PEP 425 compressed tag set into every `{python}-{abi}-{platform}` triple it names.

    **The two representations differ by construction, and comparing them as strings was the bug
    this replaced.** A wheel's FILENAME carries the compressed set — platforms joined with `.` —
    while `.dist-info/WHEEL` carries one `Tag:` line per tag. Joining those lines with any separator
    and comparing strings fails on every multi-tag wheel, which is what run 30613111390 showed on
    the linux leg:

        FAIL: the archive declares 'py3-none-manylinux_2_17_x86_64+py3-none-manylinux2014_x86_64',
              the matrix promises 'py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64'

    macOS and Windows wheels are single-tag, so both stayed green and only CI could find it.
    Normalising to a set makes the representation stop being part of the question.

    Each component may itself be a `.`-joined list, and the expansion is their CROSS PRODUCT rather
    than a zip: `cp39.cp310-abi3-linux_x86_64` names four tags, not two.
    """
    try:
        pythons, abis, platforms = compressed.split("-")
    except ValueError as error:
        message = f"{compressed!r} is not a `{{python}}-{{abi}}-{{platform}}` tag set"
        raise ArtifactError(message) from error
    return frozenset(
        f"{python}-{abi}-{platform}"
        for python in pythons.split(".")
        for abi in abis.split(".")
        for platform in platforms.split(".")
    )


def _tags_from_filename(path: Path) -> frozenset[str]:
    """Return the tag set a wheel's own filename declares: its last three `-`-separated fields."""
    parts = path.name.removesuffix(".whl").split("-")
    minimum_fields = 3
    if len(parts) < minimum_fields:
        message = f"{path.name} is not a wheel filename"
        raise ArtifactError(message)
    return tag_set("-".join(parts[-minimum_fields:]))


def _wheel(path: Path) -> tuple[list[str], bytes]:
    """Return the wheel's member names and its `METADATA` bytes."""
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        metadata = next((n for n in names if n.endswith(".dist-info/METADATA")), None)
        if metadata is None:
            message = f"{path.name} has no .dist-info/METADATA"
            raise ArtifactError(message)
        return names, archive.read(metadata)


def _tags_inside(path: Path) -> frozenset[str]:
    """Return the tag set `.dist-info/WHEEL` declares, which is what a resolver actually reads."""
    with zipfile.ZipFile(path) as archive:
        member = next((n for n in archive.namelist() if n.endswith(".dist-info/WHEEL")), None)
        if member is None:
            message = f"{path.name} has no .dist-info/WHEEL"
            raise ArtifactError(message)
        lines = archive.read(member).decode().splitlines()
    declared = [line.split(":", 1)[1].strip() for line in lines if line.startswith("Tag:")]
    if not declared:
        message = f"{path.name}: .dist-info/WHEEL declares no Tag:"
        raise ArtifactError(message)
    return frozenset[str]().union(*(tag_set(tag) for tag in declared))


def _sdist(path: Path) -> tuple[list[str], bytes]:
    """Return the sdist's member names and its `PKG-INFO` bytes."""
    with tarfile.open(path) as archive:
        names = archive.getnames()
        info = next((n for n in names if n.count("/") == 1 and n.endswith("/PKG-INFO")), None)
        if info is None:
            message = f"{path.name} has no top-level PKG-INFO"
            raise ArtifactError(message)
        handle = archive.extractfile(info)
        if handle is None:
            message = f"{path.name}: {info} is not a regular file"
            raise ArtifactError(message)
        return names, handle.read()


def _member_bytes(path: Path, member: str) -> bytes:
    """Read one named member from a wheel or sdist."""
    if path.suffix == ".whl":
        with zipfile.ZipFile(path) as archive:
            return archive.read(member)

    with tarfile.open(path) as archive:
        handle = archive.extractfile(member)
        if handle is None:
            message = f"{path.name}: {member} is not a regular file"
            raise ArtifactError(message)
        return handle.read()


def _normalise(text: str) -> str:
    """
    Return `text` with line endings flattened and surrounding blank space removed.

    **The one deliberate exception to "byte-identical", named because that was the promise.** Git
    checks out CRLF on the Windows runner, so the description maturin embedded was byte-different
    from the README on disk while being the same text — measured in run 30613111390:
    `4927 vs 4827 characters` on a 99-line file, i.e. one carriage return per line plus the
    payload's trailing newline. Nothing else is normalised: a difference in TEXT still fails.
    """
    return text.replace("\r\n", "\n").replace("\r", "\n").strip()


def _problems(path: Path, readme: Path, expect_tag: str | None) -> Iterator[str]:
    """Yield one line per promise the artifact at `path` does not keep."""
    is_wheel = path.suffix == ".whl"
    names, raw = _wheel(path) if is_wheel else _sdist(path)
    message = email.message_from_bytes(raw)

    # Grade the PARSE first: every check below reads what `email` RECOVERED, and a recovery is a guess.
    for defect in message.defects:
        yield f"the metadata did not parse cleanly: {type(defect).__name__}"

    # AC2. `expect_tag` comes from the matrix, so it is fixed independently of whatever the build
    # produced; the archive and the filename are then both required to agree with it and therefore
    # with each other. The in-archive read is the one a resolver honours — a filename is a label
    # anybody can type — and the filename read is what catches a rename.
    if expect_tag is not None and is_wheel:
        promised = tag_set(expect_tag)
        for source, actual in (("archive", _tags_inside(path)), ("filename", _tags_from_filename(path))):
            if actual != promised:
                yield f"the {source} declares {sorted(actual)}, the matrix promises {sorted(promised)}"

    # `get_all`, never `get`: `get` answers with the FIRST occurrence, hiding a duplicated header.
    for header, expected in REQUIRED_HEADERS.items():
        actual = message.get_all(header) or []
        if actual != [expected]:
            yield f"{header}: expected exactly one {expected!r}, archive says {actual!r}"

    # The header is the claim; the archive member is the fact. EVERY declared licence file is graded,
    # by its declared PATH — which subsumes the old basename-only `LICENSE` check, now deleted.
    declared_license_files = message.get_all("License-File") or []
    for required in REQUIRED_LICENSE_FILES:
        if required not in declared_license_files:
            yield f"License-File: expected {required!r} among {declared_license_files!r}"
    for declared in declared_license_files:
        member = next((name for name in names if name == declared or name.endswith(f"/{declared}")), None)
        if member is None:
            yield f"License-File declares {declared!r}, which is not in the archive"
        elif declared in REQUIRED_LICENSE_FILES:
            expected = (Path(__file__).resolve().parents[1] / declared).read_bytes()
            if _member_bytes(path, member) != expected:
                yield f"License-File {declared!r} does not match the committed file byte-for-byte"

    if not is_wheel:
        for document in REQUIRED_SDIST_DOCUMENTS:
            if not any(n.endswith(f"/{document}") for n in names):
                yield f"{document} is missing from the sdist"

    # AC8, and the object graded is the ARCHIVE's description rather than the file on disk. The
    # transformer verifies README.md; that says nothing about what maturin actually embedded, and
    # `readme = {text = "..."}` would ship something else while every other check stayed green.
    #
    # `decode=True` and an explicit utf-8: `get_payload()` alone mangles the em-dashes and the
    # comparison then measures the probe rather than the artifact.
    payload = message.get_payload(decode=True)
    shipped = payload.decode("utf-8") if isinstance(payload, bytes) else str(message.get_payload())
    wanted = readme.read_text(encoding="utf-8")
    if _normalise(shipped) != _normalise(wanted):
        yield (
            f"the description in the archive is not {readme} — the transformed README is what "
            f'`readme = "README.md"` is supposed to ship ({len(_normalise(shipped))} vs '
            f"{len(_normalise(wanted))} characters, line endings already normalised)"
        )


def main(argv: list[str] | None = None) -> int:
    """Check one artifact; return 0, or 1 with every problem named on stderr."""
    parser = argparse.ArgumentParser(description="Grade a built wheel or sdist against AC3 and AC8.")
    parser.add_argument("artifact", type=Path, help="the .whl or .tar.gz to open")
    parser.add_argument(
        "readme", nargs="?", default="README.md", type=Path, help="the transformed README the description must equal"
    )
    parser.add_argument(
        "--expect-tag", default=None, help="the compressed PEP 425 tag set the matrix promises, e.g. py3-none-win_amd64"
    )
    arguments = parser.parse_args(argv)

    artifact: Path = arguments.artifact
    if artifact.suffix != ".whl" and not artifact.name.endswith(".tar.gz"):
        print(f"error: {artifact} is neither a .whl nor a .tar.gz", file=sys.stderr)
        return 2

    try:
        problems = list(_problems(artifact, arguments.readme, arguments.expect_tag))
    except (ArtifactError, OSError, tarfile.TarError, zipfile.BadZipFile) as error:
        print(f"error: {artifact.name}: {error}", file=sys.stderr)
        return 1

    if problems:
        print(f"FAIL: {artifact.name} does not keep {len(problems)} promise(s):", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    print(f"check_artifact: {artifact.name} carries MIT metadata, required licence files, and the transformed README")
    return 0


if __name__ == "__main__":
    sys.exit(main())
