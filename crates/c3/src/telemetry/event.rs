//! The T-hub event and its typed `details` allowlist.
//!
//! Privacy by construction (DESIGN §3 invariant 8, README "Telemetry"): the payload is
//! a fixed set of numeric, boolean and *label* fields. No field carries free text, a
//! path, a task name, a prompt, a brief, a thread id, a finding text or anything
//! key-shaped, because the four label fields ([`Details::engine`], `provider`, `model`,
//! `purpose`) are the only `String`s copied from the ledger and each passes through
//! [`safe_label`]/[`engine_label`] first: a value with whitespace, a `/`, `\`, `=`, a
//! control char, a secret-name token, or an unexpected charset is replaced by a
//! placeholder rather than echoed. The seeded-secret test in `tests/telemetry.rs` builds
//! an event from a ledger entry with secrets and paths in every text field and asserts
//! none of them reach the serialized payload.

use serde::Serialize;

use c3_core::ledger::LedgerEntry;

/// The C3 crate version, sent as `app_version`.
pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Secret-name tokens the server scrubs (README "The four rules"); a label containing any
/// of them is dropped client-side so `scrubbed` stays zero (invariant: a non-zero
/// `scrubbed` is our bug).
const SECRET_TOKENS: [&str; 6] = [
    "key",
    "token",
    "secret",
    "password",
    "authorization",
    "bearer",
];

/// One telemetry event: the exact T-hub event shape (README "The event").
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub app_id: &'static str,
    pub app_version: &'static str,
    pub instance_id: String,
    pub event_type: &'static str,
    pub severity: &'static str,
    /// The purpose label (same safe value as `details.purpose`); never free text.
    pub title: String,
    pub details: Details,
    /// RFC 3339 UTC, seconds precision.
    pub client_time: String,
    pub os: String,
    pub runtime: String,
    pub tags: Vec<String>,
}

/// The `details` object: the typed allowlist. Field order matches the public keys the
/// T-hub page shows (README "The event").
#[derive(Debug, Clone, Serialize)]
pub struct Details {
    /// `codex | agy | muse | http | other`.
    pub engine: String,
    /// Provider label (safe) or `unknown`.
    pub provider: String,
    /// Model label (safe) or `unknown`.
    pub model: String,
    /// Purpose label (safe) or `unknown`.
    pub purpose: String,
    /// `usable` or `failed:<class>`.
    pub outcome: String,
    pub wall_seconds: f64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub findings: i64,
    pub structured: bool,
    pub format_retry: bool,
    pub panel_size: u32,
    pub os: String,
    pub runtime: String,
    /// A tag from a fixed vocabulary; `None` for now (DESIGN §8). Never free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic_tag: Option<String>,
    /// The coordinator's usefulness mark (`yes | partly | no`) once known; else absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub useful: Option<String>,
    /// Verified finding count once findings settle; else absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<i64>,
    /// Rejected finding count once findings settle; else absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected: Option<i64>,
}

impl Event {
    /// Build the event for one consultation from its ledger entry. `panel_size` is the
    /// coordinator's known seat count; when `None` it falls back to `entry.panel.of` or 1.
    /// `instance_id` is passed in so the caller controls when the salt file is touched.
    pub fn from_ledger(entry: &LedgerEntry, panel_size: Option<u32>, instance_id: &str) -> Self {
        let purpose = label_or(&entry.purpose, 48);
        let (tokens_in, tokens_out) = entry
            .usage
            .as_ref()
            .map(|u| (u.input_tokens, u.output_tokens))
            .unwrap_or((0, 0));
        let findings = entry.findings.blocker
            + entry.findings.major
            + entry.findings.minor
            + entry.findings.note;
        let panel_size = panel_size.unwrap_or_else(|| {
            entry
                .panel
                .as_ref()
                .map(|p| p.of)
                .filter(|&of| of > 0)
                .map(|of| of as u32)
                .unwrap_or(1)
        });
        let details = Details {
            engine: engine_label(&entry.reviewer.engine),
            provider: label_or(&entry.reviewer.provider, 64),
            model: label_or(&entry.reviewer.model, 64),
            purpose: purpose.clone(),
            outcome: outcome_class(entry),
            wall_seconds: entry.wall_seconds,
            tokens_in,
            tokens_out,
            findings,
            structured: entry.structured,
            format_retry: entry
                .format_retry
                .as_ref()
                .map(|f| f.attempted)
                .unwrap_or(false),
            panel_size,
            os: os_label(),
            runtime: runtime_label(),
            topic_tag: None,
            useful: None,
            verified: None,
            rejected: None,
        };
        Event {
            app_id: "c3",
            app_version: APP_VERSION,
            instance_id: instance_id.to_string(),
            event_type: "consultation",
            severity: "info",
            title: purpose,
            details,
            client_time: now_rfc3339(),
            os: os_label(),
            runtime: runtime_label(),
            tags: Vec::new(),
        }
    }
}

/// The fixed topic vocabulary v1 (M9 §7 D-b, DESIGN §8): a rating (or consultation) carries a
/// topic tag ONLY when it maps to one of these; anything else is sent as `other`, never the
/// free text. The router's priors lookup maps the request's topics through the same function,
/// so a local free slug outside the vocabulary has no topic-level cell and falls to the
/// purpose cell, while local rating evidence keeps matching the raw slug exactly (as R15 does).
pub(crate) const TOPIC_VOCAB: [&str; 16] = [
    "architecture",
    "api",
    "protocol",
    "security",
    "correctness",
    "concurrency",
    "performance",
    "data",
    "storage",
    "testing",
    "build",
    "packaging",
    "docs",
    "ui",
    "dependencies",
    "observability",
];

/// Map a raw topic to its fixed-vocabulary slug, applying the v1 aliases first, or `None` when
/// it is not a known class. A secret hidden in a topic never survives (it is not in the
/// vocabulary), so nothing free-text can be echoed.
pub(crate) fn topic_slug(raw: &str) -> Option<&'static str> {
    let s = safe_label(raw, 48)?.to_ascii_lowercase();
    let mapped = match s.as_str() {
        "tests" | "test" => "testing",
        "perf" => "performance",
        "doc" | "documentation" => "docs",
        "deps" => "dependencies",
        "db" | "database" => "storage",
        "auth" => "security",
        other => other,
    };
    TOPIC_VOCAB.iter().copied().find(|v| *v == mapped)
}

/// The topic tag for the wire: the vocabulary slug, or `other`.
pub(crate) fn topic_label(raw: &str) -> String {
    topic_slug(raw)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "other".to_string())
}

/// A rating event: the coordinator's later usefulness mark for a consultation (`c3 findings
/// --rate`), reaching the hub as its own allowlisted event (M9 §7). Carries only classes:
/// the lineage labels, the purpose label, the topic tags from the fixed vocabulary, the mark
/// and the consultation's age in days — no ids, no paths, no free text.
#[derive(Debug, Clone, Serialize)]
pub struct RatingEvent {
    pub app_id: &'static str,
    pub app_version: &'static str,
    pub instance_id: String,
    pub event_type: &'static str,
    pub severity: &'static str,
    pub title: String,
    pub details: RatingDetails,
    pub client_time: String,
    pub os: String,
    pub runtime: String,
    pub tags: Vec<String>,
}

/// The `details` allowlist of a rating event.
#[derive(Debug, Clone, Serialize)]
pub struct RatingDetails {
    pub engine: String,
    pub provider: String,
    pub model: String,
    pub purpose: String,
    /// `yes | partly | no`.
    pub mark: String,
    /// The consultation's age in days at the time it was rated.
    pub age_days: i64,
    pub os: String,
    pub runtime: String,
    /// Topic tags from the fixed vocabulary (never free text); empty when none apply.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub topic_tags: Vec<String>,
}

impl RatingEvent {
    /// Build a rating event from the rated consultation's ledger entry, the mark and the age
    /// in days. Every text input passes through [`safe_label`]/[`engine_label`]/[`topic_label`],
    /// so no path, prompt, id or secret can reach the payload.
    pub fn from_rating(
        entry: &LedgerEntry,
        mark: &str,
        age_days: i64,
        raw_topics: &[String],
        instance_id: &str,
    ) -> Self {
        let purpose = label_or(&entry.purpose, 48);
        let mark = match mark.trim().to_ascii_lowercase().as_str() {
            m @ ("yes" | "partly" | "no") => m.to_string(),
            _ => "other".to_string(),
        };
        let mut topic_tags: Vec<String> = Vec::new();
        for t in raw_topics {
            let tag = topic_label(t);
            if tag != "other" && !topic_tags.contains(&tag) {
                topic_tags.push(tag);
            }
        }
        let details = RatingDetails {
            engine: engine_label(&entry.reviewer.engine),
            provider: label_or(&entry.reviewer.provider, 64),
            model: label_or(&entry.reviewer.model, 64),
            purpose: purpose.clone(),
            mark,
            age_days,
            os: os_label(),
            runtime: runtime_label(),
            topic_tags,
        };
        RatingEvent {
            app_id: "c3",
            app_version: APP_VERSION,
            instance_id: instance_id.to_string(),
            event_type: "rating",
            severity: "info",
            title: purpose,
            details,
            client_time: now_rfc3339(),
            os: os_label(),
            runtime: runtime_label(),
            tags: Vec::new(),
        }
    }
}

/// The outcome class from the ledger: `usable`, or `failed:<class>` where the class is a
/// safe label taken from the provider failure (preferred) or the bridge outcome.
fn outcome_class(entry: &LedgerEntry) -> String {
    if let Some(pf) = &entry.provider_failure {
        let class = safe_label(&pf.class, 48).unwrap_or_else(|| "unknown".to_string());
        return format!("failed:{class}");
    }
    let bo = entry.bridge_outcome.trim();
    let usable = bo.is_empty()
        || bo.eq_ignore_ascii_case("ok")
        || bo.eq_ignore_ascii_case("usable")
        || bo.eq_ignore_ascii_case("success")
        || bo.eq_ignore_ascii_case("completed")
        || bo.eq_ignore_ascii_case("done");
    if usable {
        "usable".to_string()
    } else {
        let class = safe_label(bo, 48).unwrap_or_else(|| "unknown".to_string());
        format!("failed:{class}")
    }
}

/// Map an engine name to the closed vocabulary; anything else is `other` (never echoed).
fn engine_label(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "codex" => "codex",
        "agy" => "agy",
        "muse" => "muse",
        "http" => "http",
        _ => "other",
    }
    .to_string()
}

/// A label that is safe or the placeholder `unknown`.
fn label_or(raw: &str, max: usize) -> String {
    safe_label(raw, max).unwrap_or_else(|| "unknown".to_string())
}

/// Return the label only if it is a plain, short, secret-free token; otherwise `None`.
/// Rejects whitespace, control chars, `\`, `=`, over-length values, secret-name tokens, and
/// anything outside `[A-Za-z0-9._:/-]` starting with an alphanumeric. A single `/` is allowed
/// so an OpenRouter model id (`openai/gpt-5`) survives as its own label; two or more `/`, or a
/// `\`, still reads as a path and is dropped. This keeps paths, prompts, free text and
/// key-shaped strings impossible to echo.
pub(crate) fn safe_label(raw: &str, max: usize) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > max {
        return None;
    }
    if s.chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\' || c == '=')
    {
        return None;
    }
    // One `/` is a provider-qualified model id (`openai/gpt-5`); more than one is a path.
    if s.matches('/').count() > 1 {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if SECRET_TOKENS.iter().any(|t| lower.contains(t)) {
        return None;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return None,
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-' | '/'))
    {
        return None;
    }
    Some(s.to_string())
}

/// A friendly OS label from the compile-time target.
fn os_label() -> String {
    match std::env::consts::OS {
        "windows" => "Windows",
        "linux" => "Linux",
        "macos" => "macOS",
        other => other,
    }
    .to_string()
}

/// The runtime label: a native Rust binary, tagged with its crate version.
fn runtime_label() -> String {
    format!("rust {APP_VERSION}")
}

/// Now as RFC 3339 UTC with seconds precision, matching the T-hub example.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_label_allows_one_slash_model_id() {
        // OpenRouter model ids carry one `/` and must survive as a label.
        assert_eq!(
            safe_label("openai/gpt-5", 64),
            Some("openai/gpt-5".to_string())
        );
        assert_eq!(
            safe_label("anthropic/claude-opus-4.1", 64),
            Some("anthropic/claude-opus-4.1".to_string())
        );
    }

    #[test]
    fn safe_label_still_rejects_paths_and_secrets() {
        // Two or more `/` is a path.
        assert_eq!(safe_label("crates/c3/src/lib.rs", 64), None);
        // A leading `/` is not an alphanumeric start.
        assert_eq!(safe_label("/etc/passwd", 64), None);
        // A backslash path is still rejected.
        assert_eq!(safe_label("crates\\c3", 64), None);
        // Secret-name tokens are still dropped even with a legal `/`.
        assert_eq!(safe_label("provider/api_key", 64), None);
        // `=` still rejected.
        assert_eq!(safe_label("a=b", 64), None);
    }
}
