//! Unit tests for the budget module: multiplier math, the kimi `/usages` parse, the snapshot
//! round-trip and the display cells.

use super::*;
use std::sync::Mutex;

/// Serialises the tests that read / write `CODEX_CONSULT_BUDGET_ROUTING` so one test's env does
/// not leak into another's multiplier.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn period_of(name: &str) -> f64 {
    match name {
        "5h" => 5.0 * 3600.0,
        "week" | "wk" => 7.0 * 86_400.0,
        "month" | "mo" => 30.0 * 86_400.0,
        "day" | "daily" => 86_400.0,
        _ => 0.0,
    }
}

fn win(name: &str, used: f64, elapsed: f64, stale: bool) -> BudgetWindow {
    BudgetWindow {
        plan: "p".into(),
        name: name.into(),
        used_ratio: used,
        elapsed_ratio: elapsed,
        period_secs: period_of(name),
        resets_at: utc("2026-11-26T00:00:00Z"),
        source: "test".into(),
        observed_at: utc("2026-11-08T00:00:00Z"),
        stale,
        // The multiplier tests exercise the boost path, so the default helper window is fresh; the
        // age-floor tests build their own non-fresh windows.
        fresh: true,
    }
}

#[test]
fn multiplier_kimi_example_has_headroom() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var("CODEX_CONSULT_BUDGET_ROUTING");
    // Kimi: used .1425, elapsed .4 -> h = .8575 / .6 = 1.4292.
    let (m, gov) = budget_multiplier(&[win("month", 0.1425, 0.4, false)]);
    assert!((m - 1.4292).abs() < 1e-3, "m = {m}");
    assert_eq!(gov.as_deref(), Some("month"));
}

#[test]
fn multiplier_openai_full_5h_pulls_below_one() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var("CODEX_CONSULT_BUDGET_ROUTING");
    // 72% used, ~? elapsed: a nearly-spent 5h window with little time left -> below 1.
    let (m, gov) = budget_multiplier(&[win("5h", 0.72, 0.5, false)]);
    // h = .28 / .5 = .56
    assert!((m - 0.56).abs() < 1e-6, "m = {m}");
    assert_eq!(gov.as_deref(), Some("5h"));
}

#[test]
fn multiplier_clamps_to_bounds() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var("CODEX_CONSULT_BUDGET_ROUTING");
    // huge headroom (0 used, period almost gone) -> clamped to MAX
    let (hi, _) = budget_multiplier(&[win("month", 0.0, 0.99, false)]);
    assert_eq!(hi, ROUTING_BUDGET_MAX);
    // fully spent -> clamped to MIN
    let (lo, _) = budget_multiplier(&[win("month", 1.0, 0.1, false)]);
    assert_eq!(lo, ROUTING_BUDGET_MIN);
}

#[test]
fn multiplier_takes_the_most_constraining_window() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var("CODEX_CONSULT_BUDGET_ROUTING");
    let w = vec![
        win("month", 0.1425, 0.4, false), // h ~ 1.43
        win("5h", 0.9, 0.5, false),       // h = .1 / .5 = .2 -> clamp .25
    ];
    let (m, gov) = budget_multiplier(&w);
    assert_eq!(m, ROUTING_BUDGET_MIN);
    assert_eq!(gov.as_deref(), Some("5h"));
}

#[test]
fn stale_snapshot_does_not_drive_the_multiplier() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var("CODEX_CONSULT_BUDGET_ROUTING");
    let (m, gov) = budget_multiplier(&[win("month", 0.9, 0.1, true)]);
    assert_eq!(m, 1.0);
    assert_eq!(gov, None);
}

#[test]
fn env_off_pins_to_one() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("CODEX_CONSULT_BUDGET_ROUTING", "off");
    let (m, gov) = budget_multiplier(&[win("month", 0.1425, 0.4, false)]);
    std::env::remove_var("CODEX_CONSULT_BUDGET_ROUTING");
    assert_eq!(m, 1.0);
    assert_eq!(gov, None);
}

/// The verified live kimi `/usages` response shape (brief fixture).
const KIMI_FIXTURE: &str = r#"{"limits":[{"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{"limit":"100","remaining":"100","resetTime":"2026-11-08T17:16:53.643321Z"}}],"usages":{"limit_5h":{"used_ratio":0,"reset_time":"2026-11-08T17:16:53Z"},"limit_month_total":{"used_ratio":0.1425,"reset_time":"2026-11-26T00:00:00Z"},"limit_month_code":{"used_ratio":0.1425,"reset_time":"2026-11-26T00:00:00Z"}}}"#;

#[test]
fn kimi_parse_yields_5h_and_month() {
    let now = utc("2026-11-08T12:00:00Z");
    let w = parse_kimi(KIMI_FIXTURE, "kimi", now, now);
    assert_eq!(w.len(), 2);
    let five = &w[0];
    assert_eq!(five.name, "5h");
    assert_eq!(five.used_ratio, 0.0);
    assert_eq!(five.source, "kimi-usages");
    let month = &w[1];
    assert_eq!(month.name, "month");
    assert!((month.used_ratio - 0.1425).abs() < 1e-9);
    // period = Nov 26 minus one calendar month = Oct 26 -> 31 days; remain Nov 26 - Nov 8 12:00
    // = 17.5 d. elapsed = 1 - 17.5/31 = 0.4355
    assert!(
        (month.elapsed_ratio - 0.4355).abs() < 0.005,
        "elapsed = {}",
        month.elapsed_ratio
    );
}

#[test]
fn kimi_bad_body_yields_nothing() {
    let now = utc("2026-11-08T12:00:00Z");
    assert!(parse_kimi("not json", "kimi", now, now).is_empty());
    assert!(parse_kimi("{}", "kimi", now, now).is_empty());
}

/// The verified live z.ai `quota/limit` response shape (nothing secret). The `TIME_LIMIT` item is a
/// monthly MCP-tools allowance (ignored); the `TOKENS_LIMIT` item is the 5-hour model-token window.
const ZAI_FIXTURE: &str = r#"{"code":200,"msg":"Operation successful","data":{"limits":[{"type":"TIME_LIMIT","unit":5,"number":1,"usage":100,"currentValue":2,"remaining":98,"percentage":2,"nextResetTime":1795471745995,"usageDetails":[{"modelCode":"search-prime","usage":1},{"modelCode":"web-reader","usage":1},{"modelCode":"zread","usage":0}]},{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":1,"nextResetTime":1794757032954}],"level":"lite"},"success":true}"#;

#[test]
fn zai_parse_yields_only_the_tokens_5h_window() {
    let resets = epoch_ms(1_794_757_032_954).unwrap();
    let now = resets - chrono::Duration::hours(1);
    let w = parse_zai(ZAI_FIXTURE, "zai", now, now);
    // only TOKENS_LIMIT becomes a window; TIME_LIMIT (the MCP-tools allowance) is ignored.
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].name, "5h");
    assert!((w[0].used_ratio - 0.01).abs() < 1e-9);
    assert_eq!(w[0].source, "zai-quota");
    assert_eq!(w[0].resets_at, resets);
    // period 5h, remain 1h -> elapsed = 1 - 1/5 = 0.8
    assert!(
        (w[0].elapsed_ratio - 0.8).abs() < 1e-6,
        "{}",
        w[0].elapsed_ratio
    );
}

#[test]
fn zai_bad_body_yields_nothing() {
    let now = utc("2026-11-08T12:00:00Z");
    assert!(parse_zai("not json", "zai", now, now).is_empty());
    assert!(parse_zai("{}", "zai", now, now).is_empty());
    // no TOKENS_LIMIT item -> nothing.
    assert!(parse_zai(
        r#"{"data":{"limits":[{"type":"TIME_LIMIT","percentage":2,"nextResetTime":1795471745995}]}}"#,
        "zai",
        now,
        now
    )
    .is_empty());
}

/// The verified live `muse usage --json` shape with a populated allowance.
const MUSE_FIXTURE: &str = r#"{"schemaVersion":1,"allowance":{"tier":"Muse Code Everyday Usage","window":{"usedPercent":0,"resetsAtMs":1794757194000},"weekly":{"usedPercent":8,"resetsAtMs":1794787200000},"asOfMs":1794739221336,"source":"mint"},"reason":null,"hint":null}"#;
/// The `muse usage --json` shape before the first use in a window (no usage data).
const MUSE_NODATA: &str = r#"{"schemaVersion":1,"allowance":null,"reason":"server_sent_no_usage","hint":"the server sent no usage data for this subscription; this can happen before your first use in a new window"}"#;

#[test]
fn muse_parse_yields_5h_and_weekly() {
    let now = epoch_ms(1_794_757_194_000).unwrap() - chrono::Duration::hours(1);
    let w = parse_muse(MUSE_FIXTURE, "meta", now, now);
    assert_eq!(w.len(), 2, "{w:?}");
    assert_eq!(w[0].name, "5h");
    assert_eq!(w[0].used_ratio, 0.0);
    assert_eq!(w[0].source, "muse-usage");
    // period 5h, remain 1h -> elapsed 0.8
    assert!(
        (w[0].elapsed_ratio - 0.8).abs() < 1e-6,
        "{}",
        w[0].elapsed_ratio
    );
    assert_eq!(w[1].name, "week");
    assert!((w[1].used_ratio - 0.08).abs() < 1e-9);
    assert_eq!(w[1].source, "muse-usage");
}

#[test]
fn muse_no_usage_data_yields_nothing() {
    // a null allowance (before the first use in a window) yields no windows, so a snapshot still
    // applies.
    let now = utc("2026-11-08T12:00:00Z");
    assert!(parse_muse(MUSE_NODATA, "meta", now, now).is_empty());
    assert!(parse_muse("not json", "meta", now, now).is_empty());
    assert!(parse_muse("{}", "meta", now, now).is_empty());
}

#[test]
fn snapshot_round_trip() {
    let resets = utc("2026-11-26T00:00:00Z");
    let observed = utc("2026-11-08T00:00:00Z");
    let now = utc("2026-11-09T00:00:00Z");
    let v = snapshot_json("month", 0.42, resets, observed, Some(30));
    let w = window_from_snapshot("kimi", &v, now).unwrap();
    assert_eq!(w.name, "month");
    assert!((w.used_ratio - 0.42).abs() < 1e-9);
    assert_eq!(w.resets_at, resets);
    assert_eq!(w.source, "snapshot");
    assert!(!w.stale);
    // period 30 days, remain Nov 26 - Nov 9 = 17 days -> elapsed = 1 - 17/30 = 0.433
    assert!(
        (w.elapsed_ratio - 0.4333).abs() < 0.01,
        "elapsed = {}",
        w.elapsed_ratio
    );
}

#[test]
fn snapshot_older_than_stale_days_is_stale() {
    let resets = utc("2026-11-26T00:00:00Z");
    let observed = utc("2026-10-20T00:00:00Z"); // 20 days before `now`
    let now = utc("2026-11-09T00:00:00Z");
    let v = snapshot_json("month", 0.42, resets, observed, Some(30));
    let w = window_from_snapshot("kimi", &v, now).unwrap();
    assert!(w.stale);
    // a stale window contributes no multiplier
    assert_eq!(budget_multiplier(&[w]).0, 1.0);
}

#[test]
fn explain_cell_shows_used_over_elapsed() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var("CODEX_CONSULT_BUDGET_ROUTING");
    let w = vec![win("month", 0.14, 0.40, false)];
    let (_m, gov) = budget_multiplier(&w);
    assert_eq!(explain_budget_cell(&w, &gov), "month 14%/40%");
    assert_eq!(explain_budget_cell(&[], &None), "-");
}

#[test]
fn providers_cell_openai_and_snapshot_styles() {
    let now = utc("2026-11-08T12:00:00Z");
    // month snapshot, seen 3 days ago
    let mut w = win("month", 0.14, 0.40, false);
    w.source = "snapshot".into();
    w.observed_at = utc("2026-11-05T12:00:00Z");
    w.resets_at = utc("2026-11-26T00:00:00Z");
    let cell = providers_quota_cell(&[w], now);
    assert!(cell.starts_with("mo 14% until "), "cell = {cell}");
    assert!(cell.contains("(seen 3d ago)"), "cell = {cell}");
}

#[test]
fn effective_multiplier_never_boosts_a_below_neutral_lineage() {
    let neutral = 1.125;
    // below neutral + idle plan -> no boost
    assert_eq!(effective_multiplier(0.95, neutral, 2.0), 1.0);
    // below neutral + hot plan -> the penalty still applies
    assert_eq!(effective_multiplier(0.95, neutral, 0.5), 0.5);
    // at / above neutral -> the plan multiplier as is
    assert_eq!(effective_multiplier(1.125, neutral, 1.45), 1.45);
    assert_eq!(effective_multiplier(1.9, neutral, 0.4), 0.4);
}

#[test]
fn an_idle_plan_at_most_doubles() {
    assert_eq!(ROUTING_BUDGET_MAX, 2.0);
}

#[test]
fn a_snapshot_whose_window_already_reset_is_stale() {
    let now = Utc::now();
    let v = serde_json::json!({
        "window": "week", "used_ratio": 0.03,
        "resets_at": (now - chrono::Duration::hours(2)).to_rfc3339(),
        "observed_at": (now - chrono::Duration::days(1)).to_rfc3339(),
        "period_days": 7
    });
    let w = window_from_snapshot("supergrok", &v, now).expect("window");
    assert!(
        w.stale,
        "a rolled-over snapshot must not drive the multiplier"
    );
    assert_eq!(budget_multiplier(std::slice::from_ref(&w)).0, 1.0);
    assert!(providers_quota_cell(&[w], now).contains("window reset since - refresh"));
}

#[test]
fn openai_fallback_windows_keep_future_and_show_their_age() {
    let now = utc("2026-11-15T12:00:00Z");
    // A recorded quota whose weekly window reset is still ahead and whose 5h window already reset.
    let quota = serde_json::json!({
        "primary":   {"used_percent": 60, "window_minutes": 300,   "resets_at": "2026-11-15T10:00:00Z"},
        "secondary": {"used_percent": 97, "window_minutes": 10080, "resets_at": "2026-11-20T00:00:00Z"},
    });
    let observed = utc("2026-11-13T00:00:00Z"); // 2 days before `now`
    let w = windows_from_quota("", &quota, observed, "codex-recorded", now);
    assert_eq!(w.len(), 2);
    // keep_future drops the 5h window (reset in the past), keeps the weekly one.
    let kept = keep_future(w, now);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].name, "wk");
    assert!((kept[0].used_ratio - 0.97).abs() < 1e-9);
    assert!(kept[0].is_long());
    // The display names the real observation age.
    let cell = providers_quota_cell(&kept, now);
    assert!(cell.contains("(seen 2d ago)"), "cell = {cell}");
    // The fallback window drives the multiplier (not stale) and a 15% reserve holds on it
    // (97% used at ~71% of the week).
    assert!(reserve_hold(&kept, 15, now).is_some());
}

#[test]
fn parse_when_accepts_dates_instants_and_minute_precision() {
    // A bare date is taken as midnight UTC.
    assert_eq!(
        parse_when("2026-11-26").unwrap().with_timezone(&Utc),
        utc("2026-11-26T00:00:00Z")
    );
    // A full RFC 3339 instant (the already-accepted form) still works.
    assert_eq!(
        parse_when("2026-11-16T15:18:00Z")
            .unwrap()
            .with_timezone(&Utc),
        utc("2026-11-16T15:18:00Z")
    );
    // The `--help` example `2026-11-26T00:00Z` (no seconds) is now accepted.
    assert_eq!(
        parse_when("2026-11-26T00:00Z").unwrap().with_timezone(&Utc),
        utc("2026-11-26T00:00:00Z")
    );
    // A minute-precision UTC instant.
    assert_eq!(
        parse_when("2026-11-16T15:18Z").unwrap().with_timezone(&Utc),
        utc("2026-11-16T15:18:00Z")
    );
    // A minute-precision instant with an explicit offset.
    assert_eq!(
        parse_when("2026-11-16T15:18+02:00")
            .unwrap()
            .with_timezone(&Utc),
        utc("2026-11-16T13:18:00Z")
    );
    // Garbage is still rejected.
    assert!(parse_when("not-a-date").is_none());
    assert!(parse_when("2026-11-16T15Z").is_none());
}

// ------------------------------------------------------------------ reserve hold (0.2.4)

/// A long (weekly) window with explicit used / elapsed, reset still ahead of `HOLD_NOW`.
fn long_win(name: &str, used: f64, elapsed: f64) -> BudgetWindow {
    let mut w = win(name, used, elapsed, false);
    w.resets_at = utc("2026-11-26T00:00:00Z"); // ahead of HOLD_NOW below
    w
}

const HOLD_NOW_STR: &str = "2026-11-20T00:00:00Z";
fn hold_now() -> DateTime<Utc> {
    utc(HOLD_NOW_STR)
}

#[test]
fn reserve_hold_corners_for_15_percent() {
    let now = hold_now();
    let n = 15;
    // start of the week: 0% used, 0% elapsed -> min(15, 85) = 15; 0 < 15 -> not held.
    assert!(reserve_hold(&[long_win("week", 0.0, 0.0)], n, now).is_none());
    // mid-week on pace: 50% used at 50% elapsed -> min(65, 85) = 65; 50 < 65 -> not held.
    assert!(reserve_hold(&[long_win("week", 0.50, 0.50)], n, now).is_none());
    // mid-week ahead: 70% used at 50% elapsed -> 70 >= 65 -> HELD.
    assert!(reserve_hold(&[long_win("week", 0.70, 0.50)], n, now).is_some());
    // the last N%: 86% used near the end (90% elapsed) -> min(105, 85) = 85; 86 >= 85 -> HELD.
    assert!(reserve_hold(&[long_win("week", 0.86, 0.90)], n, now).is_some());
    // just under the reserve line at the end: 84% used -> 84 < 85 -> not held.
    assert!(reserve_hold(&[long_win("week", 0.84, 0.90)], n, now).is_none());
}

#[test]
fn reserve_hold_ignores_short_windows_and_missing_data() {
    let now = hold_now();
    // a 5-hour window, nearly spent, is exempt (not long).
    assert!(reserve_hold(&[long_win("5h", 0.99, 0.10)], 15, now).is_none());
    // no data at all -> not held.
    assert!(reserve_hold(&[], 15, now).is_none());
    // reserve 0 is never a hold.
    assert!(reserve_hold(&[long_win("week", 0.99, 0.10)], 0, now).is_none());
}

#[test]
fn reserve_hold_ignores_stale_and_already_reset_windows() {
    let now = hold_now();
    // a stale window is not a live fact -> not held even though it is over the line.
    let mut stale = long_win("week", 0.95, 0.10);
    stale.stale = true;
    assert!(reserve_hold(&[stale], 15, now).is_none());
    // a window whose reset already passed carries no info for the new period -> ignored.
    let mut past = long_win("week", 0.95, 0.10);
    past.resets_at = now - chrono::Duration::hours(2);
    assert!(reserve_hold(&[past], 15, now).is_none());
}

#[test]
fn reserve_hold_note_reads_naturally() {
    let w = long_win("wk", 0.97, 0.55);
    assert_eq!(
        reserve_hold_note(&w, 15, hold_now()),
        "weekly window 97% used at 55% of the week, reserve 15%"
    );
    let mut mo = long_win("mo", 0.80, 0.40);
    mo.name = "mo".into();
    assert_eq!(
        reserve_hold_note(&mo, 20, hold_now()),
        "monthly window 80% used at 40% of the month, reserve 20%"
    );
}

// ------------------------------------------------------------------ observation age (F06-4/F07-1)

/// A long (weekly) window observed `days` ago (not fresh unless `days == 0`), reset still ahead.
fn aged_win(name: &str, used: f64, elapsed: f64, days: i64, now: DateTime<Utc>) -> BudgetWindow {
    let observed = now - chrono::Duration::days(days);
    BudgetWindow {
        plan: "p".into(),
        name: name.into(),
        used_ratio: used,
        elapsed_ratio: elapsed,
        period_secs: period_of(name),
        resets_at: now + chrono::Duration::days(1),
        source: "codex-rollout-8d".into(),
        observed_at: observed,
        stale: false,
        fresh: observed_fresh(observed, now),
    }
}

#[test]
fn an_old_high_reading_still_holds_and_names_its_age() {
    let now = hold_now();
    // A 6-day-old 97% weekly reading, reset ahead: it is NOT stale, so it still holds a 15% reserve
    // (usage can only have grown), and the note names the age because the reading is not fresh.
    let w = aged_win("week", 0.97, 0.20, 6, now);
    assert!(!w.fresh);
    let held = reserve_hold(std::slice::from_ref(&w), 15, now);
    assert!(held.is_some(), "a stale-but-high old reading must hold");
    let note = reserve_hold_note(held.unwrap(), 15, now);
    assert!(note.contains("(seen 6d ago)"), "note = {note}");
}

#[test]
fn an_old_low_reading_neither_holds_nor_boosts() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var("CODEX_CONSULT_BUDGET_ROUTING");
    let now = hold_now();
    // A 6-day-old 20% reading early in the window: not held (20 < min(60+15, 85)=75), and although a
    // fresh reading this idle would boost (h = .8/.4 = 2.0), the OLD reading is a floor - no boost.
    let w = aged_win("week", 0.20, 0.60, 6, now);
    assert!(reserve_hold(std::slice::from_ref(&w), 15, now).is_none());
    let (m, _gov) = budget_multiplier(std::slice::from_ref(&w));
    assert!(m <= 1.0, "an old reading must never boost (got {m})");
    // The same reading seen just now DOES boost.
    let fresh = aged_win("week", 0.20, 0.60, 0, now);
    assert!(fresh.fresh);
    assert!(budget_multiplier(&[fresh]).0 > 1.0);
}

#[test]
fn a_merged_old_weekly_window_keeps_its_own_age_not_the_aggregate() {
    // (R5) A recent primary (5h) observed just now is merged with an OLD weekly (secondary) observed
    // 6 days ago. The aggregate observed_at is "now" (the newest), but each window must keep ITS OWN
    // age: the weekly window is NOT fresh, so an idle old weekly reading cannot raise a routing
    // weight above 1. Before the fix every window inherited the aggregate time and the stale weekly
    // reading looked fresh and boosted.
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var("CODEX_CONSULT_BUDGET_ROUTING");
    let now = utc("2026-11-15T12:00:00Z");
    let quota = serde_json::json!({
        "primary": {"used_percent": 10, "window_minutes": 300, "resets_at": "2026-11-15T16:00:00Z"},
        "secondary": {"used_percent": 20, "window_minutes": 10080, "resets_at": "2026-11-18T00:00:00Z"},
        "observed_at": "2026-11-15T12:00:00Z",
        "primary_observed_at": "2026-11-15T11:59:00Z",
        "secondary_observed_at": "2026-11-09T12:00:00Z"
    });
    let w = windows_from_quota("", &quota, now, "codex-rollout", now);
    let weekly = w
        .iter()
        .find(|x| x.period_secs > 100_000.0)
        .expect("the weekly window is present");
    assert!(
        !weekly.fresh,
        "the old weekly window is not fresh despite a recent primary"
    );
    let (m, _gov) = budget_multiplier(std::slice::from_ref(weekly));
    assert!(m <= 1.0, "an old weekly reading must never boost (got {m})");
    // the 5h primary window DID keep its own recent instant and is fresh.
    let primary = w
        .iter()
        .find(|x| x.period_secs < 100_000.0)
        .expect("the primary window is present");
    assert!(primary.fresh, "the recent primary window is fresh");
}

#[test]
fn a_future_dated_observation_is_ignored() {
    let now = hold_now();
    // Beyond a few minutes of clock skew in the future: not fresh, and not accepted as a reading.
    assert!(!observed_fresh(now + chrono::Duration::minutes(10), now));
    assert!(!observed_not_future(
        now + chrono::Duration::minutes(10),
        now
    ));
    // Within skew and within the hour: fresh and accepted.
    assert!(observed_fresh(now - chrono::Duration::minutes(30), now));
    assert!(observed_not_future(now + chrono::Duration::minutes(2), now));
    // Over an hour old: a valid (floor) reading but not fresh.
    assert!(!observed_fresh(now - chrono::Duration::hours(2), now));
    assert!(observed_not_future(now - chrono::Duration::hours(2), now));
}

#[test]
fn a_two_day_rollout_window_past_its_reset_is_dropped_by_keep_future() {
    // (F06-4 a) windows_from_quota keeps a past-reset window; keep_future drops it, so openai_windows
    // falls through to the next source instead of returning a dead period's figure.
    let now = utc("2026-11-15T12:00:00Z");
    let quota = serde_json::json!({
        "primary": {"used_percent": 80, "window_minutes": 10080, "resets_at": "2026-11-14T00:00:00Z"},
    });
    let w = windows_from_quota(
        "",
        &quota,
        now - chrono::Duration::days(1),
        "codex-rollout",
        now,
    );
    assert_eq!(w.len(), 1, "the window is parsed");
    assert!(
        keep_future(w, now).is_empty(),
        "but dropped once its reset is in the past"
    );
}

#[test]
fn cache_path_is_keyed_by_source_and_plan() {
    let _g = ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join("c3-budget-cachekey-test");
    std::env::set_var("CODEX_HOME", &dir);
    let kimi = cache_path("kimi", "shared").expect("home set");
    let zai = cache_path("zai", "shared").expect("home set");
    let muse = cache_path("muse", "shared").expect("home set");
    // Two sources on ONE plan slug get distinct cache files, so neither reads the other's body.
    assert_ne!(kimi, zai);
    assert_ne!(kimi, muse);
    assert_ne!(zai, muse);
    assert!(kimi.to_string_lossy().contains("kimi"));
    assert!(kimi.to_string_lossy().contains("shared"));
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn a_past_reset_live_window_is_dropped() {
    // A live reading whose window has already reset carries no info for the new period: drop it, so
    // a snapshot or "no data" applies instead of a pre-reset usage figure driving the multiplier.
    // kimi: 5h reset 2026-11-08T17:16:53Z and month reset 2026-11-26; `now` after both -> no windows.
    let now = utc("2026-11-27T00:00:00Z");
    assert!(
        parse_kimi(KIMI_FIXTURE, "kimi", now, now).is_empty(),
        "kimi past reset"
    );
    // zai: TOKENS_LIMIT nextResetTime 1794757032954 ms, now one hour later -> no windows.
    let zai_resets = epoch_ms(1_794_757_032_954).unwrap();
    assert!(
        parse_zai(
            ZAI_FIXTURE,
            "zai",
            zai_resets + chrono::Duration::hours(1),
            zai_resets + chrono::Duration::hours(1)
        )
        .is_empty(),
        "zai past reset"
    );
    // muse: 5h resetsAtMs 1794757194000 past, weekly resetsAtMs 1794787200000 still future.
    let muse_5h = epoch_ms(1_794_757_194_000).unwrap();
    let w = parse_muse(
        MUSE_FIXTURE,
        "meta",
        muse_5h + chrono::Duration::hours(1),
        muse_5h + chrono::Duration::hours(1),
    );
    assert_eq!(
        w.len(),
        1,
        "only the still-open weekly window survives: {w:?}"
    );
    assert_eq!(w[0].name, "week");
}
