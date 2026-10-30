//! The never-commit set and the pure function that decides what `c3 finish` unstages.
//!
//! `c3 finish` runs `git add -A` (which already honours `.gitignore`), then unstages the
//! machine-local run artifacts that must never land in a commit: consult/work event
//! streams, lock files and a work run's `.log`/`.stderr`. `Cargo.lock` and every other
//! lock file outside `.collab` are NOT in the set, and `.consult.detached-*.log` stays
//! committed (the repository's convention). The set is matched against the file's
//! repo-relative POSIX path.

use globset::{Glob, GlobSet, GlobSetBuilder};
use std::sync::OnceLock;

/// The never-commit globs (repo-relative POSIX paths), as the port spec lists them.
pub const NEVER_COMMIT: &[&str] = &[
    ".collab/**/events.jsonl",
    ".collab/**/*.events.jsonl",
    ".collab/**/.admission.lock",
    ".collab/**/.consult.lock",
    ".collab/**/.consult.write.lock",
    ".collab/**/.consult.pending*.json",
    ".collab/**/work/**/.log",
    ".collab/**/work/**/.stderr",
];

fn never_commit_set() -> &'static GlobSet {
    static SET: OnceLock<GlobSet> = OnceLock::new();
    SET.get_or_init(|| {
        let mut b = GlobSetBuilder::new();
        for p in NEVER_COMMIT {
            b.add(
                Glob::new(p).unwrap_or_else(|e| panic!("never-commit glob {p:?} is invalid: {e}")),
            );
        }
        b.build().expect("never-commit globset builds")
    })
}

/// Whether a repo-relative POSIX path is in the never-commit set.
pub fn is_never_commit(rel: &str) -> bool {
    never_commit_set().is_match(rel)
}

/// Given the raw bytes of `git status --porcelain=v1 -z`, return the repo-relative POSIX
/// paths that are in the never-commit set and must be unstaged, in input order.
///
/// Each `-z` record is `XY<space>PATH\0`; a rename/copy record carries the origin path as
/// the following NUL-terminated field, which this skips.
pub fn paths_to_unstage(porcelain_z: &[u8]) -> Vec<String> {
    let s = String::from_utf8_lossy(porcelain_z);
    let mut out = Vec::new();
    let mut it = s.split('\0').filter(|t| !t.is_empty());
    while let Some(tok) = it.next() {
        if tok.len() < 4 {
            // Too short to be "XY PATH"; ignore (also skips a stray origin field).
            continue;
        }
        let bytes = tok.as_bytes();
        let x = bytes[0] as char;
        let y = bytes[1] as char;
        let path = &tok[3..];
        // A rename/copy in either index or worktree has the origin as the next field.
        if x == 'R' || x == 'C' || y == 'R' || y == 'C' {
            let _ = it.next();
        }
        let norm = path.replace('\\', "/");
        if is_never_commit(&norm) {
            out.push(norm);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `--porcelain=v1 -z` buffer from `(xy, path)` records.
    fn z(records: &[(&str, &str)]) -> Vec<u8> {
        let mut v = Vec::new();
        for (xy, p) in records {
            v.extend_from_slice(xy.as_bytes());
            v.push(b' ');
            v.extend_from_slice(p.as_bytes());
            v.push(0);
        }
        v
    }

    #[test]
    fn unstages_events_locks_and_work_logs_keeps_the_rest() {
        let buf = z(&[
            ("M ", "Cargo.lock"),
            ("A ", ".collab/wave1/work/01/events.jsonl"),
            ("A ", ".collab/wave1/handoffs/01-x.events.jsonl"),
            ("A ", ".collab/wave1/work/01/.log"),
            ("A ", ".collab/wave1/work/01/.stderr"),
            ("A ", ".collab/wave1/.admission.lock"),
            ("A ", ".collab/wave1/.consult.lock"),
            ("A ", ".collab/wave1/.consult.write.lock"),
            ("A ", ".collab/wave1/.consult.pending-03.json"),
            ("A ", ".collab/wave1/.consult.detached-abcd1234.log"),
            ("M ", "src/main.rs"),
        ]);
        let got = paths_to_unstage(&buf);
        assert_eq!(
            got,
            vec![
                ".collab/wave1/work/01/events.jsonl",
                ".collab/wave1/handoffs/01-x.events.jsonl",
                ".collab/wave1/work/01/.log",
                ".collab/wave1/work/01/.stderr",
                ".collab/wave1/.admission.lock",
                ".collab/wave1/.consult.lock",
                ".collab/wave1/.consult.write.lock",
                ".collab/wave1/.consult.pending-03.json",
            ]
        );
    }

    #[test]
    fn cargo_lock_and_detached_log_are_never_unstaged() {
        assert!(!is_never_commit("Cargo.lock"));
        assert!(!is_never_commit(
            ".collab/wave1/.consult.detached-abcd1234.log"
        ));
        // A plain lock outside .collab stays.
        assert!(!is_never_commit("some/dir/.admission.lock"));
        // A non-work .log under .collab stays (only work/**/.log is in the set).
        assert!(!is_never_commit(".collab/wave1/run.log"));
    }

    #[test]
    fn rename_record_origin_field_is_skipped() {
        // "R  new\0old" — the origin must not be read as its own record.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"R  src/new.rs");
        buf.push(0);
        buf.extend_from_slice(b"src/old.rs");
        buf.push(0);
        buf.extend_from_slice(b"A  .collab/w/work/01/events.jsonl");
        buf.push(0);
        let got = paths_to_unstage(&buf);
        assert_eq!(got, vec![".collab/w/work/01/events.jsonl"]);
    }
}
