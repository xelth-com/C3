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

use std::cell::RefCell;
use std::fmt;

use c3_core::engine::StructuredReply;
use regex::Regex;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
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
    validation_error_inner(text, true)
}

/// As [`first_validation_error`], but the http-only normaliser suffix (`; normaliser: <reason>`)
/// is appended ONLY for the http engine. codex/agy/muse replies get the plugin's validation text
/// byte for byte, with no suffix — the normaliser and its reason belong to the http engine alone.
pub fn first_validation_error_engine(text: &str, engine: &str) -> String {
    validation_error_inner(text, engine == "http")
}

fn validation_error_inner(text: &str, append_normaliser: bool) -> String {
    let t = text.trim();
    if t.is_empty() {
        return "empty reply".to_string();
    }
    let body = strip_fence(t);
    let strict = match serde_json::from_str::<serde_json::Value>(&body) {
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
    };
    if strict.is_empty() {
        return String::new();
    }
    // (N3) For the http path only, a reply shaped as a JSON object keeps the original strict error
    // and appends the normaliser's reason (the engine sets the repaired text as the reply-of-record
    // on success, so this fires only when the normaliser ALSO failed). codex/agy/muse never run the
    // normaliser, so their validation text stays byte-for-byte the plugin's.
    if append_normaliser && body.trim_start().starts_with('{') {
        if let Normalisation::Failed { reason } = normalise_reply(text) {
            return format!("{strict}; normaliser: {reason}");
        }
    }
    strict
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
pub fn normalise_reply(raw_text: &str) -> Normalisation {
    let mut notes: Vec<String> = Vec::new();

    // a. strip a Markdown code fence around the WHOLE reply (reuses the existing fence net).
    let trimmed = raw_text.trim();
    let unfenced = strip_fence(trimmed);
    if unfenced != trimmed {
        notes.push("stripped a code fence".to_string());
    }

    // b (N2). Collect every top-level balanced object (ignoring code fences / spans and tracking
    // strings), and keep only those that parse as an object carrying `verdict` and `findings`.
    // Exactly one such candidate is the reply; none or several is ambiguous and stays INVALID.
    let candidates: Vec<String> = top_level_objects(&unfenced)
        .into_iter()
        .filter(|o| is_reply_candidate(o))
        .collect();
    let obj_text = match candidates.len() {
        1 => candidates.into_iter().next().unwrap(),
        n => {
            return Normalisation::Failed {
                reason: format!("no single reply object found ({n} candidates)"),
            }
        }
    };
    if obj_text.trim() != unfenced.trim() {
        notes.push("prose around the object removed".to_string());
    }

    // c. escape raw control characters (U+0000..U+001F) that occur INSIDE string literals.
    let (escaped, escaped_count) = escape_control_chars_in_strings(&obj_text);
    if escaped_count > 0 {
        notes.push(format!(
            "escaped {escaped_count} control character(s) inside string(s)"
        ));
    }

    // d (N1). A duplicate-aware parse over the raw object text, BEFORE any lossy conversion to a
    // Value (which would silently keep the last of a duplicated key). Identical duplicates are
    // dropped with a note; different values keep the reply INVALID, naming the key, never the value.
    let (mut value, dup_keys) = match dedup_parse(&escaped) {
        Ok(v) => v,
        Err(reason) => return Normalisation::Failed { reason },
    };
    for k in dup_keys {
        notes.push(format!("duplicate key {k} with identical values removed"));
    }

    // e. structural repairs on the parsed value (array wrapping, schema_version).
    structural_repairs(&mut value, &mut notes);

    // Validate strictly again — the same RawReply -> StructuredReply path parse_structured uses.
    let json = serde_json::to_string(&value).unwrap_or_default();
    let raw: c3_core::engine::RawReply = match serde_json::from_value(value) {
        Ok(r) => r,
        Err(_) => {
            return Normalisation::Failed {
                reason: "the reply is not a v1 object after normalisation".to_string(),
            }
        }
    };
    match StructuredReply::try_from(raw) {
        Ok(reply) => Normalisation::Repaired { reply, json, notes },
        Err(e) => Normalisation::Failed {
            reason: c3_core::one_line(&e.to_string()),
        },
    }
}

/// The outcome of the local normaliser.
pub enum Normalisation {
    /// The reply was repaired into a valid structured object. `json` is the repaired reply text
    /// (valid v1 JSON, so the reply-of-record's strict parse succeeds downstream), and `notes`
    /// lists every change — never empty when anything was changed.
    Repaired {
        reply: StructuredReply,
        json: String,
        notes: Vec<String>,
    },
    /// The reply could not be made valid; `reason` is appended to the strict error in the summary.
    Failed { reason: String },
}

/// The single-line note recorded and printed when a reply was normalised (`reply normalised: a; b`).
pub fn normalised_note(notes: &[String]) -> String {
    format!("reply normalised: {}", notes.join("; "))
}

/// (N2) Collect every top-level balanced `{...}` object in `text`, skipping anything inside a
/// Markdown code fence (```) or inline code span (`), and tracking JSON string literals so a brace
/// inside a string is never a boundary. Objects nested inside another object are part of it, not
/// separate candidates.
fn top_level_objects(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut objs = Vec::new();
    let mut i = 0usize;
    let mut in_fence = false;
    let mut in_inline = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'`' {
            let mut n = 0usize;
            while i + n < bytes.len() && bytes[i + n] == b'`' {
                n += 1;
            }
            if n >= 3 {
                in_fence = !in_fence;
            } else if !in_fence && n % 2 == 1 {
                in_inline = !in_inline;
            }
            i += n;
            continue;
        }
        if in_fence || in_inline {
            i += 1;
            continue;
        }
        if b == b'"' {
            // A quoted span in the prose: skip to its close so a `{` inside it is not a boundary.
            i += 1;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => i += 2,
                    b'"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            continue;
        }
        if b == b'{' {
            match balanced_object_end(bytes, i) {
                Some(end) => {
                    objs.push(text[i..=end].to_string());
                    i = end + 1;
                }
                // An unbalanced `{` has no close to EOF, so no complete top-level object can begin
                // at or after it either — stop (this also keeps the scan linear).
                None => break,
            }
            continue;
        }
        i += 1;
    }
    objs
}

/// The index of the `}` closing the object at `start` (`bytes[start] == b'{'`), tracking JSON
/// string literals and escapes. `None` when the object is not balanced before EOF.
fn balanced_object_end(bytes: &[u8], start: usize) -> Option<usize> {
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
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
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether an object substring parses as a JSON object carrying `verdict` and `findings` (control
/// characters escaped first so a near-valid object still parses for this key check).
fn is_reply_candidate(obj_text: &str) -> bool {
    let (escaped, _) = escape_control_chars_in_strings(obj_text);
    match serde_json::from_str::<Value>(&escaped) {
        Ok(Value::Object(m)) => m.contains_key("verdict") && m.contains_key("findings"),
        _ => false,
    }
}

// --- (N1) duplicate-aware parse: a streaming walk that keeps every key/value pair at every depth.

#[derive(Default)]
struct DupState {
    /// Distinct keys that had a duplicate with an IDENTICAL value (dropped, one kept).
    identical: Vec<String>,
    /// The first key that had a duplicate with a DIFFERENT value (the reply stays INVALID).
    different: Option<String>,
}

thread_local! {
    static DUP_STATE: RefCell<DupState> = RefCell::new(DupState::default());
}

/// A JSON value that rejects a key repeated with a different value and records identical-value
/// duplicates, recursively at every object depth.
struct Dedup(Value);

impl<'de> Deserialize<'de> for Dedup {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Value;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON value")
            }
            fn visit_unit<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
                Ok(Value::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
                Ok(Value::from(v))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
                Ok(Value::from(v))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
                Ok(Value::from(v))
            }
            fn visit_str<E>(self, v: &str) -> Result<Value, E> {
                Ok(Value::String(v.to_string()))
            }
            fn visit_string<E>(self, v: String) -> Result<Value, E> {
                Ok(Value::String(v))
            }
            fn visit_none<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_some<D2>(self, d: D2) -> Result<Value, D2::Error>
            where
                D2: Deserializer<'de>,
            {
                Ok(Dedup::deserialize(d)?.0)
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut out = Vec::new();
                while let Some(Dedup(v)) = seq.next_element()? {
                    out.push(v);
                }
                Ok(Value::Array(out))
            }
            fn visit_map<A>(self, mut map: A) -> Result<Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut out = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    let Dedup(val) = map.next_value()?;
                    match out.get(&key) {
                        Some(existing) if existing == &val => DUP_STATE.with(|s| {
                            let mut s = s.borrow_mut();
                            if !s.identical.contains(&key) {
                                s.identical.push(key.clone());
                            }
                        }),
                        Some(_) => {
                            DUP_STATE.with(|s| {
                                let mut s = s.borrow_mut();
                                if s.different.is_none() {
                                    s.different = Some(key.clone());
                                }
                            });
                            return Err(de::Error::custom("duplicate key with different values"));
                        }
                        None => {
                            out.insert(key, val);
                        }
                    }
                }
                Ok(Value::Object(out))
            }
        }
        d.deserialize_any(V).map(Dedup)
    }
}

/// Parse `text` with duplicate-key detection. `Ok((value, identical_keys))` keeps one of each
/// identical-value duplicate; `Err(reason)` names a key repeated with a different value, or reports
/// unparseable JSON. Never echoes a value.
fn dedup_parse(text: &str) -> Result<(Value, Vec<String>), String> {
    DUP_STATE.with(|s| *s.borrow_mut() = DupState::default());
    let parsed: Result<Dedup, _> = serde_json::from_str(text);
    let (identical, different) = DUP_STATE.with(|s| {
        let s = s.borrow();
        (s.identical.clone(), s.different.clone())
    });
    match parsed {
        Ok(Dedup(v)) => Ok((v, identical)),
        Err(_) => match different {
            Some(k) => Err(format!("duplicate key {k} with different values")),
            None => Err("the reply is not a JSON object c3 could parse".to_string()),
        },
    }
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

    // -------------------------------------------------- item 2 / N1-N3: local reply normalisation

    const FIXTURE_01: &str = include_str!("../../tests/fixtures/http/01-http-reply.reply.json");
    const FIXTURE_03: &str = include_str!("../../tests/fixtures/http/03-http-reply.reply.json");
    const FIXTURE_04: &str = include_str!("../../tests/fixtures/http/04-http-reply.reply.json");

    fn repaired(n: Normalisation) -> (StructuredReply, String, Vec<String>) {
        match n {
            Normalisation::Repaired { reply, json, notes } => (reply, json, notes),
            Normalisation::Failed { reason } => panic!("expected Repaired, got Failed: {reason}"),
        }
    }
    fn failed_reason(n: Normalisation) -> String {
        match n {
            Normalisation::Failed { reason } => reason,
            Normalisation::Repaired { notes, .. } => {
                panic!("expected Failed, got Repaired: {notes:?}")
            }
        }
    }
    fn real_object() -> &'static str {
        r#"{"schema_version":"1","verdict":"REJECT","verdict_reason":"r","reply_markdown":"the real one","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#
    }

    #[test]
    fn normalise_wraps_single_evidence_object_space_bunny() {
        // Space Bunny: findings[].evidence is ONE object where the schema wants an array.
        assert!(crate::engines::codex::parse_structured(FIXTURE_01).is_none());
        let (s, _json, notes) = repaired(normalise_reply(FIXTURE_01));
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
        // Nemotron: raw line breaks inside JSON strings, plus single-object evidence -> 3 findings.
        assert!(crate::engines::codex::parse_structured(FIXTURE_03).is_none());
        let (s, _json, notes) = repaired(normalise_reply(FIXTURE_03));
        assert_eq!(s.findings.len(), 3);
        assert!(
            notes.iter().any(|n| n.contains("control character")),
            "notes: {notes:?}"
        );
    }

    // ---- N1: duplicate keys

    #[test]
    fn n1_duplicate_schema_version_identical_removed_fixture_04() {
        // The live review of the normaliser: `schema_version` written twice, both "1". A lossy
        // Value conversion would keep the last silently; the dedup pass keeps one and notes it.
        assert!(crate::engines::codex::parse_structured(FIXTURE_04).is_none());
        let (s, json, notes) = repaired(normalise_reply(FIXTURE_04));
        assert_eq!(s.findings.len(), 2);
        assert_eq!(s.verdict, c3_core::engine::Verdict::Reject);
        assert_eq!(
            notes,
            vec!["duplicate key schema_version with identical values removed".to_string()]
        );
        // The repaired reply-of-record is now strictly valid.
        assert!(crate::engines::codex::parse_structured(&json).is_some());
    }

    #[test]
    fn n1_duplicate_key_different_values_stays_invalid_and_names_the_key() {
        // verdict twice with different values must never collapse to a valid ACCEPT.
        let dup = r#"{"schema_version":"1","verdict":"REJECT","verdict":"ACCEPT","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        let reason = failed_reason(normalise_reply(dup));
        assert_eq!(reason, "duplicate key verdict with different values");
        assert!(
            !reason.contains("ACCEPT") && !reason.contains("REJECT"),
            "names the key, never the values: {reason}"
        );
    }

    // ---- N2: outermost object

    #[test]
    fn n2_fenced_sample_then_real_object_takes_the_real_object() {
        let sample = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"s","reply_markdown":"sample","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        let text = format!(
            "Ignore this sample:\n```json\n{sample}\n```\nActual reply:\n{}",
            real_object()
        );
        let (s, _json, notes) = repaired(normalise_reply(&text));
        assert_eq!(s.reply_markdown, "the real one");
        assert!(
            notes
                .iter()
                .any(|n| n.contains("prose around the object removed")),
            "notes: {notes:?}"
        );
    }

    #[test]
    fn n2_two_unfenced_candidates_stay_invalid() {
        let text = format!("{}\n\n{}", real_object(), real_object());
        assert_eq!(
            failed_reason(normalise_reply(&text)),
            "no single reply object found (2 candidates)"
        );
    }

    #[test]
    fn n2_brace_inside_a_string_does_not_confuse_the_walk() {
        // A `{` inside a prose string, and `{` inside reply_markdown, must not create a false
        // candidate nor truncate the real object.
        let obj = r#"{"schema_version":"1","verdict":"HOLD","verdict_reason":"r","reply_markdown":"use {braces} like {this}","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        let text = format!("He said \"the cost is {{x}} dollars\" and then:\n{obj}");
        let (s, _json, _notes) = repaired(normalise_reply(&text));
        assert_eq!(s.verdict, c3_core::engine::Verdict::Hold);
        assert_eq!(s.reply_markdown, "use {braces} like {this}");
    }

    // ---- truncated / control-chars-outside

    #[test]
    fn normalise_leaves_truncated_json_invalid() {
        let truncated = r#"prose {"schema_version":"1","verdict":"ACCEPT","verdict_reason":"r","findings":[{"severity":"note""#;
        assert!(matches!(
            normalise_reply(truncated),
            Normalisation::Failed { .. }
        ));
    }

    #[test]
    fn normalise_ignores_control_chars_outside_strings() {
        // A raw newline OUTSIDE any string is legal JSON whitespace; the escape step reports nothing.
        let with_outer_newline = "{\n  \"schema_version\": \"1\",\n  \"verdict\": \"ACCEPT\",\n  \"verdict_reason\": \"r\",\n  \"reply_markdown\": \"m\",\n  \"findings\": [],\n  \"prior_findings\": [],\n  \"unproven\": [],\n  \"first_run_checklist\": []\n}";
        let (_s, _json, notes) = repaired(normalise_reply(with_outer_newline));
        assert!(
            !notes.iter().any(|n| n.contains("control character")),
            "notes: {notes:?}"
        );
    }

    // ---- N3: notes discipline + the summary reason

    #[test]
    fn n3_notes_are_empty_only_when_nothing_changed() {
        let (_s, _j, notes) = repaired(normalise_reply(real_object()));
        assert!(
            notes.is_empty(),
            "a valid object needs no repair: {notes:?}"
        );
        let (_s, _j, notes) = repaired(normalise_reply(FIXTURE_04));
        assert!(!notes.is_empty(), "04 was changed (dedup)");
    }

    #[test]
    fn n3_summary_appends_normaliser_reason_for_json_object_replies() {
        // A JSON-object reply the normaliser also cannot repair: the strict error is KEPT and the
        // normaliser reason is appended after "; normaliser: ".
        let dup = r#"{"schema_version":"1","verdict":"REJECT","verdict":"ACCEPT","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        let ve = first_validation_error(dup);
        assert!(
            ve.contains("; normaliser: duplicate key verdict with different values"),
            "{ve}"
        );
        // A prose reply (codex-shaped, not starting with `{`) is unaffected — no normaliser suffix.
        let ve = first_validation_error("Looks good overall, but the error path is untested.");
        assert!(!ve.contains("normaliser:"), "{ve}");
    }

    #[test]
    fn normaliser_suffix_is_http_only_engine_gated() {
        // A JSON-object reply the normaliser cannot repair: the suffix is appended for the http
        // engine but NOT for codex/agy/muse (their validation text is the plugin's, byte for byte).
        let dup = r#"{"schema_version":"1","verdict":"REJECT","verdict":"ACCEPT","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        let http = first_validation_error_engine(dup, "http");
        assert!(http.contains("; normaliser: "), "{http}");
        for engine in ["codex", "agy", "muse"] {
            let ve = first_validation_error_engine(dup, engine);
            assert!(!ve.contains("normaliser:"), "{engine}: {ve}");
        }
    }

    // ---- the reviewer's two open questions: idempotence and linear cost

    #[test]
    fn normalise_is_idempotent() {
        // normalise(normalise(x)) == normalise(x): the repaired reply and json are a fixed point,
        // for all three fixtures and the N2 trigger.
        let sample = r#"{"schema_version":"1","verdict":"ADVISE","verdict_reason":"s","reply_markdown":"sample","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;
        let trigger = format!(
            "Ignore:\n```json\n{sample}\n```\nActual:\n{}",
            real_object()
        );
        for input in [FIXTURE_01, FIXTURE_03, FIXTURE_04, trigger.as_str()] {
            let (r1, j1, _) = repaired(normalise_reply(input));
            let (r2, j2, notes2) = repaired(normalise_reply(&j1));
            assert_eq!(r1, r2, "reply is a fixed point");
            assert_eq!(j1, j2, "json is a fixed point");
            assert!(
                notes2.is_empty(),
                "repaired json needs no further change: {notes2:?}"
            );
        }
    }

    #[test]
    fn normalise_is_linear_on_a_large_reply() {
        // A ~4 MB reply with 100 000 short strings normalises well under a generous bound.
        let mut unproven = String::from("[");
        for i in 0..100_000 {
            if i > 0 {
                unproven.push(',');
            }
            unproven.push_str("\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"");
        }
        unproven.push(']');
        let big = format!(
            r#"{{"schema_version":"1","verdict":"ACCEPT","verdict_reason":"r","reply_markdown":"m","findings":[],"prior_findings":[],"unproven":{unproven},"first_run_checklist":[]}}"#
        );
        assert!(
            big.len() > 3_000_000,
            "≈4 MB reply, got {} bytes",
            big.len()
        );
        let start = std::time::Instant::now();
        let out = normalise_reply(&big);
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "linear cost: {elapsed:?}"
        );
        assert!(matches!(out, Normalisation::Repaired { .. }));
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
