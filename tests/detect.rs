//! The detectors' public API, exercised from OUTSIDE the crate.
//!
//! Same reason `tests/extract.rs` exists: everything public in `src/detect/*.rs` is also
//! reachable from `mod tests` *inside* that file, where privacy cannot fail. Narrowing any of these
//! items to `pub(crate)` would leave fmt, clippy, every unit test and all three Python gates green
//! while breaking `build-cli-with-exit-contract-and-rule-codes`, which has to render these findings.
//!
//! It touches **each public item once**, so it is a compile-time guard first and a behaviour test
//! second: if it stops compiling, the API surface moved.
//!
//! # `Display` is reached here, and deliberately not pinned byte for byte
//!
//! Both detectors document their `Display` as "for tests and diagnostics, **not** the user-facing
//! string", and this file used to `assert_eq!` its exact bytes from outside the crate — which made
//! the disowned string a published contract anyway. The two claims cannot both hold at an API
//! freeze, so `build-cli-with-exit-contract-and-rule-codes` chose: **`Display` stays a diagnostic**,
//! the user-facing line is [`tooprolix::finding::Finding`]'s, and that one is pinned byte for byte
//! in `tests/cli.rs` against the real process output.
//!
//! The calls stay, because they are what would stop compiling if `Display` were narrowed or
//! removed; only the byte-level expectations moved to the type that owns them. The determinism
//! probes *inside* the crate still compare these renderings, which is legitimate — they are
//! internal and they are comparing a rendering to itself, not publishing it.

use std::path::Path;

use tooprolix::detect::duplicate::{Cluster, SHINGLE_K, SIMILARITY_THRESHOLD, duplicates};
use tooprolix::detect::volume::{
    DEFAULT_COMMENT_MAX_VOLUME, DEFAULT_DOCSTRING_MAX_VOLUME, Limits, Overrun, volume,
};
use tooprolix::extract::extract;

#[test]
fn the_duplicate_api_is_reachable_from_another_crate() {
    // Arrange — one rationale, written as a comment run in one file, re-wrapped in another, and
    // once more with a word changed, so the cluster carries an exact edge and a near edge at once.
    let left = "# The retry budget here is deliberately small, and that matters because\n\
                # the upstream service rate limits us on every fourth call.\n";
    let right = "#   The retry budget here is deliberately\n\
                 #   small, and that matters because the\n\
                 #   upstream service rate limits us on every fourth call.\n";
    let reworded = "# The retry budget here is deliberately small, and that matters because\n\
                    # the upstream service rate limits us on every fourth request.\n";
    let mut blocks = extract(Path::new("client.py"), left).expect("the source is valid Python");
    blocks.extend(extract(Path::new("server.py"), right).expect("the source is valid Python"));
    blocks.extend(extract(Path::new("worker.py"), reworded).expect("the source is valid Python"));

    // Act
    let report = duplicates(&blocks);

    // Assert
    assert_eq!(SHINGLE_K, 3);
    assert!((SIMILARITY_THRESHOLD - 0.75).abs() < f64::EPSILON);
    assert_eq!(
        report.comparisons, 2,
        "the exact-text pair is connected arithmetically; only the two mixed pairs are scored"
    );
    assert_eq!(report.clusters.len(), 1, "got {:?}", report.clusters);

    let cluster: &Cluster<'_> = &report.clusters[0];
    let addresses: Vec<&Path> = cluster
        .members
        .iter()
        .map(|member| member.path.as_path())
        .collect();
    assert_eq!(
        addresses,
        vec![
            Path::new("client.py"),
            Path::new("server.py"),
            Path::new("worker.py"),
        ]
    );
    assert_eq!(cluster.weakest.0.path, Path::new("client.py"));
    assert_eq!(cluster.weakest.1.path, Path::new("worker.py"));
    assert!(cluster.weakest_score < 1.0 && cluster.weakest_score >= SIMILARITY_THRESHOLD);
    // `Display` is reached from outside the crate so that narrowing it would stop this file
    // compiling — but its BYTES are deliberately no longer pinned here. See the module header.
    let diagnostic = cluster.to_string();
    for address in ["client.py:1", "server.py:1", "worker.py:1"] {
        assert!(diagnostic.contains(address), "{diagnostic}");
    }
}

/// The volume detector's surface, including the one item the CLI must be able to **construct**.
///
/// `Limits` is the only type here a consumer builds rather than reads, so this builds one with a
/// struct literal on purpose: marking it `#[non_exhaustive]` would still compile inside the crate
/// and would break `build-cli-with-exit-contract-and-rule-codes` here, and nowhere else.
#[test]
fn the_volume_api_is_reachable_and_configurable_from_another_crate() {
    // Arrange — 232 normalised words of docstring, over the default limit of 200 and under a
    // limit a noisy repository might raise for itself.
    let source = format!("\"\"\"Overview.\n{}\"\"\"\n", "word ".repeat(231));
    let blocks = extract(Path::new("api.py"), &source).expect("the fixture is valid Python");

    // Act
    let report = volume(&blocks, Limits::default());
    let relaxed = volume(
        &blocks,
        Limits {
            comment_max_volume: DEFAULT_COMMENT_MAX_VOLUME,
            docstring_max_volume: 500,
        },
    );

    // Assert
    assert_eq!(DEFAULT_DOCSTRING_MAX_VOLUME, 200);
    assert_eq!(DEFAULT_COMMENT_MAX_VOLUME, 150);
    assert_eq!(report.overruns.len(), 1, "got {:?}", report.overruns);

    let overrun: &Overrun<'_> = &report.overruns[0];
    assert_eq!(overrun.block.path, Path::new("api.py"));
    assert_eq!(overrun.words, 232);
    assert_eq!(overrun.max_volume, DEFAULT_DOCSTRING_MAX_VOLUME);
    // Reached, not pinned — see the module header for why the bytes moved to `tests/cli.rs`.
    assert!(overrun.to_string().contains("api.py:1"), "{overrun}");
    assert!(
        relaxed.overruns.is_empty(),
        "a per-repository limit had no effect: {:?}",
        relaxed.overruns
    );
}
