//! Build script for the `c3` binary: stamp the git commit that was built into the version string
//! clap prints for `c3 --version`, so a bare `c3 0.2.3` becomes `c3 0.2.3 (035d386)` (with
//! `-dirty` appended when tracked files differ from HEAD at build time; untracked files - this
//! repo always has untracked `.collab` files - do NOT count).
//!
//! The commit is exported as `C3_VERSION` through `cargo:rustc-env`, read by `env!` in `main.rs`.
//! When git or `.git` is absent the build degrades to the plain `CARGO_PKG_VERSION`. The version
//! number itself is never changed here.

use std::process::Command;

/// Run `git <args>` from the build script's directory (git walks up to the repo root) and return
/// its trimmed stdout on a clean exit, else `None` (no git binary, not a repo, a non-zero exit).
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn main() {
    let pkg = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut version = pkg.clone();

    if let Some(hash) = git(&["rev-parse", "--short=7", "HEAD"]) {
        // tracked changes only (untracked `.collab` files do not count as dirty).
        let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        version = format!("{pkg} ({hash}{})", if dirty { "-dirty" } else { "" });

        // Rebuild when HEAD moves, the current ref is updated, or the index changes (a staged or
        // reverted change flips `-dirty`). Resolve the real git dir so this works from a worktree
        // and from a package subdirectory.
        if let Some(gitdir) = git(&["rev-parse", "--absolute-git-dir"]) {
            println!("cargo:rerun-if-changed={gitdir}/HEAD");
            println!("cargo:rerun-if-changed={gitdir}/index");
            if let Some(refname) = git(&["symbolic-ref", "--quiet", "HEAD"]) {
                println!("cargo:rerun-if-changed={gitdir}/{refname}");
            }
        }
    }

    println!("cargo:rustc-env=C3_VERSION={version}");
}
