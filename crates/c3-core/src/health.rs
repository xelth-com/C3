//! Recorded endpoint health from the task ledgers (`Get-EndpointHealth`) and the
//! reset-time reader (`Get-RetryAfter`), ported from `codex-consult-common.ps1`.
//!
//! Only the *read* path is ported: `codex-providers` reads the ledgers, it never
//! writes a failure, so a wall-clock reset time is read with the offset of the
//! failure's own `when` (`-ReferenceOffset`) and there is no local-timezone / DST
//! logic here. Health is computed per endpoint fingerprint, newest by completion
//! wins, and a later usable reply clears an earlier auth or quota failure.

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Utc};
use regex::Regex;
use serde_json::Value;

use crate::sha256_hex;

/// The fingerprint entries recorded before 0.3.0 count as (built-in openai).
pub fn builtin_openai_fingerprint() -> String {
    sha256_hex(b"cc-provider-v1|builtin:openai")
}

const QUOTA_TEXT: &str =
    r"(?i)usage[ _]limit|quota|rate[ _]limit|resource_exhausted|too many requests";
const CONTEXT_OVERFLOW: &str = r"(?i)supports?\s+only\b.{0,80}?\b(?:context|tokens?)\b|context[ _-]?(?:length|window)|maximum\s+context|context_length_exceeded|prompt\s+is\s+too\s+long|input\s+(?:is\s+)?too\s+long|input\s+token\s+count|exceeds?\s+the\s+maximum\s+number\s+of\s+tokens";

fn quota_re() -> Regex {
    Regex::new(QUOTA_TEXT).unwrap()
}
fn context_overflow_re() -> Regex {
    Regex::new(CONTEXT_OVERFLOW).unwrap()
}

/// `Test-ContextOverflow`.
pub fn is_context_overflow(msg: &str) -> bool {
    !msg.is_empty() && context_overflow_re().is_match(msg) && !quota_re().is_match(msg)
}

/// `Get-ProviderFailureClass`: permission | capability | auth | quota | transport | unknown.
pub fn provider_failure_class(message: &str) -> String {
    // Ordered patterns; capability short-circuits on context overflow, auth on quota text.
    let patterns: [(&str, &str); 5] = [
        (
            "permission",
            r"(?i)no output produced|auto-denied|permission that headless mode",
        ),
        (
            "capability",
            r"(?i)not supported|unsupported|does(?: not|n[’']t) support|do not support|feature_not_supported|json_schema|invalid_argument|invalid model selection|conflicts with --effort",
        ),
        (
            "auth",
            r"(?i)\b40[13]\b|unauthori[sz]ed|forbidden|invalid[ _]api[ _]key|\bauthentication\b|\bauth\b|\bapi key\b|permission_denied|unauthenticated|not signed in|login required|sign in to",
        ),
        (
            "quota",
            r"(?i)usage[ _]limit|quota|rate[ _]limit|\b429\b|insufficient balance|too many requests|credits? exhausted|credit balance|payment required|\b402\b|token plan|plan exhausted|billing|resource_exhausted|rate_limit_exceeded",
        ),
        (
            "transport",
            r"(?i)timeout|timed out|connection|econn|enotfound|\bdns\b|\btls\b|certificate|\b50[234]\b|network|\bunavailable\b|deadline_exceeded",
        ),
    ];
    for (name, pat) in patterns.iter() {
        if *name == "capability" && is_context_overflow(message) {
            return "capability".into();
        }
        if *name == "auth" && quota_re().is_match(message) {
            return "quota".into();
        }
        if Regex::new(pat).unwrap().is_match(message) {
            return (*name).into();
        }
    }
    "unknown".into()
}

/// Minutes an endpoint stays out after a quota failure that named no reset time: a burst
/// 429 recovers fast (`$script:BurstOutMinutes`, wave 24c); a real usage window keeps 60.
pub const BURST_OUT_MINUTES: i64 = 10;
/// Minutes a reset-less usage-limit failure keeps the endpoint out (`$script:QuotaOutMinutes`).
pub const QUOTA_OUT_MINUTES: i64 = 60;

fn burst_text_re() -> Regex {
    Regex::new(r"(?i)\b429\b|too many requests|concurren").unwrap()
}

fn quota_window_re() -> Regex {
    Regex::new(r"(?i)usage[ _]?limit|\bquota|resource_exhausted|insufficient|\bbalance|\bcredits?\b|\bbilling|\bpayment|\b402\b|token[ _]plan|plan exhausted|\b(?:hours?|days?|weeks?|months?)\b|hourly|daily|weekly|monthly|\bwindow|\bresets?\b").unwrap()
}

/// `Get-FailureKind` (wave 24c): `"burst"` for a quota failure whose text names a 429 /
/// too-many-requests / concurrency condition but NO usage window, quota, balance, credits,
/// billing, token plan or reset; `""` for every other failure (and every non-quota class).
pub fn failure_kind(class: &str, text: &str) -> String {
    if class != "quota" || text.is_empty() {
        return String::new();
    }
    if burst_text_re().is_match(text) && !quota_window_re().is_match(text) {
        return "burst".into();
    }
    String::new()
}

/// `Test-UsableOutcome`.
pub fn is_usable_outcome(outcome: &str) -> bool {
    outcome == "usable reply" || outcome == "usable reply (after a timeout continuation)"
}

/// `ConvertFrom-ProviderErrorText`: (code, message) from an SSE/JSON error payload,
/// else the one-lined text.
pub fn convert_from_provider_error_text(text: &str) -> (String, String) {
    let mut message = crate::one_line(text);
    let code = String::new();
    if text.is_empty() {
        return (code, message);
    }
    let mut candidates: Vec<String> = Vec::new();
    let sse = Regex::new(r"(?m)data:\s*(\{.*\})\s*$").unwrap();
    for c in sse.captures_iter(text) {
        candidates.push(c[1].to_string());
    }
    if let Some(at) = text.find("{\"error\"") {
        candidates.push(text[at..].trim().to_string());
    }
    for json in candidates {
        let o: Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let e = &o["error"];
        if e.is_null() {
            continue;
        }
        if let Some(s) = e.as_str() {
            return (String::new(), crate::one_line(s));
        }
        let mut code = e["code"].as_str().unwrap_or("").to_string();
        if code.is_empty() {
            code = e["type"].as_str().unwrap_or("").to_string();
        }
        if let Some(m) = e["message"].as_str() {
            message = crate::one_line(m);
        }
        return (code, message);
    }
    (code, message)
}

const DUR_UNIT: &str = r"(?:weeks?|wks?|w|days?|d|hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)";

fn duration_seconds(n: i64, unit: &str) -> i64 {
    let u = unit.to_lowercase();
    if u.starts_with('w') {
        n * 604800
    } else if u.starts_with('d') {
        n * 86400
    } else if u.starts_with('h') {
        n * 3600
    } else if u.starts_with('m') {
        n * 60
    } else {
        n
    }
}

fn go_duration_seconds(text: &str) -> Option<f64> {
    let re = Regex::new(r"(?i)(?P<n>[0-9]+(?:\.[0-9]+)?)\s*(?P<u>ms|hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)").unwrap();
    let mut total = 0.0;
    let mut any = false;
    for c in re.captures_iter(text) {
        let n: f64 = c["n"].parse().ok()?;
        let u = c["u"].to_lowercase();
        if u == "ms" {
            total += n / 1000.0;
        } else if u.starts_with('h') {
            total += n * 3600.0;
        } else if u.starts_with('m') {
            total += n * 60.0;
        } else {
            total += n;
        }
        any = true;
    }
    if any {
        Some(total)
    } else {
        None
    }
}

fn compact_duration_seconds(text: &str) -> Option<f64> {
    let re = Regex::new(r"(?i)(?P<n>[0-9]+(?:\.[0-9]+)?)(?P<u>w|d|h|ms|m|s)").unwrap();
    let mut total = 0.0;
    let mut any = false;
    for c in re.captures_iter(text) {
        let n: f64 = c["n"].parse().ok()?;
        match c["u"].to_lowercase().as_str() {
            "w" => total += n * 604800.0,
            "d" => total += n * 86400.0,
            "h" => total += n * 3600.0,
            "m" => total += n * 60.0,
            "ms" => total += n / 1000.0,
            _ => total += n,
        }
        any = true;
    }
    if any {
        Some(total)
    } else {
        None
    }
}

/// `Get-RetryAfter` on the read path (`-ReferenceOffset`): the reset time a message
/// names, with the offset of `reference`, or `None`.
pub fn retry_after_ref(
    message: &str,
    reference: DateTime<FixedOffset>,
) -> Option<DateTime<FixedOffset>> {
    if message.is_empty() {
        return None;
    }
    let text = message.replace(['\u{2018}', '\u{2019}'], "'");
    let off = *reference.offset();

    // 1. Codex month-name wall clock.
    let codex = Regex::new(
        r"(?i)(?:try\s+again\s+(?:at|on|after)|resets?\s+(?:at|on)|available\s+(?:again\s+)?(?:at|on|after)|until)\s+(?P<mon>jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\.?\s+(?P<day>[0-9]{1,2})(?:st|nd|rd|th)?,?\s*(?:(?P<year>[0-9]{4}),?\s*)?(?:at\s+)?(?P<hour>[0-9]{1,2}):(?P<min>[0-9]{2})(?::(?P<sec>[0-9]{2}))?(?:\s*(?P<ampm>[ap])\.?\s?m\b\.?)?"
    ).unwrap();
    if let Some(c) = codex.captures(&text) {
        let months = [
            "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
        ];
        let mon3 = &c["mon"][..3].to_lowercase();
        if let Some(month) = months.iter().position(|m| m == mon3).map(|i| i as u32 + 1) {
            let day: u32 = c["day"].parse().unwrap();
            let mut hour: i64 = c["hour"].parse().unwrap();
            let minute: u32 = c["min"].parse().unwrap();
            let second: u32 = c
                .name("sec")
                .map(|m| m.as_str().parse().unwrap())
                .unwrap_or(0);
            let mut ok = true;
            if let Some(ap) = c.name("ampm") {
                if !(1..=12).contains(&hour) {
                    ok = false;
                }
                let pm = ap.as_str().eq_ignore_ascii_case("p");
                if hour == 12 {
                    hour = 0;
                }
                if pm {
                    hour += 12;
                }
            }
            if ok {
                let build = |year: i32| -> Option<DateTime<FixedOffset>> {
                    let nd = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(
                        hour as u32,
                        minute,
                        second,
                    )?;
                    off.from_local_datetime(&nd).single()
                };
                if let Some(y) = c.name("year") {
                    let year: i32 = y.as_str().parse().unwrap();
                    return build(year);
                }
                if let Some(at) = build(reference.year()) {
                    if at < reference - Duration::days(1) {
                        return build(reference.year() + 1);
                    }
                    return Some(at);
                }
            }
        }
    }

    // 2. ISO after try again / retry / reset / until / available.
    let iso = Regex::new(r"(?i)(?:try\s+again|retry|resets?|until|available)[^0-9\r\n]{0,24}?(?P<date>[0-9]{4}-[0-9]{2}-[0-9]{2})[T ](?P<time>[0-9]{2}:[0-9]{2}(?::[0-9]{2}(?:\.[0-9]+)?)?)(?P<tz>Z|[+-][0-9]{2}:?[0-9]{2})?").unwrap();
    if let Some(c) = iso.captures(&text) {
        let stamp = format!("{}T{}", &c["date"], &c["time"]);
        if let Some(tz) = c.name("tz") {
            let mut tzs = tz.as_str().to_string();
            if tzs.len() == 5 && tzs != "Z" && tzs != "z" {
                tzs = format!("{}:{}", &tzs[..3], &tzs[3..]);
            }
            let full = format!("{stamp}{}", tzs.to_uppercase());
            if let Ok(dto) = DateTime::parse_from_rfc3339(&full) {
                return Some(dto.with_timezone(&off));
            }
        } else {
            // wall clock in reference offset
            let secs = if c["time"].len() <= 5 {
                format!("{}:00", &c["time"])
            } else {
                c["time"].to_string()
            };
            let nd = format!("{}T{}", &c["date"], secs);
            if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&nd, "%Y-%m-%dT%H:%M:%S") {
                if let Some(dt) = off.from_local_datetime(&naive).single() {
                    return Some(dt);
                }
            }
        }
    }

    // 3. retry-after: N unit (negative lookahead -> fancy-regex)
    let after = fancy_regex::Regex::new(&format!(
        r"(?i)retry[- ]after[:\s]\s*(?P<n>[0-9]+)(?![0-9:.\-])(?:\s*(?P<u>{DUR_UNIT}))?"
    ))
    .unwrap();
    if let Ok(Some(c)) = after.captures(&text) {
        let n: i64 = c.name("n").unwrap().as_str().parse().unwrap();
        let unit = c.name("u").map(|m| m.as_str()).unwrap_or("s");
        return Some(reference + Duration::seconds(duration_seconds(n, unit)));
    }

    // 4. try again / resets in <parts>
    let in_re = Regex::new(&format!(r"(?i)(?:try\s+again|resets?)\s+in\s+(?P<parts>[0-9]+\s*{DUR_UNIT}(?:(?:\s*,\s*|\s+and\s+|\s+)[0-9]+\s*{DUR_UNIT})*)")).unwrap();
    if let Some(c) = in_re.captures(&text) {
        let part = Regex::new(&format!(r"(?i)(?P<n>[0-9]+)\s*(?P<u>{DUR_UNIT})")).unwrap();
        let mut total = 0i64;
        for p in part.captures_iter(&c["parts"]) {
            total += duration_seconds(p["n"].parse().unwrap(), &p["u"]);
        }
        return Some(reference + Duration::seconds(total));
    }

    // 5. compact duration (negative lookahead -> fancy-regex)
    let compact = fancy_regex::Regex::new(r"(?i)\b(?:try\s+again|resets?|retry|available(?:\s+again)?)\s+in\s+(?P<dur>(?:[0-9]+(?:\.[0-9]+)?(?:w|d|h|ms|m|s))+)(?![A-Za-z0-9])").unwrap();
    if let Ok(Some(c)) = compact.captures(&text) {
        if let Some(secs) = compact_duration_seconds(c.name("dur").unwrap().as_str()) {
            return Some(reference + Duration::seconds(secs.ceil() as i64));
        }
    }

    // 6. Google "retry in ..."
    let retry_in = fancy_regex::Regex::new(r"(?i)\bretry\s+in\s+(?P<dur>(?:[0-9]+(?:\.[0-9]+)?(?:ms|h|m|s))+(?![A-Za-z0-9])|[0-9]+(?:\.[0-9]+)?\s*(?:hours?|hrs?|minutes?|mins?|seconds?|secs?)\b(?:(?:\s*,\s*|\s+and\s+|\s+)[0-9]+(?:\.[0-9]+)?\s*(?:hours?|hrs?|minutes?|mins?|seconds?|secs?)\b)*)").unwrap();
    if let Ok(Some(c)) = retry_in.captures(&text) {
        if let Some(secs) = go_duration_seconds(c.name("dur").unwrap().as_str()) {
            return Some(reference + Duration::seconds(secs.ceil() as i64));
        }
    }

    // 7. gRPC retryDelay
    let retry_delay = fancy_regex::Regex::new(r#"(?i)retryDelay"?\s*[:=]\s*(?:\{\s*"?seconds"?\s*:\s*"?(?P<sec>[0-9]+)|"?(?P<dur>(?:[0-9]+(?:\.[0-9]+)?(?:ms|h|m|s))+)(?![A-Za-z0-9]))"#).unwrap();
    if let Ok(Some(c)) = retry_delay.captures(&text) {
        let secs = if let Some(s) = c.name("sec") {
            Some(s.as_str().parse::<f64>().unwrap())
        } else {
            c.name("dur").and_then(|d| go_duration_seconds(d.as_str()))
        };
        if let Some(s) = secs {
            return Some(reference + Duration::seconds(s.ceil() as i64));
        }
    }

    // 8. rolling window
    let window = Regex::new(r"(?i)\bresets?\s+when\s+the\s+current\s+(?P<n>[0-9]+)[- ](?P<u>minute|hour|day|week)s?\s+window\s+ends").unwrap();
    if let Some(c) = window.captures(&text) {
        return Some(
            reference + Duration::seconds(duration_seconds(c["n"].parse().unwrap(), &c["u"])),
        );
    }

    None
}

/// `Format-OffsetIso`.
pub fn format_offset_iso(v: DateTime<FixedOffset>) -> String {
    v.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// One health record.
#[derive(Debug, Clone)]
pub struct Record {
    pub class: String,
    /// `"burst"` for a reset-less burst 429, else `""` (wave 24c).
    pub kind: String,
    pub code: String,
    pub message: String,
    pub when: String,
    pub age_minutes: i64,
    pub retry_after: Option<DateTime<FixedOffset>>,
    pub retry_after_iso: String,
    pub hit: DateTime<FixedOffset>,
    pub hit_iso: String,
    pub until: DateTime<FixedOffset>,
    // sort keys
    order: DateTime<FixedOffset>,
    n: i64,
    ok: bool,
    age: f64,
}

/// `Get-EndpointHealth`'s result.
#[derive(Debug, Clone, Default)]
pub struct EndpointHealth {
    pub auth: Option<Record>,
    pub quota: Option<Record>,
    pub quota_known: bool,
    pub last_limit: Option<Record>,
    pub last_failure: Option<Record>,
    pub recent_usable: Option<Record>,
}

fn dto(v: &Value) -> Option<DateTime<FixedOffset>> {
    let s = v.as_str()?;
    DateTime::parse_from_rfc3339(s).ok()
}

/// `Get-EndpointHealth`: health of one endpoint fingerprint from all task consults.
pub fn endpoint_health(
    consults: &[Value],
    fingerprint: &str,
    now_utc: DateTime<Utc>,
) -> EndpointHealth {
    let mut h = EndpointHealth::default();
    if fingerprint.is_empty() {
        return h;
    }
    let builtin = builtin_openai_fingerprint();
    let mut records: Vec<Record> = Vec::new();
    for c in consults {
        let rev = &c["reviewer"];
        let fp = if rev.is_null() {
            builtin.clone()
        } else {
            rev["provider_fingerprint"]
                .as_str()
                .unwrap_or("")
                .to_string()
        };
        if fp.is_empty() || fp != fingerprint {
            continue;
        }
        let outcome = c["bridge_outcome"].as_str().unwrap_or("");
        if outcome.is_empty() {
            continue;
        }
        let at = match dto(&c["when"]) {
            Some(a) => a,
            None => continue,
        };
        let at_utc = at.with_timezone(&Utc);
        let age_ms = (now_utc - at_utc).num_milliseconds() as f64;
        let age = (age_ms / 60000.0).max(0.0);
        let mut order = dto(&c["finished_at"]);
        if order.is_none() {
            let mut ord = at;
            if let Some(ws) = c["wall_seconds"]
                .as_f64()
                .or_else(|| c["wall_seconds"].as_str().and_then(|s| s.parse().ok()))
            {
                if ws > 0.0 {
                    ord = at + Duration::milliseconds((ws * 1000.0) as i64);
                }
            }
            order = Some(ord);
        }
        let n = c["n"]
            .as_i64()
            .or_else(|| c["n"].as_str().and_then(|s| s.parse().ok()))
            .unwrap_or(0);
        let ok = is_usable_outcome(outcome);
        let when = at.format("%Y-%m-%dT%H:%M:%S%:z").to_string();
        let mut rec = Record {
            class: String::new(),
            kind: String::new(),
            code: String::new(),
            message: String::new(),
            when,
            age_minutes: age.floor().max(0.0) as i64,
            retry_after: None,
            retry_after_iso: String::new(),
            hit: at,
            hit_iso: String::new(),
            until: at + Duration::minutes(60),
            order: order.unwrap(),
            n,
            ok,
            age,
        };
        if !ok {
            let mut reference = at;
            let pf = &c["provider_failure"];
            if !pf.is_null() {
                let mut class = pf["class"].as_str().unwrap_or("unknown").to_string();
                let pmsg = pf["message"].as_str().unwrap_or("");
                if class == "auth" && quota_re().is_match(pmsg) {
                    class = "quota".into();
                } else if class == "auth" && is_context_overflow(pmsg) {
                    class = "capability".into();
                }
                rec.class = class;
                rec.code = pf["code"].as_str().unwrap_or("").to_string();
                rec.message = pmsg.to_string();
                if let Some(w) = dto(&pf["when"]) {
                    reference = w;
                }
                let recorded = &pf["retry_after"];
                if !recorded.is_null() && !recorded.as_str().unwrap_or("").is_empty() {
                    rec.retry_after = dto(recorded);
                }
            } else {
                let (code, message) = convert_from_provider_error_text(outcome);
                rec.class = provider_failure_class(&format!("{code} {outcome}"));
                rec.code = code;
                rec.message = message;
            }
            // (wave 24c) the failure kind decides the reset-less out-window: a burst 429 is
            // out for 10 minutes, a real usage window for 60.
            rec.kind = failure_kind(&rec.class, &format!("{} {}", rec.code, rec.message));
            let out_minutes = if rec.kind == "burst" {
                BURST_OUT_MINUTES
            } else {
                QUOTA_OUT_MINUTES
            };
            if rec.retry_after.is_none() {
                rec.retry_after = retry_after_ref(&rec.message, reference);
            }
            let mut hit = reference;
            if hit.with_timezone(&Utc) > now_utc {
                hit = now_utc.with_timezone(hit.offset());
            }
            rec.hit = hit;
            rec.hit_iso = format_offset_iso(hit);
            rec.until = hit + Duration::minutes(out_minutes);
            if let Some(ra) = rec.retry_after {
                rec.retry_after_iso = format_offset_iso(ra);
                rec.until = ra;
            }
            let msg_chars: Vec<char> = rec.message.chars().collect();
            if msg_chars.len() > 100 {
                rec.message = msg_chars[..100].iter().collect();
            }
        }
        records.push(rec);
    }
    // wave 26c D2: newest `order` wins, ties broken by higher `n`, then by later `until` so
    // an artificial tie (e.g. two machine-health records minted in the same instant) still
    // resolves deterministically.
    records.sort_by(|a, b| {
        b.order
            .cmp(&a.order)
            .then(b.n.cmp(&a.n))
            .then(b.until.cmp(&a.until))
    });

    let auth = records.iter().find(|r| r.ok || r.class == "auth").cloned();
    if let Some(a) = auth {
        if !a.ok && a.age_minutes <= 24 * 60 {
            h.auth = Some(a);
        }
    }
    let quota = records.iter().find(|r| r.ok || r.class == "quota").cloned();
    if let Some(q) = quota {
        if !q.ok {
            if let Some(ra) = q.retry_after {
                if ra.with_timezone(&Utc) > now_utc {
                    h.quota = Some(q);
                }
            } else if q.until.with_timezone(&Utc) > now_utc {
                h.quota = Some(q);
            }
        }
    }
    h.quota_known = h
        .quota
        .as_ref()
        .map(|q| q.retry_after.is_some())
        .unwrap_or(false);
    h.last_limit = records
        .iter()
        .find(|r| !r.ok && r.class == "quota" && r.age_minutes <= 24 * 60)
        .cloned();
    h.last_failure = records
        .iter()
        .find(|r| !r.ok && r.age_minutes <= 24 * 60)
        .cloned();
    if h.last_limit.is_none() {
        h.last_limit = h.quota.clone();
    }
    if h.last_failure.is_none() {
        h.last_failure = h.quota.clone();
    }
    h.recent_usable = records.iter().find(|r| r.ok && r.age <= 60.0).cloned();
    h
}

// --------------------------------------------------------------------------- machine health
//
// One JSON file per machine (`codex-consult-health.json` under the codex home, or the path
// named by `CODEX_CONSULT_HEALTH`) that repositories share so they see each other's endpoint
// failures and running panel members. Ported from the plugin's machine-wide health file
// (wave 26c). `endpoint_health` above stays ledger-only; `machine_endpoint_consults` folds
// these records in as synthetic consults for a caller that wants the merge.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

const MACHINE_HEALTH_FILE: &str = "codex-consult-health.json";
const MACHINE_HEALTH_MAX_ENDPOINTS: usize = 500;

/// One endpoint record in the machine-wide health file.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct MachineEndpoint {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub retry_after: Option<String>,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub message: String,
}

/// One running-member row in the machine-wide health file.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct MachineRunning {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub start_time: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub nn: String,
    #[serde(default)]
    pub panel: String,
    #[serde(default)]
    pub since: String,
}

/// The parsed machine health file. Not the wire shape directly — `update_machine_health`
/// writes the `{health_version, endpoints, running}` object the file actually holds.
#[derive(Clone, Debug, Default)]
pub struct MachineHealth {
    pub endpoints: Vec<MachineEndpoint>,
    pub running: Vec<MachineRunning>,
}

/// The fields an `Add-MachineHealthRecord` needs from a provider failure.
#[derive(Clone, Debug, Default)]
pub struct MachineFailure {
    pub class: String,
    pub kind: String,
    pub when: String,
    pub retry_after: Option<String>,
    pub message: String,
}

/// `Get-MachineHealthPath`: `CODEX_CONSULT_HEALTH` wins when set and non-blank (the literal
/// `none`, case-insensitively, disables the file), else `<codex_home>/codex-consult-health.json`,
/// else `None` when there is no codex home.
pub fn machine_health_path(codex_home: &str) -> Option<PathBuf> {
    if let Ok(v) = std::env::var("CODEX_CONSULT_HEALTH") {
        let v = v.trim();
        if !v.is_empty() {
            if v.eq_ignore_ascii_case("none") {
                return None;
            }
            return Some(PathBuf::from(v));
        }
    }
    if codex_home.is_empty() {
        return None;
    }
    Some(Path::new(codex_home).join(MACHINE_HEALTH_FILE))
}

/// `Read-MachineHealth`: never errors — a missing, unreadable, or unparseable file reads as
/// empty. Endpoint entries with an empty `endpoint` fingerprint are dropped.
pub fn read_machine_health(path: &Path) -> MachineHealth {
    let mut out = MachineHealth::default();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return out,
    };
    let v: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return out,
    };
    if let Some(arr) = v["endpoints"].as_array() {
        for e in arr {
            if let Ok(rec) = serde_json::from_value::<MachineEndpoint>(e.clone()) {
                if !rec.endpoint.is_empty() {
                    out.endpoints.push(rec);
                }
            }
        }
    }
    if let Some(arr) = v["running"].as_array() {
        for r in arr {
            if let Ok(rec) = serde_json::from_value::<MachineRunning>(r.clone()) {
                out.running.push(rec);
            }
        }
    }
    out
}

fn now_offset() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap())
}

/// `Add-MachineHealthRecord`: builds a `MachineEndpoint` from an outcome/failure and merges
/// it into the machine-wide file. Returns `false` without writing when there is nothing to
/// record: no fingerprint, an operator-class failure, or neither a usable outcome nor a
/// failure.
pub fn add_machine_health_record(
    path: &Path,
    fingerprint: &str,
    outcome: &str,
    failure: Option<&MachineFailure>,
    repo: &str,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> bool {
    if fingerprint.is_empty() {
        return false;
    }
    let record = if is_usable_outcome(outcome) {
        MachineEndpoint {
            endpoint: fingerprint.to_string(),
            class: "ok".into(),
            kind: String::new(),
            until: None,
            retry_after: None,
            repo: repo.to_string(),
            when: format_offset_iso(now_offset()),
            message: String::new(),
        }
    } else if let Some(f) = failure {
        let class = f.class.clone();
        if class.is_empty() || class == "operator" {
            return false;
        }
        let when = DateTime::parse_from_rfc3339(&f.when).unwrap_or_else(|_| now_offset());
        let kind = f.kind.clone();
        let (until, retry_after) = if class == "quota" {
            let ra = f
                .retry_after
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok());
            match ra {
                Some(r) => (Some(r), Some(r)),
                None => {
                    let mins = if kind == "burst" {
                        BURST_OUT_MINUTES
                    } else {
                        QUOTA_OUT_MINUTES
                    };
                    (Some(when + Duration::minutes(mins)), None)
                }
            }
        } else if class == "auth" {
            (Some(when + Duration::hours(24)), None)
        } else {
            (None, None)
        };
        let mut message = f.message.clone();
        let chars: Vec<char> = message.chars().collect();
        if chars.len() > 200 {
            message = chars[..200].iter().collect();
        }
        MachineEndpoint {
            endpoint: fingerprint.to_string(),
            class,
            kind,
            until: until.map(format_offset_iso),
            retry_after: retry_after.map(format_offset_iso),
            repo: repo.to_string(),
            when: format_offset_iso(when),
            message,
        }
    } else {
        return false;
    };
    update_machine_health(path, Some(record), None, 0, is_alive)
}

/// `Register-MachineRunning`: removes any existing row for `row.pid`, then adds it.
pub fn register_machine_running(
    path: &Path,
    row: MachineRunning,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> bool {
    let pid = row.pid;
    update_machine_health(path, None, Some(row), pid, is_alive)
}

/// `Unregister-MachineRunning`: drops any running row for `pid`.
pub fn unregister_machine_running(
    path: &Path,
    pid: u32,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> bool {
    update_machine_health(path, None, None, pid, is_alive)
}

/// `Get-MachineRunningCount`: the live running rows (per `is_alive`) whose endpoint is one of
/// `fingerprints`, excluding `exclude_panel`'s own rows when that panel id is non-empty. Reads
/// the file directly, not under the write lock.
pub fn machine_running_count(
    path: &Path,
    fingerprints: &[String],
    exclude_panel: &str,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> Vec<MachineRunning> {
    read_machine_health(path)
        .running
        .into_iter()
        .filter(|r| fingerprints.iter().any(|f| f == &r.endpoint))
        .filter(|r| !(!exclude_panel.is_empty() && r.panel == exclude_panel))
        .filter(|r| is_alive(r.pid, &r.start_time))
        .collect()
}

/// Acquire the machine-health lock file (`<path>.lock`), retrying with a doubling back-off
/// (25 ms, capped at 500 ms) for up to 10 seconds. `None` when the lock could never be taken.
#[cfg(windows)]
fn acquire_machine_lock(lock_path: &Path) -> Option<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let started = std::time::Instant::now();
    let mut delay = std::time::Duration::from_millis(25);
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(lock_path)
        {
            Ok(f) => return Some(f),
            Err(_) => {
                if started.elapsed() >= std::time::Duration::from_secs(10) {
                    return None;
                }
                std::thread::sleep(delay);
                delay = (delay * 2).min(std::time::Duration::from_millis(500));
            }
        }
    }
}

#[cfg(not(windows))]
fn acquire_machine_lock(lock_path: &Path) -> Option<std::fs::File> {
    let started = std::time::Instant::now();
    let mut delay = std::time::Duration::from_millis(25);
    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(f) => return Some(f),
            Err(_) => {
                if started.elapsed() >= std::time::Duration::from_secs(10) {
                    return None;
                }
                std::thread::sleep(delay);
                delay = (delay * 2).min(std::time::Duration::from_millis(500));
            }
        }
    }
}

/// Read-modify-write the machine health file under the `<path>.lock` exclusive lock: apply
/// `add_endpoint`/`add_running`/`remove_pid`, prune stale endpoints and dead running rows, cap
/// endpoints to the last 500, and write atomically. Never panics; any failure returns `false`.
fn update_machine_health(
    path: &Path,
    add_endpoint: Option<MachineEndpoint>,
    add_running: Option<MachineRunning>,
    remove_pid: u32,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> bool {
    let mut lock_os = path.as_os_str().to_os_string();
    lock_os.push(".lock");
    let lock_path = PathBuf::from(lock_os);
    let _lock = match acquire_machine_lock(&lock_path) {
        Some(f) => f,
        None => return false,
    };

    let result = (|| -> bool {
        let current = read_machine_health(path);
        let mut endpoints = current.endpoints;
        if let Some(e) = add_endpoint {
            endpoints.push(e);
        }
        let mut running = current.running;
        if remove_pid > 0 {
            running.retain(|r| r.pid != remove_pid);
        }
        if let Some(r) = add_running {
            running.push(r);
        }

        let now = Utc::now();
        endpoints.retain(|e| {
            let recent = DateTime::parse_from_rfc3339(&e.when)
                .map(|w| (now - w.with_timezone(&Utc)).num_hours() <= 24)
                .unwrap_or(false);
            let still_out = e
                .until
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|u| u.with_timezone(&Utc) > now)
                .unwrap_or(false);
            recent || still_out
        });
        if endpoints.len() > MACHINE_HEALTH_MAX_ENDPOINTS {
            let drop = endpoints.len() - MACHINE_HEALTH_MAX_ENDPOINTS;
            endpoints.drain(0..drop);
        }
        running.retain(|r| is_alive(r.pid, &r.start_time));

        let out = serde_json::json!({
            "health_version": 1,
            "endpoints": endpoints,
            "running": running,
        });
        let mut text = match serde_json::to_string_pretty(&out) {
            Ok(s) => s,
            Err(_) => return false,
        };
        text.push('\n');
        crate::store::write_text_atomic(path, text.as_bytes()).is_ok()
    })();

    #[cfg(not(windows))]
    {
        let _ = std::fs::remove_file(&lock_path);
    }

    result
}

/// `Get-MachineEndpointConsults`: synthesizes a "consult" `Value` per machine-health record
/// for `fingerprint`, in the shape `endpoint_health` already reads, so a caller can fold the
/// machine file's records in alongside the ledger's own consults.
pub fn machine_endpoint_consults(path: &Path, fingerprint: &str) -> Vec<Value> {
    machine_consults_filtered(path, Some(fingerprint))
}

/// Every endpoint record of the machine file as a synthetic consult (all fingerprints), for
/// folding into an [`endpoint_health`] computation across a repository's own ledgers.
pub fn machine_endpoint_consults_all(path: &Path) -> Vec<Value> {
    machine_consults_filtered(path, None)
}

fn machine_consults_filtered(path: &Path, fingerprint: Option<&str>) -> Vec<Value> {
    read_machine_health(path)
        .endpoints
        .into_iter()
        .filter(|e| fingerprint.is_none_or(|fp| e.endpoint == fp))
        .map(|e| {
            let mut v = serde_json::json!({
                "when": e.when,
                "n": 0,
                "finished_at": e.when,
                "reviewer": { "provider_fingerprint": e.endpoint },
            });
            if e.class == "ok" {
                v["bridge_outcome"] = Value::String("usable reply".into());
            } else {
                let msg = if !e.message.is_empty() {
                    e.message.clone()
                } else {
                    e.class.clone()
                };
                v["bridge_outcome"] = Value::String(format!("failed: {msg}"));
                v["provider_failure"] = serde_json::json!({
                    "class": e.class,
                    "kind": e.kind,
                    "message": e.message,
                    "when": e.when,
                    "retry_after": e.retry_after,
                });
            }
            v
        })
        .collect()
}

#[cfg(test)]
mod machine_health_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "c3-health-test-{}-{nanos:x}-{n:x}-{name}.json",
            std::process::id()
        ))
    }

    fn alive_true(_pid: u32, _start: &str) -> bool {
        true
    }
    fn alive_false(_pid: u32, _start: &str) -> bool {
        false
    }

    #[test]
    fn machine_health_path_env_none_disables() {
        std::env::set_var("CODEX_CONSULT_HEALTH", "none");
        assert_eq!(machine_health_path("C:/codex-home"), None);
        std::env::set_var("CODEX_CONSULT_HEALTH", "  NoNe  ");
        assert_eq!(machine_health_path("C:/codex-home"), None);
        std::env::remove_var("CODEX_CONSULT_HEALTH");
    }

    #[test]
    fn machine_health_path_env_overrides() {
        std::env::set_var("CODEX_CONSULT_HEALTH", "C:/somewhere/health.json");
        assert_eq!(
            machine_health_path("C:/codex-home"),
            Some(PathBuf::from("C:/somewhere/health.json"))
        );
        std::env::remove_var("CODEX_CONSULT_HEALTH");
    }

    #[test]
    fn machine_health_path_default_from_codex_home() {
        std::env::remove_var("CODEX_CONSULT_HEALTH");
        assert_eq!(
            machine_health_path("C:/codex-home"),
            Some(PathBuf::from("C:/codex-home").join("codex-consult-health.json"))
        );
        assert_eq!(machine_health_path(""), None);
    }

    #[test]
    fn add_record_quota_failure_no_retry_after_uses_out_window() {
        let path = temp_path("quota");
        let failure = MachineFailure {
            class: "quota".into(),
            kind: String::new(),
            when: format_offset_iso(now_offset()),
            retry_after: None,
            message: "usage limit reached".into(),
        };
        let ok = add_machine_health_record(
            &path,
            "fp1",
            "failed: usage limit",
            Some(&failure),
            "repo-a",
            &alive_true,
        );
        assert!(ok);
        let health = read_machine_health(&path);
        assert_eq!(health.endpoints.len(), 1);
        let rec = &health.endpoints[0];
        assert_eq!(rec.class, "quota");
        assert!(rec.retry_after.is_none());
        assert!(!rec.message.is_empty());
        let when = DateTime::parse_from_rfc3339(&rec.when).unwrap();
        let until = DateTime::parse_from_rfc3339(rec.until.as_deref().unwrap()).unwrap();
        assert_eq!((until - when).num_minutes(), QUOTA_OUT_MINUTES);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn add_record_usable_outcome_is_ok_class() {
        let path = temp_path("ok");
        let ok =
            add_machine_health_record(&path, "fp2", "usable reply", None, "repo-a", &alive_true);
        assert!(ok);
        let health = read_machine_health(&path);
        assert_eq!(health.endpoints.len(), 1);
        assert_eq!(health.endpoints[0].class, "ok");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn merge_newer_ok_clears_older_quota() {
        let older = now_offset() - Duration::minutes(30);
        let newer = now_offset();
        let ledger_consult = serde_json::json!({
            "when": format_offset_iso(older),
            "finished_at": format_offset_iso(older),
            "n": 1,
            "reviewer": { "provider_fingerprint": "fp3" },
            "bridge_outcome": "failed: usage limit",
            "provider_failure": {
                "class": "quota",
                "kind": "",
                "message": "usage limit reached",
                "when": format_offset_iso(older),
                "retry_after": null,
            },
        });
        let path = temp_path("merge-ok");
        add_machine_health_record(&path, "fp3", "usable reply", None, "repo-a", &alive_true);
        // stamp the ok record's `when` to be newer than the ledger consult
        {
            let mut h = read_machine_health(&path);
            h.endpoints[0].when = format_offset_iso(newer);
            let out = serde_json::json!({
                "health_version": 1,
                "endpoints": h.endpoints,
                "running": h.running,
            });
            std::fs::write(&path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
        }
        let machine_consults = machine_endpoint_consults(&path, "fp3");
        let mut consults = vec![ledger_consult];
        consults.extend(machine_consults);
        let health = endpoint_health(&consults, "fp3", Utc::now());
        assert!(health.quota.is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn merge_newer_quota_keeps_quota() {
        let older = now_offset() - Duration::minutes(30);
        let newer = now_offset();
        let ledger_consult = serde_json::json!({
            "when": format_offset_iso(older),
            "finished_at": format_offset_iso(older),
            "n": 1,
            "reviewer": { "provider_fingerprint": "fp4" },
            "bridge_outcome": "usable reply",
        });
        let path = temp_path("merge-quota");
        let failure = MachineFailure {
            class: "quota".into(),
            kind: String::new(),
            when: format_offset_iso(newer),
            retry_after: None,
            message: "usage limit reached".into(),
        };
        add_machine_health_record(
            &path,
            "fp4",
            "failed: usage limit",
            Some(&failure),
            "repo-a",
            &alive_true,
        );
        let machine_consults = machine_endpoint_consults(&path, "fp4");
        let mut consults = vec![ledger_consult];
        consults.extend(machine_consults);
        let health = endpoint_health(&consults, "fp4", Utc::now());
        assert!(health.quota.is_some());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn prune_drops_old_expired_keeps_old_still_out_drops_dead_running() {
        let path = temp_path("prune");
        let old_when = now_offset() - Duration::hours(48);
        let past_until = now_offset() - Duration::hours(1);
        let future_until = now_offset() + Duration::hours(1);
        let expired = MachineEndpoint {
            endpoint: "fp-expired".into(),
            class: "quota".into(),
            kind: String::new(),
            until: Some(format_offset_iso(past_until)),
            retry_after: None,
            repo: "repo-a".into(),
            when: format_offset_iso(old_when),
            message: String::new(),
        };
        let still_out = MachineEndpoint {
            endpoint: "fp-still-out".into(),
            class: "auth".into(),
            kind: String::new(),
            until: Some(format_offset_iso(future_until)),
            retry_after: None,
            repo: "repo-a".into(),
            when: format_offset_iso(old_when),
            message: String::new(),
        };
        let running_row = MachineRunning {
            endpoint: "fp-run".into(),
            label: "label".into(),
            pid: 4242,
            start_time: format_offset_iso(now_offset()),
            repo: "repo-a".into(),
            task: "task".into(),
            nn: "01".into(),
            panel: String::new(),
            since: format_offset_iso(now_offset()),
        };
        // seed the file directly, then run one update via unregister with a no-op pid to
        // trigger the prune pass.
        let seed = serde_json::json!({
            "health_version": 1,
            "endpoints": [expired, still_out],
            "running": [running_row],
        });
        std::fs::write(&path, serde_json::to_string_pretty(&seed).unwrap()).unwrap();
        let ok = unregister_machine_running(&path, 0, &alive_false);
        assert!(ok);
        let health = read_machine_health(&path);
        let endpoints: Vec<&str> = health
            .endpoints
            .iter()
            .map(|e| e.endpoint.as_str())
            .collect();
        assert!(!endpoints.contains(&"fp-expired"));
        assert!(endpoints.contains(&"fp-still-out"));
        assert!(health.running.is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
