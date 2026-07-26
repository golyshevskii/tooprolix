//! The detectors: they consume the block contract from [`crate::extract`] and compare.
//!
//! Extraction owns *what a prose block is*; a detector owns *what makes prose a finding*. Neither
//! redefines the other's half, and no detector walks the filesystem —
//! `build-cli-with-exit-contract-and-rule-codes` owns the walk.
//!
//! # There are two finding shapes, and that is deliberate
//!
//! This paragraph used to say "a finding is never a pair … a second detector must emit that same
//! shape", written when the second detector was expected to be another *relational* one. It is
//! false now, and left corrected rather than deleted so nobody rebuilds the wrong type from it:
//!
//! * [`duplicate::Cluster`] is **relational** — a whole connected component of the similarity
//!   graph, so one rationale copied into `n` files is one finding with `n` addresses rather than
//!   `C(n, 2)` findings, which is the shape that keeps a licence header from producing 1 999 000
//!   findings. A component of fewer than two members is discarded by construction;
//! * [`volume::Overrun`] is **per block** — one block that is simply too long. It deliberately does
//!   **not** reuse `Cluster`: a single-member cluster is a contradiction in that type, and forcing
//!   one would mean either a permanently dead `weakest` pair or a members list that lies about why
//!   the finding exists.
//!
//! So `build-cli-with-exit-contract-and-rule-codes`'s schema carries two forms, not one, and a
//! consumer must not assume every finding has more than one address.

pub mod duplicate;
pub mod volume;
