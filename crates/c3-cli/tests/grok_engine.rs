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
    // The default purpose is non-review, so a 0-tool-call reply fails the guard; a compliant run
    // opens the brief, so the fake makes one tool call and the single turn stays usable.
    let o = e.consult(
        &["--engine", "grok", "--model", "grok-4.7"],
        &[("FAKE_GROK_TOOL_CALLS", "1")],
    );
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
    // (The default purpose is non-review; a compliant run opens the brief, so the fake makes one
    // tool call and the single turn passes the guard.)
    let e = setup("guard-strip");
    let o = e.consult(
        &["--engine", "grok", "--model", "grok-4.7"],
        &[("XAI_API_KEY", "xai-SECRET"), ("FAKE_GROK_TOOL_CALLS", "1")],
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
            ("FAKE_GROK_TOOL_CALLS", "1"),
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

// --------------------------------------------------- the zero-tool-call review guard

#[test]
fn a_zero_tool_call_reply_fails_the_guard_but_a_compliant_run_is_usable() {
    // On a reviewing purpose (acceptance) a reply that read no file cannot have checked anything: a
    // capability failure (disable the re-ask so the first-turn guard failure is observed directly).
    let e = setup("guard-acceptance");
    let o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "acceptance",
        ],
        &[("CODEX_CONSULT_READ_INSIST", "0")],
    );
    let t = text(&o);
    let last = e.last();
    let outcome = last["bridge_outcome"].as_str().unwrap_or("");
    assert!(
        outcome.starts_with(
            "failed: grok answered in one model call without reading any file (0 tool calls)"
        ) && outcome.contains("a acceptance review must read the code; ask again"),
        "acceptance/0-tools must fail the guard: {outcome}\n{t}"
    );
    assert_eq!(
        last["provider_failure"]["class"], "capability",
        "the guard uses the capability class (no provider outage): {last}"
    );
    // the reply is kept for the record even though the run failed.
    assert!(!e.turns().is_empty(), "the primary turn ran");

    // A compliant framing run opens the brief (one tool call): usable, a single turn.
    let e2 = setup("guard-framing-ok");
    let o2 = e2.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "framing",
        ],
        &[("FAKE_GROK_TOOL_CALLS", "1")],
    );
    let t2 = text(&o2);
    assert_eq!(o2.status.code(), Some(0), "{t2}");
    assert!(
        e2.last()["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("usable"),
        "a framing run that reads is usable: {}",
        e2.last()
    );
    assert_eq!(
        e2.turns().len(),
        1,
        "a single compliant turn: {:?}",
        e2.turns()
    );
}

// --------------------------------------------------- the read-insist re-ask (zero tool calls)

#[test]
fn read_insist_rescues_a_zero_tool_call_acceptance_reply() {
    // The primary acceptance turn answers with 0 tool calls (the guard fires); the grok read-insist
    // re-ask starts a FRESH conversation (no resume) carrying the original prompt, reads 2 files, so
    // its reply becomes the primary reply and the run is usable. The thread recorded is the re-ask's
    // own session, never the abandoned primary one. The handoff header records the insist.
    let e = setup("insist-rescue");
    let o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "acceptance",
        ],
        &[("FAKE_GROK_RESUME_TOOL_CALLS", "2")],
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
    assert_eq!(
        last["read_insist"]["new_conversation"], true,
        "the grok re-ask ran in a fresh conversation: {last}"
    );
    // a primary turn + one insist re-ask, both FRESH (no --resume).
    let turns = e.turns();
    assert_eq!(turns.len(), 2, "primary + one re-ask: {:?}", turns);
    assert!(turns[0]["resume"].as_str().unwrap_or("").is_empty());
    assert!(
        turns[1]["resume"].as_str().unwrap_or("").is_empty(),
        "the re-ask is a NEW conversation, not a resume: {:?}",
        turns[1]
    );
    // the recorded thread is the re-ask's own session, not the abandoned primary's.
    let primary_sid = turns[0]["sid"].as_str().unwrap_or("");
    let reask_sid = turns[1]["sid"].as_str().unwrap_or("");
    assert!(is_uuid(primary_sid) && is_uuid(reask_sid) && primary_sid != reask_sid);
    assert_eq!(
        last["thread"].as_str().unwrap_or(""),
        reask_sid,
        "the consultation records the re-ask's thread: {last}"
    );
    // the re-ask carried the WHOLE original primary prompt (not the short same-thread follow-up)
    // with the insist line in front.
    let reask_prompt = turns[1]["prompt"].as_str().unwrap_or("");
    let primary_prompt = turns[0]["prompt"].as_str().unwrap_or("");
    assert!(
        reask_prompt.contains("This is a NEW conversation")
            && !primary_prompt.is_empty()
            && reask_prompt.contains(primary_prompt),
        "the fresh re-ask carries the whole original prompt plus the insist line: {reask_prompt}"
    );
    let handoff = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("01-grok-reply.md"),
    )
    .unwrap_or_default();
    assert!(
        handoff.contains("Read insist: 1 turn(s) in a fresh conversation each, tool calls 0 -> 2"),
        "the handoff records the read-insist line: {handoff}"
    );
}

#[test]
fn read_insist_fails_after_two_re_asks_when_the_model_keeps_reading_nothing() {
    // The primary and both re-asks all read 0 files: after the last re-ask the guard failure stands
    // with " after 2 re-asks".
    let e = setup("insist-stubborn");
    let o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
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
    assert_eq!(last["read_insist"]["new_conversation"], true);
    // every attempt ran in a throwaway conversation: no thread is recorded (none may be resumed).
    assert_eq!(
        last["thread"].as_str().unwrap_or(""),
        "",
        "no abandoned zero-tool thread is recorded for a resume: {last}"
    );
    // all three turns are fresh (no --resume).
    let turns = e.turns();
    assert!(turns
        .iter()
        .all(|t| t["resume"].as_str().unwrap_or("").is_empty()));
    // primary + 2 insist re-asks = 3 turns.
    assert_eq!(turns.len(), 3, "{:?}", turns);
}

#[test]
fn read_insist_disabled_fails_on_the_first_turn() {
    // CODEX_CONSULT_READ_INSIST=0 restores today's behaviour: the guard fails at once, no re-ask.
    let e = setup("insist-off");
    let _o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "acceptance",
        ],
        &[("CODEX_CONSULT_READ_INSIST", "0")],
    );
    let last = e.last();
    let outcome = last["bridge_outcome"].as_str().unwrap_or("");
    assert!(
        outcome.starts_with(
            "failed: grok answered in one model call without reading any file (0 tool calls)"
        ) && !outcome.contains("re-asks"),
        "env 0 fails on the first turn as today: {outcome}"
    );
    assert_eq!(last["provider_failure"]["class"], "capability");
    assert!(
        last["read_insist"].is_null(),
        "no read_insist record when disabled: {last}"
    );
    assert_eq!(e.turns().len(), 1, "only the primary turn: {:?}", e.turns());
}

#[test]
fn a_guard_failure_run_reports_the_zero_tool_primarys_wall() {
    // (A1) the zero-tool guard turns the primary into a `ProviderFailure`, which carries no wall:
    // the primary's own seconds must still be the run's wall (before: `wall_seconds` 0 for a guard
    // failure with no re-ask, and only the re-asks' seconds with them).
    let e = setup("guard-wall");
    let _o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "acceptance",
        ],
        &[("CODEX_CONSULT_READ_INSIST", "0")],
    );
    let last = e.last();
    assert!(
        last["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .contains("(0 tool calls)"),
        "{last}"
    );
    assert!(
        last["wall_seconds"].as_f64().unwrap_or(0.0) > 0.0,
        "a guard-failure run reports its primary turn's wall: {last}"
    );
}

#[test]
fn a_detached_run_that_died_in_a_read_insist_turn_names_its_stream_in_the_death_record() {
    // (A1) a detached diff-review: the zero-tool primary is re-asked, the re-ask reads and then
    // crashes (no end event, exit 1). The run commits a failure, so the background writes a death
    // record - and since the commit removed the run's pending record (and a re-ask writes none),
    // its events_file / events_mtime / partial_hint now come from the stream the run named itself.
    let e = setup("detach-insist-death");
    let placeholder = e.work.join("placeholder.json");
    std::fs::write(
        &placeholder,
        r#"{"schema_version":"1","verdict":"HOLD","verdict_reason":"PLACEHOLDER","reply_markdown":"PLACEHOLDER","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#,
    )
    .unwrap();
    let interim = e.work.join("interim.txt");
    std::fs::write(&interim, "INTERIM-REVIEW-TEXT before it died").unwrap();
    let fg = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "diff-review",
            "--detach",
        ],
        &[
            ("FAKE_GROK_REPLY", &placeholder.to_string_lossy()),
            ("FAKE_GROK_RESUME_REPLY", &interim.to_string_lossy()),
            ("FAKE_GROK_RESUME_TOOL_CALLS", "2"),
            ("FAKE_GROK_RESUME_NO_END", "1"),
        ],
    );
    assert_eq!(fg.status.code(), Some(0), "{}", text(&fg));
    let task_dir = e.repo.join(".collab").join("t");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    let status: Value = loop {
        let found = std::fs::read_dir(&task_dir).ok().and_then(|rd| {
            rd.flatten().map(|x| x.path()).find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(".consult.detached-") && n.ends_with(".status.json"))
                    .unwrap_or(false)
            })
        });
        if let Some(p) = found {
            let v: Value = std::fs::read_to_string(&p)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(Value::Null);
            if v["state"] == "done" {
                break v;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the detached run never finished"
        );
        std::thread::sleep(std::time::Duration::from_millis(250));
    };
    assert_ne!(status["exit"], 0, "the run failed: {status}");
    let death = &status["death"];
    assert_eq!(death["recorded_by"], "background", "{status}");
    let events_file = death["events_file"].as_str().unwrap_or("");
    assert!(
        events_file.ends_with("read-insist-1.events.jsonl"),
        "the death record names the dead re-ask's stream: {death}"
    );
    assert!(
        !death["events_mtime"].as_str().unwrap_or("").is_empty(),
        "{death}"
    );
    assert!(
        death["partial_hint"]
            .as_str()
            .unwrap_or("")
            .contains("read-insist-1.events.jsonl"),
        "{death}"
    );
}

// ------------------------------ the hard guard on a non-review grok purpose (brief by reference)

#[test]
fn read_insist_rescues_a_zero_tool_framing_reply_with_the_re_asks_answer() {
    // A framing primary answers with 0 tool calls: it cannot have read the brief, so the hard guard
    // fails it. The re-ask reads 2 files, so its (distinct) reply becomes the primary reply and the
    // run is usable; the ledger records one re-ask with 2 tool calls, driven by the brief re-ask text.
    let e = setup("framing-rescue");
    let resume = e.work.join("resume.json");
    std::fs::write(
        &resume,
        r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"reread","reply_markdown":"REASKED-ANSWER after reading the code","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#,
    )
    .unwrap();
    let o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "framing",
        ],
        &[
            ("FAKE_GROK_RESUME_TOOL_CALLS", "2"),
            ("FAKE_GROK_RESUME_REPLY", &resume.to_string_lossy()),
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
        "the re-ask rescued the framing run: {last}"
    );
    assert_eq!(last["structured"], true);
    assert_eq!(last["read_insist"]["attempts"], 1, "one re-ask: {last}");
    assert_eq!(
        last["read_insist"]["final_tool_calls"], 2,
        "the re-ask read 2 files: {last}"
    );
    // a primary framing turn + one re-ask, both FRESH conversations (no --resume).
    let turns = e.turns();
    assert_eq!(turns.len(), 2, "primary + one re-ask: {:?}", turns);
    assert!(turns[0]["resume"].as_str().unwrap_or("").is_empty());
    assert!(
        turns[1]["resume"].as_str().unwrap_or("").is_empty(),
        "the re-ask is a NEW conversation, not a resume: {:?}",
        turns[1]
    );
    // the recorded thread is the re-ask's own session, not the abandoned primary's.
    let reask_sid = turns[1]["sid"].as_str().unwrap_or("");
    assert!(is_uuid(reask_sid) && turns[0]["sid"].as_str().unwrap_or("") != reask_sid);
    assert_eq!(last["thread"].as_str().unwrap_or(""), reask_sid, "{last}");
    // the fresh re-ask carries the WHOLE original primary prompt (brief by reference) with the
    // insist line, not the short same-thread "you cannot have read the brief" follow-up.
    let reask_prompt = turns[1]["prompt"].as_str().unwrap_or("");
    let primary_prompt = turns[0]["prompt"].as_str().unwrap_or("");
    assert!(
        reask_prompt.contains("This is a NEW conversation")
            && !primary_prompt.is_empty()
            && reask_prompt.contains(primary_prompt),
        "the fresh re-ask carries the whole original prompt plus the insist line: {reask_prompt}"
    );
    // the re-ask's reply became the primary reply.
    let handoff = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("01-grok-reply.md"),
    )
    .unwrap_or_default();
    assert!(
        handoff.contains("REASKED-ANSWER after reading the code"),
        "the re-ask reply replaced the primary: {handoff}"
    );
    assert!(
        handoff.contains("Read insist: 1 turn(s) in a fresh conversation each, tool calls 0 -> 2"),
        "the handoff records the re-ask: {handoff}"
    );
}

#[test]
fn read_insist_fails_a_framing_run_after_two_re_asks_when_it_keeps_reading_nothing() {
    // The framing primary and both re-asks all read nothing: after the last re-ask the guard failure
    // (brief wording) stands with " after 2 re-asks".
    let e = setup("framing-stubborn");
    let o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "framing",
        ],
        &[],
    );
    let t = text(&o);
    let last = e.last();
    let outcome = last["bridge_outcome"].as_str().unwrap_or("");
    assert!(
        outcome.starts_with(
            "failed: grok answered in one model call without reading any file (0 tool calls)"
        ) && outcome.contains("it cannot have read the brief")
            && outcome.contains("after 2 re-asks"),
        "a stubborn framing run fails after 2 re-asks: {outcome}\n{t}"
    );
    assert_eq!(last["provider_failure"]["class"], "capability");
    assert_eq!(last["read_insist"]["attempts"], 2, "two re-asks: {last}");
    assert_eq!(last["read_insist"]["final_tool_calls"], 0);
    // primary + 2 re-asks = 3 turns.
    assert_eq!(e.turns().len(), 3, "{:?}", e.turns());
}

#[test]
fn read_insist_disabled_fails_a_framing_run_at_once() {
    // CODEX_CONSULT_READ_INSIST=0: the guard fails the framing run at once, no re-ask.
    let e = setup("framing-off");
    let o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "framing",
        ],
        &[("CODEX_CONSULT_READ_INSIST", "0")],
    );
    let t = text(&o);
    let last = e.last();
    let outcome = last["bridge_outcome"].as_str().unwrap_or("");
    assert!(
        outcome.starts_with(
            "failed: grok answered in one model call without reading any file (0 tool calls)"
        ) && outcome.contains("it cannot have read the brief")
            && !outcome.contains("re-asks"),
        "env 0 fails the framing run at once: {outcome}\n{t}"
    );
    assert_eq!(last["provider_failure"]["class"], "capability");
    assert!(
        last["read_insist"].is_null(),
        "no read_insist record when disabled: {last}"
    );
    assert_eq!(e.turns().len(), 1, "only the primary turn: {:?}", e.turns());
}

#[test]
fn a_raw_framing_run_with_zero_tools_is_usable_in_a_single_turn() {
    // A `raw` consultation has no reply contract and no brief duty: a 0-tool framing reply is usable
    // and never re-asked (the guard exempts raw on a non-review purpose).
    let e = setup("framing-raw");
    let o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "framing",
            "--raw",
        ],
        &[],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    let last = e.last();
    assert!(
        last["bridge_outcome"]
            .as_str()
            .unwrap_or("")
            .starts_with("usable"),
        "a raw framing run is usable with no tools: {last}"
    );
    assert!(
        last["read_insist"].is_null(),
        "no re-ask for a raw run: {last}"
    );
    assert_eq!(e.turns().len(), 1, "a single turn: {:?}", e.turns());
}

// ------------------------------ a read-insist re-ask that runs the review and then dies (Item 4)

#[test]
fn read_insist_death_describes_the_dead_reask_not_the_zero_tool_primary() {
    // The primary diff-review turn answers with 0 tool calls (a schema-shaped PLACEHOLDER); the
    // guard fires. The fresh re-ask reads 2 files, emits interim review text, then CRASHES mid-stream
    // (no end event, exit 1). The run's outcome must describe THAT dead attempt - classified like a
    // primary turn, named as read-insist attempt 1 with its real tool-call count - not attempt 1's
    // "0 tool calls". The member wall is non-zero, the events pointer is the re-ask's stream, and the
    // re-ask's salvage (not the placeholder) is the reply.
    let e = setup("insist-death");
    let placeholder = e.work.join("placeholder.json");
    std::fs::write(
        &placeholder,
        r#"{"schema_version":"1","verdict":"HOLD","verdict_reason":"PLACEHOLDER-starting the read now","reply_markdown":"PLACEHOLDER-reading before answering","findings":[],"prior_findings":[],"unproven":["placeholder must not be ingested"],"first_run_checklist":[]}"#,
    )
    .unwrap();
    let interim = e.work.join("interim.txt");
    std::fs::write(
        &interim,
        "INTERIM-REVIEW-TEXT the real review got this far before it died",
    )
    .unwrap();
    let o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "diff-review",
        ],
        &[
            ("FAKE_GROK_REPLY", &placeholder.to_string_lossy()),
            ("FAKE_GROK_RESUME_REPLY", &interim.to_string_lossy()),
            ("FAKE_GROK_RESUME_TOOL_CALLS", "2"),
            ("FAKE_GROK_RESUME_NO_END", "1"),
        ],
    );
    let t = text(&o);
    let last = e.last();
    let outcome = last["bridge_outcome"].as_str().unwrap_or("");
    // the outcome describes the attempt that ENDED the run, with its real tool-call count.
    assert!(
        outcome.contains("read-insist attempt 1 after a zero-tool primary")
            && outcome.contains("2 tool calls")
            && !outcome.contains("0 tool calls)"),
        "the outcome must name the dead re-ask, not the zero-tool primary: {outcome}\n{t}"
    );
    // it carries the grok exit (4d: why it stopped is answerable).
    assert!(
        outcome.contains("exit 1"),
        "the grok exit is in the outcome: {outcome}"
    );
    // member wall is the total across attempts, never 0.
    assert!(
        last["wall_seconds"].as_f64().unwrap_or(0.0) > 0.0,
        "the member wall must be non-zero: {last}"
    );
    // the read-insist record keeps the real tool-call count of the dead attempt.
    assert_eq!(last["read_insist"]["final_tool_calls"], 2, "{last}");
    assert_eq!(last["read_insist"]["attempts"], 1);
    // the summary's events-file line points to the re-ask's stream, not the primary's.
    assert!(
        t.contains("read-insist-1.events.jsonl"),
        "the events pointer follows the dead re-ask: {t}"
    );
    // the reply is the dead re-ask's salvage, NOT attempt 1's placeholder.
    let reply_md = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("14-grok-agent-result-size.md"),
    )
    .or_else(|_| {
        std::fs::read_to_string(
            e.repo
                .join(".collab")
                .join("t")
                .join("handoffs")
                .join("01-grok-reply.md"),
        )
    })
    .unwrap_or_default();
    let reply_json = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("01-grok-reply.reply.json"),
    )
    .unwrap_or_default();
    let kept = format!("{reply_md}{reply_json}");
    assert!(
        kept.contains("INTERIM-REVIEW-TEXT") && !kept.contains("PLACEHOLDER-reading"),
        "the dead re-ask salvage is the reply, not attempt 1's placeholder: {kept}"
    );
    // two turns: primary + one re-ask (both fresh).
    assert_eq!(e.turns().len(), 2, "{:?}", e.turns());
}

// ------------------------------ F03-1: a turn that reads then answers nothing is not usable

#[test]
fn a_turn_that_reads_then_answers_nothing_is_not_usable_under_either_transport() {
    // (F03-1) A turn that wrote a placeholder, made a tool call, then ended with NO text after the
    // last tool call has no final answer. The run must FAIL - the pre-tool placeholder is never
    // ingested as the reply - under BOTH the default prompt-only transport and the selectable native
    // one. One tool call is made, so the zero-tool guard does not own this case: grok_turn_outcome
    // must fail it.
    for transport in ["prompt-only", "native"] {
        let e = setup(&format!("no-final-{transport}"));
        let placeholder = e.work.join("placeholder.json");
        std::fs::write(
            &placeholder,
            r#"{"schema_version":"1","verdict":"HOLD","verdict_reason":"PLACEHOLDER-starting the read","reply_markdown":"PLACEHOLDER-reading before answering","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#,
        )
        .unwrap();
        let o = e.consult(
            &[
                "--engine",
                "grok",
                "--model",
                "grok-4.7",
                "--purpose",
                "diff-review",
                "--schema-transport",
                transport,
            ],
            &[
                ("FAKE_GROK_PRETOOL_REPLY", &placeholder.to_string_lossy()),
                ("FAKE_GROK_REPLY", ""), // no text after the last tool call
                ("FAKE_GROK_TOOL_CALLS", "1"),
            ],
        );
        let t = text(&o);
        let last = e.last();
        assert!(
            !last["bridge_outcome"]
                .as_str()
                .unwrap_or("")
                .starts_with("usable"),
            "[{transport}] a turn with no post-tool answer is not usable: {last}\n{t}"
        );
        assert_ne!(
            last["structured"], true,
            "[{transport}] not structured: {last}"
        );
        // the placeholder is never written as the reply.
        let reply_json = std::fs::read_to_string(
            e.repo
                .join(".collab")
                .join("t")
                .join("handoffs")
                .join("01-grok-reply.reply.json"),
        )
        .unwrap_or_default();
        assert!(
            !reply_json.contains("PLACEHOLDER-reading"),
            "[{transport}] the placeholder must not be ingested as the reply: {reply_json}"
        );
        // a non-guard primary failure drives no read-insist re-ask: a single turn.
        assert_eq!(
            e.turns().len(),
            1,
            "[{transport}] single turn: {:?}",
            e.turns()
        );
    }
}

// ------------------------------ F02-5: a dead re-ask with only pre-tool text clears the reply

#[test]
fn a_dead_reask_with_only_pre_tool_text_clears_the_reply_not_the_placeholder() {
    // (F02-5) The primary diff-review turn answers with 0 tool calls (a placeholder; the guard
    // fires). The fresh re-ask reads 2 files, emits ONLY a pre-tool interim note, then CRASHES with
    // no text after the last tool call. The salvage is the text AFTER the last tool call only - here
    // empty - so the recorded reply is CLEARED ("no reply captured"), never the primary's placeholder
    // nor the pre-tool note.
    let e = setup("insist-death-pretool");
    let placeholder = e.work.join("placeholder.json");
    std::fs::write(
        &placeholder,
        r#"{"schema_version":"1","verdict":"HOLD","verdict_reason":"r","reply_markdown":"PLACEHOLDER-reading before answering","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#,
    )
    .unwrap();
    let pretool = e.work.join("pretool.txt");
    std::fs::write(
        &pretool,
        "PRETOOL-NOTE about to read the files now before I answer",
    )
    .unwrap();
    // an EMPTY reply FILE (not an unset env, which the fake would treat as "fall back to the primary
    // reply"): the re-ask emits its pre-tool note, reads 2 files, then NO text after the last one.
    let empty_reply = e.work.join("empty-reply.txt");
    std::fs::write(&empty_reply, "").unwrap();
    let o = e.consult(
        &[
            "--engine",
            "grok",
            "--model",
            "grok-4.7",
            "--purpose",
            "diff-review",
        ],
        &[
            ("FAKE_GROK_REPLY", &placeholder.to_string_lossy()),
            ("FAKE_GROK_RESUME_REPLY", &empty_reply.to_string_lossy()),
            ("FAKE_GROK_PRETOOL_REPLY", &pretool.to_string_lossy()),
            ("FAKE_GROK_RESUME_TOOL_CALLS", "2"),
            ("FAKE_GROK_RESUME_NO_END", "1"),
        ],
    );
    let t = text(&o);
    let last = e.last();
    let outcome = last["bridge_outcome"].as_str().unwrap_or("");
    // the re-ask DID make tool calls, so it still ends the run, named attempt 1 with its count.
    assert!(
        outcome.contains("read-insist attempt 1 after a zero-tool primary")
            && outcome.contains("2 tool calls"),
        "the dead re-ask still ends the run: {outcome}\n{t}"
    );
    let reply_md = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("01-grok-reply.md"),
    )
    .unwrap_or_default();
    let reply_json = std::fs::read_to_string(
        e.repo
            .join(".collab")
            .join("t")
            .join("handoffs")
            .join("01-grok-reply.reply.json"),
    )
    .unwrap_or_default();
    let kept = format!("{reply_md}{reply_json}");
    assert!(
        !kept.contains("PLACEHOLDER-reading") && !kept.contains("PRETOOL-NOTE"),
        "neither the placeholder nor the pre-tool note is the reply: {kept}"
    );
    assert!(
        reply_md.contains("no reply captured"),
        "an empty dead-reask salvage renders as 'no reply captured': {reply_md}"
    );
}

// ------------------------------------------------------------------ the resume path

#[test]
fn a_prose_first_reply_is_repaired_on_a_resume_turn() {
    let e = setup("resume");
    // first turn: prose (no JSON) but a compliant tool call (so it passes the zero-tool guard); the
    // resume (format-repair) turn: the structured object.
    let o = e.consult(
        &["--engine", "grok", "--model", "grok-4.7"],
        &[
            ("FAKE_GROK_REPLY", &e.prose.to_string_lossy()),
            ("FAKE_GROK_RESUME_REPLY", &e.reply.to_string_lossy()),
            ("FAKE_GROK_TOOL_CALLS", "1"),
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
