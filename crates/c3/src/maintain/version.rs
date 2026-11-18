//! Pure version helpers for `c3 maintain`: the installed-version parse of a CLI's `--version`
//! line, a numeric semver compare, and the model-id successor rule (`C3_MAINTAIN_MODELS=swap`).
//!
//! Everything here is a pure function over strings, so the whole of the maintenance logic that
//! decides *what* to do (is there a newer CLI, which seated model has a successor) is proven by
//! unit tests without touching the machine, the network or a model.

/// A dotted numeric version (`0.169.0`, `1.0.77`), with an optional pre-release-ish tail kept for
/// display but ignored by the ordering (`1.0.70-fake` sorts as `1.0.70`). `parts` is the comparison
/// order, so a longer prefix-equal version is the greater one (`1.0.1 > 1.0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliVersion {
    pub parts: Vec<u64>,
    pub raw: String,
}

impl CliVersion {
    /// The version string as installed (the exact token parsed, e.g. `0.169.0`).
    pub fn text(&self) -> &str {
        &self.raw
    }
}

impl PartialOrd for CliVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CliVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let n = self.parts.len().max(other.parts.len());
        for i in 0..n {
            let a = self.parts.get(i).copied().unwrap_or(0);
            let b = other.parts.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                std::cmp::Ordering::Equal => {}
                ord => return ord,
            }
        }
        std::cmp::Ordering::Equal
    }
}

/// The first semver-like token of a CLI's `--version` output: the first whitespace-separated token
/// that starts (after an optional leading `v`) with `<digits>.<digits>`. `codex-cli 0.169.0` →
/// `0.169.0`; `grok 1.0.77 (abc) [stable]` → `1.0.77`; `2.5.3` → `2.5.3`; `grok 1.0.70-fake` →
/// `1.0.70`. `None` when no token looks like a version.
pub fn parse_cli_version(text: &str) -> Option<CliVersion> {
    for token in text.split_whitespace() {
        let token = token.trim_start_matches('v').trim_start_matches('V');
        if let Some(v) = version_from_token(token) {
            return Some(v);
        }
    }
    None
}

/// Parse a single token as a dotted numeric version: a leading `<digits>(.<digits>)*`, keeping the
/// raw numeric prefix for display and the numeric parts for comparison. A trailing non-numeric tail
/// (`-fake`, `-alpha.2`) ends the numeric run and is ignored. Requires at least a `major.minor`.
fn version_from_token(token: &str) -> Option<CliVersion> {
    let bytes = token.as_bytes();
    // the numeric-and-dots prefix
    let mut end = 0;
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        end += 1;
    }
    let head = &token[..end];
    let parts: Vec<u64> = head
        .split('.')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u64>().ok())
        .collect::<Option<Vec<u64>>>()?;
    if parts.len() < 2 {
        return None; // a bare "7" is not a version token we trust
    }
    // the raw: the numeric prefix with no trailing dot
    let raw = head.trim_end_matches('.').to_string();
    Some(CliVersion { parts, raw })
}

// --------------------------------------------------------------------------- model-id shape guard

/// The strict shape a model id must match to be accepted from an untrusted source (a `/v1/models`
/// body, a `grok models` / `agy models` list): `[A-Za-z0-9][A-Za-z0-9._:/-]{0,99}`. Everything else
/// (a quote, a backslash, a newline, a control character, an over-long id) is dropped at collection
/// and refused by the roster edit, so a hostile id can neither break out of the roster JSON nor
/// smuggle a second line into the report.
pub fn valid_model_id(id: &str) -> bool {
    let b = id.as_bytes();
    if b.is_empty() || b.len() > 100 {
        return false;
    }
    if !b[0].is_ascii_alphanumeric() {
        return false;
    }
    b[1..]
        .iter()
        .all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b':' | b'/' | b'-'))
}

// --------------------------------------------------------------------------- the model successor rule

/// A model id split into a prefix, a `major[.minor]` version and a suffix, for the successor rule.
/// `glm-5.3-flash` → (`glm-`, 5, 3, `-flash`); `kimi-code/k3.1-pro` → (`kimi-code/k`, 3, 1,
/// `-pro`); `qwen3.8-max` → (`qwen`, 3, 8, `-max`); `grok-4.7` → (`grok-`, 4, 7, ``). An id with no
/// dotted version takes its FIRST digit run as the major and 0 as the minor: `qwen4-max` → (`qwen`,
/// 4, 0, `-max`); `gpt-6-astra` → (`gpt-`, 6, 0, `-astra`); `glm-6` → (`glm-`, 6, 0, ``).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelId {
    pub prefix: String,
    pub major: u64,
    pub minor: u64,
    pub suffix: String,
}

impl ModelId {
    /// The version as an ordered pair (`qwen4-max` is `(4, 0)`, so `4 > 3.8`).
    pub fn version(&self) -> (u64, u64) {
        (self.major, self.minor)
    }
}

/// Parse a model id for the successor rule. The version is the FIRST `<digits>.<digits>` run in the
/// id (a single `major.minor`); an id with no dotted version takes its FIRST digit run as the major
/// (minor 0). `None` when the id carries no digit at all (a family with no version is only ever
/// announced, never swapped) or when the id carries a date/build stamp ([`is_stamped`]), which is
/// never swapped — so a candidate carrying such a stamp is never a successor either.
pub fn parse_model_id(id: &str) -> Option<ModelId> {
    if is_stamped(id) {
        return None;
    }
    let chars: Vec<char> = id.chars().collect();
    let (start, end, major, minor) =
        first_dotted_version(&chars).or_else(|| first_bare_version(&chars))?;
    Some(ModelId {
        prefix: chars[..start].iter().collect(),
        major,
        minor,
        suffix: chars[end..].iter().collect(),
    })
}

/// The FIRST digit run of `chars` as `(start, end, major, 0)`, for an id with no dotted version. A
/// run of four or more digits is a year/build number, not a model version (`None`).
fn first_bare_version(chars: &[char]) -> Option<(usize, usize, u64, u64)> {
    let start = chars.iter().position(|c| c.is_ascii_digit())?;
    let mut end = start;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    if end - start >= 4 {
        return None;
    }
    let major: u64 = chars[start..end].iter().collect::<String>().parse().ok()?;
    Some((start, end, major, 0))
}

/// The FIRST `<digits>.<digits>` run in `chars` as `(start, end, major, minor)` (byte-index-free:
/// char indices), or `None` when there is no dotted version. A trailing further `.<digits>` (a third
/// dotted part) is NOT consumed: the version is a single `major.minor`.
fn first_dotted_version(chars: &[char]) -> Option<(usize, usize, u64, u64)> {
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if chars[i].is_ascii_digit() {
            let ds = i;
            while i < n && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < n && chars[i] == '.' && i + 1 < n && chars[i + 1].is_ascii_digit() {
                let dot = i;
                let mut j = i + 1;
                while j < n && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let major: u64 = chars[ds..dot].iter().collect::<String>().parse().ok()?;
                let minor: u64 = chars[dot + 1..j].iter().collect::<String>().parse().ok()?;
                return Some((ds, j, major, minor));
            }
            // a digit run not followed by `.<digits>`: keep scanning past it (i already advanced).
        } else {
            i += 1;
        }
    }
    None
}

/// Whether an id carries a date or build stamp and so must never be swapped. The model version is
/// the FIRST `major[.minor]` number; an id that carries any FURTHER date/build-like segment is
/// stamped: a run of 4+ digits (`-1017`, `-261017`, a year `2026`), or a SECOND dotted number
/// (`...-2026.11`). An id with no dotted version at all is stamped when its last `-`/`/` segment is
/// all digits and at least three long (`deepseek-v4-1-flash-261017`), matching the pre-wave rule.
pub fn is_stamped(id: &str) -> bool {
    let chars: Vec<char> = id.chars().collect();
    match first_dotted_version(&chars) {
        Some((_, end, _, _)) => remainder_is_stampish(&chars[end..]),
        None => {
            let last_seg_stamp = match id.rsplit(['-', '/']).next() {
                Some(seg) => seg.len() >= 3 && seg.bytes().all(|b| b.is_ascii_digit()),
                None => false,
            };
            // with no dotted version the version is the first digit run: a 4+-digit run after it
            // (`gpt-6-2026-preview`) is a stamp too
            let later_stamp = match chars.iter().position(|c| c.is_ascii_digit()) {
                Some(start) => {
                    let mut end = start;
                    while end < chars.len() && chars[end].is_ascii_digit() {
                        end += 1;
                    }
                    remainder_is_stampish(&chars[end..])
                }
                None => false,
            };
            last_seg_stamp || later_stamp
        }
    }
}

/// Whether the part of an id AFTER its `major.minor` version carries a date/build-like segment: a
/// run of 4+ digits, or a further dotted number (`<digits>.<digits>`).
fn remainder_is_stampish(rest: &[char]) -> bool {
    let m = rest.len();
    let mut i = 0;
    while i < m {
        if rest[i].is_ascii_digit() {
            let ds = i;
            while i < m && rest[i].is_ascii_digit() {
                i += 1;
            }
            if i - ds >= 4 {
                return true; // a build number or a year-like stamp
            }
            if i < m && rest[i] == '.' && i + 1 < m && rest[i + 1].is_ascii_digit() {
                return true; // a second dotted number (a dotted date/build)
            }
        } else {
            i += 1;
        }
    }
    false
}

/// The successor of `seated` among `candidates` under mode `swap`: the candidate with the identical
/// prefix and suffix and a strictly higher version - a higher major included (`qwen3.8-max` ->
/// `qwen4-max`, `glm-5.3` -> `glm-6`); the highest version wins (on a tie the first listed). `None`
/// when the seated id cannot be split ([`parse_model_id`]) or no candidate qualifies (a different
/// suffix or prefix is a new family member, only announced, never returned here). A candidate that
/// is itself stamped or unparsable is skipped.
pub fn successor<'a>(seated: &str, candidates: &'a [String]) -> Option<&'a str> {
    let base = parse_model_id(seated)?;
    let mut best: Option<(&'a str, (u64, u64))> = None;
    for cand in candidates {
        if cand == seated {
            continue;
        }
        let Some(c) = parse_model_id(cand) else {
            continue;
        };
        if c.prefix == base.prefix
            && c.suffix == base.suffix
            && c.version() > base.version()
            && best.map(|(_, v)| c.version() > v).unwrap_or(true)
        {
            best = Some((cand.as_str(), c.version()));
        }
    }
    best.map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(raw: &str) -> CliVersion {
        parse_cli_version(raw).unwrap()
    }

    #[test]
    fn parses_the_first_semver_token() {
        assert_eq!(v("codex-cli 0.169.0").text(), "0.169.0");
        assert_eq!(v("grok 1.0.77 (50ca579fa996) [stable]").text(), "1.0.77");
        assert_eq!(v("2.5.3").text(), "2.5.3");
        assert_eq!(v("1.5.9").text(), "1.5.9");
        assert_eq!(v("grok 1.0.70-fake").text(), "1.0.70");
        assert_eq!(v("version v12.0.9 here").text(), "12.0.9");
        // no version-like token
        assert!(parse_cli_version("no version here").is_none());
        assert!(parse_cli_version("build 7").is_none()); // a bare integer is not trusted
    }

    #[test]
    fn version_ordering_is_numeric_not_lexical() {
        assert!(v("0.2.10") > v("0.2.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert!(v("0.169.0") > v("0.155.1"));
        assert!(v("2.6.0") > v("2.5.3"));
        assert_eq!(v("1.0.77").cmp(&v("1.0.77")), std::cmp::Ordering::Equal);
        // a longer prefix-equal version is greater
        assert!(v("1.0.1") > v("1.0"));
    }

    #[test]
    fn model_id_split() {
        assert_eq!(
            parse_model_id("glm-5.3-flash"),
            Some(ModelId {
                prefix: "glm-".into(),
                major: 5,
                minor: 3,
                suffix: "-flash".into()
            })
        );
        assert_eq!(
            parse_model_id("kimi-code/k3.1-pro"),
            Some(ModelId {
                prefix: "kimi-code/k".into(),
                major: 3,
                minor: 1,
                suffix: "-pro".into()
            })
        );
        assert_eq!(
            parse_model_id("qwen3.8-max"),
            Some(ModelId {
                prefix: "qwen".into(),
                major: 3,
                minor: 8,
                suffix: "-max".into()
            })
        );
        assert_eq!(
            parse_model_id("muse-spark-1.3-contributor"),
            Some(ModelId {
                prefix: "muse-spark-".into(),
                major: 1,
                minor: 3,
                suffix: "-contributor".into()
            })
        );
        assert_eq!(
            parse_model_id("grok-4.7"),
            Some(ModelId {
                prefix: "grok-".into(),
                major: 4,
                minor: 7,
                suffix: "".into()
            })
        );
    }

    #[test]
    fn stamped_ids_never_parse_and_never_swap() {
        assert!(is_stamped("deepseek-v4.1-flash-1017"));
        assert!(is_stamped("deepseek-v4-1-flash-261017"));
        assert!(!is_stamped("glm-5.3-flash"));
        assert!(!is_stamped("grok-4.7"));
        assert!(!is_stamped("kimi-code/k3.1-pro"));
        // a stamped id is unswappable
        assert!(parse_model_id("deepseek-v4.1-flash-1017").is_none());
        let cands = vec![
            "deepseek-v4.1-flash-1017".to_string(),
            "deepseek-v4.2-flash-1099".to_string(),
        ];
        assert_eq!(successor("deepseek-v4.1-flash-1017", &cands), None);
    }

    #[test]
    fn dotted_date_stamps_are_never_swapped() {
        // the reviewer's case: a dotted date suffix must not be read as the model version
        assert!(is_stamped("model-5.3-2026.11"));
        assert_eq!(parse_model_id("model-5.3-2026.11"), None);
        assert_eq!(
            successor("model-5.3-2026.11", &["model-5.3-2026.12".into()]),
            None
        );
        // the version is the FIRST major.minor; a trailing 4+-digit run is a stamp
        assert!(is_stamped("glm-5.3-20261115"));
        assert!(is_stamped("grok-4.7-1017"));
        assert_eq!(parse_model_id("grok-4.7-1017"), None);
        // a candidate carrying such a stamp is never a successor of a clean seated id
        assert_eq!(successor("grok-4.7", &["grok-4.8-2026".into()]), None);
        // clean ids still parse by their FIRST dotted version
        assert_eq!(
            parse_model_id("glm-5.3-flash").map(|m| (m.major, m.minor)),
            Some((5, 3))
        );
        assert_eq!(
            parse_model_id("kimi-code/k3.1-pro").map(|m| (m.major, m.minor)),
            Some((3, 1))
        );
    }

    #[test]
    fn model_id_shape_guard() {
        assert!(valid_model_id("glm-5.4"));
        assert!(valid_model_id("kimi-code/k3.1-pro"));
        assert!(valid_model_id("gpt-6-astra"));
        assert!(valid_model_id("a:b_c.d/e-f"));
        // rejected: a leading non-alnum, a quote, a backslash, a newline, a space, over-long
        assert!(!valid_model_id("-leading"));
        assert!(!valid_model_id("glm\"-5.4"));
        assert!(!valid_model_id("glm\\5.4"));
        assert!(!valid_model_id("glm-5.4\nignore the roster"));
        assert!(!valid_model_id("glm 5.4"));
        assert!(!valid_model_id(""));
        assert!(!valid_model_id(&"a".repeat(101)));
    }

    #[test]
    fn successor_highest_version_wins_across_majors() {
        let cands = vec![
            "glm-5.3-flash".to_string(),
            "glm-5.4-flash".to_string(),
            "glm-5.6-flash".to_string(),
            "glm-5.4-pro".to_string(),   // different suffix
            "glm-5.2-flash".to_string(), // lower: not a successor
        ];
        assert_eq!(successor("glm-5.3-flash", &cands), Some("glm-5.6-flash"));
        // a higher major is a successor too, and the highest version wins
        let mut cands = cands;
        cands.push("glm-6.0-flash".to_string());
        cands.push("glm-6-flash".to_string()); // the same version spelled without a minor: a tie
        assert_eq!(successor("glm-5.3-flash", &cands), Some("glm-6.0-flash"));
    }

    #[test]
    fn ids_without_a_minor_parse_as_major_dot_zero() {
        let v = |id: &str| parse_model_id(id).map(|m| (m.prefix, m.major, m.minor, m.suffix));
        assert_eq!(v("qwen4-max"), Some(("qwen".into(), 4, 0, "-max".into())));
        assert_eq!(
            v("gpt-6-astra"),
            Some(("gpt-".into(), 6, 0, "-astra".into()))
        );
        assert_eq!(v("glm-6"), Some(("glm-".into(), 6, 0, "".into())));
        assert_eq!(v("no-digits-here"), None);
        // a year-like first number or a later stamp is no version
        assert_eq!(v("model-2026"), None);
        assert!(is_stamped("gpt-6-2026-preview"));
        assert!(!is_stamped("qwen4-max"));
    }

    #[test]
    fn successor_across_a_major_version() {
        // today's case: qwen3.8-max -> qwen4-max (a major without a minor)
        assert_eq!(
            successor("qwen3.8-max", &["qwen4-max".into(), "qwen4-plus".into()]),
            Some("qwen4-max")
        );
        assert_eq!(successor("glm-5.3", &["glm-6".into()]), Some("glm-6"));
        assert_eq!(
            successor(
                "gpt-6-astra",
                &["gpt-6.1-astra".into(), "gpt-7-astra".into()]
            ),
            Some("gpt-7-astra")
        );
        // the same version is not higher
        assert_eq!(successor("glm-6", &["glm-6.0".into()]), None);
    }

    #[test]
    fn successor_rejects_a_different_suffix_or_family() {
        // a different suffix (a new family member) is never a successor
        assert_eq!(
            successor("gemini-3.8-flash-high", &["gemini-4-hydrogen".into()]),
            None
        );
        // a different family (different prefix) → no successor
        let cands = vec!["qwen4.0-flash".to_string()];
        assert_eq!(successor("glm-5.3-flash", &cands), None);
        // claude-sonnet-4-6 splits as (claude-sonnet-, 4, -6): another trailing number is another
        // suffix, never a successor
        assert_eq!(
            successor("claude-sonnet-4-6", &["claude-sonnet-4-7".into()]),
            None
        );
    }

    #[test]
    fn successor_none_when_no_candidate() {
        assert_eq!(successor("grok-4.7", &[]), None);
        assert_eq!(successor("grok-4.7", &["grok-4.7".into()]), None); // same id
    }
}
