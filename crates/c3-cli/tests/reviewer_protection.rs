//! (0.2.4, reviewer-protection) The two per-entry roster settings end to end through the installed
//! binary against fake CLIs: `ext.reserve_percent` (a PROTECTED entry held back by its budget
//! reserve) and `ext.on_request` (an entry seated only when named exactly). Every check is a
//! `--dry-run` or a pre-launch refusal, so no model turn runs.
//!
//! Windows only: the reviewers are codex custom providers on the z.ai host (known to the caps
//! catalog), so a preflight needs only the provider env key - no real codex login. A protected
//! plan's window is fed with `c3 budget set <plan> --window week --used 97% --resets <future>`.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

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

/// The fake codex: `--version`, `login status` (preflight), and an exec turn that reads the prompt
/// from stdin, prints thread/turn events and copies `FAKE_CODEX_REPLY` to the `-o` file (so a real
/// single consult records a usable reply and a thread - needed by the resume/fork continuation test).
const FAKE_CMD: &str = "@echo off\r\nset \"FAKE_CODEX_ARGS=%*\"\r\npowershell -NoProfile -ExecutionPolicy Bypass -File \"%~dp0fake-codex.ps1\"\r\nexit /b %ERRORLEVEL%\r\n";
const FAKE_PS1: &str = r#"$ErrorActionPreference = 'Stop'
$raw = [string]$env:FAKE_CODEX_ARGS
if ($raw -match '--version') { Write-Output 'codex-cli 0.155.1-fake'; exit 0 }
if ($raw -match '^\s*login\s+status(\s|$)') { [Console]::Error.WriteLine('Logged in using ChatGPT'); exit 0 }
$o = $null
if ($raw -match '(?:^| )-o (?:"([^"]+)"|(\S+))') { $o = $(if ($Matches[1]) { $Matches[1] } else { $Matches[2] }) }
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

/// A scratch Codex config: the built-in openai (gpt-5.1) plus the z.ai custom provider ZAI.
const CODEX_TOML: &str = "model = \"gpt-5.1\"\n\n[model_providers.ZAI]\nbase_url = \"https://api.z.ai/api/v1\"\nenv_key = \"RT_ZAI_KEY\"\nwire_api = \"responses\"\n";

struct Fixture {
    work: PathBuf,
    repo: PathBuf,
    home: PathBuf,
    fake: PathBuf,
    roster: PathBuf,
}

fn setup(tag: &str, roster_json: &str) -> Fixture {
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
    let roster = work.join("roster.json");
    std::fs::write(&roster, roster_json).unwrap();
    std::fs::write(work.join("advise.json"), ADVISE).unwrap();
    Fixture {
        work,
        repo,
        home,
        fake,
        roster,
    }
}

impl Fixture {
    fn cmd(&self) -> Command {
        let mut cmd = Command::new(c3_bin());
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_") || k.starts_with("FAKE_") || k.starts_with("C3_") {
                cmd.env_remove(&k);
            }
        }
        cmd.current_dir(&self.repo)
            .env("CODEX_HOME", &self.home)
            .env("RT_ZAI_KEY", "zai-test-key")
            .env("CODEX_CONSULT_EXE", &self.fake)
            .env("CODEX_CONSULT_ROSTER", &self.roster)
            .env("CODEX_CONSULT_TELEMETRY", "off")
            .env("C3_PRIORS", "off")
            .env("FAKE_CODEX_REPLY", self.work.join("advise.json"));
        cmd
    }

    /// Run `c3 <args>` and return (code, stdout, stderr).
    fn run(&self, args: &[&str]) -> (Option<i32>, String, String) {
        let out = self
            .cmd()
            .args(args)
            .arg("--codex-exe")
            .arg(&self.fake)
            .output()
            .expect("run c3");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
            String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
        )
    }

    /// Feed a plan's weekly window: used%, reset far in the future (so it is never dropped).
    fn budget_set(&self, plan: &str, used: &str) {
        let out = self
            .cmd()
            .args([
                "budget",
                "set",
                plan,
                "--window",
                "week",
                "--used",
                used,
                "--resets",
                "2099-01-01T00:00:00Z",
            ])
            .output()
            .expect("run c3 budget set");
        assert_eq!(
            out.status.code(),
            Some(0),
            "budget set failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn cleanup(self) {
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

// #1 weighty, #2 and #3 ordinary (always) - no plans, so no budget hold anywhere.
const ROSTER_ROUTE: &str = r#"{"roster_version":1,"reviewers":[
    {"provider":"ZAI","model":"glm-5.3","panel":"weighty"},
    {"provider":"ZAI","model":"glm-5.3-flash"},
    {"provider":"ZAI","model":"glm-5.2"}
]}"#;

#[test]
fn bare_diff_review_does_not_go_to_a_weighty_first_entry() {
    let f = setup("rp-bare-diff", ROSTER_ROUTE);
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--purpose",
        "diff-review",
        "--prompt",
        "x",
        "--panel-seed",
        "s1",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "stdout:\n{out}\nstderr:\n{err}");
    // --panel-seed is accepted on a bare consult and seeds the single routed pick.
    assert!(
        out.contains("routed bare default (diff-review purpose) picked"),
        "{out}"
    );
    // The weighty #1 glm-5.3 is not the pick; a non-weighty entry is.
    assert!(
        out.contains("glm-5.3-flash") || out.contains("glm-5.2"),
        "a non-weighty entry was picked: {out}"
    );
    // The line explains that #1 was not eligible (weighty on a light purpose).
    assert!(
        out.contains("#1") && out.contains("weighty"),
        "names why #1 was skipped: {out}"
    );
    f.cleanup();
}

#[test]
fn bare_acceptance_takes_the_first_available_entry() {
    let f = setup("rp-bare-acc", ROSTER_ROUTE);
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--purpose",
        "acceptance",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "stdout:\n{out}\nstderr:\n{err}");
    // Standing purpose: no routed draw, the standing partner (first entry) is chosen.
    assert!(
        !out.contains("routed bare default"),
        "acceptance must not route: {out}"
    );
    assert!(
        out.contains("reviewer    : ZAI :: glm-5.3 ") && !out.contains("glm-5.3-flash"),
        "first entry glm-5.3 chosen: {out}"
    );
    f.cleanup();
}

// #1 ordinary (no plan, never held), #2 protected on plan `zaiplan` (reserve 15).
const ROSTER_PROTECT: &str = r#"{"roster_version":1,"reviewers":[
    {"provider":"ZAI","model":"glm-5.3"},
    {"provider":"ZAI","model":"glm-5.3-flash","plan":"zaiplan","ext":{"reserve_percent":15}}
]}"#;

#[test]
fn a_held_protected_entry_is_skipped_refused_and_overridable() {
    let f = setup("rp-held", ROSTER_PROTECT);
    f.budget_set("zaiplan", "97%"); // 97% used, reset far ahead -> held (97 >= min(15,85))

    // (a) a panel on a non-standing purpose does not seat the held entry (skip kind reserve).
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--panel",
        "--purpose",
        "diff-review",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "panel dry stdout:\n{out}\nstderr:\n{err}");
    assert!(
        out.contains("glm-5.3-flash") && out.to_lowercase().contains("reserve"),
        "held entry shown with its reserve hold: {out}"
    );

    // (b) an explicit --provider/--model naming it, on a non-standing purpose, is refused before
    //     launch (exit 1, nothing started), naming the hold and --spend-reserve.
    let (code, _out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--provider",
        "ZAI",
        "--model",
        "glm-5.3-flash",
        "--purpose",
        "diff-review",
        "--prompt",
        "x",
    ]);
    assert_eq!(code, Some(1), "held refusal exit 1: {err}");
    assert!(err.contains("is held"), "names the hold: {err}");
    assert!(
        err.contains("--spend-reserve"),
        "offers the override: {err}"
    );
    assert!(
        err.contains("acceptance and core-contract"),
        "says what it is kept for: {err}"
    );

    // (c) --spend-reserve lets it through (preflight available on the dry run).
    let (code, out, _err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--provider",
        "ZAI",
        "--model",
        "glm-5.3-flash",
        "--purpose",
        "diff-review",
        "--prompt",
        "x",
        "--spend-reserve",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "spend-reserve dry: {out}");
    assert!(!out.contains("held (protected reserve)"), "{out}");

    // (d) a standing purpose (acceptance) is exempt - allowed without the override.
    let (code, out, _err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--provider",
        "ZAI",
        "--model",
        "glm-5.3-flash",
        "--purpose",
        "acceptance",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "acceptance dry: {out}");
    assert!(!out.contains("held (protected reserve)"), "{out}");

    // (e) a panel that --requires the held entry on a non-standing purpose refuses (exit 5).
    let (code, _out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--panel",
        "--purpose",
        "diff-review",
        "--require",
        "#2",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(code, Some(5), "required held entry outage exit 5: {err}");
    assert!(
        err.to_lowercase().contains("reserve") || err.contains("--spend-reserve"),
        "names the hold: {err}"
    );
    f.cleanup();
}

#[test]
fn a_held_entry_is_refused_even_with_skip_preflight() {
    // (F06-2/F07-2) the reserve hold is a budget protection, not a credential check: --skip-preflight
    // does not bypass it.
    let f = setup("rp-held-skip", ROSTER_PROTECT);
    f.budget_set("zaiplan", "97%");
    let (code, _out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--provider",
        "ZAI",
        "--model",
        "glm-5.3-flash",
        "--purpose",
        "diff-review",
        "--prompt",
        "x",
        "--skip-preflight",
    ]);
    assert_eq!(
        code,
        Some(1),
        "held refusal exit 1 even with --skip-preflight: {err}"
    );
    assert!(err.contains("is held"), "{err}");
    assert!(err.contains("--spend-reserve"), "{err}");
    f.cleanup();
}

// #1 ordinary (content), #2 protected AND weighty on plan `zaiplan`.
const ROSTER_PROTECT_WEIGHTY: &str = r#"{"roster_version":1,"reviewers":[
    {"provider":"ZAI","model":"glm-5.3"},
    {"provider":"ZAI","model":"glm-5.3-flash","panel":"weighty","plan":"zaiplan","ext":{"reserve_percent":15}}
]}"#;

#[test]
fn a_required_held_weighty_entry_on_a_light_purpose_refuses() {
    // (F06-1) a held WEIGHTY protected entry on a light purpose must carry skip kind `reserve`
    // (a hard outage), NOT `weighty` (which the required-outage check and select_panel_routing treat
    // as a soft skip that rejoins). --require of it on diff-review therefore refuses the run (exit 5).
    let f = setup("rp-held-weighty", ROSTER_PROTECT_WEIGHTY);
    f.budget_set("zaiplan", "97%");
    let (code, _out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--panel",
        "--purpose",
        "diff-review",
        "--require",
        "#2",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(
        code,
        Some(5),
        "required held weighty entry refuses (exit 5): {err}"
    );
    assert!(
        err.to_lowercase().contains("reserve") || err.contains("--spend-reserve"),
        "names the hold, not the weighty gate: {err}"
    );
    f.cleanup();
}

// #1 PROTECTED on plan `zaiplan` (held once its plan is spent), #2 ordinary (the fallback).
const ROSTER_FORK: &str = r#"{"roster_version":1,"reviewers":[
    {"provider":"ZAI","model":"glm-5.3","plan":"zaiplan","ext":{"reserve_percent":15}},
    {"provider":"ZAI","model":"glm-5.3-flash"}
]}"#;

#[test]
fn a_fork_follow_up_continues_the_previous_reviewer_or_draws_when_held() {
    // (F06-3) a bare --mode fork must continue the task's latest verified reviewer when it is still
    // eligible (no hop), and draw a new one with a fresh thread only when it is not (e.g. held).
    let f = setup("rp-fork", ROSTER_FORK);
    // Seed the task with a real consult on glm-5.3 (records its thread + lineage).
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--provider",
        "ZAI",
        "--model",
        "glm-5.3",
        "--purpose",
        "checkpoint",
        "--prompt",
        "x",
    ]);
    assert_eq!(code, Some(0), "seed consult:\n{out}\n{err}");

    // (a) a bare --mode fork continues glm-5.3 (eligible), not a different reviewer.
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--mode",
        "fork",
        "--purpose",
        "checkpoint",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "fork continue:\n{out}\n{err}");
    assert!(
        out.contains("continuing the task's previous reviewer") && out.contains("glm-5.3 "),
        "continues glm-5.3: {out}"
    );
    assert!(
        out.contains("reviewer    : ZAI :: glm-5.3 "),
        "the reviewer is glm-5.3, not glm-5.3-flash: {out}"
    );

    // (b) hold glm-5.3's plan: the fork cannot continue it, so it draws glm-5.3-flash afresh.
    f.budget_set("zaiplan", "97%");
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--mode",
        "fork",
        "--purpose",
        "checkpoint",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "fork draw:\n{out}\n{err}");
    assert!(
        out.contains("was not continued") && out.contains("glm-5.3-flash"),
        "draws glm-5.3-flash and says the previous reviewer was not continued: {out}"
    );
    f.cleanup();
}

// #1 ordinary, #2 BOTH on-request AND protected (reserve 15) on plan `zaiplan`.
const ROSTER_ONREQ_PROTECT: &str = r#"{"roster_version":1,"reviewers":[
    {"provider":"ZAI","model":"glm-5.3"},
    {"provider":"ZAI","model":"glm-5.3-flash","plan":"zaiplan","ext":{"on_request":true,"reserve_percent":15}}
]}"#;

#[test]
fn a_required_on_request_and_held_entry_refuses_without_spend_reserve() {
    // (R4) an entry that is BOTH on_request and over its protected reserve must not run in a
    // non-standing panel merely because an exact --require names it: the reserve hold is evaluated
    // before the on-request gate, so the required entry carries the HARD `reserve` skip and the panel
    // refuses (exit 5) - unless --spend-reserve. Before the fix on_request hid the reserve hold and
    // the required-outage check re-admitted the entry, spending the protected reserve.
    let f = setup("rp-onreq-held", ROSTER_ONREQ_PROTECT);
    f.budget_set("zaiplan", "97%");

    // (a) --panel --require #2 on a non-standing purpose refuses (exit 5), nothing started.
    let (code, _out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--panel",
        "--purpose",
        "diff-review",
        "--require",
        "#2",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(
        code,
        Some(5),
        "required on-request+held entry refuses: {err}"
    );
    assert!(
        err.to_lowercase().contains("reserve") || err.contains("--spend-reserve"),
        "names the reserve hold, not just the on-request gate: {err}"
    );

    // (b) --spend-reserve seats the required entry (preflight available on the dry run).
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--panel",
        "--purpose",
        "diff-review",
        "--require",
        "#2",
        "--prompt",
        "x",
        "--spend-reserve",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "spend-reserve seats it: {out}\n{err}");
    assert!(
        out.contains("glm-5.3-flash"),
        "the entry is seated with --spend-reserve: {out}"
    );
    f.cleanup();
}

// #1 ordinary, #2 on-request (seated only when named).
const ROSTER_ON_REQUEST: &str = r#"{"roster_version":1,"reviewers":[
    {"provider":"ZAI","model":"glm-5.3"},
    {"provider":"ZAI","model":"glm-5.3-flash","ext":{"on_request":true}}
]}"#;

#[test]
fn an_on_request_entry_is_named_only_never_drawn() {
    let f = setup("rp-onreq", ROSTER_ON_REQUEST);

    // (a) --panel-all never seats the on-request entry (shown "on request only").
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--panel-all",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "panel-all dry:\n{out}\n{err}");
    assert!(
        out.contains("on request only"),
        "on-request entry shown skipped: {out}"
    );

    // (b) --provider/--model naming it runs it (preflight available on the dry run).
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--provider",
        "ZAI",
        "--model",
        "glm-5.3-flash",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "named dry:\n{out}\n{err}");
    assert!(
        out.contains("reviewer    : ZAI :: glm-5.3-flash "),
        "the named on-request entry is the reviewer: {out}"
    );

    // (c) a panel that --requires it by position seats it.
    let (code, out, err) = f.run(&[
        "consult",
        "--task",
        "t",
        "--panel",
        "--require",
        "#2",
        "--prompt",
        "x",
        "--dry-run",
    ]);
    assert_eq!(code, Some(0), "require #2 dry:\n{out}\n{err}");
    assert!(
        out.contains("glm-5.3-flash") && out.to_lowercase().contains("required"),
        "the required on-request entry is seated: {out}"
    );
    f.cleanup();
}
