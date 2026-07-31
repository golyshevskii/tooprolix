"""
Grade a built wheel or sdist by opening it, not by re-reading the configuration that produced it.

🔴 **This exists because three acceptance criteria were true only because somebody looked once.**
AC3 (MIT metadata, a physical `LICENSE`, `docs/` in the sdist) and AC8 (the shipped description is
the *transformed* README) were verified by hand and written into a report, and nothing in the
repository asserted any of it. `twine check --strict` validates the METADATA envelope and the
description's markup; it never asks what the licence says or which file the description came from.

Measured 2026-07-31, all three exploits green under every gate that existed:

  * `license-files = []` in `pyproject.toml` -> a wheel with **no** `License-File` header and **no**
    `LICENSE` inside it. `twine check --strict`: PASSED. `install-smoke.sh`: OK.
    ⚠️ The obvious exploit, *deleting* `license-files`, does NOT reproduce — maturin 1.14.1 finds
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
REQUIRED_HEADERS: dict[str, str] = {"License-Expression": "MIT", "License-File": "LICENSE"}

#: Documents `README.md` links to that must travel with the source. The README's own links are
#: rewritten to GitHub URLs for the project page, so the sdist is the only place a consumer without
#: network access can read them.
REQUIRED_SDIST_DOCUMENTS: tuple[str, ...] = ("docs/cli-contract.md", "docs/rules-and-configuration.md")


class ArtifactError(AssertionError):
    """The built artifact does not carry what the acceptance criteria promise."""


def _wheel(path: Path) -> tuple[list[str], bytes]:
    """Return the wheel's member names and its `METADATA` bytes."""
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        metadata = next((n for n in names if n.endswith(".dist-info/METADATA")), None)
        if metadata is None:
            message = f"{path.name} has no .dist-info/METADATA"
            raise ArtifactError(message)
        return names, archive.read(metadata)


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


def _problems(path: Path, readme: Path) -> Iterator[str]:
    """Yield one line per promise the artifact at `path` does not keep."""
    is_wheel = path.suffix == ".whl"
    names, raw = _wheel(path) if is_wheel else _sdist(path)
    message = email.message_from_bytes(raw)

    for header, expected in REQUIRED_HEADERS.items():
        actual = message.get(header)
        if actual != expected:
            yield f"{header}: expected {expected!r}, archive says {actual!r}"

    # The header is a claim; the file is the artifact. Both are required — a `License-File` naming
    # a file that is not in the archive is exactly the shape of a self-report.
    licences = [n for n in names if Path(n).name == "LICENSE"]
    if not licences:
        yield "no file named LICENSE anywhere in the archive"

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
    if shipped.strip() != readme.read_text(encoding="utf-8").strip():
        yield (
            f"the description in the archive is not {readme} — the transformed README is what "
            f'`readme = "README.md"` is supposed to ship ({len(shipped)} vs '
            f"{len(readme.read_text(encoding='utf-8'))} characters)"
        )


def main(argv: list[str] | None = None) -> int:
    """Check one artifact; return 0, or 1 with every problem named on stderr."""
    parser = argparse.ArgumentParser(description="Grade a built wheel or sdist against AC3 and AC8.")
    parser.add_argument("artifact", type=Path, help="the .whl or .tar.gz to open")
    parser.add_argument(
        "readme", nargs="?", default="README.md", type=Path, help="the transformed README the description must equal"
    )
    arguments = parser.parse_args(argv)

    artifact: Path = arguments.artifact
    if artifact.suffix != ".whl" and not artifact.name.endswith(".tar.gz"):
        print(f"error: {artifact} is neither a .whl nor a .tar.gz", file=sys.stderr)
        return 2

    try:
        problems = list(_problems(artifact, arguments.readme))
    except (ArtifactError, OSError, tarfile.TarError, zipfile.BadZipFile) as error:
        print(f"error: {artifact.name}: {error}", file=sys.stderr)
        return 1

    if problems:
        print(f"FAIL: {artifact.name} does not keep {len(problems)} promise(s):", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    print(f"check_artifact: {artifact.name} carries MIT metadata, its LICENSE, and the transformed README")
    return 0


if __name__ == "__main__":
    sys.exit(main())
