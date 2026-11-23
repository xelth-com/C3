//! End-to-end tests of the delegation-rule repair (0.2.5): `c3 init` writes
//! `.claude/rules/02-delegation.md` only when it is missing, so a project set up before a section
//! existed never got it. `c3 doctor` reports such a file; `c3 doctor --fix` appends the missing
//! "BRANCHES AND WORKTREES" section (only that, only to a file that still carries C3's title line);
//! a rewritten file is reported and never edited; the SessionStart hook and a C3 child never edit.
//!
//! Each test builds a throwaway repository in a temp dir and sets its identity LOCALLY - never
//! `git config --global`. The `c3` binary cargo built for this test is used (`CARGO_BIN_EXE_c3`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git on PATH");
    assert!(
        out.status.success(),
        "git {args:?} failed in {} (exit {:?}): {}",
        repo.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
}

fn scratch_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "c3-deleg-{name}-{}-{}",
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

fn run_c3(repo: &Path, args: &[&str], envs: &[(&str, &str)]) -> (i32, String, String) {
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

/// The delegation template `c3 init` writes (the same file the binary embeds).
const DELEGATION_TEMPLATE: &str = include_str!("../../c3/templates/02-delegation.md");

const NO_CONFIG: &[(&str, &str)] = &[("C3_CONFIG", "none")];

/// The template as an OLD project has it: without the "BRANCHES AND WORKTREES" section.
fn old_delegation_rule() -> String {
    let all = DELEGATION_TEMPLATE.replace("\r\n", "\n");
    let cut = all
        .find("## BRANCHES AND WORKTREES")
        .expect("the template has the section");
    format!("{}\n", all[..cut].trim_end())
}

/// An otherwise clean workspace holding `rule` as `.claude/rules/02-delegation.md`.
fn repo_with_rule(name: &str, rule: &str) -> PathBuf {
    let repo = scratch_dir(name);
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "t@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.join("README.md"), "# scratch\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "init"]);
    std::fs::create_dir_all(repo.join(".c3")).unwrap();
    std::fs::write(repo.join(".c3/ROADMAP.md"), "# ROADMAP\n\nAll done.\n").unwrap();
    std::fs::write(
        repo.join(".mcp.json"),
        "{\"mcpServers\":{\"c3\":{\"command\":\"/abs/c3\",\"args\":[\"mcp\"]}}}",
    )
    .unwrap();
    std::fs::create_dir_all(repo.join(".claude/rules")).unwrap();
    std::fs::write(repo.join(".claude/rules/02-delegation.md"), rule).unwrap();
    repo
}

#[test]
fn doctor_reports_an_old_delegation_rule_and_fix_appends_only_the_section() {
    let old = old_delegation_rule();
    let repo = repo_with_rule("old", &old);
    let rule = repo.join(".claude/rules/02-delegation.md");

    let (code, out, err) = run_c3(&repo, &["doctor"], NO_CONFIG);
    assert_eq!(code, 1, "an old C3 rule is a finding: {out}\n{err}");
    assert!(
        out.contains("[!] .claude/rules/02-delegation.md: C3 wrote it before the \"BRANCHES AND WORKTREES\" section existed - c3 doctor --fix appends that section"),
        "{out}"
    );
    let (_c, json, _e) = run_c3(&repo, &["doctor", "--json"], NO_CONFIG);
    let v: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    assert_eq!(v["clean"], false, "{v}");
    assert_eq!(v["delegation_rule"]["status"], "missing_section", "{v}");
    assert_eq!(v["delegation_rule"]["c3_written"], true, "{v}");
    // reporting changes nothing
    assert_eq!(std::fs::read_to_string(&rule).unwrap(), old);

    // the SessionStart hook never does this
    let quiet = &[
        ("C3_CONFIG", "none"),
        ("C3_MAINTAIN", "off"),
        ("C3_UPDATE_CHECK", "off"),
    ];
    let (code, _out, _e) = run_c3(&repo, &["hook"], quiet);
    assert_eq!(code, 0);
    assert_eq!(
        std::fs::read_to_string(&rule).unwrap(),
        old,
        "the hook edited the rule file"
    );

    // --fix appends the section and ends clean
    let (code, out, err) = run_c3(&repo, &["doctor", "--fix"], NO_CONFIG);
    assert_eq!(code, 0, "doctor --fix should end clean: {out}\n{err}");
    assert!(
        out.contains("[fixed] .claude/rules/02-delegation.md: appended the \"BRANCHES AND WORKTREES\" section"),
        "{out}"
    );
    assert!(
        out.contains(
            "[ok] .claude/rules/02-delegation.md has the \"BRANCHES AND WORKTREES\" section"
        ),
        "{out}"
    );
    let body = std::fs::read_to_string(&rule).unwrap();
    assert!(body.starts_with(&old), "the old text was changed");
    assert_eq!(
        body.trim_end(),
        DELEGATION_TEMPLATE.replace("\r\n", "\n").trim_end(),
        "the file is the template now"
    );
    // no temporary file is left behind
    let leftovers: Vec<String> = std::fs::read_dir(repo.join(".claude/rules"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("c3-repair"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // a second run has nothing to do
    let (code, out, _e) = run_c3(&repo, &["doctor", "--fix"], NO_CONFIG);
    assert_eq!(code, 0);
    assert!(!out.contains("[fixed]"), "{out}");
    assert_eq!(std::fs::read_to_string(&rule).unwrap(), body);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn doctor_fix_json_names_the_completed_rule_file() {
    let repo = repo_with_rule("json", &old_delegation_rule());
    let (code, json, err) = run_c3(&repo, &["doctor", "--fix", "--json"], NO_CONFIG);
    assert_eq!(code, 0, "{json}\n{err}");
    let v: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    assert_eq!(v["fixed_rules"][0], ".claude/rules/02-delegation.md", "{v}");
    assert_eq!(v["delegation_rule"]["status"], "current", "{v}");
    assert_eq!(v["clean"], true, "{v}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_rewritten_delegation_rule_is_reported_and_never_edited() {
    let mine = "# My own delegation rules\n\nDo it my way.\n";
    let repo = repo_with_rule("mine", mine);
    let rule = repo.join(".claude/rules/02-delegation.md");
    for args in [&["doctor"][..], &["doctor", "--fix"][..]] {
        let (code, out, err) = run_c3(&repo, args, NO_CONFIG);
        assert_eq!(code, 0, "a rewritten file is a warning only: {out}\n{err}");
        assert!(
            out.contains("[warn] .claude/rules/02-delegation.md: no \"BRANCHES AND WORKTREES\" section, and the file is not C3's template any more - left as it is"),
            "{out}"
        );
        assert!(!out.contains("[fixed]"), "{out}");
        assert_eq!(std::fs::read_to_string(&rule).unwrap(), mine);
    }
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn doctor_fix_leaves_the_delegation_rule_alone_inside_a_c3_child() {
    let old = old_delegation_rule();
    let repo = repo_with_rule("child", &old);
    let rule = repo.join(".claude/rules/02-delegation.md");
    let (code, out, _e) = run_c3(
        &repo,
        &["doctor", "--fix"],
        &[("C3_CONFIG", "none"), ("C3_CHILD", "1")],
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("[i] --fix skipped inside a C3 child"), "{out}");
    assert!(!out.contains("[fixed]"), "{out}");
    assert_eq!(std::fs::read_to_string(&rule).unwrap(), old);
    let _ = std::fs::remove_dir_all(&repo);
}

// A second name of the file (a hard link) would see the edit too: refused and reported, both names
// keep their bytes.
#[test]
fn doctor_fix_refuses_a_hard_linked_delegation_rule() {
    let old = old_delegation_rule();
    let base = scratch_dir("hardlink");
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join(".claude/rules")).unwrap();
    git(&repo, &["init", "-q"]);
    let outside = base.join("sentinel.md");
    std::fs::write(&outside, &old).unwrap();
    let rule = repo.join(".claude/rules/02-delegation.md");
    std::fs::hard_link(&outside, &rule).unwrap();
    let (code, out, _e) = run_c3(&repo, &["doctor", "--fix"], NO_CONFIG);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("[!] .claude/rules/02-delegation.md: the \"BRANCHES AND WORKTREES\" section not appended - it has 2 hard links"),
        "{out}"
    );
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), old);
    assert_eq!(std::fs::read_to_string(&rule).unwrap(), old);
    let _ = std::fs::remove_dir_all(&base);
}

// (R4, F17-6) What is checked, said exactly: `c3 doctor` recognises a C3-written rule by its title
// line alone, so a file that keeps that line around a body of the user's own counts as C3's - and
// `--fix` only ever APPENDS the missing section: every existing byte stays as it was.
#[test]
fn doctor_fix_on_a_title_line_file_only_appends_and_keeps_every_byte() {
    let title = DELEGATION_TEMPLATE
        .lines()
        .next()
        .unwrap()
        .trim_start_matches('\u{feff}')
        .trim_end();
    let mine = format!("{title}\r\n\r\nmy own rules\r\n  with odd   spacing\r\n");
    let repo = repo_with_rule("title-only", &mine);
    let rule = repo.join(".claude/rules/02-delegation.md");
    let (code, out, err) = run_c3(&repo, &["doctor", "--fix"], NO_CONFIG);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(
        out.contains("[fixed] .claude/rules/02-delegation.md"),
        "{out}"
    );
    let body = std::fs::read(&rule).unwrap();
    assert!(
        body.starts_with(mine.as_bytes()),
        "an existing byte changed:\n{}",
        String::from_utf8_lossy(&body)
    );
    let added = String::from_utf8(body[mine.len()..].to_vec()).unwrap();
    assert!(
        added.trim_start().starts_with("## BRANCHES AND WORKTREES"),
        "{added}"
    );
    // in the file's own line ending
    assert!(!added.replace("\r\n", "").contains('\n'), "{added:?}");
    let _ = std::fs::remove_dir_all(&repo);
}

// (0.2.5, S1) A rules file C3 wrote that still has the old template sentence naming `.eck/` is a
// finding, and `c3 doctor --fix` replaces exactly that line with the current one (the file then
// equals the template); a rules file whose text about `.eck/` the user wrote is reported and left
// alone; inside a C3 child nothing is replaced.
#[test]
fn doctor_fix_replaces_the_old_eck_sentence_and_leaves_a_rewritten_file_alone() {
    const OLD: &str = "(in a C3-managed repository `.eck/` is ignored, so there is no commit).";
    const NEW: &str = "(in a C3-managed repository `.c3/` is ignored, so there is no commit).";
    assert!(
        DELEGATION_TEMPLATE.contains(NEW),
        "the template has the new sentence"
    );
    let old_rule = DELEGATION_TEMPLATE.replace(NEW, OLD);
    let repo = repo_with_rule("eck-sentence", &old_rule);
    let rule = repo.join(".claude/rules/02-delegation.md");
    let mine = "# My protocol\n\nOur notes live in `.eck/` and stay there.\n";
    let proto = repo.join(".claude/rules/01-c3-protocol.md");
    std::fs::write(&proto, mine).unwrap();

    let (code, out, err) = run_c3(&repo, &["doctor"], NO_CONFIG);
    assert_eq!(code, 1, "an old sentence is a finding: {out}\n{err}");
    assert!(
        out.contains("[!] .claude/rules/02-delegation.md: an old C3 template sentence names .eck/ - c3 doctor --fix replaces it"),
        "{out}"
    );
    assert!(
        out.contains("[warn] .claude/rules/01-c3-protocol.md: names .eck/ in text that is not C3's template - left as it is"),
        "{out}"
    );

    // a C3 child replaces nothing
    let child = &[("C3_CONFIG", "none"), ("C3_CHILD", "1")];
    let (_c, _out, _e) = run_c3(&repo, &["doctor", "--fix"], child);
    assert_eq!(std::fs::read_to_string(&rule).unwrap(), old_rule);

    let (code, out, err) = run_c3(&repo, &["doctor", "--fix"], NO_CONFIG);
    assert_eq!(code, 0, "only the warning is left: {out}\n{err}");
    assert!(
        out.contains("[fixed] .claude/rules/02-delegation.md: the template sentence naming .eck/ now names .c3/"),
        "{out}"
    );
    assert_eq!(
        std::fs::read_to_string(&rule).unwrap(),
        DELEGATION_TEMPLATE,
        "exactly the old line was replaced"
    );
    assert_eq!(std::fs::read_to_string(&proto).unwrap(), mine);
    assert!(
        out.contains("[warn] .claude/rules/01-c3-protocol.md"),
        "{out}"
    );
    let _ = std::fs::remove_dir_all(&repo);
}
