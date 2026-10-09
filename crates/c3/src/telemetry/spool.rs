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
//! - `flush.owner.json` - (wave 5) the sender lock's OWNER record `{pid, start_time, start_ticks,
//!   token, since, host}` (the plugin's `.flush.lock` record, wave 28d D3): written whole (a
//!   temporary file renamed into place) right after the lock is taken, removed by its owner when it
//!   releases the lock. A separate file because an OS-locked file cannot be read by another process
//!   on Windows; the OS lock stays THE lock (never stale, never taken over) - the record only lets a
//!   refused sender and `c3 telemetry --status` say who holds it: "sender busy since <t>", and once
//!   the lock is 30 minutes old "sender stuck since <t> (pid <n>)" (a note in the last flush's
//!   record, once).
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
//! (wave 5) THE SEND, as the plugin's `Invoke-TelemetryFlush` against the intake as it is built:
//! every fresh event in batches of at most 100 (`{"events": [...]}`), oldest first; delivered only
//! for a 2xx whose body is the intake's JSON `{"ok": true}`. A 429 whose `Retry-After` is at most
//! 60 s - and fits into what is left of the flush's deadline - is waited for and the SAME request
//! sent once more (a longer or missing `Retry-After`, or a second 429, stops there; the batch is
//! kept). A 400 naming `events[i]: <reason>` drops event i (one line in the record's `rejected`)
//! and resends the rest - at most three times per flush. A 413 halves the batch; an event refused
//! alone is dropped (`rejected`: "HTTP 413 (too large alone)"). A 403 stops the flush ("the intake
//! refuses this app"); another 4xx, a 5xx, no answer stop it too - the spool is kept, nothing is
//! hammered. One flush ends after 60 s in all, one request after 8 s (TEST HOOKS, test mode only:
//! `CODEX_CONSULT_TEST_TELEMETRY_FLUSH_MS`, `CODEX_CONSULT_TEST_TELEMETRY_REQUEST_MS`): a request
//! starts only while 1.5 s are left (1 s kept back for the rewrite that follows) - what is not sent
//! stays. Lines queued more than 7 days ago are dropped.
//!
//! Never in a consultation's critical path: [`flush_in_background`] runs the sender in a thread
//! joined with a cap at the run's end (a send still running then is cut by the process's exit: its
//! lines stay queued - at worst a duplicate later, never a loss).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
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
/// (wave 28b, D2) One flush ends after 60 s in all - the lock, the reads, every request and
/// rewrite (`$script:TelemetryFlushMs`; TEST HOOK `CODEX_CONSULT_TEST_TELEMETRY_FLUSH_MS`).
const FLUSH_BUDGET: Duration = Duration::from_secs(60);
/// One request ends after 8 s (`$script:TelemetryRequestMs`; TEST HOOK
/// `CODEX_CONSULT_TEST_TELEMETRY_REQUEST_MS`).
const REQUEST_BUDGET: Duration = Duration::from_secs(8);
/// The connect bound of one request.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// The intake's rate rule: a 429 whose `Retry-After` is at most this is waited for once.
const RETRY_AFTER_MAX_SECS: i64 = 60;
/// (wave 28b, D8) At most three per-event refusals (`400 events[i]`) are acted on per flush.
const REJECT_ROUNDS: usize = 3;
/// Kept back of the deadline for the rewrite that follows a send.
const REWRITE_RESERVE: Duration = Duration::from_millis(1000);
/// A request starts only while this much more than [`REWRITE_RESERVE`] is left.
const REQUEST_FLOOR: Duration = Duration::from_millis(500);
/// (wave 28d, D3) A sender lock whose owner lives this long is reported "sender stuck".
const STUCK_LOCK_SECS: u64 = 1800;
/// An intake answer longer than this is cut (`Invoke-TelemetryRequest`).
const ANSWER_TEXT_MAX: usize = 65_536;
/// How long the sender waits for the spool lock for its read and its rewrite.
const SENDER_LOCK_WAIT: Duration = Duration::from_secs(2);
/// The spool file, the spool lock, the sender lock and the last flush's record.
pub(crate) const SPOOL_FILE: &str = "spool.ndjson";
pub(crate) const SPOOL_LOCK: &str = "spool.lock";
pub(crate) const FLUSH_LOCK: &str = "flush.lock";
/// (wave 5) The sender lock's owner record (the module docs).
pub(crate) const FLUSH_OWNER: &str = "flush.owner.json";
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
    /// (wave 5) The flush's deadline and one request's bound.
    flush_budget: Duration,
    request_budget: Duration,
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
    /// (wave 5, D8) The events the intake refused one by one - dropped, never resent: one line
    /// each, `event queued <time> refused: <reason>` (the record's `rejected`).
    pub rejected: Vec<String>,
    /// (wave 5) Why the send stopped before every event was delivered (`""`: it did not stop) - a
    /// 429, a 403, another refusal, no answer, the flush's deadline.
    pub stopped: String,
}

impl FlushReport {
    /// Every line the flush removed without delivering it: stale or unreadable, not closable, and
    /// refused by the intake (the record's `dropped`).
    pub fn dropped(&self) -> usize {
        self.dropped_stale + self.discarded + self.rejected.len()
    }
}

/// (wave 5) One POST's answer (`Invoke-TelemetryRequest`): delivered only for a 2xx whose body is
/// the intake's JSON object with `ok: true`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostAnswer {
    /// The HTTP status (`None`: no answer).
    pub status: Option<u16>,
    /// The intake accepted the batch.
    pub delivered: bool,
    /// The intake's `error` text, else the answer's text - where a 400 names `events[i]: <reason>`.
    pub error: String,
    /// `Retry-After` in seconds (a delta, or a date - rounded up, never negative); `None` when the
    /// answer gave none.
    pub retry_after: Option<i64>,
    /// Why it was not delivered (`""` when delivered).
    pub why: String,
}

impl PostAnswer {
    /// The intake's `{"ok": true}` (HTTP 200).
    pub fn ok() -> Self {
        PostAnswer {
            status: Some(200),
            delivered: true,
            ..Default::default()
        }
    }

    /// No answer (a refused connection, a timeout): `why`.
    pub fn none(why: impl Into<String>) -> Self {
        PostAnswer {
            why: why.into(),
            ..Default::default()
        }
    }

    /// An answer, classified as the plugin does: a 2xx JSON object with `ok: true` is delivered;
    /// an answer that is no JSON object is "HTTP <s> (<type>) is not the intake's JSON answer";
    /// otherwise "HTTP <s>: <error>" (or "HTTP <s> without ok: true").
    pub fn answer(status: u16, body: &str, content_type: &str, retry_after: Option<i64>) -> Self {
        let json = if body.trim_start().starts_with('{') {
            serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .filter(|v| v.is_object())
        } else {
            None
        };
        let err_field = json
            .as_ref()
            .and_then(|j| j.get("error"))
            .map(|e| match e {
                serde_json::Value::String(t) => t.clone(),
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        let ok = json
            .as_ref()
            .and_then(|j| j.get("ok"))
            .and_then(|v| v.as_bool())
            == Some(true);
        let mut a = PostAnswer {
            status: Some(status),
            retry_after,
            error: if err_field.is_empty() {
                body.to_string()
            } else {
                err_field.clone()
            },
            ..Default::default()
        };
        if (200..300).contains(&status) && ok {
            a.delivered = true;
        } else if json.is_none() {
            a.why = format!(
                "HTTP {status}{} is not the intake's JSON answer",
                if content_type.is_empty() {
                    String::new()
                } else {
                    format!(" ({content_type})")
                }
            );
        } else if !err_field.is_empty() {
            a.why = format!("HTTP {status}: {}", c3_core::one_line(&err_field));
        } else {
            a.why = format!("HTTP {status} without ok: true");
        }
        a
    }
}

/// `Retry-After`: delta seconds, or an HTTP date (seconds from now, rounded up); never negative.
pub(crate) fn parse_retry_after(v: &str) -> Option<i64> {
    let t = v.trim();
    if let Ok(n) = t.parse::<i64>() {
        return Some(n.max(0));
    }
    let date = DateTime::parse_from_rfc2822(t).ok()?;
    let ms = (date.with_timezone(&Utc) - Utc::now()).num_milliseconds();
    Some(((ms + 999).div_euclid(1000)).max(0))
}

/// A positive millisecond test hook (test mode only), else `default`.
fn hook_duration(name: &str, default: Duration) -> Duration {
    c3_core::test_hooks::hook(name)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis)
        .unwrap_or(default)
}

/// How a flush posts one body within a request bound (the real POST, or a test's).
type Post<'a> = dyn Fn(&str, Duration) -> PostAnswer + 'a;

/// Seconds as the plugin prints them (`[Math]::Round(x, 1)`: `60`, `1.5`).
fn secs_text(d: Duration) -> String {
    let r = (d.as_secs_f64() * 10.0).round() / 10.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r:.1}")
    }
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
            flush_budget: hook_duration("CODEX_CONSULT_TEST_TELEMETRY_FLUSH_MS", FLUSH_BUDGET),
            request_budget: hook_duration(
                "CODEX_CONSULT_TEST_TELEMETRY_REQUEST_MS",
                REQUEST_BUDGET,
            ),
        }
    }

    /// (wave 5) This spool with another flush deadline and request bound (tests; production takes
    /// 60 s and 8 s, or the test hooks).
    pub fn with_limits(mut self, flush: Duration, request: Duration) -> Self {
        self.flush_budget = flush;
        self.request_budget = request;
        self
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
    /// every event closed through the classes (F09-5); every fresh event posted in batches of at
    /// most 100 with the intake's answer handling (wave 5: 429, 400 `events[i]`, 413, 403, the
    /// deadline); then - under the spool lock again - exactly the delivered and the dropped lines
    /// removed from the CURRENT file.
    pub fn flush(&self) -> Result<FlushReport> {
        self.flush_inner(
            &|body, t| self.post_events(body, t),
            &FlushHooks::default(),
            false,
        )
    }

    /// [`Spool::flush`] with an injected sender (tests hold the "network" while they append):
    /// `true` is the intake's `{"ok": true}`, `false` no answer.
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
        let p = |b: &str, _t: Duration| bool_answer(post(b));
        self.flush_inner(&p, hooks, false)
    }

    /// (wave 5) [`Spool::flush`] with an injected intake that answers each posted body.
    pub fn flush_answered(&self, post: impl Fn(&str) -> PostAnswer) -> Result<FlushReport> {
        let p = |b: &str, _t: Duration| post(b);
        self.flush_inner(&p, &FlushHooks::default(), false)
    }

    /// THE flush of `c3 telemetry --flush` and of a consultation's background sender: the send of
    /// [`Spool::flush`], then - still under the sender lock - the last flush's record with the fold
    /// of the not-spooled files (wave 3b, [`Spool::record_flush`]).
    pub fn flush_recorded(&self) -> Result<FlushReport> {
        self.flush_inner(
            &|body, t| self.post_events(body, t),
            &FlushHooks::default(),
            true,
        )
    }

    /// [`Spool::flush_recorded`] with an injected sender.
    pub fn flush_recorded_with(&self, post: impl Fn(&str) -> bool) -> Result<FlushReport> {
        let p = |b: &str, _t: Duration| bool_answer(post(b));
        self.flush_inner(&p, &FlushHooks::default(), true)
    }

    /// (wave 5) [`Spool::flush_recorded`] with an injected intake that answers each posted body.
    pub fn flush_recorded_answered(
        &self,
        post: impl Fn(&str) -> PostAnswer,
    ) -> Result<FlushReport> {
        let p = |b: &str, _t: Duration| post(b);
        self.flush_inner(&p, &FlushHooks::default(), true)
    }

    fn flush_inner(
        &self,
        post: &Post<'_>,
        hooks: &FlushHooks<'_>,
        record: bool,
    ) -> Result<FlushReport> {
        // (wave 28c, D5) the deadline covers the WHOLE flush: the lock, the reads, every request
        let started = Instant::now();
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
        // a sender that finds the sender lock busy writes nothing but (wave 28d, D3) the note of a
        // stuck one: the sender that holds it records its own flush
        let Some(_sender) = lock_within(&self.dir.join(FLUSH_LOCK), Duration::ZERO)? else {
            report.skipped = self.busy_sender_why();
            return Ok(report);
        };
        // (wave 5) the owner record, born whole; removed before the lock is released (drop order)
        let _owner = SenderOwner::write(&self.dir.join(FLUSH_OWNER));
        let r = self.send_locked(post, hooks, report, started);
        if record {
            return self.record_flush(r);
        }
        r
    }

    /// (wave 28d, D3) Why a sender that found the sender lock busy skips - who holds it, from the
    /// owner record: "sender busy since <t> (pid <n> holds its lock, <s> s old)", or once the lock
    /// is 30 minutes old "sender stuck since <t> (pid <n>)", which also goes into the last flush's
    /// record's notes (once - a sender that holds the lock again drops it). The lock is never taken
    /// over: the OS releases it when its holder ends.
    fn busy_sender_why(&self) -> String {
        match sender_owner(&self.dir) {
            Some(o) if o.alive && o.age_secs >= STUCK_LOCK_SECS => {
                let stuck = format!("sender stuck since {} (pid {})", o.since, o.pid);
                if let Ok(Some(_l)) =
                    lock_within(&self.dir.join(SPOOL_LOCK), Duration::from_secs(1))
                {
                    notspooled::add_last_note(&self.paths, &stuck);
                }
                format!(
                    "another flush is running: {stuck} - its lock is {} min old and its owner lives: it is never taken over; stop pid {} if it hangs (the lock goes with its process)",
                    o.age_secs / 60,
                    o.pid
                )
            }
            Some(o) if o.alive => format!(
                "another flush is running: sender busy since {} (pid {} holds its lock, {} s old)",
                o.since, o.pid, o.age_secs
            ),
            _ => "another flush is running (its lock is held)".into(),
        }
    }

    /// The send, under the sender lock the caller holds (the rule of the module docs); `started`
    /// is when the flush began (its deadline runs from there).
    fn send_locked(
        &self,
        post: &Post<'_>,
        hooks: &FlushHooks<'_>,
        mut report: FlushReport,
        started: Instant,
    ) -> Result<FlushReport> {
        let left = || self.flush_budget.saturating_sub(started.elapsed());
        let deadline_text = format!(
            "the flush's deadline ({} s) was reached",
            secs_text(self.flush_budget)
        );
        // (wave 28c, D5) the read only while there is time for it and a send
        if left() < REQUEST_FLOOR + REWRITE_RESERVE {
            report.stopped = deadline_text;
            report.kept = self.pending();
            return Ok(report);
        }
        // the snapshot, read under the spool lock
        let snapshot = {
            let wait = SENDER_LOCK_WAIT.min(left().saturating_sub(REQUEST_FLOOR + REWRITE_RESERVE));
            let Some(_lock) = lock_within(&self.dir.join(SPOOL_LOCK), wait)? else {
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
        // (line, closed body, queued unix seconds), oldest first
        let mut fresh: Vec<(&str, String, i64)> = Vec::new();
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
                    Some(body) => fresh.push((l, body, s.queued)),
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
        if !fresh.is_empty() {
            if self.hub.trim().is_empty() {
                report.skipped = "the configured intake is refused".into();
            } else {
                self.send_batches(post, fresh, &mut remove, &mut report, &left, &deadline_text);
            }
        }
        if !remove.is_empty() {
            let reread: &Reread = match hooks.reread {
                Some(r) => r,
                None => &|p| fs::read_to_string(p),
            };
            // (wave 28c, D5) the lines already delivered must go: the rewrite is always attempted,
            // waiting at most what is left of the deadline (at least 0.1 s)
            let wait = SENDER_LOCK_WAIT.min(left()).max(Duration::from_millis(100));
            self.remove_lines(&remove, reread, wait)?;
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
    fn remove_lines(&self, lines: &[&str], reread: &Reread, wait: Duration) -> Result<()> {
        let Some(_lock) = lock_within(&self.dir.join(SPOOL_LOCK), wait)? else {
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

    /// (wave 5) Post every fresh event in batches of at most 100 with the intake's answer handling
    /// (the module docs); the delivered and the refused lines go into `remove`, the rest stays.
    fn send_batches<'s>(
        &self,
        post: &Post<'_>,
        mut events: Vec<(&'s str, String, i64)>,
        remove: &mut Vec<&'s str>,
        report: &mut FlushReport,
        left: &dyn Fn() -> Duration,
        deadline_text: &str,
    ) {
        let mut batch_size = MAX_BATCH;
        let mut i = 0usize;
        let mut rounds = 0usize;
        while i < events.len() && report.stopped.is_empty() {
            let l = left();
            if l < REQUEST_FLOOR + REWRITE_RESERVE {
                report.stopped = deadline_text.to_string();
                break;
            }
            let end = (i + batch_size).min(events.len());
            let n = end - i;
            let body = format!(
                "{{\"events\":[{}]}}",
                events[i..end]
                    .iter()
                    .map(|(_, b, _)| b.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            report.attempted = true;
            let a = self.send_rate_ruled(post, &body, l - REWRITE_RESERVE);
            if a.status.is_some() {
                report.http = a.status;
            }
            if a.delivered {
                report.sent += n;
                remove.extend(events[i..end].iter().map(|(line, _, _)| *line));
                i = end;
                continue;
            }
            let status = a.status.unwrap_or(0);
            // (D8) 400 "events[i]: <reason>": event i is dropped - one line in `rejected` - and the
            // rest resent; a fourth such refusal in one flush stops it
            if status == 400 {
                if let Some((k, reason)) = event_refusal(&a.error).filter(|(k, _)| *k < n) {
                    if rounds >= REJECT_ROUNDS {
                        report.stopped = format!(
                            "HTTP 400: {} - a fourth refused event in this flush; the rest stays",
                            c3_core::one_line(&a.error)
                        );
                        break;
                    }
                    rounds += 1;
                    let (line, _, queued) = events.remove(i + k);
                    remove.push(line);
                    report.rejected.push(format!(
                        "event queued {} refused: {}",
                        queued_text(queued),
                        c3_core::one_line(&reason)
                    ));
                    continue;
                }
            }
            // 413: the batch is halved; an event refused alone is dropped
            if status == 413 {
                if n > 1 {
                    batch_size = (n / 2).max(1);
                    continue;
                }
                let (line, _, queued) = events.remove(i);
                remove.push(line);
                report.rejected.push(format!(
                    "event queued {} refused: HTTP 413 (too large alone)",
                    queued_text(queued)
                ));
                continue;
            }
            // 403 - the app refused; another 4xx, a 5xx, a 429 not retried, no answer: the flush
            // stops there and the spool is kept (nothing is hammered)
            report.stopped = if status == 403 {
                format!(
                    "HTTP 403 - the intake refuses this app ({}); the spool is kept",
                    c3_core::one_line(&a.why)
                )
            } else if (400..500).contains(&status) && status != 429 {
                format!("{} - the spool is kept", c3_core::one_line(&a.why))
            } else {
                a.why.clone()
            };
        }
    }

    /// `Invoke-TelemetrySend`: one POST within `budget` (each request bounded by the request budget
    /// and what is left); a 429 whose `Retry-After` is at most 60 s - and fits into the budget with
    /// a second to spare - is waited for and the SAME body sent once more. No other retry.
    fn send_rate_ruled(&self, post: &Post<'_>, body: &str, budget: Duration) -> PostAnswer {
        let watch = Instant::now();
        let one = |left: Duration| {
            left.min(self.request_budget)
                .max(Duration::from_millis(100))
        };
        let mut a = post(body, one(budget.saturating_sub(watch.elapsed())));
        if a.status == Some(429) {
            let left = budget.saturating_sub(watch.elapsed());
            a.why = match a.retry_after {
                Some(ra)
                    if ra <= RETRY_AFTER_MAX_SECS
                        && Duration::from_millis(ra as u64 * 1000 + 1000) < left =>
                {
                    thread::sleep(Duration::from_secs(ra as u64));
                    a = post(body, one(budget.saturating_sub(watch.elapsed())));
                    if a.status == Some(429) {
                        "HTTP 429 again after its Retry-After".to_string()
                    } else {
                        a.why.clone()
                    }
                }
                Some(ra) if ra <= RETRY_AFTER_MAX_SECS => format!(
                    "HTTP 429 (Retry-After {ra} s does not fit into the flush's deadline: not retried now)"
                ),
                Some(ra) => format!(
                    "HTTP 429 (Retry-After {ra} s, more than {RETRY_AFTER_MAX_SECS} s: not retried now)"
                ),
                None => "HTTP 429 (Retry-After not given: not retried now)".to_string(),
            };
        }
        a
    }

    /// The real POST of one batch to `<hub>/v2/events`, bounded by `timeout` (no redirect).
    fn post_events(&self, body: &str, timeout: Duration) -> PostAnswer {
        if self.hub.trim().is_empty() {
            return PostAnswer::none("the configured intake is refused");
        }
        let url = format!("{}/v2/events", self.hub.trim_end_matches('/'));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT.min(timeout))
            .timeout(timeout)
            .redirects(0)
            .build();
        let answer = agent
            .post(&url)
            .set("content-type", "application/json")
            .set("accept", "application/json")
            .send_string(body);
        let resp = match answer {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => {
                let m = c3_core::one_line(&e.to_string());
                debug_log(&format!("telemetry flush kept the spool: {m}"));
                let lower = m.to_ascii_lowercase();
                return PostAnswer::none(
                    if lower.contains("timed out") || lower.contains("timeout") {
                        format!("no answer within {} s", secs_text(timeout))
                    } else {
                        format!("no answer ({m})")
                    },
                );
            }
        };
        let status = resp.status();
        let ctype = resp
            .header("content-type")
            .map(|c| c.split(';').next().unwrap_or("").trim().to_string())
            .unwrap_or_default();
        let retry_after = resp.header("retry-after").and_then(parse_retry_after);
        let mut text = resp.into_string().unwrap_or_default();
        if text.len() > ANSWER_TEXT_MAX {
            let mut cut = ANSWER_TEXT_MAX;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
        }
        let a = PostAnswer::answer(status, &text, &ctype, retry_after);
        if !a.delivered {
            debug_log(&format!("telemetry flush kept the spool: {}", a.why));
        }
        a
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
    /// and nothing is seen - the record's `not_spooled_seen` stays the baseline the last fold left
    /// (wave 3f, F31-1: wave 3d's lines of the files a fold would keep let a producer live here and
    /// gone at the next fold be seen here and folded there; the plugin's flush sees every line but
    /// the legacy file's); the names an earlier fold recorded stay while their files are there.
    /// (wave 3d, F24-6) A rewrite that drops the names fails: `last_warning` says so and the record
    /// goes on naming the deleted files - no count depends on them (`--status` and the next fold
    /// look only at files that exist, the next record drops them, a staged generation never takes
    /// such a name).
    ///
    /// TEST HOOK (test mode only): `CODEX_CONSULT_TEST_FOLD_CRASH=1` - the process exits (87)
    /// between the save of the record and the deletes, as a crash would; `=2` - it exits (88) between
    /// the deletes and the rewrite that drops their names; `CODEX_CONSULT_TEST_FOLD_REWRITE_FAIL=1`
    /// (C3's) - that rewrite fails.
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
            // (wave 3f, F31-1) no fold, so no line is seen: the last fold's baseline is carried
            // and every line since counts until a fold keeps or takes its file
            let kept: Vec<notspooled::FoldedEntry> = folded_before
                .values()
                .filter(|e| p.ns_dir.join(&e.name).exists())
                .cloned()
                .collect();
            (notspooled::carried_seen(before.as_ref()), kept)
        };
        let (delivered, kept, dropped, rejected, http) = match &r {
            // `kept`: the spool's count now (a skipped flush did not count it); `dropped` counts the
            // events the intake refused too, each named in `rejected` (wave 5)
            Ok(rep) => (
                serde_json::json!(rep.sent),
                serde_json::json!(self.pending()),
                serde_json::json!(rep.dropped()),
                serde_json::json!(rep.rejected),
                serde_json::json!(rep.http),
            ),
            Err(_) => (
                serde_json::json!(0),
                serde_json::Value::Null,
                serde_json::json!(0),
                serde_json::json!([]),
                serde_json::Value::Null,
            ),
        };
        let mut last = serde_json::json!({
            "time": notspooled::now_iso(),
            "result": result,
            "delivered": delivered,
            "kept": kept,
            "dropped": dropped,
            "rejected": rejected,
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
                        let rewrite =
                            if c3_core::test_hooks::hook("CODEX_CONSULT_TEST_FOLD_REWRITE_FAIL")
                                .is_some_and(|v| v.trim() == "1")
                            {
                                Err(Error::new("test hook CODEX_CONSULT_TEST_FOLD_REWRITE_FAIL"))
                            } else {
                                notspooled::write_last(&p.last, &last)
                            };
                        if let Err(e) = rewrite {
                            // (wave 3d, F24-6) said, never swallowed; the names stay until the
                            // next record, which keeps only the names whose files are there
                            warning = format!(
                                "{} could not be rewritten after the fold's deletes ({e}) - it still names {} deleted file(s); no count depends on those names and the next flush drops them",
                                p.last.display(),
                                folded.len() - left.len()
                            );
                        }
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
    match report {
        Ok(r) => r.result_text(),
        Err(e) => format!("failed - {e}"),
    }
}

impl FlushReport {
    /// (wave 5) The result line as the plugin's `Invoke-TelemetryFlush` words it (`.last` `result`
    /// and `codex-telemetry: <result>`): "nothing to send", "delivered <n>, kept <k>, dropped <d>
    /// (...)", or "not delivered: <why> - delivered <n>, kept <k>, dropped <d>[, <r> refused by the
    /// intake]"; a flush that did not reach the send says "skipped - <why>"; C3's discarded events
    /// (no closable C3 event) are named after it.
    pub fn result_text(&self) -> String {
        if !self.skipped.is_empty() {
            return format!("skipped - {}", self.skipped);
        }
        let rej = self.rejected.len();
        let dropped = self.dropped();
        let mut result = if !self.stopped.is_empty() {
            format!(
                "not delivered: {} - delivered {}, kept {}, dropped {}{}",
                self.stopped,
                self.sent,
                self.kept,
                dropped,
                if rej > 0 {
                    format!(", {rej} refused by the intake")
                } else {
                    String::new()
                }
            )
        } else if self.sent == 0 && dropped == 0 {
            "nothing to send".to_string()
        } else {
            format!(
                "delivered {}, kept {}, dropped {}{}",
                self.sent,
                self.kept,
                dropped,
                if dropped > rej {
                    format!(
                        " (older than 7 days or unreadable{})",
                        if rej > 0 { ", or refused" } else { "" }
                    )
                } else if rej > 0 {
                    " (refused by the intake)".to_string()
                } else {
                    String::new()
                }
            )
        };
        if self.discarded > 0 {
            result.push_str(&format!(
                "; discarded {} queued event(s) that are no closable C3 event (not sent)",
                self.discarded
            ));
        }
        result
    }
}

/// An injected sender's yes/no as an intake answer (`true`: `{"ok": true}`).
fn bool_answer(delivered: bool) -> PostAnswer {
    if delivered {
        PostAnswer::ok()
    } else {
        PostAnswer::none("no answer (the injected sender did not deliver)")
    }
}

/// The intake's per-event refusal in a 400's error: `events[<i>]: <reason>`.
fn event_refusal(error: &str) -> Option<(usize, String)> {
    let re = regex::Regex::new(r"events\[(\d+)\]\s*:\s*([^\r\n]*)").ok()?;
    let c = re.captures(error)?;
    Some((c[1].parse().ok()?, c[2].to_string()))
}

/// A queue time (unix seconds) as the plugin names a refused event: the local
/// `yyyy-MM-ddTHH:mm:sszzz`.
fn queued_text(queued: i64) -> String {
    DateTime::from_timestamp(queued, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%dT%H:%M:%S%:z")
                .to_string()
        })
        .unwrap_or_else(|| queued.to_string())
}

/// (wave 28d, D3) The sender lock's owner as its record says (read only): `None` without one.
#[derive(Debug, Clone)]
pub(crate) struct SenderOwnerInfo {
    pub pid: u32,
    pub since: String,
    /// The owner process lives (or its identity cannot be confirmed).
    pub alive: bool,
    /// How old the record is (it is written once, when the lock is taken).
    pub age_secs: u64,
}

/// Read the owner record of the sender lock under `dir` (`flush.owner.json`).
pub(crate) fn sender_owner(dir: &Path) -> Option<SenderOwnerInfo> {
    let path = dir.join(FLUSH_OWNER);
    let m = notspooled::forgetting_owner(&path);
    if !m.there {
        return None;
    }
    let age_secs = fs::metadata(&path)
        .and_then(|md| md.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let since = if m.since.is_empty() {
        fs::metadata(&path)
            .and_then(|md| md.modified())
            .ok()
            .map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%Y-%m-%dT%H:%M:%S%:z")
                    .to_string()
            })
            .unwrap_or_else(|| "?".to_string())
    } else {
        m.since
    };
    Some(SenderOwnerInfo {
        pid: m.pid,
        since,
        alive: m.alive,
        age_secs,
    })
}

/// (wave 28d, D3) The `--status` line of the sender lock (`sender     : ...`), read only: busy,
/// stuck (30 minutes), or a record its owner left behind; `None` when no record is there.
pub fn sender_status(dir: &Path) -> Option<String> {
    let o = sender_owner(dir)?;
    Some(if o.alive && o.age_secs >= STUCK_LOCK_SECS {
        format!(
            "sender stuck since {} (pid {}) - its lock is {} min old and its owner lives: it is never taken over; stop pid {} if it hangs (the lock goes with its process)",
            o.since,
            o.pid,
            o.age_secs / 60,
            o.pid
        )
    } else if o.alive {
        format!(
            "busy since {} (pid {} holds its lock, {} s old)",
            o.since, o.pid, o.age_secs
        )
    } else if o.pid > 0 {
        format!(
            "a lock record whose owner pid {} is gone - the next sender replaces it",
            o.pid
        )
    } else {
        "a lock record that names no owner - the next sender replaces it".to_string()
    })
}

/// The owner record a flush writes once it holds the sender lock; removed when dropped, and only
/// while it still carries this sender's token.
struct SenderOwner {
    path: PathBuf,
    token: String,
}

impl SenderOwner {
    /// Write `{pid, start_time, start_ticks, token, since, host}` of this process, whole (a
    /// temporary file renamed into place). `None` when it cannot be written (the flush goes on: the
    /// OS lock is the lock; only the "who holds it" is then unknown).
    fn write(path: &Path) -> Option<SenderOwner> {
        let me = std::process::id();
        let token = uuid::Uuid::new_v4().simple().to_string();
        let ticks = notspooled::own_start_ticks();
        let o = serde_json::json!({
            "pid": me,
            "start_time": crate::liveness::proc::process_start_iso(me).unwrap_or_default(),
            "start_ticks": if ticks > 0 { serde_json::json!(ticks) } else { serde_json::Value::Null },
            "token": token,
            "since": notspooled::now_iso(),
            "host": c3_core::host::machine_name(),
        });
        replace_atomic(path, format!("{o}\n").as_bytes()).ok()?;
        Some(SenderOwner {
            path: path.to_path_buf(),
            token,
        })
    }
}

impl Drop for SenderOwner {
    fn drop(&mut self) {
        let mine = fs::read_to_string(&self.path)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("token").and_then(|t| t.as_str()).map(String::from))
            .is_some_and(|t| t == self.token);
        if mine {
            let _ = fs::remove_file(&self.path);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// (wave 5) The intake's answer as the plugin classifies it.
    #[test]
    fn answers_are_classified_as_the_plugin_does() {
        let ok = PostAnswer::answer(200, r#"{"ok":true,"accepted":1}"#, "application/json", None);
        assert!(ok.delivered && ok.why.is_empty());
        let html = PostAnswer::answer(200, "<html>T</html>", "text/html", None);
        assert_eq!(
            html.why,
            "HTTP 200 (text/html) is not the intake's JSON answer"
        );
        let notok = PostAnswer::answer(200, r#"{"ok":false}"#, "application/json", None);
        assert_eq!(notok.why, "HTTP 200 without ok: true");
        let e = PostAnswer::answer(400, r#"{"ok":false,"error":"events[2]: x"}"#, "", None);
        assert_eq!(e.why, "HTTP 400: events[2]: x");
        assert_eq!(e.error, "events[2]: x");
        assert_eq!(event_refusal(&e.error), Some((2, "x".to_string())));
        assert_eq!(
            event_refusal("events[1] : too long\nmore"),
            Some((1, "too long".into()))
        );
        assert_eq!(event_refusal("no index"), None);
        let plain = PostAnswer::answer(404, "not found", "", None);
        assert_eq!(plain.why, "HTTP 404 is not the intake's JSON answer");
        assert_eq!(plain.error, "not found");
    }

    #[test]
    fn retry_after_takes_seconds_or_a_date() {
        assert_eq!(parse_retry_after(" 2 "), Some(2));
        assert_eq!(parse_retry_after("-5"), Some(0));
        assert_eq!(parse_retry_after("soon"), None);
        let date = (Utc::now() + chrono::Duration::seconds(30))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let s = parse_retry_after(&date).unwrap();
        assert!((29..=31).contains(&s), "{s}");
        assert_eq!(parse_retry_after("Sun, 06 Nov 1994 08:49:37 GMT"), Some(0));
    }

    #[test]
    fn the_result_line_is_the_plugins() {
        let mut r = FlushReport::default();
        assert_eq!(r.result_text(), "nothing to send");
        r.sent = 2;
        r.kept = 1;
        assert_eq!(r.result_text(), "delivered 2, kept 1, dropped 0");
        r.dropped_stale = 1;
        assert_eq!(
            r.result_text(),
            "delivered 2, kept 1, dropped 1 (older than 7 days or unreadable)"
        );
        r.rejected.push("event queued t refused: x".into());
        assert_eq!(
            r.result_text(),
            "delivered 2, kept 1, dropped 2 (older than 7 days or unreadable, or refused)"
        );
        r.stopped = "HTTP 500: boom".into();
        assert_eq!(
            r.result_text(),
            "not delivered: HTTP 500: boom - delivered 2, kept 1, dropped 2, 1 refused by the intake"
        );
        r.skipped = "another flush is running (its lock is held)".into();
        assert_eq!(
            r.result_text(),
            "skipped - another flush is running (its lock is held)"
        );
        assert_eq!(secs_text(Duration::from_secs(60)), "60");
        assert_eq!(secs_text(Duration::from_millis(1600)), "1.6");
    }
}
