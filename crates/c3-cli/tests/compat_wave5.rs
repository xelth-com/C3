//! (wave 5 of the compatibility track) The last known gaps before the 0.2.0 acceptance, end to
//! end through the real `c3` binary against a fake codex `.cmd` launcher - the Rust mirror of the
//! plugin's harness-host checks:
//!
//! - PREFIX D6 (the plugin's wave 27c): `--brief-prefix` / `CODEX_CONSULT_BRIEF_PREFIX`, a reply
//!   prefix and a non-slug refused before anything starts (the dry run too), the dry run's
//!   `brief prefix:` line; TESTLINE D12: a refused run says nothing of test mode;
//! - REFUSE D3: an unparseable `CODEX_CONSULT_COORDINATOR` refused with the plugin's character
//!   rule (`Get-IdentityStringProblem`), the dry run and a real run alike, nothing written.
//!
//! Nothing reaches a real provider or intake. Windows only (the fake codex is a `.cmd` wrapper
//! around a PowerShell script, as the plugin's `fake-codex.cmd`).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn c3_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_c3"))
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!("c3-w5-{tag}-{}-{nanos}", std::process::id()));
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

/// `--version`, `login status`, and an exec turn that copies `FAKE_CODEX_REPLY` to the `-o` file.
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
if ($o -and $env:FAKE_CODEX_REPLY) { [IO.File]::Copy($env:FAKE_CODEX_REPLY, $o, $true) }
[Console]::Out.Write("{""type"":""turn.completed"",""usage"":{""input_tokens"":1000,""cached_input_tokens"":200,""output_tokens"":300}}`n")
exit 0
"#;

const ADVISE: &str = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;

const CODEX_TOML: &str = "model = \"gpt-5.1\"\n";

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
    /// `c3 consult --task t --prompt x <extra>` in the repo with only this test's switches (no
    /// inherited bridge, fake or C3 variable), telemetry off, test mode on.
    fn consult(&self, extra: &[&str], env: &[(&str, &str)]) -> Output {
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
            .env("CODEX_CONSULT_EXE", &self.fake)
            .env("CODEX_CONSULT_ROSTER", "none")
            .env("CODEX_CONSULT_HEALTH", "none")
            .env("CODEX_CONSULT_TELEMETRY", "off")
            .env("CODEX_CONSULT_TEST_MODE", "1")
            .env("C3_TELEMETRY_HUB", "http://127.0.0.1:9/T")
            .env("C3_PRIORS", "off")
            .env("FAKE_CODEX_REPLY", &self.reply);
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.args(["consult", "--task", "t", "--prompt", "x"]);
        cmd.args(extra);
        cmd.output().expect("run c3")
    }

    fn collab(&self) -> PathBuf {
        self.repo.join(".collab")
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

fn first(o: &Output) -> String {
    text(o).lines().next().unwrap_or("").to_string()
}

// ------------------------------------------------------------------ PREFIX D6

/// harness-host PREFIX D6 #1: a reply prefix (every engine's) is refused as the brief prefix -
/// `--brief-prefix` and `CODEX_CONSULT_BRIEF_PREFIX` alike, the dry run too (exit 1, nothing
/// written, no test-mode line: TESTLINE D12); a prefix that is not a lowercase slug too.
#[test]
fn a_reply_prefix_or_a_non_slug_is_refused_as_the_brief_prefix() {
    let e = setup("prefix-refused");
    for p in ["codex", "agy", "muse", "claudecode", "http"] {
        let o = e.consult(&["--dry-run", "--brief-prefix", p], &[]);
        assert_eq!(o.status.code(), Some(1), "{}", text(&o));
        assert!(
            first(&o).starts_with(&format!(
                "codex-consult: the brief prefix '{p}' (-BriefPrefix) is a reply prefix: the bridge names its replies handoffs/<NN>-<codex|agy|muse|claudecode|http>-<slug>.*; give the coordinator's briefs a prefix of their own (the default: claude); nothing was started."
            )),
            "{}",
            text(&o)
        );
        assert!(!text(&o).contains("test mode is ON"), "{}", text(&o));
    }
    let o = e.consult(&["--dry-run"], &[("CODEX_CONSULT_BRIEF_PREFIX", "agy")]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        first(&o).contains("'agy' (CODEX_CONSULT_BRIEF_PREFIX) is a reply prefix"),
        "{}",
        text(&o)
    );
    let o = e.consult(&["--dry-run", "--brief-prefix", "Bad_Prefix"], &[]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        first(&o),
        "codex-consult: the brief prefix 'Bad_Prefix' (-BriefPrefix) must be a lowercase slug (a letter, then letters, digits or dashes; at most 32 characters); nothing was started."
    );
    // a real run is refused the same way, before anything is written
    let o = e.consult(&["--brief-prefix", "muse"], &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(!e.collab().exists(), "nothing written");
}

/// harness-host PREFIX D6 #2: the default stays `claude`; `CODEX_CONSULT_BRIEF_PREFIX=codexhost`
/// names another host's briefs; the flag wins over the variable; a real run with its own prefix
/// goes on (the bridge never writes a brief).
#[test]
fn the_dry_run_names_the_brief_prefix_and_its_source() {
    let e = setup("prefix-line");
    let o = e.consult(&["--dry-run"], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(
        text(&o).lines().any(|l| l
            == "brief prefix: claude (the default) - the coordinator's briefs are handoffs/<NN>-claude-<slug>.md, this reply 01-codex-reply.*"),
        "{}",
        text(&o)
    );
    let o = e.consult(
        &["--dry-run"],
        &[("CODEX_CONSULT_BRIEF_PREFIX", "codexhost")],
    );
    assert!(
        text(&o).lines().any(|l| l.starts_with(
            "brief prefix: codexhost (CODEX_CONSULT_BRIEF_PREFIX) - the coordinator's briefs are handoffs/<NN>-codexhost-<slug>.md"
        )),
        "{}",
        text(&o)
    );
    let o = e.consult(
        &["--dry-run", "--brief-prefix", "lead", "--reply-name", "r2"],
        &[("CODEX_CONSULT_BRIEF_PREFIX", "codexhost")],
    );
    assert!(
        text(&o).lines().any(|l| l
            == "brief prefix: lead (-BriefPrefix) - the coordinator's briefs are handoffs/<NN>-lead-<slug>.md, this reply 01-codex-r2.*"),
        "{}",
        text(&o)
    );
    let o = e.consult(&["--brief-prefix", "lead"], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
}

// ------------------------------------------------------------------ REFUSE D3

/// harness-host REFUSE D3: a `CODEX_CONSULT_COORDINATOR` that does not parse refuses the dry run
/// and a real run with the plugin's wording - the character rule of a provider label and a model
/// id (`::`, `[`, `]`, `|`, `,`, `#`) - and nothing is written; interior blanks are no refusal.
#[test]
fn an_unparseable_coordinator_is_refused_with_the_plugins_wording() {
    let e = setup("refuse");
    let o = e.consult(&["--dry-run"], &[("CODEX_CONSULT_COORDINATOR", "open::ai")]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert_eq!(
        first(&o),
        "codex-consult: CODEX_CONSULT_COORDINATOR='open::ai' cannot be used: the provider 'open::ai' must not contain '::' - give '<provider> :: <model>' (optionally ' [<engine>]'), a roster position '#<n>' or a provider label; nothing was started."
    );
    let o = e.consult(&[], &[("CODEX_CONSULT_COORDINATOR", "openai :: gpt|5")]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(
        first(&o).starts_with(
            "codex-consult: CODEX_CONSULT_COORDINATOR='openai :: gpt|5' cannot be used: the model 'gpt|5' must not contain '|'"
        ),
        "{}",
        text(&o)
    );
    assert!(!e.collab().exists(), "nothing written");
    let o = e.consult(
        &["--dry-run"],
        &[("CODEX_CONSULT_COORDINATOR", "open ai :: gpt 5")],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(
        text(&o).contains("coordinator : open ai :: gpt 5; host "),
        "{}",
        text(&o)
    );
}

// ------------------------------------------------------------------ WARN D3

/// harness-host WARN D3 (the rows C3 can meet - not the multi-host `host codex` ones): a real run
/// whose coordinator is given as a label (`openai`) in a Claude Code host is a WARNING, never a
/// refusal; the ledger's coordinator is the resolved triple {openai, gpt-5.1 (the config's model -
/// no roster), codex, claude-code, explicit}, `warnings[]` holds the self-review warning and the
/// console printed it; another coordinator model seats no warning.
#[test]
fn the_coordinators_own_model_seated_is_a_warning_with_the_resolved_triple() {
    let e = setup("warn");
    let o = e.consult(
        &["--reply-name", "w"],
        &[("CODEX_CONSULT_COORDINATOR", "openai"), ("CLAUDECODE", "1")],
    );
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(
        t.contains("WARNING: coordinator: openai :: gpt-5.1 is the coordinator"),
        "{t}"
    );
    let p = e.repo.join(".collab").join("t").join("sessions.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let last = v["codex"]["consults"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    let c = &last["coordinator"];
    assert_eq!(
        (
            c["provider"].as_str(),
            c["model"].as_str(),
            c["engine"].as_str(),
            c["host"].as_str(),
            c["source"].as_str()
        ),
        (
            Some("openai"),
            Some("gpt-5.1"),
            Some("codex"),
            Some("claude-code"),
            Some("explicit")
        ),
        "{c}"
    );
    let own = last["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| {
            w.as_str()
                .unwrap_or("")
                .contains("a second opinion from the coordinator's own model")
        })
        .count();
    assert_eq!(own, 1, "{}", last["warnings"]);
    let o = e.consult(
        &["--dry-run"],
        &[("CODEX_CONSULT_COORDINATOR", "ZAI :: glm-5.3")],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(!text(&o).contains("WARNING: coordinator:"), "{}", text(&o));
}

/// fixes27c COORD D11/D12 with the plugin's `Format-CoordinatorText` / `Format-CoordinatorId`: a
/// coordinator no roster entry matches - the dry run's line ends "(not in the roster - no reviewer
/// can match it)", the real run says "coordinator: openai :: gpt-9 (not in the roster ...)"; a `#9`
/// with no seat is shown as "#9 (names no roster position here); host ..." and gets no D11 line;
/// a codex coordinator carries no ` [codex]`.
#[test]
fn the_coordinator_line_is_the_plugins() {
    let e = setup("coordline");
    let roster = e.work.join("roster.json");
    std::fs::write(
        &roster,
        r#"{"roster_version":1,"reviewers":[{"provider":"openai","model":"gpt-5.1"}]}"#,
    )
    .unwrap();
    let r = roster.to_string_lossy().to_string();
    let line = |o: &Output| {
        text(o)
            .lines()
            .find(|l| l.starts_with("coordinator : "))
            .unwrap_or("")
            .to_string()
    };
    let o = e.consult(
        &["--dry-run"],
        &[
            ("CODEX_CONSULT_ROSTER", r.as_str()),
            ("CODEX_CONSULT_COORDINATOR", "openai :: gpt-9"),
        ],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let l = line(&o);
    assert!(
        l.starts_with("coordinator : openai :: gpt-9; host ")
            && l.ends_with("; source explicit (not in the roster - no reviewer can match it)"),
        "{l}"
    );
    let o = e.consult(
        &["--reply-name", "c1"],
        &[
            ("CODEX_CONSULT_ROSTER", r.as_str()),
            ("CODEX_CONSULT_COORDINATOR", "openai :: gpt-9"),
        ],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(
        text(&o)
            .lines()
            .any(|l| l
                == "coordinator: openai :: gpt-9 (not in the roster - no reviewer can match it)"),
        "{}",
        text(&o)
    );
    let o = e.consult(
        &["--dry-run"],
        &[
            ("CODEX_CONSULT_ROSTER", r.as_str()),
            ("CODEX_CONSULT_COORDINATOR", "#9"),
        ],
    );
    let l = line(&o);
    assert!(
        l.starts_with("coordinator : #9 (names no roster position here); host ")
            && l.ends_with("; source explicit"),
        "{l}"
    );
    let o = e.consult(
        &["--dry-run"],
        &[
            ("CODEX_CONSULT_ROSTER", r.as_str()),
            ("CODEX_CONSULT_COORDINATOR", "#1"),
        ],
    );
    let l = line(&o);
    assert!(
        l.starts_with("coordinator : openai :: gpt-5.1; host "),
        "{l}"
    );
}
