//! The `tooprolix` executable — a wrapper, and deliberately nothing else.
//!
//! Every line of the command line lives in [`tooprolix::cli`]: argument parsing, the walk, the
//! rendering and the choice of exit code. The wheel carries this very executable under
//! `*.data/scripts/`, so there is no second entry point anywhere and no feature gate.
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
