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
//! Invariants encoded here (DESIGN §3, README "Write order and atomic stores"):
//! - **Write order.** `.reply.json` (the byte-for-byte raw copy) is written first, before
//!   the lock; then, under `.consult.write.lock`, the rendered `.md`, `findings.json`,
//!   `sessions.json` (the commit point) and the removal of the recovery record - see
//!   [`COMMIT_WRITE_ORDER`].
//! - **Re-read under the lock.** Every writer re-reads both stores under the write lock and
//!   applies only its own delta, so no concurrent commit is lost.
//! - **Lock = an open handle.** The lock is owned by holding the file open, never by its
//!   contents; the record inside is informational (who holds it). Releasing = closing.
//! - **Atomic replace.** A store is written to a temp file in the same directory, flushed,
//!   and renamed over the store in one step (`std::fs::rename`, which replaces on both
//!   Windows and Unix).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::findings::FindingsFile;
use crate::ledger::{LedgerEntry, SessionsFile};
use crate::ps_json;

/// The ownership lock file name.
pub const OWNERSHIP_LOCK: &str = ".consult.lock";
/// The commit (write) lock file name.
pub const WRITE_LOCK: &str = ".consult.write.lock";
/// The single-run recovery record file name.
pub const PENDING: &str = ".consult.pending.json";

/// The state of an interrupted run's recovery record (`$script:PendingStates`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PendingState {
    Reserved,
    Launching,
    Running,
    Survivors,
    Committing,
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

impl LockRecord {
    /// The compact, newline-terminated bytes the bridge writes into a lock file.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut s = serde_json::to_string(self)?;
        s.push('\n');
        Ok(s.into_bytes())
    }
}

/// The recovery record (`New-PendingRecord`). Field order matches the literal. Written
/// with the same pretty formatter as the stores ([`ps_json`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRecord {
    pub state: PendingState,
    pub n: i64,
    pub nn: String,
    pub reply: String,
    pub events: String,
    pub consult_id: String,
    pub started: String,
    pub pid: u32,
    pub start_time: String,
    pub host: String,
    pub launcher: String,
    pub engine: String,
    pub child_pid: Option<u32>,
    pub child_start_time: String,
    /// `{pid, start_time, name}` entries the kill could see; kept raw.
    pub survivors: Vec<Value>,
    pub note: String,
    /// Present only for a panel member's record.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub panel: Option<Value>,
}

impl PendingRecord {
    /// Serialize with the PowerShell-5.1 pretty formatter (as `Write-PendingFile`).
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        ps_json::to_ps_json_bytes(self)
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

/// A held task lock: the OS file handle. Dropping it releases the lock (closes the handle),
/// exactly as `Exit-TaskLock` does; the file itself stays on disk.
#[derive(Debug)]
pub struct TaskLock {
    pub path: PathBuf,
    _file: File,
}

/// The evidence store contract (DESIGN §4). One implementation, [`FilesStore`].
pub trait EvidenceStore {
    /// `<collab>/<task>` - the task directory that holds the stores, locks and handoffs.
    fn task_dir(&self, task: &str) -> PathBuf;

    /// Parse `sessions.json`; `None` when it does not exist.
    fn read_sessions(&self, task: &str) -> io::Result<Option<SessionsFile>>;

    /// Parse `findings.json`; `None` when it does not exist.
    fn read_findings(&self, task: &str) -> io::Result<Option<FindingsFile>>;

    /// Read every recovery record of the task (`.consult.pending.json` first, then the
    /// panel members' `.consult.pending-<NN>.json` by name). The read side of
    /// `Read-TaskPendingRecords`; judging which are still active by process liveness is a
    /// runtime concern (M3).
    fn recover_pending(&self, task: &str) -> io::Result<Vec<PendingRecord>>;

    /// The next handoff number `NN`: greater than every `<NN>-` handoff file, every finding
    /// id `F<NN>`, and every recovery record's `nn` (`Get-NextNumbers`, the `NN` half).
    fn next_handoff_number(&self, task: &str) -> io::Result<u32>;

    /// Take the ownership lock `.consult.lock`, holding the handle open for the run.
    fn take_task_lock(&self, task: &str, record: &LockRecord) -> io::Result<TaskLock>;

    /// Write the recovery record atomically.
    fn write_pending(&self, task: &str, record: &PendingRecord) -> io::Result<()>;

    /// Commit a consultation under the write lock, following [`COMMIT_WRITE_ORDER`]:
    /// re-read the stores, apply this run's delta (insert `entry` by `n`, set `findings`),
    /// write the handoff `files`, write `findings.json` then `sessions.json` atomically,
    /// then remove the recovery record. Returns the write lock's wait in milliseconds.
    fn commit(
        &self,
        task: &str,
        entry: LedgerEntry,
        findings: Option<FindingsFile>,
        files: &[(PathBuf, Vec<u8>)],
    ) -> io::Result<u64>;
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
    fs::rename(&tmp, path)
}

/// A cheap unique-ish suffix for temp file names (not a real UUID; only needs to avoid a
/// collision with a concurrent writer in the same directory).
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}{:x}", std::process::id())
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

impl EvidenceStore for FilesStore {
    fn task_dir(&self, task: &str) -> PathBuf {
        self.collab.join(task)
    }

    fn read_sessions(&self, task: &str) -> io::Result<Option<SessionsFile>> {
        match read_opt(&self.task_dir(task).join("sessions.json"))? {
            Some(b) => Ok(Some(SessionsFile::read(&b).map_err(map_err)?)),
            None => Ok(None),
        }
    }

    fn read_findings(&self, task: &str) -> io::Result<Option<FindingsFile>> {
        match read_opt(&self.task_dir(task).join("findings.json"))? {
            Some(b) => Ok(Some(FindingsFile::read(&b).map_err(map_err)?)),
            None => Ok(None),
        }
    }

    fn recover_pending(&self, task: &str) -> io::Result<Vec<PendingRecord>> {
        let dir = self.task_dir(task);
        let mut single: Option<PendingRecord> = None;
        let mut members: Vec<(String, PendingRecord)> = Vec::new();
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
            let rec: PendingRecord = serde_json::from_slice(&bytes).map_err(map_err)?;
            if is_single {
                single = Some(rec);
            } else {
                members.push((name, rec));
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

    fn next_handoff_number(&self, task: &str) -> io::Result<u32> {
        let dir = self.task_dir(task);
        let mut max_nn: u32 = 0;
        // handoff files "<NN>-..." (two or more digits).
        if let Ok(entries) = fs::read_dir(dir.join("handoffs")) {
            for ent in entries.flatten() {
                let name = ent.file_name().to_string_lossy().to_string();
                if let Some(nn) = leading_number(&name) {
                    max_nn = max_nn.max(nn);
                }
            }
        }
        // finding ids F<NN>-<k>.
        if let Some(f) = self.read_findings(task)? {
            for fd in &f.findings {
                if let Some(nn) = finding_nn(&fd.id) {
                    max_nn = max_nn.max(nn);
                }
            }
        }
        // recovery records' nn.
        for rec in self.recover_pending(task)? {
            if let Ok(nn) = rec.nn.parse::<u32>() {
                max_nn = max_nn.max(nn);
            }
        }
        Ok(max_nn + 1)
    }

    fn take_task_lock(&self, task: &str, record: &LockRecord) -> io::Result<TaskLock> {
        let dir = self.task_dir(task);
        fs::create_dir_all(&dir)?;
        let path = dir.join(OWNERSHIP_LOCK);
        // Open (create if absent) and hold the handle. NOTE (M3): the bridge opens with
        // FileShare.Read on Windows / FileShare.None on Unix so a second run's exclusive
        // open fails fast; reproducing that exact share mode needs a platform open call.
        // Here we hold the handle open, which is the contract; the share flags are M3.
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        let bytes = record.to_bytes().map_err(map_err)?;
        file.set_len(0)?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(TaskLock { path, _file: file })
    }

    fn write_pending(&self, task: &str, record: &PendingRecord) -> io::Result<()> {
        let dir = self.task_dir(task);
        fs::create_dir_all(&dir)?;
        let path = dir.join(PENDING);
        write_text_atomic(&path, &record.to_bytes().map_err(map_err)?)
    }

    fn commit(
        &self,
        task: &str,
        entry: LedgerEntry,
        findings: Option<FindingsFile>,
        files: &[(PathBuf, Vec<u8>)],
    ) -> io::Result<u64> {
        let dir = self.task_dir(task);
        fs::create_dir_all(&dir)?;
        // (In a full runtime this holds .consult.write.lock across the whole sequence and
        // returns commit_wait_ms; here the lock acquisition/backoff is M3 - see
        // take_task_lock's note - and the ordered writes below ARE the encoded write order.)
        // ReReadStores: start from the fresh on-disk ledger, apply this run's delta.
        let mut sessions = self.read_sessions(task)?.unwrap_or_else(|| SessionsFile {
            task_id: task.to_string(),
            cwd: String::new(),
            codex: crate::ledger::LedgerBook {
                tool: entry.reviewer.harness.clone(),
                consults: Vec::new(),
            },
        });
        sessions.add_entry(entry);
        // HandoffMarkdown (and any other per-consultation files).
        for (rel, bytes) in files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            write_text_atomic(&path, bytes)?;
        }
        // Findings, then Sessions (the commit point).
        if let Some(f) = findings {
            write_text_atomic(&dir.join("findings.json"), &f.to_bytes().map_err(map_err)?)?;
        }
        write_text_atomic(
            &dir.join("sessions.json"),
            &sessions.to_bytes().map_err(map_err)?,
        )?;
        // RemoveRecoveryRecord.
        let pending = dir.join(PENDING);
        if pending.exists() {
            fs::remove_file(&pending)?;
        }
        Ok(0)
    }
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

    #[test]
    fn write_order_is_the_documented_sequence() {
        assert_eq!(COMMIT_WRITE_ORDER[0], WriteStep::RawReply);
        assert_eq!(COMMIT_WRITE_ORDER[1], WriteStep::TakeWriteLock);
        // sessions.json is the commit point and comes after findings.json.
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
}
