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
#   3. `tooprolix --version` prints ONE line: the version the distribution's own METADATA declares,
#      and the date. The date is compared against the value the CALLER supplies when it has one —
#      a shape check alone accepted `tooprolix 0.4.1 (2000-01-01)`, i.e. a wheel assembled around a
#      binary from another commit (measured). The date comes from `build.rs`, which resolves
#      `SOURCE_DATE_EPOCH` -> git -> `unknown`; an sdist carries no `.git` and (since task #7) will
#      not borrow a surrounding repository's, so **a release build that forgets to export
#      `SOURCE_DATE_EPOCH` ships a binary that does not know what it was built from**. Measured
#      2026-07-31: a wheel built from our own sdist without it prints `tooprolix 0.4.1 (unknown)`
#      and passed every other assertion here.
#   4. `import tooprolix` raises **ModuleNotFoundError** — the distribution carries an executable
#      and no importable module (AC0). Until this was asserted, a wheel keeping the binary AND
#      shipping a `tooprolix/` package passed everything else here.
#   5. `tooprolix check <a file with a known finding>` exits **1** and names **TPX002** — the CLI is
#      really wired to the linter. ⚠️ `tooprolix check .` cannot serve here: on a tree with no `.py`
#      files it honestly exits 0 with `warning: no Python files` (measured 2026-07-27), i.e. it
#      "passes" having checked nothing. An empty directory is the same trap.
#   6. A clean fixture exits 0 — so assertion 5 is not passing because the tool fails on everything.
set -euo pipefail

if [ $# -ne 2 ]; then
	echo "usage: $0 <install-source> <expected-date>" >&2
	echo "  a matrix wheel: $0 dist/x.whl \"\$(git log -1 --format=%cs)\"" >&2
	echo "  offline sdist:  $0 dist/x.tar.gz unknown" >&2
	echo "  after the tag:  $0 git+https://github.com/golyshevskii/tooprolix@vX.Y.Z \\" >&2
	echo "                     \"\$(git log -1 --format=%cs vX.Y.Z)\"" >&2
	echo >&2
	echo "  <expected-date> is REQUIRED — see the note on the oracle below. The publication task" >&2
	echo "  reads it from the tag it is publishing: \`git log -1 --format=%cs <tag>\`." >&2
	exit 2
fi

source_spec="$1"
# 🔴 THE DATE IS AN ORACLE THE CALLER SUPPLIES, AND WITHOUT IT THIS CHECK IS A SHAPE CHECK.
# Measured 2026-07-31 on a wheel repacked around a shim: `tooprolix 0.4.1 (2000-01-01)` printed
# `install-smoke: OK` and exited 0. Any `YYYY-MM-DD` passed, so a wheel assembled around a binary
# from a different commit was indistinguishable from the right one — the date was graded as a
# self-report while the version beside it was cross-checked against METADATA.
#
# 🔴 AND IT IS REQUIRED, because an optional oracle is a guard with a silent weak branch. It was
# optional for one round: a caller who simply forgot the argument got the shape check and no
# warning, which is the same class of failure — a check quietly grading less than it claims — that
# has already bitten this task twice. There is no caller that cannot answer the question: CI knows
# the commit, and the publication task knows the tag.
#
# `unknown` is a legal expected value and is used deliberately: it is what an offline
# `pip install` from the sdist produces, and pinning it is how that documented behaviour stops
# being something the matrix never exercises.
expected_date="$2"
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
# Three layers, because unsetting names is a blocklist and blocklists rot:
#   1. the three variables that redirect the environment are cleared here, and
#   2. after the install, the console script is required to EXIST inside the scratch project's own
#      venv (below). No environment variable, present or future, survives that check — it grades the
#      file that the install was supposed to produce;
#   3. `UV_NO_CACHE=1`, so an sdist is rebuilt here rather than answered from a wheel uv built
#      earlier under a different environment. See the note beside it.
unset UV_PROJECT_ENVIRONMENT UV_NO_SYNC VIRTUAL_ENV

# 🔴 LAYER 3, AND IT WAS FOUND THE HARD WAY. uv caches the wheel it BUILDS from an sdist, and the
# cache key does not include the build environment — so the same tarball installed twice answers
# with the first build's binary. Measured 2026-07-31: with `SOURCE_DATE_EPOCH` correctly exported,
# this script reported `tooprolix 0.4.1 (unknown)` and FAILED, because an earlier cache-filling run
# had built the same sdist without it. With `UV_NO_CACHE=1` the same command answers
# `(2026-07-31)`, and without the epoch it answers `(unknown)` — both as designed.
#
# So the cache made the smoke grade a build that did not happen, which is the same defect as
# grading a self-report one layer down. A wheel install is a copy and pays nothing for this; an
# sdist install rebuilds the crate, and that cost is the price of the answer being about THIS
# artifact.
export UV_NO_CACHE=1

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
# POSIX venvs put it in `bin/`, Windows venvs in `Scripts/` with an `.exe` suffix. Both spellings
# are listed and BOTH are required to be absent before this fails — a check that only knew `bin/`
# would report a missing command on every Windows runner, and one that only knew `Scripts/` would
# pass on Linux without looking. The supported matrix includes Windows x86_64, so this script runs
# under Git Bash there and has to answer for that layout.
installed=""
for candidate in "$work/.venv/bin/tooprolix" "$work/.venv/Scripts/tooprolix.exe"; do
	if [ -x "$candidate" ]; then
		installed="$candidate"
		break
	fi
done
if [ -z "$installed" ]; then
	echo "FAIL: installing '$source_spec' produced no executable in $work/.venv" >&2
	echo "      (looked for bin/tooprolix and Scripts/tooprolix.exe)" >&2
	echo "      — either the wheel carries nothing under *.data/scripts/, or uv installed elsewhere." >&2
	exit 1
fi
echo "install-smoke: executable=$installed"

run_check 0 'tooprolix check <path>' uv run tooprolix --help

# The version and the date, both read off the artifact rather than off a claim about it.
#
# The expected version is the one the INSTALLED distribution declares in its own METADATA, which is
# a different path out of Cargo.toml from the one the binary took (maturin reads the manifest;
# rustc bakes `CARGO_PKG_VERSION` in at compile time). A wheel assembled around a binary from some
# other build disagrees here, and nothing else in this script would notice.
expected_version="$(uv run python -c 'import importlib.metadata as m; print(m.version("tooprolix"))')"
printf '$ tooprolix --version   (expecting version %s and date %s)\n' \
	"$expected_version" "${expected_date:-<any YYYY-MM-DD>}"
version_output="$(uv run tooprolix --version)"
printf '%s\n\n' "$version_output"

# ONE line, counted before anything is matched. `grep -E "^…$"` matches per LINE, so a two-line
# `--version` carrying `(unknown)` on the first and a valid date on the second satisfied the old
# anchored pattern and printed `install-smoke: OK` — measured 2026-07-31 on a repacked wheel. The
# contract is a single line, so that is what is asserted, and it is asserted first: every check
# below is only meaningful once there is exactly one thing to check.
if [ "$(printf '%s\n' "$version_output" | wc -l | tr -d ' ')" != "1" ]; then
	echo "FAIL: --version printed more than one line:" >&2
	printf '%s\n' "$version_output" >&2
	exit 1
fi

# With an oracle, the whole line must match exactly — that is what makes a binary from another
# commit fail. Without one, the shape is all that can honestly be asserted, and `unknown` still
# fails because it is not a date.
if [ -n "$expected_date" ]; then
	if [ "$version_output" != "tooprolix ${expected_version} (${expected_date})" ]; then
		echo "FAIL: --version printed '$version_output'" >&2
		echo "      expected exactly 'tooprolix ${expected_version} (${expected_date})'." >&2
		echo "      The binary was built from a different commit than the one that was packaged." >&2
		exit 1
	fi
elif ! printf '%s' "$version_output" |
	grep -Eq "^tooprolix ${expected_version} \([0-9]{4}-[0-9]{2}-[0-9]{2}\)$"; then
	echo "FAIL: --version printed '$version_output'" >&2
	echo "      expected 'tooprolix ${expected_version} (YYYY-MM-DD)'." >&2
	echo "      A date of 'unknown' means the build did not export SOURCE_DATE_EPOCH: an sdist has" >&2
	echo "      no .git, and build.rs will not borrow a surrounding repository's commit date." >&2
	exit 1
fi

# 🔴 AC0, ASSERTED RATHER THAN OBSERVED ONCE BY HAND. `bindings = "bin"` means the distribution
# carries an executable and NO importable module; until this check existed, a wheel that kept the
# binary and also shipped a `tooprolix/` package passed every assertion here (measured: repacked
# such a wheel, `install-smoke: OK`, exit 0). That is exactly how a provisional Python surface gets
# published by accident, which is the thing epic 2 Decisions #19.1 removed on purpose.
#
# The probe reports what it FOUND rather than swallowing an exception, and it covers the case a
# plain `try: import` would miss: a bare `tooprolix/` directory with no `__init__.py` is still
# importable as a namespace package, and `find_spec` is what sees it.
echo '$ python -c "import tooprolix"   (must raise ModuleNotFoundError)'
import_probe="$(uv run python -c '
import importlib.util
spec = importlib.util.find_spec("tooprolix")
if spec is not None:
    print("IMPORTABLE:" + (spec.origin or "<namespace package>"))
else:
    try:
        import tooprolix  # noqa: F401
    except ModuleNotFoundError:
        print("absent")
    else:
        print("IMPORTABLE:<no spec but imported>")
')"
printf '%s\n\n' "$import_probe"
if [ "$import_probe" != "absent" ]; then
	echo "FAIL: the distribution installed an importable module: $import_probe" >&2
	echo '      The wheel ships a native executable and nothing else: `import tooprolix` must' >&2
	echo "      raise ModuleNotFoundError. A module here is an accidental public API." >&2
	exit 1
fi

run_check 1 'TPX002' uv run tooprolix check "$repo/tests/fixtures/broken/long_docstring.py"
run_check 0 '' uv run tooprolix check "$repo/tests/fixtures/clean"

echo "install-smoke: OK"
