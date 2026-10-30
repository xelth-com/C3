//! End-to-end tests for `c3 finish` / `c3 fail` / `c3 manifest` in a scratch git repo.
//!
//! Each test builds a throwaway repository in a temp dir and sets its identity LOCALLY
//! (`git -C <repo> config ...`) — never `git config --global`. The `c3` binary cargo built
//! for this test is used (`CARGO_BIN_EXE_c3`), so a stale on-disk binary is never picked up.

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

/// A fresh git repo in a unique temp dir with a LOCAL identity and one initial commit.
fn scratch_repo(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "c3-finish-{name}-{}-{}",
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
    // `.eck/` is gitignored (as in a real C3 workspace), so the report and journal writes
    // never count as staged content. An initial commit so HEAD exists and `git reset` can
    // unstage.
    std::fs::write(d.join(".gitignore"), ".eck/\n").unwrap();
    std::fs::write(d.join("README.md"), "# scratch\n").unwrap();
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-qm", "init"]);
    d
}

/// Run the `c3` binary in `repo` with `args`; returns (exit code, stdout, stderr).
fn run_c3(repo: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(c3_bin())
        .current_dir(repo)
        .args(args)
        .output()
        .expect("spawn c3");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn head_subject(repo: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["log", "-1", "--pretty=%s"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn is_tracked(repo: &Path, rel: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "--error-unmatch", rel])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn finish_commits_leaves_never_commit_unstaged_and_writes_handoff() {
    let repo = scratch_repo("commit");

    // A normal source change (committed) plus never-commit machine-local files.
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    std::fs::create_dir_all(repo.join(".collab/wave1/work/01")).unwrap();
    std::fs::write(
        repo.join(".collab/wave1/work/01/events.jsonl"),
        "{\"e\":1}\n",
    )
    .unwrap();
    std::fs::write(repo.join(".collab/wave1/work/01/.log"), "log\n").unwrap();
    std::fs::write(repo.join(".collab/wave1/work/01/.stderr"), "err\n").unwrap();
    std::fs::write(
        repo.join(".collab/wave1/.consult.detached-abcd1234.log"),
        "kept\n",
    )
    .unwrap();

    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: add src\n\nbody line",
            "--status",
            "Implemented the thing.",
            "--keep",
            "10",
        ],
    );
    assert_eq!(code, 0, "finish failed: {out}\n{err}");

    // Committed, with the message verbatim (first line is the subject).
    assert_eq!(head_subject(&repo), "feat: add src");
    assert!(is_tracked(&repo, "src.rs"), "src.rs should be committed");

    // The never-commit files were not committed; the detached log WAS.
    assert!(!is_tracked(&repo, ".collab/wave1/work/01/events.jsonl"));
    assert!(!is_tracked(&repo, ".collab/wave1/work/01/.log"));
    assert!(!is_tracked(&repo, ".collab/wave1/work/01/.stderr"));
    assert!(is_tracked(
        &repo,
        ".collab/wave1/.consult.detached-abcd1234.log"
    ));

    // AnswerToSA.md and the journal.
    let answer = std::fs::read_to_string(repo.join(".eck/lastsnapshot/AnswerToSA.md")).unwrap();
    assert_eq!(answer, "# Agent Report\n\nImplemented the thing.\n");
    let journal = std::fs::read_to_string(repo.join(".eck/JOURNAL.md")).unwrap();
    assert!(journal.starts_with("# JOURNAL"));
    assert!(journal.contains("- feat: add src"), "journal: {journal}");
    assert!(journal.contains("Implemented the thing."));

    // A snapshot in both .eck/snapshots and .eck/lastsnapshot, with the report embedded.
    let snaps: Vec<_> = std::fs::read_dir(repo.join(".eck/snapshots"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".md"))
        .collect();
    assert_eq!(snaps.len(), 1, "expected one snapshot");
    let snap_text = std::fs::read_to_string(snaps[0].path()).unwrap();
    assert!(snap_text.contains("MESSAGE FROM THE EXECUTION AGENT"));
    assert!(snap_text.contains("Implemented the thing."));
    let last: Vec<_> = std::fs::read_dir(repo.join(".eck/lastsnapshot"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(last
        .iter()
        .any(|n| n.ends_with(".md") && n != "AnswerToSA.md"));
    assert!(last.iter().any(|n| n == "AnswerToSA.md"));

    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn finish_prunes_snapshots_beyond_keep() {
    let repo = scratch_repo("prune");
    // Pre-populate .eck/snapshots with old snapshots.
    std::fs::create_dir_all(repo.join(".eck/snapshots")).unwrap();
    for i in 0..5 {
        std::fs::write(
            repo.join(format!(".eck/snapshots/old_{i}.md")),
            format!("old {i}\n"),
        )
        .unwrap();
    }
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();

    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: prune",
            "--status",
            "done",
            "--keep",
            "2",
        ],
    );
    assert_eq!(code, 0, "finish failed: {out}\n{err}");

    let count = std::fs::read_dir(repo.join(".eck/snapshots"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".md"))
        .count();
    assert_eq!(count, 2, "prune should keep exactly --keep newest");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn finish_refuses_a_staged_secret_and_names_path_and_kind() {
    let repo = scratch_repo("secret");
    std::fs::write(
        repo.join("cfg.rs"),
        "let key = \"sk-abcdefghijklmnopqrstuvwxyz0123456789\";\n",
    )
    .unwrap();

    let (code, _out, err) = run_c3(
        &repo,
        &["finish", "--message", "feat: cfg", "--status", "done"],
    );
    assert_eq!(code, 1, "expected refusal");
    assert!(err.contains("cfg.rs"), "err should name the file: {err}");
    assert!(
        err.contains("openai-key"),
        "err should name the kind: {err}"
    );
    // The value never appears.
    assert!(
        !err.contains("sk-abcdefghij"),
        "err must not echo the value: {err}"
    );
    // Nothing was committed.
    assert_eq!(head_subject(&repo), "init");

    // --allow-secret lets the reviewed file through.
    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: cfg",
            "--status",
            "done",
            "--allow-secret",
            "cfg.rs",
        ],
    );
    assert_eq!(code, 0, "allow-secret finish failed: {out}\n{err}");
    assert_eq!(head_subject(&repo), "feat: cfg");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn finish_with_nothing_to_commit_refuses() {
    let repo = scratch_repo("empty");
    let (code, _out, err) = run_c3(
        &repo,
        &["finish", "--message", "feat: nothing", "--status", "done"],
    );
    assert_eq!(code, 1, "expected refusal");
    assert!(err.contains("nothing to commit"), "err: {err}");
    // Still produced the report (written before staging).
    assert!(repo.join(".eck/lastsnapshot/AnswerToSA.md").exists());
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn fail_writes_report_and_snapshot_without_committing() {
    let repo = scratch_repo("fail");
    // A full snapshot first establishes the anchor so a --fail delta has a baseline.
    let (code, _o, e) = run_c3(&repo, &["snapshot", "--no-tree"]);
    assert_eq!(code, 0, "snapshot failed: {e}");

    // An uncommitted change for the emergency delta to capture.
    std::fs::write(repo.join("wip.rs"), "fn wip() {}\n").unwrap();

    let before = head_subject(&repo);
    let (code, out, err) = run_c3(&repo, &["fail", "--status", "Stuck on the parser."]);
    assert_eq!(code, 0, "fail failed: {out}\n{err}");
    // No commit happened.
    assert_eq!(head_subject(&repo), before);

    let answer = std::fs::read_to_string(repo.join(".eck/lastsnapshot/AnswerToSA.md")).unwrap();
    assert_eq!(
        answer,
        "# Agent Report (BLOCKED/FAILED)\n\nStuck on the parser.\n"
    );
    let snaps = std::fs::read_dir(repo.join(".eck/snapshots"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".md"))
        .count();
    assert!(snaps >= 1, "fail should write an emergency snapshot");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn fail_without_anchor_writes_a_full_snapshot() {
    // No prior full snapshot → no anchor. `c3 fail` must fall back to a FULL emergency
    // snapshot rather than reporting "nothing changed".
    let repo = scratch_repo("fail-noanchor");
    std::fs::write(repo.join("wip.rs"), "fn wip() {}\n").unwrap();

    let (code, out, err) = run_c3(&repo, &["fail", "--status", "Blocked, no baseline."]);
    assert_eq!(code, 0, "fail failed: {out}\n{err}");
    assert!(
        out.contains("Snapshot:"),
        "expected a snapshot path, got: {out}"
    );
    let snaps: Vec<_> = std::fs::read_dir(repo.join(".eck/snapshots"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".md"))
        .collect();
    assert_eq!(
        snaps.len(),
        1,
        "a full emergency snapshot should be written"
    );
    // A full snapshot of the repo includes the committed README and the WIP file.
    let text = std::fs::read_to_string(snaps[0].path()).unwrap();
    assert!(text.contains("--- File: wip.rs ---"), "snapshot: {text}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn eck_snapshot_body_excludes_the_never_commit_set() {
    let repo = scratch_repo("eck-body");
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    std::fs::create_dir_all(repo.join(".collab/w/work/01")).unwrap();
    // A recognisable marker in a never-commit event stream.
    std::fs::write(
        repo.join(".collab/w/work/01/events.jsonl"),
        "{\"MARKER\":\"EVENTBODYUNIQUE\"}\n",
    )
    .unwrap();
    std::fs::write(repo.join(".collab/w/work/01/.stderr"), "STDERRUNIQUE\n").unwrap();

    let (code, out, err) = run_c3(
        &repo,
        &["finish", "--message", "feat: body", "--status", "done"],
    );
    assert_eq!(code, 0, "finish failed: {out}\n{err}");

    let snaps: Vec<_> = std::fs::read_dir(repo.join(".eck/snapshots"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".md"))
        .collect();
    assert_eq!(snaps.len(), 1);
    let text = std::fs::read_to_string(snaps[0].path()).unwrap();
    // Ordinary code is present; the never-commit artifacts are not.
    assert!(text.contains("--- File: src.rs ---"), "snapshot: {text}");
    assert!(!text.contains("EVENTBODYUNIQUE"), "events body leaked");
    assert!(!text.contains("STDERRUNIQUE"), "stderr body leaked");
    assert!(!text.contains("events.jsonl"), "events path marker leaked");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn manifest_actions_edit_eck_files() {
    let repo = scratch_repo("manifest");

    // append_to_section creates the file (# ROADMAP) and the section at the end.
    let (code, _o, e) = run_c3(
        &repo,
        &[
            "manifest",
            "--file",
            "ROADMAP.md",
            "--action",
            "append_to_section",
            "--section",
            "## Backlog",
            "--content",
            "- first item",
        ],
    );
    assert_eq!(code, 0, "append failed: {e}");
    let roadmap = std::fs::read_to_string(repo.join(".eck/ROADMAP.md")).unwrap();
    assert!(roadmap.starts_with("# ROADMAP"), "roadmap: {roadmap}");
    assert!(
        roadmap.contains("## Backlog\n- first item"),
        "roadmap: {roadmap}"
    );

    // A second append goes to the END of the same section.
    run_c3(
        &repo,
        &[
            "manifest",
            "--file",
            "ROADMAP.md",
            "--action",
            "append_to_section",
            "--section",
            "## Backlog",
            "--content",
            "- second item",
        ],
    );
    let roadmap = std::fs::read_to_string(repo.join(".eck/ROADMAP.md")).unwrap();
    let first = roadmap.find("- first item").unwrap();
    let second = roadmap.find("- second item").unwrap();
    assert!(
        first < second,
        "second item should come after first: {roadmap}"
    );

    // replace_text substitutes.
    run_c3(
        &repo,
        &[
            "manifest",
            "--file",
            "ROADMAP.md",
            "--action",
            "replace_text",
            "--target",
            "- first item",
            "--content",
            "- DONE item",
        ],
    );
    let roadmap = std::fs::read_to_string(repo.join(".eck/ROADMAP.md")).unwrap();
    assert!(roadmap.contains("- DONE item"));
    assert!(!roadmap.contains("- first item"));

    // A bad name is refused.
    let (code, _o, err) = run_c3(
        &repo,
        &[
            "manifest",
            "--file",
            "../escape.md",
            "--action",
            "replace_file",
            "--content",
            "x",
        ],
    );
    assert_eq!(code, 1);
    assert!(err.contains("refused"), "err: {err}");

    let _ = std::fs::remove_dir_all(&repo);
}
