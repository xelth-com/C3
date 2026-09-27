//! The T-hub telemetry client (milestone 5).
//!
//! One anonymous event per consultation, on by default and off with one line, is the
//! published contract (README "Telemetry", DESIGN §8, the T-hub client rules at
//! `xelth.com/docs/t-hub`). This module implements it with privacy by construction: the
//! payload is a typed allowlist ([`event`]) that cannot carry content, an NDJSON spool
//! with background send/retry/7-day drop ([`spool`]), the visible off switch, the
//! first-run notice, `--complain` and `--forget-me` ([`complaint`]).
//!
//! Nothing here ever fails the caller: every public function returns [`Result`], the
//! consult flow ignores failures, and diagnostics go to stderr only under `C3_DEBUG=1`.
//! Endpoints (all under `https://xelth.com/T`): `POST /v2/events`,
//! `POST /v2/complaints`, `DELETE /v2/instances/<id>?public_ref=<ref>`.

mod complaint;
mod event;
mod spool;

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use c3_core::ledger::{LedgerEntry, SessionsFile};

pub use complaint::{complain, complain_to, forget_me, forget_me_at, ForgetOutcome};
pub use event::{Details, Event};
pub use spool::{flush_in_background, BackgroundFlush, FlushReport, Spool};

/// The default T-hub base URL.
const HUB_DEFAULT: &str = "https://xelth.com/T";

/// The condensed three-line first-run notice (README "Telemetry").
const NOTICE: &str = "\
C3 telemetry is on. Installing C3 means accepting these terms: one anonymous event per consultation goes to https://xelth.com/T/ — never your code, prompts, paths, or keys.
Turn it off any time with CODEX_CONSULT_TELEMETRY=off (or --telemetry off per run); everything collected is shown as counts at https://xelth.com/C3/.
Run `c3 forget-me` to erase everything this installation ever sent.";

// --------------------------------------------------------------------------- error type

/// A telemetry error. Public functions return it, but the consult flow ignores failures.
#[derive(Debug)]
pub struct Error(String);

impl Error {
    fn new(msg: impl Into<String>) -> Self {
        Error(msg.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(c3_core::one_line(&e.to_string()))
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error(c3_core::one_line(&e.to_string()))
    }
}

/// The telemetry result type.
pub type Result<T> = std::result::Result<T, Error>;

// --------------------------------------------------------------------------- config / switches

/// Caller-supplied run configuration: the `--telemetry on|off` value, if the caller passed
/// one. The environment switch `CODEX_CONSULT_TELEMETRY=off` is read separately and always
/// wins.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// `Some(false)` for `--telemetry off`, `Some(true)` for `--telemetry on`, `None` when
    /// the flag was absent.
    pub telemetry: Option<bool>,
}

/// Whether telemetry is enabled: off if the environment switch is `off`, else off if
/// `--telemetry off` was given, else on (the default).
pub fn is_enabled(config: &Config) -> bool {
    if env_off() {
        return false;
    }
    !matches!(config.telemetry, Some(false))
}

/// The dry-run status sentence.
pub fn status(config: &Config) -> String {
    if is_enabled(config) {
        format!(
            "telemetry: on (spool {} pending)",
            default_spool().pending()
        )
    } else if env_off() {
        "telemetry: off (CODEX_CONSULT_TELEMETRY=off)".to_string()
    } else {
        "telemetry: off (--telemetry off)".to_string()
    }
}

/// The first-run notice, returned once for the caller to print, then never again (a marker
/// file records that it was shown). The marker lives under the REAL home config dir
/// (`~/.codex/c3/telemetry/notice-shown`), ignoring `CODEX_HOME`, so scratch homes used by
/// the harnesses never mark it shown nor trigger it.
pub fn first_run_notice() -> Option<String> {
    first_run_notice_in(&real_home_telemetry_dir())
}

/// The real-home telemetry config dir for the notice marker, ignoring `CODEX_HOME`
/// (`~/.codex/c3/telemetry`).
fn real_home_telemetry_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok().filter(|s| !s.is_empty()));
    match home {
        Some(h) => PathBuf::from(h).join(".codex").join("c3").join("telemetry"),
        None => PathBuf::from(".codex").join("c3").join("telemetry"),
    }
}

/// [`first_run_notice`] at an explicit directory (for tests).
pub fn first_run_notice_in(dir: &Path) -> Option<String> {
    let marker = dir.join("notice-shown");
    if marker.exists() {
        return None;
    }
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(&marker, b"1");
    Some(NOTICE.to_string())
}

/// True when the visible environment switch disables telemetry.
pub(crate) fn env_off() -> bool {
    std::env::var("CODEX_CONSULT_TELEMETRY")
        .map(|v| v.trim().eq_ignore_ascii_case("off"))
        .unwrap_or(false)
}

// --------------------------------------------------------------------------- record

/// Build the event for one consultation, enqueue it to the spool (unless the environment
/// switch is off), and return the payload for the dry-run line. The call site in the
/// consult flow is wired by the coordinator; failures are ignored there.
pub fn record_consultation(
    entry: &LedgerEntry,
    panel_size: Option<u32>,
) -> Result<serde_json::Value> {
    let event = Event::from_ledger(entry, panel_size, &instance_id());
    let payload = serde_json::to_value(&event)?;
    if !env_off() {
        default_spool().enqueue(&event)?;
    }
    Ok(payload)
}

// --------------------------------------------------------------------------- instance id

/// The stable, anonymous installation id: `sha256(salt + machine name)` where the salt is
/// 32 random bytes written once. Uses the default directory.
pub fn instance_id() -> String {
    instance_id_in(&telemetry_dir())
}

/// [`instance_id`] with an explicit directory (for tests): stable across calls in the same
/// directory, different for a different salt or machine.
pub fn instance_id_in(dir: &Path) -> String {
    let salt = read_or_create_salt(dir);
    let host = machine_name();
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(host.as_bytes());
    to_hex(&hasher.finalize())
}

/// Read the 64-hex-char salt, creating it (32 random bytes) once.
fn read_or_create_salt(dir: &Path) -> String {
    let path = dir.join("salt");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    // 32 bytes of randomness from two v4 UUIDs, stored as hex.
    let salt = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(&path, &salt);
    salt
}

/// The machine name, from the same environment the reference client reads.
fn machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default()
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// --------------------------------------------------------------------------- paths / defaults

/// `<CODEX_HOME or ~/.codex>/c3/telemetry` — the config directory for the spool, salt,
/// notice marker and complaint references.
pub fn telemetry_dir() -> PathBuf {
    codex_home().join("c3").join("telemetry")
}

/// The Codex home, matching `providers`: `CODEX_HOME`, else `~/.codex`.
fn codex_home() -> PathBuf {
    if let Ok(h) = std::env::var("CODEX_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    let home = std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok().filter(|s| !s.is_empty()));
    match home {
        Some(h) => PathBuf::from(h).join(".codex"),
        None => PathBuf::from(".codex"),
    }
}

/// The default spool (default directory + default hub).
fn default_spool() -> Spool {
    Spool::new(telemetry_dir(), default_hub())
}

/// The hub base URL, overridable by `C3_TELEMETRY_HUB` (for staging/tests).
pub(crate) fn default_hub() -> String {
    std::env::var("C3_TELEMETRY_HUB")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| HUB_DEFAULT.to_string())
}

/// Log a diagnostic to stderr only under `C3_DEBUG=1`.
pub(crate) fn debug_log(msg: &str) {
    if std::env::var("C3_DEBUG").map(|v| v == "1").unwrap_or(false) {
        eprintln!("c3 telemetry: {msg}");
    }
}

// --------------------------------------------------------------------------- last-run summary

/// A one-line, secret-free summary of the newest task's last ledger entry in `collab_dir`
/// of the current repository, for a complaint's `context.last_run`. Reads only. Returns
/// `None` when there is no ledger to summarize.
pub fn last_run_summary(collab_dir: &str) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let repo_root = crate::providers::resolve_repo_root(&cwd);
    let collab_root = crate::providers::resolve_collab_root(&repo_root, collab_dir);
    let entry = newest_task_last_entry(&collab_root)?;
    Some(summarize_entry(&entry))
}

/// The last ledger entry of the most recently modified task under `collab_root`.
fn newest_task_last_entry(collab_root: &Path) -> Option<LedgerEntry> {
    if !collab_root.is_dir() {
        return None;
    }
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(collab_root).ok()?.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let sessions = p.join("sessions.json");
        if !sessions.is_file() {
            continue;
        }
        let modified = sessions
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
            newest = Some((modified, sessions));
        }
    }
    let (_, sessions) = newest?;
    let bytes = std::fs::read(&sessions).ok()?;
    let file = SessionsFile::read(&bytes).ok()?;
    file.codex.consults.into_iter().last()
}

/// A summary built only from label/numeric fields (never a brief, thread or path).
fn summarize_entry(entry: &LedgerEntry) -> String {
    let engine =
        event::safe_label(&entry.reviewer.engine, 32).unwrap_or_else(|| "unknown".to_string());
    let purpose = event::safe_label(&entry.purpose, 48).unwrap_or_else(|| "unknown".to_string());
    let verdict = event::safe_label(&entry.verdict, 48).unwrap_or_else(|| "unknown".to_string());
    let findings =
        entry.findings.blocker + entry.findings.major + entry.findings.minor + entry.findings.note;
    format!(
        "consult n={} purpose={} engine={} verdict={} findings={} wall={:.0}s",
        entry.n, purpose, engine, verdict, findings, entry.wall_seconds
    )
}
