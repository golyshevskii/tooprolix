"""Executable contracts for comparing an approved release manifest with downloaded files."""

from __future__ import annotations

import hashlib
import subprocess
import sys
from pathlib import Path

import pytest

REPO: Path = Path(__file__).parents[2]
CHECKER: Path = REPO / "scripts" / "check_release_manifest.py"
VERSION: str = "0.5.1"
FILENAMES: tuple[str, ...] = (
    f"tooprolix-{VERSION}.tar.gz",
    f"tooprolix-{VERSION}-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
    f"tooprolix-{VERSION}-py3-none-macosx_11_0_arm64.whl",
    f"tooprolix-{VERSION}-py3-none-win_amd64.whl",
)


def fixture(tmp_path: Path) -> tuple[Path, Path]:
    directory = tmp_path / "dist"
    directory.mkdir()
    rows: list[str] = []
    for filename in FILENAMES:
        content = f"artifact:{filename}\n".encode()
        (directory / filename).write_bytes(content)
        rows.append(f"{len(content)}\t{filename}\t{hashlib.sha256(content).hexdigest()}")
    manifest = tmp_path / "release-manifest.txt"
    manifest.write_text(
        "tag: v0.5.1\n"
        f"tag-target-sha: {'a1' * 20}\n"
        f"tree-sha: {'b2' * 20}\n"
        "run-id: 424242\n\n"
        "size\tfilename\tsha256\n" + "\n".join(rows) + "\n",
        encoding="utf-8",
    )
    return manifest, directory


def run_checker(manifest: Path, directory: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(CHECKER), "--version", VERSION, str(manifest), str(directory)],
        capture_output=True,
        text=True,
        check=False,
    )


def test_an_honest_download_matches_the_approved_manifest(tmp_path: Path) -> None:
    manifest, directory = fixture(tmp_path)

    result = run_checker(manifest, directory)

    assert result.returncode == 0, f"{result.stdout}{result.stderr}"


@pytest.mark.parametrize("mutation", ["wrong-hash", "extra", "missing", "duplicate", "path"])
def test_a_changed_or_malformed_download_is_refused(tmp_path: Path, mutation: str) -> None:
    manifest, directory = fixture(tmp_path)
    honest = run_checker(manifest, directory)
    assert honest.returncode == 0, f"honest fixture failed:\n{honest.stdout}{honest.stderr}"

    if mutation == "wrong-hash":
        text = manifest.read_text(encoding="utf-8")
        manifest.write_text(text.replace(text.splitlines()[6].rsplit("\t", 1)[1], "0" * 64, 1), encoding="utf-8")
    elif mutation == "extra":
        (directory / "foreign.whl").write_bytes(b"foreign")
    elif mutation == "missing":
        (directory / FILENAMES[0]).unlink()
    elif mutation == "duplicate":
        lines = manifest.read_text(encoding="utf-8").splitlines()
        lines[9] = lines[6]
        manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
    else:
        lines = manifest.read_text(encoding="utf-8").splitlines()
        size, _, digest = lines[6].split("\t")
        lines[6] = f"{size}\t../{FILENAMES[0]}\t{digest}"
        manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")

    result = run_checker(manifest, directory)

    assert result.returncode != 0, f"{mutation} passed:\n{result.stdout}{result.stderr}"
