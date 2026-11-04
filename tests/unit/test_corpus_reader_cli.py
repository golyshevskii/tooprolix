"""Exercise malformed corpus input through the real sampler and classification commands."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

CORPUS = Path(__file__).resolve().parents[2] / "corpus"


@pytest.mark.parametrize("command", ["sample_clusters", "classification"])
@pytest.mark.parametrize("mutation", ["none", "nan", "range", "empty", "address"])
def test_corpus_commands_validate_before_printing_results(tmp_path: Path, command: str, mutation: str) -> None:
    runs = tmp_path / "runs"
    shutil.copytree(CORPUS / "runs", runs)
    run = runs / "pydantic.json"
    document = json.loads(run.read_text())
    cluster = next(item for item in document["findings"] if item["code"] == "TPX003")
    if mutation == "nan":
        cluster["weakest"]["similarity"] = "NaN"
    elif mutation == "range":
        cluster["weakest"]["similarity"] = 1.25
    elif mutation == "empty":
        cluster["locations"] = []
    elif mutation == "address":
        cluster["line"] = None

    if command == "sample_clusters":
        for other in runs.glob("*.json"):
            if other != run:
                other.unlink()
        document["findings"] = [cluster]
        source_root = tmp_path / "sources"
        for member in cluster["locations"]:
            source = source_root / member["path"]
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_text("# fixture prose\n" * member["end_line"])
        arguments = ["--runs", str(runs), "--population", "all", "--limit", "1"]
    else:
        arguments = ["--runs", str(runs), "--profile", "dry_run", "--artifact", str(tmp_path / "artifact.json")]
        source_root = tmp_path

    run.write_text(json.dumps(document))
    if command == "classification":
        artifact = json.loads((CORPUS / "dry_run_classification.json").read_text())
        # Keep the fixture hash honest so semantic validation, not provenance, rejects the input.
        for entry in artifact["runs"]:
            if entry["name"] == "pydantic":
                entry["sha256"] = hashlib.sha256(run.read_bytes()).hexdigest()
        (tmp_path / "artifact.json").write_text(json.dumps(artifact))

    result = subprocess.run(
        [sys.executable, str(CORPUS / f"{command}.py"), *arguments],
        env={**os.environ, "CORPUS_ROOT": str(source_root)},
        capture_output=True,
        text=True,
        check=False,
    )
    if mutation == "none":
        assert result.returncode == 0, result.stderr
        assert "# Sample" in result.stdout if command == "sample_clusters" else "artifact verified" in result.stdout
        assert result.stderr == ""
    else:
        assert result.returncode == 1
        assert result.stdout == ""
        assert result.stderr.startswith("error: pydantic: TPX003 finding at ")
        assert "Traceback" not in result.stderr
