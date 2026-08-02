"""Compare an approved tooprolix release manifest with exactly four downloaded files."""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
from pathlib import Path


class ManifestError(AssertionError):
    """The manifest or downloaded directory is not the exact approved release."""


def expected_filenames(version: str) -> tuple[str, ...]:
    """Return the only four distribution filenames supported for one version."""
    if not version or "/" in version or "\\" in version or version in {".", ".."}:
        raise ManifestError(f"unsafe version {version!r}")
    return (
        f"tooprolix-{version}.tar.gz",
        f"tooprolix-{version}-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        f"tooprolix-{version}-py3-none-macosx_11_0_arm64.whl",
        f"tooprolix-{version}-py3-none-win_amd64.whl",
    )


def digest(path: Path) -> str:
    """Return a file's SHA-256 without loading an artifact into memory."""
    checksum = hashlib.sha256()
    try:
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                checksum.update(chunk)
    except OSError as error:
        raise ManifestError(f"cannot read {path}: {error}") from error
    return checksum.hexdigest()


def verify(manifest: Path, directory: Path, version: str) -> None:
    """Fail unless the manifest and directory describe the exact same supported release."""
    try:
        lines = manifest.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError) as error:
        raise ManifestError(f"cannot read {manifest}: {error}") from error

    if len(lines) != 10 or lines[4] != "" or lines[5] != "size\tfilename\tsha256":
        raise ManifestError("manifest must contain four metadata lines, one blank, one header, and four rows")
    if lines[0] != f"tag: v{version}":
        raise ManifestError(f"manifest tag does not match version {version}")
    if not re.fullmatch(r"tag-target-sha: [0-9a-f]{40}", lines[1]):
        raise ManifestError("manifest tag-target-sha is not a full lowercase commit SHA")
    if not re.fullmatch(r"tree-sha: [0-9a-f]{40}", lines[2]):
        raise ManifestError("manifest tree-sha is not a full lowercase tree SHA")
    if not re.fullmatch(r"run-id: [1-9][0-9]*", lines[3]):
        raise ManifestError("manifest run-id is not a positive integer")

    expected = expected_filenames(version)
    rows: dict[str, tuple[int, str]] = {}
    for row in lines[6:]:
        fields = row.split("\t")
        if len(fields) != 3:
            raise ManifestError(f"manifest row must have exactly three tab-separated fields: {row!r}")
        size, filename, checksum = fields
        if filename not in expected or filename in rows:
            raise ManifestError(f"unknown, duplicated, or unsafe manifest filename: {filename!r}")
        if not re.fullmatch(r"[0-9]+", size) or not re.fullmatch(r"[0-9a-f]{64}", checksum):
            raise ManifestError(f"invalid size or SHA-256 for {filename}")
        rows[filename] = (int(size), checksum)
    if set(rows) != set(expected):
        raise ManifestError(f"manifest filenames differ from the supported set: {sorted(rows)}")

    try:
        entries = list(directory.iterdir())
    except OSError as error:
        raise ManifestError(f"cannot list {directory}: {error}") from error
    if len(entries) != 4 or {entry.name for entry in entries} != set(expected):
        raise ManifestError(f"download directory must contain exactly {list(expected)!r}")
    if any(not entry.is_file() or entry.is_symlink() for entry in entries):
        raise ManifestError("every downloaded entry must be a regular, non-symlink file")

    for filename in expected:
        path = directory / filename
        approved_size, approved_digest = rows[filename]
        try:
            actual_size = path.stat().st_size
        except OSError as error:
            raise ManifestError(f"cannot stat {path}: {error}") from error
        actual_digest = digest(path)
        if actual_size != approved_size or actual_digest != approved_digest:
            raise ManifestError(f"{filename} differs from the approved size or SHA-256")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True, help="release version without the leading v")
    parser.add_argument("manifest", type=Path, help="approved release-manifest.txt")
    parser.add_argument("directory", type=Path, help="directory containing files downloaded from PyPI")
    parsed = parser.parse_args(argv)

    try:
        verify(parsed.manifest, parsed.directory, parsed.version)
    except ManifestError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print(f"OK: {parsed.directory} exactly matches the approved manifest for tooprolix {parsed.version}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
