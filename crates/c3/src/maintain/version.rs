//! Pure version helpers for `c3 maintain`: the release read from a CLI's `--version` line (an exact
//! identity plus a semver ordering), and the model-id successor rule (`C3_MAINTAIN_MODELS=swap`).
//!
//! Everything here is a pure function over strings, so the whole of the maintenance logic that
//! decides *what* to do (is there a newer CLI, which seated model has a successor) is proven by
//! unit tests without touching the machine, the network or a model.

use std::cmp::Ordering;

/// A CLI release as a `--version` line or a vendor's latest-release reading names it. (F62-1) It
/// answers two different questions with two different parts, never the one by the other:
///
/// * IDENTITY - "is this the same release" ([`CliVersion::text`], [`CliVersion::same_release`] and
///   `==`): the exact release text as the CLI printed it, normalised only by what cannot tell two
///   releases apart - the surrounding whitespace, a leading `v`, a trailing full stop and the
///   punctuation the CLI prints right after the token (`version 1.0.77,` -> `1.0.77`). The tag after
///   the numbers is part of it: `1.0.80-rc.1` and `1.0.80` are DIFFERENT releases. So is build
///   metadata: `1.0.80+build.7` is not `1.0.80` (two builds are two artifacts; the parser before
///   F62-1 dropped it together with every other suffix, which was the defect, not a rule to keep).
///   Every attribution uses the identity: which release a smoke ran on, whether the installed
///   release is the pending target, the previous one or still the one measured before a smoke,
///   whether a rollback restored the previous release, the version an install command pins, and what
///   the pending journal, the state and the report name.
/// * ORDERING - "is A newer than B" ([`CliVersion::precedence`], [`CliVersion::is_newer_than`]):
///   semver precedence. The numeric parts first, zero-padded (`1.0.1` is newer than `1.0`, `1.0`
///   ties `1.0.0`); on equal numbers a PRE-RELEASE - any tag after the numbers, `1.0.80-rc.1` or
///   `1.0.80rc1` - sorts BEFORE the final release `1.0.80`, two tags compare by semver's identifier
///   rule, and build metadata is ignored. Ordering only decides whether a release is worth
///   installing; a tie never makes two releases the same and never seats or verifies anything.
///
/// The type deliberately has no `Ord`: an ordering that ignores build metadata cannot agree with an
/// identity `==` that keeps it.
#[derive(Debug, Clone)]
pub struct CliVersion {
    /// The exact release identity (`1.0.80-rc.1`, `0.169.0`).
    raw: String,
    /// The numeric parts (`1.0.80-rc.1` -> `[1, 0, 80]`): ordering only, never identity.
    parts: Vec<u64>,
    /// The tag between the numbers and any build metadata, without its leading `-` (`rc.1`);
    /// `None` for a final release. Ordering only.
    pre: Option<String>,
}

impl PartialEq for CliVersion {
    /// Identity: the same exact release ([`CliVersion::same_release`]), never the same numbers.
    fn eq(&self, other: &Self) -> bool {
        self.same_release(other)
    }
}

impl Eq for CliVersion {}

impl CliVersion {
    /// The exact release identity as installed or announced (`0.169.0`, `1.0.80-rc.1`).
    pub fn text(&self) -> &str {
        &self.raw
    }

    /// Whether `self` and `other` are the same release: the exact identity, so `1.0.80-rc.1` is not
    /// `1.0.80`, and `1.0.80+build.7` is not `1.0.80`.
    pub fn same_release(&self, other: &Self) -> bool {
        self.raw == other.raw
    }

    /// Semver precedence of `self` against `other`: the numeric parts (zero-padded), then a
    /// pre-release BEFORE the final release of the same numbers, then the tags by semver's identifier
    /// rule; build metadata is ignored. `Equal` is a tie in ordering only - it never means the same
    /// release (that is [`CliVersion::same_release`]).
    pub fn precedence(&self, other: &Self) -> Ordering {
        let n = self.parts.len().max(other.parts.len());
        for i in 0..n {
            let a = self.parts.get(i).copied().unwrap_or(0);
            let b = other.parts.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                Ordering::Equal => {}
                ord => return ord,
            }
        }
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            // a pre-release of the same numbers comes BEFORE its final release
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => cmp_pre_release(a, b),
        }
    }

    /// Whether `self` is a strictly newer release than `other` by [`CliVersion::precedence`]: a final
    /// release is newer than its own pre-release, a pre-release is never newer than (nor ties) its
    /// final release, and a build-metadata-only difference is newer in neither direction.
    pub fn is_newer_than(&self, other: &Self) -> bool {
        self.precedence(other) == Ordering::Greater
    }
}

/// Semver's pre-release precedence: dot-separated identifiers left to right, numeric ones
/// numerically, others in ASCII order, a numeric identifier below a non-numeric one, and of two runs
/// equal so far the shorter one first (`alpha` < `alpha.1` < `beta` < `rc.2` < `rc.10`). (F67-2) A
/// numeric identifier is recognised by its digits alone, never by a machine-integer conversion, so
/// one of any length compares numerically: by its count of significant digits (leading zeros
/// stripped), then digit by digit (`18446744073709551616` < `100000000000000000000`).
fn cmp_pre_release(a: &str, b: &str) -> Ordering {
    /// The significant digits of an all-digit identifier (`"007"` -> `"7"`, `"0"` -> `""`), or
    /// `None` when the identifier is not numeric.
    fn numeric(s: &str) -> Option<&str> {
        if !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()) {
            Some(s.trim_start_matches('0'))
        } else {
            None
        }
    }
    let mut left = a.split('.');
    let mut right = b.split('.');
    loop {
        let ord = match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match (numeric(x), numeric(y)) {
                // more significant digits is the larger number; equal counts compare digit by
                // digit, which for ASCII digits is the byte order
                (Some(m), Some(n)) => m.len().cmp(&n.len()).then_with(|| m.cmp(n)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => x.cmp(y),
            },
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
}

/// The release named by a CLI's `--version` output (or a vendor's latest-release reading): the first
/// whitespace-separated token that starts (after an optional leading `v`) with `<digits>.<digits>`,
/// read exactly ([`CliVersion`]). `codex-cli 0.169.0` -> `0.169.0`; `grok 1.0.77 (abc) [stable]` ->
/// `1.0.77`; `2.5.3` -> `2.5.3`; `grok 1.0.80-rc.1` -> `1.0.80-rc.1`; `version v12.0.9,` ->
/// `12.0.9`. `None` when no token looks like a version, and (F62-1) when the first version-shaped
/// token cannot be read exactly (`1.0.80(rc1)`, `1.0.80_rc1`): the line then names no readable
/// release, and a later number on it is not taken instead.
pub fn parse_cli_version(text: &str) -> Option<CliVersion> {
    for token in text.split_whitespace() {
        let token = token.trim_start_matches('v').trim_start_matches('V');
        match read_token(token) {
            Token::Release(v) => return Some(v),
            Token::Unclear => return None,
            Token::Other => {}
        }
    }
    None
}

/// What one whitespace-separated token of a `--version` line is.
enum Token {
    /// Not version-shaped (`grok`, `(50ca579fa996)`, a bare `7`): the scan goes on.
    Other,
    /// A release, read exactly.
    Release(CliVersion),
    /// Version-shaped, but the release it names cannot be told exactly: the scan stops.
    Unclear,
}

/// Read one token. It must start with a dotted number of at least `major.minor` (a bare `7` is not
/// a version we trust). The release identity runs on over semver's characters (ASCII letters and
/// digits, `.`, `-`, `+`), less a trailing `.`; what follows it in the token must be punctuation
/// only (the CLI's decoration: `1.0.77,` `1.0.77)`), else the token is [`Token::Unclear`] - a letter
/// or digit beyond an unexpected character could make two different releases read as one.
fn read_token(token: &str) -> Token {
    let bytes = token.as_bytes();
    // the numeric-and-dots prefix: the parts the ordering uses
    let mut end = 0;
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        end += 1;
    }
    let head = &token[..end];
    let pieces: Vec<&str> = head.split('.').filter(|s| !s.is_empty()).collect();
    if pieces.len() < 2 {
        return Token::Other;
    }
    let Some(parts) = pieces
        .iter()
        .map(|s| s.parse::<u64>().ok())
        .collect::<Option<Vec<u64>>>()
    else {
        // (diff review 64) dotted numbers of which one does not fit a u64: version-shaped, but
        // not a release that can be read exactly - the scan stops, a later number is not taken
        return Token::Unclear;
    };
    let numbers = head.trim_end_matches('.');
    // the release identity: the numbers and any tag / build metadata in semver's characters
    let mut run = end;
    while run < bytes.len()
        && (bytes[run].is_ascii_alphanumeric() || matches!(bytes[run], b'.' | b'-' | b'+'))
    {
        run += 1;
    }
    if !token[run..].chars().all(|c| c.is_ascii_punctuation()) {
        return Token::Unclear;
    }
    // trimming trailing dots never cuts into `numbers` (it ends in a digit)
    let raw = token[..run].trim_end_matches('.');
    // the tag: what follows the numbers up to the first `+` (build metadata, ordering-free)
    let tag = raw[numbers.len()..].split('+').next().unwrap_or("");
    let pre = if tag.is_empty() {
        None
    } else {
        Some(tag.strip_prefix('-').unwrap_or(tag).to_string())
    };
    Token::Release(CliVersion {
        raw: raw.to_string(),
        parts,
        pre,
    })
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
        // (F62-1) the tag is part of the release, never dropped
        assert_eq!(v("grok 1.0.70-fake").text(), "1.0.70-fake");
        assert_eq!(v("version v12.0.9 here").text(), "12.0.9");
        // no version-like token
        assert!(parse_cli_version("no version here").is_none());
        // a bare integer is not trusted
        assert!(parse_cli_version("build 7").is_none());
        // (diff review 64) a dotted number that does not fit a u64 is version-shaped but
        // unreadable: the scan stops there and the later `1.2.3` is not taken; a bare number
        // of that size is not version-shaped at all
        assert!(parse_cli_version("tool 18446744073709551616.1 also 1.2.3").is_none());
        assert_eq!(v("tool 18446744073709551616 1.2.3").text(), "1.2.3");
    }

    #[test]
    fn version_ordering_is_numeric_not_lexical() {
        assert!(v("0.2.10").is_newer_than(&v("0.2.9")));
        assert!(v("1.0.0").is_newer_than(&v("0.99.99")));
        assert!(v("0.169.0").is_newer_than(&v("0.155.1")));
        assert!(v("2.6.0").is_newer_than(&v("2.5.3")));
        assert!(!v("2.5.3").is_newer_than(&v("2.6.0")));
        assert_eq!(v("1.0.77").precedence(&v("1.0.77")), Ordering::Equal);
        // a longer prefix-equal version is greater
        assert!(v("1.0.1").is_newer_than(&v("1.0")));
    }

    /// (F62-1) The release identity is exact - equal numbers with different tags or build metadata
    /// are different releases - and only the decoration that cannot tell two releases apart is
    /// dropped. The CLI-specific shapes: codex `codex-cli 0.169.0`, grok `grok 1.0.77 (hash)
    /// [stable]`, a bare marker body, a trailing comma or full stop.
    #[test]
    fn equal_numbers_with_different_identities_are_different_releases() {
        let rc = v("grok 1.0.80-rc.1");
        let fin = v("grok 1.0.80");
        assert_eq!(rc.text(), "1.0.80-rc.1");
        assert_eq!(fin.text(), "1.0.80");
        assert!(!rc.same_release(&fin) && !fin.same_release(&rc));
        assert_ne!(rc, fin);
        // a tag without a hyphen and build metadata are part of the identity too
        assert_eq!(v("1.0.80rc1").text(), "1.0.80rc1");
        assert_ne!(v("1.0.80rc1"), fin);
        assert_eq!(v("1.0.80+build.7").text(), "1.0.80+build.7");
        assert_ne!(v("1.0.80+build.7"), fin);
        assert_eq!(v("1.0.80-rc.1+b.2").text(), "1.0.80-rc.1+b.2");
        assert_ne!(v("1.0.80-rc.1+b.2"), rc);
        // the tag is case-sensitive: no folding that could merge two releases
        assert_ne!(v("1.0.80-RC.1"), rc);
        // what cannot tell two releases apart is dropped: whitespace, a leading v, the CLI's
        // decoration after the token, a trailing full stop
        assert_eq!(v("  v1.0.80-rc.1\n"), rc);
        assert_eq!(v("grok 1.0.80 (50ca579fa996) [stable]"), fin);
        assert_eq!(v("kimi, version 1.0.80,"), fin);
        assert_eq!(v("installed 1.0.80."), fin);
        assert_eq!(v("installed 1.0.80-rc.1."), rc);
        assert_eq!(v("(1.0.80-rc.1) 2.0.0").text(), "2.0.0"); // a parenthesised token is not read
                                                              // the identity text reads back as itself (the pending journal stores it)
        let full = v("grok 1.0.80-rc.1+b.2");
        assert_eq!(parse_cli_version(full.text()), Some(full.clone()));
        assert_eq!(parse_cli_version(rc.text()), Some(rc.clone()));
        // a version-shaped token whose release cannot be read exactly names no release - and no
        // later number on the line is taken instead
        assert!(parse_cli_version("grok 1.0.80(rc1)").is_none());
        assert!(parse_cli_version("grok 1.0.80_rc1 built with 2.0.0").is_none());
        assert!(parse_cli_version("grok 1.0.80/linux").is_none());
    }

    /// (F62-1) The ordering: numbers first; on equal numbers a pre-release sorts BEFORE its final
    /// release (never newer than it, never tied with it); tags by semver's identifier rule; build
    /// metadata ignored - a tie in ordering that is still not the same release.
    #[test]
    fn a_pre_release_sorts_before_its_final_release() {
        let rc = v("1.0.80-rc.1");
        let fin = v("1.0.80");
        assert_eq!(rc.precedence(&fin), Ordering::Less);
        assert_eq!(fin.precedence(&rc), Ordering::Greater);
        assert!(fin.is_newer_than(&rc));
        assert!(!rc.is_newer_than(&fin));
        assert!(!v("1.0.80rc1").is_newer_than(&fin));
        assert!(fin.is_newer_than(&v("1.0.80rc1")));
        // the numbers decide first
        assert!(rc.is_newer_than(&v("1.0.79")));
        assert!(v("1.0.81-alpha").is_newer_than(&fin));
        // tags: numeric identifiers numerically, a numeric one below a word, shorter first
        assert!(v("1.0.80-rc.2").is_newer_than(&rc));
        assert!(v("1.0.80-rc.10").is_newer_than(&v("1.0.80-rc.9")));
        assert!(v("1.0.80-rc.1").is_newer_than(&v("1.0.80-beta.9")));
        assert!(v("1.0.80-alpha.1").is_newer_than(&v("1.0.80-alpha")));
        assert!(v("1.0.80-alpha.beta").is_newer_than(&v("1.0.80-alpha.1")));
        // build metadata: equal precedence, newer in neither direction, still not the same release
        let b1 = v("1.0.80+b1");
        let b2 = v("1.0.80+b2");
        assert_eq!(b1.precedence(&b2), Ordering::Equal);
        assert_eq!(b1.precedence(&fin), Ordering::Equal);
        assert!(!b1.is_newer_than(&b2) && !b2.is_newer_than(&b1));
        assert!(!b1.same_release(&b2) && !b1.same_release(&fin));
        // a pre-release with build metadata is still a pre-release
        assert!(fin.is_newer_than(&v("1.0.80-rc.1+b.2")));
    }

    /// (F67-2, the reviewer's RC2) Numeric pre-release identifiers compare numerically at any
    /// length - by the count of significant digits, then digit by digit - never lexically once
    /// they no longer fit a machine integer; numeric still sorts below alphanumeric.
    #[test]
    fn numeric_pre_release_identifiers_compare_numerically_at_any_length() {
        let small = v("1.0.0-18446744073709551616"); // 2^64: one past u64
        let large = v("1.0.0-100000000000000000000"); // 10^20
        assert_eq!(small.precedence(&large), Ordering::Less);
        assert_eq!(large.precedence(&small), Ordering::Greater);
        assert!(large.is_newer_than(&small));
        assert!(!small.is_newer_than(&large));
        // equal counts of significant digits compare digit by digit, past u64 too
        assert!(
            v("1.0.0-rc.18446744073709551617").is_newer_than(&v("1.0.0-rc.18446744073709551616"))
        );
        // leading zeros are not significant: a tie in ordering, still two releases
        let zeros = v("1.0.0-rc.007");
        assert_eq!(zeros.precedence(&v("1.0.0-rc.7")), Ordering::Equal);
        assert!(!zeros.same_release(&v("1.0.0-rc.7")));
        assert!(v("1.0.0-rc.1").is_newer_than(&v("1.0.0-rc.0")));
        // ordinary numeric identifiers still compare numerically, in both directions
        assert!(v("1.0.0-rc.10").is_newer_than(&v("1.0.0-rc.2")));
        assert!(!v("1.0.0-rc.2").is_newer_than(&v("1.0.0-rc.10")));
        assert_eq!(
            v("1.0.0-rc.2").precedence(&v("1.0.0-rc.10")),
            Ordering::Less
        );
        // numeric below alphanumeric, whatever the length of the number
        assert!(v("1.0.0-alpha").is_newer_than(&v("1.0.0-100000000000000000000")));
        assert!(v("1.0.0-rc.a").is_newer_than(&v("1.0.0-rc.99999999999999999999999")));
        assert!(!v("1.0.0-99").is_newer_than(&v("1.0.0-a")));
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
