"""
Guards for `scripts/check_artifact.py`, the gate that opens a built wheel or sdist and grades it.

The script exists because AC3 and AC8 were true only because somebody looked once — no committed
code asserted the licence metadata, the physical `LICENSE`, `docs/` in the sdist, or that the
shipped description is the *transformed* README. This file is the level above that: it stops the
gate itself from being quietly gutted, because a check deleted from `_problems` would otherwise turn
every one of those promises back into a thing nobody verifies.

The archives here are built by hand rather than by maturin, on purpose. A test that needed a real
build would be too slow to run in `make test`, and it would also grade whatever maturin happens to
do today — the point is to pin what THIS script refuses, one refusal at a time. The real artifacts
are graded by the workflow, on every build, which is where maturin's behaviour belongs.

Run: make test
"""

from __future__ import annotations

import io
import tarfile
import zipfile
from pathlib import Path

import pytest
from check_artifact import main

DESCRIPTION = "# tooprolix\n\nThe transformed README.\n"
HEADERS = "Metadata-Version: 2.4\nName: tooprolix\nVersion: 0.0.0\nLicense-Expression: MIT\nLicense-File: LICENSE\n"


def _metadata(headers: str = HEADERS, description: str = DESCRIPTION) -> bytes:
    return f"{headers}\n{description}".encode()


def _wheel(path: Path, *, metadata: bytes | None = None, licence: bool = True) -> Path:
    wheel = path / "tooprolix-0.0.0-py3-none-any.whl"
    with zipfile.ZipFile(wheel, "w") as archive:
        archive.writestr("tooprolix-0.0.0.dist-info/METADATA", metadata or _metadata())
        archive.writestr("tooprolix-0.0.0.data/scripts/tooprolix", b"\x7fELF")
        if licence:
            archive.writestr("tooprolix-0.0.0.dist-info/licenses/LICENSE", "MIT\n")
    return wheel


def _sdist(path: Path, *, metadata: bytes | None = None, licence: bool = True, docs: bool = True) -> Path:
    sdist = path / "tooprolix-0.0.0.tar.gz"
    with tarfile.open(sdist, "w:gz") as archive:

        def add(name: str, data: bytes) -> None:
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))

        add("tooprolix-0.0.0/PKG-INFO", metadata or _metadata())
        if licence:
            add("tooprolix-0.0.0/LICENSE", b"MIT\n")
        if docs:
            add("tooprolix-0.0.0/docs/cli-contract.md", b"x\n")
            add("tooprolix-0.0.0/docs/rules-and-configuration.md", b"x\n")
    return sdist


@pytest.fixture
def readme(tmp_path: Path) -> Path:
    path = tmp_path / "README.md"
    path.write_text(DESCRIPTION, encoding="utf-8")
    return path


class TestAnHonestArtifactPasses:
    """The floor. Without these, every refusal below could be "it always fails"."""

    def test_a_complete_wheel_passes(self, tmp_path: Path, readme: Path) -> None:
        assert main([str(_wheel(tmp_path)), str(readme)]) == 0

    def test_a_complete_sdist_passes(self, tmp_path: Path, readme: Path) -> None:
        assert main([str(_sdist(tmp_path)), str(readme)]) == 0


class TestEachPromiseIsRefusedSeparately:
    """
    One case per promise, because a single "something is wrong" test passes on a script that checks
    only the first thing and would let the other three ship.
    """

    def test_a_wheel_without_the_licence_file_fails(self, tmp_path: Path, readme: Path) -> None:
        # The measured exploit: `license-files = []` produces exactly this — no header, no file,
        # `twine check --strict` PASSED, install-smoke OK.
        headers = HEADERS.replace("License-File: LICENSE\n", "")

        assert main([str(_wheel(tmp_path, metadata=_metadata(headers), licence=False)), str(readme)]) == 1

    def test_a_licence_header_naming_a_file_that_is_not_there_fails(self, tmp_path: Path, readme: Path) -> None:
        # The self-report shape: METADATA claims a LICENSE, the archive has none. The header alone
        # would satisfy a check that only read METADATA.
        assert main([str(_wheel(tmp_path, licence=False)), str(readme)]) == 1

    def test_a_licence_that_is_not_mit_fails(self, tmp_path: Path, readme: Path) -> None:
        headers = HEADERS.replace("License-Expression: MIT", "License-Expression: Apache-2.0")

        assert main([str(_wheel(tmp_path, metadata=_metadata(headers))), str(readme)]) == 1

    def test_an_sdist_without_the_documents_the_readme_links_to_fails(self, tmp_path: Path, readme: Path) -> None:
        # The measured exploit: `exclude = ["corpus/", "docs/"]`, twine PASSED.
        assert main([str(_sdist(tmp_path, docs=False)), str(readme)]) == 1

    @pytest.mark.parametrize("kind", ["wheel", "sdist"])
    def test_a_description_that_is_not_the_transformed_readme_fails(
        self, tmp_path: Path, readme: Path, kind: str
    ) -> None:
        # The measured exploit: `readme = {text = "..."}`. The transformer still verifies README.md
        # perfectly happily while a completely different description ships — which is why AC8 has to
        # be graded on the ARCHIVE and not on the file the transformer touched.
        metadata = _metadata(description="Something else entirely.\n")
        archive = _wheel(tmp_path, metadata=metadata) if kind == "wheel" else _sdist(tmp_path, metadata=metadata)

        assert main([str(archive), str(readme)]) == 1


class TestItRefusesWhatItCannotGrade:
    """A gate that shrugs at input it does not understand is a gate that is off."""

    def test_an_archive_of_an_unknown_kind_is_refused(self, tmp_path: Path, readme: Path) -> None:
        other = tmp_path / "tooprolix-0.0.0.zip"
        other.write_bytes(b"not a wheel")

        assert main([str(other), str(readme)]) == 2

    def test_a_wheel_with_no_metadata_at_all_is_refused(self, tmp_path: Path, readme: Path) -> None:
        wheel = tmp_path / "tooprolix-0.0.0-py3-none-any.whl"
        with zipfile.ZipFile(wheel, "w") as archive:
            archive.writestr("tooprolix-0.0.0.data/scripts/tooprolix", b"\x7fELF")

        assert main([str(wheel), str(readme)]) == 1
