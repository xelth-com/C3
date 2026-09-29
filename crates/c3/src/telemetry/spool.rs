//! The NDJSON spool and its background sender.
//!
//! One event per line under `<codex home>/c3/telemetry/spool.ndjson`. [`Spool::enqueue`]
//! appends; [`Spool::flush`] drops lines older than 7 days, sends up to 100 events in one
//! POST with a 3 s connect+read timeout, removes what was accepted and keeps the rest, and
//! never retries within the same run (a 403 or 429 just keeps the spool for next run).
//! Flushing runs in a detached thread ([`flush_in_background`]) joined with a cap at exit,
//! so a consultation is never blocked (README "The four rules", rule 2).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::telemetry::event::Event;
use crate::telemetry::{debug_log, default_hub, env_off, telemetry_dir, Result};

/// Maximum events per POST (README: `≤ 100`).
const MAX_BATCH: usize = 100;
/// Drop spooled events older than this (README: 7 days).
const MAX_AGE_SECS: i64 = 7 * 86_400;
/// The 3 s background-send budget (README rule 2).
const SEND_TIMEOUT: Duration = Duration::from_secs(3);

/// The spool at a directory, posting to a hub base URL (`.../T`).
pub struct Spool {
    dir: PathBuf,
    hub: String,
}

/// What a [`Spool::flush`] did, for the debug log and tests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FlushReport {
    /// Events accepted by the hub and removed from the spool.
    pub sent: usize,
    /// Events dropped for being older than 7 days.
    pub dropped_stale: usize,
    /// Events left in the spool (over the batch cap, or kept after a failed send).
    pub kept: usize,
    /// Whether a network send was attempted (false when nothing fresh remained).
    pub attempted: bool,
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
        self.dir.join("spool.ndjson")
    }

    /// The number of events currently queued.
    pub fn pending(&self) -> usize {
        fs::read_to_string(self.path())
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
    }

    /// Append one event as a single NDJSON line.
    pub fn enqueue(&self, event: &Event) -> Result<()> {
        self.enqueue_line(event)
    }

    /// Append any serializable allowlisted event (e.g. a [`crate::telemetry::RatingEvent`]).
    pub fn enqueue_line<T: serde::Serialize>(&self, event: &T) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let line = serde_json::to_string(event)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path())?;
        writeln!(f, "{line}")?;
        Ok(())
    }

    /// Send the spool once. Drops stale lines, posts a batch, removes what was accepted,
    /// and never retries within the run. On any send failure the file is left as it is
    /// (stale lines are filtered again next run).
    pub fn flush(&self) -> Result<FlushReport> {
        let text = match fs::read_to_string(self.path()) {
            Ok(t) => t,
            Err(_) => return Ok(FlushReport::default()),
        };
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.is_empty() {
            return Ok(FlushReport::default());
        }
        let cutoff = Utc::now().timestamp() - MAX_AGE_SECS;
        let fresh: Vec<String> = lines
            .iter()
            .filter(|l| line_timestamp(l).is_none_or(|t| t > cutoff))
            .map(|s| s.to_string())
            .collect();
        let dropped_stale = lines.len() - fresh.len();

        if fresh.is_empty() {
            // Everything was stale: clear the file, no network.
            let _ = fs::write(self.path(), "");
            return Ok(FlushReport {
                sent: 0,
                dropped_stale,
                kept: 0,
                attempted: false,
            });
        }

        let batch = &fresh[..fresh.len().min(MAX_BATCH)];
        let body = format!("{{\"events\":[{}]}}", batch.join(","));
        if self.post_events(&body) {
            let rest: String = fresh[batch.len()..]
                .iter()
                .map(|l| format!("{l}\n"))
                .collect();
            let _ = fs::write(self.path(), &rest);
            Ok(FlushReport {
                sent: batch.len(),
                dropped_stale,
                kept: fresh.len() - batch.len(),
                attempted: true,
            })
        } else {
            // Leave the file alone; stale lines are filtered again next run.
            Ok(FlushReport {
                sent: 0,
                dropped_stale: 0,
                kept: lines.len(),
                attempted: true,
            })
        }
    }

    fn post_events(&self, body: &str) -> bool {
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
}

/// Read the RFC 3339 `client_time` of one spooled line as a UNIX timestamp.
fn line_timestamp(line: &str) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let t = v.get("client_time")?.as_str()?;
    DateTime::parse_from_rfc3339(t).ok().map(|d| d.timestamp())
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

/// Spawn a background flush of the default spool at the start of a consultation. Does
/// nothing over the network when telemetry is switched off, but still returns a handle so
/// the call site is uniform.
pub fn flush_in_background() -> BackgroundFlush {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        if !env_off() {
            let _ = Spool::new(telemetry_dir(), default_hub()).flush();
            // The priors refresh runs AFTER the events are sent (M9 §3, F8): once a day at most,
            // injected fetcher in tests. A panic or error inside it must never affect the flush,
            // so it is isolated in catch_unwind.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::router::maybe_refresh_priors();
            }));
        }
        let _ = tx.send(());
    });
    BackgroundFlush { done: rx }
}
