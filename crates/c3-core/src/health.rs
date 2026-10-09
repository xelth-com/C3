//! Recorded endpoint health from the task ledgers (`Get-EndpointHealth`) and the
//! reset-time reader (`Get-RetryAfter`), ported from `codex-consult-common.ps1`.
//!
//! The reader has both of the plugin's modes: at READ time ([`retry_after_ref`], what
//! `codex-providers` and the health view use) a wall-clock reset time is read with the
//! offset of the failure's own `when` (`-ReferenceOffset`); at WRITE time
//! ([`retry_after_in`], a consultation recording its provider failure) it is read with the
//! rules of a zone ([`ResetZone`]: `chrono::Local` there, daylight saving included), and a
//! time without a date (0.6.1) is placed on the day before, of or after the reference.
//! Health is computed per endpoint fingerprint, newest by completion wins, and a later
//! usable reply clears an earlier auth or quota failure.
//!
//! (C3 wave 1b, the plugin's 0.6.0 wave 29b E5/E15/E16) [`endpoint_health_set`] reads the
//! records of SEVERAL fingerprints as one record set (the routes of one coding plan), every
//! record carrying its fingerprint; a usable reply whose ledger entry carries
//! `engine_run.quota_mark` also counts as that quota failure 1 ms after it; the machine-wide
//! file keeps an endpoint record's `quota_mark` and a running row's `plan`, and
//! [`machine_running_count`] counts the rows of a set of plans too.

use chrono::{
    DateTime, Datelike, Duration, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, NaiveTime,
    Offset, TimeZone, Timelike, Utc,
};
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

/// The zone a wall-clock reset time is read in (`Get-RetryAfter -TimeZone`, F15-1): the offsets
/// a wall-clock time can have there (one; both in the repeated hour of a fall-back night, the
/// earlier instant first; none inside a spring-forward gap) and its offset at an instant. Every
/// chrono [`TimeZone`] is one: `chrono::Local` at write time (the machine that saw the failure),
/// a zone database's zone in the tests.
pub trait ResetZone {
    /// The offsets the zone itself reports for `wall` - its own classification, which is NOT
    /// trusted at the exact edges of a transition (see [`ResetZone::wall_offsets`]).
    fn reported_wall_offsets(&self, wall: NaiveDateTime) -> LocalResult<FixedOffset>;
    /// This zone's offset at the UTC instant `utc`.
    fn utc_offset(&self, utc: NaiveDateTime) -> FixedOffset;

    /// (F04-2) The offsets `wall` GENUINELY has in this zone, normalised against a UTC-to-local
    /// round trip: an offset `o` is genuine only when the zone's offset at the instant `wall - o`
    /// is `o` again (local(utc(wall at o)) == wall). The candidates are the reported offsets and
    /// the zone's offsets a day before and a day after (both sides of any transition near
    /// `wall`). Two genuine offsets: the repeated hour (`Ambiguous`, the earlier instant first);
    /// one: `Single`; none: a spring-forward gap (`None`). This corrects chrono's Windows
    /// `Local`, whose boundary classification counts the exact END of a repeated hour (03:00 on
    /// the Berlin fall-back day) as ambiguous and the exact START of a gap (02:00 on the
    /// spring-forward day) as a single wall time at the earlier offset - .NET (the plugin) and the
    /// zone database say: 03:00 is +01:00 only, 02:00 does not exist.
    ///
    /// (F06-1) The UTC lookup is the authority only when the round-trip instant `wall - o` falls in
    /// the SAME calendar year as `wall`. A Windows zone keeps one rule set per year (dynamic DST),
    /// and its UTC lookup picks the rules by the UTC year (chrono's `Local`, as
    /// `SystemTimeToTzSpecificLocalTime`) while the wall time belongs to its local year (.NET's
    /// `TimeZoneInfo` and the zone's own classification pick the rules by the local date). Within
    /// the hours around New Year where the two years differ, a zone whose rules change between the
    /// years (Windows' Samoa Standard Time: 2011 bias 660, 2012 bias -780) would lose a genuine
    /// offset: 2012-01-01 00:00 is +14:00, but its instant 2011-12-31T10:00Z is looked up with the
    /// 2011 rules (-10:00). There the zone's own (local-year) classification decides. The DST
    /// edges F04-2 corrects lie far from New Year and keep the round trip.
    fn wall_offsets(&self, wall: NaiveDateTime) -> LocalResult<FixedOffset> {
        let reported: Vec<FixedOffset> = match self.reported_wall_offsets(wall) {
            LocalResult::Single(o) => vec![o],
            LocalResult::Ambiguous(a, b) => vec![a, b],
            LocalResult::None => Vec::new(),
        };
        let mut cands = reported.clone();
        let probes = [
            wall.checked_sub_signed(Duration::days(1)),
            wall.checked_add_signed(Duration::days(1)),
        ];
        cands.extend(probes.into_iter().flatten().map(|p| self.utc_offset(p)));
        let mut genuine: Vec<FixedOffset> = Vec::new();
        for o in cands {
            let round_trip = wall.checked_sub_offset(o).is_some_and(|utc| {
                if utc.year() == wall.year() {
                    self.utc_offset(utc) == o
                } else {
                    // (F06-1) another year's rules would answer: the local year's decide
                    reported.contains(&o)
                }
            });
            if round_trip && !genuine.contains(&o) {
                genuine.push(o);
            }
        }
        // the earlier instant first: the larger offset
        genuine.sort_by_key(|o| std::cmp::Reverse(o.local_minus_utc()));
        match genuine.as_slice() {
            [] => LocalResult::None,
            [o] => LocalResult::Single(*o),
            [a, b, ..] => LocalResult::Ambiguous(*a, *b),
        }
    }
}

impl<T: TimeZone> ResetZone for T {
    fn reported_wall_offsets(&self, wall: NaiveDateTime) -> LocalResult<FixedOffset> {
        self.offset_from_local_datetime(&wall).map(|o| o.fix())
    }
    fn utc_offset(&self, utc: NaiveDateTime) -> FixedOffset {
        self.offset_from_utc_datetime(&utc).fix()
    }
}

/// A wall-clock time at a fixed offset (`None` only out of chrono's range).
fn at_offset(wall: NaiveDateTime, off: FixedOffset) -> Option<DateTime<FixedOffset>> {
    off.from_local_datetime(&wall).single()
}

/// The first offset a wall-clock time has in `zone` (the earlier instant's when ambiguous).
fn first_wall_offset(zone: &dyn ResetZone, wall: NaiveDateTime) -> Option<FixedOffset> {
    match zone.wall_offsets(wall) {
        LocalResult::Single(o) => Some(o),
        LocalResult::Ambiguous(a, _) => Some(a),
        LocalResult::None => None,
    }
}

/// `ConvertFrom-WallClock`: a wall-clock time -> an instant, at the offset `zone` has at that time
/// (a nonexistent time: the offset after the transition; an ambiguous one: the offset before
/// it), or without a zone (`-ReferenceOffset`) at the reference's offset.
fn from_wall_clock(
    wall: NaiveDateTime,
    reference: DateTime<FixedOffset>,
    zone: Option<&dyn ResetZone>,
) -> Option<DateTime<FixedOffset>> {
    let Some(z) = zone else {
        return at_offset(wall, *reference.offset());
    };
    let off = match z.wall_offsets(wall) {
        LocalResult::Single(o) => o,
        LocalResult::None => first_wall_offset(z, wall + Duration::days(1))?,
        LocalResult::Ambiguous(_, _) => first_wall_offset(z, wall - Duration::days(1))?,
    };
    at_offset(wall, off)
}

/// `ConvertTo-ZoneTime`: an instant shown in `zone` (without one, in the reference's offset).
fn to_zone_time(
    at: DateTime<FixedOffset>,
    reference: DateTime<FixedOffset>,
    zone: Option<&dyn ResetZone>,
) -> DateTime<FixedOffset> {
    match zone {
        Some(z) => at.with_timezone(&z.utc_offset(at.naive_utc())),
        None => at.with_timezone(reference.offset()),
    }
}

/// (0.6.1, F06-3) How late a time-only reset may be parsed and still be TODAY's (passed): the
/// message shows whole minutes, and its delivery and parsing take time.
pub const TIME_ONLY_LATE_MINUTES: i64 = 5;

/// (0.6.1, F06-4) `Get-WallClockCandidates`: every instant a wall-clock time can mean in `zone`
/// (without one: the reference's offset - one instant): an ambiguous time (the repeated hour of a
/// fall-back night) BOTH instants, earliest first; a time inside a spring-forward gap the FIRST
/// valid instant after the gap (its first valid minute, at the offset after the transition);
/// else the one.
fn wall_clock_candidates(
    wall: NaiveDateTime,
    reference: DateTime<FixedOffset>,
    zone: Option<&dyn ResetZone>,
) -> Vec<DateTime<FixedOffset>> {
    let Some(z) = zone else {
        return at_offset(wall, *reference.offset()).into_iter().collect();
    };
    match z.wall_offsets(wall) {
        LocalResult::Single(o) => at_offset(wall, o).into_iter().collect(),
        LocalResult::Ambiguous(a, b) => {
            let mut v: Vec<DateTime<FixedOffset>> =
                [a, b].iter().filter_map(|o| at_offset(wall, *o)).collect();
            v.sort();
            v
        }
        LocalResult::None => {
            let mut c = wall
                .with_second(0)
                .and_then(|w| w.with_nanosecond(0))
                .unwrap_or(wall);
            let mut i = 0;
            while i < 1440 && matches!(z.wall_offsets(c), LocalResult::None) {
                c += Duration::minutes(1);
                i += 1;
            }
            first_wall_offset(z, c)
                .and_then(|o| at_offset(c, o))
                .into_iter()
                .collect()
        }
    }
}

/// (0.6.1, F06-3, F06-4) `Select-TimeOnlyReset`: the instant of a time-only reset (`tod`): of
/// the candidates on the day before, the day of and the day after the reference's date - at
/// `fixed` when the message named an offset ([`time_only_zone`]), else in the zone
/// ([`wall_clock_candidates`]; without a zone the reference's offset) - the EARLIEST that is not
/// more than [`TIME_ONLY_LATE_MINUTES`] before the reference. So: today, unless today's is more
/// than 5 minutes past (then tomorrow); a reset that has just passed stays passed (the hold ends
/// at once); in the repeated hour the second instant when the first is past; just after midnight
/// yesterday's when it passed within the 5 minutes.
fn select_time_only_reset(
    tod: NaiveTime,
    reference: DateTime<FixedOffset>,
    zone: Option<&dyn ResetZone>,
    fixed: Option<FixedOffset>,
) -> Option<DateTime<FixedOffset>> {
    let floor = reference - Duration::minutes(TIME_ONLY_LATE_MINUTES);
    let day = match fixed {
        Some(f) => reference.with_timezone(&f).date_naive(),
        None => to_zone_time(reference, reference, zone).date_naive(),
    };
    let mut best: Option<DateTime<FixedOffset>> = None;
    for d in [-1i64, 0, 1] {
        let wall = (day + Duration::days(d)).and_time(tod);
        let cands: Vec<DateTime<FixedOffset>> = match fixed {
            Some(f) => at_offset(wall, f).into_iter().collect(),
            None => wall_clock_candidates(wall, reference, zone),
        };
        for c in cands {
            if c < floor {
                continue;
            }
            if best.map(|b| c < b).unwrap_or(true) {
                best = Some(c);
            }
        }
    }
    best
}

/// What [`time_only_zone`] read after a time-only clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct TimeOnlyZone {
    /// The named offset (UTC/GMT/Z: zero), `None` when no qualifier follows.
    offset: Option<FixedOffset>,
    /// A zone word the bridge does not read, or a malformed qualifier: the wording is not parsed.
    declined: bool,
}

/// (0.6.1, F08-2) A character of a time-only zone qualifier token: anything but a space, a comma,
/// a semicolon, a bracket, a quote, `!`, `?`, `|` or a backslash.
const TIME_ONLY_TOKEN_CHAR: &str = r#"[^\s,;()\[\]{}<>!?"'\x{201C}\x{201D}|\\]"#;

fn time_only_re() -> &'static fancy_regex::Regex {
    static RE: std::sync::OnceLock<fancy_regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        fancy_regex::Regex::new(
            r"(?i)(?:try\s+again\s+(?:at|after)|resets?\s+at|available\s+(?:again\s+)?(?:at|after)|until)\s+(?P<hour>[0-9]{1,2}):(?P<min>[0-9]{2})(?::(?P<sec>[0-9]{2}))?(?![0-9:])(?:\s*(?P<ampm>[ap])\.?\s?m\b\.?)?",
        )
        .unwrap()
    })
}

fn time_only_zone_token_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"^[ \t]*\(?[ \t]*(?P<tok>{TIME_ONLY_TOKEN_CHAR}*)"
        ))
        .unwrap()
    })
}

fn time_only_zone_next_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"^[ \t]+(?P<tok>[+-]{TIME_ONLY_TOKEN_CHAR}*)")).unwrap())
}

fn time_only_other_zone_re() -> &'static fancy_regex::Regex {
    static RE: std::sync::OnceLock<fancy_regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        fancy_regex::Regex::new(r"^[ \t]*\(?[ \t]*(?P<w>[A-Z]{2,5})(?![A-Za-z0-9])").unwrap()
    })
}

/// The zone word a qualifier token starts with (`TimeOnlyZoneWord`): UTC / GMT in any case, or
/// `Z` when it does not start a lower-case word; `""` when none.
fn time_only_zone_word(tok: &str) -> &str {
    if let Some(head) = tok.get(..3) {
        if head.eq_ignore_ascii_case("utc") || head.eq_ignore_ascii_case("gmt") {
            return head;
        }
    }
    if let Some(rest) = tok.strip_prefix('Z') {
        if !rest.starts_with(|c: char| c.is_ascii_lowercase()) {
            return &tok[..1];
        }
    }
    ""
}

/// `TimeOnlyOffset`: a COMPLETE numeric offset - `+H`, `+HH`, `+H:MM`, `+HH:MM` or `+HHMM`
/// (`-` likewise) - as (negative, hours, minutes), `None` for anything else.
fn time_only_offset(text: &str) -> Option<(bool, u32, u32)> {
    let negative = match text.chars().next()? {
        '+' => false,
        '-' => true,
        _ => return None,
    };
    let body = &text[1..];
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let (h, m) = match body.split_once(':') {
        Some((h, m)) if (1..=2).contains(&h.len()) && digits(h) && m.len() == 2 && digits(m) => {
            (h, m)
        }
        Some(_) => return None,
        None if (1..=2).contains(&body.len()) && digits(body) => (body, "0"),
        None if body.len() == 4 && digits(body) => (&body[..2], &body[2..]),
        None => return None,
    };
    Some((negative, h.parse().ok()?, m.parse().ok()?))
}

/// Whether a text starts with spaces and then a digit (a number after a lone sign).
fn number_follows(s: &str) -> bool {
    let t = s.trim_start_matches([' ', '\t']);
    t.len() < s.len() && t.starts_with(|c: char| c.is_ascii_digit())
}

/// (0.6.1, F06-5; F08-2) `Get-TimeOnlyZone`: the zone qualifier right after a time-only clock
/// (`clock`: the matched wording, `rest`: the text after it; spaces or an opening parenthesis
/// between - after a period that ends the sentence, "9:43 PM.", nothing more is read; the period
/// of "p.m." is the abbreviation's). The qualifier is read as a WHOLE token (up to a space, a
/// comma, a semicolon, a bracket, a quote, `!` or `?`; a sentence-ending period stripped) and must
/// be complete: UTC, GMT (any case) or Z alone -> offset 00:00; UTC / GMT followed by a numeric
/// offset (UTC+2, UTC+05:30, also UTC +02:00) or a bare one (+02:00, +0200, +02, +2, -05:30) ->
/// that offset. A token that STARTS a qualifier (UTC, GMT, Z, a sign) but is not one of these - a
/// dangling sign (UTC+), an incomplete minute (UTC+05:3), excess digits (+02:000), letters after
/// the sign (UTC+oops), Z with an offset, beyond 14 hours or minutes of 60 and more (+15:00,
/// +02:60) - DECLINES the wording: never a shorter prefix of it. A lone sign between words
/// (`21:43 - or upgrade`) is a dash, unless a number follows it (`21:43 + 2`: declined). Another
/// zone-like word - two to five capital letters (PST, CET, CEST, BST) other than AND / OR - ->
/// declined: the bridge does not guess what an abbreviation means. A comma, "and", a period or
/// the end stays fine (no qualifier: the zone's local time).
fn time_only_zone(clock: &str, rest: &str) -> TimeOnlyZone {
    let mut r = TimeOnlyZone::default();
    let ampm_period = Regex::new(r"(?i)[ap]\.\s?m\.$").unwrap();
    if clock.ends_with('.') && !ampm_period.is_match(clock) {
        return r;
    }
    let Some(lead) = time_only_zone_token_re().captures(rest) else {
        return r;
    };
    let lead_len = lead.get(0).map(|m| m.end()).unwrap_or(0);
    let mut tok = lead.name("tok").map(|m| m.as_str()).unwrap_or("");
    let after = &rest[lead_len..];
    // a period that ends the sentence ends the qualifier too: nothing after it is read
    let ended = tok.ends_with('.');
    if ended {
        tok = tok.trim_end_matches('.');
    }
    let zone_word = time_only_zone_word(tok);
    if !zone_word.is_empty() || tok.starts_with('+') || tok.starts_with('-') {
        let mut off_text = tok[zone_word.len()..].to_string();
        if zone_word.is_empty() && (off_text == "+" || off_text == "-") {
            // a lone sign: a dash between words - unless a number follows it (a spaced offset)
            if !ended && number_follows(after) {
                r.declined = true;
            }
            return r;
        }
        if !zone_word.is_empty() && off_text.is_empty() && !ended {
            // the offset after spaces (UTC +02:00); a lone sign there is a dash unless a number
            // follows
            if let Some(next) = time_only_zone_next_re().captures(after) {
                let t2 = next.name("tok").map(|m| m.as_str()).unwrap_or("");
                if t2 == "+" || t2 == "-" {
                    let next_len = next.get(0).map(|m| m.end()).unwrap_or(0);
                    if number_follows(&after[next_len..]) {
                        r.declined = true;
                        return r;
                    }
                } else {
                    off_text = t2.trim_end_matches('.').to_string();
                }
            }
        }
        // Z takes no offset
        if zone_word == "Z" && !off_text.is_empty() {
            r.declined = true;
            return r;
        }
        if off_text.is_empty() {
            r.offset = FixedOffset::east_opt(0);
            return r;
        }
        let Some((negative, h, mm)) = time_only_offset(&off_text) else {
            r.declined = true;
            return r;
        };
        if h > 14 || mm > 59 || h * 60 + mm > 840 {
            r.declined = true;
            return r;
        }
        let secs = (h * 3600 + mm * 60) as i32;
        r.offset = FixedOffset::east_opt(if negative { -secs } else { secs });
        return r;
    }
    if let Ok(Some(w)) = time_only_other_zone_re().captures(rest) {
        let word = w.name("w").map(|m| m.as_str()).unwrap_or("");
        if word != "AND" && word != "OR" {
            r.declined = true;
        }
    }
    r
}

/// Form 1b (0.6.1), a time without a date: "try again at 9:43 PM.", "try again at 21:43" (also after
/// `resets at`, `available at`, `until`): today at that time unless that moment is more than 5
/// minutes before the reference (the moment of parsing) - then tomorrow; a reset just passed stays
/// passed (F06-3); both instants of a repeated wall time, the first after a spring gap (F06-4); a
/// UTC / GMT / Z or numeric-offset qualifier is honoured, another zone word declines the wording
/// (F06-5, F08-2).
fn time_only_reset(
    text: &str,
    reference: DateTime<FixedOffset>,
    zone: Option<&dyn ResetZone>,
) -> Option<DateTime<FixedOffset>> {
    let c = time_only_re().captures(text).ok().flatten()?;
    let whole = c.get(0)?;
    let mut hour: u32 = c.name("hour")?.as_str().parse().ok()?;
    let minute: u32 = c.name("min")?.as_str().parse().ok()?;
    let second: u32 = match c.name("sec") {
        Some(s) => s.as_str().parse().ok()?,
        None => 0,
    };
    let mut ok = minute <= 59 && second <= 59;
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
    } else if hour > 23 {
        ok = false;
    }
    if !ok {
        return None;
    }
    let qualifier = time_only_zone(whole.as_str(), &text[whole.end()..]);
    if qualifier.declined {
        return None;
    }
    let tod = NaiveTime::from_hms_opt(hour, minute, second)?;
    let at = select_time_only_reset(tod, reference, zone, qualifier.offset)?;
    // a qualified time is an instant: shown in the zone, as an ISO instant is
    if qualifier.offset.is_some() {
        return Some(to_zone_time(at, reference, zone));
    }
    Some(at)
}

/// Form 1, the Codex wording: a month-name date with a wall-clock time ("try again at Sep 28th,
/// 2026 8:35 PM"); without a year the reference's year, the next one when that date is more than
/// a day past. A wording that names no valid time falls through to the next form.
fn dated_reset(
    text: &str,
    reference: DateTime<FixedOffset>,
    zone: Option<&dyn ResetZone>,
) -> Option<DateTime<FixedOffset>> {
    let codex = Regex::new(
        r"(?i)(?:try\s+again\s+(?:at|on|after)|resets?\s+(?:at|on)|available\s+(?:again\s+)?(?:at|on|after)|until)\s+(?P<mon>jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\.?\s+(?P<day>[0-9]{1,2})(?:st|nd|rd|th)?\b,?\s*(?:(?P<year>[0-9]{4})\b,?\s*)?(?:at\s+)?(?P<hour>[0-9]{1,2}):(?P<min>[0-9]{2})(?::(?P<sec>[0-9]{2}))?(?:\s*(?P<ampm>[ap])\.?\s?m\b\.?)?"
    ).unwrap();
    let c = codex.captures(text)?;
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let mon3 = c["mon"][..3].to_lowercase();
    let month = months.iter().position(|m| *m == mon3)? as u32 + 1;
    let day: u32 = c["day"].parse().ok()?;
    let mut hour: u32 = c["hour"].parse().ok()?;
    let minute: u32 = c["min"].parse().ok()?;
    let second: u32 = match c.name("sec") {
        Some(s) => s.as_str().parse().ok()?,
        None => 0,
    };
    if let Some(ap) = c.name("ampm") {
        if !(1..=12).contains(&hour) {
            return None;
        }
        let pm = ap.as_str().eq_ignore_ascii_case("p");
        if hour == 12 {
            hour = 0;
        }
        if pm {
            hour += 12;
        }
    }
    let build = |year: i32| -> Option<DateTime<FixedOffset>> {
        let wall = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)?;
        from_wall_clock(wall, reference, zone)
    };
    if let Some(y) = c.name("year") {
        return build(y.as_str().parse().ok()?);
    }
    let at = build(reference.year())?;
    if at < reference - Duration::days(1) {
        return build(reference.year() + 1);
    }
    Some(at)
}

/// Form 2, an ISO timestamp after try again / retry / reset / until / available: with an offset
/// that instant (shown in the zone), without one a wall-clock time of the zone.
fn iso_reset(
    text: &str,
    reference: DateTime<FixedOffset>,
    zone: Option<&dyn ResetZone>,
) -> Option<DateTime<FixedOffset>> {
    let iso = Regex::new(r"(?i)(?:try\s+again|retry|resets?|until|available)[^0-9\r\n]{0,24}?(?P<date>[0-9]{4}-[0-9]{2}-[0-9]{2})[T ](?P<time>[0-9]{2}:[0-9]{2}(?::[0-9]{2}(?:\.[0-9]+)?)?)(?P<tz>Z|[+-][0-9]{2}:?[0-9]{2})?").unwrap();
    let c = iso.captures(text)?;
    let time = if c["time"].len() == 5 {
        format!("{}:00", &c["time"])
    } else {
        c["time"].to_string()
    };
    let stamp = format!("{}T{}", &c["date"], time);
    if let Some(tz) = c.name("tz") {
        let mut tzs = tz.as_str().to_string();
        if tzs.len() == 5 && tzs != "Z" && tzs != "z" {
            tzs = format!("{}:{}", &tzs[..3], &tzs[3..]);
        }
        let dto = DateTime::parse_from_rfc3339(&format!("{stamp}{}", tzs.to_uppercase())).ok()?;
        return Some(to_zone_time(dto, reference, zone));
    }
    let format = if time.contains('.') {
        "%Y-%m-%dT%H:%M:%S%.f"
    } else {
        "%Y-%m-%dT%H:%M:%S"
    };
    let wall = NaiveDateTime::parse_from_str(&stamp, format).ok()?;
    from_wall_clock(wall, reference, zone)
}

/// `Get-RetryAfter` on the read path (`-ReferenceOffset`): the reset time a message names, with
/// the offset of `reference` (the failure's own `when` - the reader's zone may not be the
/// recorder's), or `None`.
pub fn retry_after_ref(
    message: &str,
    reference: DateTime<FixedOffset>,
) -> Option<DateTime<FixedOffset>> {
    retry_after_with(message, reference, None)
}

/// `Get-RetryAfter` at write time (`New-ProviderFailure`, on the machine that saw the failure):
/// a wall-clock reset time is read with the rules of `zone` (`chrono::Local` there; daylight
/// saving included - F15-1, and for a time without a date the 0.6.1 rules), every instant is
/// shown in it. `reference` is the moment of parsing (the consult clock).
pub fn retry_after_in(
    message: &str,
    reference: DateTime<FixedOffset>,
    zone: &dyn ResetZone,
) -> Option<DateTime<FixedOffset>> {
    retry_after_with(message, reference, Some(zone))
}

/// `Get-RetryAfter`: the reset time a message names, in this order - 1. the Codex month-name
/// wording, 2. an ISO timestamp, 1b. a time without a date (0.6.1), 3. `Retry-After: N`, 4. "try
/// again in <parts>", 5. a compact duration, 6. Google's "retry in", 7. gRPC `retryDelay`, 8. a
/// rolling window (an upper bound). `zone` `None` is `-ReferenceOffset`.
fn retry_after_with(
    message: &str,
    reference: DateTime<FixedOffset>,
    zone: Option<&dyn ResetZone>,
) -> Option<DateTime<FixedOffset>> {
    if message.is_empty() {
        return None;
    }
    let text = message.replace(['\u{2018}', '\u{2019}'], "'");
    let shown = |at: DateTime<FixedOffset>| to_zone_time(at, reference, zone);

    if let Some(at) = dated_reset(&text, reference, zone) {
        return Some(at);
    }
    if let Some(at) = iso_reset(&text, reference, zone) {
        return Some(at);
    }
    if let Some(at) = time_only_reset(&text, reference, zone) {
        return Some(at);
    }

    // 3. retry-after: N unit (negative lookahead -> fancy-regex)
    let after = fancy_regex::Regex::new(&format!(
        r"(?i)retry[- ]after[:\s]\s*(?P<n>[0-9]+)(?![0-9:.\-])(?:\s*(?P<u>{DUR_UNIT}))?"
    ))
    .unwrap();
    if let Ok(Some(c)) = after.captures(&text) {
        let n: i64 = c.name("n").unwrap().as_str().parse().unwrap();
        let unit = c.name("u").map(|m| m.as_str()).unwrap_or("s");
        return Some(shown(
            reference + Duration::seconds(duration_seconds(n, unit)),
        ));
    }

    // 4. try again / resets in <parts>
    let in_re = Regex::new(&format!(r"(?i)(?:try\s+again|resets?)\s+in\s+(?P<parts>[0-9]+\s*{DUR_UNIT}(?:(?:\s*,\s*|\s+and\s+|\s+)[0-9]+\s*{DUR_UNIT})*)")).unwrap();
    if let Some(c) = in_re.captures(&text) {
        let part = Regex::new(&format!(r"(?i)(?P<n>[0-9]+)\s*(?P<u>{DUR_UNIT})")).unwrap();
        let mut total = 0i64;
        for p in part.captures_iter(&c["parts"]) {
            total += duration_seconds(p["n"].parse().unwrap(), &p["u"]);
        }
        return Some(shown(reference + Duration::seconds(total)));
    }

    // 5. compact duration (negative lookahead -> fancy-regex)
    let compact = fancy_regex::Regex::new(r"(?i)\b(?:try\s+again|resets?|retry|available(?:\s+again)?)\s+in\s+(?P<dur>(?:[0-9]+(?:\.[0-9]+)?(?:w|d|h|ms|m|s))+)(?![A-Za-z0-9])").unwrap();
    if let Ok(Some(c)) = compact.captures(&text) {
        if let Some(secs) = compact_duration_seconds(c.name("dur").unwrap().as_str()) {
            return Some(shown(reference + Duration::seconds(secs.ceil() as i64)));
        }
    }

    // 6. Google "retry in ..."
    let retry_in = fancy_regex::Regex::new(r"(?i)\bretry\s+in\s+(?P<dur>(?:[0-9]+(?:\.[0-9]+)?(?:ms|h|m|s))+(?![A-Za-z0-9])|[0-9]+(?:\.[0-9]+)?\s*(?:hours?|hrs?|minutes?|mins?|seconds?|secs?)\b(?:(?:\s*,\s*|\s+and\s+|\s+)[0-9]+(?:\.[0-9]+)?\s*(?:hours?|hrs?|minutes?|mins?|seconds?|secs?)\b)*)").unwrap();
    if let Ok(Some(c)) = retry_in.captures(&text) {
        if let Some(secs) = go_duration_seconds(c.name("dur").unwrap().as_str()) {
            return Some(shown(reference + Duration::seconds(secs.ceil() as i64)));
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
            return Some(shown(reference + Duration::seconds(s.ceil() as i64)));
        }
    }

    // 8. rolling window
    let window = Regex::new(r"(?i)\bresets?\s+when\s+the\s+current\s+(?P<n>[0-9]+)[- ](?P<u>minute|hour|day|week)s?\s+window\s+ends").unwrap();
    if let Some(c) = window.captures(&text) {
        return Some(shown(
            reference + Duration::seconds(duration_seconds(c["n"].parse().unwrap(), &c["u"])),
        ));
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
    /// (wave 1b, E5) the endpoint fingerprint the record was recorded on (a plan's record set
    /// spans several).
    pub fingerprint: String,
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

/// The bridge outcome of a quota mark read as a failure (`Get-EndpointHealth`, E15).
const QUOTA_MARK_OUTCOME: &str =
    "failed: quota (a rate limit rejected a request during a usable turn)";

/// (wave 29b, E15) The quota failure a usable reply's `engine_run.quota_mark` stands for: an
/// entry of its own 1 ms after the entry that carries it (by completion), so the route stays out
/// until the mark's reset (or 60 minutes, 10 for a burst) exactly as after a failed quota turn,
/// and a later usable reply clears it as usual. `None` without a mark of class quota.
fn quota_mark_entry(c: &Value) -> Option<Value> {
    let qm = &c["engine_run"]["quota_mark"];
    if !qm.is_object() || qm["class"].as_str() != Some("quota") {
        return None;
    }
    let at = dto(&c["when"])?;
    let order = dto(&c["finished_at"]).unwrap_or_else(|| {
        let ws = c["wall_seconds"]
            .as_f64()
            .or_else(|| c["wall_seconds"].as_str().and_then(|s| s.parse().ok()))
            .unwrap_or(0.0);
        if ws > 0.0 {
            at + Duration::milliseconds((ws * 1000.0) as i64)
        } else {
            at
        }
    });
    let finished =
        (order + Duration::milliseconds(1)).to_rfc3339_opts(chrono::SecondsFormat::Millis, false);
    Some(serde_json::json!({
        "n": c["n"].clone(),
        "when": c["when"].clone(),
        "finished_at": finished,
        "bridge_outcome": QUOTA_MARK_OUTCOME,
        "reviewer": c["reviewer"].clone(),
        "provider_failure": qm.clone(),
    }))
}

/// `Get-EndpointHealth`: health of one endpoint fingerprint from all task consults.
pub fn endpoint_health(
    consults: &[Value],
    fingerprint: &str,
    now_utc: DateTime<Utc>,
) -> EndpointHealth {
    endpoint_health_set(consults, &[fingerprint.to_string()], now_utc)
}

/// `Get-EndpointHealth -Fingerprints` (wave 29b, E5): the records of every fingerprint in
/// `fingerprints` read as ONE record set - the routes of one coding plan, of which only the
/// quota is used ([`crate::plan::plan_quota`]). Every record carries its fingerprint.
pub fn endpoint_health_set(
    consults: &[Value],
    fingerprints: &[String],
    now_utc: DateTime<Utc>,
) -> EndpointHealth {
    let mut h = EndpointHealth::default();
    let fp_set: Vec<&str> = fingerprints
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    if fp_set.is_empty() {
        return h;
    }
    let builtin = builtin_openai_fingerprint();
    // (E15) the quota marks as entries of their own, after the entries that carry them
    let marks: Vec<Value> = consults.iter().filter_map(quota_mark_entry).collect();
    let mut records: Vec<Record> = Vec::new();
    for c in consults.iter().chain(marks.iter()) {
        let rev = &c["reviewer"];
        let fp = if rev.is_null() {
            builtin.clone()
        } else {
            rev["provider_fingerprint"]
                .as_str()
                .unwrap_or("")
                .to_string()
        };
        if fp.is_empty() || !fp_set.contains(&fp.as_str()) {
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
            fingerprint: fp,
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
            // (wave 26c, D2 / F26-2) a machine-wide record's stored until (and a quota mark's) is
            // its until - the tie-break of two records at the same time
            if let Some(stored) = dto(&c["provider_failure"]["until"]) {
                rec.until = stored;
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
    /// (wave 29b, E15) on an `ok` record whose turn saw a rejecting rate-limit event: the quota
    /// failure it stands for (`{class, kind, until, retry_after, message, when}`). C3 does not
    /// write one yet (the claude engine, wave 4) but keeps a record's mark when it rewrites the
    /// file, prunes by it and reads it ([`machine_endpoint_consults`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_mark: Option<Value>,
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
    /// (wave 29b, E16) the roster entry's plan - written only when the entry names one, so a
    /// panel elsewhere counts this run against the plan's limit too ([`machine_running_count`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub plan: String,
}

/// [`machine_running_count`]'s result (`Get-MachineRunningCount`): the live rows that count, and
/// those of them that count ONLY through their plan (their endpoint is none of the group's).
#[derive(Clone, Debug, Default)]
pub struct MachineRunningCount {
    pub rows: Vec<MachineRunning>,
    pub by_plan: Vec<MachineRunning>,
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
) -> HealthUpdate {
    add_machine_health_record_with(
        path,
        fingerprint,
        outcome,
        failure,
        repo,
        is_alive,
        LockBudget::full(),
    )
}

/// (wave 27c, D8) `add_machine_health_record` with the BOUNDED in-lock budget (one attempt of at
/// most one second): used at the ledger commit while the task write lock is held, so the health
/// retry never extends the hold on it. The caller does the full retry after releasing the lock.
pub fn add_machine_health_record_bounded(
    path: &Path,
    fingerprint: &str,
    outcome: &str,
    failure: Option<&MachineFailure>,
    repo: &str,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> HealthUpdate {
    add_machine_health_record_with(
        path,
        fingerprint,
        outcome,
        failure,
        repo,
        is_alive,
        LockBudget::bounded(),
    )
}

#[allow(clippy::too_many_arguments)]
fn add_machine_health_record_with(
    path: &Path,
    fingerprint: &str,
    outcome: &str,
    failure: Option<&MachineFailure>,
    repo: &str,
    is_alive: &dyn Fn(u32, &str) -> bool,
    budget: LockBudget,
) -> HealthUpdate {
    match new_machine_health_record(fingerprint, outcome, failure, repo) {
        Some(record) => update_machine_health(path, Some(record), None, 0, is_alive, budget),
        None => HealthUpdate::Skipped,
    }
}

/// (wave 28b, D13) `Add-MachineHealthRecord -Record`: merge a record built ONCE by
/// [`new_machine_health_record`] (the one a run journals at its commit and retries after it - the
/// same `when`, so applying it twice adds it once) with the full lock budget.
pub fn add_machine_health_endpoint(
    path: &Path,
    record: &MachineEndpoint,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> HealthUpdate {
    update_machine_health(
        path,
        Some(record.clone()),
        None,
        0,
        is_alive,
        LockBudget::full(),
    )
}

/// (wave 28b, D13) `New-MachineHealthRecord`: the endpoint record of a run's outcome - class `ok`
/// for a usable reply, else its provider failure (class `operator` excepted) - or `None` when there
/// is nothing to record (no fingerprint, an operator-class failure, neither a usable outcome nor a
/// failure). Its `when` is fixed here, so the journaled record and the retried one are the same.
pub fn new_machine_health_record(
    fingerprint: &str,
    outcome: &str,
    failure: Option<&MachineFailure>,
    repo: &str,
) -> Option<MachineEndpoint> {
    if fingerprint.is_empty() {
        return None;
    }
    if is_usable_outcome(outcome) {
        return Some(MachineEndpoint {
            endpoint: fingerprint.to_string(),
            class: "ok".into(),
            kind: String::new(),
            until: None,
            retry_after: None,
            repo: repo.to_string(),
            when: format_offset_iso(now_offset()),
            message: String::new(),
            quota_mark: None,
        });
    }
    let f = failure?;
    let class = f.class.clone();
    if class.is_empty() || class == "operator" {
        return None;
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
    Some(MachineEndpoint {
        endpoint: fingerprint.to_string(),
        class,
        kind,
        until: until.map(format_offset_iso),
        retry_after: retry_after.map(format_offset_iso),
        repo: repo.to_string(),
        when: format_offset_iso(when),
        message,
        quota_mark: None,
    })
}

// ----------------------------------------------------------------------------- the journal
//
// (wave 28b, D13 / F36-6, F37-1) THE JOURNAL beside the health file: `<health file>.journal`,
// append-only NDJSON, one endpoint record per line. A run whose health update failed before its
// ledger commit appends its record there INSIDE the commit (a local append - no wait for the health
// lock; [`add_machine_health_journal`]); every health update (the retry after the commit, the next
// run of ANY repository - its registration, its outcome) applies the journal under the health lock
// and empties it once the health file is written. Applying is idempotent: a record already in
// `endpoints[]` (the same endpoint, class, kind, when and repository - and quota mark) is not added
// again. (wave 28c, D10) A line that does not parse is moved to `<health file>.journal.bad`
// (appended: the time, a tab, the line's bytes) and counted in a note; the journal loses only what
// was applied or moved. The plugin and C3 share the health file and its journal.

/// `Get-MachineHealthJournalPath`: `<health file>.journal`.
pub fn machine_health_journal_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".journal");
    PathBuf::from(os)
}

/// (wave 28c, D10) The notes of the journal's apply in this process ("health journal: <n>
/// unreadable line(s) kept in <file>"), taken by [`take_machine_health_journal_notes`].
static JOURNAL_NOTES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// `Get-MachineHealthJournalNotes`: the notes so far, and the list starts again - the run puts them
/// into its `warnings[]` (before the commit) or its summary (after it).
pub fn take_machine_health_journal_notes() -> Vec<String> {
    match JOURNAL_NOTES.lock() {
        Ok(mut g) => std::mem::take(&mut *g),
        Err(e) => std::mem::take(&mut *e.into_inner()),
    }
}

fn push_journal_note(note: String) {
    match JOURNAL_NOTES.lock() {
        Ok(mut g) => g.push(note),
        Err(e) => e.into_inner().push(note),
    }
}

/// Open a file for exclusive use (Windows: share mode none; elsewhere an advisory `flock`).
#[cfg(windows)]
fn open_exclusive(path: &Path, append: bool, create: bool) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut o = OpenOptions::new();
    if append {
        o.append(true);
    } else {
        o.read(true).write(true);
    }
    o.create(create).share_mode(0).open(path)
}

#[cfg(not(windows))]
fn open_exclusive(path: &Path, append: bool, create: bool) -> std::io::Result<std::fs::File> {
    let mut o = OpenOptions::new();
    if append {
        o.append(true);
    } else {
        o.read(true).write(true);
    }
    let f = o.create(create).open(path)?;
    match f.try_lock() {
        Ok(()) => Ok(f),
        Err(std::fs::TryLockError::WouldBlock) => {
            Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
        }
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

/// `Add-MachineHealthJournal`: append one endpoint record to the journal (exclusive open, retried up
/// to 2 s). `Err(why)` when it was not written; never panics.
pub fn add_machine_health_journal(path: &Path, record: &MachineEndpoint) -> Result<(), String> {
    use std::io::Write;
    let j = machine_health_journal_path(path);
    if let Some(dir) = j.parent() {
        if !dir.as_os_str().is_empty() && !dir.is_dir() {
            return Err(format!("the directory {} does not exist", dir.display()));
        }
    }
    let mut line = serde_json::to_vec(record).map_err(|e| crate::one_line(&e.to_string()))?;
    line.push(b'\n');
    let started = std::time::Instant::now();
    loop {
        match open_exclusive(&j, true, true) {
            Ok(mut f) => {
                return f
                    .write_all(&line)
                    .and_then(|_| f.sync_all())
                    .map_err(|e| crate::one_line(&e.to_string()));
            }
            Err(e) => {
                // a sharing violation (32), a lock violation (33), a held flock: busy - wait
                let busy = matches!(
                    e.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
                ) || matches!(e.raw_os_error(), Some(32) | Some(33));
                if !busy {
                    return Err(crate::one_line(&e.to_string()));
                }
                if started.elapsed() >= std::time::Duration::from_secs(2) {
                    return Err(format!("the journal {} stayed busy", j.display()));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

/// .NET ticks (100 ns since 0001-01-01 UTC) of an ISO time, `""` when it does not parse.
fn utc_ticks(iso: &str) -> String {
    match DateTime::parse_from_rfc3339(iso.trim()) {
        Ok(d) => {
            let d = d.with_timezone(&Utc);
            const UNIX_EPOCH_TICKS: i128 = 621_355_968_000_000_000;
            let t = UNIX_EPOCH_TICKS
                + d.timestamp() as i128 * 10_000_000
                + (d.timestamp_subsec_nanos() / 100) as i128;
            t.to_string()
        }
        Err(_) => String::new(),
    }
}

/// `Get-MachineHealthRecordKey`: the identity of an endpoint record for the journal's idempotent
/// apply - endpoint, class, kind, repository and the instant of `when`; (wave 29b, A6) a record that
/// carries a quota mark adds the mark's `until`.
fn machine_record_key(e: &MachineEndpoint) -> String {
    let mut k = format!(
        "{}|{}|{}|{}|{}",
        e.endpoint,
        e.class,
        e.kind,
        e.repo,
        utc_ticks(&e.when)
    );
    if let Some(m) = &e.quota_mark {
        k.push_str("|mark:");
        k.push_str(&utc_ticks(m["until"].as_str().unwrap_or("")));
    }
    k
}

/// One unreadable journal line: its byte range (the line end included when there is one).
struct BadLine {
    start: usize,
    end: usize,
}

/// `Register-MachineRunning`: removes any existing row for `row.pid`, then adds it.
pub fn register_machine_running(
    path: &Path,
    row: MachineRunning,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> bool {
    let pid = row.pid;
    update_machine_health(path, None, Some(row), pid, is_alive, LockBudget::full())
        == HealthUpdate::Written
}

/// `Unregister-MachineRunning`: drops any running row for `pid`.
pub fn unregister_machine_running(
    path: &Path,
    pid: u32,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> bool {
    update_machine_health(path, None, None, pid, is_alive, LockBudget::full())
        == HealthUpdate::Written
}

/// `Get-MachineRunningCount`: the live running rows (per `is_alive`) whose endpoint is one of
/// `fingerprints` or - (wave 29b, E16) - whose plan is one of `plans` (whatever their endpoint,
/// engine and repository), excluding `exclude_panel`'s own rows when that panel id is non-empty.
/// Reads the file directly, not under the write lock.
pub fn machine_running_count(
    path: &Path,
    fingerprints: &[String],
    exclude_panel: &str,
    plans: &[String],
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> MachineRunningCount {
    let rows: Vec<MachineRunning> = read_machine_health(path)
        .running
        .into_iter()
        .filter(|r| {
            (!r.endpoint.is_empty() && fingerprints.iter().any(|f| f == &r.endpoint))
                || (!r.plan.is_empty() && plans.iter().any(|p| !p.is_empty() && p == &r.plan))
        })
        .filter(|r| !(!exclude_panel.is_empty() && r.panel == exclude_panel))
        .filter(|r| is_alive(r.pid, &r.start_time))
        .collect();
    let by_plan = rows
        .iter()
        .filter(|r| !fingerprints.iter().any(|f| f == &r.endpoint))
        .cloned()
        .collect();
    MachineRunningCount { rows, by_plan }
}

/// The outcome of a machine-health update: written, skipped (nothing to record), blocked by a lock
/// timeout, or a write failure (wave 26c D2, wave 27c D7). Every non-`Written`/`Skipped` outcome
/// carries a cause the caller names in `machine-wide health not updated (<cause>)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthUpdate {
    /// The file was updated.
    Written,
    /// Nothing to record (no fingerprint, an operator-class failure, or no usable outcome/failure).
    Skipped,
    /// `<file>.lock` could not be acquired within the retry budget (`MachineHealthLastError`).
    LockTimeout,
    /// The lock was held but the read-modify-write itself failed (serialize/atomic-write); the
    /// string is the cause (wave 27c, D7).
    Failed(String),
}

impl HealthUpdate {
    /// (wave 27c, D7) The cause to name in the warning, or `None` for `Written`/`Skipped`.
    pub fn cause(&self) -> Option<String> {
        match self {
            HealthUpdate::Written | HealthUpdate::Skipped => None,
            HealthUpdate::LockTimeout => Some("lock timeout".to_string()),
            HealthUpdate::Failed(why) => Some(why.clone()),
        }
    }
    /// Whether the update did not happen for a real reason (a cause the caller warns about).
    pub fn failed(&self) -> bool {
        matches!(self, HealthUpdate::LockTimeout | HealthUpdate::Failed(_))
    }
}

/// Seconds per lock attempt (`CODEX_CONSULT_TEST_HEALTH_LOCK_SEC`, else 5); three attempts are
/// made (wave 26c, D2).
fn machine_lock_attempt_secs() -> f64 {
    crate::test_hooks::hook("CODEX_CONSULT_TEST_HEALTH_LOCK_SEC")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(5.0)
}

const MACHINE_LOCK_ATTEMPTS: u32 = 3;

/// (wave 27c, D8) The lock budget of a machine-health update. The FULL budget — three attempts of
/// five seconds each (`CODEX_CONSULT_TEST_HEALTH_LOCK_SEC` shortens an attempt) — runs only OUTSIDE
/// the task write lock. Inside the write lock a run takes the BOUNDED budget (one attempt of at
/// most one second) so the health retry never extends the hold on the commit lock.
#[derive(Debug, Clone, Copy)]
pub struct LockBudget {
    attempts: u32,
    attempt_secs: f64,
}

impl LockBudget {
    /// The full budget: three attempts of `CODEX_CONSULT_TEST_HEALTH_LOCK_SEC` (else 5) seconds.
    pub fn full() -> Self {
        LockBudget {
            attempts: MACHINE_LOCK_ATTEMPTS,
            attempt_secs: machine_lock_attempt_secs(),
        }
    }
    /// The bounded in-lock budget: one attempt of at most one second.
    pub fn bounded() -> Self {
        LockBudget {
            attempts: 1,
            attempt_secs: machine_lock_attempt_secs().min(1.0),
        }
    }
}

/// Acquire the machine-health lock file (`<path>.lock`, wave 26c D2 / 27c D8) within `budget`, each
/// attempt polling with a doubling back-off (25 ms, capped at 500 ms). `None` when the lock could
/// never be taken.
#[cfg(windows)]
fn acquire_machine_lock(lock_path: &Path, budget: LockBudget) -> Option<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let open = || {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(lock_path)
    };
    let attempt = std::time::Duration::from_secs_f64(budget.attempt_secs);
    for _ in 0..budget.attempts {
        let started = std::time::Instant::now();
        let mut delay = std::time::Duration::from_millis(25);
        loop {
            match open() {
                Ok(f) => return Some(f),
                Err(_) => {
                    if started.elapsed() >= attempt {
                        break;
                    }
                    std::thread::sleep(delay);
                    delay = (delay * 2).min(std::time::Duration::from_millis(500));
                }
            }
        }
    }
    None
}

/// Non-Windows: there is no share mode, so the lock is an exclusive advisory `flock` on the file
/// (the same mechanism `store.rs` uses for the task lock). The file is created once and kept; a
/// crash releases the lock with the handle, so a stale `.lock` can never block the next writer
/// (a `create_new` file would).
#[cfg(not(windows))]
fn acquire_machine_lock(lock_path: &Path, budget: LockBudget) -> Option<std::fs::File> {
    let open = || -> std::io::Result<std::fs::File> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        match f.try_lock() {
            Ok(()) => Ok(f),
            Err(std::fs::TryLockError::WouldBlock) => {
                Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
            }
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    };
    let attempt = std::time::Duration::from_secs_f64(budget.attempt_secs);
    for _ in 0..budget.attempts {
        let started = std::time::Instant::now();
        let mut delay = std::time::Duration::from_millis(25);
        loop {
            match open() {
                Ok(f) => return Some(f),
                Err(_) => {
                    if started.elapsed() >= attempt {
                        break;
                    }
                    std::thread::sleep(delay);
                    delay = (delay * 2).min(std::time::Duration::from_millis(500));
                }
            }
        }
    }
    None
}

/// (wave 28b, D13) The test hook `CODEX_CONSULT_TEST_HEALTH_FAIL_FIRST=1` (test mode only) has been
/// used in this process.
static HEALTH_FAIL_FIRST_DONE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Read-modify-write the machine health file under the `<path>.lock` exclusive lock: apply the
/// journal (wave 28b, D13), then `add_endpoint`/`add_running`/`remove_pid`, prune stale endpoints and
/// dead running rows, cap endpoints to the last 500, and write atomically; then move the journal's
/// unreadable lines to `<journal>.bad` and empty the journal by what was applied or moved. Never
/// panics; a failure is named.
fn update_machine_health(
    path: &Path,
    add_endpoint: Option<MachineEndpoint>,
    add_running: Option<MachineRunning>,
    remove_pid: u32,
    is_alive: &dyn Fn(u32, &str) -> bool,
    budget: LockBudget,
) -> HealthUpdate {
    // (wave 28b, D13) TEST HOOK (test mode only): CODEX_CONSULT_TEST_HEALTH_FAIL_FIRST=1 - the FIRST
    // update of this process that adds an endpoint record fails (a lock timeout): the run's own
    // record before the commit, so the journal and the retry after the commit can be watched.
    if add_endpoint.is_some()
        && crate::test_hooks::hook("CODEX_CONSULT_TEST_HEALTH_FAIL_FIRST")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
        && !HEALTH_FAIL_FIRST_DONE.swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return HealthUpdate::Failed(
            "lock timeout (test hook CODEX_CONSULT_TEST_HEALTH_FAIL_FIRST)".to_string(),
        );
    }
    // (wave 27c, D7) a failure that is NOT a lock timeout is named by its cause. A missing parent
    // directory is checked first (the plugin's `[IO.Directory]::Exists` guard) so it is not
    // mistaken for a lock timeout when the `.lock` cannot be created.
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() && !dir.is_dir() {
            return HealthUpdate::Failed(format!("the directory {} does not exist", dir.display()));
        }
    }
    let mut lock_os = path.as_os_str().to_os_string();
    lock_os.push(".lock");
    let lock_path = PathBuf::from(lock_os);
    // (wave 26c D2 / 27c D8) a lock timeout is a distinct outcome; inside the task write lock the
    // caller passes the BOUNDED budget and does the full retry after the lock is released.
    let lock = match acquire_machine_lock(&lock_path, budget) {
        Some(f) => f,
        None => return HealthUpdate::LockTimeout,
    };

    let result = (|| -> Result<(), String> {
        use std::io::{Read, Seek, SeekFrom, Write};
        let current = read_machine_health(path);
        let mut endpoints = current.endpoints;
        let mut keys: std::collections::HashSet<String> =
            endpoints.iter().map(machine_record_key).collect();

        // (wave 28b, D13) the journal: held exclusively until the file is written, then emptied
        // (busy: left for the next update). (wave 28c, D10 / F42-6, F44-1) read as BYTES, line by
        // line: a line that parses is applied; one that does not (a torn append of a writer that
        // died) is MOVED to <journal>.bad after the write.
        let journal_path = machine_health_journal_path(path);
        let mut journal: Option<std::fs::File> = None;
        let mut j_bytes: Vec<u8> = Vec::new();
        let mut j_bad: Vec<BadLine> = Vec::new();
        if journal_path.is_file() {
            for _ in 0..20 {
                match open_exclusive(&journal_path, false, false) {
                    Ok(f) => {
                        journal = Some(f);
                        break;
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
                }
            }
            if let Some(f) = journal.as_mut() {
                let _ = f.seek(SeekFrom::Start(0));
                let _ = f.read_to_end(&mut j_bytes);
                let mut pos = 0usize;
                while pos < j_bytes.len() {
                    let end = match j_bytes[pos..].iter().position(|b| *b == b'\n') {
                        Some(i) => pos + i + 1,
                        None => j_bytes.len(),
                    };
                    let line = String::from_utf8_lossy(&j_bytes[pos..end]);
                    let t = line.trim().trim_start_matches('\u{feff}');
                    if !t.is_empty() {
                        let rec = serde_json::from_str::<Value>(t)
                            .ok()
                            .filter(|v| {
                                v.is_object()
                                    && v["endpoint"].as_str().is_some_and(|e| !e.is_empty())
                            })
                            .and_then(|v| serde_json::from_value::<MachineEndpoint>(v).ok());
                        match rec {
                            Some(r) => {
                                if keys.insert(machine_record_key(&r)) {
                                    endpoints.push(r);
                                }
                            }
                            None => j_bad.push(BadLine { start: pos, end }),
                        }
                    }
                    pos = end;
                }
            }
        }

        if let Some(e) = add_endpoint {
            if keys.insert(machine_record_key(&e)) {
                endpoints.push(e);
            }
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
            // (wave 29b, E15) an ok record stays while its quota mark still blocks
            let until = e
                .until
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .or_else(|| {
                    e.quota_mark
                        .as_ref()
                        .and_then(|m| m["until"].as_str())
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                });
            let still_out = until.map(|u| u.with_timezone(&Utc) > now).unwrap_or(false);
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
        // (wave 26b, D13) written through the Windows PowerShell 5.1 `ConvertTo-Json` formatter the
        // plugin uses (`ps_json`), not serde pretty: both tools rewrite the same file, so its bytes
        // must not flip with the last writer. The plugin writes it with `Write-TextAtomic` (no
        // CRLF->LF pass), so the on-disk form is CRLF between lines + a trailing LF.
        let bytes = crate::ps_json::to_ps_json_crlf_bytes(&out)
            .map_err(|e| format!("write failed: {}", crate::one_line(&e.to_string())))?;
        crate::store::write_text_atomic(path, &bytes)
            .map_err(|e| format!("write failed: {}", crate::one_line(&e.to_string())))?;

        // (D13) applied: the journal is emptied (a crash before this applies it again - no
        // double); (wave 28c, D10) first the unreadable lines go to <journal>.bad (with the time),
        // then the journal loses exactly the prefix that was applied or moved.
        if let Some(f) = journal.as_mut() {
            let mut consumed = j_bytes.len();
            if !j_bad.is_empty() {
                let mut bad_os = journal_path.as_os_str().to_os_string();
                bad_os.push(".bad");
                let bad_path = PathBuf::from(bad_os);
                let moved = (|| -> std::io::Result<()> {
                    let mut b = open_exclusive(&bad_path, true, true)?;
                    for bl in &j_bad {
                        let mut len = bl.end - bl.start;
                        if len > 0 && j_bytes[bl.end - 1] == b'\n' {
                            len -= 1;
                        }
                        let head = format!(
                            "{}\t",
                            chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z")
                        );
                        b.write_all(head.as_bytes())?;
                        b.write_all(&j_bytes[bl.start..bl.start + len])?;
                        b.write_all(b"\n")?;
                    }
                    b.sync_all()
                })()
                .is_ok();
                if moved {
                    push_journal_note(format!(
                        "health journal: {} unreadable line(s) kept in {}",
                        j_bad.len(),
                        bad_path.display()
                    ));
                } else {
                    // not moved: they stay in the journal - with everything from the first on
                    consumed = j_bad[0].start;
                    push_journal_note(format!(
                        "health journal: {} unreadable line(s) could not be moved to {} - kept in {}",
                        j_bad.len(),
                        bad_path.display(),
                        journal_path.display()
                    ));
                }
            }
            let rest = j_bytes.len() - consumed;
            if rest == 0 {
                let _ = f.set_len(0);
            } else {
                let _ = f.seek(SeekFrom::Start(0));
                let _ = f.write_all(&j_bytes[consumed..]);
                let _ = f.set_len(rest as u64);
            }
            let _ = f.flush();
        }
        Ok(())
    })();

    // Release and remove the lock file we acquired (both platforms), so a waiter never inherits a
    // held name and the machine-wide `.lock` never lingers. Non-Windows: the handle's flock is the
    // lock and the file's NAME is what the next writer locks, so the file stays (removing it would
    // let a waiter that opened the old inode and a newcomer hold the lock at once).
    drop(lock);
    #[cfg(windows)]
    let _ = std::fs::remove_file(&lock_path);

    match result {
        Ok(()) => HealthUpdate::Written,
        // (wave 27c, D7) the lock was held but the read-modify-write failed: a named failure, not a
        // silent skip, so the caller sets the retry flag and warns.
        Err(why) => HealthUpdate::Failed(why),
    }
}

/// (wave 28b, D13) An update that only applies the journal (and prunes) - what every later update
/// does first; for the tests and a caller with nothing else to add.
pub fn apply_machine_health_journal(
    path: &Path,
    is_alive: &dyn Fn(u32, &str) -> bool,
) -> HealthUpdate {
    update_machine_health(path, None, None, 0, is_alive, LockBudget::full())
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
                // (wave 29b, E15) a usable reply with a quota mark: the mark travels as
                // engine_run.quota_mark (its own when, reset and until when valid)
                if let Some(mk) = e.quota_mark.as_ref().filter(|m| m.is_object()) {
                    let valid = |k: &str| -> Value {
                        if dto(&mk[k]).is_some() {
                            mk[k].clone()
                        } else {
                            Value::Null
                        }
                    };
                    let when = if dto(&mk["when"]).is_some() {
                        mk["when"].clone()
                    } else {
                        Value::String(e.when.clone())
                    };
                    v["engine_run"] = serde_json::json!({
                        "quota_mark": {
                            "class": "quota",
                            "kind": mk["kind"].as_str().unwrap_or(""),
                            "code": "",
                            "message": mk["message"].as_str().unwrap_or(""),
                            "when": when,
                            "retry_after": valid("retry_after"),
                            "until": valid("until"),
                        }
                    });
                }
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
                    "until": e.until,
                });
            }
            v
        })
        .collect()
}

#[cfg(test)]
mod machine_health_tests {
    use super::*;

    fn journal_scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "c3-journal-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn ok_record(fp: &str, repo: &str) -> MachineEndpoint {
        new_machine_health_record(fp, "usable reply", None, repo).unwrap()
    }

    fn endpoints_of(path: &Path, fp: &str) -> usize {
        read_machine_health(path)
            .endpoints
            .iter()
            .filter(|e| e.endpoint == fp)
            .count()
    }

    // (wave 28b, D13) the journal beside the health file: any later update applies it and empties
    // it; the same record again (journal + direct) adds nothing.
    #[test]
    fn the_journal_is_applied_once_and_emptied() {
        let alive = |_: u32, _: &str| true;
        let dir = journal_scratch("apply");
        let hp = dir.join("health.json");
        let rec = ok_record("fp-28b", "C:\\repo-a");
        add_machine_health_journal(&hp, &rec).unwrap();
        let jp = machine_health_journal_path(&hp);
        assert_eq!(jp, dir.join("health.json.journal"));
        assert_eq!(std::fs::read_to_string(&jp).unwrap().lines().count(), 1);
        assert_eq!(
            apply_machine_health_journal(&hp, &alive),
            HealthUpdate::Written
        );
        assert_eq!(endpoints_of(&hp, "fp-28b"), 1);
        assert_eq!(std::fs::metadata(&jp).unwrap().len(), 0);
        add_machine_health_journal(&hp, &rec).unwrap();
        assert_eq!(
            add_machine_health_endpoint(&hp, &rec, &alive),
            HealthUpdate::Written
        );
        assert_eq!(endpoints_of(&hp, "fp-28b"), 1);
        // a missing directory is named, for the journal as for the file
        let missing = dir.join("no-such").join("health.json");
        let e = add_machine_health_journal(&missing, &rec).unwrap_err();
        assert!(
            e.starts_with("the directory ") && e.ends_with(" does not exist"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (wave 28c, D10) torn lines go to <journal>.bad ("<time>\t<bytes>"), the good ones are
    // applied, the journal is emptied by what was applied or moved; a .bad that cannot be written
    // keeps the unreadable line and everything after it in the journal.
    #[test]
    fn torn_journal_lines_are_moved_aside_never_dropped() {
        let alive = |_: u32, _: &str| true;
        let dir = journal_scratch("torn");
        let hp = dir.join("health-j.json");
        let jp = machine_health_journal_path(&hp);
        let bad_path = PathBuf::from(format!("{}.bad", jp.display()));
        let a = serde_json::to_string(&ok_record("fp-a", "C:\\repo-a")).unwrap();
        let b = serde_json::to_string(&ok_record("fp-b", "C:\\repo-b")).unwrap();
        let torn1 = r#"{"endpoint":"fp-torn","cla"#;
        let torn2 = r#"{"endpoint":"fp-tail"#;
        std::fs::write(&jp, format!("{a}\n{torn1}\n{b}\n{torn2}")).unwrap();
        let _ = take_machine_health_journal_notes();
        assert_eq!(
            apply_machine_health_journal(&hp, &alive),
            HealthUpdate::Written
        );
        let notes = take_machine_health_journal_notes();
        assert_eq!(endpoints_of(&hp, "fp-a"), 1);
        assert_eq!(endpoints_of(&hp, "fp-b"), 1);
        assert_eq!(std::fs::metadata(&jp).unwrap().len(), 0);
        let bad = std::fs::read_to_string(&bad_path).unwrap();
        let bl: Vec<&str> = bad.lines().collect();
        assert_eq!(bl.len(), 2, "{bad}");
        assert!(
            bl[0].contains('\t') && bl[0].ends_with(&format!("\t{torn1}")),
            "{bad}"
        );
        assert!(bl[1].ends_with(&format!("\t{torn2}")), "{bad}");
        assert_eq!(
            notes,
            vec![format!(
                "health journal: 2 unreadable line(s) kept in {}",
                bad_path.display()
            )]
        );
        // the .bad cannot be written (a directory is in its place)
        std::fs::remove_file(&bad_path).unwrap();
        std::fs::create_dir_all(&bad_path).unwrap();
        let c = serde_json::to_string(&ok_record("fp-c", "C:\\repo-c")).unwrap();
        let d = serde_json::to_string(&ok_record("fp-d", "C:\\repo-d")).unwrap();
        std::fs::write(&jp, format!("{c}\ngarbage-line\n{d}\n")).unwrap();
        assert_eq!(
            apply_machine_health_journal(&hp, &alive),
            HealthUpdate::Written
        );
        let notes2 = take_machine_health_journal_notes().join("|");
        assert_eq!(
            std::fs::read_to_string(&jp).unwrap(),
            format!("garbage-line\n{d}\n")
        );
        assert!(
            notes2.contains("could not be moved to") && notes2.contains(" - kept in "),
            "{notes2}"
        );
        assert_eq!(endpoints_of(&hp, "fp-c"), 1);
        std::fs::remove_dir_all(&bad_path).unwrap();
        assert_eq!(
            apply_machine_health_journal(&hp, &alive),
            HealthUpdate::Written
        );
        assert_eq!(std::fs::metadata(&jp).unwrap().len(), 0);
        assert_eq!(endpoints_of(&hp, "fp-d"), 1);
        assert_eq!(
            take_machine_health_journal_notes(),
            vec![format!(
                "health journal: 1 unreadable line(s) kept in {}",
                bad_path.display()
            )]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (wave 28b, D13) the record key: the same instant in another offset is the same record; a
    // quota mark makes another record.
    #[test]
    fn the_record_key_is_the_instant_and_the_mark() {
        let mut a = ok_record("fp", "r");
        a.when = "2026-10-09T10:00:00+00:00".into();
        let mut b = a.clone();
        b.when = "2026-10-09T12:00:00+02:00".into();
        assert_eq!(machine_record_key(&a), machine_record_key(&b));
        let mut m = a.clone();
        m.quota_mark = Some(serde_json::json!({"until": "2026-10-09T11:00:00+00:00"}));
        assert_ne!(machine_record_key(&a), machine_record_key(&m));
    }
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    // (wave 26b, D13) a machine-health file the plugin wrote (`(ConvertTo-Json -Depth 6) + "`n"`
    // via Write-TextAtomic under Windows PowerShell 5.1: CRLF between lines, trailing LF) must
    // round-trip byte for byte through c3's read + re-serialize, so both tools rewriting the same
    // file never flip its bytes.
    #[test]
    fn plugin_written_file_round_trips_byte_for_byte() {
        let fixture: &[u8] = include_bytes!("../tests/fixtures/machine-health.json");
        let path = temp_path("roundtrip");
        std::fs::write(&path, fixture).unwrap();
        let mh = read_machine_health(&path);
        assert_eq!(mh.endpoints.len(), 2);
        assert!(mh.running.is_empty());
        let out = serde_json::json!({
            "health_version": 1,
            "endpoints": mh.endpoints,
            "running": mh.running,
        });
        let bytes = crate::ps_json::to_ps_json_crlf_bytes(&out).unwrap();
        assert_eq!(
            bytes, fixture,
            "c3's PS-5.1 re-serialize must equal the plugin's bytes"
        );
        let _ = std::fs::remove_file(&path);
    }

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

    /// The three tests below change one process-wide variable; they take turns.
    static HEALTH_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn health_env() -> std::sync::MutexGuard<'static, ()> {
        HEALTH_ENV.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn machine_health_path_env_none_disables() {
        let _env = health_env();
        std::env::set_var("CODEX_CONSULT_HEALTH", "none");
        assert_eq!(machine_health_path("C:/codex-home"), None);
        std::env::set_var("CODEX_CONSULT_HEALTH", "  NoNe  ");
        assert_eq!(machine_health_path("C:/codex-home"), None);
        std::env::remove_var("CODEX_CONSULT_HEALTH");
    }

    #[test]
    fn machine_health_path_env_overrides() {
        let _env = health_env();
        std::env::set_var("CODEX_CONSULT_HEALTH", "C:/somewhere/health.json");
        assert_eq!(
            machine_health_path("C:/codex-home"),
            Some(PathBuf::from("C:/somewhere/health.json"))
        );
        std::env::remove_var("CODEX_CONSULT_HEALTH");
    }

    #[test]
    fn machine_health_path_default_from_codex_home() {
        let _env = health_env();
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
        assert_eq!(ok, HealthUpdate::Written);
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

    // (wave 27c, D8) the bounded in-lock budget is one attempt of at most one second; the full
    // budget is three attempts. Both write when the lock is uncontended, and a real failure names
    // its cause (D7) rather than reporting a silent skip.
    #[test]
    fn bounded_and_full_budgets_and_named_failure() {
        assert_eq!(LockBudget::bounded().attempts, 1);
        assert!(LockBudget::bounded().attempt_secs <= 1.0);
        assert_eq!(LockBudget::full().attempts, MACHINE_LOCK_ATTEMPTS);

        let path = temp_path("bounded");
        let ok = add_machine_health_record_bounded(
            &path,
            "fp-bounded",
            "usable reply",
            None,
            "repo-a",
            &alive_true,
        );
        assert_eq!(ok, HealthUpdate::Written);
        assert!(!ok.failed());
        assert!(ok.cause().is_none());
        // A named failure carries a cause for the warning.
        let failed = HealthUpdate::LockTimeout;
        assert!(failed.failed());
        assert_eq!(failed.cause().as_deref(), Some("lock timeout"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn add_record_usable_outcome_is_ok_class() {
        let path = temp_path("ok");
        let ok =
            add_machine_health_record(&path, "fp2", "usable reply", None, "repo-a", &alive_true);
        assert_eq!(ok, HealthUpdate::Written);
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
            quota_mark: None,
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
            quota_mark: None,
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
            plan: String::new(),
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

    fn run_row(endpoint: &str, label: &str, pid: u32, panel: &str, plan: &str) -> MachineRunning {
        MachineRunning {
            endpoint: endpoint.into(),
            label: label.into(),
            pid,
            start_time: "2026-10-08T10:00:00.0000000Z".into(),
            repo: "C:\\repo-e".into(),
            task: "t".into(),
            nn: "01".into(),
            panel: panel.into(),
            since: "2026-10-08T12:00:00+02:00".into(),
            plan: plan.into(),
        }
    }

    // (wave 29b, E16) the running row carries the entry's plan - written only when there is one,
    // as the plugin's Register-MachineRunning adds it - and the plan's rows count for a panel of
    // another route, engine or repository.
    #[test]
    fn running_rows_carry_the_plan_and_count_by_plan() {
        let with = serde_json::to_value(run_row("fp-a", "ZAI", 11, "", "zai")).unwrap();
        assert_eq!(with["plan"], "zai");
        let keys: Vec<&str> = with
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(
            keys,
            vec![
                "endpoint",
                "label",
                "pid",
                "start_time",
                "repo",
                "task",
                "nn",
                "panel",
                "since",
                "plan"
            ]
        );
        let without = serde_json::to_value(run_row("fp-a", "ZAI", 11, "", "")).unwrap();
        assert!(without.get("plan").is_none(), "no plan key without a plan");
        let _env = health_env();
        let path = temp_path("plan-count");
        let seed = serde_json::json!({
            "health_version": 1,
            "endpoints": [],
            "running": [
                run_row("fp-a", "ZAI", 11, "", "zai"),
                run_row("fp-b", "ZAI-b", 12, "p1", "zai"),
                run_row("fp-c", "mimo", 13, "", "mimo"),
                run_row("fp-d", "openai", 14, "", ""),
            ],
        });
        std::fs::write(&path, serde_json::to_string_pretty(&seed).unwrap()).unwrap();
        // a ZAI-b group (endpoint fp-b, plan zai) of panel p2: its endpoint row and the plan's
        let c = machine_running_count(&path, &["fp-b".into()], "p2", &["zai".into()], &alive_true);
        let pids: Vec<u32> = c.rows.iter().map(|r| r.pid).collect();
        assert_eq!(pids, vec![11, 12]);
        let by_plan: Vec<u32> = c.by_plan.iter().map(|r| r.pid).collect();
        assert_eq!(
            by_plan,
            vec![11],
            "fp-b's own row counts through its endpoint"
        );
        // the panel's own rows never count; dead rows never count
        let c = machine_running_count(&path, &["fp-b".into()], "p1", &["zai".into()], &alive_true);
        assert_eq!(c.rows.iter().map(|r| r.pid).collect::<Vec<_>>(), vec![11]);
        let c = machine_running_count(&path, &["fp-b".into()], "p2", &["zai".into()], &alive_false);
        assert!(c.rows.is_empty());
        // without plans: the endpoint alone (the wave 26b rule)
        let c = machine_running_count(&path, &["fp-a".into()], "", &[], &alive_true);
        assert_eq!(c.rows.iter().map(|r| r.pid).collect::<Vec<_>>(), vec![11]);
        assert!(c.by_plan.is_empty());
        // a group with only a plan (its identities unresolved)
        let c = machine_running_count(&path, &[], "", &["mimo".into()], &alive_true);
        assert_eq!(
            c.by_plan.iter().map(|r| r.pid).collect::<Vec<_>>(),
            vec![13]
        );
        // a register keeps the plan of another run's row when it rewrites the file
        assert!(register_machine_running(
            &path,
            run_row("fp-e", "x", 15, "", ""),
            &alive_true
        ));
        let mh = read_machine_health(&path);
        assert_eq!(mh.running.len(), 5);
        assert_eq!(mh.running[0].plan, "zai");
        assert!(mh.running[4].plan.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    // (wave 29b, E15) an ok record's quota_mark (written by the plugin's claude engine) survives
    // C3's rewrite of the file, keeps the record while it blocks, and is read as a quota failure.
    #[test]
    fn a_quota_mark_survives_a_rewrite_and_is_read() {
        let _env = health_env();
        let path = temp_path("quota-mark");
        let now = Utc::now();
        let old_when =
            (now - Duration::hours(30)).with_timezone(&FixedOffset::east_opt(7200).unwrap());
        let mark_until =
            (now + Duration::hours(2)).with_timezone(&FixedOffset::east_opt(7200).unwrap());
        let seed = serde_json::json!({
            "health_version": 1,
            "endpoints": [{
                "endpoint": "fp-claude", "class": "ok", "kind": "", "until": null,
                "retry_after": null, "repo": "C:\\other", "when": format_offset_iso(old_when),
                "message": "",
                "quota_mark": {
                    "class": "quota", "kind": "", "until": format_offset_iso(mark_until),
                    "retry_after": format_offset_iso(mark_until), "message": "rejected",
                    "when": format_offset_iso(old_when)
                }
            }],
            "running": [],
        });
        std::fs::write(&path, serde_json::to_string_pretty(&seed).unwrap()).unwrap();
        // an update (any) rewrites the file: the 30-hour-old ok record stays (its mark blocks)
        assert!(unregister_machine_running(&path, 0, &alive_true));
        let mh = read_machine_health(&path);
        assert_eq!(mh.endpoints.len(), 1);
        let mk = mh.endpoints[0]
            .quota_mark
            .as_ref()
            .expect("the mark is kept");
        assert_eq!(mk["until"], format_offset_iso(mark_until));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"quota_mark\""));
        // read: the route is out until the mark's reset
        let consults = machine_endpoint_consults(&path, "fp-claude");
        let h = endpoint_health(&consults, "fp-claude", now);
        let q = h.quota.expect("the mark reads as a quota failure");
        assert_eq!(
            q.retry_after.map(format_offset_iso),
            Some(format_offset_iso(mark_until))
        );
        assert!(h.quota_known);
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod retry_after_tests {
    use super::*;

    /// Berlin by its IANA id (the plugin's harness: `W. Europe Standard Time` / `Europe/Berlin`).
    fn berlin() -> chrono_tz::Tz {
        "Europe/Berlin".parse().expect("the Europe/Berlin zone")
    }

    fn dto(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    /// EVERY sample of the plugin's `harness-roster.ps1` UNIT `Get-RetryAfter` check (v0.6.1, 69
    /// samples): (name, message, reference, expected ISO or "" for none, read time). A write-time
    /// sample reads in Berlin, a read-time one (`-ReferenceOffset`) in the reference's offset.
    const SAMPLES: &[(&str, &str, &str, &str, bool)] = &[
        ("Codex wording, curly apostrophe", "You\u{2019}ve hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at Sep 28th, 2026 8:35 PM.", "2026-09-24T12:54:23.000+02:00", "2026-09-28T20:35:00+02:00", false),
        ("read time (-ReferenceOffset): Codex wording read in the reference offset (-05:00)", "You\u{2019}ve hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at Sep 28th, 2026 8:35 PM.", "2026-09-24T12:54:23.000-05:00", "2026-09-28T20:35:00-05:00", true),
        ("DST: Berlin failure 2026-10-25T01:00+02:00, reset \"Oct 26th, 2026 8:35 PM\" -> +01:00 (19:35Z)", "You\u{2019}ve hit your usage limit ... try again at Oct 26th, 2026 8:35 PM.", "2026-10-25T01:00:00.000+02:00", "2026-10-26T20:35:00+01:00", false),
        ("DST, read time (-ReferenceOffset): the reference offset +02:00 (the documented fallback)", "try again at Oct 26th, 2026 8:35 PM.", "2026-10-25T01:00:00.000+02:00", "2026-10-26T20:35:00+02:00", true),
        ("DST gap: 2026-03-29 02:30 does not exist in Berlin -> the offset after (+02:00)", "try again at Mar 29th, 2026 2:30 AM", "2026-03-28T12:00:00.000+01:00", "2026-03-29T02:30:00+02:00", false),
        ("DST overlap: 2026-10-25 02:30 is ambiguous in Berlin -> the offset before (+02:00)", "try again at Oct 25th, 2026 2:30 AM", "2026-10-24T12:00:00.000+02:00", "2026-10-25T02:30:00+02:00", false),
        ("a duration across the DST change is shown in the zone offset then (same instant)", "try again in 2 days", "2026-10-24T12:00:00.000+02:00", "2026-10-26T11:00:00+01:00", false),
        ("straight apostrophe, full month, no ordinal", "You've hit your usage limit ... try again at September 28, 2026 8:35 PM", "2026-09-24T12:54:23.000+02:00", "2026-09-28T20:35:00+02:00", false),
        ("resets at, 12 AM = midnight", "Limit reached; resets at Oct 1st, 2026 12:05 AM.", "2026-09-24T12:54:23.000+02:00", "2026-10-01T00:05:00+02:00", false),
        ("until + 24-hour clock", "blocked until Sep 30, 2026 17:45", "2026-09-24T12:54:23.000+02:00", "2026-09-30T17:45:00+02:00", false),
        ("missing year -> the reference year", "try again at Sep 28th 8:35 PM.", "2026-09-24T12:54:23.000+02:00", "2026-09-28T20:35:00+02:00", false),
        ("missing year, date already past -> next year", "try again at Jan 3rd, 9:00 AM", "2026-12-30T10:00:00.000+01:00", "2027-01-03T09:00:00+01:00", false),
        ("ISO after \"until\" keeps its own offset", "rate limited until 2026-09-25T08:00:00Z", "2026-09-24T12:54:23.000+02:00", "2026-09-25T10:00:00+02:00", false),
        ("ISO without offset -> a wall-clock time of the zone", "Retry at 2026-09-25 08:00:00 please", "2026-09-24T12:54:23.000+02:00", "2026-09-25T08:00:00+02:00", false),
        ("Retry after 30 seconds", "Rate limit exceeded. Retry after 30 seconds.", "2026-09-24T12:54:23.000+02:00", "2026-09-24T12:54:53+02:00", false),
        ("Retry-After: 120 (no unit = seconds)", "429 Too Many Requests; Retry-After: 120", "2026-09-24T12:54:23.000+02:00", "2026-09-24T12:56:23+02:00", false),
        ("try again in 5 minutes", "Please try again in 5 minutes.", "2026-09-24T12:54:23.000+02:00", "2026-09-24T12:59:23+02:00", false),
        ("resets in 1 hour 30 minutes", "quota resets in 1 hour 30 minutes", "2026-09-24T12:54:23.000+02:00", "2026-09-24T14:24:23+02:00", false),
        ("nothing -> null", "the model produced nothing", "2026-09-24T12:54:23.000+02:00", "", false),
        ("mixed days + hours + minutes are summed", "You've hit your usage limit. Upgrade to Pro or try again in 3 days 1 hour 7 minutes.", "2026-09-24T12:54:23.000+02:00", "2026-09-27T14:01:23+02:00", false),
        ("resets in 2 days", "Your quota resets in 2 days.", "2026-09-24T12:54:23.000+02:00", "2026-09-26T12:54:23+02:00", false),
        ("retry after 1 week", "Plan exhausted - retry after 1 week", "2026-09-24T12:54:23.000+02:00", "2026-10-01T12:54:23+02:00", false),
        ("try again in 2 weeks, 1 day", "try again in 2 weeks, 1 day", "2026-09-24T12:54:23.000+02:00", "2026-10-09T12:54:23+02:00", false),
        ("no reset time named -> null", "You've hit your usage limit. Upgrade to Pro or try again later.", "2026-09-24T12:54:23.000+02:00", "", false),
        ("time only, still ahead today -> today", "You\u{2019}ve hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at 9:43 PM.", "2026-10-07T21:00:00.000+02:00", "2026-10-07T21:43:00+02:00", false),
        ("time only, already past -> tomorrow (the day rollover)", "You\u{2019}ve hit your usage limit. ... or try again at 9:43 PM.", "2026-10-07T22:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F06-3 time only, 24-hour clock, parsed 0.5 s after the minute -> today (passed)", "try again at 21:43", "2026-10-07T21:43:00.500+02:00", "2026-10-07T21:43:00+02:00", false),
        ("F06-3 time only, 24-hour clock, 30 s past -> today (passed)", "try again at 21:43", "2026-10-07T21:43:30.000+02:00", "2026-10-07T21:43:00+02:00", false),
        ("F06-3 time only, 9:43 PM, 4 min past -> today (passed)", "try again at 9:43 PM.", "2026-10-07T21:47:00.000+02:00", "2026-10-07T21:43:00+02:00", false),
        ("F06-3 time only, 6 min past -> tomorrow", "try again at 21:43", "2026-10-07T21:49:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F06-3 time only, 23:59 parsed at 00:02 -> yesterday's (passed 3 min ago), not tonight's", "try again at 23:59", "2026-10-08T00:02:00.000+02:00", "2026-10-07T23:59:00+02:00", false),
        ("F06-4 Berlin fall-back 2026-10-25, reference the SECOND 02:15 (+01:00), \"2:30 AM\" -> the second 02:30 that day (+01:00), 15 min away", "try again at 2:30 AM", "2026-10-25T02:15:00.000+01:00", "2026-10-25T02:30:00+01:00", false),
        ("F06-4 Berlin fall-back, reference the FIRST 02:15 (+02:00), \"2:30 AM\" -> the first 02:30 (+02:00)", "try again at 2:30 AM", "2026-10-25T02:15:00.000+02:00", "2026-10-25T02:30:00+02:00", false),
        ("F06-4 Berlin spring-forward 2026-03-29, reference 01:50 (+01:00), \"2:30 AM\" (inside the gap) -> 03:00 (+02:00)", "try again at 2:30 AM", "2026-03-29T01:50:00.000+01:00", "2026-03-29T03:00:00+02:00", false),
        ("F06-5 \"resets at 21:43 UTC\" -> 21:43Z (23:43+02:00), not 21:43 local", "Limit reached; resets at 21:43 UTC.", "2026-10-08T21:00:00.000+02:00", "2026-10-08T23:43:00+02:00", false),
        ("F06-5 \"9:43 PM GMT.\" -> 21:43Z", "try again at 9:43 PM GMT.", "2026-10-08T21:00:00.000+02:00", "2026-10-08T23:43:00+02:00", false),
        ("F06-5 \"21:43Z\" -> 21:43Z", "try again at 21:43Z", "2026-10-08T21:00:00.000+02:00", "2026-10-08T23:43:00+02:00", false),
        ("F06-5 \"23:30 UTC\" parsed at 01:00+02:00 (23:00Z) -> the UTC day's 23:30Z, 30 min away", "resets at 23:30 UTC", "2026-10-08T01:00:00.000+02:00", "2026-10-08T01:30:00+02:00", false),
        ("F06-5 \"21:43 +02:00\" -> that offset", "try again at 21:43 +02:00", "2026-10-08T21:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F06-5 \"21:43 -0500\" -> 02:43Z next day (04:43+02:00)", "until 21:43 -0500", "2026-10-08T21:00:00.000+02:00", "2026-10-09T04:43:00+02:00", false),
        ("F06-5 \"23:43 +2\" -> that offset", "available at 23:43 +2", "2026-10-08T21:00:00.000+02:00", "2026-10-08T23:43:00+02:00", false),
        ("F06-5 \"9:43 PM PST\" -> null (another zone word: not parsed)", "try again at 9:43 PM PST", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F06-5 \"21:43 CET.\" -> null", "until 21:43 CET.", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F06-5 \"9:43 PM PDT\" -> null", "try again at 9:43 PM PDT", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F06-5 \"21:43 (BST)\" -> null", "resets at 21:43 (BST)", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F06-5 a trailing comma, \"and\", a period and the end stay fine (local)", "try again at 21:43, or upgrade", "2026-10-08T21:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F06-5 \"... 21:43 and ...\" -> local", "try again at 21:43 and retry", "2026-10-08T21:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F06-5 \"9:43 PM. API keys ...\" - the period ends the sentence -> local", "try again at 9:43 PM. API keys are not affected.", "2026-10-08T21:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F08-2 \"21:43 UTC+05:3\" (an incomplete minute) -> null, not UTC+05", "resets at 21:43 UTC+05:3", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F08-2 \"21:43 +02:000\" (excess digits) -> null, not +02", "resets at 21:43 +02:000", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F08-2 \"21:43 UTC+oops\" (letters after the sign) -> null, not UTC", "resets at 21:43 UTC+oops", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F08-2 \"21:43 UTC+\" (a dangling sign) -> null", "try again at 21:43 UTC+", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F08-2 \"21:43 +15:00\" (beyond 14 hours) -> null", "until 21:43 +15:00", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F08-2 \"21:43 +02:60\" (minutes of 60) -> null", "until 21:43 +02:60", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F08-2 \"21:43 +020\" (three digits) -> null", "until 21:43 +020", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F08-2 \"21:43 Z+02\" (Z takes no offset) -> null", "try again at 21:43 Z+02", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F08-2 \"21:43 + 2\" (a spaced sign before a number) -> null", "try again at 21:43 + 2", "2026-10-08T21:00:00.000+02:00", "", false),
        ("F08-2 \"21:43 UTC+05:30.\" (a complete offset, the sentence's period) -> 16:13Z next day", "try again at 21:43 UTC+05:30.", "2026-10-08T21:00:00.000+02:00", "2026-10-09T18:13:00+02:00", false),
        ("F08-2 \"21:43 +0200\" -> that offset", "until 21:43 +0200", "2026-10-08T21:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F08-2 \"21:43 +02\" -> that offset", "until 21:43 +02", "2026-10-08T21:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F08-2 \"21:43 -05:30\" -> 03:13Z next day (05:13+02:00)", "until 21:43 -05:30", "2026-10-08T21:00:00.000+02:00", "2026-10-09T05:13:00+02:00", false),
        ("F08-2 \"21:43 UTC +02:00\" (the offset after a space) -> that offset", "resets at 21:43 UTC +02:00", "2026-10-08T21:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F08-2 \"21:43 (GMT+2)\" -> that offset", "resets at 21:43 (GMT+2)", "2026-10-08T21:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("F08-2 \"21:43 UTC!\" -> 21:43Z", "resets at 21:43 UTC!", "2026-10-08T21:00:00.000+02:00", "2026-10-08T23:43:00+02:00", false),
        ("F08-2 \"21:43 - or upgrade\" (a dash, no qualifier) -> local", "try again at 21:43 - or upgrade", "2026-10-08T21:00:00.000+02:00", "2026-10-08T21:43:00+02:00", false),
        ("time only, 12:05 AM after 23:00 -> tomorrow 00:05", "Limit reached; resets at 12:05 AM.", "2026-10-07T23:00:00.000+02:00", "2026-10-08T00:05:00+02:00", false),
        ("time only, tomorrow across the DST change -> the offset then (+01:00)", "try again at 3:30 AM", "2026-10-24T23:00:00.000+02:00", "2026-10-25T03:30:00+01:00", false),
        ("time only, read time (-ReferenceOffset): the reference's day and offset, rolled over", "try again at 9:43 PM.", "2026-10-07T22:00:00.000-05:00", "2026-10-08T21:43:00-05:00", true),
        ("time only, not a clock time (13:43 PM) -> null", "try again at 13:43 PM", "2026-09-24T12:54:23.000+02:00", "", false),
    ];

    #[test]
    fn get_retry_after_plugin_unit_samples() {
        assert_eq!(SAMPLES.len(), 69);
        let zone = berlin();
        let mut bad: Vec<String> = Vec::new();
        for (name, msg, reference, want, ref_offset) in SAMPLES {
            let r = dto(reference);
            let got = if *ref_offset {
                retry_after_ref(msg, r)
            } else {
                retry_after_in(msg, r, &zone)
            };
            let got = got.map(format_offset_iso).unwrap_or_default();
            if got != *want {
                bad.push(format!("{name}: got '{got}', want '{want}'"));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    /// The harness-roster TIMEONLY section's three recorded resets (parsed at the consult clock,
    /// in the machine's zone there - Berlin here): 21:00 -> today 21:43; 22:00 -> tomorrow 21:43
    /// (the day rollover); 21:43:30 -> today 21:43 (F06-3: passed, the hold ends at once).
    #[test]
    fn timeonly_section_resets() {
        let zone = berlin();
        let msg = "You\u{2019}ve hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at 9:43 PM.";
        let cases = [
            ("2026-10-07T21:00:00+02:00", "2026-10-07T21:43:00+02:00"),
            ("2026-10-07T22:00:00+02:00", "2026-10-08T21:43:00+02:00"),
            ("2026-10-07T21:43:30+02:00", "2026-10-07T21:43:00+02:00"),
        ];
        for (reference, want) in cases {
            let got = retry_after_in(msg, dto(reference), &zone).map(format_offset_iso);
            assert_eq!(got.as_deref(), Some(want), "parsed at {reference}");
        }
        // the hold: at 21:40 the next day the reset (tomorrow 21:43) is still ahead, at 21:44 past
        let until = dto("2026-10-08T21:43:00+02:00");
        assert!(dto("2026-10-08T21:40:00+02:00") < until);
        assert!(dto("2026-10-08T21:44:00+02:00") > until);
    }

    #[test]
    fn time_only_qualifier_tokens() {
        let off = |h: i32, m: i32| FixedOffset::east_opt(h * 3600 + h.signum() * m * 60);
        let cases: [(&str, &str, Option<FixedOffset>, bool); 12] = [
            ("21:43", " UTC.", off(0, 0), false),
            ("21:43", " (GMT+2)", off(2, 0), false),
            ("21:43", " UTC +02:00", off(2, 0), false),
            ("21:43", " -05:30", off(-5, 30), false),
            ("21:43", " +0200", off(2, 0), false),
            ("21:43", " Z+02", None, true),
            ("21:43", " + 2", None, true),
            ("21:43", " - or upgrade", None, false),
            ("21:43", " UTC + 2", None, true),
            ("9:43 PM.", " PST", None, false),
            ("9:43 p.m.", " PST", None, true),
            ("21:43", " AND more", None, false),
        ];
        for (clock, rest, offset, declined) in cases {
            let z = time_only_zone(clock, rest);
            assert_eq!((z.offset, z.declined), (offset, declined), "{clock}{rest}");
        }
    }

    #[test]
    fn dated_wording_needs_a_word_boundary_after_the_day() {
        // "until jun 21:43" is no month-name date (the plugin's \b after the day): never Jun 2 1:43
        let r = dto("2026-10-08T21:00:00+02:00");
        assert_eq!(
            retry_after_ref("blocked until jun 21:43", r).map(format_offset_iso),
            None
        );
    }

    // ---- (F04-2) the exact edges of a transition, normalised against a UTC round trip ----

    /// (F04-2) The exact edges of Berlin's 2026 transitions: (name, message, reference, expected).
    /// The expected instants are the plugin's own (`Get-RetryAfter -TimeZone` with `W. Europe
    /// Standard Time`, v0.6.1, run on .NET): 03:00 on the fall-back day is +01:00 only (the
    /// repeated hour is 02:00..02:59), 02:00 on the spring-forward day does not exist (dated: the
    /// offset after the transition; time-only: the first valid instant after the gap).
    const BOUNDARIES: &[(&str, &str, &str, &str)] = &[
        ("autumn time-only 03:00 at the second 02:00 -> 03:00+01:00, one hour after the reference", "try again at 3:00 AM", "2026-10-25T02:00:00+01:00", "2026-10-25T03:00:00+01:00"),
        ("autumn dated 03:00 -> +01:00", "try again at Oct 25th, 2026 3:00 AM", "2026-10-24T12:00:00+02:00", "2026-10-25T03:00:00+01:00"),
        ("spring dated 02:00 (the gap's start) -> the offset after the transition, +02:00", "try again at Mar 29th, 2026 2:00 AM", "2026-03-28T12:00:00+01:00", "2026-03-29T02:00:00+02:00"),
        ("spring time-only 02:00 (the gap's start) -> the first valid instant after the gap", "try again at 2:00 AM", "2026-03-29T01:50:00+01:00", "2026-03-29T03:00:00+02:00"),
        ("autumn dated 02:00 (the repeated hour's start) -> the offset before, +02:00", "try again at Oct 25th, 2026 2:00 AM", "2026-10-24T12:00:00+02:00", "2026-10-25T02:00:00+02:00"),
        ("spring time-only 03:00 (the gap's end) -> 03:00+02:00", "try again at 3:00 AM", "2026-03-29T01:50:00+01:00", "2026-03-29T03:00:00+02:00"),
    ];

    fn boundary_failures(zone: &dyn ResetZone) -> Vec<String> {
        BOUNDARIES
            .iter()
            .filter_map(|(name, msg, reference, want)| {
                let got = retry_after_in(msg, dto(reference), zone)
                    .map(format_offset_iso)
                    .unwrap_or_default();
                (got != *want).then(|| format!("{name}: got '{got}', want '{want}'"))
            })
            .collect()
    }

    fn wall(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    fn hours(h: i32) -> FixedOffset {
        FixedOffset::east_opt(h * 3600).unwrap()
    }

    /// Berlin 2026 as chrono's Windows `Local` classifies it (chrono 0.4.45
    /// `lookup_with_dst_transitions`): the fold's wall-time range is closed at BOTH ends (03:00 on
    /// the fall-back day ambiguous), the gap's start a single wall time at the earlier offset
    /// (02:00 on the spring-forward day +01:00); its UTC-to-local offsets are right. So the F04-2
    /// normalisation is proven on every machine, not only on a Berlin Windows one.
    struct WindowsLikeBerlin2026;

    impl ResetZone for WindowsLikeBerlin2026 {
        fn reported_wall_offsets(&self, w: NaiveDateTime) -> LocalResult<FixedOffset> {
            if w <= wall("2026-03-29T02:00:00") {
                LocalResult::Single(hours(1))
            } else if w < wall("2026-03-29T03:00:00") {
                LocalResult::None
            } else if w < wall("2026-10-25T02:00:00") {
                LocalResult::Single(hours(2))
            } else if w <= wall("2026-10-25T03:00:00") {
                LocalResult::Ambiguous(hours(2), hours(1))
            } else {
                LocalResult::Single(hours(1))
            }
        }
        fn utc_offset(&self, utc: NaiveDateTime) -> FixedOffset {
            if utc >= wall("2026-03-29T01:00:00") && utc < wall("2026-10-25T01:00:00") {
                hours(2)
            } else {
                hours(1)
            }
        }
    }

    #[test]
    fn wall_offsets_are_normalised_against_a_utc_round_trip() {
        let z = WindowsLikeBerlin2026;
        // the raw classification is the faulty one ...
        assert_eq!(
            z.reported_wall_offsets(wall("2026-10-25T03:00:00")),
            LocalResult::Ambiguous(hours(2), hours(1))
        );
        assert_eq!(
            z.reported_wall_offsets(wall("2026-03-29T02:00:00")),
            LocalResult::Single(hours(1))
        );
        // ... the normalised one is the zone's: false ambiguity dropped, the boundary gap
        // recognised, the genuine repeated hour kept (earlier instant first)
        let cases = [
            ("2026-10-25T01:59:00", LocalResult::Single(hours(2))),
            (
                "2026-10-25T02:00:00",
                LocalResult::Ambiguous(hours(2), hours(1)),
            ),
            (
                "2026-10-25T02:59:00",
                LocalResult::Ambiguous(hours(2), hours(1)),
            ),
            ("2026-10-25T03:00:00", LocalResult::Single(hours(1))),
            ("2026-03-29T01:59:00", LocalResult::Single(hours(1))),
            ("2026-03-29T02:00:00", LocalResult::None),
            ("2026-03-29T02:59:00", LocalResult::None),
            ("2026-03-29T03:00:00", LocalResult::Single(hours(2))),
        ];
        for (w, want) in cases {
            assert_eq!(z.wall_offsets(wall(w)), want, "{w} (Windows-like)");
            assert_eq!(berlin().wall_offsets(wall(w)), want, "{w} (Europe/Berlin)");
        }
    }

    /// (F06-1) Windows' `Samoa Standard Time` around New Year 2012, as chrono's Windows `Local`
    /// reads it: one rule set per year (the registry's Dynamic DST: 2011 Bias 660, DaylightBias -60,
    /// standard from the first Saturday of April 04:00, daylight from the fourth Saturday of
    /// September 03:00; 2012 Bias -780, DaylightBias -60, the first Sunday of April 04:00, the last
    /// Sunday of September 03:00). The wall-time classification takes the rules of the wall time's
    /// own (local) year, the UTC-to-local lookup the rules of the UTC year (chrono 0.4.45
    /// `offset_from_utc_datetime`, as `SystemTimeToTzSpecificLocalTime`) - so 2012-01-01 00:00
    /// (+14:00) has its instant 2011-12-31T10:00Z looked up with the 2011 rules (-10:00).
    struct WindowsLikeSamoa2012;

    /// One year's rules: (standard, daylight, the DST->standard transition as a daylight wall time,
    /// the standard->DST transition as a standard wall time). Southern hemisphere: daylight before
    /// the first and from the second.
    fn samoa_rules(year: i32) -> (FixedOffset, FixedOffset, NaiveDateTime, NaiveDateTime) {
        if year <= 2011 {
            (
                hours(-11),
                hours(-10),
                wall(&format!("{year}-04-02T04:00:00")),
                wall(&format!("{year}-09-24T03:00:00")),
            )
        } else {
            (
                hours(13),
                hours(14),
                wall(&format!("{year}-04-01T04:00:00")),
                wall(&format!("{year}-09-30T03:00:00")),
            )
        }
    }

    impl ResetZone for WindowsLikeSamoa2012 {
        fn reported_wall_offsets(&self, w: NaiveDateTime) -> LocalResult<FixedOffset> {
            let (std, dst, to_std, to_dst) = samoa_rules(w.year());
            if w < to_std - Duration::hours(1) {
                LocalResult::Single(dst)
            } else if w <= to_std {
                LocalResult::Ambiguous(dst, std)
            } else if w <= to_dst {
                LocalResult::Single(std)
            } else if w < to_dst + Duration::hours(1) {
                LocalResult::None
            } else {
                LocalResult::Single(dst)
            }
        }
        fn utc_offset(&self, utc: NaiveDateTime) -> FixedOffset {
            let (std, dst, to_std, to_dst) = samoa_rules(utc.year());
            let to_std_utc = to_std - Duration::seconds(dst.local_minus_utc() as i64);
            let to_dst_utc = to_dst - Duration::seconds(std.local_minus_utc() as i64);
            if utc >= to_std_utc && utc < to_dst_utc {
                std
            } else {
                dst
            }
        }
    }

    #[test]
    fn samoa_year_boundary_keeps_the_local_years_offset() {
        let z = WindowsLikeSamoa2012;
        // the two raw lookups disagree across the year boundary ...
        assert_eq!(
            z.reported_wall_offsets(wall("2012-01-01T00:00:00")),
            LocalResult::Single(hours(14))
        );
        assert_eq!(z.utc_offset(wall("2011-12-31T10:00:00")), hours(-10));
        assert_eq!(z.utc_offset(wall("2012-01-01T10:00:00")), hours(14));
        // ... the valid midnight keeps +14:00 (.NET: valid and unambiguous at +14:00), the last
        // local hours of 2011 keep -10:00, the days around stay single
        let cases = [
            ("2012-01-01T00:00:00", LocalResult::Single(hours(14))),
            ("2012-01-01T05:00:00", LocalResult::Single(hours(14))),
            ("2012-01-01T14:00:00", LocalResult::Single(hours(14))),
            ("2012-01-02T00:00:00", LocalResult::Single(hours(14))),
            ("2011-12-31T00:00:00", LocalResult::Single(hours(-10))),
            ("2011-12-31T20:00:00", LocalResult::Single(hours(-10))),
        ];
        for (w, want) in cases {
            assert_eq!(z.wall_offsets(wall(w)), want, "{w} (Windows-like Samoa)");
        }
        // the trigger: the pinned plugin's result (v0.6.1 `Get-RetryAfter -TimeZone` with
        // `Samoa Standard Time`): tomorrow's midnight, not an artificial gap's end (14:00, passed)
        let got = retry_after_in(
            "try again at 12:00 AM",
            dto("2012-01-01T14:01:00+14:00"),
            &z,
        )
        .map(format_offset_iso);
        assert_eq!(got.as_deref(), Some("2012-01-02T00:00:00+14:00"));
        // the zone database agrees (Pacific/Apia skipped 2011-12-30 instead)
        let apia: chrono_tz::Tz = "Pacific/Apia".parse().expect("the Pacific/Apia zone");
        let got = retry_after_in(
            "try again at 12:00 AM",
            dto("2012-01-01T14:01:00+14:00"),
            &apia,
        )
        .map(format_offset_iso);
        assert_eq!(got.as_deref(), Some("2012-01-02T00:00:00+14:00"));
        // and the Berlin edges (F04-2) are untouched by the cross-year rule
        assert!(boundary_failures(&WindowsLikeBerlin2026).is_empty());
    }

    #[test]
    fn reset_boundaries_follow_the_plugin_in_every_zone_implementation() {
        let bad = boundary_failures(&berlin());
        assert!(bad.is_empty(), "Europe/Berlin:\n{}", bad.join("\n"));
        let bad = boundary_failures(&WindowsLikeBerlin2026);
        assert!(bad.is_empty(), "Windows-like Berlin:\n{}", bad.join("\n"));
    }

    /// (F04-2, RC2) The production zone: `chrono::Local` on a Windows machine whose zone is
    /// `W. Europe Standard Time`. Any other machine zone skips the test (it says so): the zone is
    /// recognised by its 2026 offsets (+01:00 in winter, +02:00 in summer) and transition instants
    /// (01:00Z on 2026-03-29 and 2026-10-25), which every zone with Berlin's rules shares.
    #[test]
    #[cfg(windows)]
    fn windows_local_reset_boundaries() {
        let local = chrono::Local;
        let off = |utc: &str| {
            ResetZone::utc_offset(&local, dto(utc).naive_utc()).local_minus_utc() / 3600
        };
        let berlin_rules = [
            ("2026-01-15T12:00:00Z", 1),
            ("2026-07-15T12:00:00Z", 2),
            ("2026-03-29T00:59:59Z", 1),
            ("2026-03-29T01:00:00Z", 2),
            ("2026-10-25T00:59:59Z", 2),
            ("2026-10-25T01:00:00Z", 1),
        ]
        .iter()
        .all(|(utc, h)| off(utc) == *h);
        if !berlin_rules {
            println!(
                "windows_local_reset_boundaries SKIPPED: the machine zone is not W. Europe Standard Time (offsets {:+} / {:+} in winter / summer 2026)",
                off("2026-01-15T12:00:00Z"),
                off("2026-07-15T12:00:00Z")
            );
            return;
        }
        println!(
            "windows_local_reset_boundaries: the machine zone has W. Europe Standard Time's 2026 rules - production chrono::Local"
        );
        for w in ["2026-10-25T03:00:00", "2026-03-29T02:00:00"] {
            println!(
                "  {w}: chrono reports {:?}, normalised {:?}",
                local.reported_wall_offsets(wall(w)),
                local.wall_offsets(wall(w))
            );
        }
        for (name, msg, reference, want) in BOUNDARIES {
            let got = retry_after_in(msg, dto(reference), &local).map(format_offset_iso);
            println!("  {name}: {msg} @ {reference} -> {got:?}");
            assert_eq!(got.as_deref(), Some(*want), "{name}");
        }
        assert_eq!(
            local.wall_offsets(wall("2026-10-25T03:00:00")),
            LocalResult::Single(hours(1))
        );
        assert_eq!(
            local.wall_offsets(wall("2026-03-29T02:00:00")),
            LocalResult::None
        );
        assert_eq!(
            local.wall_offsets(wall("2026-10-25T02:30:00")),
            LocalResult::Ambiguous(hours(2), hours(1))
        );
    }
}
