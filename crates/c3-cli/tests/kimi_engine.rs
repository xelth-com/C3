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
        uh.join(".kimi-code").join("credentials").join("oauth-token"),
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
    assert!(
        cmd_line.contains("Read it completely first"),
        "{cmd_line}"
    );
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
        turns[0]["cwd"].as_str().unwrap_or("").contains("c3-kimi-wt"),
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

// ------------------------------------------------------------------ the zero-tool-call guard

#[test]
fn a_reviewing_purpose_with_zero_tool_calls_fails_but_a_framing_one_is_usable() {
    // The fake kimi emits no tool-call line by default. On a reviewing purpose (acceptance) a reply
    // that read no file cannot have checked anything: the run is a capability failure. The SAME
    // stream on a framing purpose is usable (the brief carries the question; no tools needed).
    let e = setup("guard-acceptance");
    let o = e.consult(
        &[
            "--engine", "kimi", "--model", "kimi-code/k3", "--purpose", "acceptance",
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
            "--engine", "kimi", "--model", "kimi-code/k3", "--purpose", "framing",
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
            "--engine", "kimi", "--model", "kimi-code/k3", "--purpose", "acceptance",
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
    assert_eq!(last["read_insist"]["final_tool_calls"], 2, "it read 2 files: {last}");
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
            "--engine", "kimi", "--model", "kimi-code/k3", "--purpose", "acceptance",
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
