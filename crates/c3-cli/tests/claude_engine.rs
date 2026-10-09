//! (wave 4 of the compatibility track) The plugin 0.6.0 `claude` engine end to end through the real
//! `c3` binary against a FAKE claude CLI - `tests/fixtures/fake-claude.{cmd,ps1}`, the plugin's own
//! fake at v0.6.1 (driven by `FAKE_CLAUDE_*`, reached through the test hook
//! `CODEX_CONSULT_TEST_CHILD_ENV_PASS=FAKE_CLAUDE_` in test mode): the dry run's plan, a structured
//! run with its ledger evidence and the ALLOW-listed child environment, the lineage (resume pins the
//! resolved model, fork answers on a new session), the init proof and the model proof (E13), a
//! killed turn judged by its init (E12), a rejecting rate-limit event beside a successful result
//! (E15: the quota mark in the ledger, the machine-wide record and the listing), the endpoint route
//! (E3/E4), the roster's claude keys and the providers row; (wave 4f) the endpoint preflight's
//! launcher probe (F25-1) and its transcript guard (F25-2); (wave 4g) that probe in the endpoint
//! turn's environment minus its token (F32-1) and the fail-closed home derivation (F32-2).
//!
//! GUARD: every child gets a scratch USERPROFILE/HOME and a PATH without any directory that holds a
//! claude launcher, so the real CLI can never start; the fake logs every start. Nothing reaches a
//! real provider or intake. Windows only (the fake is a `.cmd` wrapper around a PowerShell script).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const FAKE_CMD: &str = include_str!("fixtures/fake-claude.cmd");
const FAKE_PS1: &str = include_str!("fixtures/fake-claude.ps1");
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

/// A PATH without any directory that holds a claude launcher (GUARD).
fn safe_path() -> String {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(';')
        .filter(|d| {
            !d.is_empty()
                && !["claude", "claude.exe", "claude.cmd", "claude.ps1"]
                    .iter()
                    .any(|n| Path::new(d).join(n).exists())
        })
        .collect::<Vec<_>>()
        .join(";")
}

struct Env {
    work: PathBuf,
    repo: PathBuf,
    home: PathBuf,
    fake: PathBuf,
    reply: PathBuf,
    log: PathBuf,
}

fn setup(tag: &str) -> Env {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let work = std::env::temp_dir().join(format!("c3-w4-{tag}-{}-{nanos}", std::process::id()));
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
    std::fs::create_dir_all(work.join("userhome").join("AppData").join("Local")).unwrap();
    std::fs::create_dir_all(work.join("claude-config")).unwrap();
    let fake = work.join("fake-claude.cmd");
    std::fs::write(&fake, FAKE_CMD.replace("\r\n", "\n").replace('\n', "\r\n")).unwrap();
    std::fs::write(work.join("fake-claude.ps1"), FAKE_PS1).unwrap();
    let reply = work.join("advise.json");
    std::fs::write(&reply, ADVISE).unwrap();
    let log = work.join("argv.jsonl");
    Env {
        work,
        repo,
        home,
        fake,
        reply,
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
                || k.starts_with("ANTHROPIC_")
                || k.starts_with("CLAUDE")
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
            .env("CLAUDE_CONFIG_DIR", self.work.join("claude-config"))
            .env("CODEX_CONSULT_CLAUDE_EXE", &self.fake)
            .env("CODEX_CONSULT_TEST_MODE", "1")
            .env("CODEX_CONSULT_TEST_CHILD_ENV_PASS", "FAKE_CLAUDE_")
            .env("CODEX_CONSULT_ROSTER", "none")
            .env("CODEX_CONSULT_HEALTH", "none")
            .env("CODEX_CONSULT_TELEMETRY", "off")
            .env("C3_TELEMETRY_HUB", "http://127.0.0.1:9/T")
            .env("C3_PRIORS", "off")
            .env("FAKE_CLAUDE_ARGV_LOG", &self.log)
            .env("FAKE_CLAUDE_REPLY", &self.reply);
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
        let mut args = vec![
            "consult",
            "--task",
            "t",
            "--prompt",
            "Check the %APPDATA% words",
        ];
        args.extend_from_slice(extra);
        self.c3(&args, env)
    }

    fn ledger(&self) -> Vec<Value> {
        let p = self.repo.join(".collab").join("t").join("sessions.json");
        match std::fs::read_to_string(p) {
            Ok(t) => serde_json::from_str::<Value>(&t).unwrap()["codex"]["consults"]
                .as_array()
                .unwrap()
                .clone(),
            Err(_) => Vec::new(),
        }
    }

    fn last(&self) -> Value {
        self.ledger().last().cloned().unwrap()
    }

    /// The fake's starts of the last call: `(kind, record)`.
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

fn arg_of(rec: &Value, flag: &str) -> String {
    let a = rec["argv"].as_array().cloned().unwrap_or_default();
    for i in 0..a.len().saturating_sub(1) {
        if a[i] == flag {
            return a[i + 1].as_str().unwrap_or("").to_string();
        }
    }
    String::new()
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

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn is_uuid(s: &str) -> bool {
    regex::Regex::new(UUID_RE).unwrap().is_match(s)
}

// ------------------------------------------------------------------ DRYRUN

#[test]
fn a_dry_run_plans_the_restricted_turn_and_starts_no_turn() {
    let e = setup("dry");
    let x = e.consult(
        &["--dry-run", "--engine", "claude", "--model", "sonnet"],
        &[],
    );
    let t = text(&x);
    assert_eq!(x.status.code(), Some(0), "{t}");
    assert!(t.contains("harness     : claude-cli 2.1.285-fake"), "{t}");
    assert!(
        t.contains("preflight   : available (ok: signed in (claude.ai subscription))"),
        "{t}"
    );
    assert!(
        t.contains("child env   : an allow list (auth subscription): "),
        "{t}"
    );
    assert!(
        t.contains("max steps   : the claude CLI's default (no --max-turns)"),
        "{t}"
    );
    assert!(t.contains("denial retry: 1 attempt"), "{t}");
    let cmd_line = t
        .lines()
        .find(|l| l.starts_with("command     : "))
        .unwrap()
        .to_string();
    assert!(cmd_line.starts_with("command     : claude -p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model sonnet --effort high --json-schema "), "{cmd_line}");
    let minted = cmd_line.rsplit(' ').next().unwrap();
    assert!(
        cmd_line.contains(&format!(" --session-id {minted}")) && is_uuid(minted),
        "{cmd_line}"
    );
    assert!(
        t.contains("reply file  : ") && t.contains("01-claudecode-reply.md"),
        "{t}"
    );
    assert!(
        t.contains("claude model sonnet is an alias: the alias floats"),
        "{t}"
    );
    // the sign-in check and the version probe ran once each, in the child environment; no turn
    let kinds: Vec<String> = e
        .starts()
        .iter()
        .map(|s| s["kind"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| *k == "auth").count(),
        1,
        "{kinds:?}"
    );
    assert_eq!(
        kinds.iter().filter(|k| *k == "version").count(),
        1,
        "{kinds:?}"
    );
    assert!(!kinds.iter().any(|k| k == "turn"), "{kinds:?}");
    // the account's e-mail and organisation never surface
    assert!(!t.contains("fake-person@example") && !t.contains("FAKE-ORG-NAME"));
}

#[test]
fn refusals_name_claude() {
    let e = setup("refuse");
    let cases: Vec<(Vec<&str>, &str)> = vec![
        (vec!["--engine", "claude", "--model", "gpt-5.1"], "codex-consult: the claude model 'gpt-5.1' is not in the claude engine's model table"),
        (vec!["--engine", "claude"], "codex-consult: the claude engine needs a model"),
        (vec!["--engine", "claude", "--model", "sonnet", "--schema-transport", "output-schema"], "codex-consult: -SchemaTransport output-schema is refused for the claude engine: it takes native or prompt-only"),
        (vec!["--schema-transport", "native"], "codex-consult: -SchemaTransport native is for the agy, muse and claude engines"),
        (vec!["--max-model-steps", "4"], "codex-consult: -MaxModelSteps is for an engine with a model-step cap (muse --max-model-steps, claude --max-turns); the codex engine has none."),
    ];
    for (args, want) in cases {
        let mut a = vec!["--dry-run"];
        a.extend(args.iter());
        let x = e.consult(&a, &[]);
        let t = text(&x);
        assert_eq!(x.status.code(), Some(1), "{args:?}: {t}");
        assert!(t.contains(want), "{args:?}: {t}");
    }
}

// ------------------------------------------------------------------ RUN + HYGIENE

#[test]
fn a_structured_run_records_the_engines_evidence_and_the_child_gets_the_allow_list() {
    let e = setup("run");
    let x = e.consult(
        &[
            "--engine",
            "claude",
            "--model",
            "sonnet",
            "--reply-name",
            "run",
        ],
        &[
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:9/"),
            ("ANTHROPIC_API_KEY", "fake-key-0815"),
            ("CLAUDE_CODE_EFFORT_LEVEL", "max"),
            ("W4_OPERATOR_VAR", "x"),
        ],
    );
    let t = text(&x);
    assert_eq!(x.status.code(), Some(0), "{t}");
    let l = e.last();
    assert_eq!(l["bridge_outcome"], "usable reply", "{l}");
    assert_eq!(l["reviewer"]["engine"], "claude");
    assert_eq!(l["reviewer"]["harness"], "claude-cli 2.1.285-fake");
    assert_eq!(
        l["reviewer"]["provider_config"]["credential_mechanism"],
        "subscription"
    );
    assert_eq!(l["reviewer"]["provider_config"]["auth_method"], "claude.ai");
    assert_eq!(
        l["reviewer"]["provider_config"]["api_provider"],
        "firstParty"
    );
    assert_eq!(l["usage"]["input_tokens"], 12012);
    assert_eq!(l["effort_mapping"], "claude-v1");
    assert_eq!(l["schema_transport"], "native");
    let turns = e.turns();
    assert_eq!(turns.len(), 1);
    let minted = arg_of(&turns[0], "--session-id");
    assert!(is_uuid(&minted) && l["thread"] == minted.as_str(), "{l}");
    let er = &l["engine_run"];
    assert_eq!(er["turns"], 1);
    assert_eq!(er["auth"], "subscription");
    assert_eq!(
        strs(&er["init_tools"]).join(","),
        "Glob,Grep,Read,StructuredOutput"
    );
    assert_eq!(er["mcp_servers"], 0);
    assert_eq!(er["permission_mode"], "dontAsk");
    assert_eq!(er["api_key_source"], "none");
    assert_eq!(er["model_resolved"], "claude-sonnet-5-5");
    assert!(er["rate_limit"].is_null() && er["quota_mark"].is_null());
    assert_eq!(strs(&er["switched_off"]).len(), 11);
    let allowed = strs(&er["child_env_allowed"]);
    assert!(allowed.contains(&"DISABLE_AUTOUPDATER".to_string()));
    assert!(!allowed
        .iter()
        .any(|n| n.starts_with("ANTHROPIC_") || n.starts_with("CLAUDE_CODE_")));
    // what the child got
    let hy = &turns[0];
    for absent in [
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_API_KEY",
        "CLAUDE_CODE_EFFORT_LEVEL",
        "W4_OPERATOR_VAR",
        "CODEX_CONSULT_TEST_MODE",
        "CODEX_HOME",
    ] {
        assert!(!has_name(hy, absent), "{absent} reached the child");
    }
    assert!(has_name(hy, "CLAUDE_CONFIG_DIR") && has_name(hy, "PATH"));
    assert_eq!(hy["autoupdater"], "1");
    assert_eq!(hy["schema_ok"], true);
    let stdin = hy["stdin"].as_str().unwrap();
    assert!(stdin.contains("Check the %APPDATA% words"));
    assert!(stdin.ends_with(&format!(
        "Consultation id: {}",
        l["consult_id"].as_str().unwrap()
    )));
    assert!(!t.contains("fake-key-0815"));
    // the reply files carry the engine's reply prefix and the header names the turns
    let reply = l["reply"].as_str().unwrap();
    assert!(reply.starts_with("handoffs/01-claudecode-run"), "{reply}");
    let md = std::fs::read_to_string(e.repo.join(".collab").join("t").join(reply)).unwrap();
    assert!(md.contains("Engine turns: 1 (claude -p, auth subscription; model claude-sonnet-5-5; init tools Glob, Grep, Read, StructuredOutput; permission denials 0)."), "{md}");
}

// ------------------------------------------------------------------ RESUME + FORK (D4)

#[test]
fn resume_pins_the_resolved_model_and_fork_answers_on_a_new_session() {
    let e = setup("resume");
    let first = e.consult(
        &[
            "--engine",
            "claude",
            "--model",
            "sonnet",
            "--reply-name",
            "a",
        ],
        &[],
    );
    assert_eq!(first.status.code(), Some(0), "{}", text(&first));
    let t0 = e.last()["thread"].as_str().unwrap().to_string();
    let r = e.consult(
        &[
            "--engine",
            "claude",
            "--model",
            "sonnet",
            "--mode",
            "resume",
            "--reply-name",
            "b",
        ],
        &[],
    );
    assert_eq!(r.status.code(), Some(0), "{}", text(&r));
    let tr = e.turns();
    assert_eq!(arg_of(&tr[0], "--resume"), t0);
    assert_eq!(arg_of(&tr[0], "--model"), "claude-sonnet-5-5");
    assert_eq!(e.last()["thread"], t0.as_str());
    let f = e.consult(
        &[
            "--engine",
            "claude",
            "--model",
            "sonnet",
            "--mode",
            "fork",
            "--reply-name",
            "c",
        ],
        &[],
    );
    assert_eq!(f.status.code(), Some(0), "{}", text(&f));
    let tf = e.turns();
    assert_eq!(arg_of(&tf[0], "--resume"), t0);
    assert!(tf[0]["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "--fork-session"));
    let l = e.last();
    assert_eq!(l["parent_thread"], t0.as_str());
    assert!(is_uuid(l["thread"].as_str().unwrap()) && l["thread"] != t0.as_str());
}

// ------------------------------------------------------------------ the proofs (item 4, E13, E12)

#[test]
fn the_init_and_the_model_prove_the_turn() {
    let e = setup("proof");
    type Case<'a> = (Vec<(&'a str, &'a str)>, &'a str, &'a str);
    let cases: Vec<Case> = vec![
        (
            vec![(
                "FAKE_CLAUDE_INIT_TOOLS",
                "Glob,Grep,Read,StructuredOutput,Bash",
            )],
            "failed: the init event lists tools outside Read, Grep, Glob, StructuredOutput: Bash",
            "permission",
        ),
        (
            vec![("FAKE_CLAUDE_INIT_MODEL", "claude-opus-5-5")],
            "failed: model drift: asked sonnet, served claude-opus-5-5",
            "capability",
        ),
        (
            vec![("FAKE_CLAUDE_INIT_DROP", "tools")],
            "failed: init event lacks tools - the CLI's schema changed; pin the version",
            "capability",
        ),
        (
            vec![("FAKE_CLAUDE_APIKEYSOURCE", "ANTHROPIC_API_KEY")],
            "failed: the init event names apiKeySource ANTHROPIC_API_KEY, not none",
            "auth",
        ),
    ];
    for (k, (env, want, class)) in cases.into_iter().enumerate() {
        let name = format!("p{k}");
        let x = e.consult(
            &[
                "--engine",
                "claude",
                "--model",
                "sonnet",
                "--reply-name",
                &name,
            ],
            &env,
        );
        let l = e.last();
        assert_eq!(x.status.code(), Some(1), "{}", text(&x));
        assert!(
            l["bridge_outcome"].as_str().unwrap().starts_with(want),
            "{l}"
        );
        assert_eq!(l["provider_failure"]["class"], class);
    }
    // E13: another model authored the answer while the pinned one has the largest modelUsage (a
    // repository of its own: the auth failure above marks the sonnet route out for 24 hours)
    let e = setup("e13");
    let x = e.consult(
        &[
            "--engine",
            "claude",
            "--model",
            "claude-sonnet-5-5",
            "--reply-name",
            "e13",
        ],
        &[
            ("FAKE_CLAUDE_ASSISTANT_MODEL", "claude-opus-5-5"),
            (
                "FAKE_CLAUDE_MODEL_USAGE",
                "claude-sonnet-5-5=100,claude-opus-5-5=10",
            ),
        ],
    );
    assert_eq!(x.status.code(), Some(1));
    assert_eq!(
        e.last()["bridge_outcome"],
        "failed: a different model authored an assistant message: claude-opus-5-5"
    );
}

#[test]
fn a_killed_turn_is_judged_by_its_init_and_a_clean_one_continues_on_the_minted_session() {
    let e = setup("killed");
    // E12: Bash in the killed turn's init - no continuation, the problem is the reason
    let x = e.consult(
        &[
            "--engine",
            "claude",
            "--model",
            "sonnet",
            "--reply-name",
            "e12",
            "--timeout-sec",
            "8",
            "--continue-sec",
            "60",
        ],
        &[
            ("FAKE_CLAUDE_HANG", "new"),
            ("FAKE_CLAUDE_TEXT", "1"),
            ("FAKE_CLAUDE_INIT_SCOPE", "new"),
            (
                "FAKE_CLAUDE_INIT_TOOLS",
                "Glob,Grep,Read,StructuredOutput,Bash",
            ),
        ],
    );
    let l = e.last();
    assert_eq!(x.status.code(), Some(1), "{}", text(&x));
    let bo = l["bridge_outcome"].as_str().unwrap();
    assert!(bo.starts_with("failed: the init event lists tools outside Read, Grep, Glob, StructuredOutput: Bash - "), "{bo}");
    assert!(
        bo.contains("(the turn was also stopped: timeout after 8 s (process tree killed"),
        "{bo}"
    );
    assert_eq!(l["provider_failure"]["class"], "permission");
    assert!(l["timeout_continue"]["outcome"]
        .as_str()
        .unwrap()
        .starts_with("not attempted: the killed turn failed its proof (class permission: the init event lists tools outside"));
    assert_eq!(e.turns().len(), 1);
    assert!(is_uuid(l["thread_candidate"].as_str().unwrap()));
    // a clean killed turn: the continuation resumes the MINTED session with the resolved model
    let y = e.consult(
        &[
            "--engine",
            "claude",
            "--model",
            "sonnet",
            "--reply-name",
            "to",
            "--timeout-sec",
            "8",
            "--continue-sec",
            "60",
        ],
        &[("FAKE_CLAUDE_HANG", "new"), ("FAKE_CLAUDE_TEXT", "1")],
    );
    assert_eq!(y.status.code(), Some(0), "{}", text(&y));
    let t = e.turns();
    let minted = arg_of(&t[0], "--session-id");
    assert_eq!(t.len(), 2);
    assert_eq!(arg_of(&t[1], "--resume"), minted);
    assert_eq!(arg_of(&t[1], "--model"), "claude-sonnet-5-5");
    let l = e.last();
    assert_eq!(
        l["bridge_outcome"],
        "usable reply (after a timeout continuation)"
    );
    assert_eq!(l["thread"], minted.as_str());
}

// ------------------------------------------------------------------ E15: the quota mark

#[test]
fn a_rejecting_rate_limit_beside_a_successful_result_marks_the_route() {
    let e = setup("e15");
    let health = e.work.join("health.json");
    let reset = (chrono::Utc::now() + chrono::Duration::hours(3)).timestamp();
    let rs = reset.to_string();
    let hp = health.to_string_lossy().to_string();
    let x = e.consult(
        &[
            "--engine",
            "claude",
            "--model",
            "sonnet",
            "--reply-name",
            "e15",
        ],
        &[
            ("FAKE_CLAUDE_RATE_LIMIT", "rejected-success"),
            ("FAKE_CLAUDE_RATE_RESET", &rs),
            ("CODEX_CONSULT_HEALTH", &hp),
        ],
    );
    assert_eq!(x.status.code(), Some(0), "{}", text(&x));
    let l = e.last();
    assert_eq!(l["bridge_outcome"], "usable reply");
    assert_eq!(l["engine_run"]["rate_limit"]["status"], "rejected");
    let qm = &l["engine_run"]["quota_mark"];
    assert_eq!(qm["class"], "quota", "{l}");
    let ra = chrono::DateTime::parse_from_rfc3339(qm["retry_after"].as_str().unwrap()).unwrap();
    assert_eq!(ra.timestamp(), reset);
    assert!(strs(&l["warnings"])
        .iter()
        .any(|w| w.starts_with("a rate limit rejected a request during the turn: {")));
    let h: Value = serde_json::from_str(&std::fs::read_to_string(&health).unwrap()).unwrap();
    let ok = h["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["class"] == "ok")
        .cloned()
        .unwrap();
    assert_eq!(ok["quota_mark"]["class"], "quota", "{ok}");
    // the dry run of the same reviewer now refuses: the route is out until the reset
    let d = e.consult(
        &["--dry-run", "--engine", "claude", "--model", "sonnet"],
        &[],
    );
    assert!(
        text(&d).contains("preflight   : unavailable (usage limit until "),
        "{}",
        text(&d)
    );
}

// ------------------------------------------------------------------ ENDPOINT (E1-E4)

#[test]
fn the_endpoint_route_takes_its_own_variables_and_refuses_without_its_token() {
    let e = setup("ep");
    let roster = e.roster(r#"{"roster_version":1,"reviewers":[{"provider":"ZAI-claude","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"W4_FAKE_ZAI_TOKEN","timeout_ms":3000000},"plan":"zai"}]}"#);
    let d = e.consult(
        &["--dry-run"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4_FAKE_ZAI_TOKEN", "fake-token-w4"),
        ],
    );
    let t = text(&d);
    assert_eq!(d.status.code(), Some(0), "{t}");
    assert!(
        t.contains("preflight   : available (ok: env W4_FAKE_ZAI_TOKEN set)"),
        "{t}"
    );
    assert!(t.contains("endpoint    : https://api.z.ai/api/anthropic (ANTHROPIC_BASE_URL); token from env W4_FAKE_ZAI_TOKEN (ANTHROPIC_AUTH_TOKEN - the value is never shown); API_TIMEOUT_MS 3000000; plan zai; no claude auth status - the model the init event names is the proof"), "{t}");
    assert!(
        t.contains("ANTHROPIC_AUTH_TOKEN, ANTHROPIC_BASE_URL, API_TIMEOUT_MS"),
        "{t}"
    );
    assert!(!t.contains("fake-token-w4"));
    assert!(
        !e.starts().iter().any(|s| s["kind"] == "auth"),
        "no `claude auth status` on the route"
    );
    // a run: the child gets the route's three variables, never the parent's
    let x = e.consult(
        &["--reply-name", "ep"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4_FAKE_ZAI_TOKEN", "fake-token-w4"),
            ("FAKE_CLAUDE_TOKEN_EXPECT", "fake-token-w4"),
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:9/"),
            ("ANTHROPIC_API_KEY", "fake-key-0815"),
        ],
    );
    assert_eq!(x.status.code(), Some(0), "{}", text(&x));
    let turn = &e.turns()[0];
    assert_eq!(turn["base_url"], "https://api.z.ai/api/anthropic");
    assert_eq!(turn["api_timeout_ms"], "3000000");
    assert_eq!(turn["token_match"], true);
    assert!(!has_name(turn, "ANTHROPIC_API_KEY") && !has_name(turn, "W4_FAKE_ZAI_TOKEN"));
    let l = e.last();
    assert_eq!(l["engine_run"]["model_resolved"], "glm-5.3");
    assert_eq!(
        l["reviewer"]["provider_config"]["base_url"],
        "https://api.z.ai/api/anthropic"
    );
    assert_eq!(l["reviewer"]["provider_config"]["plan"], "zai");
    let ledger_text =
        std::fs::read_to_string(e.repo.join(".collab").join("t").join("sessions.json")).unwrap();
    assert!(!ledger_text.contains("fake-token-w4"));
    // the token variable unset: -SkipPreflight is refused before launch
    let n = e.consult(
        &["--skip-preflight", "--reply-name", "n"],
        &[("CODEX_CONSULT_ROSTER", &roster)],
    );
    assert_eq!(n.status.code(), Some(1));
    assert!(text(&n).contains("the claude run is refused before launch: the claude engine's child environment is not usable: env W4_FAKE_ZAI_TOKEN not set (the token of auth endpoint); nothing was started"), "{}", text(&n));
    assert!(e.turns().is_empty());
}

/// (wave 4f, F25-1 / RC1) A valid endpoint entry with its token variable set but a launcher that
/// does not run - `--engine-exe` naming a file that is no program, the configured launcher one
/// whose `--version` fails - is unavailable at the preflight and no turn starts; the fake (a
/// runnable launcher) is available. Still no `claude auth status` on the route (E3).
#[test]
fn an_endpoint_launcher_that_does_not_run_is_unavailable_before_any_turn() {
    let e = setup("eplaunch");
    let roster = e.roster(r#"{"roster_version":1,"reviewers":[{"provider":"ZAI-claude","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"W4F_FAKE_ZAI_TOKEN"},"plan":"zai"}]}"#);
    let junk = e.work.join("not-claude.exe");
    std::fs::write(&junk, "this is not a program\n").unwrap();
    let junk_s = junk.to_string_lossy().to_string();
    // --engine-exe names a file that is no program: the dry run of the entry says unavailable
    let d = e.consult(
        &[
            "--dry-run",
            "--provider",
            "ZAI-claude",
            "--engine-exe",
            &junk_s,
        ],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4F_FAKE_ZAI_TOKEN", "fake-token-w4f"),
        ],
    );
    let t = text(&d);
    assert_eq!(d.status.code(), Some(0), "{t}");
    assert!(t.contains("preflight   : unavailable (the claude launcher does not run - `claude --version` could not be started ("), "{t}");
    assert!(t.contains(") - a real run is refused"), "{t}");
    assert!(!t.contains("fake-token-w4f"));
    // the real run (the roster walk): no available reviewer, nothing started
    let x = e.consult(
        &["--engine-exe", &junk_s, "--reply-name", "junk"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4F_FAKE_ZAI_TOKEN", "fake-token-w4f"),
        ],
    );
    let tx = text(&x);
    assert_eq!(x.status.code(), Some(1), "{tx}");
    assert!(tx.contains("is available; nothing was started: #1 ZAI-claude :: glm-5.3 [claude] (missing: the claude launcher does not run - `claude --version` could not be started ("), "{tx}");
    assert!(e.starts().is_empty(), "nothing may start");
    assert!(e.ledger().is_empty());
    // the configured launcher (CODEX_CONSULT_CLAUDE_EXE) whose `--version` fails
    let bad = e.work.join("bad-claude.cmd");
    std::fs::write(&bad, "@echo not claude\r\n@exit /b 3\r\n").unwrap();
    let b = e.consult(
        &["--dry-run", "--provider", "ZAI-claude"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4F_FAKE_ZAI_TOKEN", "fake-token-w4f"),
            ("CODEX_CONSULT_CLAUDE_EXE", &bad.to_string_lossy()),
        ],
    );
    let tb = text(&b);
    assert!(
        tb.contains("preflight   : unavailable (the claude launcher does not run - `claude --version` exited 3) - a real run is refused"),
        "{tb}"
    );
    // the runnable fake: available, one `--version` probe (the harness's), no `claude auth status`
    let g = e.consult(
        &["--dry-run"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4F_FAKE_ZAI_TOKEN", "fake-token-w4f"),
        ],
    );
    let tg = text(&g);
    assert!(
        tg.contains("preflight   : available (ok: env W4F_FAKE_ZAI_TOKEN set)"),
        "{tg}"
    );
    let starts = e.starts();
    assert_eq!(
        starts.iter().filter(|s| s["kind"] == "version").count(),
        1,
        "{starts:?}"
    );
    assert!(!starts.iter().any(|s| s["kind"] == "auth"));
}

/// (wave 4f, F25-2 / RC2) Auth endpoint runs no `claude auth status`, so the transcript guard reads
/// the projects directory Claude Code derives: `CLAUDE_CONFIG_DIR` outside the repository but its
/// `projects` directory a junction into it - refused before any turn with the plugin's text;
/// a plain `CLAUDE_CONFIG_DIR` outside - allowed (a usable run).
#[test]
fn endpoint_transcripts_that_would_land_in_the_repository_are_refused() {
    let e = setup("epprojdir");
    let roster = e.roster(r#"{"roster_version":1,"reviewers":[{"provider":"ZAI-claude","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"W4F_FAKE_ZAI_TOKEN"},"plan":"zai"}]}"#);
    let target = e.repo.join("transcripts");
    std::fs::create_dir_all(&target).unwrap();
    let cfg = e.work.join("claude-config-linked");
    std::fs::create_dir_all(&cfg).unwrap();
    let link = cfg.join("projects");
    let st = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(st.success(), "mklink /J");
    let cfg_s = cfg.to_string_lossy().to_string();
    let refusal = format!("the claude projectsDirectory ({}) lies inside the repository under review: the engine's transcripts would change the tree", link.to_string_lossy());
    let d = e.consult(
        &["--dry-run"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4F_FAKE_ZAI_TOKEN", "fake-token-w4f"),
            ("CLAUDE_CONFIG_DIR", &cfg_s),
        ],
    );
    let t = text(&d);
    assert_eq!(d.status.code(), Some(0), "{t}");
    assert!(
        t.contains(&format!("a real run is refused: {refusal}")),
        "{t}"
    );
    let x = e.consult(
        &["--reply-name", "inside"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4F_FAKE_ZAI_TOKEN", "fake-token-w4f"),
            ("CLAUDE_CONFIG_DIR", &cfg_s),
        ],
    );
    let tx = text(&x);
    assert_eq!(x.status.code(), Some(1), "{tx}");
    assert!(
        tx.contains(&format!(
            "the claude engine is refused: {refusal}; nothing was started."
        )),
        "{tx}"
    );
    assert!(e.turns().is_empty());
    assert!(!e.starts().iter().any(|s| s["kind"] == "auth"));
    let _ = std::fs::remove_dir(&link);
    // CLAUDE_CONFIG_DIR outside with a plain projects directory: the run goes on
    let ok = e.consult(
        &["--reply-name", "outside"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4F_FAKE_ZAI_TOKEN", "fake-token-w4f"),
            ("FAKE_CLAUDE_TOKEN_EXPECT", "fake-token-w4f"),
        ],
    );
    assert_eq!(ok.status.code(), Some(0), "{}", text(&ok));
    assert_eq!(e.turns().len(), 1);
    assert_eq!(e.last()["bridge_outcome"], "usable reply");
}

/// A launcher whose `--version` answers only in one environment, logging every start's arguments
/// to `<work>/probe-starts.txt`: `endpoint_env` false - only WITHOUT `ANTHROPIC_BASE_URL` (exit 4
/// under the endpoint turn's environment); true - only WITH it and WITHOUT `ANTHROPIC_AUTH_TOKEN`
/// (exit 5 / 6 otherwise).
fn env_launcher(e: &Env, name: &str, endpoint_env: bool) -> PathBuf {
    let log = e.work.join("probe-starts.txt");
    let check = if endpoint_env {
        "@if not defined ANTHROPIC_BASE_URL exit /b 5\r\n@if defined ANTHROPIC_AUTH_TOKEN exit /b 6\r\n"
    } else {
        "@if defined ANTHROPIC_BASE_URL exit /b 4\r\n"
    };
    let p = e.work.join(name);
    std::fs::write(
        &p,
        format!(
            "@echo %*>>\"{}\"\r\n{check}@echo 2.1.0-w4g (Claude Code)\r\n@exit /b 0\r\n",
            log.to_string_lossy()
        ),
    )
    .unwrap();
    p
}

/// (wave 4g, F32-1 / RC1) The endpoint preflight probes the launcher as the endpoint turn starts
/// it - the same launcher, the turn's child environment minus its token: a launcher that answers
/// `--version` only without the route's variables is unavailable before any turn (the 4f probe,
/// in auth subscription's environment, called it available); one that answers only with them is
/// available with its version as the harness string (one `--version` start); the fake's own
/// probe start carries the roster's base URL and `API_TIMEOUT_MS` and never a token.
#[test]
fn the_endpoint_preflight_probes_the_launcher_in_the_endpoint_turns_environment() {
    let e = setup("epprobeenv");
    let roster = e.roster(r#"{"roster_version":1,"reviewers":[{"provider":"ZAI-claude","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"W4G_FAKE_ZAI_TOKEN"},"plan":"zai"}]}"#);
    let probe_log = e.work.join("probe-starts.txt");
    // answers only WITHOUT ANTHROPIC_BASE_URL: unavailable on the endpoint route
    let off = env_launcher(&e, "off-claude.cmd", false);
    let off_s = off.to_string_lossy().to_string();
    let d = e.consult(
        &["--dry-run", "--provider", "ZAI-claude"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4G_FAKE_ZAI_TOKEN", "fake-token-w4g"),
            ("CODEX_CONSULT_CLAUDE_EXE", &off_s),
        ],
    );
    let t = text(&d);
    assert_eq!(d.status.code(), Some(0), "{t}");
    assert!(
        t.contains("preflight   : unavailable (the claude launcher does not run - `claude --version` exited 4) - a real run is refused"),
        "{t}"
    );
    assert!(!t.contains("fake-token-w4g"));
    let x = e.consult(
        &["--reply-name", "off"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4G_FAKE_ZAI_TOKEN", "fake-token-w4g"),
            ("CODEX_CONSULT_CLAUDE_EXE", &off_s),
        ],
    );
    let tx = text(&x);
    assert_eq!(x.status.code(), Some(1), "{tx}");
    assert!(tx.contains("is available; nothing was started: #1 ZAI-claude :: glm-5.3 [claude] (missing: the claude launcher does not run - `claude --version` exited 4)"), "{tx}");
    let starts = std::fs::read_to_string(&probe_log).unwrap_or_default();
    assert!(
        !starts.is_empty() && starts.lines().all(|l| l.trim() == "--version"),
        "only `--version` probes, never a turn: {starts}"
    );
    assert!(e.ledger().is_empty());
    let _ = std::fs::remove_file(&probe_log);
    // answers only WITH the route's variables and WITHOUT a token: available, its version the
    // harness string, one `--version` start
    let on = env_launcher(&e, "on-claude.cmd", true);
    let on_s = on.to_string_lossy().to_string();
    let g = e.consult(
        &["--dry-run", "--provider", "ZAI-claude"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4G_FAKE_ZAI_TOKEN", "fake-token-w4g"),
            ("CODEX_CONSULT_CLAUDE_EXE", &on_s),
        ],
    );
    let tg = text(&g);
    assert!(
        tg.contains("preflight   : available (ok: env W4G_FAKE_ZAI_TOKEN set)"),
        "{tg}"
    );
    assert!(tg.contains("harness     : claude-cli 2.1.0-w4g"), "{tg}");
    let starts = std::fs::read_to_string(&probe_log).unwrap_or_default();
    assert_eq!(starts.lines().count(), 1, "{starts}");
    // the fake: its `--version` start got the roster's base URL and API_TIMEOUT_MS, never a
    // token (nor the parent's ANTHROPIC_* values); one start in the dry run of the walk
    let f = e.consult(
        &["--dry-run"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4G_FAKE_ZAI_TOKEN", "fake-token-w4g"),
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:9/"),
            ("ANTHROPIC_AUTH_TOKEN", "parent-token-w4g"),
        ],
    );
    let tf = text(&f);
    assert!(
        tf.contains("preflight   : available (ok: env W4G_FAKE_ZAI_TOKEN set)"),
        "{tf}"
    );
    let versions: Vec<Value> = e
        .starts()
        .into_iter()
        .filter(|s| s["kind"] == "version")
        .collect();
    assert_eq!(versions.len(), 1, "{versions:?}");
    let v = &versions[0];
    assert_eq!(v["base_url"], "https://api.z.ai/api/anthropic");
    assert_eq!(v["api_timeout_ms"], "3000000");
    assert_eq!(v["has_token"], false);
    assert!(!has_name(v, "ANTHROPIC_AUTH_TOKEN") && !has_name(v, "W4G_FAKE_ZAI_TOKEN"));
    assert!(!e.starts().iter().any(|s| s["kind"] == "auth"));
}

/// (wave 4g, F32-2 / RC2) The transcript guard derives the home as Claude Code's `os.homedir()`
/// does and is fail-closed: neither `CLAUDE_CONFIG_DIR` nor `USERPROFILE` in the child's
/// environment (`HOMEDRIVE`/`HOMEPATH` pointing into the repository: not read) - the dry run says
/// a real run is refused and the run is refused before any turn with `the transcript location
/// cannot be established (...)`; `USERPROFILE` inside the repository - the plugin's text.
#[test]
fn a_transcript_location_that_cannot_be_established_refuses_the_endpoint_run() {
    let e = setup("epnohome");
    let roster = e.roster(r#"{"roster_version":1,"reviewers":[{"provider":"ZAI-claude","engine":"claude","model":"glm-5.3","auth":"endpoint","endpoint":{"base_url":"https://api.z.ai/api/anthropic","env_key":"W4G_FAKE_ZAI_TOKEN"},"plan":"zai"}]}"#);
    let home_in = e.repo.join("home");
    std::fs::create_dir_all(&home_in).unwrap();
    let home_in_s = home_in.to_string_lossy().to_string();
    let drive = home_in_s[..2].to_string();
    let path_rest = home_in_s[2..].to_string();
    let why = "the transcript location cannot be established (neither CLAUDE_CONFIG_DIR nor USERPROFILE is set: Claude Code would fall back to the account's profile directory from the system, which c3 does not resolve); set CLAUDE_CONFIG_DIR to a directory outside the repository";
    let no_home: Vec<(&str, &str)> = vec![
        ("CODEX_CONSULT_ROSTER", &roster),
        ("W4G_FAKE_ZAI_TOKEN", "fake-token-w4g"),
        ("CLAUDE_CONFIG_DIR", ""),
        ("USERPROFILE", ""),
        ("HOME", ""),
        ("HOMEDRIVE", &drive),
        ("HOMEPATH", &path_rest),
    ];
    let d = e.consult(&["--dry-run"], &no_home);
    let t = text(&d);
    assert_eq!(d.status.code(), Some(0), "{t}");
    assert!(t.contains(&format!("a real run is refused: {why}")), "{t}");
    let x = e.consult(&["--reply-name", "nohome"], &no_home);
    let tx = text(&x);
    assert_eq!(x.status.code(), Some(1), "{tx}");
    assert!(
        tx.contains(&format!(
            "the claude engine is refused: {why}; nothing was started."
        )),
        "{tx}"
    );
    assert!(e.turns().is_empty());
    assert!(!e.starts().iter().any(|s| s["kind"] == "auth"));
    // USERPROFILE inside the repository (HOME outside: not read on Windows): the plugin's text
    let derived = home_in.join(".claude").join("projects");
    let refusal = format!("the claude projectsDirectory ({}) lies inside the repository under review: the engine's transcripts would change the tree", derived.to_string_lossy());
    let uh = e.work.join("userhome").to_string_lossy().to_string();
    let y = e.consult(
        &["--reply-name", "inrepo"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4G_FAKE_ZAI_TOKEN", "fake-token-w4g"),
            ("CLAUDE_CONFIG_DIR", ""),
            ("USERPROFILE", &home_in_s),
            ("HOME", &uh),
        ],
    );
    let ty = text(&y);
    assert_eq!(y.status.code(), Some(1), "{ty}");
    assert!(
        ty.contains(&format!(
            "the claude engine is refused: {refusal}; nothing was started."
        )),
        "{ty}"
    );
    assert!(e.turns().is_empty());
    // USERPROFILE outside (HOME pointing into the repository changes nothing on Windows): usable
    let ok = e.consult(
        &["--reply-name", "outside"],
        &[
            ("CODEX_CONSULT_ROSTER", &roster),
            ("W4G_FAKE_ZAI_TOKEN", "fake-token-w4g"),
            ("FAKE_CLAUDE_TOKEN_EXPECT", "fake-token-w4g"),
            ("CLAUDE_CONFIG_DIR", ""),
            ("USERPROFILE", &uh),
            ("HOME", &home_in_s),
        ],
    );
    assert_eq!(ok.status.code(), Some(0), "{}", text(&ok));
    assert_eq!(e.turns().len(), 1);
    assert_eq!(e.last()["bridge_outcome"], "usable reply");
}

// ------------------------------------------------------------------ ROSTER + LISTING

#[test]
fn the_roster_validates_the_claude_keys() {
    let e = setup("roster");
    let cases = [
        (r#"{"roster_version":1,"reviewers":[{"provider":"anthropic","engine":"claude","model":"opus","auth":"key"}]}"#, "entry 1: auth of engine claude must be \"subscription\" (the claude.ai login, the default) or \"api-key\""),
        (r#"{"roster_version":1,"reviewers":[{"provider":"anthropic","engine":"claude","model":"claude-opus-9"}]}"#, "entry 1: the claude model 'claude-opus-9' is not in the claude engine's model table"),
        (r#"{"roster_version":1,"reviewers":[{"provider":"x","engine":"claude","model":"sonnet","endpoint":{"base_url":"https://a.example/v","env_key":"ABC"}}]}"#, "entry 1: endpoint applies only to engine claude with auth \"endpoint\" (this entry: engine claude, auth subscription)"),
        (r#"{"roster_version":1,"reviewers":[{"provider":"x","engine":"claude","model":"sonnet","auth":"endpoint","endpoint":{"base_url":"https://a.example/v","env_key":"ABC"}}]}"#, "is an Anthropic model id, which the endpoint route cannot carry"),
        (r#"{"roster_version":1,"reviewers":[{"provider":"g","engine":"gemini-cli","model":"m"}]}"#, "entry 1: engine must be one of: codex, agy, muse, claude (got \"gemini-cli\")"),
    ];
    for (json, want) in cases {
        let r = e.roster(json);
        let x = e.c3(&["providers"], &[("CODEX_CONSULT_ROSTER", &r)]);
        assert!(text(&x).contains(want), "{json}: {}", text(&x));
    }
    let ok = e.roster(r#"{"roster_version":1,"reviewers":[{"provider":"anthropic","engine":"claude","model":"claude-opus-5-5[1m]","auth":"subscription","context_tokens":1000000},{"provider":"openai","model":"gpt-5.1"}],"parallel":{"anthropic":2}}"#);
    let j = e.c3(&["providers", "--json"], &[("CODEX_CONSULT_ROSTER", &ok)]);
    assert_eq!(j.status.code(), Some(0), "{}", text(&j));
    let rows: Value = serde_json::from_slice(&j.stdout).unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "anthropic")
        .cloned()
        .unwrap();
    assert_eq!(row["engine"], "claude");
    assert_eq!(row["kind"], "engine claude");
    assert_eq!(row["credentials"], "ok: signed in (claude.ai subscription)");
    assert_eq!(row["effort_vocabulary"], "claude");
    assert_eq!(row["schema_transport"], "native");
    assert_eq!(row["verdict"], "available");
    assert_eq!(row["roster_selected"], true);
    let fake = e.fake.to_string_lossy().to_string();
    assert!(row["endpoint"].as_str().unwrap().starts_with("claude ("));
    assert!(row["endpoint"]
        .as_str()
        .unwrap()
        .contains(&fake[fake.len() - 15..]));
    assert_eq!(e.starts().iter().filter(|s| s["kind"] == "auth").count(), 1);
}

// ------------------------------------------------------------------ TELEMETRY (FORGET D3)

/// (harness-telemetry FORGET D3) a run whose event meets a running `--forget --local` (the marker
/// of a LIVING owner - this test process) commits its entry, spools nothing and says so with the
/// plugin's ending: `warning    : telemetry event not spooled (<why>) - dropped`.
#[test]
fn an_event_refused_by_a_living_forget_is_said_dropped() {
    let e = setup("forget");
    let dir = e.home.join("c3").join("telemetry");
    std::fs::create_dir_all(&dir).unwrap();
    let marker = c3::telemetry::notspooled::LocalPaths::in_dir(&dir).marker;
    std::fs::write(
        &marker,
        format!(
            "{{\"pid\":{},\"start_ticks\":{},\"since\":\"2026-10-09T08:00:00+02:00\"}}\n",
            std::process::id(),
            c3::telemetry::notspooled::own_start_ticks()
        ),
    )
    .unwrap();
    let x = e.consult(
        &[
            "--engine",
            "claude",
            "--model",
            "sonnet",
            "--telemetry",
            "on",
        ],
        &[("CODEX_CONSULT_TELEMETRY", "")],
    );
    let out = text(&x);
    assert_eq!(x.status.code(), Some(0), "{out}");
    let re = regex::Regex::new(
        r"(?m)^warning    : telemetry event not spooled \(c3 telemetry --forget --local is deleting .*\) - dropped\r?$",
    )
    .unwrap();
    assert!(re.is_match(&out), "{out}");
    assert!(!out.contains("- counted (c3 telemetry --status)"), "{out}");
    assert_eq!(e.ledger().len(), 1, "the entry is committed");
    assert!(marker.exists(), "the living owner's marker stays");
}
