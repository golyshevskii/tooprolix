//! Prose-budget linting for Python source code.
//!
//! The contract this crate is built on — *what is one prose block?* — lives in [`extract`], which
//! is its single owner. [`detect`] owns the other half — *what makes prose a finding* — in two
//! shapes, not one: a cluster of blocks that say the same thing ([`detect::duplicate`]) and a
//! single block that is too long ([`detect::volume`]). The claim that used to stand here, that a
//! finding is always a cluster and never a single block, was written before the second detector
//! existed and is corrected in [`detect`]'s own documentation rather than quietly dropped. This
//! file is the crate root and the pyo3 boundary.
//!
//! The three modules added by `build-cli-with-exit-contract-and-rule-codes` sit on top of those
//! two: [`rules`] is the `TPX` code namespace and the opt-out marker that switches one off,
//! [`finding`] is the owned finding the JSON schema is the shape of, and [`cli`] is the walk, the
//! exit contract and the rendering. All of the command line lives there rather than in
//! `src/main.rs`, so that `package-python-distribution-and-publish-0-1-0` can still choose how the
//! tool is delivered without rewriting any of it.
//!
//! The **Python API over findings** (`tooprolix.check(path) -> list[Finding]`) is still a later
//! task, and the one export below is deliberately about extraction rather than about findings.
//! [`finding::Finding`] is the type that will carry it: it owns its data and holds no prose, so it
//! is `'static` and can become a `#[pyclass]` without a redesign.

pub mod cli;
pub mod config;
pub mod detect;
pub mod extract;
pub mod finding;
pub mod rules;

use pyo3::exceptions::{PyFileNotFoundError, PyOSError, PyPermissionError, PyValueError};
use pyo3::{PyErr, prelude::*};

pub use crate::extract::Error;

impl From<Error> for PyErr {
    /// Maps each extraction failure onto the exception a Python caller would write a handler for.
    ///
    /// `Parse` and `UnsupportedSource` are `ValueError`: from Python's side they are both "you gave
    /// me something I cannot read prose from". `Io` is **not** — and that is the decision this
    /// `match` exists to force. A missing file reaching Python as `ValueError` would make
    /// `except FileNotFoundError` useless and put a filesystem failure in the same bucket as bad
    /// syntax. The `io::ErrorKind` is preserved through [`Error::Io`] precisely so the mapping can
    /// be made on it, and the three arms are the three a caller actually distinguishes; everything
    /// rarer keeps `OSError`, which is the base class of both of the others, so
    /// `except OSError` still catches all three.
    ///
    /// The `match` is load-bearing and is not decoration over a constant answer. [`Error`] is
    /// `#[non_exhaustive]`, which only forces a wildcard on *foreign* crates, so within this crate
    /// the match stays exhaustive — which is what made adding `Io` a **compile error right here**,
    /// at the one place a mapping decision belongs, instead of a silent mistyping no test can see.
    /// Measured before this match existed: adding an `Io` variant left all 73 tests green and
    /// mapped it to `ValueError`. It worked exactly as designed — this arm was written because the
    /// build failed, not because anybody remembered to write it.
    ///
    /// **Do not add a `_ =>` arm.** A wildcard re-arms precisely that defect.
    fn from(error: Error) -> Self {
        match &error {
            Error::Parse(_) | Error::UnsupportedSource(_) => {
                PyValueError::new_err(error.to_string())
            }
            Error::Io { source, .. } => match source.kind() {
                std::io::ErrorKind::NotFound => PyFileNotFoundError::new_err(error.to_string()),
                std::io::ErrorKind::PermissionDenied => {
                    PyPermissionError::new_err(error.to_string())
                }
                _ => PyOSError::new_err(error.to_string()),
            },
        }
    }
}

/// The `tooprolix` Python extension module.
#[pymodule]
mod tooprolix {
    use std::path::Path;

    use pyo3::prelude::*;

    /// Return the prose blocks of `source` as a list of
    /// `(kind, line_start, line_end, normalized)` tuples.
    ///
    /// Raises `ValueError` if `source` cannot be parsed or `path` is not a source tooprolix reads.
    ///
    /// # Provisional
    ///
    /// This is the wheel's only export, and it exists so that the pyo3 boundary is covered by
    /// tests rather than by hope: a wheel that exports nothing used to pass every gate.
    /// `build-cli-with-exit-contract-and-rule-codes` and
    /// `package-python-distribution-and-publish-0-1-0` own the real Python API
    /// (`tooprolix.check(path) -> list[Finding]`) and may replace this wholesale. Nothing should
    /// depend on the tuple shape — but whatever replaces it must keep the boundary tests below
    /// alive, which is what the probe it grew out of failed to say.
    // `pub(crate)` only so `mod tests` can call the Python-facing wrapper itself. Nothing outside
    // this crate can reach it; the wheel exposes it through `#[pymodule]`, not through this path.
    #[pyfunction]
    pub(crate) fn prose_blocks(
        path: &str,
        source: &str,
    ) -> PyResult<Vec<(&'static str, usize, usize, String)>> {
        Ok(crate::extract::extract(Path::new(path), source)?
            .into_iter()
            .map(|block| {
                (
                    block.kind.as_str(),
                    block.line_start,
                    block.line_end,
                    block.normalized,
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use pyo3::Python;
    use pyo3::exceptions::PyValueError;
    use pyo3::types::PyAnyMethods;

    /// The Python boundary must raise `ValueError` specifically, and the message must keep the
    /// parser's detail. Asserting "some exception was raised" would not be enough: swapping the
    /// exception type in `From<Error> for PyErr` breaks `except ValueError` in every caller while
    /// leaving fmt, clippy, `cargo test` and all three Python gates green.
    #[test]
    fn a_parse_failure_reaches_python_as_a_value_error() {
        Python::initialize();
        Python::attach(|py| {
            let error = crate::tooprolix::prose_blocks("a.py", "def broken(:")
                .expect_err("the fixture is not valid Python");

            assert!(
                error.is_instance_of::<PyValueError>(py),
                "expected ValueError, got {error:?}"
            );
            assert!(
                error
                    .value(py)
                    .to_string()
                    .contains("could not parse Python source"),
                "the parser detail was dropped on the way to Python: {error}"
            );
        });
    }

    /// Every [`crate::Error`] variant is named here, and each one's mapping is pinned separately.
    ///
    /// The test above covers `Parse` through the module wrapper only; this one covers the
    /// *conversion* itself, variant by variant, so that an arm which quietly starts sending one
    /// variant to a different exception type is a red test rather than a Python-side surprise.
    /// `Parse` and `UnsupportedSource` map to `ValueError`, and that is a decision recorded here,
    /// not a coincidence of a catch-all: both are "you gave me something I cannot read prose from".
    /// `Io` deliberately does **not** — a missing file arriving as `ValueError` would make
    /// `except FileNotFoundError` useless and is exactly the mistyping this test exists to pin.
    ///
    /// The other half of the guard is not a test and cannot be: a *new* variant is caught by the
    /// exhaustive `match` in `From<Error> for PyErr` as a build error. Verified empirically —
    /// adding an `Io` variant to [`Error`] before that match existed left all 73 tests green while
    /// mapping `Io` to `ValueError`; with the match it fails to compile at `src/lib.rs`, which is
    /// precisely how the arm below came to be written.
    #[test]
    fn every_error_variant_maps_to_its_own_python_exception() {
        Python::initialize();
        Python::attach(|py| {
            let parse = crate::extract::extract(std::path::Path::new("a.py"), "def broken(:")
                .expect_err("the fixture is not valid Python");
            let unsupported =
                crate::Error::UnsupportedSource(std::path::PathBuf::from("notes.txt"));

            for (detail, error) in [
                ("could not parse Python source", parse),
                ("is not a source tooprolix extracts prose from", unsupported),
            ] {
                let raised = pyo3::PyErr::from(error);

                assert!(
                    raised.is_instance_of::<PyValueError>(py),
                    "expected ValueError, got {raised:?}"
                );
                assert!(
                    raised.value(py).to_string().contains(detail),
                    "the variant's own message was lost on the way to Python: {raised}"
                );
            }
        });
    }

    /// A filesystem failure keeps its `io::ErrorKind` all the way into the Python exception type.
    ///
    /// Three kinds, three exceptions, and the path in every message. Asserting only "some
    /// `OSError`" would pass on a single arm that collapsed all three, which is the mapping this
    /// whole `match` exists to prevent — a caller writing `except FileNotFoundError` around
    /// `tooprolix.check()` would then never catch anything.
    #[test]
    fn a_filesystem_failure_keeps_its_kind_on_the_way_to_python() {
        use pyo3::exceptions::{PyFileNotFoundError, PyOSError, PyPermissionError};
        use std::io::ErrorKind;

        /// The exception raised for `kind`, with the path always at `missing/api.py`.
        fn raise(kind: ErrorKind) -> pyo3::PyErr {
            pyo3::PyErr::from(crate::Error::Io {
                path: std::path::PathBuf::from("missing/api.py"),
                source: std::io::Error::new(kind, "measured"),
            })
        }

        Python::initialize();
        Python::attach(|py| {
            let missing = raise(ErrorKind::NotFound);
            let forbidden = raise(ErrorKind::PermissionDenied);
            let other = raise(ErrorKind::InvalidData);

            assert!(
                missing.is_instance_of::<PyFileNotFoundError>(py),
                "expected FileNotFoundError, got {missing:?}"
            );
            assert!(
                forbidden.is_instance_of::<PyPermissionError>(py),
                "expected PermissionError, got {forbidden:?}"
            );
            assert!(
                other.is_instance_of::<PyOSError>(py)
                    && !other.is_instance_of::<PyFileNotFoundError>(py)
                    && !other.is_instance_of::<PyPermissionError>(py),
                "expected a plain OSError, got {other:?}"
            );

            for raised in [&missing, &forbidden, &other] {
                assert!(
                    raised.value(py).to_string().contains("missing/api.py"),
                    "the path was lost on the way to Python: {raised}"
                );
                // ... and none of the three is the `ValueError` the parser failures use, which is
                // the exact confusion the exhaustive match was built to make impossible.
                assert!(!raised.is_instance_of::<PyValueError>(py), "{raised:?}");
            }
        });
    }

    /// The wheel must actually EXPORT the function, not merely contain it. Removing the
    /// `#[pyfunction]` registration leaves the Rust call sites — and therefore every other test
    /// here — perfectly green while `import tooprolix` succeeds on a module that exports nothing,
    /// which is exactly what an `import tooprolix` smoke test cannot distinguish. This builds the
    /// real module object and asks Python what is on it.
    #[test]
    fn the_extension_module_exports_prose_blocks() {
        Python::initialize();
        Python::attach(|py| {
            let module = pyo3::wrap_pymodule!(crate::tooprolix)(py);

            assert!(
                module
                    .bind(py)
                    .hasattr("prose_blocks")
                    .expect("hasattr on a freshly built module cannot fail"),
                "the extension module exports nothing named prose_blocks"
            );
        });
    }

    /// ... and real prose must survive the round trip through that same Python-facing wrapper, so
    /// a mutation that swallows the error cannot pass by returning an empty list for everything.
    #[test]
    fn prose_round_trips_through_the_python_wrapper() {
        Python::initialize();
        Python::attach(|_py| {
            let source = "# Retries are capped at three attempts here,\n\
                          # because the upstream service rate-limits us.\n";

            let blocks = crate::tooprolix::prose_blocks("client.py", source)
                .expect("the fixture is valid Python");

            assert_eq!(blocks, vec![(
                "comment",
                1,
                2,
                "retries are capped at three attempts here because the upstream service rate limits us"
                    .to_owned(),
            )]);
        });
    }
}
