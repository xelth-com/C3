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
/// ... nor above this: an idle plan at most doubles a seat's weight (a boost is cheaper evidence
/// than a penalty - the usefulness score still has to carry the seat).
pub const ROUTING_BUDGET_MAX: f64 = 2.0;
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
    /// The window's full period in seconds (`5h` = 18000, `week` = 604800, ...); `0.0` when the
    /// source could not fix it. A window is LONG (the protected reserve applies to it) when this is
    /// 24 h or more.
    pub period_secs: f64,
    pub resets_at: DateTime<Utc>,
    /// `codex-rollout` | `kimi-usages` | `snapshot`.
    pub source: String,
    pub observed_at: DateTime<Utc>,
    /// A snapshot older than [`STALE_DAYS`]: not used for the multiplier at all.
    pub stale: bool,
    /// (0.2.4, F06-4/F07-1) The reading was observed within the last hour. An OLD reading (not
    /// fresh) is a FLOOR on usage - usage can only have grown since - so it may HOLD a protected
    /// reviewer and may PENALIZE a seat, but it must never BOOST one: [`budget_multiplier`] caps a
    /// non-fresh window's contribution at 1.0. This is distinct from `stale`, which drops a window
    /// entirely; a days-old 97 % weekly reading is not stale (it still holds) but not fresh (it
    /// cannot boost).
    pub fresh: bool,
}

/// (0.2.4) A reading observed within this many seconds of `now` is FRESH (may boost a seat); older
/// readings are a usage floor (hold / penalize only).
pub const FRESH_SECS: i64 = 3600;

/// Whether an observation at `observed_at` is fresh at `now`: within [`FRESH_SECS`], and not dated
/// in the future beyond a few minutes of clock skew.
fn observed_fresh(observed_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    let age = (now - observed_at).num_seconds();
    (-300..=FRESH_SECS).contains(&age)
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
        let mut mw = h.clamp(ROUTING_BUDGET_MIN, ROUTING_BUDGET_MAX);
        // (F06-4/F07-1) an old reading is a usage floor: it may penalize (mw < 1) but never boost.
        if !w.fresh {
            mw = mw.min(1.0);
        }
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

/// The multiplier that actually scales a seat: a plan's idle quota never BOOSTS a lineage whose
/// usefulness score sits below `neutral` (the ratings say it is worse than an unknown reviewer, so
/// its free quota is not worth seats); penalties (`mult < 1`) always apply.
pub fn effective_multiplier(score: f64, neutral: f64, plan_mult: f64) -> f64 {
    if plan_mult > 1.0 && score < neutral {
        1.0
    } else {
        plan_mult
    }
}

/// A window is LONG when its period is 24 h or more (weekly, monthly): the protected reserve
/// applies only to long windows (the 5-hour window is exempt). The threshold uses a small slack so
/// a period expressed as exactly a day is not lost to float error.
pub const LONG_WINDOW_SECS: f64 = 24.0 * 3600.0 - 1.0;

impl BudgetWindow {
    /// Whether this is a long window (period of 24 h or more).
    pub fn is_long(&self) -> bool {
        self.period_secs >= LONG_WINDOW_SECS
    }
}

/// (0.2.4, reviewer-protection) Whether a protected entry with reserve `reserve_percent` (1..=50)
/// is HELD at `now`, given its budget group's `windows`. The entry is held when, on ANY long window
/// (period >= 24 h), non-stale, whose reset is still ahead, its usage has run past the reserve line:
/// `used% >= min(elapsed% + N, 100 - N)`. In words, the route may not run more than N points ahead
/// of the calendar, and its last N % is kept back. No long window with data -> not held. Returns the
/// governing window (the first that holds) so the caller can name the hold; `None` when not held.
pub fn reserve_hold<'a>(
    windows: &'a [BudgetWindow],
    reserve_percent: i64,
    now: DateTime<Utc>,
) -> Option<&'a BudgetWindow> {
    if reserve_percent <= 0 {
        return None;
    }
    let n = reserve_percent as f64;
    windows.iter().find(|w| {
        if !w.is_long() || w.stale || w.resets_at <= now {
            return false;
        }
        let used = w.used_ratio * 100.0;
        let elapsed = w.elapsed_ratio * 100.0;
        used >= (elapsed + n).min(100.0 - n)
    })
}

/// (0.2.4, reviewer-protection) The note a held entry shows on `c3 providers` / `c3 router explain`
/// / the preflight refusal: `weekly window 97 % used at 55 % of the week, reserve 15 %`. When the
/// reading is not fresh (observed over an hour ago - an 8-day rollout fallback, a snapshot) the age
/// is named, since the hold rests on a reading that is only a floor: `... reserve 15 % (seen 6d ago)`.
pub fn reserve_hold_note(w: &BudgetWindow, reserve_percent: i64, now: DateTime<Utc>) -> String {
    let (adjective, period) = long_window_phrase(&w.name);
    let mut note = format!(
        "{adjective} {}% used at {}% of the {period}, reserve {reserve_percent}%",
        (w.used_ratio * 100.0).round() as i64,
        (w.elapsed_ratio * 100.0).round() as i64,
    );
    if !w.fresh {
        let days = (now - w.observed_at).num_days().max(0);
        if days >= 1 {
            note.push_str(&format!(" (seen {days}d ago)"));
        } else {
            let hours = (now - w.observed_at).num_hours().max(0);
            note.push_str(&format!(" (seen {hours}h ago)"));
        }
    }
    note
}

/// The human phrase for a long window's name: `(adjective window, period noun)`.
fn long_window_phrase(name: &str) -> (String, &'static str) {
    match name {
        "wk" | "week" | "weekly" => ("weekly window".to_string(), "week"),
        "mo" | "month" | "monthly" => ("monthly window".to_string(), "month"),
        "day" | "daily" => ("daily window".to_string(), "day"),
        other => (format!("{other} window"), "window"),
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
    // 3. z.ai: the live quota/limit endpoint (cached).
    if entries.iter().any(is_zai) {
        let w = zai_windows(plan, entries, now, network);
        if !w.is_empty() {
            return w;
        }
    }
    // 4. muse: the local `muse usage --json` probe (cached).
    if entries.iter().any(is_muse) {
        let w = muse_windows(plan, now, network);
        if !w.is_empty() {
            return w;
        }
    }
    // 5. snapshot: a file the operator writes with `c3 budget set`.
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

/// (0.2.4, reviewer-protection) The budget-group windows of one roster entry (the group it shares
/// through [`budget_key`]: all built-in openai entries share the ChatGPT account, every other entry
/// its `plan`). `network == false` reads cache / snapshot only, never the network. Empty when the
/// entry has no budget group or no reading is available. The reserve hold and the display both use
/// this so they see exactly the windows the panel draw's multiplier saw.
pub fn entry_windows(
    roster: &c3_core::roster::Roster,
    e: &RosterEntry,
    now: DateTime<Utc>,
    network: bool,
) -> Vec<BudgetWindow> {
    let key = budget_key(e);
    if key.is_empty() {
        return Vec::new();
    }
    let group: Vec<RosterEntry> = roster
        .entries
        .iter()
        .filter(|x| budget_key(x) == key)
        .cloned()
        .collect();
    let plan_arg = if key == "openai-account" {
        ""
    } else {
        e.plan.as_str()
    };
    plan_windows(plan_arg, &group, now, network)
}

/// (0.2.4, reviewer-protection) Whether `e` is HELD by its protected reserve at `now`, and the note
/// naming the hold. `None` when the entry is not protected (`reserve_percent == 0`) or not held.
/// `network == false` reads cache / snapshot only. The standing-purpose exemption and the
/// `--spend-reserve` override are the CALLER's to apply - this reports the budget fact only.
pub fn entry_reserve_hold(
    roster: &c3_core::roster::Roster,
    e: &RosterEntry,
    now: DateTime<Utc>,
    network: bool,
) -> Option<String> {
    if e.reserve_percent <= 0 {
        return None;
    }
    let windows = entry_windows(roster, e, now, network);
    reserve_hold(&windows, e.reserve_percent, now)
        .map(|w| reserve_hold_note(w, e.reserve_percent, now))
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

/// A roster entry on the z.ai (GLM coding) plan: an endpoint entry whose base URL host is
/// `api.z.ai`.
fn is_zai(e: &RosterEntry) -> bool {
    e.endpoint
        .as_ref()
        .map(|ep| ep.host_name.eq_ignore_ascii_case("api.z.ai"))
        .unwrap_or(false)
}

/// The env var holding the z.ai key: the endpoint entry's `env_key`, else `ZAI_API_KEY`.
const ZAI_ENV_KEY_DEFAULT: &str = "ZAI_API_KEY";

/// A roster entry on the Meta Muse Code plan: the native `muse` engine.
fn is_muse(e: &RosterEntry) -> bool {
    e.engine.eq_ignore_ascii_case("muse")
}

// --------------------------------------------------------------------------- openai (codex)

/// The ChatGPT account's 5h / weekly windows. The primary source is today's and yesterday's codex
/// rollouts ([`c3_core::quota::account_window_at`], the same reading the health verdict uses). When
/// that gives nothing - the operator had no Codex session in two days, so the account's window is
/// "forgotten" and the budget would wrongly read as idle - two fallbacks step in, their windows
/// kept only while their reset is still ahead and tagged with the real observation time (so the
/// display can say "seen Nd ago"):
///   1. the newest observation in the rollouts of the last 8 days;
///   2. the recorded quota of the machine-wide health file / ledgers.
fn openai_windows(plan: &str, now: DateTime<Utc>) -> Vec<BudgetWindow> {
    let home = crate::providers::get_codex_home();
    // (F06-4) Every source keeps only windows whose reset is still ahead and ignores a reading whose
    // observation is dated in the future: if the two-day result has no live window (all reset in the
    // past, or it is empty), fall through to the 8-day scan and then the recorded health quota.
    // 1. Today + yesterday (cached; the health verdict's two-day window).
    if let Some(aw) = c3_core::quota::account_window_at(&home, now) {
        let observed = aw.observed_at.with_timezone(&Utc);
        if observed_not_future(observed, now) {
            let w = keep_future(
                windows_from_quota(plan, &aw.quota, observed, "codex-rollout", now),
                now,
            );
            if !w.is_empty() {
                return w;
            }
        }
    }
    // 2. Fallback: the newest observation in the rollouts of the last 8 days.
    if let Some(aw) = c3_core::quota::account_window_within(&home, now, 8) {
        let observed = aw.observed_at.with_timezone(&Utc);
        if observed_not_future(observed, now) {
            let w = keep_future(
                windows_from_quota(plan, &aw.quota, observed, "codex-rollout-8d", now),
                now,
            );
            if !w.is_empty() {
                return w;
            }
        }
    }
    // 3. Fallback: the recorded quota of the machine-wide health file / ledgers.
    if let Some(ew) = openai_recorded_window(now) {
        let observed = ew.observed_at.with_timezone(&Utc);
        if observed_not_future(observed, now) {
            let w = keep_future(
                windows_from_quota(plan, &ew.quota, observed, "codex-recorded", now),
                now,
            );
            if !w.is_empty() {
                return w;
            }
        }
    }
    Vec::new()
}

/// Whether an observation at `observed_at` is NOT dated in the future beyond a few minutes of clock
/// skew (F06-4: a future-dated reading is ignored - it would otherwise win a merge and hide a real
/// one, and its "elapsed" would be nonsense).
fn observed_not_future(observed_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    (now - observed_at).num_seconds() >= -300
}

/// Keep only the windows whose reset is still ahead of `now` (a fallback reading of a window that
/// already reset carries no usage info for the current period).
fn keep_future(windows: Vec<BudgetWindow>, now: DateTime<Utc>) -> Vec<BudgetWindow> {
    windows.into_iter().filter(|w| w.resets_at > now).collect()
}

/// The built-in openai endpoint's most recent recorded window from the machine-wide health file and
/// its ledgers (never the network). `None` when health is off, there is no record, or none names a
/// window.
fn openai_recorded_window(now: DateTime<Utc>) -> Option<c3_core::quota::EndpointWindow> {
    let home = crate::providers::get_codex_home();
    let hp = c3_core::health::machine_health_path(&home)?;
    let consults = c3_core::health::machine_endpoint_consults_all(&hp);
    let fp = c3_core::health::builtin_openai_fingerprint();
    c3_core::quota::endpoint_window_at(&consults, &[&fp], &fp, now)
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
        let period_secs = match minutes {
            Some(m) if m > 0 => (m * 60) as f64,
            _ => 0.0,
        };
        let elapsed = if period_secs > 0.0 {
            elapsed_ratio(resets_at, period_secs, now)
        } else {
            0.0
        };
        out.push(BudgetWindow {
            plan: plan.to_string(),
            name: c3_core::quota::window_name(minutes, true),
            used_ratio: used / 100.0,
            elapsed_ratio: elapsed,
            period_secs,
            resets_at,
            source: source.to_string(),
            observed_at,
            stale: false,
            fresh: observed_fresh(observed_at, now),
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
    let cache = cache_path("kimi", plan);
    if let Some((fetched_at, body)) = read_cache(cache.as_deref()) {
        let fresh = (now - fetched_at).num_seconds() < CACHE_SECS;
        if fresh || !network {
            // (F07-1) a cached body carries the cache's fetch time as its real observation time.
            return parse_kimi(&body, plan, fetched_at, now);
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
    parse_kimi(&body, plan, now, now)
}

/// GET `https://api.kimi.ai/coding/v1/usages` with the bearer token; the body on a 200, else None.
/// A keyed GET never follows a redirect (`redirects(0)`): a 3xx is treated as no data, so the
/// `Authorization` header is never replayed to a redirect target.
fn kimi_get(token: &str) -> Option<String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(HTTP_TIMEOUT)
        .redirects(0)
        .build();
    match agent
        .get("https://api.kimi.ai/coding/v1/usages")
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(resp) if resp.status() < 300 => resp.into_string().ok(),
        _ => None,
    }
}

/// Parse the kimi `/usages` body into the 5h and month windows.
fn parse_kimi(
    body: &str,
    plan: &str,
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Vec<BudgetWindow> {
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let u = &v["usages"];
    let mut out = Vec::new();
    if let Some(w) = kimi_window(plan, &u["limit_5h"], "5h", Some(300), observed_at, now) {
        out.push(w);
    }
    if let Some(w) = kimi_window(plan, &u["limit_month_total"], "month", None, observed_at, now) {
        out.push(w);
    }
    out
}

/// One kimi usage window: `{used_ratio, reset_time}`. `period_min` fixes the period (5h); `None`
/// (month) computes it as `reset_time` minus one calendar month. `observed_at` is the reading's real
/// observation time (the cache fetch time, or `now` for a fresh fetch).
fn kimi_window(
    plan: &str,
    obj: &Value,
    name: &str,
    period_min: Option<i64>,
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Option<BudgetWindow> {
    let used = obj["used_ratio"].as_f64()?;
    let resets_at = obj["reset_time"]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())?
        .with_timezone(&Utc);
    // (F03-10) A live window whose reset is already past describes a finished period (the new one
    // starts at 0 and only a fresh reading knows it): drop it, so a snapshot or "no data" applies.
    if resets_at <= now {
        return None;
    }
    let period_secs = match period_min {
        Some(m) => (m * 60) as f64,
        None => (resets_at - minus_one_month(resets_at)).num_seconds() as f64,
    };
    Some(BudgetWindow {
        plan: plan.to_string(),
        name: name.to_string(),
        used_ratio: used,
        elapsed_ratio: elapsed_ratio(resets_at, period_secs, now),
        period_secs,
        resets_at,
        source: "kimi-usages".to_string(),
        observed_at,
        stale: false,
        fresh: observed_fresh(observed_at, now),
    })
}

// --------------------------------------------------------------------------- z.ai (quota/limit)

/// The z.ai (GLM coding) plan's 5-hour token window. Cached under
/// `<codex home>/c3-budget/.cache-<plan>.json` for [`CACHE_SECS`]; `network == false` reads the
/// cache only.
fn zai_windows(
    plan: &str,
    entries: &[RosterEntry],
    now: DateTime<Utc>,
    network: bool,
) -> Vec<BudgetWindow> {
    let cache = cache_path("zai", plan);
    if let Some((fetched_at, body)) = read_cache(cache.as_deref()) {
        let fresh = (now - fetched_at).num_seconds() < CACHE_SECS;
        if fresh || !network {
            // (F07-1) a cached body is observed at the cache's fetch time, not now.
            return parse_zai(&body, plan, fetched_at, now);
        }
    } else if !network {
        return Vec::new();
    }
    let env_key = entries
        .iter()
        .find_map(|e| {
            e.endpoint
                .as_ref()
                .filter(|ep| ep.host_name.eq_ignore_ascii_case("api.z.ai"))
                .map(|ep| ep.env_key.clone())
        })
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| ZAI_ENV_KEY_DEFAULT.to_string());
    let token = std::env::var(&env_key).ok().filter(|v| !v.is_empty());
    let token = match token {
        Some(t) => t,
        None => return Vec::new(),
    };
    let body = match zai_get(&token) {
        Some(b) => b,
        None => return Vec::new(),
    };
    write_cache(cache.as_deref(), now, &body);
    parse_zai(&body, plan, now, now)
}

/// GET `https://api.z.ai/api/monitor/usage/quota/limit` with the bearer token; the body on a 200,
/// else None. A keyed GET never follows a redirect (`redirects(0)`): a 3xx is treated as no data, so
/// the `Authorization` header is never replayed to a redirect target.
fn zai_get(token: &str) -> Option<String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(HTTP_TIMEOUT)
        .redirects(0)
        .build();
    match agent
        .get("https://api.z.ai/api/monitor/usage/quota/limit")
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(resp) if resp.status() < 300 => resp.into_string().ok(),
        _ => None,
    }
}

/// Parse the z.ai quota/limit body into the 5-hour token window. `data.limits[]` holds a
/// `TOKENS_LIMIT` item (the 5-hour model-token window: `percentage` used, `nextResetTime` in epoch
/// ms) and a `TIME_LIMIT` item (a monthly MCP-tools allowance, NOT a model quota - ignored).
fn parse_zai(
    body: &str,
    plan: &str,
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Vec<BudgetWindow> {
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let limits = match v["data"]["limits"].as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in limits {
        if item["type"].as_str() == Some("TOKENS_LIMIT") {
            if let Some(w) = zai_window(plan, item, observed_at, now) {
                out.push(w);
            }
        }
    }
    out
}

/// One z.ai `TOKENS_LIMIT` item -> the `5h` window: `{percentage (0-100), nextResetTime (epoch ms)}`.
fn zai_window(
    plan: &str,
    obj: &Value,
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Option<BudgetWindow> {
    let pct = c3_core::quota::finite_percent(&obj["percentage"])?;
    let resets_at = epoch_ms(obj["nextResetTime"].as_i64()?)?;
    // (F03-10) A live window past its reset carries no usage info for the current period: drop it.
    if resets_at <= now {
        return None;
    }
    let period_secs = 5.0 * 3600.0;
    Some(BudgetWindow {
        plan: plan.to_string(),
        name: "5h".to_string(),
        used_ratio: pct / 100.0,
        elapsed_ratio: elapsed_ratio(resets_at, period_secs, now),
        period_secs,
        resets_at,
        source: "zai-quota".to_string(),
        observed_at,
        stale: false,
        fresh: observed_fresh(observed_at, now),
    })
}

// --------------------------------------------------------------------------- muse (usage --json)

/// The Meta Muse Code plan's 5-hour and weekly windows. Cached under
/// `<codex home>/c3-budget/.cache-<plan>.json` for [`CACHE_SECS`]; `network == false` reads the
/// cache only (the probe spawns `muse usage --json`, which makes no model turn).
fn muse_windows(plan: &str, now: DateTime<Utc>, network: bool) -> Vec<BudgetWindow> {
    let cache = cache_path("muse", plan);
    if let Some((fetched_at, body)) = read_cache(cache.as_deref()) {
        let fresh = (now - fetched_at).num_seconds() < CACHE_SECS;
        if fresh || !network {
            // (F07-1) a cached body is observed at the cache's fetch time, not now.
            return parse_muse(&body, plan, fetched_at, now);
        }
    } else if !network {
        return Vec::new();
    }
    let body = match muse_get() {
        Some(b) => b,
        None => return Vec::new(),
    };
    write_cache(cache.as_deref(), now, &body);
    parse_muse(&body, plan, now, now)
}

/// Run `muse usage --json` (the muse launcher resolved as every muse turn resolves it) bounded to
/// ~10 s and return its stdout on a clean exit; any failure (no launcher, a non-zero exit, a
/// timeout) yields None.
fn muse_get() -> Option<String> {
    let launcher = crate::providers::resolve_engine_launcher("muse", "")
        .ok()
        .flatten()
        .filter(|l| !l.trim().is_empty())?;
    let (code, out, _err) =
        crate::providers::run_with_timeout(&launcher, &["usage", "--json"], HTTP_TIMEOUT)?;
    if code != 0 {
        return None;
    }
    Some(out)
}

/// Parse the `muse usage --json` body into the 5-hour and weekly windows. Before the first use in a
/// window `allowance` is null (`reason: "server_sent_no_usage"`) -> no windows (the snapshot, if any,
/// still applies). A populated `allowance` carries `window` (5h) and `weekly`, each
/// `{usedPercent (0-100), resetsAtMs (epoch ms)}`.
fn parse_muse(
    body: &str,
    plan: &str,
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Vec<BudgetWindow> {
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let allowance = &v["allowance"];
    if !allowance.is_object() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(w) = muse_window(plan, &allowance["window"], "5h", 5.0 * 3600.0, observed_at, now) {
        out.push(w);
    }
    if let Some(w) = muse_window(
        plan,
        &allowance["weekly"],
        "week",
        7.0 * 86_400.0,
        observed_at,
        now,
    ) {
        out.push(w);
    }
    out
}

/// One muse allowance window: `{usedPercent (0-100), resetsAtMs (epoch ms)}` over a fixed period.
fn muse_window(
    plan: &str,
    obj: &Value,
    name: &str,
    period_secs: f64,
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Option<BudgetWindow> {
    let pct = c3_core::quota::finite_percent(&obj["usedPercent"])?;
    let resets_at = epoch_ms(obj["resetsAtMs"].as_i64()?)?;
    // (F03-10) A live window past its reset carries no usage info for the current period: drop it.
    if resets_at <= now {
        return None;
    }
    Some(BudgetWindow {
        plan: plan.to_string(),
        name: name.to_string(),
        used_ratio: pct / 100.0,
        elapsed_ratio: elapsed_ratio(resets_at, period_secs, now),
        period_secs,
        resets_at,
        source: "muse-usage".to_string(),
        observed_at,
        stale: false,
        fresh: observed_fresh(observed_at, now),
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
    // Stale: observed too long ago, OR the window it describes has already reset (the figure
    // belongs to a finished period; the new one starts at 0 and only a fresh reading knows it).
    let stale = (now - observed_at).num_days() > STALE_DAYS || resets_at <= now;
    Some(BudgetWindow {
        plan: plan.to_string(),
        name: window,
        used_ratio: used,
        elapsed_ratio: elapsed_ratio(resets_at, period_secs, now),
        period_secs,
        resets_at,
        source: "snapshot".to_string(),
        observed_at,
        stale,
        // A hand-written snapshot is never "fresh" unless it was written within the hour: it may
        // hold / penalize, but it does not boost a seat (F06-4/F07-1).
        fresh: observed_fresh(observed_at, now),
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

/// An epoch-milliseconds instant (z.ai `nextResetTime`, muse `resetsAtMs`) as a UTC datetime.
fn epoch_ms(ms: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms).single()
}

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

/// Parse an ISO datetime, or a bare `YYYY-MM-DD` (taken as `T00:00:00Z`). An instant without
/// seconds is accepted too: `YYYY-MM-DDTHH:MMZ` (UTC) and `YYYY-MM-DDTHH:MM+HH:MM` (offset), which
/// RFC 3339 itself rejects.
pub fn parse_when(s: &str) -> Option<DateTime<chrono::FixedOffset>> {
    let s = s.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d);
    }
    // A minute-precision instant (no seconds): `...THH:MMZ` is UTC, `...THH:MM+HH:MM` carries its
    // own offset. RFC 3339 requires the seconds, so these fall through to here.
    if let Ok(nd) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%MZ") {
        return Some(Utc.from_utc_datetime(&nd).fixed_offset());
    }
    if let Ok(d) = DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M%:z") {
        return Some(d);
    }
    // bare date, taken as `T00:00:00Z`.
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

/// The live-reading cache of one source on one plan: `.cache-<source>-<plan>.json`. Keyed by source
/// AND plan so two sources that happen to share a plan slug never read each other's cached body.
fn cache_path(source: &str, plan: &str) -> Option<PathBuf> {
    budget_dir().map(|d| d.join(format!(".cache-{source}-{plan}.json")))
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
                if w.resets_at <= now {
                    s.push_str(&format!(" (seen {days}d ago, window reset since - refresh)"));
                } else if w.stale {
                    s.push_str(&format!(" (seen {days}d ago, stale - refresh)"));
                } else {
                    s.push_str(&format!(" (seen {days}d ago)"));
                }
            } else if w.source == "codex-rollout-8d" || w.source == "codex-recorded" {
                // (0.2.4) the openai fallback window: the account's live window was not in the last
                // two days' rollouts, so the real observation age is shown.
                let days = (now - w.observed_at).num_days().max(0);
                s.push_str(&format!(" (seen {days}d ago)"));
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
