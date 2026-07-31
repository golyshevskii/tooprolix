//! A deliberate panic, so `[profile.release]`'s panic strategy can be OBSERVED rather than read.
//!
//! `tests/cli.rs::a_panic_in_a_release_build_stays_a_code_and_keeps_its_output` runs this under
//! `--release` and asserts the two properties `panic = "abort"` destroys: the process dies with an
//! exit **code** rather than on a signal, and bytes still sitting in a buffer when the panic
//! happened are not lost.
//!
//! An example rather than a `[[bin]]`, deliberately: `bindings = "bin"` puts the package's one
//! binary target in the wheel and nothing else, so this never ships. It is also why the panic is
//! here and not behind a flag in `src/` — the tool has no panic path, and adding one so a test
//! could reach it would be production surface bought with test money.
use std::io::Write as _;

fn main() {
    let mut out = std::io::BufWriter::new(std::io::stdout());
    // 25 bytes into an 8 KiB buffer, so nothing reaches the fd yet, and NOT flushed or dropped
    // before the panic. Under `unwind` this writer is dropped while the stack unwinds and its
    // `Drop` flushes it; under `abort` the process is gone before any destructor runs and these
    // bytes never leave the process. That difference is the assertion.
    write!(out, "buffered-before-the-panic").expect("writing into a BufWriter's own buffer");
    panic!("deliberate: proving the panic strategy is unwind");
}
