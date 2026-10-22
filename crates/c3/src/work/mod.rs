//! `c3 work` — one worker run (`docs/work-mode.md`, design v1): a roster model as an
//! ordinary CODING WORKER. The child is the claude engine's launcher with the write
//! argv ([`c3_core::claude::work_args`]), the prompt (a fixed preamble + the brief file +
//! `--prompt`) on stdin, its stream-json captured verbatim, and the record
//! (`brief.md`, `events.jsonl`, `report.md`, `diff.txt`, `run.json`) written by c3 —
//! never by the worker. The roster is untouched: a work run is not a consultation, it
//! does not touch the review ledger and `c3 consult --status` does not list it.
//!
//! The record and the detach/status/wait lifecycle live in [`record`] and [`detach`];
//! this module owns the guards (roster selection, prompt size, allow list, live-run
//! exclusion), the launcher and the child environment (the review engine's own
//! [`crate::engines::claude_auth::child_env`], so a worker and a reviewer travel the
//! exact same route), and the run itself through the shared
//! [`crate::engines::subprocess::run_turn`].

pub mod detach;
pub mod engines;
pub mod record;

use std::path::{Path, PathBuf};
use std::time::Duration;

use c3_core::claude::{self, ClaudeWorkArgs, MAX_PROMPT_BYTES};
use c3_core::roster::{Roster, RosterEntry};

use crate::engines::claude_auth;
use crate::engines::subprocess::{run_turn, SpawnRequest};
use crate::providers;
use chrono::{DateTime, Utc};
use engines::WorkerEngine;
use record::{
    diff_stat, diff_text, files_changed, read_record, status_short, write_record, Quota, RunRecord,
};

const TOOL: &str = "c3 work";

/// The efforts a work run accepts (the same ladder the consult purposes use).
const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh"];

/// How long `c3 work` waits before it resumes a quota-hit child when the provider gave no reset
/// time (5 h, the usual coding-plan window). `--quota-window-sec` and a roster entry's
/// `ext.quota_window_sec` override it.
pub(crate) const QUOTA_WINDOW_DEFAULT_SEC: i64 = 18000;
/// How many times a work run may be resumed before it needs a human (`--max-resumes` overrides).
pub(crate) const MAX_RESUMES_DEFAULT: i64 = 3;
/// The prompt a quota resume sends on stdin: the brief file is NOT re-sent - the session has it.
const RESUME_PROMPT: &str = "Your usage limit has reset. Continue the brief from where you stopped; do not redo finished steps; finish with the final report.";
/// The alarm's sleep slice: the detached background wakes this often to check the kick file.
const ALARM_SLICE_SEC: i64 = 30;

fn iso_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

fn refuse(msg: &str) -> i32 {
    eprintln!("{TOOL}: {msg}");
    1
}

/// Everything one `c3 work` was told (the CLI's flags, the wire of a detached run's
/// background). `model` empty = the roster entry's; `max_turns`/`timeout_sec` `0` = the
/// defaults (80 / 3600); `allow` empty = [`c3_core::claude::WORK_DEFAULT_ALLOWS`].
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub task: String,
    /// The brief file (repo-relative or absolute); resolved to an absolute path before a
    /// run starts, so the detached background (whose cwd is the repo root) reads the same
    /// file.
    pub brief: String,
    pub provider: String,
    pub model: String,
    /// Extra ask text appended to the prompt after the brief.
    pub prompt: String,
    /// low | medium | high | xhigh ("" = send none).
    pub effort: String,
    pub max_turns: i64,
    pub timeout_sec: i64,
    pub allow: Vec<String>,
    pub collab_dir: String,
    pub detach: bool,
    pub status: bool,
    pub wait: bool,
    /// The detach id (or its beginning) `--status`/`--wait` narrows to.
    pub id: String,
    pub wait_timeout_sec: i64,
    /// Whether `--wait-timeout-sec` was given at all (an explicit `0` is refused).
    pub wait_timeout_sec_given: bool,
    /// INTERNAL: the background process of `--detach`; never pass it yourself.
    pub detach_id: String,
    /// `--resume`: continue a `quota` (or `timeout`) run's child in the same session. The run is
    /// the task's single resumable run, or the one `--id` narrows to.
    pub resume: bool,
    /// `--kick`: wake a `waiting_quota` run's sleeper at once (it resumes now), or resume a
    /// foreground `quota` run directly.
    pub kick: bool,
    /// `--quota-window-sec`: how long to wait before resuming when the provider gave no reset
    /// time (`0` = the roster entry's `ext.quota_window_sec`, else 18000 = 5 h).
    pub quota_window_sec: i64,
    /// `--max-resumes`: the resume cap of one run (`0` = 3).
    pub max_resumes: i64,
}

/// A work run with every guard passed: the resolved paths, the roster entry, the built
/// prompt and allow list, and the launcher. [`Options`] alone is not enough to run a
/// child — the background re-derives this from its wire before it starts one.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub repo_root: PathBuf,
    pub collab_root: PathBuf,
    pub task_dir: PathBuf,
    /// `<collab>/<task>/work` — the task's work-run root.
    pub work_root: PathBuf,
    /// The roster label as written (the record's provider).
    pub provider: String,
    /// The model id sent (`--model`, else the entry's).
    pub model: String,
    /// The worker engine of the entry (claude | codex | muse).
    pub engine: WorkerEngine,
    pub entry: RosterEntry,
    /// The `--allowedTools` list (the defaults when `--allow` was empty).
    pub allow: Vec<String>,
    /// The launcher path as resolved.
    pub launcher: String,
    /// The prompt as sent (preamble + brief + `--prompt`).
    pub prompt: String,
}

/// The repo root, the collab root and the task directory of `o` (a relative `--collab-dir`
/// resolves against the repo root, as `c3 consult` resolves it). The task slug is checked
/// here: it names a directory under the collab root, so it may not escape it.
pub fn prepare_paths(o: &Options) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let task = o.task.trim();
    if task.is_empty() {
        return Err("--task is required (a slug of the collab dir: letters, digits, dot, dash, underscore).".into());
    }
    if task.len() > 64
        || task == "."
        || task == ".."
        || !task
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(format!(
            "--task must be a slug of letters, digits, dot, dash and underscore (got '{task}')."
        ));
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let given = Path::new(o.collab_dir.trim());
    let collab_root = if given.is_absolute() {
        given.to_path_buf()
    } else {
        repo_root.join(given)
    };
    let task_dir = collab_root.join(task);
    Ok((repo_root, collab_root, task_dir))
}

/// The roster's worker entries: a worker engine (claude | codex | muse), role `worker` (the
/// EXISTING `roles` key — the design changes no schema).
fn worker_entries(roster: &Roster) -> Vec<&RosterEntry> {
    roster
        .entries
        .iter()
        .filter(|e| {
            WorkerEngine::parse(&e.engine).is_some() && e.roles.iter().any(|r| r == "worker")
        })
        .collect()
}

fn worker_list(roster: &Roster) -> String {
    let workers = worker_entries(roster);
    if workers.is_empty() {
        "the roster has no worker entries (a worker engine — claude, codex or muse — with role \"worker\"); add \"roles\": [\"worker\"] to such an entry first.".to_string()
    } else {
        format!(
            "the roster's worker entries: {}",
            workers
                .iter()
                .map(|e| format!("{} :: {} [{}]", e.provider, e.model, e.engine))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// The `--provider` lookup: a worker entry by label, with the design's refusals (a
/// non-worker label, a non-worker engine, an unknown label — each naming the workers).
fn select_worker(roster: &Roster, label: &str) -> Result<RosterEntry, String> {
    let workers = worker_entries(roster);
    if let Some(e) = workers.iter().find(|e| e.provider == label) {
        return Ok((*e).clone());
    }
    if let Some(e) = roster.entries.iter().find(|e| e.provider == label) {
        if WorkerEngine::parse(&e.engine).is_none() {
            return Err(format!(
                "the roster entry '{label}' runs on the '{}' engine; workers run on the engines {}. {}",
                e.engine,
                engines::WORKER_ENGINE_LIST,
                worker_list(roster)
            ));
        }
        return Err(format!(
            "the roster entry '{label}' is not a worker (its roles have no \"worker\"). {}",
            worker_list(roster)
        ));
    }
    Err(format!(
        "no roster entry '{label}'. {}",
        worker_list(roster)
    ))
}

/// Every guard a run needs, in one place (the foreground and the detached background
/// both come through here; the live-run exclusion is NOT here — the run's own `running`
/// record is live by then, so [`run`] checks it before it allocates one).
fn prepare(o: &mut Options) -> Result<Prepared, String> {
    if !o.effort.trim().is_empty() && !EFFORTS.contains(&o.effort.trim()) {
        return Err(format!(
            "--effort must be one of {} (got '{}')",
            EFFORTS.join("|"),
            o.effort
        ));
    }
    if o.brief.trim().is_empty() {
        return Err("--brief is required (a file the worker works from).".into());
    }
    if o.provider.trim().is_empty() {
        return Err("--provider is required (a roster label with role \"worker\").".into());
    }
    let (repo_root, collab_root, task_dir) = prepare_paths(o)?;
    let work_root = task_dir.join("work");

    // The brief: repo-relative or absolute, resolved to absolute so the detached
    // background (cwd = the repo root) reads the same file.
    let brief_path = Path::new(&o.brief);
    let brief_abs = if brief_path.is_absolute() {
        brief_path.to_path_buf()
    } else {
        repo_root.join(brief_path)
    };
    let brief_text = std::fs::read_to_string(&brief_abs).map_err(|e| {
        format!(
            "the brief '{}' could not be read ({e})",
            brief_abs.display()
        )
    })?;
    o.brief = brief_abs.to_string_lossy().to_string();

    // The prompt: preamble + brief + --prompt, refused above the engine's cap.
    let mut prompt = format!("{}\n\n{}", claude::WORK_PREAMBLE, brief_text);
    if !o.prompt.trim().is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&o.prompt);
    }
    if prompt.len() as u64 > MAX_PROMPT_BYTES {
        return Err(format!(
            "the prompt is {} bytes, above the engine's {}-byte cap; shorten the brief",
            prompt.len(),
            MAX_PROMPT_BYTES
        ));
    }

    // The roster entry.
    let roster = providers::read_reviewer_roster()?;
    let entry = select_worker(&roster, o.provider.trim())?;
    let engine = WorkerEngine::parse(&entry.engine).ok_or_else(|| {
        format!(
            "the roster entry '{}' runs on the '{}' engine; workers run on the engines {}",
            o.provider.trim(),
            entry.engine,
            engines::WORKER_ENGINE_LIST
        )
    })?;
    let model = if o.model.trim().is_empty() {
        entry.model.clone()
    } else {
        o.model.trim().to_string()
    };

    // The engine's credentials must be usable NOW: a guard of the design's "before anything
    // starts" list, so it is checked here too — `run_prepared` re-checks it, but by then a
    // record directory would already exist. claude: the endpoint's token variable; muse: the
    // per-token-billing / oauth launch guard; codex: none (a missing ChatGPT login fails the run).
    engine_launch_guard(engine, &entry)?;

    // The allow list: the defaults when empty; a pattern naming git is refused — a
    // worker never commits.
    let allow = if o.allow.is_empty() {
        claude::WORK_DEFAULT_ALLOWS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        o.allow.clone()
    };
    if let Some(p) = allow.iter().find(|p| p.to_lowercase().contains("git")) {
        return Err(format!(
            "--allow '{p}' names git: a worker never runs git (the preamble forbids it; the allow list must not invite it)"
        ));
    }

    // The defaults a wire must carry explicitly.
    if o.max_turns <= 0 {
        o.max_turns = claude::WORK_MAX_TURNS_DEFAULT as i64;
    }
    if o.timeout_sec <= 0 {
        o.timeout_sec = 3600;
    }

    // The launcher (the review engine's own resolution: the PATH names, then the
    // install locations).
    let launcher = resolve_worker_launcher(engine)?;

    Ok(Prepared {
        repo_root,
        collab_root,
        task_dir,
        work_root,
        provider: entry.provider.clone(),
        model,
        engine,
        entry,
        allow,
        launcher,
        prompt,
    })
}

/// The "before anything starts" credential guard of a worker engine: claude re-uses the review
/// engine's endpoint environment (its `problem` is the unset-token refusal); muse re-uses the
/// per-token-billing / oauth launch guard; codex has none.
fn engine_launch_guard(engine: WorkerEngine, entry: &RosterEntry) -> Result<(), String> {
    match engine {
        WorkerEngine::Claude => {
            let env = claude_auth::child_env(&entry.auth, entry.endpoint.as_ref());
            if env.problem.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "the entry's endpoint is not usable: {}",
                    env.problem
                ))
            }
        }
        WorkerEngine::Muse => {
            let block = providers::get_muse_launch_block();
            if block.is_empty() {
                Ok(())
            } else {
                Err(format!("the muse engine is not usable: {block}"))
            }
        }
        WorkerEngine::Codex => Ok(()),
    }
}

/// The child environment of a worker turn plus the scrubbed variable names for the record.
type ChildEnvAndScrubbed = (Option<Vec<(String, String)>>, Vec<String>);

/// The child environment of a worker turn: claude travels the endpoint route (the token read at
/// launch, host markers scrubbed); codex and muse inherit the parent environment (as the review
/// adapters do, `env: None`). Returns `(env, scrubbed names)`.
fn worker_child_env(
    engine: WorkerEngine,
    entry: &RosterEntry,
) -> Result<ChildEnvAndScrubbed, String> {
    match engine {
        WorkerEngine::Claude => {
            let env = claude_auth::child_env(&entry.auth, entry.endpoint.as_ref());
            if !env.problem.is_empty() {
                return Err(format!(
                    "the entry's endpoint is not usable: {}",
                    env.problem
                ));
            }
            Ok((Some(env.env), c3_core::host::host_marker_names()))
        }
        WorkerEngine::Codex | WorkerEngine::Muse => {
            engine_launch_guard(engine, entry)?;
            Ok((None, Vec::new()))
        }
    }
}

/// Resolve the launcher of a worker engine (the review engine's own discovery).
fn resolve_worker_launcher(engine: WorkerEngine) -> Result<String, String> {
    match providers::resolve_engine_launcher(engine.as_str(), "") {
        Ok(Some(l)) => Ok(l),
        Ok(None) => Err(format!(
            "the {} launcher was not found (none of its PATH names or install locations)",
            engine.as_str()
        )),
        Err(e) => Err(e),
    }
}

/// Mark the task's `running` record `refused` (a guard failed after the record was
/// written; the record stays as the evidence).
fn refuse_record(rec_dir: &Path, why: &str) -> i32 {
    if let Ok(Some(mut rec)) = read_record(rec_dir) {
        rec.status = "refused".into();
        rec.ended = Some(iso_now());
        let _ = write_record(rec_dir, &rec);
    }
    refuse(why)
}

/// Run the work child against an existing `running` record (the foreground after it
/// allocated the record; the detached background after it took the wire). Re-derives
/// [`Prepared`] — every guard runs again — then writes `brief.md`, runs the child with
/// the endpoint's child environment, and makes the record final.
pub fn run_prepared(o: &Options, rec_dir: &Path, _id: &str, _seq: i64, detached: bool) -> i32 {
    let mut o = o.clone();
    let pre = match prepare(&mut o) {
        Ok(p) => p,
        Err(why) => return refuse_record(rec_dir, &why),
    };

    // brief.md: the prompt as sent (muse also reads it as its `--prompt-file`).
    let brief_md = rec_dir.join("brief.md");
    if let Err(e) = std::fs::write(&brief_md, &pre.prompt) {
        return refuse_record(rec_dir, &format!("could not write brief.md ({e})"));
    }

    // The child environment: claude travels the endpoint route (token read at launch, host
    // markers scrubbed); codex/muse inherit the parent environment (`env: None`).
    let (child_env, scrubbed) = match worker_child_env(pre.engine, &pre.entry) {
        Ok(x) => x,
        Err(why) => return refuse_record(rec_dir, &why),
    };

    // The argv (no secrets travel in it) and the record's launch fields. claude/muse mint the
    // session id up front, so a quota resume continues THIS session; codex names its thread in
    // the stream, read back after the run.
    let repo_root_s = pre.repo_root.to_string_lossy().to_string();
    let effort = (!o.effort.trim().is_empty()).then(|| o.effort.trim().to_string());
    let mut session_id = match pre.engine {
        WorkerEngine::Claude | WorkerEngine::Muse => uuid::Uuid::new_v4().to_string(),
        WorkerEngine::Codex => String::new(),
    };
    let argv = match pre.engine {
        WorkerEngine::Claude => claude::work_args(&ClaudeWorkArgs {
            model: pre.model.clone(),
            effort: effort.clone(),
            max_turns: o.max_turns.clamp(1, u32::MAX as i64) as u32,
            allowed_tools: pre.allow.clone(),
            session_id: session_id.clone(),
            resume: false,
        }),
        WorkerEngine::Codex => {
            engines::codex_work_args(&pre.model, effort.as_deref(), &repo_root_s, None)
        }
        WorkerEngine::Muse => engines::muse_work_args(
            &pre.model,
            effort.as_deref(),
            &repo_root_s,
            &brief_md.to_string_lossy(),
            &session_id,
        ),
    };
    let before = status_short(&pre.repo_root);
    if let Ok(Some(mut rec)) = read_record(rec_dir) {
        rec.argv = argv.clone();
        rec.child_env_scrubbed = scrubbed;
        rec.timeout_sec = o.timeout_sec;
        rec.session_id = session_id.clone();
        rec.baseline = before.clone();
        if let Err(e) = write_record(rec_dir, &rec) {
            return refuse(&format!(
                "the work run could not report to {} ({e}); nothing was started.",
                rec_dir.join("run.json").display()
            ));
        }
    }

    println!(
        "{TOOL}: work run starting ({} :: {} [{}]) in {}",
        pre.provider,
        pre.model,
        pre.engine.as_str(),
        pre.repo_root.display()
    );
    let events_path = rec_dir.join("events.jsonl");
    let stderr_path = rec_dir.join(".stderr");
    // muse takes its prompt through the `--prompt-file` in the argv (stdin empty); claude/codex
    // take it on stdin.
    let stdin_text = match pre.engine {
        WorkerEngine::Muse => "",
        _ => pre.prompt.as_str(),
    };
    let spawn = SpawnRequest {
        launcher: &pre.launcher,
        argv: &argv,
        cwd: &pre.repo_root,
        prompt_delivery: pre.engine.prompt_delivery(),
        stdin_text,
        events_path: &events_path,
        stderr_path: &stderr_path,
        timeout: Duration::from_secs(o.timeout_sec.max(1) as u64),
        stall_sec: 0,
        kick_path: None,
        tool_flight: None,
        on_running: None,
        on_kill: None,
        env: child_env.as_deref(),
        crt_quote: matches!(pre.engine, WorkerEngine::Claude),
        budget: None,
    };
    let result = run_turn(&spawn);

    // codex names its thread only in the stream: read it back so a resume can continue it.
    let events_text = std::fs::read_to_string(&events_path).unwrap_or_default();
    if session_id.trim().is_empty() {
        let parsed = engines::session_from_events(pre.engine, &events_text);
        if !parsed.is_empty() {
            session_id = parsed;
        }
    }

    // Classify THIS attempt from its own stream/stderr, then refresh the record artifacts over
    // the whole stream and make the record final (quota included).
    let (status, quota_text, reset_at) = classify_attempt(
        pre.engine,
        &events_text,
        &std::fs::read_to_string(&stderr_path).unwrap_or_default(),
        result.stop == crate::engines::subprocess::TurnStop::Timeout,
        result.exit_code,
    );
    let window = eff_window(&o, pre.entry.quota_window_sec);
    let mut rec = refresh_artifacts(pre.engine, rec_dir, &pre.repo_root, &before);
    rec.session_id = session_id.clone();
    rec.exit_code = result.exit_code.map(|c| c as i64);
    rec.timeout_sec = o.timeout_sec;
    rec.status = status.clone();
    rec.ended = Some(iso_now());
    if status == "quota" {
        rec.quota = Some(build_quota(&quota_text, reset_at, window, 0, false));
    }
    if let Err(e) = write_record(rec_dir, &rec) {
        return refuse(&format!(
            "the work run could not write its final record ({e})."
        ));
    }
    if !result.started {
        return refuse(&format!(
            "the claude process did not start ({})",
            result.error.unwrap_or_else(|| "no reason given".into())
        ));
    }
    print_run_line(&rec);

    // A quota hit: a detached run sleeps and resumes itself (the alarm); a foreground run prints
    // how to resume and exits 3.
    if rec.status == "quota" {
        if detached {
            return run_alarm(&o, rec_dir);
        }
        return foreground_quota(&o, rec_dir, &rec);
    }
    exit_for(&rec.status)
}

/// The run-summary line printed when a work attempt ends.
fn print_run_line(rec: &RunRecord) {
    println!(
        "{TOOL}: work run {} ({} :: {}) - turns {}, tool calls {}, files changed: {}",
        rec.status,
        rec.provider,
        rec.model,
        rec.turns
            .map(|t| t.to_string())
            .unwrap_or_else(|| "(none)".into()),
        if rec.tool_calls.is_empty() {
            "(none)".to_string()
        } else {
            rec.tool_calls
                .iter()
                .map(|(k, v)| format!("{k} x{v}"))
                .collect::<Vec<_>>()
                .join(", ")
        },
        if rec.files_changed.is_empty() {
            "(none)".to_string()
        } else {
            rec.files_changed.join(", ")
        },
    );
}

/// The exit code of a terminal work status: `done` -> 0, anything else (`died`, `timeout`,
/// `refused`, `quota`) -> 1. A foreground `quota` returns 3 of its own (see [`foreground_quota`]).
fn exit_for(status: &str) -> i32 {
    if status == "done" {
        0
    } else {
        1
    }
}

/// The effective quota window: `--quota-window-sec`, else the roster entry's `ext.quota_window_sec`,
/// else [`QUOTA_WINDOW_DEFAULT_SEC`].
fn eff_window(o: &Options, entry_window: i64) -> i64 {
    if o.quota_window_sec > 0 {
        o.quota_window_sec
    } else if entry_window > 0 {
        entry_window
    } else {
        QUOTA_WINDOW_DEFAULT_SEC
    }
}

/// The effective resume cap: `--max-resumes`, else [`MAX_RESUMES_DEFAULT`].
fn eff_max_resumes(o: &Options) -> i64 {
    if o.max_resumes > 0 {
        o.max_resumes
    } else {
        MAX_RESUMES_DEFAULT
    }
}

/// Classify one work attempt from ITS OWN stream and stderr (never a prior attempt's): the base
/// status (`timeout` | `done` | `died`), turned into `quota` when the attempt did not succeed and
/// the engine found a usage limit. Returns `(status, quota_text, reset_at)`.
fn classify_attempt(
    engine: WorkerEngine,
    events_text: &str,
    stderr_text: &str,
    timed_out: bool,
    exit_code: Option<i32>,
) -> (String, String, Option<DateTime<Utc>>) {
    let (quota_text, reset_at) = engines::quota_evidence(engine, events_text, stderr_text);
    let base = if timed_out {
        "timeout"
    } else if exit_code == Some(0) {
        "done"
    } else {
        "died"
    };
    let status = if base != "done" && !quota_text.is_empty() {
        "quota".to_string()
    } else {
        base.to_string()
    };
    (status, quota_text, reset_at)
}

/// A fresh [`Quota`] block: `resume_at` = `reset_at` + 60 s when the provider gave a reset time,
/// else now + the quota window.
fn build_quota(
    quota_text: &str,
    reset_at: Option<DateTime<Utc>>,
    window_sec: i64,
    resumes: i64,
    exhausted: bool,
) -> Quota {
    let resume_at = match reset_at {
        Some(r) => r + chrono::Duration::seconds(60),
        None => Utc::now() + chrono::Duration::seconds(window_sec.max(60)),
    };
    Quota {
        text: quota_text.to_string(),
        reset_at: reset_at.map(|r| r.to_rfc3339()),
        resume_at: resume_at.to_rfc3339(),
        resumes,
        exhausted,
    }
}

/// Recompute `report.md`, `diff.txt` and the summary fields over the WHOLE event stream (every
/// attempt - the `{"type":"c3","subtype":"resume"}` separators are skipped by the summary) and the
/// current git state against `before` (the first launch's baseline). Returns the record with its
/// summary fields filled; the caller sets `status`, `ended`, `exit_code` and `quota`.
fn refresh_artifacts(
    engine: WorkerEngine,
    rec_dir: &Path,
    repo_root: &Path,
    before: &[String],
) -> RunRecord {
    let events_text = std::fs::read_to_string(rec_dir.join("events.jsonl")).unwrap_or_default();
    let sum = engines::summarize(engine, &events_text);
    let _ = std::fs::write(rec_dir.join("report.md"), &sum.report);
    let after = status_short(repo_root);
    let stat = diff_stat(repo_root);
    let _ = std::fs::write(rec_dir.join("diff.txt"), diff_text(before, &after, &stat));
    let mut rec = read_record(rec_dir).ok().flatten().unwrap_or_default();
    rec.turns = sum.turns;
    rec.tool_calls = sum.tool_calls;
    rec.files_changed = files_changed(before, &after);
    rec.usage = sum.usage;
    rec
}

/// The entry point: `--status`/`--wait` read the records only; `--detach-id` is the
/// background; anything else is a run (foreground, or detached once its record exists).
pub fn run(mut o: Options) -> i32 {
    if o.status || o.wait {
        return detach::query(&o);
    }
    if !o.detach_id.trim().is_empty() {
        return detach::background(&o);
    }
    if o.task.trim().is_empty() {
        return refuse("--task is required (see c3 work --help).");
    }
    if o.resume || o.kick {
        return resume_or_kick(&o);
    }
    let pre = match prepare(&mut o) {
        Ok(p) => p,
        Err(why) => return refuse(&why),
    };
    // One live work run per task.
    if let Some(id) = record::live_run(&pre.work_root) {
        return refuse(&format!(
            "task '{}' already has a live work run ({id}); c3 work --task {}{} --wait waits for it",
            o.task,
            o.task,
            if o.collab_dir != ".collab" {
                format!(" --collab-dir \"{}\"", o.collab_dir)
            } else {
                String::new()
            }
        ));
    }
    let (id, seq, rec_dir) = match record::allocate(&pre.work_root, &pre.provider, &pre.model) {
        Ok(x) => x,
        Err(e) => return refuse(&e),
    };
    let rec = RunRecord {
        id: id.clone(),
        task: o.task.clone(),
        seq,
        provider: pre.provider.clone(),
        model: pre.model.clone(),
        engine: pre.engine.as_str().into(),
        child_env_scrubbed: Vec::new(),
        started: iso_now(),
        status: "running".into(),
        timeout_sec: o.timeout_sec,
        args: o.detach.then(|| detach::wire_of(&o, &pre.collab_root)),
        ..Default::default()
    };
    if let Err(e) = write_record(&rec_dir, &rec) {
        return refuse(&format!(
            "the work run could not write {} ({e}); nothing was started.",
            rec_dir.join("run.json").display()
        ));
    }
    if o.detach {
        return detach::start_detached(&o, &pre, &id, &rec_dir);
    }
    run_prepared(&o, &rec_dir, &id, seq, false)
}

// ------------------------------------------------------------------ the quota resume / alarm

/// The alarm's decision for one sleep slice: sleep a bounded slice, resume now, or give up because
/// the resume cap is spent. A PURE function of the clock, the reset time, the kick file and the
/// resume count - unit-tested without any real sleeping.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AlarmAction {
    Sleep(Duration),
    Resume,
    GiveUp,
}

/// [`AlarmAction`] for `now`: give up when `resumes >= max`; resume when the kick file exists or
/// the window has reset; else sleep up to [`ALARM_SLICE_SEC`] (never past `resume_at`, never 0).
pub(crate) fn next_action(
    now: DateTime<Utc>,
    resume_at: DateTime<Utc>,
    kick_exists: bool,
    resumes: i64,
    max: i64,
) -> AlarmAction {
    if resumes >= max {
        return AlarmAction::GiveUp;
    }
    if kick_exists || now >= resume_at {
        return AlarmAction::Resume;
    }
    let secs = (resume_at - now).num_seconds().clamp(1, ALARM_SLICE_SEC);
    AlarmAction::Sleep(Duration::from_secs(secs as u64))
}

/// An RFC3339 instant, defaulting to now (resume at once) when it does not parse.
fn parse_utc(rfc: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(rfc)
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// An RFC3339 instant as a local wall-clock time (the string unchanged when it does not parse).
fn local_time(rfc: &str) -> String {
    match DateTime::parse_from_rfc3339(rfc) {
        Ok(t) => t
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S %Z")
            .to_string(),
        Err(_) => rfc.to_string(),
    }
}

/// The session id to resume: the record's stored one, else the value after `--session-id` in its
/// argv (a record that predates quota resume).
fn session_from_argv(argv: &[String]) -> Option<String> {
    argv.iter()
        .position(|a| a == "--session-id")
        .and_then(|i| argv.get(i + 1))
        .cloned()
}

/// Everything a resume needs, re-derived from the record and the roster (NOT from the brief: the
/// session already holds it). Built once per `--resume`/`--kick` and once per detached alarm.
struct ResumeEnv {
    engine: WorkerEngine,
    repo_root: PathBuf,
    launcher: String,
    /// The child environment (claude's endpoint route) or `None` to inherit (codex/muse).
    env: Option<Vec<(String, String)>>,
    argv: Vec<String>,
    prompt_delivery: c3_core::engine::PromptDelivery,
    /// The resume nudge on stdin (claude/codex); empty for muse (it reads a `--prompt-file`).
    stdin_text: String,
    crt_quote: bool,
    timeout_sec: i64,
    max_resumes: i64,
    window_sec: i64,
}

fn resume_env(o: &Options, rec: &RunRecord, rec_dir: &Path) -> Result<ResumeEnv, String> {
    let engine = WorkerEngine::parse(&rec.engine).ok_or_else(|| {
        format!(
            "the work record's engine '{}' is not a worker engine ({})",
            rec.engine,
            engines::WORKER_ENGINE_LIST
        )
    })?;
    let (repo_root, _collab, _task_dir) = prepare_paths(o)?;
    let roster = providers::read_reviewer_roster()?;
    let entry = select_worker(&roster, &rec.provider)?;
    let (env, _scrubbed) = worker_child_env(engine, &entry)?;
    let launcher = resolve_worker_launcher(engine)?;
    let session_id = if !rec.session_id.trim().is_empty() {
        rec.session_id.clone()
    } else {
        session_from_argv(&rec.argv).unwrap_or_default()
    };
    // muse resumes through a `--prompt-file`; claude/codex through stdin.
    let resume_prompt_file = rec_dir.join(".resume-prompt.md");
    let (prompt_delivery, stdin_text) = match engine {
        WorkerEngine::Muse => {
            std::fs::write(&resume_prompt_file, RESUME_PROMPT)
                .map_err(|e| format!("could not write the resume prompt ({e})"))?;
            (c3_core::engine::PromptDelivery::PromptFile, String::new())
        }
        _ => (
            c3_core::engine::PromptDelivery::Stdin,
            RESUME_PROMPT.to_string(),
        ),
    };
    let argv = engines::resume_args(
        engine,
        &rec.argv,
        &session_id,
        &resume_prompt_file.to_string_lossy(),
    )
    .ok_or_else(|| {
        "the record has no session to resume (a codex run that never named a thread, or an argv without a session handoff)".to_string()
    })?;
    Ok(ResumeEnv {
        engine,
        repo_root,
        launcher,
        env,
        argv,
        prompt_delivery,
        stdin_text,
        crt_quote: matches!(engine, WorkerEngine::Claude),
        timeout_sec: if rec.timeout_sec > 0 {
            rec.timeout_sec
        } else {
            3600
        },
        max_resumes: eff_max_resumes(o),
        window_sec: eff_window(o, entry.quota_window_sec),
    })
}

/// Append the resume separator line and this attempt's stream to `events.jsonl` (verbatim, the
/// whole record kept in order).
fn append_resume(rec_dir: &Path, n: i64, attempt_events: &str) {
    use std::io::Write;
    let path = rec_dir.join("events.jsonl");
    let needs_nl = std::fs::read(&path)
        .ok()
        .map(|b| !b.is_empty() && b.last() != Some(&b'\n'))
        .unwrap_or(false);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        if needs_nl {
            let _ = f.write_all(b"\n");
        }
        let sep = format!(
            "{{\"type\":\"c3\",\"subtype\":\"resume\",\"n\":{n},\"at\":\"{}\"}}\n",
            Utc::now().to_rfc3339()
        );
        let _ = f.write_all(sep.as_bytes());
        let trimmed = attempt_events.trim_end_matches(['\n', '\r']);
        if !trimmed.is_empty() {
            let _ = f.write_all(trimmed.as_bytes());
            let _ = f.write_all(b"\n");
        }
    }
}

/// Resume the child once: relaunch with `--resume`, append the attempt to `events.jsonl`,
/// recompute every artifact over the whole record, bump `quota.resumes`, and make the record final
/// again (quota, exhausted, done, died or timeout). Returns the new status.
fn resume_once(renv: &ResumeEnv, rec_dir: &Path) -> Result<String, String> {
    let rec = read_record(rec_dir)
        .ok()
        .flatten()
        .ok_or_else(|| "the work record vanished".to_string())?;
    let n = rec.quota.as_ref().map(|q| q.resumes).unwrap_or(0) + 1;
    println!(
        "{TOOL}: resuming work run {} ({} :: {}) - resume {}/{}",
        rec.id, rec.provider, rec.model, n, renv.max_resumes
    );
    let tmp_events = rec_dir.join(".events-resume.jsonl");
    let tmp_stderr = rec_dir.join(".stderr-resume");
    let _ = std::fs::remove_file(&tmp_events);
    let _ = std::fs::remove_file(&tmp_stderr);
    let spawn = SpawnRequest {
        launcher: &renv.launcher,
        argv: &renv.argv,
        cwd: &renv.repo_root,
        prompt_delivery: renv.prompt_delivery,
        stdin_text: &renv.stdin_text,
        events_path: &tmp_events,
        stderr_path: &tmp_stderr,
        timeout: Duration::from_secs(renv.timeout_sec.max(1) as u64),
        stall_sec: 0,
        kick_path: None,
        tool_flight: None,
        on_running: None,
        on_kill: None,
        env: renv.env.as_deref(),
        crt_quote: renv.crt_quote,
        budget: None,
    };
    let result = run_turn(&spawn);
    let attempt_events = std::fs::read_to_string(&tmp_events).unwrap_or_default();
    let attempt_stderr = std::fs::read_to_string(&tmp_stderr).unwrap_or_default();
    append_resume(rec_dir, n, &attempt_events);
    let _ = std::fs::remove_file(&tmp_events);
    let _ = std::fs::remove_file(&tmp_stderr);

    let (status, quota_text, reset_at) = classify_attempt(
        renv.engine,
        &attempt_events,
        &attempt_stderr,
        result.stop == crate::engines::subprocess::TurnStop::Timeout,
        result.exit_code,
    );
    let before = rec.baseline.clone();
    let mut nrec = refresh_artifacts(renv.engine, rec_dir, &renv.repo_root, &before);
    nrec.exit_code = result.exit_code.map(|c| c as i64);
    nrec.ended = Some(iso_now());
    nrec.status = status.clone();
    if status == "quota" {
        let exhausted = n >= renv.max_resumes;
        nrec.quota = Some(build_quota(
            &quota_text,
            reset_at,
            renv.window_sec,
            n,
            exhausted,
        ));
    } else if let Some(q) = nrec.quota.as_mut() {
        // success/death after a quota: keep the last evidence, record how many resumes it took
        q.resumes = n;
    }
    write_record(rec_dir, &nrec)?;
    print_run_line(&nrec);
    if !result.started {
        return Err(format!(
            "the claude process did not start ({})",
            result.error.unwrap_or_else(|| "no reason given".into())
        ));
    }
    Ok(status)
}

/// A foreground run parked on quota: print the resume time and the exact resume command, exit 3;
/// or, when the resume cap is spent, report it and exit 1.
fn foreground_quota(o: &Options, rec_dir: &Path, rec: &RunRecord) -> i32 {
    let q = rec.quota.clone().unwrap_or_default();
    let collab_flag = if o.collab_dir != ".collab" && !o.collab_dir.is_empty() {
        format!(" --collab-dir \"{}\"", o.collab_dir)
    } else {
        String::new()
    };
    if q.exhausted {
        eprintln!(
            "{TOOL}: work run {} hit its usage limit and spent the resume cap ({}): {}. It needs a human; the record is at {}.",
            rec.id,
            eff_max_resumes(o),
            q.text,
            rec_dir.display()
        );
        return 1;
    }
    println!("{TOOL}: work run {} hit a usage limit: {}", rec.id, q.text);
    println!("resume at : {} ({})", local_time(&q.resume_at), q.resume_at);
    println!(
        "resume now: c3 work --task {}{} --resume {}",
        o.task, collab_flag, rec.id
    );
    3
}

/// The detached alarm: the background does NOT exit on quota. It parks the run `waiting_quota`,
/// sleeps in slices (waking on the kick file), resumes the child in-process when the window
/// resets, and repeats on a further quota hit until the cap. Returns the final exit code.
fn run_alarm(o: &Options, rec_dir: &Path) -> i32 {
    let rec0 = match read_record(rec_dir) {
        Ok(Some(r)) => r,
        _ => return 1,
    };
    let renv = match resume_env(o, &rec0, rec_dir) {
        Ok(e) => e,
        Err(why) => {
            eprintln!("{TOOL}: cannot arm the quota resume for {}: {why}", rec0.id);
            return 1;
        }
    };
    let kick = rec_dir.join(".kick");
    loop {
        let rec = match read_record(rec_dir) {
            Ok(Some(r)) => r,
            _ => return 1,
        };
        if !record::is_pending(&rec.status) && rec.status != "quota" {
            return exit_for(&rec.status);
        }
        let q = match &rec.quota {
            Some(q) => q.clone(),
            None => return 1,
        };
        if q.exhausted {
            return 1;
        }
        // park the run waiting_quota (the background stays alive; its pid is kept)
        if rec.status != "waiting_quota" {
            let mut wrec = rec.clone();
            wrec.status = "waiting_quota".into();
            let _ = write_record(rec_dir, &wrec);
            println!(
                "{TOOL}: work run {} is waiting for its usage limit to reset, until {} (resume {}/{})",
                rec.id,
                local_time(&q.resume_at),
                q.resumes,
                renv.max_resumes
            );
        }
        let resume_at = parse_utc(&q.resume_at);
        loop {
            match next_action(
                Utc::now(),
                resume_at,
                kick.exists(),
                q.resumes,
                renv.max_resumes,
            ) {
                AlarmAction::Sleep(d) => std::thread::sleep(d),
                AlarmAction::Resume => {
                    let _ = std::fs::remove_file(&kick);
                    break;
                }
                AlarmAction::GiveUp => {
                    let mut er = rec.clone();
                    if let Some(qq) = er.quota.as_mut() {
                        qq.exhausted = true;
                    }
                    er.status = "quota".into();
                    let _ = write_record(rec_dir, &er);
                    return 1;
                }
            }
        }
        match resume_once(&renv, rec_dir) {
            Ok(status) => {
                if status != "quota" {
                    return exit_for(&status);
                }
            }
            Err(why) => {
                eprintln!("{TOOL}: resume of work run {} failed: {why}", rec.id);
                return 1;
            }
        }
    }
}

/// `--resume`/`--kick`: find the run and act. `--kick` wakes a sleeping detached run (or resumes a
/// foreground quota run directly); `--resume` resumes a `quota`/`timeout` run here and now.
fn resume_or_kick(o: &Options) -> i32 {
    let (_repo_root, _collab, task_dir) = match prepare_paths(o) {
        Ok(p) => p,
        Err(why) => return refuse(&why),
    };
    let work_root = task_dir.join("work");
    if o.kick {
        return do_kick(o, &work_root);
    }
    let (rec_dir, rec) = match pick_run(&work_root, &o.id, &["quota", "timeout"]) {
        Ok(x) => x,
        Err(why) => return refuse(&why),
    };
    do_resume_foreground(o, &rec_dir, &rec)
}

fn do_kick(o: &Options, work_root: &Path) -> i32 {
    let (rec_dir, rec) = match pick_run(work_root, &o.id, &["waiting_quota", "quota"]) {
        Ok(x) => x,
        Err(why) => return refuse(&why),
    };
    match rec.status.as_str() {
        "waiting_quota" => {
            if let Err(e) = std::fs::write(rec_dir.join(".kick"), b"1") {
                return refuse(&format!("could not write the kick file ({e})"));
            }
            println!(
                "{TOOL}: kicked work run {} - its sleeper will resume now.",
                rec.id
            );
            0
        }
        "quota" => do_resume_foreground(o, &rec_dir, &rec),
        other => refuse(&format!(
            "work run {} is in state {other}, not waiting on a quota reset; --kick has nothing to wake.",
            rec.id
        )),
    }
}

fn do_resume_foreground(o: &Options, rec_dir: &Path, rec: &RunRecord) -> i32 {
    if rec.status != "quota" && rec.status != "timeout" {
        return refuse(&format!(
            "work run {} is in state {}, not quota or timeout; nothing to resume.",
            rec.id, rec.status
        ));
    }
    if rec.quota.as_ref().map(|q| q.exhausted).unwrap_or(false) {
        return refuse(&format!(
            "work run {} has spent its resume cap ({}); it needs a human.",
            rec.id,
            eff_max_resumes(o)
        ));
    }
    let renv = match resume_env(o, rec, rec_dir) {
        Ok(e) => e,
        Err(why) => return refuse(&why),
    };
    match resume_once(&renv, rec_dir) {
        Ok(status) => {
            if status == "quota" {
                let nrec = read_record(rec_dir).ok().flatten().unwrap_or_default();
                foreground_quota(o, rec_dir, &nrec)
            } else {
                exit_for(&status)
            }
        }
        Err(why) => refuse(&why),
    }
}

/// Find the run to act on: when `id` narrows to exactly one, that run (whatever its status, so the
/// caller can refuse with the state); otherwise the single run whose status is one of `want`.
fn pick_run(work_root: &Path, id: &str, want: &[&str]) -> Result<(PathBuf, RunRecord), String> {
    let id = id.trim().to_lowercase();
    let mut all: Vec<(PathBuf, RunRecord)> = Vec::new();
    for (_, dir) in record::run_dirs(work_root) {
        if let Ok(Some(rec)) = read_record(&dir) {
            all.push((dir, rec));
        }
    }
    if all.is_empty() {
        return Err(format!(
            "the task has no work run (under {})",
            work_root.display()
        ));
    }
    if !id.is_empty() {
        let hits: Vec<_> = all
            .into_iter()
            .filter(|(_, r)| r.id.to_lowercase().starts_with(&id))
            .collect();
        return match hits.len() {
            0 => Err(format!("no work run has an id starting with '{id}'")),
            1 => Ok(hits.into_iter().next().unwrap()),
            _ => Err(format!(
                "'{id}' matches {} work runs; give a longer --id",
                hits.len()
            )),
        };
    }
    let hits: Vec<_> = all
        .into_iter()
        .filter(|(_, r)| want.contains(&r.status.as_str()))
        .collect();
    match hits.len() {
        0 => Err(format!(
            "the task has no work run in state {} (give --id to target one)",
            want.join(" or ")
        )),
        1 => Ok(hits.into_iter().next().unwrap()),
        _ => Err(format!(
            "the task has {} work runs in state {}; give --id to choose one",
            hits.len(),
            want.join(" or ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(provider: &str, model: &str, engine: &str, roles: &[&str]) -> RosterEntry {
        RosterEntry {
            provider: provider.into(),
            model: model.into(),
            engine: engine.into(),
            roles: roles.iter().map(|r| r.to_string()).collect(),
            ..Default::default()
        }
    }

    fn roster(entries: Vec<RosterEntry>) -> Roster {
        Roster {
            entries,
            ..Default::default()
        }
    }

    #[test]
    fn a_worker_entry_is_found_by_label() {
        let r = roster(vec![
            entry("ZAI-claude", "glm-5.3", "claude", &["reviewer"]),
            entry("ZAI-claude", "glm-5.3-flash", "claude", &["worker"]),
        ]);
        let e = select_worker(&r, "ZAI-claude").unwrap();
        assert_eq!(e.model, "glm-5.3-flash");
    }

    #[test]
    fn a_non_worker_label_is_refused_with_the_worker_list() {
        let r = roster(vec![entry(
            "ZAI-claude",
            "glm-5.3",
            "claude",
            &["reviewer"],
        )]);
        let err = select_worker(&r, "ZAI-claude").unwrap_err();
        assert!(err.contains("is not a worker"), "{err}");
        assert!(err.contains("no worker entries"), "{err}");
    }

    #[test]
    fn codex_and_muse_worker_entries_are_accepted() {
        let r = roster(vec![
            entry("openai", "gpt-6.1-sol", "codex", &["worker"]),
            entry("meta", "muse-spark-1.3-contributor", "muse", &["worker"]),
        ]);
        assert_eq!(select_worker(&r, "openai").unwrap().engine, "codex");
        assert_eq!(select_worker(&r, "meta").unwrap().engine, "muse");
    }

    #[test]
    fn a_non_worker_engine_is_refused_listing_three_engines() {
        // an entry on an engine that is not a worker engine (e.g. agy) is refused, naming the three.
        let r = roster(vec![entry("gemini", "gemini-3.8", "agy", &["worker"])]);
        let err = select_worker(&r, "gemini").unwrap_err();
        assert!(err.contains("runs on the 'agy' engine"), "{err}");
        assert!(err.contains("claude, codex or muse"), "{err}");
    }

    #[test]
    fn an_unknown_label_is_refused() {
        let r = roster(vec![entry("ZAI-claude", "glm-5.3", "claude", &["worker"])]);
        let err = select_worker(&r, "nobody").unwrap_err();
        assert!(err.contains("no roster entry 'nobody'"), "{err}");
        assert!(err.contains("ZAI-claude :: glm-5.3"), "{err}");
    }

    /// The alarm's pure decision: give up past the cap, resume on the kick file or once the window
    /// has reset, else sleep a bounded slice - no real sleeping.
    #[test]
    fn the_alarm_next_action() {
        let now = Utc::now();
        let soon = now + chrono::Duration::seconds(10);
        let far = now + chrono::Duration::seconds(3600);
        let past = now - chrono::Duration::seconds(5);
        // cap spent -> give up, whatever the clock
        assert_eq!(next_action(now, past, false, 3, 3), AlarmAction::GiveUp);
        assert_eq!(next_action(now, far, true, 3, 3), AlarmAction::GiveUp);
        // the kick file resumes at once
        assert_eq!(next_action(now, far, true, 0, 3), AlarmAction::Resume);
        // the window has reset -> resume
        assert_eq!(next_action(now, past, false, 1, 3), AlarmAction::Resume);
        assert_eq!(next_action(now, now, false, 0, 3), AlarmAction::Resume);
        // a near reset sleeps only up to it; a far one sleeps a full slice
        assert_eq!(
            next_action(now, soon, false, 0, 3),
            AlarmAction::Sleep(Duration::from_secs(10))
        );
        assert_eq!(
            next_action(now, far, false, 0, 3),
            AlarmAction::Sleep(Duration::from_secs(ALARM_SLICE_SEC as u64))
        );
    }

    /// `build_quota`: a known reset gives resume_at = reset + 60 s; an unknown one gives
    /// now + the window.
    #[test]
    fn build_quota_resume_at() {
        let reset = Utc::now() + chrono::Duration::seconds(100);
        let q = build_quota("usage limit", Some(reset), 18000, 0, false);
        assert_eq!(q.reset_at.as_deref(), Some(reset.to_rfc3339()).as_deref());
        let ra = parse_utc(&q.resume_at);
        assert_eq!((ra - reset).num_seconds(), 60);
        let q2 = build_quota("usage limit", None, 120, 2, true);
        assert!(q2.reset_at.is_none());
        assert!(q2.exhausted && q2.resumes == 2);
        let ra2 = parse_utc(&q2.resume_at);
        assert!((ra2 - Utc::now()).num_seconds() >= 110);
    }

    /// `session_from_argv` finds the `--session-id` value in a stored claude/muse argv (the
    /// per-engine resume transforms themselves are tested in [`engines`]).
    #[test]
    fn session_from_argv_reads_the_session_flag() {
        let argv: Vec<String> = ["-p", "--model", "glm-5.3", "--session-id", "sess-1"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(session_from_argv(&argv).as_deref(), Some("sess-1"));
    }
}
