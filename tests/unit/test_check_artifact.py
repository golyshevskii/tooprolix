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
from check_artifact import main, tag_set

DESCRIPTION = "# tooprolix\n\nThe transformed README.\n"
HEADERS = (
    "Metadata-Version: 2.4\nName: tooprolix\nVersion: 0.0.0\n"
    "License-Expression: MIT\nLicense-File: LICENSE\nRequires-Python: >=3.11\n"
)


def _metadata(headers: str = HEADERS, description: str = DESCRIPTION) -> bytes:
    return f"{headers}\n{description}".encode()


#: The real manylinux case, which is why C1 existed: the FILENAME carries PEP 425's compressed tag
#: set (platforms joined with `.`) while `.dist-info/WHEEL` carries one `Tag:` line PER tag. The two
#: are different representations of the same thing by construction, and comparing them as strings
#: fails on every multi-tag wheel. macOS is single-tag, which is why only CI caught it.
MANYLINUX_COMPRESSED = "py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64"
MANYLINUX_WHEEL_LINES = "Wheel-Version: 1.0\nTag: py3-none-manylinux_2_17_x86_64\nTag: py3-none-manylinux2014_x86_64\n"


def _wheel(
    path: Path,
    *,
    metadata: bytes | None = None,
    licence: bool = True,
    compressed: str = "py3-none-any",
    wheel_file: str | None = None,
) -> Path:
    wheel = path / f"tooprolix-0.0.0-{compressed}.whl"
    with zipfile.ZipFile(wheel, "w") as archive:
        archive.writestr("tooprolix-0.0.0.dist-info/METADATA", metadata or _metadata())
        archive.writestr("tooprolix-0.0.0.dist-info/WHEEL", wheel_file or f"Wheel-Version: 1.0\nTag: {compressed}\n")
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
    def test_a_python_floor_that_is_not_the_one_the_project_promises_fails(
        self, tmp_path: Path, readme: Path, kind: str
    ) -> None:
        # `>=3.12` is the floor this project shipped BEFORE the distribution floor was split from
        # the corpus tooling's own 3.12 floor, so it is exactly the value a regression drifts back
        # to — and the wheel is `py3-none-<platform>`, carrying a native executable and no Python,
        # so nothing else in the archive would notice. `Requires-Python` is what an installer reads
        # to refuse the wheel on 3.11; a narrower floor locks out the interpreters AC1 promises.
        metadata = _metadata(HEADERS.replace("Requires-Python: >=3.11", "Requires-Python: >=3.12"))
        archive = _wheel(tmp_path, metadata=metadata) if kind == "wheel" else _sdist(tmp_path, metadata=metadata)

        assert main([str(archive), str(readme)]) == 1

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


class TestTheWheelTagIsComparedLikeWithLike:
    """
    🔴 C1, and it took a CI run to find because macOS wheels are single-tag.

    A manylinux wheel's `.dist-info/WHEEL` carries **two** `Tag:` lines; its filename carries PEP
    425's *compressed tag set*, where the platform components are joined with `.`. Joining the
    `Tag:` lines with anything and comparing strings therefore fails on every multi-tag wheel:

        FAIL: the archive declares 'py3-none-manylinux_2_17_x86_64+py3-none-manylinux2014_x86_64',
              the matrix promises 'py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64'

    Both are now expanded into the same normalised form — a SET of `{python}-{abi}-{platform}`
    triples — before anything is compared, so the representation stops being part of the question.
    """

    def test_a_compressed_tag_set_expands_to_every_triple_it_names(self) -> None:
        assert tag_set(MANYLINUX_COMPRESSED) == {"py3-none-manylinux_2_17_x86_64", "py3-none-manylinux2014_x86_64"}
        # The cross product, not a zip: `cp39.cp310-abi3-linux_x86_64` is four tags, not two.
        assert tag_set("py2.py3-none-any") == {"py2-none-any", "py3-none-any"}

    def test_a_real_multi_tag_manylinux_wheel_passes(self, tmp_path: Path, readme: Path) -> None:
        wheel = _wheel(tmp_path, compressed=MANYLINUX_COMPRESSED, wheel_file=MANYLINUX_WHEEL_LINES)

        assert main([str(wheel), str(readme), "--expect-tag", MANYLINUX_COMPRESSED]) == 0

    def test_a_wheel_renamed_to_claim_another_platform_is_rejected(self, tmp_path: Path, readme: Path) -> None:
        # THE mutation the in-archive read exists for, and it must keep reddening after the fix: the
        # filename says manylinux, the archive says macOS.
        wheel = _wheel(
            tmp_path,
            compressed=MANYLINUX_COMPRESSED,
            wheel_file="Wheel-Version: 1.0\nTag: py3-none-macosx_11_0_arm64\n",
        )

        assert main([str(wheel), str(readme), "--expect-tag", MANYLINUX_COMPRESSED]) == 1

    def test_a_wheel_that_is_not_the_tag_the_matrix_promised_is_rejected(self, tmp_path: Path, readme: Path) -> None:
        # Both the filename and the archive agree — with each other, and not with the matrix. That
        # is a build that targeted the wrong platform, and `expect_tag` is declared in the matrix
        # precisely so it is fixed independently of whatever came out.
        wheel = _wheel(tmp_path, compressed="py3-none-macosx_11_0_arm64")

        assert main([str(wheel), str(readme), "--expect-tag", MANYLINUX_COMPRESSED]) == 1

    def test_a_wheel_missing_only_one_of_the_promised_platform_tags_is_rejected(
        self, tmp_path: Path, readme: Path
    ) -> None:
        # A set comparison, not "is a subset": a wheel tagged manylinux_2_17 alone does not keep a
        # promise that also named manylinux2014.
        wheel = _wheel(
            tmp_path,
            compressed=MANYLINUX_COMPRESSED,
            wheel_file="Wheel-Version: 1.0\nTag: py3-none-manylinux_2_17_x86_64\n",
        )

        assert main([str(wheel), str(readme), "--expect-tag", MANYLINUX_COMPRESSED]) == 1


class TestLineEndingsAreNotADifferenceInTheDescription:
    r"""
    🔴 C2, also found only by CI. Git checks out CRLF on the Windows runner, so the description
    maturin embedded was byte-different from the README on disk while being the same text —
    `4927 vs 4827 characters` on a 99-line file: one `\r` per line, plus the payload's trailing
    newline in the printed count.

    "Byte-identical" was the promise and this is the one deliberate, named exception to it. Every
    other respect stays exact, which the second test here is what proves.
    """

    def test_a_crlf_description_still_matches_an_lf_readme(self, tmp_path: Path, readme: Path) -> None:
        crlf = _metadata(description=DESCRIPTION.replace("\n", "\r\n"))

        assert main([str(_wheel(tmp_path, metadata=crlf)), str(readme)]) == 0

    def test_normalising_line_endings_does_not_make_the_check_lenient(self, tmp_path: Path, readme: Path) -> None:
        # The mutation that must still redden: different TEXT, not different line endings.
        crlf_but_wrong = _metadata(description="Something else entirely.\r\n")

        assert main([str(_wheel(tmp_path, metadata=crlf_but_wrong)), str(readme)]) == 1


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
