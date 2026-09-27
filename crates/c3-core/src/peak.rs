//! Peak-window evaluation (`ConvertFrom-PeakSpec` / `ConvertFrom-PeakExceptions` /
//! `Get-PeakStatus` / `Get-ConsultClock` / `Get-PeakStatusNow`,
//! `codex-consult-common.ps1:2819-3013`).
//!
//! `CODEX_CONSULT_PEAK_<PROVIDER>` declares a weekly peak window
//! (`"<days> <HH:MM>-<HH:MM> <+HH:MM|-HH:MM>"`); `_EXCEPT` lists calendar dates that are
//! off-peak all day. The window is evaluated ONCE at launch in the declared offset (start
//! inclusive, end exclusive; an overnight window spans midnight and its day check applies to
//! the day it started). `peak = Some(true)` inside, `Some(false)` outside/exception,
//! `None` = no schedule (unknown). The provider name is upper-cased with every non-`A-Z0-9`
//! character replaced by `_`. The test hook `CODEX_CONSULT_NOW` supplies one or more ISO
//! timestamps (comma-separated); the k-th evaluation of a run uses the k-th (the last
//! repeats), so a test can place the early check off-peak and the launch check inside.

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, Timelike, Utc};

/// A parsed peak window.
#[derive(Debug, Clone)]
pub struct PeakSpec {
    /// Indexed by day-of-week, Sunday = 0 (`[int][DayOfWeek]`).
    pub days: [bool; 7],
    /// Start-of-window minutes-of-day (inclusive).
    pub start: i32,
    /// End-of-window minutes-of-day (exclusive; `24:00` = 1440).
    pub end: i32,
    /// Offset in whole minutes east of UTC (may be negative).
    pub offset_minutes: i32,
}

/// The evaluated peak status (`Get-PeakStatus` + `EvaluatedAt`).
#[derive(Debug, Clone, Default)]
pub struct PeakStatus {
    /// `Some(true)` inside, `Some(false)` outside/exception, `None` no schedule.
    pub peak: Option<bool>,
    pub schedule: String,
    /// `none` | `env` | `env (CODEX_CONSULT_NOW)`.
    pub source: String,
    pub variable: String,
    pub local: String,
    pub detail: String,
    pub error: String,
    /// The clock's ISO reading (`peak_evaluated_at`).
    pub evaluated_at: String,
}

const DAY_NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/// `Get-PeakVariableName`.
pub fn peak_variable_name(provider: &str) -> String {
    let mut s = String::from("CODEX_CONSULT_PEAK_");
    for c in provider.to_uppercase().chars() {
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            s.push(c);
        } else {
            s.push('_');
        }
    }
    s
}

fn day_index(abbr: &str) -> Option<usize> {
    let lc = abbr.to_lowercase();
    DAY_NAMES.iter().position(|d| *d == lc)
}

/// `ConvertFrom-PeakSpec`.
pub fn parse_peak_spec(spec: &str, var: &str) -> Result<PeakSpec, String> {
    let format =
        "expected '<days> <HH:MM>-<HH:MM> <+HH:MM|-HH:MM>', e.g. 'Mon-Fri 14:00-18:00 +08:00'";
    let trimmed = spec.trim();
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.len() != 3 {
        return Err(format!(
            "{var}='{spec}' is malformed: bad token '{trimmed}' ({} field(s) instead of 3); {format}",
            tokens.len()
        ));
    }
    let day_help =
        "day names are Mon Tue Wed Thu Fri Sat Sun, a range Mon-Fri, a list Mon,Wed or *";
    let mut days = [false; 7];
    if tokens[0] == "*" {
        days = [true; 7];
    } else {
        for item in tokens[0].split(',') {
            if let Some((a, b)) = item.split_once('-') {
                if a.len() == 3
                    && b.len() == 3
                    && a.chars().all(|c| c.is_ascii_alphabetic())
                    && b.chars().all(|c| c.is_ascii_alphabetic())
                {
                    match (day_index(a), day_index(b)) {
                        (Some(ai), Some(bi)) => {
                            let mut d = ai;
                            loop {
                                days[d] = true;
                                if d == bi {
                                    break;
                                }
                                d = (d + 1) % 7;
                            }
                            continue;
                        }
                        _ => {
                            return Err(format!(
                                "{var}='{spec}' is malformed: bad token '{item}' ({day_help}); {format}"
                            ))
                        }
                    }
                } else {
                    return Err(format!(
                        "{var}='{spec}' is malformed: bad token '{item}' ({day_help}); {format}"
                    ));
                }
            }
            let one = if item.len() == 3 && item.chars().all(|c| c.is_ascii_alphabetic()) {
                day_index(item)
            } else {
                None
            };
            match one {
                Some(i) => days[i] = true,
                None => {
                    return Err(format!(
                        "{var}='{spec}' is malformed: bad token '{item}' ({day_help}); {format}"
                    ))
                }
            }
        }
    }
    // window HH:MM-HH:MM
    let (start, end) = parse_window(tokens[1]).ok_or_else(|| {
        format!(
            "{var}='{spec}' is malformed: bad token '{}' (a window HH:MM-HH:MM, 00:00..23:59, end up to 24:00, start <> end); {format}",
            tokens[1]
        )
    })?;
    // offset +HH:MM / -HH:MM
    let offset_minutes = parse_offset(tokens[2]).ok_or_else(|| {
        format!(
            "{var}='{spec}' is malformed: bad token '{}' (a UTC offset +HH:MM or -HH:MM, at most 14:00); {format}",
            tokens[2]
        )
    })?;
    Ok(PeakSpec {
        days,
        start,
        end,
        offset_minutes,
    })
}

fn parse_window(tok: &str) -> Option<(i32, i32)> {
    let re = regex::Regex::new(r"^(\d{2}):(\d{2})-(\d{2}):(\d{2})$").ok()?;
    let c = re.captures(tok)?;
    let sh: i32 = c[1].parse().ok()?;
    let sm: i32 = c[2].parse().ok()?;
    let eh: i32 = c[3].parse().ok()?;
    let em: i32 = c[4].parse().ok()?;
    if !(sh <= 23 && sm <= 59 && em <= 59 && (eh <= 23 || (eh == 24 && em == 0))) {
        return None;
    }
    let start = sh * 60 + sm;
    let end = eh * 60 + em;
    if start == end {
        return None;
    }
    Some((start, end))
}

fn parse_offset(tok: &str) -> Option<i32> {
    let re = regex::Regex::new(r"^([+-])(\d{2}):(\d{2})$").ok()?;
    let c = re.captures(tok)?;
    let oh: i32 = c[2].parse().ok()?;
    let om: i32 = c[3].parse().ok()?;
    if !(om <= 59 && (oh * 60 + om) <= 14 * 60) {
        return None;
    }
    let mins = oh * 60 + om;
    Some(if &c[1] == "-" { -mins } else { mins })
}

/// `ConvertFrom-PeakExceptions`: `(start, end)` inclusive date intervals; only `end < start`
/// is refused (ranges of any length are kept, never expanded).
pub fn parse_peak_exceptions(text: &str, var: &str) -> Result<Vec<(NaiveDate, NaiveDate)>, String> {
    let format = "expected comma-separated YYYY-MM-DD or YYYY-MM-DD..YYYY-MM-DD";
    let re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
    let mut out = Vec::new();
    for raw in text.split(',') {
        let item = raw.trim();
        if item.is_empty() {
            continue;
        }
        let parts: Vec<&str> = item.split("..").collect();
        let mut parsed = Vec::new();
        for p in &parts {
            let d = if re.is_match(p) {
                NaiveDate::parse_from_str(p, "%Y-%m-%d").ok()
            } else {
                None
            };
            match d {
                Some(d) => parsed.push(d),
                None => {
                    return Err(format!(
                        "{var}='{text}' is malformed: bad token '{item}' ({format})"
                    ))
                }
            }
        }
        if parsed.len() > 2 || (parsed.len() == 2 && parsed[1] < parsed[0]) {
            return Err(format!(
                "{var}='{text}' is malformed: bad token '{item}' ({format}; a range must not run backwards)"
            ));
        }
        out.push((parsed[0], parsed[parsed.len() - 1]));
    }
    Ok(out)
}

/// `Get-PeakStatus`. `spec`/`except` override the environment when `Some` (for tests);
/// `None` reads `<var>` / `<var>_EXCEPT` from the environment.
pub fn peak_status(
    provider: &str,
    utc_now: DateTime<Utc>,
    spec: Option<&str>,
    except: Option<&str>,
) -> PeakStatus {
    let mut st = PeakStatus {
        peak: None,
        source: "none".into(),
        detail: "no schedule".into(),
        ..Default::default()
    };
    if provider.is_empty() {
        st.detail = "provider unknown".into();
        return st;
    }
    let var = peak_variable_name(provider);
    st.variable = var.clone();
    let spec_val: String = match spec {
        Some(s) => s.to_string(),
        None => std::env::var(&var).unwrap_or_default(),
    };
    let except_val: Option<String> = match except {
        Some(e) => Some(e.to_string()),
        None => std::env::var(format!("{var}_EXCEPT")).ok(),
    };
    if spec_val.trim().is_empty() {
        return st;
    }
    let parsed = match parse_peak_spec(&spec_val, &var) {
        Ok(p) => p,
        Err(e) => {
            st.error = e;
            return st;
        }
    };
    let mut exceptions: Vec<(NaiveDate, NaiveDate)> = Vec::new();
    if let Some(ex) = &except_val {
        if !ex.trim().is_empty() {
            match parse_peak_exceptions(ex, &format!("{var}_EXCEPT")) {
                Ok(v) => exceptions = v,
                Err(e) => {
                    st.error = e;
                    return st;
                }
            }
        }
    }
    st.source = "env".into();
    st.schedule = spec_val.trim().to_string();
    if let Some(ex) = &except_val {
        if !ex.trim().is_empty() {
            st.schedule += &format!("; except {}", ex.trim());
        }
    }
    // local = utc + offset
    let off = FixedOffset::east_opt(parsed.offset_minutes * 60)
        .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let local = utc_now.with_timezone(&off);
    let sign = if parsed.offset_minutes < 0 { '-' } else { '+' };
    let abs = parsed.offset_minutes.abs();
    st.local = format!(
        "{} {}{:02}:{:02}",
        local.format("%Y-%m-%d %H:%M %a"),
        sign,
        abs / 60,
        abs % 60
    );
    let tod = local.hour() as f64 * 60.0 + local.minute() as f64 + local.second() as f64 / 60.0;
    let dow = local.weekday().num_days_from_sunday() as usize;
    let prev = (dow + 6) % 7;
    let start = parsed.start as f64;
    let end = parsed.end as f64;
    let inside = if parsed.start < parsed.end {
        parsed.days[dow] && tod >= start && tod < end
    } else {
        (parsed.days[dow] && tod >= start) || (parsed.days[prev] && tod < end)
    };
    let date_key = local.format("%Y-%m-%d").to_string();
    let local_date = local.date_naive();
    let exception_hit = exceptions
        .iter()
        .any(|(a, b)| local_date >= *a && local_date <= *b);
    if exception_hit {
        st.peak = Some(false);
        st.detail = format!("exception date {date_key} (off-peak all day)");
    } else if inside {
        st.peak = Some(true);
        st.detail = "inside the peak window".into();
    } else {
        st.peak = Some(false);
        st.detail = "outside the peak window".into();
    }
    st
}

/// The clock reading (`Get-ConsultClock`): the ISO timestamp the `call_index`-th evaluation
/// uses. Without `CODEX_CONSULT_NOW` it is the system clock; with it, the k-th comma-separated
/// token (the last repeats). Returns `(utc, iso, from_env)` or an error naming the bad token.
pub fn consult_clock(call_index: usize) -> Result<(DateTime<Utc>, String, bool), String> {
    let raw = std::env::var("CODEX_CONSULT_NOW").unwrap_or_default();
    if raw.trim().is_empty() {
        let now: DateTime<chrono::Local> = chrono::Local::now();
        let iso = now.format("%Y-%m-%dT%H:%M:%S%:z").to_string();
        return Ok((now.with_timezone(&Utc), iso, false));
    }
    let re =
        regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?([+-]\d{2}:\d{2}|Z)$").unwrap();
    let items: Vec<&str> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let mut parsed: Vec<DateTime<FixedOffset>> = Vec::new();
    for item in &items {
        let ok = re.is_match(item) && DateTime::parse_from_rfc3339(item).is_ok();
        match (ok, DateTime::parse_from_rfc3339(item)) {
            (true, Ok(dt)) => parsed.push(dt),
            _ => {
                return Err(format!(
                    "CODEX_CONSULT_NOW='{raw}' is malformed: bad token '{item}' (a test hook: ISO timestamps with an offset, e.g. 2026-09-24T13:59:59+08:00, comma-separated)"
                ))
            }
        }
    }
    let pick = parsed[call_index.min(parsed.len() - 1)];
    let iso = pick.format("%Y-%m-%dT%H:%M:%S%:z").to_string();
    Ok((pick.with_timezone(&Utc), iso, true))
}

/// `Get-PeakStatusNow`: [`consult_clock`] then [`peak_status`] with the environment schedule,
/// carrying `EvaluatedAt` and the `env (CODEX_CONSULT_NOW)` source marker.
pub fn peak_status_now(provider: &str, call_index: usize) -> PeakStatus {
    match consult_clock(call_index) {
        Err(e) => PeakStatus {
            peak: None,
            source: "none".into(),
            error: e,
            ..Default::default()
        },
        Ok((utc, iso, from_env)) => {
            let mut st = peak_status(provider, utc, None, None);
            if from_env && st.source == "env" {
                st.source = "env (CODEX_CONSULT_NOW)".into();
            }
            st.evaluated_at = iso;
            st
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn variable_name_sanitises() {
        assert_eq!(peak_variable_name("ZAI"), "CODEX_CONSULT_PEAK_ZAI");
        assert_eq!(
            peak_variable_name("api.z.ai"),
            "CODEX_CONSULT_PEAK_API_Z_AI"
        );
    }

    #[test]
    fn spec_parse_and_errors() {
        let s = parse_peak_spec("Mon-Fri 14:00-18:00 +08:00", "V").unwrap();
        assert!(s.days[1] && s.days[5] && !s.days[0] && !s.days[6]);
        assert_eq!(s.start, 14 * 60);
        assert_eq!(s.end, 18 * 60);
        assert_eq!(s.offset_minutes, 8 * 60);
        assert!(parse_peak_spec("Mon-Fri 14:00-18:00", "V")
            .unwrap_err()
            .contains("instead of 3"));
        assert!(parse_peak_spec("Xyz 14:00-18:00 +08:00", "V")
            .unwrap_err()
            .contains("bad token 'Xyz'"));
        assert!(parse_peak_spec("* 14:00-14:00 +08:00", "V")
            .unwrap_err()
            .contains("start <> end"));
        assert!(parse_peak_spec("* 14:00-18:00 +15:00", "V")
            .unwrap_err()
            .contains("at most 14:00"));
    }

    #[test]
    fn inside_window_zai() {
        // 14:00-18:00 +08:00 on Mon-Fri; UTC 2026-09-24 (Thu) 07:00 = 15:00 +08:00 → peak.
        let st = peak_status(
            "ZAI",
            utc("2026-09-24T07:00:00Z"),
            Some("Mon-Fri 14:00-18:00 +08:00"),
            None,
        );
        assert_eq!(st.peak, Some(true));
        assert_eq!(st.detail, "inside the peak window");
        // one minute before start is off-peak (start inclusive, but 13:59 < 14:00).
        let st2 = peak_status(
            "ZAI",
            utc("2026-09-24T05:59:00Z"),
            Some("Mon-Fri 14:00-18:00 +08:00"),
            None,
        );
        assert_eq!(st2.peak, Some(false));
        // end is exclusive: 18:00 local (10:00 UTC) is off-peak.
        let st3 = peak_status(
            "ZAI",
            utc("2026-09-24T10:00:00Z"),
            Some("Mon-Fri 14:00-18:00 +08:00"),
            None,
        );
        assert_eq!(st3.peak, Some(false));
    }

    #[test]
    fn overnight_window_spans_midnight() {
        // 22:00-02:00 +00:00: Mon 23:00 is peak (started Mon); Tue 01:00 is peak (prev day Mon).
        let st = peak_status(
            "P",
            utc("2026-09-21T23:00:00Z"),
            Some("Mon 22:00-02:00 +00:00"),
            None,
        );
        assert_eq!(st.peak, Some(true));
        let st2 = peak_status(
            "P",
            utc("2026-09-22T01:00:00Z"),
            Some("Mon 22:00-02:00 +00:00"),
            None,
        );
        assert_eq!(st2.peak, Some(true));
        let st3 = peak_status(
            "P",
            utc("2026-09-22T03:00:00Z"),
            Some("Mon 22:00-02:00 +00:00"),
            None,
        );
        assert_eq!(st3.peak, Some(false));
    }

    #[test]
    fn exception_date_off_peak() {
        let st = peak_status(
            "ZAI",
            utc("2026-09-24T07:00:00Z"),
            Some("Mon-Fri 14:00-18:00 +08:00"),
            Some("2026-09-24"),
        );
        assert_eq!(st.peak, Some(false));
        assert!(st.detail.contains("exception date 2026-09-24"));
        assert!(st.schedule.contains("; except 2026-09-24"));
    }

    #[test]
    fn exception_range_any_length_backwards_refused() {
        assert!(parse_peak_exceptions("2030-01-01..2020-01-01", "V")
            .unwrap_err()
            .contains("must not run backwards"));
        // a multi-year range is fine (never expanded).
        let iv = parse_peak_exceptions("2020-01-01..2030-12-31", "V").unwrap();
        assert_eq!(iv.len(), 1);
    }

    #[test]
    fn no_schedule_is_unknown() {
        let st = peak_status("ZAI", utc("2026-09-24T07:00:00Z"), Some(""), None);
        assert_eq!(st.peak, None);
        assert_eq!(st.source, "none");
        assert_eq!(st.variable, "CODEX_CONSULT_PEAK_ZAI");
    }
}
