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

/// Deserialize an optional string so a *present* value (including `null`) becomes `Some(..)`
/// and an *absent* key stays `None`: the tri-state `revision_moved` needs (see that field).
/// With `#[serde(default)]`, an absent key never calls this and defaults to `None`.
fn deserialize_present_string<'de, D>(d: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(d)?))
}

/// Same tri-state as [`deserialize_present_string`] for any object record (the wave-26
/// `mode_fallback`/`stall`/`tree_check` fields): a *present* value (including `null`) becomes
/// `Some(..)`, an *absent* key stays `None` (via `#[serde(default)]`), so a pre-wave-26 store
/// round-trips byte-identical while a fresh entry writes the field in position (null or object).
fn deserialize_present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(d)?))
}

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
    /// `-Topics` (an empty array by default) — a top-level entry field, between `purpose` and
    /// `consult_id`. A tri-state so byte-identity survives: **absent** in a pre-topics store
    /// (`None`) is skipped on rewrite; a fresh entry writes `Some(vec![])` → `[]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<Value>>,
    /// `-Role` (empty by default) — a top-level entry field, between `topics` and `consult_id`.
    /// Omittable like `topics`: absent (`None`) skipped on rewrite; a fresh entry writes
    /// `Some(String::new())` → `""`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default)]
    pub consult_id: String,
    /// (0.6.1, U5) A SECOND random id of this consultation, minted by the process that commits the
    /// entry (a panel member its own), derived from nothing (not the `consult_id` the reviewer sees
    /// in the prompt, not a hash of anything local); a lower-case guid right after `consult_id`.
    /// The telemetry events carry it (the consultation event and every rating event of the entry)
    /// so the intake can link them. Absent in an entry recorded before 0.6.1 (kept absent on
    /// rewrite).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consult_ref: Option<String>,
    #[serde(default)]
    pub reviewer: Reviewer,
    #[serde(default)]
    pub lineage: String,
    /// (wave 27) The coordinator that started this run: `{provider, model, engine, host, source}`,
    /// sits between `lineage` and `preflight`. `source` is `explicit | inferred | none`; `host` is
    /// `codex | zcode | claude-code | unknown`. Tri-state so byte-identity survives a pre-wave-27
    /// store: **absent** (`None`) is skipped on rewrite; a fresh entry always writes the object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator: Option<Coordinator>,
    #[serde(default)]
    pub preflight: String,
    #[serde(default)]
    pub preflight_warning: String,
    /// The roster record `{path, position, skipped[], applied[]}`; `null` for a run with no
    /// roster (`$rosterRecord` is `$null` then, `codex-consult.ps1`). All real-roster runs
    /// write an object, so the byte-identity fixtures round-trip through `Some`.
    #[serde(default)]
    pub roster: Option<RosterRef>,
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
    /// (wave 26) The mode-fallback record `{from, to, reason}` written when a fork/resume was
    /// downgraded to `new` (the parent thread's context is too full); `null` otherwise. Sits
    /// between `mode` and `command` in the wave-26 field order. Tri-state so a pre-wave-26 store
    /// round-trips byte-identical: **absent** (`None`) is skipped on rewrite; a fresh entry
    /// writes `null` (`Some(None)`) or the object (`Some(Some(_))`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub mode_fallback: Option<Option<ModeFallback>>,
    #[serde(default)]
    pub command: String,
    /// (wave 27) The host-marker variable NAMES removed from the reviewer child's environment
    /// (`Get-HostMarkerNames`: those present in the parent env, sorted ordinal; NEVER a value).
    /// Sits between `command` and `brief`. Tri-state for byte-identity like [`topics`]: **absent**
    /// (`None`) skipped on rewrite; a fresh entry writes `Some(vec![])` → `[]` (or the names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_env_scrubbed: Option<Vec<Value>>,
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
    /// (wave 28b, D15) The reviewer's context window as it reached the engine: `{tokens,
    /// auto_compact_limit, items}` for a codex reviewer whose roster entry names `context_tokens`
    /// (`items`: the `-c` options added - the operator's own `-CodexConfig`/`codex_config` value of
    /// a key wins and is not repeated), else `null`. Tri-state: absent in an entry recorded before
    /// wave 28b (kept absent on rewrite); a fresh entry writes it in position.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub context_window: Option<Option<Value>>,
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
    /// (wave 26) The stall record `{seconds, last_event}` written when the main turn was killed
    /// for going silent past `-StallSec`; `null` otherwise. Sits between `timeout_continue` and
    /// `base_commit`. Tri-state so a pre-wave-26 store round-trips byte-identical (see
    /// [`mode_fallback`](LedgerEntry::mode_fallback)).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub stall: Option<Option<Stall>>,
    /// (wave 27c, D16) whether a process-tree kill was confirmed to have stopped the root the
    /// bridge started. `true` when confirmed, `false` when it could not be (`pid <n> may still
    /// run` — no continuation turn runs and a warning is written), and JSON `null` (`Some(None)`)
    /// for a run with no tree kill. Sits between `stall` and `base_commit`. Tri-state so a pre-27c
    /// entry (no key, `None`) round-trips byte-identical.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub kill_confirmed: Option<Option<bool>>,
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
    /// A moved HEAD whose file contents stayed identical during the run (wave 24c,
    /// `codex-consult.ps1`). A tri-state so byte-identity survives: **absent** in a pre-24c
    /// store (`None`) is skipped on rewrite; a real entry writes `null` (HEAD did not move,
    /// `Some(None)`) or the `<old> -> <new>` string (`Some(Some(_))`). It sits between
    /// `tree_changed_during_review` and `changed_files` in the wave-24c field order.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present_string"
    )]
    pub revision_moved: Option<Option<String>>,
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
    /// (wave 26b, D9) The engine's read-only tree check `{outcome, files[]}`; `null` for codex
    /// (no check). Sits between `artifacts_changed_during_review` and `bridge_outcome`.
    /// Tri-state so a pre-wave-26 store round-trips byte-identical (see
    /// [`mode_fallback`](LedgerEntry::mode_fallback)).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub tree_check: Option<Option<TreeCheck>>,
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
    /// (wave 28c, D11) The compactions the engine REPORTED in the event streams of the run's turns:
    /// a number when it reported one or more, `"unknown"` when none was reported by a reviewer with
    /// a context window (`context_tokens` - none seen is not none happened), else `null`.
    /// Tri-state: absent in an entry recorded before wave 28c; a fresh entry writes it in position
    /// (right after `usage`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub compactions: Option<Option<Value>>,
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

/// `entry.coordinator` (wave 27): who ran the bridge. Field order is the plugin literal
/// `{provider, model, engine, host, source}` and MUST NOT change. `provider`/`model`/`engine`
/// are `null` when the host was only inferred (no `CODEX_CONSULT_COORDINATOR` value); `host` is
/// `codex | zcode | claude-code | unknown`; `source` is `explicit | inferred | none`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Coordinator {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub source: String,
    /// (wave 27c, D11) `false` when the coordinator parses but no reviewer of the roster can match
    /// it — said, not refused. Absent (skipped) when there is no roster to check or the coordinator
    /// was only inferred, so a pre-27c entry round-trips byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_roster: Option<bool>,
    /// (wave 27c, D12) `"#n"` when the coordinator is a roster position that names no seat here:
    /// the run goes on with a warning. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.mode_fallback` (wave 26): a downgraded fork/resume, `{from, to, reason}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModeFallback {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub reason: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.stall` (wave 26): the main turn killed for silence, `{seconds, last_event}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stall {
    #[serde(default)]
    pub seconds: i64,
    /// The ISO time of the last event before the stall kill; `null` when none was seen.
    #[serde(default)]
    pub last_event: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.tree_check` (wave 26b, D9): the engine's read-only tree check, `{outcome, files[]}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreeCheck {
    /// `clean` | `warned` | `failed`.
    #[serde(default)]
    pub outcome: String,
    /// The changed paths the check saw (working tree, collab directory, brief, artifacts).
    #[serde(default)]
    pub files: Vec<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.roster`: which roster entry supplied the identity, and what it applied.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RosterRef {
    #[serde(default)]
    pub path: String,
    /// The selected entry's position, or `null` when no entry matched (`-Provider`/`-Thread`
    /// with no roster entry).
    #[serde(default)]
    pub position: Option<i64>,
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
    /// (wave 26) The seats the panel asked for (= `of`); the members started; the members
    /// that gave a usable reply. Written into every member's entry when the panel ends, so a
    /// pre-wave-26 panel record (and one whose run died before the end) has neither. Omittable
    /// so those older records round-trip byte-identical: absent (`None`) is skipped on rewrite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<i64>,
    /// `started`/`usable`: a tri-state so a member's committed entry writes explicit `null`
    /// (the panel run patches them to the real counts after every member finishes), while a
    /// pre-wave-26 panel record that has neither key round-trips byte-identical (absent →
    /// `None`, skipped on rewrite). `Some(None)` → `null`; `Some(Some(n))` → the count.
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub started: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub usable: Option<Option<i64>>,
    /// (wave 26) The routing record of a routed panel; absent in a pre-wave-26 store. Omittable
    /// like the counts above so an older panel record round-trips byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<PanelRouting>,
    /// (wave 26b, D8) the panel-wide roles note (`Select-RoleAssignment`); written always (empty
    /// string when no `-Roles` fallback note), after `routing`. Tri-state for byte-identity with a
    /// pre-wave-26b record that has no key (absent → `None`, skipped on rewrite).
    #[serde(
        default,
        deserialize_with = "deserialize_present_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub roles_note: Option<Option<String>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `entry.panel.routing` (wave 26): how a routed panel drew its seats. Field order is the
/// plugin literal `{mode, order, fallback, seed, nonce, nonce_source, size, size_source,
/// eligible[], picked[], explored[], required[]}` (`Add-PanelRouting`, D2-D5) and MUST NOT
/// change. C3 does not write panels (the consult flow does); this types the record so a
/// wave-26 store round-trips byte-identically and the panel milestone can read it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PanelRouting {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub order: String,
    #[serde(default)]
    pub fallback: String,
    #[serde(default)]
    pub seed: String,
    #[serde(default)]
    pub nonce: String,
    #[serde(default)]
    pub nonce_source: String,
    #[serde(default)]
    pub size: i64,
    /// (wave 26b, D2) the size as asked (`0` = the eligible count), before the pinned/eligible
    /// clamp. Between `size` and `size_source`.
    #[serde(default)]
    pub size_asked: i64,
    #[serde(default)]
    pub size_source: String,
    /// (wave 26b, D5) the seats the lab reserve filled (`rule` `lab-*`). Between `size_source`
    /// and `eligible`.
    #[serde(default)]
    pub reserve: i64,
    #[serde(default)]
    pub eligible: Vec<RoutingEligible>,
    #[serde(default)]
    pub picked: Vec<RoutingPicked>,
    /// The lineages explored (a uniform draw), in seat order.
    #[serde(default)]
    pub explored: Vec<Value>,
    /// The required lineages (`-Require`/roster `require`), in roster order.
    #[serde(default)]
    pub required: Vec<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `panel.routing.eligible[]`: one eligible entry with its routing basis. Field order:
/// `{position, lineage, lab, lab_source, score, basis, ratings, required}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutingEligible {
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub lineage: String,
    #[serde(default)]
    pub lab: String,
    #[serde(default)]
    pub lab_source: String,
    /// The routing score (a number scaled into `[0.25, 2]`); a whole value is written without
    /// a decimal point by [`crate::ps_json`].
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub basis: String,
    /// The rating count behind the basis (topic credit pooled, so it may be fractional).
    #[serde(default)]
    pub ratings: f64,
    #[serde(default)]
    pub required: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `panel.routing.picked[]`: one seat with the rule that filled it. Field order:
/// `{slot, position, lineage, lab, rule}` (`rule`: `required` | `roster` | `lab-draw` |
/// `lab-explore` | `rank-draw` | `rank-explore`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutingPicked {
    #[serde(default)]
    pub slot: i64,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub lineage: String,
    #[serde(default)]
    pub lab: String,
    #[serde(default)]
    pub rule: String,
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
    /// The repair turn's usage; `null` when the engine reported none (empty/failed turn).
    #[serde(default)]
    pub usage: Option<Usage>,
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
