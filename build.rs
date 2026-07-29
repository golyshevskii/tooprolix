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
//!
//! Since `ship-v0-1-0-delivery-and-release` the whole pyo3 boundary is behind the `python` feature,
//! so there is nothing to point an rpath at when that feature is off — and emitting one anyway
//! would make `pyo3-build-config` go looking for an interpreter during a build that needs none.
//! The branch is on the `CARGO_FEATURE_PYTHON` **environment variable**, which cargo sets for build
//! scripts exactly when the feature is on, and NOT on `#[cfg(feature = "python")]`: gating the
//! `use`/call that way gives E0433, because a build script is compiled before the feature
//! resolution that would activate an optional build-dependency. So `pyo3-build-config` stays
//! non-optional in Cargo.toml. Measured in epic 1; do not "tidy" this into a `cfg`.
//!
//! # The second job: the date `--version` prints
//!
//! `TOOPROLIX_COMMIT_DATE` is the other half of `tooprolix --version`, and it is the **commit**
//! date rather than the wall clock (epic 2, Decisions #14): a binary whose `--version` changes
//! because an hour passed is not reproducible, and two builds of one commit would disagree about
//! what they are. There are three sources, in this order, and the order is the whole design:
//!
//! 1. **`SOURCE_DATE_EPOCH`** — the cross-ecosystem convention for "pretend the build happened
//!    then". It wins over git because it is the only thing a *release* build can set: the wheel is
//!    built from an sdist, which carries no `.git` at all.
//! 2. **`git log -1 --format=%cs`** — the committer date of `HEAD`, already `YYYY-MM-DD` (`%cs` is
//!    the short committer date; no `--date=` needed and no formatting on our side).
//! 3. **`unknown`** — a tree with no git and no `SOURCE_DATE_EPOCH`. Deliberately not today's date:
//!    substituting the wall clock is exactly the thing being avoided, and a build that cannot know
//!    its provenance should say so rather than invent one.
//!
//! ⚠️ **Emitting the `rerun-if` lines is what makes the answer true rather than merely printed.**
//! Cargo's default is to re-run a build script whenever any file in the package changes; the moment
//! this file emits one `rerun-if-changed`, that default is **replaced** by exactly what is listed.
//! So all four lines below are load-bearing: `build.rs` itself (or editing this file would not
//! re-run it), `.git/HEAD` (moving between branches), the file `HEAD` points at (committing on the
//! current branch), and `SOURCE_DATE_EPOCH`. Miss the ref file and `--version` reports the date of
//! whatever commit was checked out the last time the script happened to run.

use std::process::Command;

fn main() {
    if std::env::var_os("CARGO_FEATURE_PYTHON").is_some() {
        pyo3_build_config::add_libpython_rpath_link_args();
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    for path in git_inputs() {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rustc-env=TOOPROLIX_COMMIT_DATE={}", commit_date());
}

/// The date to stamp into the binary: `SOURCE_DATE_EPOCH`, then git, then `unknown`.
fn commit_date() -> String {
    if let Some(epoch) = source_date_epoch() {
        return epoch;
    }
    git(&["log", "-1", "--format=%cs"]).unwrap_or_else(|| "unknown".to_owned())
}

/// `SOURCE_DATE_EPOCH` as `YYYY-MM-DD` UTC, or `None` if it is unset or not a number.
///
/// The civil-date arithmetic is spelled out rather than pulled from `chrono` or `time`: this is a
/// build script, a build dependency is paid for by every consumer on every build, and the whole
/// computation is Howard Hinnant's `civil_from_days` — proleptic Gregorian, no leap seconds, which
/// is exactly what `SOURCE_DATE_EPOCH` is defined to be. An unparsable value falls through to git
/// rather than failing the build: it is somebody else's environment variable.
fn source_date_epoch() -> Option<String> {
    let seconds: i64 = std::env::var("SOURCE_DATE_EPOCH")
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let days = seconds.div_euclid(86_400);

    // Shift the epoch to 0000-03-01 so that a leap day lands at the end of the 400-year era.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;

    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// The files whose contents decide what `git log -1` answers, so cargo re-runs when they move.
///
/// `HEAD` covers changing branch or detaching; the ref `HEAD` names covers committing on the branch
/// you are already on. Both halves are needed and neither substitutes for the other — on an
/// attached branch `HEAD` is the constant text `ref: refs/heads/<branch>` and does **not** change
/// when you commit.
///
/// ⚠️ **Every path here comes from `git rev-parse --git-path`, and joining onto a git directory by
/// hand is the bug this replaced.** In a linked worktree the two halves live in *different*
/// directories: `HEAD` is per-worktree (`<main>/.git/worktrees/<name>/HEAD`) while
/// `refs/heads/<branch>` is in the **common** directory (`<main>/.git/refs/heads/<branch>`).
/// Building `<absolute-git-dir>/<ref>` therefore produced a path that does not exist, the ref was
/// silently dropped, only the never-changing `HEAD` was watched, and the date went stale and stayed
/// stale — measured in a real linked worktree on a real attached branch. `--git-path` is the
/// command that knows that mapping; it is right in a plain checkout, a worktree and a bare repo.
///
/// **The ref path is emitted even when no file is there yet, and that is deliberate.** A packed ref
/// (`git gc`, a fresh clone) has no loose file until the next commit writes one, and `packed-refs`
/// is *not* rewritten by that commit — so filtering on existence would drop the one path that is
/// about to appear. Measured: cargo treats a watched path that is missing as permanently dirty
/// ("the file `…` is missing"), which re-runs this script and recompiles the crate on every build.
/// That is a real cost, it is bounded — it self-heals the moment the ref is unpacked — and it is
/// the safe side of the trade: a spurious rebuild is noise, a stale `--version` is a lie. It also
/// makes watching `packed-refs` redundant rather than merely unnecessary, which is why it is not
/// here: while the ref is packed the missing path already forces a fresh answer every time.
///
/// An empty answer means there is no git here at all, so the date is `unknown`. Nothing is watched
/// then, on purpose: the only path available would be a guess like `.git/HEAD`, which is missing,
/// which by the measurement above would recompile the crate on **every** build of the published
/// sdist forever. A checkout that gains a `.git` after being built needs one `touch build.rs`; the
/// release path does not go through git at all and is covered by `rerun-if-env-changed`.
fn git_inputs() -> Vec<String> {
    let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) else {
        return Vec::new();
    };

    let mut inputs = vec![head];
    // Empty on a detached HEAD — where the SHA is in `HEAD` itself, so there is no ref to watch.
    if let Some(reference) = git(&["symbolic-ref", "--quiet", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        inputs.push(path);
    }
    inputs
}

/// One git invocation, trimmed, or `None` when git is missing, fails, or answers nothing.
///
/// Every failure mode collapses to `None` on purpose: a build outside a repository, a machine with
/// no git, and a repository with no commits are all "the date is not knowable here", and the caller
/// has exactly one thing to do about all three.
fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}
