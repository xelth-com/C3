//! The telemetry outbox (wave 2b, F02-1): a synchronised NDJSON spool and its sender.
//!
//! Files under the telemetry directory (`<codex home>/c3/telemetry/`, C3's own - see
//! `docs/port/wave2-telemetry.md` for why C3 does not share the plugin's `telemetry-spool/`):
//!
//! - `spool.ndjson` - one line per queued item, the plugin's spool line
//!   `{"v":1,"kind":"event","queued_unix":<s>,"body":"<the event JSON as a string>"}` (the body
//!   travels as a string, so the sender posts exactly the bytes that were built). A line C3 wrote
//!   before wave 2 (the raw event) is still read and sent.
//! - `spool.lock` - THE spool lock: an OS file lock (`LockFileEx` / `flock`), released by the OS
//!   when its holder dies (never stale). Every append holds it, and so do the sender's READ and
//!   its REWRITE and the local deletion of `forget`. A producer waits for it at most its `wait`.
//! - `flush.lock` - the sender lock (the same kind of lock), held for one whole flush: a second
//!   sender that finds it busy skips (no event is posted twice by two senders at once).
//! - `last-flush.json` - what the last flush did (`c3 telemetry --status` shows it): (wave 3b) the
//!   plugin's `.last` shape `{time, result, delivered, kept, dropped, rejected, http,
//!   not_spooled_seen, not_spooled_folded, notes}` - a flush that held the sender lock folds the
//!   not-spooled files of gone producers into one of its notes (`notspooled`, E2/E20/E24/E26).
//! - `telemetry-not-spooled-<pid>-<start ticks>.ndjson` - one per producer process: the events it
//!   could not spool (`notspooled`); `telemetry-forgetting` - the marker a local deletion holds (E3).
//!
//! THE RULE (F02-1): the sender reads a snapshot under the spool lock, releases it, posts, and
//! then - under the spool lock again - re-reads the CURRENT file and removes exactly the lines it
//! delivered or dropped (a multiset of exact lines, one occurrence each), writing the remainder
//! atomically (a temporary file renamed over the spool). A line appended while the sender
//! posted is not in that multiset and stays. Crash-safe: before the rename the old file is
//! intact (a crash re-sends what was delivered - at worst a duplicate, never a loss); a torn
//! append (a crash mid-write) leaves a line without its newline, which the next append ends
//! first and the sender drops as unreadable. (wave 2d, F09-4) The rewrite needs the CURRENT
//! contents: a reread that fails replaces nothing (the flush reports the error and the spool keeps
//! its bytes - the delivered lines go again next time); a spool that is gone is not re-created.
//!
//! (wave 2d) The deletion transaction (`forget-pending.json`, `complaint`): the sender decides on it
//! UNDER the sender lock and the spool lock (F09-2) - every operation that sets or clears it holds
//! both - and posts nothing while one exists; a confirmed one whose local cleanup did not finish is
//! finished by the sender instead of a send (F09-3). Every event is closed through the classes
//! before it leaves (F09-5, `classes::close_event_body`): a line queued before wave 2 carries the
//! roster's labels as typed and leaves with `other` in their place; one that cannot be closed is
//! discarded with a local diagnostic.
//!
//! Never in a consultation's critical path: [`flush_in_background`] runs the sender in a thread
//! joined with a cap; one POST of at most 100 events, a 3 s budget, no retry within a run (a
//! 403/429/5xx keeps the spool for the next run); lines queued more than 7 days ago are dropped.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::telemetry::classes;
use crate::telemetry::complaint::{deletion_state, run_local_cleanup, DeletionState};
use crate::telemetry::event::Event;
use crate::telemetry::notspooled::{self, LocalPaths};
use crate::telemetry::{debug_log, default_hub, telemetry_dir, Error, Result};

/// Maximum events per POST (README: `≤ 100`).
const MAX_BATCH: usize = 100;
/// Drop spooled events queued longer ago than this (README: 7 days).
const MAX_AGE_SECS: i64 = 7 * 86_400;
/// The 3 s background-send budget (README rule 2).
const SEND_TIMEOUT: Duration = Duration::from_secs(3);
/// How long the sender waits for the spool lock for its read and its rewrite.
const SENDER_LOCK_WAIT: Duration = Duration::from_secs(2);
/// The spool file, the spool lock, the sender lock and the last flush's record.
pub(crate) const SPOOL_FILE: &str = "spool.ndjson";
pub(crate) const SPOOL_LOCK: &str = "spool.lock";
pub(crate) const FLUSH_LOCK: &str = "flush.lock";
pub(crate) const LAST_FLUSH: &str = "last-flush.json";
/// A deletion of this instance waiting for the intake's confirmation (`complaint::forget`):
/// while it exists nothing is spooled or sent.
pub(crate) const FORGET_PENDING: &str = "forget-pending.json";

/// An exclusive OS lock on a lock file, released when dropped (or when the process dies).
pub(crate) struct FileLock {
    _file: File,
}

/// Take the exclusive lock on `path` (created when absent, never truncated), waiting at most
/// `wait`; `Ok(None)` when it stayed busy.
pub(crate) fn lock_within(path: &Path, wait: Duration) -> std::io::Result<Option<FileLock>> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Some(FileLock { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => {
                if started.elapsed() >= wait {
                    return Ok(None);
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(e),
        }
    }
}

/// How a [`lock_within_checked`] wait ended.
pub(crate) enum LockWait {
    /// The lock is held.
    Got(FileLock),
    /// It stayed busy for the whole wait.
    Busy,
    /// The check refused while waiting (its why).
    Refused(String),
}

/// [`lock_within`] that asks `check` before every attempt: a `Some(why)` ends the wait at once (a
/// producer meets the marker of a living forget - the plugin's `Enter-TelemetryLock` loop).
pub(crate) fn lock_within_checked(
    path: &Path,
    wait: Duration,
    check: &dyn Fn() -> Option<String>,
) -> std::io::Result<LockWait> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let started = Instant::now();
    loop {
        if let Some(why) = check() {
            return Ok(LockWait::Refused(why));
        }
        match file.try_lock() {
            Ok(()) => return Ok(LockWait::Got(FileLock { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => {
                if started.elapsed() >= wait {
                    return Ok(LockWait::Busy);
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(e),
        }
    }
}

/// The outbox at a directory, posting to a hub base URL (`.../T`; empty = the configured intake
/// is refused, nothing is sent).
pub struct Spool {
    dir: PathBuf,
    hub: String,
    /// (wave 3b) Where the not-spooled files, the forgetting marker and the last flush's record are.
    paths: LocalPaths,
    /// The HTTP status of the last POST (for the record's `http`).
    http: Mutex<Option<u16>>,
}

/// What a [`Spool::flush`] did, for the debug log, `last-flush.json` and tests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FlushReport {
    /// Events accepted by the hub and removed from the spool.
    pub sent: usize,
    /// Lines dropped: queued more than 7 days ago, or unreadable.
    pub dropped_stale: usize,
    /// Lines left in the spool (over the batch cap, kept after a failed send, or appended while
    /// the sender posted).
    pub kept: usize,
    /// Whether a network send was attempted (false when nothing fresh remained or the flush was
    /// skipped).
    pub attempted: bool,
    /// Why the flush did nothing (`""` when it ran): another sender holds the sender lock, a
    /// deletion of this instance is pending at the intake (or its local cleanup was finished
    /// instead), the intake is refused, the spool lock stayed busy.
    pub skipped: String,
    /// (F09-5) Queued events discarded unsent: no C3 consultation or rating event, or without an
    /// instance id or a time of this client's shape (a local diagnostic says so).
    pub discarded: usize,
    /// (wave 3b) The intake's HTTP status of the POST, when one was answered.
    pub http: Option<u16>,
    /// (wave 3b, E20) The last flush's record could not be written: why - nothing was folded.
    pub last_warning: String,
}

/// The test seams of a flush ([`Spool::flush_hooked`]); the default is the real flush.
#[doc(hidden)]
#[derive(Default)]
pub struct FlushHooks<'a> {
    /// Runs right after the early, unlocked deletion check - before the sender lock (F09-2's
    /// interleaving: a forget completes while the sender waits here).
    pub after_check: Option<&'a dyn Fn()>,
    /// Reads the CURRENT spool for the rewrite after a send (F09-4: a failing reread).
    pub reread: Option<&'a Reread>,
}

/// How the sender reads the current spool for its rewrite ([`FlushHooks::reread`]).
#[doc(hidden)]
pub type Reread = dyn Fn(&Path) -> std::io::Result<String>;

/// One spool line parsed.
struct SpoolLine {
    kind: String,
    queued: i64,
    body: String,
}

impl Spool {
    /// A spool at `dir` posting to `hub` (the T base, e.g. `https://xelth.com/T`).
    pub fn new(dir: impl Into<PathBuf>, hub: impl Into<String>) -> Self {
        let dir = dir.into();
        let paths = notspooled::local_paths(&dir);
        Spool::with_paths(dir, hub, paths)
    }

    /// A spool whose not-spooled files, marker and last flush's record are at `paths`.
    pub fn with_paths(dir: impl Into<PathBuf>, hub: impl Into<String>, paths: LocalPaths) -> Self {
        Spool {
            dir: dir.into(),
            hub: hub.into(),
            paths,
            http: Mutex::new(None),
        }
    }

    /// Where the not-spooled files, the forgetting marker and the last flush's record are.
    pub fn local_paths(&self) -> &LocalPaths {
        &self.paths
    }

    fn path(&self) -> PathBuf {
        self.dir.join(SPOOL_FILE)
    }

    /// The number of items currently queued (a read without the lock: a count, not a decision).
    pub fn pending(&self) -> usize {
        fs::read_to_string(self.path())
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
    }

    /// Append one consultation event.
    pub fn enqueue(&self, event: &Event) -> Result<()> {
        self.enqueue_line_within(event, Duration::from_secs(5))
    }

    /// Append any serializable allowlisted event with the default 5 s wait.
    pub fn enqueue_line<T: serde::Serialize>(&self, event: &T) -> Result<()> {
        self.enqueue_line_within(event, Duration::from_secs(5))
    }

    /// Append any serializable allowlisted event as ONE spool line, under the spool lock, waiting
    /// at most `wait` for it. `Err(why)` when it was not spooled.
    pub fn enqueue_line_within<T: serde::Serialize>(
        &self,
        event: &T,
        wait: Duration,
    ) -> Result<()> {
        let body = serde_json::to_string(event)?;
        self.enqueue_built(wait, || Ok(body))
    }

    /// Append ONE spool line whose body `build` makes UNDER the spool lock (a producer computes the
    /// instance id there, so a forget's local deletion - which holds the same lock - is never
    /// interleaved with it), waiting at most `wait` for the lock. Refused while a deletion of this
    /// instance is pending at the intake (`forget-pending.json`): nothing is spooled then.
    pub fn enqueue_built(
        &self,
        wait: Duration,
        build: impl FnOnce() -> Result<String>,
    ) -> Result<()> {
        let lock_path = self.dir.join(SPOOL_LOCK);
        // (wave 3b, E3) a forgetting marker whose owner lives refuses at once - that local deletion
        // holds this lock while it deletes
        let marker = self.paths.marker.clone();
        let alive_marker = move || {
            let m = notspooled::forgetting_owner(&marker);
            (m.there && m.alive).then_some(m.why)
        };
        let _lock = match lock_within_checked(&lock_path, wait, &alive_marker)? {
            LockWait::Got(l) => l,
            LockWait::Refused(why) => return Err(Error::new(why)),
            LockWait::Busy => {
                return Err(Error::new(format!(
                    "the spool lock {} stayed busy for {:.1} s",
                    lock_path.display(),
                    wait.as_secs_f64()
                )))
            }
        };
        // a deletion transaction in ANY phase (or an unreadable one) refuses: pending at the
        // intake, or confirmed with its local cleanup not finished (F09-3)
        match deletion_state(&self.dir) {
            DeletionState::None => {}
            DeletionState::Pending => {
                return Err(Error::new(format!(
                    "a deletion of this instance is pending at the intake ({}): nothing is spooled until it is confirmed - c3 forget-me retries it",
                    self.dir.join(FORGET_PENDING).display()
                )))
            }
            DeletionState::Cleanup(_) => {
                return Err(Error::new(format!(
                    "the local deletion of this instance is not finished ({}): nothing is spooled until it is - the next flush or c3 forget-me finishes it",
                    self.dir.join(FORGET_PENDING).display()
                )))
            }
        }
        // (wave 3b, E3) the marker again, under the lock: one whose owner is gone (or that names
        // none) is removed - a note in the last flush's record - and the producer goes on; one
        // whose owner lives refuses
        notspooled::resolve_forgetting(&self.paths).map_err(Error::new)?;
        let line = serde_json::json!({
            "v": 1,
            "kind": "event",
            "queued_unix": Utc::now().timestamp(),
            "body": build()?,
        })
        .to_string();
        let mut f = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(self.path())?;
        // a torn append (a crash mid-write) left a line without its newline: end it first, so the
        // new line stays whole (the torn one is dropped as unreadable by the sender)
        let len = f.metadata()?.len();
        let mut prefix = "";
        if len > 0 {
            let mut last = [0u8; 1];
            f.seek(SeekFrom::Start(len - 1))?;
            f.read_exact(&mut last)?;
            if last[0] != b'\n' {
                prefix = "\n";
            }
        }
        f.write_all(format!("{prefix}{line}\n").as_bytes())?;
        f.flush()?;
        Ok(())
    }

    /// Send the spool once (see the module docs for the rule): under the sender lock, a snapshot
    /// read under the spool lock; lines queued more than 7 days ago and unreadable lines dropped;
    /// every event closed through the classes (F09-5); up to 100 events posted; then - under the
    /// spool lock again - exactly the delivered and the dropped lines removed from the CURRENT file.
    /// Never retries within the run.
    pub fn flush(&self) -> Result<FlushReport> {
        self.flush_with(|body| self.post_events(body))
    }

    /// [`Spool::flush`] with an injected sender (tests hold the "network" while they append).
    pub fn flush_with(&self, post: impl Fn(&str) -> bool) -> Result<FlushReport> {
        self.flush_hooked(post, &FlushHooks::default())
    }

    /// [`Spool::flush_with`] with the test seams of [`FlushHooks`].
    #[doc(hidden)]
    pub fn flush_hooked(
        &self,
        post: impl Fn(&str) -> bool,
        hooks: &FlushHooks<'_>,
    ) -> Result<FlushReport> {
        self.flush_inner(&post, hooks, false)
    }

    /// THE flush of `c3 telemetry --flush` and of a consultation's background sender: the send of
    /// [`Spool::flush`], then - still under the sender lock - the last flush's record with the fold
    /// of the not-spooled files (wave 3b, [`Spool::record_flush`]).
    pub fn flush_recorded(&self) -> Result<FlushReport> {
        self.flush_recorded_with(|body| self.post_events(body))
    }

    /// [`Spool::flush_recorded`] with an injected sender.
    pub fn flush_recorded_with(&self, post: impl Fn(&str) -> bool) -> Result<FlushReport> {
        self.flush_inner(&post, &FlushHooks::default(), true)
    }

    fn flush_inner(
        &self,
        post: &dyn Fn(&str) -> bool,
        hooks: &FlushHooks<'_>,
        record: bool,
    ) -> Result<FlushReport> {
        let mut report = FlushReport::default();
        // a cheap early skip without any lock; NOT the decision - the deletion state is decided
        // below, under the sender lock and the spool lock (F09-2)
        if matches!(deletion_state(&self.dir), DeletionState::Pending) {
            report.skipped = "a deletion of this instance is pending at the intake".into();
            let r = Ok(report);
            if record {
                self.record_skip(&r);
            }
            return r;
        }
        if let Some(after_check) = hooks.after_check {
            after_check();
        }
        // a sender that finds the sender lock busy writes nothing: the one that holds it records
        // its own flush
        let Some(_sender) = lock_within(&self.dir.join(FLUSH_LOCK), Duration::ZERO)? else {
            report.skipped = "another flush is running (its lock is held)".into();
            return Ok(report);
        };
        let r = self.send_locked(post, hooks, report);
        if record {
            return self.record_flush(r);
        }
        r
    }

    /// The send, under the sender lock the caller holds (the rule of the module docs).
    fn send_locked(
        &self,
        post: &dyn Fn(&str) -> bool,
        hooks: &FlushHooks<'_>,
        mut report: FlushReport,
    ) -> Result<FlushReport> {
        // the snapshot, read under the spool lock
        let snapshot = {
            let Some(_lock) = lock_within(&self.dir.join(SPOOL_LOCK), SENDER_LOCK_WAIT)? else {
                report.skipped = "the spool lock stayed busy".into();
                return Ok(report);
            };
            // (F09-2) the deletion state as it is NOW, under BOTH locks: every operation that sets
            // or clears it (a forget, a cleanup) holds the same two, so it cannot change before
            // this flush releases the sender lock - a forget that failed its DELETE while this
            // flush waited is seen here, and nothing is posted
            match deletion_state(&self.dir) {
                DeletionState::None => {}
                DeletionState::Pending => {
                    report.skipped = "a deletion of this instance is pending at the intake".into();
                    return Ok(report);
                }
                DeletionState::Cleanup(txn) => {
                    // (F09-3) a confirmed deletion whose local cleanup did not finish: it is
                    // finished now, with the transaction's identity - and nothing is sent
                    let c = run_local_cleanup(&self.dir, &txn, true, &|p| fs::remove_file(p));
                    report.skipped = match &c.error {
                        None => format!(
                            "finished the local deletion of instance {} (removed {}) - nothing was sent",
                            txn.instance_id,
                            if c.removed.is_empty() { "nothing".to_string() } else { c.removed.join(", ") }
                        ),
                        Some(e) => format!(
                            "the local deletion of instance {} is not finished ({e}) - nothing was sent",
                            txn.instance_id
                        ),
                    };
                    return Ok(report);
                }
            }
            // (wave 3b, E3) the forgetting marker: one whose owner is gone is removed (a note in
            // the last flush's record); one whose owner lives stops this flush - nothing is sent
            // while a local deletion runs
            if let Err(why) = notspooled::resolve_forgetting(&self.paths) {
                report.skipped = why;
                return Ok(report);
            }
            match fs::read_to_string(self.path()) {
                Ok(t) => t,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(report),
                Err(e) => return Err(e.into()),
            }
        };
        let lines: Vec<&str> = snapshot
            .lines()
            .map(|l| l.trim_end_matches('\r'))
            .filter(|l| !l.trim().is_empty())
            .collect();
        if lines.is_empty() {
            return Ok(report);
        }
        let cutoff = Utc::now().timestamp() - MAX_AGE_SECS;
        let mut remove: Vec<&str> = Vec::new();
        let mut fresh: Vec<(&str, String)> = Vec::new();
        for l in &lines {
            match parse_line(l) {
                None => {
                    report.dropped_stale += 1;
                    remove.push(l);
                }
                Some(s) if s.queued <= cutoff => {
                    report.dropped_stale += 1;
                    remove.push(l);
                }
                // (F09-5) every event leaves through the closed classes - a line queued before
                // wave 2 carries the roster's labels as typed; one that cannot be closed is
                // discarded here, with a local diagnostic, never sent
                Some(s) if s.kind == "event" => match classes::close_event_body(&s.body) {
                    Some(body) => fresh.push((l, body)),
                    None => {
                        report.discarded += 1;
                        remove.push(l);
                    }
                },
                // another kind (a complaint line of a later wave): kept untouched
                Some(_) => {}
            }
        }
        if report.discarded > 0 {
            debug_log(&format!(
                "telemetry flush discarded {} queued event(s) that are no closable C3 event (no consultation or rating of this client, no instance id or time) - not sent",
                report.discarded
            ));
        }
        let batch: Vec<(&str, String)> = fresh.iter().take(MAX_BATCH).cloned().collect();
        if !batch.is_empty() {
            if self.hub.trim().is_empty() {
                report.skipped = "the configured intake is refused".into();
            } else {
                report.attempted = true;
                let body = format!(
                    "{{\"events\":[{}]}}",
                    batch
                        .iter()
                        .map(|(_, b)| b.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                if let Ok(mut h) = self.http.lock() {
                    *h = None;
                }
                let delivered = post(&body);
                report.http = self.http.lock().ok().and_then(|h| *h);
                if delivered {
                    report.sent = batch.len();
                    remove.extend(batch.iter().map(|(l, _)| *l));
                }
            }
        }
        if !remove.is_empty() {
            let reread: &Reread = match hooks.reread {
                Some(r) => r,
                None => &|p| fs::read_to_string(p),
            };
            self.remove_lines(&remove, reread)?;
        }
        report.kept = self.pending();
        Ok(report)
    }

    /// Remove exactly `lines` (one occurrence each) from the CURRENT spool file, under the spool
    /// lock, by an atomic replace; lines appended since the snapshot stay. (F09-4) The current
    /// file is read by `reread`; when its contents cannot be established the spool is NOT
    /// replaced (a rewrite from a guess would erase the events appended while the sender posted)
    /// and the error is returned - the delivered lines are then sent once more by the next flush
    /// (at worst a duplicate, never a loss).
    fn remove_lines(&self, lines: &[&str], reread: &Reread) -> Result<()> {
        let Some(_lock) = lock_within(&self.dir.join(SPOOL_LOCK), SENDER_LOCK_WAIT)? else {
            // not rewritten: the delivered lines are sent once more by the next flush
            return Err(Error::new("the spool lock stayed busy for the rewrite"));
        };
        let current = match reread(&self.path()) {
            Ok(t) => t,
            // the invariant: only the local deletion removes the spool, and it holds the sender
            // lock this flush holds - so a missing spool is an outside removal; nothing was
            // appended to it that a rewrite could keep, and nothing is rewritten (no file made)
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(Error::new(format!(
                    "the spool {} could not be re-read for its rewrite ({}); it is kept exactly as it is - the delivered events are sent again by the next flush",
                    self.path().display(),
                    c3_core::one_line(&e.to_string())
                )))
            }
        };
        let mut want: HashMap<&str, usize> = HashMap::new();
        for l in lines {
            *want.entry(l).or_insert(0) += 1;
        }
        let mut keep = String::new();
        for l in current.lines().map(|l| l.trim_end_matches('\r')) {
            if l.trim().is_empty() {
                continue;
            }
            if let Some(n) = want.get_mut(l) {
                if *n > 0 {
                    *n -= 1;
                    continue;
                }
            }
            keep.push_str(l);
            keep.push('\n');
        }
        replace_atomic(&self.path(), keep.as_bytes())
    }

    fn post_events(&self, body: &str) -> bool {
        if self.hub.trim().is_empty() {
            return false;
        }
        let url = format!("{}/v2/events", self.hub.trim_end_matches('/'));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(SEND_TIMEOUT)
            .timeout(SEND_TIMEOUT)
            .build();
        let answer = agent
            .post(&url)
            .set("content-type", "application/json")
            .send_string(body);
        let status = match &answer {
            Ok(r) => Some(r.status()),
            Err(ureq::Error::Status(code, _)) => Some(*code),
            Err(_) => None,
        };
        if let Ok(mut h) = self.http.lock() {
            *h = status;
        }
        match answer {
            Ok(_) => true,
            Err(e) => {
                debug_log(&format!("telemetry flush kept the spool: {e}"));
                false
            }
        }
    }

    /// (wave 3b) Record a flush and fold the not-spooled files (the plugin's end of
    /// `Invoke-TelemetryFlush`, E2/E20/E24/E26), under the sender lock the caller holds and the
    /// spool lock (every writer of the record holds it): the record's notes carried on, the files of
    /// gone producers and the legacy files counted ([`notspooled::merge`] - nothing deleted yet),
    /// the record SAVED - its note `folded <n> not-spooled line(s) of <m> gone producer(s)`, the new
    /// `not_spooled_seen` (the lines of the files kept) and `not_spooled_folded[]` ({name, bytes} of
    /// the files the fold covers) -, ONLY THEN the files deleted under the handles the fold holds,
    /// then the record written once more without the names of the files now gone. A record that
    /// cannot be saved folds nothing (`last_warning`). The spool lock busy for 1 s: nothing is folded
    /// and the record says every line (but the legacy files') was seen, as the plugin's flush
    /// without its telemetry lock.
    ///
    /// TEST HOOK (test mode only): `CODEX_CONSULT_TEST_FOLD_CRASH=1` - the process exits (87)
    /// between the save of the record and the deletes, as a crash would; `=2` - it exits (88) between
    /// the deletes and the rewrite that drops their names.
    fn record_flush(&self, r: Result<FlushReport>) -> Result<FlushReport> {
        let p = &self.paths;
        let result = result_text(&r);
        let lock = lock_within(&self.dir.join(SPOOL_LOCK), Duration::from_secs(1))
            .ok()
            .flatten();
        let before = notspooled::read_last(&p.last);
        let mut notes: Vec<String> = notspooled::notes_of(before.as_ref())
            .into_iter()
            .filter(|n| !is_stuck_note(n))
            .collect();
        let folded_before = notspooled::folded_map(before.as_ref());
        let mut fold = None;
        let (seen, folded) = if lock.is_some() {
            let f = notspooled::merge(p, &folded_before);
            if f.producers > 0 {
                notes.push(format!(
                    "{} folded {} not-spooled line(s) of {} gone producer(s)",
                    notspooled::now_iso(),
                    f.lines,
                    f.producers
                ));
            }
            for n in &f.notes {
                notes.push(format!("{} {n}", notspooled::now_iso()));
            }
            let sf = (f.seen, f.folded.clone());
            fold = Some(f);
            sf
        } else {
            // (E26) the legacy files' lines are never seen: they count until a fold stages them
            let all = notspooled::count(p, true);
            let kept: Vec<notspooled::FoldedEntry> = folded_before
                .values()
                .filter(|e| p.ns_dir.join(&e.name).exists())
                .cloned()
                .collect();
            (all.total.saturating_sub(all.legacy_lines), kept)
        };
        let (delivered, kept, dropped, http) = match &r {
            // `kept`: the spool's count now (a skipped flush did not count it)
            Ok(rep) => (
                serde_json::json!(rep.sent),
                serde_json::json!(self.pending()),
                serde_json::json!(rep.dropped_stale + rep.discarded),
                serde_json::json!(rep.http),
            ),
            Err(_) => (
                serde_json::json!(0),
                serde_json::Value::Null,
                serde_json::json!(0),
                serde_json::Value::Null,
            ),
        };
        let mut last = serde_json::json!({
            "time": notspooled::now_iso(),
            "result": result,
            "delivered": delivered,
            "kept": kept,
            "dropped": dropped,
            "rejected": [],
            "http": http,
            "not_spooled_seen": seen,
            "not_spooled_folded": notspooled::folded_value(&folded),
            "notes": notspooled::last_notes(notes),
        });
        let mut warning = String::new();
        match notspooled::write_last(&p.last, &last) {
            Err(e) => {
                warning = format!(
                    "{} could not be written ({e}) - nothing was folded: the not-spooled files and the last baseline stay, the next flush counts them",
                    p.last.display()
                );
            }
            Ok(()) => {
                if let Some(f) = fold.as_mut().filter(|f| !f.folded.is_empty()) {
                    let crash = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_FOLD_CRASH")
                        .unwrap_or_default();
                    if crash.trim() == "1" {
                        std::process::exit(87);
                    }
                    let left = f.complete(true);
                    if crash.trim() == "2" {
                        std::process::exit(88);
                    }
                    if left.len() != folded.len() {
                        last["not_spooled_folded"] = notspooled::folded_value(&left);
                        let _ = notspooled::write_last(&p.last, &last);
                    }
                }
            }
        }
        // every handle the fold still holds is closed (nothing deleted without a saved record)
        if let Some(f) = fold.as_mut() {
            let _ = f.complete(false);
        }
        drop(lock);
        match r {
            Ok(mut rep) => {
                rep.last_warning = warning;
                Ok(rep)
            }
            Err(e) if !warning.is_empty() => Err(Error::new(format!("{e}; warning: {warning}"))),
            Err(e) => Err(e),
        }
    }

    /// Record a flush that did not reach the sender lock's work (a deletion pending at the
    /// intake): its result, the fold's fields and the notes carried on; under the spool lock (1 s,
    /// else nothing is written).
    fn record_skip(&self, r: &Result<FlushReport>) {
        let p = &self.paths;
        let Some(_lock) = lock_within(&self.dir.join(SPOOL_LOCK), Duration::from_secs(1))
            .ok()
            .flatten()
        else {
            return;
        };
        let before = notspooled::read_last(&p.last);
        let field = |k: &str, d: serde_json::Value| {
            before.as_ref().and_then(|b| b.get(k)).cloned().unwrap_or(d)
        };
        let kept = match r {
            Ok(_) => serde_json::json!(self.pending()),
            Err(_) => serde_json::Value::Null,
        };
        let last = serde_json::json!({
            "time": notspooled::now_iso(),
            "result": result_text(r),
            "delivered": 0,
            "kept": kept,
            "dropped": 0,
            "rejected": [],
            "http": serde_json::Value::Null,
            "not_spooled_seen": field("not_spooled_seen", serde_json::json!(0)),
            "not_spooled_folded": field("not_spooled_folded", serde_json::json!([])),
            "notes": notspooled::notes_of(before.as_ref()),
        });
        let _ = notspooled::write_last(&p.last, &last);
    }

    /// The items queued, by kind, and the oldest queue time (unix seconds): `(events, other,
    /// unreadable, oldest)`. Reads only.
    pub fn counts(&self) -> (usize, usize, usize, Option<i64>) {
        let text = fs::read_to_string(self.path()).unwrap_or_default();
        let (mut ev, mut other, mut bad, mut oldest) = (0, 0, 0, None::<i64>);
        for l in text.lines().filter(|l| !l.trim().is_empty()) {
            match parse_line(l.trim_end_matches('\r')) {
                Some(s) => {
                    if s.kind == "event" {
                        ev += 1;
                    } else {
                        other += 1;
                    }
                    oldest = Some(oldest.map_or(s.queued, |o: i64| o.min(s.queued)));
                }
                None => bad += 1,
            }
        }
        (ev, other, bad, oldest)
    }
}

/// The result line of a flush, for the last flush's record.
fn result_text(report: &Result<FlushReport>) -> String {
    let result = match report {
        Ok(r) if !r.skipped.is_empty() => format!("skipped - {}", r.skipped),
        Ok(r) if !r.attempted && r.sent == 0 => format!(
            "nothing to send - dropped {}, kept {}",
            r.dropped_stale, r.kept
        ),
        Ok(r) if r.sent > 0 => format!(
            "delivered {} - dropped {}, kept {}",
            r.sent, r.dropped_stale, r.kept
        ),
        Ok(r) => format!(
            "not delivered - delivered 0, kept {}, dropped {}",
            r.kept, r.dropped_stale
        ),
        Err(e) => format!("failed - {e}"),
    };
    match report {
        Ok(r) if r.discarded > 0 => format!(
            "{result}; discarded {} queued event(s) that are no closable C3 event (not sent)",
            r.discarded
        ),
        _ => result,
    }
}

/// A plugin sender's "sender stuck since" note (`^\S+ sender stuck since `): a flush that holds
/// the sender lock drops it, as the plugin's does.
fn is_stuck_note(n: &str) -> bool {
    n.split_once(' ')
        .is_some_and(|(t, rest)| !t.is_empty() && rest.starts_with("sender stuck since "))
}

/// One spool line: the plugin's `{v, kind, queued_unix, body}`, or - a line C3 wrote before
/// wave 2 - the raw event (queued at its `client_time`). `None` when it is neither.
fn parse_line(line: &str) -> Option<SpoolLine> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let o = v.as_object()?;
    if let Some(body) = o.get("body") {
        let body = body.as_str().filter(|b| !b.is_empty())?.to_string();
        let kind = o.get("kind")?.as_str()?.to_string();
        let queued = o.get("queued_unix")?.as_i64()?;
        return Some(SpoolLine { kind, queued, body });
    }
    // legacy: the event itself
    let t = o.get("client_time")?.as_str()?;
    let queued = DateTime::parse_from_rfc3339(t).ok()?.timestamp();
    Some(SpoolLine {
        kind: "event".into(),
        queued,
        body: line.to_string(),
    })
}

/// Write `bytes` to a temporary file beside `path` and rename it over `path` (atomic; a reader
/// that holds the file open without delete sharing is waited for briefly).
pub(crate) fn replace_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("spool");
    let tmp = dir.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
        f.sync_all()?;
    }
    let mut last = None;
    for _ in 0..8 {
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    let _ = fs::remove_file(&tmp);
    Err(Error::new(format!(
        "{} could not be replaced ({}); it is kept as it was",
        path.display(),
        last.map(|e| e.to_string()).unwrap_or_default()
    )))
}

/// A detached background flush that the caller joins with a cap at exit.
pub struct BackgroundFlush {
    done: mpsc::Receiver<()>,
}

impl BackgroundFlush {
    /// Wait at most `cap` for the flush to finish, then return. If the send is still
    /// running it is left to time out on its own 3 s budget (the process may exit first).
    pub fn join_with_cap(self, cap: Duration) {
        let _ = self.done.recv_timeout(cap);
    }
}

/// Run the default spool's sender once, synchronously (`c3 telemetry --flush`), recording the
/// result for `--status`.
pub fn flush_now() -> Result<FlushReport> {
    // (wave 3b) the record and the fold of the not-spooled files, under the sender lock
    Spool::new(telemetry_dir(), default_hub()).flush_recorded()
}

/// Spawn a background flush of the default spool (a consultation's start, after a rating or a
/// backfill). The CALLER decides the switch (a run's own `--telemetry on` wins over the
/// environment); the priors refresh keeps its own environment gate.
pub fn flush_in_background() -> BackgroundFlush {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = flush_now();
        // The priors refresh runs AFTER the events are sent (M9 §3, F8): once a day at most,
        // injected fetcher in tests. A panic or error inside it must never affect the flush, so
        // it is isolated in catch_unwind.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::router::maybe_refresh_priors();
        }));
        let _ = tx.send(());
    });
    BackgroundFlush { done: rx }
}
