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
    run_c3_env(repo, args, &[])
}

fn run_c3_env(repo: &Path, args: &[&str], envs: &[(&str, &str)]) -> (i32, String, String) {
    let mut cmd = Command::new(c3_bin());
    // a test run inside a C3 child (a worker running the suite) must not inherit the mark: the
    // tests that need it set it explicitly
    cmd.current_dir(repo).args(args).env_remove("C3_CHILD");
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
        let p = repo.join(format!(".c3/{name}.md"));
        let body = std::fs::read_to_string(&p).unwrap_or_else(|_| panic!("missing {name}"));
        assert!(body.contains("[STUB"), "{name} missing marker: {body}");
    }
    // .gitignore carries .c3/.
    let gi = std::fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert!(gi.contains(".c3/"), "gitignore: {gi}");
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
fn init_writes_agent_files_with_floating_aliases() {
    let repo = scratch_repo("agent-aliases");
    let (code, out, err) = run_c3(&repo, &["init"]);
    assert_eq!(code, 0, "init failed: {out}\n{err}");

    for (file, alias) in [
        ("opus-worker.md", "opus"),
        ("sonnet-worker.md", "sonnet"),
        ("haiku-worker.md", "haiku"),
    ] {
        let body = std::fs::read_to_string(repo.join(format!(".claude/agents/{file}"))).unwrap();
        assert!(
            body.contains(&format!("model: {alias}")),
            "{file} should pin the alias {alias}: {body}"
        );
        // No frontmatter model line names a concrete claude-* id.
        assert!(
            !body
                .lines()
                .any(|l| l.starts_with("model:") && l.contains("claude-")),
            "{file} still pins a concrete model id: {body}"
        );
    }
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn doctor_flags_a_pinned_agent_model_only() {
    let repo = scratch_repo("doctor-pin");
    // Otherwise-clean workspace (resolved manifest, c3 mcp, c3 rule, no eck-core).
    std::fs::create_dir_all(repo.join(".c3")).unwrap();
    std::fs::write(repo.join(".c3/ROADMAP.md"), "# ROADMAP\n\nAll done.\n").unwrap();
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
    // Agent files: one pinned to an old id, plus an alias / inherit / missing-line one.
    std::fs::create_dir_all(repo.join(".claude/agents")).unwrap();
    std::fs::write(
        repo.join(".claude/agents/opus-worker.md"),
        "---\nname: opus-worker\nmodel: claude-opus-4-8\n---\nbody\n",
    )
    .unwrap();
    std::fs::write(
        repo.join(".claude/agents/sonnet-worker.md"),
        "---\nname: sonnet-worker\nmodel: sonnet\n---\nbody\n",
    )
    .unwrap();
    std::fs::write(
        repo.join(".claude/agents/inherit-worker.md"),
        "---\nname: inherit-worker\nmodel: inherit\n---\nbody\n",
    )
    .unwrap();
    std::fs::write(
        repo.join(".claude/agents/nomodel-worker.md"),
        "---\nname: nomodel-worker\n---\nbody\n",
    )
    .unwrap();

    let (code, out, err) = run_c3(&repo, &["doctor"]);
    assert_eq!(
        code, 1,
        "a pinned agent must make doctor dirty: {out}\n{err}"
    );
    assert!(
        out.contains("opus-worker.md") && out.contains("claude-opus-4-8"),
        "pinned file not reported: {out}"
    );
    assert!(out.contains("(opus)"), "suggested alias missing: {out}");
    // The alias / inherit / missing-line files are not warned about.
    assert!(
        !out.contains("sonnet-worker.md"),
        "alias file wrongly reported: {out}"
    );
    assert!(
        !out.contains("inherit-worker.md"),
        "inherit file wrongly reported: {out}"
    );
    assert!(
        !out.contains("nomodel-worker.md"),
        "missing-model file wrongly reported: {out}"
    );
    // --json carries the pinned agent and the dirty verdict.
    let (jcode, jout, _e) = run_c3(&repo, &["doctor", "--json"]);
    assert_eq!(jcode, 1);
    assert!(jout.contains("claude-opus-4-8"), "json: {jout}");
    assert!(
        jout.contains("\"clean\":false") || jout.contains("\"clean\": false"),
        "json verdict: {jout}"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn doctor_fix_rewrites_a_pin_then_exits_clean() {
    let repo = scratch_repo("doctor-fix");
    // Otherwise-clean workspace.
    std::fs::create_dir_all(repo.join(".c3")).unwrap();
    std::fs::write(repo.join(".c3/ROADMAP.md"), "# ROADMAP\n\nAll done.\n").unwrap();
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
    // A pinned agent (Claude family) plus one of an unknown family (left reported, not fixed).
    std::fs::create_dir_all(repo.join(".claude/agents")).unwrap();
    std::fs::write(
        repo.join(".claude/agents/opus-worker.md"),
        "---\nname: opus-worker\nmodel: claude-opus-4-8\n---\nbody\n",
    )
    .unwrap();

    let (code, out, err) = run_c3_env(&repo, &["doctor", "--fix"], &[("C3_CONFIG", "none")]);
    assert_eq!(code, 0, "doctor --fix should end clean: {out}\n{err}");
    assert!(
        out.contains("[fixed] .claude/agents/opus-worker.md: model claude-opus-4-8 -> opus"),
        "fix line missing: {out}"
    );
    assert!(out.contains("clean"), "verdict: {out}");
    let body = std::fs::read_to_string(repo.join(".claude/agents/opus-worker.md")).unwrap();
    assert!(body.contains("model: opus"), "not rewritten: {body}");
    assert!(!body.contains("claude-opus-4-8"), "old id left: {body}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn hook_refreshes_repo_models_once_and_honours_the_off_switch() {
    let repo = scratch_repo("hook-models");
    std::fs::create_dir_all(repo.join(".claude/agents")).unwrap();
    let agent = repo.join(".claude/agents/opus-worker.md");
    let pinned = "---\nname: opus-worker\nmodel: claude-opus-4-8\n---\nbody\n";
    std::fs::write(&agent, pinned).unwrap();

    // Quiet the other hook actions so the run is deterministic and fast.
    let envs = &[
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
    ];

    let (code, out, err) = run_c3_env(&repo, &["hook"], envs);
    assert_eq!(code, 0, "hook must exit 0: {out}\n{err}");
    assert!(
        out.contains("c3: model settings refreshed in this repository"),
        "refresh line missing: {out}"
    );
    assert!(
        out.contains(".claude/agents/opus-worker.md claude-opus-4-8 -> opus"),
        "refresh detail missing: {out}"
    );
    assert!(out.contains("(left uncommitted)"), "suffix missing: {out}");
    let body = std::fs::read_to_string(&agent).unwrap();
    assert!(body.contains("model: opus"), "not rewritten: {body}");

    // A second run has nothing to change, so the line is not printed again.
    let (_c, out2, _e) = run_c3_env(&repo, &["hook"], envs);
    assert!(
        !out2.contains("model settings refreshed"),
        "refresh line printed with nothing to do: {out2}"
    );

    // Re-pin, then with the off switch nothing is written and nothing is printed.
    std::fs::write(&agent, pinned).unwrap();
    let off = &[
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
        ("C3_REPO_MODELS", "off"),
    ];
    let (_c, out3, _e) = run_c3_env(&repo, &["hook"], off);
    assert!(
        !out3.contains("model settings refreshed"),
        "off switch ignored: {out3}"
    );
    let body = std::fs::read_to_string(&agent).unwrap();
    assert!(
        body.contains("claude-opus-4-8"),
        "off switch still rewrote the file: {body}"
    );

    // Inside a child that C3 itself started (a reviewer or a worker on the claude engine runs this
    // same hook in the repository) the feature is ON but must not touch the tree.
    let child = &[
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
        ("C3_CHILD", "1"),
    ];
    let (code4, out4, _e) = run_c3_env(&repo, &["hook"], child);
    assert_eq!(code4, 0, "hook must exit 0 in a child: {out4}");
    assert!(
        !out4.contains("model settings refreshed"),
        "the hook edited the repository inside a C3 child: {out4}"
    );
    let body = std::fs::read_to_string(&agent).unwrap();
    assert!(
        body.contains("claude-opus-4-8"),
        "a C3 child's hook rewrote the file: {body}"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, F04-4) `c3 doctor --fix` inside a child C3 started repairs nothing and says so; the
// report itself still runs.
#[test]
fn doctor_fix_does_nothing_inside_a_c3_child() {
    let repo = scratch_repo("doctor-fix-child");
    std::fs::create_dir_all(repo.join(".claude/agents")).unwrap();
    let agent = repo.join(".claude/agents/opus-worker.md");
    let pinned = "---\nname: opus-worker\nmodel: claude-opus-4-8\n---\nbody\n";
    std::fs::write(&agent, pinned).unwrap();
    let env = &[("C3_CONFIG", "none"), ("C3_CHILD", "1")];
    let (code, out, err) = run_c3_env(&repo, &["doctor", "--fix"], env);
    assert_eq!(code, 1, "the pin is still there: {out}\n{err}");
    assert!(
        out.contains("[i] --fix skipped inside a C3 child"),
        "skip line missing: {out}"
    );
    assert!(!out.contains("[fixed]"), "{out}");
    assert!(out.contains("model pinned to claude-opus-4-8"), "{out}");
    assert_eq!(std::fs::read_to_string(&agent).unwrap(), pinned);
    let (_c, json, _e) = run_c3_env(&repo, &["doctor", "--fix", "--json"], env);
    let v: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    assert_eq!(v["fix_skipped"], "skipped inside a C3 child", "{v}");
    assert_eq!(v["fixed"].as_array().map(Vec::len), Some(0), "{v}");
    assert_eq!(std::fs::read_to_string(&agent).unwrap(), pinned);
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, F02-1) The real hook never writes through a hard link: an agent file hard-linked to a
// file outside the repository is left alone (both names keep their bytes) and the hook line says
// why; `c3 doctor --fix` reports it as a finding.
#[test]
fn the_hook_never_edits_a_hard_linked_agent_file() {
    let base = scratch_dir("hook-hardlink");
    let repo = base.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::create_dir_all(repo.join(".claude/agents")).unwrap();
    let pinned = "---\nname: opus-worker\nmodel: claude-opus-4-8\n---\nbody\n";
    let outside = base.join("sentinel.md");
    std::fs::write(&outside, pinned).unwrap();
    let agent = repo.join(".claude/agents/opus-worker.md");
    std::fs::hard_link(&outside, &agent).unwrap();
    let envs = &[
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
        ("C3_BRANCH_CHECK", "off"),
    ];
    let (code, out, err) = run_c3_env(&repo, &["hook"], envs);
    assert_eq!(code, 0, "hook must exit 0: {out}\n{err}");
    assert!(
        out.contains("c3: model settings NOT refreshed in this repository - .claude/agents/opus-worker.md claude-opus-4-8 -> opus: it has 2 hard links"),
        "{out}"
    );
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), pinned);
    assert_eq!(std::fs::read_to_string(&agent).unwrap(), pinned);
    let (code, out, _e) = run_c3_env(&repo, &["doctor", "--fix"], &[("C3_CONFIG", "none")]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("[!] .claude/agents/opus-worker.md: model claude-opus-4-8 -> opus not repaired - it has 2 hard links"),
        "{out}"
    );
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), pinned);
    let _ = std::fs::remove_dir_all(&base);
}

// (R2, F13-2) The hook's repair needs a real repository: behind a stray `.git` entry (a regular
// file that is no `gitdir:` file) nothing is repaired, and with `GIT_DIR` set in the environment
// nothing is either; the same pin in a real repository without `GIT_DIR` is repaired.
#[test]
fn the_hook_repairs_nothing_behind_a_stray_git_entry_or_with_git_dir_set() {
    let pinned = "---\nname: opus-worker\nmodel: claude-opus-4-8\n---\nbody\n";
    let envs = [
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
        ("C3_BRANCH_CHECK", "off"),
        ("C3_CI_CHECK", "off"),
    ];
    let dir = scratch_dir("stray-git");
    std::fs::write(dir.join(".git"), "not a repository\n").unwrap();
    std::fs::create_dir_all(dir.join(".claude/agents")).unwrap();
    let agent = dir.join(".claude/agents/opus-worker.md");
    std::fs::write(&agent, pinned).unwrap();
    let (code, out, err) = run_c3_env(&dir, &["hook"], &envs);
    assert_eq!(code, 0, "hook must exit 0: {out}\n{err}");
    assert!(!out.contains("model settings"), "{out}");
    assert_eq!(std::fs::read_to_string(&agent).unwrap(), pinned);

    let repo = scratch_repo("git-dir");
    std::fs::create_dir_all(repo.join(".claude/agents")).unwrap();
    let agent = repo.join(".claude/agents/opus-worker.md");
    std::fs::write(&agent, pinned).unwrap();
    let git_dir = repo.join(".git").to_string_lossy().to_string();
    let mut with_git_dir = envs.to_vec();
    with_git_dir.push(("GIT_DIR", git_dir.as_str()));
    let (code, out, err) = run_c3_env(&repo, &["hook"], &with_git_dir);
    assert_eq!(code, 0, "hook must exit 0: {out}\n{err}");
    assert!(!out.contains("model settings"), "{out}");
    assert_eq!(std::fs::read_to_string(&agent).unwrap(), pinned);
    // the control: without GIT_DIR the same pin is repaired
    let (_c, out, _e) = run_c3_env(&repo, &["hook"], &envs);
    assert!(
        out.contains("c3: model settings refreshed in this repository"),
        "{out}"
    );
    assert!(std::fs::read_to_string(&agent)
        .unwrap()
        .contains("model: opus"));
    let _ = std::fs::remove_dir_all(&dir);
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
    assert!(
        out.contains("C3 does not need that server"),
        "the old server's line says C3 does not need it: {out}"
    );
    assert!(out.contains("eck_* tool"), "eck rule not reported: {out}");
    // (0.2.5, S1) before any writing command the legacy folder is read as the fallback - and not
    // moved by a read-only doctor
    assert!(out.contains(".eck/ROADMAP.md"), "fallback read: {out}");
    assert!(out.contains("no .c3/ folder yet"), "{out}");
    assert!(repo.join(".eck/ROADMAP.md").is_file());
    assert!(!repo.join(".c3").exists());
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) A writing command creates `.c3/` with every stub manifest and the `.gitignore` line;
// a second run adds nothing.
#[test]
fn a_writing_command_creates_the_c3_folder_with_every_stub_and_the_gitignore_line() {
    let repo = scratch_repo("folder-create");
    assert!(!repo.join(".gitignore").exists());
    let (code, out, err) = run_c3_env(
        &repo,
        &[
            "manifest",
            "--file",
            "TECH_DEBT.md",
            "--action",
            "append_to_section",
            "--section",
            "## Open",
            "--content",
            "- one",
        ],
        &[("C3_CONFIG", "none")],
    );
    assert_eq!(code, 0, "manifest failed: {out}\n{err}");
    assert!(out.contains("created .c3/"), "{out}");
    assert!(out.contains("added `.c3/` to .gitignore"), "{out}");
    for name in [
        "CONTEXT",
        "ARCHITECTURE",
        "ROADMAP",
        "TECH_DEBT",
        "JOURNAL",
        "OPERATIONS",
        "ENVIRONMENT",
        "RUNTIME_STATE",
        "DEPLOY_CHECKLIST",
    ] {
        let body = std::fs::read_to_string(repo.join(format!(".c3/{name}.md")))
            .unwrap_or_else(|_| panic!("missing {name}"));
        assert!(body.contains("[STUB:"), "{name}: {body}");
    }
    assert!(std::fs::read_to_string(repo.join(".c3/TECH_DEBT.md"))
        .unwrap()
        .contains("## Open\n- one"));
    assert_eq!(
        std::fs::read_to_string(repo.join(".gitignore")).unwrap(),
        ".c3/\n"
    );
    // a second writing command finds everything in place
    let (code, out, _e) = run_c3_env(&repo, &["fetch", "README.md"], &[("C3_CONFIG", "none")]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read_to_string(repo.join(".gitignore")).unwrap(),
        ".c3/\n"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) The SessionStart hook and a read-only `c3 doctor` never create the folder; doctor
// says how it comes into being, as a note that does not count as a finding.
#[test]
fn the_hook_and_a_read_only_doctor_never_create_the_folder() {
    let repo = scratch_repo("folder-readonly");
    let envs = &[
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
        ("C3_BRANCH_CHECK", "off"),
        ("C3_CI_CHECK", "off"),
    ];
    let (code, out, err) = run_c3_env(&repo, &["hook"], envs);
    assert_eq!(code, 0, "hook must exit 0: {out}\n{err}");
    assert!(!repo.join(".c3").exists(), "the hook created .c3/");
    // otherwise clean: only the folder note
    std::fs::write(
        repo.join(".mcp.json"),
        "{\"mcpServers\":{\"c3\":{\"command\":\"/abs/c3\",\"args\":[\"mcp\"]}}}",
    )
    .unwrap();
    let (code, out, err) = run_c3_env(&repo, &["doctor"], &[("C3_CONFIG", "none")]);
    assert!(
        out.contains("[i] no .c3/ folder - `c3 init` creates it"),
        "{out}"
    );
    assert_eq!(code, 0, "the folder note is no finding: {out}\n{err}");
    assert!(!repo.join(".c3").exists(), "doctor created .c3/");
    assert!(!repo.join(".gitignore").exists(), "doctor wrote .gitignore");
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) Inside a C3 child nothing creates or moves the folder: a writing command refuses,
// `.eck/` stays where it is.
#[test]
fn a_c3_child_never_creates_or_moves_the_folder() {
    let repo = scratch_repo("folder-child");
    let child = &[("C3_CONFIG", "none"), ("C3_CHILD", "1")];
    let (code, out, err) = run_c3_env(
        &repo,
        &[
            "manifest",
            "--file",
            "ROADMAP.md",
            "--action",
            "replace_file",
            "--content",
            "x",
        ],
        child,
    );
    assert_eq!(code, 1, "{out}\n{err}");
    assert!(err.contains("inside a C3 child"), "{err}");
    assert!(!repo.join(".c3").exists());
    std::fs::create_dir_all(repo.join(".eck")).unwrap();
    std::fs::write(repo.join(".eck/ROADMAP.md"), "# ROADMAP\n\nOLD\n").unwrap();
    for args in [
        &["init"][..],
        &["fail", "--status", "blocked"][..],
        &["doctor", "--fix"][..],
    ] {
        let (_c, out, err) = run_c3_env(&repo, args, child);
        assert!(!repo.join(".c3").exists(), "{args:?}: {out}\n{err}");
        assert!(repo.join(".eck/ROADMAP.md").is_file(), "{args:?}");
    }
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) `c3 doctor --fix` moves an `.eck/` it finds: the old report becomes
// `last-report.md`, the leftovers go to `legacy/`; it writes no stubs (doctor reports, it does not
// provision). With both folders present nothing moves and doctor says so.
#[test]
fn doctor_fix_moves_an_eck_folder_and_reports_both_folders() {
    let repo = scratch_repo("doctor-migrate");
    std::fs::create_dir_all(repo.join(".eck/lastsnapshot")).unwrap();
    std::fs::write(repo.join(".eck/ROADMAP.md"), "# ROADMAP\n\nAll done.\n").unwrap();
    std::fs::write(repo.join(".eck/anchor"), "old-tool-anchor\n").unwrap();
    std::fs::write(
        repo.join(".eck/lastsnapshot/AnswerToSA.md"),
        "# Agent Report\n\nOLD-REPORT\n",
    )
    .unwrap();
    let (_c, out, err) = run_c3_env(&repo, &["doctor", "--fix"], &[("C3_CONFIG", "none")]);
    assert!(out.contains("[fixed] moved .eck/ to .c3/"), "{out}\n{err}");
    assert!(!repo.join(".eck").exists());
    assert_eq!(
        std::fs::read_to_string(repo.join(".c3/last-report.md")).unwrap(),
        "# Agent Report\n\nOLD-REPORT\n"
    );
    assert!(repo.join(".c3/legacy/anchor").is_file());
    assert!(!repo.join(".c3/anchor").exists());
    assert!(
        !repo.join(".c3/CONTEXT.md").exists(),
        "no stubs from doctor"
    );
    assert!(std::fs::read_to_string(repo.join(".gitignore"))
        .unwrap()
        .contains(".c3/"));

    // both folders: nothing moves, doctor names it
    std::fs::create_dir_all(repo.join(".eck")).unwrap();
    std::fs::write(repo.join(".eck/ROADMAP.md"), "# ROADMAP\n\nSECOND\n").unwrap();
    let (_c, out, _e) = run_c3_env(&repo, &["doctor", "--fix"], &[("C3_CONFIG", "none")]);
    assert!(
        out.contains("both .c3/ and .eck/ exist - C3 uses .c3/; merge or remove .eck/ by hand"),
        "{out}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join(".eck/ROADMAP.md")).unwrap(),
        "# ROADMAP\n\nSECOND\n"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn doctor_passes_on_a_clean_fixture() {
    let repo = scratch_repo("doctor-clean");
    // A resolved manifest (no [STUB), a c3 mcp registration, a c3-named rule, no eck-core.
    std::fs::create_dir_all(repo.join(".c3")).unwrap();
    std::fs::write(repo.join(".c3/ROADMAP.md"), "# ROADMAP\n\nAll done.\n").unwrap();
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
fn scout_writes_a_tree_into_c3_scouts_and_refuses_inside_repo() {
    let repo = scratch_repo("scout");
    // An external directory to scout.
    let ext = scratch_dir("scout-ext");
    std::fs::create_dir_all(ext.join("lib")).unwrap();
    std::fs::write(ext.join("lib/mod.rs"), "pub fn x() {}\n").unwrap();
    std::fs::write(ext.join("main.rs"), "fn main() {}\n").unwrap();

    let (code, out, err) = run_c3(&repo, &["scout", &ext.to_string_lossy()]);
    assert_eq!(code, 0, "scout failed: {out}\n{err}");
    let scouts: Vec<_> = std::fs::read_dir(repo.join(".c3/scouts"))
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
fn fetch_writes_matched_files_into_c3_scouts() {
    let repo = scratch_repo("fetch");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(repo.join("src/b.rs"), "fn b() {}\n").unwrap();
    std::fs::write(repo.join("notes.txt"), "not matched\n").unwrap();

    let (code, out, err) = run_c3(&repo, &["fetch", "src/*.rs"]);
    assert_eq!(code, 0, "fetch failed: {out}\n{err}");
    let data: Vec<_> = std::fs::read_dir(repo.join(".c3/scouts"))
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
    // fixture would). `.gitignore` keeps .c3/ out of staging.
    std::fs::write(repo.join(".gitignore"), ".c3/\n").unwrap();
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
