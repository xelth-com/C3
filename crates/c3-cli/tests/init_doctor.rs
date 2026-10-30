//! End-to-end tests for `c3 init` / `c3 doctor` / `c3 scout` / `c3 fetch` and the secret
//! gate's added-lines behaviour, in scratch directories.
//!
//! Each test builds a throwaway repository in a temp dir and sets its identity LOCALLY
//! (`git -C <repo> config ...`) — never `git config --global`. The `c3` binary cargo built
//! for this test is used (`CARGO_BIN_EXE_c3`).

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

fn scratch_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "c3-initdoc-{name}-{}-{}",
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

/// A fresh git repo with a LOCAL identity and one initial commit.
fn scratch_repo(name: &str) -> PathBuf {
    let d = scratch_dir(name);
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@example.com"]);
    git(&d, &["config", "user.name", "Test"]);
    git(&d, &["config", "commit.gpgsign", "false"]);
    std::fs::write(d.join("README.md"), "# scratch\n").unwrap();
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-qm", "init"]);
    d
}

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

#[test]
fn init_creates_everything_then_a_second_init_skips_everything() {
    let repo = scratch_repo("init");

    let (code, out, err) = run_c3(&repo, &["init"]);
    assert_eq!(code, 0, "init failed: {out}\n{err}");

    // Manifests with [STUB markers.
    for name in [
        "CONTEXT",
        "ARCHITECTURE",
        "ROADMAP",
        "RUNTIME_STATE",
        "DEPLOY_CHECKLIST",
    ] {
        let p = repo.join(format!(".eck/{name}.md"));
        let body = std::fs::read_to_string(&p).unwrap_or_else(|_| panic!("missing {name}"));
        assert!(body.contains("[STUB"), "{name} missing marker: {body}");
    }
    // .gitignore carries .eck/.
    let gi = std::fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert!(gi.contains(".eck/"), "gitignore: {gi}");
    // Project files.
    assert!(repo.join("CLAUDE.md").exists());
    assert!(repo.join(".claude/rules/01-c3-protocol.md").exists());
    assert!(repo.join(".claude/rules/02-delegation.md").exists());
    assert!(repo.join(".claude/agents/sonnet-worker.md").exists());
    assert!(repo.join(".claude/agents/opus-worker.md").exists());
    assert!(repo.join(".claude/agents/haiku-worker.md").exists());
    // The protocol rule names c3 tools, not eck tools.
    let proto = std::fs::read_to_string(repo.join(".claude/rules/01-c3-protocol.md")).unwrap();
    assert!(proto.contains("c3_finish_task"), "proto: {proto}");
    assert!(!proto.contains("eck_finish_task"));
    let deleg = std::fs::read_to_string(repo.join(".claude/rules/02-delegation.md")).unwrap();
    assert!(
        !deleg.contains("eck_finish_task"),
        "delegation still names eck tool"
    );
    // .mcp.json registers a c3 mcp server.
    let mcp = std::fs::read_to_string(repo.join(".mcp.json")).unwrap();
    assert!(mcp.contains("\"c3\""), "mcp.json: {mcp}");
    assert!(mcp.contains("\"mcp\""), "mcp.json args: {mcp}");

    // A second init creates nothing.
    let (code, out, err) = run_c3(&repo, &["init"]);
    assert_eq!(code, 0, "second init failed: {out}\n{err}");
    assert!(
        out.contains("Created nothing"),
        "second init should create nothing: {out}"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn init_never_overwrites_an_existing_claude_md() {
    let repo = scratch_repo("noclobber");
    std::fs::write(repo.join("CLAUDE.md"), "MY OWN CONTENT\n").unwrap();

    let (code, out, err) = run_c3(&repo, &["init"]);
    assert_eq!(code, 0, "init failed: {out}\n{err}");
    let body = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
    assert_eq!(body, "MY OWN CONTENT\n", "CLAUDE.md was overwritten");
    assert!(
        out.contains("CLAUDE.md"),
        "skip list should mention it: {out}"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn init_keeps_other_mcp_servers() {
    let repo = scratch_repo("mcpmerge");
    std::fs::write(
        repo.join(".mcp.json"),
        "{\n  \"mcpServers\": {\n    \"eck-core\": { \"command\": \"node\", \"args\": [\"x.js\"] }\n  }\n}\n",
    )
    .unwrap();

    let (code, _o, err) = run_c3(&repo, &["init"]);
    assert_eq!(code, 0, "init failed: {err}");
    let mcp = std::fs::read_to_string(repo.join(".mcp.json")).unwrap();
    assert!(mcp.contains("eck-core"), "other server dropped: {mcp}");
    assert!(mcp.contains("\"c3\""), "c3 not added: {mcp}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn doctor_flags_stubs_missing_mcp_and_eck_core() {
    let repo = scratch_repo("doctor-dirty");
    // A stub manifest.
    std::fs::create_dir_all(repo.join(".eck")).unwrap();
    std::fs::write(
        repo.join(".eck/ROADMAP.md"),
        "# ROADMAP\n\n[STUB: fill me]\n",
    )
    .unwrap();
    // An eck-core server, no c3 server.
    std::fs::write(
        repo.join(".mcp.json"),
        "{\"mcpServers\":{\"eck-core\":{\"command\":\"node\",\"args\":[\"x.js\"]}}}",
    )
    .unwrap();
    // A rule naming an eck tool.
    std::fs::create_dir_all(repo.join(".claude/rules")).unwrap();
    std::fs::write(
        repo.join(".claude/rules/01-eck-protocol.md"),
        "Call `eck_finish_task` when done.\n",
    )
    .unwrap();

    let (code, out, err) = run_c3(&repo, &["doctor"]);
    assert_eq!(code, 1, "doctor should report issues: {out}\n{err}");
    assert!(out.contains("ROADMAP.md"), "stub not reported: {out}");
    assert!(out.contains("NOT registered"), "mcp not reported: {out}");
    assert!(out.contains("eck-core"), "eck-core not reported: {out}");
    assert!(out.contains("eck_* tool"), "eck rule not reported: {out}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn doctor_passes_on_a_clean_fixture() {
    let repo = scratch_repo("doctor-clean");
    // A resolved manifest (no [STUB), a c3 mcp registration, a c3-named rule, no eck-core.
    std::fs::create_dir_all(repo.join(".eck")).unwrap();
    std::fs::write(repo.join(".eck/ROADMAP.md"), "# ROADMAP\n\nAll done.\n").unwrap();
    std::fs::write(
        repo.join(".mcp.json"),
        "{\"mcpServers\":{\"c3\":{\"command\":\"/abs/c3\",\"args\":[\"mcp\"]}}}",
    )
    .unwrap();
    std::fs::create_dir_all(repo.join(".claude/rules")).unwrap();
    std::fs::write(
        repo.join(".claude/rules/01-c3-protocol.md"),
        "Call `c3_finish_task` when done.\n",
    )
    .unwrap();

    let (code, out, err) = run_c3(&repo, &["doctor"]);
    assert_eq!(code, 0, "doctor should be clean: {out}\n{err}");
    assert!(out.contains("clean"), "out: {out}");
    // --json carries the same verdict.
    let (jcode, jout, _e) = run_c3(&repo, &["doctor", "--json"]);
    assert_eq!(jcode, 0);
    assert!(
        jout.contains("\"clean\":true") || jout.contains("\"clean\": true"),
        "json: {jout}"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn scout_writes_a_tree_into_eck_scouts_and_refuses_inside_repo() {
    let repo = scratch_repo("scout");
    // An external directory to scout.
    let ext = scratch_dir("scout-ext");
    std::fs::create_dir_all(ext.join("lib")).unwrap();
    std::fs::write(ext.join("lib/mod.rs"), "pub fn x() {}\n").unwrap();
    std::fs::write(ext.join("main.rs"), "fn main() {}\n").unwrap();

    let (code, out, err) = run_c3(&repo, &["scout", &ext.to_string_lossy()]);
    assert_eq!(code, 0, "scout failed: {out}\n{err}");
    let scouts: Vec<_> = std::fs::read_dir(repo.join(".eck/scouts"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("scout_tree_"))
        .collect();
    assert_eq!(scouts.len(), 1, "expected one scout_tree file");
    let text = std::fs::read_to_string(scouts[0].path()).unwrap();
    assert!(text.contains("# Scout:"), "header missing: {text}");
    assert!(text.contains("c3 fetch"), "fetch hint missing");
    assert!(text.contains("main.rs"), "tree missing a file: {text}");

    // A path inside this repo is refused.
    let (code, _o, err) = run_c3(&repo, &["scout", "."]);
    assert_eq!(code, 1, "scouting self should fail");
    assert!(err.contains("inside this repository"), "err: {err}");

    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&ext);
}

#[test]
fn fetch_writes_matched_files_into_eck_scouts() {
    let repo = scratch_repo("fetch");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(repo.join("src/b.rs"), "fn b() {}\n").unwrap();
    std::fs::write(repo.join("notes.txt"), "not matched\n").unwrap();

    let (code, out, err) = run_c3(&repo, &["fetch", "src/*.rs"]);
    assert_eq!(code, 0, "fetch failed: {out}\n{err}");
    let data: Vec<_> = std::fs::read_dir(repo.join(".eck/scouts"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("scout_data_"))
        .collect();
    assert_eq!(data.len(), 1, "expected one scout_data file");
    let text = std::fs::read_to_string(data[0].path()).unwrap();
    assert!(text.contains("--- File: src/a.rs ---"), "{text}");
    assert!(text.contains("--- File: src/b.rs ---"));
    assert!(!text.contains("notes.txt"), "unmatched file leaked");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn gate_passes_an_old_fixture_line_but_catches_a_new_secret() {
    let repo = scratch_repo("gate");
    // Commit a fixture file that ALREADY contains a secret-shaped line (as a committed test
    // fixture would). `.gitignore` keeps .eck/ out of staging.
    std::fs::write(repo.join(".gitignore"), ".eck/\n").unwrap();
    std::fs::write(
        repo.join("fixtures.rs"),
        "const OLD: &str = \"sk-abcdefghijklmnopqrstuvwxyz0123456789\";\n",
    )
    .unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "add fixture (allowed once)"]);

    // Now add an UNRELATED clean line to the same file and finish: the old fixture line is
    // not re-flagged, so the commit succeeds.
    std::fs::write(
        repo.join("fixtures.rs"),
        "const OLD: &str = \"sk-abcdefghijklmnopqrstuvwxyz0123456789\";\nconst NOTE: &str = \"fine\";\n",
    )
    .unwrap();
    let (code, out, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "chore: note",
            "--status",
            "ok",
            "--no-snapshot",
        ],
    );
    assert_eq!(
        code, 0,
        "finish should pass (old fixture not re-flagged): {out}\n{err}"
    );

    // Adding a NEW secret line is caught.
    std::fs::write(
        repo.join("other.rs"),
        "const KEY: &str = \"sk-zyxwvutsrqponmlkjihgfedcba9876543210\";\n",
    )
    .unwrap();
    let (code, _o, err) = run_c3(
        &repo,
        &[
            "finish",
            "--message",
            "feat: other",
            "--status",
            "ok",
            "--no-snapshot",
        ],
    );
    assert_eq!(code, 1, "a new secret must be caught");
    assert!(err.contains("other.rs"), "err: {err}");
    assert!(err.contains("openai-key"), "err kind: {err}");
    let _ = std::fs::remove_dir_all(&repo);
}
