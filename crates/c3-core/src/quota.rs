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
//! that reset (`crate::health::endpoint_health_set`, like a usage limit with a known reset); at or
//! above [`QUOTA_WARN_PERCENT`] the available verdict says `low quota: ...`. A reset in the past
//! means the window was reset: shown, never judged.
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

/// A window at or above this share whose reset lies ahead keeps its endpoint out until that reset.
pub const QUOTA_BLOCK_PERCENT: f64 = 98.0;
/// A window at or above this share (not reset) adds `low quota: ...` to an available verdict.
pub const QUOTA_WARN_PERCENT: f64 = 80.0;
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
        Value::Number(n) => n.as_f64()?,
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

/// One observation of the windows: the windows of one `token_count` event and when it was written.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub primary: Option<Window>,
    pub secondary: Option<Window>,
    pub at: Option<DateTime<FixedOffset>>,
    pub limit_id: String,
}

impl Observation {
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

    /// The ledger's `quota` `{primary, secondary, observed_at}` (observed_at local).
    pub fn to_quota(&self) -> Value {
        json!({
            "primary": self.primary.as_ref().map(Window::to_json),
            "secondary": self.secondary.as_ref().map(Window::to_json),
            "observed_at": self.at.map(|a| iso(to_local(a))),
        })
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
        if newer_wins(win.as_ref(), su.window.as_ref()) {
            win = su.window.clone();
        }
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
        let used = match &w["used_percent"] {
            Value::Number(n) => match n.as_f64() {
                Some(f) => f,
                None => continue,
            },
            Value::String(s) => match s.trim().parse::<f64>() {
                Ok(f) => f,
                Err(_) => continue,
            },
            _ => continue,
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
            blocks: !reset && resets_at.is_some() && used >= QUOTA_BLOCK_PERCENT,
            low: !reset && used >= QUOTA_WARN_PERCENT,
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
    pub observed_at: DateTime<FixedOffset>,
    /// `recorded` (a ledger entry or a machine-wide record) | `rollout` (the codex home's newest).
    pub source: &'static str,
    pub fingerprint: String,
}

/// The account's windows as the newest rollout of a codex home shows them
/// (`Get-CodexAccountWindow`).
#[derive(Debug, Clone)]
pub struct AccountWindow {
    pub quota: Value,
    pub observed_at: DateTime<FixedOffset>,
    pub path: PathBuf,
}

/// The per-process cache of [`account_window`]: codex home -> (read at, the window).
type AccountCache = Mutex<HashMap<String, (Instant, Option<AccountWindow>)>>;

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
    if codex_home.is_empty() {
        return None;
    }
    if let Ok(c) = account_cache().lock() {
        if let Some((at, v)) = c.get(codex_home) {
            if at.elapsed().as_secs() < 60 {
                return v.clone();
            }
        }
    }
    let mut value = None;
    let root = Path::new(codex_home).join("sessions");
    if root.is_dir() {
        let today = Local::now().date_naive();
        let mut files: Vec<(SystemTime, PathBuf)> = Vec::new();
        for d in local_day_dirs(&root, today.pred_opt().unwrap_or(today), today) {
            rollouts_in(&d, None, &mut files);
        }
        files.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
        let provider_re = regex::Regex::new(r#""model_provider"\s*:\s*"([^"]*)""#).unwrap();
        let mut best: Option<Observation> = None;
        let mut best_path = PathBuf::new();
        for (_, p) in files.into_iter().take(16) {
            let head = read_rollout_head(&p);
            if let Some(c) = provider_re.captures(&head) {
                if &c[1] != "openai" {
                    continue;
                }
            }
            let u = rollout_usage(&p, None, ROLLOUT_TAIL_BYTES);
            if newer_wins(best.as_ref(), u.window.as_ref()) {
                best = u.window;
                best_path = p;
            }
        }
        if let Some(b) = best.filter(|b| b.named() && b.at.is_some()) {
            value = Some(AccountWindow {
                quota: b.to_quota(),
                observed_at: b.at.unwrap(),
                path: best_path,
            });
        }
    }
    if let Ok(mut c) = account_cache().lock() {
        c.insert(codex_home.to_string(), (Instant::now(), value.clone()));
    }
    value
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

/// The used share of a recorded `quota` member (`primary` / `secondary`), 0 when absent.
fn quota_used(q: &Value, key: &str) -> f64 {
    match &q[key]["used_percent"] {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// The ONE comparison of the endpoint window selection (recorded and live): a candidate at `at`
/// with `quota` replaces `best` when it is strictly newer; at an equal `observed_at` (the ledger
/// serialises without fractions, so ties happen) the higher primary share wins, then the higher
/// secondary - the same fuller-wins tie rule as [`newer_wins`].
fn window_obs_wins(at: DateTime<FixedOffset>, quota: &Value, best: &EndpointWindow) -> bool {
    if at != best.observed_at {
        return at > best.observed_at;
    }
    let cand = (quota_used(quota, "primary"), quota_used(quota, "secondary"));
    let cur = (
        quota_used(&best.quota, "primary"),
        quota_used(&best.quota, "secondary"),
    );
    cand > cur
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
        let at = when_offset(&q["observed_at"])
            .or_else(|| when_offset(&c["finished_at"]))
            .or_else(|| when_offset(&c["when"]));
        let at = match at {
            Some(a) => a,
            None => continue,
        };
        if best
            .as_ref()
            .map(|b| window_obs_wins(at, q, b))
            .unwrap_or(true)
        {
            best = Some(EndpointWindow {
                quota: q.clone(),
                observed_at: at,
                source: "recorded",
                fingerprint: fp,
            });
        }
    }
    if fp_set.contains(&builtin_fingerprint) {
        if let Some(home) = live_codex_home() {
            if let Some(live) = account_window(&home) {
                if best
                    .as_ref()
                    .map(|b| window_obs_wins(live.observed_at, &live.quota, b))
                    .unwrap_or(true)
                {
                    best = Some(EndpointWindow {
                        quota: live.quota,
                        observed_at: live.observed_at,
                        source: "rollout",
                        fingerprint: builtin_fingerprint.to_string(),
                    });
                }
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
            let w = endpoint_window(&order, &["fp"], "fp").unwrap();
            assert_eq!(w.source, "recorded");
            assert_eq!(w.quota["primary"]["used_percent"], 99.0);
            let b = blocking_window(&w.quota, Utc::now()).unwrap();
            assert_eq!(b.name, "5-hour window");
        }
    }

    // UNIT (F46-1): the live rollout at the very instant of a lower recorded observation wins.
    #[test]
    fn the_live_window_replaces_a_lower_recorded_one_at_an_equal_time() {
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
        let w = endpoint_window(&[rec], &["fp"], "fp");
        set_live_codex_home(None);
        let _ = std::fs::remove_dir_all(&home);
        let w = w.unwrap();
        assert_eq!(w.source, "rollout");
        assert_eq!(w.quota["primary"]["used_percent"], 99.0);
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
            limit_id: "codex".into(),
        };
        let none = Observation {
            primary: None,
            secondary: None,
            at: Some(at),
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
