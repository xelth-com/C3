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
    records.sort_by(|a, b| b.order.cmp(&a.order).then(b.n.cmp(&a.n)));

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
