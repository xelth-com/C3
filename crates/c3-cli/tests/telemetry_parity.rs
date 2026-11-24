//! (wave 2 of the 0.6.1 parity) The 0.6.1 telemetry semantics end to end, through the real `c3`
//! binary against a fake codex CLI - the Rust mirror of the plugin's harness-telemetry SPOOL / RATE
//! / BACKFILL checks and of harness-0.3 LEDGER, harness-fixes28b CONTEXT and harness-fixes28c
//! COMPACT:
//!
//! - the ledger entry's key order (0.6.1: `consult_ref` right after `consult_id`, `context_window`
//!   after `extra_config_source`, `compactions` after `usage`), `consult_ref` a fresh lower-case guid
//!   per consultation, never the `consult_id`, and the consultation event carrying it as the LAST key
//!   of its details;
//! - the roster `context_tokens` reaching codex as `-c model_context_window` /
//!   `-c model_auto_compact_token_limit` (the operator's own value wins), the ledger `context_window`,
//!   and `compactions` (a number + the warning, `unknown`, or null);
//! - `c3 findings --rate`: `rating_rev` (1 + the consultation's highest, also with telemetry off),
//!   the judge resolved AT RATING TIME from three sources (rating_actor, consult_coordinator,
//!   unknown) and saved in the mark, `telemetry_sent`, the rating event's details in the plugin's
//!   exact order with the same `consult_ref` in every rating, no label and no host in any event;
//! - RC1: an abort between the mark's commit and its spool leaves a committed mark without
//!   `telemetry_sent` that `c3 telemetry --backfill-ratings` sends with the MARK's judge, rating_rev
//!   and `when` - never the backfilling process's coordinator - once.
//!
//! Nothing reaches a real intake: `C3_TELEMETRY_HUB` names a closed loopback port, `C3_PRIORS=off`,
//! and HOME/USERPROFILE/CODEX_HOME are scratch directories. Windows only (the fake codex is a `.cmd`
//! wrapper around a PowerShell script, as the plugin's `fake-codex3.cmd`).
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

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

const FAKE_CMD: &str = "@echo off\r\nset \"FAKE_CODEX_ARGS=%*\"\r\npowershell -NoProfile -ExecutionPolicy Bypass -File \"%~dp0fake-codex.ps1\"\r\nexit /b %ERRORLEVEL%\r\n";
/// A trimmed fake-codex3: `--version`, `login status`, and an exec turn that logs its argv
/// (`FAKE_CODEX_LOG`), reports a compaction when `FAKE_CODEX_COMPACT=1`, and copies
/// `FAKE_CODEX_REPLY` to the `-o` file.
const FAKE_PS1: &str = r#"$ErrorActionPreference = 'Stop'
$raw = [string]$env:FAKE_CODEX_ARGS
if ($raw -match '--version') { Write-Output 'codex-cli 0.155.1-fake'; exit 0 }
if ($raw -match '^\s*login\s+status(\s|$)') { [Console]::Error.WriteLine('Logged in using ChatGPT'); exit 0 }
if ($env:FAKE_CODEX_LOG) { [IO.File]::AppendAllText($env:FAKE_CODEX_LOG, "ARGS: $raw`n") }
$o = $null
if ($raw -match '(?:^| )-o (?:"([^"]+)"|(\S+))') { $o = $(if ($Matches[1]) { $Matches[1] } else { $Matches[2] }) }
$null = [Console]::In.ReadToEnd()
$tid = [guid]::NewGuid().ToString()
[Console]::Out.Write("{""type"":""thread.started"",""thread_id"":""$tid""}`n")
[Console]::Out.Write("{""type"":""turn.started""}`n")
if ($env:FAKE_CODEX_COMPACT -eq '1') { [Console]::Out.Write("{""type"":""item.completed"",""item"":{""id"":""c1"",""type"":""context_compaction""}}`n") }
[Console]::Out.Flush()
if ($o -and $env:FAKE_CODEX_REPLY) { [IO.File]::Copy($env:FAKE_CODEX_REPLY, $o, $true) }
[Console]::Out.Write("{""type"":""turn.completed"",""usage"":{""input_tokens"":1000,""cached_input_tokens"":200,""cache_write_input_tokens"":0,""output_tokens"":300,""reasoning_output_tokens"":40}}`n")
exit 0
"#;

const ADVISE: &str = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;

/// The scratch Codex config: the built-in openai (model gpt-5.1), a roster label on api.z.ai and one
/// on api.kimi.ai (labels that are NOT their vendor classes).
const CODEX_TOML: &str = "model = \"gpt-5.1\"\n\n[model_providers.ZAI]\nbase_url = \"https://api.z.ai/api/v1\"\nenv_key = \"RT_ZAI_KEY\"\nwire_api = \"responses\"\n\n[model_providers.JudgeLabel-Kimi]\nbase_url = \"https://api.kimi.ai/coding/v1\"\nenv_key = \"RT_KIMI_KEY\"\nwire_api = \"responses\"\n";

/// The plugin's 0.6.1 ledger entry, key for key (harness-0.3 LEDGER `$order`).
// (wave 8 = plugin wave 30) a codex entry ends with quota, tokens, subagents_allowed, budget
const LEDGER_ORDER: &str = "n,when,purpose,topics,role,consult_id,consult_ref,reviewer,lineage,coordinator,preflight,preflight_warning,roster,panel,parent_thread,thread,thread_source,thread_candidate,mode,mode_fallback,command,child_env_scrubbed,brief,range,prompt_chars,reply,reply_json,events,partial_reply,model,effort,effort_requested,effort_sent,effort_mapping,effort_caps,effort_confirmed,max_words,sandbox,timeout_sec,timeout_source,continue_sec,extra_config,extra_config_source,context_window,peak,peak_schedule,peak_source,peak_evaluated_at,structured,schema,schema_transport,schema_transport_source,validation_error,format_retry,denial_retry,timeout_continue,stall,kill_confirmed,base_commit,reviewed_revision,tree_sha256,tree_sha256_after,tree_changed_during_review,revision_moved,changed_files,brief_sha256,brief_sha256_after,brief_changed_during_review,fingerprint_note,artifacts,artifacts_changed_during_review,tree_check,bridge_outcome,provider_failure,warnings,verdict,verdict_reason,findings,finding_ids,prior_findings,unchecked_prior_blockers,usage,compactions,engine_run,wall_seconds,finished_at,commit_wait_ms,quota,tokens,subagents_allowed,budget";

const RATING_DETAIL_KEYS: &str =
    "engine,provider,model,purpose,mark,age_days,bridge_version,os,ps_version,judge";

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
    /// variable), telemetry ON unless `env` says otherwise, the intake a closed loopback port.
    fn c3(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(c3_bin());
        for (k, _) in std::env::vars() {
            if k.starts_with("CODEX_CONSULT_") || k.starts_with("FAKE_") || k.starts_with("C3_") {
                cmd.env_remove(&k);
            }
        }
        cmd.current_dir(&self.repo)
            .env("CODEX_HOME", &self.home)
            .env("HOME", self.work.join("userhome"))
            .env("USERPROFILE", self.work.join("userhome"))
            .env("RT_ZAI_KEY", "zai-test-key")
            .env("RT_KIMI_KEY", "kimi-test-key")
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

    fn consult(&self, extra: &[&str], env: &[(&str, &str)]) -> Output {
        let fake = self.fake.to_string_lossy().to_string();
        let mut args = vec![
            "consult",
            "--task",
            "t",
            "--prompt",
            "x",
            "--codex-exe",
            &fake,
        ];
        args.extend_from_slice(extra);
        self.c3(&args, env)
    }

    fn ledger(&self) -> Vec<Value> {
        let p = self.repo.join(".collab").join("t").join("sessions.json");
        let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        v["codex"]["consults"].as_array().unwrap().clone()
    }

    fn marks(&self) -> Vec<Value> {
        let p = self.repo.join(".collab").join("t").join("findings.json");
        let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        v["ratings"].as_array().cloned().unwrap_or_default()
    }

    fn findings_text(&self) -> String {
        std::fs::read_to_string(self.repo.join(".collab").join("t").join("findings.json"))
            .unwrap_or_default()
    }

    /// The raw spool lines (C3's own outbox under `<codex home>/c3/telemetry/`).
    fn spool_lines(&self) -> Vec<String> {
        let dir = self.home.join("c3").join("telemetry");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with("spool"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        let mut out = Vec::new();
        for f in files {
            for l in std::fs::read_to_string(f).unwrap_or_default().lines() {
                if !l.trim().is_empty() {
                    out.push(l.to_string());
                }
            }
        }
        out
    }

    /// The spooled events (a spool line's `body` string, or the line itself).
    fn spool_events(&self) -> Vec<Value> {
        self.spool_lines()
            .iter()
            .map(|l| {
                let v: Value = serde_json::from_str(l).unwrap();
                match v.get("body").and_then(|b| b.as_str()) {
                    Some(b) => serde_json::from_str(b).unwrap(),
                    None => v,
                }
            })
            .collect()
    }
}

fn keys(v: &Value) -> String {
    v.as_object()
        .map(|o| o.keys().cloned().collect::<Vec<_>>().join(","))
        .unwrap_or_default()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
    .replace("\r\n", "\n")
}

fn is_ref(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && [8, 4, 4, 4, 12]
            .iter()
            .zip(&parts)
            .all(|(n, p)| p.len() == *n && p.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
}

fn judge_of(ev: &Value) -> String {
    let j = &ev["details"]["judge"];
    format!(
        "{}/{}/{}",
        j["provider"].as_str().unwrap_or("?"),
        j["model"].as_str().unwrap_or("?"),
        j["source"].as_str().unwrap_or("?")
    )
}

fn mark_judge(m: &Value) -> String {
    let j = &m["judge"];
    format!(
        "{}/{}/{}",
        j["provider"].as_str().unwrap_or("?"),
        j["model"].as_str().unwrap_or("?"),
        j["source"].as_str().unwrap_or("?")
    )
}

fn utc_of(when: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(when)
        .unwrap()
        .with_timezone(&chrono::Utc)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

// --------------------------------------------------------------------------- SPOOL / LEDGER

#[test]
fn ledger_order_consult_ref_and_the_consultation_event() {
    let e = setup("tel-ledger");
    let a = e.consult(&["--purpose", "checkpoint", "--reply-name", "a"], &[]);
    assert_eq!(a.status.code(), Some(0), "{}", text(&a));
    let b = e.consult(&["--purpose", "checkpoint", "--reply-name", "b"], &[]);
    assert_eq!(b.status.code(), Some(0), "{}", text(&b));
    let ledger = e.ledger();
    assert_eq!(ledger.len(), 2);
    // (0.6.1) the exact key order of the plugin's entry
    assert_eq!(keys(&ledger[0]), LEDGER_ORDER);
    let r1 = ledger[0]["consult_ref"].as_str().unwrap();
    let r2 = ledger[1]["consult_ref"].as_str().unwrap();
    assert!(is_ref(r1) && is_ref(r2), "{r1} {r2}");
    assert_ne!(r1, r2, "each consultation mints its own consult_ref");
    assert_ne!(r1, ledger[0]["consult_id"].as_str().unwrap().to_lowercase());
    // no roster context_tokens: context_window and compactions null, present
    assert!(ledger[0]["context_window"].is_null());
    assert!(ledger[0]["compactions"].is_null());
    // the events: one per consultation, consult_ref LAST in details, equal to the ledger's
    let evs = e.spool_events();
    assert_eq!(evs.len(), 2, "{evs:#?}");
    for (ev, r) in evs.iter().zip([r1, r2]) {
        assert_eq!(ev["event_type"], "consultation");
        // (wave 2c) the built-in openai provider's class, the closed-list model, a usable outcome
        assert_eq!(ev["details"]["provider"], "openai");
        assert_eq!(ev["details"]["model"], "gpt-5.1");
        assert_eq!(ev["details"]["outcome"], "usable");
        assert_eq!(ev["severity"], "info");
        let d = ev["details"].as_object().unwrap();
        assert_eq!(
            d.keys().next_back().map(|s| s.as_str()),
            Some("consult_ref")
        );
        assert_eq!(ev["details"]["consult_ref"], r);
    }
    // never the task name, the consult_id or the thread in a spool line
    let raw = e.spool_lines().join("\n");
    assert!(!raw.contains(ledger[0]["consult_id"].as_str().unwrap()));
    assert!(!raw.contains(ledger[0]["thread"].as_str().unwrap()));
    let _ = std::fs::remove_dir_all(&e.work);
}

#[test]
fn telemetry_off_writes_nothing_and_a_runs_switch_wins() {
    let e = setup("tel-switch");
    let off = e.consult(&["--reply-name", "o1", "--telemetry", "off"], &[]);
    assert_eq!(off.status.code(), Some(0), "{}", text(&off));
    assert!(e.spool_lines().is_empty(), "--telemetry off spools nothing");
    // CODEX_CONSULT_TELEMETRY=off with --telemetry on: the run's switch wins (0.6.1 parity)
    let on = e.consult(
        &["--reply-name", "o2", "--telemetry", "on"],
        &[("CODEX_CONSULT_TELEMETRY", "off")],
    );
    assert_eq!(on.status.code(), Some(0), "{}", text(&on));
    assert_eq!(e.spool_lines().len(), 1);
    // any value that is not on counts as off
    let odd = e.consult(
        &["--reply-name", "o3"],
        &[("CODEX_CONSULT_TELEMETRY", "maybe")],
    );
    assert_eq!(odd.status.code(), Some(0), "{}", text(&odd));
    assert_eq!(e.spool_lines().len(), 1);
    // the dry run names the switch and its source; a value other than on|off is refused
    let dry = e.consult(
        &["--reply-name", "d1", "--dry-run", "--telemetry", "off"],
        &[],
    );
    assert!(
        text(&dry).contains("telemetry   : off (-Telemetry) - nothing is spooled or sent"),
        "{}",
        text(&dry)
    );
    let dry2 = e.consult(
        &["--reply-name", "d2", "--dry-run"],
        &[("CODEX_CONSULT_TELEMETRY", "off")],
    );
    assert!(text(&dry2).contains("telemetry   : off (CODEX_CONSULT_TELEMETRY) - nothing"));
    let dry3 = e.consult(&["--reply-name", "d3", "--dry-run"], &[]);
    assert!(text(&dry3).contains("telemetry   : on (the default) - after the commit ONE"));
    let bad = e.consult(&["--reply-name", "d4", "--telemetry", "maybe"], &[]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(text(&bad).contains("-Telemetry must be on or off (got 'maybe')"));
    assert_eq!(e.spool_lines().len(), 1);
    let _ = std::fs::remove_dir_all(&e.work);
}

// --------------------------------------------------------------------------- CONTEXT / COMPACT

#[test]
fn context_window_reaches_codex_and_compactions_are_recorded() {
    let e = setup("tel-context");
    let log = e.work.join("fake.log");
    let roster = e.work.join("roster.json");
    std::fs::write(
        &roster,
        r#"{"roster_version":1,"reviewers":[{"provider":"openai","model":"gpt-5.1","context_tokens":256000}]}"#,
    )
    .unwrap();
    let r = roster.to_string_lossy().to_string();
    let l = log.to_string_lossy().to_string();
    // a stream that reports a compaction: compactions 1 + the warning
    let x = e.consult(
        &["--reply-name", "c1", "--telemetry", "off"],
        &[
            ("CODEX_CONSULT_ROSTER", &r),
            ("FAKE_CODEX_LOG", &l),
            ("FAKE_CODEX_COMPACT", "1"),
        ],
    );
    assert_eq!(x.status.code(), Some(0), "{}", text(&x));
    // (wave 2b) a batch launcher gets a plain `k=v` as it is, as the plugin passes it (Rust std
    // alone would quote it: `-c "k=v"`) - the fake's raw `%*` text, unquoted
    let args = std::fs::read_to_string(&log).unwrap();
    assert!(args.contains(" -c model_context_window=256000 "), "{args}");
    assert!(
        args.contains(" -c model_auto_compact_token_limit=204800 "),
        "{args}"
    );
    let entry = e.ledger().last().cloned().unwrap();
    assert_eq!(
        entry["context_window"],
        serde_json::json!({"tokens": 256000, "auto_compact_limit": 204800, "items": ["model_context_window=256000", "model_auto_compact_token_limit=204800"]})
    );
    assert_eq!(entry["compactions"], 1);
    assert!(keys(&entry).contains(",usage,compactions,engine_run,"));
    let warn = "the reviewer compacted its context 1 time(s) - the reply may rest on a summary of the brief";
    assert!(
        entry["warnings"]
            .as_array()
            .unwrap()
            .contains(&Value::from(warn)),
        "{}",
        entry["warnings"]
    );
    assert!(
        text(&x).contains(&format!("warning    : {warn}")),
        "{}",
        text(&x)
    );
    // none reported: "unknown"
    let y = e.consult(
        &["--reply-name", "c2", "--telemetry", "off"],
        &[("CODEX_CONSULT_ROSTER", &r)],
    );
    assert_eq!(y.status.code(), Some(0), "{}", text(&y));
    assert_eq!(e.ledger().last().unwrap()["compactions"], "unknown");
    // the operator's own value wins: only the auto-compact limit is added
    std::fs::write(
        &roster,
        r#"{"roster_version":1,"reviewers":[{"provider":"openai","model":"gpt-5.1","context_tokens":200000,"codex_config":["model_context_window=100000"]}]}"#,
    )
    .unwrap();
    std::fs::remove_file(&log).unwrap();
    let z = e.consult(
        &["--reply-name", "c3", "--telemetry", "off"],
        &[("CODEX_CONSULT_ROSTER", &r), ("FAKE_CODEX_LOG", &l)],
    );
    assert_eq!(z.status.code(), Some(0), "{}", text(&z));
    let args = std::fs::read_to_string(&log).unwrap().replace('"', "");
    assert!(args.contains("model_context_window=100000"), "{args}");
    assert!(!args.contains("model_context_window=200000"), "{args}");
    assert!(
        args.contains(" -c model_auto_compact_token_limit=160000 "),
        "{args}"
    );
    assert_eq!(
        e.ledger().last().unwrap()["context_window"]["items"],
        serde_json::json!(["model_auto_compact_token_limit=160000"])
    );
    let _ = std::fs::remove_dir_all(&e.work);
}

// --------------------------------------------------------------------------- RATE / BACKFILL

#[test]
fn rating_judge_rev_telemetry_sent_and_the_backfill_rc1() {
    let e = setup("tel-rate");
    let roster = e.work.join("roster.json");
    std::fs::write(
        &roster,
        r#"{"roster_version":1,"reviewers":[{"provider":"JudgeLabel-Kimi","model":"k3"},{"provider":"ZAI","model":"glm-5.3"}]}"#,
    )
    .unwrap();
    let r = roster.to_string_lossy().to_string();
    // the consultation (telemetry off: the spool holds only what the ratings write); its
    // coordinator openai :: gpt-6-astra is recorded in the ledger
    let c = e.consult(
        &[
            "--purpose",
            "diff-review",
            "--provider",
            "ZAI",
            "--model",
            "glm-5.3",
            "--reply-name",
            "j1",
            "--telemetry",
            "off",
        ],
        &[
            ("CODEX_CONSULT_ROSTER", &r),
            ("CODEX_CONSULT_COORDINATOR", "openai :: gpt-6-astra"),
        ],
    );
    assert_eq!(c.status.code(), Some(0), "{}", text(&c));
    let entry = e.ledger()[0].clone();
    let cref = entry["consult_ref"].as_str().unwrap().to_string();
    assert!(e.spool_lines().is_empty());

    let rate = |useful: &str, extra: &[&str], env: &[(&str, &str)]| {
        let mut args = vec!["findings", "--task", "t", "--rate", "1", "--useful", useful];
        args.extend_from_slice(extra);
        let mut envs: Vec<(&str, &str)> = vec![("CODEX_CONSULT_ROSTER", r.as_str())];
        envs.extend_from_slice(env);
        e.c3(&args, &envs)
    };

    // (a) the rating actor: a roster label on api.kimi.ai -> moonshot / k3 / rating_actor
    let j1 = rate(
        "yes",
        &[],
        &[("CODEX_CONSULT_COORDINATOR", "JudgeLabel-Kimi :: k3")],
    );
    assert_eq!(j1.status.code(), Some(0), "{}", text(&j1));
    assert_eq!(
        text(&j1).trim_end(),
        "codex-findings: consult n=1 (ZAI :: glm-5.3, diff-review) rated yes."
    );
    let marks = e.marks();
    assert_eq!(
        keys(&marks[0]),
        "n,consult_id,lineage,provider,model,engine,purpose,topics,consult_when,useful,note,when,rating_rev,judge,telemetry_sent"
    );
    assert_eq!(marks[0]["rating_rev"], 1);
    assert_eq!(mark_judge(&marks[0]), "moonshot/k3/rating_actor");
    assert!(marks[0]["telemetry_sent"].is_i64());
    let evs = e.spool_events();
    assert_eq!(evs.len(), 1);
    let ev1 = &evs[0];
    assert_eq!(ev1["event_type"], "rating");
    assert_eq!(
        keys(&ev1["details"]),
        format!("{RATING_DETAIL_KEYS},rating_rev,consult_ref")
    );
    assert_eq!(judge_of(ev1), "moonshot/k3/rating_actor");
    assert_eq!(ev1["details"]["rating_rev"], 1);
    assert_eq!(ev1["details"]["consult_ref"], cref.as_str());
    assert_eq!(ev1["title"], "yes");
    assert_eq!(ev1["details"]["age_days"], 0);
    assert_eq!(
        ev1["client_time"].as_str().unwrap(),
        utc_of(marks[0]["when"].as_str().unwrap())
    );

    // (b) no CODEX_CONSULT_COORDINATOR: the consultation's own coordinator
    let j2 = rate("partly", &[], &[]);
    assert_eq!(j2.status.code(), Some(0), "{}", text(&j2));
    assert!(text(&j2).contains("re-rated partly (was yes)"));
    // (c) a value the bridge would refuse: other / other / rating_actor, the rating still recorded
    let j3 = rate(
        "no",
        &["--note", "n"],
        &[("CODEX_CONSULT_COORDINATOR", "a :: b [x] [y]")],
    );
    assert_eq!(j3.status.code(), Some(0), "{}", text(&j3));
    let evs = e.spool_events();
    assert_eq!(evs.len(), 3);
    assert_eq!(judge_of(&evs[1]), "openai/gpt-6-astra/consult_coordinator");
    assert_eq!(judge_of(&evs[2]), "other/other/rating_actor");
    let revs: Vec<i64> = evs
        .iter()
        .map(|v| v["details"]["rating_rev"].as_i64().unwrap())
        .collect();
    assert_eq!(revs, [1, 2, 3]);
    // every rating of the consultation carries the SAME consult_ref - its ledger entry's
    assert!(evs
        .iter()
        .all(|v| v["details"]["consult_ref"] == cref.as_str()));
    // no label and no host in any event, never the refused value
    let raw = e.spool_lines().join("\n");
    assert!(!raw.contains("JudgeLabel") && !raw.contains("[x]") && !raw.contains("ZAI"));
    // (wave 2c) the reviewer as classes: the vendor class of api.z.ai, never the label ZAI
    assert_eq!(evs[0]["details"]["provider"], "zai");
    assert_eq!(evs[0]["details"]["model"], "glm-5.3");
    assert_eq!(evs[0]["tags"], serde_json::json!(["zai", "glm-5.3"]));
    assert!(!raw.contains("claude-code") && !raw.contains("\"host\""));
    assert!(!e.findings_text().contains("[x]"));
    let marks = e.marks();
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0]["rating_rev"], 3);
    assert_eq!(mark_judge(&marks[0]), "other/other/rating_actor");

    // (d) telemetry off saves the judge and the revision too - and spools nothing
    let off = rate("yes", &["--telemetry", "off"], &[]);
    assert_eq!(off.status.code(), Some(0), "{}", text(&off));
    let m = &e.marks()[0];
    assert_eq!(m["rating_rev"], 4);
    assert_eq!(mark_judge(m), "openai/gpt-6-astra/consult_coordinator");
    assert!(m.get("telemetry_sent").is_none());
    assert_eq!(e.spool_events().len(), 3);
    // -Telemetry refusals
    let bad = rate("yes", &["--telemetry", "maybe"], &[]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(text(&bad).contains("-Telemetry must be on or off"));
    let bad2 = e.c3(
        &["findings", "--task", "t", "--list", "--telemetry", "off"],
        &[],
    );
    assert_eq!(bad2.status.code(), Some(1));
    assert_eq!(
        text(&bad2).trim_end(),
        "codex-findings: -Telemetry only goes with -Rate."
    );

    // RC1: the rating process dies between the mark's commit and its spool (test hook) under a
    // rating actor; the committed mark has no telemetry_sent
    let abort = rate(
        "partly",
        &[],
        &[
            ("CODEX_CONSULT_COORDINATOR", "JudgeLabel-Kimi :: k3"),
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("CODEX_CONSULT_TEST_RATE_ABORT_AFTER_COMMIT", "1"),
        ],
    );
    assert_eq!(abort.status.code(), Some(87), "{}", text(&abort));
    let m = e.marks()[0].clone();
    assert_eq!(m["rating_rev"], 5);
    assert_eq!(mark_judge(&m), "moonshot/k3/rating_actor");
    assert!(m.get("telemetry_sent").is_none());
    assert_eq!(
        e.spool_events().len(),
        3,
        "nothing spooled for the aborted rating"
    );

    // the backfill's dry run names the mark's classes and writes nothing
    let dry = e.c3(
        &["telemetry", "--backfill-ratings", "--dry-run"],
        &[("CODEX_CONSULT_COORDINATOR", "openai :: gpt-5.1")],
    );
    assert_eq!(dry.status.code(), Some(0), "{}", text(&dry));
    let dt = text(&dry);
    assert!(
        dt.contains(&format!(
            "codex-telemetry: would send: zai / glm-5.3 (codex), purpose diff-review, mark partly, age_days 0, client_time {}, judge moonshot / k3 (rating_actor)",
            utc_of(m["when"].as_str().unwrap())
        )),
        "{dt}"
    );
    assert!(
        dt.contains("codex-telemetry: t: would send 1, already 0, skipped 0"),
        "{dt}"
    );
    assert!(dt.contains("dry run - nothing was spooled or written"));
    assert_eq!(e.spool_events().len(), 3);
    // the backfill under ANOTHER coordinator: the mark's own judge, rating_rev and `when`
    let bf = e.c3(
        &["telemetry", "--backfill-ratings"],
        &[("CODEX_CONSULT_COORDINATOR", "openai :: gpt-5.1")],
    );
    assert_eq!(bf.status.code(), Some(0), "{}", text(&bf));
    assert!(
        text(&bf).contains("codex-telemetry: t: sent 1, already 0, skipped 0"),
        "{}",
        text(&bf)
    );
    let evs = e.spool_events();
    assert_eq!(evs.len(), 4);
    let late = &evs[3];
    assert_eq!(judge_of(late), "moonshot/k3/rating_actor");
    assert_eq!(late["details"]["rating_rev"], 5);
    assert_eq!(late["details"]["consult_ref"], cref.as_str());
    assert_eq!(
        late["client_time"].as_str().unwrap(),
        utc_of(m["when"].as_str().unwrap())
    );
    assert!(e.marks()[0]["telemetry_sent"].is_i64());
    // once: a second backfill sends nothing
    let bf2 = e.c3(&["telemetry", "--backfill-ratings"], &[]);
    assert_eq!(bf2.status.code(), Some(0), "{}", text(&bf2));
    assert!(text(&bf2).contains("codex-telemetry: t: sent 0, already 1, skipped 0"));
    assert_eq!(e.spool_events().len(), 4);
    // telemetry off: refused, nothing written
    let bf3 = e.c3(
        &["telemetry", "--backfill-ratings"],
        &[("CODEX_CONSULT_TELEMETRY", "off")],
    );
    assert_eq!(bf3.status.code(), Some(1));
    assert!(text(&bf3).contains("telemetry is off"));
    let _ = std::fs::remove_dir_all(&e.work);
}

/// (F09-6 / RC5) The rating actor of a BARE provider label: `JudgeLabel-Kimi` (its sole roster
/// entry's model k3, its endpoint api.kimi.ai) resolves as the plugin's `Resolve-CoordinatorIdentity`
/// does - the entry's one model - so the mark saves and the event carries `moonshot/k3/rating_actor`
/// (C3 sent `moonshot/other`). The same resolver records the consultation's coordinator: a bare
/// label in the ledger names that model and engine; a label of two models names none.
#[test]
fn rating_actor_of_a_bare_label_infers_its_sole_roster_model_rc5() {
    let e = setup("tel-rate-bare");
    let roster = e.work.join("roster.json");
    std::fs::write(
        &roster,
        r#"{"roster_version":1,"reviewers":[{"provider":"JudgeLabel-Kimi","model":"k3"},{"provider":"ZAI","model":"glm-5.3"},{"provider":"ZAI","model":"glm-5.1"}]}"#,
    )
    .unwrap();
    let r = roster.to_string_lossy().to_string();
    let c = e.consult(
        &[
            "--purpose",
            "diff-review",
            "--provider",
            "ZAI",
            "--model",
            "glm-5.3",
            "--reply-name",
            "b1",
            "--telemetry",
            "off",
        ],
        &[
            ("CODEX_CONSULT_ROSTER", &r),
            ("CODEX_CONSULT_COORDINATOR", "JudgeLabel-Kimi"),
        ],
    );
    assert_eq!(c.status.code(), Some(0), "{}", text(&c));
    let co = &e.ledger()[0]["coordinator"];
    assert_eq!(co["provider"], "JudgeLabel-Kimi");
    assert_eq!(co["model"], "k3", "{co}");
    assert_eq!(co["engine"], "codex", "{co}");
    assert_eq!(co["in_roster"], true, "{co}");
    let rate = |useful: &str, coordinator: &str| {
        e.c3(
            &["findings", "--task", "t", "--rate", "1", "--useful", useful],
            &[
                ("CODEX_CONSULT_ROSTER", r.as_str()),
                ("CODEX_CONSULT_COORDINATOR", coordinator),
            ],
        )
    };
    let j = rate("yes", "JudgeLabel-Kimi");
    assert_eq!(j.status.code(), Some(0), "{}", text(&j));
    let m = e.marks()[0].clone();
    assert_eq!(mark_judge(&m), "moonshot/k3/rating_actor");
    assert!(m["telemetry_sent"].is_i64());
    let evs = e.spool_events();
    assert_eq!(evs.len(), 1);
    assert_eq!(judge_of(&evs[0]), "moonshot/k3/rating_actor");
    // an ambiguous label (two models of ZAI): the vendor class of its endpoint, the model not named
    let j2 = rate("partly", "ZAI");
    assert_eq!(j2.status.code(), Some(0), "{}", text(&j2));
    assert_eq!(mark_judge(&e.marks()[0]), "zai/other/rating_actor");
    let evs = e.spool_events();
    assert_eq!(evs.len(), 2);
    assert_eq!(judge_of(&evs[1]), "zai/other/rating_actor");
    // a bare openai label outside the roster takes the Codex config's model (gpt-5.1)
    let j3 = rate("yes", "openai");
    assert_eq!(j3.status.code(), Some(0), "{}", text(&j3));
    assert_eq!(mark_judge(&e.marks()[0]), "openai/gpt-5.1/rating_actor");
    let raw = e.spool_lines().join("\n");
    assert!(!raw.contains("JudgeLabel") && !raw.contains("ZAI"), "{raw}");
    let _ = std::fs::remove_dir_all(&e.work);
}

#[test]
fn backfill_of_a_mark_without_a_saved_judge_never_takes_the_backfills_coordinator() {
    let e = setup("tel-backfill-legacy");
    // a seeded ledger: entry 1 with a coordinator (openai :: gpt-6-astra) and a consult_ref, entry
    // 2 recorded before wave 27 (no coordinator) and before 0.6.1 (no consult_ref)
    let tdir = e.repo.join(".collab").join("t");
    std::fs::create_dir_all(tdir.join("handoffs")).unwrap();
    let now = chrono::Local::now();
    let w1 = (now - chrono::Duration::days(3) - chrono::Duration::hours(2))
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let w2 = (now - chrono::Duration::minutes(5))
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let cref = "6f1c2a9e-4b7d-4e2a-9c3f-0d8e5b7a1c24";
    let sessions = serde_json::json!({
        "task_id": "t", "cwd": "x",
        "codex": {"tool": "x", "consults": [
            {"n": 1, "when": w1, "purpose": "framing", "consult_id": "00000000-0000-4000-8000-000000000241", "consult_ref": cref,
             "reviewer": {"provider": "MyGLM-Plan", "model": "glm-5.3", "engine": "codex", "provider_config": {"base_url": "https://api.z.ai/api/v1"}},
             "coordinator": {"provider": "openai", "model": "gpt-6-astra", "engine": null, "host": "unknown", "source": "explicit"}},
            {"n": 2, "when": w2, "purpose": "checkpoint", "consult_id": "00000000-0000-4000-8000-000000000242",
             "reviewer": {"provider": "AcmeCorp-Legal", "model": "acmecorp-contracts-7b", "engine": "codex", "provider_config": {"base_url": "https://llm.acmecorp-internal.example/v1"}}}
        ]}
    });
    std::fs::write(tdir.join("sessions.json"), sessions.to_string()).unwrap();
    // marks given before 0.6.1: no judge, no rating_rev, no telemetry_sent; one whose entry is gone
    let marked = now.format("%Y-%m-%dT%H:%M:%S%:z").to_string();
    let findings = serde_json::json!({
        "task_id": "t", "findings": [],
        "ratings": [
            {"n": 1, "consult_id": "00000000-0000-4000-8000-000000000241", "lineage": "MyGLM-Plan :: glm-5.3", "provider": "MyGLM-Plan", "model": "glm-5.3", "purpose": "framing", "consult_when": w1, "useful": "yes", "note": "", "when": marked},
            {"n": 2, "consult_id": "00000000-0000-4000-8000-000000000242", "lineage": "AcmeCorp-Legal :: acmecorp-contracts-7b", "provider": "AcmeCorp-Legal", "model": "acmecorp-contracts-7b", "purpose": "checkpoint", "useful": "partly", "note": "", "when": marked},
            {"n": 9, "consult_id": "00000000-0000-4000-8000-000000000249", "lineage": "x", "provider": "x", "model": "y", "purpose": "framing", "useful": "no", "note": "n", "when": marked}
        ]
    });
    std::fs::write(tdir.join("findings.json"), findings.to_string()).unwrap();
    let bf = e.c3(
        &["telemetry", "--backfill-ratings"],
        &[("CODEX_CONSULT_COORDINATOR", "JudgeLabel-Kimi :: k3")],
    );
    assert_eq!(bf.status.code(), Some(0), "{}", text(&bf));
    assert!(
        text(&bf).contains("codex-telemetry: t: sent 2, already 0, skipped 1"),
        "{}",
        text(&bf)
    );
    let evs = e.spool_events();
    assert_eq!(evs.len(), 2);
    // entry 1: its coordinator, never the backfill's actor; no rating_rev (none saved); its ref
    assert_eq!(judge_of(&evs[0]), "openai/gpt-6-astra/consult_coordinator");
    assert_eq!(
        keys(&evs[0]["details"]),
        format!("{RATING_DETAIL_KEYS},consult_ref")
    );
    assert_eq!(evs[0]["details"]["age_days"], 3);
    // entry 2: no coordinator -> unknown; no consult_ref key
    assert_eq!(judge_of(&evs[1]), "other/other/unknown");
    assert_eq!(keys(&evs[1]["details"]), RATING_DETAIL_KEYS);
    // (wave 2c) the labels of the seeded reviewers never leave: MyGLM-Plan on api.z.ai is zai,
    // AcmeCorp-Legal on its private host other / other
    let raw = e.spool_lines().join("\n");
    assert!(
        !raw.to_lowercase().contains("acmecorp") && !raw.contains("MyGLM"),
        "{raw}"
    );
    assert_eq!(evs[0]["details"]["provider"], "zai");
    assert_eq!(evs[1]["details"]["provider"], "other");
    assert_eq!(evs[1]["details"]["model"], "other");
    let marks = e.marks();
    assert!(marks[0]["telemetry_sent"].is_i64() && marks[1]["telemetry_sent"].is_i64());
    assert!(marks[2].get("telemetry_sent").is_none());
    let _ = std::fs::remove_dir_all(&e.work);
}

// --------------------------------------------------------------------------- PUBLISHED NAMES (F02-19)

/// The reviewers of the published-name tests: `(engine, base_url, model)` - every one rated once.
const PUBLISHED_REVIEWERS: [(&str, &str, &str); 11] = [
    ("codex", "https://api.z.ai/api/v1", "glm-9-pub"),
    ("codex", "https://api.z.ai/api/v1", "glm-9-cfg"),
    ("codex", "https://api.z.ai/api/v1", "glm-9-ep"),
    ("codex", "https://api.z.ai/api/v1", "glm-9-page"),
    ("codex", "https://api.z.ai/api/v1", "glm-9-trusted"),
    ("claude", "", "claude-opus-9-1"),
    ("claude", "", "claude-opus-9-2"),
    (
        "codex",
        "https://llm.acmecorp-internal.example/v1",
        "glm-9-pub",
    ),
    ("codex", "https://api.z.ai/api/v1", "glm-5.3"),
    ("http", "https://openrouter.ai/api/v1", "z-ai/glm-9-pub"),
    ("claude", "", "opus[1M]"),
];

/// Seed a ledger with one entry per [`PUBLISHED_REVIEWERS`] row and an unsent rating of each, run
/// `c3 telemetry --backfill-ratings`, and return each rating event's `details.model`.
fn backfilled_models(e: &Env) -> Vec<String> {
    backfilled_models_of(e, &PUBLISHED_REVIEWERS)
}

/// [`backfilled_models`] over the given `(engine, base_url, model)` rows.
fn backfilled_models_of(e: &Env, reviewers: &[(&str, &str, &str)]) -> Vec<String> {
    let tdir = e.repo.join(".collab").join("t");
    std::fs::create_dir_all(tdir.join("handoffs")).unwrap();
    let when = (chrono::Local::now() - chrono::Duration::hours(1))
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let mut consults = Vec::new();
    let mut ratings = Vec::new();
    for (i, (engine, base_url, model)) in reviewers.iter().enumerate() {
        let n = i + 1;
        let cid = format!("00000000-0000-4000-8000-{n:012}");
        let pc = if base_url.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({"base_url": base_url})
        };
        consults.push(serde_json::json!({"n": n, "when": when, "purpose": "checkpoint", "consult_id": cid,
            "reviewer": {"provider": "Label", "model": model, "engine": engine, "provider_config": pc}}));
        ratings.push(serde_json::json!({"n": n, "consult_id": cid, "lineage": format!("Label :: {model}"),
            "provider": "Label", "model": model, "purpose": "checkpoint", "useful": "yes", "note": "", "when": when}));
    }
    let sessions = serde_json::json!({"task_id": "t", "cwd": "x", "codex": {"tool": "x", "consults": consults}});
    std::fs::write(tdir.join("sessions.json"), sessions.to_string()).unwrap();
    let findings = serde_json::json!({"task_id": "t", "findings": [], "ratings": ratings});
    std::fs::write(tdir.join("findings.json"), findings.to_string()).unwrap();
    let bf = e.c3(&["telemetry", "--backfill-ratings"], &[]);
    assert_eq!(bf.status.code(), Some(0), "{}", text(&bf));
    let evs = e.spool_events();
    assert_eq!(evs.len(), reviewers.len(), "{evs:#?}");
    evs.iter()
        .map(|ev| ev["details"]["model"].as_str().unwrap_or("?").to_string())
        .collect()
}

#[test]
fn only_a_trusted_vendor_bound_published_id_is_sent_by_name() {
    let e = setup("tel-published");
    // the maintain state `c3 maintain` left: the built-in catalog, a configured catalog NOT declared
    // public, one declared public, an endpoint's own list and a configured page (not public) -
    // (F62-3) each catalog with the URL it was read from and whether it was public then
    let mdir = e.home.join("c3").join("maintain");
    std::fs::create_dir_all(&mdir).unwrap();
    let state = serde_json::json!({"models": {"sources": [
        {"source": "web:https://openrouter.ai:443/api/v1/models", "url": "https://openrouter.ai:443/api/v1/models", "public": true, "ids": ["z-ai/glm-9-pub", "anthropic/claude-opus-9-2"]},
        {"source": "web:https://models.example.com:443/v1/models", "url": "https://models.example.com:443/v1/models", "ids": ["z-ai/glm-9-cfg"]},
        {"source": "web:https://pub.example.com:443/v1/models", "url": "https://pub.example.com:443/v1/models", "public": true, "ids": ["z-ai/glm-9-trusted"]},
        {"source": "endpoint:https://api.z.ai:443/api/v1|RT_ZAI_KEY|", "ids": ["glm-9-ep"]},
        {"source": "page:zai-docs", "ids": ["glm-9-page"]}
    ]}});
    std::fs::write(mdir.join("state.json"), state.to_string()).unwrap();
    let cfg = serde_json::json!({"config_version": 1,
        "model_catalogs": ["https://models.example.com/v1/models", {"url": "https://pub.example.com/v1/models", "public": true}],
        "model_pages": [{"name": "zai-docs", "url": "https://docs.z.ai/models", "pattern": "(glm-[0-9a-z.-]+)"}]});
    std::fs::write(e.home.join("c3").join("config.json"), cfg.to_string()).unwrap();
    let models = backfilled_models(&e);
    assert_eq!(
        models,
        [
            "glm-9-pub",       // the built-in catalog, for its own vendor (z-ai -> zai)
            "other",           // a configured catalog not declared public
            "other",           // an endpoint's own list
            "other",           // a configured page not declared public
            "glm-9-trusted",   // a configured catalog declared "public": true
            "other",           // an unpublished Claude-shaped id
            "claude-opus-9-2", // a Claude id the built-in catalog attributes to anthropic
            "other",           // a published name on a reviewer of no vendor class
            "glm-5.3",         // the closed list
            "z-ai/glm-9-pub",  // OpenRouter's own form of the published name
            "opus",            // the closed list, `[1M]` stripped
        ]
    );
    let _ = std::fs::remove_dir_all(&e.work);
}

/// (0.2.5 D3) A Claude id the built-in catalog publishes with dots (`anthropic/claude-opus-9.3`) is
/// known to telemetry in the claude engine's dash spelling without a release: a claude reviewer on
/// `claude-opus-9-3` is sent by name, the OpenRouter route keeps the catalog's form, and an id the
/// catalog does not list stays `other`.
#[test]
fn a_published_claude_id_spelled_with_dots_is_sent_by_its_dash_name() {
    let e = setup("tel-published-claude-dots");
    let mdir = e.home.join("c3").join("maintain");
    std::fs::create_dir_all(&mdir).unwrap();
    let state = serde_json::json!({"models": {"sources": [
        {"source": "web:https://openrouter.ai:443/api/v1/models", "url": "https://openrouter.ai:443/api/v1/models", "public": true, "ids": ["anthropic/claude-opus-9.3"]}
    ]}});
    std::fs::write(mdir.join("state.json"), state.to_string()).unwrap();
    let models = backfilled_models_of(
        &e,
        &[
            ("claude", "", "claude-opus-9-3"),
            ("claude", "", "claude-opus-9-4"),
            (
                "http",
                "https://openrouter.ai/api/v1",
                "anthropic/claude-opus-9.3",
            ),
        ],
    );
    assert_eq!(
        models,
        ["claude-opus-9-3", "other", "anthropic/claude-opus-9.3"]
    );
    let _ = std::fs::remove_dir_all(&e.work);
}

/// What [`backfilled_models`] yields when no catalog id is published: the closed lists alone.
const CLOSED_LISTS_ONLY: [&str; 11] = [
    "other", "other", "other", "other", "other", "other", "other", "other", "glm-5.3", "other",
    "opus",
];

/// (F16-1, F17-5) A maintain state written by an older build keys its catalogs by host
/// (`web:<host>`): it is read without error, and none of its ids is sent by name - not the built-in
/// catalog host's, not a declared host's - until a fetch rewrites the record under the URL.
#[test]
fn a_legacy_host_keyed_maintain_state_publishes_nothing() {
    let e = setup("tel-published-legacy");
    let mdir = e.home.join("c3").join("maintain");
    std::fs::create_dir_all(&mdir).unwrap();
    let state = serde_json::json!({"models": {"sources": [
        {"source": "web:openrouter.ai", "ids": ["z-ai/glm-9-pub", "anthropic/claude-opus-9-2"]},
        {"source": "web:pub.example.com", "ids": ["z-ai/glm-9-trusted"]}
    ]}});
    std::fs::write(mdir.join("state.json"), state.to_string()).unwrap();
    let cfg = serde_json::json!({"config_version": 1,
        "model_catalogs": ["https://openrouter.ai/api/v1/models", {"url": "https://pub.example.com/v1/models", "public": true}]});
    std::fs::write(e.home.join("c3").join("config.json"), cfg.to_string()).unwrap();
    assert_eq!(backfilled_models(&e), CLOSED_LISTS_ONLY);
    let _ = std::fs::remove_dir_all(&e.work);
}

/// (F16-1, F17-5, the reviewers' RC1) Two catalogs on ONE host, the private one listed first and
/// only the second declared `"public": true`, end to end: `c3 maintain --models` fetches both (each
/// served from a local file in test mode) and records each under its own URL, replacing the older
/// build's host-keyed record; the backfill then sends the public catalog's id by name and the
/// private catalog's id as `other` - before the fix, the private URL was the only one fetched for
/// the host and the host was trusted because its other URL was public.
#[test]
fn two_catalogs_on_one_host_the_private_first_never_publish_the_private_id() {
    let e = setup("tel-published-same-host");
    let mdir = e.home.join("c3").join("maintain");
    std::fs::create_dir_all(&mdir).unwrap();
    // what an older build left: one record for the host, holding both ids
    let legacy = serde_json::json!({"models": {"sources": [
        {"source": "web:models.example.com", "ids": ["z-ai/glm-9-cfg", "z-ai/glm-9-trusted"]}
    ]}});
    std::fs::write(mdir.join("state.json"), legacy.to_string()).unwrap();
    let private = "https://models.example.com/private/v1/models";
    let public = "https://models.example.com/public/v1/models";
    let cfg = serde_json::json!({"config_version": 1,
        "model_catalogs": [private, {"url": public, "public": true}]});
    std::fs::write(e.home.join("c3").join("config.json"), cfg.to_string()).unwrap();
    let private_body = e.work.join("private.json");
    let public_body = e.work.join("public.json");
    std::fs::write(
        &private_body,
        r#"{"data":[{"id":"z-ai/glm-9-cfg","created":1794000000}]}"#,
    )
    .unwrap();
    std::fs::write(
        &public_body,
        r#"{"data":[{"id":"z-ai/glm-9-trusted","created":1794000000}]}"#,
    )
    .unwrap();
    let map = format!(
        "{private}|{};{public}|{}",
        private_body.display(),
        public_body.display()
    );
    let log = e.work.join("http.log");
    let log_s = log.to_string_lossy().into_owned();
    let o = e.c3(
        &["maintain", "--models"],
        &[
            ("CODEX_CONSULT_TEST_MODE", "1"),
            ("C3_MAINTAIN_MODELS", "announce"),
            ("C3_MAINTAIN_TEST_HTTP", map.as_str()),
            ("C3_MAINTAIN_TEST_HTTP_LOG", log_s.as_str()),
        ],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let asked = std::fs::read_to_string(&log).unwrap_or_default();
    assert_eq!(
        asked.lines().collect::<Vec<_>>(),
        vec![private, public],
        "both URLs of the host were fetched"
    );
    let st: Value =
        serde_json::from_str(&std::fs::read_to_string(mdir.join("state.json")).unwrap()).unwrap();
    let sources: Vec<String> = st["models"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["source"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        sources,
        vec![
            "web:https://models.example.com:443/private/v1/models".to_string(),
            "web:https://models.example.com:443/public/v1/models".to_string(),
        ],
        "each URL under its own key; the host-keyed record is replaced"
    );
    // (F62-3) each record says whether its URL was public when it was read
    let records = st["models"]["sources"].as_array().unwrap();
    assert!(records[0].get("public").is_none(), "{}", records[0]);
    assert_eq!(records[1]["public"], true, "{}", records[1]);
    let mut want = CLOSED_LISTS_ONLY;
    want[4] = "glm-9-trusted"; // the declared catalog's id, for its own vendor
    assert_eq!(
        backfilled_models(&e),
        want,
        "glm-9-cfg (the private catalog's id, row 2) is never sent by name"
    );
    let _ = std::fs::remove_dir_all(&e.work);
}

/// (F57-1, the reviewer's RC1) A page's label reused for another URL, end to end and offline:
/// `c3 maintain --models` reads `zai-docs` while it names an internal page NOT declared public and
/// records the id with that URL; the configuration then gives the label to a public vendor page on
/// `docs.z.ai` - the backfill sends the earlier page's id as `other` (before the fix trust was
/// joined by the label, and the plain id took the new URL's vendor class). A label-only record of
/// an older build is withheld the same way. After a refresh reads the id from the public URL itself,
/// it is sent by name.
#[test]
fn a_page_label_reused_for_a_public_url_never_publishes_the_earlier_pages_id() {
    let e = setup("tel-published-page-reuse");
    let mdir = e.home.join("c3").join("maintain");
    std::fs::create_dir_all(&mdir).unwrap();
    let internal = "https://wiki.acmecorp-internal.example/llm";
    let public = "https://docs.z.ai/models";
    let body = e.work.join("page.html");
    std::fs::write(&body, "<code>glm-9-page</code>").unwrap();
    let configure = |url: &str, public: bool| {
        let cfg = serde_json::json!({"config_version": 1,
            "model_pages": [{"name": "zai-docs", "url": url, "pattern": "<code>(glm-[0-9a-z.-]+)</code>", "public": public}]});
        std::fs::write(e.home.join("c3").join("config.json"), cfg.to_string()).unwrap();
    };
    let maintain = |url: &str| {
        let map = format!("{url}|{}", body.display());
        let o = e.c3(
            &["maintain", "--models"],
            &[
                ("CODEX_CONSULT_TEST_MODE", "1"),
                ("C3_MAINTAIN_MODELS", "announce"),
                ("C3_MAINTAIN_TEST_HTTP", map.as_str()),
            ],
        );
        assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    };
    let page_record = || -> Value {
        let st: Value =
            serde_json::from_str(&std::fs::read_to_string(mdir.join("state.json")).unwrap())
                .unwrap();
        st["models"]["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["source"] == "page:zai-docs")
            .cloned()
            .unwrap_or(Value::Null)
    };
    // each backfill below starts from an empty spool
    let clear_spool = || {
        let dir = e.home.join("c3").join("telemetry");
        for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let n = f.file_name().to_string_lossy().into_owned();
            if n.starts_with("spool") && n.ends_with(".ndjson") {
                std::fs::remove_file(f.path()).unwrap();
            }
        }
    };
    let mut sent_by_name = CLOSED_LISTS_ONLY;
    sent_by_name[3] = "glm-9-page"; // row 4: a zai reviewer on glm-9-page

    // 1. the label names the internal page (not public): the id is recorded with that URL
    configure(internal, false);
    maintain(internal);
    let rec = page_record();
    assert_eq!(rec["ids"], serde_json::json!(["glm-9-page"]), "{rec}");
    assert_eq!(rec["url"], "https://wiki.acmecorp-internal.example:443/llm");
    assert!(rec.get("public").is_none(), "{rec}");
    // 2. the label now names a public vendor page; no refresh yet: the id stays `other`
    configure(public, true);
    assert_eq!(backfilled_models(&e), CLOSED_LISTS_ONLY);
    assert!(!e.spool_lines().join("\n").contains("glm-9-page"));
    // 3. an older build's label-only record of the same page: withheld as well
    let legacy = serde_json::json!({"models": {"sources": [
        {"source": "page:zai-docs", "ids": ["glm-9-page"]}
    ]}});
    std::fs::write(mdir.join("state.json"), legacy.to_string()).unwrap();
    clear_spool();
    assert_eq!(backfilled_models(&e), CLOSED_LISTS_ONLY);
    // 4. a refresh reads the id from the public URL itself: now it is sent by name
    maintain(public);
    let rec = page_record();
    assert_eq!(rec["url"], "https://docs.z.ai:443/models", "{rec}");
    assert_eq!(rec["public"], true, "{rec}");
    clear_spool();
    assert_eq!(backfilled_models(&e), sent_by_name);
    let _ = std::fs::remove_dir_all(&e.work);
}

/// (F62-3, the reviewer's RC3) A catalog URL declared public only after it was read, end to end and
/// offline: `c3 maintain --models` reads a configured catalog while it is NOT declared public, its
/// response holding a synthetic private id (`glm-9-cfg`) beside a public one (`glm-9-trusted`); the
/// configuration then declares the SAME URL public and the old cache is kept - the backfilled
/// events' models stay `other`, exactly as before the declaration (before the fix, today's trust of
/// the URL published the old cache). A custom catalog's record of an older build (no
/// collection-time status) is withheld the same way. After a refresh of the cleaned catalog (the
/// private id removed) the public id is sent by name and the private one stays `other`.
#[test]
fn a_catalog_declared_public_later_never_publishes_an_id_cached_while_it_was_private() {
    let e = setup("tel-published-catalog-reclassified");
    let mdir = e.home.join("c3").join("maintain");
    std::fs::create_dir_all(&mdir).unwrap();
    let url = "https://models.example.com/v1/models";
    let body = e.work.join("catalog.json");
    let configure = |public: bool| {
        let cfg = serde_json::json!({"config_version": 1,
            "model_catalogs": [{"url": url, "public": public}]});
        std::fs::write(e.home.join("c3").join("config.json"), cfg.to_string()).unwrap();
    };
    let maintain = || {
        let map = format!("{url}|{}", body.display());
        let o = e.c3(
            &["maintain", "--models"],
            &[
                ("CODEX_CONSULT_TEST_MODE", "1"),
                ("C3_MAINTAIN_MODELS", "announce"),
                ("C3_MAINTAIN_TEST_HTTP", map.as_str()),
            ],
        );
        assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    };
    let catalog_record = || -> Value {
        let st: Value =
            serde_json::from_str(&std::fs::read_to_string(mdir.join("state.json")).unwrap())
                .unwrap();
        st["models"]["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["source"] == "web:https://models.example.com:443/v1/models")
            .cloned()
            .unwrap_or(Value::Null)
    };
    // each backfill after the first starts from an empty spool
    let clear_spool = || {
        let dir = e.home.join("c3").join("telemetry");
        for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let n = f.file_name().to_string_lossy().into_owned();
            if n.starts_with("spool") && n.ends_with(".ndjson") {
                std::fs::remove_file(f.path()).unwrap();
            }
        }
    };

    // 1. read while NOT declared public: the private id is cached, with that status
    std::fs::write(
        &body,
        r#"{"data":[{"id":"z-ai/glm-9-cfg","created":1794000000},{"id":"z-ai/glm-9-trusted","created":1794000000}]}"#,
    )
    .unwrap();
    configure(false);
    maintain();
    let rec = catalog_record();
    assert_eq!(
        rec["ids"],
        serde_json::json!(["z-ai/glm-9-cfg", "z-ai/glm-9-trusted"]),
        "{rec}"
    );
    assert_eq!(
        rec["url"], "https://models.example.com:443/v1/models",
        "{rec}"
    );
    assert!(rec.get("public").is_none(), "{rec}");
    assert_eq!(
        backfilled_models(&e),
        CLOSED_LISTS_ONLY,
        "before the declaration"
    );
    // 2. the same URL declared public, the old cache kept (no refresh): still anonymised
    configure(true);
    clear_spool();
    assert_eq!(
        backfilled_models(&e),
        CLOSED_LISTS_ONLY,
        "declared public, not refreshed"
    );
    let spool = e.spool_lines().join("\n");
    assert!(
        !spool.contains("glm-9-cfg") && !spool.contains("glm-9-trusted"),
        "{spool}"
    );
    // 3. an older build's record of the same custom catalog (no collection-time status): withheld
    let legacy = serde_json::json!({"models": {"sources": [
        {"source": "web:https://models.example.com:443/v1/models", "ids": ["z-ai/glm-9-cfg", "z-ai/glm-9-trusted"]}
    ]}});
    std::fs::write(mdir.join("state.json"), legacy.to_string()).unwrap();
    clear_spool();
    assert_eq!(backfilled_models(&e), CLOSED_LISTS_ONLY, "a legacy record");
    // 4. a refresh of the cleaned catalog: the public id by name; the private one - no longer
    // listed - stays `other`
    std::fs::write(
        &body,
        r#"{"data":[{"id":"z-ai/glm-9-trusted","created":1794000000}]}"#,
    )
    .unwrap();
    maintain();
    let rec = catalog_record();
    assert_eq!(
        rec["ids"],
        serde_json::json!(["z-ai/glm-9-trusted"]),
        "{rec}"
    );
    assert_eq!(rec["public"], true, "{rec}");
    clear_spool();
    let mut want = CLOSED_LISTS_ONLY;
    want[4] = "glm-9-trusted"; // row 5: a zai reviewer on glm-9-trusted
    assert_eq!(
        backfilled_models(&e),
        want,
        "glm-9-cfg (row 2) is never sent by name"
    );
    let _ = std::fs::remove_dir_all(&e.work);
}

#[test]
fn without_a_maintain_state_the_closed_lists_decide() {
    let e = setup("tel-published-none");
    let models = backfilled_models(&e);
    assert_eq!(models, CLOSED_LISTS_ONLY);
    let _ = std::fs::remove_dir_all(&e.work);
}

// --------------------------------------------------------------------------- STATUS / FORGET (CLI)

#[test]
fn telemetry_status_and_the_form_refusals() {
    let e = setup("tel-status");
    let st = e.c3(
        &["telemetry", "--status"],
        &[("CODEX_CONSULT_TELEMETRY", "off")],
    );
    assert_eq!(st.status.code(), Some(0), "{}", text(&st));
    let t = text(&st);
    assert!(t.contains("telemetry off (CODEX_CONSULT_TELEMETRY)"), "{t}");
    assert!(t.contains("0 event(s), 0 complaint(s) in 0 file(s)"), "{t}");
    assert!(t.contains("\nlast flush : never"), "{t}");
    assert!(t.contains("none yet"), "{t}");
    assert!(
        !e.home.join("c3").join("telemetry").join("salt").exists(),
        "-Status creates no salt"
    );
    let none = e.c3(&["telemetry"], &[]);
    assert_eq!(none.status.code(), Some(1));
    assert!(text(&none).contains("give exactly one of -Flush, -Status, -Complain"));
    let local = e.c3(&["telemetry", "--status", "--local"], &[]);
    assert_eq!(local.status.code(), Some(1));
    assert!(text(&local).contains("-Local goes with -Forget"));
    let forget = e.c3(&["telemetry", "--forget"], &[]);
    assert_eq!(forget.status.code(), Some(1));
    assert!(text(&forget).contains("needs -PublicRef <ref>"));
    // forget-me without any stored reference: nothing sent, nothing removed
    let fm = e.c3(&["forget-me", "--yes"], &[]);
    assert_eq!(fm.status.code(), Some(1), "{}", text(&fm));
    assert!(text(&fm).contains("no public reference is stored"));
    let _ = std::fs::remove_dir_all(&e.work);
}
