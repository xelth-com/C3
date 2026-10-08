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
//! - `last-flush.json` - what the last flush did (`c3 telemetry --status` shows it).
//!
//! THE RULE (F02-1): the sender reads a snapshot under the spool lock, releases it, posts, and
//! then - under the spool lock again - re-reads the CURRENT file and removes exactly the lines it
//! delivered or dropped (a multiset of exact lines, one occurrence each), writing the remainder
//! atomically (a temporary file renamed over the spool). A line appended while the sender
//! posted is not in that multiset and stays. Crash-safe: before the rename the old file is
//! intact (a crash re-sends what was delivered - at worst a duplicate, never a loss); a torn
//! append (a crash mid-write) leaves a line without its newline, which the next append ends
//! first and the sender drops as unreadable.
//!
//! Never in a consultation's critical path: [`flush_in_background`] runs the sender in a thread
//! joined with a cap; one POST of at most 100 events, a 3 s budget, no retry within a run (a
//! 403/429/5xx keeps the spool for the next run); lines queued more than 7 days ago are dropped.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::telemetry::event::Event;
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

/// The outbox at a directory, posting to a hub base URL (`.../T`; empty = the configured intake
/// is refused, nothing is sent).
pub struct Spool {
    dir: PathBuf,
    hub: String,
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
    /// deletion of this instance is pending, the intake is refused, the spool lock stayed busy.
    pub skipped: String,
}

/// One spool line parsed.
struct SpoolLine {
    kind: String,
    queued: i64,
    body: String,
}

impl Spool {
    /// A spool at `dir` posting to `hub` (the T base, e.g. `https://xelth.com/T`).
    pub fn new(dir: impl Into<PathBuf>, hub: impl Into<String>) -> Self {
        Spool {
            dir: dir.into(),
            hub: hub.into(),
        }
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
        let _lock = lock_within(&lock_path, wait)?.ok_or_else(|| {
            Error::new(format!(
                "the spool lock {} stayed busy for {:.1} s",
                lock_path.display(),
                wait.as_secs_f64()
            ))
        })?;
        if self.dir.join(FORGET_PENDING).exists() {
            return Err(Error::new(format!(
                "a deletion of this instance is pending at the intake ({}): nothing is spooled until it is confirmed - c3 forget-me retries it",
                self.dir.join(FORGET_PENDING).display()
            )));
        }
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
    /// up to 100 events posted; then - under the spool lock again - exactly the delivered and the
    /// dropped lines removed from the CURRENT file. Never retries within the run.
    pub fn flush(&self) -> Result<FlushReport> {
        self.flush_with(|body| self.post_events(body))
    }

    /// [`Spool::flush`] with an injected sender (tests hold the "network" while they append).
    pub fn flush_with(&self, post: impl Fn(&str) -> bool) -> Result<FlushReport> {
        let mut report = FlushReport::default();
        if self.dir.join(FORGET_PENDING).exists() {
            report.skipped = "a deletion of this instance is pending at the intake".into();
            return Ok(report);
        }
        let Some(_sender) = lock_within(&self.dir.join(FLUSH_LOCK), Duration::ZERO)? else {
            report.skipped = "another flush is running (its lock is held)".into();
            return Ok(report);
        };
        // the snapshot, read under the spool lock
        let snapshot = {
            let Some(_lock) = lock_within(&self.dir.join(SPOOL_LOCK), SENDER_LOCK_WAIT)? else {
                report.skipped = "the spool lock stayed busy".into();
                return Ok(report);
            };
            match fs::read_to_string(self.path()) {
                Ok(t) => t,
                Err(_) => return Ok(report),
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
                Some(s) if s.kind == "event" => fresh.push((l, s.body)),
                // another kind (a complaint line of a later wave): kept untouched
                Some(_) => {}
            }
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
                if post(&body) {
                    report.sent = batch.len();
                    remove.extend(batch.iter().map(|(l, _)| *l));
                }
            }
        }
        if !remove.is_empty() {
            self.remove_lines(&remove)?;
        }
        report.kept = self.pending();
        Ok(report)
    }

    /// Remove exactly `lines` (one occurrence each) from the CURRENT spool file, under the spool
    /// lock, by an atomic replace; lines appended since the snapshot stay.
    fn remove_lines(&self, lines: &[&str]) -> Result<()> {
        let Some(_lock) = lock_within(&self.dir.join(SPOOL_LOCK), SENDER_LOCK_WAIT)? else {
            // not rewritten: the delivered lines are sent once more by the next flush
            return Err(Error::new("the spool lock stayed busy for the rewrite"));
        };
        let current = fs::read_to_string(self.path()).unwrap_or_default();
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
        match agent
            .post(&url)
            .set("content-type", "application/json")
            .send_string(body)
        {
            Ok(_) => true,
            Err(e) => {
                debug_log(&format!("telemetry flush kept the spool: {e}"));
                false
            }
        }
    }

    /// Record what a flush did (`last-flush.json`), for `c3 telemetry --status`.
    fn write_last(&self, report: &Result<FlushReport>) {
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
        let v = serde_json::json!({
            "time": chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
            "result": result,
        });
        let _ = fs::create_dir_all(&self.dir);
        let _ = replace_atomic(&self.dir.join(LAST_FLUSH), v.to_string().as_bytes());
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
    let dir = telemetry_dir();
    let spool = Spool::new(&dir, default_hub());
    let r = spool.flush();
    spool.write_last(&r);
    // the events not spooled are counted since the last flush that ran (the plugin's per-producer
    // files and their `.last` fold are wave 3)
    if matches!(&r, Ok(rep) if rep.skipped.is_empty()) {
        let _ = fs::remove_file(dir.join("not-spooled.ndjson"));
    }
    r
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
