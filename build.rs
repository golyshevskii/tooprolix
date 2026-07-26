//! Puts an rpath to libpython on the binaries *this* crate builds.
//!
//! Without this, `cargo test` on Linux compiles and links cleanly and then dies at **load** time:
//!
//! ```text
//! target/debug/deps/tooprolix-…: error while loading shared libraries:
//!     libpython3.12.so.1.0: cannot open shared object file: No such file or directory
//! ```
//!
//! pyo3-ffi's own build script does emit `cargo:rustc-link-arg=-Wl,-rpath,<libdir>`
//! (`pyo3-build-config-0.29.0/src/lib.rs:332`), but `rustc-link-arg` applies only to the *emitting
//! package's* targets. This crate had no build script, so nothing ever put an rpath on our test
//! binary. macOS hid the problem completely: Homebrew's `libpython3.….dylib` carries an absolute
//! `install_name`, so the loader finds it with no rpath at all and every local run passed.
//!
//! Measured in `rust:1.97` with **no system libpython** (`ldconfig -p | grep -c
//! libpython3.12.so.1.0` → `0`) and only a uv-managed interpreter: without this file
//! `make rust.test` exits 127, with it exit 0.
//!
//! This is a no-op for the wheel: `add_libpython_rpath_link_args` emits nothing when libpython is not
//! being linked, which is the case under pyo3's `extension-module` feature that maturin turns on.
//! So the distributed artifact carries no rpath into a path that only exists on the build machine.

fn main() {
    pyo3_build_config::add_libpython_rpath_link_args();
}
