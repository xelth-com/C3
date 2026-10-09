//! (wave 6) The four parity differences the RC3 live matrix found (`docs/port/rc3-live-matrix-
//! 2026-10-09.md`, "Differences from the plugin" 1-4), end to end through the real `c3` binary
//! against a fake codex CLI:
//!
//! 1. the ledger: a plugin-written entry survives a C3 rating and a C3 consultation byte for byte,
//!    and C3's own `coordinator` record carries `host_by`, `in_roster` and `unresolved` in the
//!    plugin's order;
//! 2. `c3 providers`: the header's consultation count is THIS repository's, not the machine-wide
//!    health records';
//! 3. (the agy/muse reviewer line is covered by the unit tests of `consult::orchestrate`);
//! 4. telemetry: the detached sender a run starts right after its commit delivers the run's event
//!    to the intake without another run - the run does not wait for it and the sender holds none of
//!    the run's handles - with the allow-listed environment only; a rating does the same.
//!
//! Nothing reaches a real intake or provider: the intake is a loopback listener of this test (or
//! telemetry is off), `C3_PRIORS=off`, and HOME/USERPROFILE/CODEX_HOME are scratch directories.
//! Windows only (the fake codex is a `.cmd` wrapper around a PowerShell script).
#![cfg(windows)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use serde_json::Value;

/// The plugin's own 0.6.1 `sessions.json` (the RC3 interchange, step (a), task `rc3-x`).
const PLUGIN_LEDGER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../c3-core/tests/fixtures/plugin-0.6.1-sessions.json"
));
/// A machine-wide health file of two endpoint records.
const MACHINE_HEALTH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../c3-core/tests/fixtures/machine-health.json"
));

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!("c3-w6-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn git(repo: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .expect("git on PATH");
    assert!(st.success(), "git {args:?}");
}

const FAKE_CMD: &str = "@echo off\r\nset \"FAKE_CODEX_ARGS=%*\"\r\npowershell -NoProfile -ExecutionPolicy Bypass -File \"%~dp0fake-codex.ps1\"\r\nexit /b %ERRORLEVEL%\r\n";
/// A trimmed fake-codex3: `--version`, `login status`, and an exec turn that copies
/// `FAKE_CODEX_REPLY` to the `-o` file.
const FAKE_PS1: &str = r#"$ErrorActionPreference = 'Stop'
$raw = [string]$env:FAKE_CODEX_ARGS
if ($raw -match '--version') { Write-Output 'codex-cli 0.155.1-fake'; exit 0 }
if ($raw -match '^\s*login\s+status(\s|$)') { [Console]::Error.WriteLine('Logged in using ChatGPT'); exit 0 }
$o = $null
if ($raw -match '(?:^| )-o (\S+)') { $o = $Matches[1] }
$null = [Console]::In.ReadToEnd()
$tid = [guid]::NewGuid().ToString()
[Console]::Out.Write("{""type"":""thread.started"",""thread_id"":""$tid""}`n")
[Console]::Out.Write("{""type"":""turn.started""}`n")
[Console]::Out.Flush()
if ($o -and $env:FAKE_CODEX_REPLY) { [IO.File]::Copy($env:FAKE_CODEX_REPLY, $o, $true) }
[Console]::Out.Write("{""type"":""turn.completed"",""usage"":{""input_tokens"":1000,""cached_input_tokens"":200,""cache_write_input_tokens"":0,""output_tokens"":300,""reasoning_output_tokens"":40}}`n")
exit 0
"#;

const ADVISE: &str = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;

/// The scratch Codex config: the built-in openai (model gpt-5.1) and the plugin entry's ZAI table.
const CODEX_TOML: &str = "model = \"gpt-5.1\"\n\n[model_providers.ZAI]\nbase_url = \"https://api.z.ai/api/v1\"\nenv_key = \"RT_ZAI_KEY\"\nwire_api = \"responses\"\n";

struct Env {
    work: PathBuf,
    repo: PathBuf,
    home: PathBuf,
    fake: PathBuf,
    reply: PathBuf,
}

fn setup(tag: &str) -> Env {
    let work = scratch(tag);
    let repo = work.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@e.com"]);
    git(&repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("app.txt"), "one\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    let home = work.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), CODEX_TOML).unwrap();
    let fake = work.join("fake-codex.cmd");
    std::fs::write(&fake, FAKE_CMD).unwrap();
    std::fs::write(work.join("fake-codex.ps1"), FAKE_PS1).unwrap();
    let reply = work.join("advise.json");
    std::fs::write(&reply, ADVISE).unwrap();
    std::fs::create_dir_all(work.join("userhome")).unwrap();
    Env {
        work,
        repo,
        home,
        fake,
        reply,
    }
}

impl Env {
    /// A `c3` command in the repo with only this test's switches (no inherited bridge, fake, C3 or
    /// host-marker variable); `env` adds (an empty value removes).
    fn c3(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(c3_bin());
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_")
                || k.starts_with("FAKE_")
                || k.starts_with("C3_")
                || k.starts_with("CLAUDE")
                || k == "AI_AGENT"
            {
                cmd.env_remove(&k);
            }
        }
        cmd.current_dir(&self.repo)
            .env("CODEX_HOME", &self.home)
            .env("HOME", self.work.join("userhome"))
            .env("USERPROFILE", self.work.join("userhome"))
            .env("RT_ZAI_KEY", "zai-test-key")
            .env("CODEX_CONSULT_EXE", &self.fake)
            .env("CODEX_CONSULT_ROSTER", "none")
            .env("CODEX_CONSULT_HEALTH", "none")
            .env("C3_TELEMETRY_HUB", "http://127.0.0.1:9/T")
            .env("C3_PRIORS", "off")
            .env("FAKE_CODEX_REPLY", &self.reply);
        for (k, v) in env {
            if v.is_empty() {
                cmd.env_remove(k);
            } else {
                cmd.env(k, v);
            }
        }
        cmd.args(args).output().expect("run c3")
    }

    fn consult(&self, task: &str, extra: &[&str], env: &[(&str, &str)]) -> Output {
        let fake = self.fake.to_string_lossy().to_string();
        let mut args = vec![
            "consult",
            "--task",
            task,
            "--prompt",
            "x",
            "--codex-exe",
            &fake,
        ];
        args.extend_from_slice(extra);
        self.c3(&args, env)
    }

    fn sessions_text(&self, task: &str) -> String {
        std::fs::read_to_string(self.repo.join(".collab").join(task).join("sessions.json")).unwrap()
    }

    fn telemetry_dir(&self) -> PathBuf {
        self.home.join("c3").join("telemetry")
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// The bytes of `text` through its last entry's closing brace (the `}` before the consults array's
/// closing `]`).
fn through_last_entry(text: &str) -> &str {
    let close = text.rfind(']').unwrap();
    let brace = text[..close].rfind('}').unwrap();
    &text[..=brace]
}

fn keys(v: &Value) -> String {
    v.as_object()
        .map(|o| o.keys().cloned().collect::<Vec<_>>().join(","))
        .unwrap_or_default()
}

// ------------------------------------------------------------------ 1. the ledger

#[test]
fn a_plugin_entry_survives_a_c3_rating_and_a_c3_consultation_byte_for_byte() {
    let e = setup("ledger");
    let task_dir = e.repo.join(".collab").join("rc3-x");
    std::fs::create_dir_all(&task_dir).unwrap();
    std::fs::write(task_dir.join("sessions.json"), PLUGIN_LEDGER).unwrap();
    let off = [("CODEX_CONSULT_TELEMETRY", "off")];

    // a C3 rating of the plugin's consultation: findings.json gets the mark, sessions.json is not
    // touched (neither side's rating writes the ledger)
    let r = e.c3(
        &[
            "findings", "--task", "rc3-x", "--rate", "1", "--useful", "yes",
        ],
        &off,
    );
    assert_eq!(r.status.code(), Some(0), "{}", text(&r));
    assert!(
        text(&r).contains("consult n=1 (ZAI :: glm-5.3, chore) rated yes."),
        "{}",
        text(&r)
    );
    assert_eq!(e.sessions_text("rc3-x"), PLUGIN_LEDGER);

    // a C3 consultation appends its entry: every byte of the plugin's file through its entry stays
    let c = e.consult(
        "rc3-x",
        &["--mode", "new", "--reply-name", "c3"],
        &[off[0], ("CLAUDECODE", "1")],
    );
    assert_eq!(c.status.code(), Some(0), "{}", text(&c));
    let after = e.sessions_text("rc3-x");
    let prefix = through_last_entry(PLUGIN_LEDGER);
    assert!(
        after.starts_with(prefix),
        "the plugin's entry was rewritten:\n{after}"
    );
    assert!(after[prefix.len()..].starts_with(",\n"), "{after}");

    // C3's own coordinator record: the plugin's eight keys in its order
    let v: Value = serde_json::from_str(&after).unwrap();
    let consults = v["codex"]["consults"].as_array().unwrap();
    assert_eq!(consults.len(), 2);
    let co = &consults[1]["coordinator"];
    assert_eq!(
        keys(co),
        "provider,model,engine,host,host_by,source,in_roster,unresolved"
    );
    assert_eq!(co["host"], "claude-code");
    assert_eq!(co["host_by"], "markers");
    assert_eq!(co["source"], "inferred");
    assert!(
        co["in_roster"].is_null() && co["unresolved"].is_null(),
        "{co}"
    );
    // the two entries' coordinators are now written alike
    assert_eq!(keys(&consults[0]["coordinator"]), keys(co));

    // a second C3 rating (of C3's entry) leaves the ledger as it is
    let r2 = e.c3(
        &[
            "findings", "--task", "rc3-x", "--rate", "2", "--useful", "partly",
        ],
        &off,
    );
    assert_eq!(r2.status.code(), Some(0), "{}", text(&r2));
    assert_eq!(e.sessions_text("rc3-x"), after);
    let _ = std::fs::remove_dir_all(&e.work);
}

#[test]
fn a_coordinator_without_markers_records_host_by_none() {
    let e = setup("nohost");
    let c = e.consult(
        "t",
        &["--reply-name", "a"],
        &[("CODEX_CONSULT_TELEMETRY", "off")],
    );
    assert_eq!(c.status.code(), Some(0), "{}", text(&c));
    let v: Value = serde_json::from_str(&e.sessions_text("t")).unwrap();
    let co = &v["codex"]["consults"][0]["coordinator"];
    assert_eq!(
        serde_json::to_string(co).unwrap(),
        r#"{"provider":null,"model":null,"engine":null,"host":"unknown","host_by":"none","source":"none","in_roster":null,"unresolved":null}"#
    );
    let _ = std::fs::remove_dir_all(&e.work);
}

// ------------------------------------------------------------------ 2. the providers header

#[test]
fn the_providers_header_counts_this_repositorys_consultations_only() {
    let e = setup("providers");
    let health = e.work.join("health.json");
    std::fs::write(&health, MACHINE_HEALTH).unwrap();
    let h = health.to_string_lossy().to_string();
    let hv = [("CODEX_CONSULT_HEALTH", h.as_str())];
    // no ledger here: the two machine-wide records are not "consultations of THIS repository"
    let p = e.c3(&["providers"], &hv);
    assert_eq!(p.status.code(), Some(0), "{}", text(&p));
    assert!(
        text(&p).contains("(0 task ledgers, 0 consultations), read at "),
        "{}",
        text(&p)
    );
    // one ledger of one consultation
    let task_dir = e.repo.join(".collab").join("rc3-x");
    std::fs::create_dir_all(&task_dir).unwrap();
    std::fs::write(task_dir.join("sessions.json"), PLUGIN_LEDGER).unwrap();
    let p = e.c3(&["providers"], &hv);
    assert_eq!(p.status.code(), Some(0), "{}", text(&p));
    let t = text(&p);
    assert!(
        t.contains("(1 task ledger, 1 consultation), read at "),
        "{t}"
    );
    assert!(t.contains(" - the ledgers of THIS repository"), "{t}");
    // the JSON names the same source
    let j = e.c3(&["providers", "--json"], &hv);
    let jt = String::from_utf8_lossy(&j.stdout).to_string();
    assert!(jt.contains("(1 task ledger, 1 consultation)"), "{jt}");
    let _ = std::fs::remove_dir_all(&e.work);
}

// ------------------------------------------------------------------ 4. the detached sender

/// One HTTP request the intake received.
struct Request {
    line: String,
    body: String,
}

/// Accept requests on `listener` until `count` arrived or `deadline` passed, answering each with
/// the intake's `{"ok":true}`.
fn serve(listener: &TcpListener, count: usize, deadline: Duration) -> Vec<Request> {
    listener.set_nonblocking(true).unwrap();
    let until = Instant::now() + deadline;
    let mut out = Vec::new();
    while out.len() < count && Instant::now() < until {
        let stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(_) => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut stream = stream;
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut first = String::new();
        let _ = reader.read_line(&mut first);
        let mut content_length = 0usize;
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
        let mut buf = vec![0u8; content_length];
        let _ = reader.read_exact(&mut buf);
        let body = r#"{"ok":true}"#;
        let resp = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.flush();
        out.push(Request {
            line: first.trim().to_string(),
            body: String::from_utf8_lossy(&buf).to_string(),
        });
    }
    out
}

/// Wait (at most `deadline`) for the last flush's record to say `delivered >= 1` after `since`.
fn wait_delivered(dir: &Path, deadline: Duration) -> Option<Value> {
    let until = Instant::now() + deadline;
    while Instant::now() < until {
        if let Ok(t) = std::fs::read_to_string(dir.join("last-flush.json")) {
            if let Ok(v) = serde_json::from_str::<Value>(&t) {
                if v["delivered"].as_i64().unwrap_or(0) >= 1 {
                    return Some(v);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

fn spool_lines(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("spool.ndjson"))
        .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}

#[test]
fn the_detached_sender_delivers_a_runs_event_without_another_run() {
    let e = setup("sender");
    // the intake listens from the start but answers only once the run has returned: a run that
    // waited for its sender - or a sender holding the run's output pipes - would stall here
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}/T", listener.local_addr().unwrap());
    let dump = e.work.join("sender-env.txt");
    let dump_s = dump.to_string_lossy().to_string();
    let env = [
        ("C3_TELEMETRY_HUB", hub.as_str()),
        ("CODEX_CONSULT_TEST_MODE", "1"),
        ("CODEX_CONSULT_TEST_TELEMETRY_ENV", dump_s.as_str()),
        // host markers and a provider key the sender must not inherit
        ("CLAUDECODE", "1"),
        ("CODEX_THREAD_ID", "01a0e4bc-0000-7000-8000-000000000006"),
    ];
    let t0 = Instant::now();
    let run = e.consult("t", &["--reply-name", "s1"], &env);
    let run_secs = t0.elapsed().as_secs_f64();
    assert_eq!(run.status.code(), Some(0), "{}", text(&run));
    // the event was spooled at the commit; nothing was delivered while the run ran
    let tdir = e.telemetry_dir();
    assert_eq!(spool_lines(&tdir), 1, "{}", text(&run));

    let reqs = serve(&listener, 1, Duration::from_secs(20));
    assert_eq!(
        reqs.len(),
        1,
        "no request reached the intake (run {run_secs:.1} s)"
    );
    assert_eq!(
        reqs[0].line.split(' ').take(2).collect::<Vec<_>>(),
        ["POST", "/T/v2/events"]
    );
    let body: Value = serde_json::from_str(&reqs[0].body).unwrap();
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1, "{body}");
    assert_eq!(events[0]["event_type"], "consultation");
    let last = wait_delivered(&tdir, Duration::from_secs(10)).expect("the sender's record");
    assert_eq!(last["kept"], 0, "{last}");
    assert_eq!(last["http"], 200, "{last}");
    assert_eq!(spool_lines(&tdir), 0, "delivered = deleted");

    // the sender's environment: the allow list only (names, never values)
    let names: Vec<String> = std::fs::read_to_string(&dump)
        .expect("the sender's environment dump")
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    assert!(!names.is_empty());
    for n in &names {
        assert!(
            c3::telemetry::sender::is_sender_env_name(n, true),
            "{n} is not on the sender's allow list: {names:?}"
        );
    }
    let has = |x: &str| names.iter().any(|n| n.eq_ignore_ascii_case(x));
    assert!(has("CODEX_HOME") && has("C3_TELEMETRY_HUB"), "{names:?}");
    assert!(
        !has("CLAUDECODE") && !has("CODEX_THREAD_ID") && !has("RT_ZAI_KEY"),
        "{names:?}"
    );

    // a rating starts its sender too: the rating event is delivered without another run
    let r = e.c3(
        &["findings", "--task", "t", "--rate", "1", "--useful", "yes"],
        &env,
    );
    assert_eq!(r.status.code(), Some(0), "{}", text(&r));
    let reqs = serve(&listener, 1, Duration::from_secs(20));
    assert_eq!(reqs.len(), 1, "the rating's event did not reach the intake");
    let body: Value = serde_json::from_str(&reqs[0].body).unwrap();
    assert_eq!(body["events"][0]["event_type"], "rating", "{body}");
    let until = Instant::now() + Duration::from_secs(10);
    while spool_lines(&tdir) > 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(spool_lines(&tdir), 0);
    let _ = std::fs::remove_dir_all(&e.work);
}

#[test]
fn a_run_with_telemetry_off_starts_no_sender() {
    let e = setup("off");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}/T", listener.local_addr().unwrap());
    let run = e.consult(
        "t",
        &["--reply-name", "o1", "--telemetry", "off"],
        &[("C3_TELEMETRY_HUB", hub.as_str())],
    );
    assert_eq!(run.status.code(), Some(0), "{}", text(&run));
    assert!(serve(&listener, 1, Duration::from_secs(3)).is_empty());
    assert!(!e.telemetry_dir().join("last-flush.json").exists());
    let _ = std::fs::remove_dir_all(&e.work);
}
