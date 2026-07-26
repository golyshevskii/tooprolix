#!/usr/bin/env bash
#
# Run the shipped CLI over every checkout `corpus/corpus.lock` pins and assert what it found.
#
# The raw JSON lands in `corpus/runs/<name>.json` and the stderr in `corpus/runs/<name>.err`.
# Those files are the evidence for AC1/AC2/AC4; every number in `corpus/REPORT.md` §7 is read
# out of them and nowhere else.
#
# ## Why this asserts instead of just recording
#
# A run that silently lost a repository is the failure this script exists to make loud. It is not
# hypothetical: with the checkouts left under such an ancestor, `check crewAI` walks
# **5** of its 1269 `.py` files, reports **1** finding, and exits **1** with an empty stderr —
# indistinguishable, from the outside, from a clean measurement. See the trap below.
#
# So each repository carries an expected exit code and an expected count per rule code, and a
# deviation is a red run naming the repository. The numbers are not this script's own opinion:
# the `TPX001`/`TPX002` columns are reproduced independently by `tests/volume_corpus.rs`
# (`cargo test --locked --release --test volume_corpus -- --ignored --nocapture`), which walks the
# tree through `std::fs` rather than through the `ignore` crate, and the `TPX003` columns are the
# cluster counts `change-finding-model-to-clusters` recorded. Two of the three disagreements
# between the two walks are explained in REPORT.md §7.4 rather than averaged away.
#
# ## 🔴 The trap, and why $CORPUS_ROOT must have no ancestor `.gitignore`
#
# The `ignore` crate collects `.gitignore` files from the **ancestors** of the start path, and
# `src/cli.rs` builds its walk with `require_git(false)`, so it does that whether or not any
# ancestor is a git repository. a stock Python-template
# ignore file in a directory that is **not** a repository, and its line 17 is `lib/`. Every
# `lib/` subtree under it therefore disappears from the walk — 1264 of crewAI's 1269 files.
#
# Proved with one file rather than argued: `<dir>/lib/big.py` holding a 326-word docstring gives
# `exit 1` and one `TPX002` under the scratchpad, and `warning: no Python files` with `exit 0`
# under it. It is **not** `tooprolix/.gitignore:33` (`corpus/checkouts/`),
# which is what this epic recorded until 2026-07-26; running from a different working directory
# does not help, because the rule is anchored at the path being checked. Copy the checkouts out.
#
# Usage:
#   CORPUS_ROOT=/somewhere/outside ./corpus/run_all.sh
#
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly BINARY="${TOOPROLIX_BIN:-$REPO_ROOT/target/release/tooprolix}"
readonly RUNS_DIR="$REPO_ROOT/corpus/runs"

if [[ -z "${CORPUS_ROOT:-}" ]]; then
	echo "error: set CORPUS_ROOT to the directory holding the pinned checkouts." >&2
	echo "       No ancestor directory may carry a .gitignore — see the trap in this file's header." >&2
	exit 2
fi
if [[ ! -d "$CORPUS_ROOT" ]]; then
	echo "error: CORPUS_ROOT=$CORPUS_ROOT is not a directory." >&2
	exit 2
fi
if [[ ! -x "$BINARY" ]]; then
	echo "error: $BINARY is not executable; build it with" >&2
	echo "       PYO3_PYTHON=\"\$(uv python find)\" cargo build --release --locked" >&2
	exit 2
fi
for tool in jq git rg; do
	if ! command -v "$tool" >/dev/null; then
		echo "error: $tool is required (jq counts findings, git checks the pins, rg counts the walk)." >&2
		exit 2
	fi
done

# name | walk root, relative to CORPUS_ROOT | exit | TPX001 | TPX002 | TPX003
#
# `crewAI-full` and `crewAI` are the same checkout at two roots, on purpose. The whole repository
# cannot be measured at all: five Jinja templates named `.py` under `lib/cli/src/crewai_cli/
# templates/` do not parse, and the exit-2 contract is all-or-nothing by the owner's decision of
# 2026-07-26, so the full run yields zero findings. Keeping it in the table pins that fact instead
# of leaving the narrowed root looking arbitrary. `crewAI/lib/crewai` is the narrowed root the
# owner chose (EPIC.md Decisions #7.2); the 515 `.py` files outside it are recorded in REPORT.md
# §7.2. A union of per-subdirectory runs was rejected: `TPX003` is cross-file, so that is a
# different answer, not a more complete one.
#
# `crewAI` appears twice above because it is one checkout measured at two roots, so the table has
# seven rows over the six checkouts `corpus/corpus.lock` pins.
# name | walk root | checkout | exit | .py walked | TPX001 | TPX002 | TPX003
readonly EXPECTED=(
	"OpenHands|OpenHands|OpenHands|1|914|3|12|65"
	"crewAI-full|crewAI|crewAI|2|1269|0|0|0"
	"crewAI|crewAI/lib/crewai|crewAI|1|754|0|16|90"
	"langgraph|langgraph|langgraph|1|445|2|74|264"
	"openai-agents-python|openai-agents-python|openai-agents-python|1|834|2|14|70"
	"pydantic|pydantic|pydantic|1|404|1|46|120"
	"requests|requests|requests|1|37|0|3|8"
)

# The five files `crewAI` ships as `.py` and the parser rejects — Jinja templates. Asserted by name
# rather than by count: "exit 2 with an empty stdout" is also what an unreadable tree or an
# unrelated fatal error looks like, and the owner approved *these* five, not exit 2 in general.
readonly CREWAI_UNPARSEABLE=(
	"crewAI/lib/cli/src/crewai_cli/templates/crew/crew.py"
	"crewAI/lib/cli/src/crewai_cli/templates/crew/main.py"
	"crewAI/lib/cli/src/crewai_cli/templates/flow/main.py"
	"crewAI/lib/cli/src/crewai_cli/templates/tool/src/{{folder_name}}/__init__.py"
	"crewAI/lib/cli/src/crewai_cli/templates/tool/src/{{folder_name}}/tool.py"
)

mkdir -p "$RUNS_DIR"

# ---------------------------------------------------------------------------
# The bytes being measured are the bytes `corpus/corpus.lock` pins.
#
# Without this the guard grades finding counts over an unknown input: a substituted or locally
# edited checkout that happens to produce the same counts stays green. `HEAD == sha` alone is NOT
# enough and this epic already measured that — a checkout with dirty or untracked `.py` files was
# accepted as pinned — so the worktree has to be clean too.
# ---------------------------------------------------------------------------
integrity_failures=0
while read -r repo want_sha; do
	[[ -z "$repo" ]] && continue
	checkout="$CORPUS_ROOT/$repo"
	if [[ ! -d "$checkout/.git" ]]; then
		echo "FAIL $repo: $checkout is not a git checkout, so its pin cannot be verified" >&2
		integrity_failures=$((integrity_failures + 1))
		continue
	fi
	got_sha="$(git -C "$checkout" rev-parse HEAD)"
	if [[ "$got_sha" != "$want_sha" ]]; then
		echo "FAIL $repo: HEAD is $got_sha, corpus.lock pins $want_sha" >&2
		integrity_failures=$((integrity_failures + 1))
	fi
	dirt="$(git -C "$checkout" status --porcelain)"
	if [[ -n "$dirt" ]]; then
		echo "FAIL $repo: worktree is not clean, so the pin does not describe the bytes:" >&2
		echo "$dirt" | head -5 >&2
		integrity_failures=$((integrity_failures + 1))
	fi
done < <(awk '!/^#/ && NF {n = split($1, parts, "/"); repo = parts[n]; sub(/\.git$/, "", repo); print repo, $2}' "$REPO_ROOT/corpus/corpus.lock")

if ((integrity_failures > 0)); then
	echo >&2
	echo "$integrity_failures checkout(s) are not the corpus corpus.lock pins; nothing was measured." >&2
	exit 1
fi
echo "corpus.lock: $(awk '!/^#/ && NF' "$REPO_ROOT/corpus/corpus.lock" | wc -l | tr -d ' ')" \
	"checkouts at their pinned SHAs, all worktrees clean"
echo

failures=0
printf '%-28s %6s %8s %8s %8s %8s\n' repo exit files TPX001 TPX002 TPX003

for row in "${EXPECTED[@]}"; do
	IFS='|' read -r name subpath checkout want_exit want_files want1 want2 want3 <<<"$row"

	if [[ ! -d "$CORPUS_ROOT/$subpath" ]]; then
		echo "FAIL $name: $CORPUS_ROOT/$subpath does not exist — the checkout did not materialise" >&2
		failures=$((failures + 1))
		continue
	fi

	# Run from CORPUS_ROOT with a relative path so the recorded JSON carries repo-relative paths
	# and is diffable between machines. `set -e` must not swallow the exit code we are asserting.
	# `rg --no-require-git` is the same `ignore` crate with the same settings the CLI walks with,
	# so this is the file set the run measured — recorded next to the finding counts because that is
	# what makes a silently truncated walk visible instead of plausible.
	# `|| true` because rg exits 1 on "no matching files", a legitimate answer for a root holding no
	# Python, which would otherwise kill the run under `set -o pipefail` without printing a FAIL.
	got_files="$(cd "$CORPUS_ROOT" && { rg --no-require-git --files --glob '*.py' "$subpath" 2>/dev/null || true; } | wc -l | tr -d ' ')"

	got_exit=0
	(cd "$CORPUS_ROOT" && "$BINARY" check "$subpath" --format json) \
		>"$RUNS_DIR/$name.json" 2>"$RUNS_DIR/$name.err" || got_exit=$?

	if [[ "$got_files" != "$want_files" ]]; then
		echo "FAIL $name: the walk saw $got_files .py files, expected $want_files" >&2
		failures=$((failures + 1))
	fi

	# Count from the JSON document rather than from the exit code or from stdout line counts: the
	# artifact is what the next task reads, so the artifact is what gets graded.
	#
	# An exit-2 run writes **nothing at all** to stdout — not an empty document, no bytes. That is
	# the all-or-nothing contract being visible in the artifact, so it is asserted rather than
	# described: an empty stdout counts as three zeros only when the run failed, and a failed run
	# that nevertheless printed a document is a contract break. That used to be a comment claiming
	# the second half while the code let `{"findings":[]}` match a 0/0/0 row and pass.
	if [[ -s "$RUNS_DIR/$name.json" && "$got_exit" == 2 ]]; then
		echo "FAIL $name: exit 2 but stdout holds a document; a tree that was not fully measured" \
			"must report no findings at all" >&2
		failures=$((failures + 1))
		continue
	fi

	if [[ ! -s "$RUNS_DIR/$name.json" && "$got_exit" == 2 ]]; then
		got1=0 got2=0 got3=0
		printf '%-28s %6s %8s %8s %8s %8s\n' "$name" "$got_exit" "$got_files" "$got1" "$got2" "$got3"
		if [[ "$got_exit" != "$want_exit" || "0|0|0" != "$want1|$want2|$want3" ]]; then
			echo "FAIL $name: expected exit=$want_exit TPX001=$want1 TPX002=$want2 TPX003=$want3," \
				"got exit=2 and no output document" >&2
			failures=$((failures + 1))
		fi
		if [[ "$name" == "crewAI-full" ]]; then
			named="$(grep -c 'could not parse Python source' "$RUNS_DIR/$name.err" || true)"
			if [[ "$named" != "${#CREWAI_UNPARSEABLE[@]}" ]]; then
				echo "FAIL $name: stderr names $named unparsable files, expected ${#CREWAI_UNPARSEABLE[@]}" >&2
				failures=$((failures + 1))
			fi
			for template in "${CREWAI_UNPARSEABLE[@]}"; do
				if ! grep -qF "$template: could not parse" "$RUNS_DIR/$name.err"; then
					echo "FAIL $name: exit 2 was not the approved Jinja-template failure —" \
						"$template is not named in stderr" >&2
					failures=$((failures + 1))
				fi
			done
		fi
		continue
	fi

	read -r got1 got2 got3 <<<"$(jq -r '
		[.findings[].code] as $codes |
		[ ($codes | map(select(. == "TPX001")) | length),
		  ($codes | map(select(. == "TPX002")) | length),
		  ($codes | map(select(. == "TPX003")) | length) ] | @tsv
	' "$RUNS_DIR/$name.json" 2>/dev/null || echo "?	?	?")"

	printf '%-28s %6s %8s %8s %8s %8s\n' "$name" "$got_exit" "$got_files" "$got1" "$got2" "$got3"

	if [[ "$got_exit" != "$want_exit" || "$got1" != "$want1" || "$got2" != "$want2" || "$got3" != "$want3" ]]; then
		echo "FAIL $name: expected exit=$want_exit TPX001=$want1 TPX002=$want2 TPX003=$want3," \
			"got exit=$got_exit TPX001=$got1 TPX002=$got2 TPX003=$got3" >&2
		failures=$((failures + 1))
	fi
done

if ((failures > 0)); then
	echo >&2
	echo "$failures repository/repositories deviated from the recorded measurement." >&2
	echo "A run that lost a repository is red, not quiet. Check CORPUS_ROOT for the .gitignore" >&2
	echo "trap in this file's header before assuming the detector changed." >&2
	exit 1
fi

echo
echo "all $(( ${#EXPECTED[@]} )) runs matched the recorded measurement; raw JSON in corpus/runs/"
