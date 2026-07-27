#!/usr/bin/env bash
# Install-smoke for the `tooprolix` CONSOLE SCRIPT, run against a real installed distribution.
#
# `[project.scripts]` only exists in an *installed* distribution, so no gate inside this repo can
# see it: `make test` is `uv run --only-group test pytest`, which deliberately does not build or
# install the project (Makefile:27-31), and `cargo test` never produces a wheel at all. This script
# is therefore the only place the console script is checked, and it checks it the only honest way —
# by installing the artifact into a fresh environment and running the command.
#
# Usage:
#   scripts/install-smoke.sh <install-source>
#
# `<install-source>` is anything `uv add` accepts, and it is a parameter because three callers need
# three different values: a local wheel before the tag exists, and
# `git+https://github.com/golyshevskii/tooprolix@v0.1.0` after it does (AC4's literal form). Nothing
# else here is configurable on purpose.
#
# What it asserts, and why each one is load-bearing:
#   1. `tooprolix --help` exits 0 — the command exists at all. This is the assertion that is RED
#      without `[project.scripts]`, with `tooprolix: command not found`.
#   2. `tooprolix check <a file with a known finding>` exits **1** and names **TPX002** — the CLI is
#      really wired to the linter. ⚠️ `tooprolix check .` cannot serve here: on a tree with no `.py`
#      files it honestly exits 0 with `warning: no Python files` (measured 2026-07-27), i.e. it
#      "passes" having checked nothing. An empty directory is the same trap.
#   3. A clean fixture exits 0 — so assertion 2 is not passing because the tool fails on everything.
set -euo pipefail

if [ $# -ne 1 ]; then
	echo "usage: $0 <install-source>" >&2
	echo "  local wheel:  $0 /path/to/tooprolix-0.1.0-....whl" >&2
	echo "  after the tag: $0 git+https://github.com/golyshevskii/tooprolix@v0.1.0" >&2
	exit 2
fi

source_spec="$1"
# `-P` on every `pwd`: the checks below compare paths, and `./`, `..` and symlinks defeat a lexical
# comparison. Resolve first, compare after.
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

# A local path is made absolute, because the install runs from a different working directory.
if [ -e "$source_spec" ]; then
	source_spec="$(cd "$(dirname "$source_spec")" && pwd -P)/$(basename "$source_spec")"
fi

work="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/tooprolix-smoke.XXXXXX")" && pwd -P)"
# The scratch project must live OUTSIDE this repository and outside its parents' checkouts: a
# parent `.gitignore` carrying `lib/` silently swallowed a scratch install once already (epic 1
# Decisions #7.3). TMPDIR is caller-controlled, so this is checked rather than assumed.
case "$work/" in
"$repo"/*)
	echo "error: the scratch project landed inside the repository ($work); set TMPDIR elsewhere" >&2
	exit 2
	;;
esac
trap 'rm -rf "$work"' EXIT

# `<want-rc> <needle-or-empty> <command...>`, printing what actually happened either way. It grades
# the artifact — the real exit code and the real bytes on stdout/stderr — not a claim about it.
run_check() {
	local want_rc="$1" needle="$2" output rc
	shift 2

	set +e
	output="$("$@" 2>&1)"
	rc=$?
	set -e

	printf '$ %s\n%s\n(exit %d)\n\n' "$*" "$output" "$rc"

	if [ "$rc" -ne "$want_rc" ]; then
		echo "FAIL: expected exit $want_rc from '$*', got $rc" >&2
		return 1
	fi
	if [ -n "$needle" ] && ! printf '%s' "$output" | grep -q -- "$needle"; then
		echo "FAIL: expected '$needle' in the output of '$*'" >&2
		return 1
	fi
}

echo "install-smoke: source=$source_spec"
echo "install-smoke: scratch project=$work"
echo

cd "$work"
uv init --name tooprolix-smoke >/dev/null
uv add "$source_spec"
echo

run_check 0 'tooprolix check <path>' uv run tooprolix --help
run_check 1 'TPX002' uv run tooprolix check "$repo/tests/fixtures/broken/long_docstring.py"
run_check 0 '' uv run tooprolix check "$repo/tests/fixtures/clean"

echo "install-smoke: OK"
