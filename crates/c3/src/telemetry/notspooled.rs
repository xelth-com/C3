//! (wave 3b of the 0.6.1 parity; the plugin's 0.6.0 wave 28e decisions E2, E3, E20, E24, E26) The
//! count of events that could not be spooled, its fold into the last flush's record, and the
//! forgetting marker - the plugin's `Add-TelemetryNotSpooled`, `Get-TelemetryNotSpooled`,
//! `Merge-TelemetryNotSpooled`, `Complete-TelemetryNotSpooledFold`, `Get-TelemetryForgettingOwner`,
//! `Resolve-TelemetryForgetting`, `Add-TelemetryLastNote`, `Get-ProcessStartTicks` and
//! `Get-PidIdentityTicks`, with the plugin's file names and shapes, under C3's OWN telemetry root
//! (`<codex home>/c3/telemetry/`, P7 of the parity plan):
//!
//! - **E2** one file per producer process, `telemetry-not-spooled-<pid>-<start ticks>.ndjson` (the
//!   pid and the process's start time in .NET ticks: 100 ns since 0001-01-01 UTC), one `{time, why}`
//!   line per event, appended WITHOUT any lock (no other process appends to it); the legacy single
//!   files - the plugin's `telemetry-not-spooled.ndjson` and C3's own `not-spooled.ndjson` of wave 2 -
//!   are counted too. `--status` sums the complete lines of every file less the last flush's
//!   `not_spooled_seen`; a flush folds the files of GONE producers (no process with that pid and
//!   those ticks) and the legacy files into ONE note of the last flush's record (`folded <n>
//!   not-spooled line(s) of <m> gone producer(s)`) and keeps a live producer's file, recording its
//!   lines as `not_spooled_seen`.
//! - **E20** the fold saves before it deletes: the gone files are held open exclusively (delete
//!   sharing only), counted, the record SAVED with the note and `not_spooled_folded[]`, and only
//!   then deleted under the held handles; the record is then written once more without the names of
//!   the files now gone. A file `not_spooled_folded[]` already names (a crash between the save and
//!   the deletes) is deleted without being counted again; a record that cannot be saved folds
//!   nothing (the caller warns).
//! - **E24** `not_spooled_folded[]` entries are `{name, bytes}`: a named file of the recorded length
//!   is deleted uncounted, a LONGER one has its complete lines beyond the recorded bytes counted, a
//!   SHORTER one is another file under that name (folded afresh), a bare name counts nothing.
//! - **E26** a legacy file is never counted under its own name: the fold first renames it to a
//!   unique staged name `telemetry-not-spooled-legacy-<utc ticks>.ndjson` (retried about 1 s while a
//!   writer holds it - then skipped this flush with a note).
//! - **E3** the forgetting marker `telemetry-forgetting` `{pid, start_time, start_ticks, since}`: its
//!   owner is judged on the pid AND the start ticks when it has them (exactly equal on Windows; a
//!   start that cannot be read counts as alive), an older marker by its `start_time`.
//! - **wave 3d** (the diff-review F24-1..F24-6): a line is never "seen" unless a fold would keep
//!   its file (a flush without the spool lock, a gone file the fold cannot open); a named file's
//!   tail without a line end is counted; no count is written while the local data is being deleted
//!   (the file made first, the deletion checked after); `--status` names the latest UNSEEN line; a
//!   staged generation never takes a name the record holds.
//!
//! The last flush's record is C3's `last-flush.json` (the plugin's `.last`), in the plugin's shape
//! `{time, result, delivered, kept, dropped, rejected, http, not_spooled_seen, not_spooled_folded,
//! notes}`.
//!
//! TEST HOOK (test mode only, [`PLUGIN_HOME_VAR`]): the not-spooled files, the marker and the
//! record at the PLUGIN's places under a codex home (`<home>/telemetry-not-spooled-*.ndjson`,
//! `<home>/telemetry-forgetting`, `<home>/telemetry-spool/.last`), so the plugin's harnesses run C3's
//! count, fold and marker on the plugin's own files (the shims set it in test mode). C3's outbox,
//! salt and locks stay under its own root either way.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::telemetry::complaint::{deletion_state, DeletionState};
use crate::telemetry::spool::{replace_atomic, FORGET_PENDING, LAST_FLUSH};

/// The producers' files: `telemetry-not-spooled-<pid>-<start ticks>.ndjson`.
const NS_PREFIX: &str = "telemetry-not-spooled-";
/// The plugin's legacy single file (written before its wave 28e).
pub(crate) const LEGACY_FILE: &str = "telemetry-not-spooled.ndjson";
/// C3's legacy single file (wave 2: one `{time, why}` line per event, reset by a flush).
pub(crate) const C3_LEGACY_FILE: &str = "not-spooled.ndjson";
/// A legacy file's staged generations: `telemetry-not-spooled-legacy-<utc ticks>.ndjson`.
const STAGED_PREFIX: &str = "telemetry-not-spooled-legacy-";
/// The forgetting marker a local deletion writes while it runs.
pub(crate) const MARKER_FILE: &str = "telemetry-forgetting";
/// TEST HOOK (test mode only): a codex home whose plugin places hold the not-spooled files, the
/// marker and the last flush's record (see the module docs).
pub const PLUGIN_HOME_VAR: &str = "C3_TEST_TELEMETRY_PLUGIN_HOME";
/// The last flush's record keeps its last 10 notes (`$script:TelemetryLastNotesMax`).
const NOTES_MAX: usize = 10;
/// .NET ticks (100 ns since 0001-01-01) of the Unix epoch.
const DOTNET_UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
/// One second in ticks (the start-time jitter allowed outside Windows).
const TICKS_PER_SECOND: i64 = 10_000_000;

// --------------------------------------------------------------------------- paths

/// Where the not-spooled files, the forgetting marker and the last flush's record live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPaths {
    /// The directory of the not-spooled files.
    pub ns_dir: PathBuf,
    /// The forgetting marker.
    pub marker: PathBuf,
    /// The last flush's record (`last-flush.json`; the plugin's `.last`).
    pub last: PathBuf,
    /// C3's own telemetry root - the outbox, the locks and the deletion transaction
    /// (`forget-pending.json`) stay there under the test hook too (wave 3d, F24-3: a producer reads
    /// the transaction before it writes a count).
    pub root: PathBuf,
}

impl LocalPaths {
    /// C3's layout: everything in its telemetry directory.
    pub fn in_dir(dir: &Path) -> Self {
        LocalPaths {
            ns_dir: dir.to_path_buf(),
            marker: dir.join(MARKER_FILE),
            last: dir.join(LAST_FLUSH),
            root: dir.to_path_buf(),
        }
    }

    /// The plugin's places under a codex home (the test hook [`PLUGIN_HOME_VAR`]); `root` is C3's
    /// own telemetry directory.
    pub fn plugin_home(home: &Path, root: &Path) -> Self {
        LocalPaths {
            ns_dir: home.to_path_buf(),
            marker: home.join(MARKER_FILE),
            last: home.join("telemetry-spool").join(".last"),
            root: root.to_path_buf(),
        }
    }

    /// THIS process's not-spooled file (`NotSpooledOwn`).
    pub fn own_file(&self) -> PathBuf {
        self.ns_dir.join(format!(
            "{NS_PREFIX}{}-{}.ndjson",
            std::process::id(),
            own_start_ticks()
        ))
    }
}

/// The paths of the telemetry directory `dir`: C3's layout, or - in test mode with
/// [`PLUGIN_HOME_VAR`] set - the plugin's places under that home.
pub fn local_paths(dir: &Path) -> LocalPaths {
    if c3_core::test_hooks::mode_on() {
        if let Some(h) = std::env::var_os(PLUGIN_HOME_VAR).filter(|h| !h.is_empty()) {
            return LocalPaths::plugin_home(Path::new(&h), dir);
        }
    }
    LocalPaths::in_dir(dir)
}

// --------------------------------------------------------------------------- process identity

/// The process's start time as a UTC round-trip string ([`crate::liveness::proc::process_start_iso`])
/// with the plugin's TEST HOOK (test mode only) `CODEX_CONSULT_TEST_START_UNREADABLE=<pid>[,<pid>]`:
/// those pids, while they exist, read as `""` (a start time that cannot be read).
fn start_iso_hooked(pid: u32) -> Option<String> {
    let iso = crate::liveness::proc::process_start_iso(pid)?;
    if let Some(list) = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_START_UNREADABLE") {
        if list.split(',').any(|p| p.trim() == pid.to_string()) {
            return Some(String::new());
        }
    }
    Some(iso)
}

/// A UTC round-trip time as .NET ticks (100 ns since 0001-01-01 UTC); `None` when it does not parse.
fn iso_ticks(iso: &str) -> Option<i64> {
    let t = iso.trim();
    if t.is_empty() {
        return None;
    }
    let d = DateTime::parse_from_rfc3339(t).ok()?.with_timezone(&Utc);
    Some(
        d.timestamp() * TICKS_PER_SECOND
            + i64::from(d.timestamp_subsec_nanos()) / 100
            + DOTNET_UNIX_EPOCH_TICKS,
    )
}

/// `Get-ProcessStartTicks`: a process's start time in ticks (UTC, 100 ns - the full resolution
/// Windows reports): `None` - no such process; `Some(-1)` - it exists but its start time cannot be
/// read; else the ticks.
pub fn process_start_ticks(pid: u32) -> Option<i64> {
    if pid == 0 {
        return None;
    }
    let iso = start_iso_hooked(pid)?;
    Some(iso_ticks(&iso).unwrap_or(-1))
}

/// The identity of a pid against the start ticks recorded for it (`Get-PidIdentityTicks`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity {
    /// A process with this pid runs with exactly those ticks (outside Windows: within a second).
    Alive,
    /// No such process, or one with another start (the pid was handed to another process).
    Gone,
    /// No ticks were recorded, or the start time cannot be read now - never removed on a guess.
    Unknown,
}

/// `Get-PidIdentityTicks`.
pub fn pid_identity_ticks(pid: u32, ticks: i64) -> Identity {
    if pid == 0 {
        return Identity::Gone;
    }
    let Some(live) = process_start_ticks(pid) else {
        return Identity::Gone;
    };
    if ticks <= 0 || live < 0 {
        return Identity::Unknown;
    }
    if live == ticks {
        return Identity::Alive;
    }
    if !cfg!(windows) && (live - ticks).abs() < TICKS_PER_SECOND {
        return Identity::Alive;
    }
    Identity::Gone
}

/// This process's start ticks, read once; 0 when they cannot be read (its file then names no
/// start: its identity counts as unknown, never folded).
pub fn own_start_ticks() -> i64 {
    static OWN: OnceLock<i64> = OnceLock::new();
    *OWN.get_or_init(|| match process_start_ticks(std::process::id()) {
        Some(t) if t > 0 => t,
        _ => 0,
    })
}

/// `Test-PidAlive` with the start-unreadable hook: a process with this pid exists and - when a
/// start time was recorded and one can be read now - has that start time.
fn pid_alive_hooked(pid: u32, start_time: &str) -> bool {
    if pid == 0 {
        return false;
    }
    let Some(live) = start_iso_hooked(pid) else {
        return false;
    };
    if !start_time.is_empty()
        && !live.is_empty()
        && !crate::liveness::proc::same_start_time(&live, start_time)
    {
        return false;
    }
    true
}

// --------------------------------------------------------------------------- small helpers

/// The local time as the plugin's `Get-IsoTimestamp` writes it.
pub(crate) fn now_iso() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// `Get-TelemetryCount`: a whole number >= 0 (a JSON number or its digits), else `None`.
fn count_value(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64))
            .filter(|n| *n >= 0),
        Value::String(s) => {
            let t = s.trim();
            if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) {
                t.parse::<i64>().ok()
            } else {
                None
            }
        }
        _ => None,
    }
}

/// A JSON value as text (`ConvertTo-JsonText`): a string as it is, null as `""`, else its JSON.
fn value_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// A file's text read with every sharing (`Read-SharedText`); `None` when it cannot be read.
fn read_shared(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// `Read-SharedTextFrom`: the text from byte `offset` on; `None` when the file is shorter than
/// `offset` (another file under that name) or cannot be read.
fn read_shared_from(path: &Path, offset: u64) -> Option<String> {
    let mut f = File::open(path).ok()?;
    if f.metadata().ok()?.len() < offset {
        return None;
    }
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut b = Vec::new();
    f.read_to_end(&mut b).ok()?;
    Some(String::from_utf8_lossy(&b).into_owned())
}

/// `Get-TelemetryNotSpooledLines`: the complete lines of a not-spooled text (an append in progress
/// has no line end yet); `tail`: a last piece without one counts too (a gone producer's file).
fn complete_lines(text: &str, tail: bool) -> Vec<&str> {
    let t = if tail {
        text
    } else {
        match text.rfind('\n') {
            Some(i) => &text[..i],
            None => "",
        }
    };
    t.split('\n').filter(|l| !l.trim().is_empty()).collect()
}

// --------------------------------------------------------------------------- E2: the files

/// One not-spooled file (`Get-TelemetryNotSpooledFiles`).
#[derive(Debug, Clone)]
pub(crate) struct NsFile {
    pub path: PathBuf,
    pub name: String,
    /// The producer's pid (0: a legacy or a staged file).
    pub pid: u32,
    /// The producer's start ticks (a staged file: its staging ticks).
    pub ticks: i64,
    /// A legacy single file (the plugin's or C3's wave-2 name).
    pub legacy: bool,
    /// A legacy file's staged generation.
    pub staged: bool,
}

/// The not-spooled files of `dir`: the producers' files, the legacy single files and their staged
/// generations, sorted by name (case-insensitively). Another name that starts the same way is not
/// one of them.
pub(crate) fn list_files(dir: &Path) -> Vec<NsFile> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let Some(name) = e.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        let path = e.path();
        if lower == LEGACY_FILE || lower == C3_LEGACY_FILE {
            out.push(NsFile {
                path,
                name,
                pid: 0,
                ticks: 0,
                legacy: true,
                staged: false,
            });
            continue;
        }
        let Some(rest) = lower
            .strip_prefix(NS_PREFIX)
            .and_then(|r| r.strip_suffix(".ndjson"))
        else {
            continue;
        };
        let digits = |s: &str, max: usize| {
            !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit())
        };
        if let Some(t) = rest.strip_prefix("legacy-") {
            if digits(t, 19) {
                if let Ok(ticks) = t.parse::<i64>() {
                    out.push(NsFile {
                        path,
                        name,
                        pid: 0,
                        ticks,
                        legacy: false,
                        staged: true,
                    });
                }
            }
            continue;
        }
        let Some((p, t)) = rest.split_once('-') else {
            continue;
        };
        if !digits(p, 10) || !digits(t, 19) {
            continue;
        }
        let (Ok(pid), Ok(ticks)) = (p.parse::<u64>(), t.parse::<i64>()) else {
            continue;
        };
        if pid > i32::MAX as u64 {
            continue;
        }
        out.push(NsFile {
            path,
            name,
            pid: pid as u32,
            ticks,
            legacy: false,
            staged: false,
        });
    }
    out.sort_by_key(|f| f.name.to_ascii_lowercase());
    out
}

/// `Add-TelemetryNotSpooled`: one `{time, why}` line appended to THIS process's own file - no lock
/// (no other process appends to it; the retry, up to 1 s, only covers a reader of this very file).
/// `Err(why)` when the line could not be written.
///
/// (wave 3d, F24-3) Never while the local telemetry data is being deleted: the own file is opened
/// (made) FIRST and only then is the deletion checked ([`deletion_refusal`]: a deletion transaction
/// in any phase, or a forgetting marker whose owner lives) - so a check that passes means the file
/// existed before any deletion began, and that deletion's cleanup (which lists the files only after
/// its transaction and marker are written) removes it; a refused append writes nothing and removes
/// the file it made (an empty one: a file with lines is the cleanup's). Still no lock: a busy lock
/// never stops a count (fixes28d D4).
pub fn append(paths: &LocalPaths, why: &str) -> Result<(), String> {
    let line = format!(
        "{}\n",
        json!({ "time": now_iso(), "why": c3_core::one_line(why) })
    );
    let own = paths.own_file();
    let mut err = String::new();
    for _ in 0..20 {
        let opened = fs::create_dir_all(&paths.ns_dir)
            .and_then(|_| OpenOptions::new().create(true).append(true).open(&own));
        let r = match opened {
            Ok(mut f) => {
                if let Some(refused) = deletion_refusal(paths) {
                    let empty = f.metadata().map(|m| m.len() == 0).unwrap_or(false);
                    drop(f);
                    if empty {
                        let _ = fs::remove_file(&own);
                    }
                    return Err(format!(
                        "the not-spooled count {} is not written while the local telemetry data is being deleted ({refused})",
                        own.display()
                    ));
                }
                f.write_all(line.as_bytes())
            }
            Err(e) => Err(e),
        };
        match r {
            Ok(()) => return Ok(()),
            Err(e) => {
                err = c3_core::one_line(&e.to_string());
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Err(format!(
        "the not-spooled count {} could not be written ({err})",
        own.display()
    ))
}

/// (wave 3d, F24-3) Why no not-spooled line may be written now: a deletion transaction of this
/// instance in ANY phase (or one that cannot be read - fail closed: pending at the intake, confirmed,
/// cleaning) or a forgetting marker whose owner lives (a local deletion runs). `None`: write.
pub(crate) fn deletion_refusal(paths: &LocalPaths) -> Option<String> {
    match deletion_state(&paths.root) {
        DeletionState::None => {}
        DeletionState::Pending => {
            return Some(format!(
                "a deletion of this instance is pending at the intake ({})",
                paths.root.join(FORGET_PENDING).display()
            ))
        }
        DeletionState::Cleanup(_) => {
            return Some(format!(
                "the local deletion of this instance is not finished ({})",
                paths.root.join(FORGET_PENDING).display()
            ))
        }
    }
    let m = forgetting_owner(&paths.marker);
    (m.there && m.alive).then_some(m.why)
}

// --------------------------------------------------------------------------- the last record

/// The last flush's record, or `None` (none, or it cannot be read).
pub(crate) fn read_last(path: &Path) -> Option<Value> {
    let t = read_shared(path)?;
    serde_json::from_str(&t).ok()
}

/// One `{name, bytes}` entry of `not_spooled_folded[]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldedEntry {
    pub name: String,
    /// The file's length when the fold counted it (-1: a bare name, the length unknown).
    pub bytes: i64,
}

impl FoldedEntry {
    fn to_value(&self) -> Value {
        json!({ "name": self.name, "bytes": self.bytes })
    }
}

/// The entries as the record's `not_spooled_folded` array.
pub(crate) fn folded_value(entries: &[FoldedEntry]) -> Value {
    Value::Array(entries.iter().map(FoldedEntry::to_value).collect())
}

/// `Get-TelemetryFoldedMap`: a record's `not_spooled_folded[]` as name (lower-cased: file names are
/// case-insensitive on Windows) -> (the name as written, bytes); a bare name maps to -1.
pub(crate) fn folded_map(last: Option<&Value>) -> HashMap<String, FoldedEntry> {
    let mut m = HashMap::new();
    let items: Vec<&Value> = match last.and_then(|l| l.get("not_spooled_folded")) {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().collect(),
        Some(other) => vec![other],
    };
    for e in items {
        let (name, bytes) = match e {
            Value::String(s) if !s.is_empty() => (s.clone(), -1),
            Value::Object(o) => {
                let name = value_text(o.get("name"));
                if name.is_empty() {
                    continue;
                }
                let bytes = match o.get("bytes") {
                    Some(Value::Number(n)) => n
                        .as_i64()
                        .or_else(|| n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64))
                        .unwrap_or(-1),
                    Some(Value::String(s)) => s.trim().parse::<i64>().unwrap_or(-1),
                    _ => -1,
                };
                (name, bytes)
            }
            _ => continue,
        };
        m.insert(name.to_ascii_lowercase(), FoldedEntry { name, bytes });
    }
    m
}

/// The notes of a record (non-empty strings).
pub(crate) fn notes_of(last: Option<&Value>) -> Vec<String> {
    let items: Vec<&Value> = match last.and_then(|l| l.get("notes")) {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().collect(),
        Some(other) => vec![other],
    };
    items
        .into_iter()
        .map(|v| value_text(Some(v)))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Write a record (pretty JSON, atomically), making its directory.
pub(crate) fn write_last(path: &Path, v: &Value) -> crate::telemetry::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = format!("{}\n", serde_json::to_string_pretty(v)?);
    replace_atomic(path, text.as_bytes())
}

/// `Add-TelemetryLastNote`: one line `<time> <text>` in the record's `notes` - a text already there
/// is not added again; the last 10 lines are kept; the record's other fields stay. The caller holds
/// the spool lock (every writer of the record does). Best effort.
pub(crate) fn add_last_note(paths: &LocalPaths, text: &str) {
    if text.is_empty() {
        return;
    }
    let before = read_last(&paths.last);
    let mut o = before
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut notes = notes_of(before.as_ref());
    let tail = format!(" {text}");
    if !notes.iter().any(|n| n.ends_with(&tail)) {
        notes.push(format!("{} {text}", now_iso()));
    }
    let skip = notes.len().saturating_sub(NOTES_MAX);
    o.insert(
        "notes".into(),
        Value::Array(notes.into_iter().skip(skip).map(Value::String).collect()),
    );
    let _ = write_last(&paths.last, &Value::Object(o));
}

/// The last 10 of `notes`.
pub(crate) fn last_notes(notes: Vec<String>) -> Vec<String> {
    let skip = notes.len().saturating_sub(NOTES_MAX);
    notes.into_iter().skip(skip).collect()
}

// --------------------------------------------------------------------------- E2, E20, E24: the count

/// The events not spooled since the last flush (`Get-TelemetryNotSpooled`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NsCount {
    /// Since the last flush: every complete line less the record's `not_spooled_seen`.
    pub count: u64,
    /// The latest `why` by time (`""` when none since the last flush).
    pub last: String,
    /// Its `time` as written.
    pub when: String,
    /// Every complete line of every file counted.
    pub total: u64,
    /// The files counted.
    pub files: u64,
    /// The legacy files' complete lines (part of `total`).
    pub legacy_lines: u64,
}

/// `Get-TelemetryNotSpooled`: the complete lines of every not-spooled file, summed, less the last
/// flush's `not_spooled_seen` (`all`: every line counts). A file the record's `not_spooled_folded[]`
/// names counts only its complete lines BEYOND the recorded bytes (those are in a fold's note
/// already); one shorter than that is another file and counts whole; a bare name counts nothing. A
/// legacy file counts whole, always (a fold stages it before counting it).
pub fn count(paths: &LocalPaths, all: bool) -> NsCount {
    let mut r = NsCount::default();
    let files = list_files(&paths.ns_dir);
    if files.is_empty() {
        return r;
    }
    let last = read_last(&paths.last);
    let folded = folded_map(last.as_ref());
    let seen = if all {
        0
    } else {
        count_value(last.as_ref().and_then(|l| l.get("not_spooled_seen"))).unwrap_or(0)
    };
    // (wave 3d, F24-4) the lines that can have been seen - a producer's file the record does not
    // name, line by line in its order - and those that never are (a legacy file, a staged
    // generation, a named file's lines beyond its recorded bytes)
    let mut seeable: Vec<Vec<NsLine>> = Vec::new();
    let mut never: Vec<NsLine> = Vec::new();
    for f in &files {
        let mut text = None;
        if !f.legacy {
            if let Some(e) = folded.get(&f.name.to_ascii_lowercase()) {
                if e.bytes < 0 {
                    continue;
                }
                text = read_shared_from(&f.path, e.bytes as u64);
            }
        }
        let named = text.is_some();
        let text = match text.or_else(|| read_shared(&f.path)) {
            Some(t) => t,
            None => continue,
        };
        let lines = complete_lines(&text, false);
        // a named file with nothing beyond its recorded bytes is accounted for
        if named && lines.is_empty() {
            continue;
        }
        r.files += 1;
        r.total += lines.len() as u64;
        if f.legacy {
            r.legacy_lines += lines.len() as u64;
        }
        let parsed: Vec<NsLine> = lines.into_iter().map(NsLine::parse).collect();
        if f.legacy || f.staged || named {
            never.extend(parsed);
        } else {
            seeable.push(parsed);
        }
    }
    r.count = (r.total as i64 - seen).max(0) as u64;
    if r.count > 0 {
        if let Some(l) = latest_unseen(seeable, never, seen.max(0) as u64) {
            r.last = l.why;
            r.when = l.when;
        }
    }
    r
}

/// One not-spooled line as `--status` reads it: its `time` (parsed, and as written) and its `why`;
/// a line that is no JSON object has neither (it still counts).
#[derive(Debug, Clone)]
struct NsLine {
    at: Option<DateTime<chrono::FixedOffset>>,
    when: String,
    why: String,
    json: bool,
}

impl NsLine {
    fn parse(l: &str) -> NsLine {
        match serde_json::from_str::<Value>(l) {
            Ok(o) => {
                let when = value_text(o.get("time"));
                NsLine {
                    at: DateTime::parse_from_rfc3339(when.trim()).ok(),
                    why: value_text(o.get("why")),
                    when,
                    json: true,
                }
            }
            Err(_) => NsLine {
                at: None,
                when: String::new(),
                why: String::new(),
                json: false,
            },
        }
    }
}

/// (wave 3d, F24-4) The latest line by time among the lines NOT seen by the last flush. The record
/// keeps one number (`not_spooled_seen`: the complete lines of the producers' files that flush kept),
/// so the seen lines are taken from the front of those files - each file in its own order (they only
/// grow), the files interleaved by time (the earliest first; a line without a time first) -, and
/// every line of a legacy file, of a staged generation and beyond a named file's recorded bytes is
/// unseen. Exact for one producer whatever the clock did (a seen line dated later than a new one is
/// not named) and for several under a clock that only moves forward; the one approximation left: a
/// producer that was gone already at the last flush but not folded (a flush without the spool lock)
/// may have its older lines taken for the seen ones - the COUNT is exact either way.
fn latest_unseen(seeable: Vec<Vec<NsLine>>, never: Vec<NsLine>, seen: u64) -> Option<NsLine> {
    let mut heads = vec![0usize; seeable.len()];
    for _ in 0..seen {
        let mut pick: Option<usize> = None;
        for (i, f) in seeable.iter().enumerate() {
            let Some(l) = f.get(heads[i]) else {
                continue;
            };
            pick = match pick {
                None => Some(i),
                Some(p) => {
                    let best = &seeable[p][heads[p]];
                    let earlier = match (l.at, best.at) {
                        (None, Some(_)) => true,
                        (Some(a), Some(b)) => a < b,
                        _ => false,
                    };
                    Some(if earlier { i } else { p })
                }
            };
        }
        match pick {
            Some(i) => heads[i] += 1,
            None => break,
        }
    }
    let unseen = seeable
        .into_iter()
        .zip(heads)
        .flat_map(|(f, h)| f.into_iter().skip(h))
        .chain(never);
    // the plugin's rule: the latest by time; a line without a time only when none has one
    let mut latest: Option<NsLine> = None;
    for l in unseen.filter(|l| l.json) {
        let newer = match &latest {
            None => true,
            Some(prev) => l.at.is_some() && (prev.at.is_none() || l.at >= prev.at),
        };
        if newer {
            latest = Some(l);
        }
    }
    latest
}

// --------------------------------------------------------------------------- E2, E20, E24, E26: the fold

/// A fold in progress (`Merge-TelemetryNotSpooled`): the files it counted are held open until
/// [`Fold::complete`].
#[derive(Debug, Default)]
pub(crate) struct Fold {
    /// The lines folded now.
    pub lines: u64,
    /// The files folded now ("gone producers").
    pub producers: u64,
    /// The complete lines of the files kept (live producers) - the record's `not_spooled_seen`.
    pub seen: u64,
    /// Every file this fold covers, `{name, bytes}` - the record's `not_spooled_folded`.
    pub folded: Vec<FoldedEntry>,
    /// Why a legacy file was not staged (one note each).
    pub notes: Vec<String>,
    open: Vec<(PathBuf, File)>,
    ns_dir: PathBuf,
}

/// Open a file for the fold: read and write, sharing only delete (an appender waits; the fold can
/// still delete it under this handle).
fn open_exclusive(path: &Path) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    o.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_DELETE: u32 = 0x0000_0004;
        o.share_mode(FILE_SHARE_DELETE);
    }
    o.open(path)
}

/// UTC now in .NET ticks.
fn utc_now_ticks() -> i64 {
    let n = Utc::now();
    n.timestamp() * TICKS_PER_SECOND
        + i64::from(n.timestamp_subsec_nanos()) / 100
        + DOTNET_UNIX_EPOCH_TICKS
}

/// (E26) A staged generation's name from `ticks` on: the first that is neither on disk nor named by
/// the record's `not_spooled_folded[]` (wave 3d, F24-6: a record whose last rewrite failed still
/// names files the fold deleted - a new generation never takes such a name, so E24's `{name, bytes}`
/// can never mistake it for the folded one).
fn staged_path(dir: &Path, mut ticks: i64, taken: &HashMap<String, FoldedEntry>) -> PathBuf {
    loop {
        let name = format!("{STAGED_PREFIX}{ticks}.ndjson");
        let p = dir.join(&name);
        if !p.exists() && !taken.contains_key(&name.to_ascii_lowercase()) {
            return p;
        }
        ticks += 1;
    }
}

/// (E26) Rename a legacy file to a unique staged name before it is counted; `Err(note)` when a
/// writer kept it (retried about 1 s) - not folded this flush.
fn stage_legacy(
    dir: &Path,
    legacy: &Path,
    taken: &HashMap<String, FoldedEntry>,
) -> Result<(), String> {
    let staged = staged_path(dir, utc_now_ticks(), taken);
    let mut err = String::new();
    for _ in 0..10 {
        match fs::rename(legacy, &staged) {
            Ok(()) => return Ok(()),
            Err(e) => {
                err = c3_core::one_line(&e.to_string());
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    if legacy.exists() {
        return Err(format!(
            "the legacy not-spooled file {} could not be staged ({err}) - not folded this flush",
            legacy.display()
        ));
    }
    Ok(())
}

/// Whether a fold takes this (non-legacy) file: a file the record names, a staged generation, or a
/// producer that is gone (no process with its pid and start ticks). A producer that lives - or whose
/// identity cannot be confirmed - keeps its file.
fn foldable(f: &NsFile, already: &HashMap<String, FoldedEntry>) -> bool {
    already.contains_key(&f.name.to_ascii_lowercase())
        || f.staged
        || pid_identity_ticks(f.pid, f.ticks) == Identity::Gone
}

/// (wave 3d, F24-1) The record's `not_spooled_seen` of a flush that could not fold (the spool lock
/// stayed busy): the complete lines of the files a fold would KEEP (live producers - the fold's
/// `seen`), nothing else. The lines of a gone producer's file, a staged generation, a legacy file
/// or a named file stay unseen - the next fold counts them in its note, and `--status` counts them
/// meanwhile -, so no line is ever both "seen" and folded.
pub(crate) fn kept_seen(paths: &LocalPaths, already: &HashMap<String, FoldedEntry>) -> u64 {
    list_files(&paths.ns_dir)
        .iter()
        .filter(|f| !f.legacy && !foldable(f, already))
        .filter_map(|f| read_shared(&f.path))
        .map(|t| complete_lines(&t, false).len() as u64)
        .sum()
}

/// `Merge-TelemetryNotSpooled`, under the spool lock: every file whose producer is gone (no process
/// with its pid and start ticks), every staged legacy generation and every file `already` names is
/// opened exclusively and counted (a gone producer's last line without a line end too); a producer
/// that lives - or whose identity cannot be confirmed - keeps its file (its complete lines: `seen`).
/// The legacy files are staged first (E26). NOTHING is deleted here (E20): the caller saves the
/// record, then [`Fold::complete`] deletes. A file `already` names (E24) of the same length or
/// longer is not counted again - only its lines beyond the recorded bytes are new (wave 3d, F24-2:
/// a last piece without a line end too - the file is deleted, its producer is gone); a shorter one
/// is another file, folded afresh; a bare name is held for its delete, uncounted. (wave 3d, F24-1)
/// A file the fold takes but cannot open this time is neither counted nor seen: it counts in
/// `--status` until a later fold takes it.
pub(crate) fn merge(paths: &LocalPaths, already: &HashMap<String, FoldedEntry>) -> Fold {
    let mut r = Fold {
        ns_dir: paths.ns_dir.clone(),
        ..Fold::default()
    };
    for f in list_files(&paths.ns_dir).iter().filter(|f| f.legacy) {
        if let Err(note) = stage_legacy(&paths.ns_dir, &f.path, already) {
            r.notes.push(note);
        }
    }
    for f in list_files(&paths.ns_dir) {
        // (E26) a legacy file is never counted (nor seen) under its own name: staged above, or busy
        if f.legacy {
            continue;
        }
        let rec = already.get(&f.name.to_ascii_lowercase()).map(|e| e.bytes);
        if foldable(&f, already) {
            if let Ok(mut fs_) = open_exclusive(&f.path) {
                let mut earlier = false;
                let counted: Option<(u64, i64)> = (|| {
                    let len = fs_.metadata().ok()?.len() as i64;
                    let n;
                    if rec.is_some_and(|b| b < 0 || len >= b) {
                        earlier = true;
                        let b = rec.unwrap_or(-1);
                        n = if b >= 0 && len > b {
                            fs_.seek(SeekFrom::Start(b as u64)).ok()?;
                            let mut buf = Vec::new();
                            fs_.read_to_end(&mut buf).ok()?;
                            complete_lines(&String::from_utf8_lossy(&buf), true).len() as u64
                        } else {
                            0
                        };
                    } else {
                        let mut buf = Vec::new();
                        fs_.read_to_end(&mut buf).ok()?;
                        n = complete_lines(&String::from_utf8_lossy(&buf), true).len() as u64;
                    }
                    Some((n, len))
                })();
                if let Some((n, len)) = counted {
                    r.folded.push(FoldedEntry {
                        name: f.name.clone(),
                        bytes: len,
                    });
                    r.open.push((f.path.clone(), fs_));
                    if !earlier || n > 0 {
                        r.lines += n;
                        r.producers += 1;
                    }
                    continue;
                }
            }
            if let Some(b) = rec {
                // named and cannot be opened: it stays named, never counted as kept
                r.folded.push(FoldedEntry {
                    name: f.name.clone(),
                    bytes: b,
                });
            }
            // (wave 3d, F24-1) taken but not opened: neither folded nor seen - a later fold
            continue;
        }
        if let Some(t) = read_shared(&f.path) {
            r.seen += complete_lines(&t, false).len() as u64;
        }
    }
    r
}

impl Fold {
    /// `Complete-TelemetryNotSpooledFold`: `delete` - ONLY after the record was saved - deletes
    /// every file the fold holds (under its handle: nothing appends meanwhile); then every handle is
    /// closed. The entries whose file is still there afterwards (the record goes on naming them).
    pub(crate) fn complete(&mut self, delete: bool) -> Vec<FoldedEntry> {
        for (path, file) in self.open.drain(..) {
            if delete {
                let _ = fs::remove_file(&path);
            }
            drop(file);
        }
        self.folded
            .iter()
            .filter(|e| self.ns_dir.join(&e.name).exists())
            .cloned()
            .collect()
    }
}

// --------------------------------------------------------------------------- E3: the marker

/// The forgetting marker as it is (`Get-TelemetryForgettingOwner`) - read only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkerOwner {
    /// A marker is there.
    pub there: bool,
    /// Its owner's pid (0: none named).
    pub pid: u32,
    /// Since when (as written).
    pub since: String,
    /// Its owner lives - or its identity cannot be confirmed (never removed on a guess); a marker
    /// that names no owner is not alive.
    pub alive: bool,
    /// The refusal while it lives.
    pub why: String,
}

/// `Get-TelemetryForgettingOwner`: a marker that records `start_ticks` is judged on its pid AND
/// those ticks ([`pid_identity_ticks`]: `Unknown` counts as alive); an older one without them by its
/// `start_time`.
pub fn forgetting_owner(path: &Path) -> MarkerOwner {
    let mut m = MarkerOwner::default();
    if !path.exists() {
        return m;
    }
    m.there = true;
    let o: Option<Value> = read_shared(path)
        .filter(|t| t.trim_start().starts_with('{'))
        .and_then(|t| serde_json::from_str(&t).ok());
    let get = |k: &str| o.as_ref().and_then(|o| o.get(k));
    if let Some(n) = count_value(get("pid")).filter(|n| *n <= i64::from(i32::MAX)) {
        m.pid = n as u32;
    }
    m.since = value_text(get("since"));
    let ticks = count_value(get("start_ticks"));
    if m.pid > 0 {
        m.alive = match ticks {
            Some(t) if t > 0 => pid_identity_ticks(m.pid, t) != Identity::Gone,
            _ => pid_alive_hooked(m.pid, &value_text(get("start_time"))),
        };
    }
    m.why = format!(
        "c3 telemetry --forget --local is deleting the local telemetry data (pid {}{}; the marker {})",
        m.pid,
        if m.since.is_empty() {
            String::new()
        } else {
            format!(", since {}", m.since)
        },
        path.display()
    );
    m
}

/// `Resolve-TelemetryForgetting`, UNDER the spool lock: a marker whose owner is gone (or that names
/// none) is removed - one note in the last flush's record - and `Ok` (the holder goes on); a marker
/// whose owner lives is `Err(the refusal)`.
pub(crate) fn resolve_forgetting(paths: &LocalPaths) -> Result<(), String> {
    let m = forgetting_owner(&paths.marker);
    if !m.there {
        return Ok(());
    }
    if m.alive {
        return Err(m.why);
    }
    if let Err(e) = fs::remove_file(&paths.marker) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!(
                "the forgetting marker {} of a forget that is gone could not be removed ({})",
                paths.marker.display(),
                c3_core::one_line(&e.to_string())
            ));
        }
    }
    add_last_note(
        paths,
        &format!(
            "removed the forgetting marker of {}{} - a -Forget -Local that did not finish; run it again to finish the local deletion",
            if m.pid > 0 {
                format!("pid {} (gone)", m.pid)
            } else {
                "no named owner".to_string()
            },
            if m.since.is_empty() {
                String::new()
            } else {
                format!(" since {}", m.since)
            }
        ),
    );
    Ok(())
}

/// The marker a local deletion holds while it runs: written by [`ForgettingMarker::write`],
/// removed when dropped (it never outlives the deletion - a killed process leaves one whose owner is
/// gone, which the next producer or sender removes).
pub(crate) struct ForgettingMarker {
    path: PathBuf,
}

impl ForgettingMarker {
    /// Write `{pid, start_time, start_ticks, since}` of this process (E3: its start in ticks too).
    pub(crate) fn write(path: &Path) -> std::io::Result<ForgettingMarker> {
        let me = std::process::id();
        let start = crate::liveness::proc::process_start_iso(me).unwrap_or_default();
        let ticks = own_start_ticks();
        let o = json!({
            "pid": me,
            "start_time": start,
            "start_ticks": if ticks > 0 { json!(ticks) } else { Value::Null },
            "since": now_iso(),
        });
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, format!("{o}\n"))?;
        Ok(ForgettingMarker {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for ForgettingMarker {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "c3-ns-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn ticks_of_a_round_trip_time_are_dotnet_ticks() {
        // DateTimeOffset.Parse('2026-10-09T03:50:00.1234567Z').UtcTicks
        assert_eq!(
            iso_ticks("2026-10-09T03:50:00.1234567Z"),
            Some(639_271_146_001_234_567)
        );
        assert_eq!(
            iso_ticks("1970-01-01T00:00:00Z"),
            Some(DOTNET_UNIX_EPOCH_TICKS)
        );
        assert_eq!(iso_ticks(""), None);
    }

    #[test]
    fn this_process_is_alive_on_its_ticks_and_gone_one_tick_off() {
        let me = std::process::id();
        let t = own_start_ticks();
        assert!(t > 0);
        assert_eq!(pid_identity_ticks(me, t), Identity::Alive);
        if cfg!(windows) {
            assert_eq!(pid_identity_ticks(me, t + 1), Identity::Gone);
        }
        assert_eq!(pid_identity_ticks(me, 0), Identity::Unknown);
        assert_eq!(pid_identity_ticks(999_999, t), Identity::Gone);
        assert_eq!(process_start_ticks(999_999), None);
    }

    #[test]
    fn the_file_list_knows_producers_legacy_and_staged_names_only() {
        let d = scratch("list");
        for n in [
            "telemetry-not-spooled-12-34.ndjson",
            "telemetry-not-spooled.ndjson",
            "not-spooled.ndjson",
            "telemetry-not-spooled-legacy-5.ndjson",
            "telemetry-not-spooled-notes.ndjson",
            "telemetry-not-spooled-99999999999-1.ndjson",
            "telemetry-not-spooled-1-2-3.ndjson",
        ] {
            fs::write(d.join(n), "x\n").unwrap();
        }
        let names: Vec<(String, u32, bool, bool)> = list_files(&d)
            .into_iter()
            .map(|f| (f.name, f.pid, f.legacy, f.staged))
            .collect();
        assert_eq!(
            names,
            vec![
                ("not-spooled.ndjson".to_string(), 0, true, false),
                (
                    "telemetry-not-spooled-12-34.ndjson".to_string(),
                    12,
                    false,
                    false
                ),
                (
                    "telemetry-not-spooled-legacy-5.ndjson".to_string(),
                    0,
                    false,
                    true
                ),
                ("telemetry-not-spooled.ndjson".to_string(), 0, true, false),
            ]
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn complete_lines_leave_out_an_append_in_progress_unless_the_tail_counts() {
        assert_eq!(complete_lines("a\nb\nhalf", false), vec!["a", "b"]);
        assert_eq!(complete_lines("a\nb\nhalf", true), vec!["a", "b", "half"]);
        assert!(complete_lines("half", false).is_empty());
        assert_eq!(complete_lines("a\r\n\n", false), vec!["a\r"]);
    }

    #[test]
    fn the_folded_map_takes_entries_and_bare_names() {
        let v = json!({"not_spooled_folded": [{"name": "A.ndjson", "bytes": 12}, "b.ndjson", {"name": "c", "bytes": "x"}, null, {"bytes": 3}]});
        let m = folded_map(Some(&v));
        assert_eq!(m.len(), 3);
        assert_eq!(m["a.ndjson"].bytes, 12);
        assert_eq!(m["a.ndjson"].name, "A.ndjson");
        assert_eq!(m["b.ndjson"].bytes, -1);
        assert_eq!(m["c"].bytes, -1);
    }

    #[test]
    fn a_marker_is_judged_on_pid_and_ticks_and_an_older_one_by_its_start_time() {
        let d = scratch("marker");
        let mk = d.join(MARKER_FILE);
        let me = std::process::id();
        let t = own_start_ticks();
        let iso = crate::liveness::proc::process_start_iso(me).unwrap();
        fs::write(
            &mk,
            format!("{{\"pid\":{me},\"start_ticks\":{t},\"since\":\"s\"}}\n"),
        )
        .unwrap();
        let m = forgetting_owner(&mk);
        assert!(m.there && m.alive && m.pid == me, "{m:?}");
        assert!(m.why.contains(&format!("(pid {me}, since s; the marker ")));
        if cfg!(windows) {
            fs::write(
                &mk,
                format!(
                    "{{\"pid\":{me},\"start_time\":\"{iso}\",\"start_ticks\":{},\"since\":\"s\"}}\n",
                    t + 1
                ),
            )
            .unwrap();
            assert!(!forgetting_owner(&mk).alive, "the ticks decide");
        }
        fs::write(&mk, format!("{{\"pid\":{me},\"start_time\":\"{iso}\"}}\n")).unwrap();
        assert!(
            forgetting_owner(&mk).alive,
            "an older marker: its start_time"
        );
        fs::write(
            &mk,
            format!("{{\"pid\":{me},\"start_time\":\"2000-01-01T00:00:00.0000000Z\"}}\n"),
        )
        .unwrap();
        assert!(!forgetting_owner(&mk).alive);
        fs::write(&mk, "garb").unwrap();
        let g = forgetting_owner(&mk);
        assert!(g.there && !g.alive && g.pid == 0);
        let _ = fs::remove_dir_all(&d);
    }

    // ----------------------------------------------------------------------- wave 3d (F24-1..F24-6)

    fn ns_line(time: &str, why: &str) -> String {
        format!("{{\"time\":\"{time}\",\"why\":\"{why}\"}}\n")
    }

    fn record_with_seen(paths: &LocalPaths, seen: u64) {
        write_last(
            &paths.last,
            &json!({"time": "2026-10-09T10:00:00+02:00", "result": "seeded", "not_spooled_seen": seen, "not_spooled_folded": [], "notes": []}),
        )
        .unwrap();
    }

    /// (F24-4) `--status` names the latest line the last flush did NOT see: a seen line dated later
    /// (a clock set back) is not it, nor is a seen line of another producer when the unseen one is
    /// older (a legacy file's).
    #[test]
    fn f24_4_the_latest_named_is_an_unseen_line() {
        let d = scratch("f24-4");
        let paths = LocalPaths::in_dir(&d);
        // one producer: the seen line dated 2030, the new one 2020
        fs::write(
            d.join("telemetry-not-spooled-12-34.ndjson"),
            ns_line("2030-01-01T00:00:00+00:00", "seen-2030")
                + &ns_line("2020-01-01T00:00:00+00:00", "new-2020"),
        )
        .unwrap();
        record_with_seen(&paths, 1);
        let n = count(&paths, false);
        assert_eq!((n.count, n.last.as_str()), (1, "new-2020"), "{n:?}");
        assert_eq!(n.when, "2020-01-01T00:00:00+00:00");
        // a second producer (its name sorts first) whose one line is new: the seen lines are the
        // other producer's two
        fs::remove_file(d.join("telemetry-not-spooled-12-34.ndjson")).unwrap();
        fs::write(
            d.join("telemetry-not-spooled-20-1.ndjson"),
            ns_line("2026-10-01T10:00:00+02:00", "a-1")
                + &ns_line("2026-10-01T10:01:00+02:00", "a-2"),
        )
        .unwrap();
        fs::write(
            d.join("telemetry-not-spooled-10-1.ndjson"),
            ns_line("2026-10-02T10:00:00+02:00", "b-new"),
        )
        .unwrap();
        record_with_seen(&paths, 2);
        let n = count(&paths, false);
        assert_eq!((n.count, n.last.as_str()), (1, "b-new"), "{n:?}");
        // the one unseen line is an OLD legacy line: it is named, not the producers' seen ones
        fs::remove_file(d.join("telemetry-not-spooled-10-1.ndjson")).unwrap();
        fs::write(
            d.join(LEGACY_FILE),
            ns_line("2026-09-01T09:00:00+02:00", "legacy-old"),
        )
        .unwrap();
        let n = count(&paths, false);
        assert_eq!((n.count, n.last.as_str()), (1, "legacy-old"), "{n:?}");
        // every line counts with `all`: the latest of all
        let n = count(&paths, true);
        assert_eq!((n.count, n.last.as_str()), (3, "a-2"), "{n:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// (F24-1) The `not_spooled_seen` of a flush without the spool lock: the lines of the files a
    /// fold would keep - this live process's - and none of a gone producer's, a staged, a legacy or
    /// a named file's.
    #[test]
    fn f24_1_without_a_fold_only_the_kept_files_lines_are_seen() {
        let d = scratch("f24-1");
        let paths = LocalPaths::in_dir(&d);
        append(&paths, "own-1").unwrap();
        append(&paths, "own-2").unwrap();
        fs::write(
            d.join("telemetry-not-spooled-999999-639000000000000000.ndjson"),
            ns_line("2026-10-01T10:00:00+02:00", "gone-1").repeat(3),
        )
        .unwrap();
        fs::write(
            d.join("telemetry-not-spooled-legacy-639000000000000001.ndjson"),
            ns_line("2026-10-01T10:00:00+02:00", "staged"),
        )
        .unwrap();
        fs::write(
            d.join(LEGACY_FILE),
            ns_line("2026-10-01T10:00:00+02:00", "legacy"),
        )
        .unwrap();
        assert_eq!(kept_seen(&paths, &HashMap::new()), 2);
        // a file the record names is the fold's, never seen
        let own_name = paths
            .own_file()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let mut named = HashMap::new();
        named.insert(
            own_name.to_ascii_lowercase(),
            FoldedEntry {
                name: own_name,
                bytes: 3,
            },
        );
        assert_eq!(kept_seen(&paths, &named), 0);
        let _ = fs::remove_dir_all(&d);
    }

    /// (F24-1) A gone producer's file the fold cannot open this time is neither folded nor seen;
    /// free again, the next fold counts it once.
    #[cfg(windows)]
    #[test]
    fn f24_1_a_gone_file_the_fold_cannot_open_is_neither_folded_nor_seen() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = scratch("f24-1b");
        let paths = LocalPaths::in_dir(&d);
        append(&paths, "own").unwrap();
        let gone = d.join("telemetry-not-spooled-999999-639000000000000000.ndjson");
        fs::write(
            &gone,
            ns_line("2026-10-01T10:00:00+02:00", "gone").repeat(2),
        )
        .unwrap();
        let hold = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&gone)
            .unwrap();
        let mut f = merge(&paths, &HashMap::new());
        assert_eq!((f.lines, f.producers, f.seen), (0, 0, 1), "{f:?}");
        assert!(f.folded.is_empty());
        let _ = f.complete(false);
        drop(hold);
        let mut f = merge(&paths, &HashMap::new());
        assert_eq!((f.lines, f.producers, f.seen), (2, 1, 1), "{f:?}");
        let _ = f.complete(false);
        let _ = fs::remove_dir_all(&d);
    }

    /// (F24-2) A file the record names that grew by a last piece without a line end: the fold
    /// counts that piece (the file is deleted - its producer is gone).
    #[test]
    fn f24_2_a_recorded_files_tail_without_a_line_end_is_counted() {
        let d = scratch("f24-2");
        let paths = LocalPaths::in_dir(&d);
        let name = "telemetry-not-spooled-999999-639000000000000000.ndjson";
        let head =
            ns_line("2026-10-01T10:00:00+02:00", "a") + &ns_line("2026-10-01T10:01:00+02:00", "b");
        fs::write(
            d.join(name),
            head.clone() + "{\"time\":\"2026-10-01T10:02:00+02:00\",\"why\":\"tail\"}",
        )
        .unwrap();
        let mut already = HashMap::new();
        already.insert(
            name.to_string(),
            FoldedEntry {
                name: name.to_string(),
                bytes: head.len() as i64,
            },
        );
        let mut f = merge(&paths, &already);
        assert_eq!((f.lines, f.producers), (1, 1), "{f:?}");
        let _ = f.complete(false);
        let _ = fs::remove_dir_all(&d);
    }

    /// (F24-3) No count is written while a deletion is pending, confirmed or cleaning (or its
    /// record cannot be read), or while a living owner's forgetting marker is there; the empty file
    /// the refused append made is gone, a file with lines is left to the cleanup; a gone owner's
    /// marker refuses nothing.
    #[test]
    fn f24_3_no_count_is_written_while_the_local_data_is_being_deleted() {
        let d = scratch("f24-3");
        let paths = LocalPaths::in_dir(&d);
        let own = paths.own_file();
        let txn = |phase: &str| {
            fs::write(
                d.join(FORGET_PENDING),
                format!("{{\"instance_id\":\"i\",\"public_ref\":\"\",\"since\":\"s\",\"attempts\":0,\"last_error\":\"\",\"phase\":\"{phase}\"}}"),
            )
            .unwrap();
        };
        for phase in ["pending", "confirmed", "cleaning"] {
            txn(phase);
            let e = append(&paths, "x").unwrap_err();
            assert!(
                e.contains("is not written while the local telemetry data is being deleted"),
                "{e}"
            );
            assert!(
                !own.exists(),
                "{phase}: the empty file the append made is removed"
            );
        }
        fs::write(d.join(FORGET_PENDING), "garb").unwrap();
        assert!(
            append(&paths, "x").is_err(),
            "an unreadable record: fail closed"
        );
        fs::remove_file(d.join(FORGET_PENDING)).unwrap();
        let me = std::process::id();
        fs::write(
            &paths.marker,
            format!(
                "{{\"pid\":{me},\"start_ticks\":{},\"since\":\"s\"}}\n",
                own_start_ticks()
            ),
        )
        .unwrap();
        let e = append(&paths, "x").unwrap_err();
        assert!(
            e.contains("c3 telemetry --forget --local is deleting"),
            "{e}"
        );
        assert!(!own.exists());
        fs::write(&paths.marker, "{\"pid\":999999,\"since\":\"s\"}\n").unwrap();
        append(&paths, "after").unwrap();
        assert_eq!(fs::read_to_string(&own).unwrap().lines().count(), 1);
        txn("cleaning");
        assert!(append(&paths, "y").is_err());
        assert_eq!(
            fs::read_to_string(&own).unwrap().lines().count(),
            1,
            "a file with lines is left to the cleanup"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// (F24-6) A staged generation never takes a name on disk nor one the record names (a record
    /// whose last rewrite failed still names a file the fold deleted).
    #[test]
    fn f24_6_a_staged_name_is_neither_on_disk_nor_recorded() {
        let d = scratch("f24-6");
        let t = 639_000_000_000_000_000_i64;
        fs::write(d.join(format!("{STAGED_PREFIX}{t}.ndjson")), "x\n").unwrap();
        assert_eq!(
            staged_path(&d, t, &HashMap::new()),
            d.join(format!("{STAGED_PREFIX}{}.ndjson", t + 1))
        );
        let n = format!("{STAGED_PREFIX}{}.ndjson", t + 1);
        let mut taken = HashMap::new();
        taken.insert(n.clone(), FoldedEntry { name: n, bytes: 2 });
        assert_eq!(
            staged_path(&d, t, &taken),
            d.join(format!("{STAGED_PREFIX}{}.ndjson", t + 2))
        );
        let _ = fs::remove_dir_all(&d);
    }
}
