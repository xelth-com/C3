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
    assert_eq!(p["patches_on_default"], true, "{p}");
    assert_eq!(p["patches_checked"], true, "{p}");
    assert_eq!(p["unknown"], true);
    assert_eq!(p["ahead"], 1);
    let o = branch(&j, "open");
    assert_eq!(o["merged"], false);
    assert_eq!(o["patches_on_default"], false, "{o}");
    assert_eq!(o["unknown"], true, "a foreign description is not a state");
    assert_eq!(o["foreign_description"], true);
    assert_eq!(j["unknown"], 2);
    assert_eq!(j["start_folder"]["on_default"], true);
    assert_eq!(j["start_folder"]["dirty"], 0);

    // The text listing says the cherry-pick case in words, and exits 0 with unknown branches.
    let (code, out, _e) = run_c3(&repo, &["branches"], &[]);
    assert_eq!(code, 0);
    assert!(
        out.contains("picked") && out.contains("patches already on main"),
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

/// The description exactly as the local config stores it (`--local -z --get-all`, the one value
/// without its NUL), `None` when unset. Panics on several values.
fn raw_description(repo: &Path, b: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "config",
            "--local",
            "-z",
            "--get-all",
            &format!("branch.{b}.description"),
        ])
        .output()
        .unwrap();
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).unwrap();
    let values: Vec<&str> = s.strip_suffix('\0').unwrap().split('\0').collect();
    assert_eq!(values.len(), 1, "several values: {values:?}");
    Some(values[0].to_string())
}

// (F02-7, F03-3, F04-9) Branch names with dots, slashes and non-ASCII characters address their own
// key; a foreign `c3: ` line is foreign; every foreign byte survives --set and --clear.
#[test]
fn descriptions_are_exact_and_odd_names_address_their_own_key() {
    let repo = fixture_repo("exact");
    let names = ["foo.bar", "feat/x-y", "функция-ü"];
    for n in names {
        git(&repo, &["branch", n]);
    }
    let foreign = "c3: hello\r\nkeep me\r\n\r\n";
    for n in names {
        git(
            &repo,
            &[
                "config",
                "--local",
                &format!("branch.{n}.description"),
                foreign,
            ],
        );
        let (code, out, err) = run_c3(
            &repo,
            &[
                "branches", "--set", n, "--state", "work", "--note", "t", "--owner", "o",
            ],
            &[],
        );
        assert_eq!(code, 0, "--set {n}: {out}\n{err}");
        let d = raw_description(&repo, n).unwrap();
        assert!(
            d.starts_with("c3: work | o | ") && d.ends_with(&format!(" | t\n{foreign}")),
            "{n}: {d:?}"
        );
    }
    // No neighbouring key was touched by the dotted name.
    assert_eq!(raw_description(&repo, "foo"), None);
    let j = branches_json(&repo);
    for n in names {
        assert_eq!(branch(&j, n)["state"], "work", "{n}: {j}");
        assert_eq!(branch(&j, n)["foreign_description"], true, "{n}: {j}");
    }
    for n in names {
        let (code, out, err) = run_c3(&repo, &["branches", "--clear", n], &[]);
        assert_eq!(code, 0, "--clear {n}: {out}\n{err}");
        assert_eq!(raw_description(&repo, n).as_deref(), Some(foreign), "{n}");
        // The foreign `c3: hello` line is not C3's: a second --clear leaves it alone.
        let (code, out, _e) = run_c3(&repo, &["branches", "--clear", n], &[]);
        assert_eq!(code, 0);
        assert!(out.contains("not a C3 record"), "{out}");
        assert_eq!(raw_description(&repo, n).as_deref(), Some(foreign), "{n}");
    }
    // A whitespace-only and an empty description survive the round trip too.
    for value in ["  \n ", ""] {
        git(
            &repo,
            &["config", "--local", "branch.foo.bar.description", value],
        );
        let (code, _o, err) = run_c3(
            &repo,
            &["branches", "--set", "foo.bar", "--state", "work"],
            &[],
        );
        assert_eq!(code, 0, "{err}");
        let (code, _o, err) = run_c3(&repo, &["branches", "--clear", "foo.bar"], &[]);
        assert_eq!(code, 0, "{err}");
        assert_eq!(
            raw_description(&repo, "foo.bar").as_deref(),
            Some(value),
            "{value:?}"
        );
    }
    // A name git would not accept is refused before anything is read or written.
    for bad in ["bad..name", "a b", "x.lock"] {
        let (code, _o, err) = run_c3(&repo, &["branches", "--set", bad, "--state", "work"], &[]);
        assert_eq!(code, 1, "{bad}");
        assert!(err.contains("is not a valid branch name"), "{bad}: {err}");
    }
    let _ = std::fs::remove_dir_all(&repo);
}

// (F02-7, F04-9) A description C3 cannot read exactly, or one that changes between C3's read and
// its write, is never written over.
#[test]
fn an_unreadable_or_concurrently_changed_description_is_not_written_over() {
    let repo = fixture_repo("race");
    // Two values under one key: not one description C3 could keep.
    git(
        &repo,
        &[
            "config",
            "--local",
            "--add",
            "branch.open.description",
            "second",
        ],
    );
    let (code, _o, err) = run_c3(
        &repo,
        &["branches", "--set", "open", "--state", "work"],
        &[],
    );
    assert_eq!(code, 1, "a set over two values must be refused");
    assert!(
        err.contains("holds 2 values") && err.contains("nothing written"),
        "{err}"
    );
    let (code, _o, err) = run_c3(&repo, &["branches", "--clear", "open"], &[]);
    assert_eq!(code, 1, "{err}");
    let all = git_out(
        &repo,
        &[
            "config",
            "--local",
            "-z",
            "--get-all",
            "branch.open.description",
        ],
    );
    assert_eq!(all, "my own note\0second\0");
    git(
        &repo,
        &[
            "config",
            "--local",
            "--unset-all",
            "branch.open.description",
        ],
    );
    git(
        &repo,
        &[
            "config",
            "--local",
            "branch.open.description",
            "my own note",
        ],
    );

    // Another writer between C3's read and its write (test hook): refused, the other value stays.
    let race = [
        ("CODEX_CONSULT_TEST_MODE", "1"),
        ("CODEX_CONSULT_TEST_BRANCH_DESC_RACE", "someone else's note"),
    ];
    let (code, _o, err) = run_c3(
        &repo,
        &["branches", "--set", "open", "--state", "work"],
        &race,
    );
    assert_eq!(code, 1, "a concurrent change must be refused");
    assert!(err.contains("changed while"), "{err}");
    assert_eq!(
        raw_description(&repo, "open").as_deref(),
        Some("someone else's note")
    );
    let _ = std::fs::remove_dir_all(&repo);
}

// (F02-8, F04-1, F03-6) The read paths never refresh or lock the index, even on a stale stat cache.
#[test]
fn the_read_paths_never_rewrite_the_index() {
    let repo = fixture_repo("index");
    make_doctor_clean(&repo);
    // A stale stat cache: same content, another mtime - a plain `git status` would rewrite the index.
    let f = std::fs::File::options()
        .write(true)
        .open(repo.join("README.md"))
        .unwrap();
    f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
        .unwrap();
    drop(f);
    let index = || std::fs::read(repo.join(".git").join("index")).unwrap();
    let before = index();
    let quiet = [
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
        ("C3_REPO_MODELS", "off"),
    ];
    for args in [
        &["branches"][..],
        &["branches", "--json"][..],
        &["doctor"][..],
        &["doctor", "--json"][..],
        &["hook"][..],
    ] {
        let (_c, out, err) = run_c3(&repo, args, &quiet);
        assert!(
            index() == before,
            "c3 {args:?} rewrote .git/index\n{out}\n{err}"
        );
        assert!(!repo.join(".git").join("index.lock").exists());
    }
    // The control: the fixture really was stale - a plain status rewrites the index.
    let st = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["status", "--porcelain"])
        .env_remove("GIT_OPTIONAL_LOCKS")
        .output()
        .unwrap();
    assert!(st.status.success());
    assert!(index() != before, "the fixture's stat cache was not stale");
    let _ = std::fs::remove_dir_all(&repo);
}

// (F02-9, F04-7) The default branch is found independently of the invoking checkout, and never
// guessed: c3.defaultBranch, origin/HEAD, init.defaultBranch, main, master - else "unknown".
#[test]
fn the_default_branch_is_found_independently_of_the_checkout() {
    let d = scratch_dir("default");
    git(&d, &["init", "-q", "-b", "trunk"]);
    git(&d, &["config", "user.email", "t@example.com"]);
    git(&d, &["config", "user.name", "Test"]);
    git(&d, &["config", "commit.gpgsign", "false"]);
    // A local value that names no branch shadows any global init.defaultBranch.
    git(&d, &["config", "init.defaultBranch", "no-such-branch"]);
    commit_file(&d, "README.md", "# scratch\n", "init");
    git(&d, &["branch", "develop"]);
    git(&d, &["checkout", "-q", "-b", "topic"]);
    commit_file(&d, "t.txt", "topic\n", "topic work");

    // No source applies: unknown, never the checkout's branch; every branch counts as unknown.
    let j = branches_json(&d);
    assert!(j["default_branch"].is_null(), "{j}");
    assert!(
        j["default_branch_error"]
            .as_str()
            .unwrap()
            .contains("c3 branches --default <name>"),
        "{j}"
    );
    assert_eq!(j["unknown"], 3, "{j}");
    assert!(branch(&j, "develop")["merged"].is_null(), "{j}");
    let (code, out, _e) = run_c3(&d, &["doctor"], &[("C3_CONFIG", "none")]);
    assert_eq!(code, 1);
    assert!(out.contains("[!] default branch unknown"), "{out}");
    let quiet = [
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
        ("C3_REPO_MODELS", "off"),
    ];
    let (_c, out, _e) = run_c3(&d, &["hook"], &quiet);
    assert!(
        out.contains("c3: default branch unknown - c3 branches --default <name>"),
        "{out}"
    );

    // init.defaultBranch names an existing branch: it is the default, from a topic checkout and
    // from a detached one alike - and it beats a `main` that also exists.
    git(&d, &["config", "init.defaultBranch", "trunk"]);
    git(&d, &["branch", "main", "trunk"]);
    for detach in [false, true] {
        if detach {
            git(&d, &["checkout", "-q", "--detach"]);
        }
        let j = branches_json(&d);
        assert_eq!(j["default_branch"], "trunk", "{j}");
        assert_eq!(branch(&j, "develop")["merged"], true, "{j}");
        assert_eq!(branch(&j, "topic")["merged"], false, "{j}");
        assert_eq!(j["start_folder"]["on_default"], false, "{j}");
    }
    git(&d, &["checkout", "-q", "topic"]);

    // c3 branches --default wins over everything; a missing branch is refused.
    let (code, out, err) = run_c3(&d, &["branches", "--default", "develop"], &[]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(
        git_out(&d, &["config", "--local", "--get", "c3.defaultBranch"]).trim(),
        "develop"
    );
    assert_eq!(branches_json(&d)["default_branch"], "develop");
    let (code, _o, err) = run_c3(&d, &["branches", "--default", "nope"], &[]);
    assert_eq!(code, 1);
    assert!(err.contains("no local branch named 'nope'"), "{err}");
    // A stored default whose branch is gone: unknown, said so - no fallback guess.
    git(&d, &["config", "--local", "c3.defaultBranch", "gone"]);
    let j = branches_json(&d);
    assert!(j["default_branch"].is_null(), "{j}");
    assert!(
        j["default_branch_error"]
            .as_str()
            .unwrap()
            .contains("'gone'"),
        "{j}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// A 32-hex id like the kimi engine's uuid, unique per call.
fn hex_id(salt: u128) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{:032x}",
        nanos ^ ((std::process::id() as u128) << 64) ^ (salt << 100)
    )
}

/// Write the owner record into a worktree's git admin directory, as the kimi engine does.
fn write_owner(wt: &Path, record: &str) {
    let dotgit = std::fs::read_to_string(wt.join(".git")).unwrap();
    let admin = dotgit
        .lines()
        .next()
        .unwrap()
        .strip_prefix("gitdir:")
        .unwrap()
        .trim()
        .to_string();
    std::fs::write(Path::new(&admin).join("c3-owner"), record).unwrap();
}

// (F02-11, F04-7, TECH_DEBT 302 c) Only a worktree directly under the temp directory with the exact
// kimi name is C3's; one no live run owns is reported stale and removed by `c3 doctor --fix`.
#[test]
fn kimi_throwaways_are_recognised_exactly_and_stale_ones_are_reported_and_removed() {
    let repo = fixture_repo("kimi");
    make_doctor_clean(&repo);
    let tmp = std::env::temp_dir();
    let stale = tmp.join(format!("c3-kimi-wt-{}", hex_id(1)));
    let live = tmp.join(format!("c3-kimi-wt-{}", hex_id(2)));
    let elsewhere_dir = scratch_dir("kimi-elsewhere");
    let elsewhere = elsewhere_dir.join(format!("c3-kimi-wt-{}", hex_id(3)));
    let notes = tmp.join(format!("c3-kimi-wt-notes-{}", hex_id(4)));
    for p in [&stale, &live, &elsewhere, &notes] {
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                &p.to_string_lossy(),
                "HEAD",
            ],
        );
    }
    let me = std::process::id();
    // The stale one names this process with a start time it never had (a reused pid); the live one
    // names this process with no start time (alive while the test runs).
    write_owner(&stale, &format!("{me} 1999-01-01T00:00:00.0000000Z\n"));
    write_owner(&live, &format!("{me} \n"));
    let file_name = |p: &Path| p.file_name().unwrap().to_string_lossy().to_string();

    let j = branches_json(&repo);
    let listed: Vec<String> = j["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["path"].as_str().unwrap().to_string())
        .collect();
    let stale_listed: Vec<String> = j["stale_worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["path"].as_str().unwrap().to_string())
        .collect();
    let has = |v: &[String], p: &Path| v.iter().any(|x| x.ends_with(&file_name(p)));
    assert!(has(&stale_listed, &stale), "stale one not reported: {j}");
    assert_eq!(stale_listed.len(), 1, "{j}");
    assert!(!has(&listed, &stale) && !has(&listed, &live), "{j}");
    assert!(
        has(&listed, &elsewhere) && has(&listed, &notes),
        "a user's worktree with the prefix was hidden: {j}"
    );

    let (code, out, _e) = run_c3(&repo, &["doctor"], &[("C3_CONFIG", "none")]);
    assert_eq!(code, 1);
    assert!(
        out.contains("[!] stale c3 worktree") && out.contains(&file_name(&stale)),
        "{out}"
    );
    assert!(!out.contains(&file_name(&live)), "{out}");

    let (_c, out, err) = run_c3(
        &repo,
        &["doctor", "--fix"],
        &[("C3_CONFIG", "none"), ("CODEX_CONSULT_ROSTER", "none")],
    );
    assert!(
        out.contains("[fixed] removed the stale c3 worktree"),
        "{out}\n{err}"
    );
    assert!(!stale.exists(), "the stale worktree is still there");
    assert!(live.exists(), "the live run's worktree was removed");
    let list = git_out(&repo, &["worktree", "list", "--porcelain"]);
    assert!(!list.contains(&file_name(&stale)), "{list}");
    let j = branches_json(&repo);
    assert_eq!(j["stale_worktrees"].as_array().unwrap().len(), 0, "{j}");

    for p in [&live, &elsewhere, &notes] {
        git(
            &repo,
            &["worktree", "remove", "--force", &p.to_string_lossy()],
        );
    }
    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&elsewhere_dir);
}
