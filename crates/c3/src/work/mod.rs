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
    /// `--unrestricted`: lift the allow-list grammar (accept every `--allow` pattern), permit the
    /// muse worker's `--disable-sandbox`. Recorded in `run.json` as `unrestricted: true`.
    pub unrestricted: bool,
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
    /// (0.2.4, F89-4) Whether the run was admitted `--unrestricted` (flag or roster
    /// `ext.unrestricted`): the allow-list grammar is lifted and the muse worker is permitted.
    pub unrestricted: bool,
    /// (0.2.4, F89-4) A one-line description of the effective permission boundary for `run.json`.
    pub permission: String,
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
            // (ext.c3.inactive) an inactive entry is never a worker candidate.
            !e.inactive
                && WorkerEngine::parse(&e.engine).is_some()
                && e.roles.iter().any(|r| r == "worker")
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
    // (ext.c3.inactive) a direct request for an inactive worker label fails clearly.
    if roster
        .entries
        .iter()
        .any(|e| e.provider == label && e.inactive)
    {
        return Err(format!(
            "reviewer '{label}' is inactive (ext.c3.inactive in the roster)"
        ));
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

    // (0.2.4, F89-4) Unrestricted execution is an explicit operator opt-in: the `--unrestricted`
    // flag, or the roster entry's `ext.unrestricted: true`.
    let unrestricted = o.unrestricted || entry.unrestricted;

    // (0.2.4, F89-4) The muse worker disables its shell sandbox (`--disable-sandbox`): there is no
    // real containment, so it is refused unless the operator opted in.
    if engine == WorkerEngine::Muse && !unrestricted {
        return Err(
            "the muse worker runs with --disable-sandbox (no shell containment); pass --unrestricted (or set \"ext\": {\"unrestricted\": true} on its roster entry) to allow it"
                .into(),
        );
    }

    // The engine's credentials must be usable NOW: a guard of the design's "before anything
    // starts" list, so it is checked here too — `run_prepared` re-checks it, but by then a
    // record directory would already exist. claude: the endpoint's token variable; muse: the
    // per-token-billing / oauth launch guard; codex: none (a missing ChatGPT login fails the run).
    engine_launch_guard(engine, &entry)?;

    // The allow list: the defaults when empty; a pattern naming git is refused — a
    // worker never commits (unless --unrestricted lifts the whole grammar).
    let allow = if o.allow.is_empty() {
        claude::WORK_DEFAULT_ALLOWS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        o.allow.clone()
    };
    if !unrestricted {
        // The pattern is never echoed (it could carry a credential): the position is named.
        if let Some(i) = allow.iter().position(|p| p.to_lowercase().contains("git")) {
            return Err(format!(
                "--allow pattern #{} names git: a worker never runs git (the preamble forbids it; the allow list must not invite it)",
                i + 1
            ));
        }
        // (0.2.4, F89-4) A real allow-list grammar for operator-supplied patterns (the trusted
        // defaults are exempt): each must be <Tool>, <Tool>(<exe>) or <Tool>(<exe>:*) with <exe>
        // a plain executable name. The refusal names the POSITION only — never the pattern, which
        // could carry a credential.
        for (i, p) in o.allow.iter().enumerate() {
            if let Err(why) = validate_allow_pattern(p) {
                return Err(format!(
                    "--allow pattern #{} is not a valid restricted tool pattern: {why}; pass --unrestricted to lift the allow-list grammar",
                    i + 1
                ));
            }
        }
    }

    // (0.2.4, F89-4) The effective permission boundary, recorded in run.json.
    let permission = match engine {
        WorkerEngine::Claude if unrestricted => {
            "claude, unrestricted (allow-list grammar lifted)".to_string()
        }
        WorkerEngine::Claude => "claude --allowedTools allow list".to_string(),
        WorkerEngine::Codex => {
            "codex --sandbox workspace-write (the allow list does not apply to codex)".to_string()
        }
        WorkerEngine::Muse => {
            "muse --disable-sandbox, unrestricted (the allow list does not apply to muse)"
                .to_string()
        }
    };

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
        unrestricted,
        permission,
    })
}

/// (0.2.4, F89-4) Validate one operator `--allow` pattern against the restricted grammar:
/// `<Tool>`, `<Tool>(<exe>)` or `<Tool>(<exe>:*)` where `<exe>` is a plain executable name
/// (`[A-Za-z0-9_.+-]+`, optionally followed by plain subcommand words such as `cargo test` — no
/// wildcards, quotes or shell operators) and is not a shell or git. Returns
/// why it is invalid (never echoing the pattern, which could carry a credential; the forbidden
/// shell/git keyword is safe to name).
fn validate_allow_pattern(p: &str) -> Result<(), String> {
    // (0.2.4, F89-4 remainder) The tools that may appear BARE (no executable): read/write tools
    // that run no shell. A bare shell/exec tool (e.g. `Bash`) is refused — it must name an exe.
    const BARE_OK: &[&str] = &[
        "Read",
        "Edit",
        "Write",
        "Glob",
        "Grep",
        "MultiEdit",
        "NotebookEdit",
        "WebFetch",
        "WebSearch",
    ];
    // Shells AND interpreters AND git: compared against the exe BASENAME, lowercased, with a
    // trailing .exe/.cmd/.bat/.ps1 stripped.
    const DENY: &[&str] = &[
        "sh",
        "bash",
        "zsh",
        "fish",
        "dash",
        "pwsh",
        "powershell",
        "cmd",
        "wsl",
        "python",
        "python3",
        "py",
        "node",
        "deno",
        "bun",
        "perl",
        "ruby",
        "php",
        "git",
    ];
    // `<exe>` is a plain executable name, optionally followed by plain subcommand words
    // (`cargo test`, `npm run lint`): single spaces only, a word never starts with `-` (flags such
    // as `python -c` / `node -e` are where shell escapes hide), no wildcards, no shell operators,
    // and — because the class excludes `/` and `\` — no path separators.
    let re = regex::Regex::new(
        r"^([A-Za-z][A-Za-z0-9_]*)(?:\(([A-Za-z0-9_.+-]+)((?: [A-Za-z0-9][A-Za-z0-9_.+-]*)*)(?::\*)?\))?$",
    )
    .expect("valid regex");
    let caps = re.captures(p.trim()).ok_or_else(|| {
        "expected <Tool>, <Tool>(<exe>) or <Tool>(<exe>:*) with <exe> a plain executable name (letters, digits, '.', '_', '+', '-') optionally followed by plain subcommand words (never flags); no wildcards, quotes, shell operators or path separators".to_string()
    })?;
    let tool = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    match caps.get(2) {
        None => {
            // A bare tool is allowed only for the non-shell whitelist.
            if !BARE_OK.contains(&tool) {
                return Err(format!(
                    "a bare '{tool}' is not allowed in restricted mode; name an executable, e.g. {tool}(cargo:*)"
                ));
            }
        }
        Some(exe) => {
            let lower = exe.as_str().to_lowercase();
            let base = [".exe", ".cmd", ".bat", ".ps1"]
                .iter()
                .find_map(|s| lower.strip_suffix(s))
                .unwrap_or(lower.as_str());
            if DENY.contains(&base) {
                return Err(format!(
                    "the '{}' executable is a shell, interpreter or git, which a worker may not be granted",
                    exe.as_str()
                ));
            }
        }
    }
    Ok(())
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
    let before_hashes = record::baseline_hashes(&pre.repo_root, &before);
    if let Ok(Some(mut rec)) = read_record(rec_dir) {
        // (0.2.4, F89-5) the persisted argv passes the sanitizer (flags/uuids/grammar-safe allow
        // values are untouched; a secret-shaped value would be redacted).
        rec.argv = sanitize_argv(&argv);
        rec.child_env_scrubbed = scrubbed;
        rec.timeout_sec = o.timeout_sec;
        rec.session_id = session_id.clone();
        rec.baseline = before.clone();
        rec.baseline_hashes = before_hashes.clone();
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
    let (status, quota_text, reset_at, reason) = classify_attempt(
        pre.engine,
        &events_text,
        &std::fs::read_to_string(&stderr_path).unwrap_or_default(),
        result.stop == crate::engines::subprocess::TurnStop::Timeout,
        result.exit_code,
    );
    let window = eff_window(&o, pre.entry.quota_window_sec);
    // (0.2.4, F89-3) An artifact write failure makes the run `artifact_error`, never `done`.
    let mut rec =
        match refresh_artifacts(pre.engine, rec_dir, &pre.repo_root, &before, &before_hashes) {
            Ok(r) => r,
            Err(e) => return finish_artifact_error(rec_dir, result.exit_code, &e),
        };
    rec.session_id = session_id.clone();
    rec.exit_code = result.exit_code.map(|c| c as i64);
    rec.timeout_sec = o.timeout_sec;
    rec.status = status.clone();
    rec.ended = Some(iso_now());
    rec.error = if status == "died" {
        reason
    } else {
        String::new()
    };
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

/// (0.2.4, F89-6) The effective resume cap of a RESUME: the explicit `--max-resumes`, else the cap
/// persisted in the record at the first run, else the default. So a plain `--resume` honours the
/// original cap.
fn eff_max_resumes_rec(o: &Options, rec: &RunRecord) -> i64 {
    if o.max_resumes > 0 {
        o.max_resumes
    } else if rec.max_resumes > 0 {
        rec.max_resumes
    } else {
        MAX_RESUMES_DEFAULT
    }
}

/// (0.2.4, F89-6) The effective quota window of a RESUME: the explicit `--quota-window-sec`, else
/// the window persisted in the record, else the roster entry's, else the default.
fn eff_window_rec(o: &Options, rec: &RunRecord, entry_window: i64) -> i64 {
    if o.quota_window_sec > 0 {
        o.quota_window_sec
    } else if rec.quota_window_sec > 0 {
        rec.quota_window_sec
    } else if entry_window > 0 {
        entry_window
    } else {
        QUOTA_WINDOW_DEFAULT_SEC
    }
}

/// (0.2.4, F89-2) Claim a DETACHED run's placeholder record for its first spawn under the task
/// admission lock: refuse if any other run is live or the inspection fails; the record must still
/// be the `running` placeholder this process owns. Writes pid + start + attempt 0 durably before
/// the caller spawns.
fn claim_start(work_root: &Path, rec_dir: &Path) -> Result<RunRecord, String> {
    let _lock = record::admission_lock(work_root)?;
    if let Some(other) = record::other_live_run(work_root, rec_dir)? {
        return Err(format!("task already has a live work run ({other})"));
    }
    let mut rec = read_record(rec_dir)?
        .ok_or_else(|| "the work record vanished before it could be claimed".to_string())?;
    if rec.status != "running" {
        return Err(format!(
            "work run {} is in state {}, not the fresh placeholder; refusing to launch",
            rec.id, rec.status
        ));
    }
    let (pid, start) = record::self_owner();
    if record::is_live(&rec) && rec.pid.is_some() && rec.pid != Some(pid) {
        return Err(format!(
            "work run {} is already live under pid {}; refusing a second launch",
            rec.id,
            rec.pid.unwrap_or(0)
        ));
    }
    rec.pid = Some(pid);
    rec.pid_start = start;
    rec.attempt = 0;
    rec.ended = None;
    rec.error = String::new();
    rec.args = None;
    write_record(rec_dir, &rec)?;
    Ok(rec)
}

/// (0.2.4, F91-1) Claim a run for a RESUME spawn under the task admission lock, validating
/// EVERYTHING against the locked re-read, not the caller's earlier unlocked read: no other run is
/// live, this record is still in a resumable state (`quota`/`timeout`), it is NOT exhausted, it is
/// not already live under another owner, and the attempt generation the caller expected still
/// holds. The next attempt number is computed from THIS locked record, so a stale claim (now
/// `done`/`exhausted`/already advanced by a concurrent resume) refuses before any spawn. Returns
/// the claimed record and the attempt number `n`.
fn claim_resume(
    work_root: &Path,
    rec_dir: &Path,
    expected_resumes: i64,
) -> Result<(RunRecord, i64), String> {
    let _lock = record::admission_lock(work_root)?;
    if let Some(other) = record::other_live_run(work_root, rec_dir)? {
        return Err(format!("task already has a live work run ({other})"));
    }
    let mut rec = read_record(rec_dir)?
        .ok_or_else(|| "the work record vanished before it could be claimed".to_string())?;
    // Resumable states: `quota`/`timeout` for a foreground --resume, and `waiting_quota` for the
    // detached alarm (which parked the record before it resumes in-process). A terminal state
    // (done/died/exhausted) means a concurrent resume/completion won: refuse the stale claim.
    if !matches!(rec.status.as_str(), "quota" | "timeout" | "waiting_quota") {
        return Err(format!(
            "work run {} is in state {}, not resumable; nothing to resume (it may have been completed or resumed concurrently)",
            rec.id, rec.status
        ));
    }
    if rec.quota.as_ref().map(|q| q.exhausted).unwrap_or(false) {
        return Err(format!(
            "work run {} has spent its resume cap; it needs a human",
            rec.id
        ));
    }
    let resumes = rec.quota.as_ref().map(|q| q.resumes).unwrap_or(0);
    if resumes != expected_resumes {
        return Err(format!(
            "work run {} changed since it was read (resume {} now, {} expected); refusing a stale claim",
            rec.id, resumes, expected_resumes
        ));
    }
    let (pid, start) = record::self_owner();
    if record::is_pending(&rec.status) && record::is_live(&rec) && rec.pid != Some(pid) {
        return Err(format!(
            "work run {} is already live under pid {}; refusing a second launch",
            rec.id,
            rec.pid.unwrap_or(0)
        ));
    }
    let n = resumes + 1;
    rec.status = "running".into();
    rec.pid = Some(pid);
    rec.pid_start = start;
    rec.attempt = n;
    rec.ended = None;
    rec.error = String::new();
    rec.args = None;
    write_record(rec_dir, &rec)?;
    Ok((rec, n))
}

/// (0.2.4, F89-3) Make the record terminal `artifact_error` (an artifact write failed): the error
/// text in `run.json`, a non-zero exit. The child's exit code is still recorded for forensics.
fn finish_artifact_error(rec_dir: &Path, exit_code: Option<i32>, err: &str) -> i32 {
    if let Ok(Some(mut rec)) = read_record(rec_dir) {
        rec.status = "artifact_error".into();
        rec.error = err.to_string();
        rec.ended = Some(iso_now());
        rec.exit_code = exit_code.map(|c| c as i64);
        let _ = write_record(rec_dir, &rec);
        print_run_line(&rec);
    }
    eprintln!("{TOOL}: the work run could not durably write its artifacts: {err}");
    1
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
) -> (String, String, Option<DateTime<Utc>>, String) {
    let (quota_text, reset_at) = engines::quota_evidence(engine, events_text, stderr_text);
    let outcome = engines::terminal_outcome(engine, events_text);
    // (0.2.4, F89-8) A rejecting rate-limit / quota wording takes precedence over exit 0 (so a run
    // resumes), UNLESS the engine also emitted a clean successful terminal. Otherwise success
    // requires BOTH exit 0 AND the engine's successful terminal event; an error terminal or an
    // empty stream fails closed (`died`), never `done`.
    if !quota_text.is_empty() && outcome != engines::Terminal::Success {
        return ("quota".to_string(), quota_text, reset_at, String::new());
    }
    if timed_out {
        return ("timeout".to_string(), quota_text, reset_at, String::new());
    }
    let (status, reason) = match outcome {
        engines::Terminal::Success if exit_code == Some(0) => ("done", String::new()),
        engines::Terminal::Success => (
            "died",
            format!(
                "the engine reported success but the process exited {}",
                exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "(none)".into())
            ),
        ),
        engines::Terminal::Error => ("died", "the engine reported an error result".to_string()),
        engines::Terminal::None => ("died", "no terminal event".to_string()),
    };
    (status.to_string(), quota_text, reset_at, reason)
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
    before_hashes: &std::collections::BTreeMap<String, String>,
) -> Result<RunRecord, String> {
    // (0.2.4, F91-3) Read the aggregate stream STRICTLY: a transient read failure must NOT become
    // an empty report. report.md is never replaced when its source cannot be read.
    let events_text = read_events_strict(&rec_dir.join("events.jsonl"))?;
    let sum = engines::summarize(engine, &events_text);
    // (0.2.4, F89-3/F89-5) report.md and diff.txt write failures are propagated (the caller makes
    // the run `artifact_error`), and both pass the project's single sanitizer so no secret-shaped
    // value survives in them.
    std::fs::write(rec_dir.join("report.md"), sanitize(&sum.report))
        .map_err(|e| format!("could not write report.md ({e})"))?;
    let after = status_short(repo_root);
    let stat = diff_stat(repo_root);
    std::fs::write(
        rec_dir.join("diff.txt"),
        sanitize(&diff_text(before, &after, &stat)),
    )
    .map_err(|e| format!("could not write diff.txt ({e})"))?;
    let mut rec = read_record(rec_dir).ok().flatten().unwrap_or_default();
    rec.turns = sum.turns;
    rec.tool_calls = sum.tool_calls;
    rec.files_changed = files_changed(before_hashes, &after, repo_root);
    rec.usage = sum.usage;
    Ok(rec)
}

/// (0.2.4, F91-3) Read an event file STRICTLY: a genuinely absent file (`NotFound`) is an empty
/// stream, but any other read error (permission, a path replaced by a directory) is propagated so
/// the caller treats it as an artifact error rather than empty input.
fn read_events_strict(path: &Path) -> Result<String, String> {
    match std::fs::read(path) {
        Ok(b) => Ok(String::from_utf8_lossy(&b).into_owned()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("could not read {} ({e})", path.display())),
    }
}

/// (0.2.4, F89-5) The project's single outbound sanitizer: `run.json`, `report.md` and `diff.txt`
/// pass through it (secret-shaped values redacted, the same rules as a pack). `events.jsonl` does
/// NOT — it is the child's raw stream, retained verbatim.
fn sanitize(text: &str) -> String {
    crate::pack::redact::redact(text).0
}

/// (0.2.4, F91-1) TEST-ONLY claim barrier (honoured only in test mode via `test_hooks::hook`):
/// when `CODEX_CONSULT_TEST_WORK_CLAIM_BARRIER` names a path, wait (bounded) until it exists, so a
/// test can interpose a competing resume between an unlocked read and the locked claim.
fn test_claim_barrier() {
    if let Some(gate) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_WORK_CLAIM_BARRIER") {
        let gate = PathBuf::from(gate);
        for _ in 0..600 {
            if gate.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// (0.2.4, F89-5) Sanitize each argv element persisted in `run.json` (flags, uuids and
/// grammar-validated allow values pass through unchanged; a secret-shaped value is redacted).
fn sanitize_argv(argv: &[String]) -> Vec<String> {
    argv.iter().map(|a| sanitize(a)).collect()
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
    // (0.2.4, F89-1/F89-2) One task-level admission for starts: hold the admission lock across the
    // live-run check, the allocation and the initial claim, so two starts cannot both pass. A
    // foreground run writes its OWN pid + start time BEFORE the child is spawned (the owner that
    // keeps the record `running` past the start grace); a detached run leaves the pid to its
    // background (claimed within its first second).
    let lock = match record::admission_lock(&pre.work_root) {
        Ok(l) => l,
        Err(e) => return refuse(&e),
    };
    match record::live_run(&pre.work_root) {
        Ok(Some(id)) => {
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
        Ok(None) => {}
        // (0.2.4, F91-2) fail closed: a record that cannot be inspected refuses admission.
        Err(e) => return refuse(&format!("cannot admit a work run: {e}")),
    }
    let (id, seq, rec_dir) = match record::allocate(&pre.work_root, &pre.provider, &pre.model) {
        Ok(x) => x,
        Err(e) => return refuse(&e),
    };
    let (pid, pid_start) = if o.detach {
        (None, String::new())
    } else {
        let (p, s) = record::self_owner();
        (Some(p), s)
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
        pid,
        pid_start,
        max_resumes: eff_max_resumes(&o),
        quota_window_sec: eff_window(&o, pre.entry.quota_window_sec),
        unrestricted: pre.unrestricted,
        permission: pre.permission.clone(),
        args: o.detach.then(|| detach::wire_of(&o, &pre.collab_root)),
        ..Default::default()
    };
    if let Err(e) = write_record(&rec_dir, &rec) {
        return refuse(&format!(
            "the work run could not write {} ({e}); nothing was started.",
            rec_dir.join("run.json").display()
        ));
    }
    drop(lock);
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
        max_resumes: eff_max_resumes_rec(o, rec),
        window_sec: eff_window_rec(o, rec, entry.quota_window_sec),
    })
}

/// Append the resume separator line and this attempt's stream to `events.jsonl` (verbatim, the
/// whole record kept in order).
fn append_resume(rec_dir: &Path, n: i64, attempt_events: &str) -> Result<(), String> {
    use std::io::Write;
    let path = rec_dir.join("events.jsonl");
    let needs_nl = std::fs::read(&path)
        .ok()
        .map(|b| !b.is_empty() && b.last() != Some(&b'\n'))
        .unwrap_or(false);
    // (0.2.4, F89-3) Every open/write/flush error is propagated — the caller keeps the attempt
    // files and makes the run `artifact_error` rather than deleting them and reporting `done`.
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("could not open events.jsonl to append the resume ({e})"))?;
    let mut write = |bytes: &[u8]| -> Result<(), String> {
        f.write_all(bytes)
            .map_err(|e| format!("could not append to events.jsonl ({e})"))
    };
    if needs_nl {
        write(b"\n")?;
    }
    let sep = format!(
        "{{\"type\":\"c3\",\"subtype\":\"resume\",\"n\":{n},\"at\":\"{}\"}}\n",
        Utc::now().to_rfc3339()
    );
    write(sep.as_bytes())?;
    let trimmed = attempt_events.trim_end_matches(['\n', '\r']);
    if !trimmed.is_empty() {
        write(trimmed.as_bytes())?;
        write(b"\n")?;
    }
    f.flush()
        .map_err(|e| format!("could not flush events.jsonl ({e})"))?;
    f.sync_all()
        .map_err(|e| format!("could not sync events.jsonl ({e})"))?;
    Ok(())
}

/// Resume the child once: relaunch with `--resume`, append the attempt to `events.jsonl`,
/// recompute every artifact over the whole record, bump `quota.resumes`, and make the record final
/// again (quota, exhausted, done, died or timeout). Returns the new status.
fn resume_once(renv: &ResumeEnv, rec_dir: &Path) -> Result<String, String> {
    let pre = read_record(rec_dir)?.ok_or_else(|| "the work record vanished".to_string())?;
    let expected_resumes = pre.quota.as_ref().map(|q| q.resumes).unwrap_or(0);
    // (0.2.4, F91-1) TEST-ONLY barrier (honoured only in test mode): pause here, AFTER the caller's
    // unlocked read and BEFORE the locked claim, until the gate file appears — the acceptance test
    // lets a competing resume finish/exhaust in this window, so the locked re-read below sees the
    // stale state and refuses.
    test_claim_barrier();
    // (0.2.4, F89-2/F91-1) Claim under the task admission lock, validating the permitted state,
    // exhaustion, owner and attempt generation against the LOCKED re-read; the attempt number `n`
    // comes from that record, so a stale claim refuses before spawning.
    let work_root = rec_dir.parent().unwrap_or(rec_dir).to_path_buf();
    let (rec, n) = claim_resume(&work_root, rec_dir, expected_resumes)?;
    println!(
        "{TOOL}: resuming work run {} ({} :: {}) - resume {}/{}",
        rec.id, rec.provider, rec.model, n, renv.max_resumes
    );
    // (0.2.4, F89-2) Per-attempt file names (never a fixed path two resumes could overwrite).
    let tmp_events = rec_dir.join(format!(".events-resume-{n}.jsonl"));
    let tmp_stderr = rec_dir.join(format!(".stderr-resume-{n}"));
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
    // (0.2.4, F91-3) Read the attempt stream STRICTLY: an unreadable attempt file is an artifact
    // error, and the per-attempt temp files are PRESERVED (not deleted) so the bytes are
    // recoverable — never append an empty attempt and then delete its source.
    let attempt_events = match read_events_strict(&tmp_events) {
        Ok(t) => t,
        Err(e) => {
            let _ = finish_artifact_error(rec_dir, result.exit_code, &e);
            return Ok("artifact_error".to_string());
        }
    };
    let attempt_stderr = std::fs::read_to_string(&tmp_stderr).unwrap_or_default();

    let (status, quota_text, reset_at, reason) = classify_attempt(
        renv.engine,
        &attempt_events,
        &attempt_stderr,
        result.stop == crate::engines::subprocess::TurnStop::Timeout,
        result.exit_code,
    );
    // (0.2.4, F89-3) Append the attempt to events.jsonl durably FIRST; the per-attempt files are
    // deleted only after the append succeeded and was flushed. A failed append keeps them and
    // makes the run `artifact_error` (never `done`, never a deleted-then-lost report).
    if let Err(e) = append_resume(rec_dir, n, &attempt_events) {
        let _ = finish_artifact_error(rec_dir, result.exit_code, &e);
        return Ok("artifact_error".to_string());
    }
    let _ = std::fs::remove_file(&tmp_events);
    let _ = std::fs::remove_file(&tmp_stderr);

    let before = rec.baseline.clone();
    let before_hashes = rec.baseline_hashes.clone();
    let mut nrec = match refresh_artifacts(
        renv.engine,
        rec_dir,
        &renv.repo_root,
        &before,
        &before_hashes,
    ) {
        Ok(r) => r,
        Err(e) => {
            let _ = finish_artifact_error(rec_dir, result.exit_code, &e);
            return Ok("artifact_error".to_string());
        }
    };
    nrec.exit_code = result.exit_code.map(|c| c as i64);
    nrec.ended = Some(iso_now());
    nrec.status = status.clone();
    nrec.error = if status == "died" {
        reason
    } else {
        String::new()
    };
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
            eff_max_resumes_rec(o, rec),
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
            eff_max_resumes_rec(o, rec)
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
    // (0.2.4, F91-2) fail closed: enumeration and per-record inspection errors propagate.
    // (0.2.4, F93-1) a dead owner's `died` transition goes through the locked display recovery.
    for (_, dir) in record::run_dirs(work_root)? {
        if let Some(rec) = record::read_for_display(work_root, &dir)? {
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

    /// (0.2.4, F91-3) refresh_artifacts fails closed when events.jsonl cannot be read and does NOT
    /// replace report.md with empty content. The allow grammar accepts/refuses as F89-4 wants.
    #[test]
    fn refresh_artifacts_does_not_destroy_report_on_read_failure() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let rec_dir =
            std::env::temp_dir().join(format!("c3-refresh-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&rec_dir).unwrap();
        std::fs::write(rec_dir.join("report.md"), "OLD REPORT").unwrap();
        // make events.jsonl UNREADABLE by making it a directory
        std::fs::create_dir_all(rec_dir.join("events.jsonl")).unwrap();
        let before: Vec<String> = Vec::new();
        let bh = std::collections::BTreeMap::new();
        let r = refresh_artifacts(WorkerEngine::Claude, &rec_dir, &rec_dir, &before, &bh);
        assert!(
            r.is_err(),
            "an unreadable events.jsonl must be an artifact error"
        );
        assert_eq!(
            std::fs::read_to_string(rec_dir.join("report.md")).unwrap(),
            "OLD REPORT",
            "report.md must not be replaced when events.jsonl cannot be read"
        );
        let _ = std::fs::remove_dir_all(&rec_dir);
    }

    #[test]
    fn read_events_strict_distinguishes_absent_from_unreadable() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("c3-strict-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // absent -> empty (Ok)
        assert_eq!(read_events_strict(&dir.join("missing")).unwrap(), "");
        // a directory in place of the file -> Err (unreadable)
        std::fs::create_dir_all(dir.join("as-dir")).unwrap();
        assert!(read_events_strict(&dir.join("as-dir")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// (0.2.4, F89-4 remainder) the restricted allow grammar: bare shells and interpreters refused,
    /// path separators and suffixed shells refused, plain exes and whitelisted bare tools allowed.
    #[test]
    fn validate_allow_pattern_grammar() {
        for bad in [
            "Bash",
            "Bash(python:*)",
            "Bash(powershell.exe:*)",
            "Bash(C:\\x\\node.exe:*)",
            "Bash(*)",
            "Bash(python -c:*)",
        ] {
            assert!(
                validate_allow_pattern(bad).is_err(),
                "{bad} must be refused"
            );
        }
        for ok in [
            "Read",
            "Bash(cargo:*)",
            "Bash(cargo test:*)",
            "Bash(npm run lint:*)",
        ] {
            assert!(validate_allow_pattern(ok).is_ok(), "{ok} must be allowed");
        }
    }

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
