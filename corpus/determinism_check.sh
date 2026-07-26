#!/usr/bin/env bash
#
# AC2 — the tool is deterministic and never asks a model anything.
#
# Three claims, each proved against an artifact rather than against a description of one:
#
#   1. **Byte-identical output.** Repeated runs of the shipped binary over the same checkout, in
#      both output formats, compared with `cmp`. Deliberately on a repository with **many**
#      findings: "the runs agreed" is trivially true of empty outputs, and this epic's verification
#      policy counts a check that passed on an empty detector output as RED, not green. The
#      minimum is asserted before the comparison, so an empty run fails here instead of passing.
#
#      **Five passes, not two, and the number was chosen by a mutation rather than by taste.**
#      Replacing the output sort with a per-process coin flip — real run-to-run nondeterminism —
#      left a two-pass comparison *green*: two flips agree half the time. Five passes miss it once
#      in sixteen. A flaky guard that passes half the time is worse than none, because it is the
#      half that gets believed.
#
#   2. **No network at run time.** The run is repeated under `sandbox-exec` with `(deny network*)`.
#      A sandbox that denies nothing would pass this vacuously, so the sandbox itself is tested
#      first: `curl` must fail inside it and succeed outside it. If that control does not
#      distinguish, the check aborts rather than reporting a pass.
#
#   3. **No network or ML crate is linked in at all.** `Cargo.lock` is the committed dependency
#      set, and it is grepped for a named deny list. The grep is likewise proved able to fail: it
#      is run once against a synthetic lock file carrying `reqwest`, and must report it.
#
# Usage:
#   CORPUS_ROOT=/somewhere/outside ./corpus/determinism_check.sh
#
# `CORPUS_ROOT` carries the same requirement as `corpus/run_all.sh` — outside
# an ancestor `.gitignore` listing `lib/`, which the walk collects. See that
# file's header for the measurement.
#
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly BINARY="${TOOPROLIX_BIN:-$REPO_ROOT/target/release/tooprolix}"
readonly LOCKFILE="$REPO_ROOT/Cargo.lock"

# The subject of the double run. `pydantic` is mature human-written OSS with 167 findings across
# all three rule codes, so the comparison is over a large, mixed, non-empty output.
readonly SUBJECT="${DETERMINISM_SUBJECT:-pydantic}"
readonly MIN_FINDINGS=10

# Crates that would mean a network call or a model. Matched against the `name = "…"` entries of
# Cargo.lock as a whole name or a `<name>-<suffix>` family, so `openssl` catches `openssl-sys` and
# `hyper` catches `hyper-tls`, while a bare substring search cannot fire on an unrelated crate.
#
# The anchoring is not cosmetic: as a substring, `ort` matched `portable-atomic` and this script
# reported a network dependency that does not exist. A check that cries wolf gets switched off.
#
# `getrandom`/`rand` are NOT here on purpose: they are randomness, not network, they arrive through
# `tempfile`/`ahash`-style transitive use, and banning them would make this list look strict while
# saying nothing. Determinism of the *output* is claim 1's job, proved by `cmp` on real output.
readonly FORBIDDEN=(
	reqwest hyper ureq curl isahc surf attohttpc
	tokio async-std smol mio socket2 trust-dns hickory
	tonic grpc prost-build h2 quinn
	rustls native-tls openssl-sys
	onnx ort tract candle torch tch llama ggml llm-chain
	tiktoken tokenizers hf-hub openai anthropic
)

if [[ -z "${CORPUS_ROOT:-}" ]]; then
	echo "error: set CORPUS_ROOT to the directory holding the pinned checkouts (outside <parent>)." >&2
	exit 2
fi
if [[ ! -x "$BINARY" ]]; then
	echo "error: $BINARY is not executable; build it with" >&2
	echo "       PYO3_PYTHON=\"\$(uv python find)\" cargo build --release --locked" >&2
	exit 2
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

failures=0
fail() {
	echo "FAIL $*" >&2
	failures=$((failures + 1))
}

# ---------------------------------------------------------------------------
# 1. Byte-identical output, on a non-empty finding set.
# ---------------------------------------------------------------------------
readonly PASSES=5
echo "== 1. $PASSES runs on $SUBJECT =="
for format in text json; do
	for ((pass = 1; pass <= PASSES; pass++)); do
		(cd "$CORPUS_ROOT" && "$BINARY" check "$SUBJECT" --format "$format") \
			>"$work/$format.$pass" 2>"$work/$format.$pass.err" || true
	done
done

# `jq` prints nothing when stdout was empty — a run that failed rather than a run with no findings.
# The two need different words: "produced  findings" is what this said before, and an operator
# reading it looks for a detector bug instead of a missing subject.
if ! findings="$(jq -e '.findings | length' "$work/json.1" 2>/dev/null)"; then
	findings="no document at all ($(head -c 200 "$work/text.1.err" | tr '\n' ' '))"
fi
if ! [[ "$findings" =~ ^[0-9]+$ ]] || ((findings < MIN_FINDINGS)); then
	fail "$SUBJECT produced $findings; determinism proved on fewer than $MIN_FINDINGS findings" \
		"is proved on nothing (epic verification policy)"
	# Fatal here rather than at the end: with no output the comparisons below would all report
	# "byte-identical (0 bytes)", which is precisely the green-on-an-empty-output this guard exists
	# to refuse, and a reader who skims the middle of the log would see three passes.
	exit 1
fi
echo "   $SUBJECT: $findings findings — a non-empty subject"

for format in text json; do
	identical=1
	for ((pass = 2; pass <= PASSES; pass++)); do
		if ! cmp -s "$work/$format.1" "$work/$format.$pass"; then
			fail "$format stdout differs between run 1 and run $pass"
			cmp "$work/$format.1" "$work/$format.$pass" >&2 || true
			identical=0
		fi
		if ! cmp -s "$work/$format.1.err" "$work/$format.$pass.err"; then
			fail "$format stderr differs between run 1 and run $pass"
			identical=0
		fi
	done
	if ((identical == 1)); then
		echo "   $format: $PASSES runs byte-identical ($(wc -c <"$work/$format.1" | tr -d ' ') bytes)"
	fi
done

# ---------------------------------------------------------------------------
# 2. The same run with the network denied.
# ---------------------------------------------------------------------------
echo "== 2. offline run =="
readonly SANDBOX='(version 1)(allow default)(deny network*)'
if ! command -v sandbox-exec >/dev/null; then
	fail "sandbox-exec is unavailable; the offline claim cannot be proved here (macOS only)"
elif sandbox-exec -p "$SANDBOX" /usr/bin/curl -s -m 10 -o /dev/null https://crates.io/ 2>/dev/null; then
	fail "the sandbox does not actually deny the network; every result under it would be vacuous"
elif ! /usr/bin/curl -s -m 10 -o /dev/null https://crates.io/; then
	echo "   SKIP: no network available outside the sandbox either, so the control cannot" >&2
	echo "         distinguish. Re-run with network up; not reported as a pass." >&2
	fail "the offline control could not be established"
else
	echo "   control: curl fails inside the sandbox and succeeds outside it"
	(cd "$CORPUS_ROOT" && sandbox-exec -p "$SANDBOX" "$BINARY" check "$SUBJECT" --format json) \
		>"$work/offline.json" 2>"$work/offline.err" || true
	if cmp -s "$work/json.1" "$work/offline.json"; then
		echo "   offline output is byte-identical to the networked run"
	else
		fail "the offline run differs from the networked run — something reached out"
	fi
fi

# ---------------------------------------------------------------------------
# 3. No network or ML crate in the committed dependency set.
# ---------------------------------------------------------------------------
echo "== 3. Cargo.lock =="
crate_names() { awk -F'"' '/^name = /{print $2}' "$1"; }

# Prove the matcher can fail before trusting it to pass.
printf 'name = "reqwest"\nname = "serde"\n' >"$work/poisoned.lock"
if ! crate_names "$work/poisoned.lock" | grep -qiE '^reqwest(-.*)?$'; then
	fail "the crate-name matcher cannot even see a planted 'reqwest'; its verdict means nothing"
else
	echo "   control: a planted 'reqwest' is detected"
fi

hits=0
for pattern in "${FORBIDDEN[@]}"; do
	if crate_names "$LOCKFILE" | grep -qiE "^${pattern}(-.*)?$"; then
		fail "Cargo.lock contains a crate matching '$pattern':" \
			"$(crate_names "$LOCKFILE" | grep -iE "^${pattern}(-.*)?$" | tr '\n' ' ')"
		hits=$((hits + 1))
	fi
done
if ((hits == 0)); then
	echo "   none of the ${#FORBIDDEN[@]} forbidden patterns matches any of" \
		"$(crate_names "$LOCKFILE" | wc -l | tr -d ' ') locked crates"
fi

echo
if ((failures > 0)); then
	echo "$failures determinism/offline check(s) failed." >&2
	exit 1
fi
echo "determinism, offline behaviour and the dependency set all hold."
