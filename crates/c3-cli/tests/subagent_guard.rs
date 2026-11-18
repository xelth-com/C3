//! (C3 0.2.4) The sub-agent guard end to end, through the real `c3` binary against a fake codex
//! `.cmd` launcher. Since codex-cli 0.169.0 the `-c features.multi_agent=false` switch no longer
//! removes the multi-agent v2 tool surface (model metadata outranks the feature flag), so the bridge
//! now passes `-c agents.enabled=false` as well AND, as defence in depth, the watch that already
//! reads a run's rollout for the quota budget also notices a sub-agent (a `spawn_agent` call in the
//! run's rollout, or a child rollout naming this thread as parent) and stops the turn at once.
//!
//! Here a fake codex emits `thread.started`, writes a rollout holding a `spawn_agent` call, then
//! hangs: with sub-agents off the guard stops the run and records it `failed: ... spawned a sub-agent
//! ...`, class `operator` (a tooling failure, not the reviewer's quota or capability), the partial
//! kept. An opted-in run (`--codex-config features.multi_agent=true`) is NOT stopped by the guard.
//!
//! Nothing reaches a real provider or intake. Windows only (the fake codex is a `.cmd` wrapper
//! around a PowerShell script, as the plugin's `fake-codex3.cmd`).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

/// The `c3` under test: this build's, or - `C3_TEST_EXE` - a copy of it.
fn c3_bin() -> PathBuf {
    std::env::var_os("C3_TEST_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_c3")))
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir();
    let tmp = std::fs::canonicalize(&tmp)
        .map(|c| {
            let s = c.to_string_lossy().to_string();
            PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
        })
        .unwrap_or(tmp);
    let p = tmp.join(format!("c3-{tag}-{}-{nanos}", std::process::id()));
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

/// A fake codex 0.169.0: `--version`, `login status`, and an exec turn that emits `thread.started`,
/// writes its rollout under `CODEX_HOME/sessions/<y>/<m>/<d>`, then hangs (the guard or the run's
/// timeout stops it). Every `spawn_agent` line carries an RFC3339 `timestamp` like a real rollout.
/// Env knobs model a resumed thread (F11-4):
///   FAKE_CODEX_OLD_SPAWN=1          pre-write a spawn_agent call stamped 30 min ago (prior turn).
///   FAKE_CODEX_NO_CURRENT_SPAWN=1   do NOT write a current-turn spawn (the current turn is clean).
///   FAKE_CODEX_NEW_SPAWN_AFTER_MS=n after writing the above and sleeping n ms, append a current-turn
///                                   spawn (so the guard polls once seeing only history, then fires).
/// With no knobs it writes one current-turn spawn immediately (the plain guard-stop case).
const FAKE_PS1: &str = r#"$ErrorActionPreference = 'Stop'
$raw = [string]$env:FAKE_CODEX_ARGS
if ($raw -match '--version') { Write-Output 'codex-cli 0.169.0-fake'; exit 0 }
if ($raw -match '^\s*login\s+status(\s|$)') { [Console]::Error.WriteLine('Logged in using ChatGPT'); exit 0 }
$null = [Console]::In.ReadToEnd()
$tid = [guid]::NewGuid().ToString()
[Console]::Out.Write("{""type"":""thread.started"",""thread_id"":""$tid""}`n")
[Console]::Out.Write("{""type"":""turn.started""}`n")
[Console]::Out.Flush()
$ch = $env:CODEX_HOME
$now = Get-Date
$day = Join-Path $ch ("sessions/{0:yyyy}/{0:MM}/{0:dd}" -f $now)
New-Item -ItemType Directory -Force -Path $day | Out-Null
$stamp = "{0:yyyy-MM-ddTHH-mm-ss}" -f $now
$roll = Join-Path $day "rollout-$stamp-$tid.jsonl"
function SpawnLine([datetime]$at) {
  $z = $at.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
  return '{"timestamp":"' + $z + '","type":"response_item","payload":{"type":"function_call","name":"spawn_agent","namespace":"collaboration","arguments":"{}"}}'
}
$meta = '{"type":"session_meta","payload":{"id":"' + $tid + '","source":"exec"}}'
$lines = $meta
if ($env:FAKE_CODEX_OLD_SPAWN) { $lines = $lines + "`n" + (SpawnLine $now.AddMinutes(-30)) }
if (-not $env:FAKE_CODEX_NO_CURRENT_SPAWN -and -not $env:FAKE_CODEX_NEW_SPAWN_AFTER_MS) {
  $lines = $lines + "`n" + (SpawnLine $now)
}
[IO.File]::WriteAllText($roll, $lines + "`n")
if ($env:FAKE_CODEX_NEW_SPAWN_AFTER_MS) {
  Start-Sleep -Milliseconds ([int]$env:FAKE_CODEX_NEW_SPAWN_AFTER_MS)
  [IO.File]::AppendAllText($roll, (SpawnLine (Get-Date)) + "`n")
}
Start-Sleep -Seconds 60
exit 0
"#;

const CODEX_TOML: &str = "model = \"gpt-5.1\"\n";

fn prepare(work: &Path) {
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
    std::fs::create_dir_all(work.join("userhome")).unwrap();
    std::fs::write(work.join("fake-codex.cmd"), FAKE_CMD).unwrap();
    std::fs::write(work.join("fake-codex.ps1"), FAKE_PS1).unwrap();
}

fn c3_cmd(work: &Path, env: &[(&str, &str)]) -> Command {
    let repo = work.join("repo");
    let home = work.join("home");
    let fake = work.join("fake-codex.cmd");
    let mut cmd = Command::new(c3_bin());
    for (k, _) in std::env::vars() {
        if k.starts_with("CODEX_CONSULT_")
            || k.starts_with("FAKE_")
            || k.starts_with("C3_")
            || k == "CLAUDE_PLUGIN_ROOT"
        {
            cmd.env_remove(&k);
        }
    }
    cmd.current_dir(&repo)
        .env("CODEX_HOME", &home)
        .env("HOME", work.join("userhome"))
        .env("USERPROFILE", work.join("userhome"))
        .env("CODEX_CONSULT_EXE", &fake)
        .env("CODEX_CONSULT_ROSTER", "none")
        .env("CODEX_CONSULT_HEALTH", "none")
        .env("CODEX_CONSULT_TELEMETRY", "off")
        .env("C3_TELEMETRY_HUB", "http://127.0.0.1:9/T")
        .env("C3_PRIORS", "off")
        .env("CODEX_CONSULT_TEST_MODE", "1")
        // the guard shares the budget watch's poll - shorten it so the test is quick
        .env("CODEX_CONSULT_TEST_BUDGET_POLL_MS", "200");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd
}

/// Run one `c3 consult` to completion with `args` (after the task/prompt/reply-name) and its text.
fn run_consult(work: &Path, args: &[&str]) -> (Option<i32>, String) {
    run_consult_env(work, args, &[])
}

/// As [`run_consult`], with extra process env (the fake-codex `FAKE_CODEX_*` knobs).
fn run_consult_env(work: &Path, args: &[&str], env: &[(&str, &str)]) -> (Option<i32>, String) {
    let out = c3_cmd(work, env)
        .args([
            "consult",
            "--task",
            "t",
            "--prompt",
            "x",
            "--reply-name",
            "sg",
            "--stall-sec",
            "0",
        ])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run c3 consult");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

/// The last codex consult entry of the task ledger.
fn last_consult(work: &Path) -> Value {
    let sessions = work
        .join("repo")
        .join(".collab")
        .join("t")
        .join("sessions.json");
    let v: Value = serde_json::from_slice(&std::fs::read(&sessions).unwrap()).unwrap();
    v["codex"]["consults"]
        .as_array()
        .and_then(|a| a.last())
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn the_guard_stops_a_run_that_spawns_a_sub_agent_and_records_it_operator() {
    let work = scratch("subguard-stop");
    prepare(&work);
    let (code, text) = run_consult(&work, &["--timeout-sec", "30", "--continue-sec", "0"]);
    assert_eq!(code, Some(1), "a failed run exits 1: {text}");
    assert!(
        text.contains("spawned a sub-agent although sub-agents are off")
            && text.contains("codex-cli 0.169.0-fake")
            && text.contains("the partial is kept"),
        "the outcome names the guard and the codex version: {text}"
    );
    let entry = last_consult(&work);
    assert!(
        entry["bridge_outcome"]
            .as_str()
            .is_some_and(|b| b.contains("spawned a sub-agent although sub-agents are off")),
        "the ledger records the guard stop: {entry}"
    );
    assert_eq!(
        entry["provider_failure"]["class"], "operator",
        "classed operator (not quota, not capability): {entry}"
    );
    assert_eq!(
        entry["subagents_allowed"], false,
        "the run had sub-agents off: {entry}"
    );
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn an_opted_in_run_is_not_stopped_by_the_guard() {
    let work = scratch("subguard-optin");
    prepare(&work);
    // the user opts in: the bridge passes none of its off switches and the guard is off; the fake's
    // spawn_agent rollout does NOT stop the run (it runs to its own timeout instead)
    let (code, text) = run_consult(
        &work,
        &[
            "--timeout-sec",
            "4",
            "--continue-sec",
            "0",
            "--codex-config",
            "features.multi_agent=true",
        ],
    );
    assert_eq!(code, Some(1), "{text}");
    assert!(
        !text.contains("spawned a sub-agent"),
        "the guard did not fire on an opted-in run: {text}"
    );
    assert!(
        text.contains("timeout"),
        "the opted-in run ended on its own timeout, not the guard: {text}"
    );
    let entry = last_consult(&work);
    assert_eq!(
        entry["subagents_allowed"], true,
        "the opted-in run had sub-agents allowed: {entry}"
    );
    let _ = std::fs::remove_dir_all(&work);
}

// ------------------------------- (F11-4) a resumed thread's historical spawn does not stop the run

#[test]
fn a_resumed_thread_with_an_old_spawn_call_is_not_stopped() {
    // (F11-4) The thread's rollout already holds a spawn_agent call from a PRIOR turn (30 min ago),
    // but the current turn spawns nothing. The guard polls the rollout at least once; the historical
    // call (timestamp before the turn launched) must NOT stop the run, which ends on its own timeout.
    let work = scratch("subguard-resumed-old");
    prepare(&work);
    let (code, text) = run_consult_env(
        &work,
        &["--timeout-sec", "3", "--continue-sec", "0"],
        &[
            ("FAKE_CODEX_OLD_SPAWN", "1"),
            ("FAKE_CODEX_NO_CURRENT_SPAWN", "1"),
        ],
    );
    assert_eq!(code, Some(1), "the run ends (on timeout): {text}");
    assert!(
        !text.contains("spawned a sub-agent"),
        "a historical spawn call alone must not stop the run: {text}"
    );
    assert!(
        text.contains("timeout"),
        "the run survived the guard and ended on its own timeout: {text}"
    );
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn a_resumed_thread_that_makes_a_new_spawn_is_stopped() {
    // (F11-4) The rollout holds an old spawn call AND the current turn makes a NEW one shortly after
    // launch (after the guard has already polled once seeing only history). The new call stops the
    // run - operator class, partial kept - proving the scope bounds the scan, it does not disable it.
    let work = scratch("subguard-resumed-new");
    prepare(&work);
    let (code, text) = run_consult_env(
        &work,
        &["--timeout-sec", "30", "--continue-sec", "0"],
        &[
            ("FAKE_CODEX_OLD_SPAWN", "1"),
            ("FAKE_CODEX_NEW_SPAWN_AFTER_MS", "800"),
        ],
    );
    assert_eq!(code, Some(1), "a guard-stopped run exits 1: {text}");
    assert!(
        text.contains("spawned a sub-agent although sub-agents are off")
            && text.contains("the partial is kept"),
        "the new spawn stops the run: {text}"
    );
    let entry = last_consult(&work);
    assert_eq!(
        entry["provider_failure"]["class"], "operator",
        "classed operator: {entry}"
    );
    let _ = std::fs::remove_dir_all(&work);
}
