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
pub mod record;

use std::path::{Path, PathBuf};
use std::time::Duration;

use c3_core::claude::{self, ClaudeWorkArgs, MAX_PROMPT_BYTES};
use c3_core::engine::PromptDelivery;
use c3_core::roster::{Roster, RosterEntry};

use crate::engines::claude_auth;
use crate::engines::subprocess::{run_turn, SpawnRequest};
use crate::providers;
use record::{
    diff_stat, diff_text, files_changed, read_record, status_short, summarize_events, write_record,
    RunRecord,
};

const TOOL: &str = "c3 work";

/// The efforts a work run accepts (the same ladder the consult purposes use).
const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh"];

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

/// The roster's worker entries: engine `claude`, role `worker` (the EXISTING `roles` key
/// — the design changes no schema).
fn worker_entries(roster: &Roster) -> Vec<&RosterEntry> {
    roster
        .entries
        .iter()
        .filter(|e| e.engine == "claude" && e.roles.iter().any(|r| r == "worker"))
        .collect()
}

fn worker_list(roster: &Roster) -> String {
    let workers = worker_entries(roster);
    if workers.is_empty() {
        "the roster has no worker entries (engine claude, role \"worker\"); add \"roles\": [\"worker\"] to a claude entry first.".to_string()
    } else {
        format!(
            "the roster's worker entries: {}",
            workers
                .iter()
                .map(|e| format!("{} :: {}", e.provider, e.model))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// The `--provider` lookup: a worker entry by label, with the design's refusals (a
/// non-worker label, a non-claude engine, an unknown label — each naming the workers).
fn select_worker(roster: &Roster, label: &str) -> Result<RosterEntry, String> {
    let workers = worker_entries(roster);
    if let Some(e) = workers.iter().find(|e| e.provider == label) {
        return Ok((*e).clone());
    }
    if let Some(e) = roster.entries.iter().find(|e| e.provider == label) {
        if e.engine != "claude" {
            return Err(format!(
                "the roster entry '{label}' runs on the '{}' engine; workers run on the claude engine only. {}",
                e.engine,
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
    let model = if o.model.trim().is_empty() {
        entry.model.clone()
    } else {
        o.model.trim().to_string()
    };

    // The entry's endpoint must be usable NOW (its token variable set): a guard of the
    // design's "before anything starts" list, so it is checked here too — `run_prepared`
    // re-checks it, but by then a record directory would already exist.
    let env = claude_auth::child_env(&entry.auth, entry.endpoint.as_ref());
    if !env.problem.is_empty() {
        return Err(format!(
            "the entry's endpoint is not usable: {}",
            env.problem
        ));
    }

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
    let launcher = match providers::resolve_engine_launcher("claude", "") {
        Ok(Some(l)) => l,
        Ok(None) => {
            return Err(
                "the claude launcher was not found (none of its PATH names or install locations); run `claude` once or point CODEX_CONSULT_CLAUDE_EXE at it".into(),
            )
        }
        Err(e) => return Err(e),
    };

    Ok(Prepared {
        repo_root,
        collab_root,
        task_dir,
        work_root,
        provider: entry.provider.clone(),
        model,
        entry,
        allow,
        launcher,
        prompt,
    })
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
pub fn run_prepared(o: &Options, rec_dir: &Path, _id: &str, _seq: i64) -> i32 {
    let mut o = o.clone();
    let pre = match prepare(&mut o) {
        Ok(p) => p,
        Err(why) => return refuse_record(rec_dir, &why),
    };

    // brief.md: the prompt as sent.
    if let Err(e) = std::fs::write(rec_dir.join("brief.md"), &pre.prompt) {
        return refuse_record(rec_dir, &format!("could not write brief.md ({e})"));
    }

    // The child environment: the review engine's own builder (the route's three
    // variables, the host markers gone). Its `problem` is the env_key-unset refusal.
    let env = claude_auth::child_env(&pre.entry.auth, pre.entry.endpoint.as_ref());
    if !env.problem.is_empty() {
        return refuse_record(
            rec_dir,
            &format!("the entry's endpoint is not usable: {}", env.problem),
        );
    }

    // The argv (no secrets travel in it) and the record's launch fields.
    let argv = claude::work_args(&ClaudeWorkArgs {
        model: pre.model.clone(),
        effort: (!o.effort.trim().is_empty()).then(|| o.effort.trim().to_string()),
        max_turns: o.max_turns.clamp(1, u32::MAX as i64) as u32,
        allowed_tools: pre.allow.clone(),
        session_id: uuid::Uuid::new_v4().to_string(),
    });
    let before = status_short(&pre.repo_root);
    if let Ok(Some(mut rec)) = read_record(rec_dir) {
        rec.argv = argv.clone();
        rec.child_env_scrubbed = c3_core::host::host_marker_names();
        rec.timeout_sec = o.timeout_sec;
        if let Err(e) = write_record(rec_dir, &rec) {
            return refuse(&format!(
                "the work run could not report to {} ({e}); nothing was started.",
                rec_dir.join("run.json").display()
            ));
        }
    }

    println!(
        "{TOOL}: work run starting ({} :: {}) in {}",
        pre.provider,
        pre.model,
        pre.repo_root.display()
    );
    let spawn = SpawnRequest {
        launcher: &pre.launcher,
        argv: &argv,
        cwd: &pre.repo_root,
        prompt_delivery: PromptDelivery::Stdin,
        stdin_text: &pre.prompt,
        events_path: &rec_dir.join("events.jsonl"),
        stderr_path: &rec_dir.join(".stderr"),
        timeout: Duration::from_secs(o.timeout_sec.max(1) as u64),
        stall_sec: 0,
        kick_path: None,
        tool_flight: None,
        on_running: None,
        on_kill: None,
        env: Some(&env.env),
        crt_quote: true,
        budget: None,
    };
    let result = run_turn(&spawn);

    // The summary: what the stream says, what git says.
    let events_text = std::fs::read_to_string(rec_dir.join("events.jsonl")).unwrap_or_default();
    let sum = summarize_events(&events_text);
    let report = if !sum.result_text.trim().is_empty() {
        sum.result_text.clone()
    } else {
        sum.last_assistant_text.clone()
    };
    let _ = std::fs::write(rec_dir.join("report.md"), &report);
    let after = status_short(&pre.repo_root);
    let stat = diff_stat(&pre.repo_root);
    let _ = std::fs::write(rec_dir.join("diff.txt"), diff_text(&before, &after, &stat));

    let status = if result.stop == crate::engines::subprocess::TurnStop::Timeout {
        "timeout"
    } else if result.exit_code == Some(0) {
        "done"
    } else {
        "died"
    };
    let mut rec = match read_record(rec_dir) {
        Ok(Some(r)) => r,
        _ => RunRecord::default(),
    };
    rec.status = status.into();
    rec.ended = Some(iso_now());
    rec.exit_code = result.exit_code.map(|c| c as i64);
    rec.turns = sum.num_turns.or(if sum.assistant_count > 0 {
        Some(sum.assistant_count)
    } else {
        None
    });
    rec.tool_calls = sum.tool_calls;
    rec.files_changed = files_changed(&before, &after);
    rec.usage = sum.usage;
    rec.timeout_sec = o.timeout_sec;
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
    println!(
        "{TOOL}: work run {status} ({} :: {}) - turns {}, tool calls {}, files changed: {}",
        pre.provider,
        pre.model,
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
    println!("report    : {}", rec_dir.join("report.md").display());
    println!("record dir: {}", rec_dir.display());
    if status == "done" {
        0
    } else {
        1
    }
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
        engine: "claude".into(),
        child_env_scrubbed: c3_core::host::host_marker_names(),
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
    run_prepared(&o, &rec_dir, &id, seq)
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
    fn a_non_claude_engine_is_refused() {
        let r = roster(vec![entry("ZAI-claude", "glm-5.3", "codex", &["worker"])]);
        let err = select_worker(&r, "ZAI-claude").unwrap_err();
        assert!(
            err.contains("workers run on the claude engine only"),
            "{err}"
        );
    }

    #[test]
    fn an_unknown_label_is_refused() {
        let r = roster(vec![entry("ZAI-claude", "glm-5.3", "claude", &["worker"])]);
        let err = select_worker(&r, "nobody").unwrap_err();
        assert!(err.contains("no roster entry 'nobody'"), "{err}");
        assert!(err.contains("ZAI-claude :: glm-5.3"), "{err}");
    }
}
