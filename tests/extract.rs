//! The public API, exercised from OUTSIDE the crate.
//!
//! `Cargo.toml` carries `crate-type = ["rlib"]` precisely so this file can link the library, and
//! until it existed nothing used that. It read `["cdylib", "rlib"]` until epic 2 Decisions #19.1 —
//! `cdylib` was the pyo3 extension module and went with it; the `rlib` is what this file needs and
//! is the reason the list is not simply deleted. Everything public was reachable only from
//! `mod tests` *inside* `src/extract.rs`, where privacy cannot fail — so narrowing any item to
//! `pub(crate)`, or dropping the `pub use crate::extract::Error` re-export, left fmt, clippy, every
//! unit test, the doctest and all three Python gates green while breaking the CLI task that has to
//! consume this contract.
//!
//! This test therefore touches **each public item once**. It is a compile-time guard first and a
//! behaviour test second: if it stops compiling, the API surface moved.

use std::path::Path;

use tooprolix::extract::{MIN_BLOCK_LINES, MIN_BLOCK_WORDS, ProseKind, extract, normalize};

#[test]
fn the_public_api_is_reachable_from_another_crate() {
    // Arrange — two adjacent own-line comments: 2 lines and 13 words, so the block clears both
    // halves of the size conjunction.
    let source = "# Retries are capped at three attempts here,\n\
                  # because the upstream service throttles us.\n";

    // Act
    let blocks = extract(Path::new("client.py"), source).expect("the source is valid Python");

    // Assert
    assert_eq!(blocks.len(), 1);
    let block = &blocks[0];
    assert_eq!(block.kind, ProseKind::Comment);
    assert_eq!(block.kind.as_str(), "comment");
    assert_eq!(block.path, Path::new("client.py"));
    assert_eq!((block.line_start, block.line_end), (1, 2));
    assert!(block.raw.starts_with("# Retries are capped"));
    assert_eq!(block.normalized, normalize(&block.raw));
    assert_eq!(block.size_lines(), 2);
    assert_eq!(block.size_words(), 13);
    assert!(block.size_lines() >= MIN_BLOCK_LINES && block.size_words() >= MIN_BLOCK_WORDS);
    assert!(block.is_large_enough());
}

/// The error type must be nameable and matchable from outside, through the crate-root re-export —
/// that is what a caller writes in its own `From` impl.
#[test]
fn the_error_type_is_reachable_through_the_crate_root() {
    let error: tooprolix::Error =
        extract(Path::new("notes.txt"), "").expect_err("only .py is a source");

    assert!(matches!(error, tooprolix::Error::UnsupportedSource(_)));
    assert!(error.to_string().contains("not a source"));
}
