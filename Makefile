# Use bash from PATH instead of /bin/sh — uniform behaviour on macOS/Linux/CI.
SHELL := /usr/bin/env bash
# -e fail on first error, -u error on unset vars, -o pipefail propagate pipe failures.
.SHELLFLAGS := -euo pipefail -c
# `make` with no target shows help, not the first recipe.
.DEFAULT_GOAL := help

UV ?= uv
# The toolchain is pinned in rust-toolchain.toml, so plain `cargo` already resolves to 1.97.0.
CARGO ?= cargo

# Paths the Python gates run on. This is a Rust project with one Python corner: the
# throwaway corpus measurement.
LINT_PATHS ?= corpus tests/unit
TY_PATHS ?= corpus tests/unit

# Corpus measurement inputs.
LOCK ?= corpus/corpus.lock

.PHONY: help lint.fix lint.check type test corpus.measure \
	rust.fmt rust.fmt.check rust.lint rust.test rust.build.nopython py.build

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make <target>\n\nTargets:\n"} \
	/^[a-zA-Z_.-]+:.*##/ {printf "\033[36m%-16s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# `--only-group`, not `--group`, in all three Python gates below. It installs the group WITHOUT
# building this project. This is a LIVE constraint, not a precaution: the maturin `[build-system]` is
# in pyproject.toml now, so `--group` makes uv attempt an editable build of the Rust extension and
# every Python gate dies with "'--editable'] returned non-zero exit status 1". Do not "fix" these to
# `--group`; verified consequence, not style.
lint.fix: ## Format and autofix the Python code with ruff (LINT_PATHS)
	@$(UV) run --only-group lint ruff format $(LINT_PATHS)
	@$(UV) run --only-group lint ruff check --fix $(LINT_PATHS)

lint.check: ## Check formatting and lint rules with ruff (LINT_PATHS) — CI mode
	@$(UV) run --only-group lint ruff format --check $(LINT_PATHS)
	@$(UV) run --only-group lint ruff check $(LINT_PATHS)

type: ## Check types with ty (TY_PATHS)
	@# Both groups, because TY_PATHS includes tests/unit and those files import pytest.
	@# With only the `type` group, ty cannot resolve that import and reports
	@# `unresolved-import` — which looked green here only because a leftover .venv already
	@# had pytest in it. From a clean checkout, i.e. in every isolated CI job, it fails.
	@$(UV) run --only-group type --only-group test ty check $(TY_PATHS)

test: ## Run the Python tests (pytest, tests/unit)
	@$(UV) run --only-group test pytest

corpus.measure: ## Measure the pinned prose corpus and print the distributions
	@uv run python3 corpus/measure.py --lock $(LOCK)

# The three Rust gates below are one CI job each (cargo-fmt / cargo-clippy / cargo-test) and are
# what every later task has to keep green. `--locked` everywhere: Cargo.lock is committed, so a
# gate that silently re-resolved it would not be testing the code that CI builds.
#
# FIND_PYTHON exists because pyo3-ffi's build script locates CPython by scanning PATH, so any cargo
# command that COMPILES the crate silently depends on whichever `python3` comes first. That is an
# accident in both places it matters, and it bites in opposite directions:
#   - locally, this repo's agent harness puts a `python3` shim first that refuses to run, so a fresh
#     clone fails with `failed to run custom build command for pyo3-ffi` / `no Python 3.x interpreter
#     found` (measured: `cargo clippy --all-targets --locked` exits 101);
#   - in CI, it resolves to whatever the runner image happens to ship. `astral-sh/setup-uv`'s
#     `python-version` input does NOT install an interpreter — per its README it only sets `UV_PYTHON`,
#     which pyo3 ignores — so a green job there would be luck, not configuration.
# `uv python find` removes the guesswork from both: uv is already this project's Python, and it
# prefers the project's own `.venv` before falling back to an installed interpreter.
#
# The `-z` test is NOT redundant belt-and-braces. `.SHELLFLAGS` (and therefore `set -e`) is silently
# IGNORED by the GNU Make 3.81 that ships with macOS — measured, `$-` inside a recipe is `hBc`, not
# `ehuBc`. Without an explicit check, a failed lookup would run cargo with an empty PYO3_PYTHON and
# fall straight back through to pyo3's own confusing error.
#
# `rust.fmt`/`rust.fmt.check` deliberately do NOT use it: rustfmt parses source and builds no
# dependencies, so it needs no interpreter (verified — fmt passes on a fresh clone where clippy fails).
FIND_PYTHON = PYO3_PYTHON="$$($(UV) python find || true)"; \
	if [ -z "$$PYO3_PYTHON" ]; then \
		echo "error: no CPython found for the pyo3 build script; '$(UV) python find' resolved nothing." >&2; \
		echo "       install one with '$(UV) python install 3.12', or set PYO3_PYTHON yourself." >&2; \
		exit 1; \
	fi; \
	export PYO3_PYTHON;

rust.fmt: ## Format the Rust code with rustfmt
	@$(CARGO) fmt

rust.fmt.check: ## Check Rust formatting without writing (CI mode)
	@$(CARGO) fmt --check

# `--features python` on BOTH: since `ship-v0-1-0-delivery-and-release` the pyo3 boundary is behind
# an off-by-default feature, and with the feature off `cargo test` compiles none of the 5 boundary
# tests in src/lib.rs. Measured 2026-07-27 by removing this flag: `make rust.test` stayed **exit 0**
# and reported **128 passed** where it had been 133 — green by testing less, with no other signal.
#
# That silence is why the flag is not the only thing holding the line: `src/lib.rs` carries a
# `#[cfg(all(test, not(feature = "python")))] compile_error!`, so removing the flag from either
# recipe below is now a build failure rather than a smaller number. Both halves are wanted — the
# flag makes the gate right, the `compile_error!` makes deleting the flag loud.
#
# It lives HERE and not in `.github/workflows/ci.yml` — which is what the task asked for — because
# ci.yml contains zero direct `cargo` invocations: all six jobs shell out to these recipes
# Measured, because this repo's comments are facts: `grep -c "run: make" .github/workflows/ci.yml`
# prints **7** — one per job, plus the second `make` step (`rust.build.nopython`) in `cargo-clippy` —
# and `grep -cE '^\s+run:.*cargo'` prints **0**. Putting the flag in the one place both callers go
# through is also what AC3 actually wants ("the Rust test count in CI equals the local
# `cargo test --features python` count") — true by construction, not by two edits staying in sync.
rust.lint: ## Lint the Rust code with clippy, warnings are errors
	@$(FIND_PYTHON) $(CARGO) clippy --all-targets --locked --features python -- -D warnings

rust.test: ## Run the Rust tests (unit + doctests)
	@$(FIND_PYTHON) $(CARGO) test --locked --features python

# The OTHER half of the feature gate, and the only thing in CI that compiles it: with `--features
# python` on both gates above, nothing would ever build the configuration the standalone binary is
# actually shipped in, so a stray `use pyo3::…` outside the `#[cfg]` would break `cargo build` and
# no gate would say so.
#
# 🔴 IT ASSERTS THE LINKAGE, NOT THE COMPILATION, and the difference is the whole guard. AC1 does
# not promise "it builds", it promises a binary that needs no interpreter — and `cargo build` alone
# cannot tell the two apart. Measured 2026-07-27: with `default = ["python"]` added to `[features]`
# and an interpreter on PATH, the build step exits **0** and blesses a binary whose `otool -L` reads
#
#     /opt/homebrew/opt/python@3.14/Frameworks/Python.framework/Versions/3.14/Python
#
# In CI that mutation would sail through, because this target is the last step of `cargo-clippy`,
# four steps after "Install a managed Python for the pyo3 build script" — an interpreter is always
# on PATH there. On a machine with no usable `python3` the build happens to fail, which is an
# accident of the environment and not the guard working. So the guard now reads the produced
# artifact and fails on any dynamic dependency naming Python.
#
# `otool -L` on macOS, `ldd` on Linux (both CI runners and this laptop are covered; nothing else
# runs it). Deliberately NOT a `cargo tree`/feature-graph check: that grades a proxy for the
# artifact, which is the same mistake as trusting the flag instead of the `compile_error!`.
#
# 🔴 THE PATH COMES FROM CARGO, and hardcoding it was a second instance of the very defect this
# target exists to close. `target/debug/tooprolix` is not where cargo necessarily writes: both
# `CARGO_TARGET_DIR` and `build.target-dir` in `.cargo/config.toml` move it. Measured 2026-07-27
# with `default = ["python"]` and `CARGO_TARGET_DIR` set elsewhere — cargo compiled a
# libpython-linked binary into the alternate directory while this recipe read a stale clean binary
# left over at `target/debug/`, printed `ok: ... links no libpython` and exited 0. The stale file is
# what made it silent. `--message-format=json` makes cargo name the executable it just linked, so
# the guard reads the artifact it actually produced and no reconstruction can drift from it.
#
# An empty listing is treated as a FAILURE, not a pass, and so is cargo reporting no executable at
# all. A missing or renamed `otool`/`ldd` would otherwise make the grep match nothing and the guard
# report success without having looked.
#
# No FIND_PYTHON: not needing an interpreter is the whole point, and this target failing without one
# would mean the gate had stopped testing what it is for.
rust.build.nopython: ## Build the standalone binary with the pyo3 feature OFF, and prove it links no libpython
	@json="$$($(CARGO) build --locked --message-format=json)"; \
	rc=$$?; \
	if [ $$rc -ne 0 ]; then exit $$rc; fi; \
	binary="$$(printf '%s\n' "$$json" | sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | tail -1)"; \
	if [ -z "$$binary" ]; then \
		echo "error: cargo reported no executable; the AC1 guard has nothing to inspect." >&2; \
		exit 1; \
	fi; \
	case "$$(uname -s)" in \
		Darwin) linked="$$(otool -L $$binary)" ;; \
		*)      linked="$$(ldd $$binary)" ;; \
	esac; \
	if [ -z "$$linked" ]; then \
		echo "error: could not read the dynamic dependencies of $$binary; the AC1 guard did not run." >&2; \
		exit 1; \
	fi; \
	if printf '%s\n' "$$linked" | grep -i python; then \
		echo "error: $$binary links the Python library above, built with the 'python' feature OFF." >&2; \
		echo "       AC1 promises a standalone binary that runs without an interpreter." >&2; \
		exit 1; \
	fi; \
	printf '%s\n' "$$linked"; \
	echo "ok: $$binary links no libpython"

# Rebuild the pyo3 extension into .venv and reinstall it. `--reinstall-package` is NOT optional, and
# it is the ONLY thing here that reliably rebuilds:
#   - a plain `uv sync` after a Rust edit answers "Checked 1 package" and keeps the previous binary;
#   - `rm -rf .venv && uv sync` is ALSO not enough — measured: with `src/lib.rs` edited to raise
#     RuntimeError, a full `rm -rf .venv && uv sync` still installed a wheel that raised ValueError,
#     because uv's build cache is not invalidated by the Rust source change.
# So any check of Rust behaviour from Python must go through this target (or `maturin develop`).
# Deleting the venv looks like a clean slate and is not one.
py.build: ## Rebuild and reinstall the Rust extension into .venv
	@$(UV) sync --reinstall-package tooprolix
