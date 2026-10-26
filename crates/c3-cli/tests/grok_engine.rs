//! The C3 `grok` engine end to end through the real `c3` binary against a FAKE grok CLI -
//! `tests/fixtures/fake-grok.{cmd,ps1}` (driven by `FAKE_GROK_*`): the roster entry validates and
//! shows in `c3 providers`, the dry run's plan and read-only argv, a structured run with its ledger
//! evidence and token usage, the resume path (a prose first reply repaired on a `--resume` turn),
//! and the BILLING GUARD (the xAI API key is stripped from the child unless `C3_GROK_ALLOW_API_KEY=1`).
//!
//! GUARD: every child gets a scratch USERPROFILE/HOME and a PATH without any directory that holds a
//! grok launcher, so the real CLI can never start; the fake logs every start. Nothing reaches a
//! real provider. Windows only (the fake is a `.cmd` wrapper around a PowerShell script).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const FAKE_CMD: &str = include_str!("fixtures/fake-grok.cmd");
const FAKE_PS1: &str = include_str!("fixtures/fake-grok.ps1");
const ADVISE: &str = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
const UUID_RE: &str = r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$";

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

/// A PATH without any directory that holds a grok launcher (GUARD).
fn safe_path() -> String {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(';')
        .filter(|d| {
            !d.is_empty()
                && !["grok", "grok.exe", "grok.cmd"]
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
    let work = std::env::temp_dir().join(format!("c3-grok-{tag}-{}-{nanos}", std::process::id()));
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
    // the scratch USERPROFILE/HOME, with a grok oauth sign-in file (its contents never read).
    let uh = work.join("userhome");
    std::fs::create_dir_all(uh.join("AppData").join("Local")).unwrap();
    std::fs::create_dir_all(uh.join(".grok")).unwrap();
    std::fs::write(uh.join(".grok").join("auth.json"), "{\"oauth\":\"x\"}\n").unwrap();
    let fake = work.join("fake-grok.cmd");
    std::fs::write(&fake, FAKE_CMD.replace("\r\n", "\n").replace('\n', "\r\n")).unwrap();
    std::fs::write(work.join("fake-grok.ps1"), FAKE_PS1).unwrap();
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
            if k.starts_with("CODEX_CONSULT_")
                || k.starts_with("FAKE_")
                || k.starts_with("C3_")
                || k.starts_with("XAI_")
                || k.starts_with("GROK_")
            {
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
            .env("CODEX_CONSULT_GROK_EXE", &self.fake)
            .env("CODEX_CONSULT_TEST_MODE", "1")
            .env("CODEX_CONSULT_ROSTER", "none")
            .env("CODEX_CONSULT_HEALTH", "none")
            .env("CODEX_CONSULT_TELEMETRY", "off")
            .env("C3_TELEMETRY_HUB", "http://127.0.0.1:9/T")
            .env("C3_PRIORS", "off")
            .env("FAKE_GROK_ARGV_LOG", &self.log)
            .env("FAKE_GROK_REPLY", &self.reply);
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

    fn starts(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect()
    }

    fn turns(&self) -> Vec<Value> {
        self.starts()
            .into_iter()
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

fn has_name(rec: &Value, name: &str) -> bool {
    rec["env"]
        .as_array()
        .map(|a| {
            a.iter()
                .any(|v| v.as_str().is_some_and(|s| s.eq_ignore_ascii_case(name)))
        })
        .unwrap_or(false)
}

fn is_uuid(s: &str) -> bool {
    regex::Regex::new(UUID_RE).unwrap().is_match(s)
}

// ------------------------------------------------------------------ roster + providers

#[test]
fn the_grok_roster_entry_validates_and_shows_in_providers() {
    let e = setup("providers");
    let roster = e.roster(
        r#"{"roster_version":1,"reviewers":[{"provider":"grok","engine":"grok","model":"grok-4.7","plan":"supergrok","panel":"weighty"}]}"#,
    );
    let o = e.c3(&["providers"], &[("CODEX_CONSULT_ROSTER", &roster)]);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(t.contains("grok-4.7"), "the roster model shows: {t}");
    assert!(t.contains("[grok]") || t.contains("engine grok"), "{t}");
    // the subscription route is reported (no xAI API key opted in)
    assert!(t.contains("subscription (grok.com oauth)"), "{t}");
}

// ------------------------------------------------------------------ dry run

#[test]
fn a_dry_run_plans_the_read_only_grok_turn() {
    let e = setup("dry");
    let o = e.consult(
        &["--dry-run", "--engine", "grok", "--model", "grok-4.7"],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(t.contains("grok-cli 1.0.70-fake"), "harness: {t}");
    assert!(
        t.contains("signed in (~/.grok/auth.json)"),
        "preflight: {t}"
    );
    let cmd_line = t
        .lines()
        .find(|l| l.trim_start().starts_with("command"))
        .unwrap_or("")
        .to_string();
    assert!(
        cmd_line.contains("--output-format streaming-json"),
        "{cmd_line}"
    );
    assert!(
        cmd_line.contains("--tools read_file,list_dir,grep"),
        "{cmd_line}"
    );
    assert!(
        cmd_line.contains("--disallowed-tools use_tool,search_tool,write,run_terminal_command"),
        "{cmd_line}"
    );
    assert!(cmd_line.contains("--permission-mode dontAsk"), "{cmd_line}");
    assert!(cmd_line.contains("--no-subagents"), "{cmd_line}");
    assert!(cmd_line.contains("-m grok-4.7"), "{cmd_line}");
    // a dry run starts no turn.
    assert!(e.turns().is_empty(), "no turn in a dry run");
}

// ------------------------------------------------------------------ a structured run

#[test]
fn a_structured_run_is_usable_with_token_usage_and_a_subscription_prompt_note() {
    let e = setup("run");
    let o = e.consult(&["--engine", "grok", "--model", "grok-4.7"], &[]);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    // the engine-turns note is written into the handoff record (SuperGrok subscription prompt).
    let handoff = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("01-grok-reply.md"),
    )
    .unwrap_or_default();
    assert!(
        handoff.contains("each one a SuperGrok subscription prompt"),
        "handoff engine-turns note: {handoff}"
    );
    let last = e.last();
    assert!(
        last["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("usable"),
        "outcome: {last}"
    );
    assert_eq!(last["reviewer"]["engine"], "grok");
    assert_eq!(last["structured"], true);
    // the end event's usage reached the ledger.
    assert_eq!(last["usage"]["input_tokens"], 5820);
    assert_eq!(last["usage"]["output_tokens"], 83);
    assert_eq!(last["usage"]["total_tokens"], 9743);
    // exactly one grok turn, with the read-only argv and a session minted by the fake.
    let turns = e.turns();
    assert_eq!(turns.len(), 1, "{:?}", turns);
}

// ------------------------------------------------------------------ the billing guard

#[test]
fn the_billing_guard_strips_the_xai_api_key_from_the_child_unless_opted_in() {
    // default: the key is set in the bridge's environment but stripped from the child.
    let e = setup("guard-strip");
    let o = e.consult(
        &["--engine", "grok", "--model", "grok-4.7"],
        &[("XAI_API_KEY", "xai-SECRET")],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let turns = e.turns();
    assert_eq!(turns.len(), 1);
    assert!(
        !has_name(&turns[0], "XAI_API_KEY"),
        "the xAI API key must be stripped from the child: {:?}",
        turns[0]["env"]
    );

    // opted in: the key reaches the child (the operator chose the API-key route).
    let e2 = setup("guard-allow");
    let o2 = e2.consult(
        &["--engine", "grok", "--model", "grok-4.7"],
        &[
            ("XAI_API_KEY", "xai-SECRET"),
            ("C3_GROK_ALLOW_API_KEY", "1"),
        ],
    );
    assert_eq!(o2.status.code(), Some(0), "{}", text(&o2));
    let turns2 = e2.turns();
    assert_eq!(turns2.len(), 1);
    assert!(
        has_name(&turns2[0], "XAI_API_KEY"),
        "with the opt-in the key reaches the child: {:?}",
        turns2[0]["env"]
    );
}

// ------------------------------------------------------------------ the resume path

#[test]
fn a_prose_first_reply_is_repaired_on_a_resume_turn() {
    let e = setup("resume");
    // first turn: prose (no JSON); the resume (format-repair) turn: the structured object.
    let o = e.consult(
        &["--engine", "grok", "--model", "grok-4.7"],
        &[
            ("FAKE_GROK_REPLY", &e.prose.to_string_lossy()),
            ("FAKE_GROK_RESUME_REPLY", &e.reply.to_string_lossy()),
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
    // the first turn has no --resume; the second resumes the session the first minted.
    assert!(turns[0]["resume"].as_str().unwrap_or("").is_empty());
    let resumed = turns[1]["resume"].as_str().unwrap_or("");
    assert!(
        is_uuid(resumed),
        "the second turn resumes a uuid session: {resumed}"
    );
    assert_eq!(
        e.last()["structured"],
        true,
        "the repaired reply is structured"
    );
}
