//! The evidence store: the files under `.collab/<task>/`, their locks and the commit
//! write order.
//!
//! Mirrors `codex-consult-common.ps1`: the ownership lock `.consult.lock` and the commit
//! lock `.consult.write.lock` (each a permanent file *owned by holding it open*, released
//! when the handle closes - `Enter-TaskLock`/`Enter-WriteLock`/`Exit-TaskLock`), the
//! recovery record `.consult.pending.json` (a panel member: `.consult.pending-<NN>.json`,
//! `New-PendingRecord`), the atomic replace (`Write-TextAtomic`/`Write-JsonFile`) and the
//! commit sequence (`Enter-StoreCommit` -> re-read -> `Complete-StoreCommit`: findings.json
//! then sessions.json -> remove the recovery record -> `Exit-StoreCommit`).
//!
//! ## The lock guards (F02-1, F04-2, F08-4, F09-2/3)
//!
//! [`FilesStore::take_task_lock`] and [`FilesStore::take_write_lock`] take a *real*
//! exclusive OS lock, not just an open handle. The plugin opens both lock files with
//! `[IO.File]::Open(path, OpenOrCreate, ReadWrite, $share)` where `$share` is
//! `FileShare.Read` on Windows and `FileShare.None` elsewhere
//! (`codex-consult-common.ps1:6205`, `:6286`) - a second `ReadWrite` open then hits a
//! sharing violation and fails. C3 reproduces that exactly on Windows via
//! `OpenOptionsExt::share_mode(FILE_SHARE_READ)`; on Unix, where there is no share mode, it
//! takes a `flock` advisory lock through `fs4`. The task lock is **fail-fast** (one
//! attempt, `Enter-TaskLock`); the write lock **backs off** 50 ms doubling to a 1 s cap up
//! to a 60 s timeout and reports the measured wait (`Enter-WriteLock`), so
//! [`CommitReceipt::wait_ms`] carries `commit_wait_ms`.
//!
//! Note: the plugin passes `FileShare.Read` for the **write** lock too (`Enter-WriteLock`,
//! `:6286`), not `FileShare.None`; C3 mirrors the literal (both locks use
//! `FILE_SHARE_READ` on Windows) rather than the guess in the review brief, so the lock
//! record stays readable by a diagnostics reader while the exclusion still holds.
//!
//! ## The commit seam (B, C, G)
//!
//! Every store mutation now takes a [`WriteLock`] guard, so the [`COMMIT_WRITE_ORDER`]
//! cannot be bypassed: [`FilesStore::commit`], [`FilesStore::update_findings`],
//! [`FilesStore::update_pending`] and [`FilesStore::remove_pending`] all require `&WriteLock`.
//! `commit` re-reads `findings.json` under the lock and applies a [`FindingsDelta`] to the
//! fresh copy (never a caller snapshot, F02-2/F04-1), inserts the ledger entry by `n`
//! (refusing a duplicate, F03-4/F04-3), and treats the recovery record by an explicit
//! [`RecoveryDisposition`] so a `launching`/`survivors` record is never deleted unless the
//! caller says [`RecoveryDisposition::Remove`] (F02-4). A cleanup failure *after* the
//! commit point (sessions.json renamed) is a [`CommitReceipt::cleanup_warning`], not a
//! commit error (F02-5).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::findings::{
    Finding, FindingStatus, FindingsFile, Rating, ReviewerCheck, TransitionError,
};
use crate::ledger::{AddEntryError, LedgerBook, LedgerEntry, SessionsFile};
use crate::ps_json;
use crate::task_slug::{self, TaskSlug};

/// The ownership lock file name.
pub const OWNERSHIP_LOCK: &str = ".consult.lock";
/// The commit (write) lock file name.
pub const WRITE_LOCK: &str = ".consult.write.lock";
/// The single-run recovery record file name.
pub const PENDING: &str = ".consult.pending.json";
/// The write-lock backoff ceiling in seconds (`Get-WriteLockTimeout` default; overridable
/// by the plugin's `CODEX_CONSULT_TEST_WRITE_LOCK_SEC`).
pub const WRITE_LOCK_TIMEOUT_SEC: u64 = 60;

/// The state of an interrupted run's recovery record (`$script:PendingStates`). Tolerant of
/// a token a later plugin wave may add ([`PendingState::Other`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PendingState {
    #[default]
    Reserved,
    Launching,
    Running,
    Survivors,
    Committing,
    /// A state token C3 does not know; preserved verbatim.
    Other(String),
}

impl PendingState {
    /// The on-disk token.
    pub fn as_str(&self) -> &str {
        match self {
            PendingState::Reserved => "reserved",
            PendingState::Launching => "launching",
            PendingState::Running => "running",
            PendingState::Survivors => "survivors",
            PendingState::Committing => "committing",
            PendingState::Other(s) => s,
        }
    }

    /// Parse a token into a known variant or [`PendingState::Other`].
    pub fn from_token(s: &str) -> PendingState {
        match s {
            "reserved" => PendingState::Reserved,
            "launching" => PendingState::Launching,
            "running" => PendingState::Running,
            "survivors" => PendingState::Survivors,
            "committing" => PendingState::Committing,
            other => PendingState::Other(other.to_string()),
        }
    }

    /// A `launching` or `survivors` record still protects a possibly-live child, so a
    /// commit must not delete it unless explicitly told to (F02-4).
    pub fn protects_child(&self) -> bool {
        matches!(self, PendingState::Launching | PendingState::Survivors)
    }
}

impl Serialize for PendingState {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PendingState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(PendingState::from_token(&s))
    }
}

/// The informational content of `.consult.lock` / `.consult.write.lock`. Written compact
/// (one line) plus a trailing `\n`; nothing recoverable lives here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockRecord {
    pub pid: u32,
    /// The process start time as an ISO string (used with `pid` to tell a stale lock's
    /// owner from a live one).
    pub start_time: String,
    pub host: String,
    pub task: String,
    pub started: String,
    /// Present only for a panel run.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub panel: Option<Value>,
}

/// The process-wide bridge pid the lock records name. In production c3 IS the bridge, so this
/// stays c3's own pid. Under the plugin's harnesses c3 runs as a child of a thin shim; the shim
/// exports `CODEX_CONSULT_TEST_BRIDGE_PID` (its own pid) and the c3 runtime validates it once (a
/// live ancestor only) and sets it here via [`set_bridge_pid`]. c3-core never reads the
/// environment itself — it takes the validated pid from the caller. `0` means "unset" (use this
/// process's own pid).
static BRIDGE_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Set the validated bridge pid the lock records should name (called once by the c3 runtime at
/// process start with the value of its own ancestor-validated resolution).
pub fn set_bridge_pid(pid: u32) {
    BRIDGE_PID.store(pid, std::sync::atomic::Ordering::Relaxed);
}

/// The bridge pid the lock records name: the value [`set_bridge_pid`] recorded, else this
/// process's own pid.
pub fn bridge_pid() -> u32 {
    let p = BRIDGE_PID.load(std::sync::atomic::Ordering::Relaxed);
    if p > 0 {
        p
    } else {
        std::process::id()
    }
}

impl LockRecord {
    /// A minimal lock record for `task` from the current process context (pid, machine
    /// name, now). The record is informational; the lock is the open handle.
    pub fn now(task: &TaskSlug, panel: Option<Value>) -> LockRecord {
        LockRecord {
            pid: bridge_pid(),
            start_time: String::new(),
            host: hostname(),
            task: task.as_str().to_string(),
            started: chrono::Utc::now().to_rfc3339(),
            panel,
        }
    }

    /// The compact, newline-terminated bytes the bridge writes into a lock file.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut s = serde_json::to_string(self)?;
        s.push('\n');
        Ok(s.into_bytes())
    }
}

fn hostname() -> String {
    crate::host::machine_name()
}

/// The recovery record (`New-PendingRecord`). Field order matches the literal. The recovery
/// pointers `original`, `first_reply`, `reply_json`, `raw_reply` are added by the plugin
/// via `Add-Member` during the run and cleared on success; C3 models them as omittable
/// fields (F02-7). Written with the pretty formatter as `Write-PendingFile`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PendingRecord {
    #[serde(default)]
    pub state: PendingState,
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub nn: String,
    #[serde(default)]
    pub reply: String,
    #[serde(default)]
    pub events: String,
    #[serde(default)]
    pub consult_id: String,
    #[serde(default)]
    pub started: String,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub start_time: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub launcher: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub child_pid: Option<u32>,
    #[serde(default)]
    pub child_start_time: String,
    /// `{pid, start_time, name}` entries the kill could see; kept raw.
    #[serde(default)]
    pub survivors: Vec<Value>,
    /// (wave 28e, E1 / F54-1) `{pid, why}` entries: the descendants a kill could not verify (their
    /// start time could not be read); a new record carries `[]`. Kept raw.
    #[serde(default)]
    pub unverified: Vec<Value>,
    #[serde(default)]
    pub note: String,
    /// Present only for a panel member's record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<Value>,
    /// Recovery pointer: the kept prose original (`Add-Member 'original'`); omitted when
    /// unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_reply: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_reply: Option<String>,
    /// (wave 28e, E23 / F30-1) A kill that was not confirmed and named no pid: its why. The tree is
    /// unknown - the next run releases the record only after a clean scan. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kill_unconfirmed: Option<String>,
    /// Unknown members, preserved in place on rewrite.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl PendingRecord {
    /// Serialize with the PowerShell-5.1 pretty formatter (as `Write-PendingFile`).
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        ps_json::to_ps_json_bytes(self)
    }
}

/// The key that decides a recovery record's filename: the single-run
/// `.consult.pending.json` (`nn = None`) or a panel member's `.consult.pending-<NN>.json`
/// (`Get-MemberPendingPath`). `nn` is rendered zero-padded to two digits, as
/// `Get-NextNumbers` formats it (F02-3, F04-5, F07-4, F08-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRef {
    pub task: TaskSlug,
    pub nn: Option<u32>,
}

impl PendingRef {
    /// The single-run recovery record for `task`.
    pub fn single(task: TaskSlug) -> PendingRef {
        PendingRef { task, nn: None }
    }

    /// A panel member's recovery record for `task` at handoff number `nn`.
    pub fn member(task: TaskSlug, nn: u32) -> PendingRef {
        PendingRef { task, nn: Some(nn) }
    }

    /// The file name this key addresses.
    pub fn file_name(&self) -> String {
        match self.nn {
            None => PENDING.to_string(),
            Some(nn) => format!(".consult.pending-{nn:02}.json"),
        }
    }
}

/// A recovery record read back from disk, carrying its path and parsed key so a recovery
/// pass can address it (`recover_pending`, F02-3, path-bearing per the review).
#[derive(Debug, Clone)]
pub struct RecoveredPending {
    pub record: PendingRecord,
    pub path: PathBuf,
    /// The member number parsed from the filename; `None` for the single-run record.
    pub nn: Option<u32>,
}

/// The two numbers `Get-NextNumbers` returns: the next consult number `N` and the next
/// handoff number `NN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextNumbers {
    pub n: i64,
    pub nn: u32,
}

/// A findings-only transaction, applied to `findings.json` re-read under the write lock
/// (B, F02-2/F04-1): append `new` findings, apply each `status_changes` entry through the
/// gated [`Finding::set_status`], and append `ratings` (creating the `ratings` member if a
/// fresh store had none).
#[derive(Debug, Clone, Default)]
pub struct FindingsDelta {
    pub new: Vec<Finding>,
    pub status_changes: Vec<StatusChange>,
    pub ratings: Vec<Rating>,
    /// `(existing finding id, reviewer_check)`: a later reply's report on a prior finding,
    /// appended to that finding's `reviewer_checks[]` (`Add-ReplyFindings`). An unknown id is
    /// dropped here (the caller reports it separately).
    pub reviewer_checks: Vec<(String, ReviewerCheck)>,
    /// `(old finding id, new finding id)`: the new finding supersedes the old, so the old
    /// finding gains the new id in its `superseded_by[]` (deduplicated).
    pub superseded_by: Vec<(String, String)>,
}

impl FindingsDelta {
    /// Nothing to apply (a ledger-only commit).
    pub fn is_empty(&self) -> bool {
        self.new.is_empty()
            && self.status_changes.is_empty()
            && self.ratings.is_empty()
            && self.reviewer_checks.is_empty()
            && self.superseded_by.is_empty()
    }
}

/// One gated status change addressed by finding id.
#[derive(Debug, Clone)]
pub struct StatusChange {
    pub id: String,
    pub to: FindingStatus,
    pub note: String,
    pub evidence: String,
    pub when: String,
    pub by: String,
}

/// What to do with the recovery record at the end of a commit (C, F02-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDisposition {
    /// Delete the record named by the [`PendingRef`] (the run is fully done, no survivors).
    Remove,
    /// Keep the record (a `launching`/`survivors` record still protects a possibly-live
    /// child, or a panel is mid-flight).
    Retain,
}

/// The inputs to one commit under the write lock.
pub struct CommitRequest<'a> {
    /// The ledger entry to insert by `n` (the commit point).
    pub entry: LedgerEntry,
    /// The findings transaction, applied to `findings.json` re-read under the lock.
    pub findings: FindingsDelta,
    /// The recovery record this run reserved.
    pub pending: &'a PendingRef,
    /// What to do with that recovery record after the commit point.
    pub disposition: RecoveryDisposition,
    /// Per-consultation files (handoff `.md`, ...), each a task-relative path and its bytes.
    pub files: &'a [(String, Vec<u8>)],
    /// When `sessions.json` does not yet exist, the `cwd` the first store records (the repo
    /// root, F02-14/F08-12) - not `String::new()`.
    pub bootstrap_cwd: String,
    /// When `sessions.json` does not yet exist, the `codex.tool` string (the harness/CLI
    /// version) - not the engine harness copied off the reviewer.
    pub bootstrap_tool: String,
    /// A pause (ms) held INSIDE the commit, between `findings.json` and `sessions.json` -
    /// the ORPHAN window a kill can hit and the write-lock contention window
    /// (`CODEX_CONSULT_TEST_COMMIT_PAUSE_MS`). 0 = no pause.
    pub commit_pause_ms: u64,
}

/// The outcome of a commit (G, F03-4/F04-3/F08-8).
#[derive(Debug, Clone)]
pub struct CommitReceipt {
    /// `true` once `sessions.json` is renamed (the commit point) - the run is durable even
    /// if a later cleanup step warns.
    pub committed: bool,
    /// A non-fatal problem *after* the commit point (e.g. the recovery record could not be
    /// removed); the commit still stands (F02-5).
    pub cleanup_warning: Option<String>,
    /// The write lock's measured wait, in milliseconds (`commit_wait_ms`).
    pub wait_ms: u64,
}

/// What can go wrong applying a [`FindingsDelta`].
#[derive(Debug)]
pub enum StoreError {
    /// A status change named a finding id that is not in the store.
    UnknownFinding(String),
    /// A status change was refused by the transition gate.
    Transition(String, TransitionError),
    /// The ledger refused the entry (duplicate `n`/`consult_id`).
    AddEntry(AddEntryError),
    /// A path escaped the task directory.
    Path(task_slug::PathError),
    /// An I/O or serialization error.
    Io(io::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::UnknownFinding(id) => {
                write!(f, "status change names an unknown finding id '{id}'")
            }
            StoreError::Transition(id, e) => write!(f, "finding '{id}': {e}"),
            StoreError::AddEntry(e) => write!(f, "{e}"),
            StoreError::Path(e) => write!(f, "{e}"),
            StoreError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

/// One step of the commit sequence, in order. The variants before [`WriteStep::TakeWriteLock`]
/// happen without the lock; the rest happen while it is held; releasing is implicit at the
/// end. This is the documented, machine-checkable encoding of the write order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStep {
    /// `.reply.json` - the byte-for-byte raw copy, before anything parses it.
    RawReply,
    /// Take `.consult.write.lock` (backoff up to 60 s) and mark the record `committing`.
    TakeWriteLock,
    /// Re-read `findings.json` and `sessions.json` under the lock.
    ReReadStores,
    /// The rendered handoff `.md`.
    HandoffMarkdown,
    /// `findings.json` (this run's delta applied to the fresh store).
    Findings,
    /// `sessions.json` - the entry inserted by `n`; THE commit point.
    Sessions,
    /// Remove (or keep, marked `committing`) the recovery record.
    RemoveRecoveryRecord,
    /// Close the write-lock handle.
    ReleaseWriteLock,
}

/// The commit write order (README "Write order and atomic stores"; wave-21 lock discipline).
pub const COMMIT_WRITE_ORDER: [WriteStep; 8] = [
    WriteStep::RawReply,
    WriteStep::TakeWriteLock,
    WriteStep::ReReadStores,
    WriteStep::HandoffMarkdown,
    WriteStep::Findings,
    WriteStep::Sessions,
    WriteStep::RemoveRecoveryRecord,
    WriteStep::ReleaseWriteLock,
];

/// A held task (ownership) lock: the OS file handle. Dropping it releases the lock (closes
/// the handle), exactly as `Exit-TaskLock` does; the file itself stays on disk.
#[derive(Debug)]
pub struct TaskLock {
    pub path: PathBuf,
    _file: File,
}

/// A held write (commit) lock, with the measured acquisition wait. Every store mutation
/// takes `&WriteLock`, so the write order cannot be bypassed. The lock remembers the task
/// it locked, so a commit can prove the guard belongs to the store it touches.
#[derive(Debug)]
pub struct WriteLock {
    pub path: PathBuf,
    task: TaskSlug,
    wait_ms: u64,
    _file: File,
}

impl WriteLock {
    /// The task this lock was taken for.
    pub fn task(&self) -> &TaskSlug {
        &self.task
    }

    /// The measured acquisition wait, in milliseconds (`commit_wait_ms`).
    pub fn wait_ms(&self) -> u64 {
        self.wait_ms
    }
}

/// The evidence store contract (DESIGN §4). One implementation, [`FilesStore`].
pub trait EvidenceStore {
    /// `<collab>/<task>` - the task directory that holds the stores, locks and handoffs.
    fn task_dir(&self, task: &TaskSlug) -> PathBuf;

    /// Parse `sessions.json`; `None` when it does not exist.
    fn read_sessions(&self, task: &TaskSlug) -> io::Result<Option<SessionsFile>>;

    /// Parse `findings.json`; `None` when it does not exist.
    fn read_findings(&self, task: &TaskSlug) -> io::Result<Option<FindingsFile>>;

    /// Read every recovery record of the task, path-bearing (`Read-TaskPendingRecords`).
    fn recover_pending(&self, task: &TaskSlug) -> io::Result<Vec<RecoveredPending>>;

    /// Both halves of `Get-NextNumbers`: the next consult `N` (ledger `n`, findings
    /// `source.consult` and `reviewer_checks[].consult`, leftover `n`) and the next handoff
    /// `NN` (handoff filenames, finding ids `F<NN>`, leftover `nn`).
    fn next_numbers(&self, task: &TaskSlug) -> io::Result<NextNumbers>;

    /// Take the ownership lock `.consult.lock` fail-fast (one attempt); errors if another
    /// handle holds it (`Enter-TaskLock`).
    fn take_task_lock(&self, task: &TaskSlug, record: &LockRecord) -> io::Result<TaskLock>;

    /// Take the commit lock `.consult.write.lock`, backing off up to 60 s and reporting the
    /// measured wait (`Enter-WriteLock`).
    fn take_write_lock(&self, task: &TaskSlug) -> io::Result<WriteLock>;

    /// Write the byte-for-byte raw reply (`.reply.json`), the first write of the commit
    /// order and the only one *before* the write lock (`WriteStep::RawReply`).
    fn write_raw_reply(&self, pending: &PendingRef, bytes: &[u8]) -> io::Result<PathBuf>;

    /// Write (reserve/update) the recovery record for `pending` before the write lock is
    /// held (the reservation happens under the ownership lock, not the write lock).
    fn write_pending(&self, pending: &PendingRef, record: &PendingRecord) -> io::Result<()>;

    /// Update the recovery record under the write lock.
    fn update_pending(
        &self,
        lock: &WriteLock,
        pending: &PendingRef,
        record: &PendingRecord,
    ) -> io::Result<()>;

    /// Remove the recovery record under the write lock (only when the run truly no longer
    /// needs it).
    fn remove_pending(&self, lock: &WriteLock, pending: &PendingRef) -> io::Result<()>;

    /// A findings-only transaction (status/rating changes with no ledger entry, B): re-read
    /// `findings.json` under the lock, apply the delta, write it back atomically.
    fn update_findings(
        &self,
        lock: &WriteLock,
        task: &TaskSlug,
        delta: &FindingsDelta,
    ) -> Result<(), StoreError>;

    /// Commit a consultation under the write lock, following [`COMMIT_WRITE_ORDER`].
    fn commit(&self, lock: &WriteLock, req: CommitRequest) -> Result<CommitReceipt, StoreError>;
}

/// The files-on-disk store. `collab` is the collaboration root (default `.collab`).
#[derive(Debug, Clone)]
pub struct FilesStore {
    pub collab: PathBuf,
}

impl FilesStore {
    pub fn new(collab: impl Into<PathBuf>) -> Self {
        FilesStore {
            collab: collab.into(),
        }
    }

    fn assert_lock_task(&self, lock: &WriteLock, task: &TaskSlug) -> Result<(), StoreError> {
        if lock.task() != task {
            return Err(StoreError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("write lock is for task '{}', not '{}'", lock.task(), task),
            )));
        }
        Ok(())
    }

    fn pending_path(&self, pending: &PendingRef) -> PathBuf {
        self.task_dir(&pending.task).join(pending.file_name())
    }
}

/// Atomic replace: write to a temp file in the same directory, flush, rename over `path`.
/// `std::fs::rename` replaces an existing destination on both Windows and Unix. Mirrors
/// `Write-TextAtomic`.
pub fn write_text_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("store");
    let tmp = dir.join(format!(".{name}.{}.tmp", uuid_like()));
    {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
        f.sync_all()?;
    }
    // (the plugin's Write-TextAtomic) A reader holding the destination open without
    // FILE_SHARE_DELETE makes the rename fail with a sharing violation / access denied; it is gone
    // a moment later - up to 8 attempts, 250 ms apart. On the final failure (e.g. the destination is
    // a directory a test created to block the copy, F04-11), remove the temp so no
    // `.<name>.<uuid>.tmp` is left behind.
    let mut last: Option<io::Error> = None;
    for attempt in 1..=WRITE_ATOMIC_ATTEMPTS {
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                if attempt < WRITE_ATOMIC_ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
        }
    }
    let _ = fs::remove_file(&tmp);
    Err(last.unwrap_or_else(|| io::Error::other("the rename failed")))
}

/// `Write-TextAtomic`'s rename attempts (250 ms apart).
const WRITE_ATOMIC_ATTEMPTS: u32 = 8;

/// A cheap unique-ish suffix for temp file names (not a real UUID; only needs to avoid a
/// collision with a concurrent writer in the same directory).
fn uuid_like() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}{:x}{c:x}", std::process::id())
}

fn read_opt(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn map_err(e: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

/// `Read-JsonStore`'s refusal wording for an unusable sessions/findings store: an empty or
/// unparseable file is refused (never replaced) with the path and the one-line reason. The
/// consult run reads these through `next_numbers`, so the refusal lands before any lock or
/// pending record is written.
fn store_parse_err(path: &Path, bytes: &[u8], e: &serde_json::Error) -> io::Error {
    let restore = "An existing store is never replaced by a new one - restore it (e.g. from git) or move it aside deliberately, then retry.";
    let msg = if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        format!(
            "refusing to use '{}': it is empty or could not be read. {restore}",
            path.display()
        )
    } else {
        format!(
            "refusing to use '{}': it does not parse: {}. {restore}",
            path.display(),
            crate::one_line(&e.to_string())
        )
    };
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

/// `Read-PendingFile`'s refusal wording for an unusable recovery record, so the consult run's
/// `recover_pending` refuses an unparseable `.consult.pending.json` with the plugin's text
/// instead of a bare serde message.
fn pending_parse_err(path: &Path, e: &serde_json::Error) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "the recovery record '{}' is unusable: it does not parse: {}. It describes an interrupted consultation - inspect it (and any codex process it may name), then repair or delete it deliberately.",
            path.display(),
            crate::one_line(&e.to_string())
        ),
    )
}

// --------------------------------------------------------------------------- platform locks

/// Try to open `path` with the plugin's exclusive share mode. `Ok(Some(file))` when the
/// lock was acquired, `Ok(None)` when another handle already holds it (a sharing/lock
/// violation), `Err` for any other I/O error.
#[cfg(windows)]
fn try_open_exclusive(path: &Path) -> io::Result<Option<File>> {
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_SHARE_READ = 0x00000001: other handles may open for read but not write, so a
    // second ReadWrite open hits a sharing violation - exactly what the plugin relies on.
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(FILE_SHARE_READ)
        .open(path)
    {
        Ok(f) => Ok(Some(f)),
        Err(e) => match e.raw_os_error() {
            // ERROR_SHARING_VIOLATION (32) / ERROR_LOCK_VIOLATION (33): held elsewhere.
            Some(32) | Some(33) => Ok(None),
            _ => Err(e),
        },
    }
}

#[cfg(unix)]
fn try_open_exclusive(path: &Path) -> io::Result<Option<File>> {
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    // There is no share mode on Unix; take a non-blocking exclusive advisory lock instead
    // (std's `File::try_lock`, `flock(LOCK_EX | LOCK_NB)` on Linux). A separate open in the
    // same or another process gets its own file description, so its try-lock fails while this
    // one is held - the exclusion the plugin's FileShare.None gives on Windows. The lock goes
    // with the handle: dropped, closed or killed, it is released.
    match f.try_lock() {
        Ok(()) => Ok(Some(f)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

fn write_lock_record(file: &mut File, record: &LockRecord) -> io::Result<()> {
    let bytes = record.to_bytes().map_err(map_err)?;
    file.set_len(0)?;
    use std::io::Seek;
    file.seek(io::SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.flush()?;
    Ok(())
}

/// The write-lock wait ceiling in seconds (`Get-WriteLockTimeout`): the default, or the
/// `CODEX_CONSULT_TEST_WRITE_LOCK_SEC` test override. The "commit blocked" message quotes it.
pub fn write_lock_timeout_secs() -> u64 {
    crate::test_hooks::hook("CODEX_CONSULT_TEST_WRITE_LOCK_SEC")
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(WRITE_LOCK_TIMEOUT_SEC)
}

fn write_lock_timeout() -> Duration {
    Duration::from_secs(write_lock_timeout_secs())
}

impl EvidenceStore for FilesStore {
    fn task_dir(&self, task: &TaskSlug) -> PathBuf {
        self.collab.join(task.as_str())
    }

    fn read_sessions(&self, task: &TaskSlug) -> io::Result<Option<SessionsFile>> {
        let path = self.task_dir(task).join("sessions.json");
        match read_opt(&path)? {
            Some(b) => Ok(Some(
                SessionsFile::read(&b).map_err(|e| store_parse_err(&path, &b, &e))?,
            )),
            None => Ok(None),
        }
    }

    fn read_findings(&self, task: &TaskSlug) -> io::Result<Option<FindingsFile>> {
        let path = self.task_dir(task).join("findings.json");
        match read_opt(&path)? {
            Some(b) => Ok(Some(
                FindingsFile::read(&b).map_err(|e| store_parse_err(&path, &b, &e))?,
            )),
            None => Ok(None),
        }
    }

    fn recover_pending(&self, task: &TaskSlug) -> io::Result<Vec<RecoveredPending>> {
        let dir = self.task_dir(task);
        let mut single: Option<RecoveredPending> = None;
        let mut members: Vec<(String, RecoveredPending)> = Vec::new();
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        for ent in entries {
            let ent = ent?;
            let name = ent.file_name().to_string_lossy().to_string();
            let is_single = name.eq_ignore_ascii_case(PENDING);
            let is_member = name.starts_with(".consult.pending-") && name.ends_with(".json");
            if !is_single && !is_member {
                continue;
            }
            let bytes = fs::read(ent.path())?;
            let record: PendingRecord =
                serde_json::from_slice(&bytes).map_err(|e| pending_parse_err(&ent.path(), &e))?;
            let rp = RecoveredPending {
                record,
                path: ent.path(),
                nn: if is_single {
                    None
                } else {
                    member_nn_from_name(&name)
                },
            };
            if is_single {
                single = Some(rp);
            } else {
                members.push((name, rp));
            }
        }
        members.sort_by_key(|(name, _)| name.to_lowercase());
        let mut out = Vec::new();
        if let Some(s) = single {
            out.push(s);
        }
        out.extend(members.into_iter().map(|(_, r)| r));
        Ok(out)
    }

    fn next_numbers(&self, task: &TaskSlug) -> io::Result<NextNumbers> {
        let dir = self.task_dir(task);
        let sessions = self.read_sessions(task)?;
        let findings = self.read_findings(task)?;
        let pending = self.recover_pending(task)?;

        // N: max(count, ledger n, source.consult, reviewer_checks.consult, leftover n) + 1.
        let mut max_n: i64 = sessions
            .as_ref()
            .map(|s| s.codex.consults.len() as i64)
            .unwrap_or(0);
        if let Some(s) = &sessions {
            for e in &s.codex.consults {
                max_n = max_n.max(e.n);
            }
        }
        if let Some(f) = &findings {
            for fd in &f.findings {
                max_n = max_n.max(fd.source.consult);
                for rc in &fd.reviewer_checks {
                    max_n = max_n.max(rc.consult);
                }
            }
        }
        for rp in &pending {
            max_n = max_n.max(rp.record.n);
        }

        // NN: max(handoff filenames, finding ids F<NN>, leftover nn) + 1.
        let mut max_nn: u32 = 0;
        if let Ok(entries) = fs::read_dir(dir.join("handoffs")) {
            for ent in entries.flatten() {
                // Only FILES count as handoffs (`Get-NextNumbers`: `Get-ChildItem -File`): a
                // directory that happens to carry a leading number — e.g. a blocked `.reply.json`
                // path a test created to fail the copy (F04-11) — is not a handoff and must not
                // bump the numbering.
                if !ent.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let name = ent.file_name().to_string_lossy().to_string();
                if let Some(nn) = leading_number(&name) {
                    max_nn = max_nn.max(nn);
                }
            }
        }
        if let Some(f) = &findings {
            for fd in &f.findings {
                if let Some(nn) = finding_nn(&fd.id) {
                    max_nn = max_nn.max(nn);
                }
            }
        }
        for rp in &pending {
            if let Ok(nn) = rp.record.nn.parse::<u32>() {
                max_nn = max_nn.max(nn);
            }
        }

        Ok(NextNumbers {
            n: max_n + 1,
            nn: max_nn + 1,
        })
    }

    fn take_task_lock(&self, task: &TaskSlug, record: &LockRecord) -> io::Result<TaskLock> {
        let dir = self.task_dir(task);
        fs::create_dir_all(&dir)?;
        let path = dir.join(OWNERSHIP_LOCK);
        match try_open_exclusive(&path)? {
            Some(mut file) => {
                write_lock_record(&mut file, record)?;
                Ok(TaskLock { path, _file: file })
            }
            None => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                format!(
                    "another handle holds the task lock {} (task '{}')",
                    path.display(),
                    task
                ),
            )),
        }
    }

    fn take_write_lock(&self, task: &TaskSlug) -> io::Result<WriteLock> {
        let dir = self.task_dir(task);
        fs::create_dir_all(&dir)?;
        let path = dir.join(WRITE_LOCK);
        let timeout = write_lock_timeout();
        let started = Instant::now();
        let mut delay = Duration::from_millis(50);
        let mut slept = false;
        loop {
            match try_open_exclusive(&path)? {
                Some(mut file) => {
                    let record = LockRecord::now(task, None);
                    write_lock_record(&mut file, &record)?;
                    // The plugin reports 0 ms when the first attempt succeeded (it never
                    // slept), else the measured elapsed (Enter-WriteLock's WaitedMs).
                    let wait_ms = if slept {
                        started.elapsed().as_millis() as u64
                    } else {
                        0
                    };
                    return Ok(WriteLock {
                        path,
                        task: task.clone(),
                        wait_ms,
                        _file: file,
                    });
                }
                None => {
                    if started.elapsed() >= timeout {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!(
                                "the write lock {} was held for more than {} s",
                                path.display(),
                                timeout.as_secs()
                            ),
                        ));
                    }
                    std::thread::sleep(delay);
                    slept = true;
                    delay = (delay * 2).min(Duration::from_millis(1000));
                }
            }
        }
    }

    fn write_raw_reply(&self, pending: &PendingRef, bytes: &[u8]) -> io::Result<PathBuf> {
        let dir = self.task_dir(&pending.task);
        fs::create_dir_all(&dir)?;
        // The raw reply sits beside the recovery record, keyed the same way.
        let name = match pending.nn {
            None => ".consult.reply.json".to_string(),
            Some(nn) => format!(".consult.reply-{nn:02}.json"),
        };
        let path = dir.join(name);
        write_text_atomic(&path, bytes)?;
        Ok(path)
    }

    fn write_pending(&self, pending: &PendingRef, record: &PendingRecord) -> io::Result<()> {
        let dir = self.task_dir(&pending.task);
        fs::create_dir_all(&dir)?;
        write_text_atomic(
            &self.pending_path(pending),
            &record.to_bytes().map_err(map_err)?,
        )
    }

    fn update_pending(
        &self,
        lock: &WriteLock,
        pending: &PendingRef,
        record: &PendingRecord,
    ) -> io::Result<()> {
        if lock.task() != &pending.task {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "write lock does not match the pending record's task",
            ));
        }
        write_text_atomic(
            &self.pending_path(pending),
            &record.to_bytes().map_err(map_err)?,
        )
    }

    fn remove_pending(&self, lock: &WriteLock, pending: &PendingRef) -> io::Result<()> {
        if lock.task() != &pending.task {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "write lock does not match the pending record's task",
            ));
        }
        let path = self.pending_path(pending);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn update_findings(
        &self,
        lock: &WriteLock,
        task: &TaskSlug,
        delta: &FindingsDelta,
    ) -> Result<(), StoreError> {
        self.assert_lock_task(lock, task)?;
        let dir = self.task_dir(task);
        fs::create_dir_all(&dir)?;
        let mut findings = self.read_findings(task)?.unwrap_or_else(|| FindingsFile {
            task_id: task.as_str().to_string(),
            ..Default::default()
        });
        apply_findings_delta(&mut findings, delta)?;
        write_text_atomic(
            &dir.join("findings.json"),
            &findings.to_bytes().map_err(map_err)?,
        )?;
        Ok(())
    }

    fn commit(&self, lock: &WriteLock, req: CommitRequest) -> Result<CommitReceipt, StoreError> {
        let task = &req.pending.task;
        self.assert_lock_task(lock, task)?;
        let dir = self.task_dir(task);
        fs::create_dir_all(&dir)?;

        // ReReadStores: start from the fresh on-disk stores, apply this run's delta.
        let mut sessions = self.read_sessions(task)?.unwrap_or_else(|| SessionsFile {
            task_id: task.as_str().to_string(),
            cwd: req.bootstrap_cwd.clone(),
            codex: LedgerBook {
                tool: req.bootstrap_tool.clone(),
                consults: Vec::new(),
                extra: Map::new(),
            },
            extra: Map::new(),
        });
        sessions
            .add_entry(req.entry)
            .map_err(StoreError::AddEntry)?;

        let mut findings = self.read_findings(task)?;
        if !req.findings.is_empty() {
            let f = findings.get_or_insert_with(|| FindingsFile {
                task_id: task.as_str().to_string(),
                ..Default::default()
            });
            apply_findings_delta(f, &req.findings)?;
        }

        // HandoffMarkdown (and any other per-consultation files), each contained.
        for (rel, bytes) in req.files {
            let path = task_slug::contained_join(&dir, rel).map_err(StoreError::Path)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            write_text_atomic(&path, bytes)?;
        }

        // Findings, then Sessions (the commit point).
        if let Some(f) = &findings {
            write_text_atomic(&dir.join("findings.json"), &f.to_bytes().map_err(map_err)?)?;
        }
        // A pause between findings.json and sessions.json: the ORPHAN window (a kill here leaves
        // the finding written with no ledger entry and the record in `committing`) and the
        // write-lock contention window (this run holds the lock while others wait).
        if req.commit_pause_ms > 0 {
            std::thread::sleep(Duration::from_millis(req.commit_pause_ms));
        }
        write_text_atomic(
            &dir.join("sessions.json"),
            &sessions.to_bytes().map_err(map_err)?,
        )?;

        // RemoveRecoveryRecord: past the commit point, a cleanup failure is a warning, not
        // an error (F02-5). A launching/survivors record is only removed on Remove (F02-4).
        let mut cleanup_warning = None;
        if req.disposition == RecoveryDisposition::Remove {
            let path = self.pending_path(req.pending);
            if let Err(e) = fs::remove_file(&path) {
                if e.kind() != io::ErrorKind::NotFound {
                    cleanup_warning = Some(format!(
                        "sessions.json committed, but the recovery record {} could not be removed: {e}",
                        path.display()
                    ));
                }
            }
        }

        Ok(CommitReceipt {
            committed: true,
            cleanup_warning,
            wait_ms: lock.wait_ms(),
        })
    }
}

/// Apply a [`FindingsDelta`] to a re-read [`FindingsFile`] (B).
fn apply_findings_delta(f: &mut FindingsFile, delta: &FindingsDelta) -> Result<(), StoreError> {
    // Insert this run's findings in id order (`Add-ReplyFindings`, wave 21): after every finding
    // whose `F<NN>` is not greater than this run's NN — so a panel member that commits first still
    // lands after the lower-NN members' findings, and a single run (the highest NN) appends as
    // before. This run's findings all share one NN, kept in reply order within the block.
    if !delta.new.is_empty() {
        let mine = finding_nn(&delta.new[0].id).unwrap_or(0);
        let mut at = f.findings.len();
        for i in (0..f.findings.len()).rev() {
            match finding_nn(&f.findings[i].id) {
                Some(v) if v > mine => at = i,
                _ => break,
            }
        }
        for (k, nf) in delta.new.iter().enumerate() {
            f.findings.insert(at + k, nf.clone());
        }
    }
    for sc in &delta.status_changes {
        let target = f
            .findings
            .iter_mut()
            .find(|fd| fd.id == sc.id)
            .ok_or_else(|| StoreError::UnknownFinding(sc.id.clone()))?;
        target
            .set_status(sc.to.clone(), &sc.note, &sc.evidence, &sc.when, &sc.by)
            .map_err(|e| StoreError::Transition(sc.id.clone(), e))?;
    }
    if !delta.ratings.is_empty() {
        let ratings = f.ratings.get_or_insert_with(Vec::new);
        for r in &delta.ratings {
            ratings.push(r.clone());
        }
    }
    // A later reply's report on a prior finding (`reviewer_checks[]`); an unknown id is dropped.
    for (id, rc) in &delta.reviewer_checks {
        if let Some(target) = f.findings.iter_mut().find(|fd| &fd.id == id) {
            target.reviewer_checks.push(rc.clone());
        }
    }
    // A new finding supersedes an old one: the old finding gains the new id (deduplicated).
    for (old, new_id) in &delta.superseded_by {
        if let Some(target) = f.findings.iter_mut().find(|fd| &fd.id == old) {
            if !target.superseded_by.iter().any(|x| x == new_id) {
                target.superseded_by.push(new_id.clone());
            }
        }
    }
    Ok(())
}

fn member_nn_from_name(name: &str) -> Option<u32> {
    let rest = name.strip_prefix(".consult.pending-")?;
    let digits = rest.strip_suffix(".json")?;
    digits.parse().ok()
}

fn leading_number(name: &str) -> Option<u32> {
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 2 && name[digits.len()..].starts_with('-') {
        digits.parse().ok()
    } else {
        None
    }
}

fn finding_nn(id: &str) -> Option<u32> {
    let rest = id.strip_prefix('F')?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // (wave 2b, muse TREE) a reader holding the destination open without FILE_SHARE_DELETE makes
    // the rename fail at first; `write_text_atomic` retries (8 x 250 ms, the plugin's
    // Write-TextAtomic) and succeeds once the reader is gone.
    #[cfg(windows)]
    #[test]
    fn atomic_write_waits_for_a_reader_that_blocks_the_rename() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir =
            std::env::temp_dir().join(format!("c3-atomic-{}-{}", std::process::id(), uuid_like()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("record.json");
        fs::write(&path, b"old").unwrap();
        // FILE_SHARE_READ | FILE_SHARE_WRITE, no FILE_SHARE_DELETE: a rename over it fails
        let reader = OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(&path)
            .unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(600));
            drop(reader);
        });
        let started = std::time::Instant::now();
        write_text_atomic(&path, b"new").expect("the write waits for the reader");
        assert!(started.elapsed() >= std::time::Duration::from_millis(200));
        releaser.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        // no temp file is left behind
        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(left.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_order_is_the_documented_sequence() {
        assert_eq!(COMMIT_WRITE_ORDER[0], WriteStep::RawReply);
        assert_eq!(COMMIT_WRITE_ORDER[1], WriteStep::TakeWriteLock);
        let s = COMMIT_WRITE_ORDER
            .iter()
            .position(|x| *x == WriteStep::Sessions)
            .unwrap();
        let f = COMMIT_WRITE_ORDER
            .iter()
            .position(|x| *x == WriteStep::Findings)
            .unwrap();
        assert!(f < s, "findings.json is written before sessions.json");
        assert_eq!(
            *COMMIT_WRITE_ORDER.last().unwrap(),
            WriteStep::ReleaseWriteLock
        );
    }

    #[test]
    fn leading_and_finding_numbers() {
        assert_eq!(leading_number("15-codex-x.md"), Some(15));
        assert_eq!(leading_number("100-x.md"), Some(100));
        assert_eq!(leading_number("9-x.md"), None); // needs two+ digits
        assert_eq!(finding_nn("F15-4"), Some(15));
        assert_eq!(finding_nn("F02-1"), Some(2));
    }

    #[test]
    fn pending_ref_file_names() {
        let t = TaskSlug::new("demo").unwrap();
        assert_eq!(
            PendingRef::single(t.clone()).file_name(),
            ".consult.pending.json"
        );
        assert_eq!(
            PendingRef::member(t, 7).file_name(),
            ".consult.pending-07.json"
        );
        assert_eq!(member_nn_from_name(".consult.pending-07.json"), Some(7));
    }

    #[test]
    fn pending_state_is_tolerant() {
        assert_eq!(PendingState::from_token("running"), PendingState::Running);
        let o = PendingState::from_token("paused");
        assert_eq!(o, PendingState::Other("paused".into()));
        assert!(PendingState::Survivors.protects_child());
        assert!(!PendingState::Running.protects_child());
    }

    // (wave 28e, E1 / E23) a record carries `unverified: []` right after `survivors` (the plugin's
    // New-PendingRecord order); `kill_unconfirmed` only when a kill named no pid.
    #[test]
    fn pending_record_carries_unverified_after_survivors() {
        let mut r = PendingRecord {
            state: PendingState::Survivors,
            ..Default::default()
        };
        let text = String::from_utf8(r.to_bytes().unwrap()).unwrap();
        let s = text.find("\"survivors\"").unwrap();
        let u = text.find("\"unverified\"").unwrap();
        let n = text.find("\"note\"").unwrap();
        assert!(s < u && u < n, "{text}");
        assert!(!text.contains("kill_unconfirmed"), "{text}");
        r.kill_unconfirmed = Some("denied".into());
        r.unverified =
            vec![serde_json::json!({ "pid": 7, "why": "start time of pid 7 unreadable" })];
        let back: PendingRecord = serde_json::from_slice(&r.to_bytes().unwrap()).unwrap();
        assert_eq!(back.kill_unconfirmed.as_deref(), Some("denied"));
        assert_eq!(back.unverified.len(), 1);
        // an older record without the keys reads with empty ones
        let old: PendingRecord =
            serde_json::from_str(r#"{"state":"survivors","survivors":[],"note":""}"#).unwrap();
        assert!(old.unverified.is_empty() && old.kill_unconfirmed.is_none());
    }
}
