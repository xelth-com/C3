//! (wave 2b of the compatibility track) The pre-0.6 parity gaps the RC2 triage found, end to end
//! through the real `c3` binary against a fake codex `.cmd` launcher - the Rust mirror of the
//! plugin's harness checks:
//!
//! - fixes28b TESTMODE D10: the test-mode line on the console and once in `warnings[]` (a dry run
//!   too), and no `CODEX_CONSULT_TEST_*` variable in the engine child or a launcher probe;
//! - fixes28b CONTEXT / engines RUN argv: a batch launcher gets a plain argument as it is
//!   (`-c k=v`, not `-c "k=v"`), a quoted one with every `"` doubled;
//! - fixes27c HEALTH D7, fixes28b HEALTH D13, fixes28c JOURNAL D10: the machine-wide health
//!   journal at a failed commit, the retry after it, an orphan replayed by the next run, a torn
//!   line moved to `<journal>.bad`;
//! - pending (e): a registration failure's outcome is not overwritten by `codex exit 1`;
//! - fixes27c KICK D2: a kick during the timeout continuation cancels it, the timeout outcome
//!   stays;
//! - fixes27c STREAM D6: a stall cut names the open tool call;
//! - host EXPLAIN / HOOK: `--explain`, `--task` required otherwise, the hook's pointer line.
//!
//! Nothing reaches a real provider or intake. Windows only (the fake codex is a `.cmd` wrapper
//! around a PowerShell script, as the plugin's `fake-codex3.cmd`).
#![cfg(windows)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
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
    let p = std::env::temp_dir().join(format!("c3-w2b-{tag}-{}-{nanos}", std::process::id()));
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

/// A trimmed fake-codex3: `--version`, `login status`, and an exec turn. `FAKE_CODEX_LOG` gets the
/// raw `%*` text; `FAKE_CODEX_ENV_DUMP=<dir>` gets `<kind>-<pid>.env` with the names of the
/// CODEX_* variables the process inherited; `FAKE_CODEX_HANG=1` hangs a new turn after
/// `thread.started`, `FAKE_CODEX_HANG_RESUME=1` hangs a `resume` turn; `FAKE_CODEX_TOOL_OPEN=1`
/// opens a command_execution item (item_9) and hangs; otherwise `FAKE_CODEX_REPLY` is copied to
/// the `-o` file.
const FAKE_PS1: &str = r#"$ErrorActionPreference = 'Stop'
$raw = [string]$env:FAKE_CODEX_ARGS
if ($env:FAKE_CODEX_ENV_DUMP) {
    $kind = $(if ($raw -match '--version') { 'version' } elseif ($raw -match '^\s*login\s+status(\s|$)') { 'login' } else { 'exec' })
    $names = @(Get-ChildItem env: | Where-Object { $_.Name -like 'CODEX_*' } | Sort-Object Name | ForEach-Object { $_.Name })
    [IO.File]::WriteAllText((Join-Path $env:FAKE_CODEX_ENV_DUMP "$kind-$PID.env"), (($names -join "`n") + "`n"))
}
if ($raw -match '--version') { Write-Output 'codex-cli 0.155.1-fake'; exit 0 }
if ($raw -match '^\s*login\s+status(\s|$)') { [Console]::Error.WriteLine('Logged in using ChatGPT'); exit 0 }
if ($env:FAKE_CODEX_LOG) { [IO.File]::AppendAllText($env:FAKE_CODEX_LOG, "ARGS: $raw`n") }
$o = $null
if ($raw -match '(?:^| )-o (\S+)') { $o = $Matches[1] }
$null = [Console]::In.ReadToEnd()
$isResume = ($raw -match ' resume ')
$tid = [guid]::NewGuid().ToString()
[Console]::Out.Write("{""type"":""thread.started"",""thread_id"":""$tid""}`n")
[Console]::Out.Write("{""type"":""turn.started""}`n")
[Console]::Out.Flush()
if ($env:FAKE_CODEX_TOOL_OPEN -eq '1') {
    [Console]::Out.Write("{""type"":""item.started"",""item"":{""id"":""item_9"",""type"":""command_execution"",""command"":""long build"",""status"":""in_progress""}}`n")
    [Console]::Out.Flush()
    Start-Sleep -Seconds 60
}
if ($env:FAKE_CODEX_HANG -eq '1' -and -not $isResume) { Start-Sleep -Seconds 60 }
if ($env:FAKE_CODEX_HANG_RESUME -eq '1' -and $isResume) { Start-Sleep -Seconds 60 }
if ($o -and $env:FAKE_CODEX_REPLY) { [IO.File]::Copy($env:FAKE_CODEX_REPLY, $o, $true) }
[Console]::Out.Write("{""type"":""turn.completed"",""usage"":{""input_tokens"":1000,""cached_input_tokens"":200,""cache_write_input_tokens"":0,""output_tokens"":300,""reasoning_output_tokens"":40}}`n")
exit 0
"#;

const ADVISE: &str = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;

const CODEX_TOML: &str = "model = \"gpt-5.1\"\n\n[model_providers.ZAI]\nbase_url = \"https://api.z.ai/api/v1\"\nenv_key = \"RT_ZAI_KEY\"\nwire_api = \"responses\"\n";

const TEST_MODE_LINE: &str = "test mode is ON: test hooks are honoured";

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
    /// A `c3` command in the repo with only this test's switches (no inherited bridge, fake or C3
    /// variable), telemetry off, no health file unless `env` names one.
    fn cmd(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
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
        cmd.current_dir(&self.repo)
            .env("CODEX_HOME", &self.home)
            .env("HOME", self.work.join("userhome"))
            .env("USERPROFILE", self.work.join("userhome"))
            .env("RT_ZAI_KEY", "zai-test-key")
            .env("CODEX_CONSULT_EXE", &self.fake)
            .env("CODEX_CONSULT_ROSTER", "none")
            .env("CODEX_CONSULT_HEALTH", "none")
            .env("CODEX_CONSULT_TELEMETRY", "off")
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
        cmd.args(args);
        cmd
    }

    fn c3(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        self.cmd(args, env).output().expect("run c3")
    }

    fn consult(&self, extra: &[&str], env: &[(&str, &str)]) -> Output {
        let mut args = vec!["consult", "--task", "t", "--prompt", "x"];
        args.extend_from_slice(extra);
        self.c3(&args, env)
    }

    fn ledger(&self) -> Vec<Value> {
        let p = self.repo.join(".collab").join("t").join("sessions.json");
        let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        v["codex"]["consults"].as_array().unwrap().clone()
    }

    fn last(&self) -> Value {
        self.ledger().last().cloned().unwrap()
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

fn warnings(e: &Value) -> Vec<String> {
    e["warnings"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn count_lines(t: &str, line: &str) -> usize {
    t.lines().filter(|l| *l == line).count()
}

// ------------------------------------------------------------------ TESTMODE (fixes28b D10)

#[test]
fn test_mode_is_said_and_never_handed_to_an_engine_child() {
    let e = setup("testmode");
    let dump = e.work.join("dump");
    std::fs::create_dir_all(&dump).unwrap();
    let d = dump.to_string_lossy().to_string();
    let x = e.consult(
        &["--reply-name", "tm"],
        &[
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("CODEX_CONSULT_TEST_XYZ", "x"),
            ("FAKE_CODEX_ENV_DUMP", &d),
        ],
    );
    let t = text(&x);
    assert_eq!(x.status.code(), Some(0), "{t}");
    assert_eq!(
        count_lines(&t, &format!("WARNING: {TEST_MODE_LINE}")),
        1,
        "{t}"
    );
    let w = warnings(&e.last());
    assert_eq!(
        w.iter().filter(|s| *s == TEST_MODE_LINE).count(),
        1,
        "{w:?}"
    );
    // the engine child and the launcher probes saw no CODEX_CONSULT_TEST_* variable
    let mut kinds: Vec<String> = Vec::new();
    for f in std::fs::read_dir(&dump).unwrap().flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        kinds.push(name.split('-').next().unwrap_or("").to_string());
        let body = std::fs::read_to_string(f.path()).unwrap();
        assert!(
            !body
                .lines()
                .any(|l| l.to_ascii_uppercase().starts_with("CODEX_CONSULT_TEST_")),
            "{name}: {body}"
        );
        // the bridge's own other variables are kept (the fake's launcher override)
        assert!(
            body.lines().any(|l| l == "CODEX_CONSULT_EXE"),
            "{name}: {body}"
        );
    }
    assert!(kinds.iter().any(|k| k == "exec"), "{kinds:?}");
    assert!(
        kinds.iter().any(|k| k == "version" || k == "login"),
        "{kinds:?}"
    );

    // without test mode: no such line
    let y = e.consult(&["--reply-name", "tm0"], &[]);
    let ty = text(&y);
    assert_eq!(y.status.code(), Some(0), "{ty}");
    assert!(!ty.contains("test mode is ON"), "{ty}");
    assert!(!warnings(&e.last()).iter().any(|s| s == TEST_MODE_LINE));

    // a dry run says it once and its preview's warnings[] holds it
    let z = e.consult(
        &["--reply-name", "tm1", "--dry-run"],
        &[("CODEX_CONSULT_TEST_MODE", "1")],
    );
    let tz = text(&z);
    assert_eq!(z.status.code(), Some(0), "{tz}");
    assert_eq!(
        count_lines(&tz, &format!("WARNING: {TEST_MODE_LINE}")),
        1,
        "{tz}"
    );
    assert!(tz.contains(&format!("\"{TEST_MODE_LINE}\"")), "{tz}");
}

// ------------------------------------------------------------------ the launcher quoting

#[test]
fn a_batch_launcher_gets_plain_arguments_as_they_are() {
    let e = setup("quoting");
    let log = e.work.join("fake.log");
    let l = log.to_string_lossy().to_string();
    let x = e.consult(
        &[
            "--reply-name",
            "q",
            "--codex-config",
            "foo_plain=256000",
            "--codex-config",
            "foo_text=\"a b\"",
        ],
        &[("FAKE_CODEX_LOG", &l)],
    );
    assert_eq!(x.status.code(), Some(0), "{}", text(&x));
    let raw = std::fs::read_to_string(&log).unwrap();
    // `=` alone keeps an argument plain (Rust std would send `-c "foo_plain=256000"`)
    assert!(raw.contains(" -c foo_plain=256000 "), "{raw}");
    // spaces and quotes: quoted, every `"` doubled (the plugin's ConvertTo-ProcArg form)
    assert!(raw.contains(" -c \"foo_text=\"\"a b\"\"\" "), "{raw}");
    // the model override is a plain `-c` pair too
    assert!(!raw.contains("\"-c\""), "{raw}");
}

// ------------------------------------------------------------------ the health journal

fn health_text(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn health_endpoints(p: &Path) -> Vec<Value> {
    let t = health_text(p);
    let v: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap_or(Value::Null);
    v["endpoints"].as_array().cloned().unwrap_or_default()
}

#[test]
fn the_health_journal_at_a_failed_commit_and_the_retry_after_it() {
    let e = setup("journal");
    let hp = e.work.join("health.json");
    let h = hp.to_string_lossy().to_string();
    let x = e.consult(
        &["--reply-name", "hr"],
        &[
            ("CODEX_CONSULT_HEALTH", &h),
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("CODEX_CONSULT_TEST_HEALTH_FAIL_FIRST", "1"),
        ],
    );
    let t = text(&x);
    assert_eq!(x.status.code(), Some(0), "{t}");
    let want = "machine-wide health not updated at the commit (lock timeout (test hook CODEX_CONSULT_TEST_HEALTH_FAIL_FIRST)); the record is kept in the journal; a retry follows the commit";
    assert!(warnings(&e.last()).iter().any(|w| w == want), "{t}");
    assert!(
        t.lines().any(|l| l
            == "health     : machine-wide health updated by the retry after the commit (the journal applied)"),
        "{t}"
    );
    assert_eq!(health_endpoints(&hp).len(), 1, "{}", health_text(&hp));
    let jp = e.work.join("health.json.journal");
    assert!(health_text(&jp).trim().is_empty(), "{}", health_text(&jp));
}

#[test]
fn an_orphan_is_replayed_and_a_torn_line_kept_aside() {
    let e = setup("orphan");
    let hp = e.work.join("health.json");
    let jp = e.work.join("health.json.journal");
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string();
    // the plugin's own journal line (ConvertTo-Json -Compress of its record), then a torn append
    let orphan = format!(
        r#"{{"endpoint":"fp-crash","class":"quota","kind":"","until":null,"retry_after":null,"repo":"C:\\repo-died","when":"{now}","message":"m"}}"#
    );
    let torn = r#"{"endpoint":"fp-x","cl"#;
    std::fs::write(&jp, format!("{orphan}\n{torn}\n")).unwrap();
    let h = hp.to_string_lossy().to_string();
    let x = e.consult(&["--reply-name", "jr"], &[("CODEX_CONSULT_HEALTH", &h)]);
    let t = text(&x);
    assert_eq!(x.status.code(), Some(0), "{t}");
    let eps = health_endpoints(&hp);
    assert_eq!(
        eps.iter().filter(|v| v["endpoint"] == "fp-crash").count(),
        1,
        "{}",
        health_text(&hp)
    );
    assert_eq!(std::fs::metadata(&jp).unwrap().len(), 0);
    let bad = health_text(&e.work.join("health.json.journal.bad"));
    let bl: Vec<&str> = bad.lines().collect();
    assert_eq!(bl.len(), 1, "{bad}");
    assert!(bl[0].ends_with(&format!("\t{torn}")), "{bad}");
    let want = format!(
        "health journal: 1 unreadable line(s) kept in {}.bad",
        jp.display()
    );
    assert!(warnings(&e.last()).contains(&want), "{t}");
    assert!(t.contains(&want), "{t}");
}

#[test]
fn a_missing_health_directory_is_named_and_the_journal_too() {
    let e = setup("nodir");
    let missing = e.work.join("no-such-dir").join("health.json");
    let m = missing.to_string_lossy().to_string();
    let x = e.consult(&["--reply-name", "hm"], &[("CODEX_CONSULT_HEALTH", &m)]);
    let t = text(&x);
    assert_eq!(x.status.code(), Some(0), "{t}");
    let cause = format!(
        "the directory {} does not exist",
        missing.parent().unwrap().display()
    );
    let w = warnings(&e.last());
    let at_commit: Vec<&String> = w
        .iter()
        .filter(|s| {
            s.starts_with(&format!(
                "machine-wide health not updated at the commit ({cause}); the journal could not be written ("
            )) && s.ends_with("); a retry follows the commit")
        })
        .collect();
    assert_eq!(at_commit.len(), 1, "{w:?}");
    let line = format!(
        "warning    : machine-wide health not updated by the retry after the commit ({cause})"
    );
    assert!(t.lines().any(|l| l == line), "{t}");
}

// ------------------------------------------------------------------ pending (e)

#[test]
fn a_registration_failure_keeps_its_outcome() {
    let e = setup("register");
    let x = e.consult(
        &["--reply-name", "inj"],
        &[
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("CODEX_CONSULT_TEST_REGISTER_FAIL", "1"),
            ("FAKE_CODEX_HANG", "1"),
        ],
    );
    assert_eq!(x.status.code(), Some(1), "{}", text(&x));
    let outcome = e.last()["bridge_outcome"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert_eq!(
        outcome,
        "failed: could not register the codex process (injected: registration write failed); codex was stopped"
    );
}

// ------------------------------------------------------------------ KICK D2 / STREAM D6

#[test]
fn a_kick_during_the_timeout_continuation_cancels_it() {
    let e = setup("kickcont");
    let mut child = e
        .cmd(
            &[
                "consult",
                "--task",
                "t",
                "--prompt",
                "x",
                "--reply-name",
                "kc",
                "--timeout-sec",
                "4",
                "--continue-sec",
                "60",
                "--stall-sec",
                "0",
            ],
            &[
                ("CODEX_CONSULT_TEST_MODE", "1"),
                ("FAKE_CODEX_HANG", "1"),
                ("FAKE_CODEX_HANG_RESUME", "1"),
            ],
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for l in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    // wait until the continuation turn is announced, then give it a moment to start
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut seen = String::new();
    let mut cont = false;
    while Instant::now() < deadline {
        if let Ok(l) = rx.recv_timeout(Duration::from_millis(200)) {
            seen.push_str(&l);
            seen.push('\n');
            if l.contains("one continuation turn on thread") {
                cont = true;
                break;
            }
        }
    }
    assert!(cont, "no continuation announced: {seen}");
    std::thread::sleep(Duration::from_millis(1500));
    let mut kicked = None;
    let kdeadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < kdeadline {
        let k = e.c3(
            &["consult", "--task", "t", "--kick", "--member", "01"],
            &[("CODEX_CONSULT_TEST_MODE", "1")],
        );
        if k.status.code() == Some(0) {
            kicked = Some(text(&k));
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let status = child.wait().unwrap();
    let _ = reader.join();
    assert!(kicked.is_some(), "the kick was never taken: {seen}");
    assert_eq!(status.code(), Some(1));
    let last = e.last();
    assert_eq!(
        last["bridge_outcome"], "failed: timeout after 4 s (process tree killed)",
        "{last}"
    );
    assert!(
        last["timeout_continue"]["outcome"]
            .as_str()
            .unwrap_or("")
            .contains("stopped by the operator (-Kick)"),
        "{}",
        last["timeout_continue"]
    );
    assert_ne!(last["provider_failure"]["class"], "operator");
    assert!(warnings(&last).iter().any(|w| w
        == "kick: the operator stopped the timeout continuation (-Kick); the timeout outcome and its salvage stay"));
}

#[test]
fn a_stall_cut_names_the_open_tool_call() {
    let e = setup("toolcap");
    let x = e.consult(
        &[
            "--reply-name",
            "tc",
            "--stall-sec",
            "3",
            "--continue-sec",
            "0",
        ],
        &[
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("CODEX_CONSULT_TEST_TOOL_CAP_SEC", "4"),
            ("FAKE_CODEX_TOOL_OPEN", "1"),
        ],
    );
    assert_eq!(x.status.code(), Some(1), "{}", text(&x));
    let last = e.last();
    let o = last["bridge_outcome"].as_str().unwrap_or("");
    let re = regex::Regex::new(r"^failed: stalled after 3 s without an event - no output for \d+ s \(a tool call open for \d+ s: codex command_execution item_9\) \(process tree killed\)$").unwrap();
    assert!(re.is_match(o), "{o}");
    assert_eq!(last["stall"]["seconds"], 3);
}

// ------------------------------------------------------------------ EXPLAIN / -Task / HOOK

#[test]
fn explain_and_the_task_requirement() {
    let e = setup("explain");
    let plugin = e.work.join("plugin");
    std::fs::create_dir_all(plugin.join("skills").join("coordinate")).unwrap();
    std::fs::write(
        plugin.join("skills").join("coordinate").join("SKILL.md"),
        "---\nname: coordinate\n---\n\n# The rules\nrun ${CLAUDE_PLUGIN_ROOT}/scripts/x\n",
    )
    .unwrap();
    let p = plugin.to_string_lossy().to_string();
    let env = [("CLAUDE_PLUGIN_ROOT", p.as_str())];
    let x = e.c3(&["consult", "--explain", "coordinate"], &env);
    let t = String::from_utf8_lossy(&x.stdout).to_string();
    assert_eq!(x.status.code(), Some(0), "{}", text(&x));
    assert!(
        t.starts_with("codex-consult -Explain coordinate: the coordinate skill, "),
        "{t}"
    );
    assert!(t.contains(&format!("run {p}/scripts/x")), "{t}");
    let rest = &t[t.find('\n').unwrap()..];
    assert!(!rest.contains("${CLAUDE_PLUGIN_ROOT}"), "{t}");
    let y = e.c3(&["consult", "--explain", "coordinate", "--task", "t"], &env);
    assert_eq!(y.status.code(), Some(1));
    assert!(text(&y).contains("-Explain takes no other parameter (got -Task)."));
    let b = e.c3(&["consult", "--explain", "bogus"], &env);
    assert_eq!(b.status.code(), Some(1));
    assert!(text(&b).contains("-Explain takes coordinate, consult or providers (got 'bogus')."));
    let nt = e.c3(&["consult", "--dry-run", "--prompt", "x"], &env);
    assert_eq!(nt.status.code(), Some(1));
    assert!(text(&nt).contains("codex-consult: -Task <id> is required (a slug: the task directory <CollabDir>/<id>/); the one form without it is -Explain coordinate|consult|providers."));
    assert!(!e.repo.join(".collab").exists());
}

#[test]
fn the_hook_prints_the_pointer_line() {
    let e = setup("hook");
    let x = e.c3(&["hook", "--explain-command", "the-explain-command"], &[]);
    let t = String::from_utf8_lossy(&x.stdout).replace("\r\n", "\n");
    assert_eq!(x.status.code(), Some(0));
    let lines: Vec<&str> = t.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 2, "{t}");
    assert!(lines[0].starts_with("codex-consult: "), "{t}");
    assert_eq!(
        lines[1],
        "codex-consult: coordinator rules - skill codex-consult:coordinate (or the-explain-command); telemetry: off"
    );
}
