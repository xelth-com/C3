//! The work run's record (design v1, `docs/work-mode.md`): the `<NN>-<provider>-<model>`
//! directory under `<collab>/<task>/work/`, its `run.json`, the git diff c3 computes around
//! the child, and the summary parsed from the child's event stream.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The quota block of a work run that hit a provider usage limit (`docs/work-mode.md`, quota
/// resume): the limit wording, when the window resets, when `c3 work` will resume the child, how
/// many times it already has, and whether the resume cap is spent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Quota {
    /// The usage-limit wording (the engine's `quota_evidence`, never a secret).
    pub text: String,
    /// When the provider says the window resets (RFC3339 UTC), or `null` when it said none.
    #[serde(default)]
    pub reset_at: Option<String>,
    /// When `c3 work` resumes the child: `reset_at` + 60 s when known, else now + the quota window.
    pub resume_at: String,
    /// How many times this run has already been resumed.
    #[serde(default)]
    pub resumes: i64,
    /// The resume cap is spent (`resumes` reached `--max-resumes`): the run needs a human.
    #[serde(default)]
    pub exhausted: bool,
}

/// `run.json` of one work run. Written `running` before the child starts, made final
/// (`done` | `died` | `timeout` | `refused` | `quota`, or `waiting_quota` while a detached run's
/// alarm sleeps) when it ends; `args` carries the background's options wire of a detached run
/// (taken by the background, never a secret).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunRecord {
    /// The record directory's name (`NN-provider-model`).
    pub id: String,
    pub task: String,
    pub seq: i64,
    pub provider: String,
    pub model: String,
    pub engine: String,
    /// The argv after the launcher (no secrets travel in argv).
    pub argv: Vec<String>,
    /// The host-marker NAMES scrubbed from the child (never a value).
    pub child_env_scrubbed: Vec<String>,
    pub started: String,
    #[serde(default)]
    pub ended: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    /// `running` while live, `waiting_quota` while a detached run's alarm sleeps, else `done` |
    /// `died` | `timeout` | `refused` | `quota`.
    pub status: String,
    /// The child session id (`--session-id` of the first launch): the session a quota resume
    /// continues with `--resume`.
    #[serde(default)]
    pub session_id: String,
    /// The quota block when the run hit a usage limit (status `quota` or `waiting_quota`).
    #[serde(default)]
    pub quota: Option<Quota>,
    #[serde(default)]
    pub turns: Option<i64>,
    /// Tool name -> how often the assistant called it.
    #[serde(default)]
    pub tool_calls: BTreeMap<String, i64>,
    #[serde(default)]
    pub files_changed: Vec<String>,
    /// The result event's usage/cost when present.
    #[serde(default)]
    pub usage: Option<Value>,
    /// The run's own timeout (the `--wait` default of a detached run).
    #[serde(default)]
    pub timeout_sec: i64,
    /// The OWNING process's pid (the c3 process that spawned the child: a foreground run's own
    /// pid, written before the child starts; a detached run's background pid). While a run is
    /// `running`/`waiting_quota` this pid with [`RunRecord::pid_start`] is the liveness owner.
    #[serde(default)]
    pub pid: Option<i64>,
    /// (0.2.4, F89-1) The owning process's start time (the liveness anchor against pid reuse;
    /// `""` when it could not be read). `pid` + `pid_start` together prove the owner is still
    /// alive; [`START_GRACE_SEC`] only covers the window before any pid is written.
    #[serde(default)]
    pub pid_start: String,
    /// (0.2.4, F89-2) The attempt number of the current claim: `0` the first launch, `n` the
    /// n-th resume. Written under the task admission lock when the record is claimed `running`.
    #[serde(default)]
    pub attempt: i64,
    /// (0.2.4, F89-6) The effective resume cap persisted at the first run, reused by a later plain
    /// `--resume` (an explicit `--max-resumes` still overrides). `0` = the default (3).
    #[serde(default)]
    pub max_resumes: i64,
    /// (0.2.4, F89-6) The effective quota window (seconds) persisted at the first run, reused by a
    /// later plain `--resume`. `0` = the default (18000 = 5 h).
    #[serde(default)]
    pub quota_window_sec: i64,
    /// (0.2.4, F89-4) Whether the run was admitted `--unrestricted` (allow-list grammar lifted;
    /// muse's `--disable-sandbox` permitted).
    #[serde(default)]
    pub unrestricted: bool,
    /// (0.2.4, F89-4) A one-line description of the effective permission boundary (the allow list
    /// applies only to claude; codex is `workspace-write`; muse is `--disable-sandbox`).
    #[serde(default)]
    pub permission: String,
    /// (0.2.4, F89-3/F89-8) The terminal error text when the run ended `died`/`artifact_error`
    /// (the artifact write failure, or `no terminal event`); `""` otherwise.
    #[serde(default)]
    pub error: String,
    /// The background's options wire (a detached run's `running` record only).
    #[serde(default)]
    pub args: Option<String>,
    /// The `git status --short` baseline captured before the FIRST launch: a quota resume diffs
    /// the whole run against it, so `diff.txt` and `files_changed` stay stable across resumes.
    #[serde(default)]
    pub baseline: Vec<String>,
    /// (0.2.4, F89-7) The content hash (`git hash-object`) of every path dirty at the baseline,
    /// captured before the FIRST launch: `files_changed` lists a path whose content now differs
    /// from its baseline hash, so a pre-dirty file the worker edited further is reported.
    #[serde(default)]
    pub baseline_hashes: BTreeMap<String, String>,
}

impl RunRecord {
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec_pretty(self).map_err(|e| e.to_string())
    }
}

/// Write `run.json` (atomic replace).
pub fn write_record(dir: &Path, rec: &RunRecord) -> Result<(), String> {
    let bytes = rec.to_bytes()?;
    c3_core::store::write_text_atomic(&dir.join("run.json"), &bytes).map_err(|e| e.to_string())
}

/// Read one `run.json`. (0.2.4, F91-2) FAIL CLOSED: only a confirmed `NotFound` is absence
/// (`Ok(None)`); any other read error, or a parse failure, is `Err` (the message names the
/// directory, never the contents) so admission refuses rather than treats an unreadable record as
/// an absent run.
pub fn read_record(dir: &Path) -> Result<Option<RunRecord>, String> {
    let p = dir.join("run.json");
    let bytes = match std::fs::read(&p) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} could not be read ({e})", p.display())),
    };
    let mut rec: RunRecord = serde_json::from_slice(&bytes)
        .map_err(|e| format!("{} does not parse: {e}", p.display()))?;
    // A pending record (`running`, or `waiting_quota` while a detached alarm sleeps) whose
    // background is gone (the pid is dead, or it never reported one within the start grace) is a
    // run that DIED: say so on disk, so `--status`, `--wait` and the one-live-run guard never wait
    // on a ghost.
    if is_pending(&rec.status) && !is_live(&rec) {
        rec.status = "died".into();
        rec.ended = Some(chrono::Utc::now().to_rfc3339());
        rec.args = None;
        let _ = write_record(dir, &rec);
    }
    Ok(Some(rec))
}

/// How long a pending record may go without a reported pid before it counts as dead
/// (the background reports its pid within its first second).
const START_GRACE_SEC: i64 = 120;

/// Whether a status means the run's background is still supposed to be alive: `running`, or
/// `waiting_quota` while a detached run's alarm sleeps until the quota window resets.
pub fn is_pending(status: &str) -> bool {
    status == "running" || status == "waiting_quota"
}

/// Whether a pending record's background is alive: its reported pid exists, or no pid was
/// reported yet and the record is younger than [`START_GRACE_SEC`]. Anything else is dead.
pub fn is_live(rec: &RunRecord) -> bool {
    if !is_pending(&rec.status) {
        return false;
    }
    match rec.pid {
        Some(pid) if pid > 0 => crate::liveness::proc::pid_alive(pid as u32, &rec.pid_start),
        _ => match chrono::DateTime::parse_from_rfc3339(&rec.started) {
            Ok(t) => {
                (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds() < START_GRACE_SEC
            }
            Err(_) => false,
        },
    }
}

/// Every work run directory of a task's `work/` root, by sequence. (0.2.4, F91-2) FAIL CLOSED: a
/// missing work root is `Ok([])`, but an enumeration or entry error is `Err`; a directory with the
/// `NN-` prefix is a candidate when its `run.json` path EXISTS in any form (file, directory or
/// otherwise), so an unreadable or replaced record is NOT silently dropped — `read_record` then
/// reports the inspection error.
pub fn run_dirs(work_root: &Path) -> Result<Vec<(i64, PathBuf)>, String> {
    let mut out: Vec<(i64, PathBuf)> = Vec::new();
    let rd = match std::fs::read_dir(work_root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => {
            return Err(format!(
                "could not enumerate the work directory {} ({e})",
                work_root.display()
            ))
        }
    };
    for entry in rd {
        let entry = entry.map_err(|e| {
            format!(
                "could not read an entry of the work directory {} ({e})",
                work_root.display()
            )
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        let first = name.split('-').next().unwrap_or("");
        if first.is_empty() || !first.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let seq: i64 = first.parse().unwrap_or(0);
        if seq <= 0 {
            continue;
        }
        let run_json = entry.path().join("run.json");
        match run_json.try_exists() {
            Ok(true) => out.push((seq, entry.path())),
            Ok(false) => continue, // a directory with no run.json is not a run
            Err(e) => {
                return Err(format!(
                    "could not inspect {} ({e})",
                    run_json.display()
                ))
            }
        }
    }
    out.sort_by_key(|(seq, _)| *seq);
    Ok(out)
}

/// Whether a live work run exists for the task (a `running` or `waiting_quota` record): its id,
/// or `None`. (0.2.4, F91-2) `Err` when any candidate record cannot be inspected.
pub fn live_run(work_root: &Path) -> Result<Option<String>, String> {
    for (_, dir) in run_dirs(work_root)? {
        if let Some(r) = read_record(&dir)? {
            if is_pending(&r.status) {
                return Ok(Some(r.id));
            }
        }
    }
    Ok(None)
}

/// (0.2.4, F89-2) The id of any LIVE run of the task other than the one at `exclude`, or `None`.
/// Used under the admission lock so a start/resume/alarm refuses while another run is alive.
/// (0.2.4, F91-2) `Err` when any candidate record cannot be inspected.
pub fn other_live_run(work_root: &Path, exclude: &Path) -> Result<Option<String>, String> {
    for (_, dir) in run_dirs(work_root)? {
        if dir == exclude {
            continue;
        }
        if let Some(r) = read_record(&dir)? {
            if is_pending(&r.status) && is_live(&r) {
                return Ok(Some(r.id));
            }
        }
    }
    Ok(None)
}

/// (0.2.4, F89-1/F89-2) This c3 process's own pid and start time: the owner written into a
/// record when it is claimed `running` before the child is spawned.
pub fn self_owner() -> (i64, String) {
    let pid = std::process::id();
    (
        pid as i64,
        crate::liveness::proc::process_start_iso(pid).unwrap_or_default(),
    )
}

/// (0.2.4, F89-2) The task-level admission lock file (held only across the claim's
/// check-and-write, released on drop or when the process dies).
pub const ADMISSION_LOCK: &str = ".admission.lock";

/// (0.2.4, F89-2) An exclusive OS lock on the task's admission-lock file, released when dropped
/// (or when the process dies). The same advisory-lock mechanism the telemetry spool uses.
pub struct AdmissionLock {
    _file: std::fs::File,
}

/// (0.2.4, F89-2) Take the task's admission lock (one claim at a time across starts, resumes and
/// the detached alarm), waiting at most 10 s. The guard releases the lock when dropped.
pub fn admission_lock(work_root: &Path) -> Result<AdmissionLock, String> {
    let _ = std::fs::create_dir_all(work_root);
    let path = work_root.join(ADMISSION_LOCK);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("could not open the task admission lock ({e})"))?;
    let started = std::time::Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(AdmissionLock { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => {
                if started.elapsed() >= std::time::Duration::from_secs(10) {
                    return Err(
                        "the task's admission lock stayed busy (another c3 work is admitting a run)"
                            .into(),
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(std::fs::TryLockError::Error(e)) => {
                return Err(format!("could not take the task admission lock ({e})"))
            }
        }
    }
}

/// A directory-name-safe form of a provider label or model id: everything outside
/// `[A-Za-z0-9._-]` becomes `-` (never empty).
fn dir_part(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    out.truncate(60);
    if out.is_empty() {
        out.push('x');
    }
    out
}

/// The next record directory of a task: `<work>/<NN>-<provider>-<model>` with NN the next
/// 2-digit sequence (never one that exists).
pub fn allocate(
    work_root: &Path,
    provider: &str,
    model: &str,
) -> Result<(String, i64, PathBuf), String> {
    let next = run_dirs(work_root)?
        .iter()
        .map(|(seq, _)| *seq)
        .max()
        .unwrap_or(0)
        + 1;
    for seq in next..(next + 50) {
        let id = format!("{seq:02}-{}-{}", dir_part(provider), dir_part(model));
        let dir = work_root.join(&id);
        if !dir.exists() {
            std::fs::create_dir_all(&dir).map_err(|e| {
                format!(
                    "could not create the work record directory '{}' ({e})",
                    dir.display()
                )
            })?;
            return Ok((id, seq, dir));
        }
    }
    Err(format!(
        "no free work record directory under '{}' (50 tried); remove old work runs",
        work_root.display()
    ))
}

// ------------------------------------------------------------------ the git diff (c3 runs git, not the worker)

/// One read-only git command's stdout lines (empty on any failure).
fn git_lines(repo: &Path, args: &[&str]) -> Vec<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map(|o| {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|l| l.to_string())
                    .collect()
            } else {
                Vec::new()
            }
        })
        .unwrap_or_default()
}

/// `git status --short` lines (the baseline before the run, the state after it).
pub fn status_short(repo: &Path) -> Vec<String> {
    git_lines(repo, &["status", "--short"])
}

/// `git diff --stat` lines after the run.
pub fn diff_stat(repo: &Path) -> Vec<String> {
    git_lines(repo, &["diff", "--stat"])
}

/// The path a `git status --short` line names (the `R  old -> new` form names the new path).
fn path_of_status_line(line: &str) -> String {
    let p = line.get(3..).unwrap_or("").trim();
    match p.split_once(" -> ") {
        Some((_, new)) => new.trim().to_string(),
        None => p.to_string(),
    }
}

/// (0.2.4, F89-7) The content hash of a working-tree path (`git hash-object`), `"<absent>"` when
/// the file does not exist (a deletion), `""` when git cannot hash it.
pub fn hash_of(repo: &Path, path: &str) -> String {
    if !repo.join(path).exists() {
        return "<absent>".to_string();
    }
    git_lines(repo, &["hash-object", "--", path])
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// (0.2.4, F89-7) The content hash of every path dirty at the baseline (`git status --short`
/// before the run). `files_changed` compares against this so a path already dirty but edited
/// further by the worker is reported, and one left untouched is not.
pub fn baseline_hashes(repo: &Path, before: &[String]) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for line in before {
        let p = path_of_status_line(line);
        if !p.is_empty() {
            let h = hash_of(repo, &p);
            m.insert(p, h);
        }
    }
    m
}

/// (0.2.4, F89-7) The files the run changed, by CONTENT IDENTITY: every path dirty after the run
/// whose current content hash differs from its baseline hash (a new/untracked path has no baseline
/// hash, so it is listed; a pre-dirty path the worker edited further differs, so it is listed; a
/// pre-dirty path left untouched matches, so it is not). After-order, deduplicated.
pub fn files_changed(
    baseline_hashes: &BTreeMap<String, String>,
    after: &[String],
    repo: &Path,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in after {
        let p = path_of_status_line(line);
        if p.is_empty() || out.contains(&p) {
            continue;
        }
        let now = hash_of(repo, &p);
        match baseline_hashes.get(&p) {
            Some(b) if *b == now => {} // dirty before and still the same content: not the worker's
            _ => out.push(p),
        }
    }
    out
}

/// `diff.txt`: the baseline, the after state and the diff stat.
pub fn diff_text(before: &[String], after: &[String], stat: &[String]) -> String {
    let block = |title: &str, lines: &[String]| -> String {
        if lines.is_empty() {
            format!("{title}: (empty)\n")
        } else {
            format!("{title}:\n{}\n", lines.join("\n"))
        }
    };
    format!(
        "{}\n{}\n{}",
        block("git status --short (before the run)", before),
        block("git status --short (after the run)", after),
        block("git diff --stat (after the run)", stat)
    )
}

// ------------------------------------------------------------------ the event summary

/// What c3 reads out of a work run's event stream (`events.jsonl`).
#[derive(Debug, Clone, Default)]
pub struct EventSummary {
    pub has_result: bool,
    /// The result event's `result` text (the report).
    pub result_text: String,
    pub is_error: bool,
    /// (0.2.4, F91-4) Affirmative success: a `result` event with `is_error` present AND false, and
    /// `subtype` either absent or `"success"`. A result that merely EXISTS (no `is_error`, or an
    /// error subtype) is NOT success.
    pub result_ok: bool,
    pub num_turns: Option<i64>,
    /// How many assistant events the stream holds (the `turns` fallback).
    pub assistant_count: i64,
    /// The last assistant text (the report of a run that died without a result).
    pub last_assistant_text: String,
    /// Tool name -> call count.
    pub tool_calls: BTreeMap<String, i64>,
    /// The result's `usage` merged with its `total_cost_usd` when present.
    pub usage: Option<Value>,
}

/// (0.2.4, F89-5) The usage of a claude `result` event, rebuilt from an ALLOW LIST of numeric
/// counters — the token columns from the `usage` object plus `total_cost_usd`, `duration_ms` and
/// `num_turns` from the result. Any other field (a sentinel, a secret) is dropped. `None` when no
/// counter is present.
fn claude_usage(result_obj: &Value) -> Option<Value> {
    let mut map = serde_json::Map::new();
    if let Some(u) = result_obj.get("usage").filter(|v| v.is_object()) {
        for k in [
            "input_tokens",
            "output_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
        ] {
            if let Some(n) = u.get(k).and_then(|v| v.as_i64()) {
                map.insert(k.into(), Value::from(n));
            }
        }
    }
    for k in ["num_turns", "duration_ms"] {
        if let Some(n) = result_obj
            .get(k)
            .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|x| x.parse().ok())))
        {
            map.insert(k.into(), Value::from(n));
        }
    }
    if let Some(c) = result_obj.get("total_cost_usd").and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|x| x.parse().ok()))
    }) {
        map.insert("total_cost_usd".into(), Value::from(c));
    }
    (!map.is_empty()).then(|| Value::Object(map))
}

/// Parse the event stream into an [`EventSummary`] (tolerant: a line that is not a JSON
/// object is skipped).
pub fn summarize_events(text: &str) -> EventSummary {
    let mut s = EventSummary::default();
    for line in text.split('\n') {
        let t = line.trim();
        if t.is_empty() || !t.starts_with('{') {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(t) else {
            continue;
        };
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty == "assistant" {
            s.assistant_count += 1;
            let content = obj
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            for b in content.iter().filter(|b| b.is_object()) {
                match b.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                    "text" => {
                        let tx = b.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        if !tx.trim().is_empty() {
                            s.last_assistant_text = tx.to_string();
                        }
                    }
                    "tool_use" => {
                        let name = b
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("(unnamed)")
                            .to_string();
                        *s.tool_calls.entry(name).or_insert(0) += 1;
                    }
                    _ => {}
                }
            }
        } else if ty == "result" {
            s.has_result = true;
            s.result_text = obj
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_error_field = obj.get("is_error").and_then(|v| v.as_bool());
            s.is_error = is_error_field == Some(true);
            let subtype = obj.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
            // (0.2.4, F91-4) success needs affirmative evidence: is_error present and false, and
            // subtype success (or absent). A missing is_error is NOT success.
            s.result_ok =
                is_error_field == Some(false) && (subtype.is_empty() || subtype == "success");
            s.num_turns = obj.get("num_turns").and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|x| x.parse().ok()))
            });
            // (0.2.4, F89-5) Rebuild usage from an ALLOW LIST of numeric counters — never clone the
            // whole usage object (a sentinel/secret in an extra field must not reach run.json).
            s.usage = claude_usage(&obj);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let d = std::env::temp_dir().join(format!("c3-work-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_summary_counts_tools_and_reads_the_result() {
        let text = concat!(
            "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"u\",\"model\":\"glm-5.3\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"text\",\"text\":\"reading\"},{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Read\",\"input\":{\"file_path\":\"a\"}},{\"type\":\"tool_use\",\"id\":\"t2\",\"name\":\"Bash\",\"input\":{}}]}}\n",
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\"}]}}\n",
            "{\"type\":\"assistant\",\"message\":{\"model\":\"glm-5.3\",\"content\":[{\"type\":\"tool_use\",\"id\":\"t3\",\"name\":\"Write\",\"input\":{\"file_path\":\"b\"}},{\"type\":\"text\",\"text\":\"done\"}]}}\n",
            "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":4,\"result\":\"final report\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5},\"total_cost_usd\":0.02}\n"
        );
        let s = summarize_events(text);
        assert!(s.has_result && !s.is_error);
        assert_eq!(s.result_text, "final report");
        assert_eq!(s.last_assistant_text, "done");
        assert_eq!(s.num_turns, Some(4));
        assert_eq!(s.assistant_count, 2);
        assert_eq!(
            s.tool_calls.into_iter().collect::<Vec<_>>(),
            vec![
                ("Bash".to_string(), 1),
                ("Read".to_string(), 1),
                ("Write".to_string(), 1)
            ]
        );
        assert_eq!(s.usage.unwrap()["total_cost_usd"], 0.02);
        // a partial stream (a run that died) keeps the last assistant text
        let d = summarize_events(&text[..text.find("{\"type\":\"result\"").unwrap()]);
        assert!(!d.has_result);
        assert_eq!(d.last_assistant_text, "done");
        assert_eq!(d.num_turns, None);
    }

    #[test]
    fn path_of_status_line_and_diff_text() {
        let before = vec!["?? .collab/".to_string(), " M src/lib.rs".to_string()];
        let after = vec![
            "?? .collab/".to_string(),
            " M src/lib.rs".to_string(),
            "?? done.txt".to_string(),
            "R  old.rs -> new.rs".to_string(),
        ];
        assert_eq!(path_of_status_line("?? a.txt"), "a.txt");
        assert_eq!(path_of_status_line("R  old -> new"), "new");
        let d = diff_text(
            &before,
            &after,
            &[" src/lib.rs | 2 +-\n 1 file changed".to_string()],
        );
        assert!(d.contains("git status --short (before the run):"));
        assert!(d.contains("git status --short (after the run):"));
        assert!(d.contains("git diff --stat (after the run):"));
    }

    /// (0.2.4, F89-7) `files_changed` compares CONTENT IDENTITY: a file already dirty before the
    /// run that the worker edits further is listed; one left at its baseline content is not; a new
    /// untracked file is listed.
    #[test]
    fn files_changed_uses_content_identity() {
        let repo = scratch("fc");
        let git = |a: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(a)
                .output()
                .expect("git on PATH");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.com"]);
        git(&["config", "user.name", "T"]);
        std::fs::write(repo.join("tracked.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "init"]);
        // pre-dirty the tracked file BEFORE the run (this is the baseline)
        std::fs::write(repo.join("tracked.txt"), "two\n").unwrap();
        let before = status_short(&repo);
        let base_h = baseline_hashes(&repo, &before);
        // the worker edits the pre-dirty file FURTHER and creates a new one
        std::fs::write(repo.join("tracked.txt"), "three\n").unwrap();
        std::fs::write(repo.join("new.txt"), "x\n").unwrap();
        let after = status_short(&repo);
        let changed = files_changed(&base_h, &after, &repo);
        assert!(changed.contains(&"tracked.txt".to_string()), "{changed:?}");
        assert!(changed.contains(&"new.txt".to_string()), "{changed:?}");
        // revert the pre-dirty file to its baseline-dirty content: it is no longer the worker's
        std::fs::write(repo.join("tracked.txt"), "two\n").unwrap();
        let after2 = status_short(&repo);
        let changed2 = files_changed(&base_h, &after2, &repo);
        assert!(!changed2.contains(&"tracked.txt".to_string()), "{changed2:?}");
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// (0.2.4, F89-5) `summarize_events` rebuilds usage from an allow list of numeric counters:
    /// a sentinel in an extra usage field never reaches the summary.
    #[test]
    fn usage_is_rebuilt_from_an_allow_list() {
        let text = concat!(
            "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":2,",
            "\"result\":\"ok\",\"total_cost_usd\":0.01,\"duration_ms\":1234,",
            "\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":3,",
            "\"secret\":\"sentinel-do-not-leak\"}}\n"
        );
        let s = summarize_events(text);
        let usage = s.usage.expect("usage present");
        assert_eq!(usage["input_tokens"], 10);
        assert_eq!(usage["output_tokens"], 5);
        assert_eq!(usage["cache_read_input_tokens"], 3);
        assert_eq!(usage["total_cost_usd"], 0.01);
        assert_eq!(usage["num_turns"], 2);
        assert_eq!(usage["duration_ms"], 1234);
        assert!(usage.get("secret").is_none(), "{usage}");
        assert!(
            !serde_json::to_string(&usage).unwrap().contains("sentinel"),
            "{usage}"
        );
    }

    #[test]
    fn the_record_round_trips_and_allocates() {
        let work = scratch("record");
        let mut rec = RunRecord {
            id: "01-zai-claude-glm-5.3".into(),
            task: "t".into(),
            seq: 1,
            provider: "ZAI-claude".into(),
            model: "glm-5.3".into(),
            engine: "claude".into(),
            argv: vec!["-p".into(), "--model".into(), "glm-5.3".into()],
            child_env_scrubbed: vec!["CLAUDE_CODE_SOMETHING".into()],
            started: "2026-10-20T10:00:00+00:00".into(),
            status: "done".into(),
            timeout_sec: 3600,
            ..Default::default()
        };
        let dir = work.join(&rec.id);
        std::fs::create_dir_all(&dir).unwrap();
        write_record(&dir, &rec).unwrap();
        assert_eq!(read_record(&dir).unwrap().unwrap().id, rec.id);
        assert!(live_run(&work).unwrap().is_none());
        // a running record with a live pid (this process) is live; with a dead one it is
        // rewritten as died and never counts as live
        rec.status = "running".into();
        rec.pid = Some(std::process::id() as i64);
        write_record(&dir, &rec).unwrap();
        assert_eq!(live_run(&work).unwrap().as_deref(), Some("01-zai-claude-glm-5.3"));
        rec.pid = None;
        write_record(&dir, &rec).unwrap();
        assert!(
            live_run(&work).unwrap().is_none(),
            "an old running record without a pid is dead"
        );
        assert_eq!(read_record(&dir).unwrap().unwrap().status, "died");
        rec.pid = Some(std::process::id() as i64);
        rec.status = "done".into();
        write_record(&dir, &rec).unwrap();
        assert!(live_run(&work).unwrap().is_none());
        // the next sequence continues past it; the parts are sanitized
        let (id2, seq2, dir2) = allocate(&work, "byteplus-claude", "deepseek/v4.1 flash").unwrap();
        assert_eq!(seq2, 2);
        assert!(id2.starts_with("02-byteplus-claude-deepseek-v4.1-flash"));
        assert!(dir2.is_dir());
        let _ = std::fs::remove_dir_all(&work);
    }
}
