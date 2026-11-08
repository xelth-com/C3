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

fn win(name: &str, used: f64, elapsed: f64, stale: bool) -> BudgetWindow {
    BudgetWindow {
        plan: "p".into(),
        name: name.into(),
        used_ratio: used,
        elapsed_ratio: elapsed,
        resets_at: utc("2026-11-26T00:00:00Z"),
        source: "test".into(),
        observed_at: utc("2026-11-08T00:00:00Z"),
        stale,
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
        win("5h", 0.9, 0.5, false),        // h = .1 / .5 = .2 -> clamp .25
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
    let w = parse_kimi(KIMI_FIXTURE, "kimi", now);
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
    assert!(parse_kimi("not json", "kimi", now).is_empty());
    assert!(parse_kimi("{}", "kimi", now).is_empty());
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
    assert!((w.elapsed_ratio - 0.4333).abs() < 0.01, "elapsed = {}", w.elapsed_ratio);
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
    assert!(w.stale, "a rolled-over snapshot must not drive the multiplier");
    assert_eq!(budget_multiplier(&[w.clone()]).0, 1.0);
    assert!(providers_quota_cell(&[w], now).contains("window reset since - refresh"));
}
