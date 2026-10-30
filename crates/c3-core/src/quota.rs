//! (C3 wave 8 = the plugin's wave 30, TECH_DEBT T15) Quota windows: what Codex itself reports.
//!
//! Codex CLI (0.155) writes a `token_count` event into a thread's rollout,
//! `<codex home>/sessions/<yyyy>/<mm>/<dd>/rollout-<timestamp>-<thread id>.jsonl`, after every model
//! request:
//!
//! ```text
//! {"timestamp":"...","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":
//!  {"input_tokens":..,"cached_input_tokens":..,"output_tokens":..},"last_token_usage":{...},
//!  "model_context_window":258400},"rate_limits":{"limit_id":"codex","primary":{"used_percent":96.0,
//!  "window_minutes":300,"resets_at":<unix s>},"secondary":{...},...}}}
//! ```
//!
//! `primary` = the 5-hour window, `secondary` = the weekly one of a ChatGPT subscription (null on
//! other plans and in older rollouts). Every request writes TWO such lines: the `codex` one, then one
//! with `"limit_id":"premium"`, null windows and the same totals - the reader takes the LAST line that
//! names a window, and counts a call only where the total changed. The totals are CUMULATIVE over the
//! thread's life and a fork INHERITS its parent's: the tokens of ONE run are its last total minus the
//! last total before it started (the parent's rollout for a fork, the same rollout for a resume).
//! Sub-agent threads (Codex feature `multi_agent`) are rollouts of their own whose first line
//! (`session_meta`) names `"parent_thread_id":"<parent>"`; their totals start at zero.
//!
//! A window at or above [`QUOTA_BLOCK_PERCENT`] whose reset lies ahead keeps the endpoint out until
//! that reset; at or above [`QUOTA_WARN_PERCENT`] the available verdict says `low quota: ...`; a
//! reset in the past means the window was reset (shown, never judged). The retention / clearing
//! policy these shares belong to lives in [`crate::retention`].
//!
//! Ported from the plugin's `codex-consult-common.ps1` (wave 30): `Get-RolloutTokenEvents`,
//! `Get-RolloutUsage`, `Get-CodexRolloutPath`, `Get-CodexSubagentRollouts`, `Get-CodexRunQuota`,
//! `Select-NewerWindow`, `Get-QuotaWindowName`, `Format-QuotaPercent`, `Format-QuotaReset`,
//! `Get-QuotaWindowStates`, `Format-QuotaWindows`, `Get-CodexAccountWindow`, `Get-EndpointWindow`,
//! `Format-QuotaLowWarning`, `Read-RolloutPrimaryWindow`, `Get-ForkReplayGuard`,
//! `Get-ThreadIdFromStreamHead`.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime};

use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate, TimeZone, Utc};
use serde_json::{json, Value};

// The window block / warn shares are part of the retention policy; see [`crate::retention`]. They
// are re-exported here so the `c3_core::quota::` paths stay unchanged.
pub use crate::retention::{QUOTA_BLOCK_PERCENT, QUOTA_WARN_PERCENT};
/// `CODEX_CONSULT_FORK_MAX_KB`'s default: a parent rollout over it refuses a fork / resume.
pub const FORK_MAX_KB_DEFAULT: i64 = 512;
/// From this size on (up to the limit) a fork / resume is warned about.
pub const FORK_WARN_KB: i64 = 128;
/// `CODEX_CONSULT_QUOTA_BUDGET`'s default: the main turn of a codex run stops at 90 % of the
/// 5-hour window.
pub const QUOTA_BUDGET_DEFAULT: i64 = 90;
/// The budget watch's poll interval (the test hook `CODEX_CONSULT_TEST_BUDGET_POLL_MS` shortens it).
pub const BUDGET_POLL_MS: u64 = 15_000;
/// How much of a rollout's end the budget watch and the account window read.
pub const ROLLOUT_TAIL_BYTES: u64 = 262_144;
/// The whole-file read cap (`Read-RolloutText -MaxBytes`): the file's END when larger.
const ROLLOUT_MAX_BYTES: u64 = 67_108_864;
/// The first line of a rollout is read up to this many bytes (`Read-RolloutHead`).
const ROLLOUT_HEAD_BYTES: u64 = 65_536;

/// The reading discipline of the reviewing purposes (acceptance, diff-review, checkpoint,
/// core-contract): a reviewer that prints whole files replays them on every later tool call of its
/// thread (cached input counts against a subscription's window too). The plugin's exact text.
pub const READING_DISCIPLINE: &str = "Reading discipline: never print whole files. Run `git diff <range> --stat` first, then read the hunks of ONE path at a time with a line cap (`git diff <range> -- <path> | head -n 200`; more lines only where you need them); outside the diff, find with `grep -n` and read only the line ranges you need. Read the documents the brief names before the code. Give your verdict even when your reading is incomplete, and list what you did not read.";

/// The purposes whose paragraph ends with [`READING_DISCIPLINE`].
pub const READING_DISCIPLINE_PURPOSES: [&str; 4] =
    ["checkpoint", "core-contract", "acceptance", "diff-review"];

/// The `-c` item that turns a reviewer's sub-agents off.
pub const MULTI_AGENT_OFF: &str = "features.multi_agent=false";

// --------------------------------------------------------------------------- the formats

/// `Format-OffsetIso` of a local time.
fn iso(v: DateTime<FixedOffset>) -> String {
    v.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// A time as the machine's local wall clock with its offset (`.ToLocalTime()`).
pub fn to_local(v: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
    v.with_timezone(&Local).fixed_offset()
}

/// `ConvertTo-WhenOffset`: an ISO time with an offset (a space for the `T` tolerated); `None`
/// otherwise.
pub fn when_offset(v: &Value) -> Option<DateTime<FixedOffset>> {
    let s = v.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(s)
        .ok()
        .or_else(|| DateTime::parse_from_rfc3339(&s.replacen(' ', "T", 1)).ok())
}

/// One window of a `rate_limits` object as the ledger records it: `{used_percent, window_minutes,
/// resets_at}` (`resets_at` ISO with the LOCAL offset, from unix seconds).
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub used_percent: f64,
    pub window_minutes: Option<i64>,
    pub resets_at: Option<String>,
}

impl Window {
    /// The ledger's shape.
    pub fn to_json(&self) -> Value {
        json!({
            "used_percent": self.used_percent,
            "window_minutes": self.window_minutes,
            "resets_at": self.resets_at,
        })
    }
}

/// `ConvertFrom-RolloutWindow`: an object with `used_percent`; `None` for null or a text without it.
fn window_of(v: &Value) -> Option<Window> {
    let o = v.as_object()?;
    let used = match o.get("used_percent")? {
        v @ Value::Number(_) => finite_percent(v)?,
        _ => return None,
    };
    let window_minutes = o.get("window_minutes").and_then(|m| {
        m.as_i64()
            .or_else(|| m.as_f64().filter(|f| *f >= 0.0).map(|f| f as i64))
    });
    let resets_at = o
        .get("resets_at")
        .and_then(|r| r.as_i64().or_else(|| r.as_f64().map(|f| f as i64)))
        .and_then(|secs| Utc.timestamp_opt(secs, 0).single())
        .map(|t| iso(t.with_timezone(&Local).fixed_offset()));
    Some(Window {
        used_percent: used,
        window_minutes,
        resets_at,
    })
}

/// The running totals of a `total_token_usage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Totals {
    pub input: Option<i64>,
    pub cached: Option<i64>,
    pub output: Option<i64>,
}

/// An observation instant as a record stores it (wave 8i): RFC3339 with the fractional seconds it
/// has (`2026-10-13T12:00:00.900+00:00`; none when it has none), so it reads back exactly.
fn iso_instant(v: DateTime<FixedOffset>) -> String {
    v.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, false)
}

/// Compare two observation instants at WHOLE-SECOND granularity (floor to the second; fractions
/// are bookkeeping, never an ordering key): a strictly later second wins on recency, the same
/// second is a tie that [`cmp_window_rank`] decides. (floor second, then the rank) is a
/// lexicographic total order, so a fold over any mix of precisions is transitive and
/// order-independent.
fn cmp_instants(a: DateTime<FixedOffset>, b: DateTime<FixedOffset>) -> std::cmp::Ordering {
    a.timestamp().cmp(&b.timestamp())
}

/// The keys of a recorded `quota` that carry each window's OWN observation instant (wave 8h),
/// primary then secondary.
pub const WINDOW_AT_KEYS: [&str; 2] = ["primary_observed_at", "secondary_observed_at"];

/// One observation of the windows: the windows of one `token_count` event and when it was written.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub primary: Option<Window>,
    pub secondary: Option<Window>,
    /// When the observation was written; for an AGGREGATE, derived (the latest of the per-window
    /// instants) and for display only - never a replacement decision.
    pub at: Option<DateTime<FixedOffset>>,
    /// The instant of the primary window / of the secondary window when it differs from `at` (an
    /// aggregate keeps each retained window's OWN observation instant); `None` = `at`.
    pub primary_at: Option<DateTime<FixedOffset>>,
    pub secondary_at: Option<DateTime<FixedOffset>>,
    pub limit_id: String,
}

impl Observation {
    /// The observation instant of ONE window (0 = primary, 1 = secondary): its own, else `at`.
    pub fn window_at(&self, kind: usize) -> Option<DateTime<FixedOffset>> {
        let own = if kind == 0 {
            self.primary_at
        } else {
            self.secondary_at
        };
        own.or(self.at)
    }

    /// It names a window (primary or secondary).
    pub fn named(&self) -> bool {
        self.primary.is_some() || self.secondary.is_some()
    }

    fn fullest(&self) -> f64 {
        let p = self.primary.as_ref().map(|w| w.used_percent).unwrap_or(0.0);
        let s = self
            .secondary
            .as_ref()
            .map(|w| w.used_percent)
            .unwrap_or(0.0);
        p.max(s)
    }

    /// The ledger's `quota` `{primary, secondary, observed_at}` (observed_at local), then - only for
    /// a window that is present - `primary_observed_at` / `secondary_observed_at`: the instant
    /// each retained window was OBSERVED (wave 8h; `observed_at` is the derived aggregate, the
    /// legacy fallback of a record written without them).
    pub fn to_quota(&self) -> Value {
        let mut q = json!({
            "primary": self.primary.as_ref().map(Window::to_json),
            "secondary": self.secondary.as_ref().map(Window::to_json),
            "observed_at": self.at.map(|a| iso(to_local(a))),
        });
        for (kind, key) in WINDOW_AT_KEYS.iter().enumerate() {
            let present = if kind == 0 {
                self.primary.is_some()
            } else {
                self.secondary.is_some()
            };
            if let (true, Some(t)) = (present, self.window_at(kind)) {
                q[*key] = Value::String(iso_instant(to_local(t)));
            }
        }
        q
    }
}

/// The ledger's `quota` when the rollout names no window or was not found.
pub fn null_quota() -> Value {
    json!({ "primary": null, "secondary": null, "observed_at": null })
}

/// One `token_count` event (`Get-RolloutTokenEvents`).
#[derive(Debug, Clone)]
struct TokenEvent {
    at: Option<DateTime<FixedOffset>>,
    total: Option<Totals>,
    /// The raw total: a new call changes it.
    total_key: Option<Value>,
    last_input: Option<i64>,
    context_window: Option<i64>,
    has_limits: bool,
    limit_id: String,
    primary: Option<Window>,
    secondary: Option<Window>,
}

fn num(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_u64().map(|u| u as i64))
}

/// Every `token_count` event of a rollout text, in file order.
fn token_events(text: &str) -> Vec<TokenEvent> {
    let mut out = Vec::new();
    for line in text.split('\n') {
        if !line.contains("\"token_count\"") {
            continue;
        }
        let v: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let payload = if v.get("payload").map(|p| p.is_object()).unwrap_or(false) {
            &v["payload"]
        } else {
            &v
        };
        if payload.get("type").and_then(|t| t.as_str()) != Some("token_count") {
            continue;
        }
        let at = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok());
        let info = &payload["info"];
        let tt = &info["total_token_usage"];
        let (total, total_key) = if tt.is_object() {
            (
                Some(Totals {
                    input: num(&tt["input_tokens"]),
                    cached: num(&tt["cached_input_tokens"]),
                    output: num(&tt["output_tokens"]),
                }),
                Some(tt.clone()),
            )
        } else {
            (None, None)
        };
        let last_input = num(&info["last_token_usage"]["input_tokens"]);
        let context_window = num(&info["model_context_window"]);
        let rl = &payload["rate_limits"];
        let has_limits = rl.is_object();
        let (limit_id, primary, secondary) = if has_limits {
            (
                rl["limit_id"].as_str().unwrap_or("").to_string(),
                window_of(&rl["primary"]),
                window_of(&rl["secondary"]),
            )
        } else {
            (String::new(), None, None)
        };
        out.push(TokenEvent {
            at,
            total,
            total_key,
            last_input,
            context_window,
            has_limits,
            limit_id,
            primary,
            secondary,
        });
    }
    out
}

// --------------------------------------------------------------------------- reading a rollout

/// `Read-RolloutText`: the whole file (at most 64 MiB - its end), or its last `tail_bytes`; a cut
/// start drops the first, partial line. `""` when unreadable.
fn read_rollout_text(path: &Path, tail_bytes: u64) -> String {
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let len = match f.metadata() {
        Ok(m) => m.len(),
        Err(_) => return String::new(),
    };
    let mut take = len;
    if tail_bytes > 0 && take > tail_bytes {
        take = tail_bytes;
    }
    if take > ROLLOUT_MAX_BYTES {
        take = ROLLOUT_MAX_BYTES;
    }
    let start = len - take;
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::with_capacity(take as usize);
    if f.take(take).read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    if start > 0 {
        match text.find('\n') {
            Some(i) => text[i + 1..].to_string(),
            None => String::new(),
        }
    } else {
        text
    }
}

/// `Read-RolloutHead`: the first line (its `session_meta`), at most 64 KiB of it. `""` when
/// unreadable.
pub fn read_rollout_head(path: &Path) -> String {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let mut buf = Vec::new();
    if f.take(ROLLOUT_HEAD_BYTES).read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let end = buf.iter().position(|b| *b == b'\n').unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// What one rollout says (`Get-RolloutUsage`).
#[derive(Debug, Clone, Default)]
pub struct RolloutUsage {
    pub found: bool,
    pub path: PathBuf,
    pub bytes: u64,
    /// The LAST `token_count` that names a window, else the last one that carries `rate_limits`.
    pub window: Option<Observation>,
    /// The last running total.
    pub total: Option<Totals>,
    /// The last total before `before` (`None` when none or without `before`).
    pub baseline: Option<Totals>,
    /// The `token_count` events at or after `before` (all without it) whose total differs from the
    /// event before: one per model request.
    pub calls: i64,
    /// The largest per-call input among them.
    pub max_input: Option<i64>,
    /// The last `model_context_window`.
    pub context_window: Option<i64>,
}

/// `Get-RolloutUsage`: `tail_bytes` > 0 reads only the end of the file (the budget watch; the
/// baseline and the calls then cover only that end).
pub fn rollout_usage(
    path: &Path,
    before: Option<DateTime<FixedOffset>>,
    tail_bytes: u64,
) -> RolloutUsage {
    let mut r = RolloutUsage {
        path: path.to_path_buf(),
        ..Default::default()
    };
    if path.as_os_str().is_empty() || !path.is_file() {
        return r;
    }
    r.found = true;
    r.bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut prev_total: Option<Value> = None;
    let mut last_limits: Option<Observation> = None;
    for ev in token_events(&read_rollout_text(path, tail_bytes)) {
        let in_run = match before {
            None => true,
            Some(b) => ev.at.map(|a| a >= b).unwrap_or(false),
        };
        if let Some(t) = ev.total {
            r.total = Some(t);
            if let (Some(b), Some(a)) = (before, ev.at) {
                if a < b {
                    r.baseline = Some(t);
                }
            }
            if in_run && ev.total_key != prev_total {
                r.calls += 1;
                if let Some(li) = ev.last_input {
                    if r.max_input.map(|m| li > m).unwrap_or(true) {
                        r.max_input = Some(li);
                    }
                }
            }
            prev_total = ev.total_key.clone();
        }
        if ev.context_window.is_some() {
            r.context_window = ev.context_window;
        }
        if ev.has_limits {
            let w = Observation {
                primary: ev.primary,
                secondary: ev.secondary,
                at: ev.at,
                primary_at: None,
                secondary_at: None,
                limit_id: ev.limit_id,
            };
            if w.named() {
                r.window = Some(w.clone());
            }
            last_limits = Some(w);
        }
    }
    if r.window.is_none() {
        r.window = last_limits;
    }
    r
}

// --------------------------------------------------------------------------- finding rollouts

fn uuid_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        )
        .unwrap()
    })
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36 && uuid_re().find(s).map(|m| m.as_str() == s).unwrap_or(false)
}

/// `Get-UuidV7Time`: the creation time of a UUIDv7 (its first 48 bits, unix milliseconds); `None`
/// for another version.
pub fn uuid_v7_time(id: &str) -> Option<DateTime<Utc>> {
    let b = id.as_bytes();
    if !is_uuid(id) || b[14] != b'7' {
        return None;
    }
    let hex = format!("{}{}", &id[0..8], &id[9..13]);
    let ms = i64::from_str_radix(&hex, 16).ok()?;
    Utc.timestamp_millis_opt(ms).single()
}

/// `Get-LocalDayDirs`: the `<root>/<yyyy>/<MM>/<dd>` directories of the LOCAL days from `from` to
/// `to` (at most 7 plus the last day).
pub fn local_day_dirs(root: &Path, from: NaiveDate, to: NaiveDate) -> Vec<PathBuf> {
    let (mut a, mut b) = (from, to);
    if b < a {
        std::mem::swap(&mut a, &mut b);
    }
    let mut days: Vec<NaiveDate> = Vec::new();
    let mut d = a;
    while d <= b && days.len() < 7 {
        days.push(d);
        d = match d.succ_opt() {
            Some(n) => n,
            None => break,
        };
    }
    if days.last() != Some(&b) {
        days.push(b);
    }
    days.iter()
        .map(|d| {
            root.join(format!("{:04}", d.year()))
                .join(format!("{:02}", d.month()))
                .join(format!("{:02}", d.day()))
        })
        .collect()
}

fn local_date(t: DateTime<Utc>) -> NaiveDate {
    t.with_timezone(&Local).date_naive()
}

/// The `rollout-*.jsonl` files of a directory whose name ends with `<suffix>` (lower case
/// compared), with their write times.
fn rollouts_in(dir: &Path, suffix: Option<&str>, out: &mut Vec<(SystemTime, PathBuf)>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if !(name.starts_with("rollout-") && name.ends_with(".jsonl")) {
            continue;
        }
        if let Some(s) = suffix {
            if !name.ends_with(s) {
                continue;
            }
        }
        let meta = match e.metadata() {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };
        out.push((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), e.path()));
    }
}

fn rollouts_recursive(dir: &Path, suffix: &str, out: &mut Vec<(SystemTime, PathBuf)>, depth: u32) {
    if depth > 6 {
        return;
    }
    rollouts_in(dir, Some(suffix), out);
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                rollouts_recursive(&e.path(), suffix, out, depth + 1);
            }
        }
    }
}

/// `Get-CodexRolloutPath`: the newest `<codex home>/sessions/**/rollout-*-<id>.jsonl`, `None` when
/// there is none. A UUIDv7 id carries its creation time: that local day and the days around it are
/// looked at first, then the whole tree.
pub fn codex_rollout_path(thread: &str, codex_home: &str) -> Option<PathBuf> {
    if !is_uuid(thread) || codex_home.is_empty() {
        return None;
    }
    let root = Path::new(codex_home).join("sessions");
    if !root.is_dir() {
        return None;
    }
    let suffix = format!("-{}.jsonl", thread.to_lowercase());
    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
    if let Some(created) = uuid_v7_time(thread) {
        let day = local_date(created);
        let from = day.pred_opt().unwrap_or(day);
        let to = day.succ_opt().unwrap_or(day);
        for d in local_day_dirs(&root, from, to) {
            rollouts_in(&d, Some(&suffix), &mut found);
        }
    }
    if found.is_empty() {
        rollouts_recursive(&root, &suffix, &mut found, 0);
    }
    found.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    found.into_iter().next().map(|(_, p)| p)
}

fn parent_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r#""parent_thread_id"\s*:\s*"([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})""#,
        )
        .unwrap()
    })
}

/// `Get-CodexSubagentRollouts`: the rollouts of the local days from `since` to now, written since
/// then (5 s slack), whose first line names the thread - or one of its sub-agents, recursively - as
/// `parent_thread_id`.
pub fn subagent_rollouts(parent: &str, since: DateTime<Utc>, codex_home: &str) -> Vec<PathBuf> {
    if parent.is_empty() || codex_home.is_empty() {
        return Vec::new();
    }
    let root = Path::new(codex_home).join("sessions");
    if !root.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<(SystemTime, PathBuf)> = Vec::new();
    for d in local_day_dirs(&root, local_date(since), local_date(Utc::now())) {
        rollouts_in(&d, None, &mut files);
    }
    let cut: SystemTime = (since - Duration::seconds(5)).into();
    let name_re = regex::Regex::new(&format!(r"({})\.jsonl$", uuid_re().as_str())).unwrap();
    let mut parent_of: Vec<(PathBuf, String, String)> = Vec::new();
    for (_, p) in files.into_iter().filter(|(t, _)| *t >= cut) {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let id = match name_re.captures(&name) {
            Some(c) => c[1].to_lowercase(),
            None => continue,
        };
        let head = read_rollout_head(&p);
        if let Some(c) = parent_re().captures(&head) {
            parent_of.push((p, c[1].to_lowercase(), id));
        }
    }
    parent_of.sort_by(|a, b| a.0.cmp(&b.0));
    let mut parents: HashSet<String> = HashSet::new();
    parents.insert(parent.to_lowercase());
    let mut out: Vec<PathBuf> = Vec::new();
    let mut changed = true;
    while changed {
        changed = false;
        for (p, par, id) in &parent_of {
            if out.contains(p) || !parents.contains(par) {
                continue;
            }
            out.push(p.clone());
            parents.insert(id.clone());
            changed = true;
        }
    }
    out
}

/// `Select-NewerWindow`: `true` when `b` wins over `a` - the newer one that names a window; at the
/// same instant the one whose fullest window is fuller (the threads of one account report at the
/// same moment, and some report a stale share: the highest is the account's).
pub fn newer_wins(a: Option<&Observation>, b: Option<&Observation>) -> bool {
    let named = |w: Option<&Observation>| w.map(|w| w.named()).unwrap_or(false);
    let (bw, b_at) = match b {
        Some(bw) if named(b) && bw.at.is_some() => (bw, bw.at.unwrap()),
        _ => return false,
    };
    let (aw, a_at) = match a {
        Some(aw) if named(a) && aw.at.is_some() => (aw, aw.at.unwrap()),
        _ => return true,
    };
    if b_at > a_at {
        return true;
    }
    if b_at < a_at {
        return false;
    }
    bw.fullest() > aw.fullest()
}

/// A used share as the quota records it (a number or a numeric text): `None` - the window is ABSENT
/// - for a non-finite (`NaN`, `inf`) or out-of-range (outside 0..=100) value.
pub fn finite_percent(v: &Value) -> Option<f64> {
    let f = match v {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    (f.is_finite() && (0.0..=100.0).contains(&f)).then_some(f)
}

/// The TOTAL, order-independent ranking of two observations of the SAME window at an equal
/// instant, evaluated at `now`: (1) a known future reset beats a known past reset beats a missing
/// one (a past reset means the window is reset, whatever its share); (2) the higher
/// `used_percent`; (3) the later `resets_at`; (4) identical - either. `Greater` when `a` ranks
/// above `b`.
fn cmp_window_rank(
    a: (f64, Option<DateTime<FixedOffset>>),
    b: (f64, Option<DateTime<FixedOffset>>),
    now: DateTime<Utc>,
) -> std::cmp::Ordering {
    let rank = |r: Option<DateTime<FixedOffset>>| match r {
        Some(t) if t.with_timezone(&Utc) > now => 2u8,
        Some(_) => 1,
        None => 0,
    };
    rank(a.1)
        .cmp(&rank(b.1))
        .then(a.0.clamp(0.0, 100.0).total_cmp(&b.0.clamp(0.0, 100.0)))
        .then(a.1.cmp(&b.1))
}

/// The observation the thread/sub-agent/account aggregation keeps after seeing `b`: PER WINDOW,
/// a candidate window is compared with the instant of THAT retained window - strictly newer
/// replaces, an equal instant takes the total [`cmp_window_rank`] at `now`, older is kept; a window
/// the candidate lacks (or an invalid one) never replaces. Each kept window carries its own
/// instant; `at` of the result is derived (the latest of them), for display only.
pub fn merge_observation(
    a: Option<Observation>,
    b: Option<&Observation>,
    now: DateTime<Utc>,
) -> Option<Observation> {
    // an observation that names no (valid) window or has no instant is absent
    let usable = |o: &Observation| o.named() && o.at.is_some();
    let (av, bv) = match (a.as_ref().filter(|o| usable(o)), b.filter(|o| usable(o))) {
        (Some(av), Some(bv)) => (av, bv),
        (None, Some(bv)) => return Some(bv.clone()),
        _ => return a,
    };
    let key = |w: &Window| {
        (
            w.used_percent,
            w.resets_at
                .as_deref()
                .and_then(|r| DateTime::parse_from_rfc3339(r).ok()),
        )
    };
    let mut wins: [Option<Window>; 2] = [None, None];
    let mut ats: [Option<DateTime<FixedOffset>>; 2] = [None, None];
    for kind in 0..2 {
        let win = |o: &Observation| {
            if kind == 0 {
                o.primary.clone()
            } else {
                o.secondary.clone()
            }
        };
        let (aw, bw) = (win(av), win(bv));
        let (a_at, b_at) = (av.window_at(kind), bv.window_at(kind));
        let take_b = match (&aw, &bw) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(x), Some(y)) => match match (b_at, a_at) {
                (Some(bt), Some(at)) => cmp_instants(bt, at),
                (b_t, a_t) => b_t.cmp(&a_t),
            } {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal => {
                    cmp_window_rank(key(y), key(x), now) == std::cmp::Ordering::Greater
                }
            },
        };
        if take_b {
            wins[kind] = bw;
            ats[kind] = b_at;
        } else {
            wins[kind] = aw;
            ats[kind] = a_at;
        }
    }
    let [primary, secondary] = wins;
    let [primary_at, secondary_at] = ats;
    let at = [(&primary, primary_at), (&secondary, secondary_at)]
        .into_iter()
        .filter(|(w, _)| w.is_some())
        .filter_map(|(_, t)| t)
        .max();
    let newest = if bv.at > av.at { bv } else { av };
    Some(Observation {
        primary,
        secondary,
        at,
        primary_at,
        secondary_at,
        limit_id: newest.limit_id.clone(),
    })
}

/// `Get-CodexRunQuota`'s result: the ledger's `quota` and `tokens` of ONE codex run.
#[derive(Debug, Clone)]
pub struct RunQuota {
    /// `{primary, secondary, observed_at}` - null members when none or no rollout.
    pub quota: Value,
    /// `{input, cached, output, thread_input, calls, max_input, context_window, subagents {count,
    /// input, calls}}`, `None` when the rollout was not found.
    pub tokens: Option<Value>,
    pub path: Option<PathBuf>,
    pub bytes: u64,
}

/// `Get-CodexRunQuota`: the rollout of the run's thread and the sub-agent rollouts of that thread
/// written during the run. `baseline_thread`: the thread whose running total before the run is
/// subtracted - the parent of a fork, the thread itself on a resume, `""` for a new thread.
pub fn codex_run_quota(
    thread: &str,
    started_at: DateTime<Utc>,
    baseline_thread: &str,
    codex_home: &str,
) -> RunQuota {
    // ONE comparison instant for the whole aggregation (the rollout reads happen between merges)
    let now = Utc::now();
    let mut r = RunQuota {
        quota: null_quota(),
        tokens: None,
        path: None,
        bytes: 0,
    };
    if thread.is_empty() {
        return r;
    }
    let path = match codex_rollout_path(thread, codex_home) {
        Some(p) => p,
        None => return r,
    };
    let before = Some(started_at.fixed_offset());
    let u = rollout_usage(&path, before, 0);
    r.path = Some(path);
    r.bytes = u.bytes;
    let base = if !baseline_thread.is_empty() && baseline_thread.eq_ignore_ascii_case(thread) {
        u.baseline
    } else if !baseline_thread.is_empty() {
        codex_rollout_path(baseline_thread, codex_home)
            .and_then(|pp| rollout_usage(&pp, before, 0).baseline)
    } else {
        None
    };
    let mut win = u.window.clone();
    let (mut sub_count, mut sub_input, mut sub_calls) = (0i64, 0i64, 0i64);
    for sp in subagent_rollouts(thread, started_at, codex_home) {
        let su = rollout_usage(&sp, None, 0);
        sub_count += 1;
        sub_calls += su.calls;
        if let Some(i) = su.total.and_then(|t| t.input) {
            sub_input += i;
        }
        win = merge_observation(win, su.window.as_ref(), now);
    }
    if let Some(w) = &win {
        r.quota = w.to_quota();
    }
    let minus = |a: Option<i64>, b: Option<i64>| -> Option<i64> {
        match (a, b) {
            (None, _) => None,
            (Some(a), None) => Some(a),
            (Some(a), Some(b)) => Some((a - b).max(0)),
        }
    };
    let t = u.total;
    let bt = |f: fn(&Totals) -> Option<i64>| base.as_ref().and_then(f);
    r.tokens = Some(json!({
        "input": t.and_then(|t| minus(t.input, bt(|b| b.input))),
        "cached": t.and_then(|t| minus(t.cached, bt(|b| b.cached))),
        "output": t.and_then(|t| minus(t.output, bt(|b| b.output))),
        "thread_input": t.and_then(|t| t.input),
        "calls": u.calls,
        "max_input": u.max_input,
        "context_window": u.context_window,
        "subagents": { "count": sub_count, "input": sub_input, "calls": sub_calls },
    }));
    r
}

// --------------------------------------------------------------------------- the views

/// `Get-QuotaWindowName`: `5-hour window` (300 minutes), `weekly window` (10080), else
/// `<n>-hour window` / `<n>-minute window`; short: `5h`, `wk`, `<n>h`, `<n>m`.
pub fn window_name(minutes: Option<i64>, short: bool) -> String {
    let m = minutes.unwrap_or(0);
    if m == 10080 {
        return if short { "wk" } else { "weekly window" }.into();
    }
    if m > 0 && m % 60 == 0 {
        let h = m / 60;
        return if short {
            format!("{h}h")
        } else {
            format!("{h}-hour window")
        };
    }
    if m > 0 {
        return if short {
            format!("{m}m")
        } else {
            format!("{m}-minute window")
        };
    }
    if short { "window" } else { "quota window" }.into()
}

/// `Format-QuotaPercent`: `96`, `96.5` (one decimal, banker's rounding as .NET's `Math.Round`).
pub fn format_percent(v: f64) -> String {
    let r = (v * 10.0).round_ties_even() / 10.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r:.1}")
    }
}

/// `Format-QuotaReset`: local `HH:mm` within 24 hours of now, else `MMM d` (`Oct 20`).
pub fn format_reset(when: DateTime<FixedOffset>, now: DateTime<Utc>) -> String {
    let local = to_local(when);
    let hours = (when.with_timezone(&Utc) - now).num_milliseconds().abs() as f64 / 3_600_000.0;
    if hours < 24.0 {
        local.format("%H:%M").to_string()
    } else {
        local.format("%b %-d").to_string()
    }
}

/// One judged window (`Get-QuotaWindowStates`).
#[derive(Debug, Clone)]
pub struct WindowState {
    /// `primary` | `secondary`.
    pub key: &'static str,
    pub name: String,
    pub short: String,
    pub used: f64,
    pub resets_at: Option<DateTime<FixedOffset>>,
    /// The reset lies in the past.
    pub reset: bool,
    /// At [`QUOTA_BLOCK_PERCENT`] or more with its reset ahead.
    pub blocks: bool,
    /// At [`QUOTA_WARN_PERCENT`] or more, not reset.
    pub low: bool,
}

/// `Get-QuotaWindowStates`: the windows of a recorded quota (`{primary, secondary}`), judged at
/// `now`.
pub fn window_states(quota: &Value, now: DateTime<Utc>) -> Vec<WindowState> {
    let mut out = Vec::new();
    if !quota.is_object() {
        return out;
    }
    for key in ["primary", "secondary"] {
        let w = &quota[key];
        if !w.is_object() {
            continue;
        }
        let used = match finite_percent(&w["used_percent"]) {
            Some(f) => f,
            None => continue,
        };
        let minutes = w["window_minutes"].as_i64().or_else(|| {
            w["window_minutes"]
                .as_str()
                .and_then(|s| s.trim().parse().ok())
        });
        let resets_at = when_offset(&w["resets_at"]);
        let reset = resets_at
            .map(|r| r.with_timezone(&Utc) <= now)
            .unwrap_or(false);
        out.push(WindowState {
            key,
            name: window_name(minutes, false),
            short: window_name(minutes, true),
            used,
            resets_at,
            reset,
            blocks: !reset && resets_at.is_some() && crate::retention::window_blocks(used),
            low: !reset && crate::retention::window_low(used),
        });
    }
    out
}

/// `Format-QuotaWindows`: `5h 96% until 18:32; wk 15% until Oct 20` (local times), a window whose
/// reset lies in the past `5h reset (last 96% until 18:32)`; `""` when the quota names no window.
pub fn format_windows(quota: &Value, now: DateTime<Utc>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for s in window_states(quota, now) {
        let pct = format!("{}%", format_percent(s.used));
        match s.resets_at {
            None => parts.push(format!("{} {pct}", s.short)),
            Some(r) if s.reset => parts.push(format!(
                "{} reset (last {pct} until {})",
                s.short,
                format_reset(r, now)
            )),
            Some(r) => parts.push(format!("{} {pct} until {}", s.short, format_reset(r, now))),
        }
    }
    parts.join("; ")
}

/// `Format-QuotaLowWarning`: `low quota: 5-hour window 84 % until 18:32` (the fullest window at
/// [`QUOTA_WARN_PERCENT`] or more), `""` otherwise.
pub fn format_low_warning(quota: &Value, now: DateTime<Utc>) -> String {
    let mut low: Vec<WindowState> = window_states(quota, now)
        .into_iter()
        .filter(|s| s.low)
        .collect();
    if low.is_empty() {
        return String::new();
    }
    low.sort_by(|a, b| {
        b.used
            .partial_cmp(&a.used)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let s = &low[0];
    let until = s
        .resets_at
        .map(|r| format!(" until {}", format_reset(r, now)))
        .unwrap_or_default();
    format!("low quota: {} {} %{until}", s.name, format_percent(s.used))
}

/// The blocking window with the latest reset (`New-QuotaWindowRecord`'s choice), `None` when none
/// blocks.
pub fn blocking_window(quota: &Value, now: DateTime<Utc>) -> Option<WindowState> {
    let mut b: Vec<WindowState> = window_states(quota, now)
        .into_iter()
        .filter(|s| s.blocks)
        .collect();
    b.sort_by_key(|x| std::cmp::Reverse(x.resets_at));
    b.into_iter().next()
}

// --------------------------------------------------------------------------- the endpoint's windows

/// The newest observed windows of an endpoint (`Get-EndpointWindow`).
#[derive(Debug, Clone)]
pub struct EndpointWindow {
    /// `{primary, secondary, observed_at}`.
    pub quota: Value,
    /// Derived: the latest of the per-window instants (display, the age of the record).
    pub observed_at: DateTime<FixedOffset>,
    /// The observation instant of each retained window (primary, secondary): the replacement
    /// decisions compare a candidate with THESE, never with `observed_at`.
    pub window_at: [Option<DateTime<FixedOffset>>; 2],
    /// A window whose instant is UNKNOWN (a record written before wave 8h: `window_at` holds the
    /// aggregate `observed_at`): it never wins on recency against a window with a known instant.
    pub window_legacy: [bool; 2],
    /// The best candidate window of each kind seen so far - `[known, legacy][window]`. The visible
    /// window is resolved from them (see [`select_window`]), so the pick is a function of the SET of
    /// candidates, whatever the fold order.
    slots: [[Option<WindowSlot>; 2]; 2],
    /// `recorded` (a ledger entry or a machine-wide record) | `rollout` (the codex home's newest).
    pub source: &'static str,
    pub fingerprint: String,
}

/// One candidate window kept by [`select_window`]: the window object and its observation instant.
#[derive(Debug, Clone)]
struct WindowSlot {
    win: Value,
    at: DateTime<FixedOffset>,
}

/// The account's windows as the newest rollout of a codex home shows them
/// (`Get-CodexAccountWindow`).
#[derive(Debug, Clone)]
pub struct AccountWindow {
    pub quota: Value,
    /// Derived (the latest per-window instant).
    pub observed_at: DateTime<FixedOffset>,
    /// The instant of each retained window (primary, secondary).
    pub window_at: [Option<DateTime<FixedOffset>>; 2],
    pub path: PathBuf,
}

/// The per-process cache of [`account_window`]: codex home -> (read at, the RAW observations of
/// the account's rollouts, file order). The selection is NOT cached: it runs at every lookup with
/// the requested instant, so a window that reset since the last lookup cannot be served stale.
type AccountCache = Mutex<HashMap<String, (Instant, Vec<(Observation, PathBuf)>)>>;

fn account_cache() -> &'static AccountCache {
    static C: OnceLock<AccountCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `Get-CodexAccountWindow`: the ChatGPT account's windows as the newest rollout of the codex home
/// shows them (every Codex session of the account writes them): the rollouts of today and yesterday
/// (local) - the 16 last written - of sessions on the built-in `openai` provider that name a window;
/// the newest OBSERVATION decides ([`newer_wins`]), not the file. Cached per process and codex home
/// for 60 s.
pub fn account_window(codex_home: &str) -> Option<AccountWindow> {
    account_window_at(codex_home, Utc::now())
}

/// [`account_window`] judged at the one comparison instant `now`.
pub fn account_window_at(codex_home: &str, now: DateTime<Utc>) -> Option<AccountWindow> {
    if codex_home.is_empty() {
        return None;
    }
    let cached = account_cache().lock().ok().and_then(|c| {
        c.get(codex_home)
            .filter(|(at, _)| at.elapsed().as_secs() < 60)
            .map(|(_, v)| v.clone())
    });
    let observations = match cached {
        Some(v) => v,
        None => {
            let v = read_account_observations(codex_home);
            if let Ok(mut c) = account_cache().lock() {
                c.insert(codex_home.to_string(), (Instant::now(), v.clone()));
            }
            v
        }
    };
    let mut best: Option<Observation> = None;
    let mut best_path = PathBuf::new();
    for (o, p) in &observations {
        let before = best.clone();
        best = merge_observation(best, Some(o), now);
        if best != before {
            best_path = p.clone();
        }
    }
    best.filter(|b| b.named() && b.at.is_some())
        .map(|b| AccountWindow {
            quota: b.to_quota(),
            observed_at: b.at.unwrap(),
            window_at: [
                b.primary.as_ref().and(b.window_at(0)),
                b.secondary.as_ref().and(b.window_at(1)),
            ],
            path: best_path,
        })
}

/// The observations of the account's last 16 rollouts (today and yesterday, local) on the built-in
/// `openai` provider that name a window - read from disk, no selection.
fn read_account_observations(codex_home: &str) -> Vec<(Observation, PathBuf)> {
    let mut out = Vec::new();
    let root = Path::new(codex_home).join("sessions");
    if !root.is_dir() {
        return out;
    }
    let today = Local::now().date_naive();
    let mut files: Vec<(SystemTime, PathBuf)> = Vec::new();
    for d in local_day_dirs(&root, today.pred_opt().unwrap_or(today), today) {
        rollouts_in(&d, None, &mut files);
    }
    files.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    let provider_re = regex::Regex::new(r#""model_provider"\s*:\s*"([^"]*)""#).unwrap();
    for (_, p) in files.into_iter().take(16) {
        let head = read_rollout_head(&p);
        if let Some(c) = provider_re.captures(&head) {
            if &c[1] != "openai" {
                continue;
            }
        }
        let u = rollout_usage(&p, None, ROLLOUT_TAIL_BYTES);
        if let Some(w) = u.window {
            out.push((w, p));
        }
    }
    out
}

/// The live source of the built-in openai endpoint's windows: set by the runtime when the
/// machine-wide health is on (`CODEX_CONSULT_HEALTH` not `none`) - the codex home whose newest
/// rollout [`endpoint_window`] reads. Unset (the default, and every unit test): the ledgers alone.
fn live_home() -> &'static Mutex<Option<String>> {
    static L: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(None))
}

/// Turn the live source on (`Some(codex home)`) or off (`None`) for this process.
pub fn set_live_codex_home(home: Option<String>) {
    if let Ok(mut l) = live_home().lock() {
        *l = home.filter(|h| !h.is_empty());
    }
}

/// The codex home of the live source, when it is on.
pub fn live_codex_home() -> Option<String> {
    live_home().lock().ok().and_then(|l| l.clone())
}

/// The used share of a recorded `quota` member (`primary` / `secondary`); `None` when it names
/// no window.
fn quota_used(q: &Value, key: &str) -> Option<f64> {
    if !q[key].is_object() {
        return None;
    }
    finite_percent(&q[key]["used_percent"])
}

/// The ONE selection of the endpoint window (recorded and live): a candidate strictly newer than
/// `best` replaces it; at an EQUAL `observed_at` (the ledger serialises without fractions, so ties
/// happen) the windows merge PER WINDOW - the higher primary `used_percent` keeps its own reset,
/// and independently the higher secondary - so a blocking weekly window is never hidden behind a
/// fuller 5-hour one. `source` of the result is the candidate's when it contributed a window.
fn select_window(
    best: Option<EndpointWindow>,
    cand_at: [Option<DateTime<FixedOffset>>; 2],
    cand_legacy: [bool; 2],
    quota: &Value,
    source: &'static str,
    fingerprint: String,
    now: DateTime<Utc>,
) -> Option<EndpointWindow> {
    const KEYS: [&str; 2] = ["primary", "secondary"];
    // a window is VALID with a finite share AND an instant; an invalid one never replaces
    let cand_valid = |k: usize| quota_used(quota, KEYS[k]).is_some() && cand_at[k].is_some();
    if !cand_valid(0) && !cand_valid(1) {
        return best;
    }
    // the total order WITHIN one kind of window: (floor second of the instant, then the 8d rank)
    let key = |w: &Value| {
        (
            finite_percent(&w["used_percent"]).unwrap_or(0.0),
            when_offset(&w["resets_at"]),
        )
    };
    let beats = |c: &WindowSlot, cur: &WindowSlot| match cmp_instants(c.at, cur.at) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            cmp_window_rank(key(&c.win), key(&cur.win), now) == std::cmp::Ordering::Greater
        }
    };
    let first = best.is_none();
    let mut out = best.unwrap_or_else(|| EndpointWindow {
        quota: json!({"primary": null, "secondary": null, "observed_at": quota["observed_at"].clone()}),
        observed_at: cand_at[0].or(cand_at[1]).unwrap_or_else(now_offset_fixed),
        window_at: [None, None],
        window_legacy: [false, false],
        slots: [[None, None], [None, None]],
        source,
        fingerprint: fingerprint.clone(),
    });
    let prev_observed = out.observed_at;
    let mut took = false;
    for k in 0..2 {
        if !cand_valid(k) {
            continue;
        }
        let Some(at) = cand_at[k] else { continue };
        let cand = WindowSlot {
            win: quota[KEYS[k]].clone(),
            at,
        };
        let kind = usize::from(cand_legacy[k]);
        let replace = match &out.slots[kind][k] {
            None => true,
            Some(cur) => beats(&cand, cur),
        };
        if replace {
            out.slots[kind][k] = Some(cand.clone());
        }
        // resolve the visible window: the best known one, unless the best legacy one outranks it
        // by the 8d rank alone (an unknown instant never wins on recency against a known one)
        let (known, legacy) = (&out.slots[0][k], &out.slots[1][k]);
        let (win, at, is_legacy) = match (known, legacy) {
            (Some(kn), Some(lg)) => {
                if cmp_window_rank(key(&lg.win), key(&kn.win), now) == std::cmp::Ordering::Greater {
                    (lg.win.clone(), lg.at, true)
                } else {
                    (kn.win.clone(), kn.at, false)
                }
            }
            (Some(kn), None) => (kn.win.clone(), kn.at, false),
            (None, Some(lg)) => (lg.win.clone(), lg.at, true),
            (None, None) => continue,
        };
        if replace && win == cand.win && at == cand.at && is_legacy == cand_legacy[k] {
            took = true;
        }
        out.quota[KEYS[k]] = win;
        out.window_at[k] = Some(at);
        out.window_legacy[k] = is_legacy;
    }
    if let Some(l) = out.window_at.iter().flatten().max().copied() {
        out.observed_at = l;
    }
    if took || first {
        out.source = source;
        if first || out.observed_at > prev_observed {
            out.quota["observed_at"] = quota["observed_at"].clone();
            out.fingerprint = fingerprint;
        }
    }
    stamp_window_instants(&mut out);
    Some(out)
}

fn now_offset_fixed() -> DateTime<FixedOffset> {
    Utc::now().fixed_offset()
}

/// Write each retained window's instant into the selected `quota` (the keys of [`WINDOW_AT_KEYS`]).
fn stamp_window_instants(w: &mut EndpointWindow) {
    for (k, key) in WINDOW_AT_KEYS.iter().enumerate() {
        match (
            w.window_at[k],
            quota_used(&w.quota, ["primary", "secondary"][k]),
        ) {
            (Some(t), Some(_)) if !w.window_legacy[k] => {
                w.quota[*key] = Value::String(iso_instant(to_local(t)))
            }
            _ => {
                if let Some(o) = w.quota.as_object_mut() {
                    o.remove(*key);
                }
            }
        }
    }
}

/// `Get-EndpointWindow`: every consultation (ledger entry or machine-wide record) of `fp_set` that
/// carries a `quota` naming a window (its `observed_at`, else the entry's `finished_at`, else its
/// `when`, orders them) and - when the live source is on and the set holds the built-in openai
/// endpoint - the newest rollout of the codex home ([`account_window`]).
pub fn endpoint_window(
    consults: &[Value],
    fp_set: &[&str],
    builtin_fingerprint: &str,
) -> Option<EndpointWindow> {
    endpoint_window_at(consults, fp_set, builtin_fingerprint, Utc::now())
}

/// [`endpoint_window`] at ONE comparison instant `now` - the instant of every comparison of the
/// selection and (the caller's) "reset in the past" verdict.
pub fn endpoint_window_at(
    consults: &[Value],
    fp_set: &[&str],
    builtin_fingerprint: &str,
    now: DateTime<Utc>,
) -> Option<EndpointWindow> {
    let mut best: Option<EndpointWindow> = None;
    for c in consults {
        let q = &c["quota"];
        if !q.is_object() || (q["primary"].is_null() && q["secondary"].is_null()) {
            continue;
        }
        let rev = &c["reviewer"];
        let fp = if rev.is_null() {
            builtin_fingerprint.to_string()
        } else {
            rev["provider_fingerprint"]
                .as_str()
                .unwrap_or("")
                .to_string()
        };
        if fp.is_empty() || !fp_set.contains(&fp.as_str()) {
            continue;
        }
        // the aggregate instant is the LEGACY fallback (a record written before wave 8h)
        let legacy = when_offset(&q["observed_at"])
            .or_else(|| when_offset(&c["finished_at"]))
            .or_else(|| when_offset(&c["when"]));
        let own = [
            when_offset(&q[WINDOW_AT_KEYS[0]]),
            when_offset(&q[WINDOW_AT_KEYS[1]]),
        ];
        let ats = [own[0].or(legacy), own[1].or(legacy)];
        if ats[0].is_none() && ats[1].is_none() {
            continue;
        }
        let unknown = [own[0].is_none(), own[1].is_none()];
        best = select_window(best, ats, unknown, q, "recorded", fp, now);
    }
    if fp_set.contains(&builtin_fingerprint) {
        if let Some(home) = live_codex_home() {
            if let Some(live) = account_window_at(&home, now) {
                best = select_window(
                    best,
                    live.window_at,
                    [false, false],
                    &live.quota,
                    "rollout",
                    builtin_fingerprint.to_string(),
                    now,
                );
            }
        }
    }
    best
}

// --------------------------------------------------------------------------- the budget watch

/// The 5-hour (primary) window in a rollout's end, for the budget watch
/// (`Read-RolloutPrimaryWindow`): `used` `None` when the rollout is not there (yet) or names no
/// primary window.
#[derive(Debug, Clone, PartialEq)]
pub struct PrimaryWindow {
    pub used: Option<f64>,
    pub resets_at: Option<DateTime<FixedOffset>>,
    pub name: String,
    pub path: PathBuf,
    pub at: Option<DateTime<FixedOffset>>,
}

/// `Read-RolloutPrimaryWindow`.
pub fn read_primary_window(path: Option<&Path>) -> PrimaryWindow {
    let mut r = PrimaryWindow {
        used: None,
        resets_at: None,
        name: "5-hour window".into(),
        path: path.map(Path::to_path_buf).unwrap_or_default(),
        at: None,
    };
    let path = match path {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => return r,
    };
    let u = rollout_usage(path, None, ROLLOUT_TAIL_BYTES);
    let w = match u.window {
        Some(w) => w,
        None => return r,
    };
    let p = match &w.primary {
        Some(p) => p,
        None => return r,
    };
    r.at = w.at;
    r.used = Some(p.used_percent);
    r.resets_at = p
        .resets_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok());
    r.name = window_name(p.window_minutes, false);
    r
}

/// Whether a watched window stops the turn: an observation of THIS turn (not older than
/// `turn_start`) at `budget` percent or more, its reset ahead (or unknown).
pub fn budget_reached(
    w: &PrimaryWindow,
    budget: i64,
    turn_start: DateTime<FixedOffset>,
    now: DateTime<Utc>,
) -> bool {
    match (w.used, w.at) {
        (Some(u), Some(at)) => {
            budget > 0
                && u >= budget as f64
                && at >= turn_start
                && w.resets_at
                    .map(|r| r.with_timezone(&Utc) > now)
                    .unwrap_or(true)
        }
        _ => false,
    }
}

/// The stop text of a run stopped at its budget: `stopped at the quota budget: 5-hour window 91 %
/// used, resets 18:32; the partial is kept`.
pub fn budget_stop_text(w: &PrimaryWindow, now: DateTime<Utc>) -> String {
    let resets = w
        .resets_at
        .map(|r| format!(", resets {}", format_reset(r, now)))
        .unwrap_or_default();
    format!(
        "stopped at the quota budget: {} {} % used{resets}; the partial is kept",
        w.name,
        format_percent(w.used.unwrap_or(0.0))
    )
}

/// `Get-ThreadIdFromStreamHead`: the thread id of a codex event stream's first line
/// (`{"type":"thread.started","thread_id":"<uuid>"}`); `""` when not there yet.
pub fn thread_id_from_stream_head(path: &Path) -> String {
    if !path.is_file() {
        return String::new();
    }
    let head = read_rollout_head(path);
    let re =
        regex::Regex::new(&format!(r#""thread_id"\s*:\s*"({})""#, uuid_re().as_str())).unwrap();
    re.captures(&head)
        .map(|c| c[1].to_string())
        .unwrap_or_default()
}

/// The budget of a run: the percent (0 = off) and its source (`-Budget`,
/// `CODEX_CONSULT_QUOTA_BUDGET`, `default`, or a panel's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    pub percent: i64,
    pub source: String,
}

/// Resolve `-Budget` / `CODEX_CONSULT_QUOTA_BUDGET` / the default (`codex-consult.ps1`): `Err`
/// with the plugin's refusal text for a value outside 0..100 or a malformed variable.
pub fn resolve_budget(given: Option<i64>, env: Option<&str>) -> Result<Budget, String> {
    if let Some(b) = given {
        if !(0..=100).contains(&b) {
            return Err(format!(
                "-Budget must be 0 (off) or a share of the 5-hour window from 1 to 100 percent (got {b})."
            ));
        }
        return Ok(Budget {
            percent: b,
            source: "-Budget".into(),
        });
    }
    let raw = env.map(str::trim).unwrap_or("");
    if raw.is_empty() {
        return Ok(Budget {
            percent: QUOTA_BUDGET_DEFAULT,
            source: "default".into(),
        });
    }
    match raw.parse::<i64>() {
        Ok(v) if (0..=100).contains(&v) => Ok(Budget {
            percent: v,
            source: "CODEX_CONSULT_QUOTA_BUDGET".into(),
        }),
        _ => Err(format!(
            "CODEX_CONSULT_QUOTA_BUDGET must be 0 (off) or a share of the 5-hour window from 1 to 100 percent (got '{raw}')."
        )),
    }
}

// --------------------------------------------------------------------------- the replay guard

/// `Get-ForkReplayGuard`'s result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForkGuard {
    pub path: Option<PathBuf>,
    pub bytes: u64,
    pub kb: i64,
    /// `CODEX_CONSULT_FORK_MAX_KB` (default 512; 0 = no refusal).
    pub max_kb: i64,
    pub over_limit: bool,
    /// `""` or `the parent thread <id> replays <N> KB (~<N/4> k tokens) on EVERY tool call of the
    /// run: use -Mode new, or raise CODEX_CONSULT_FORK_MAX_KB (now <max>)`.
    pub refusal: String,
    /// `""` or the same from 128 KB up to the limit.
    pub warning: String,
    /// `""` or why the variable was ignored.
    pub problem: String,
}

/// .NET's `[Math]::Round` (to even) of a non-negative ratio, as an integer.
fn round_even(v: f64) -> i64 {
    v.round_ties_even() as i64
}

/// `Get-ForkReplayGuard`: the parent thread's rollout is replayed on EVERY tool call of a codex fork
/// or resume. A rollout that cannot be found is neither refused nor warned about. `max_kb_env`: the
/// raw `CODEX_CONSULT_FORK_MAX_KB`.
pub fn fork_replay_guard(thread: &str, codex_home: &str, max_kb_env: Option<&str>) -> ForkGuard {
    let mut g = ForkGuard {
        max_kb: FORK_MAX_KB_DEFAULT,
        ..Default::default()
    };
    let raw = max_kb_env.map(str::trim).unwrap_or("");
    if !raw.is_empty() {
        match raw.parse::<i32>() {
            Ok(v) if v >= 0 => g.max_kb = v as i64,
            _ => {
                g.problem = format!(
                    "CODEX_CONSULT_FORK_MAX_KB='{raw}' is not a number of KB (0 = no limit) - the default 512 applies"
                )
            }
        }
    }
    if thread.is_empty() {
        return g;
    }
    let path = match codex_rollout_path(thread, codex_home) {
        Some(p) => p,
        None => return g,
    };
    g.bytes = match std::fs::metadata(&path) {
        Ok(m) => m.len(),
        Err(_) => {
            g.path = Some(path);
            return g;
        }
    };
    g.path = Some(path);
    g.kb = round_even(g.bytes as f64 / 1024.0);
    let tokens_k = round_even(g.kb as f64 / 4.0);
    let what = format!(
        "the parent thread {thread} replays {} KB (~{tokens_k} k tokens) on EVERY tool call of the run",
        g.kb
    );
    if g.max_kb > 0 && g.kb > g.max_kb {
        g.over_limit = true;
        g.refusal = format!(
            "{what}: use -Mode new, or raise CODEX_CONSULT_FORK_MAX_KB (now {})",
            g.max_kb
        );
    } else if g.kb >= FORK_WARN_KB {
        let lim = if g.max_kb > 0 {
            g.max_kb.to_string()
        } else {
            "off".into()
        };
        g.warning = format!(
            "{what} (CODEX_CONSULT_FORK_MAX_KB {lim}); -Mode new starts without that replay"
        );
    }
    g
}

/// Whether the user's `-c` items name `features.multi_agent` (the last one wins): `Some(allowed)`,
/// else `None` (the bridge then passes [`MULTI_AGENT_OFF`]).
pub fn user_multi_agent(extra_config: &[String]) -> Option<bool> {
    let re = regex::Regex::new(r"^features\.multi_agent\s*=").unwrap();
    let last = extra_config.iter().rev().find(|c| re.is_match(c.trim()))?;
    let t = last.trim();
    Some(regex::Regex::new(r"=\s*true$").unwrap().is_match(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialises the tests that turn the process-wide live source on, and those that read
    /// `endpoint_window` (which would see it).
    static LIVE: Mutex<()> = Mutex::new(());

    fn t_endpoint_window(c: &[Value], fp: &[&str], b: &str) -> Option<EndpointWindow> {
        let _g = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        endpoint_window(c, fp, b)
    }

    fn t_endpoint_window_at(
        c: &[Value],
        fp: &[&str],
        b: &str,
        now: DateTime<Utc>,
    ) -> Option<EndpointWindow> {
        let _g = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        endpoint_window_at(c, fp, b, now)
    }

    fn stamp(t: DateTime<Utc>) -> String {
        t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    }

    fn win(used: f64, reset: DateTime<Utc>, minutes: i64) -> String {
        format!(
            "{{\"used_percent\":{used:.1},\"window_minutes\":{minutes},\"resets_at\":{}}}",
            reset.timestamp()
        )
    }

    /// A `token_count` line as codex-cli 0.155 (and the plugin's harness) writes it.
    // kept: test-only fixture builder with ~19 positional call sites; a struct would bloat every
    // call site without any production benefit.
    #[allow(clippy::too_many_arguments)]
    fn token_line(
        at: DateTime<Utc>,
        input: i64,
        cached: i64,
        out: i64,
        last: i64,
        primary: &str,
        secondary: &str,
        limit_id: &str,
    ) -> String {
        format!(
            "{{\"timestamp\":\"{}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":{input},\"cached_input_tokens\":{cached},\"cache_write_input_tokens\":0,\"output_tokens\":{out},\"reasoning_output_tokens\":0,\"total_tokens\":{}}},\"last_token_usage\":{{\"input_tokens\":{last},\"cached_input_tokens\":0,\"cache_write_input_tokens\":0,\"output_tokens\":5,\"reasoning_output_tokens\":0,\"total_tokens\":{last}}},\"model_context_window\":258400}},\"rate_limits\":{{\"limit_id\":\"{limit_id}\",\"limit_name\":null,\"primary\":{primary},\"secondary\":{secondary},\"credits\":null,\"plan_type\":\"plus\"}}}}}}",
            stamp(at),
            input + out
        )
    }

    fn meta_line(at: DateTime<Utc>, id: &str, parent: &str) -> String {
        let src = if parent.is_empty() {
            "\"exec\"".to_string()
        } else {
            format!(
                "{{\"subagent\":{{\"thread_spawn\":{{\"parent_thread_id\":\"{parent}\",\"depth\":1}}}}}}"
            )
        };
        format!(
            "{{\"timestamp\":\"{}\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"source\":{src},\"model_provider\":\"openai\"}}}}",
            stamp(at)
        )
    }

    fn v7(at: DateTime<Utc>, salt: u32) -> String {
        let ms = format!("{:012x}", at.timestamp_millis());
        format!(
            "{}-{}-7{:03x}-8{:03x}-{:012x}",
            &ms[0..8],
            &ms[8..12],
            salt & 0xfff,
            (salt >> 3) & 0xfff,
            salt as u64 * 7919
        )
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "c3-quota-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_rollout(home: &Path, id: &str, lines: &[String], at: DateTime<Utc>) -> PathBuf {
        let l = at.with_timezone(&Local);
        let d = home
            .join("sessions")
            .join(format!("{:04}", l.year()))
            .join(format!("{:02}", l.month()))
            .join(format!("{:02}", l.day()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(format!(
            "rollout-{}-{id}.jsonl",
            l.format("%Y-%m-%dT%H-%M-%S")
        ));
        std::fs::write(&p, format!("{}\n", lines.join("\n"))).unwrap();
        p
    }

    // UNIT (harness-quota): the rollout reader on a fork, its parent and a sub-agent.
    #[test]
    fn the_rollout_reader_follows_the_plugin() {
        let home = scratch("reader");
        let h = home.to_string_lossy().to_string();
        let now = Utc::now();
        let reset5 = now + Duration::minutes(150);
        let reset_wk = now + Duration::days(5);
        let parent = v7(now - Duration::hours(2), 1);
        let p_lines = vec![
            meta_line(now - Duration::hours(2), &parent, ""),
            token_line(
                now - Duration::hours(2),
                100000,
                80000,
                500,
                50000,
                "null",
                "null",
                "codex",
            ),
            token_line(
                now - Duration::hours(2) + Duration::seconds(1),
                100000,
                80000,
                500,
                50000,
                "null",
                "null",
                "premium",
            ),
            token_line(
                now - Duration::hours(1),
                200000,
                160000,
                900,
                100000,
                &win(20.0, reset5, 300),
                &win(10.0, reset_wk, 10080),
                "codex",
            ),
        ];
        write_rollout(&home, &parent, &p_lines, now - Duration::hours(2));
        let start = now - Duration::minutes(5);
        let fork = v7(start, 2);
        let f_lines = vec![
            meta_line(start, &fork, ""),
            token_line(
                start + Duration::seconds(5),
                260000,
                200000,
                950,
                60000,
                &win(90.0, reset5, 300),
                &win(12.0, reset_wk, 10080),
                "codex",
            ),
            token_line(
                start + Duration::seconds(5),
                260000,
                200000,
                950,
                60000,
                "null",
                "null",
                "premium",
            ),
            token_line(
                start + Duration::seconds(9),
                330000,
                260000,
                990,
                70000,
                &win(96.0, reset5, 300),
                &win(15.0, reset_wk, 10080),
                "codex",
            ),
            token_line(
                start + Duration::seconds(9),
                330000,
                260000,
                990,
                70000,
                "null",
                "null",
                "premium",
            ),
        ];
        let f_path = write_rollout(&home, &fork, &f_lines, start);
        let sub = v7(start + Duration::seconds(6), 3);
        let s_lines = vec![
            meta_line(start + Duration::seconds(6), &sub, &fork),
            token_line(
                start + Duration::seconds(7),
                30000,
                10000,
                50,
                30000,
                &win(97.0, reset5, 300),
                &win(15.0, reset_wk, 10080),
                "codex",
            ),
            token_line(
                start + Duration::seconds(11),
                70000,
                40000,
                90,
                40000,
                &win(99.0, reset5, 300),
                &win(16.0, reset_wk, 10080),
                "codex",
            ),
        ];
        write_rollout(&home, &sub, &s_lines, start + Duration::seconds(6));

        assert_eq!(
            codex_rollout_path(&fork, &h).as_deref(),
            Some(f_path.as_path())
        );
        let v4 = "0b1e2a33-4c5d-4e6f-8a9b-0c1d2e3f4a5b";
        let v4_path = write_rollout(
            &home,
            v4,
            &[meta_line(now, v4, "")],
            now - Duration::days(20),
        );
        assert_eq!(
            codex_rollout_path(v4, &h).as_deref(),
            Some(v4_path.as_path())
        );
        assert_eq!(
            codex_rollout_path("6a1e2a33-4c5d-4e6f-8a9b-0c1d2e3f4a5b", &h),
            None
        );

        let u = rollout_usage(&f_path, Some(start.fixed_offset()), 0);
        let w = u.window.clone().unwrap();
        assert_eq!(w.primary.as_ref().unwrap().used_percent, 96.0);
        assert_eq!(w.secondary.as_ref().unwrap().used_percent, 15.0);
        assert_eq!(w.primary.as_ref().unwrap().window_minutes, Some(300));
        assert_eq!(u.total.unwrap().input, Some(330000));
        assert_eq!(u.calls, 2);
        assert_eq!(u.max_input, Some(70000));
        assert_eq!(u.context_window, Some(258400));
        // resets_at: ISO with the LOCAL offset, from unix seconds
        let want = iso(Utc
            .timestamp_opt(reset5.timestamp(), 0)
            .unwrap()
            .with_timezone(&Local)
            .fixed_offset());
        assert_eq!(
            w.primary.as_ref().unwrap().resets_at.as_deref(),
            Some(want.as_str())
        );

        let q = codex_run_quota(&fork, start, &parent, &h);
        let t = q.tokens.unwrap();
        assert_eq!(t["input"], 130000);
        assert_eq!(t["cached"], 100000);
        assert_eq!(t["output"], 90);
        assert_eq!(t["thread_input"], 330000);
        assert_eq!(t["calls"], 2);
        assert_eq!(t["max_input"], 70000);
        assert_eq!(t["context_window"], 258400);
        assert_eq!(
            t["subagents"],
            json!({"count": 1, "input": 70000, "calls": 2})
        );
        // the NEWEST window decides: the sub-agent's 99 % observed after the thread's 96 %
        assert_eq!(q.quota["primary"]["used_percent"], 99.0);
        assert!(q.quota["observed_at"].is_string());

        let qn = codex_run_quota(&parent, now - Duration::hours(3), "", &h);
        assert_eq!(qn.tokens.unwrap()["input"], 200000);
        let qx = codex_run_quota("6a1e2a33-4c5d-4e6f-8a9b-0c1d2e3f4a5b", now, "", &h);
        assert!(qx.tokens.is_none());
        assert_eq!(qx.quota, null_quota());
        let keys: Vec<&String> = qx.quota.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["primary", "secondary", "observed_at"]);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_rollout_without_windows_reads_null_windows() {
        let home = scratch("null");
        let now = Utc::now();
        let id = v7(now, 9);
        let p = write_rollout(
            &home,
            &id,
            &[
                meta_line(now, "x", ""),
                token_line(now, 10, 0, 1, 10, "null", "null", "codex"),
            ],
            now,
        );
        let u = rollout_usage(&p, None, 0);
        let w = u.window.unwrap();
        assert!(w.primary.is_none() && w.secondary.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_views_at_a_fixed_clock() {
        let clock: DateTime<Utc> = "2026-10-13T12:00:00Z".parse().unwrap();
        let q = json!({
            "primary": {"used_percent": 96.0, "window_minutes": 300, "resets_at": "2026-10-13T16:32:16+00:00"},
            "secondary": {"used_percent": 15.0, "window_minutes": 10080, "resets_at": "2026-10-20T11:32:16+00:00"},
            "observed_at": "2026-10-13T11:35:49+00:00"
        });
        let lt = to_local("2026-10-13T16:32:16+00:00".parse().unwrap())
            .format("%H:%M")
            .to_string();
        let ld = to_local("2026-10-20T11:32:16+00:00".parse().unwrap())
            .format("%b %-d")
            .to_string();
        assert_eq!(
            format_windows(&q, clock),
            format!("5h 96% until {lt}; wk 15% until {ld}")
        );
        let past: DateTime<Utc> = "2026-10-13T17:00:00Z".parse().unwrap();
        assert!(format_windows(&q, past)
            .starts_with(&format!("5h reset (last 96% until {lt}); wk 15% until ")));
        assert_eq!(format_percent(96.5), "96.5");
        assert_eq!(format_percent(42.0), "42");
        assert_eq!(window_name(Some(300), false), "5-hour window");
        assert_eq!(window_name(Some(10080), true), "wk");
        assert_eq!(window_name(Some(90), false), "90-minute window");
        let q84 = json!({"primary": {"used_percent": 84.0, "window_minutes": 300, "resets_at": "2026-10-13T16:32:16+00:00"}, "secondary": null});
        assert_eq!(
            format_low_warning(&q84, clock),
            format!("low quota: 5-hour window 84 % until {lt}")
        );
        let q99 = json!({"primary": {"used_percent": 99.0, "window_minutes": 300, "resets_at": "2026-10-13T16:32:16+00:00"}});
        let b = blocking_window(&q99, clock).unwrap();
        assert_eq!(b.name, "5-hour window");
        assert!(blocking_window(&q99, past).is_none());
        assert!(format_low_warning(&q99, past).is_empty());
    }

    fn tie_consult(used: f64, at: &str, reset: DateTime<Utc>) -> Value {
        json!({
            "finished_at": at,
            "quota": {
                "primary": {"used_percent": used, "window_minutes": 300, "resets_at": reset.to_rfc3339()},
                "secondary": null,
                "observed_at": at
            }
        })
    }

    // UNIT (F46-1): equal observed_at - the fuller primary window wins, in either order.
    #[test]
    fn endpoint_window_at_an_equal_time_keeps_the_fuller_window_in_both_orders() {
        let reset = Utc::now() + Duration::minutes(120);
        let at = "2026-10-13T11:35:49+00:00";
        let lo = tie_consult(30.0, at, reset);
        let hi = tie_consult(99.0, at, reset);
        for order in [vec![lo.clone(), hi.clone()], vec![hi, lo]] {
            let w = t_endpoint_window(&order, &["fp"], "fp").unwrap();
            assert_eq!(w.source, "recorded");
            assert_eq!(w.quota["primary"]["used_percent"], 99.0);
            let b = blocking_window(&w.quota, Utc::now()).unwrap();
            assert_eq!(b.name, "5-hour window");
        }
    }

    // UNIT (F46-1): the live rollout at the very instant of a lower recorded observation wins.
    #[test]
    fn the_live_window_replaces_a_lower_recorded_one_at_an_equal_time() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let home = scratch("tie-live");
        let h = home.to_string_lossy().to_string();
        let now = Utc::now();
        let at = DateTime::<Utc>::from_timestamp(now.timestamp(), 0).unwrap();
        let reset = now + Duration::minutes(120);
        let id = v7(at, 7);
        let lines = vec![
            meta_line(at, &id, ""),
            token_line(
                at,
                10,
                0,
                1,
                10,
                &win(99.0, reset, 300),
                &win(10.0, reset + Duration::days(5), 10080),
                "codex",
            ),
        ];
        write_rollout(&home, &id, &lines, at);
        let rec = tie_consult(30.0, &at.to_rfc3339(), reset);
        set_live_codex_home(Some(h));
        let w = super::endpoint_window(&[rec], &["fp"], "fp");
        set_live_codex_home(None);
        let _ = std::fs::remove_dir_all(&home);
        let w = w.unwrap();
        assert_eq!(w.source, "rollout");
        assert_eq!(w.quota["primary"]["used_percent"], 99.0);
    }

    fn cross_consult(p: f64, w: f64, at: &str, reset: DateTime<Utc>) -> Value {
        json!({
            "finished_at": at,
            "quota": {
                "primary": {"used_percent": p, "window_minutes": 300, "resets_at": reset.to_rfc3339()},
                "secondary": {"used_percent": w, "window_minutes": 10080, "resets_at": (reset + Duration::days(3)).to_rfc3339()},
                "observed_at": at
            }
        })
    }

    // UNIT (F48-1): at an equal time each window is merged on its own, in both orders.
    #[test]
    fn endpoint_window_at_an_equal_time_merges_per_window_in_both_orders() {
        let reset = Utc::now() + Duration::minutes(120);
        let at = "2026-10-13T11:35:49+00:00";
        let a = cross_consult(40.0, 99.0, at, reset);
        let b = cross_consult(50.0, 20.0, at, reset + Duration::minutes(7));
        for order in [vec![a.clone(), b.clone()], vec![b, a]] {
            let w = t_endpoint_window(&order, &["fp"], "fp").unwrap();
            assert_eq!(w.quota["primary"]["used_percent"], 50.0);
            assert_eq!(w.quota["secondary"]["used_percent"], 99.0);
            let blocking = blocking_window(&w.quota, Utc::now()).unwrap();
            assert_eq!(blocking.name, "weekly window");
        }
    }

    // UNIT (F48-1): the live rollout crossing a recorded observation, either way round.
    #[test]
    fn the_live_window_merges_per_window_with_a_crossing_recorded_one() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        for (live_p, live_w, rec_p, rec_w) in [(50.0, 20.0, 40.0, 99.0), (40.0, 99.0, 50.0, 20.0)] {
            let home = scratch("cross-live");
            let h = home.to_string_lossy().to_string();
            let now = Utc::now();
            let at = DateTime::<Utc>::from_timestamp(now.timestamp(), 0).unwrap();
            let reset = now + Duration::minutes(120);
            let id = v7(at, 9);
            let lines = vec![
                meta_line(at, &id, ""),
                token_line(
                    at,
                    10,
                    0,
                    1,
                    10,
                    &win(live_p, reset, 300),
                    &win(live_w, reset + Duration::days(3), 10080),
                    "codex",
                ),
            ];
            write_rollout(&home, &id, &lines, at);
            let rec = cross_consult(rec_p, rec_w, &at.to_rfc3339(), reset);
            set_live_codex_home(Some(h));
            let w = super::endpoint_window(&[rec], &["fp"], "fp");
            set_live_codex_home(None);
            let _ = std::fs::remove_dir_all(&home);
            let w = w.unwrap();
            assert_eq!(w.quota["primary"]["used_percent"], 50.0);
            assert_eq!(w.quota["secondary"]["used_percent"], 99.0);
        }
    }

    fn rq(
        p: Option<(f64, Option<DateTime<Utc>>)>,
        w: Option<(f64, Option<DateTime<Utc>>)>,
        at: &str,
    ) -> Value {
        let m = |x: Option<(f64, Option<DateTime<Utc>>)>, min: i64| match x {
            None => Value::Null,
            Some((u, r)) => json!({
                "used_percent": u, "window_minutes": min,
                "resets_at": r.map(|r| r.to_rfc3339())
            }),
        };
        json!({"finished_at": at, "quota": {"primary": m(p, 300), "secondary": m(w, 10080), "observed_at": at}})
    }

    fn obs_of(c: &Value, at: DateTime<FixedOffset>) -> Observation {
        let win = |k: &str| {
            let w = &c["quota"][k];
            let used = finite_percent(&w["used_percent"])?;
            Some(Window {
                used_percent: used,
                window_minutes: w["window_minutes"].as_i64(),
                resets_at: w["resets_at"].as_str().map(|x| x.to_string()),
            })
        };
        Observation {
            primary: win("primary"),
            secondary: win("secondary"),
            at: Some(at),
            primary_at: None,
            secondary_at: None,
            limit_id: "codex".into(),
        }
    }

    fn ts_of(r: Option<&str>) -> Option<i64> {
        r.and_then(|r| DateTime::parse_from_rfc3339(r).ok())
            .map(|t| t.timestamp())
    }

    // UNIT (F50-1): equal time, equal share - a future reset beats a past one beats a null one,
    // in both orders, for the primary and the secondary window, recorded and merge_observation.
    #[test]
    fn equal_shares_rank_by_the_reset_in_every_order() {
        let now = Utc::now();
        let fut = Some(now + Duration::minutes(90));
        let past = Some(now - Duration::minutes(90));
        let at = "2026-10-13T11:35:49+00:00";
        let atd: DateTime<FixedOffset> = at.parse().unwrap();
        for (lo, hi) in [(None, fut), (past, fut), (None, past)] {
            for on_primary in [true, false] {
                let mk = |r: Option<DateTime<Utc>>| {
                    if on_primary {
                        rq(Some((99.0, r)), None, at)
                    } else {
                        rq(None, Some((99.0, r)), at)
                    }
                };
                let (a, b) = (mk(lo), mk(hi));
                let k = if on_primary { "primary" } else { "secondary" };
                for order in [vec![a.clone(), b.clone()], vec![b.clone(), a.clone()]] {
                    let w = t_endpoint_window(&order, &["fp"], "fp").unwrap();
                    assert_eq!(
                        ts_of(w.quota[k]["resets_at"].as_str()),
                        hi.map(|t| t.timestamp())
                    );
                    let m = merge_observation(
                        Some(obs_of(&order[0], atd)),
                        Some(&obs_of(&order[1], atd)),
                        Utc::now(),
                    )
                    .unwrap();
                    let mw = if on_primary { m.primary } else { m.secondary }.unwrap();
                    assert_eq!(ts_of(mw.resets_at.as_deref()), hi.map(|t| t.timestamp()));
                    if hi == fut {
                        assert!(blocking_window(&w.quota, Utc::now()).is_some());
                    }
                }
            }
        }
    }

    // UNIT (F50-1): recorded/live assignments of tied 99 % windows (null vs future reset).
    #[test]
    fn the_live_window_ranks_a_future_reset_over_a_null_one_at_a_tie() {
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        for live_null in [true, false] {
            let home = scratch("tie-reset-live");
            let h = home.to_string_lossy().to_string();
            let now = Utc::now();
            let at = DateTime::<Utc>::from_timestamp(now.timestamp(), 0).unwrap();
            let reset = now + Duration::minutes(120);
            let id = v7(at, 11);
            let primary = if live_null {
                String::from(r#"{"used_percent":99.0,"window_minutes":300,"resets_at":null}"#)
            } else {
                win(99.0, reset, 300)
            };
            let lines = vec![
                meta_line(at, &id, ""),
                token_line(at, 10, 0, 1, 10, &primary, "null", "codex"),
            ];
            write_rollout(&home, &id, &lines, at);
            let rec_reset = if live_null { Some(reset) } else { None };
            let rec = rq(Some((99.0, rec_reset)), None, &at.to_rfc3339());
            set_live_codex_home(Some(h));
            let w = super::endpoint_window(&[rec], &["fp"], "fp");
            set_live_codex_home(None);
            let _ = std::fs::remove_dir_all(&home);
            let w = w.unwrap();
            assert!(
                blocking_window(&w.quota, Utc::now()).is_some(),
                "live_null={live_null}"
            );
        }
    }

    // UNIT (F50-1): a fuller share with a past reset does not hide a future-reset one.
    #[test]
    fn a_past_reset_never_hides_a_future_one_whatever_the_share() {
        let now = Utc::now();
        let at = "2026-10-13T11:35:49+00:00";
        let a = rq(Some((99.5, Some(now - Duration::minutes(5)))), None, at);
        let b = rq(Some((99.0, Some(now + Duration::minutes(60)))), None, at);
        for order in [vec![a.clone(), b.clone()], vec![b, a]] {
            let w = t_endpoint_window(&order, &["fp"], "fp").unwrap();
            assert_eq!(w.quota["primary"]["used_percent"], 99.0);
            assert!(blocking_window(&w.quota, Utc::now()).is_some());
        }
    }

    fn permutations4(cur: &mut Vec<usize>, used: &mut [bool; 4], out: &mut Vec<Vec<usize>>) {
        if cur.len() == 4 {
            out.push(cur.clone());
            return;
        }
        for i in 0..4 {
            if !used[i] {
                used[i] = true;
                cur.push(i);
                permutations4(cur, used, out);
                cur.pop();
                used[i] = false;
            }
        }
    }

    // UNIT (F50-1): the result is the same for all 24 orders of four tied observations.
    #[test]
    fn the_tie_selection_is_the_same_for_every_permutation() {
        let now = Utc::now();
        let at = "2026-10-13T11:35:49+00:00";
        let atd: DateTime<FixedOffset> = at.parse().unwrap();
        let obs = [
            rq(
                Some((99.0, None)),
                Some((20.0, Some(now + Duration::days(2)))),
                at,
            ),
            rq(
                Some((99.5, Some(now - Duration::minutes(3)))),
                Some((98.0, None)),
                at,
            ),
            rq(
                Some((99.0, Some(now + Duration::minutes(30)))),
                Some((98.0, Some(now - Duration::hours(1)))),
                at,
            ),
            rq(
                Some((60.0, Some(now + Duration::minutes(80)))),
                Some((98.0, Some(now + Duration::days(3)))),
                at,
            ),
        ];
        let mut perms = vec![];
        permutations4(&mut vec![], &mut [false; 4], &mut perms);
        assert_eq!(perms.len(), 24);
        let norm = |q: &Value| {
            let f = |k: &str| {
                (
                    q[k]["used_percent"].as_f64(),
                    ts_of(q[k]["resets_at"].as_str()),
                )
            };
            (f("primary"), f("secondary"))
        };
        let mut first = None;
        for perm in &perms {
            let list: Vec<Value> = perm.iter().map(|i| obs[*i].clone()).collect();
            let w = t_endpoint_window(&list, &["fp"], "fp").unwrap();
            let n = norm(&w.quota);
            let mut m = None;
            for c in &list {
                m = merge_observation(m, Some(&obs_of(c, atd)), Utc::now());
            }
            let m = m.unwrap();
            let mk = |w: Option<Window>| {
                let w = w.unwrap();
                (Some(w.used_percent), ts_of(w.resets_at.as_deref()))
            };
            let mn = (mk(m.primary), mk(m.secondary));
            assert_eq!(n, mn);
            match &first {
                None => first = Some(n),
                Some(f) => assert_eq!(&n, f),
            }
        }
        // the winners: the primary is the future-reset 99, the secondary the future-reset 98
        let f = first.unwrap();
        assert_eq!(f.0 .0, Some(99.0));
        assert_eq!(f.1 .0, Some(98.0));
    }

    fn perms_of(n: usize) -> Vec<Vec<usize>> {
        fn go(cur: &mut Vec<usize>, used: &mut Vec<bool>, out: &mut Vec<Vec<usize>>) {
            if cur.len() == used.len() {
                out.push(cur.clone());
                return;
            }
            for i in 0..used.len() {
                if !used[i] {
                    used[i] = true;
                    cur.push(i);
                    go(cur, used, out);
                    cur.pop();
                    used[i] = false;
                }
            }
        }
        let mut out = vec![];
        go(&mut vec![], &mut vec![false; n], &mut out);
        out
    }

    // UNIT (F52-1, RC1): ONE injected comparison instant - the reviewer's A=(100, R), B=(99, S),
    // C=(30, S) schedule gives the same result for every order; an instant before R keeps A, one
    // between R and S keeps B, and both block.
    #[test]
    fn one_comparison_instant_makes_the_aggregation_order_independent() {
        let base = Utc::now();
        let (r, s_) = (base + Duration::minutes(10), base + Duration::minutes(60));
        let at = "2026-10-13T11:35:49+00:00";
        let atd: DateTime<FixedOffset> = at.parse().unwrap();
        let obs = [
            rq(Some((100.0, Some(r))), None, at),
            rq(Some((99.0, Some(s_))), None, at),
            rq(Some((30.0, Some(s_))), None, at),
        ];
        for (now, want) in [(base, 100.0), (base + Duration::minutes(30), 99.0)] {
            for perm in perms_of(3) {
                let list: Vec<Value> = perm.iter().map(|i| obs[*i].clone()).collect();
                let w = t_endpoint_window_at(&list, &["fp"], "fp", now).unwrap();
                assert_eq!(w.quota["primary"]["used_percent"], want, "{perm:?}");
                assert!(blocking_window(&w.quota, now).is_some());
                let mut m = None;
                for c in &list {
                    m = merge_observation(m, Some(&obs_of(c, atd)), now);
                }
                assert_eq!(m.unwrap().primary.unwrap().used_percent, want, "{perm:?}");
            }
        }
    }

    fn nan_quota(used: Value, reset: DateTime<Utc>, at: &str) -> Value {
        json!({"finished_at": at, "quota": {
            "primary": {"used_percent": used, "window_minutes": 300, "resets_at": reset.to_rfc3339()},
            "secondary": null, "observed_at": at}})
    }

    // UNIT (F52-2, RC2): a non-finite share is an ABSENT window and never hides a 99 % block.
    #[test]
    fn a_non_finite_percent_never_hides_a_valid_block() {
        let reset = Utc::now() + Duration::minutes(90);
        let at = "2026-10-13T11:35:49+00:00";
        for bad in [json!("NaN"), json!("inf"), json!("-inf"), json!(150.0)] {
            let a = nan_quota(bad, reset, at);
            let b = nan_quota(json!(99), reset, at);
            for order in [vec![a.clone(), b.clone()], vec![b, a]] {
                let w = t_endpoint_window(&order, &["fp"], "fp").unwrap();
                assert_eq!(w.quota["primary"]["used_percent"], 99);
                assert!(blocking_window(&w.quota, Utc::now()).is_some());
            }
        }
        assert!(finite_percent(&json!("NaN")).is_none());
        assert!(finite_percent(&json!("99.5")).is_some());
    }

    // UNIT (F52-1/2): 720 orders of six tied observations - one past reset, one NaN included.
    #[test]
    fn the_tie_selection_is_the_same_for_every_order_of_six() {
        let now = Utc::now();
        let at = "2026-10-13T11:35:49+00:00";
        let atd: DateTime<FixedOffset> = at.parse().unwrap();
        let mut obs = vec![
            rq(
                Some((99.0, None)),
                Some((20.0, Some(now + Duration::days(2)))),
                at,
            ),
            rq(
                Some((99.5, Some(now - Duration::minutes(3)))),
                Some((98.0, None)),
                at,
            ),
            rq(
                Some((99.0, Some(now + Duration::minutes(30)))),
                Some((98.0, Some(now - Duration::hours(1)))),
                at,
            ),
            rq(
                Some((60.0, Some(now + Duration::minutes(80)))),
                Some((98.0, Some(now + Duration::days(3)))),
                at,
            ),
            rq(Some((100.0, Some(now - Duration::days(1)))), None, at),
        ];
        obs.push(nan_quota(json!("NaN"), now + Duration::minutes(500), at));
        // a strictly NEWER record whose only window is invalid
        obs.push(nan_quota(
            json!("NaN"),
            now + Duration::minutes(500),
            "2026-10-13T11:50:00+00:00",
        ));
        let norm = |q: &Value| {
            let f = |k: &str| {
                (
                    q[k]["used_percent"].as_f64(),
                    ts_of(q[k]["resets_at"].as_str()),
                )
            };
            (f("primary"), f("secondary"))
        };
        let mut first = None;
        let perms = perms_of(7);
        assert_eq!(perms.len(), 5040);
        for perm in &perms {
            let list: Vec<Value> = perm.iter().map(|i| obs[*i].clone()).collect();
            let w = t_endpoint_window_at(&list, &["fp"], "fp", now).unwrap();
            let n = norm(&w.quota);
            let mut m = None;
            for c in &list {
                m = merge_observation(
                    m,
                    Some(&obs_of(c, when_offset(&c["finished_at"]).unwrap_or(atd))),
                    now,
                );
            }
            let m = m.unwrap();
            if let (Some(p), Some(q)) = (m.primary.as_ref(), n.0 .0) {
                assert_eq!(p.used_percent, q);
            }
            match &first {
                None => first = Some(n),
                Some(f) => assert_eq!(&n, f, "{perm:?}"),
            }
        }
        let f = first.unwrap();
        assert_eq!(f.0 .0, Some(99.0));
        assert_eq!(f.1 .0, Some(98.0));
    }

    fn two_rollouts(tag: &str, at: DateTime<Utc>, r: DateTime<Utc>, s_: DateTime<Utc>) -> PathBuf {
        let home = scratch(tag);
        for (salt, used, reset) in [(1u32, 100.0, r), (2u32, 99.0, s_)] {
            let id = v7(at, salt);
            let lines = vec![
                meta_line(at, &id, ""),
                token_line(at, 10, 0, 1, 10, &win(used, reset, 300), "null", "codex"),
            ];
            write_rollout(&home, &id, &lines, at);
        }
        home
    }

    // UNIT (F55-1, RC1): the account cache holds the RAW observations - a later lookup within the
    // TTL at an instant after A's reset retains B and blocks, like a fresh home does.
    #[test]
    fn the_account_cache_selects_at_each_requested_instant() {
        let now = Utc::now();
        let at = DateTime::<Utc>::from_timestamp(now.timestamp(), 0).unwrap();
        let (r, s_) = (now + Duration::minutes(10), now + Duration::minutes(60));
        let cached = two_rollouts("cache-a", at, r, s_);
        let fresh = two_rollouts("cache-b", at, r, s_);
        let (hc, hf) = (
            cached.to_string_lossy().to_string(),
            fresh.to_string_lossy().to_string(),
        );
        let early = account_window_at(&hc, now).unwrap();
        assert_eq!(early.quota["primary"]["used_percent"], 100.0);
        let later = now + Duration::minutes(30);
        let second = account_window_at(&hc, later).unwrap();
        let reference = account_window_at(&hf, later).unwrap();
        assert_eq!(second.quota["primary"]["used_percent"], 99.0);
        assert_eq!(second.quota, reference.quota);
        assert!(blocking_window(&second.quota, later).is_some());
        let _ = std::fs::remove_dir_all(&cached);
        let _ = std::fs::remove_dir_all(&fresh);
    }

    // UNIT (F55-2, RC2): a newer record with no valid window never replaces an older valid block;
    // a newer partial record replaces only its own valid window.
    #[test]
    fn a_newer_invalid_record_keeps_the_older_valid_window() {
        let now = Utc::now();
        let reset = now + Duration::minutes(90);
        let (t0, t1) = ("2026-10-13T11:35:49+00:00", "2026-10-13T11:40:00+00:00");
        let old = rq(Some((99.0, Some(reset))), None, t0);
        for bad in [json!("NaN"), json!(250.0)] {
            let newer = nan_quota(bad, reset, t1);
            for order in [vec![old.clone(), newer.clone()], vec![newer, old.clone()]] {
                let w = t_endpoint_window_at(&order, &["fp"], "fp", now).unwrap();
                assert_eq!(w.quota["primary"]["used_percent"], 99.0);
                assert!(blocking_window(&w.quota, now).is_some());
            }
        }
        // the newer record has a valid secondary only, and a null primary
        let newer = rq(None, Some((40.0, Some(reset))), t1);
        let old2 = rq(Some((99.0, Some(reset))), Some((10.0, Some(reset))), t0);
        for order in [vec![old2.clone(), newer.clone()], vec![newer, old2]] {
            let w = t_endpoint_window_at(&order, &["fp"], "fp", now).unwrap();
            assert_eq!(w.quota["primary"]["used_percent"], 99.0);
            assert_eq!(w.quota["secondary"]["used_percent"], 40.0);
            assert!(blocking_window(&w.quota, now).is_some());
        }
    }

    fn iso_s(t: DateTime<Utc>) -> String {
        t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    fn secs(t: DateTime<Utc>) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(t.timestamp(), 0).unwrap()
    }

    fn at_of(c: &Value) -> DateTime<FixedOffset> {
        when_offset(&c["finished_at"]).unwrap()
    }

    // The reviewer's F58-1 fixture: t1 < t2 < t3, every window with a future reset.
    // A = primary 30 % at t1, B = secondary 10 % at t3, C = primary 99 % at t2.
    fn abc(now: DateTime<Utc>) -> ([DateTime<Utc>; 3], [Value; 3]) {
        let t = [
            secs(now - Duration::minutes(30)),
            secs(now - Duration::minutes(20)),
            secs(now - Duration::minutes(10)),
        ];
        let reset = now + Duration::minutes(120);
        let a = rq(Some((30.0, Some(reset))), None, &iso_s(t[0]));
        let b = rq(None, Some((10.0, Some(reset))), &iso_s(t[2]));
        let c = rq(Some((99.0, Some(reset))), None, &iso_s(t[1]));
        (t, [a, b, c])
    }

    fn check_abc(w: &EndpointWindow, t: &[DateTime<Utc>; 3], now: DateTime<Utc>, what: &str) {
        assert_eq!(w.quota["primary"]["used_percent"], 99.0, "{what}");
        assert_eq!(w.quota["secondary"]["used_percent"], 10.0, "{what}");
        assert_eq!(
            w.window_at[0].map(|x| x.timestamp()),
            Some(t[1].timestamp()),
            "{what}"
        );
        assert_eq!(
            w.window_at[1].map(|x| x.timestamp()),
            Some(t[2].timestamp()),
            "{what}"
        );
        assert_eq!(w.observed_at.timestamp(), t[2].timestamp(), "{what}");
        assert!(blocking_window(&w.quota, now).is_some(), "{what}");
    }

    // UNIT (F58-1, a): the recorded selection keeps each window at ITS OWN instant - all six orders.
    #[test]
    fn partial_windows_keep_their_own_instants_in_the_recorded_selection() {
        let now = Utc::now();
        let (t, obs) = abc(now);
        for perm in perms_of(3) {
            let list: Vec<Value> = perm.iter().map(|i| obs[*i].clone()).collect();
            let w = t_endpoint_window_at(&list, &["fp"], "fp", now).unwrap();
            check_abc(&w, &t, now, &format!("{perm:?}"));
        }
    }

    // UNIT (F58-1, b): merge_observation (the rollout / account aggregation) - all six orders.
    #[test]
    fn partial_windows_keep_their_own_instants_in_merge_observation() {
        let now = Utc::now();
        let (t, obs) = abc(now);
        for perm in perms_of(3) {
            let mut m = None;
            for i in &perm {
                m = merge_observation(m, Some(&obs_of(&obs[*i], at_of(&obs[*i]))), now);
            }
            let m = m.unwrap();
            assert_eq!(m.primary.as_ref().unwrap().used_percent, 99.0, "{perm:?}");
            assert_eq!(m.secondary.as_ref().unwrap().used_percent, 10.0, "{perm:?}");
            assert_eq!(m.window_at(0).unwrap().timestamp(), t[1].timestamp());
            assert_eq!(m.window_at(1).unwrap().timestamp(), t[2].timestamp());
            assert_eq!(m.at.unwrap().timestamp(), t[2].timestamp());
        }
    }

    // UNIT (F58-1, c): the recorded/live split - the rollout window instants travel into the
    // endpoint selection (A,B recorded + C live, and C recorded + A,B live).
    #[test]
    fn partial_windows_keep_their_own_instants_across_the_recorded_live_split() {
        let now = Utc::now();
        let (t, obs) = abc(now);
        let reset = now + Duration::minutes(120);
        // observation index: 0 = A, 1 = B, 2 = C (the instants are t[0], t[2], t[1])
        let at_idx = [t[0], t[2], t[1]];
        let rollout = |home: &Path, idx: usize, salt: u32| {
            let at = at_idx[idx];
            let id = v7(at, salt);
            let (p, s_) = match idx {
                0 => (win(30.0, reset, 300), "null".to_string()),
                1 => ("null".to_string(), win(10.0, reset, 10080)),
                _ => (win(99.0, reset, 300), "null".to_string()),
            };
            let lines = vec![
                meta_line(at, &id, ""),
                token_line(at, 10, 0, 1, 10, &p, &s_, "codex"),
            ];
            write_rollout(home, &id, &lines, at);
        };
        // (live rollouts, recorded entries)
        let cases: [(&[usize], &[usize]); 4] = [
            (&[2], &[0, 1]),
            (&[2], &[1, 0]),
            (&[0, 1], &[2]),
            (&[1, 0], &[2]),
        ];
        for (n, (live, rec)) in cases.iter().enumerate() {
            let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
            let home = scratch("split-live");
            for (k, idx) in live.iter().enumerate() {
                rollout(&home, *idx, 20 + k as u32);
            }
            let recorded: Vec<Value> = rec.iter().map(|i| obs[*i].clone()).collect();
            set_live_codex_home(Some(home.to_string_lossy().to_string()));
            let w = super::endpoint_window_at(&recorded, &["fp"], "fp", now);
            set_live_codex_home(None);
            let _ = std::fs::remove_dir_all(&home);
            check_abc(&w.unwrap(), &t, now, &format!("case {n}"));
        }
    }

    // UNIT (F58-1, d): distinct instants and partial windows, 720 orders - the result is the same
    // for recorded selection and merge_observation: primary 99 % at t2, secondary 40 % at t4.
    #[test]
    fn distinct_instant_partial_windows_are_order_independent() {
        let now = Utc::now();
        let t: Vec<DateTime<Utc>> = (1..=4)
            .map(|k| secs(now - Duration::minutes(50 - 10 * k)))
            .collect();
        let reset = now + Duration::minutes(120);
        let obs = [
            rq(Some((30.0, Some(reset))), None, &iso_s(t[0])),
            rq(None, Some((10.0, Some(reset))), &iso_s(t[2])),
            rq(Some((99.0, Some(reset))), None, &iso_s(t[1])),
            rq(None, Some((5.0, Some(reset))), &iso_s(t[1])),
            nan_quota(json!("NaN"), reset, &iso_s(t[3])),
            rq(None, Some((40.0, Some(reset))), &iso_s(t[3])),
        ];
        let fingerprint = |q: &Value| {
            let f = |k: &str| {
                (
                    q[k]["used_percent"].as_f64(),
                    ts_of(q[k]["resets_at"].as_str()),
                )
            };
            (f("primary"), f("secondary"))
        };
        let mut first = None;
        for perm in perms_of(6) {
            let list: Vec<Value> = perm.iter().map(|i| obs[*i].clone()).collect();
            let w = t_endpoint_window_at(&list, &["fp"], "fp", now).unwrap();
            assert_eq!(
                w.window_at[0].map(|x| x.timestamp()),
                Some(t[1].timestamp())
            );
            assert_eq!(
                w.window_at[1].map(|x| x.timestamp()),
                Some(t[3].timestamp())
            );
            let n = fingerprint(&w.quota);
            let mut m = None;
            for c in &list {
                m = merge_observation(m, Some(&obs_of(c, at_of(c))), now);
            }
            let m = m.unwrap();
            assert_eq!(m.primary.as_ref().unwrap().used_percent, 99.0, "{perm:?}");
            assert_eq!(m.secondary.as_ref().unwrap().used_percent, 40.0, "{perm:?}");
            assert_eq!(m.window_at(0).unwrap().timestamp(), t[1].timestamp());
            assert_eq!(m.window_at(1).unwrap().timestamp(), t[3].timestamp());
            match &first {
                None => first = Some(n),
                Some(f) => assert_eq!(&n, f, "{perm:?}"),
            }
        }
        let f = first.unwrap();
        assert_eq!(f.0 .0, Some(99.0));
        assert_eq!(f.1 .0, Some(40.0));
    }

    // A and B merged (the production path: merge_observation -> to_quota), written as a ledger
    // entry and read back through the real serde of the ledger.
    fn persisted_ab(now: DateTime<Utc>) -> ([DateTime<Utc>; 3], [Value; 3], Value, Observation) {
        let (t, obs) = abc(now);
        let m = merge_observation(
            Some(obs_of(&obs[0], at_of(&obs[0]))),
            Some(&obs_of(&obs[1], at_of(&obs[1]))),
            now,
        )
        .unwrap();
        let mut entry = crate::ledger::LedgerEntry {
            n: 1,
            finished_at: iso_s(t[2]),
            quota: Some(m.to_quota()),
            ..Default::default()
        };
        entry.reviewer.provider_fingerprint = "fp".into();
        let text = serde_json::to_string(&entry).unwrap();
        let back: crate::ledger::LedgerEntry = serde_json::from_str(&text).unwrap();
        let v: Value = serde_json::to_value(&back).unwrap();
        (t, obs, v, m)
    }

    // UNIT (F60-1): the record written for an aggregate carries each window's own instant and it
    // survives the ledger entry and the machine-wide health record round trips.
    #[test]
    fn the_quota_record_carries_per_window_instants_through_ledger_and_health() {
        let now = Utc::now();
        let (t, _obs, v, m) = persisted_ab(now);
        let q = &v["quota"];
        assert_eq!(
            ts_of(q["primary_observed_at"].as_str()),
            Some(t[0].timestamp())
        );
        assert_eq!(
            ts_of(q["secondary_observed_at"].as_str()),
            Some(t[2].timestamp())
        );
        assert_eq!(ts_of(q["observed_at"].as_str()), Some(t[2].timestamp()));
        // an absent window writes no instant; a null quota keeps exactly its three keys
        let only = Observation {
            secondary: None,
            ..m.clone()
        };
        assert!(only.to_quota().get("secondary_observed_at").is_none());
        let nq = null_quota();
        let keys: Vec<&String> = nq.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["primary", "secondary", "observed_at"]);
        // the machine-wide health record
        let rec = crate::health::new_machine_health_record_quota(
            "fp",
            "usable reply",
            None,
            "r",
            None,
            Some(&m.to_quota()),
        )
        .unwrap();
        let text = serde_json::to_string(&rec).unwrap();
        let back: crate::health::MachineEndpoint = serde_json::from_str(&text).unwrap();
        assert_eq!(back.quota.unwrap(), m.to_quota());
    }

    // UNIT (F60-1): the persisted aggregate (A primary 30 at t1 + B secondary 10 at t3) against C
    // (primary 99 at t2): recorded in both orders, and with C live - primary 99 % at t2 survives
    // and blocks.
    #[test]
    fn a_persisted_aggregate_cannot_promote_its_older_window() {
        let now = Utc::now();
        let (t, obs, agg, _m) = persisted_ab(now);
        let c = obs[2].clone();
        for order in [vec![agg.clone(), c.clone()], vec![c.clone(), agg.clone()]] {
            let w = t_endpoint_window_at(&order, &["fp"], "fp", now).unwrap();
            check_abc(&w, &t, now, "recorded");
        }
        // C live (a rollout), the aggregate recorded
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let home = scratch("persisted-live");
        let reset = now + Duration::minutes(120);
        let id = v7(t[1], 31);
        let lines = vec![
            meta_line(t[1], &id, ""),
            token_line(t[1], 10, 0, 1, 10, &win(99.0, reset, 300), "null", "codex"),
        ];
        write_rollout(&home, &id, &lines, t[1]);
        set_live_codex_home(Some(home.to_string_lossy().to_string()));
        let w = super::endpoint_window_at(std::slice::from_ref(&agg), &["fp"], "fp", now);
        set_live_codex_home(None);
        let _ = std::fs::remove_dir_all(&home);
        check_abc(&w.unwrap(), &t, now, "live");
    }

    fn primary_obs(used: f64, at: DateTime<Utc>, reset: DateTime<Utc>) -> Observation {
        Observation {
            primary: Some(Window {
                used_percent: used,
                window_minutes: Some(300),
                resets_at: Some(reset.to_rfc3339()),
            }),
            secondary: None,
            at: Some(at.fixed_offset()),
            primary_at: None,
            secondary_at: None,
            limit_id: "codex".into(),
        }
    }

    // a ledger entry (real serde round trip) carrying `quota`
    fn ledger_roundtrip(quota: Value, finished: DateTime<Utc>) -> Value {
        let mut entry = crate::ledger::LedgerEntry {
            n: 1,
            finished_at: iso_s(finished),
            quota: Some(quota),
            ..Default::default()
        };
        entry.reviewer.provider_fingerprint = "fp".into();
        let text = serde_json::to_string(&entry).unwrap();
        let back: crate::ledger::LedgerEntry = serde_json::from_str(&text).unwrap();
        serde_json::to_value(&back).unwrap()
    }

    // a live rollout (one token_count) with primary `used` at `at`; returns the home
    fn live_rollout(tag: &str, used: f64, at: DateTime<Utc>, reset: DateTime<Utc>) -> PathBuf {
        let home = scratch(tag);
        let id = v7(at, 41);
        let lines = vec![
            meta_line(at, &id, ""),
            token_line(at, 10, 0, 1, 10, &win(used, reset, 300), "null", "codex"),
        ];
        write_rollout(&home, &id, &lines, at);
        home
    }

    // UNIT (F62-1, RC1): the instants are stored losslessly (fraction kept), and a recorded 99 %
    // observed at :00.900 is NOT replaced by an OLDER live 30 % at :00.100 of the same second.
    #[test]
    fn a_subsecond_instant_survives_the_record_and_beats_an_older_live_one() {
        let now = Utc::now();
        let base = secs(now - Duration::minutes(10));
        let reset = now + Duration::minutes(120);
        let rec_at = base + Duration::milliseconds(900);
        let live_at = base + Duration::milliseconds(100);
        let q = primary_obs(99.0, rec_at, reset).to_quota();
        let stored = q["primary_observed_at"].as_str().unwrap().to_string();
        assert!(stored.contains(".900"), "{stored}");
        let v = ledger_roundtrip(q, rec_at);
        assert_eq!(v["quota"]["primary_observed_at"].as_str().unwrap(), stored);
        assert_eq!(
            when_offset(&v["quota"]["primary_observed_at"])
                .unwrap()
                .timestamp_subsec_millis(),
            900
        );
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        let home = live_rollout("subsec-live", 30.0, live_at, reset);
        set_live_codex_home(Some(home.to_string_lossy().to_string()));
        let w = super::endpoint_window_at(std::slice::from_ref(&v), &["fp"], "fp", now);
        set_live_codex_home(None);
        let _ = std::fs::remove_dir_all(&home);
        let w = w.unwrap();
        assert_eq!(w.quota["primary"]["used_percent"], 99.0);
        assert!(blocking_window(&w.quota, now).is_some());
    }

    // UNIT (F62-1): a record with SECOND precision (an 8h-style one) against a live instant of the
    // same second is a tie -> the 8d rank (the higher share) decides, not recency.
    #[test]
    fn a_second_precision_record_ties_with_a_live_instant_of_the_same_second() {
        let now = Utc::now();
        let base = secs(now - Duration::minutes(10));
        let reset = now + Duration::minutes(120);
        let q = primary_obs(99.0, base, reset).to_quota();
        let stored = q["primary_observed_at"].as_str().unwrap().to_string();
        assert!(!stored.contains('.'), "{stored}");
        let v = ledger_roundtrip(q, base);
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        for (live_used, live_at) in [
            (30.0, base + Duration::milliseconds(100)),
            (30.0, base + Duration::milliseconds(900)),
        ] {
            let home = live_rollout("sec-live", live_used, live_at, reset);
            set_live_codex_home(Some(home.to_string_lossy().to_string()));
            let w = super::endpoint_window_at(std::slice::from_ref(&v), &["fp"], "fp", now);
            set_live_codex_home(None);
            let _ = std::fs::remove_dir_all(&home);
            let w = w.unwrap();
            assert_eq!(w.quota["primary"]["used_percent"], 99.0);
            assert!(blocking_window(&w.quota, now).is_some());
        }
        // a strictly later whole second does win on recency
        let home = live_rollout("sec-live2", 30.0, base + Duration::seconds(2), reset);
        set_live_codex_home(Some(home.to_string_lossy().to_string()));
        let w = super::endpoint_window_at(std::slice::from_ref(&v), &["fp"], "fp", now);
        set_live_codex_home(None);
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(w.unwrap().quota["primary"]["used_percent"], 30.0);
    }

    // UNIT (F62-1 / F60-1 legacy): a record WITHOUT per-window fields has UNKNOWN window instants;
    // the promoted 30 % of a legacy aggregate never beats a known, genuinely newer 99 % on recency,
    // and the endpoint health evaluation sees the block. Legacy against legacy keeps the aggregate.
    #[test]
    fn a_legacy_record_never_wins_on_recency_against_a_known_instant() {
        let now = Utc::now();
        let (t, obs, mut agg, _m) = persisted_ab(now);
        let q = agg["quota"].as_object_mut().unwrap();
        q.remove("primary_observed_at");
        q.remove("secondary_observed_at");
        // C as a CURRENT record (known per-window instants)
        let reset = now + Duration::minutes(120);
        let c_known = ledger_roundtrip(primary_obs(99.0, t[1], reset).to_quota(), t[1]);
        for order in [
            vec![agg.clone(), c_known.clone()],
            vec![c_known.clone(), agg.clone()],
        ] {
            let w = t_endpoint_window_at(&order, &["fp"], "fp", now).unwrap();
            assert_eq!(w.quota["primary"]["used_percent"], 99.0);
            assert!(blocking_window(&w.quota, now).is_some());
            // the endpoint health evaluation (the verdict) sees the block
            let h = crate::health::endpoint_health(&order, "fp", now);
            assert!(h.quota.is_some(), "health sees the block");
        }
        // legacy vs legacy: the aggregate instant still orders them (the pre-8h behaviour)
        let w = t_endpoint_window_at(&[agg, obs[2].clone()], &["fp"], "fp", now).unwrap();
        assert_eq!(w.quota["primary"]["used_percent"], 30.0);
    }

    fn mixed_obs(
        p: Option<f64>,
        s_: Option<f64>,
        at: DateTime<Utc>,
        reset: DateTime<Utc>,
    ) -> Observation {
        let w = |u: f64, m: i64| Window {
            used_percent: u,
            window_minutes: Some(m),
            resets_at: Some(reset.to_rfc3339()),
        };
        Observation {
            primary: p.map(|u| w(u, 300)),
            secondary: s_.map(|u| w(u, 10080)),
            at: Some(at.fixed_offset()),
            primary_at: None,
            secondary_at: None,
            limit_id: "codex".into(),
        }
    }

    fn live_rollout2(
        tag: &str,
        p: Option<f64>,
        s_: Option<f64>,
        at: DateTime<Utc>,
        reset: DateTime<Utc>,
    ) -> PathBuf {
        let home = scratch(tag);
        let id = v7(at, 51);
        let cell = |u: Option<f64>, m: i64| u.map_or("null".to_string(), |u| win(u, reset, m));
        let lines = vec![
            meta_line(at, &id, ""),
            token_line(at, 10, 0, 1, 10, &cell(p, 300), &cell(s_, 10080), "codex"),
        ];
        write_rollout(&home, &id, &lines, at);
        home
    }

    // UNIT (8j): the a/b/c case of the 8i caveat - a imprecise :00 with 50 %, b :00.100 with 99 %,
    // c :00.900 with 30 % - gives 99 % in all six orders (recorded and merge_observation).
    #[test]
    fn whole_second_order_is_transitive_for_the_abc_precision_case() {
        let now = Utc::now();
        let base = secs(now - Duration::minutes(10));
        let reset = now + Duration::minutes(120);
        let obs = [
            mixed_obs(Some(50.0), None, base, reset),
            mixed_obs(Some(99.0), None, base + Duration::milliseconds(100), reset),
            mixed_obs(Some(30.0), None, base + Duration::milliseconds(900), reset),
        ];
        for perm in perms_of(3) {
            let recs: Vec<Value> = perm
                .iter()
                .map(|i| ledger_roundtrip(obs[*i].to_quota(), obs[*i].at.unwrap().to_utc()))
                .collect();
            let w = t_endpoint_window_at(&recs, &["fp"], "fp", now).unwrap();
            assert_eq!(w.quota["primary"]["used_percent"], 99.0, "{perm:?}");
            assert!(blocking_window(&w.quota, now).is_some());
            let mut m = None;
            for i in &perm {
                m = merge_observation(m, Some(&obs[*i]), now);
            }
            assert_eq!(m.unwrap().primary.unwrap().used_percent, 99.0, "{perm:?}");
        }
    }

    // UNIT (8j): mixed precision inside one second, partial windows and a legacy record - 720
    // orders through the recorded selection and merge_observation, and 2 x 120 orders through a
    // recorded/live split: one pick, and the active block whenever a candidate blocks.
    #[test]
    fn mixed_precision_candidates_give_one_pick_in_every_order() {
        let now = Utc::now();
        let base = secs(now - Duration::minutes(10));
        let reset = now + Duration::minutes(120);
        let ms = Duration::milliseconds;
        let o = [
            mixed_obs(Some(50.0), None, base, reset),
            mixed_obs(Some(60.0), Some(20.0), base, reset),
            mixed_obs(Some(99.0), None, base + ms(100), reset),
            mixed_obs(None, Some(30.0), base + ms(900), reset),
            mixed_obs(
                None,
                Some(15.0),
                base + Duration::seconds(1) + ms(500),
                reset,
            ),
            mixed_obs(Some(70.0), Some(25.0), base + Duration::seconds(2), reset),
        ];
        let rec = |i: usize| ledger_roundtrip(o[i].to_quota(), o[i].at.unwrap().to_utc());
        let mut records: Vec<Value> = (0..6).map(rec).collect();
        // the legacy record: no per-window keys, the aggregate instant only
        let q = records[5]["quota"].as_object_mut().unwrap();
        q.remove("primary_observed_at");
        q.remove("secondary_observed_at");
        let pick = |q: &Value| {
            (
                q["primary"]["used_percent"].as_f64(),
                q["secondary"]["used_percent"].as_f64(),
            )
        };
        // recorded selection: primary 99 (the rank inside the base second; the legacy 70 does not
        // outrank it), secondary: the next second's known 15 against the legacy 25 -> 25 by rank
        let mut want = None;
        for perm in perms_of(6) {
            let list: Vec<Value> = perm.iter().map(|i| records[*i].clone()).collect();
            let w = t_endpoint_window_at(&list, &["fp"], "fp", now).unwrap();
            assert!(blocking_window(&w.quota, now).is_some(), "{perm:?}");
            let got = pick(&w.quota);
            match want {
                None => want = Some(got),
                Some(x) => assert_eq!(got, x, "{perm:?}"),
            }
            // merge_observation over the same six (all known there): the newest second wins
            let mut m = None;
            for i in &perm {
                m = merge_observation(m, Some(&o[*i]), now);
            }
            let m = m.unwrap();
            assert_eq!(m.primary.unwrap().used_percent, 70.0, "{perm:?}");
            assert_eq!(m.secondary.unwrap().used_percent, 25.0, "{perm:?}");
        }
        assert_eq!(want, Some((Some(99.0), Some(25.0))));
        // recorded/live split: the live rollout is o[2] (primary 99) or o[4] (secondary 15)
        let _live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        for live_idx in [2usize, 4] {
            let home = live_rollout2(
                "mixed-live",
                o[live_idx].primary.as_ref().map(|w| w.used_percent),
                o[live_idx].secondary.as_ref().map(|w| w.used_percent),
                o[live_idx].at.unwrap().to_utc(),
                reset,
            );
            set_live_codex_home(Some(home.to_string_lossy().to_string()));
            let others: Vec<usize> = (0..6).filter(|i| *i != live_idx).collect();
            for perm in perms_of(5) {
                let list: Vec<Value> = perm.iter().map(|i| records[others[*i]].clone()).collect();
                let w = super::endpoint_window_at(&list, &["fp"], "fp", now).unwrap();
                assert_eq!(
                    pick(&w.quota),
                    (Some(99.0), Some(25.0)),
                    "{live_idx} {perm:?}"
                );
                assert!(blocking_window(&w.quota, now).is_some());
            }
            set_live_codex_home(None);
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    // UNIT (F48-1): the sub-agent / rollout aggregation merges per window at an equal instant.
    #[test]
    fn merge_observation_keeps_each_fuller_window_at_an_equal_instant() {
        let at: DateTime<FixedOffset> = "2026-10-13T12:00:00+00:00".parse().unwrap();
        let mk = |p: f64, w: f64| Observation {
            primary: Some(Window {
                used_percent: p,
                window_minutes: Some(300),
                resets_at: None,
            }),
            secondary: Some(Window {
                used_percent: w,
                window_minutes: Some(10080),
                resets_at: None,
            }),
            at: Some(at),
            primary_at: None,
            secondary_at: None,
            limit_id: "codex".into(),
        };
        for (a, b) in [
            (mk(30.0, 99.0), mk(31.0, 10.0)),
            (mk(31.0, 10.0), mk(30.0, 99.0)),
        ] {
            let m = merge_observation(Some(a), Some(&b), Utc::now()).unwrap();
            assert_eq!(m.primary.unwrap().used_percent, 31.0);
            assert_eq!(m.secondary.unwrap().used_percent, 99.0);
        }
        let newer = Observation {
            at: Some(at + Duration::seconds(1)),
            ..mk(1.0, 2.0)
        };
        let m = merge_observation(Some(mk(30.0, 99.0)), Some(&newer), Utc::now()).unwrap();
        assert_eq!(m.secondary.unwrap().used_percent, 2.0);
    }

    #[test]
    fn the_newer_observation_wins_and_the_fuller_at_the_same_instant() {
        let at: DateTime<FixedOffset> = "2026-10-13T12:00:00+00:00".parse().unwrap();
        let mk = |u: f64, at: DateTime<FixedOffset>| Observation {
            primary: Some(Window {
                used_percent: u,
                window_minutes: Some(300),
                resets_at: None,
            }),
            secondary: None,
            at: Some(at),
            primary_at: None,
            secondary_at: None,
            limit_id: "codex".into(),
        };
        let none = Observation {
            primary: None,
            secondary: None,
            at: Some(at),
            primary_at: None,
            secondary_at: None,
            limit_id: "premium".into(),
        };
        assert!(newer_wins(
            Some(&mk(50.0, at)),
            Some(&mk(10.0, at + Duration::seconds(1)))
        ));
        assert!(!newer_wins(
            Some(&mk(50.0, at)),
            Some(&mk(90.0, at - Duration::seconds(1)))
        ));
        assert!(newer_wins(Some(&mk(50.0, at)), Some(&mk(60.0, at))));
        assert!(!newer_wins(Some(&mk(50.0, at)), Some(&none)));
        assert!(newer_wins(Some(&none), Some(&mk(1.0, at))));
        assert!(newer_wins(None, Some(&mk(1.0, at))));
    }

    #[test]
    fn the_replay_guard_refuses_over_the_limit_and_warns_from_128_kb() {
        let home = scratch("guard");
        let h = home.to_string_lossy().to_string();
        let now = Utc::now();
        let big = v7(now, 11);
        write_rollout(
            &home,
            &big,
            &[
                meta_line(now, &big, ""),
                format!("{{\"pad\":\"{}\"}}", "x".repeat(600 * 1024)),
            ],
            now,
        );
        let mid = v7(now, 12);
        write_rollout(
            &home,
            &mid,
            &[
                meta_line(now, &mid, ""),
                format!("{{\"pad\":\"{}\"}}", "x".repeat(200 * 1024)),
            ],
            now,
        );
        let small = v7(now, 13);
        write_rollout(&home, &small, &[meta_line(now, &small, "")], now);
        let g_big = fork_replay_guard(&big, &h, None);
        assert!(g_big.over_limit);
        assert_eq!(g_big.refusal, format!("the parent thread {big} replays 600 KB (~150 k tokens) on EVERY tool call of the run: use -Mode new, or raise CODEX_CONSULT_FORK_MAX_KB (now 512)"));
        let g_mid = fork_replay_guard(&mid, &h, None);
        assert!(g_mid.refusal.is_empty());
        assert!(g_mid.warning.starts_with(&format!("the parent thread {mid} replays 200 KB (~50 k tokens) on EVERY tool call of the run (CODEX_CONSULT_FORK_MAX_KB 512)")));
        let g_small = fork_replay_guard(&small, &h, None);
        assert!(g_small.refusal.is_empty() && g_small.warning.is_empty());
        let g_raised = fork_replay_guard(&big, &h, Some("1024"));
        assert!(g_raised.refusal.is_empty() && !g_raised.warning.is_empty());
        let g_bad = fork_replay_guard(&big, &h, Some("lots"));
        assert!(!g_bad.refusal.is_empty() && !g_bad.problem.is_empty());
        let g_off = fork_replay_guard(&big, &h, Some("0"));
        assert!(
            g_off.refusal.is_empty() && g_off.warning.contains("CODEX_CONSULT_FORK_MAX_KB off")
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_budget_resolves_and_refuses_like_the_plugin() {
        assert_eq!(
            resolve_budget(None, None).unwrap(),
            Budget {
                percent: 90,
                source: "default".into()
            }
        );
        assert_eq!(
            resolve_budget(Some(0), Some("50")).unwrap(),
            Budget {
                percent: 0,
                source: "-Budget".into()
            }
        );
        assert_eq!(
            resolve_budget(None, Some(" 93 ")).unwrap(),
            Budget {
                percent: 93,
                source: "CODEX_CONSULT_QUOTA_BUDGET".into()
            }
        );
        assert!(resolve_budget(Some(150), None)
            .unwrap_err()
            .starts_with("-Budget must be 0 (off) or a share"));
        assert!(resolve_budget(None, Some("most"))
            .unwrap_err()
            .starts_with("CODEX_CONSULT_QUOTA_BUDGET must be 0"));
    }

    #[test]
    fn the_budget_watch_stops_only_on_an_observation_of_this_turn() {
        let now = Utc::now();
        let start = (now - Duration::seconds(2)).fixed_offset();
        let w = PrimaryWindow {
            used: Some(95.0),
            resets_at: Some((now + Duration::minutes(100)).fixed_offset()),
            name: "5-hour window".into(),
            path: PathBuf::new(),
            at: Some(now.fixed_offset()),
        };
        assert!(budget_reached(&w, 90, start, now));
        assert!(!budget_reached(&w, 0, start, now));
        assert!(!budget_reached(&w, 96, start, now));
        let old = PrimaryWindow {
            at: Some((now - Duration::minutes(5)).fixed_offset()),
            ..w.clone()
        };
        assert!(!budget_reached(&old, 90, start, now));
        let reset = PrimaryWindow {
            resets_at: Some((now - Duration::minutes(1)).fixed_offset()),
            ..w.clone()
        };
        assert!(!budget_reached(&reset, 90, start, now));
        let t = budget_stop_text(&w, now);
        assert!(regex::Regex::new(r"^stopped at the quota budget: 5-hour window 95 % used, resets \d\d:\d\d; the partial is kept$").unwrap().is_match(&t), "{t}");
    }

    #[test]
    fn the_users_multi_agent_item_wins() {
        assert_eq!(user_multi_agent(&[]), None);
        assert_eq!(
            user_multi_agent(&["features.multi_agent=true".into()]),
            Some(true)
        );
        assert_eq!(
            user_multi_agent(&["features.multi_agent = false".into(), "x=1".into()]),
            Some(false)
        );
        assert_eq!(
            user_multi_agent(&[
                "features.multi_agent=false".into(),
                "features.multi_agent=true".into()
            ]),
            Some(true)
        );
    }

    #[test]
    fn a_v7_id_carries_its_time() {
        let at: DateTime<Utc> = "2026-10-13T11:35:49.123Z".parse().unwrap();
        let id = v7(at, 5);
        assert_eq!(uuid_v7_time(&id), Some(at));
        assert_eq!(uuid_v7_time("0b1e2a33-4c5d-4e6f-8a9b-0c1d2e3f4a5b"), None);
    }
}
