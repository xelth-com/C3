//! (wave 3c, F23-3, RC3; wave 3e, F30-2 / F30-3) The recovery record of a kill is on disk AT the
//! kill: a real `c3 consult` against a fake codex that hangs is killed on its timeout, and the bridge
//! itself is terminated inside `CODEX_CONSULT_TEST_KILL_PAUSE_MS` - placed by
//! `CODEX_CONSULT_TEST_KILL_PAUSE_AT` either right after the record's write after the kill (`write`,
//! before anything else of the run: the ledger, the commit, the end-of-run record) or right after
//! the kill and BEFORE its result is written (`kill`), at the main turn, the timeout continuation or
//! the format repair. The record left behind must already name the run's tree: what the kill left
//! (the unknown tree of an unconfirmed kill, a descendant it could not verify) or - the bridge dead
//! before the kill's result was written - the record written BEFORE the kill (state `survivors`, the
//! root and its descendants with their start times, `kill_unconfirmed`). A record path that cannot
//! be written ends the run refused, nothing committed, the warning naming the path.
//!
//! (Wave 7, F30-2 failure path, RC2) A record that cannot be replaced: the record before the kill
//! goes into its fallback file `.consult.pending-kill.json` (read by the recovery as any record), and
//! with the bridge terminated right after the kill, a process of the killed tree still running and
//! the file access restored, the next run is refused - from the fallback and the launch record, and
//! (the fallback blocked too) from the launch record alone.
//!
//! Nothing reaches a real provider or intake. Windows only (the fake codex is a `.cmd` wrapper
//! around a PowerShell script, as the plugin's `fake-codex3.cmd`).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// The `c3` under test: this build's, or - `C3_TEST_EXE` - a copy of it (a target directory that
/// several checkouts share can replace or lock `target/debug/c3.exe` while the tests run).
fn c3_bin() -> PathBuf {
    std::env::var_os("C3_TEST_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_c3")))
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
/// `thread.started` / `turn.started` and then, by `FAKE_MAIN` (a new turn) or `FAKE_RESUME` (a
/// `resume <thread>` turn): `prose` - copies a prose reply to the `-o` file and ends the turn;
/// anything else - hangs (the run's timeout kills it).
const FAKE_PS1: &str = r#"$ErrorActionPreference = 'Stop'
$raw = [string]$env:FAKE_CODEX_ARGS
if ($raw -match '--version') { Write-Output 'codex-cli 0.155.1-fake'; exit 0 }
if ($raw -match '^\s*login\s+status(\s|$)') { [Console]::Error.WriteLine('Logged in using ChatGPT'); exit 0 }
$null = [Console]::In.ReadToEnd()
$tid = [guid]::NewGuid().ToString()
$resume = $false
if ($raw -match ' resume ([0-9a-fA-F-]{36})') { $resume = $true; $tid = $Matches[1] }
[Console]::Out.Write("{""type"":""thread.started"",""thread_id"":""$tid""}`n")
[Console]::Out.Write("{""type"":""turn.started""}`n")
[Console]::Out.Flush()
$mode = $(if ($resume) { [string]$env:FAKE_RESUME } else { [string]$env:FAKE_MAIN })
if ($mode -eq 'prose') {
    $o = ''
    if ($raw -match '(?:^| )-o (\S+)') { $o = $Matches[1] }
    $words = @(1..40 | ForEach-Object { "word$_" }) -join ' '
    $text = "1. The record is written before the kill and updated after it; $words.`n2. A write that fails ends the run refused and nothing is committed; $words.`n3. Every kill site is covered; $words. Verdict: OK.`n"
    [IO.File]::WriteAllText($o, $text)
    [Console]::Out.Write("{""type"":""turn.completed"",""usage"":{""input_tokens"":1,""cached_input_tokens"":0,""output_tokens"":1}}`n")
    exit 0
}
if ($env:FAKE_PID_FILE) { [IO.File]::WriteAllText($env:FAKE_PID_FILE, [string]$PID) }
$sleep = 60
if ($env:FAKE_SLEEP_SEC) { $sleep = [int]$env:FAKE_SLEEP_SEC }
Start-Sleep -Seconds $sleep
exit 0
"#;

const CODEX_TOML: &str = "model = \"gpt-5.1\"\n";

/// One running `c3 consult` against the fake codex.
struct Run {
    child: Child,
    work: PathBuf,
    repo: PathBuf,
    /// The run's recovery record.
    path: PathBuf,
}

/// Starts one consultation in a fresh repository with `args` (after the task, prompt and reply
/// name) and the environment `env` (the fake's modes, the test hooks).
fn start(tag: &str, args: &[&str], env: &[(&str, &str)], capture: bool) -> Run {
    let work = scratch(tag);
    prepare(&work);
    launch(&work, args, env, capture)
}

/// The scratch repository, the codex home and the fake codex under `work`.
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
    let fake = work.join("fake-codex.cmd");
    std::fs::write(&fake, FAKE_CMD).unwrap();
    std::fs::write(work.join("fake-codex.ps1"), FAKE_PS1).unwrap();
}

/// Starts one consultation in the repository `prepare` made under `work` (a first one, or the next
/// one after an interrupted run).
fn launch(work: &Path, args: &[&str], env: &[(&str, &str)], capture: bool) -> Run {
    let repo = work.join("repo");
    let home = work.join("home");
    let fake = work.join("fake-codex.cmd");
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
        .env("CODEX_CONSULT_TEST_MODE", "1");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let (out, err) = if capture {
        (Stdio::piped(), Stdio::piped())
    } else {
        (Stdio::null(), Stdio::null())
    };
    let child = cmd
        .args([
            "consult",
            "--task",
            "t",
            "--prompt",
            "x",
            "--reply-name",
            "kd",
            "--stall-sec",
            "0",
        ])
        .args(args)
        .stdout(out)
        .stderr(err)
        .spawn()
        .expect("run c3 consult");
    let path = repo.join(".collab").join("t").join(".consult.pending.json");
    Run {
        child,
        work: work.to_path_buf(),
        repo,
        path,
    }
}

fn read_record(path: &Path) -> Option<Value> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
}

/// Polls the run's record until `want` holds (the run must not end first).
fn wait_record(run: &mut Run, what: &str, want: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut last = Value::Null;
    while Instant::now() < deadline {
        if let Some(v) = read_record(&run.path) {
            if want(&v) {
                return v;
            }
            last = v;
        }
        if let Ok(Some(st)) = run.child.try_wait() {
            panic!("the run ended ({st}) before its record showed {what}: {last}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("no record showed {what} within 90 s: {last}");
}

/// The bridge dies here (TerminateProcess: no unwinding, no further write); returns the record it
/// left on disk.
fn kill_the_bridge(run: &mut Run) -> Value {
    let _ = run.child.kill();
    let _ = run.child.wait();
    read_record(&run.path).expect("the record stays, whole")
}

/// Whether a process with this pid runs.
fn alive(pid: u64) -> bool {
    c3::liveness::proc::process_start_iso(pid as u32).is_some()
}

/// Waits until the pid no longer runs (the kill went through).
fn wait_gone(pid: u64) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while alive(pid) {
        assert!(Instant::now() < deadline, "pid {pid} outlived its kill");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The record written BEFORE a kill (its note says the kill is in progress).
fn is_pre_kill(v: &Value) -> bool {
    v["note"]
        .as_str()
        .is_some_and(|n| n.contains("is in progress; its result is not recorded yet"))
}

/// The run got no further than the kill: nothing was committed.
fn assert_nothing_committed(repo: &Path) {
    let sessions = repo.join(".collab").join("t").join("sessions.json");
    let consults = std::fs::read_to_string(&sessions)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["codex"]["consults"].as_array().map(|a| a.len()))
        .unwrap_or(0);
    assert_eq!(consults, 0, "the run was stopped before its commit");
}

/// A main turn killed on its 3 s timeout with `hooks`, the bridge held after the record's write
/// after the kill (and terminated there): the record it left.
fn killed_after_the_record_of_the_main_kill(tag: &str, hooks: &[(&str, &str)]) -> (Value, PathBuf) {
    let mut env = vec![
        ("CODEX_CONSULT_TEST_KILL_PAUSE_MS", "120000"),
        ("CODEX_CONSULT_TEST_KILL_PAUSE_AT", "write,main"),
    ];
    env.extend_from_slice(hooks);
    let mut run = start(
        tag,
        &["--timeout-sec", "3", "--continue-sec", "0"],
        &env,
        false,
    );
    // the record written before the kill comes first; wait for the one written after it
    wait_record(&mut run, "the record of the kill's result", |v| {
        v["state"] == "survivors" && !is_pre_kill(v)
    });
    let r = kill_the_bridge(&mut run);
    (r, run.repo)
}

#[test]
fn an_unconfirmed_kill_leaves_its_unknown_tree_on_disk_when_the_bridge_dies() {
    let (r, repo) = killed_after_the_record_of_the_main_kill(
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
    let (r, repo) = killed_after_the_record_of_the_main_kill(
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

/// The record written before `turn`'s kill, its root `child_pid` killed: the shape every
/// pre-write fixture checks (the root first in `survivors[]` with its start time and name, the
/// pre-kill `kill_unconfirmed` and note).
fn assert_the_record_written_before_the_kill(r: &Value, turn: &str) -> u64 {
    assert_eq!(r["state"], "survivors", "{r}");
    let root = r["child_pid"]
        .as_u64()
        .expect("the record names the killed child");
    assert_eq!(
        r["kill_unconfirmed"],
        format!("the kill of the {turn} (pid {root}) started and its result was never recorded"),
        "{r}"
    );
    assert_eq!(
        r["note"],
        format!(
            "the kill of the {turn} (pid {root}) is in progress; its result is not recorded yet"
        ),
        "{r}"
    );
    let survivors = r["survivors"].as_array().expect("survivors[]");
    assert_eq!(survivors[0]["pid"], root, "the root first: {r}");
    assert_eq!(survivors[0]["name"], "cmd", "the .cmd launcher: {r}");
    assert!(
        survivors
            .iter()
            .all(|s| s["start_time"].as_str().is_some_and(|t| !t.is_empty())),
        "every entry with its start time: {r}"
    );
    // the descendants of the one enumeration: the fake's PowerShell below the launcher
    assert!(
        survivors.iter().any(|s| s["name"] == "powershell"),
        "the launcher's descendants are named: {r}"
    );
    root
}

#[test]
fn a_bridge_that_dies_before_the_main_kills_result_is_written_leaves_the_record_written_before_it()
{
    // (F30-2 / F30-3, RC2) the pause right after the kill, BEFORE its result is written
    let mut run = start(
        "killrec-prewrite-main",
        &["--timeout-sec", "3", "--continue-sec", "0"],
        &[
            ("CODEX_CONSULT_TEST_KILL_PAUSE_MS", "120000"),
            ("CODEX_CONSULT_TEST_KILL_PAUSE_AT", "kill,main"),
        ],
        false,
    );
    let pre = wait_record(&mut run, "the record written before the kill", is_pre_kill);
    // the kill went through (the bridge now holds in the pause after it)
    for s in pre["survivors"].as_array().unwrap() {
        wait_gone(s["pid"].as_u64().unwrap());
    }
    let r = kill_the_bridge(&mut run);
    assert_eq!(r, pre, "nothing was written after the kill");
    assert_the_record_written_before_the_kill(&r, "main turn");
    assert_nothing_committed(&run.repo);
}

#[test]
fn a_bridge_that_dies_during_the_continuations_kill_leaves_its_record() {
    // (F30-3) the timeout continuation: the main turn killed (clean - its record put back), the
    // continuation registered with ITS child, then killed: the record written before that kill
    let mut run = start(
        "killrec-prewrite-continuation",
        &["--timeout-sec", "3", "--continue-sec", "3"],
        &[
            ("CODEX_CONSULT_TEST_KILL_PAUSE_MS", "120000"),
            ("CODEX_CONSULT_TEST_KILL_PAUSE_AT", "kill,continuation"),
        ],
        false,
    );
    let main = wait_record(&mut run, "the main turn running", |v| {
        v["state"] == "running" && v["child_pid"].as_u64().is_some()
    });
    let main_child = main["child_pid"].as_u64().unwrap();
    let pre = wait_record(&mut run, "the continuation's record before its kill", |v| {
        is_pre_kill(v)
            && v["note"]
                .as_str()
                .is_some_and(|n| n.contains("the timeout continuation"))
    });
    for s in pre["survivors"].as_array().unwrap() {
        wait_gone(s["pid"].as_u64().unwrap());
    }
    let r = kill_the_bridge(&mut run);
    let root = assert_the_record_written_before_the_kill(&r, "timeout continuation");
    assert_ne!(root, main_child, "the continuation's own child: {r}");
    assert!(!alive(main_child));
    assert_nothing_committed(&run.repo);
}

#[test]
fn a_bridge_that_dies_during_the_format_repairs_kill_leaves_its_record() {
    // (F30-3) the format repair: a prose reply, its repair turn hangs and is killed on its timeout
    // (min(--timeout-sec, 300) s): the record written before that kill names the repair's child
    // AND the saved prose
    let mut run = start(
        "killrec-prewrite-repair",
        &["--timeout-sec", "4", "--continue-sec", "0"],
        &[
            ("FAKE_MAIN", "prose"),
            ("CODEX_CONSULT_TEST_KILL_PAUSE_MS", "120000"),
            ("CODEX_CONSULT_TEST_KILL_PAUSE_AT", "kill,repair"),
        ],
        false,
    );
    let pre = wait_record(&mut run, "the repair's record before its kill", |v| {
        is_pre_kill(v)
            && v["note"]
                .as_str()
                .is_some_and(|n| n.contains("the format repair"))
    });
    for s in pre["survivors"].as_array().unwrap() {
        wait_gone(s["pid"].as_u64().unwrap());
    }
    let r = kill_the_bridge(&mut run);
    assert_the_record_written_before_the_kill(&r, "format repair");
    let original = r["original"].as_str().expect("the saved prose is named");
    assert!(original.ends_with("original.md"), "{r}");
    assert!(run.repo.join(original).is_file(), "{r}");
    assert_eq!(
        r["first_reply"], "usable prose (format repair in progress)",
        "{r}"
    );
    assert_nothing_committed(&run.repo);
}

#[test]
fn a_kill_record_that_cannot_be_written_ends_the_run_refused() {
    // (F30-2 / F30-3) the record path read-only once the main turn runs and - wave 7 - the
    // fallback file blocked (test hook): the write before the kill, its retry into the fallback,
    // the one after the kill and the end of the run's all fail - the run ends refused, the warning
    // names the record, nothing is committed
    let mut run = start(
        "killrec-readonly",
        &["--timeout-sec", "3", "--continue-sec", "3"],
        &[("CODEX_CONSULT_TEST_KILL_FALLBACK_FAIL", "1")],
        true,
    );
    let running = wait_record(&mut run, "the main turn running", |v| {
        v["state"] == "running" && v["child_pid"].as_u64().is_some()
    });
    let child = running["child_pid"].as_u64().unwrap();
    let mut perms = std::fs::metadata(&run.path).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&run.path, perms).unwrap();
    let out = run.child.wait_with_output().expect("the run ends");
    let mut perms = std::fs::metadata(&run.path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    let _ = std::fs::set_permissions(&run.path, perms);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "refused: {stderr}");
    assert!(
        stderr.contains(
            "the kill's evidence is not on disk: the unconfirmed kill could not be recorded ("
        ) && stderr.contains(&format!(
            ".consult.pending.json still names only child pid {child}"
        )) && stderr.contains("This run is not committed"),
        "{stderr}"
    );
    // no continuation ran on it (the kill is not confirmed), nothing was committed, the record on
    // disk is the one the write could not replace
    assert!(!stdout.contains("one continuation turn"), "{stdout}");
    assert_nothing_committed(&run.repo);
    let r = read_record(&run.path).unwrap();
    assert_eq!(r["state"], "running", "{r}");
    assert_eq!(r["child_pid"], child, "{r}");
}

/// The record path made read-only (`true`) or writable again.
fn set_readonly(path: &Path, on: bool) {
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(on);
    std::fs::set_permissions(path, perms).unwrap();
}

/// A process this test left running is stopped when the test ends, however it ends.
struct Reaper(Option<u64>);

impl Drop for Reaper {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// The run's text (stdout and stderr).
fn output_text(o: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn a_kill_record_that_cannot_be_written_is_kept_in_its_fallback() {
    // (wave 7, F30-2) the record path read-only once the main turn runs: the write before the kill
    // is retried ONCE into `.consult.pending-kill.json`, which keeps the record written before the
    // kill; the evidence is on disk, so the run is not refused - the kill is not confirmed (no
    // continuation), the outcome warns naming the fallback, the run commits
    let mut run = start(
        "killrec-fallback",
        &["--timeout-sec", "3", "--continue-sec", "3"],
        &[],
        true,
    );
    let running = wait_record(&mut run, "the main turn running", |v| {
        v["state"] == "running" && v["child_pid"].as_u64().is_some()
    });
    let child = running["child_pid"].as_u64().unwrap();
    set_readonly(&run.path, true);
    let out = run.child.wait_with_output().expect("the run ends");
    set_readonly(&run.path, false);
    let text = output_text(&out);
    let fallback = run.path.with_file_name(".consult.pending-kill.json");
    assert!(
        text.contains("the unconfirmed kill could not be recorded (")
            && text.contains(&format!(
                "{} keeps the record written before the kill (its tree unknown",
                fallback.display()
            ))
            && text.contains(&format!(
                ".consult.pending.json still names only child pid {child}"
            )),
        "{text}"
    );
    assert!(!text.contains("This run is not committed"), "{text}");
    assert!(!text.contains("one continuation turn"), "{text}");
    let r = read_record(&fallback).expect("the fallback keeps the record written before the kill");
    assert_the_record_written_before_the_kill(&r, "main turn");
    assert_eq!(r["child_pid"], child, "{r}");
    let p = read_record(&run.path).unwrap();
    assert_eq!(p["state"], "running", "{p}");
    let sessions = run.repo.join(".collab").join("t").join("sessions.json");
    let ledger: Value = serde_json::from_slice(&std::fs::read(sessions).unwrap()).unwrap();
    let last = ledger["codex"]["consults"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["kill_confirmed"], false, "{last}");
    let _ = std::fs::remove_dir_all(&run.work);
}

/// (Wave 7, F30-2 failure path, RC2) The read-only fixture with the bridge terminated in the pause
/// right after the MAIN kill (the kill's result never written), on a host where the kill cannot
/// enumerate the children (`CODEX_CONSULT_TEST_KILL_DENIED=1`: only the launcher is stopped, the
/// fake's PowerShell below it survives - the surviving helper). `fallback_ok`: the record before
/// the kill went into the fallback file; else the fallback is blocked too (test hook) and only the
/// launch record (`running`, child pid and start) is on disk. File access restored, the next run of
/// the task must be refused while the helper runs - and, `fallback_ok`, the fallback is consumed
/// once it no longer does.
fn rc2_recovery_after_a_lost_kill_record(tag: &str, fallback_ok: bool) {
    let work = scratch(tag);
    prepare(&work);
    let pid_file = work.join("helper.pid");
    let pid_file_s = pid_file.to_string_lossy().to_string();
    let mut env = vec![
        ("CODEX_CONSULT_TEST_KILL_PAUSE_MS", "120000"),
        ("CODEX_CONSULT_TEST_KILL_PAUSE_AT", "kill,main"),
        ("CODEX_CONSULT_TEST_KILL_DENIED", "1"),
        ("FAKE_SLEEP_SEC", "600"),
        ("FAKE_PID_FILE", pid_file_s.as_str()),
    ];
    if !fallback_ok {
        env.push(("CODEX_CONSULT_TEST_KILL_FALLBACK_FAIL", "1"));
    }
    let mut run = launch(
        &work,
        &["--timeout-sec", "3", "--continue-sec", "0"],
        &env,
        false,
    );
    let mut reaper = Reaper(None);
    let launch_rec = wait_record(&mut run, "the main turn running", |v| {
        v["state"] == "running" && v["child_pid"].as_u64().is_some()
    });
    let root = launch_rec["child_pid"].as_u64().unwrap();
    set_readonly(&run.path, true);
    let fallback = run.path.with_file_name(".consult.pending-kill.json");
    if fallback_ok {
        // the record before the kill, in the fallback file
        let deadline = Instant::now() + Duration::from_secs(90);
        while read_record(&fallback).is_none() {
            assert!(Instant::now() < deadline, "no fallback record within 90 s");
            if let Ok(Some(st)) = run.child.try_wait() {
                panic!("the run ended ({st}) before its fallback record");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    // the kill went through (the bridge now holds in the pause right after it)
    wait_gone(root);
    std::thread::sleep(Duration::from_millis(1000));
    let _ = run.child.kill();
    let _ = run.child.wait();
    set_readonly(&run.path, false);
    let helper: u64 = std::fs::read_to_string(&pid_file)
        .expect("the fake wrote its pid")
        .trim()
        .parse()
        .unwrap();
    reaper.0 = Some(helper);
    assert!(alive(helper), "the helper survived the kill");
    // what is on disk: the launch record, unchanged; the fallback (or nothing)
    let left = read_record(&run.path).expect("the launch record stays");
    assert_eq!(left, launch_rec, "nothing replaced the launch record");
    if fallback_ok {
        let r = read_record(&fallback).unwrap();
        assert_eq!(r["state"], "survivors", "{r}");
        assert_eq!(r["survivors"][0]["pid"], root, "{r}");
        assert_eq!(
            r["kill_unconfirmed"],
            format!(
                "the kill of the main turn (pid {root}) started and its result was never recorded"
            ),
            "{r}"
        );
    } else {
        assert!(!fallback.exists(), "the fallback was blocked");
    }
    assert_nothing_committed(&run.repo);
    // the next run of the task: refused while the helper runs - the dry run names every record
    let dry = launch(&work, &["--dry-run"], &[("FAKE_MAIN", "prose")], true)
        .child
        .wait_with_output()
        .unwrap();
    let dry_text = output_text(&dry);
    assert!(
        dry_text.contains(".consult.pending.json (state 'running'")
            && dry_text.contains("the next run would be REFUSED"),
        "{dry_text}"
    );
    if fallback_ok {
        assert!(
            dry_text.contains(".consult.pending-kill.json (state 'survivors'"),
            "{dry_text}"
        );
        assert_eq!(
            dry_text.matches("the next run would be REFUSED").count(),
            2,
            "both records refuse: {dry_text}"
        );
    }
    let next = launch(
        &work,
        &["--timeout-sec", "30"],
        &[("FAKE_MAIN", "prose")],
        true,
    )
    .child
    .wait_with_output()
    .unwrap();
    let next_text = output_text(&next);
    assert_eq!(next.status.code(), Some(1), "{next_text}");
    assert!(
        next_text.contains("an interrupted consultation (state 'running'")
            && next_text.contains(&format!("pid {helper} ")),
        "refused for the surviving helper: {next_text}"
    );
    assert_nothing_committed(&run.repo);
    assert_eq!(read_record(&run.path).unwrap(), launch_rec);
    if fallback_ok {
        assert!(fallback.exists(), "the fallback stays while it refuses");
        // the helper gone: both records are consumed, the fallback removed, the run goes on
        drop(reaper);
        wait_gone(helper);
        let ok = launch(
            &work,
            &["--timeout-sec", "30"],
            &[("FAKE_MAIN", "prose")],
            true,
        )
        .child
        .wait_with_output()
        .unwrap();
        let ok_text = output_text(&ok);
        if ok.status.code() == Some(1) && ok_text.contains("task not verifiable") {
            // a real codex process of this machine, started meanwhile, holds the machine-wide rule
            // (fail-closed, as designed): the release cannot be shown here
            eprintln!("release not shown - a codex-like process of this machine runs: {ok_text}");
            let _ = std::fs::remove_dir_all(&work);
            return;
        }
        assert_eq!(ok.status.code(), Some(0), "{ok_text}");
        assert!(
            ok_text.contains(".consult.pending-kill.json: state 'survivors'"),
            "{ok_text}"
        );
        assert!(!fallback.exists(), "the consumed fallback is removed");
    }
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn rc2_a_lost_kill_record_leaves_its_fallback_and_the_next_run_is_refused() {
    rc2_recovery_after_a_lost_kill_record("killrec-rc2-fallback", true);
}

#[test]
fn rc2_a_kill_record_lost_with_its_fallback_is_refused_from_the_launch_record_alone() {
    rc2_recovery_after_a_lost_kill_record("killrec-rc2-launch", false);
}
