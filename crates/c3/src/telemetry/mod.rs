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

pub mod backfill;
pub mod classes;
mod complaint;
mod event;
pub mod notspooled;
mod spool;

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use c3_core::ledger::{LedgerEntry, SessionsFile};

pub use classes::{
    close_judge, closed_judge, consult_ref_of, mark_judge, rating_actor, resolve_judge, Judge,
};
pub use complaint::{
    complain, complain_to, forget, forget_at, forget_with, newest_ref, pending_deletion_in,
    ForgetOutcome, ForgetRequest, PendingDeletion, PHASE_CLEANING, PHASE_CONFIRMED, PHASE_PENDING,
};
pub(crate) use event::topic_slug;
pub use event::{Details, Event, RatingDetails, RatingEvent, RatingInput};
pub use notspooled::{local_paths, LocalPaths};
pub use spool::{
    flush_in_background, flush_now, sender_status, BackgroundFlush, FlushHooks, FlushReport,
    PostAnswer, Spool,
};

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
/// one. (0.6.1 parity, `Get-TelemetrySwitch`) A run's own `--telemetry on|off` wins over the
/// environment switch `CODEX_CONSULT_TELEMETRY` (see [`switch`]).
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// `Some(false)` for `--telemetry off`, `Some(true)` for `--telemetry on`, `None` when
    /// the flag was absent.
    pub telemetry: Option<bool>,
}

/// The telemetry switch as resolved (`Get-TelemetrySwitch`): on or off, and where that comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Switch {
    pub on: bool,
    /// `the default`, `-Telemetry` (a run's own `--telemetry on|off`), `CODEX_CONSULT_TELEMETRY`,
    /// or `CODEX_CONSULT_TELEMETRY='<v>' (not on or off: counts as off)`.
    pub source: String,
}

impl Switch {
    /// `on` | `off`.
    pub fn text(&self) -> &'static str {
        if self.on {
            "on"
        } else {
            "off"
        }
    }
}

/// `Get-TelemetrySwitch`: a run's `--telemetry on|off` (`override_`) wins; else
/// `CODEX_CONSULT_TELEMETRY` - unset or empty: ON (the default); `on`, `1`, `true`, `yes`: on;
/// `off`, `0`, `false`, `no`, `none`: off; ANY other value counts as off (a switch that cannot be
/// read never sends).
pub fn switch(override_: Option<bool>) -> Switch {
    if let Some(on) = override_ {
        return Switch {
            on,
            source: "-Telemetry".into(),
        };
    }
    let v = std::env::var("CODEX_CONSULT_TELEMETRY")
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    if v.is_empty() {
        return Switch {
            on: true,
            source: "the default".into(),
        };
    }
    if ["on", "1", "true", "yes"].contains(&v.as_str()) {
        return Switch {
            on: true,
            source: "CODEX_CONSULT_TELEMETRY".into(),
        };
    }
    if ["off", "0", "false", "no", "none"].contains(&v.as_str()) {
        return Switch {
            on: false,
            source: "CODEX_CONSULT_TELEMETRY".into(),
        };
    }
    Switch {
        on: false,
        source: format!("CODEX_CONSULT_TELEMETRY='{v}' (not on or off: counts as off)"),
    }
}

/// Whether telemetry is enabled for a run ([`switch`] with the run's override).
pub fn is_enabled(config: &Config) -> bool {
    switch(config.telemetry).on
}

/// The dry-run status sentence.
pub fn status(config: &Config) -> String {
    let sw = switch(config.telemetry);
    if sw.on {
        format!(
            "telemetry: on (spool {} pending)",
            default_spool().pending()
        )
    } else if sw.source == "-Telemetry" {
        "telemetry: off (--telemetry off)".to_string()
    } else if sw.source == "CODEX_CONSULT_TELEMETRY" {
        "telemetry: off (CODEX_CONSULT_TELEMETRY=off)".to_string()
    } else {
        format!("telemetry: off ({})", sw.source)
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

/// The first-run notice's marker (`~/.codex/c3/telemetry/notice-shown`, the REAL home).
pub fn notice_marker() -> PathBuf {
    real_home_telemetry_dir().join("notice-shown")
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

/// True when the environment switch alone (no run override) disables telemetry - any value that
/// is not on counts as off ([`switch`]). The priors download and the router read it.
pub(crate) fn env_off() -> bool {
    !switch(None).on
}

// --------------------------------------------------------------------------- record

/// Build the event for one consultation and enqueue it to the spool when the run's switch is on
/// ([`is_enabled`] with `config`, the run's `--telemetry`); return the payload. The consult flow
/// ignores failures.
pub fn record_consultation(
    entry: &LedgerEntry,
    panel_size: Option<u32>,
    config: &Config,
) -> Result<serde_json::Value> {
    if !is_enabled(config) {
        return Err(Error::new("telemetry is off"));
    }
    let mut payload = serde_json::Value::Null;
    default_spool().enqueue_built(std::time::Duration::from_secs(5), || {
        let event = Event::from_ledger(entry, panel_size, &instance_id());
        payload = serde_json::to_value(&event)?;
        Ok(serde_json::to_string(&event)?)
    })?;
    Ok(payload)
}

/// Build the rating event of a COMMITTED mark (`c3 findings --rate`, `c3 telemetry
/// --backfill-ratings`) and enqueue it; `Err(why)` when it was not spooled (the caller counts it
/// and leaves the mark for the backfill). The caller decides the switch. `wait` bounds the wait
/// for the spool (the commit's 1 s, the retry's and the backfill's 5 s).
pub fn record_rating(
    input: &RatingInput<'_>,
    wait: std::time::Duration,
) -> Result<serde_json::Value> {
    let mut payload = serde_json::Value::Null;
    default_spool().enqueue_built(wait, || {
        let event = RatingEvent::from_rating(input, &instance_id());
        payload = serde_json::to_value(&event)?;
        Ok(serde_json::to_string(&event)?)
    })?;
    Ok(payload)
}

/// A mark's `when` (or any time C3 and the plugin write) as an instant; `None` when it does not
/// parse.
pub fn parse_mark_when(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    event::parse_when(s)
}

/// Count an event that could not be spooled (`Add-TelemetryNotSpooled`; wave 3b, E2): one NDJSON
/// line `{time, why}` appended - without any lock - to THIS process's own file
/// `<telemetry dir>/telemetry-not-spooled-<pid>-<start ticks>.ndjson`; `c3 telemetry --status`
/// sums every producer's file, a flush folds the files of gone producers into the last flush's
/// record. Never fails the caller ([`note_not_spooled_checked`] says why a line was not written).
/// (wave 3d, F24-3) Nothing is written while the local telemetry data is being deleted (a deletion
/// transaction in any phase, a living owner's forgetting marker): the event is dropped, as the
/// deletion would remove its count anyway.
pub fn note_not_spooled(why: &str) {
    let _ = note_not_spooled_checked(why);
}

/// [`note_not_spooled`], `Err(why)` when the line could not be written.
pub fn note_not_spooled_checked(why: &str) -> std::result::Result<(), String> {
    notspooled::append(&local_paths(&telemetry_dir()), why)
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
    instance_of_salt(&read_or_create_salt(dir))
}

/// The instance id a salt names on this machine.
fn instance_of_salt(salt: &str) -> String {
    let host = machine_name();
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(host.as_bytes());
    to_hex(&hasher.finalize())
}

/// What the salt file says about this installation's identity, read without creating anything
/// (wave 2g, F19-2): a deletion and a complaint must tell a salt that is not there from one that is
/// there but cannot be read - the second may still name the deleted instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SaltIdentity {
    /// No salt file.
    Missing,
    /// A salt file that names no instance (empty, or not UTF-8): a producer replaces it with a new
    /// salt, so no identity can come back from it.
    Blank,
    /// The salt file exists but could not be read (another handle denies sharing, permissions,
    /// ...): its identity is UNKNOWN - neither absent nor another instance. The why, one line.
    Unreadable(String),
    /// The instance id the salt names.
    Id(String),
}

/// The [`SaltIdentity`] of `dir`'s salt.
pub(crate) fn salt_identity_in(dir: &Path) -> SaltIdentity {
    match std::fs::read(dir.join("salt")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SaltIdentity::Missing,
        Err(e) => SaltIdentity::Unreadable(c3_core::one_line(&e.to_string())),
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) if !s.trim().is_empty() => SaltIdentity::Id(instance_of_salt(s.trim())),
            _ => SaltIdentity::Blank,
        },
    }
}

/// The stable, anonymous installation id if it has already been created, without creating
/// one. `None` when no salt has been written yet — this call itself writes nothing, unlike
/// [`instance_id`]. Uses the default directory.
pub fn instance_id_if_exists() -> Option<String> {
    instance_id_if_exists_in(&telemetry_dir())
}

/// [`instance_id_if_exists`] with an explicit directory (for tests).
/// `None` also for a salt that cannot be read: a caller that must tell that one from a missing salt
/// reads `salt_identity_in` (F19-2).
pub fn instance_id_if_exists_in(dir: &Path) -> Option<String> {
    match salt_identity_in(dir) {
        SaltIdentity::Id(id) => Some(id),
        _ => None,
    }
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

/// The machine name, from the same environment the reference client reads (plus, off Windows,
/// `/etc/hostname` when the shell exports no `HOSTNAME`).
fn machine_name() -> String {
    c3_core::host::machine_name()
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

/// The intake as resolved: its base URL (no trailing slash; empty when refused), where it comes
/// from, and why it is refused (empty when usable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hub {
    pub base: String,
    pub source: String,
    pub error: String,
}

/// The intake (`Get-TelemetryUrl`, plus C3's own override): `C3_TELEMETRY_HUB` (C3's staging/test
/// override, taken as given), else `CODEX_CONSULT_TELEMETRY_URL` (the operator setting the plugin
/// reads too), else `https://xelth.com/T`. `CODEX_CONSULT_TELEMETRY_URL` must be an absolute https
/// URL; plain http only for a LOOPBACK host (127.0.0.1, localhost, ::1) AND with
/// `CODEX_CONSULT_TEST_MODE=1` (a harness's local intake) - anything else is refused (nothing is
/// sent).
pub fn hub() -> Hub {
    if let Some(h) = std::env::var("C3_TELEMETRY_HUB")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        return Hub {
            base: h.trim().trim_end_matches('/').to_string(),
            source: "C3_TELEMETRY_HUB".into(),
            error: String::new(),
        };
    }
    let raw = std::env::var("CODEX_CONSULT_TELEMETRY_URL")
        .unwrap_or_default()
        .trim()
        .to_string();
    let (raw, source) = if raw.is_empty() {
        (HUB_DEFAULT.to_string(), "the default".to_string())
    } else {
        (raw, "CODEX_CONSULT_TELEMETRY_URL".to_string())
    };
    let refused = |error: String| Hub {
        base: String::new(),
        source: source.clone(),
        error,
    };
    let parsed = match url::Url::parse(&raw) {
        Ok(u) if u.host_str().is_some_and(|h| !h.is_empty()) => u,
        _ => {
            return refused(format!(
                "the intake URL '{raw}' ({source}) is not an absolute URL"
            ))
        }
    };
    if parsed.scheme() != "https" {
        let loopback = match parsed.host() {
            Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        if !(parsed.scheme() == "http" && loopback) {
            return refused(format!("the intake URL '{raw}' ({source}) is refused: only https (plain http only for a loopback test intake in test mode)"));
        }
        if !c3_core::test_hooks::mode_on() {
            return refused(format!("the intake URL '{raw}' ({source}) is refused: plain http to a loopback intake only with CODEX_CONSULT_TEST_MODE=1 (a harness); a real intake is https"));
        }
    }
    Hub {
        base: raw.trim_end_matches('/').to_string(),
        source,
        error: String::new(),
    }
}

/// The hub base URL the senders use ([`hub`]); empty when the configured intake is refused (a
/// spool then sends nothing).
pub(crate) fn default_hub() -> String {
    hub().base
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

/// A summary built only from classes and numbers (never a label, a brief, a thread or a path).
fn summarize_entry(entry: &LedgerEntry) -> String {
    // (wave 2c, F02-2) classes only - a private label never reaches a complaint's context either
    let (engine, _, _) = classes::reviewer_class(entry);
    let purpose = classes::purpose_class(&entry.purpose);
    let verdict = classes::verdict_class(&entry.verdict);
    let findings =
        entry.findings.blocker + entry.findings.major + entry.findings.minor + entry.findings.note;
    format!(
        "consult n={} purpose={} engine={} verdict={} findings={} wall={:.0}s",
        entry.n, purpose, engine, verdict, findings, entry.wall_seconds
    )
}
