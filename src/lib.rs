//! Prose-budget linting for Python source code.
//!
//! The contract this crate is built on — *what is one prose block?* — lives in [`extract`], which
//! is its single owner. [`detect`] owns the other half — *what makes prose a finding* — in two
//! shapes, not one: a cluster of blocks that say the same thing ([`detect::duplicate`]) and a
//! single block that is too long ([`detect::volume`]). The claim that used to stand here, that a
//! finding is always a cluster and never a single block, was written before the second detector
//! existed and is corrected in [`detect`]'s own documentation rather than quietly dropped.
//!
//! The three modules added by `build-cli-with-exit-contract-and-rule-codes` sit on top of those
//! two: [`rules`] is the `TPX` code namespace and the opt-out marker that switches one off,
//! [`finding`] is the owned finding the JSON schema is the shape of, and [`cli`] is the walk, the
//! exit contract and the rendering. All of the command line lives there rather than in
//! `src/main.rs`, which is what let the delivery decision be made without rewriting any of it.
//!
//! # There is no Python API, and that is a decision rather than an omission
//!
//! This file used to be the pyo3 boundary: an `impl From<Error> for PyErr`, a `#[pymodule]`
//! exporting `prose_blocks`, a `main` `#[pyfunction]` behind the `[project.scripts]` console
//! script, all behind an off-by-default `python` feature — and a promise, right here, that
//! `tooprolix.check(path) -> list[Finding]` was coming.
//!
//! Epic 2 Decisions #19.1 removed all of it. **The wheel now carries the native executable**
//! (`[tool.maturin] bindings = "bin"`, as ruff does), so `import tooprolix` raises
//! `ModuleNotFoundError` and the delivered artifact is byte-for-byte the standalone binary every
//! measurement in two epics was taken on — `corpus/run_all.sh`, `corpus/determinism_check.sh` and
//! `corpus/bench.py` all drive `target/release/tooprolix`. Keeping the extension module would have
//! published the one class of artifact no number here had ever been measured on.
//!
//! ⚠️ **The guard that went with it is replaced, not dropped.** The 5 boundary tests and the
//! `#[cfg(all(test, not(feature = "python")))] compile_error!` existed because a wheel that
//! exported nothing once passed every gate. Their role is now `scripts/install-smoke.sh`, which
//! installs each built artifact into a clean environment and runs the *command* — and which is
//! mutation-proved against a wheel with its executable removed.
//!
//! [`finding::Finding`] still owns its data and holds no prose, so a future Python API would not
//! need it redesigned; but nothing in this crate promises one.

pub mod cli;
pub mod config;
pub mod detect;
pub mod extract;
pub mod finding;
pub mod rules;

pub use crate::extract::Error;
