//! Fixture-based tests for the ported formats: the config walk, roster validation
//! (including the D12 `ext` object), recorded health verdicts and the `--short`
//! availability phrasing.

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use serde_json::json;

use c3_core::availability::{
    convert_to_availability_record, format_availability_line, format_relative_hint,
    AvailabilityRecord,
};
use c3_core::config::{provider_endpoint, provider_names, provider_table, scan_config_text};
use c3_core::credential::CredentialResult;
use c3_core::effort::caps;
use c3_core::health::endpoint_health;
use c3_core::lineage::resolve_reviewer_identity;
use c3_core::roster::validate_roster;
use c3_core::verdict::verdict_with_credential;

// --------------------------------------------------------------------------- config walk

const CONFIG: &str = r#"model = "gpt-5.1"
[model_providers.ZAI]
base_url = "https://api.z.ai/api/v1"
env_key = "ZAI_KEY_A"
wire_api = "responses"
[model_providers.broken]
base_url = "https://x.example/v1"
http_headers = { A = "b" }
"#;

#[test]
fn config_walk_names_and_usable_table() {
    let c = scan_config_text("/tmp/config.toml", CONFIG);
    assert!(c.exists && c.ok, "top-level config scans ok");
    let names = provider_names(&c);
    assert_eq!(names, vec!["ZAI".to_string(), "broken".to_string()]);

    let zai = provider_table(&c, "ZAI");
    assert!(zai.found && zai.ok, "ZAI table usable");
    let ep = provider_endpoint(
        &c,
        zai.table_key.as_deref().unwrap(),
        "[model_providers.ZAI]",
        "cfg",
    );
    assert!(ep.error.is_empty());
    assert_eq!(ep.host, "api.z.ai");
    assert_eq!(ep.base_url, "https://api.z.ai/api/v1");
    assert_eq!(ep.wire_api, "responses");
}

#[test]
fn config_walk_broken_inline_table_is_unusable() {
    let c = scan_config_text("/tmp/config.toml", CONFIG);
    let broken = provider_table(&c, "broken");
    assert!(broken.found && !broken.ok, "broken table is unusable");
    assert!(
        broken
            .reason
            .contains("unsupported TOML construct at line 8: http_headers = {"),
        "reason masks the inline-table value: {}",
        broken.reason
    );
    assert!(
        broken.reason.contains("(inline table)"),
        "reason names the construct: {}",
        broken.reason
    );
}

#[test]
fn config_walk_fatal_line_is_not_ok() {
    let c = scan_config_text("/tmp/x.toml", "model = \"gpt-5.1\"\nthis is not toml\n");
    assert!(c.exists && !c.ok, "a fatal line makes the config not ok");
    assert!(
        c.reason.contains("unsupported TOML construct at line 2"),
        "{}",
        c.reason
    );
}

#[test]
fn effort_caps_declared() {
    assert_eq!(caps("api.z.ai").unwrap().vocabulary, "zai");
    assert_eq!(caps("builtin:openai").unwrap().vocabulary, "openai");
    assert_eq!(caps("engine:agy").unwrap().vocabulary, "model-tier");
    assert!(caps("unknown.example").is_none());
}

// --------------------------------------------------------------------------- roster

fn roster_with_ext() -> &'static str {
    r#"{
      "roster_version": 1,
      "ext": { "c3": { "priors": "x" } },
      "reviewers": [
        { "provider": "openai", "model": "gpt-5.1", "ext": { "cost": "hi" } },
        { "provider": "ZAI", "model": "glm-5.3" }
      ]
    }"#
}

#[test]
fn roster_accepts_ext_object_top_level_and_per_entry() {
    let r = validate_roster("/tmp/roster.json", roster_with_ext(), None);
    assert!(
        r.error.is_empty(),
        "ext is accepted and ignored (D12): {}",
        r.error
    );
    assert_eq!(r.entries.len(), 2);
    assert_eq!(r.entries[0].provider, "openai");
    assert_eq!(r.entries[1].model, "glm-5.3");
}

#[test]
fn roster_rejects_unknown_top_level_key() {
    let text = r#"{ "roster_version": 1, "reviewers": [{"provider":"openai"}], "bogus": 1 }"#;
    let r = validate_roster("/tmp/roster.json", text, None);
    assert!(r.error.contains("unknown key 'bogus'"), "{}", r.error);
}

#[test]
fn roster_rejects_unknown_entry_key() {
    let text = r#"{ "roster_version": 1, "reviewers": [{"provider":"openai","weird":true}] }"#;
    let r = validate_roster("/tmp/roster.json", text, None);
    assert!(
        r.error.contains("has an unknown key 'weird'"),
        "{}",
        r.error
    );
}

#[test]
fn roster_rejects_ext_that_is_not_an_object() {
    let text = r#"{ "roster_version": 1, "ext": 3, "reviewers": [{"provider":"openai"}] }"#;
    let r = validate_roster("/tmp/roster.json", text, None);
    assert!(
        r.error.contains(
            "ext must be an object (the extension point of other implementations; got 3)"
        ),
        "{}",
        r.error
    );
}

#[test]
fn roster_rejects_forbidden_delimiters_in_roster_strings() {
    // (wave 26b, D3) `Get-RosterStringProblem`: none of '::', '[', ']', '|', ',', '#' may
    // appear in a provider label, a model or an engine.
    let cases = [
        (
            "provider",
            "a::b",
            "roster entry #1: provider must not contain '::'",
        ),
        (
            "provider",
            "a[b",
            "roster entry #1: provider must not contain '['",
        ),
        (
            "provider",
            "a]b",
            "roster entry #1: provider must not contain ']'",
        ),
        (
            "provider",
            "a|b",
            "roster entry #1: provider must not contain '|'",
        ),
        (
            "provider",
            "a,b",
            "roster entry #1: provider must not contain ','",
        ),
        (
            "provider",
            "a#1",
            "roster entry #1: provider must not contain '#'",
        ),
        (
            "model",
            "glm|5",
            "roster entry #1: model must not contain '|'",
        ),
        (
            "model",
            "glm,5",
            "roster entry #1: model must not contain ','",
        ),
        (
            "model",
            "glm]5",
            "roster entry #1: model must not contain ']'",
        ),
        (
            "engine",
            "co::dex",
            "roster entry #1: engine must not contain '::'",
        ),
    ];
    for (key, value, expected) in cases {
        let text = format!(
            r#"{{ "roster_version": 1, "reviewers": [{{"provider":"openai","model":"m","{key}":{value:?}}}] }}"#
        );
        let r = validate_roster("/tmp/roster.json", &text, None);
        assert!(r.error.contains(expected), "{key}={value}: {}", r.error);
    }
}

#[test]
fn roster_accepts_provider_model_and_engine_with_no_forbidden_characters() {
    // Edge whitespace (leading/trailing blanks) is refused by the provider/model
    // "without surrounding blanks" checks; a clean value is accepted.
    let text = r#"{ "roster_version": 1, "reviewers": [
        {"provider":"gemini","engine":"agy","model":"gemini-3.8-flash-high"}
    ] }"#;
    let r = validate_roster("/tmp/roster.json", text, None);
    assert!(r.error.is_empty(), "{}", r.error);
    assert_eq!(r.entries[0].provider, "gemini");
    assert_eq!(r.entries[0].engine, "agy");

    let blank = r#"{ "roster_version": 1, "reviewers": [{"provider":"ZAI ","model":"glm-5.3"}] }"#;
    let rb = validate_roster("/tmp/roster.json", blank, None);
    assert!(rb.error.contains("surrounding blanks"), "{}", rb.error);
}

#[test]
fn roster_rejects_duplicate_reviewer() {
    let text = r#"{ "roster_version": 1, "reviewers": [{"provider":"ZAI","model":"m"},{"provider":"ZAI","model":"m"}] }"#;
    let r = validate_roster("/tmp/roster.json", text, None);
    assert!(r.error.contains("are the same reviewer"), "{}", r.error);
}

// --------------------------------------------------------------------------- health verdicts

fn iso(s: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(s).unwrap()
}

const FP: &str = "fp-endpoint-1";

fn consult(when: &str, outcome: &str, failure: Option<serde_json::Value>) -> serde_json::Value {
    let mut c = json!({
        "reviewer": { "provider_fingerprint": FP },
        "bridge_outcome": outcome,
        "when": when,
    });
    if let Some(f) = failure {
        c["provider_failure"] = f;
    }
    c
}

#[test]
fn health_usage_limit_with_reset_blocks() {
    let now: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap();
    let consults = vec![consult(
        "2026-09-26T13:30:00+02:00", // 11:30 UTC, 30 min ago
        "provider error",
        Some(json!({
            "class": "quota",
            "code": "429",
            "message": "Individual quota reached.",
            "when": "2026-09-26T13:30:00+02:00",
            "retry_after": "2026-09-26T15:00:00+02:00" // 13:00 UTC, ahead of now
        })),
    )];
    let h = endpoint_health(&consults, FP, now);
    assert!(h.quota.is_some(), "quota with a future reset blocks");
    assert!(h.quota_known, "reset time is known");
    let id = mk_identity();
    let v = verdict_with_credential(&id, Some(&h), CredentialResult::ok("env KEY set"), true);
    assert_eq!(v.state, "unavailable");
    assert_eq!(v.kind, "quota");
    assert_eq!(v.reason, "usage limit until 2026-09-26T15:00:00+02:00");
}

#[test]
fn health_auth_failure_recent_blocks() {
    let now: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap();
    let consults = vec![consult(
        "2026-09-26T13:50:00+02:00", // 11:50 UTC, 10 min ago
        "provider error",
        Some(json!({
            "class": "auth",
            "code": "401",
            "message": "Unauthorized",
            "when": "2026-09-26T13:50:00+02:00"
        })),
    )];
    let h = endpoint_health(&consults, FP, now);
    assert!(h.auth.is_some(), "a recent auth failure blocks");
    let id = mk_identity();
    let v = verdict_with_credential(&id, Some(&h), CredentialResult::ok("env KEY set"), true);
    assert_eq!(v.state, "unavailable");
    assert_eq!(v.kind, "auth");
    assert!(v.reason.starts_with("auth failed "), "{}", v.reason);
}

#[test]
fn health_reset_unknown_quota_blocks_for_60_min_then_clears() {
    // A usage limit that names NO reset time blocks for 60 minutes (wave 24b, F08-7). The
    // message names a usage window, so wave-24c's burst rule does NOT shorten it to 10 min.
    let failure = || {
        Some(json!({
            "class": "quota",
            "code": "429",
            "message": "You've hit your usage limit; try again later.",
            "when": "2026-09-26T13:30:00+02:00" // 11:30 UTC
        }))
    };
    // 30 minutes after the hit: still out, reset unknown.
    let now: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap();
    let consults = vec![consult(
        "2026-09-26T13:30:00+02:00",
        "provider error",
        failure(),
    )];
    let h = endpoint_health(&consults, FP, now);
    assert!(
        h.quota.is_some(),
        "reset-unknown quota within 60 min blocks"
    );
    assert!(!h.quota_known, "no reset time is known");
    let id = mk_identity();
    let v = verdict_with_credential(&id, Some(&h), CredentialResult::ok("env KEY set"), true);
    assert_eq!(v.state, "unavailable");
    assert_eq!(v.kind, "quota-unknown-reset");
    assert!(v.reason.starts_with("usage limit hit "), "{}", v.reason);
    assert!(
        v.reason.contains("reset unknown; retry after "),
        "{}",
        v.reason
    );
    // roster-walk refusal omits the -SkipPreflight tail; a direct run keeps it (F08-7).
    let walk = verdict_with_credential(&id, Some(&h), CredentialResult::ok("env KEY set"), true);
    let direct = verdict_with_credential(&id, Some(&h), CredentialResult::ok("env KEY set"), false);
    assert!(
        !walk
            .refusal
            .contains("pass -SkipPreflight to launch anyway"),
        "{}",
        walk.refusal
    );
    assert!(
        direct
            .refusal
            .contains("pass -SkipPreflight to launch anyway"),
        "{}",
        direct.refusal
    );

    // 90 minutes after the hit: the 60-minute window has passed -> no longer blocking.
    let later: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 26, 13, 0, 0).unwrap();
    let h2 = endpoint_health(&consults, FP, later);
    assert!(
        h2.quota.is_none(),
        "after 60 min the reset-unknown quota clears"
    );
    let v2 = verdict_with_credential(&id, Some(&h2), CredentialResult::ok("env KEY set"), true);
    assert_eq!(v2.state, "available");
    // but it still shows as the last failure (age <= 24 h).
    assert!(
        h2.last_limit.is_some(),
        "the cleared limit still shows as the last failure"
    );
}

#[test]
fn health_burst_429_clears_after_10_minutes() {
    // (wave 24c) A 429 that names no usage window is a BURST: the endpoint is out for 10
    // minutes, not 60. At 5 min it still blocks; at 15 min it has cleared.
    use c3_core::health::failure_kind;
    assert_eq!(
        failure_kind(
            "quota",
            "429 exceeded retry limit, last status: 429 Too Many Requests"
        ),
        "burst"
    );
    assert_eq!(failure_kind("quota", "You've hit your usage limit."), "");
    assert_eq!(failure_kind("auth", "429 Too Many Requests"), "");

    let failure = || {
        Some(json!({
            "class": "quota",
            "code": "429",
            "message": "exceeded retry limit, last status: 429 Too Many Requests",
            "when": "2026-09-26T13:30:00+02:00" // 11:30 UTC
        }))
    };
    let consults = vec![consult(
        "2026-09-26T13:30:00+02:00",
        "provider error",
        failure(),
    )];
    // 5 minutes after the hit: still out.
    let at5: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 26, 11, 35, 0).unwrap();
    let h5 = endpoint_health(&consults, FP, at5);
    assert!(
        h5.quota.is_some(),
        "a burst 429 blocks for its 10-minute window"
    );
    assert_eq!(h5.quota.as_ref().unwrap().kind, "burst");
    // 15 minutes after the hit: the 10-minute burst window has passed.
    let at15: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 26, 11, 45, 0).unwrap();
    let h15 = endpoint_health(&consults, FP, at15);
    assert!(
        h15.quota.is_none(),
        "after 10 min the burst clears (not 60)"
    );
}

#[test]
fn health_usable_outcome_matches_exactly_two_strings() {
    use c3_core::health::is_usable_outcome;
    assert!(is_usable_outcome("usable reply"));
    assert!(is_usable_outcome(
        "usable reply (after a timeout continuation)"
    ));
    assert!(!is_usable_outcome("usable reply (something else)"));
    assert!(!is_usable_outcome("reply"));
    assert!(!is_usable_outcome(""));
}

#[test]
fn roster_positions_lists_every_position_of_a_label() {
    use c3_core::roster::positions_for;
    let text = r#"{ "roster_version": 1, "reviewers": [
        {"provider":"openai","model":"gpt-6"},
        {"provider":"byteplus","model":"a"},
        {"provider":"byteplus","model":"b"},
        {"provider":"byteplus","model":"c"},
        {"provider":"gemini","engine":"agy","model":"g-hi"},
        {"provider":"gemini","engine":"agy","model":"g-pro"}
    ] }"#;
    let r = validate_roster("/tmp/roster.json", text, None);
    assert!(r.error.is_empty(), "{}", r.error);
    assert_eq!(
        positions_for(&r.entries, "byteplus", "codex"),
        vec![2, 3, 4]
    );
    assert_eq!(positions_for(&r.entries, "gemini", "agy"), vec![5, 6]);
    assert_eq!(
        positions_for(&r.entries, "byteplus", "agy"),
        Vec::<i64>::new()
    );
    assert_eq!(positions_for(&r.entries, "openai", "codex"), vec![1]);
}

#[test]
fn health_later_success_clears_earlier_failure() {
    let now: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap();
    let consults = vec![
        consult(
            "2026-09-26T13:30:00+02:00",
            "provider error",
            Some(
                json!({"class":"auth","code":"401","message":"Unauthorized","when":"2026-09-26T13:30:00+02:00"}),
            ),
        ),
        consult("2026-09-26T13:45:00+02:00", "usable reply", None),
    ];
    let h = endpoint_health(&consults, FP, now);
    assert!(
        h.auth.is_none(),
        "a later usable reply clears the auth failure"
    );
}

#[test]
fn health_legacy_codex_exit_outcome_classifies_like_get_endpoint_health() {
    // An entry with NO provider_failure object (the legacy path `Get-EndpointHealth` takes for
    // an older entry): its `bridge_outcome` is classified by `ConvertFrom-ProviderErrorText` +
    // `Get-ProviderFailureClass "$($parsed.Code) $outcome"`. The plugin's main-turn outcome is
    // literally `failed: codex exit N - <detail>` (codex-consult.ps1:3847); a 401 detail must
    // classify as an auth failure and block a later run on the same endpoint.
    let now: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap();
    let consults = vec![consult(
        "2026-09-26T13:50:00+02:00", // 11:50 UTC, 10 min ago
        "failed: codex exit 1 - 401 Unauthorized: invalid API key",
        None, // no provider_failure -> the legacy bridge_outcome parse
    )];
    let h = endpoint_health(&consults, FP, now);
    assert!(
        h.auth.is_some(),
        "a legacy `failed: codex exit 1 - 401 ...` outcome is an auth failure and blocks"
    );
    // A reset-less quota framed the same way is a quota block, not auth.
    let quota = vec![consult(
        "2026-09-26T13:50:00+02:00",
        "failed: codex exit 1 - 429 Too Many Requests: usage limit reached",
        None,
    )];
    let hq = endpoint_health(&quota, FP, now);
    assert!(
        hq.quota.is_some(),
        "a 429/usage-limit exit is a quota block"
    );
}

fn mk_identity() -> c3_core::lineage::ReviewerIdentity {
    // A resolved identity via the built-in openai path.
    let cfg = scan_config_text("", "model = \"gpt-5.1\"\n");
    resolve_reviewer_identity(&cfg, "openai", "gpt-5.1", "", "codex", "")
}

#[test]
fn http_engine_has_a_lineage_row() {
    // F08-7/F03-8: an http reviewer resolves to an identity (not "unknown engine"), its
    // lineage carries [http], and two http providers get distinct fingerprints.
    let cfg = scan_config_text("", "model = \"gpt-5.1\"\n");
    let a = resolve_reviewer_identity(&cfg, "openai", "gpt-6", "", "http", "");
    assert!(a.error.is_empty(), "http resolves: {}", a.error);
    assert!(a.resolved);
    assert_eq!(a.lineage, "openai :: gpt-6");
    // the [http] suffix is added at render time by format_reviewer_lineage (as [agy]/[muse]).
    assert_eq!(
        c3_core::lineage::format_reviewer_lineage("openai", "gpt-6", "http"),
        "openai :: gpt-6 [http]"
    );
    assert!(!a.fingerprint.is_empty());
    let b = resolve_reviewer_identity(&cfg, "zai", "gpt-6", "", "http", "");
    assert_ne!(
        a.fingerprint, b.fingerprint,
        "different http providers are different lineages"
    );
    assert!(c3_core::lineage::engine_spec("http").is_some());
    assert!(c3_core::lineage::ALL_ENGINE_NAMES.contains(&"http"));
    assert!(!c3_core::lineage::ENGINE_NAMES.contains(&"http"));
}

// --------------------------------------------------------------------------- --short phrasing

fn rec(pos: i64, provider: &str, state: &str, short: &str, group: usize) -> AvailabilityRecord {
    AvailabilityRecord {
        position: pos,
        provider: provider.into(),
        model: "m".into(),
        engine: "codex".into(),
        lineage: format!("{provider} :: m"),
        group,
        state: state.into(),
        kind: String::new(),
        reason: String::new(),
        short: short.into(),
        hit: None,
        until: None,
    }
}

#[test]
fn short_line_all_available() {
    let recs = vec![
        rec(1, "openai", "available", "", 0),
        rec(2, "ZAI", "available", "", 1),
    ];
    let line = format_availability_line(&recs, "reviewers", "codex-consult: ", "");
    assert_eq!(line, "codex-consult: all 2 reviewers available");
}

#[test]
fn short_line_one_out() {
    let recs = vec![
        rec(1, "openai", "available", "", 0),
        rec(2, "ZAI", "out", "env ZAI_KEY not set", 1),
        rec(3, "mimo", "available", "", 2),
    ];
    let line = format_availability_line(&recs, "reviewers", "codex-consult: ", "");
    assert_eq!(
        line,
        "codex-consult: out - ZAI :: m (env ZAI_KEY not set); 2 of 3 reviewers available"
    );
}

#[test]
fn short_line_group_collapses() {
    // Two entries of one group, same state and phrase -> "<label> :: *".
    let recs = vec![
        rec(1, "gemini", "out", "until Mon 21:30, in 2d 2h", 0),
        rec(2, "gemini", "out", "until Mon 21:30, in 2d 2h", 0),
        rec(3, "openai", "available", "", 1),
    ];
    let line = format_availability_line(&recs, "reviewers", "codex-consult: ", "");
    assert!(
        line.contains("gemini :: * (until Mon 21:30, in 2d 2h)"),
        "{}",
        line
    );
    assert!(line.ends_with("1 of 3 reviewers available"), "{}", line);
}

#[test]
fn short_line_not_checked_counts() {
    let recs = vec![
        rec(1, "openai", "available", "", 0),
        rec(2, "gemini", "not checked", "sign-in not checked", 1),
    ];
    let line = format_availability_line(&recs, "reviewers", "codex-consult: ", "");
    assert!(
        line.contains("not checked - gemini :: m (sign-in not checked)"),
        "{}",
        line
    );
    assert!(
        line.contains("1 of 2 reviewers available, 0 out, 1 not checked"),
        "{}",
        line
    );
}

#[test]
fn relative_hint_rounding() {
    use chrono::Duration;
    assert_eq!(format_relative_hint(Duration::seconds(-5)), "now");
    assert_eq!(format_relative_hint(Duration::seconds(20)), "in <1m"); // 0.33 min rounds to 0
    assert_eq!(format_relative_hint(Duration::seconds(30)), "in 1m"); // 0.5 min rounds away to 1
    assert_eq!(format_relative_hint(Duration::minutes(52)), "in 52m");
    assert_eq!(format_relative_hint(Duration::minutes(200)), "in 3h 20m");
    assert_eq!(
        format_relative_hint(Duration::hours(2) + Duration::days(2) + Duration::minutes(24)),
        "in 2d 2h"
    );
}

#[test]
fn availability_record_out_quota_short() {
    let now: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap();
    let mut v = c3_core::verdict::PreflightVerdict {
        state: "unavailable".into(),
        preflight: String::new(),
        reason: String::new(),
        refusal: String::new(),
        label: String::new(),
        kind: "quota".into(),
        hit: Some(iso("2026-09-26T13:30:00+02:00")),
        until: Some(iso("2026-09-26T15:00:00+02:00")),
        credential: None,
        burst: false,
        plan_quota: None,
    };
    v.reason = "usage limit until 2026-09-26T15:00:00+02:00".into();
    let r = convert_to_availability_record(1, "ZAI", "glm", "codex", 0, Some(&v), "", now);
    assert_eq!(r.state, "out");
    assert!(r.short.starts_with("until "), "{}", r.short);
    // 15:00+02:00 == 13:00 UTC; now == 12:00 UTC -> one hour out (timezone-independent).
    assert!(r.short.contains("in 1h"), "{}", r.short);
    // (wave 1b, E5) out through its plan: the plan and the route it was recorded on
    v.plan_quota = Some(c3_core::verdict::PlanQuotaRef {
        plan: "zai".into(),
        label: "ZAI".into(),
    });
    let r = convert_to_availability_record(2, "ZAI-b", "glm", "codex", 0, Some(&v), "", now);
    assert!(
        r.short.starts_with("plan zai (usage limit on ZAI until "),
        "{}",
        r.short
    );
    assert!(r.short.ends_with(", in 1h)"), "{}", r.short);
    // (wave 24c) a reset-less burst names itself, through the plan or not
    v.kind = "quota-unknown-reset".into();
    v.burst = true;
    let r = convert_to_availability_record(2, "ZAI-b", "glm", "codex", 0, Some(&v), "", now);
    assert!(
        r.short.starts_with("plan zai (burst limit hit on ZAI "),
        "{}",
        r.short
    );
    v.plan_quota = None;
    let r = convert_to_availability_record(2, "ZAI-b", "glm", "codex", 0, Some(&v), "", now);
    assert!(r.short.starts_with("burst limit hit "), "{}", r.short);
}
