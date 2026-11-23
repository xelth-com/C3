//! The C3 `kimi` engine end to end through the real `c3` binary against a FAKE kimi CLI -
//! `tests/fixtures/fake-kimi.{cmd,ps1}` (driven by `FAKE_KIMI_*`): the roster entry validates and
//! shows in `c3 providers`, the dry run's plan and prompt-in-argv command line, and a structured run
//! with its ledger evidence (engine `kimi`, a `session_<uuid>` thread, no token usage).
//!
//! GUARD: every child gets a scratch USERPROFILE/HOME and a PATH without any directory that holds a
//! kimi launcher, so the real CLI can never start; the fake logs every start. Nothing reaches a real
//! provider. Windows only (the fake is a `.cmd` wrapper around a PowerShell script).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const FAKE_CMD: &str = include_str!("fixtures/fake-kimi.cmd");
const FAKE_PS1: &str = include_str!("fixtures/fake-kimi.ps1");
const ADVISE: &str = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
const SESSION_RE: &str = r"^session_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$";

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn git(repo: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git on PATH");
    assert!(st.status.success(), "git {args:?}");
}

/// git's trimmed stdout in `repo`.
fn git_out(repo: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git on PATH");
    assert!(o.status.success(), "git {args:?}");
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

/// A PATH without any directory that holds a kimi launcher (GUARD).
fn safe_path() -> String {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(';')
        .filter(|d| {
            !d.is_empty()
                && !["kimi", "kimi.exe", "kimi.cmd"]
                    .iter()
                    .any(|n| Path::new(d).join(n).exists())
        })
        .collect::<Vec<_>>()
        .join(";")
}

struct Env {
    repo: PathBuf,
    work: PathBuf,
    home: PathBuf,
    fake: PathBuf,
    reply: PathBuf,
    prose: PathBuf,
    log: PathBuf,
}

fn setup(tag: &str) -> Env {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let work = std::env::temp_dir().join(format!("c3-kimi-{tag}-{}-{nanos}", std::process::id()));
    let repo = work.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@e.com"]);
    git(&repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("app.txt"), "one\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    std::fs::create_dir_all(repo.join(".collab").join("t").join("handoffs")).unwrap();
    let home = work.join("codexhome");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "model = \"gpt-5.1\"\n").unwrap();
    // the scratch USERPROFILE/HOME, with a kimi managed-oauth token (its contents never read).
    let uh = work.join("userhome");
    std::fs::create_dir_all(uh.join("AppData").join("Local")).unwrap();
    std::fs::create_dir_all(uh.join(".kimi-code").join("credentials")).unwrap();
    std::fs::write(
        uh.join(".kimi-code")
            .join("credentials")
            .join("oauth-token"),
        "{\"oauth\":\"x\"}\n",
    )
    .unwrap();
    let fake = work.join("fake-kimi.cmd");
    std::fs::write(&fake, FAKE_CMD.replace("\r\n", "\n").replace('\n', "\r\n")).unwrap();
    std::fs::write(work.join("fake-kimi.ps1"), FAKE_PS1).unwrap();
    let reply = work.join("advise.json");
    std::fs::write(&reply, ADVISE).unwrap();
    let prose = work.join("prose.txt");
    // substantive prose (a numbered answer and 40+ words) so the format-repair gate earns a turn.
    std::fs::write(
        &prose,
        "1. The file app.txt holds a single word and there is nothing obviously wrong with it. \
         This is a deliberately long prose answer that does not use the required JSON object shape, \
         so the bridge should ask for a convert-only format-repair turn on the same session thread \
         before it records the structured reply from the reviewer.",
    )
    .unwrap();
    let log = work.join("argv.jsonl");
    Env {
        repo,
        work,
        home,
        fake,
        reply,
        prose,
        log,
    }
}

impl Env {
    fn cmd(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut cmd = Command::new(c3_bin());
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_") || k.starts_with("FAKE_") || k.starts_with("C3_") {
                cmd.env_remove(&k);
            }
        }
        let uh = self.work.join("userhome");
        cmd.current_dir(&self.repo)
            .env("PATH", safe_path())
            .env("CODEX_HOME", &self.home)
            .env("HOME", &uh)
            .env("USERPROFILE", &uh)
            .env("LOCALAPPDATA", uh.join("AppData").join("Local"))
            .env("CODEX_CONSULT_KIMI_EXE", &self.fake)
            .env("CODEX_CONSULT_TEST_MODE", "1")
            .env("CODEX_CONSULT_ROSTER", "none")
            .env("CODEX_CONSULT_HEALTH", "none")
            .env("CODEX_CONSULT_TELEMETRY", "off")
            .env("C3_TELEMETRY_HUB", "http://127.0.0.1:9/T")
            .env("C3_PRIORS", "off")
            .env("FAKE_KIMI_ARGV_LOG", &self.log)
            .env("FAKE_KIMI_REPLY", &self.reply);
        for (k, v) in env {
            if v.is_empty() {
                cmd.env_remove(k);
            } else {
                cmd.env(k, v);
            }
        }
        cmd.args(args);
        cmd
    }

    fn c3(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let _ = std::fs::remove_file(&self.log);
        self.cmd(args, env).output().expect("run c3")
    }

    fn consult(&self, extra: &[&str], env: &[(&str, &str)]) -> Output {
        let mut args = vec!["consult", "--task", "t", "--prompt", "Review app.txt"];
        args.extend_from_slice(extra);
        self.c3(&args, env)
    }

    fn ledger(&self) -> Vec<Value> {
        let p = self.repo.join(".collab").join("t").join("sessions.json");
        match std::fs::read_to_string(p) {
            Ok(t) => serde_json::from_str::<Value>(&t).unwrap()["codex"]["consults"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn last(&self) -> Value {
        self.ledger().last().cloned().unwrap()
    }

    fn turns(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter(|s| s["kind"] == "turn")
            .collect()
    }

    fn roster(&self, json: &str) -> String {
        let p = self.work.join("roster.json");
        std::fs::write(&p, json).unwrap();
        p.to_string_lossy().to_string()
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
    .replace("\r\n", "\n")
}

fn is_session(s: &str) -> bool {
    regex::Regex::new(SESSION_RE).unwrap().is_match(s)
}

// ------------------------------------------------------------------ roster + providers

#[test]
fn the_kimi_roster_entry_validates_and_shows_in_providers() {
    let e = setup("providers");
    let roster = e.roster(
        r#"{"roster_version":1,"reviewers":[{"provider":"kimi","engine":"kimi","model":"kimi-code/k3","plan":"kimi-code"}]}"#,
    );
    let o = e.c3(&["providers"], &[("CODEX_CONSULT_ROSTER", &roster)]);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(t.contains("kimi-code/k3"), "the roster model shows: {t}");
    assert!(t.contains("[kimi]") || t.contains("engine kimi"), "{t}");
}

// ------------------------------------------------------------------ dry run

#[test]
fn a_dry_run_plans_the_kimi_turn_with_the_prompt_in_argv() {
    let e = setup("dry");
    let o = e.consult(
        &["--dry-run", "--engine", "kimi", "--model", "kimi-code/k3"],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let cmd_line = t
        .lines()
        .find(|l| l.trim_start().starts_with("command"))
        .unwrap_or("")
        .to_string();
    assert!(
        cmd_line.contains("--output-format stream-json"),
        "{cmd_line}"
    );
    assert!(cmd_line.contains("-m kimi-code/k3"), "{cmd_line}");
    // (B) the argv carries a short brief POINTER, not the inline prompt.
    assert!(
        cmd_line.contains("Your brief is the file"),
        "the argv is a brief pointer: {cmd_line}"
    );
    assert!(cmd_line.contains("Read it completely first"), "{cmd_line}");
    // a dry run starts no turn.
    assert!(e.turns().is_empty(), "no turn in a dry run");
}

// ------------------------------------------------------------------ a structured run

#[test]
fn a_structured_run_is_usable_with_a_kimi_session_and_no_usage() {
    let e = setup("run");
    let o = e.consult(
        &["--engine", "kimi", "--model", "kimi-code/k3"],
        &[("FAKE_KIMI_TOOLCALL", "1")],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let last = e.last();
    assert!(
        last["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("usable"),
        "outcome: {last}"
    );
    assert_eq!(last["reviewer"]["engine"], "kimi");
    assert_eq!(last["structured"], true);
    // kimi reports no token usage.
    assert!(
        last["usage"].is_null() || last["usage"]["total_tokens"].is_null(),
        "kimi has no usage: {last}"
    );
    // exactly one kimi turn, a new thread (no resume).
    let turns = e.turns();
    assert_eq!(turns.len(), 1, "{:?}", turns);
    assert!(
        turns[0]["resume"].as_str().unwrap_or("").is_empty(),
        "a first turn mints a new session (no -S): {:?}",
        turns[0]
    );
    // (A) WRITE ISOLATION: the child ran in a throwaway git worktree, not the repo root.
    assert_eq!(
        turns[0]["git_file"], true,
        "kimi ran in a git worktree (a `.git` FILE in cwd): {:?}",
        turns[0]
    );
    assert!(
        turns[0]["cwd"]
            .as_str()
            .unwrap_or("")
            .contains("c3-kimi-wt"),
        "the cwd is a throwaway worktree: {:?}",
        turns[0]
    );
    assert_ne!(
        turns[0]["cwd"].as_str().unwrap_or(""),
        e.repo.to_string_lossy(),
        "the child did NOT run in the repo root"
    );
    // (B) the pointer named an existing brief file (the prompt written to disk).
    assert_eq!(
        turns[0]["brief_exists"], true,
        "the argv pointer named an existing brief file: {:?}",
        turns[0]
    );
    // the handoff header records the Isolation line.
    let handoff = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("01-kimi-reply.md"),
    )
    .unwrap_or_default();
    assert!(
        handoff.contains("Isolation: the reviewer ran in a throwaway git worktree"),
        "the handoff records the Isolation line: {handoff}"
    );
    // the worktree was cleaned up after the run.
    let wt = turns[0]["cwd"].as_str().unwrap_or("");
    assert!(
        !wt.is_empty() && !Path::new(wt).exists(),
        "the throwaway worktree was removed: {wt}"
    );
}

// ------------------------------------------------------------------ (F02-21) a junction left in the worktree
#[test]
fn a_junction_the_reviewer_left_is_unlinked_and_its_outside_target_survives() {
    // (F02-21, F03-2, F04-5) the reviewer leaves a `node_modules` junction in its throwaway
    // worktree pointing at a directory OUTSIDE it; the guard's removal unlinks the junction without
    // following it, removes the worktree, and the outside sentinel keeps its bytes.
    let e = setup("junction");
    let outside = e.work.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("sentinel.txt"), "keep me\n").unwrap();
    let o = e.consult(
        &["--engine", "kimi", "--model", "kimi-code/k3"],
        &[
            ("FAKE_KIMI_TOOLCALL", "1"),
            ("FAKE_KIMI_JUNCTION", &outside.to_string_lossy()),
        ],
    );
    let t = text(&o);
    let turns = e.turns();
    assert_eq!(turns.len(), 1, "{turns:?}");
    let wt = turns[0]["cwd"].as_str().unwrap_or("");
    assert!(wt.contains("c3-kimi-wt"), "{wt}");
    assert!(
        std::fs::symlink_metadata(wt).is_err(),
        "the throwaway worktree was removed: {wt}\n{t}"
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("sentinel.txt")).unwrap(),
        "keep me\n",
        "the junction's outside target survived"
    );
    assert!(!t.contains("was left in place"), "{t}");
    // (B1, item 4) the junction IS a write of the reviewer in its own worktree: the check that now
    // watches that worktree fails the kimi run for it (it never saw such a write before).
    let last = e.last();
    let outcome = last["bridge_outcome"].as_str().unwrap_or("");
    assert!(
        outcome.starts_with("failed: the reviewer's throwaway worktree changed during the run")
            && outcome.contains("node_modules"),
        "{outcome}\n{t}"
    );
    assert_eq!(last["provider_failure"]["class"], "permission", "{last}");
}

// ------------------------------------------------------- (B1) --rev, the worktree drift check

#[test]
fn rev_runs_kimi_in_a_worktree_of_that_revision_with_the_untracked_brief() {
    // (B1, item 1) `--rev <older commit>`: the reviewer's cwd checks out exactly that sha, the
    // untracked brief is staged there, the prompt, the handoff header and the ledger name the full
    // sha, and the worktree is gone after the run.
    let e = setup("rev");
    let first = git_out(&e.repo, &["rev-parse", "HEAD"]);
    std::fs::write(e.repo.join("app.txt"), "two\n").unwrap();
    git(&e.repo, &["commit", "-q", "-am", "second"]);
    let head = git_out(&e.repo, &["rev-parse", "HEAD"]);
    assert_ne!(first, head);
    let brief_rel = ".collab/t/handoffs/01-claude-framing.md";
    std::fs::write(e.repo.join(brief_rel), "# Brief\n\n1. Is app.txt fine?\n").unwrap();
    let short = &first[..10];
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--brief",
            brief_rel,
            "--purpose",
            "framing",
            "--rev",
            short,
        ],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let turns = e.turns();
    assert_eq!(turns.len(), 1, "{turns:?}");
    assert_eq!(
        turns[0]["head"],
        first.as_str(),
        "the cwd checks out --rev: {:?}",
        turns[0]
    );
    assert_eq!(turns[0]["git_file"], true);
    assert_eq!(turns[0]["op_brief_exists"], true, "{:?}", turns[0]);
    let ls: Vec<&str> = turns[0]["ls"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert!(ls.contains(&"app.txt") && ls.contains(&".collab"), "{ls:?}");
    let body = turns[0]["prompt_body"].as_str().unwrap_or("");
    assert!(
        body.contains(&format!(
            "Isolation: your working directory is a throwaway git worktree of revision {first} (`--rev {short}`)"
        )),
        "{body}"
    );
    let wt = turns[0]["cwd"].as_str().unwrap_or("");
    assert!(wt.contains("c3-kimi-wt"), "{wt}");
    assert!(std::fs::symlink_metadata(wt).is_err(), "removed: {wt}");
    let last = e.last();
    assert!(
        last["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("usable"),
        "{last}"
    );
    assert_eq!(last["worktree"]["reason"], "rev", "{last}");
    assert_eq!(last["worktree"]["rev"], short);
    assert_eq!(last["worktree"]["sha"], first.as_str());
    let handoff = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("02-kimi-reply.md"),
    )
    .unwrap_or_default();
    assert!(
        handoff.contains(&format!("of revision {first} (--rev {short};")),
        "the handoff header names the reviewed sha: {handoff}"
    );

    // a resume under --rev of the thread recorded on HEAD (another revision): warned before the run
    let o2 = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--purpose",
            "framing",
            "--mode",
            "resume",
            "--rev",
            &head,
        ],
        &[],
    );
    let t2 = text(&o2);
    assert!(
        t2.contains(&format!(
            "WARNING: -Rev {head}: the resume thread {}",
            last["thread"].as_str().unwrap_or("?")
        )) && t2.contains(&format!("was recorded on revision {}", &first[..12])),
        "{t2}"
    );

    // a revision git does not know is refused before anything starts
    let o3 = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--rev",
            "no-such-rev",
        ],
        &[],
    );
    let t3 = text(&o3);
    assert_eq!(o3.status.code(), Some(1), "{t3}");
    assert!(
        t3.contains("-Rev 'no-such-rev' is not a commit git knows"),
        "{t3}"
    );
    assert!(t3.contains("nothing was started"), "{t3}");
}

#[test]
fn a_live_tree_change_during_a_kimi_run_is_a_warning_and_its_own_write_a_failure() {
    // (B1, item 4) a parallel session edits the LIVE checkout while kimi runs in its worktree: the
    // run stays usable and says so; kimi's own write in its worktree still fails the run.
    let e = setup("live-touch");
    let o = e.consult(
        &["--engine", "kimi", "--model", "kimi-code/k3"],
        &[
            ("FAKE_KIMI_TOOLCALL", "1"),
            ("FAKE_KIMI_TOUCH", &e.repo.join("app.txt").to_string_lossy()),
        ],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let last = e.last();
    assert!(
        last["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("usable"),
        "a live-tree change no longer fails a worktree reviewer: {last}"
    );
    assert_eq!(last["tree_check"]["outcome"], "warned", "{last}");
    let warnings = last["warnings"].to_string();
    assert!(
        warnings.contains("the main working tree changed during the run")
            && warnings.contains("the reviewer ran in a throwaway worktree"),
        "{warnings}"
    );

    let e2 = setup("own-write");
    let o2 = e2.consult(
        &["--engine", "kimi", "--model", "kimi-code/k3"],
        &[
            ("FAKE_KIMI_TOOLCALL", "1"),
            ("FAKE_KIMI_WRITE", "scratch.txt"),
        ],
    );
    let t2 = text(&o2);
    let last2 = e2.last();
    let outcome = last2["bridge_outcome"].as_str().unwrap_or("");
    assert!(
        outcome.starts_with(
            "failed: the reviewer's throwaway worktree changed during the run: 1 file: scratch.txt"
        ),
        "{outcome}\n{t2}"
    );
    assert_eq!(last2["provider_failure"]["class"], "permission", "{last2}");
    assert!(
        !e2.repo.join("scratch.txt").exists(),
        "the write never reached the live checkout"
    );
}

// ------------------------------------------------------------------ (R7) an uncommitted brief reaches kimi in its worktree

#[test]
fn an_uncommitted_brief_is_readable_by_kimi_in_its_worktree() {
    // (R7) kimi reviews a throwaway worktree of HEAD; a brief that exists only in the caller's
    // working tree (uncommitted) must still reach the reviewer. The operator brief named in the
    // assembled prompt (by a repo-relative path) must resolve from the worktree cwd - the engine
    // stages it in. Before the fix the uncommitted brief was absent from the HEAD worktree.
    let e = setup("uncommitted-brief");
    // an UNCOMMITTED brief inside the repo (never git-added); `.collab/t/handoffs` exists from setup.
    std::fs::write(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("brief.md"),
        "# Brief\n\n1. Is app.txt fine?\n",
    )
    .unwrap();
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--brief",
            ".collab/t/handoffs/brief.md",
            "--purpose",
            "framing",
        ],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let turns = e.turns();
    assert_eq!(turns.len(), 1, "{:?}", turns);
    assert_eq!(
        turns[0]["op_brief_exists"], true,
        "the uncommitted operator brief is readable from the worktree: {:?}",
        turns[0]
    );
}

// ------------------------------------------------------------------ the zero-tool-call guard

#[test]
fn a_reviewing_purpose_with_zero_tool_calls_fails_but_a_framing_one_is_usable() {
    // The fake kimi emits no tool-call line by default. On a reviewing purpose (acceptance) a reply
    // that read no file cannot have checked anything: the run is a capability failure. The SAME
    // stream on a framing purpose is usable (the brief carries the question; no tools needed).
    let e = setup("guard-acceptance");
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--purpose",
            "acceptance",
        ],
        &[],
    );
    let t = text(&o);
    let last = e.last();
    let outcome = last["bridge_outcome"].as_str().unwrap_or("");
    assert!(
        outcome.starts_with("failed: kimi answered without reading any file (0 tool calls)")
            && outcome.contains("a acceptance review must read the code; ask again"),
        "acceptance/0-tools must fail the guard: {outcome}\n{t}"
    );
    assert_eq!(
        last["provider_failure"]["class"], "capability",
        "the guard uses the capability class (no provider outage): {last}"
    );

    let e2 = setup("guard-framing");
    let o2 = e2.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--purpose",
            "framing",
        ],
        &[],
    );
    let t2 = text(&o2);
    assert_eq!(o2.status.code(), Some(0), "{t2}");
    assert!(
        e2.last()["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("usable"),
        "framing/0-tools is usable: {}",
        e2.last()
    );
}

// --------------------------------------------------- the read-insist re-ask (zero tool calls)

#[test]
fn read_insist_rescues_a_zero_tool_call_acceptance_reply() {
    // The primary acceptance turn answers with 0 tool calls (the guard fires); the read-insist
    // re-ask resumes the SAME session and reads 2 files, so its reply becomes the primary reply.
    let e = setup("insist-rescue");
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--purpose",
            "acceptance",
        ],
        &[("FAKE_KIMI_RESUME_TOOLCALL", "2")],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let last = e.last();
    assert!(
        last["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("usable"),
        "the re-ask rescued the run: {last}"
    );
    assert_eq!(last["structured"], true);
    assert_eq!(last["read_insist"]["attempts"], 1, "one re-ask: {last}");
    assert_eq!(
        last["read_insist"]["final_tool_calls"], 2,
        "it read 2 files: {last}"
    );
    let turns = e.turns();
    assert_eq!(turns.len(), 2, "primary + one re-ask: {:?}", turns);
    assert!(turns[0]["resume"].as_str().unwrap_or("").is_empty());
    assert!(
        is_session(turns[1]["resume"].as_str().unwrap_or("")),
        "the re-ask resumes the session: {:?}",
        turns[1]
    );
    let handoff = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("01-kimi-reply.md"),
    )
    .unwrap_or_default();
    assert!(
        handoff.contains("Read insist: 1 turn(s), tool calls 0 -> 2"),
        "the handoff records the read-insist line: {handoff}"
    );
}

#[test]
fn read_insist_fails_after_two_re_asks_when_the_model_keeps_reading_nothing() {
    // The primary and both re-asks all read 0 files: after the last the guard failure stands with
    // " after 2 re-asks".
    let e = setup("insist-stubborn");
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--purpose",
            "acceptance",
        ],
        &[],
    );
    let t = text(&o);
    let last = e.last();
    let outcome = last["bridge_outcome"].as_str().unwrap_or("");
    assert!(
        outcome.contains("0 tool calls") && outcome.contains("after 2 re-asks"),
        "a stubborn reviewer fails after 2 re-asks: {outcome}\n{t}"
    );
    assert_eq!(last["provider_failure"]["class"], "capability");
    assert_eq!(last["read_insist"]["attempts"], 2, "two re-asks: {last}");
    assert_eq!(last["read_insist"]["final_tool_calls"], 0);
    assert_eq!(e.turns().len(), 3, "primary + 2 re-asks: {:?}", e.turns());
}

// ----------------------------- (F11-3) the uncommitted brief reaches the read-insist worktree too

#[test]
fn the_uncommitted_brief_reaches_the_read_insist_worktree_too() {
    // (F11-3) An acceptance consultation with an UNCOMMITTED operator brief: the primary makes zero
    // tool calls (the guard fires -> a read-insist re-ask), and the re-ask runs in a FRESH HEAD
    // worktree. The operator's uncommitted brief must be staged into BOTH worktrees, so the reviewer
    // reads the operator's actual (uncommitted) content in both turns. Before the fix the secondary
    // turn staged nothing, so the brief was absent/stale in the re-ask worktree.
    let e = setup("insist-brief");
    let brief_rel = ".collab/t/handoffs/brief.md";
    let brief_body = "# Brief\n\n1. Is app.txt fine?\nMARKER-UNCOMMITTED-F113\n";
    // UNCOMMITTED (never git-added); `.collab/t/handoffs` exists from setup.
    std::fs::write(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("brief.md"),
        brief_body,
    )
    .unwrap();
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--brief",
            brief_rel,
            "--purpose",
            "acceptance",
        ],
        &[
            // the primary reads nothing (guard fires); the re-ask reads 2 files and rescues the run.
            ("FAKE_KIMI_RESUME_TOOLCALL", "2"),
            // the fake resolves this repo-relative brief against its cwd (the worktree) on every turn.
            ("FAKE_KIMI_OP_BRIEF", brief_rel),
        ],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let turns = e.turns();
    assert_eq!(
        turns.len(),
        2,
        "primary + one read-insist re-ask: {:?}",
        turns
    );
    for (i, turn) in turns.iter().enumerate() {
        assert_eq!(
            turn["op_brief_exists"], true,
            "turn {i} worktree holds the operator brief: {:?}",
            turn
        );
        assert_eq!(
            turn["op_brief_body"].as_str().unwrap_or(""),
            brief_body,
            "turn {i} sees the operator's uncommitted brief contents: {:?}",
            turn
        );
    }
}

// ------------------------------------------------------------------ (A1) an untracked brief: staged, named, or refused

#[test]
fn an_untracked_brief_is_staged_and_the_prompt_names_the_uncommitted_paths() {
    // (A1, xelixir panel 47cf7308) a NEW task's brief is untracked: kimi reviews a throwaway
    // worktree of HEAD, so the bridge stages the brief there (R7) AND tells the reviewer which live
    // paths were uncommitted at launch - not only the handoff header. (F11-5) Only tracked paths
    // are named; an untracked one is counted, its name never reaches the reviewer.
    let e = setup("untracked-named");
    let brief_rel = ".collab/t/handoffs/01-claude-framing.md";
    std::fs::write(e.repo.join(brief_rel), "# Brief\n\n1. Is app.txt fine?\n").unwrap();
    // an uncommitted source change too: the reviewer's worktree holds the committed version.
    std::fs::write(e.repo.join("app.txt"), "two\n").unwrap();
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--brief",
            brief_rel,
            "--purpose",
            "framing",
        ],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let turns = e.turns();
    assert_eq!(turns.len(), 1, "{:?}", turns);
    assert_eq!(
        turns[0]["op_brief_exists"], true,
        "the untracked brief is staged into the worktree: {:?}",
        turns[0]
    );
    let body = turns[0]["prompt_body"].as_str().unwrap_or("");
    assert!(
        body.contains(
            "Isolation: your working directory is a throwaway git worktree of the committed HEAD"
        ),
        "the prompt carries the isolation paragraph: {body}"
    );
    // (F11-5) the modified TRACKED path is named; the untracked `.collab/` only counted.
    assert!(
        body.contains("1 tracked path(s) had uncommitted changes") && body.contains(": `app.txt`."),
        "the tracked change is named: {body}"
    );
    assert!(
        body.contains("1 path(s) that are not in HEAD") && !body.contains("`.collab/`"),
        "an untracked path is only counted: {body}"
    );
    assert!(
        body.contains(&format!(
            "The brief `{brief_rel}` was copied into your worktree at that path."
        )),
        "{body}"
    );
    let last = e.last();
    assert!(
        last["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("usable"),
        "{last}"
    );
}

#[test]
fn an_untracked_brief_outside_the_repository_refuses_the_seat_before_launch() {
    // (A1) a brief that is neither committed at HEAD nor stageable (a relative path that leaves the
    // repository) cannot be read in the throwaway worktree: the seat is refused before the child
    // starts, with the preflight-class wording a panel re-seats on - no paid seat answers "the brief
    // is missing".
    let e = setup("untracked-outside");
    std::fs::write(e.work.join("outside-brief.md"), "# Brief\n\n1. Anything?\n").unwrap();
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--brief",
            "../outside-brief.md",
            "--purpose",
            "framing",
        ],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(1), "{t}");
    assert!(
        t.contains("the kimi seat is not usable: the brief `../outside-brief.md` is neither committed at HEAD nor stageable"),
        "{t}"
    );
    assert!(t.contains("nothing was started"), "{t}");
    assert!(
        e.turns().is_empty(),
        "no kimi turn was started: {:?}",
        e.turns()
    );
    assert!(e.ledger().is_empty(), "nothing reached the ledger");
}

// ------------------------------------------- (F11-3, F11-4) staging never reaches outside the worktree

/// Commit a LINK entry (mode 120000, through the index - no privilege needed) at `rel` pointing at
/// `target`, and leave a REGULAR file with `live` at `rel` in the live tree.
fn commit_link_entry(repo: &Path, rel: &str, target: &Path, live: &str) {
    use std::io::Write;
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["hash-object", "-w", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("git on PATH");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(target.to_string_lossy().replace('\\', "/").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let blob = String::from_utf8_lossy(&out.stdout).trim().to_string();
    git(
        repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("120000,{blob},{rel}"),
        ],
    );
    git(repo, &["commit", "-q", "-m", "a link at the brief's path"]);
    std::fs::write(repo.join(rel), live).unwrap();
}

#[test]
fn a_committed_link_at_the_brief_path_refuses_the_kimi_seat_and_the_sentinel_survives() {
    // (F11-3, the reviewer's RC1) HEAD carries a LINK at the brief's path to a sentinel outside the
    // repository, the live tree a regular brief there: the seat is refused before anything starts
    // (preflight-class wording, so a panel re-seats it), no kimi turn starts, nothing reaches the
    // ledger, and the sentinel keeps its bytes.
    let e = setup("rc1-link");
    let sentinel = e.work.join("sentinel.txt");
    std::fs::write(&sentinel, "keep me\n").unwrap();
    commit_link_entry(
        &e.repo,
        "brief.md",
        &sentinel,
        "# Brief\n\n1. Is app.txt fine?\n",
    );
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--brief",
            "brief.md",
            "--purpose",
            "framing",
        ],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(1), "{t}");
    assert!(t.contains("the kimi seat is not usable:"), "{t}");
    assert!(
        t.contains("`brief.md` is a link in the checked-out revision"),
        "{t}"
    );
    assert!(t.contains("nothing was started"), "{t}");
    assert!(e.turns().is_empty(), "no kimi turn: {:?}", e.turns());
    assert!(e.ledger().is_empty(), "nothing reached the ledger");
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "keep me\n");
}

#[test]
fn a_junction_on_the_briefs_way_in_the_checkout_refuses_the_seat_before_launch() {
    // (F11-3, F11-4) the checkout of the reviewer's worktree carries a JUNCTION where the brief's
    // directory is (planted by a post-checkout hook - what a checked-out link leaves on disk): the
    // staging refuses it - a typed pre-start refusal with preflight-class wording, no kimi turn,
    // the reservation withdrawn, nothing written through the junction.
    let e = setup("hook-junction");
    let outside = e.work.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("brief.md"), "outside brief\n").unwrap();
    std::fs::create_dir_all(e.repo.join("docs")).unwrap();
    std::fs::write(e.repo.join("docs").join("brief.md"), "# Brief\n\n1. Ok?\n").unwrap();
    let hooks = e.work.join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(
        hooks.join("post-checkout"),
        format!(
            "#!/bin/sh\ncmd //c \"mklink /J docs {}\" >/dev/null 2>&1\nexit 0\n",
            outside.display()
        ),
    )
    .unwrap();
    git(
        &e.repo,
        &["config", "core.hooksPath", &hooks.to_string_lossy()],
    );
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--brief",
            "docs/brief.md",
            "--purpose",
            "framing",
        ],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(1), "{t}");
    assert!(
        t.contains("the kimi seat is not usable: the brief `docs/brief.md` cannot be staged into the reviewer's throwaway worktree of HEAD"),
        "{t}"
    );
    assert!(t.contains("is a link in the worktree"), "{t}");
    assert!(e.turns().is_empty(), "no kimi turn: {:?}", e.turns());
    assert!(e.ledger().is_empty(), "nothing reached the ledger");
    assert!(
        !e.repo
            .join(".collab")
            .join("t")
            .join(".consult.pending.json")
            .exists(),
        "the reservation was withdrawn"
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("brief.md")).unwrap(),
        "outside brief\n"
    );
}

#[test]
fn an_in_repo_brief_path_with_dot_dot_is_normalised_and_staged() {
    // (F11-4) `docs/sub/../brief.md` names an in-repo file: it is normalised, staged, and the
    // prompt names `docs/brief.md` - not refused as leaving the repository.
    let e = setup("dotdot");
    std::fs::create_dir_all(e.repo.join("docs").join("sub")).unwrap();
    std::fs::write(e.repo.join("docs").join("brief.md"), "# Brief\n\n1. Ok?\n").unwrap();
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--brief",
            "docs/sub/../brief.md",
            "--purpose",
            "framing",
        ],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let turns = e.turns();
    assert_eq!(turns.len(), 1, "{turns:?}");
    assert_eq!(turns[0]["op_brief"], "docs/brief.md", "{:?}", turns[0]);
    assert_eq!(turns[0]["op_brief_exists"], true, "{:?}", turns[0]);
}

#[test]
fn a_committed_brief_the_live_tree_cannot_read_is_materialised_from_head() {
    // (F11-4) the brief is committed at HEAD but the live file cannot be read (held open without
    // sharing): the seat is not refused and the reviewer finds the committed brief in its worktree.
    use std::os::windows::fs::OpenOptionsExt;
    let e = setup("locked-brief");
    std::fs::write(e.repo.join("brief.md"), "# Committed brief\n").unwrap();
    git(&e.repo, &["add", "-A"]);
    git(&e.repo, &["commit", "-q", "-m", "brief"]);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(e.repo.join("brief.md"))
        .unwrap();
    assert!(
        std::fs::read(e.repo.join("brief.md")).is_err(),
        "the lock holds"
    );
    let o = e.consult(
        &[
            "--engine",
            "kimi",
            "--model",
            "kimi-code/k3",
            "--brief",
            "brief.md",
            "--purpose",
            "framing",
        ],
        &[],
    );
    drop(lock);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let turns = e.turns();
    assert_eq!(turns.len(), 1, "{turns:?}");
    assert_eq!(turns[0]["op_brief_exists"], true, "{:?}", turns[0]);
    assert_eq!(
        turns[0]["op_brief_body"]
            .as_str()
            .unwrap_or("")
            .replace("\r\n", "\n"),
        "# Committed brief\n"
    );
}

// ------------------------------------------------------------------ the resume path

#[test]
fn a_prose_first_reply_is_repaired_on_a_resume_turn() {
    let e = setup("resume");
    // first turn: prose (no JSON); the resume (format-repair) turn: the structured object.
    let o = e.consult(
        &["--engine", "kimi", "--model", "kimi-code/k3"],
        &[
            ("FAKE_KIMI_REPLY", &e.prose.to_string_lossy()),
            ("FAKE_KIMI_RESUME_REPLY", &e.reply.to_string_lossy()),
        ],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let turns = e.turns();
    assert_eq!(
        turns.len(),
        2,
        "a first turn and a format-repair resume: {:?}",
        turns
    );
    // the first turn has no -S; the second resumes the session the first minted.
    assert!(turns[0]["resume"].as_str().unwrap_or("").is_empty());
    let resumed = turns[1]["resume"].as_str().unwrap_or("");
    assert!(
        is_session(resumed),
        "the second turn resumes a kimi session: {resumed}"
    );
    assert_eq!(
        e.last()["structured"],
        true,
        "the repaired reply is structured"
    );
}
