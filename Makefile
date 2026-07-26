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
	rust.fmt rust.fmt.check rust.lint rust.test py.build

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

rust.lint: ## Lint the Rust code with clippy, warnings are errors
	@$(FIND_PYTHON) $(CARGO) clippy --all-targets --locked -- -D warnings

rust.test: ## Run the Rust tests (unit + doctests)
	@$(FIND_PYTHON) $(CARGO) test --locked

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
