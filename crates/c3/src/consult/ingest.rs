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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_object_is_structured() {
        let json = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        assert!(matches!(classify(json), Ingestion::Structured(_)));
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
