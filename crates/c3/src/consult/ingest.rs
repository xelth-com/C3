//! Reply ingestion (`Get-ProseGate`, `codex-consult-common.ps1:1719`, and the
//! structured/prose branch of `codex-consult.ps1`).
//!
//! A reply is ingested one of three ways:
//! * **structured** — the reply is one bare JSON object that validates as a consult-reply v1
//!   ([`c3_core::engine::StructuredReply`]); its findings are tracked.
//! * **prose** — not a valid object but *substantive* prose ([`prose_gate`]); it earns one
//!   format-repair turn, and if that also fails the prose is kept as the reply of record.
//! * **not attempted** — below the substantiveness floor or refusal-shaped; no repair turn,
//!   the `validation_error` ends with the reason.

use c3_core::engine::StructuredReply;
use regex::Regex;
use serde_json::{Map, Value};

/// The prose gate's verdict (`Get-ProseGate` fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProseGate {
    /// The reply is substantive enough to earn a repair turn.
    pub substantive: bool,
    /// Why not, when it is not substantive (empty when it is).
    pub reason: String,
    pub words: usize,
    pub numbered: usize,
}

const REFUSAL_PHRASES: [&str; 9] = [
    "i cannot",
    "i can't",
    "i'm sorry",
    "i am sorry",
    "i am unable",
    "i'm unable",
    "sorry, ",
    "as an ai",
    "i won't",
];

const NUMBERED_RE: &str = r"(?m)^[ \t]*(?:\*\*Q[0-9]+[.:]\*\*|Q[0-9]+[.:]|\*\*[0-9]+\.\*\*|[0-9]+[.)]|#{1,6}[ \t]*Q[0-9]+\b)";

fn numbered_re() -> Regex {
    Regex::new(NUMBERED_RE).unwrap()
}

/// Whether the reply is substantive prose (`Get-ProseGate`). The floor is tied to how many
/// numbered answers it has (25 words + 2 answers, 40 + 1, else 120), and a refusal-shaped
/// reply with no answer markers never qualifies.
pub fn prose_gate(text: &str) -> ProseGate {
    let words = text.split_whitespace().filter(|w| !w.is_empty()).count();
    let numbered = numbered_re().find_iter(text).count();

    // head: trim, normalise curly quotes, first 200 chars, lowercased.
    let normalised: String = text.trim().replace(['\u{2018}', '\u{2019}'], "'");
    let head: String = normalised
        .chars()
        .take(200)
        .collect::<String>()
        .to_lowercase();
    // lead: strip leading whitespace / markdown decoration.
    let lead = Regex::new(r"^[\s*#>_`\-]+").unwrap().replace(&head, "");

    let starts_refusal = REFUSAL_PHRASES.iter().any(|p| lead.starts_with(p));
    let hits: usize = REFUSAL_PHRASES
        .iter()
        .map(|p| head.matches(p).count())
        .sum();

    let has_fid = Regex::new(r"\bF[0-9]{2,}-[0-9]+\b").unwrap().is_match(text);
    let has_rc = Regex::new(r"\bRC[0-9]+\b").unwrap().is_match(text);
    let has_verdict = Regex::new(r"(?i)\bverdict\b").unwrap().is_match(text);
    let markers = numbered > 0 || has_fid || has_rc || has_verdict;

    if (starts_refusal || hits >= 2) && !markers {
        return ProseGate {
            substantive: false,
            reason: "reply looks like a refusal".into(),
            words,
            numbered,
        };
    }
    if (numbered >= 2 && words >= 25) || (numbered >= 1 && words >= 40) || words >= 120 {
        return ProseGate {
            substantive: true,
            reason: String::new(),
            words,
            numbered,
        };
    }
    ProseGate {
        substantive: false,
        reason: format!("reply too short ({words} words)"),
        words,
        numbered,
    }
}

/// How the reply was ingested.
#[derive(Debug, Clone)]
pub enum Ingestion {
    /// One valid consult-reply v1 object.
    Structured(Box<StructuredReply>),
    /// Prose (or invalid JSON): the gate decides whether a repair turn is attempted.
    Prose(ProseGate),
}

/// Classify a reply text: a valid object is [`Ingestion::Structured`]; anything else is
/// [`Ingestion::Prose`] carrying the gate verdict.
pub fn classify(raw_text: &str) -> Ingestion {
    match crate::engines::codex::parse_structured(raw_text) {
        Some(s) => Ingestion::Structured(Box::new(s)),
        None => Ingestion::Prose(prose_gate(raw_text)),
    }
}

/// The `validation_error` phrase when a prose reply does NOT earn a repair turn
/// (`(format repair not attempted: <reason>)`).
pub fn not_attempted_suffix(gate: &ProseGate) -> String {
    format!("(format repair not attempted: {})", gate.reason)
}

/// The first-reply `validation_error` (`ConvertFrom-StructuredReply`'s `ValidationError`): an
/// empty reply, `not valid JSON: <msg>` (the parser's message, truncated to 120 — the exact
/// wording is runtime-specific, a documented divergence), else the first schema error. Empty
/// string when the text IS a valid reply object.
pub fn first_validation_error(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return "empty reply".to_string();
    }
    let body = strip_fence(t);
    match serde_json::from_str::<serde_json::Value>(&body) {
        Err(e) => {
            let mut m = c3_core::one_line(&e.to_string());
            if m.chars().count() > 120 {
                m = m.chars().take(120).collect::<String>() + "...";
            }
            format!("not valid JSON: {m}")
        }
        Ok(_) => match serde_json::from_str::<c3_core::engine::RawReply>(&body) {
            Ok(raw) => match StructuredReply::try_from(raw) {
                Ok(_) => String::new(),
                Err(e) => c3_core::one_line(&e.to_string()),
            },
            Err(e) => format!("not valid JSON: {}", c3_core::one_line(&e.to_string())),
        },
    }
}

/// Strip a single ```lang ... ``` fence, mirroring `ConvertFrom-StructuredReply`'s fence net.
fn strip_fence(t: &str) -> String {
    let re = Regex::new(r"(?s)^```[A-Za-z0-9_-]*[ \t]*\r?\n(.*?)\r?\n[ \t]*```$").unwrap();
    if let Some(c) = re.captures(t) {
        c.get(1)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default()
    } else {
        t.to_string()
    }
}

// ------------------------------------------------------------- local reply normalisation (item 2)

/// A deterministic, LOCAL repair of a reply from an engine WITHOUT an enforced output schema (the
/// http engine). It runs ONLY after the strict parse has already failed, and it NEVER invents
/// content: it repairs *shape* — a Markdown fence, prose around the object, raw control characters
/// inside string literals, a single value where the schema wants an array, and a missing or numeric
/// `schema_version` — then validates strictly again. It returns the validated reply and a
/// human-readable list of every change, or `None` when the reply still does not validate (it then
/// stays INVALID, exactly as today).
///
/// This is the "no enforced output schema" capability: codex/agy/muse enforce the schema at the
/// engine and the plugin does no such repair, so their replies never pass through here — only the
/// http path calls it, and only on a failed strict parse.
pub fn normalise_reply(raw_text: &str) -> Option<(StructuredReply, Vec<String>)> {
    let mut notes: Vec<String> = Vec::new();

    // a. strip a Markdown code fence around the whole reply (reuses the existing fence net).
    let trimmed = raw_text.trim();
    let unfenced = strip_fence(trimmed);
    if unfenced != trimmed {
        notes.push("stripped a code fence".to_string());
    }

    // b. take the outermost JSON object when there is prose before or after it.
    let (obj_text, dropped_prose) = outermost_object(&unfenced)?;
    if dropped_prose {
        notes.push("took the outermost JSON object (dropped surrounding prose)".to_string());
    }

    // c. escape raw control characters (U+0000..U+001F) that occur INSIDE string literals.
    let (escaped, escaped_count) = escape_control_chars_in_strings(&obj_text);
    if escaped_count > 0 {
        notes.push(format!(
            "escaped {escaped_count} control character(s) inside string(s)"
        ));
    }

    // The text must now be valid JSON; if not, the reply stays INVALID.
    let mut value: Value = serde_json::from_str(&escaped).ok()?;

    // d/e. structural repairs on the parsed value (array wrapping, schema_version).
    structural_repairs(&mut value, &mut notes);

    // Validate strictly again — the same RawReply -> StructuredReply path parse_structured uses.
    let raw: c3_core::engine::RawReply = serde_json::from_value(value).ok()?;
    let structured = StructuredReply::try_from(raw).ok()?;
    Some((structured, notes))
}

/// The single-line note recorded and printed when a reply was normalised (`reply normalised: a; b`).
pub fn normalised_note(notes: &[String]) -> String {
    format!("reply normalised: {}", notes.join("; "))
}

/// Return the slice from the first `{` to its matching `}` (a string-aware brace scan), and whether
/// any prose was dropped from before or after it. `None` when there is no balanced object.
fn outermost_object(text: &str) -> Option<(String, bool)> {
    let bytes = text.as_bytes();
    let start = text.find('{')?;
    let (mut depth, mut in_str, mut esc, mut end) = (0i32, false, false, None);
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let dropped = start > 0 || end < bytes.len() - 1;
    Some((text[start..=end].to_string(), dropped))
}

/// Escape every raw control character (U+0000..U+001F) that occurs INSIDE a string literal, using
/// a byte/char state machine (never a regex over the whole text). Control characters OUTSIDE a
/// string (JSON whitespace between tokens) are left untouched. Returns the text and the count.
fn escape_control_chars_in_strings(text: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let (mut in_str, mut esc, mut count) = (false, false, 0usize);
    for ch in text.chars() {
        if in_str {
            if esc {
                out.push(ch);
                esc = false;
            } else if ch == '\\' {
                out.push(ch);
                esc = true;
            } else if ch == '"' {
                out.push(ch);
                in_str = false;
            } else if (ch as u32) < 0x20 {
                count += 1;
                match ch {
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    _ => out.push_str(&format!("\\u{:04x}", ch as u32)),
                }
            } else {
                out.push(ch);
            }
        } else {
            if ch == '"' {
                in_str = true;
            }
            out.push(ch);
        }
    }
    (out, count)
}

/// Structural repairs on the parsed value: `schema_version` default/coercion, and wrapping a single
/// object or string (or a `null`) into the array the schema requires for the named fields.
fn structural_repairs(value: &mut Value, notes: &mut Vec<String>) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    // e. schema_version: a missing key becomes "1"; a number 1 becomes "1".
    match obj.get("schema_version") {
        None => {
            obj.insert("schema_version".into(), Value::String("1".into()));
            notes.push("schema_version defaulted to \"1\"".into());
        }
        Some(Value::Number(n)) if n.as_i64() == Some(1) => {
            obj.insert("schema_version".into(), Value::String("1".into()));
            notes.push("schema_version coerced from 1 to \"1\"".into());
        }
        _ => {}
    }
    // d. top-level array fields.
    for field in [
        "findings",
        "prior_findings",
        "unproven",
        "first_run_checklist",
    ] {
        wrap_array_field(obj, field, notes);
    }
    // d. per-finding array fields.
    if let Some(Value::Array(findings)) = obj.get_mut("findings") {
        for f in findings.iter_mut() {
            if let Some(fo) = f.as_object_mut() {
                for field in ["locations", "evidence", "supersedes"] {
                    wrap_array_field(fo, field, notes);
                }
            }
        }
    }
}

/// Where the schema requires an array: a single object or string is wrapped in a one-element array,
/// and a `null` becomes `[]`. Any other value (already an array, a number, a bool) is left as is —
/// no content is invented.
fn wrap_array_field(obj: &mut Map<String, Value>, field: &str, notes: &mut Vec<String>) {
    match obj.get(field) {
        Some(Value::Null) => {
            obj.insert(field.into(), Value::Array(Vec::new()));
            notes.push(format!("{field}: null replaced with []"));
        }
        Some(Value::Object(_)) | Some(Value::String(_)) => {
            let v = obj.remove(field).unwrap();
            let kind = if v.is_object() { "object" } else { "string" };
            obj.insert(field.into(), Value::Array(vec![v]));
            notes.push(format!("{field}: {kind} wrapped in an array"));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_object_is_structured() {
        let json = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        assert!(matches!(classify(json), Ingestion::Structured(_)));
    }

    #[test]
    fn fenced_short_object_is_structured_not_short_prose() {
        // TRANSP: a prompt-only run returns a fenced JSON object; it must validate as
        // structured (not fall to the prose gate and count "```json"/blob/"```" as 3 words).
        let json = "```json\n{\"schema_version\":\"1\",\"verdict\":\"ADVISE\",\"verdict_reason\":\"r\",\"reply_markdown\":\"m\",\"findings\":[],\"prior_findings\":[],\"unproven\":[],\"first_run_checklist\":[]}\n```";
        assert!(matches!(classify(json), Ingestion::Structured(_)));
        assert!(first_validation_error(json).is_empty());
    }

    #[test]
    fn substantive_prose_earns_repair() {
        // one numbered answer, >= 40 words.
        let text = "Q1. The change is consistent with the surrounding module and does not \
            regress the existing behaviour, but the error path is untested and one edge \
            case around empty input is not covered by the current suite so far as I read.";
        let g = prose_gate(text);
        assert!(g.substantive, "reason: {}", g.reason);
        assert!(g.numbered >= 1);
    }

    #[test]
    fn refusal_is_not_substantive() {
        let text = "I'm sorry, I cannot help with that request.";
        let g = prose_gate(text);
        assert!(!g.substantive);
        assert_eq!(g.reason, "reply looks like a refusal");
    }

    #[test]
    fn short_reply_reason_names_word_count() {
        let text = "Looks fine.";
        let g = prose_gate(text);
        assert!(!g.substantive);
        assert_eq!(g.reason, "reply too short (2 words)");
        assert_eq!(
            not_attempted_suffix(&g),
            "(format repair not attempted: reply too short (2 words))"
        );
    }

    // ------------------------------------------------------------------ item 2: normalisation

    const FIXTURE_01: &str = include_str!("../../tests/fixtures/http/01-http-reply.reply.json");
    const FIXTURE_03: &str = include_str!("../../tests/fixtures/http/03-http-reply.reply.json");

    #[test]
    fn normalise_wraps_single_evidence_object_space_bunny() {
        // Space Bunny: findings[].evidence is ONE object where the schema wants an array; the
        // strict parse fails, the normaliser wraps it, and the reply becomes structured (2 findings).
        assert!(
            crate::engines::codex::parse_structured(FIXTURE_01).is_none(),
            "the fixture must fail the strict parse first"
        );
        let (s, notes) = normalise_reply(FIXTURE_01).expect("fixture 01 normalises");
        assert_eq!(s.findings.len(), 2);
        assert!(
            notes
                .iter()
                .any(|n| n.contains("evidence") && n.contains("wrapped in an array")),
            "notes: {notes:?}"
        );
    }

    #[test]
    fn normalise_escapes_raw_control_chars_nemotron() {
        // Nemotron: raw line breaks inside JSON strings (invalid JSON) plus single-object evidence;
        // the normaliser escapes the control chars and wraps evidence -> structured (3 findings).
        assert!(crate::engines::codex::parse_structured(FIXTURE_03).is_none());
        let (s, notes) = normalise_reply(FIXTURE_03).expect("fixture 03 normalises");
        assert_eq!(s.findings.len(), 3);
        assert!(
            notes.iter().any(|n| n.contains("control character")),
            "notes: {notes:?}"
        );
    }

    #[test]
    fn normalise_leaves_truncated_json_invalid() {
        // A truncated object cannot be balanced -> the reply stays INVALID (None), never invented.
        let truncated = r#"{"schema_version":"1","verdict":"ACCEPT","verdict_reason":"r","findings":[{"severity":"note""#;
        assert!(normalise_reply(truncated).is_none());
    }

    #[test]
    fn normalise_ignores_control_chars_outside_strings() {
        // A raw newline OUTSIDE any string is legal JSON whitespace: it is left as is, so the
        // escape step reports nothing.
        let with_outer_newline = "{\n  \"schema_version\": \"1\",\n  \"verdict\": \"ACCEPT\",\n  \"verdict_reason\": \"r\",\n  \"reply_markdown\": \"m\",\n  \"findings\": [],\n  \"prior_findings\": [],\n  \"unproven\": [],\n  \"first_run_checklist\": []\n}";
        let (_, notes) =
            normalise_reply(with_outer_newline).expect("valid-with-whitespace normalises");
        assert!(
            !notes.iter().any(|n| n.contains("control character")),
            "no control chars were inside a string: {notes:?}"
        );
    }

    #[test]
    fn normalise_is_idempotent_on_a_valid_object() {
        // Running the normaliser on an already-valid v1 object is a no-op: it validates and reports
        // no change (so normalising twice yields the same result — nothing is invented or altered).
        let valid = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[{"severity":"note","locations":[{"path":"a.rs","line":1}],"claim":"c","trigger":"t","evidence":[{"kind":"read-code","reference":"r","observation":"o"}],"verification":"v","remedy":"rm","supersedes":[]}],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        let (s1, notes) = normalise_reply(valid).expect("valid object normalises");
        assert!(
            notes.is_empty(),
            "a valid object needs no repair: {notes:?}"
        );
        // A second pass over the same text produces the identical structured reply.
        let (s2, _) = normalise_reply(valid).unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn refusal_wording_with_verdict_marker_still_gates_on_length() {
        // "verdict" marker cancels the refusal short-circuit; length floor then applies.
        let text = "I cannot. Verdict: ACCEPT.";
        let g = prose_gate(text);
        // Not a refusal (marker present), but too short → not substantive.
        assert!(!g.substantive);
        assert!(g.reason.starts_with("reply too short"));
    }
}
