//! The detectors: they consume the block contract from [`crate::extract`] and compare.
//!
//! Extraction owns *what a prose block is*; a detector owns *what makes prose a finding*. Neither
//! redefines the other's half, and no detector walks the filesystem — [`crate::cli`] owns the walk.
//!
//! # There are two finding shapes, and that is deliberate
//!
//! A finding is **not** always a pair, and a detector must not be built assuming it is:
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
//! So the JSON schema carries two forms, not one, and a consumer must not assume every finding has
//! more than one address.

pub mod duplicate;
pub mod volume;
