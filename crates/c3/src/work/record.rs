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
    /// (0.2.4) `"tests"` for a run started `--role tests` (it writes tests for another author's
    /// change); `""` for an ordinary run.
    #[serde(default)]
    pub role: String,
    /// (0.2.4) The work run whose change a `--role tests` run tests (`--of`); `""` when it tests
    /// the working tree's change against HEAD (the coordinator's own code).
    #[serde(default)]
    pub of: String,
    /// (0.2.4) The globs the run may change (`--scope`, plus the test-path defaults of a
    /// `--role tests` run); empty = no scope check.
    #[serde(default)]
    pub scope: Vec<String>,
    /// (0.2.4) The changed files outside `scope` - a post-hoc check, not a containment: no worker
    /// engine can refuse an edit by path before it happens.
    #[serde(default)]
    pub scope_violations: Vec<String>,
    /// (0.2.4) The post-mortem of a run the `--status`/`--wait` reader judged `died` because its
    /// owning process was gone: which side vanished, the child exit, elapsed, the last engine event
    /// and the sanitized console-log tail - so a killed `--detach` background is not a silent
    /// `running`→`died` with no diagnostics. Reuses the consult [`DeathRecord`]; written ONCE by
    /// [`recover_dead_run`] under the admission lock, never over a newer generation. Absent on a
    /// clean run.
    ///
    /// [`DeathRecord`]: crate::consult::detached::DeathRecord
    #[serde(default)]
    pub death: Option<crate::consult::detached::DeathRecord>,
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
    // (0.2.4, F93-1) A PURE fallible reader — it NEVER writes. The dead-owner → `died` transition
    // is a separate, locked operation ([`recover_dead_run`], via [`read_for_display`]); a status
    // reader must not race the admission lock by rewriting a record it holds only a stale snapshot
    // of. Liveness is decided by [`is_live`] at the call site (so a dead pending record never counts
    // as a live run); only the display path persists `died`.
    let p = dir.join("run.json");
    let bytes = match std::fs::read(&p) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} could not be read ({e})", p.display())),
    };
    let rec: RunRecord = serde_json::from_slice(&bytes)
        .map_err(|e| format!("{} does not parse: {e}", p.display()))?;
    Ok(Some(rec))
}

/// (0.2.4, F93-1) Whether two records are the SAME generation of the same run: the owning pid, its
/// start time and the attempt number all match. A recovery write only proceeds when the record on
/// disk is still the generation the reader snapshotted (so it cannot clobber a newer claim).
fn same_generation(a: &RunRecord, b: &RunRecord) -> bool {
    a.pid == b.pid && a.pid_start == b.pid_start && a.attempt == b.attempt
}

/// (0.2.4, F93-1) TEST-ONLY recovery barrier (honoured only in test mode): when
/// `CODEX_CONSULT_TEST_WORK_RECOVERY_BARRIER` names a path, wait (bounded) until it exists — the
/// acceptance test interposes a competing resume claim between the status snapshot and the locked
/// recovery re-read.
///
/// (0.2.4, F95-1) Before waiting it SIGNALS barrier entry: when
/// `CODEX_CONSULT_TEST_WORK_RECOVERY_ENTERED` names a path it touches that marker, so the test can
/// wait on a real entry signal (bounded) instead of a fixed sleep — the dead-owner snapshot is
/// already captured by the time this runs (the caller read the record before calling the barrier).
fn test_recovery_barrier() {
    if let Some(marker) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_WORK_RECOVERY_ENTERED") {
        let _ = std::fs::write(&marker, b"1");
    }
    if let Some(gate) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_WORK_RECOVERY_BARRIER") {
        let gate = PathBuf::from(gate);
        for _ in 0..600 {
            if gate.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

/// (0.2.4, F93-1) Persist the `died` transition for a pending record whose owner looks dead, UNDER
/// the task admission lock. Re-reads the CURRENT record and writes `died` ONLY when it is still the
/// same generation as `snapshot` AND still pending AND its owner is still dead — so a resume that
/// claimed the record (new pid/attempt) in the meantime is never overwritten. Returns the current
/// record after the attempt and PROPAGATES a write failure.
pub fn recover_dead_run(
    work_root: &Path,
    dir: &Path,
    snapshot: &RunRecord,
) -> Result<Option<RunRecord>, String> {
    let _lock = admission_lock(work_root)?;
    let mut rec = match read_record(dir)? {
        Some(r) => r,
        None => return Ok(None),
    };
    if same_generation(&rec, snapshot) && is_pending(&rec.status) && !is_live(&rec) {
        let now = chrono::Utc::now();
        rec.status = "died".into();
        rec.ended = Some(now.to_rfc3339());
        rec.args = None;
        // (0.2.4) Attach the death post-mortem ONCE, under this same lock: the record is pending and
        // this reader owns the `running`→`died` transition, so no other writer can race it.
        if rec.death.is_none() {
            rec.death = Some(build_reader_death(dir, &rec, now));
        }
        write_record(dir, &rec)?;
    }
    Ok(Some(rec))
}

/// (0.2.4) The last non-empty line's timestamp of a work event stream (`ts`/`timestamp`/`time`/
/// `created_at`/`at`), `""` when the file or a timestamp cannot be read. Mirrors the consult
/// reader's `last_event_time`.
fn last_event_time(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let Some(last) = text.lines().rev().find(|l| !l.trim().is_empty()) else {
        return String::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(last) else {
        return String::new();
    };
    for k in ["ts", "timestamp", "time", "created_at", "at"] {
        if let Some(s) = v.get(k).and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
            return s.to_string();
        }
    }
    String::new()
}

/// (0.2.4) Build the death record a `--status`/`--wait` reader attaches when it judges a work run
/// `died`. The owning process (the run's "bridge") is gone — that is why the reader got here — and a
/// work run tracks no separate engine pid, so `vanished` reads `bridge`. The consult
/// [`build_death_record`] builder is reused over the run dir's console log (`.log`, detached runs
/// only) and event stream (`events.jsonl`). The two file reads are best-effort.
///
/// [`build_death_record`]: crate::consult::detached::build_death_record
fn build_reader_death(
    dir: &Path,
    rec: &RunRecord,
    now: chrono::DateTime<chrono::Utc>,
) -> crate::consult::detached::DeathRecord {
    use crate::consult::detached::{build_death_record, DeathInputs};
    let log_text = std::fs::read_to_string(dir.join(".log")).unwrap_or_default();
    let events = dir.join("events.jsonl");
    let events_mtime = std::fs::metadata(&events)
        .and_then(|m| m.modified())
        .ok()
        .map(chrono::DateTime::<chrono::Utc>::from);
    let last_event = last_event_time(&events);
    build_death_record(DeathInputs {
        recorded_by: "reader",
        now,
        started: Some(rec.started.as_str()),
        bridge_alive: false,
        engine_alive: None,
        child_exit: rec.exit_code,
        log_text: &log_text,
        events_file: "events.jsonl",
        events_mtime,
        last_event: &last_event,
        partial_hint: "",
    })
}

/// (0.2.4, F93-1) Read a record for the DISPLAY path (`--status`/`--wait`/the resume listing): a
/// pure read, then — when the owner looks dead — the `died` transition done UNDER the admission
/// lock via [`recover_dead_run`] (never an unlocked write), with its result and errors surfaced.
pub fn read_for_display(work_root: &Path, dir: &Path) -> Result<Option<RunRecord>, String> {
    let rec = match read_record(dir)? {
        Some(r) => r,
        None => return Ok(None),
    };
    if is_pending(&rec.status) && !is_live(&rec) {
        test_recovery_barrier();
        return recover_dead_run(work_root, dir, &rec);
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
            Err(e) => return Err(format!("could not inspect {} ({e})", run_json.display())),
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
            // (0.2.4, F93-1) read_record no longer rewrites a ghost, so liveness is checked here:
            // a pending record whose owner is dead does NOT count as a live run.
            if is_pending(&r.status) && is_live(&r) {
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
///
/// (0.2.4) The baseline content is also WRITTEN to git's object store (`hash-object -w`; a loose
/// object, never a commit or a ref), so `patch.diff` can show what the worker changed in a file
/// that was already dirty.
pub fn baseline_hashes(repo: &Path, before: &[String]) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for line in before {
        let p = path_of_status_line(line);
        if !p.is_empty() {
            let h = if repo.join(&p).is_file() {
                store_blob(repo, &p).unwrap_or_default()
            } else {
                hash_of(repo, &p)
            };
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

/// The stdout of a git command that succeeded; `None` otherwise.
fn git_raw(repo: &Path, args: &[&str]) -> Option<String> {
    let o = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).to_string())
}

/// (0.2.4) Write a working-tree file's content to the object store; its blob id.
fn store_blob(repo: &Path, path: &str) -> Option<String> {
    git_raw(repo, &["hash-object", "-w", "--", path])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// (0.2.4) The empty blob, written so a diff against it (a new or a deleted file) resolves.
fn empty_blob(repo: &Path) -> Option<String> {
    git_raw(repo, &["hash-object", "-w", "--stdin"])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// (0.2.4) The blob of `path` at HEAD; `None` when HEAD has no such path.
fn head_blob(repo: &Path, path: &str) -> Option<String> {
    git_raw(
        repo,
        &["rev-parse", "--verify", "-q", &format!("HEAD:{path}")],
    )
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

/// (0.2.4) A `git status --short` path that names an untracked DIRECTORY (`?? dir/`) stands for
/// every untracked file under it.
fn expand_path(repo: &Path, p: &str) -> Vec<String> {
    if p.ends_with('/') {
        git_lines(
            repo,
            &["ls-files", "--others", "--exclude-standard", "--", p],
        )
    } else {
        vec![p.to_string()]
    }
}

/// (0.2.4) The unified diff of one file from the blob `before` (`None` = it did not exist) to its
/// working-tree content (absent = deleted); `""` when nothing changed. The hunks are git's
/// (`git diff <blob> <blob>`); the header names the path, not the blob ids.
pub fn file_patch(repo: &Path, path: &str, before: Option<&str>) -> String {
    let after = if repo.join(path).is_file() {
        store_blob(repo, path)
    } else {
        None
    };
    if before.is_none() && after.is_none() || before == after.as_deref() {
        return String::new();
    }
    let Some(empty) = empty_blob(repo) else {
        return String::new();
    };
    let a = before.unwrap_or(&empty);
    let b = after.as_deref().unwrap_or(&empty);
    let Some(out) = git_raw(repo, &["diff", "--no-color", "--no-ext-diff", a, b]) else {
        return String::new();
    };
    let from = if before.is_some() {
        format!("a/{path}")
    } else {
        "/dev/null".to_string()
    };
    let to = if after.is_some() {
        format!("b/{path}")
    } else {
        "/dev/null".to_string()
    };
    let mut text = format!("diff --git a/{path} b/{path}\n--- {from}\n+++ {to}\n");
    let mut body = false;
    for line in out.lines() {
        if line.starts_with("Binary files ") {
            text.push_str(&format!("Binary files {from} and {to} differ\n"));
            break;
        }
        body |= line.starts_with("@@");
        if body {
            text.push_str(line);
            text.push('\n');
        }
    }
    text
}

/// (0.2.4) `patch.diff` of a work run: every file it changed, from its baseline content (the
/// stored blob of a pre-dirty path, else HEAD's, else nothing) to its content now.
pub fn run_patch(
    repo: &Path,
    baseline_hashes: &BTreeMap<String, String>,
    changed: &[String],
) -> String {
    let mut out = String::new();
    for p in changed {
        for f in expand_path(repo, p) {
            let before = match baseline_hashes.get(&f).map(String::as_str) {
                Some("<absent>") => None,
                Some(h) if !h.is_empty() => Some(h.to_string()),
                _ => head_blob(repo, &f),
            };
            out.push_str(&file_patch(repo, &f, before.as_deref()));
        }
    }
    out
}

/// (0.2.4) The working tree's change against HEAD (the change a `--role tests` run without `--of`
/// tests), every path under `skip` (the collab directory) left out.
pub fn worktree_patch(repo: &Path, skip: &str) -> String {
    let mut out = String::new();
    for line in status_short(repo) {
        let mut paths = expand_path(repo, &path_of_status_line(&line));
        // a staged rename (`R  old -> new`): the old path's deletion belongs to the change too
        if let Some((old, _)) = line.get(3..).unwrap_or("").split_once(" -> ") {
            paths.insert(0, old.trim().to_string());
        }
        for f in paths {
            if !skip.is_empty() && f.starts_with(skip) {
                continue;
            }
            let before = head_blob(repo, &f);
            out.push_str(&file_patch(repo, &f, before.as_deref()));
        }
    }
    out
}

/// (0.2.4) The changed files no `scope` glob matches, every path under `skip` (the collab
/// directory: the run's own record) left out. An empty scope checks nothing.
pub fn scope_violations(
    repo: &Path,
    scope: &[String],
    changed: &[String],
    skip: &str,
) -> Vec<String> {
    if scope.is_empty() {
        return Vec::new();
    }
    let kept = |changed: &[String]| -> Vec<String> {
        changed
            .iter()
            .flat_map(|p| expand_path(repo, p))
            .filter(|p| skip.is_empty() || !p.starts_with(skip))
            .collect()
    };
    match build_scope_set(scope) {
        Ok(set) => kept(changed)
            .into_iter()
            .filter(|p| !set.is_match(p))
            .collect(),
        Err(reason) => {
            // Fail closed (Muse, review-2026-10-30): a scope that cannot be evaluated protects
            // nothing, so every changed path (outside the skip prefix) is a violation and the
            // reason is listed FIRST so the caller prints it (`OUT OF SCOPE: ...`) rather than the
            // run quietly passing an unchecked scope.
            let mut out = vec![format!("{SCOPE_CHECK_FAILED}: {reason}")];
            out.extend(kept(changed));
            out
        }
    }
}

/// The sentinel that leads a [`scope_violations`] result when the scope could not be built, so the
/// `OUT OF SCOPE` line names why the check failed closed.
pub const SCOPE_CHECK_FAILED: &str = "<scope check could not be built>";

/// Build the scope glob set, surfacing the reason a bad glob or a failed build is a visible error
/// (so [`scope_violations`] fails closed instead of silently checking nothing).
fn build_scope_set(scope: &[String]) -> Result<globset::GlobSet, String> {
    let mut b = globset::GlobSetBuilder::new();
    for g in scope {
        let glob =
            globset::Glob::new(g).map_err(|e| format!("scope glob '{g}' is invalid: {e}"))?;
        b.add(glob);
    }
    b.build()
        .map_err(|e| format!("the scope glob set could not be built: {e}"))
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
        if let Some(n) = result_obj.get(k).and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|x| x.parse().ok()))
        }) {
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
        assert!(
            !changed2.contains(&"tracked.txt".to_string()),
            "{changed2:?}"
        );
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// (0.2.4) A scratch git repository with one commit of `app.txt` ("one\n").
    fn git_repo(tag: &str) -> PathBuf {
        let repo = scratch(tag);
        let git = |a: &[&str]| {
            let st = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(a)
                .status()
                .expect("git on PATH");
            assert!(st.success(), "git {a:?} failed");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.com"]);
        git(&["config", "user.name", "T"]);
        std::fs::write(repo.join("app.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "init"]);
        repo
    }

    /// (0.2.4, cases 1-4) `file_patch`: the header names the path (never the blob ids); a new
    /// file diffs from /dev/null, a deleted one to /dev/null, a modified one hunk-by-hunk, and
    /// an unchanged one is "".
    #[test]
    fn file_patch_shapes_for_new_deleted_modified_and_unchanged() {
        let repo = git_repo("fp");
        // modified: app.txt edited past HEAD
        std::fs::write(repo.join("app.txt"), "two\n").unwrap();
        let head = head_blob(&repo, "app.txt").expect("app.txt at HEAD");
        let text = file_patch(&repo, "app.txt", Some(&head));
        assert!(
            text.starts_with("diff --git a/app.txt b/app.txt\n--- a/app.txt\n+++ b/app.txt\n"),
            "{text}"
        );
        assert!(text.contains("@@"), "{text}");
        assert!(text.contains("-one"), "{text}");
        assert!(text.contains("+two"), "{text}");
        assert!(
            !text.contains(&head),
            "the header must name the path, not the blob id: {text}"
        );
        // a new file: from /dev/null
        std::fs::write(repo.join("new.txt"), "n\n").unwrap();
        let text = file_patch(&repo, "new.txt", None);
        assert!(
            text.starts_with("diff --git a/new.txt b/new.txt\n--- /dev/null\n+++ b/new.txt\n"),
            "{text}"
        );
        assert!(text.contains("+n"), "{text}");
        // unchanged content: no patch at all
        let now = store_blob(&repo, "new.txt").expect("hash new.txt");
        assert_eq!(file_patch(&repo, "new.txt", Some(&now)), "");
        // deleted: to /dev/null
        std::fs::remove_file(repo.join("app.txt")).unwrap();
        let text = file_patch(&repo, "app.txt", Some(&head));
        assert!(
            text.starts_with("diff --git a/app.txt b/app.txt\n--- a/app.txt\n+++ /dev/null\n"),
            "{text}"
        );
        assert!(text.contains("-one"), "{text}");
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// (0.2.4, cases 5-6) `run_patch`: a pre-dirty file the worker edits FURTHER shows only the
    /// worker's hunk (the baseline blob - WRITTEN to the object store by `baseline_hashes`, its
    /// content never in any commit - is the base, not HEAD), and an untracked directory in
    /// `changed` expands to the files under it.
    #[test]
    fn run_patch_diffs_from_the_baseline_content_and_expands_directories() {
        let repo = git_repo("rp");
        // pre-dirty the tracked file BEFORE the run (this is the baseline)
        std::fs::write(repo.join("app.txt"), "one\npre-dirty\n").unwrap();
        let before = status_short(&repo);
        let base_h = baseline_hashes(&repo, &before);
        // the worker edits the pre-dirty file FURTHER and creates an untracked directory
        std::fs::write(repo.join("app.txt"), "one\npre-dirty\nworker-edit\n").unwrap();
        std::fs::create_dir_all(repo.join("newdir")).unwrap();
        std::fs::write(repo.join("newdir").join("a.txt"), "a\n").unwrap();
        std::fs::write(repo.join("newdir").join("b.txt"), "b\n").unwrap();
        let after = status_short(&repo);
        let changed = files_changed(&base_h, &after, &repo);
        assert!(changed.contains(&"app.txt".to_string()), "{changed:?}");
        assert!(changed.contains(&"newdir/".to_string()), "{changed:?}");
        let patch = run_patch(&repo, &base_h, &changed);
        // only the worker's hunk: the pre-dirty line is the baseline, not part of the change
        assert!(patch.contains("+worker-edit"), "{patch}");
        assert!(!patch.contains("+pre-dirty"), "{patch}");
        // the untracked directory expanded to both files, each a new file
        assert!(patch.contains("+++ b/newdir/a.txt"), "{patch}");
        assert!(patch.contains("+++ b/newdir/b.txt"), "{patch}");
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// (0.2.4, case 7) `worktree_patch`: every dirty path against HEAD, with the collab
    /// directory (the `skip` prefix) left out - and part of the change when nothing is skipped.
    #[test]
    fn worktree_patch_skips_the_collab_directory() {
        let repo = git_repo("wt");
        std::fs::write(repo.join("app.txt"), "two\n").unwrap();
        std::fs::write(repo.join("free.txt"), "f\n").unwrap();
        let collab = repo.join(".collab").join("t").join("work").join("01-x");
        std::fs::create_dir_all(&collab).unwrap();
        std::fs::write(collab.join("run.json"), "{}").unwrap();
        let with_skip = worktree_patch(&repo, ".collab/");
        assert!(with_skip.contains("+++ b/app.txt"), "{with_skip}");
        assert!(with_skip.contains("+++ b/free.txt"), "{with_skip}");
        assert!(!with_skip.contains(".collab"), "{with_skip}");
        let without = worktree_patch(&repo, "");
        assert!(without.contains("run.json"), "{without}");
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// (0.2.4, cases 8-10) `scope_violations`: an empty scope checks nothing; the test-path
    /// defaults cover every test file (a directory entry expands); a file no glob matches is
    /// listed; a path under the skip prefix never counts.
    #[test]
    fn scope_violations_empty_defaults_violation_and_skip() {
        let repo = git_repo("sv");
        std::fs::create_dir_all(repo.join("newdir")).unwrap();
        std::fs::write(repo.join("newdir").join("n1.txt"), "1\n").unwrap();
        std::fs::write(repo.join("newdir").join("n2.txt"), "2\n").unwrap();
        let changed = [
            "src/lib.rs",
            "src/x_test.rs",
            "tests/it.rs",
            "crates/x/tests/deep.rs",
            "pkg/tests.rs",
            "notes.md",
            ".collab/t/work/01-x/run.json",
            "newdir/",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
        // an empty scope checks nothing
        assert!(scope_violations(&repo, &[], &changed, ".collab/").is_empty());
        // the test-path defaults cover every test file; the rest violate
        let defaults = [
            "tests/**",
            "**/tests/**",
            "**/tests.rs",
            "**/*_test.rs",
            "**/*_tests.rs",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
        let v = scope_violations(&repo, &defaults, &changed, ".collab/");
        assert_eq!(
            v,
            vec![
                "src/lib.rs".to_string(),
                "notes.md".to_string(),
                "newdir/n1.txt".to_string(),
                "newdir/n2.txt".to_string()
            ],
            "{v:?}"
        );
        // a path under the skip prefix (the run's own record) never counts; without the skip it does
        let one = vec!["src/lib.rs".to_string()];
        let record = vec![".collab/t/work/01-x/run.json".to_string()];
        assert!(scope_violations(&repo, &one, &record, ".collab/").is_empty());
        assert_eq!(
            scope_violations(&repo, &one, &record, ""),
            vec![".collab/t/work/01-x/run.json".to_string()]
        );
        // a scope that cannot be built fails CLOSED: a leading sentinel (with the reason) plus
        // every changed path outside the skip prefix (Muse, review-2026-10-30).
        let bad = vec!["src/**".to_string(), "a[b".to_string()];
        let v = scope_violations(&repo, &bad, &changed, ".collab/");
        assert!(
            v.first().is_some_and(|s| s.starts_with(SCOPE_CHECK_FAILED)),
            "first entry names the failure: {v:?}"
        );
        assert!(
            v.iter().any(|p| p == "src/lib.rs")
                && v.iter().any(|p| p == "tests/it.rs")
                && v.iter().any(|p| p == "newdir/n1.txt"),
            "every changed path outside skip is a violation: {v:?}"
        );
        assert!(
            !v.iter().any(|p| p.contains(".collab/")),
            "the skip prefix is still honoured: {v:?}"
        );
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
        assert_eq!(
            live_run(&work).unwrap().as_deref(),
            Some("01-zai-claude-glm-5.3")
        );
        rec.pid = None;
        write_record(&dir, &rec).unwrap();
        assert!(
            live_run(&work).unwrap().is_none(),
            "an old running record without a pid is dead"
        );
        // (F93-1) read_record is PURE: the ghost stays `running` on disk until the DISPLAY path
        // persists `died` under the admission lock.
        assert_eq!(read_record(&dir).unwrap().unwrap().status, "running");
        assert_eq!(
            read_for_display(&work, &dir).unwrap().unwrap().status,
            "died"
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

    /// (0.2.4) The display path attaches a death post-mortem when it judges a dead-owner run
    /// `died`: the owning process is gone (`vanished` = `bridge`), the console-log tail and the
    /// last engine event are captured, and the record is written ONCE (a second display read keeps
    /// the same record, never rebuilding it).
    #[test]
    fn a_dead_owner_run_gets_a_death_record() {
        let work = scratch("death");
        let dir = work.join("01-zai-claude-glm-5.3");
        std::fs::create_dir_all(&dir).unwrap();
        // a console log and an event stream for the reader to read back
        std::fs::write(dir.join(".log"), "launch line\nthe last console line\n").unwrap();
        std::fs::write(
            dir.join("events.jsonl"),
            "{\"type\":\"assistant\",\"ts\":\"2026-10-30T12:00:00Z\"}\n",
        )
        .unwrap();
        let rec = RunRecord {
            id: "01-zai-claude-glm-5.3".into(),
            task: "t".into(),
            seq: 1,
            provider: "ZAI-claude".into(),
            model: "glm-5.3".into(),
            engine: "claude".into(),
            // a dead owner: no live pid and far older than the start grace
            started: "2026-10-20T10:00:00+00:00".into(),
            status: "running".into(),
            pid: None,
            ..Default::default()
        };
        write_record(&dir, &rec).unwrap();
        let got = read_for_display(&work, &dir).unwrap().unwrap();
        assert_eq!(got.status, "died");
        let d = got.death.expect("a death record");
        assert_eq!(d.recorded_by, "reader");
        assert_eq!(d.vanished, "bridge");
        assert_eq!(d.events_file, "events.jsonl");
        assert_eq!(d.last_event, "2026-10-30T12:00:00Z");
        assert!(
            d.log_tail
                .iter()
                .any(|l| l.contains("the last console line")),
            "{:?}",
            d.log_tail
        );
        // written ONCE: a second display read keeps the same record (status is already died)
        let again = read_for_display(&work, &dir).unwrap().unwrap();
        assert_eq!(again.status, "died");
        assert_eq!(again.death, Some(d));
        let _ = std::fs::remove_dir_all(&work);
    }
}
