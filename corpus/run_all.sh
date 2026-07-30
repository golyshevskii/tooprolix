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
# So each repository carries an expected exit code, an expected number of **skipped** files and an
# expected count per rule code, and a deviation is a red run naming the repository.
#
# The skipped column is not decoration. Since `make-check-graceful-on-unreadable-files` a file the
# tool cannot read no longer stops the run — it is reported and the rest of the tree is still
# measured — so "this repository quietly stopped being readable" is now a thing that can happen
# without the exit code moving at all. The column is the only place that becomes visible, and the
# document's own `complete` flag is cross-checked against it on every row: a report that claims to
# be whole while carrying skipped files is a contract break, not a count that drifted.
#
# The numbers are not this script's own opinion:
# the `TPX001`/`TPX002` columns are reproduced independently by `tests/volume_corpus.rs`
# (`cargo test --locked --release --test volume_corpus -- --ignored --nocapture`), which walks the
# tree through `std::fs` rather than through the `ignore` crate, and the `TPX003` columns are the
# cluster counts `change-finding-model-to-clusters` recorded. Two of the three disagreements
# between the two walks are explained in REPORT.md §7.4 rather than averaged away.
#
# ## The trap, and why $CORPUS_ROOT must have no ancestor `.gitignore`
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
BINARY="${TOOPROLIX_BIN:-$REPO_ROOT/target/release/tooprolix}"
# Resolved to an absolute path HERE, before the `-x` check below, because the guard and the run do
# not share a working directory: the check runs in the caller's cwd, while every invocation happens
# inside `(cd "$CORPUS_ROOT" && "$BINARY" …)` and a relative program path resolves against the
# child's cwd. Measured with a stub at each location, both named `bin/tooprolix`:
#     GUARD PASSED for: bin/tooprolix     <- blessed the caller-cwd copy
#     I AM B (corpus-root copy)           <- ran the other one
# The guard blessed one binary and the measurement timed another, silently, and the numbers land in
# corpus/runs/ and REPORT.md. `$PWD/` rather than `realpath`, which is not present on a stock macOS.
# `corpus/bench.py` does the same with `Path.absolute()`, so all three runners land on one file.
if [[ "$BINARY" != /* ]]; then
	BINARY="$PWD/$BINARY"
fi
readonly BINARY
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

# `crewAI-full` and `crewAI` are the same checkout at two roots, on purpose. Five Jinja templates
# named `.py` under `lib/cli/src/crewai_cli/templates/` do not parse, and until
# `make-check-graceful-on-unreadable-files` that made the whole repository unmeasurable: the
# all-or-nothing contract of 2026-07-26 turned the full run into exit 2 with zero findings, and
# `corpus/runs/crewAI-full.json` was **0 bytes** on disk for exactly that reason.
#
# 🔴 **The `crewAI-full` row is a first measurement, not a corrected one.** Its old `2|1269|0|0|0`
# recorded what the refusal printed, not what the repository contains — the 1264 readable files had
# never been measured by anything. The numbers here were produced by running this script on
# 2026-07-28, after the graceful contract landed, and read out of the regenerated artifact.
#
# Keeping the row pins that the five templates are still the only thing lost. `crewAI/lib/crewai` is
# the narrowed root the owner chose (EPIC.md Decisions #7.2); the 515 `.py` files outside it are
# recorded in REPORT.md §7.2. A union of per-subdirectory runs was rejected: `TPX003` is cross-file,
# so that is a different answer, not a more complete one — which is also why `crewAI-full` and
# `crewAI` do not add up and are not supposed to.
#
# `crewAI` appears twice because it is one checkout measured at two roots, so the table has seven
# rows over the six checkouts `corpus/corpus.lock` pins.
# name | walk root | checkout | exit | .py walked | skipped | TPX001 | TPX002 | TPX003
#
# 🔴 The TPX003 column moved with `exclude-reference-scaffolding-from-tpx003` and the old numbers
# are kept here so the change is legible rather than merely applied. TPX003 now compares the
# NARRATIVE remainder of a block, so a docstring that repeats only its parameter table is no longer
# a finding, and two docstrings whose summaries match once their tables are gone become an *exact*
# cluster instead of a near one. Measured 2026-07-30 with the checkouts outside
# `/Users/vgolyshevskii/dwh`:
#
#   repo                  TPX003 before -> after   near before -> after   exact before -> after
#   OpenHands                     65 -> 64                28 -> 21               37 -> 43
#   crewAI-full                  118 -> 127               81 -> 73               37 -> 54
#   crewAI                        90 -> 94                58 -> 51               32 -> 43
#   langgraph                    264 -> 260               88 -> 47              176 -> 213
#   openai-agents-python          70 -> 72                13 -> 12               57 -> 60
#   pydantic                     120 -> 121               31 -> 27               89 -> 94
#   requests                       8 -> 6                  6 -> 2                 2 -> 4
#
# TPX001 and TPX002 are unchanged in every row, which is the check that the change stayed inside
# the duplicate rule: `narrative` is a second field beside `normalized`, and volume still counts the
# whole block.
#
# ## 2026-07-30 — `close-anti-fp-gate-with-public-reference` made relational operators survive
#
# `extract::normalize_comparable` keeps `<`, `>` and `=` as words on the path `TPX003` compares,
# because erasing them made `with size 0` and `with size > 0` the *same text* at similarity 1.000
# (`corpus/annotations.md` §4.7, record 18). Two rows move, and only in `TPX003`:
#
#   repo                  TPX003 before -> after   near before -> after   exact before -> after
#   crewAI-full                  127 -> 128               73 -> 74               54 -> 54
#   pydantic                     121 -> 122               27 -> 28               94 -> 94
#   requests                       6 -> 6                  2 -> 3                 4 -> 3
#   (the other four rows are unchanged in every column)
#
# Totals over the six `corpus.lock` rows: near **160 -> 162**, exact **457 -> 456**,
# total **617 -> 618**. One cluster appeared, none disappeared, 16 changed score and none changed
# membership — 17 of 617 touched.
#
# 🔴 **`TPX001` and `TPX002` are byte-identical on all seven rows, and that is the load-bearing
# check rather than a footnote.** The first version of this fix applied the operator rule inside
# `normalize` itself, which is the unit `size_words` counts in — and `OpenHands` `TPX001` went
# **3 -> 35** and `langgraph` `TPX002` **74 -> 79**, because an operator that survives is an operator
# the 150/200 limits suddenly count. Splitting the counting form from the comparison form is what
# holds those columns still, and this table is what would catch them moving again.
readonly EXPECTED=(
	"OpenHands|OpenHands|OpenHands|1|914|0|3|12|64"
	"crewAI-full|crewAI|crewAI|1|1269|5|0|21|128"
	"crewAI|crewAI/lib/crewai|crewAI|1|754|0|0|16|94"
	"langgraph|langgraph|langgraph|1|445|0|2|74|260"
	"openai-agents-python|openai-agents-python|openai-agents-python|1|834|0|2|14|72"
	"pydantic|pydantic|pydantic|1|404|0|1|46|122"
	"requests|requests|requests|1|37|0|0|3|6"
)

# The five files `crewAI` ships as `.py` and the parser rejects — Jinja templates. Asserted by name
# rather than by count, and now against the JSON `skipped[]` rather than against stderr: a count
# alone is also what five *different* unreadable files would produce, and the owner approved
# **these** five. They are the reason `crewAI-full` is the one row in this corpus with
# `complete: false`.
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
printf '%-28s %6s %8s %8s %8s %8s %8s\n' repo exit files skipped TPX001 TPX002 TPX003

for row in "${EXPECTED[@]}"; do
	IFS='|' read -r name subpath checkout want_exit want_files want_skipped want1 want2 want3 <<<"$row"

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

	# Exit 2 now means only that the run could not start — a bad path or a broken configuration —
	# and such a run still writes nothing at all to stdout. No row expects it any more, so reaching
	# it is itself the failure, reported here because `jq` on an empty file would otherwise print
	# three question marks and bury the reason on the row below.
	#
	# This replaces two branches that grew around the old all-or-nothing contract, where exit 2 was
	# the *expected* answer for `crewAI-full` and an empty stdout counted as three zeros.
	if [[ "$got_exit" == 2 ]]; then
		echo "FAIL $name: exit 2 — the run could not start at all. stderr:" >&2
		head -3 "$RUNS_DIR/$name.err" >&2
		failures=$((failures + 1))
		continue
	fi

	# Count from the JSON document rather than from the exit code or from stdout line counts: the
	# artifact is what the next task reads, so the artifact is what gets graded. `complete` is read
	# out of the same document rather than inferred from `skipped`, precisely so the two can be
	# compared against each other below.
	read -r got1 got2 got3 got_skipped got_complete <<<"$(jq -r '
		[.findings[].code] as $codes |
		[ ($codes | map(select(. == "TPX001")) | length),
		  ($codes | map(select(. == "TPX002")) | length),
		  ($codes | map(select(. == "TPX003")) | length),
		  (.skipped | length),
		  (.complete | tostring) ] | @tsv
	' "$RUNS_DIR/$name.json" 2>/dev/null || echo "?	?	?	?	?")"

	printf '%-28s %6s %8s %8s %8s %8s %8s\n' \
		"$name" "$got_exit" "$got_files" "$got_skipped" "$got1" "$got2" "$got3"

	if [[ "$got_exit" != "$want_exit" || "$got_skipped" != "$want_skipped" \
		|| "$got1" != "$want1" || "$got2" != "$want2" || "$got3" != "$want3" ]]; then
		echo "FAIL $name: expected exit=$want_exit skipped=$want_skipped TPX001=$want1" \
			"TPX002=$want2 TPX003=$want3, got exit=$got_exit skipped=$got_skipped" \
			"TPX001=$got1 TPX002=$got2 TPX003=$got3" >&2
		failures=$((failures + 1))
	fi

	# The flag against the list it is supposed to summarise. Graded here rather than trusted because
	# a document that says it is whole while carrying skipped files is the one failure that would
	# otherwise be invisible: the counts above still match, and the exit code does not move.
	want_complete=true
	[[ "$got_skipped" != 0 ]] && want_complete=false
	if [[ "$got_complete" != "$want_complete" ]]; then
		echo "FAIL $name: the document reports complete=$got_complete beside $got_skipped" \
			"skipped file(s)" >&2
		failures=$((failures + 1))
	fi

	# The approved five, by name. Only `crewAI-full` has any, and this is what pins them to being
	# the SAME five rather than merely five of something.
	if [[ "$name" == "crewAI-full" ]]; then
		for template in "${CREWAI_UNPARSEABLE[@]}"; do
			if ! jq -e --arg p "$template" 'any(.skipped[]; .path == $p)' \
				"$RUNS_DIR/$name.json" >/dev/null; then
				echo "FAIL $name: $template is not in the document's skipped list, so the five" \
					"unreadable files are not the approved Jinja templates" >&2
				failures=$((failures + 1))
			fi
		done
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
