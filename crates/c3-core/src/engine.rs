//! The engine contract (DESIGN §4): one attempt of one reviewer, the exact argv the plugin
//! builds for the subprocess engines, and the validated v1 reply.
//!
//! An [`Engine`] reports its [`Capabilities`] (including how it takes its prompt,
//! [`PromptDelivery`]), plans a launch for a [`Request`] ([`Engine::plan`] -> a
//! [`LaunchPlan`], a subprocess argv or an `http` request), runs a turn checked by
//! [`Engine::precheck`], and continues a conversation ([`Engine::continue_turn`]) - all
//! carried by a [`TurnRequest`] that keeps the resolved request, the ids, the [`TurnKind`]
//! and the [`Continuation`]. A turn's result is an [`AttemptOutcome`], not a bare reply, so
//! a timeout, a launch failure, a provider failure and a cancellation are first-class
//! (F02-11/F07-5).
//!
//! Subprocess engines wrap the CLIs exactly as `codex-consult.ps1` / `New-AgyArgv` /
//! `New-MuseArgv` do: codex takes the prompt on plain stdin and a `fork <thread>` /
//! `resume <thread>` subcommand *after* every exec-level option (with `-` for stdin); agy
//! takes one NDJSON `{"event":"user",...}` line on stdin and has no fork; muse takes a
//! prompt file and has no fork. The `http` engine builds one OpenAI-compatible request from
//! a reviewer pack ([`HttpPlan`], M7) and its continuation is replay (the retained pack
//! plus the prior reply).
//!
//! Ids (DESIGN §4 "Identity"): [`ConsultationId`] is one brief + one reviewer; [`AttemptId`]
//! is one engine call (a retry is a NEW attempt with the same inputs, no redraw);
//! [`ConversationId`] is the engine thread for CLI engines, or a C3-owned transcript for
//! `http`. A [`Mode::Resume`]/[`Mode::Fork`] carries the [`Lineage`] of the thread it names
//! so crossing lineages is refusable (F07-8), and a returned conversation is tagged with a
//! [`ConversationTrust`] so the runtime knows whether it may be resumed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ledger::{ProviderFailure, Usage};
use crate::lineage::format_reviewer_lineage;

/// The four engines (DESIGN §4). `codex` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Codex,
    Agy,
    Muse,
    Http,
}

/// How a reviewer receives the reply schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaTransport {
    /// Codex `--output-schema` / muse `--output-schema`.
    OutputSchema,
    /// agy `--json-schema` (a native structured-output request).
    Native,
    /// The schema is described in the prompt only.
    PromptOnly,
}

/// How an engine takes its prompt (F02-8/F03-6/F04-7). codex: plain stdin; agy: one NDJSON
/// `{"event":"user","message":{"content":...}}` line on stdin
/// (`ConvertTo-AgyStdin`); muse: a `--prompt-file` (stdin is empty).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptDelivery {
    /// The prompt text is written verbatim to stdin (codex).
    Stdin,
    /// One newline-terminated NDJSON user-message line is written to stdin (agy).
    StdinNdjsonLine,
    /// The prompt is a file named in the argv; stdin is empty (muse).
    PromptFile,
}

/// What an engine can do (DESIGN §4: `capabilities()`).
#[derive(Debug, Clone)]
pub struct Capabilities {
    /// The engine keeps conversation threads (a reply can be resumed/forked).
    pub threads: bool,
    /// Forking a thread is supported (codex only).
    pub fork: bool,
    /// Resuming a thread is supported natively.
    pub resume: bool,
    /// How the prompt reaches the engine.
    pub prompt_delivery: PromptDelivery,
    /// The schema transports the engine accepts, in preference order.
    pub schema_transport: Vec<SchemaTransport>,
    /// The effort vocabulary label (`openai`, `model-tier`, `muse`, ...).
    pub effort_vocabulary: &'static str,
    /// The engine runs its reviewer in a read-only sandbox.
    pub sandbox: bool,
    /// The handoff/file prefix for this engine (`codex`, `agy`, `muse`, `http`).
    pub file_prefix: &'static str,
}

/// The capability table for an engine, from `$script:Engines` (and DESIGN §4 for `http`).
pub fn capabilities(kind: EngineKind) -> Capabilities {
    match kind {
        EngineKind::Codex => Capabilities {
            threads: true,
            fork: true,
            resume: true,
            prompt_delivery: PromptDelivery::Stdin,
            schema_transport: vec![SchemaTransport::OutputSchema, SchemaTransport::PromptOnly],
            effort_vocabulary: "openai",
            sandbox: true,
            file_prefix: "codex",
        },
        EngineKind::Agy => Capabilities {
            threads: true,
            fork: false,
            resume: true,
            prompt_delivery: PromptDelivery::StdinNdjsonLine,
            schema_transport: vec![SchemaTransport::Native, SchemaTransport::PromptOnly],
            effort_vocabulary: "model-tier",
            // agy --sandbox restricts the terminal only; the bridge's tree check enforces
            // read-only. The capability is "runs read-only" as the bridge guarantees it.
            sandbox: true,
            file_prefix: "agy",
        },
        EngineKind::Muse => Capabilities {
            threads: true,
            fork: false,
            resume: true,
            prompt_delivery: PromptDelivery::PromptFile,
            schema_transport: vec![SchemaTransport::OutputSchema, SchemaTransport::PromptOnly],
            effort_vocabulary: "muse",
            sandbox: true,
            file_prefix: "muse",
        },
        EngineKind::Http => Capabilities {
            threads: false,
            fork: false,
            // Continuation is replay (the retained pack + the prior reply), not a native
            // thread resume; reported as unsupported.
            resume: false,
            // The http engine sends a request body, not stdin; delivery is not a CLI shape,
            // but the prompt still travels in the request, so `Stdin` is the nearest label.
            prompt_delivery: PromptDelivery::Stdin,
            schema_transport: vec![SchemaTransport::OutputSchema, SchemaTransport::PromptOnly],
            effort_vocabulary: "openai",
            // The http engine never receives tools (DESIGN §3 invariant 2).
            sandbox: false,
            file_prefix: "http",
        },
    }
}

/// A reviewer lineage key (`provider :: model [engine]`), used to refuse resuming/forking
/// across lineages (F07-8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lineage(pub String);

/// The launch mode for a CLI engine. `Resume`/`Fork` carry the [`Lineage`] of the thread so
/// a cross-lineage resume/fork is refusable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// A fresh thread.
    New,
    /// Resume a known thread on a known lineage.
    Resume { thread: String, lineage: Lineage },
    /// Fork a known thread on a known lineage (codex only).
    Fork { thread: String, lineage: Lineage },
}

/// One consultation id (one brief, one reviewer). The prompt's last line carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsultationId(pub String);

/// One attempt id (one engine call). A retry is a new [`AttemptId`] with the same inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptId(pub String);

/// One conversation id (the engine thread for CLI engines; a C3 transcript for `http`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationId(pub String);

/// A request for one attempt. Some fields are engine-specific (documented per field).
#[derive(Debug, Clone)]
pub struct Request {
    /// The full prompt text (its last line is `Consultation id: <id>`).
    pub prompt: String,
    /// The brief file, for the record.
    pub brief_path: Option<PathBuf>,
    pub model: String,
    pub provider: String,
    /// The engine this request runs on (used for the lineage key and the fork check).
    pub engine: EngineKind,
    /// The effort value to send (already mapped to the engine's vocabulary), if any.
    pub effort: Option<String>,
    /// The run timeout, in seconds.
    pub timeout_sec: f64,
    pub mode: Mode,
    /// The sandbox label (codex: `read-only`/`workspace-write`).
    pub sandbox: String,
    /// The reply schema file, when a schema transport is used.
    pub schema_path: Option<PathBuf>,
    /// Applied `-CodexConfig` items (`key=value`); codex `-c` args, passed through verbatim.
    pub extra_config: Vec<String>,
    /// codex `-o <path>`: where the last agent message is written.
    pub output_last_message: Option<PathBuf>,
    /// muse `--prompt-file <path>`: the prompt file.
    pub prompt_file: Option<PathBuf>,
    /// muse `--max-model-steps <n>`.
    pub max_model_steps: Option<u32>,
}

impl Request {
    /// The reviewer lineage key for this request (`provider :: model [engine]`).
    pub fn lineage(&self) -> Lineage {
        Lineage(format_reviewer_lineage(
            &self.provider,
            &self.model,
            engine_token(self.engine),
        ))
    }
}

fn engine_token(kind: EngineKind) -> &'static str {
    match kind {
        EngineKind::Codex => "codex",
        EngineKind::Agy => "agy",
        EngineKind::Muse => "muse",
        EngineKind::Http => "http",
    }
}

/// What kind of turn this is (F02-10/F08-6): the primary attempt or one of the three
/// secondary-turn mechanisms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnKind {
    /// The first turn of an attempt.
    Primary,
    /// A continuation after a timeout kill.
    TimeoutContinuation,
    /// A retry after a tool was auto-denied and the first turn produced nothing.
    DenialRetry,
    /// A repair turn that converts a prose reply into the structured object.
    FormatRepair,
}

/// How a continuation reaches the engine (F02-10/F04-7): a native thread resume, or - for
/// the `http` engine - replay of the retained pack plus the prior reply.
#[derive(Debug, Clone)]
pub enum Continuation {
    /// Resume the engine's own conversation thread.
    Native(ConversationId),
    /// Replay: the retained pack (by content hash) and the prior reply text.
    Replay {
        pack_hash: String,
        prior_reply: String,
    },
}

/// A request for a secondary turn or continuation. It carries the full resolved [`Request`],
/// the ids, the [`TurnKind`] and (for a continuation) the [`Continuation`], so no launch
/// setting, retry policy or replay input is dropped (F02-10).
#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub request: Request,
    pub consultation: ConsultationId,
    pub attempt: AttemptId,
    pub kind: TurnKind,
    /// `None` for a primary turn; `Some` for a continuation/resume.
    pub continuation: Option<Continuation>,
}

/// The argv the engine will run: the launcher plus its arguments (the launcher is resolved
/// on PATH separately). `args` is everything after the launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argv {
    pub command: String,
    pub args: Vec<String>,
}

impl Argv {
    /// The whole argv as one display string (`codex exec --sandbox read-only ... -`).
    pub fn to_command_string(&self) -> String {
        let mut s = self.command.clone();
        for a in &self.args {
            s.push(' ');
            s.push_str(a);
        }
        s
    }
}

/// The `http` engine's plan: one OpenAI-compatible request built from a reviewer pack (M7).
/// The pack pipeline and the exact wire body land in the runtime; this carries the resolved
/// identity and prompt the request is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpPlan {
    pub provider: String,
    pub model: String,
    pub prompt: String,
    pub schema_path: Option<PathBuf>,
    pub effort: Option<String>,
}

/// What [`Engine::plan`] produces: a subprocess argv, or an `http` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPlan {
    Subprocess(Argv),
    Http(HttpPlan),
}

/// Whether a returned conversation id may be trusted for a resume (F02-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationTrust {
    /// A conversation id the engine confirmed (safe to resume/fork).
    Verified(ConversationId),
    /// A candidate id observed but not confirmed (e.g. a killed turn); resume at your risk.
    Candidate(ConversationId),
    /// No conversation id at all.
    None,
}

/// One reply from an attempt.
#[derive(Debug, Clone)]
pub struct Reply {
    /// The verbatim reply text (the verified structured or the salvaged prose).
    pub raw_text: String,
    /// The parsed, validated v1 reply, when the attempt produced a valid structured object.
    pub structured: Option<StructuredReply>,
    /// The raw event stream file (`.events.jsonl`).
    pub events_path: PathBuf,
    /// Token usage, when the engine reports it.
    pub usage: Option<Usage>,
    /// Wall time in seconds.
    pub wall_seconds: f64,
    /// The conversation this reply came on, and whether it may be resumed.
    pub conversation: ConversationTrust,
}

/// The result of one attempt (F02-11/F07-5): a completed reply, a timeout with a salvaged
/// partial and the survivors it could not kill, a launch failure (with whether a child was
/// left behind), a provider failure, or a cancellation.
#[derive(Debug, Clone)]
pub enum AttemptOutcome {
    Completed(Reply),
    TimedOut {
        partial: Option<String>,
        survivors: Vec<u32>,
        conversation: ConversationTrust,
        /// The measured wall time of the killed turn (`[math]::Round(..., 1)`).
        wall_seconds: f64,
    },
    LaunchFailed {
        /// Whether a child process was left behind by the failed launch.
        child_exists: bool,
        message: String,
    },
    ProviderFailure {
        failure: ProviderFailure,
        /// The child's raw process exit code, for the plugin's `codex exit N` framing on a
        /// secondary (timeout-continuation / format-repair) turn. `None` when unknown.
        exit_code: Option<i32>,
    },
    Cancelled,
}

// --------------------------------------------------------------------------- v1 reply schema

/// The raw, tolerant DTO of a v1 reply as it comes off the engine, before validation. It
/// accepts any string where the schema names an enum and a missing `line`; the validation
/// is the conversion to [`StructuredReply`] via [`StructuredReply::try_from`] (F02-12/F09-6).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawReply {
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub verdict_reason: String,
    #[serde(default)]
    pub reply_markdown: String,
    #[serde(default)]
    pub findings: Vec<RawFinding>,
    #[serde(default)]
    pub prior_findings: Vec<RawPriorFinding>,
    #[serde(default)]
    pub unproven: Vec<String>,
    #[serde(default)]
    pub first_run_checklist: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One finding in a raw v1 reply.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawFinding {
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub locations: Vec<RawLocation>,
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub trigger: String,
    #[serde(default)]
    pub evidence: Vec<RawEvidence>,
    #[serde(default)]
    pub verification: String,
    #[serde(default)]
    pub remedy: String,
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A location in a raw v1 reply finding. `line` is a double option so validation can tell a
/// *missing* `line` key (`None`) from a present `null` (`Some(None)`): the schema requires
/// the key present, value `integer | null`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawLocation {
    #[serde(default)]
    pub path: String,
    #[serde(default, deserialize_with = "deserialize_present_line")]
    pub line: Option<Option<i64>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Deserialize `line` so a *present* value (including `null`) becomes `Some(..)` and an
/// *absent* key stays `None` (the plain serde `Option<Option<i64>>` collapses both to
/// `None`). This lets the validator require the `line` key while allowing a `null` value.
fn deserialize_present_line<'de, D>(d: D) -> Result<Option<Option<i64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<i64>::deserialize(d)?))
}

/// A piece of evidence in a raw v1 reply finding.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawEvidence {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub observation: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A prior-finding report in a raw v1 reply.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawPriorFinding {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub note: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The reviewer's verdict (closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    Accept,
    Hold,
    Reject,
    Advise,
}

impl Verdict {
    fn parse(s: &str) -> Option<Verdict> {
        match s {
            "ACCEPT" => Some(Verdict::Accept),
            "HOLD" => Some(Verdict::Hold),
            "REJECT" => Some(Verdict::Reject),
            "ADVISE" => Some(Verdict::Advise),
            _ => None,
        }
    }
}

/// A finding's severity (closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Blocker,
    Major,
    Minor,
    Note,
}

impl Severity {
    fn parse(s: &str) -> Option<Severity> {
        match s {
            "blocker" => Some(Severity::Blocker),
            "major" => Some(Severity::Major),
            "minor" => Some(Severity::Minor),
            "note" => Some(Severity::Note),
            _ => None,
        }
    }
}

/// A piece of evidence's kind (closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    ReadCode,
    RanCommand,
    Inferred,
    Assumed,
}

impl EvidenceKind {
    fn parse(s: &str) -> Option<EvidenceKind> {
        match s {
            "read-code" => Some(EvidenceKind::ReadCode),
            "ran-command" => Some(EvidenceKind::RanCommand),
            "inferred" => Some(EvidenceKind::Inferred),
            "assumed" => Some(EvidenceKind::Assumed),
            _ => None,
        }
    }
}

/// A prior finding's reported status (closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PriorStatus {
    Fixed,
    StillOpen,
    NotChecked,
    UnknownId,
}

impl PriorStatus {
    fn parse(s: &str) -> Option<PriorStatus> {
        match s {
            "fixed" => Some(PriorStatus::Fixed),
            "still-open" => Some(PriorStatus::StillOpen),
            "not-checked" => Some(PriorStatus::NotChecked),
            "unknown-id" => Some(PriorStatus::UnknownId),
            _ => None,
        }
    }
}

/// A validated v1 reply object (closed enums, `line` present). Produced by
/// [`StructuredReply::try_from`]; it never carries an out-of-set token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredReply {
    pub verdict: Verdict,
    pub verdict_reason: String,
    pub reply_markdown: String,
    pub findings: Vec<ReplyFinding>,
    pub prior_findings: Vec<ReplyPriorFinding>,
    pub unproven: Vec<String>,
    pub first_run_checklist: Vec<String>,
}

/// One validated finding in a v1 reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyFinding {
    pub severity: Severity,
    pub locations: Vec<ReplyLocation>,
    pub claim: String,
    pub trigger: String,
    pub evidence: Vec<ReplyEvidence>,
    pub verification: String,
    pub remedy: String,
    pub supersedes: Vec<String>,
}

/// A validated location; `line` is present (the key was there), value `Some`/`None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyLocation {
    pub path: String,
    pub line: Option<i64>,
}

/// A validated piece of evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyEvidence {
    pub kind: EvidenceKind,
    pub reference: String,
    pub observation: String,
}

/// A validated prior-finding report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyPriorFinding {
    pub id: String,
    pub status: PriorStatus,
    pub note: String,
}

/// Why a [`RawReply`] failed validation into a [`StructuredReply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyValidationError {
    /// `schema_version` was not `"1"` (checked first).
    BadSchemaVersion(String),
    BadVerdict(String),
    /// A finding at `index` had an out-of-set severity.
    BadSeverity {
        finding: usize,
        value: String,
    },
    /// A finding's evidence at `index` had an out-of-set kind.
    BadEvidenceKind {
        finding: usize,
        evidence: usize,
        value: String,
    },
    /// A finding's location at `index` had no `line` key (the schema requires it present).
    LocationMissingLine {
        finding: usize,
        location: usize,
    },
    /// A prior-finding report had an out-of-set status.
    BadPriorStatus {
        index: usize,
        value: String,
    },
}

impl std::fmt::Display for ReplyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplyValidationError::BadSchemaVersion(v) => {
                write!(f, "schema_version must be \"1\", got {v:?}")
            }
            ReplyValidationError::BadVerdict(v) => write!(f, "verdict {v:?} is not one of ACCEPT/HOLD/REJECT/ADVISE"),
            ReplyValidationError::BadSeverity { finding, value } => {
                write!(f, "findings[{finding}].severity {value:?} is not one of blocker/major/minor/note")
            }
            ReplyValidationError::BadEvidenceKind { finding, evidence, value } => write!(
                f,
                "findings[{finding}].evidence[{evidence}].kind {value:?} is not one of read-code/ran-command/inferred/assumed"
            ),
            ReplyValidationError::LocationMissingLine { finding, location } => write!(
                f,
                "findings[{finding}].locations[{location}] is missing the required `line` key"
            ),
            ReplyValidationError::BadPriorStatus { index, value } => write!(
                f,
                "prior_findings[{index}].status {value:?} is not one of fixed/still-open/not-checked/unknown-id"
            ),
        }
    }
}

impl std::error::Error for ReplyValidationError {}

impl TryFrom<RawReply> for StructuredReply {
    type Error = ReplyValidationError;

    fn try_from(raw: RawReply) -> Result<Self, Self::Error> {
        // schema_version first, as the schema lists it first and it gates everything.
        if raw.schema_version != "1" {
            return Err(ReplyValidationError::BadSchemaVersion(raw.schema_version));
        }
        let verdict = Verdict::parse(&raw.verdict)
            .ok_or_else(|| ReplyValidationError::BadVerdict(raw.verdict.clone()))?;
        let mut findings = Vec::with_capacity(raw.findings.len());
        for (fi, rf) in raw.findings.into_iter().enumerate() {
            let severity =
                Severity::parse(&rf.severity).ok_or_else(|| ReplyValidationError::BadSeverity {
                    finding: fi,
                    value: rf.severity.clone(),
                })?;
            let mut locations = Vec::with_capacity(rf.locations.len());
            for (li, rl) in rf.locations.into_iter().enumerate() {
                let line = rl.line.ok_or(ReplyValidationError::LocationMissingLine {
                    finding: fi,
                    location: li,
                })?;
                locations.push(ReplyLocation {
                    path: rl.path,
                    line,
                });
            }
            let mut evidence = Vec::with_capacity(rf.evidence.len());
            for (ei, re) in rf.evidence.into_iter().enumerate() {
                let kind = EvidenceKind::parse(&re.kind).ok_or_else(|| {
                    ReplyValidationError::BadEvidenceKind {
                        finding: fi,
                        evidence: ei,
                        value: re.kind.clone(),
                    }
                })?;
                evidence.push(ReplyEvidence {
                    kind,
                    reference: re.reference,
                    observation: re.observation,
                });
            }
            findings.push(ReplyFinding {
                severity,
                locations,
                claim: rf.claim,
                trigger: rf.trigger,
                evidence,
                verification: rf.verification,
                remedy: rf.remedy,
                supersedes: rf.supersedes,
            });
        }
        let mut prior = Vec::with_capacity(raw.prior_findings.len());
        for (pi, rp) in raw.prior_findings.into_iter().enumerate() {
            let status = PriorStatus::parse(&rp.status).ok_or_else(|| {
                ReplyValidationError::BadPriorStatus {
                    index: pi,
                    value: rp.status.clone(),
                }
            })?;
            prior.push(ReplyPriorFinding {
                id: rp.id,
                status,
                note: rp.note,
            });
        }
        Ok(StructuredReply {
            verdict,
            verdict_reason: raw.verdict_reason,
            reply_markdown: raw.reply_markdown,
            findings,
            prior_findings: prior,
            unproven: raw.unproven,
            first_run_checklist: raw.first_run_checklist,
        })
    }
}

// --------------------------------------------------------------------------- the trait

/// What can go wrong planning or running an attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// A required field for this engine's argv was missing.
    MissingField(&'static str),
    /// This engine does not support the requested mode (e.g. agy/muse fork).
    UnsupportedMode {
        engine: EngineKind,
        mode: &'static str,
    },
    /// A resume/fork was asked across lineages (`requested` names the thread's lineage).
    LineageMismatch { thread: Lineage, request: Lineage },
    /// A pre-launch guard refused the turn (e.g. muse per-token billing risk).
    Precheck(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::MissingField(name) => write!(f, "missing field for argv: {name}"),
            EngineError::UnsupportedMode { engine, mode } => write!(
                f,
                "the {} engine has no {mode}; use -Mode resume or new",
                engine_token(*engine)
            ),
            EngineError::LineageMismatch { thread, request } => write!(
                f,
                "refusing to cross lineages: the thread is {} but this request is {}",
                thread.0, request.0
            ),
            EngineError::Precheck(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// Muse would bill the subscription per token if one of these API-key variables is set; the
/// launch guard refuses (`Get-MuseLaunchBlock`, `codex-consult-common.ps1:3838`).
pub const MUSE_API_KEY_VARS: [&str; 2] = ["META_API_KEY", "MODEL_API_KEY"];

/// One attempt of one reviewer (DESIGN §4).
pub trait Engine {
    /// The engine's capabilities.
    fn capabilities(&self) -> Capabilities;

    /// The exact launch the plugin builds for `request`: a subprocess argv, or an `http`
    /// request. Refuses an unsupported mode and a cross-lineage resume/fork.
    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError>;

    /// A guard run before EVERY launch (`precheck`): the muse per-token-billing/oauth guard
    /// lives here (F02-8 area). codex/agy have none.
    fn precheck(&self, turn: &TurnRequest) -> Result<(), EngineError>;

    /// Run one turn. Implemented in M2b (subprocess launch, event reader, reply ingestion,
    /// format repair). Signature only.
    fn run(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError>;

    /// Continue a conversation (codex/agy/muse resume; `http` replay). Implemented in M2b.
    /// Signature only.
    fn continue_turn(&self, turn: &TurnRequest) -> Result<AttemptOutcome, EngineError>;
}

/// A subprocess engine (codex, agy or muse). Its [`Engine::plan`] reproduces the plugin's
/// argv byte for byte; [`Engine::run`]/[`Engine::continue_turn`] land in M2b.
#[derive(Debug, Clone)]
pub struct SubprocessEngine {
    pub kind: EngineKind,
}

/// A TOML basic string: wrap in `"`, escaping `\`, `"` and control characters, so an
/// interpolated `-c key="<value>"` cannot be broken by a quote or backslash in the value
/// (F02-17). Returns the value *with* its surrounding quotes.
pub fn toml_basic_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0C}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl SubprocessEngine {
    pub fn new(kind: EngineKind) -> Self {
        SubprocessEngine { kind }
    }

    /// Refuse a resume/fork whose thread lineage differs from the request's (F07-8).
    fn check_lineage(&self, r: &Request) -> Result<(), EngineError> {
        let want = r.lineage();
        match &r.mode {
            Mode::New => Ok(()),
            Mode::Resume { lineage, .. } | Mode::Fork { lineage, .. } => {
                if *lineage != want {
                    Err(EngineError::LineageMismatch {
                        thread: lineage.clone(),
                        request: want,
                    })
                } else {
                    Ok(())
                }
            }
        }
    }

    fn plan_codex(&self, r: &Request) -> Result<Argv, EngineError> {
        // codex exec --sandbox <s> --color never --json [-m <model>]
        //   -c model_reasoning_effort="<e>" [-c model_provider="<p>"] [-c <extra>...]
        //   -o <tmp> [--output-schema <schema>] [fork|resume <thread>] -
        // Every exec-level option precedes the fork|resume subcommand; `-` is stdin.
        let mut a: Vec<String> = vec![
            "exec".into(),
            "--sandbox".into(),
            r.sandbox.clone(),
            "--color".into(),
            "never".into(),
            "--json".into(),
        ];
        if !r.model.is_empty() {
            a.push("-m".into());
            a.push(r.model.clone());
        }
        if let Some(e) = &r.effort {
            a.push("-c".into());
            a.push(format!("model_reasoning_effort={}", toml_basic_string(e)));
        }
        if !r.provider.is_empty() {
            a.push("-c".into());
            a.push(format!("model_provider={}", toml_basic_string(&r.provider)));
        }
        for item in &r.extra_config {
            a.push("-c".into());
            a.push(item.clone());
        }
        if let Some(o) = &r.output_last_message {
            a.push("-o".into());
            a.push(o.to_string_lossy().into_owned());
        }
        if let Some(s) = &r.schema_path {
            a.push("--output-schema".into());
            a.push(s.to_string_lossy().into_owned());
        }
        match &r.mode {
            Mode::New => {}
            Mode::Resume { thread, .. } => {
                a.push("resume".into());
                a.push(thread.clone());
            }
            Mode::Fork { thread, .. } => {
                // The plugin pushes @('fork', $parentThread) (codex-consult.ps1:2462); the
                // parent thread id must not be dropped (F03-1/F04-6/F08-5/F09-1).
                a.push("fork".into());
                a.push(thread.clone());
            }
        }
        a.push("-".into());
        Ok(Argv {
            command: "codex".into(),
            args: a,
        })
    }

    fn plan_agy(&self, r: &Request) -> Result<Argv, EngineError> {
        if let Mode::Fork { .. } = r.mode {
            return Err(EngineError::UnsupportedMode {
                engine: EngineKind::Agy,
                mode: "fork",
            });
        }
        if r.model.is_empty() {
            return Err(EngineError::MissingField("model"));
        }
        let mut a: Vec<String> = vec![
            "-p=".into(),
            "--input-format".into(),
            "stream-json".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--model".into(),
            r.model.clone(),
        ];
        if let Some(s) = &r.schema_path {
            a.push("--json-schema".into());
            a.push(s.to_string_lossy().into_owned());
        }
        a.push("--print-timeout".into());
        a.push("0".into());
        a.push("--sandbox".into());
        a.push("--disable-slash-commands".into());
        if let Mode::Resume { thread, .. } = &r.mode {
            a.push("--conversation".into());
            a.push(thread.clone());
        }
        if let Some(e) = &r.effort {
            a.push("--effort".into());
            a.push(e.clone());
        }
        Ok(Argv {
            command: "agy".into(),
            args: a,
        })
    }

    fn plan_muse(&self, r: &Request) -> Result<Argv, EngineError> {
        if let Mode::Fork { .. } = r.mode {
            return Err(EngineError::UnsupportedMode {
                engine: EngineKind::Muse,
                mode: "fork",
            });
        }
        let prompt_file = r
            .prompt_file
            .as_ref()
            .ok_or(EngineError::MissingField("prompt_file"))?;
        if r.model.is_empty() {
            return Err(EngineError::MissingField("model"));
        }
        let mut a: Vec<String> = vec![
            "exec".into(),
            "--json".into(),
            "--prompt-file".into(),
            prompt_file.to_string_lossy().into_owned(),
        ];
        if let Some(s) = &r.schema_path {
            a.push("--output-schema".into());
            a.push(s.to_string_lossy().into_owned());
        }
        a.push("--model".into());
        a.push(r.model.clone());
        if let Some(e) = &r.effort {
            if !e.trim().is_empty() {
                a.push("--reasoning-effort".into());
                a.push(e.clone());
            }
        }
        a.push("--no-foreign-personal-context".into());
        a.push("--disable-web-tools".into());
        a.push("--disable-write".into());
        a.push("--disable-shell".into());
        a.push("--approval-mode".into());
        a.push("never".into());
        if let Some(n) = r.max_model_steps {
            if n > 0 {
                a.push("--max-model-steps".into());
                a.push(n.to_string());
            }
        }
        if let Mode::Resume { thread, .. } = &r.mode {
            a.push("--session-id".into());
            a.push(thread.clone());
        }
        Ok(Argv {
            command: "muse".into(),
            args: a,
        })
    }

    fn plan_http(&self, r: &Request) -> Result<HttpPlan, EngineError> {
        if r.model.is_empty() {
            return Err(EngineError::MissingField("model"));
        }
        Ok(HttpPlan {
            provider: r.provider.clone(),
            model: r.model.clone(),
            prompt: r.prompt.clone(),
            schema_path: r.schema_path.clone(),
            effort: r.effort.clone(),
        })
    }
}

impl Engine for SubprocessEngine {
    fn capabilities(&self) -> Capabilities {
        capabilities(self.kind)
    }

    fn plan(&self, request: &Request) -> Result<LaunchPlan, EngineError> {
        self.check_lineage(request)?;
        match self.kind {
            EngineKind::Codex => Ok(LaunchPlan::Subprocess(self.plan_codex(request)?)),
            EngineKind::Agy => Ok(LaunchPlan::Subprocess(self.plan_agy(request)?)),
            EngineKind::Muse => Ok(LaunchPlan::Subprocess(self.plan_muse(request)?)),
            EngineKind::Http => Ok(LaunchPlan::Http(self.plan_http(request)?)),
        }
    }

    fn precheck(&self, turn: &TurnRequest) -> Result<(), EngineError> {
        if self.kind == EngineKind::Muse {
            // The pure half of Get-MuseLaunchBlock: a set API-key variable would bill the
            // Muse Code subscription per token. The oauth/auth.json half is completed by the
            // runtime (it reads ~/.config/muse/auth.json), which may extend this guard.
            for name in MUSE_API_KEY_VARS {
                if let Ok(v) = std::env::var(name) {
                    if !v.trim().is_empty() {
                        return Err(EngineError::Precheck(format!(
                            "{name} is set: a muse run would bill per token instead of the Muse Code subscription; unset it (the muse process would inherit it)"
                        )));
                    }
                }
            }
        }
        let _ = turn;
        Ok(())
    }

    fn run(&self, _turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        // M2b: launch the subprocess, read the event stream, ingest the reply, repair a
        // prose reply, kill and continue on timeout. No execution in the contracts task.
        unimplemented!("Engine::run lands in milestone 2b")
    }

    fn continue_turn(&self, _turn: &TurnRequest) -> Result<AttemptOutcome, EngineError> {
        unimplemented!("Engine::continue_turn lands in milestone 2b")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> Request {
        Request {
            prompt: "x".into(),
            brief_path: None,
            model: "gpt-6-astra".into(),
            provider: "openai".into(),
            engine: EngineKind::Codex,
            effort: Some("high".into()),
            timeout_sec: 1800.0,
            mode: Mode::New,
            sandbox: "read-only".into(),
            schema_path: Some(PathBuf::from("schema.json")),
            extra_config: vec![],
            output_last_message: Some(PathBuf::from("last.md")),
            prompt_file: None,
            max_model_steps: None,
        }
    }

    fn subprocess_argv(kind: EngineKind, r: &Request) -> Argv {
        match SubprocessEngine::new(kind).plan(r).unwrap() {
            LaunchPlan::Subprocess(a) => a,
            LaunchPlan::Http(_) => panic!("expected a subprocess argv"),
        }
    }

    #[test]
    fn codex_argv_shape() {
        let argv = subprocess_argv(EngineKind::Codex, &base_request());
        assert_eq!(
            argv.to_command_string(),
            "codex exec --sandbox read-only --color never --json -m gpt-6-astra \
             -c model_reasoning_effort=\"high\" -c model_provider=\"openai\" \
             -o last.md --output-schema schema.json -"
        );
    }

    #[test]
    fn codex_fork_pushes_thread_id() {
        let mut r = base_request();
        r.mode = Mode::Fork {
            thread: "T-99".into(),
            lineage: r.lineage(),
        };
        let argv = subprocess_argv(EngineKind::Codex, &r);
        // fork must be followed by the parent thread id, then `-` (F03-1).
        assert_eq!(
            argv.to_command_string(),
            "codex exec --sandbox read-only --color never --json -m gpt-6-astra \
             -c model_reasoning_effort=\"high\" -c model_provider=\"openai\" \
             -o last.md --output-schema schema.json fork T-99 -"
        );
    }

    #[test]
    fn codex_resume_options_precede_subcommand() {
        let mut r = base_request();
        r.mode = Mode::Resume {
            thread: "abc".into(),
            lineage: r.lineage(),
        };
        let argv = subprocess_argv(EngineKind::Codex, &r);
        let pos_schema = argv
            .args
            .iter()
            .position(|a| a == "--output-schema")
            .unwrap();
        let pos_resume = argv.args.iter().position(|a| a == "resume").unwrap();
        assert!(pos_schema < pos_resume, "exec options precede resume");
        assert_eq!(argv.args.last().unwrap(), "-");
    }

    #[test]
    fn toml_escaping_on_c_values() {
        let mut r = base_request();
        r.provider = "we\"ird\\p".into();
        let argv = subprocess_argv(EngineKind::Codex, &r);
        assert!(argv
            .args
            .iter()
            .any(|a| a == "model_provider=\"we\\\"ird\\\\p\""));
    }

    #[test]
    fn agy_and_muse_reject_fork() {
        let mut r = base_request();
        r.engine = EngineKind::Agy;
        r.mode = Mode::Fork {
            thread: "t".into(),
            lineage: r.lineage(),
        };
        assert_eq!(
            SubprocessEngine::new(EngineKind::Agy).plan(&r),
            Err(EngineError::UnsupportedMode {
                engine: EngineKind::Agy,
                mode: "fork"
            })
        );
        let mut r2 = base_request();
        r2.engine = EngineKind::Muse;
        r2.prompt_file = Some(PathBuf::from("p.txt"));
        r2.mode = Mode::Fork {
            thread: "t".into(),
            lineage: r2.lineage(),
        };
        assert_eq!(
            SubprocessEngine::new(EngineKind::Muse).plan(&r2),
            Err(EngineError::UnsupportedMode {
                engine: EngineKind::Muse,
                mode: "fork"
            })
        );
    }

    #[test]
    fn cross_lineage_resume_is_refused() {
        let mut r = base_request();
        r.mode = Mode::Resume {
            thread: "abc".into(),
            lineage: Lineage("someone :: else".into()),
        };
        assert!(matches!(
            SubprocessEngine::new(EngineKind::Codex).plan(&r),
            Err(EngineError::LineageMismatch { .. })
        ));
    }

    #[test]
    fn agy_and_muse_shapes() {
        let mut a = base_request();
        a.engine = EngineKind::Agy;
        let agy = subprocess_argv(EngineKind::Agy, &a);
        assert_eq!(agy.command, "agy");
        assert_eq!(agy.args[0], "-p=");
        assert!(agy.args.contains(&"--json-schema".to_string()));
        assert!(agy.args.contains(&"--effort".to_string()));

        let mut r = base_request();
        r.engine = EngineKind::Muse;
        r.prompt_file = Some(PathBuf::from("p.txt"));
        r.max_model_steps = Some(3);
        let muse = subprocess_argv(EngineKind::Muse, &r);
        assert_eq!(muse.args[0], "exec");
        assert!(muse.args.contains(&"--disable-shell".to_string()));
        assert!(muse.args.contains(&"--max-model-steps".to_string()));
    }

    #[test]
    fn http_plans_a_request_not_an_argv() {
        let mut r = base_request();
        r.engine = EngineKind::Http;
        match SubprocessEngine::new(EngineKind::Http).plan(&r).unwrap() {
            LaunchPlan::Http(h) => {
                assert_eq!(h.model, "gpt-6-astra");
                assert_eq!(h.provider, "openai");
            }
            LaunchPlan::Subprocess(_) => panic!("http must plan a request"),
        }
    }

    #[test]
    fn prompt_delivery_per_engine() {
        assert_eq!(
            capabilities(EngineKind::Codex).prompt_delivery,
            PromptDelivery::Stdin
        );
        assert_eq!(
            capabilities(EngineKind::Agy).prompt_delivery,
            PromptDelivery::StdinNdjsonLine
        );
        assert_eq!(
            capabilities(EngineKind::Muse).prompt_delivery,
            PromptDelivery::PromptFile
        );
    }

    fn valid_raw() -> RawReply {
        RawReply {
            schema_version: "1".into(),
            verdict: "ACCEPT".into(),
            verdict_reason: "ok".into(),
            reply_markdown: "body".into(),
            findings: vec![RawFinding {
                severity: "blocker".into(),
                locations: vec![RawLocation {
                    path: "a.rs".into(),
                    line: Some(Some(5)),
                    extra: Map::new(),
                }],
                claim: "c".into(),
                trigger: "t".into(),
                evidence: vec![RawEvidence {
                    kind: "read-code".into(),
                    reference: "a.rs:5".into(),
                    observation: "o".into(),
                    extra: Map::new(),
                }],
                verification: "v".into(),
                remedy: "r".into(),
                supersedes: vec![],
                extra: Map::new(),
            }],
            prior_findings: vec![RawPriorFinding {
                id: "F01-1".into(),
                status: "fixed".into(),
                note: "n".into(),
                extra: Map::new(),
            }],
            unproven: vec![],
            first_run_checklist: vec![],
            extra: Map::new(),
        }
    }

    #[test]
    fn valid_reply_converts() {
        let s = StructuredReply::try_from(valid_raw()).unwrap();
        assert_eq!(s.verdict, Verdict::Accept);
        assert_eq!(s.findings[0].severity, Severity::Blocker);
        assert_eq!(s.findings[0].locations[0].line, Some(5));
        assert_eq!(s.findings[0].evidence[0].kind, EvidenceKind::ReadCode);
        assert_eq!(s.prior_findings[0].status, PriorStatus::Fixed);
    }

    #[test]
    fn bad_schema_version_is_checked_first() {
        let mut r = valid_raw();
        r.schema_version = "2".into();
        r.verdict = "NONSENSE".into();
        assert_eq!(
            StructuredReply::try_from(r),
            Err(ReplyValidationError::BadSchemaVersion("2".into()))
        );
    }

    #[test]
    fn bad_verdict_severity_kind_and_status_are_rejected() {
        let mut r = valid_raw();
        r.verdict = "MAYBE".into();
        assert_eq!(
            StructuredReply::try_from(r),
            Err(ReplyValidationError::BadVerdict("MAYBE".into()))
        );

        let mut r = valid_raw();
        r.findings[0].severity = "critical".into();
        assert_eq!(
            StructuredReply::try_from(r),
            Err(ReplyValidationError::BadSeverity {
                finding: 0,
                value: "critical".into()
            })
        );

        let mut r = valid_raw();
        r.findings[0].evidence[0].kind = "guessed".into();
        assert_eq!(
            StructuredReply::try_from(r),
            Err(ReplyValidationError::BadEvidenceKind {
                finding: 0,
                evidence: 0,
                value: "guessed".into()
            })
        );

        let mut r = valid_raw();
        r.prior_findings[0].status = "maybe".into();
        assert_eq!(
            StructuredReply::try_from(r),
            Err(ReplyValidationError::BadPriorStatus {
                index: 0,
                value: "maybe".into()
            })
        );
    }

    #[test]
    fn missing_location_line_is_rejected() {
        // A location object that came without a `line` key parses to `None` and is rejected.
        let raw: RawReply = serde_json::from_str(
            r#"{"schema_version":"1","verdict":"HOLD","verdict_reason":"","reply_markdown":"",
                "findings":[{"severity":"note","locations":[{"path":"a.rs"}],"claim":"","trigger":"",
                "evidence":[],"verification":"","remedy":"","supersedes":[]}],
                "prior_findings":[],"unproven":[],"first_run_checklist":[]}"#,
        )
        .unwrap();
        assert_eq!(
            StructuredReply::try_from(raw),
            Err(ReplyValidationError::LocationMissingLine {
                finding: 0,
                location: 0
            })
        );
        // ... but a present `null` line is fine.
        let raw2: RawReply = serde_json::from_str(
            r#"{"schema_version":"1","verdict":"HOLD","verdict_reason":"","reply_markdown":"",
                "findings":[{"severity":"note","locations":[{"path":"a.rs","line":null}],"claim":"","trigger":"",
                "evidence":[],"verification":"","remedy":"","supersedes":[]}],
                "prior_findings":[],"unproven":[],"first_run_checklist":[]}"#,
        )
        .unwrap();
        let s = StructuredReply::try_from(raw2).unwrap();
        assert_eq!(s.findings[0].locations[0].line, None);
    }
}
