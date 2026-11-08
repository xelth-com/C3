//! Budget-aware routing (C3 wave 9): every roster lineage's draw weight is multiplied by a
//! factor derived from the remaining share of its subscription plan relative to the remaining
//! share of the plan's period. The same windows surface in `c3 providers` (QUOTA) and
//! `c3 router explain`.
//!
//! A [`BudgetWindow`] is one cap window of a plan: how much of it is used (`used_ratio`), how much
//! of its period has elapsed (`elapsed_ratio`), when it resets and where the reading came from.
//! Sources are tried per plan in order - openai codex rollouts, the kimi `/usages` endpoint, then
//! a hand-written snapshot the operator refreshes from the vendor's web console - and the first
//! that yields windows wins.
//!
//! The multiplier ([`budget_multiplier`]) gives a plan with lots of headroom (used far below
//! elapsed) a weight above 1 and a plan burning fast a weight below 1; the most constraining
//! window governs. `CODEX_CONSULT_BUDGET_ROUTING=off` pins every multiplier to 1.0.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use serde_json::{json, Value};

use c3_core::roster::RosterEntry;

/// A snapshot older than this many days is stale: shown, but it does not drive the multiplier.
pub const STALE_DAYS: i64 = 7;
/// The multiplier clamp: a window never pulls a weight below this.
pub const ROUTING_BUDGET_MIN: f64 = 0.25;
/// ... nor above this.
pub const ROUTING_BUDGET_MAX: f64 = 3.0;
/// Live readings are cached this long (seconds) so providers / explain / consult do not hit the
/// network on every call.
const CACHE_SECS: i64 = 300;
/// The kimi `/usages` GET timeout.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// One cap window of a plan, judged at a fixed instant.
#[derive(Debug, Clone)]
pub struct BudgetWindow {
    /// The roster entry `plan` the window belongs to (the built-in openai entries: `""`).
    pub plan: String,
    /// `5h` | `wk` | `month` | a vendor-specific short label.
    pub name: String,
    /// 0..1 share of the window used.
    pub used_ratio: f64,
    /// 0..1 share of the window's period already elapsed.
    pub elapsed_ratio: f64,
    pub resets_at: DateTime<Utc>,
    /// `codex-rollout` | `kimi-usages` | `snapshot`.
    pub source: String,
    pub observed_at: DateTime<Utc>,
    /// A snapshot older than [`STALE_DAYS`]: not used for the multiplier.
    pub stale: bool,
}

/// `CODEX_CONSULT_BUDGET_ROUTING=off` turns the budget multiplier off for the whole process.
fn routing_off() -> bool {
    std::env::var("CODEX_CONSULT_BUDGET_ROUTING")
        .map(|v| v.trim().eq_ignore_ascii_case("off"))
        .unwrap_or(false)
}

/// The plan multiplier and the label of the window that governs it. The headroom of a window is
/// `h = (1 - used_ratio) / max(1 - elapsed_ratio, 0.05)`; its multiplier is `clamp(h, MIN, MAX)`;
/// the plan multiplier is the MINIMUM over the non-stale windows (the most constraining governs).
/// No non-stale window -> `(1.0, None)`. `CODEX_CONSULT_BUDGET_ROUTING=off` -> `(1.0, None)`.
pub fn budget_multiplier(windows: &[BudgetWindow]) -> (f64, Option<String>) {
    if routing_off() {
        return (1.0, None);
    }
    let mut m = f64::INFINITY;
    let mut governing: Option<String> = None;
    for w in windows.iter().filter(|w| !w.stale) {
        let denom = (1.0 - w.elapsed_ratio).max(0.05);
        let h = (1.0 - w.used_ratio) / denom;
        let mw = h.clamp(ROUTING_BUDGET_MIN, ROUTING_BUDGET_MAX);
        if mw < m {
            m = mw;
            governing = Some(w.name.clone());
        }
    }
    if m.is_finite() {
        (m, governing)
    } else {
        (1.0, None)
    }
}

/// The windows of one plan: the first source that yields any wins. `entries` are the roster
/// entries of this plan; `network == false` reads cache / snapshot only, never the network.
pub fn plan_windows(
    plan: &str,
    entries: &[RosterEntry],
    now: DateTime<Utc>,
    network: bool,
) -> Vec<BudgetWindow> {
    // 1. openai: the codex rollouts' account window (account-wide, read from the codex home).
    if entries.iter().any(is_openai_builtin) {
        let w = openai_windows(plan, now);
        if !w.is_empty() {
            return w;
        }
    }
    // 2. kimi: the live `/usages` endpoint (cached).
    if entries.iter().any(is_kimi) {
        let w = kimi_windows(plan, entries, now, network);
        if !w.is_empty() {
            return w;
        }
    }
    // 3. snapshot: a file the operator writes with `c3 budget set`.
    snapshot_windows(plan, now)
}

/// The budget group of a roster entry: all built-in openai entries share one ChatGPT account
/// (`openai-account`, whatever their `plan` slug); every other entry's group is its `plan` slug;
/// an entry with no plan and no account has no budget group (`""` -> multiplier 1.0).
pub fn budget_key(e: &RosterEntry) -> String {
    if is_openai_builtin(e) {
        "openai-account".to_string()
    } else if !e.plan.is_empty() {
        e.plan.clone()
    } else {
        String::new()
    }
}

/// The budget multiplier of every group present in `entries`, keyed by [`budget_key`]. The caller
/// (the panel draw) looks a member's multiplier up by its entry's key; absent -> 1.0.
pub fn multipliers_for(
    entries: &[RosterEntry],
    now: DateTime<Utc>,
    network: bool,
) -> std::collections::HashMap<String, f64> {
    let mut out = std::collections::HashMap::new();
    for e in entries {
        let key = budget_key(e);
        if key.is_empty() || out.contains_key(&key) {
            continue;
        }
        let group: Vec<RosterEntry> = entries
            .iter()
            .filter(|x| budget_key(x) == key)
            .cloned()
            .collect();
        let plan_arg = if key == "openai-account" {
            ""
        } else {
            e.plan.as_str()
        };
        let windows = plan_windows(plan_arg, &group, now, network);
        out.insert(key, budget_multiplier(&windows).0);
    }
    out
}

// --------------------------------------------------------------------------- source detection

/// A roster entry on the built-in openai (codex) endpoint, whose quota is the ChatGPT account's.
pub(crate) fn is_openai_builtin(e: &RosterEntry) -> bool {
    let engine = if e.engine.is_empty() {
        "codex"
    } else {
        e.engine.as_str()
    };
    engine == "codex" && e.auth != "endpoint" && e.provider.eq_ignore_ascii_case("openai")
}

/// A roster entry on the Kimi Code plan: the native `kimi` engine, or an endpoint entry whose
/// base URL host is `api.kimi.ai`.
fn is_kimi(e: &RosterEntry) -> bool {
    e.engine.eq_ignore_ascii_case("kimi")
        || e.endpoint
            .as_ref()
            .map(|ep| ep.host_name.eq_ignore_ascii_case("api.kimi.ai"))
            .unwrap_or(false)
}

/// The env var holding the Kimi Code key: the endpoint entry's `env_key`, else `KIMI_API_KEY`
/// (the native engine entry carries no endpoint).
const KIMI_ENV_KEY_DEFAULT: &str = "KIMI_API_KEY";

// --------------------------------------------------------------------------- openai (codex)

/// Wrap [`c3_core::quota::account_window_at`]: the ChatGPT account's 5h / weekly windows.
fn openai_windows(plan: &str, now: DateTime<Utc>) -> Vec<BudgetWindow> {
    let home = crate::providers::get_codex_home();
    let aw = match c3_core::quota::account_window_at(&home, now) {
        Some(a) => a,
        None => return Vec::new(),
    };
    windows_from_quota(
        plan,
        &aw.quota,
        aw.observed_at.with_timezone(&Utc),
        "codex-rollout",
        now,
    )
}

/// Convert a recorded `{primary, secondary}` quota (used_percent / window_minutes / resets_at ISO)
/// into budget windows; `elapsed = 1 - (resets - now) / window_minutes`.
fn windows_from_quota(
    plan: &str,
    quota: &Value,
    observed_at: DateTime<Utc>,
    source: &str,
    now: DateTime<Utc>,
) -> Vec<BudgetWindow> {
    let mut out = Vec::new();
    for key in ["primary", "secondary"] {
        let w = &quota[key];
        if !w.is_object() {
            continue;
        }
        let used = match c3_core::quota::finite_percent(&w["used_percent"]) {
            Some(f) => f,
            None => continue,
        };
        let minutes = w["window_minutes"].as_i64();
        let resets_at = match w["resets_at"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        {
            Some(r) => r.with_timezone(&Utc),
            None => continue,
        };
        let elapsed = match minutes {
            Some(m) if m > 0 => elapsed_ratio(resets_at, (m * 60) as f64, now),
            _ => 0.0,
        };
        out.push(BudgetWindow {
            plan: plan.to_string(),
            name: c3_core::quota::window_name(minutes, true),
            used_ratio: used / 100.0,
            elapsed_ratio: elapsed,
            resets_at,
            source: source.to_string(),
            observed_at,
            stale: false,
        });
    }
    out
}

// --------------------------------------------------------------------------- kimi (/usages)

/// The kimi coding plan's 5h and month windows. Cached under `<codex home>/c3-budget/.cache-<plan>.json`
/// for [`CACHE_SECS`]; `network == false` reads the cache only.
fn kimi_windows(
    plan: &str,
    entries: &[RosterEntry],
    now: DateTime<Utc>,
    network: bool,
) -> Vec<BudgetWindow> {
    let cache = cache_path(plan);
    if let Some((fetched_at, body)) = read_cache(cache.as_deref()) {
        let fresh = (now - fetched_at).num_seconds() < CACHE_SECS;
        if fresh || !network {
            return parse_kimi(&body, plan, now);
        }
    } else if !network {
        return Vec::new();
    }
    let env_key = entries
        .iter()
        .find_map(|e| {
            e.endpoint
                .as_ref()
                .filter(|ep| ep.host_name.eq_ignore_ascii_case("api.kimi.ai"))
                .map(|ep| ep.env_key.clone())
        })
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| KIMI_ENV_KEY_DEFAULT.to_string());
    let token = std::env::var(&env_key).ok().filter(|v| !v.is_empty());
    let token = match token {
        Some(t) => t,
        None => return Vec::new(),
    };
    let body = match kimi_get(&token) {
        Some(b) => b,
        None => return Vec::new(),
    };
    write_cache(cache.as_deref(), now, &body);
    parse_kimi(&body, plan, now)
}

/// GET `https://api.kimi.ai/coding/v1/usages` with the bearer token; the body on a 200, else None.
fn kimi_get(token: &str) -> Option<String> {
    let agent = ureq::AgentBuilder::new().timeout(HTTP_TIMEOUT).build();
    match agent
        .get("https://api.kimi.ai/coding/v1/usages")
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(resp) => resp.into_string().ok(),
        Err(_) => None,
    }
}

/// Parse the kimi `/usages` body into the 5h and month windows.
fn parse_kimi(body: &str, plan: &str, now: DateTime<Utc>) -> Vec<BudgetWindow> {
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let u = &v["usages"];
    let mut out = Vec::new();
    if let Some(w) = kimi_window(plan, &u["limit_5h"], "5h", Some(300), now) {
        out.push(w);
    }
    if let Some(w) = kimi_window(plan, &u["limit_month_total"], "month", None, now) {
        out.push(w);
    }
    out
}

/// One kimi usage window: `{used_ratio, reset_time}`. `period_min` fixes the period (5h); `None`
/// (month) computes it as `reset_time` minus one calendar month.
fn kimi_window(
    plan: &str,
    obj: &Value,
    name: &str,
    period_min: Option<i64>,
    now: DateTime<Utc>,
) -> Option<BudgetWindow> {
    let used = obj["used_ratio"].as_f64()?;
    let resets_at = obj["reset_time"]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())?
        .with_timezone(&Utc);
    let period_secs = match period_min {
        Some(m) => (m * 60) as f64,
        None => (resets_at - minus_one_month(resets_at)).num_seconds() as f64,
    };
    Some(BudgetWindow {
        plan: plan.to_string(),
        name: name.to_string(),
        used_ratio: used,
        elapsed_ratio: elapsed_ratio(resets_at, period_secs, now),
        resets_at,
        source: "kimi-usages".to_string(),
        observed_at: now,
        stale: false,
    })
}

// --------------------------------------------------------------------------- snapshot

/// The snapshot file of a plan: `<codex home>/c3-budget/<plan>.json`.
fn snapshot_windows(plan: &str, now: DateTime<Utc>) -> Vec<BudgetWindow> {
    if plan.is_empty() {
        return Vec::new();
    }
    let path = match snapshot_path(plan) {
        Some(p) => p,
        None => return Vec::new(),
    };
    let v: Value = match std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(v) => v,
        None => return Vec::new(),
    };
    match window_from_snapshot(plan, &v, now) {
        Some(w) => vec![w],
        None => Vec::new(),
    }
}

/// `{window, used_ratio, resets_at, observed_at, period_days?}` -> a window; `elapsed_ratio` is
/// computed live, `stale` from `observed_at` vs [`STALE_DAYS`].
fn window_from_snapshot(plan: &str, v: &Value, now: DateTime<Utc>) -> Option<BudgetWindow> {
    let window = v["window"].as_str().unwrap_or("month").to_string();
    let used = v["used_ratio"].as_f64()?;
    let resets_at = v["resets_at"]
        .as_str()
        .and_then(parse_when)
        .map(|r| r.with_timezone(&Utc))?;
    let observed_at = v["observed_at"]
        .as_str()
        .and_then(parse_when)
        .map(|r| r.with_timezone(&Utc))
        .unwrap_or(now);
    let period_secs = match v["period_days"].as_i64() {
        Some(d) if d > 0 => (d * 86_400) as f64,
        _ => default_period_secs(&window, resets_at),
    };
    let stale = (now - observed_at).num_days() > STALE_DAYS;
    Some(BudgetWindow {
        plan: plan.to_string(),
        name: window,
        used_ratio: used,
        elapsed_ratio: elapsed_ratio(resets_at, period_secs, now),
        resets_at,
        source: "snapshot".to_string(),
        observed_at,
        stale,
    })
}

/// The period of a named window without an explicit `period_days`.
fn default_period_secs(window: &str, resets_at: DateTime<Utc>) -> f64 {
    match window {
        "week" | "wk" => 7.0 * 86_400.0,
        "5h" => 5.0 * 3600.0,
        "day" | "daily" => 86_400.0,
        _ => (resets_at - minus_one_month(resets_at)).num_seconds() as f64, // month
    }
}

// --------------------------------------------------------------------------- shared math / io

/// `1 - (resets - now) / period`, clamped to 0..1.
fn elapsed_ratio(resets_at: DateTime<Utc>, period_secs: f64, now: DateTime<Utc>) -> f64 {
    if period_secs <= 0.0 {
        return 0.0;
    }
    let remain = (resets_at - now).num_seconds() as f64;
    (1.0 - remain / period_secs).clamp(0.0, 1.0)
}

/// `dt` minus one calendar month (the day clamped to the shorter month's length).
fn minus_one_month(dt: DateTime<Utc>) -> DateTime<Utc> {
    let (y, m) = (dt.year(), dt.month());
    let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
    let day = dt.day().min(days_in_month(py, pm));
    Utc.with_ymd_and_hms(py, pm, day, dt.hour(), dt.minute(), dt.second())
        .single()
        .unwrap_or(dt)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = chrono::NaiveDate::from_ymd_opt(ny, nm, 1).unwrap();
    let first_this = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    (first_next - first_this).num_days() as u32
}

/// Parse an ISO datetime, or a bare `YYYY-MM-DD` (taken as `T00:00:00Z`).
pub fn parse_when(s: &str) -> Option<DateTime<chrono::FixedOffset>> {
    let s = s.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d);
    }
    // bare date, or date with a 'Z' but no seconds etc.
    if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = nd.and_hms_opt(0, 0, 0)?;
        return Some(Utc.from_utc_datetime(&dt).fixed_offset());
    }
    None
}

/// `<codex home>/c3-budget`.
fn budget_dir() -> Option<PathBuf> {
    let home = crate::providers::get_codex_home();
    if home.is_empty() {
        return None;
    }
    Some(Path::new(&home).join("c3-budget"))
}

fn snapshot_path(plan: &str) -> Option<PathBuf> {
    budget_dir().map(|d| d.join(format!("{plan}.json")))
}

fn cache_path(plan: &str) -> Option<PathBuf> {
    budget_dir().map(|d| d.join(format!(".cache-{plan}.json")))
}

/// Read a live-reading cache file: `{fetched_at, body}`.
fn read_cache(path: Option<&Path>) -> Option<(DateTime<Utc>, String)> {
    let path = path?;
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let fetched_at = v["fetched_at"]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())?
        .with_timezone(&Utc);
    let body = v["body"].as_str()?.to_string();
    Some((fetched_at, body))
}

fn write_cache(path: Option<&Path>, now: DateTime<Utc>, body: &str) {
    let path = match path {
        Some(p) => p,
        None => return,
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let v = json!({ "fetched_at": now.to_rfc3339(), "body": body });
    let _ = std::fs::write(path, serde_json::to_string(&v).unwrap_or_default());
}

// --------------------------------------------------------------------------- snapshot set / show

/// The JSON a snapshot file holds (also the round-trip unit for [`window_from_snapshot`]).
pub fn snapshot_json(
    window: &str,
    used_ratio: f64,
    resets_at: DateTime<Utc>,
    observed_at: DateTime<Utc>,
    period_days: Option<i64>,
) -> Value {
    let mut v = json!({
        "window": window,
        "used_ratio": used_ratio,
        "resets_at": resets_at.to_rfc3339(),
        "observed_at": observed_at.to_rfc3339(),
    });
    if let Some(d) = period_days {
        v["period_days"] = json!(d);
    }
    v
}

/// Write a plan's snapshot (`c3 budget set`); returns the path written.
pub fn write_snapshot(
    plan: &str,
    window: &str,
    used_ratio: f64,
    resets_at: DateTime<Utc>,
    period_days: Option<i64>,
    now: DateTime<Utc>,
) -> Result<PathBuf, String> {
    let path = snapshot_path(plan)
        .ok_or_else(|| "no codex home (set CODEX_HOME or HOME)".to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let v = snapshot_json(window, used_ratio, resets_at, now, period_days);
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default())
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// Every plan snapshot under `<codex home>/c3-budget` (one window each), for `c3 budget show`.
pub fn all_snapshots(now: DateTime<Utc>) -> Vec<BudgetWindow> {
    let dir = match budget_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !name.ends_with(".json") {
            continue; // skip `.cache-*.json`
        }
        let plan = name.trim_end_matches(".json").to_string();
        out.extend(snapshot_windows(&plan, now));
    }
    out.sort_by(|a, b| a.plan.cmp(&b.plan));
    out
}

// --------------------------------------------------------------------------- views (providers / explain)

/// The governing window of a set (the one [`budget_multiplier`] named), for the display columns.
pub fn governing_window<'a>(windows: &'a [BudgetWindow], label: &str) -> Option<&'a BudgetWindow> {
    windows.iter().find(|w| w.name == label)
}

/// `c3 router explain` budget cell: `month 14%/40%` (used / elapsed) for the governing window;
/// `-` when there is none.
pub fn explain_budget_cell(windows: &[BudgetWindow], governing: &Option<String>) -> String {
    match governing
        .as_ref()
        .and_then(|g| governing_window(windows, g))
    {
        Some(w) => format!(
            "{} {}%/{}%",
            w.name,
            (w.used_ratio * 100.0).round() as i64,
            (w.elapsed_ratio * 100.0).round() as i64
        ),
        None => "-".to_string(),
    }
}

/// `c3 providers` QUOTA cell for a plan's windows, in the openai style
/// (`5h 72% until 22:08; wk 56% until Nov 15`; month -> `mo 14% until Nov 26`; a snapshot adds
/// `(seen Nd ago)`, a stale one `(seen Nd ago, stale - refresh)`).
pub fn providers_quota_cell(windows: &[BudgetWindow], now: DateTime<Utc>) -> String {
    let parts: Vec<String> = windows
        .iter()
        .map(|w| {
            let label = short_label(&w.name);
            let pct = (w.used_ratio * 100.0).round() as i64;
            let until = format_until(w.resets_at, now);
            let mut s = format!("{label} {pct}% until {until}");
            if w.source == "snapshot" {
                let days = (now - w.observed_at).num_days().max(0);
                if w.stale {
                    s.push_str(&format!(" (seen {days}d ago, stale - refresh)"));
                } else {
                    s.push_str(&format!(" (seen {days}d ago)"));
                }
            }
            s
        })
        .collect();
    parts.join("; ")
}

/// `month` -> `mo`, `week` -> `wk`; others unchanged.
fn short_label(name: &str) -> String {
    match name {
        "month" => "mo".to_string(),
        "week" => "wk".to_string(),
        other => other.to_string(),
    }
}

/// `HH:MM` within 24 h, else `Mon D` (local), matching the openai quota style.
fn format_until(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    c3_core::quota::format_reset(when.fixed_offset(), now)
}

#[cfg(test)]
mod tests;
