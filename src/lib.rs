//! Prose-budget linting for Python source code.
//!
//! The contract this crate is built on — *what is one prose block?* — lives in [`extract`], which
//! is its single owner. [`detect`] owns the other half — *what makes prose a finding* — in two
//! shapes, not one: a cluster of blocks that say the same thing ([`detect::duplicate`]) and a
//! single block that is too long ([`detect::volume`]).
//!
//! Three modules sit on top of those two: [`rules`] is the `TPX` code namespace and the opt-out
//! marker that switches one off,
//! [`finding`] is the owned finding the JSON schema is the shape of, and [`cli`] is the walk, the
//! exit contract and the rendering. All of the command line lives there rather than in
//! `src/main.rs`, which is what let the delivery decision be made without rewriting any of it.
//!
//! # There is no Python API, and that is a decision rather than an omission
//!
//! **The wheel carries the native executable** (`[tool.maturin] bindings = "bin"`, as ruff does),
//! so `import tooprolix` raises `ModuleNotFoundError` and the delivered artifact is the *same kind
//! of thing* every corpus number was measured on — `corpus/run_all.sh`,
//! `corpus/determinism_check.sh` and `corpus/bench.py` all drive `target/release/tooprolix`, and
//! that is a standalone binary from this source under `[profile.release]`, not an extension module.
//!
//! Measured 2026-07-31 at `72fda14` on macOS/arm64: `maturin build --release` copies the cargo
//! artifact unchanged, so the wheel's `*.data/scripts/tooprolix` and `target/release/tooprolix`
//! shared sha256 `b771fe92…`. A *published* wheel's executable is a different build: CI supplies a
//! per-platform `SOURCE_DATE_EPOCH`, and `build.rs` embeds the date, so the same source rebuilt
//! with `SOURCE_DATE_EPOCH=1000000000` gave `004e52d1…`.
//!
//! The wheel is checked to actually export something by `scripts/install-smoke.sh`, which installs
//! each built artifact into a clean environment and runs the *command*.
//!
//! [`finding::Finding`] owns its data and holds no prose, so a future Python API would not need it
//! redesigned; but nothing in this crate promises one.

pub mod cli;
pub mod config;
pub mod detect;
pub mod extract;
pub mod finding;
pub mod rules;

pub use crate::extract::Error;
