//! The ledger: `sessions.json` and its `LedgerEntry`.
//!
//! `sessions.json` is the commit point of a consultation (invariant 1, DESIGN §3): the
//! entry is inserted at its place by `n` and the file is the record the plugin and C3
//! share. This module mirrors the `$entry = [pscustomobject]@{ ... }` literal in
//! `codex-consult.ps1` (after the `# 4. sessions.json` comment) field for field, in the
//! literal's order - the order the wave-24 harness asserts (`tests/harness-0.3.ps1`,
//! `$order = 'n,when,purpose,...'`).
//!
//! Field order is load-bearing: serde serializes struct fields in declaration order, and
//! [`crate::ps_json`] preserves it, so the declaration order below IS the on-disk order.
//!
//! ## Schema tolerance (F02-6/7, F04-11, F08-1/2, F09-5)
//!
//! Every struct that mirrors a plugin file carries `#[serde(default)]` on the fields the
//! plugin may omit in an older file, and a trailing `#[serde(flatten)] extra` map so an
//! unknown member is preserved *in place* on rewrite. The four previously-unexercised
//! records - `range`, `denial_retry`, `timeout_continue`, `provider_failure` - are now
//! typed to their exact plugin literals (see each struct) rather than raw
//! `serde_json::Value`, with an `extra` map that keeps any field a later wave adds. `peak`
//! is confirmed a `boolean|null` scalar (`$peak.Peak`, `codex-consult.ps1:3892`), not an
//! object, so it is `Option<bool>`; `effort_confirmed` is always `$null` (a reserved
//! flag), also `Option<bool>`. `provider_failure.kind`/`.hint` are `Option` with
//! `skip_serializing_if`: the design evidence was written by a wave that emitted only the
//! five-field `{class, code, message, when, retry_after}` shape, while the current plugin
//! emits seven `{class, kind, code, message, when, retry_after, hint}`; making the two new
//! members omittable keeps the five-field evidence byte-identical and still round-trips a
//! seven-field file in order.
//!
//! Acceptance test: `tests/formats.rs` reads the real `.collab/c3-design/sessions.json`,
//! parses it into [`SessionsFile`] and re-serializes with [`crate::ps_json`]; the bytes
//! are identical.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ps_json;

/// The whole `sessions.json` document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionsFile {
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub codex: LedgerBook,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The `codex` object: the harness version string and the consultations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedgerBook {
    /// The Codex CLI version (`codex-cli 0.155.1`); an engine run leaves it unchanged.
    #[serde(default)]
    pub tool: String,
    /// The ledger entries, kept sorted by `n` (`Add-LedgerEntry`, D10).
    #[serde(default)]
    pub consults: Vec<LedgerEntry>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One consultation. The field order is the asserted wave-24 order and MUST NOT change.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedgerEntry {
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub consult_id: String,
    #[serde(default)]
    pub reviewer: Reviewer,
    #[serde(default)]
    pub lineage: String,
    #[serde(default)]
    pub preflight: String,
    #[serde(default)]
    pub preflight_warning: String,
    #[serde(default)]
    pub roster: RosterRef,
    /// `null` for a single (non-panel) run.
    #[serde(default)]
    pub panel: Option<Panel>,
    #[serde(default)]
    pub parent_thread: String,
    #[serde(default)]
    pub thread: String,
    #[serde(default)]
    pub thread_source: String,
    #[serde(default)]
    pub thread_candidate: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub brief: String,
    /// The `-Range` record (wave 24); `null` when the whole brief was reviewed.
    #[serde(default)]
    pub range: Option<RangeRecord>,
    #[serde(default)]
    pub prompt_chars: i64,
    #[serde(default)]
    pub reply: String,
    #[serde(default)]
    pub reply_json: String,
    #[serde(default)]
    pub events: String,
    #[serde(default)]
    pub partial_reply: String,
    #[serde(default)]
    pub model: String,
    /// The effort actually sent (= `effort_sent`); `null` when nothing was sent.
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub effort_requested: String,
    #[serde(default)]
    pub effort_sent: Option<String>,
    #[serde(default)]
    pub effort_mapping: String,
    #[serde(default)]
    pub effort_caps: String,
    /// Reserved for provider confirmation; the bridge always writes `null`
    /// (`codex-consult.ps1:3884`).
    #[serde(default)]
    pub effort_confirmed: Option<bool>,
    #[serde(default)]
    pub max_words: i64,
    #[serde(default)]
    pub sandbox: String,
    #[serde(default)]
    pub timeout_sec: i64,
    #[serde(default)]
    pub timeout_source: String,
    #[serde(default)]
    pub continue_sec: i64,
    /// The applied `-CodexConfig` items (`key=value` strings).
    #[serde(default)]
    pub extra_config: Vec<Value>,
    #[serde(default)]
    pub extra_config_source: String,
    /// The peak-window state: `$peak.Peak`, a `boolean|null` scalar
    /// (`codex-consult.ps1:3892`), not an object.
    #[serde(default)]
    pub peak: Option<bool>,
    #[serde(default)]
    pub peak_schedule: String,
    #[serde(default)]
    pub peak_source: String,
    #[serde(default)]
    pub peak_evaluated_at: String,
    #[serde(default)]
    pub structured: bool,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub schema_transport: String,
    #[serde(default)]
    pub schema_transport_source: String,
    #[serde(default)]
    pub validation_error: String,
    /// The format-repair record when a prose reply was converted; else `null`.
    #[serde(default)]
    pub format_retry: Option<FormatRetry>,
    /// The denial-retry record (a tool was auto-denied and one more turn was run); `null`
    /// when no retry happened.
    #[serde(default)]
    pub denial_retry: Option<DenialRetry>,
    /// The timeout-continuation record; `null` when the main turn was not killed.
    #[serde(default)]
    pub timeout_continue: Option<TimeoutContinue>,
    #[serde(default)]
    pub base_commit: String,
    #[serde(default)]
    pub reviewed_revision: String,
    #[serde(default)]
    pub tree_sha256: String,
    #[serde(default)]
    pub tree_sha256_after: String,
    #[serde(default)]
    pub tree_changed_during_review: bool,
    /// The count of changed files at review time (a number, not a list).
    #[serde(default)]
    pub changed_files: i64,
    #[serde(default)]
    pub brief_sha256: String,
    #[serde(default)]
    pub brief_sha256_after: String,
    #[serde(default)]
    pub brief_changed_during_review: bool,
    #[serde(default)]
    pub fingerprint_note: String,
    #[serde(default)]
    pub artifacts: Vec<Value>,
    #[serde(default)]
    pub artifacts_changed_during_review: bool,
    #[serde(default)]
    pub bridge_outcome: String,
    /// The provider's own failure classification; `null` on success.
    #[serde(default)]
    pub provider_failure: Option<ProviderFailure>,
    #[serde(default)]
    pub warnings: Vec<Value>,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub verdict_reason: String,
    #[serde(default)]
    pub findings: FindingCounts,
    /// The ids raised by this reply, in id order (`F<NN>-<k>`).
    #[serde(default)]
    pub finding_ids: Vec<Value>,
    /// This reply's report on prior findings: `{id, status}`.
    #[serde(default)]
    pub prior_findings: Vec<PriorFindingRef>,
    #[serde(default)]
    pub unchecked_prior_blockers: Vec<Value>,
    /// Token usage; `null` for an engine that does not report it.
    #[serde(default)]
    pub usage: Option<Usage>,
    /// The engine-run record (turns); `null` for a plain single Codex turn.
    #[serde(default)]
    pub engine_run: Option<EngineRun>,
    /// Wall time in seconds. A whole value is written without a decimal point (see
    /// [`crate::ps_json`]), so this field carries `243.4` and `277` alike.
    #[serde(default)]
    pub wall_seconds: f64,
    #[serde(default)]
    pub finished_at: String,
    #[serde(default)]
    pub commit_wait_ms: i64,
    /// Unknown members a later wave adds, preserved in place on rewrite.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.reviewer`: identity as `Resolve-ReviewerIdentity` recorded it. Absent entirely in
/// a pre-0.3 store (F02-6), so the whole record defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Reviewer {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub provider_source: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub model_source: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub harness: String,
    #[serde(default)]
    pub provider_fingerprint: String,
    /// Polymorphic endpoint descriptor: `{base_url,name,wire_api}` for a Codex provider,
    /// `{engine,launcher[,credential_mechanism]}` for an engine, `{builtin:true}` for the
    /// built-in OpenAI endpoint, or a `{base_url,base_url_source}` `OPENAI_BASE_URL` shape.
    #[serde(default)]
    pub provider_config: Value,
    #[serde(default)]
    pub identity_note: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.roster`: which roster entry supplied the identity, and what it applied.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RosterRef {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub position: i64,
    /// Skipped roster fields (element shape not exercised here) - raw JSON.
    #[serde(default)]
    pub skipped: Vec<Value>,
    /// Applied roster field names (`model`, `provider`, ...).
    #[serde(default)]
    pub applied: Vec<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.panel`: the panel this run belonged to.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Panel {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub of: i64,
    #[serde(default)]
    pub members: Vec<PanelMember>,
    #[serde(default)]
    pub concurrency: i64,
    /// Per-provider concurrency caps: a `provider -> count` map whose keys vary per run.
    #[serde(default)]
    pub limits: Value,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One member row of a panel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PanelMember {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub reason: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.range`: the `-Range` record (`codex-consult.ps1:1134`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RangeRecord {
    #[serde(default)]
    pub spec: String,
    #[serde(default)]
    pub files: i64,
    #[serde(default)]
    pub insertions: i64,
    #[serde(default)]
    pub deletions: i64,
    #[serde(default)]
    pub lines: i64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.format_retry`: the format-repair turn (`codex-consult.ps1:3455`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FormatRetry {
    #[serde(default)]
    pub attempted: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub succeeded: bool,
    #[serde(default)]
    pub thread: String,
    #[serde(default)]
    pub wall_seconds: f64,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub drift: Vec<Value>,
    #[serde(default)]
    pub original: String,
    /// The repair turn's event stream; `null` when it was not kept.
    #[serde(default)]
    pub events: Option<String>,
    #[serde(default)]
    pub schema_transport: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.denial_retry`: the auto-denial retry turn (`codex-consult.ps1:3026`). Field order:
/// attempted, reason, succeeded, thread, wall_seconds, usage, events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DenialRetry {
    #[serde(default)]
    pub attempted: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub succeeded: bool,
    #[serde(default)]
    pub thread: String,
    #[serde(default)]
    pub wall_seconds: f64,
    /// The retry turn's usage; `null`/absent when the engine reports none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// The retry turn's event stream; `null` when it was not kept.
    #[serde(default)]
    pub events: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.timeout_continue`: the post-timeout continuation turn
/// (`codex-consult.ps1:3114`/`3233`). Field order: thread, wall_seconds, outcome, events,
/// usage. Both `events` and `usage` are `null` on the "not attempted" path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimeoutContinue {
    #[serde(default)]
    pub thread: String,
    #[serde(default)]
    pub wall_seconds: f64,
    #[serde(default)]
    pub outcome: String,
    /// The continuation turn's event stream; `null` when it was not attempted/kept.
    #[serde(default)]
    pub events: Option<String>,
    /// The continuation turn's usage; `null` when it was not attempted or not reported.
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.usage` / `format_retry.usage`: token counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub cached_input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub reasoning_output_tokens: i64,
    /// Present for engines that report a total (agy); absent for codex. Always last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<i64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.provider_failure`: the provider's own error classification
/// (`New-ProviderFailure`, `codex-consult-common.ps1:4845`). The design evidence carries
/// the five-field shape `{class, code, message, when, retry_after}`; the current plugin
/// also emits `kind` (after `class`) and `hint` (last), so both are omittable `Option`s.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderFailure {
    #[serde(default)]
    pub class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub when: String,
    /// The provider's reset time; `null` when it named none.
    #[serde(default)]
    pub retry_after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.engine_run`: engine-turn accounting (agy/muse).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineRun {
    #[serde(default)]
    pub turns: i64,
    #[serde(default)]
    pub max_model_steps: Option<i64>,
    #[serde(default)]
    pub msp_schema_version: Option<i64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.findings`: the severity histogram of this reply.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindingCounts {
    #[serde(default)]
    pub blocker: i64,
    #[serde(default)]
    pub major: i64,
    #[serde(default)]
    pub minor: i64,
    #[serde(default)]
    pub note: i64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.prior_findings[]`: this reply's report on an earlier finding.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PriorFindingRef {
    #[serde(default)]
    pub id: String,
    /// The reviewer's own report: `fixed | still-open | not-checked | unknown-id`.
    #[serde(default)]
    pub status: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Why [`SessionsFile::add_entry`] refused an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddEntryError {
    /// An entry with this `n` is already in the ledger.
    DuplicateN(i64),
    /// An entry with this non-empty `consult_id` is already in the ledger.
    DuplicateConsultId(String),
}

impl std::fmt::Display for AddEntryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddEntryError::DuplicateN(n) => {
                write!(f, "a ledger entry with n={n} is already present")
            }
            AddEntryError::DuplicateConsultId(id) => {
                write!(
                    f,
                    "a ledger entry with consult_id '{id}' is already present"
                )
            }
        }
    }
}

impl std::error::Error for AddEntryError {}

impl SessionsFile {
    /// Parse `sessions.json` bytes.
    pub fn read(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Serialize to the exact on-disk byte stream `Write-JsonFile` would produce.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        ps_json::to_ps_json_bytes(self)
    }

    /// The next consult number `N`: the `Get-NextNumbers` N-half over the ledger only
    /// (`max(count, max n) + 1`). The findings/leftover half is folded in by the store,
    /// which has those inputs; see [`crate::store::EvidenceStore::next_numbers`].
    pub fn next_consult_n(&self) -> i64 {
        let mut max_n = self.codex.consults.len() as i64;
        for e in &self.codex.consults {
            if e.n > max_n {
                max_n = e.n;
            }
        }
        max_n + 1
    }

    /// Insert `entry` at its place by `n` (mirrors `Add-LedgerEntry`, D10): the entry lands
    /// after every entry whose `n` is not greater than its own, so the list stays sorted by
    /// `n` whatever order a panel's members commit in. Refuses a duplicate `n` or a
    /// duplicate non-empty `consult_id` (F03-4, F04-3, F08-8) so a replayed commit cannot
    /// double an entry.
    pub fn add_entry(&mut self, entry: LedgerEntry) -> Result<(), AddEntryError> {
        for e in &self.codex.consults {
            if e.n == entry.n {
                return Err(AddEntryError::DuplicateN(entry.n));
            }
            if !entry.consult_id.is_empty() && e.consult_id == entry.consult_id {
                return Err(AddEntryError::DuplicateConsultId(entry.consult_id.clone()));
            }
        }
        let mine = entry.n;
        let mut at = self.codex.consults.len();
        for i in (0..self.codex.consults.len()).rev() {
            if self.codex.consults[i].n > mine {
                at = i;
            } else {
                break;
            }
        }
        self.codex.consults.insert(at, entry);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: i64, id: &str) -> LedgerEntry {
        LedgerEntry {
            n,
            consult_id: id.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn add_entry_keeps_sorted_by_n() {
        let mut s = SessionsFile::default();
        s.add_entry(entry(3, "c")).unwrap();
        s.add_entry(entry(1, "a")).unwrap();
        s.add_entry(entry(2, "b")).unwrap();
        let ns: Vec<i64> = s.codex.consults.iter().map(|e| e.n).collect();
        assert_eq!(ns, vec![1, 2, 3]);
    }

    #[test]
    fn add_entry_refuses_duplicates() {
        let mut s = SessionsFile::default();
        s.add_entry(entry(1, "a")).unwrap();
        assert_eq!(
            s.add_entry(entry(1, "z")),
            Err(AddEntryError::DuplicateN(1))
        );
        assert_eq!(
            s.add_entry(entry(2, "a")),
            Err(AddEntryError::DuplicateConsultId("a".into()))
        );
    }

    #[test]
    fn next_consult_n_is_max_plus_one() {
        let mut s = SessionsFile::default();
        assert_eq!(s.next_consult_n(), 1);
        s.add_entry(entry(1, "a")).unwrap();
        s.add_entry(entry(2, "b")).unwrap();
        assert_eq!(s.next_consult_n(), 3);
        // a gap: n jumps past the count, N follows the max.
        s.add_entry(entry(9, "c")).unwrap();
        assert_eq!(s.next_consult_n(), 10);
    }
}
