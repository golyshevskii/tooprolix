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
//!    the short committer date; no `--date=` needed and no formatting on our side) — but **only
//!    when the repository git finds is this package's own**. See [`git_is_this_package`].
//! 3. **`unknown`** — anything else. Deliberately not today's date and deliberately not somebody
//!    else's commit date: substituting the wall clock is the thing being avoided, and a build that
//!    cannot know its provenance should say so rather than borrow one.
//!
//! ⚠️ **Emitting the `rerun-if` lines is what makes the answer true rather than merely printed.**
//! Cargo's default is to re-run a build script whenever any file in the package changes; the moment
//! this file emits one `rerun-if-changed`, that default is **replaced** by exactly what is listed.
//! So every line below is load-bearing: `build.rs` itself (or editing this file would not re-run
//! it), `HEAD` (moving between branches), the file `HEAD` points at (committing on the current
//! branch), and the three environment variables that can change the answer with no file to watch —
//! `SOURCE_DATE_EPOCH`, and `GIT_DIR`/`GIT_WORK_TREE`, which redirect git's discovery outright.
//! Miss the ref file and `--version` reports the date of whatever commit was checked out the last
//! time the script happened to run. The git paths are emitted **only** when git is the source: when
//! `SOURCE_DATE_EPOCH` decides the answer, or when the repository is not ours, no commit in it can
//! change what this binary prints, so watching it would only cause spurious rebuilds.

use std::process::Command;

fn main() {
    if std::env::var_os("CARGO_FEATURE_PYTHON").is_some() {
        pyo3_build_config::add_libpython_rpath_link_args();
    }

    println!("cargo:rerun-if-changed=build.rs");
    for variable in ["SOURCE_DATE_EPOCH", "GIT_DIR", "GIT_WORK_TREE"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }

    let date = match source_date_epoch() {
        Some(epoch) => epoch,
        None if git_is_this_package() => {
            for path in git_inputs() {
                println!("cargo:rerun-if-changed={path}");
            }
            git(&["log", "-1", "--format=%cs"]).unwrap_or_else(|| "unknown".to_owned())
        }
        None => "unknown".to_owned(),
    };
    println!("cargo:rustc-env=TOOPROLIX_COMMIT_DATE={date}");
}

/// Whether the repository git discovers is **this package's own**, rather than one enclosing it.
///
/// 🔴 **Git's discovery walks upward, and without this the date is silently borrowed.** A tree with
/// no `.git` of its own — an unpacked sdist, an extracted `cargo package` archive, a `cargo vendor`
/// directory — inherits whatever repository happens to sit above it. Reproduced: a copy of this
/// package with no git history at all, placed inside an unrelated checkout, stamped that host's
/// `2020-01-01` into `--version` and called it this package's commit date. Task 13 builds the wheel
/// from an sdist, so this is on the publication path rather than in a corner.
///
/// ⚠️ **Both paths are canonicalised before they are compared, and that is not tidying.** On macOS
/// `/tmp` is a symlink to `/private/tmp`, so `--show-toplevel` answers `/private/tmp/…` for a tree
/// reached as `/tmp/…` — the same directory, two strings. Comparing them raw would refuse a
/// perfectly good repository and print `unknown` for every build under such a path, which is a
/// worse failure than the one this closes: a wrong date is at least visibly wrong, whereas
/// `unknown` everywhere looks like the feature working as designed.
///
/// Equality with the manifest directory, not "is the manifest inside the work tree" — the vendored
/// tree *is* inside the host's work tree, which is the whole problem. The cost is that this package
/// would report `unknown` if it ever became a member of a workspace whose repository root sits
/// above it; that is not the layout today, and `unknown` is the safe direction to be wrong in.
fn git_is_this_package() -> bool {
    let canonical = |path: &str| std::fs::canonicalize(path).ok();
    let Some(toplevel) = git(&["rev-parse", "--show-toplevel"]) else {
        return false;
    };
    let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") else {
        return false;
    };
    match (canonical(&toplevel), canonical(&manifest)) {
        (Some(top), Some(here)) => top == here,
        // A path that will not canonicalise is a question we cannot answer, and the answer to an
        // unanswerable provenance question is `unknown`, never "probably ours".
        _ => false,
    }
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
