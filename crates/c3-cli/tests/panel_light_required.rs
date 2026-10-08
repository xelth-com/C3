//! (F04-3, RC3) A required light reviewer on a weighty purpose, as a REAL panel run against a fake
//! codex CLI (not a dry run), and every `panel.members` record of the ledger it writes.
//!
//! The roster is harness-panel's LIGHT one: #1 ZAI glm-5.3 (weighty), #2 ZAI glm-5.3-flash
//! (`panel: light`), #3 openai gpt-5.1. On `acceptance` without `-Require` the light #2 is held back
//! (its sibling #1 runs); `-Require #2` lifts the light gate, so #2 runs - and the plugin records it
//! `run` with an empty reason in every member's `panel.members` (it mutates the shared member
//! objects before recording). Before F04-3 C3 recorded the selection's earlier state: `skipped`
//! with the light-gate reason, while the member ran.
//!
//! Windows only: the fake codex is a `.cmd` wrapper around a PowerShell script, as the plugin's
//! own `fake-codex3.cmd` (`docs/port/fake-clis.md`). It lives in `c3-cli` so cargo builds the
//! binary with the test's own feature set and hands its path through `CARGO_BIN_EXE_c3`; a panel
//! starts its members with that same binary.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;

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

/// The fake codex (a trimmed `fake-codex3`): `--version`, `login status` (the preflight), and an
/// exec turn that reads the prompt from stdin, prints thread.started / turn.started /
/// turn.completed and copies `FAKE_CODEX_REPLY` to the `-o` file.
const FAKE_CMD: &str = "@echo off\r\nset \"FAKE_CODEX_ARGS=%*\"\r\npowershell -NoProfile -ExecutionPolicy Bypass -File \"%~dp0fake-codex.ps1\"\r\nexit /b %ERRORLEVEL%\r\n";
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

const ROSTER_LIGHT: &str = r#"{"roster_version":1,"reviewers":[{"provider":"ZAI","model":"glm-5.3","panel":"weighty","context_tokens":32000},{"provider":"ZAI","model":"glm-5.3-flash","panel":"light"},{"provider":"openai","model":"gpt-5.1"}]}"#;

/// harness-panel's scratch Codex config: the built-in openai (model gpt-5.1) plus ZAI.
const CODEX_TOML: &str = "model = \"gpt-5.1\"\n\n[model_providers.ZAI]\nbase_url = \"https://api.z.ai/api/v1\"\nenv_key = \"RT_ZAI_KEY\"\nwire_api = \"responses\"\n";

#[test]
fn required_light_reviewer_is_recorded_run_in_every_member_record() {
    let work = scratch("panel-light-required");
    let repo = work.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@e.com"]);
    git(&repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("app.txt"), "one\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    std::fs::create_dir_all(repo.join(".collab").join("t").join("handoffs")).unwrap();

    let home = work.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), CODEX_TOML).unwrap();
    let fake = work.join("fake-codex.cmd");
    std::fs::write(&fake, FAKE_CMD).unwrap();
    std::fs::write(work.join("fake-codex.ps1"), FAKE_PS1).unwrap();
    let reply = work.join("advise.json");
    std::fs::write(&reply, ADVISE).unwrap();
    let roster = work.join("roster-light.json");
    std::fs::write(&roster, ROSTER_LIGHT).unwrap();

    let mut cmd = Command::new(c3_bin());
    // the run sees only this test's switches: no inherited bridge, fake or C3 variable
    for (k, _) in std::env::vars() {
        if k.starts_with("CODEX_CONSULT_") || k.starts_with("FAKE_") || k.starts_with("C3_") {
            cmd.env_remove(&k);
        }
    }
    let out = cmd
        .current_dir(&repo)
        .env("CODEX_HOME", &home)
        .env("RT_ZAI_KEY", "zai-test-key")
        .env("CODEX_CONSULT_EXE", &fake)
        .env("CODEX_CONSULT_ROSTER", &roster)
        .env("CODEX_CONSULT_TELEMETRY", "off")
        .env("C3_PRIORS", "off")
        .env("FAKE_CODEX_REPLY", &reply)
        .args(["consult", "--task", "t", "--panel", "--prompt", "x"])
        .args([
            "--purpose",
            "acceptance",
            "--require",
            "#2",
            "--reply-name",
            "lr",
        ])
        .arg("--codex-exe")
        .arg(&fake)
        .output()
        .expect("run c3 consult --panel");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "exit {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status.code()
    );
    // the displayed plan: the required light entry takes the first seat beside its sibling
    assert!(
        stdout.contains("  #2 ZAI :: glm-5.3-flash - member, n=1, handoff 01, required\n"),
        "{stdout}"
    );

    let sessions = repo.join(".collab").join("t").join("sessions.json");
    let ledger: Value =
        serde_json::from_str(&std::fs::read_to_string(&sessions).expect("sessions.json")).unwrap();
    let consults = ledger["codex"]["consults"].as_array().expect("consults");
    assert_eq!(consults.len(), 3, "three members ran: {consults:#?}");
    for c in consults {
        assert_eq!(
            c["bridge_outcome"], "usable reply",
            "{}",
            c["reviewer"]["model"]
        );
        let members = c["panel"]["members"].as_array().expect("panel.members");
        let shown: Vec<(String, String, String)> = members
            .iter()
            .map(|m| {
                (
                    m["model"].as_str().unwrap_or_default().to_string(),
                    m["state"].as_str().unwrap_or_default().to_string(),
                    m["reason"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let want: Vec<(String, String, String)> = ["glm-5.3", "glm-5.3-flash", "gpt-5.1"]
            .iter()
            .map(|m| (m.to_string(), "run".to_string(), String::new()))
            .collect();
        assert_eq!(
            shown, want,
            "panel.members of the {} member",
            c["reviewer"]["model"]
        );
        // nothing was skipped (the member's `roster.skipped`, from the panel's skipped record):
        // the light gate held no required entry back
        assert_eq!(
            c["roster"]["skipped"],
            serde_json::json!([]),
            "roster.skipped of {}",
            c["reviewer"]["model"]
        );
    }
    let _ = std::fs::remove_dir_all(&work);
}
