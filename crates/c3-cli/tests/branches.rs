//! End-to-end tests for `c3 branches` and the branch checks of `c3 doctor`, the SessionStart hook
//! and `c3 finish`, in scratch repositories.
//!
//! Each test builds a throwaway repository in a temp dir and sets its identity LOCALLY
//! (`git -C <repo> config ...`) — never `git config --global`. The `c3` binary cargo built for this
//! test is used (`CARGO_BIN_EXE_c3`).

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn git_out(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} failed in {}: {}",
        repo.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn git(repo: &Path, args: &[&str]) {
    git_out(repo, args);
}

fn scratch_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "c3-branches-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn commit_file(repo: &Path, file: &str, body: &str, msg: &str) {
    std::fs::write(repo.join(file), body).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-qm", msg]);
}

/// A repository on `main` with a LOCAL identity and:
/// - `merged-b`: merged into main by a merge commit (ancestry);
/// - `picked`: its one commit cherry-picked onto main (content on main, not merged by ancestry);
/// - `open`: one commit that is nowhere else (really unmerged), carrying a foreign description.
fn fixture_repo(name: &str) -> PathBuf {
    let d = scratch_dir(name);
    git(&d, &["init", "-q", "-b", "main"]);
    git(&d, &["config", "user.email", "t@example.com"]);
    git(&d, &["config", "user.name", "Test"]);
    git(&d, &["config", "commit.gpgsign", "false"]);
    std::fs::write(d.join(".gitignore"), ".eck/\n").unwrap();
    commit_file(&d, "README.md", "# scratch\n", "init");

    git(&d, &["checkout", "-q", "-b", "merged-b"]);
    commit_file(&d, "m.txt", "merged\n", "merged work");
    git(&d, &["checkout", "-q", "main"]);
    git(
        &d,
        &["merge", "-q", "--no-ff", "-m", "merge merged-b", "merged-b"],
    );

    git(&d, &["checkout", "-q", "-b", "picked"]);
    commit_file(&d, "p.txt", "picked\n", "picked work");
    let sha = git_out(&d, &["rev-parse", "HEAD"]).trim().to_string();
    git(&d, &["checkout", "-q", "main"]);
    // main moves on first, so the picked copy is a different commit (picked onto the same parent
    // within the same second, git would recreate the very same commit object).
    commit_file(&d, "main2.txt", "main moves\n", "main moves");
    git(&d, &["cherry-pick", &sha]);

    git(&d, &["checkout", "-q", "-b", "open"]);
    commit_file(&d, "o.txt", "open\n", "open work");
    git(&d, &["checkout", "-q", "main"]);
    git(&d, &["config", "branch.open.description", "my own note"]);
    d
}

fn run_c3(repo: &Path, args: &[&str], envs: &[(&str, &str)]) -> (i32, String, String) {
    let mut cmd = Command::new(c3_bin());
    cmd.current_dir(repo)
        .args(args)
        .env_remove("CODEX_CONSULT_COORDINATOR")
        .env_remove("C3_BRANCH_CHECK")
        .env_remove("C3_CHILD");
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

fn branches_json(repo: &Path) -> Value {
    let (code, out, err) = run_c3(repo, &["branches", "--json"], &[]);
    assert_eq!(code, 0, "c3 branches --json: {out}\n{err}");
    serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("bad json ({e}): {out}"))
}

fn branch<'a>(j: &'a Value, name: &str) -> &'a Value {
    j["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == name)
        .unwrap_or_else(|| panic!("branch {name} not listed: {j}"))
}

fn description(repo: &Path, b: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["config", "--get", &format!("branch.{b}.description")])
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

#[test]
fn listing_set_and_clear() {
    let repo = fixture_repo("list");

    let j = branches_json(&repo);
    assert_eq!(j["default_branch"], "main");
    assert!(
        j["branches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["name"] != "main"),
        "the default branch is listed: {j}"
    );
    let m = branch(&j, "merged-b");
    assert_eq!(m["merged"], true);
    assert_eq!(m["unknown"], false);
    let p = branch(&j, "picked");
    assert_eq!(p["merged"], false);
    assert_eq!(p["content_on_default"], true, "{p}");
    assert_eq!(p["unknown"], true);
    assert_eq!(p["ahead"], 1);
    let o = branch(&j, "open");
    assert_eq!(o["merged"], false);
    assert_eq!(o["content_on_default"], false, "{o}");
    assert_eq!(o["unknown"], true, "a foreign description is not a state");
    assert_eq!(o["foreign_description"], true);
    assert_eq!(j["unknown"], 2);
    assert_eq!(j["start_folder"]["on_default"], true);
    assert_eq!(j["start_folder"]["dirty"], 0);

    // The text listing says the cherry-pick case in words, and exits 0 with unknown branches.
    let (code, out, _e) = run_c3(&repo, &["branches"], &[]);
    assert_eq!(code, 0);
    assert!(
        out.contains("picked") && out.contains("content on main"),
        "cherry-pick case not named: {out}"
    );
    assert!(out.contains("2 unknown"), "summary: {out}");

    // Refusals: an unknown branch, an empty note for waits / parked.
    let (code, _o, err) = run_c3(
        &repo,
        &[
            "branches", "--set", "nope", "--state", "work", "--note", "x",
        ],
        &[],
    );
    assert_eq!(code, 1);
    assert!(err.contains("no local branch named 'nope'"), "{err}");
    let (code, _o, err) = run_c3(
        &repo,
        &["branches", "--set", "picked", "--state", "waits"],
        &[],
    );
    assert_eq!(code, 1, "waits without a question must be refused");
    assert!(err.contains("question"), "{err}");
    let (code, _o, _e) = run_c3(
        &repo,
        &[
            "branches", "--set", "open", "--state", "parked", "--note", "  ",
        ],
        &[],
    );
    assert_eq!(code, 1, "parked without a condition must be refused");

    // --set: no longer unknown; the owner defaults to CODEX_CONSULT_COORDINATOR.
    let (code, out, err) = run_c3(
        &repo,
        &[
            "branches",
            "--set",
            "picked",
            "--state",
            "waits",
            "--note",
            "delete it? a | b",
        ],
        &[("CODEX_CONSULT_COORDINATOR", "claudecode/opus")],
    );
    assert_eq!(code, 0, "{out}\n{err}");
    let d = description(&repo, "picked").unwrap();
    assert!(
        d.starts_with("c3: waits | claudecode/opus | ") && d.ends_with(" | delete it? a | b"),
        "{d}"
    );
    // On the branch with a foreign note: the record goes on top, the note stays.
    let (code, _o, err) = run_c3(
        &repo,
        &[
            "branches",
            "--set",
            "open",
            "--state",
            "parked",
            "--note",
            "after review",
            "--owner",
            "me",
        ],
        &[],
    );
    assert_eq!(code, 0, "{err}");
    let d = description(&repo, "open").unwrap();
    assert!(d.starts_with("c3: parked | me | "), "{d}");
    assert!(d.ends_with("\nmy own note"), "foreign note lost: {d}");

    let j = branches_json(&repo);
    assert_eq!(j["unknown"], 0, "{j}");
    assert_eq!(branch(&j, "picked")["state"], "waits");
    assert_eq!(branch(&j, "picked")["note"], "delete it? a | b");
    assert_eq!(branch(&j, "open")["owner"], "me");

    // --clear removes the record and restores the foreign note; a foreign-only one is kept.
    let (code, out, _e) = run_c3(&repo, &["branches", "--clear", "open"], &[]);
    assert_eq!(code, 0);
    assert!(out.contains("recorded state removed"), "{out}");
    assert_eq!(description(&repo, "open").as_deref(), Some("my own note"));
    let (code, out, _e) = run_c3(&repo, &["branches", "--clear", "open"], &[]);
    assert_eq!(code, 0);
    assert!(out.contains("not a C3 record"), "{out}");
    assert_eq!(description(&repo, "open").as_deref(), Some("my own note"));
    let (code, _o, _e) = run_c3(&repo, &["branches", "--clear", "picked"], &[]);
    assert_eq!(code, 0);
    assert_eq!(description(&repo, "picked"), None);
    let (code, _o, _e) = run_c3(&repo, &["branches", "--clear", "nope"], &[]);
    assert_eq!(code, 1);

    let _ = std::fs::remove_dir_all(&repo);
}

/// Make `repo` an otherwise-clean C3 workspace for `c3 doctor`.
fn make_doctor_clean(repo: &Path) {
    std::fs::create_dir_all(repo.join(".eck")).unwrap();
    std::fs::write(repo.join(".eck/ROADMAP.md"), "# ROADMAP\n\nAll done.\n").unwrap();
    std::fs::write(
        repo.join(".mcp.json"),
        "{\"mcpServers\":{\"c3\":{\"command\":\"/abs/c3\",\"args\":[\"mcp\"]}}}",
    )
    .unwrap();
    // Ignored locally (not committed), so it survives a checkout of another branch and does not
    // count as an uncommitted change.
    std::fs::write(repo.join(".git/info/exclude"), ".mcp.json\n").unwrap();
}

#[test]
fn doctor_is_dirty_with_unknown_branches_and_worktrees_and_clean_once_recorded() {
    let repo = fixture_repo("doctor");
    make_doctor_clean(&repo);
    // A worktree on a fresh branch (merged by ancestry, but the worktree has no recorded state).
    let wt = scratch_dir("doctor-wt");
    let _ = std::fs::remove_dir_all(&wt);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "wtb",
            &wt.to_string_lossy(),
            "main",
        ],
    );

    let (code, out, err) = run_c3(&repo, &["doctor"], &[]);
    assert_eq!(
        code, 1,
        "unknown branches must make doctor dirty: {out}\n{err}"
    );
    assert!(
        out.contains(
            "2 branch(es) not merged into main and without a recorded state: open, picked"
        ),
        "{out}"
    );
    assert!(out.contains("c3 branches"), "{out}");
    assert!(
        out.contains("[!] worktree") && out.contains("(wtb)"),
        "{out}"
    );
    let (jcode, jout, _e) = run_c3(&repo, &["doctor", "--json"], &[]);
    assert_eq!(jcode, 1);
    let j: Value = serde_json::from_str(jout.trim()).unwrap();
    assert_eq!(j["clean"], false);
    assert_eq!(
        j["branches"]["unknown_branches"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        j["branches"]["unrecorded_worktrees"][0]["branch"], "wtb",
        "{j}"
    );

    for (b, state, note) in [
        ("picked", "waits", "delete it?"),
        ("open", "parked", "after review"),
        ("wtb", "work", "in work"),
    ] {
        let (c, _o, e) = run_c3(
            &repo,
            &["branches", "--set", b, "--state", state, "--note", note],
            &[],
        );
        assert_eq!(c, 0, "{e}");
    }
    let (code, out, err) = run_c3(&repo, &["doctor"], &[]);
    assert_eq!(
        code, 0,
        "doctor should be clean once recorded: {out}\n{err}"
    );
    assert!(out.contains("[ok] every branch is merged"), "{out}");

    // The start folder off the default branch and dirty: [warn] lines, exit code unchanged.
    git(&repo, &["checkout", "-q", "open"]);
    // One change git tracks (a staged file) and one untracked file: only the first is counted - a
    // C3-managed repository always holds untracked never-commit files.
    std::fs::write(repo.join("scratch.txt"), "uncommitted\n").unwrap();
    git(&repo, &["add", "scratch.txt"]);
    std::fs::write(repo.join("left-untracked.txt"), "not counted\n").unwrap();
    let (code, out, err) = run_c3(&repo, &["doctor"], &[]);
    assert_eq!(
        code, 0,
        "start-folder warnings must not change the exit code: {out}\n{err}"
    );
    assert!(
        out.contains("[warn] the start folder") && out.contains("is on open, not main"),
        "{out}"
    );
    assert!(out.contains("1 uncommitted change(s)"), "{out}");

    git(&repo, &["reset", "-q", "HEAD", "--", "scratch.txt"]);
    let _ = std::fs::remove_file(repo.join("scratch.txt"));
    let _ = std::fs::remove_file(repo.join("left-untracked.txt"));
    git(&repo, &["checkout", "-q", "main"]);
    git(&repo, &["worktree", "remove", &wt.to_string_lossy()]);
    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&wt);
}

fn hook_lines(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|l| l.starts_with("c3: ") && l.contains("without a recorded state"))
        .collect()
}

#[test]
fn hook_prints_one_line_only_when_something_is_unknown() {
    let repo = fixture_repo("hook");
    let quiet = [
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
        ("C3_REPO_MODELS", "off"),
    ];

    let (code, out, err) = run_c3(&repo, &["hook"], &quiet);
    assert_eq!(code, 0, "hook must exit 0: {out}\n{err}");
    let lines = hook_lines(&out);
    assert_eq!(lines.len(), 1, "exactly one branch line expected: {out}");
    assert!(
        lines[0].starts_with("c3: 2 branch(es) without a recorded state - c3 branches"),
        "{}",
        lines[0]
    );

    // Off switch and inside a C3 child: nothing.
    let mut off = quiet.to_vec();
    off.push(("C3_BRANCH_CHECK", "off"));
    let (code, out, _e) = run_c3(&repo, &["hook"], &off);
    assert_eq!(code, 0);
    assert!(hook_lines(&out).is_empty(), "off switch ignored: {out}");
    let mut child = quiet.to_vec();
    child.push(("C3_CHILD", "1"));
    let (code, out, _e) = run_c3(&repo, &["hook"], &child);
    assert_eq!(code, 0);
    assert!(
        hook_lines(&out).is_empty(),
        "printed inside a C3 child: {out}"
    );

    // Everything recorded: no line at all.
    for b in ["picked", "open"] {
        let (c, _o, e) = run_c3(
            &repo,
            &[
                "branches", "--set", b, "--state", "parked", "--note", "later",
            ],
            &[],
        );
        assert_eq!(c, 0, "{e}");
    }
    let (code, out, _e) = run_c3(&repo, &["hook"], &quiet);
    assert_eq!(code, 0);
    assert!(
        !out.lines()
            .any(|l| l.starts_with("c3: ") && l.contains("c3 branches")),
        "a line with nothing to say: {out}"
    );

    // The start folder off the default branch is named (one line).
    git(&repo, &["checkout", "-q", "open"]);
    let (code, out, _e) = run_c3(&repo, &["hook"], &quiet);
    assert_eq!(code, 0);
    let l: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with("c3: ") && l.contains("is on open"))
        .collect();
    assert_eq!(l.len(), 1, "{out}");
    assert!(l[0].contains("this checkout is on open"), "{}", l[0]);
    git(&repo, &["checkout", "-q", "main"]);

    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn finish_notes_unknown_branches_but_never_refuses() {
    let repo = fixture_repo("finish");
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: a",
            "--status",
            "done",
            "--no-snapshot",
        ],
        &[],
    );
    assert_eq!(code, 0, "finish must not refuse: {out}\n{err}");
    assert!(
        out.contains("Note: 2 branch(es) (open, picked) without a recorded state"),
        "{out}"
    );

    // --json carries it too.
    std::fs::write(repo.join("src2.rs"), "fn b() {}\n").unwrap();
    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: b",
            "--status",
            "done",
            "--no-snapshot",
            "--json",
        ],
        &[],
    );
    assert_eq!(code, 0, "{out}\n{err}");
    let j: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(j["branches"]["unknown_branches"][0], "open", "{j}");

    // Nothing unknown: no note.
    for b in ["picked", "open"] {
        let (c, _o, e) = run_c3(
            &repo,
            &[
                "branches", "--set", b, "--state", "parked", "--note", "later",
            ],
            &[],
        );
        assert_eq!(c, 0, "{e}");
    }
    std::fs::write(repo.join("src3.rs"), "fn c() {}\n").unwrap();
    let (code, out, _e) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: c",
            "--status",
            "done",
            "--no-snapshot",
        ],
        &[],
    );
    assert_eq!(code, 0);
    assert!(!out.contains("Note:"), "{out}");

    let _ = std::fs::remove_dir_all(&repo);
}
