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
    // `.c3/` is gitignored (as in a real C3 workspace), so the report and journal writes
    // never count as staged content. An initial commit so HEAD exists and `git reset` can
    // unstage.
    std::fs::write(d.join(".gitignore"), ".c3/\n").unwrap();
    std::fs::write(d.join("README.md"), "# scratch\n").unwrap();
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-qm", "init"]);
    d
}

/// Run the `c3` binary in `repo` with `args`; returns (exit code, stdout, stderr).
fn run_c3(repo: &Path, args: &[&str]) -> (i32, String, String) {
    run_c3_env(repo, args, &[])
}

/// [`run_c3`] with extra environment. A test run inside a C3 child (a worker running the suite)
/// must not inherit the mark, nor a folder switch: the tests that need them set them.
fn run_c3_env(repo: &Path, args: &[&str], envs: &[(&str, &str)]) -> (i32, String, String) {
    let mut cmd = Command::new(c3_bin());
    cmd.current_dir(repo)
        .args(args)
        .env_remove("C3_CHILD")
        .env_remove("C3_STATE_MIGRATE")
        .env("C3_CONFIG", "none");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn c3");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The `*.md` files directly in `dir` (none when it does not exist).
fn md_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
                .collect()
        })
        .unwrap_or_default()
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
            "--snapshot",
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

    // The report and the journal.
    let answer = std::fs::read_to_string(repo.join(".c3/last-report.md")).unwrap();
    assert_eq!(answer, "# Agent Report\n\nImplemented the thing.\n");
    let journal = std::fs::read_to_string(repo.join(".c3/JOURNAL.md")).unwrap();
    assert!(journal.starts_with("# JOURNAL"));
    assert!(journal.contains("- feat: add src"), "journal: {journal}");
    assert!(journal.contains("Implemented the thing."));

    // With --snapshot: one in both .c3/snapshots and .c3/lastsnapshot, with the report embedded.
    let snaps = md_files(&repo.join(".c3/snapshots"));
    assert_eq!(snaps.len(), 1, "expected one snapshot");
    let snap_text = std::fs::read_to_string(&snaps[0]).unwrap();
    assert!(snap_text.contains("MESSAGE FROM THE EXECUTION AGENT"));
    assert!(snap_text.contains("Implemented the thing."));
    assert_eq!(md_files(&repo.join(".c3/lastsnapshot")).len(), 1);

    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) `c3 finish` writes no snapshot unless asked; `--no-snapshot` is still accepted and
// does nothing; `--snapshot` writes one into `.c3/snapshots/`.
#[test]
fn finish_writes_no_snapshot_by_default_and_one_with_the_flag() {
    let repo = scratch_repo("nosnap");
    std::fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    let (code, out, err) = run_c3(&repo, &["finish", "--message", "feat: a", "--status", "ok"]);
    assert_eq!(code, 0, "finish failed: {out}\n{err}");
    assert!(!out.contains("Snapshot:"), "{out}");
    assert!(md_files(&repo.join(".c3/snapshots")).is_empty());
    assert!(!repo.join(".c3/lastsnapshot").exists());
    assert!(repo.join(".c3/last-report.md").is_file());

    std::fs::write(repo.join("b.rs"), "fn b() {}\n").unwrap();
    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: b",
            "--status",
            "ok",
            "--no-snapshot",
            "--json",
        ],
    );
    assert_eq!(code, 0, "finish --no-snapshot failed: {out}\n{err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert!(v["snapshot"].is_null(), "{v}");
    assert!(md_files(&repo.join(".c3/snapshots")).is_empty());

    std::fs::write(repo.join("c.rs"), "fn c() {}\n").unwrap();
    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: c",
            "--status",
            "ok",
            "--snapshot",
        ],
    );
    assert_eq!(code, 0, "finish --snapshot failed: {out}\n{err}");
    assert!(out.contains("Snapshot:"), "{out}");
    assert_eq!(md_files(&repo.join(".c3/snapshots")).len(), 1);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn finish_prunes_snapshots_beyond_keep() {
    let repo = scratch_repo("prune");
    // Pre-populate .c3/snapshots with old snapshots.
    std::fs::create_dir_all(repo.join(".c3/snapshots")).unwrap();
    for i in 0..5 {
        std::fs::write(
            repo.join(format!(".c3/snapshots/old_{i}.md")),
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
            "--snapshot",
        ],
    );
    assert_eq!(code, 0, "finish failed: {out}\n{err}");

    let count = md_files(&repo.join(".c3/snapshots")).len();
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
    assert!(repo.join(".c3/last-report.md").exists());
    let _ = std::fs::remove_dir_all(&repo);
}

/// Write the repository's pre-finish checks (`"finish_checks"` in `.c3/config.json`).
fn set_finish_checks(repo: &Path, checks: &[&str]) {
    std::fs::create_dir_all(repo.join(".c3")).unwrap();
    std::fs::write(
        repo.join(".c3").join("config.json"),
        serde_json::json!({ "finish_checks": checks }).to_string(),
    )
    .unwrap();
}

fn staged_names(repo: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["diff", "--cached", "--name-only"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// (0.2.5) A failing pre-finish check refuses the finish BEFORE anything is written or staged: its
// name, its exit code and the last 20 lines of its output are printed; no commit, nothing staged,
// no report or journal entry. `--skip-checks` goes on and says so in the output and the journal.
#[test]
fn a_failing_pre_finish_check_refuses_with_nothing_staged() {
    let repo = scratch_repo("checks-fail");
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    let failing = if cfg!(windows) {
        "(for /L %i in (1,1,25) do @echo line %i) & exit /b 3"
    } else {
        "i=1; while [ $i -le 25 ]; do echo line $i; i=$((i+1)); done; exit 3"
    };
    set_finish_checks(&repo, &[failing]);
    let (code, out, err) = run_c3(
        &repo,
        &["finish", "--message", "feat: src", "--status", "done"],
    );
    assert_eq!(code, 1, "expected refusal: {out}\n{err}");
    assert!(err.contains(failing), "the check is not named: {err}");
    assert!(err.contains("exit code 3"), "{err}");
    assert!(err.contains("line 25") && err.contains("line 6"), "{err}");
    assert!(!err.contains("line 5\n"), "more than 20 lines: {err}");
    assert!(err.contains("nothing was staged or committed"), "{err}");
    assert_eq!(head_subject(&repo), "init");
    assert_eq!(staged_names(&repo), "", "something was staged");
    assert!(
        !repo.join(".c3").join("JOURNAL.md").exists(),
        "a journal entry was written"
    );
    assert!(!repo.join(".c3").join("last-report.md").exists());

    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: src",
            "--status",
            "done",
            "--skip-checks",
        ],
    );
    assert_eq!(code, 0, "--skip-checks finish failed: {out}\n{err}");
    assert!(
        out.contains("Pre-finish checks skipped (--skip-checks)"),
        "{out}"
    );
    assert_eq!(head_subject(&repo), "feat: src");
    let journal = std::fs::read_to_string(repo.join(".c3").join("JOURNAL.md")).unwrap();
    assert!(
        journal.contains("Pre-finish checks skipped (--skip-checks)"),
        "{journal}"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5) Passing checks run in the repository root, marked as a C3 child, and the finish commits;
// a repository without a list runs none (the other tests), and a list that does not parse refuses.
#[test]
fn passing_pre_finish_checks_run_in_the_repository_as_a_child() {
    let repo = scratch_repo("checks-pass");
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    // the mark file is written into .c3/ (git-ignored), so the check itself stages nothing
    let mark = if cfg!(windows) {
        ">.c3\\mark.txt echo child=%C3_CHILD%"
    } else {
        "echo child=$C3_CHILD > .c3/mark.txt"
    };
    set_finish_checks(&repo, &[mark, "git --version"]);
    let (code, out, err) = run_c3(
        &repo,
        &["finish", "--message", "feat: src", "--status", "done"],
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("Pre-finish checks passed: "), "{out}");
    assert_eq!(head_subject(&repo), "feat: src");
    let seen = std::fs::read_to_string(repo.join(".c3").join("mark.txt")).unwrap();
    assert_eq!(seen.trim(), "child=1");

    std::fs::write(repo.join(".c3").join("config.json"), "{ not json").unwrap();
    std::fs::write(repo.join("src2.rs"), "fn b() {}\n").unwrap();
    let (code, _out, err) = run_c3(
        &repo,
        &["finish", "--message", "feat: src2", "--status", "done"],
    );
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("config.json"), "{err}");
    assert_eq!(head_subject(&repo), "feat: src");

    // (F46-6, F45-6) --skip-checks over that file says it could not be read and that nothing ran -
    // never "none configured" - in the output and in the journal
    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: src2",
            "--status",
            "done",
            "--skip-checks",
        ],
    );
    assert_eq!(code, 0, "{out}\n{err}");
    let said = "Pre-finish checks skipped (--skip-checks): the configuration could not be read (";
    assert!(
        out.contains(said) && out.contains("no check was run"),
        "{out}"
    );
    assert!(!out.contains("none configured"), "{out}");
    let journal = std::fs::read_to_string(repo.join(".c3").join("JOURNAL.md")).unwrap();
    assert!(journal.contains(said), "{journal}");
    assert_eq!(head_subject(&repo), "feat: src2");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn fail_writes_report_and_snapshot_without_committing() {
    let repo = scratch_repo("fail");
    // A full snapshot first establishes the anchor so a --fail delta has a baseline.
    let (code, _o, e) = run_c3(&repo, &["snapshot", "--no-tree"]);
    assert_eq!(code, 0, "snapshot failed: {e}");
    let before_snaps = md_files(&repo.join(".c3/snapshots")).len();

    // An uncommitted change for the emergency delta to capture.
    std::fs::write(repo.join("wip.rs"), "fn wip() {}\n").unwrap();

    let before = head_subject(&repo);
    let (code, out, err) = run_c3(
        &repo,
        &["fail", "--status", "Stuck on the parser.", "--snapshot"],
    );
    assert_eq!(code, 0, "fail failed: {out}\n{err}");
    // No commit happened.
    assert_eq!(head_subject(&repo), before);

    let answer = std::fs::read_to_string(repo.join(".c3/last-report.md")).unwrap();
    assert_eq!(
        answer,
        "# Agent Report (BLOCKED/FAILED)\n\nStuck on the parser.\n"
    );
    let snaps = md_files(&repo.join(".c3/snapshots")).len();
    assert_eq!(
        snaps,
        before_snaps + 1,
        "fail --snapshot should write an emergency snapshot"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn fail_without_anchor_writes_a_full_snapshot() {
    // No prior full snapshot → no anchor. `c3 fail --snapshot` must fall back to a FULL emergency
    // snapshot rather than reporting "nothing changed".
    let repo = scratch_repo("fail-noanchor");
    std::fs::write(repo.join("wip.rs"), "fn wip() {}\n").unwrap();

    let (code, out, err) = run_c3(
        &repo,
        &["fail", "--status", "Blocked, no baseline.", "--snapshot"],
    );
    assert_eq!(code, 0, "fail failed: {out}\n{err}");
    assert!(
        out.contains("Snapshot:"),
        "expected a snapshot path, got: {out}"
    );
    let snaps = md_files(&repo.join(".c3/snapshots"));
    assert_eq!(
        snaps.len(),
        1,
        "a full emergency snapshot should be written"
    );
    // A full snapshot of the repo includes the committed README and the WIP file.
    let text = std::fs::read_to_string(&snaps[0]).unwrap();
    assert!(text.contains("--- File: wip.rs ---"), "snapshot: {text}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn handoff_snapshot_body_excludes_the_never_commit_set() {
    let repo = scratch_repo("handoff-body");
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
        &[
            "finish",
            "--message",
            "feat: body",
            "--status",
            "done",
            "--snapshot",
        ],
    );
    assert_eq!(code, 0, "finish failed: {out}\n{err}");

    let snaps = md_files(&repo.join(".c3/snapshots"));
    assert_eq!(snaps.len(), 1);
    let text = std::fs::read_to_string(&snaps[0]).unwrap();
    // Ordinary code is present; the never-commit artifacts are not.
    assert!(text.contains("--- File: src.rs ---"), "snapshot: {text}");
    assert!(!text.contains("EVENTBODYUNIQUE"), "events body leaked");
    assert!(!text.contains("STDERRUNIQUE"), "stderr body leaked");
    assert!(!text.contains("events.jsonl"), "events path marker leaked");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn manifest_actions_edit_c3_files() {
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
    let roadmap = std::fs::read_to_string(repo.join(".c3/ROADMAP.md")).unwrap();
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
    let roadmap = std::fs::read_to_string(repo.join(".c3/ROADMAP.md")).unwrap();
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
    let roadmap = std::fs::read_to_string(repo.join(".c3/ROADMAP.md")).unwrap();
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

// ------------------------------------------------------------------ the project folder (S1)

/// The old tool's folder as an upgrade finds it: a resolved manifest, its leftovers, its report
/// and an old snapshot; `.gitignore` names only `.eck/`.
fn seed_legacy_folder(repo: &Path) {
    std::fs::write(repo.join(".gitignore"), ".eck/\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-qm", "ignore the old folder"]);
    let eck = repo.join(".eck");
    std::fs::create_dir_all(eck.join("lastsnapshot")).unwrap();
    std::fs::create_dir_all(eck.join("snapshots")).unwrap();
    std::fs::write(eck.join("ROADMAP.md"), "# ROADMAP\n\nKEEP-ME\n").unwrap();
    std::fs::write(eck.join("anchor"), "deadbeef-of-the-old-tool\n").unwrap();
    std::fs::write(eck.join("update_seq"), "7\n").unwrap();
    std::fs::write(eck.join("telemetry_queue.json"), "[]\n").unwrap();
    std::fs::write(
        eck.join("lastsnapshot/AnswerToSA.md"),
        "# Agent Report\n\nOLD-REPORT\n",
    )
    .unwrap();
    std::fs::write(eck.join("snapshots/old.md"), "old snapshot\n").unwrap();
}

/// Whether `p` exists in any form (a link included), read without following it.
fn exists_any(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

// (0.2.5, S1) `c3 finish` moves an `.eck/` it finds to `.c3/` once: the content kept, the old
// tool's leftovers under `.c3/legacy/` (its anchor is not taken for C3's), the stubs added for the
// manifests that were missing, and `.c3/` added to `.gitignore` next to the `.eck/` line.
#[test]
fn finish_moves_an_eck_folder_once_with_the_leftovers_under_legacy() {
    let repo = scratch_repo("migrate-finish");
    seed_legacy_folder(&repo);
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();

    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: migrate",
            "--status",
            "NEW-REPORT",
        ],
    );
    assert_eq!(code, 0, "finish failed: {out}\n{err}");
    assert!(out.contains("moved .eck/ to .c3/"), "{out}");
    assert!(!exists_any(&repo.join(".eck")), "the old folder is gone");
    let c3 = repo.join(".c3");
    assert_eq!(
        std::fs::read_to_string(c3.join("ROADMAP.md")).unwrap(),
        "# ROADMAP\n\nKEEP-ME\n"
    );
    for leftover in ["anchor", "update_seq", "telemetry_queue.json"] {
        assert!(c3.join("legacy").join(leftover).is_file(), "{leftover}");
        assert!(!exists_any(&c3.join(leftover)), "{leftover} left on top");
    }
    assert!(c3.join("snapshots/old.md").is_file());
    // the report this finish wrote replaced the moved one
    assert_eq!(
        std::fs::read_to_string(c3.join("last-report.md")).unwrap(),
        "# Agent Report\n\nNEW-REPORT\n"
    );
    assert!(!exists_any(&c3.join("lastsnapshot/AnswerToSA.md")));
    // the missing manifests were stubbed, the existing one left as it was
    assert!(std::fs::read_to_string(c3.join("CONTEXT.md"))
        .unwrap()
        .contains("[STUB:"));
    let gi = std::fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert_eq!(gi, ".eck/\n.c3/\n", "{gi}");
    // the `.gitignore` change is part of the commit; nothing under `.c3/` is
    assert!(is_tracked(&repo, ".gitignore"));
    assert!(!is_tracked(&repo, ".c3/ROADMAP.md"));
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) A `.eck` that is a link (a junction on Windows, a symlink elsewhere) is never moved:
// the link and its target stay as they are and a fresh `.c3/` is used.
#[test]
fn an_eck_link_is_refused_and_its_target_untouched() {
    let repo = scratch_repo("migrate-link");
    let outside = repo.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("ROADMAP.md"), "# ROADMAP\n\nOUTSIDE\n").unwrap();
    #[cfg(windows)]
    {
        let ok = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(repo.join(".eck"))
            .arg(&outside)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "mklink /J failed");
    }
    #[cfg(not(windows))]
    std::os::unix::fs::symlink(&outside, repo.join(".eck")).unwrap();

    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    let (code, out, err) = run_c3(&repo, &["finish", "--message", "feat: l", "--status", "ok"]);
    assert_eq!(code, 0, "finish failed: {out}\n{err}");
    assert!(out.contains("not moved"), "{out}");
    let meta = std::fs::symlink_metadata(repo.join(".eck")).unwrap();
    assert!(
        meta.file_type().is_symlink() || !meta.file_type().is_file(),
        "the link is still there"
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("ROADMAP.md")).unwrap(),
        "# ROADMAP\n\nOUTSIDE\n"
    );
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        1,
        "nothing was written into the link's target"
    );
    assert!(repo.join(".c3/ROADMAP.md").is_file(), "a fresh .c3/");
    #[cfg(windows)]
    let _ = std::fs::remove_dir(repo.join(".eck"));
    #[cfg(not(windows))]
    let _ = std::fs::remove_file(repo.join(".eck"));
    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&outside);
}

// (0.2.5, S1) With both folders present nothing is moved or merged: C3 uses `.c3/`.
#[test]
fn both_folders_present_nothing_moves() {
    let repo = scratch_repo("migrate-both");
    std::fs::create_dir_all(repo.join(".eck")).unwrap();
    std::fs::write(repo.join(".eck/ROADMAP.md"), "# ROADMAP\n\nOLD\n").unwrap();
    std::fs::create_dir_all(repo.join(".c3")).unwrap();
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    let (code, out, err) = run_c3(&repo, &["finish", "--message", "feat: b", "--status", "ok"]);
    assert_eq!(code, 0, "finish failed: {out}\n{err}");
    assert!(!out.contains("moved .eck/"), "{out}");
    assert_eq!(
        std::fs::read_to_string(repo.join(".eck/ROADMAP.md")).unwrap(),
        "# ROADMAP\n\nOLD\n"
    );
    assert!(std::fs::read_to_string(repo.join(".c3/ROADMAP.md"))
        .unwrap()
        .contains("[STUB:"));
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) `C3_STATE_MIGRATE=off`: `.eck/` is left untouched and a fresh `.c3/` is created.
#[test]
fn the_migrate_switch_off_leaves_eck_untouched() {
    let repo = scratch_repo("migrate-off");
    seed_legacy_folder(&repo);
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    let (code, out, err) = run_c3_env(
        &repo,
        &["finish", "--message", "feat: o", "--status", "ok"],
        &[("C3_STATE_MIGRATE", "off")],
    );
    assert_eq!(code, 0, "finish failed: {out}\n{err}");
    assert!(repo.join(".eck/anchor").is_file());
    assert!(repo.join(".eck/lastsnapshot/AnswerToSA.md").is_file());
    assert_eq!(
        std::fs::read_to_string(repo.join(".eck/ROADMAP.md")).unwrap(),
        "# ROADMAP\n\nKEEP-ME\n"
    );
    assert!(std::fs::read_to_string(repo.join(".c3/ROADMAP.md"))
        .unwrap()
        .contains("[STUB:"));
    assert!(!repo.join(".c3/legacy").exists());
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) A cache an earlier C3 kept under `<collab>/.c3/` moves into `<repo>/.c3/` entry by
// entry; an existing destination wins and the source entry is left and named.
#[test]
fn a_legacy_collab_anchor_moves_and_an_existing_destination_wins() {
    let repo = scratch_repo("migrate-cache");
    let legacy = repo.join(".collab/.c3");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("anchor"), "cafebabe\n").unwrap();
    std::fs::write(legacy.join("snapshot_seq"), "4").unwrap();
    std::fs::create_dir_all(repo.join(".c3")).unwrap();
    std::fs::write(repo.join(".c3/snapshot_seq"), "9").unwrap();
    let (code, out, err) = run_c3(
        &repo,
        &[
            "manifest",
            "--file",
            "ROADMAP.md",
            "--action",
            "replace_file",
            "--content",
            "# ROADMAP\n\nx\n",
        ],
    );
    assert_eq!(code, 0, "manifest failed: {out}\n{err}");
    assert_eq!(
        std::fs::read_to_string(repo.join(".c3/anchor")).unwrap(),
        "cafebabe\n"
    );
    assert!(!legacy.join("anchor").exists());
    assert_eq!(
        std::fs::read_to_string(repo.join(".c3/snapshot_seq")).unwrap(),
        "9"
    );
    assert!(legacy.join("snapshot_seq").is_file());
    assert!(
        out.contains("left .collab/.c3/snapshot_seq"),
        "the left entry is named: {out}"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

// ------------------------------------------------------------------ collab paths in the folder (S1 point 11)

/// A consultation record of task `t` under `<repo>/<collab>/t/`: one finding.
fn seed_findings(repo: &Path, collab: &str) {
    let t = repo.join(collab).join("t");
    std::fs::create_dir_all(&t).unwrap();
    std::fs::write(
        t.join("findings.json"),
        r#"{"task_id":"t","findings":[{"id":"F01-1","status":"proposed","severity":"major","claim":"MOVEDCLAIM"}]}"#,
    )
    .unwrap();
}

/// `c3 findings --task t --list` with the given collab directory.
fn list_findings(repo: &Path, collab: &str, envs: &[(&str, &str)]) -> (i32, String, String) {
    run_c3_env(
        repo,
        &["findings", "--task", "t", "--collab-dir", collab, "--list"],
        envs,
    )
}

// The records of a project whose skills keep them in `.eck/collab` move with the folder; the old
// spelling still finds them (one line says where they are now) and never creates a new `.eck/`.
#[test]
fn records_under_eck_collab_move_with_the_folder_and_the_old_path_still_finds_them() {
    let repo = scratch_repo("collab-move");
    seed_legacy_folder(&repo);
    seed_findings(&repo, ".eck/collab");
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: m",
            "--status",
            "ok",
            "--collab-dir",
            ".eck/collab",
        ],
    );
    assert_eq!(code, 0, "finish failed: {out}\n{err}");
    assert!(repo.join(".c3/collab/t/findings.json").is_file());
    assert!(!exists_any(&repo.join(".eck")));

    let (code, out, err) = list_findings(&repo, ".eck/collab", &[]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("total 1"), "the records are found: {out}");
    assert_eq!(
        err.matches(
            "c3: collab directory .eck/collab is .c3/collab since the project folder moved"
        )
        .count(),
        1,
        "one line, once: {err}"
    );
    assert!(!exists_any(&repo.join(".eck")), "no new .eck/");
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S2 F36-4) `--collab-dir .c3/collab` on a repository that still has `.eck/`: a command that
// only reads finds the records at `.eck/collab` and moves nothing - every one of them.
#[test]
fn read_only_commands_with_a_c3_collab_path_never_move_an_unmoved_repository() {
    let repo = scratch_repo("collab-c3");
    seed_legacy_folder(&repo);
    seed_findings(&repo, ".eck/collab");
    let outside = repo.with_extension("out");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(repo.join("brief.md"), "# Brief\n\nlook at src\n").unwrap();
    let pack_out = outside.join("p.md").to_string_lossy().to_string();
    let explain_out = outside.join("e.md").to_string_lossy().to_string();
    let (code, out, err) = list_findings(&repo, ".c3/collab", &[]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("total 1"), "the records are found: {out}");
    assert!(!err.contains("moved"), "{err}");
    let c = ".c3/collab";
    let runs: Vec<Vec<&str>> = vec![
        vec!["findings", "--task", "t", "--collab-dir", c, "--stats"],
        vec!["consult", "--status", "--task", "t", "--collab-dir", c],
        vec![
            "consult",
            "--dry-run",
            "--task",
            "t",
            "--collab-dir",
            c,
            "--brief",
            "brief.md",
        ],
        vec!["work", "--status", "--task", "t", "--collab-dir", c],
        vec![
            "pack",
            "--out",
            &pack_out,
            "--brief",
            "brief.md",
            "--collab-dir",
            c,
        ],
        vec!["explain", "--out", &explain_out, "--collab-dir", c],
        vec!["explain", "--collab-dir", c],
        vec!["index", "stats"],
        vec!["providers", "--no-network", "--collab-dir", c],
        vec!["doctor"],
    ];
    // an empty codex home: nothing of the machine's own setup is read or probed
    let home = outside.join("codex-home");
    std::fs::create_dir_all(&home).unwrap();
    let home_s = home.to_string_lossy().to_string();
    for args in runs {
        let (_code, out, err) = run_c3_env(&repo, &args, &[("CODEX_HOME", home_s.as_str())]);
        assert!(
            repo.join(".eck/collab/t/findings.json").is_file(),
            "{args:?} moved the folder: {out}\n{err}"
        );
        assert!(
            !exists_any(&repo.join(".c3")),
            "{args:?} made .c3/: {out}\n{err}"
        );
        assert!(!err.contains("moved .eck/"), "{args:?}: {err}");
    }
    assert!(repo.join(".eck/anchor").is_file(), "nothing relocated");
    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&outside);
}

// With `C3_STATE_MIGRATE=off` nothing moves and a collab path is used exactly as given.
#[test]
fn with_the_switch_off_a_collab_path_is_used_as_given() {
    let repo = scratch_repo("collab-off");
    seed_legacy_folder(&repo);
    seed_findings(&repo, ".eck/collab");
    let off = &[("C3_STATE_MIGRATE", "off")];
    let (_c, out, err) = list_findings(&repo, ".c3/collab", off);
    assert!(!out.contains("total 1"), "{out}");
    assert!(!err.contains("moved"), "{err}");
    assert!(repo.join(".eck/collab/t/findings.json").is_file());
    assert!(repo.join(".eck/anchor").is_file());
    assert!(!repo.join(".c3/collab/t/findings.json").exists());
    let (code, out, err) = list_findings(&repo, ".eck/collab", off);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("total 1"), "{out}");
    assert!(!err.contains("collab directory"), "{err}");
    let _ = std::fs::remove_dir_all(&repo);
}

// A rename that fails (the test hook stands in for a file a running consultation holds open) leaves
// `.eck/` in place: the old tool's leftovers were relocated inside it (the rename is the last step),
// `c3 finish` goes on with the legacy folder and says so, a read-only command reads the records
// there, `c3 doctor --fix` reports "not moved".
#[test]
fn a_failed_rename_leaves_eck_whole_and_the_command_works_on_the_legacy_path() {
    let repo = scratch_repo("collab-renamefail");
    seed_legacy_folder(&repo);
    seed_findings(&repo, ".eck/collab");
    let fail = &[
        ("CODEX_CONSULT_TEST_MODE", "1"),
        ("CODEX_CONSULT_TEST_STATE_RENAME_FAIL", "1"),
    ];
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    let (code, out, err) = run_c3_env(
        &repo,
        &["finish", "--message", "feat: f", "--status", "STILL-OK"],
        fail,
    );
    assert_eq!(code, 0, "finish goes on: {out}\n{err}");
    assert!(
        out.contains("the project folder could not be moved now (the rename of .eck/ to .c3/ failed: simulated by CODEX_CONSULT_TEST_STATE_RENAME_FAIL; nothing else was tried); using .eck/ for this run"),
        "{out}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join(".eck/last-report.md")).unwrap(),
        "# Agent Report\n\nSTILL-OK\n"
    );
    assert!(!exists_any(&repo.join(".c3")), "no fresh .c3/ beside .eck/");
    // the leftovers went first, inside the old folder; the folder itself was not renamed
    assert!(repo.join(".eck/legacy/anchor").is_file());
    assert!(!exists_any(&repo.join(".eck/anchor")));
    assert!(repo.join(".eck/collab/t/findings.json").is_file());

    let (code, out, err) = list_findings(&repo, ".c3/collab", fail);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(
        out.contains("total 1"),
        "the legacy records are read: {out}"
    );
    assert!(!exists_any(&repo.join(".c3")));

    let (code, out, _e) = run_c3_env(&repo, &["doctor", "--fix"], fail);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("[!] not moved: the rename of .eck/ to .c3/ failed"),
        "{out}"
    );
    assert!(repo.join(".eck/legacy/anchor").is_file());
    let _ = std::fs::remove_dir_all(&repo);
}

// ------------------------------------------------------------------------------ S2 (F36-1 .. F36-7)

/// [`run_c3_env`] as a started child (stdout and stderr captured).
fn spawn_c3(repo: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Child {
    let mut cmd = Command::new(c3_bin());
    cmd.current_dir(repo)
        .args(args)
        .env_remove("C3_CHILD")
        .env_remove("C3_STATE_MIGRATE")
        .env("C3_CONFIG", "none")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.spawn().expect("spawn c3")
}

/// Wait (at most 60 s) until `p` exists.
fn wait_for(p: &Path) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !p.exists() {
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    true
}

/// Every file under `dir`, repo-relative POSIX paths, sorted.
fn files_under(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(
                    p.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut v = Vec::new();
    walk(dir, dir, &mut v);
    v.sort();
    v
}

// (F36-1) Two writing commands at once on a repository with only `.eck/`: the first (`c3 fail`) is
// held inside the move (test hook) while the second (`c3 manifest`) waits for the lock; afterwards
// there is one `.c3/`, no `.eck/`, and both commands' records - the journal entry and report of the
// one, the manifest edit of the other - are in `.c3/`. (Two journal writers would race the journal's
// read-modify-write itself, which is not the move's.)
#[test]
fn two_writers_at_once_leave_one_folder_with_both_records() {
    let repo = scratch_repo("two-writers");
    seed_legacy_folder(&repo);
    let release = repo.with_extension("release");
    let release_s = release.to_string_lossy().to_string();
    let mut held = release.clone().into_os_string();
    held.push(".held");
    let held = PathBuf::from(held);
    let wait = ("CODEX_CONSULT_TEST_STATE_LOCK_WAIT_MS", "60000");
    let first = spawn_c3(
        &repo,
        &["fail", "--status", "FIRST-WRITER"],
        &[
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("CODEX_CONSULT_TEST_STATE_MOVE_HOLD", release_s.as_str()),
            wait,
        ],
    );
    assert!(wait_for(&held), "the first writer never entered the move");
    let second = spawn_c3(
        &repo,
        &[
            "manifest",
            "--file",
            "ROADMAP.md",
            "--action",
            "replace_file",
            "--content",
            "# ROADMAP\n\nSECOND-WRITER\n",
        ],
        &[("CODEX_CONSULT_TEST_MODE", "1"), wait],
    );
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(
        repo.join(".eck").is_dir(),
        "the second writer did not move it"
    );
    std::fs::write(&release, "go").unwrap();
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    for (o, who) in [(&first, "first"), (&second, "second")] {
        assert!(
            o.status.success(),
            "{who}: {}\n{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
    }
    assert!(!exists_any(&repo.join(".eck")), "no second folder");
    let journal = std::fs::read_to_string(repo.join(".c3/JOURNAL.md")).unwrap();
    assert!(journal.contains("FIRST-WRITER"), "{journal}");
    assert!(std::fs::read_to_string(repo.join(".c3/last-report.md"))
        .unwrap()
        .contains("FIRST-WRITER"));
    assert_eq!(
        std::fs::read_to_string(repo.join(".c3/ROADMAP.md")).unwrap(),
        "# ROADMAP\n\nSECOND-WRITER\n"
    );
    assert!(repo.join(".c3/legacy/anchor").is_file());
    let second_out = String::from_utf8_lossy(&second.stdout);
    assert!(!second_out.contains("could not be moved"), "{second_out}");
    let _ = std::fs::remove_file(&release);
    let _ = std::fs::remove_file(&held);
    let _ = std::fs::remove_dir_all(&repo);
}

// (F36-2) A process killed between the leftover steps and the rename (test hook), then another
// writing command: the result equals an uninterrupted move, and `.c3/anchor` is never the old tool's.
#[test]
fn a_kill_before_the_rename_then_another_writer_equals_an_uninterrupted_move() {
    let killed = scratch_repo("killed-move");
    let clean = scratch_repo("clean-move");
    seed_legacy_folder(&killed);
    seed_legacy_folder(&clean);
    let (code, out, err) = run_c3_env(
        &killed,
        &["fail", "--status", "SAME"],
        &[
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("CODEX_CONSULT_TEST_STATE_MOVE_KILL", "1"),
        ],
    );
    assert_eq!(code, 86, "killed before the rename: {out}\n{err}");
    // the leftovers were relocated inside the old folder; it was not renamed
    assert!(killed.join(".eck/legacy/anchor").is_file());
    assert!(!exists_any(&killed.join(".eck/anchor")));
    assert!(!exists_any(&killed.join(".c3")));
    let (code, out, err) = run_c3(&killed, &["fail", "--status", "SAME"]);
    assert_eq!(code, 0, "the next writer takes over: {out}\n{err}");
    let (code, out, err) = run_c3(&clean, &["fail", "--status", "SAME"]);
    assert_eq!(code, 0, "{out}\n{err}");
    for repo in [&killed, &clean] {
        assert!(!exists_any(&repo.join(".eck")));
        assert!(
            !exists_any(&repo.join(".c3/anchor")),
            "the old anchor on top"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join(".c3/legacy/anchor")).unwrap(),
            "deadbeef-of-the-old-tool\n"
        );
    }
    assert_eq!(
        files_under(&killed.join(".c3")),
        files_under(&clean.join(".c3"))
    );
    assert_eq!(
        std::fs::read_to_string(killed.join(".c3/last-report.md")).unwrap(),
        std::fs::read_to_string(clean.join(".c3/last-report.md")).unwrap()
    );
    let _ = std::fs::remove_dir_all(&killed);
    let _ = std::fs::remove_dir_all(&clean);
}

// (F36-2) A `.c3/` an S1 build left half done - the old tool's leftovers at its top - is finished by
// the next writing command; its `anchor` goes to `legacy/` too.
#[test]
fn a_half_done_folder_of_the_s1_layout_is_finished_by_the_next_writer() {
    let repo = scratch_repo("half-done");
    seed_legacy_folder(&repo);
    std::fs::rename(repo.join(".eck"), repo.join(".c3")).unwrap();
    let (code, out, err) = run_c3(
        &repo,
        &[
            "manifest",
            "--file",
            "ROADMAP.md",
            "--action",
            "replace_file",
            "--content",
            "# ROADMAP\n\nnew\n",
        ],
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("finished the move"), "{out}");
    let c3 = repo.join(".c3");
    for leftover in ["anchor", "update_seq", "telemetry_queue.json"] {
        assert!(c3.join("legacy").join(leftover).is_file(), "{leftover}");
        assert!(!exists_any(&c3.join(leftover)), "{leftover} left on top");
    }
    assert_eq!(
        std::fs::read_to_string(c3.join("last-report.md")).unwrap(),
        "# Agent Report\n\nOLD-REPORT\n"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

/// A directory link at `link` to `target`: a junction on Windows, a symlink elsewhere.
fn dir_link(link: &Path, target: &Path) {
    #[cfg(windows)]
    {
        // (cmd reads a `/` inside a path as a switch)
        let win = |p: &Path| p.to_string_lossy().replace('/', "\\");
        let ok = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(win(link))
            .arg(win(target))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "mklink /J failed");
    }
    #[cfg(not(windows))]
    std::os::unix::fs::symlink(target, link).unwrap();
}

/// Remove a link made by [`dir_link`] (never its target).
fn drop_link(link: &Path) {
    #[cfg(windows)]
    let _ = std::fs::remove_dir(link);
    #[cfg(not(windows))]
    let _ = std::fs::remove_file(link);
}

// (F36-3) Links inside `.eck/` are never followed: a linked `lastsnapshot` keeps its outside report
// (the move goes on); a linked `legacy` stops the move (nothing written outside); a linked collab
// directory stops the move (its runs cannot be checked) and its cache is never taken.
#[test]
fn links_inside_eck_are_never_read_moved_or_written_through() {
    // a linked lastsnapshot
    let repo = scratch_repo("link-snap");
    seed_legacy_folder(&repo);
    let outside = repo.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("AnswerToSA.md"), "OUTSIDE-REPORT\n").unwrap();
    std::fs::remove_dir_all(repo.join(".eck/lastsnapshot")).unwrap();
    dir_link(&repo.join(".eck/lastsnapshot"), &outside);
    let (code, out, err) = run_c3(&repo, &["fail", "--status", "NEW"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("moved .eck/ to .c3/"), "{out}");
    assert_eq!(
        std::fs::read_to_string(outside.join("AnswerToSA.md")).unwrap(),
        "OUTSIDE-REPORT\n"
    );
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
    let report = std::fs::read_to_string(repo.join(".c3/last-report.md")).unwrap();
    assert!(
        report.contains("NEW") && !report.contains("OUTSIDE"),
        "{report}"
    );
    drop_link(&repo.join(".c3/lastsnapshot"));
    let _ = std::fs::remove_dir_all(&repo);

    // a linked legacy
    let repo = scratch_repo("link-legacy");
    seed_legacy_folder(&repo);
    std::fs::remove_file(outside.join("AnswerToSA.md")).unwrap();
    dir_link(&repo.join(".eck/legacy"), &outside);
    let (code, out, err) = run_c3(&repo, &["fail", "--status", "NEW"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("could not be moved now"), "{out}");
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        0,
        "nothing written outside"
    );
    assert!(repo.join(".eck/anchor").is_file());
    assert!(!exists_any(&repo.join(".c3")));
    drop_link(&repo.join(".eck/legacy"));
    let _ = std::fs::remove_dir_all(&repo);

    // a linked collab directory with a cache inside
    let repo = scratch_repo("link-collab");
    seed_legacy_folder(&repo);
    std::fs::create_dir_all(outside.join(".c3")).unwrap();
    std::fs::write(outside.join(".c3/anchor"), "cafe\n").unwrap();
    dir_link(&repo.join(".eck/collab"), &outside);
    let (code, out, err) = run_c3(
        &repo,
        &["fail", "--status", "NEW", "--collab-dir", ".eck/collab"],
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("the runs behind it are not checked"), "{out}");
    assert_eq!(
        std::fs::read_to_string(outside.join(".c3/anchor")).unwrap(),
        "cafe\n"
    );
    assert!(repo.join(".eck/anchor").is_file());
    assert!(!exists_any(&repo.join(".c3")));
    drop_link(&repo.join(".eck/collab"));
    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&outside);
}

// (F36-5) Every spelling of the legacy collab directory finds the same records before and after the
// move, and after it no spelling - not even a command that writes there - creates a new `.eck/`.
#[test]
fn every_collab_spelling_finds_the_same_records_and_never_creates_eck() {
    let repo = scratch_repo("spellings");
    seed_legacy_folder(&repo);
    seed_findings(&repo, ".eck/collab");
    let abs = repo
        .join(".eck")
        .join("collab")
        .to_string_lossy()
        .to_string();
    let spellings = [
        ".eck/collab",
        "./.eck/collab",
        "sub/../.eck/collab",
        ".c3/../.eck/collab",
        abs.as_str(),
    ];
    for s in spellings {
        let (code, out, err) = list_findings(&repo, s, &[]);
        assert_eq!(code, 0, "{s}: {out}\n{err}");
        assert!(out.contains("total 1"), "{s}: {out}");
    }
    assert!(repo.join(".eck").is_dir());
    let (code, out, err) = run_c3(&repo, &["fail", "--status", "MOVE"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(!exists_any(&repo.join(".eck")));
    for s in spellings {
        let (code, out, err) = list_findings(&repo, s, &[]);
        assert_eq!(code, 0, "{s}: {out}\n{err}");
        assert!(out.contains("total 1"), "{s}: {out}");
        assert!(!exists_any(&repo.join(".eck")), "{s} created a new .eck/");
    }
    // a command that writes into the collab directory, through the `..` spelling
    let (_code, out, err) = run_c3(
        &repo,
        &[
            "findings",
            "--task",
            "t",
            "--collab-dir",
            "sub/../.eck/collab",
            "--id",
            "F01-1",
            "--status",
            "rejected",
            "--note",
            "spelled with ..",
        ],
    );
    assert!(!exists_any(&repo.join(".eck")), "a new .eck/: {out}\n{err}");
    assert!(repo.join(".c3/collab/t/findings.json").is_file());
    let _ = std::fs::remove_dir_all(&repo);
}

// (F36-6) The live-run guard fails closed: a run directory that cannot be listed and a run owner whose
// start time cannot be read (test hooks) both stop the move; with the record readable and its owner
// provably someone else, the move goes on.
#[test]
fn an_unreadable_run_directory_or_start_time_stops_the_move() {
    let repo = scratch_repo("failclosed");
    seed_legacy_folder(&repo);
    let run_dir = repo.join(".eck/collab/t/work/01-zai-glm");
    std::fs::create_dir_all(&run_dir).unwrap();
    let (code, out, err) = run_c3_env(
        &repo,
        &["fail", "--status", "S1"],
        &[
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("CODEX_CONSULT_TEST_STATE_UNREADABLE", "t"),
        ],
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("cannot be listed"), "{out}");
    assert!(repo.join(".eck/anchor").is_file());
    assert!(!exists_any(&repo.join(".c3")));

    // a running work record whose owner (this test process) has another recorded start time: with
    // the start time unreadable its identity cannot be told - a run may be live
    let me = std::process::id();
    std::fs::write(
        run_dir.join("run.json"),
        format!(
            r#"{{"id":"01-zai-glm","task":"t","seq":1,"provider":"zai","model":"glm","engine":"claude","argv":[],"child_env_scrubbed":[],"status":"running","started":"2026-11-23T10:00:00+00:00","pid":{me},"pid_start":"2000-01-01T00:00:00.0000000Z"}}"#
        ),
    )
    .unwrap();
    let me_s = me.to_string();
    let (code, out, err) = run_c3_env(
        &repo,
        &["fail", "--status", "S2"],
        &[
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("CODEX_CONSULT_TEST_START_UNREADABLE", me_s.as_str()),
        ],
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("a run is live under .eck/collab/t"), "{out}");
    assert!(repo.join(".eck/anchor").is_file());
    assert!(!exists_any(&repo.join(".c3")));

    // readable: the pid's start time is not the recorded one - the owner is gone, the move goes on
    let (code, out, err) = run_c3(&repo, &["fail", "--status", "S3"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("moved .eck/ to .c3/"), "{out}");
    let journal = std::fs::read_to_string(repo.join(".c3/JOURNAL.md")).unwrap();
    for s in ["S1", "S2", "S3"] {
        assert!(journal.contains(s), "{s}: {journal}");
    }
    let _ = std::fs::remove_dir_all(&repo);
}

// (F36-7) The README's sentence about the switch, fixed by its behaviour: with `C3_STATE_MIGRATE=off`
// C3 does not move `.eck/`, creates and uses `.c3/` for its own files, and a collab directory placed
// under `.eck/` is used as given.
#[test]
fn the_switch_off_keeps_eck_and_c3_holds_c3s_own_files() {
    let readme =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md"))
            .unwrap();
    let flat = readme.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("then C3 does not move `.eck/` and creates and uses `.c3/` for its own files; a collab directory you place under `.eck/` is still used as given."),
        "the README sentence changed - change this test with it"
    );
    let repo = scratch_repo("switch-off");
    seed_legacy_folder(&repo);
    seed_findings(&repo, ".eck/collab");
    let before = files_under(&repo.join(".eck"));
    let off = &[("C3_STATE_MIGRATE", "off")];
    let (code, out, err) = run_c3_env(
        &repo,
        &[
            "fail",
            "--status",
            "OWN-FILES",
            "--collab-dir",
            ".eck/collab",
        ],
        off,
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(files_under(&repo.join(".eck")), before, ".eck/ as it was");
    assert_eq!(
        std::fs::read_to_string(repo.join(".eck/anchor")).unwrap(),
        "deadbeef-of-the-old-tool\n"
    );
    assert!(std::fs::read_to_string(repo.join(".c3/last-report.md"))
        .unwrap()
        .contains("OWN-FILES"));
    assert!(!exists_any(&repo.join(".eck/last-report.md")));
    assert!(std::fs::read_to_string(repo.join(".c3/JOURNAL.md"))
        .unwrap()
        .contains("OWN-FILES"));
    let (code, out, err) = list_findings(&repo, ".eck/collab", off);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("total 1"), "used as given: {out}");
    let _ = std::fs::remove_dir_all(&repo);
}
