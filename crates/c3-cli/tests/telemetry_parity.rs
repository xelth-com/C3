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
if ($raw -match '(?:^| )-o (\S+)') { $o = $Matches[1] }
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
const LEDGER_ORDER: &str = "n,when,purpose,topics,role,consult_id,consult_ref,reviewer,lineage,coordinator,preflight,preflight_warning,roster,panel,parent_thread,thread,thread_source,thread_candidate,mode,mode_fallback,command,child_env_scrubbed,brief,range,prompt_chars,reply,reply_json,events,partial_reply,model,effort,effort_requested,effort_sent,effort_mapping,effort_caps,effort_confirmed,max_words,sandbox,timeout_sec,timeout_source,continue_sec,extra_config,extra_config_source,context_window,peak,peak_schedule,peak_source,peak_evaluated_at,structured,schema,schema_transport,schema_transport_source,validation_error,format_retry,denial_retry,timeout_continue,stall,kill_confirmed,base_commit,reviewed_revision,tree_sha256,tree_sha256_after,tree_changed_during_review,revision_moved,changed_files,brief_sha256,brief_sha256_after,brief_changed_during_review,fingerprint_note,artifacts,artifacts_changed_during_review,tree_check,bridge_outcome,provider_failure,warnings,verdict,verdict_reason,findings,finding_ids,prior_findings,unchecked_prior_blockers,usage,compactions,engine_run,wall_seconds,finished_at,commit_wait_ms";

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
    // (Rust std quotes a batch launcher's argument that holds `=`: `-c "k=v"`; codex receives the
    // same argv either way)
    let args = std::fs::read_to_string(&log).unwrap().replace('"', "");
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
    assert!(!raw.contains("JudgeLabel") && !raw.contains("[x]"));
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
            "(codex), purpose diff-review, mark partly, age_days 0, client_time {}, judge moonshot / k3 (rating_actor)",
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
    let marks = e.marks();
    assert!(marks[0]["telemetry_sent"].is_i64() && marks[1]["telemetry_sent"].is_i64());
    assert!(marks[2].get("telemetry_sent").is_none());
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
