//! End-to-end tests for `c3 snapshot` Part B: `--base` (hash and snapshot file), `.c3ignore` /
//! `.eckignore`, `--profile`, the hand-off hidden-path policy (via `c3 fail --snapshot`), and the
//! manifest digest's fallback to a legacy `.eck/`.
//!
//! Each test builds a throwaway repository in a temp dir and sets its identity LOCALLY
//! (`git -C <repo> config ...`) — never `git config --global`. The `c3` binary cargo built
//! for this test is used (`CARGO_BIN_EXE_c3`). Run single-threaded (`--test-threads=1`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn git(repo: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(ok, "git {args:?} failed in {}", repo.display());
}

fn head(repo: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn scratch_repo(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "c3-snapb-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@example.com"]);
    git(&d, &["config", "user.name", "Test"]);
    git(&d, &["config", "commit.gpgsign", "false"]);
    d
}

fn run_c3(repo: &Path, args: &[&str]) -> (i32, String, String) {
    // a test run inside a C3 child (a worker running the suite) must not inherit the mark
    let out = Command::new(c3_bin())
        .current_dir(repo)
        .args(args)
        .env_remove("C3_CHILD")
        .env_remove("C3_STATE_MIGRATE")
        .env("C3_CONFIG", "none")
        .output()
        .expect("spawn c3");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// An out-of-repo path for snapshot files so they never count as dirty changes in a delta.
fn out_path(repo: &Path, name: &str) -> PathBuf {
    repo.parent().unwrap().join(format!(
        "{}-{name}",
        repo.file_name().unwrap().to_string_lossy()
    ))
}

#[test]
fn base_from_hash_and_from_snapshot_file() {
    let repo = scratch_repo("base");
    std::fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "c0"]);
    let c0 = head(&repo);

    // A full snapshot at c0, written outside the repo; its header records `- Anchor: <c0>`.
    let full = out_path(&repo, "full.md");
    let (code, _o, e) = run_c3(
        &repo,
        &["snapshot", "--no-tree", "--out", full.to_str().unwrap()],
    );
    assert_eq!(code, 0, "full snapshot failed: {e}");
    let full_text = std::fs::read_to_string(&full).unwrap();
    assert!(
        full_text.contains(&format!("- Anchor: {c0}")),
        "header should record the anchor: {full_text}"
    );

    // Advance one commit.
    std::fs::write(repo.join("b.rs"), "fn b() {}\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "c1"]);

    // `--base <hash>`: delta c0..HEAD → b.rs only.
    let by_hash = out_path(&repo, "by_hash.md");
    let (code, _o, e) = run_c3(
        &repo,
        &[
            "snapshot",
            "--no-tree",
            "--base",
            &c0,
            "--out",
            by_hash.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "base-hash snapshot failed: {e}");
    let t = std::fs::read_to_string(&by_hash).unwrap();
    assert!(t.contains("--- File: b.rs ---"), "{t}");
    assert!(!t.contains("--- File: a.rs ---"), "{t}");

    // `--base <snapshot file>`: the file's recorded anchor is c0 → the same delta.
    let by_file = out_path(&repo, "by_file.md");
    let (code, _o, e) = run_c3(
        &repo,
        &[
            "snapshot",
            "--no-tree",
            "--base",
            full.to_str().unwrap(),
            "--out",
            by_file.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "base-file snapshot failed: {e}");
    let t = std::fs::read_to_string(&by_file).unwrap();
    assert!(t.contains("--- File: b.rs ---"), "{t}");
    assert!(!t.contains("--- File: a.rs ---"), "{t}");

    // An unresolvable ref refuses.
    let (code, _o, err) = run_c3(
        &repo,
        &[
            "snapshot",
            "--base",
            "not-a-real-ref",
            "--out",
            out_path(&repo, "bad.md").to_str().unwrap(),
        ],
    );
    assert_eq!(code, 1, "expected refusal");
    assert!(err.contains("does not resolve"), "err: {err}");

    let _ = std::fs::remove_dir_all(&repo);
    for n in ["full.md", "by_hash.md", "by_file.md"] {
        let _ = std::fs::remove_file(out_path(&repo, n));
    }
}

#[test]
fn eckignore_excludes_listed_paths() {
    let repo = scratch_repo("eckignore");
    std::fs::write(repo.join("keep.rs"), "fn a() {}\n").unwrap();
    std::fs::write(repo.join("SECRETNOTES.md"), "NEVERSHARE marker\n").unwrap();
    std::fs::write(repo.join(".eckignore"), "SECRETNOTES.md\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "init"]);

    let out = out_path(&repo, "snap.md");
    let (code, _o, e) = run_c3(
        &repo,
        &["snapshot", "--no-tree", "--out", out.to_str().unwrap()],
    );
    assert_eq!(code, 0, "snapshot failed: {e}");
    let t = std::fs::read_to_string(&out).unwrap();
    assert!(t.contains("--- File: keep.rs ---"), "{t}");
    assert!(!t.contains("NEVERSHARE"), ".eckignore path leaked: {t}");
    assert!(!t.contains("--- File: SECRETNOTES.md ---"), "{t}");

    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_file(&out);
}

// (0.2.5, S1) `.c3ignore` is read; next to it `.eckignore` is not (in a directory without one it
// still is - the test above).
#[test]
fn c3ignore_wins_over_eckignore() {
    let repo = scratch_repo("c3ignore");
    std::fs::write(repo.join("keep.rs"), "fn a() {}\n").unwrap();
    std::fs::write(repo.join("PRIVATE.md"), "C3IGNORED marker\n").unwrap();
    std::fs::write(repo.join("LEGACY.md"), "ONLYECKIGNORED marker\n").unwrap();
    std::fs::write(repo.join(".c3ignore"), "PRIVATE.md\n").unwrap();
    std::fs::write(repo.join(".eckignore"), "LEGACY.md\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "init"]);

    let out = out_path(&repo, "snap.md");
    let (code, _o, e) = run_c3(
        &repo,
        &["snapshot", "--no-tree", "--out", out.to_str().unwrap()],
    );
    assert_eq!(code, 0, "snapshot failed: {e}");
    let t = std::fs::read_to_string(&out).unwrap();
    assert!(t.contains("--- File: keep.rs ---"), "{t}");
    assert!(!t.contains("C3IGNORED"), ".c3ignore path leaked: {t}");
    assert!(
        t.contains("ONLYECKIGNORED"),
        ".eckignore is not read next to a .c3ignore: {t}"
    );

    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_file(&out);
}

// (0.2.5, S1) With only `.eck/` present and no writing command run, the manifest digest (the
// explainer pack's; `c3 pack` has none) still reads it - and nothing moves the folder.
#[test]
fn the_manifest_digest_reads_a_legacy_folder_before_any_writing_command() {
    let repo = scratch_repo("digest-fallback");
    std::fs::write(repo.join("lib.rs"), "pub fn focus_me() {}\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "init"]);
    std::fs::create_dir_all(repo.join(".eck")).unwrap();
    std::fs::write(
        repo.join(".eck/CONTEXT.md"),
        "# CONTEXT\n\nLEGACYDIGESTMARKER\n",
    )
    .unwrap();
    let out = out_path(&repo, "explain.md");
    let (code, _o, e) = run_c3(
        &repo,
        &[
            "explain",
            "--claim",
            "x",
            "--focus",
            "lib.rs",
            "--yes",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "explain failed: {e}");
    let t = std::fs::read_to_string(&out).unwrap();
    assert!(t.contains("### .eck/CONTEXT.md"), "{t}");
    assert!(t.contains("LEGACYDIGESTMARKER"), "{t}");
    assert!(repo.join(".eck/CONTEXT.md").is_file(), "not moved");
    assert!(!repo.join(".c3").exists(), "no folder created");

    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn profile_scopes_discovery_and_refuses_unknown() {
    let repo = scratch_repo("profile");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join("vendor")).unwrap();
    std::fs::create_dir_all(repo.join(".c3")).unwrap();
    std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(repo.join("vendor/lib.rs"), "fn v() {}\n").unwrap();
    std::fs::write(
        repo.join(".c3/profiles.json"),
        r#"{"core": {"include": ["src/**"], "exclude": []}}"#,
    )
    .unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "init"]);

    // `--profile core` keeps only src/**.
    let out = out_path(&repo, "snap.md");
    let (code, _o, e) = run_c3(
        &repo,
        &[
            "snapshot",
            "--no-tree",
            "--profile",
            "core",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "profile snapshot failed: {e}");
    let t = std::fs::read_to_string(&out).unwrap();
    assert!(t.contains("--- File: src/main.rs ---"), "{t}");
    assert!(!t.contains("--- File: vendor/lib.rs ---"), "{t}");

    // An unknown profile is refused with the known names.
    let (code, _o, err) = run_c3(
        &repo,
        &[
            "snapshot",
            "--profile",
            "bogus",
            "--out",
            out_path(&repo, "x.md").to_str().unwrap(),
        ],
    );
    assert_eq!(code, 1, "expected refusal");
    assert!(err.contains("bogus"), "err: {err}");
    assert!(err.contains("core"), "err should list known names: {err}");

    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn handoff_drops_hidden_paths_but_keeps_c3_manifests() {
    let repo = scratch_repo("handoff");
    // `.c3/` is gitignored (as in a real C3 workspace); `.collab/` is NOT.
    std::fs::write(repo.join(".gitignore"), ".c3/\n").unwrap();
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    std::fs::create_dir_all(repo.join(".collab/wave")).unwrap();
    std::fs::write(repo.join(".collab/wave/x.md"), "COLLABMARKER\n").unwrap();
    std::fs::create_dir_all(repo.join(".c3/snapshots")).unwrap();
    std::fs::write(repo.join(".c3/ROADMAP.md"), "# ROADMAP\n\nROADMARKER\n").unwrap();
    std::fs::write(repo.join(".c3/snapshots/old.md"), "OLDSNAPSHOTMARKER\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "init"]);

    // `c3 fail --snapshot` produces a full emergency hand-off snapshot with no anchor.
    let (code, out, err) = run_c3(&repo, &["fail", "--status", "blocked", "--snapshot"]);
    assert_eq!(code, 0, "fail failed: {out}\n{err}");

    // Read the generated snapshot (the one that is not the pre-seeded old.md).
    let snap = std::fs::read_dir(repo.join(".c3/snapshots"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension().map(|x| x == "md").unwrap_or(false)
                && p.file_name().map(|n| n != "old.md").unwrap_or(false)
        })
        .expect("a generated snapshot");
    let t = std::fs::read_to_string(&snap).unwrap();

    // Ordinary source present.
    assert!(t.contains("--- File: src.rs ---"), "{t}");
    // Hidden `.collab/` path dropped (body and path marker).
    assert!(!t.contains("COLLABMARKER"), ".collab body leaked: {t}");
    assert!(!t.contains("--- File: .collab/wave/x.md ---"), "{t}");
    // The `.c3/*.md` manifest is surfaced even though `.c3` is gitignored.
    assert!(t.contains("--- File: .c3/ROADMAP.md ---"), "{t}");
    assert!(t.contains("ROADMARKER"), "{t}");
    // A file under `.c3/snapshots/` is never included.
    assert!(
        !t.contains("OLDSNAPSHOTMARKER"),
        ".c3/snapshots leaked: {t}"
    );

    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) `c3 snapshot --state` (old name `--eck`, a hidden alias) writes the hand-off snapshot
// into `.c3/snapshots/` and `.c3/lastsnapshot/`, whatever `--out` says.
#[test]
fn snapshot_state_writes_the_handoff_into_the_c3_folder() {
    let repo = scratch_repo("state-flag");
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "init"]);
    for flag in ["--state", "--eck"] {
        let (code, out, err) = run_c3(&repo, &["snapshot", flag, "--no-tree"]);
        assert_eq!(code, 0, "{flag}: {out}\n{err}");
    }
    let snaps: Vec<_> = std::fs::read_dir(repo.join(".c3/snapshots"))
        .unwrap()
        .flatten()
        .collect();
    // (two runs in the same second may share a name)
    assert!(!snaps.is_empty(), "{snaps:?}");
    let last: Vec<_> = std::fs::read_dir(repo.join(".c3/lastsnapshot"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(last.len(), 1, "only the newest is in lastsnapshot");
    let (_c, help, _e) = run_c3(&repo, &["snapshot", "--help"]);
    assert!(help.contains("--state"), "{help}");
    assert!(!help.contains("--eck"), "the old name stays hidden: {help}");
    let _ = std::fs::remove_dir_all(&repo);
}
