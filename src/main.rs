//! The `tooprolix` executable — a wrapper, and deliberately nothing else.
//!
//! Every line of the command line lives in [`tooprolix::cli`]: argument parsing, the walk, the
//! rendering and the choice of exit code. That is a requirement of
//! `package-python-distribution-and-publish-0-1-0`, which still has to choose between shipping a
//! standalone binary, gating pyo3 behind a feature, and running the CLI inside `CPython` through a
//! console script. With the logic in the library this file is the *only* thing any of those three
//! changes, so the decision costs nothing to defer. `proj-lib-main-split` in rust-skills is the
//! same rule stated generally.
//!
//! Returning [`std::process::ExitCode`] rather than calling [`std::process::exit`] so that stdout
//! is flushed on the way out — `exit` does not run destructors, and a piped `--format json` can
//! lose its last buffer that way.

fn main() -> std::process::ExitCode {
    // `.skip(1)` here and not inside the library: dropping `argv[0]` is a convention of *this*
    // entry point, and hiding it in the parser meant a caller that built its arguments by hand
    // silently lost the first one.
    tooprolix::cli::run(std::env::args_os().skip(1))
}
