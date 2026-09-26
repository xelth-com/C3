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
//! Every field the literal writes is present (never skipped); a field the literal writes
//! as `$null` is an `Option` that serializes to `null`. Genuinely polymorphic fields -
//! `reviewer.provider_config` (a `{base_url,name,wire_api}` table, or `{engine,launcher}`
//! for an engine, or `{builtin:true}`, or an `OPENAI_BASE_URL` shape), `panel.limits` (a
//! `provider -> count` map with per-run keys), `range`, `extra_config`, `artifacts` and
//! the reserved retry records - stay as [`serde_json::Value`] so their exact on-disk shape
//! survives untouched.
//!
//! Acceptance test: `tests/formats.rs` reads the real `.collab/c3-design/sessions.json`,
//! parses it into [`SessionsFile`] and re-serializes with [`crate::ps_json`]; the bytes
//! are identical.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ps_json;

/// The whole `sessions.json` document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsFile {
    pub task_id: String,
    pub cwd: String,
    pub codex: LedgerBook,
}

/// The `codex` object: the harness version string and the consultations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerBook {
    /// The Codex CLI version (`codex-cli 0.155.1`); an engine run leaves it unchanged.
    pub tool: String,
    /// The ledger entries, kept sorted by `n` (`Add-LedgerEntry`, D10).
    pub consults: Vec<LedgerEntry>,
}

/// One consultation. The field order is the asserted wave-24 order and MUST NOT change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub n: i64,
    pub when: String,
    pub purpose: String,
    pub consult_id: String,
    pub reviewer: Reviewer,
    pub lineage: String,
    pub preflight: String,
    pub preflight_warning: String,
    pub roster: RosterRef,
    /// `null` for a single (non-panel) run.
    pub panel: Option<Panel>,
    pub parent_thread: String,
    pub thread: String,
    pub thread_source: String,
    pub thread_candidate: String,
    pub mode: String,
    pub command: String,
    pub brief: String,
    /// The `-Range` record (wave 24); `null` when the whole brief was reviewed. Shape is
    /// not exercised by the design evidence - kept as raw JSON. See contracts.md.
    pub range: Option<Value>,
    pub prompt_chars: i64,
    pub reply: String,
    pub reply_json: String,
    pub events: String,
    pub partial_reply: String,
    pub model: String,
    /// The effort actually sent (= `effort_sent`); `null` when nothing was sent.
    pub effort: Option<String>,
    pub effort_requested: String,
    pub effort_sent: Option<String>,
    pub effort_mapping: String,
    pub effort_caps: String,
    /// Reserved for provider confirmation; the bridge always writes `null`.
    pub effort_confirmed: Option<bool>,
    pub max_words: i64,
    pub sandbox: String,
    pub timeout_sec: i64,
    pub timeout_source: String,
    pub continue_sec: i64,
    /// The applied `-CodexConfig` items (`key=value` strings).
    pub extra_config: Vec<Value>,
    pub extra_config_source: String,
    /// The peak-window record; `null` when no schedule applied.
    pub peak: Option<Value>,
    pub peak_schedule: String,
    pub peak_source: String,
    pub peak_evaluated_at: String,
    pub structured: bool,
    pub schema: String,
    pub schema_transport: String,
    pub schema_transport_source: String,
    pub validation_error: String,
    /// The format-repair record when a prose reply was converted; else `null`.
    pub format_retry: Option<FormatRetry>,
    /// The denial-retry record (a tool was auto-denied); shape under-specified in the
    /// design evidence (always `null`), kept raw. See contracts.md.
    pub denial_retry: Option<Value>,
    /// The timeout-continuation record; shape as `denial_retry`, kept raw.
    pub timeout_continue: Option<Value>,
    pub base_commit: String,
    pub reviewed_revision: String,
    pub tree_sha256: String,
    pub tree_sha256_after: String,
    pub tree_changed_during_review: bool,
    /// The count of changed files at review time (a number, not a list).
    pub changed_files: i64,
    pub brief_sha256: String,
    pub brief_sha256_after: String,
    pub brief_changed_during_review: bool,
    pub fingerprint_note: String,
    pub artifacts: Vec<Value>,
    pub artifacts_changed_during_review: bool,
    pub bridge_outcome: String,
    /// The provider's own failure classification; `null` on success.
    pub provider_failure: Option<ProviderFailure>,
    pub warnings: Vec<Value>,
    pub verdict: String,
    pub verdict_reason: String,
    pub findings: FindingCounts,
    /// The ids raised by this reply, in id order (`F<NN>-<k>`).
    pub finding_ids: Vec<Value>,
    /// This reply's report on prior findings: `{id, status}`.
    pub prior_findings: Vec<PriorFindingRef>,
    pub unchecked_prior_blockers: Vec<Value>,
    /// Token usage; `null` for an engine that does not report it.
    pub usage: Option<Usage>,
    /// The engine-run record (turns); `null` for a plain single Codex turn.
    pub engine_run: Option<EngineRun>,
    /// Wall time in seconds. A whole value is written without a decimal point (see
    /// [`crate::ps_json`]), so this field carries `243.4` and `277` alike.
    pub wall_seconds: f64,
    pub finished_at: String,
    pub commit_wait_ms: i64,
}

/// `entry.reviewer`: identity as `Resolve-ReviewerIdentity` recorded it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reviewer {
    pub provider: String,
    pub provider_source: String,
    pub model: String,
    pub model_source: String,
    pub engine: String,
    pub harness: String,
    pub provider_fingerprint: String,
    /// Polymorphic endpoint descriptor: `{base_url,name,wire_api}` for a Codex provider,
    /// `{engine,launcher[,credential_mechanism]}` for an engine, `{builtin:true}` for the
    /// built-in OpenAI endpoint, or a `{base_url,base_url_source}` `OPENAI_BASE_URL` shape.
    pub provider_config: Value,
    pub identity_note: String,
}

/// `entry.roster`: which roster entry supplied the identity, and what it applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterRef {
    pub path: String,
    pub position: i64,
    /// Skipped roster fields (element shape not exercised here) - raw JSON.
    pub skipped: Vec<Value>,
    /// Applied roster field names (`model`, `provider`, ...).
    pub applied: Vec<Value>,
}

/// `entry.panel`: the panel this run belonged to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Panel {
    pub id: String,
    pub position: i64,
    pub of: i64,
    pub members: Vec<PanelMember>,
    pub concurrency: i64,
    /// Per-provider concurrency caps: a `provider -> count` map whose keys vary per run.
    pub limits: Value,
}

/// One member row of a panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelMember {
    pub provider: String,
    pub model: String,
    pub state: String,
    pub reason: String,
}

/// `entry.format_retry`: the format-repair turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatRetry {
    pub attempted: bool,
    pub reason: String,
    pub succeeded: bool,
    pub thread: String,
    pub wall_seconds: f64,
    pub usage: Usage,
    pub drift: Vec<Value>,
    pub original: String,
    /// The repair turn's event stream; `null` when it was not kept.
    pub events: Option<String>,
    pub schema_transport: String,
}

/// `entry.usage` / `format_retry.usage`: token counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    /// Present for engines that report a total (agy); absent for codex. Always last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<i64>,
}

/// `entry.provider_failure`: the provider's own error classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderFailure {
    pub class: String,
    pub code: String,
    pub message: String,
    pub when: String,
    /// The provider's reset time; `null` when it named none.
    pub retry_after: Option<String>,
}

/// `entry.engine_run`: engine-turn accounting (agy/muse).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRun {
    pub turns: i64,
    pub max_model_steps: Option<i64>,
    pub msp_schema_version: Option<i64>,
}

/// `entry.findings`: the severity histogram of this reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingCounts {
    pub blocker: i64,
    pub major: i64,
    pub minor: i64,
    pub note: i64,
}

/// `entry.prior_findings[]`: this reply's report on an earlier finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorFindingRef {
    pub id: String,
    /// The reviewer's own report: `fixed | still-open | not-checked | unknown-id`.
    pub status: String,
}

impl SessionsFile {
    /// Parse `sessions.json` bytes.
    pub fn read(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Serialize to the exact on-disk byte stream `Write-JsonFile` would produce.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        ps_json::to_ps_json_bytes(self)
    }

    /// Insert `entry` at its place by `n` (mirrors `Add-LedgerEntry`, D10): the entry
    /// lands after every entry whose `n` is not greater than its own, so the list stays
    /// sorted by `n` whatever order a panel's members commit in.
    pub fn add_entry(&mut self, entry: LedgerEntry) {
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
    }
}
