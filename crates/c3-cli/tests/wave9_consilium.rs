//! (wave 9) Three of the consilium-panel defects, end to end through the real `c3` binary against a
//! fake codex `.cmd` launcher:
//!
//! - D1: a panel starts NEW threads for every member by default; `--mode fork` forks and the fork
//!   guard still refuses a heavy parent;
//! - D2: the detach "come back" line names `c3 consult --task ... --status/--wait`, never a `.ps1`;
//! - D3: two detached single runs of one task run side by side (the task lock is held for the
//!   bookkeeping only) and land in the ledger with distinct `n`.
//!
//! Windows only (the fake codex is a `.cmd` around a PowerShell script).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use serde_json::Value;

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!("c3-w9-{tag}-{}-{nanos}", std::process::id()));
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
if ($env:FAKE_CODEX_SLEEP) { Start-Sleep -Seconds ([int]$env:FAKE_CODEX_SLEEP) }
if ($o -and $env:FAKE_CODEX_REPLY) { [IO.File]::Copy($env:FAKE_CODEX_REPLY, $o, $true) }
[Console]::Out.Write("{""type"":""turn.completed"",""usage"":{""input_tokens"":1000,""cached_input_tokens"":200,""cache_write_input_tokens"":0,""output_tokens"":300,""reasoning_output_tokens"":40}}`n")
exit 0
"#;

const ADVISE: &str = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
const ROSTER: &str =
    r#"{"roster_version":1,"reviewers":[{"provider":"openai","model":"gpt-5.1"}]}"#;

struct Env {
    work: PathBuf,
    repo: PathBuf,
    home: PathBuf,
    fake: PathBuf,
    reply: PathBuf,
    roster: PathBuf,
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
    std::fs::create_dir_all(repo.join(".collab").join("t").join("handoffs")).unwrap();
    let home = work.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "model = \"gpt-5.1\"\n").unwrap();
    let fake = work.join("fake-codex.cmd");
    std::fs::write(&fake, FAKE_CMD).unwrap();
    std::fs::write(work.join("fake-codex.ps1"), FAKE_PS1).unwrap();
    let reply = work.join("advise.json");
    std::fs::write(&reply, ADVISE).unwrap();
    let roster = work.join("roster.json");
    std::fs::write(&roster, ROSTER).unwrap();
    Env {
        work,
        repo,
        home,
        fake,
        reply,
        roster,
    }
}

impl Env {
    fn cmd(&self) -> Command {
        let mut cmd = Command::new(c3_bin());
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_") || k.starts_with("FAKE_") || k.starts_with("C3_") {
                cmd.env_remove(&k);
            }
        }
        cmd.current_dir(&self.repo)
            .env("CODEX_HOME", &self.home)
            .env("CODEX_CONSULT_EXE", &self.fake)
            .env("CODEX_CONSULT_ROSTER", &self.roster)
            .env("CODEX_CONSULT_TELEMETRY", "off")
            .env("CODEX_CONSULT_TEST_MODE", "1")
            .env("C3_PRIORS", "off")
            .env("FAKE_CODEX_REPLY", &self.reply);
        cmd
    }

    fn run(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let mut cmd = self.cmd();
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.args(args)
            .arg("--codex-exe")
            .arg(&self.fake)
            .output()
            .expect("run c3")
    }

    fn consults(&self) -> Vec<Value> {
        let p = self.repo.join(".collab").join("t").join("sessions.json");
        let v: Value = match std::fs::read_to_string(&p) {
            Ok(t) => serde_json::from_str(&t).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        };
        v["codex"]["consults"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
}

/// Seconds since the epoch of an ISO timestamp with an offset (0 when unreadable).
fn chrono_secs(s: &str) -> f64 {
    c3_core::quota::when_offset(&Value::String(s.to_string()))
        .map(|t| t.timestamp_millis() as f64 / 1000.0)
        .unwrap_or(0.0)
}

fn text(o: &Output) -> String {
    format!(
        "exit {:?}\nstdout:\n{}\nstderr:\n{}",
        o.status.code(),
        String::from_utf8_lossy(&o.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&o.stderr)
    )
}

const PANEL: [&str; 7] = [
    "consult",
    "--task",
    "t",
    "--panel",
    "--prompt",
    "x",
    "--purpose",
];

fn panel_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut v: Vec<&str> = PANEL.to_vec();
    v.push("checkpoint");
    v.extend_from_slice(extra);
    v
}

// D1: a panel on a task with existing threads starts NEW threads; --mode fork forks, and the
// fork guard still refuses a heavy parent.
#[test]
fn a_panel_starts_new_threads_by_default_and_forks_only_when_asked() {
    let e = setup("d1");
    // panel A creates the first thread of the lineage
    let a = e.run(&panel_args(&["--reply-name", "a"]), &[]);
    assert_eq!(a.status.code(), Some(0), "{}", text(&a));
    let first = e.consults();
    assert_eq!(first.len(), 1, "{first:#?}");
    let t1 = first[0]["thread"].as_str().unwrap_or("").to_string();
    assert!(
        !t1.is_empty(),
        "the first panel recorded a verified thread: {first:#?}"
    );
    let stdout_a = String::from_utf8_lossy(&a.stdout).replace("\r\n", "\n");
    assert!(
        stdout_a.contains("  mode: new - every member starts a new thread"),
        "{stdout_a}"
    );

    // panel B, a new brief, no --mode: new threads, no parent
    let b = e.run(&panel_args(&["--reply-name", "b"]), &[]);
    assert_eq!(b.status.code(), Some(0), "{}", text(&b));
    let all = e.consults();
    assert_eq!(all.len(), 2, "{all:#?}");
    assert_eq!(all[1]["mode"], "new", "{:#?}", all[1]);
    assert_eq!(
        all[1]["parent_thread"].as_str().unwrap_or(""),
        "",
        "{:#?}",
        all[1]
    );

    // panel C, --mode fork: forks the newest thread
    let c = e.run(&panel_args(&["--reply-name", "c", "--mode", "fork"]), &[]);
    assert_eq!(c.status.code(), Some(0), "{}", text(&c));
    let all = e.consults();
    assert_eq!(all.len(), 3, "{all:#?}");
    assert_eq!(all[2]["mode"], "fork", "{:#?}", all[2]);
    assert_eq!(
        all[2]["parent_thread"].as_str().unwrap_or(""),
        all[1]["thread"].as_str().unwrap_or("x"),
        "{:#?}",
        all[2]
    );

    // panel D, --mode fork with a 1 KB cap over a 4 KB parent rollout: the guard refuses the heavy
    // parent (no new ledger entry)
    let parent = all[2]["thread"].as_str().unwrap_or("").to_string();
    assert!(!parent.is_empty(), "{all:#?}");
    let dir = e.home.join("sessions").join("2026").join("01").join("01");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("rollout-2026-01-01T00-00-00-{parent}.jsonl")),
        "x".repeat(4096),
    )
    .unwrap();
    let d = e.run(
        &panel_args(&["--reply-name", "d", "--mode", "fork"]),
        &[("CODEX_CONSULT_FORK_MAX_KB", "1")],
    );
    let t = text(&d);
    assert!(
        t.contains("use -Mode new, or raise CODEX_CONSULT_FORK_MAX_KB (now 1)"),
        "{t}"
    );
    assert_eq!(
        e.consults().len(),
        3,
        "the refused member wrote no entry: {t}"
    );
    let _ = std::fs::remove_dir_all(&e.work);
}

// D2 + D3: two detached single runs of one task, a reviewer that takes a few seconds: both start
// within seconds (no lock queue), both land in the ledger with distinct n; the come-back line
// speaks c3.
#[test]
fn detached_single_runs_of_one_task_run_side_by_side_and_speak_c3() {
    let e = setup("d3");
    let t0 = Instant::now();
    // a long enough turn that even a slow foreground start (a loaded machine) overlaps it
    let envs = [("FAKE_CODEX_SLEEP", "25")];
    let (a, b) = std::thread::scope(|sc| {
        let ha = sc.spawn(|| {
            e.run(
                &[
                    "consult",
                    "--task",
                    "t",
                    "--detach",
                    "--prompt",
                    "x",
                    "--reply-name",
                    "a",
                ],
                &envs,
            )
        });
        let hb = sc.spawn(|| {
            e.run(
                &[
                    "consult",
                    "--task",
                    "t",
                    "--detach",
                    "--prompt",
                    "y",
                    "--reply-name",
                    "b",
                ],
                &envs,
            )
        });
        (ha.join().unwrap(), hb.join().unwrap())
    });
    assert_eq!(a.status.code(), Some(0), "{}", text(&a));
    assert_eq!(b.status.code(), Some(0), "{}", text(&b));
    let out_a = String::from_utf8_lossy(&a.stdout).replace("\r\n", "\n");
    assert!(
        out_a.contains("c3 consult --task t --status --id "),
        "{out_a}"
    );
    assert!(out_a.contains("--wait --id "), "{out_a}");
    assert!(!out_a.contains(".ps1"), "{out_a}");
    // both reach the ledger; two 6 s turns in sequence would need > 12 s after the starts
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if e.consults().len() >= 2 || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let all = e.consults();
    assert_eq!(all.len(), 2, "both detached runs committed: {all:#?}");
    let ns: Vec<i64> = all.iter().map(|c| c["n"].as_i64().unwrap_or(0)).collect();
    assert!(ns[0] != ns[1] && ns.iter().all(|n| *n > 0), "{ns:?}");
    // the two runs' [when, finished_at] intervals overlap: neither waited for the other's turn
    let span = |c: &Value| {
        let w = chrono_secs(c["when"].as_str().unwrap_or(""));
        let f = chrono_secs(c["finished_at"].as_str().unwrap_or(""));
        (w, f)
    };
    let (s0, s1) = (span(&all[0]), span(&all[1]));
    assert!(
        s0.0 < s1.1 && s1.0 < s0.1,
        "the runs did not overlap: {s0:?} {s1:?} (took {:.1} s)",
        t0.elapsed().as_secs_f64()
    );
    for c in &all {
        assert_eq!(c["bridge_outcome"], "usable reply", "{c:#?}");
    }
    let _ = std::fs::remove_dir_all(&e.work);
}
