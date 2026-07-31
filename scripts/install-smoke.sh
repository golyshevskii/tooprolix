#!/usr/bin/env bash
# Install-smoke for the `tooprolix` COMMAND, run against a real installed distribution.
#
# 🔴 THIS SCRIPT IS THE GUARD THAT REPLACED THE PYO3 BOUNDARY TESTS (epic 2 Decisions #19.1). Until
# then the wheel carried an extension module and 5 Rust tests plus a `compile_error!` stood between
# us and publishing a wheel that exported nothing — a thing that had already happened once and
# passed every gate. The wheel now carries the compiled executable under `*.data/scripts/`, those
# 5 tests are gone with the boundary, and this script is what stands in their place. It is
# mutation-proved rather than merely present: a wheel with its executable removed must FAIL here.
#
# Nothing inside the repository can see any of this: `make test` is `uv run --only-group test
# pytest`, which deliberately does not build or install the project, and `cargo test` never produces
# a wheel at all. The only honest check is to install the artifact into a fresh environment and run
# the command.
#
# Usage:
#   scripts/install-smoke.sh <install-source>
#
# `<install-source>` is anything `uv add` accepts, and it is a parameter because the callers need
# different values: a local wheel or sdist here and in the build workflow, and
# `git+https://github.com/golyshevskii/tooprolix@vX.Y.Z` for the publication task. Nothing else here
# is configurable on purpose.
#
# What it asserts, and why each one is load-bearing:
#   1. the install produced an executable at `.venv/bin/tooprolix`, checked on the FILESYSTEM. With
#      `bindings = "bin"` that file comes from `tooprolix-<version>.data/scripts/tooprolix` in the
#      wheel, so this is the assertion that goes red on a wheel with no binary in it.
#   2. `tooprolix --help` exits 0 — the command not only exists but runs.
#   3. `tooprolix --version` prints the version the distribution's own METADATA declares, AND a real
#      `YYYY-MM-DD` date rather than `unknown`. The date comes from `build.rs`, which resolves
#      `SOURCE_DATE_EPOCH` -> git -> `unknown`; an sdist carries no `.git` and (since task #7) will
#      not borrow a surrounding repository's, so **a release build that forgets to export
#      `SOURCE_DATE_EPOCH` ships a binary that does not know what it was built from**. Measured
#      2026-07-31: a wheel built from our own sdist without it prints `tooprolix 0.4.1 (unknown)`
#      and passed every other assertion here.
#   4. `tooprolix check <a file with a known finding>` exits **1** and names **TPX002** — the CLI is
#      really wired to the linter. ⚠️ `tooprolix check .` cannot serve here: on a tree with no `.py`
#      files it honestly exits 0 with `warning: no Python files` (measured 2026-07-27), i.e. it
#      "passes" having checked nothing. An empty directory is the same trap.
#   5. A clean fixture exits 0 — so assertion 4 is not passing because the tool fails on everything.
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

# uv reads the environment for WHICH environment to use, so an exported variable from the caller can
# redirect the whole run at a venv that already has tooprolix in it. Measured 2026-07-27: with
# `UV_PROJECT_ENVIRONMENT` pointing at this repo's own `.venv` and `UV_NO_SYNC=1`, this script
# installed nothing, ran the preloaded binary three times and printed `install-smoke: OK` — while
# the wheel it had been handed contained **0** entry_points and no `tooprolix` command at all. It
# graded a different artifact and said the wrong thing. That is not only adversarial: a developer
# with `UV_PROJECT_ENVIRONMENT` exported globally gets the same false pass.
#
# Two layers, because unsetting names is a blocklist and blocklists rot:
#   1. the three variables that redirect the environment are cleared here, and
#   2. after the install, the console script is required to EXIST inside the scratch project's own
#      venv (below). No environment variable, present or future, survives that check — it grades the
#      file that the install was supposed to produce.
unset UV_PROJECT_ENVIRONMENT UV_NO_SYNC VIRTUAL_ENV

work="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/tooprolix-smoke.XXXXXX")" && pwd -P)"
# Immediately, and BEFORE the boundary check below — not after it. The check's whole job is to keep
# scratch state out of a checkout, and with the trap installed later it exited 2 having left the
# directory it had just created sitting in exactly the place it was rejecting. Measured.
#
# The signals are listed as well as EXIT because death by an uncaught signal does not run an
# EXIT-only trap: Ctrl-C, or a caller that truncates this script's output through `head`, would leak
# the scratch project. Seen once while proving the line above, and not reproducible afterwards —
# which is reason to cover it rather than to argue about it, since the cost is four words.
trap 'rm -rf "$work"' EXIT HUP INT TERM PIPE

# The scratch project must live outside this repository AND outside every checkout enclosing it: a
# parent `.gitignore` silently swallowed a scratch install once already (epic 1 Decisions #7.3), and
# that trap is live — `/Users/vgolyshevskii/dwh/.gitignore:17` is `lib/`, one level above this repo.
# Comparing against `$repo` alone therefore did NOT check what this comment claims: measured, a
# `TMPDIR` one level up passed the guard and landed straight in the trap.
#
# The boundary is derived, never hardcoded to anyone's home: walk up from the repo and keep the
# HIGHEST ancestor that looks like a checkout (`.git`) or carries ignore rules (`.gitignore`).
# Everything below that is off limits. Both sides are `pwd -P` output, so the comparison is on
# resolved paths — `./`, `..` and symlinks are already gone.
boundary="$repo"
probe="$(dirname "$repo")"
while [ "$probe" != "/" ]; do
	if [ -e "$probe/.git" ] || [ -f "$probe/.gitignore" ]; then
		boundary="$probe"
	fi
	probe="$(dirname "$probe")"
done

case "$work/" in
"$boundary"/*)
	echo "error: the scratch project landed at $work, inside the checkout at $boundary." >&2
	echo "       Its .gitignore rules would apply to the install; set TMPDIR outside it." >&2
	exit 2
	;;
esac

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

# The install must have produced the console script HERE, in this scratch project. This is the check
# that makes the run un-redirectable: `uv run tooprolix` below resolves through uv, and uv can be
# pointed elsewhere, but this asks the filesystem whether the artifact under test actually created
# the command. Layer 2 of the note at the top.
if [ ! -x "$work/.venv/bin/tooprolix" ]; then
	echo "FAIL: installing '$source_spec' produced no executable at $work/.venv/bin/tooprolix" >&2
	echo "      — either the wheel carries nothing under *.data/scripts/, or uv installed elsewhere." >&2
	exit 1
fi

run_check 0 'tooprolix check <path>' uv run tooprolix --help

# The version and the date, both read off the artifact rather than off a claim about it.
#
# The expected version is the one the INSTALLED distribution declares in its own METADATA, which is
# a different path out of Cargo.toml from the one the binary took (maturin reads the manifest;
# rustc bakes `CARGO_PKG_VERSION` in at compile time). A wheel assembled around a binary from some
# other build disagrees here, and nothing else in this script would notice.
expected_version="$(uv run python -c 'import importlib.metadata as m; print(m.version("tooprolix"))')"
printf '$ tooprolix --version   (expecting version %s and a real date)\n' "$expected_version"
version_line="$(uv run tooprolix --version)"
printf '%s\n\n' "$version_line"

# `tooprolix <version> (<YYYY-MM-DD>)`, anchored at both ends. A substring match would accept
# `tooprolix 0.4.1 (unknown) trailing junk`, and `[0-9]` alone would accept a nine-digit year.
if ! printf '%s' "$version_line" |
	grep -Eq "^tooprolix ${expected_version} \([0-9]{4}-[0-9]{2}-[0-9]{2}\)$"; then
	echo "FAIL: --version printed '$version_line'" >&2
	echo "      expected 'tooprolix ${expected_version} (YYYY-MM-DD)'." >&2
	echo "      A date of 'unknown' means the build did not export SOURCE_DATE_EPOCH: an sdist has" >&2
	echo "      no .git, and build.rs will not borrow a surrounding repository's commit date." >&2
	exit 1
fi

run_check 1 'TPX002' uv run tooprolix check "$repo/tests/fixtures/broken/long_docstring.py"
run_check 0 '' uv run tooprolix check "$repo/tests/fixtures/clean"

echo "install-smoke: OK"
