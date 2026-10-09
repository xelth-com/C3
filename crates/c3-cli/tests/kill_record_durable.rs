//! (wave 3c, F23-3, RC3) The recovery record a kill keeps is on disk AT the kill: a real `c3
//! consult` against a fake codex that hangs is killed on its timeout, and the bridge itself is
//! terminated inside `CODEX_CONSULT_TEST_KILL_PAUSE_MS` - the pause held right after the kill site's
//! record write, before anything else of the run (the ledger, the commit, the end-of-run record).
//! The record left behind must already name what the kill left: the unknown tree of an unconfirmed
//! kill (`kill_unconfirmed`), a descendant it could not verify (`unverified[]`). Before wave 3c C3
//! wrote that evidence only at the end of the run - a bridge dying in between left the `running`
//! record naming only the killed child.
//!
//! Nothing reaches a real provider or intake. Windows only (the fake codex is a `.cmd` wrapper
//! around a PowerShell script, as the plugin's `fake-codex3.cmd`).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

/// A fresh scratch directory under the system temp dir.
fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!("c3-{tag}-{}-{nanos}", std::process::id()));
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

/// A trimmed fake-codex3: `--version`, `login status`, and an exec turn that prints
/// `thread.started` / `turn.started` and hangs (the run's timeout kills it).
const FAKE_PS1: &str = r#"$ErrorActionPreference = 'Stop'
$raw = [string]$env:FAKE_CODEX_ARGS
if ($raw -match '--version') { Write-Output 'codex-cli 0.155.1-fake'; exit 0 }
if ($raw -match '^\s*login\s+status(\s|$)') { [Console]::Error.WriteLine('Logged in using ChatGPT'); exit 0 }
$null = [Console]::In.ReadToEnd()
$tid = [guid]::NewGuid().ToString()
[Console]::Out.Write("{""type"":""thread.started"",""thread_id"":""$tid""}`n")
[Console]::Out.Write("{""type"":""turn.started""}`n")
[Console]::Out.Flush()
Start-Sleep -Seconds 60
exit 0
"#;

const CODEX_TOML: &str = "model = \"gpt-5.1\"\n";

/// Runs one consultation that is killed on its 3 s timeout with `hooks`, waits until the recovery
/// record says `survivors` (the kill site wrote it; the run is held in the pause after it),
/// terminates the bridge there and returns the record it left - and the repo, to check that the
/// run never got further.
fn kill_the_bridge_after_the_kill(tag: &str, hooks: &[(&str, &str)]) -> (Value, PathBuf) {
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
    std::fs::create_dir_all(work.join("userhome")).unwrap();
    let fake = work.join("fake-codex.cmd");
    std::fs::write(&fake, FAKE_CMD).unwrap();
    std::fs::write(work.join("fake-codex.ps1"), FAKE_PS1).unwrap();

    let mut cmd = Command::new(c3_bin());
    // the run sees only this test's switches: no inherited bridge, fake or C3 variable
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
        .env("CODEX_CONSULT_TEST_KILL_PAUSE_MS", "120000");
    for (k, v) in hooks {
        cmd.env(k, v);
    }
    let mut child = cmd
        .args([
            "consult",
            "--task",
            "t",
            "--prompt",
            "x",
            "--reply-name",
            "kd",
        ])
        .args([
            "--timeout-sec",
            "3",
            "--continue-sec",
            "0",
            "--stall-sec",
            "0",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("run c3 consult");

    let path = repo.join(".collab").join("t").join(".consult.pending.json");
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut last = Value::Null;
    while Instant::now() < deadline {
        if let Some(v) = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        {
            if v["state"] == "survivors" {
                last = v;
                break;
            }
            last = v;
        }
        if let Ok(Some(st)) = child.try_wait() {
            panic!("the run ended ({st}) before its record said survivors: {last}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // the bridge dies inside the pause after the kill site's write (TerminateProcess: no unwinding,
    // no end-of-run write)
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(
        last["state"], "survivors",
        "the kill site wrote no record: {last}"
    );
    // what is on disk after the bridge died
    let record: Value = serde_json::from_slice(&std::fs::read(&path).expect("the record stays"))
        .expect("a whole record");
    (record, repo)
}

/// The run got no further than the kill: nothing was committed.
fn assert_nothing_committed(repo: &Path) {
    let sessions = repo.join(".collab").join("t").join("sessions.json");
    let consults = std::fs::read_to_string(&sessions)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["codex"]["consults"].as_array().map(|a| a.len()))
        .unwrap_or(0);
    assert_eq!(consults, 0, "the run was terminated before its commit");
}

#[test]
fn an_unconfirmed_kill_leaves_its_unknown_tree_on_disk_when_the_bridge_dies() {
    let (r, repo) = kill_the_bridge_after_the_kill(
        "killrec-unconfirmed",
        &[("CODEX_CONSULT_TEST_KILL_UNCONFIRMED", "1")],
    );
    assert_eq!(r["state"], "survivors", "{r}");
    assert_eq!(
        r["kill_unconfirmed"], "a test hook forced the kill unconfirmed",
        "{r}"
    );
    assert_eq!(r["survivors"], serde_json::json!([]), "{r}");
    assert_eq!(r["unverified"], serde_json::json!([]), "{r}");
    // the record still names the run (its writer and its killed child)
    assert!(r["pid"].as_u64().unwrap_or(0) > 0, "{r}");
    assert!(r["child_pid"].as_u64().unwrap_or(0) > 0, "{r}");
    assert_nothing_committed(&repo);
}

#[test]
fn an_unverified_descendant_is_on_disk_when_the_bridge_dies() {
    // a live process the main turn's kill is told it could not verify
    let mut helper = Command::new("ping")
        .args(["127.0.0.1", "-n", "60"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let pid = helper.id();
    let (r, repo) = kill_the_bridge_after_the_kill(
        "killrec-unverified",
        &[("CODEX_CONSULT_TEST_UNVERIFIED", &pid.to_string())],
    );
    let _ = helper.kill();
    let _ = helper.wait();
    assert_eq!(r["state"], "survivors", "{r}");
    assert_eq!(
        r["unverified"],
        serde_json::json!([{ "pid": pid, "why": format!("start time of pid {pid} unreadable") }]),
        "{r}"
    );
    assert!(r.get("kill_unconfirmed").is_none(), "{r}");
    assert_eq!(r["survivors"], serde_json::json!([]), "{r}");
    assert_nothing_committed(&repo);
}
