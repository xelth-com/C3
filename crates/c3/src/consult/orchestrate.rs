//! The consultation orchestrator for the codex engine (milestone 2c).
//!
//! It ties the self-contained pieces (`args`, `prompt`, `ingest`, `render`, `revision`,
//! `summary`) to the identity/launcher layer reused from [`crate::providers`] and the
//! [`crate::engines::codex`] adapter. It resolves the reviewer identity, the effort plan and
//! the schema transport, assembles the prompt, and then either prints the `--dry-run` block
//! (writing nothing) or runs one codex turn, ingests the reply, renders the handoff and
//! commits the ledger entry and findings through [`c3_core::store`].
//!
//! ## Scope (M2c, codex only)
//!
//! Implemented: the full argument surface and refusals, identity/effort/transport resolution,
//! the dry-run block, the open-findings prompt snapshot, a live codex run (structured, prose
//! and failure ingestion), the handoff render and the store commit, and the summary block.
//!
//! Deferred (documented in `docs/port/m2-status.md`): the format-repair turn, the timeout
//! continuation and `.partial.md` salvage, the pending/lock recovery of an interrupted run,
//! the peak-window evaluation, the endpoint-health 24h block, the prior-finding lifecycle
//! ingestion (retained blockers / verdict-vs-blocker), and the agy/muse engines (M2d).

use std::path::{Path, PathBuf};
use std::time::Duration;

use c3_core::effort::{caps, Models};
use c3_core::engine::{
    AttemptOutcome, ConsultationId, Engine, EngineKind, Mode, Request, StructuredReply, TurnKind,
    TurnRequest,
};
use c3_core::handoff::{Author, HandoffHeader, OptionalRecords, TokenReport};
use c3_core::ledger::{FindingCounts, LedgerEntry, RangeRecord, Reviewer, Usage};
use c3_core::lineage::{resolve_reviewer_identity, ReviewerIdentity};
use c3_core::store::{
    CommitRequest, EvidenceStore, FilesStore, FindingsDelta, LockRecord, PendingRecord, PendingRef,
    PendingState, RecoveryDisposition,
};
use c3_core::task_slug::TaskSlug;
use c3_core::verdict::{verdict_pre_credential, verdict_with_credential};

use crate::engines::codex::{CodexEngine, TurnFiles};
use crate::providers;
use crate::telemetry;

use super::args::{self, Options, Resolved};
use super::ingest;
use super::prompt::{self, OpenFinding, PromptInputs};
use super::render;
use super::revision::{self, RevisionInfo};
use super::summary;

const TOOL: &str = "codex-consult";

/// `-Range` size-warning thresholds (`$rangeWarnLines` / `$rangeWarnTimeout`).
const RANGE_WARN_LINES: i64 = 1500;
const RANGE_WARN_TIMEOUT: i64 = 2400;

/// Print a refusal (`Stop-WithError`) and return the usage exit code (1).
fn refuse(msg: &str) -> i32 {
    eprintln!("{TOOL}: {msg}");
    1
}

/// The two secondary-turn mechanisms' results (timeout continuation + format repair), gathered
/// so [`render_handoff`], [`build_entry`] and the summary render at their exact points.
#[derive(Default)]
struct Secondary {
    // --- timeout continuation ---
    timeout_continue: Option<c3_core::ledger::TimeoutContinue>,
    continued: bool,
    continue_thread: String,
    continue_wall: f64,
    continue_usage: Option<Usage>,
    /// The continuation turn's event stream (`handoffs/...continue.events.jsonl`), a further turn.
    continue_events_rel: Option<String>,
    continue_rejected: bool,
    continue_rejected_text: String,
    continue_rejected_why: String,
    /// A continuation turn was actually launched (not the "not attempted" skip).
    continue_ran: bool,
    /// The continuation turn was itself killed on its timeout.
    continue_killed: bool,
    /// The format-repair turn was killed on its timeout.
    repair_killed: bool,
    /// The `-ContinueSec` used, for the partial-footer `killed at ...` line.
    repair_timeout: i64,
    // --- format repair ---
    format_retry: Option<c3_core::ledger::FormatRetry>,
    repaired_ok: bool,
    original_prose: String,
    original_rel: String,
    repair_wall: f64,
    repair_reason: String,
    drift_notes: Vec<String>,
    /// The `format repair: ...` console line (empty when no repair ran).
    repair_console: String,
    // --- salvaged partial reply ---
    partial_needed: bool,
    partial_rel: String,
    partial_footer: String,
    /// The salvaged partial body (`Format-PartialBody`), for the `.partial.md` file.
    partial_body: String,
    /// The `resume     :` summary command (the plugin's `$resumeCommand`).
    resume_command: String,
}

/// The after-run drift (`Compare-TreeContent`, the brief/artifact re-hash).
struct Drift {
    tree_sha256_after: String,
    tree_changed: bool,
    /// `"<old> -> <new>"` when HEAD moved with identical content, else `""`.
    revision_moved: String,
    brief_sha_after: String,
    brief_changed: bool,
}

/// The effort plan for a run (`Resolve-EffortPlan`).
pub(crate) struct EffortPlan {
    pub(crate) requested: String,
    /// `None` when nothing is sent (model-tier engines).
    pub(crate) sent: Option<String>,
    pub(crate) mapping: String,
    pub(crate) caps: String,
    pub(crate) basis: String,
    pub(crate) error: String,
}

fn vocabulary_map(vocab: &str, requested: &str) -> Option<(&'static str, &'static str)> {
    // (mapping-label, sent-value) for the caps-v1 vocabularies (`$script:EffortVocabularies`).
    let (label, low, medium, high, xhigh) = match vocab {
        "openai" => ("openai", "low", "medium", "high", "xhigh"),
        "zai" => ("zai-v1", "low", "high", "high", "max"),
        "mimo" => ("mimo-v1", "low", "medium", "high", "high"),
        "ark" => ("ark-v1", "low", "medium", "high", "high"),
        "kimi" => ("kimi-v1", "low", "high", "high", "max"),
        "alibaba" => ("alibaba-v1", "low", "medium", "high", "xhigh"),
        "muse" => ("muse-v1", "low", "medium", "high", "xhigh"),
        _ => return None,
    };
    let sent = match requested {
        "low" => low,
        "medium" => medium,
        "high" => high,
        "xhigh" => xhigh,
        _ => return None,
    };
    Some((label, sent))
}

fn effort_plan(id: &ReviewerIdentity, requested: &str, native: &str) -> EffortPlan {
    let caps_v = c3_core::effort::CAPS_VERSION.to_string();
    if !native.is_empty() {
        return EffortPlan {
            requested: native.to_string(),
            sent: Some(native.to_string()),
            mapping: "native".into(),
            caps: caps_v,
            basis: "-NativeEffort, sent verbatim".into(),
            error: String::new(),
        };
    }
    let host = &id.host;
    let cap = if host.is_empty() { None } else { caps(host) };
    let cap = match cap {
        Some(c) => c,
        None => {
            let host_label = if host.is_empty() {
                "unknown-host"
            } else {
                host
            };
            return EffortPlan {
                requested: requested.to_string(),
                sent: None,
                mapping: String::new(),
                caps: caps_v,
                basis: String::new(),
                error: format!(
                    "no effort vocabulary declared for {host_label} ({}); pass -NativeEffort <value> to send a value verbatim",
                    c3_core::effort::CAPS_VERSION
                ),
            };
        }
    };
    if cap.vocabulary == "model-tier" {
        return EffortPlan {
            requested: requested.to_string(),
            sent: None,
            mapping: "model-tier".into(),
            caps: caps_v,
            basis: format!(
                "{}: engine {}, the tier is part of the model id",
                c3_core::effort::CAPS_VERSION,
                host.trim_start_matches("engine:")
            ),
            error: String::new(),
        };
    }
    // Model-list check.
    if let Models::List(list) = cap.models {
        if id.model_source == "unknown" || !list.contains(&id.model.as_str()) {
            return EffortPlan {
                requested: requested.to_string(),
                sent: None,
                mapping: String::new(),
                caps: caps_v,
                basis: String::new(),
                error: format!(
                    "no effort vocabulary declared for model '{}' on {host} ({} declares: {}); pass -NativeEffort <value> to send a value verbatim",
                    id.model, c3_core::effort::CAPS_VERSION, list.join(", ")
                ),
            };
        }
    }
    match vocabulary_map(cap.vocabulary, requested) {
        Some((mapping, sent)) => {
            let basis = match cap.models {
                Models::Any => format!("{}: {host}, any model", c3_core::effort::CAPS_VERSION),
                Models::List(_) => format!("{}: {host}, {}", c3_core::effort::CAPS_VERSION, id.model),
            };
            EffortPlan {
                requested: requested.to_string(),
                sent: Some(sent.to_string()),
                mapping: mapping.to_string(),
                caps: caps_v,
                basis,
                error: String::new(),
            }
        }
        None => EffortPlan {
            requested: requested.to_string(),
            sent: None,
            mapping: String::new(),
            caps: caps_v,
            basis: String::new(),
            error: format!(
                "{} names the effort vocabulary '{}' for {host}, but no such vocabulary is declared",
                c3_core::effort::CAPS_VERSION, cap.vocabulary
            ),
        },
    }
}

/// The preflight verdict for the codex engine (M2c: credentials only). Returns the
/// `preflight` string for the ledger and, for a real run, an optional `(refusal, exit_code)`.
/// A non-openai provider's credential check is deferred: it is left unevaluated (`""`) and
/// never refuses, so such a run still proceeds (the recorded endpoint-health 24h block and
/// the non-openai credential/env-key check land later).
fn resolve_preflight(
    id: &ReviewerIdentity,
    launcher: &str,
    config: &c3_core::config::CodexConfig,
    collab_root: &Path,
) -> (String, Option<(String, i32)>) {
    // The plugin refuses a preflight through `Stop-WithError` (exit 1, nothing written,
    // `codex-consult.ps1:304`); c3 matches that, not cli-surface.md's aspirational exit 2/3.
    if let Some(v) = verdict_pre_credential(id) {
        return (v.preflight, Some((v.refusal, 1)));
    }
    // Endpoint health from every task ledger of THIS repository, read at the consult clock and
    // matched by the resolved identity's provider fingerprint: a recorded auth failure (within
    // 24 h), usage limit (with a reset), burst 429 (10 min) or reset-less quota (60 min) blocks
    // a later run before the lock (F09-2/4).
    let health = if id.resolved {
        let consults = providers::read_all_task_consults(collab_root);
        let utc_now = c3_core::peak::consult_clock(0)
            .map(|(u, _, _)| u)
            .unwrap_or_else(|_| chrono::Utc::now());
        Some(c3_core::health::endpoint_health(
            &consults,
            &id.fingerprint,
            utc_now,
        ))
    } else {
        None
    };
    // The credential check: openai runs `codex login status`; a third-party provider checks its
    // `env_key` (`env X not set`) / bearer token. `-SkipPreflight` bypasses this whole function.
    let timeout = std::env::var("CODEX_CONSULT_TEST_LOGIN_TIMEOUT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(20);
    let cred = providers::identity_credential(config, id, launcher, false, timeout);
    let v = verdict_with_credential(id, health.as_ref(), cred, false);
    if v.state == "available" {
        (v.preflight, None)
    } else {
        (v.preflight, Some((v.refusal, 1)))
    }
}

/// The `-SkipPreflight` quota warning (`Format-QuotaWarning`): empty unless a quota record is
/// active on the endpoint. A known reset names the reset time; a reset-less limit names the
/// out-window end and marks a burst 429.
fn format_quota_warning(id: &ReviewerIdentity, health: &c3_core::health::EndpointHealth) -> String {
    let q = match &health.quota {
        Some(q) => q,
        None => return String::new(),
    };
    if health.quota_known {
        format!(
            "provider {} hit a usage limit {} min ago that lasts until {}: {}",
            id.provider, q.age_minutes, q.retry_after_iso, q.message
        )
    } else {
        let limit = if q.kind == "burst" {
            "burst limit (429)"
        } else {
            "usage limit"
        };
        format!(
            "provider {} hit a {limit} {} min ago (reset unknown; out until {}): {}",
            id.provider,
            q.age_minutes,
            c3_core::health::format_offset_iso(q.until),
            q.message
        )
    }
}

/// The resolved schema transport and its source.
pub(crate) struct Transport {
    /// `output-schema` | `prompt-only` | `native` | `""` (raw).
    pub(crate) transport: String,
    pub(crate) source: String,
    pub(crate) basis: String,
}

fn resolve_transport(id: &ReviewerIdentity, r: &Resolved) -> Transport {
    if r.raw {
        return Transport {
            transport: String::new(),
            source: String::new(),
            basis: String::new(),
        };
    }
    if !r.transport_override.is_empty() {
        return Transport {
            transport: r.transport_override.clone(),
            source: "-SchemaTransport".into(),
            basis: "-SchemaTransport".into(),
        };
    }
    let host = &id.host;
    match if host.is_empty() { None } else { caps(host) } {
        Some(c) => Transport {
            transport: c.schema_transport.to_string(),
            source: c3_core::effort::CAPS_VERSION.to_string(),
            basis: format!("{}: {host}", c3_core::effort::CAPS_VERSION),
        },
        None => Transport {
            transport: "prompt-only".into(),
            source: c3_core::effort::CAPS_VERSION.to_string(),
            basis: format!(
                "{} default: no vocabulary for the endpoint",
                c3_core::effort::CAPS_VERSION
            ),
        },
    }
}

/// Resolved run context, shared by the dry-run and live branches.
pub(crate) struct Context {
    pub(crate) o: Options,
    pub(crate) r: Resolved,
    pub(crate) repo_root: PathBuf,
    pub(crate) collab_root: PathBuf,
    pub(crate) task: TaskSlug,
    pub(crate) reply_name: String,
    pub(crate) nn: u32,
    pub(crate) consult_n: i64,
    pub(crate) consult_id: String,
    pub(crate) identity: ReviewerIdentity,
    pub(crate) effort: EffortPlan,
    pub(crate) transport: Transport,
    pub(crate) launcher: String,
    pub(crate) codex_version: String,
    pub(crate) prompt_text: String,
    pub(crate) argv_display: String,
    pub(crate) argv: Vec<String>,
    pub(crate) brief_ref: String,
    pub(crate) brief_path: Option<PathBuf>,
    /// The brief's sha256 hex taken before the run (`$briefSha`); empty when no brief.
    pub(crate) brief_sha: String,
    pub(crate) schema_path: Option<PathBuf>,
    pub(crate) open_findings_count: usize,
    /// The effective mode after the parent-thread walk (`new`|`fork`|`resume`), recorded in the
    /// ledger and used to plan the codex `resume`/`fork` argv.
    pub(crate) effective_mode: String,
    /// The resolved parent thread (`Select-ParentThread`'s `$r.Parent`); empty for a new thread.
    pub(crate) parent_thread: String,
    /// The parent-thread note (`$r.Note`); empty when none.
    pub(crate) parent_note: String,
    /// The preflight string recorded in the ledger / handoff (`""` when not evaluated).
    pub(crate) preflight: String,
    /// The `-SkipPreflight` quota warning (`Format-QuotaWarning`), printed `WARNING: ...` before
    /// a live launch and recorded in the ledger; empty otherwise.
    pub(crate) preflight_warning: String,
    /// A preflight refusal `(message, exit_code)` for a real run; `None` = available/skipped.
    pub(crate) preflight_refusal: Option<(String, i32)>,
    pub(crate) revision: RevisionInfo,
    /// The measured `-Range` record for the ledger; `None` when no range was given.
    pub(crate) range_record: Option<RangeRecord>,
    /// The reviewer note text (`the range changes N files, N lines`); empty when no range.
    pub(crate) range_text: String,
    /// Run warnings (`$runWarnings`): the range size warning, roster ambiguity, drift, etc.
    pub(crate) run_warnings: Vec<String>,
    /// The provider whose peak schedule is evaluated (`""` when the identity is unresolved).
    pub(crate) peak_provider: String,
    /// The peak-window state and its record fields (evaluated at launch for a live run).
    pub(crate) peak: Option<bool>,
    pub(crate) peak_schedule: String,
    pub(crate) peak_source: String,
    pub(crate) peak_evaluated_at: String,
    /// The `WARNING: ... peak window ...` line (empty when off-peak/unknown).
    pub(crate) peak_warning: String,
    /// The dry-run `peak` label (`PEAK (...)` / `off-peak (...)` / `unknown (...)`).
    pub(crate) peak_label: String,
    /// Whether telemetry is enabled for this run (`--telemetry`/env switch).
    pub(crate) telemetry_enabled: bool,
    /// The dry-run `pending :` recovery lines (a dry run reports, never refuses).
    pub(crate) recovery_dry_lines: Vec<String>,
    /// The recovered/cleared lines of consumed records (set under the lock in `run_live`),
    /// echoed to the console and carried into the handoff header as `Recovery record: ...`.
    pub(crate) recovery_lines: Vec<String>,
    // paths
    pub(crate) handoffs_dir: PathBuf,
    pub(crate) reply_path: PathBuf,
    pub(crate) reply_json_path: PathBuf,
    pub(crate) events_path: PathBuf,
    pub(crate) last_msg_path: PathBuf,
    pub(crate) stderr_path: PathBuf,
}

/// Run one consultation; return the exit code. Wraps [`run_inner`] with the telemetry
/// background flush and the one-time notice (a real run only — a dry run does nothing), and
/// joins the flush (capped at 3 s) at every exit path.
pub fn run(o: Options) -> i32 {
    let cfg = telemetry::Config {
        telemetry: o.telemetry,
    };
    let real = telemetry::is_enabled(&cfg) && !o.dry_run;
    let bg = if real {
        Some(telemetry::flush_in_background())
    } else {
        None
    };
    if real {
        if let Some(notice) = telemetry::first_run_notice() {
            println!("{notice}");
        }
    }
    let code = run_inner(o);
    if let Some(bg) = bg {
        bg.join_with_cap(Duration::from_secs(3));
    }
    code
}

fn run_inner(o: Options) -> i32 {
    let codex_home = std::env::var("CODEX_HOME").ok();
    let home = codex_home.as_deref();
    let r = match args::validate(&o, home) {
        Ok(r) => r,
        Err(msg) => return refuse(&msg),
    };
    match build_context(o, r) {
        Ok(ctx) => {
            if ctx.o.dry_run {
                super::dryrun::render(&ctx);
                0
            } else if let Some((msg, code)) = ctx.preflight_refusal.clone() {
                // A preflight refusal happens before the lock: nothing is started or written.
                eprintln!("{TOOL}: {msg}");
                code
            } else {
                run_live(ctx)
            }
        }
        Err((msg, code)) => {
            if code == 1 {
                refuse(&msg)
            } else {
                eprintln!("{TOOL}: {msg}");
                code
            }
        }
    }
}

fn build_context(o: Options, r: Resolved) -> Result<Context, (String, i32)> {
    let o_telemetry = o.telemetry; // captured before `o` moves into the Context
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &o.collab_dir);
    let task = TaskSlug::new(o.task.clone()).map_err(|e| (e.to_string(), 1))?;

    // Launcher + config + identity.
    let launcher = providers::resolve_codex_launcher(&o.codex_exe).map_err(|m| (m, 1))?;
    let config_path = providers::get_codex_config_path();
    let config = providers::read_codex_config(&config_path);
    let openai_base_url = std::env::var("OPENAI_BASE_URL").unwrap_or_default();
    let identity = resolve_reviewer_identity(
        &config,
        &o.provider,
        &o.model,
        &openai_base_url,
        "codex",
        &launcher,
    );

    // A resolved identity is required for fork/resume (F02-1).
    if !identity.resolved && (o.mode == "fork" || o.mode == "resume") {
        return Err((
            format!(
                "provider identity could not be resolved ({}); only -Mode new is allowed.",
                if identity.note.is_empty() {
                    identity.error.clone()
                } else {
                    identity.note.clone()
                }
            ),
            1,
        ));
    }
    if !o.provider.is_empty() && o.model.is_empty() {
        return Err((
            format!("-Provider needs -Model: the bridge cannot know which model a provider serves by default (e.g. -Provider {} -Model <model>).", o.provider),
            1,
        ));
    }

    // Preflight (M2c: credentials only, no recorded endpoint-health/24h block). openai runs
    // `codex login status`; a non-openai provider's credential check is deferred (its
    // preflight is left unevaluated and never refuses). `-SkipPreflight` bypasses the check.
    let (preflight, preflight_refusal, preflight_warning) = if o.skip_preflight {
        // The check is skipped, but an active quota record still earns a warning (the plugin's
        // `Format-QuotaWarning`), printed before launch and recorded in the ledger.
        let warning = if identity.resolved {
            let consults = providers::read_all_task_consults(&collab_root);
            let utc_now = c3_core::peak::consult_clock(0)
                .map(|(u, _, _)| u)
                .unwrap_or_else(|_| chrono::Utc::now());
            let health =
                c3_core::health::endpoint_health(&consults, &identity.fingerprint, utc_now);
            format_quota_warning(&identity, &health)
        } else {
            String::new()
        };
        ("skipped".to_string(), None, warning)
    } else {
        let (p, r) = resolve_preflight(&identity, &launcher, &config, &collab_root);
        (p, r, String::new())
    };

    let effort = effort_plan(
        &identity,
        effort_requested(&o, &r).as_str(),
        &o.native_effort,
    );
    if !effort.error.is_empty() {
        return Err((effort.error, 1));
    }
    let transport = resolve_transport(&identity, &r);

    // Peak window (evaluated once now — the early check). A malformed schedule refuses (naming
    // the variable and the bad token); `-OffPeakOnly` refuses with no schedule or inside the
    // window. The status that the LEDGER records is re-evaluated right before launch
    // (`run_live`), which may cross a window boundary. `peakProvider` is the resolved provider,
    // or `""` when the identity is unresolved.
    let peak_provider = if identity.provider_source.is_empty() {
        String::new()
    } else {
        identity.provider.clone()
    };
    let peak = c3_core::peak::peak_status_now(&peak_provider, 0);
    if !peak.error.is_empty() {
        return Err((peak.error, 1));
    }
    if o.off_peak_only {
        if peak_provider.is_empty() {
            return Err((format!(
                "no schedule for provider unknown (the reviewer identity is unresolved: {}); -OffPeakOnly needs a known provider and its CODEX_CONSULT_PEAK_<PROVIDER>.",
                identity.note
            ), 1));
        }
        if peak.peak.is_none() {
            return Err((
                format!(
                    "no schedule for provider {peak_provider}; -OffPeakOnly needs {}.",
                    peak.variable
                ),
                1,
            ));
        }
        if peak.peak == Some(true) {
            return Err((format!(
                "-OffPeakOnly: {peak_provider} is inside its peak window ({}; now {}); nothing was started.",
                peak.schedule, peak.local
            ), 1));
        }
    }
    let (peak_warning, peak_label) = peak_display(&peak_provider, &peak);

    // Numbering.
    let store = FilesStore::new(collab_root.clone());

    // Parent-thread walk (`Select-ParentThread`): validate `-Thread` against this task's ledger
    // by lineage + provenance, or resolve the automatic parent (the newest verified thread of
    // this lineage) for `-Mode fork|resume`. A refusal here happens before the lock (nothing
    // started). `-Thread needs -Mode fork or resume` is already refused in `args::validate`.
    let ledger_entries = store
        .read_sessions(&task)
        .ok()
        .flatten()
        .map(|s| s.codex.consults)
        .unwrap_or_default();
    let parent = match select_parent_thread(&ledger_entries, &identity, &o.mode, &o.thread) {
        Ok(p) => p,
        Err(refusal) => return Err((refusal, 1)),
    };
    let effective_mode = parent.mode.clone();
    let parent_thread = parent.parent_thread.clone();
    let parent_note = parent.note.clone();

    let nn_n = store.next_numbers(&task).map_err(|e| (e.to_string(), 1))?;
    let (nn, consult_n) = (nn_n.nn, nn_n.n);

    let reply_name = if o.reply_name.is_empty() {
        "reply".to_string()
    } else {
        o.reply_name.clone()
    };
    let handoffs_dir = collab_root.join(task.as_str()).join("handoffs");
    let stem = format!("{:02}-codex-{}", nn, reply_name);
    let reply_path = handoffs_dir.join(format!("{stem}.md"));
    let reply_json_path = handoffs_dir.join(format!("{stem}.reply.json"));
    let events_path = handoffs_dir.join(format!("{stem}.events.jsonl"));
    // The `-o` last-message file and the stderr sidecar are system-temp files named exactly
    // like the plugin (`<temp>/codex-consult-last-<guidN>.md` / `-stderr-<guidN>.txt`), not
    // handoff files: they are transient and removed after the run.
    let tmp_id = uuid::Uuid::new_v4().simple().to_string();
    let tmp_root = std::env::temp_dir();
    let last_msg_path = tmp_root.join(format!("codex-consult-last-{tmp_id}.md"));
    let stderr_path = tmp_root.join(format!("codex-consult-stderr-{tmp_id}.txt"));

    // Brief ref (repo-relative) + existence check.
    let mut brief_ref = String::new();
    let mut brief_path = None;
    if !o.brief.is_empty() {
        let bp = if Path::new(&o.brief).is_absolute() {
            PathBuf::from(&o.brief)
        } else {
            cwd.join(&o.brief)
        };
        if !bp.is_file() {
            return Err((
                format!(
                    "brief '{}' not found (this tool never writes briefs; write it first).",
                    o.brief
                ),
                1,
            ));
        }
        brief_ref =
            c3_core::paths::repo_relative(&repo_root, &bp).unwrap_or_else(|| o.brief.clone());
        brief_path = Some(bp);
    }
    let brief_sha = brief_path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| c3_core::sha256_hex(&b))
        .unwrap_or_default();

    // The reply schema path (shipped with the tool; in prompt-only its text is inlined).
    let schema_path = if r.raw { None } else { schema_file(&repo_root) };
    let schema_text = if transport.transport == "prompt-only" {
        // The embedded schema, trimmed and normalised to CRLF exactly as the plugin inlines
        // its on-disk file (`.Trim() -replace "`r`n","`n" -replace "`n", $nl`).
        c3_core::schema::REPLY_SCHEMA_V1
            .trim()
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
    } else {
        String::new()
    };

    // Open findings snapshot.
    let (open_findings, open_findings_count) = read_open_findings(&store, &task);

    let consult_id = uuid::Uuid::new_v4().to_string();

    // Range (`git diff --shortstat <spec> --`, measured once, before the lock). An unknown
    // range refuses here (nothing started). The counts go to the prompt, the ledger `range{}`
    // record and — over 1500 lines under a sub-2400 s timeout — a size warning.
    let mut run_warnings: Vec<String> = Vec::new();
    let mut range_record: Option<RangeRecord> = None;
    let mut range_text = String::new();
    let mut prompt_range: Option<prompt::Range> = None;
    if !o.range.is_empty() {
        let rs = revision::range_stat(&repo_root, &o.range);
        if !rs.error.is_empty() {
            return Err((format!("{}; nothing was started.", rs.error), 1));
        }
        range_text = revision::range_text(rs.files, rs.lines);
        if rs.lines > RANGE_WARN_LINES && r.timeout_sec < RANGE_WARN_TIMEOUT {
            run_warnings.push(format!(
                "a range of {} lines with a {} s timeout: pass -TimeoutSec or a reading plan in the brief",
                rs.lines, r.timeout_sec
            ));
        }
        prompt_range = Some(prompt::Range {
            spec: o.range.clone(),
            text: range_text.clone(),
            insertions: rs.insertions,
            deletions: rs.deletions,
        });
        range_record = Some(RangeRecord {
            spec: o.range.clone(),
            files: rs.files,
            insertions: rs.insertions,
            deletions: rs.deletions,
            lines: rs.lines,
            ..Default::default()
        });
    }

    let prompt_text = prompt::assemble(&PromptInputs {
        raw: r.raw,
        purpose: &o.purpose,
        prompt: &o.prompt,
        brief_ref: &brief_ref,
        range: prompt_range.as_ref(),
        open_findings: &open_findings,
        schema_transport: &transport.transport,
        schema_text: &schema_text,
        max_words: r.max_words,
        consult_id: &consult_id,
    });

    // The argv (byte-identical to the core plan): build the Request and plan it.
    let request = make_request(
        &o,
        &r,
        &identity,
        &effort,
        &transport,
        &prompt_text,
        &schema_path,
        &last_msg_path,
        resolved_mode(&effective_mode, &parent_thread, &identity),
    );
    let argv = match c3_core::engine::SubprocessEngine::new(EngineKind::Codex).plan(&request) {
        Ok(c3_core::engine::LaunchPlan::Subprocess(a)) => a,
        _ => return Err(("could not plan the codex argv".into(), 1)),
    };
    let argv_display = argv.to_command_string();

    let codex_version = get_codex_version(&launcher);
    let revision = revision::revision_info(&repo_root, Some(&collab_root));

    // Recovery records: a dry run only reports them (`pending :` lines); a real run reads and
    // acts on them under the lock (`run_live`). An unusable record refuses even a dry run.
    let mut recovery_dry_lines: Vec<String> = Vec::new();
    if o.dry_run {
        let rec = super::recovery::assess(&store, &task);
        if let Some(err) = rec.error {
            return Err((err, 1));
        }
        recovery_dry_lines = rec.items.iter().map(super::recovery::dry_line).collect();
    }

    Ok(Context {
        o,
        r,
        repo_root,
        collab_root,
        task,
        reply_name,
        nn,
        consult_n,
        consult_id,
        identity,
        effort,
        transport,
        launcher,
        codex_version,
        prompt_text,
        argv_display,
        argv: argv.args,
        brief_ref,
        brief_path,
        brief_sha,
        schema_path,
        open_findings_count,
        effective_mode,
        parent_thread,
        parent_note,
        preflight,
        preflight_warning,
        preflight_refusal,
        revision,
        range_record,
        range_text,
        run_warnings,
        peak_provider,
        peak: peak.peak,
        peak_schedule: peak.schedule.clone(),
        peak_source: peak.source.clone(),
        peak_evaluated_at: peak.evaluated_at.clone(),
        peak_warning,
        peak_label,
        telemetry_enabled: telemetry::is_enabled(&telemetry::Config {
            telemetry: o_telemetry,
        }),
        recovery_dry_lines,
        recovery_lines: Vec::new(),
        handoffs_dir,
        reply_path,
        reply_json_path,
        events_path,
        last_msg_path,
        stderr_path,
    })
}

/// `($peakWarning, $peakLabel)` for a peak status (`codex-consult.ps1:2625-2627`).
fn peak_display(provider: &str, st: &c3_core::peak::PeakStatus) -> (String, String) {
    let warning = if st.peak == Some(true) {
        format!(
            "{provider} peak window ({}) - this consultation runs at peak tariff.",
            st.schedule
        )
    } else {
        String::new()
    };
    let label = match st.peak {
        None => {
            if st.variable.is_empty() {
                "unknown (provider unknown)".to_string()
            } else {
                format!("unknown ({} not set)", st.variable)
            }
        }
        Some(true) => format!("PEAK ({}; now {})", st.schedule, st.local),
        Some(false) => format!(
            "off-peak ({}; now {}; {})",
            st.schedule, st.local, st.detail
        ),
    };
    (warning, label)
}

fn effort_requested(o: &Options, _r: &Resolved) -> String {
    if !o.effort.is_empty() {
        o.effort.clone()
    } else {
        prompt::presets::effort(&o.purpose).to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn make_request(
    o: &Options,
    r: &Resolved,
    id: &ReviewerIdentity,
    effort: &EffortPlan,
    transport: &Transport,
    prompt_text: &str,
    schema_path: &Option<PathBuf>,
    last_msg_path: &Path,
    mode: Mode,
) -> Request {
    let sandbox = if o.sandbox.is_empty() {
        "read-only".to_string()
    } else {
        o.sandbox.clone()
    };
    // --output-schema only when the transport is output-schema (codex).
    let schema_arg = if transport.transport == "output-schema" {
        schema_path.clone()
    } else {
        None
    };
    Request {
        prompt: prompt_text.to_string(),
        brief_path: None,
        model: if id.model_source == "unknown" {
            String::new()
        } else {
            id.model.clone()
        },
        provider: if id.provider_source.is_empty() {
            String::new()
        } else {
            id.provider.clone()
        },
        engine: EngineKind::Codex,
        effort: effort.sent.clone(),
        timeout_sec: r.timeout_sec as f64,
        mode,
        sandbox,
        schema_path: schema_arg,
        extra_config: r.extra_config.clone(),
        output_last_message: Some(last_msg_path.to_path_buf()),
        prompt_file: None,
        max_model_steps: None,
    }
}

/// The codex `Mode` from the resolved parent walk: a `fork`/`resume` on the resolved parent
/// thread (carrying the run's own lineage so the core refuses a cross-lineage resume), else a
/// fresh thread.
fn resolved_mode(effective_mode: &str, parent_thread: &str, id: &ReviewerIdentity) -> Mode {
    if parent_thread.is_empty() {
        return Mode::New;
    }
    let lineage = c3_core::engine::Lineage(id.lineage.clone());
    if effective_mode == "resume" {
        Mode::Resume {
            thread: parent_thread.to_string(),
            lineage,
        }
    } else {
        Mode::Fork {
            thread: parent_thread.to_string(),
            lineage,
        }
    }
}

/// The resolved parent-thread walk result (`Select-ParentThread`'s `$r`).
#[derive(Debug)]
struct ParentResolved {
    parent_thread: String,
    /// The mode after the walk (`new` when no parent; `fork` defaulted from auto with a parent).
    mode: String,
    /// The parent note text (`$r.Note`); empty when none.
    note: String,
}

/// `Test-SameReviewer`: provider and model equal (ordinal, case-sensitive), each on its own,
/// and the same engine (an absent one is codex).
fn same_reviewer(rev: &Reviewer, id: &ReviewerIdentity) -> bool {
    let id_engine = if id.engine.is_empty() {
        "codex"
    } else {
        id.engine.as_str()
    };
    let entry_engine = if rev.engine.is_empty() {
        "codex"
    } else {
        rev.engine.as_str()
    };
    rev.provider == id.provider && rev.model == id.model && entry_engine == id_engine
}

/// An entry's reviewer lineage (`Get-EntryReviewer`'s `Display` = `Format-ReviewerLineage`).
fn entry_reviewer_lineage(rev: &Reviewer) -> String {
    let engine = if rev.engine.is_empty() {
        "codex"
    } else {
        rev.engine.as_str()
    };
    c3_core::lineage::format_reviewer_lineage(&rev.provider, &rev.model, engine)
}

/// Whether an entry predates the reviewer record (0.1/0.2): no reviewer fields at all. C3 cannot
/// see a truly absent `reviewer` key (it deserializes to a blank record), so an all-empty
/// reviewer is treated as legacy.
fn reviewer_absent(rev: &Reviewer) -> bool {
    rev.provider.is_empty()
        && rev.model.is_empty()
        && rev.engine.is_empty()
        && rev.provider_fingerprint.is_empty()
}

/// `Select-ParentThread`: resolve the parent thread (a `resume`/`fork` target) and the effective
/// mode from `-Mode`/`-Thread` and this task's ledger, or an `Err` refusal (byte-identical to the
/// plugin). `-Thread` is validated by lineage and provenance; the automatic parent is the newest
/// verified thread of the same lineage.
fn select_parent_thread(
    entries: &[LedgerEntry],
    id: &ReviewerIdentity,
    mode: &str,
    thread: &str,
) -> Result<ParentResolved, String> {
    let lineage = &id.lineage;
    let unresolved_msg = format!(
        "provider identity could not be resolved ({}); pass -Provider and -Model explicitly, or use -Mode new",
        id.note
    );
    let drift_msg = |t: &str, n: &str, fp: &str| {
        format!(
            "endpoint or protocol of provider {} changed since thread {t} (consult n={n} recorded provider fingerprint {}, now {}); start a new thread with -Mode new",
            id.provider,
            short_hash(fp),
            short_hash(&id.fingerprint)
        )
    };
    let thread = thread.trim();
    let mut res = ParentResolved {
        parent_thread: String::new(),
        mode: mode.to_string(),
        note: String::new(),
    };

    if !thread.is_empty() {
        if !id.resolved {
            return Err(unresolved_msg);
        }
        // Find-ThreadEntry: the newest entry recording this thread as a verified thread.
        let match_entry = match entries.iter().rev().find(|e| e.thread.trim() == thread) {
            Some(e) => e,
            None => {
                // A codex candidate (a foreign rollout) is never a parent; name it if present.
                let cand = entries
                    .iter()
                    .rev()
                    .find(|e| e.thread_candidate.trim() == thread);
                return Err(match cand {
                    Some(c) => format!(
                        "thread {thread} has unknown provenance: it is only an unverified rollout candidate of consult n={} (that rollout did not contain the run's consultation id); use -Mode new",
                        c.n
                    ),
                    None => format!(
                        "thread {thread} has unknown provenance: it is not in this task's ledger; use -Mode new"
                    ),
                });
            }
        };
        let n = match_entry.n;
        if reviewer_absent(&match_entry.reviewer) {
            return Err(format!(
                "thread {thread} has unknown provenance (recorded before 0.3.0); use -Mode new"
            ));
        }
        if match_entry.reviewer.provider_fingerprint.is_empty() {
            return Err(format!(
                "thread {thread} has unknown provenance: consult n={n} ran with an unresolved reviewer identity ({}); use -Mode new",
                entry_reviewer_lineage(&match_entry.reviewer)
            ));
        }
        if !same_reviewer(&match_entry.reviewer, id) {
            let theirs = entry_reviewer_lineage(&match_entry.reviewer);
            return Err(format!(
                "thread {thread} belongs to lineage {theirs} (consult n={n}); this run is {lineage}. A thread never changes provider or model: use -Mode new, or run as {theirs}"
            ));
        }
        let fp = &match_entry.reviewer.provider_fingerprint;
        if *fp != id.fingerprint {
            return Err(drift_msg(thread, &n.to_string(), fp));
        }
        if res.mode.is_empty() {
            res.mode = "fork".into();
        }
        res.parent_thread = thread.to_string();
        res.note = format!("-Thread, lineage {lineage} (consult n={n})");
        return Ok(res);
    }

    // No -Thread.
    if !id.resolved {
        if mode == "fork" || mode == "resume" {
            return Err(unresolved_msg);
        }
        res.mode = "new".into();
        res.note = format!(
            "reviewer identity unresolved, automatic fork/resume is off ({})",
            id.note
        );
        return Ok(res);
    }

    // The automatic parent: the newest verified thread of the same lineage.
    let mut parent: Option<&LedgerEntry> = None;
    let (mut legacy, mut unresolved, mut candidates) = (0i64, 0i64, 0i64);
    let mut others: Vec<String> = Vec::new();
    for c in entries.iter().rev() {
        let t = c.thread.trim();
        if t.is_empty() {
            if !c.thread_candidate.trim().is_empty() {
                candidates += 1;
            }
            continue;
        }
        if reviewer_absent(&c.reviewer) {
            legacy += 1;
            continue;
        }
        if c.reviewer.provider_fingerprint.is_empty() {
            unresolved += 1;
            continue;
        }
        if !same_reviewer(&c.reviewer, id) {
            let d = entry_reviewer_lineage(&c.reviewer);
            if !others.contains(&d) {
                others.push(d);
            }
            continue;
        }
        if parent.is_none() {
            parent = Some(c);
        }
    }
    if let Some(p) = parent {
        if mode == "new" {
            return Ok(res); // -Mode new starts a fresh thread even when a parent is available.
        }
        let pt = p.thread.trim().to_string();
        let n = p.n;
        let fp = &p.reviewer.provider_fingerprint;
        if *fp != id.fingerprint {
            return Err(drift_msg(&pt, &n.to_string(), fp));
        }
        if res.mode.is_empty() {
            res.mode = "fork".into();
        }
        res.parent_thread = pt;
        res.note = format!("newest thread of lineage {lineage} (consult n={n})");
        return Ok(res);
    }

    // No parent of this lineage.
    let mut why: Vec<String> = Vec::new();
    if legacy > 0 {
        why.push(format!(
            "{legacy} thread(s) recorded before 0.3.0 have unknown provenance and are never automatic parents"
        ));
    }
    if !others.is_empty() {
        why.push(format!("other lineage(s): {}", others.join(", ")));
    }
    if unresolved > 0 {
        why.push(format!(
            "{unresolved} thread(s) of runs with an unresolved reviewer identity are never parents"
        ));
    }
    if candidates > 0 {
        why.push(format!(
            "{candidates} unverified rollout candidate(s) are never parents"
        ));
    }
    let mut note = format!("no thread of lineage {lineage} in this task's ledger");
    if !why.is_empty() {
        note.push_str(&format!("; {}", why.join("; ")));
    }
    if mode == "fork" || mode == "resume" {
        return Err(format!(
            "-Mode {mode} needs a parent thread: {note}. Pass -Thread <uuid> of lineage {lineage}, or use -Mode new"
        ));
    }
    res.mode = "new".into();
    if !entries.is_empty() {
        res.note = note;
    }
    Ok(res)
}

fn schema_file(_repo_root: &Path) -> Option<PathBuf> {
    // The reply schema is embedded in the binary (`c3_core::schema::REPLY_SCHEMA_V1`, the
    // plugin's file byte-for-byte). Materialise it under `<CODEX_HOME>/c3/schemas/
    // consult-reply.v1.json` (rewritten only when missing/different) and pass THAT path to
    // `--output-schema`, mirroring the plugin passing its own on-disk schema file. With no
    // resolvable codex home, name the would-be path without writing (a dry run still shows it).
    let home = providers::get_codex_home();
    if home.is_empty() {
        return None;
    }
    let home = PathBuf::from(&home);
    match c3_core::schema::materialize(&home) {
        Ok(p) => Some(p),
        Err(_) => Some(c3_core::schema::materialized_path(&home)),
    }
}

fn get_codex_version(launcher: &str) -> String {
    if launcher.is_empty() {
        return "codex (version unknown)".to_string();
    }
    let (prog, args): (String, Vec<String>) = if cfg!(windows)
        && Path::new(launcher)
            .extension()
            .map(|e| {
                let e = e.to_string_lossy().to_lowercase();
                e == "cmd" || e == "bat"
            })
            .unwrap_or(false)
    {
        (
            "cmd".into(),
            vec!["/c".into(), launcher.into(), "--version".into()],
        )
    } else {
        (launcher.into(), vec!["--version".into()])
    };
    std::process::Command::new(prog)
        .args(args)
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "codex (version unknown)".to_string())
}

fn read_open_findings(store: &FilesStore, task: &TaskSlug) -> (Vec<OpenFinding>, usize) {
    let findings = match store.read_findings(task) {
        Ok(Some(f)) => f,
        _ => return (Vec::new(), 0),
    };
    let mut out = Vec::new();
    for f in &findings.findings {
        let status = f.status().as_str().to_string();
        if status != "proposed" && status != "implemented" {
            continue;
        }
        let locs: Vec<c3_core::engine::ReplyLocation> = f
            .locations
            .iter()
            .map(|l| c3_core::engine::ReplyLocation {
                path: l.path.clone(),
                line: l.line,
            })
            .collect();
        out.push(OpenFinding {
            id: f.id.clone(),
            status,
            locations: render::format_locations(&locs, true),
            claim: f.claim.clone(),
            trigger: f.trigger.clone(),
            verification: f.verification.clone(),
        });
    }
    let n = out.len();
    (out, n)
}

// ------------------------------------------------------------------------------- live run

fn run_live(mut ctx: Context) -> i32 {
    // Ensure the handoffs dir exists.
    if let Err(e) = std::fs::create_dir_all(&ctx.handoffs_dir) {
        return refuse(&format!("could not create the handoffs directory: {e}"));
    }
    // The `-SkipPreflight` quota warning prints before the lock (never on a dry run, which
    // never reaches `run_live`).
    if !ctx.preflight_warning.is_empty() {
        println!("WARNING: {}", ctx.preflight_warning);
    }
    let store = FilesStore::new(ctx.collab_root.clone());
    let pending = PendingRef::single(ctx.task.clone());

    // Task ownership lock (fail-fast).
    let lock_record = LockRecord::now(&ctx.task, None);
    let _task_lock = match store.take_task_lock(&ctx.task, &lock_record) {
        Ok(l) => l,
        Err(e) => {
            return refuse(&format!(
                "the task '{}' is locked by another run ({e}).",
                ctx.task
            ))
        }
    };

    // Recovery records: read and judge every `.consult.pending*.json` of the task under the
    // lock, BEFORE anything is written (`codex-consult.ps1` ~2766). A corrupt record refuses;
    // a live process of an interrupted run refuses (its message names the pid); a dead record
    // is consumed — numbering already skipped past it (`next_numbers`), a recovered/cleared
    // line is printed and carried into the handoff, and a consumed panel-member record is
    // removed (the single-run record is overwritten by this run's reservation below).
    {
        let assessed = super::recovery::assess(&store, &ctx.task);
        if let Some(err) = assessed.error {
            return refuse(&err);
        }
        if let Some(msg) = assessed.active_message() {
            return refuse(&msg);
        }
        let own = pending.file_name();
        for item in &assessed.items {
            let line = super::recovery::run_line(item);
            println!("{TOOL}: {line}");
            ctx.recovery_lines.push(line);
            let is_own = item
                .path
                .file_name()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case(&own))
                .unwrap_or(false);
            if !is_own {
                if let Err(e) = std::fs::remove_file(&item.path) {
                    println!(
                        "{TOOL}: could not remove the consumed recovery record {} ({e})",
                        item.path.display()
                    );
                }
            }
        }
    }

    // Reserve the recovery record (`New-PendingRecord -State reserved`): this bridge's pid and
    // start time (the writer-pid liveness rule), the host and this run's numbers/reply.
    let reply_rel = format!("handoffs/{:02}-codex-{}.md", ctx.nn, ctx.reply_name);
    let mut rec = new_pending_record(PendingState::Reserved, &ctx, &reply_rel);
    let _ = store.write_pending(&pending, &rec);

    // The plugin fingerprints the tree AFTER the lock and recovery record exist, so the
    // `.consult.*` files under the collab dir are counted among the excluded entries
    // (`fingerprint_note`). Re-fingerprint here to match (`$revBefore` in the plugin flow).
    ctx.revision = revision::revision_info(&ctx.repo_root, Some(&ctx.collab_root));

    // Peak status AT LAUNCH (the one the ledger records; the early check was call 0, this is
    // call 1). Preparation between the two may cross a window boundary: under `-OffPeakOnly` a
    // window entered since the early check stops the run here — the reservation is withdrawn
    // (nothing written under its numbers), no ledger entry.
    let launch_peak = c3_core::peak::peak_status_now(&ctx.peak_provider, 1);
    if !launch_peak.error.is_empty() {
        let _ = std::fs::remove_file(store_pending_path(&store, &pending));
        return refuse(&launch_peak.error);
    }
    if ctx.o.off_peak_only && launch_peak.peak != Some(false) {
        let _ = std::fs::remove_file(store_pending_path(&store, &pending));
        return refuse(&format!(
            "-OffPeakOnly: {} entered its peak window before launch ({}; now {}); nothing was started.",
            ctx.peak_provider, launch_peak.schedule, launch_peak.local
        ));
    }
    let (launch_warning, _) = peak_display(&ctx.peak_provider, &launch_peak);
    ctx.peak = launch_peak.peak;
    ctx.peak_schedule = launch_peak.schedule.clone();
    ctx.peak_source = launch_peak.source.clone();
    ctx.peak_evaluated_at = launch_peak.evaluated_at.clone();
    ctx.peak_warning = launch_warning;
    if !ctx.peak_warning.is_empty() {
        println!("WARNING: {}", ctx.peak_warning);
    }

    // (3) launching — from here on a crash may leave a codex process whose pid is not yet
    // recorded; the next run then scans the tree (`codex-consult.ps1:3299`).
    rec.state = PendingState::Launching;
    rec.note = "codex is being started; its pid is not recorded yet".into();
    let _ = store.write_pending(&pending, &rec);

    // (4) running — the callback flips the record to `running` right after the child spawns,
    // recording its pid and start time so the next run detects an interrupted-run process.
    let events_rel_for_record = c3_core::paths::repo_relative(&ctx.repo_root, &ctx.events_path)
        .unwrap_or_else(|| ctx.events_path.to_string_lossy().to_string());
    let rec_arc = std::sync::Arc::new(std::sync::Mutex::new(rec));
    let on_running: std::sync::Arc<dyn Fn(u32, String) + Send + Sync> = {
        let cb_store = store.clone();
        let cb_pending = pending.clone();
        let cb_rec = std::sync::Arc::clone(&rec_arc);
        let cb_events = events_rel_for_record.clone();
        std::sync::Arc::new(move |child_pid: u32, child_start: String| {
            if let Ok(mut r) = cb_rec.lock() {
                r.state = PendingState::Running;
                r.child_pid = Some(child_pid);
                r.child_start_time = child_start;
                r.events = cb_events.clone();
                r.note = String::new();
                let _ = cb_store.write_pending(&cb_pending, &r);
            }
        })
    };

    // Run the primary turn.
    let engine_files = TurnFiles {
        events: ctx.events_path.clone(),
        stderr: ctx.stderr_path.clone(),
    };
    let codex = CodexEngine {
        launcher: ctx.launcher.clone(),
        cwd: ctx.repo_root.clone(),
        primary: engine_files,
        secondary: TurnFiles::default(),
        on_running: Some(on_running),
    };
    let request = make_live_request(&ctx);
    let turn = TurnRequest {
        request,
        consultation: ConsultationId(ctx.consult_id.clone()),
        attempt: c3_core::engine::AttemptId(ctx.consult_id.clone()),
        kind: TurnKind::Primary,
        continuation: None,
    };

    use c3_core::engine::Engine;
    let outcome = match codex.run(&turn) {
        Ok(o) => o,
        Err(e) => {
            let _ = std::fs::remove_file(store_pending_path(&store, &pending));
            return refuse(&format!("the codex run could not be planned: {e}"));
        }
    };

    let base_record = rec_arc.lock().map(|r| r.clone()).unwrap_or_default();
    finish(ctx, store, pending, outcome, base_record)
}

/// A fresh recovery record for this run (`New-PendingRecord`): this bridge's pid/start/host
/// (the writer-pid liveness rule) plus the run's numbers, reply and launcher.
fn new_pending_record(state: PendingState, ctx: &Context, reply_rel: &str) -> PendingRecord {
    let pid = std::process::id();
    PendingRecord {
        state,
        n: ctx.consult_n,
        nn: format!("{:02}", ctx.nn),
        reply: reply_rel.to_string(),
        consult_id: ctx.consult_id.clone(),
        started: iso_now(),
        pid,
        start_time: crate::liveness::proc::process_start_iso(pid).unwrap_or_default(),
        host: pending_host(),
        launcher: ctx.launcher.clone(),
        engine: "codex".into(),
        ..Default::default()
    }
}

fn pending_host() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default()
}

/// Survivor entries `{pid, start_time, name}` for the pids a timeout kill left alive
/// (`New-SurvivorEntries`); a pid already gone is left out so a reused pid is never mistaken
/// for the survivor later.
fn survivor_entries(pids: &[u32]) -> Vec<serde_json::Value> {
    pids.iter()
        .filter_map(|&pid| {
            crate::liveness::proc::process_start_iso(pid)
                .map(|start| serde_json::json!({ "pid": pid, "start_time": start, "name": "" }))
        })
        .collect()
}

fn make_live_request(ctx: &Context) -> Request {
    let o = &ctx.o;
    let sandbox = if o.sandbox.is_empty() {
        "read-only".to_string()
    } else {
        o.sandbox.clone()
    };
    let schema_arg = if ctx.transport.transport == "output-schema" {
        ctx.schema_path.clone()
    } else {
        None
    };
    Request {
        prompt: ctx.prompt_text.clone(),
        brief_path: ctx.brief_path.clone(),
        model: if ctx.identity.model_source == "unknown" {
            String::new()
        } else {
            ctx.identity.model.clone()
        },
        provider: if ctx.identity.provider_source.is_empty() {
            String::new()
        } else {
            ctx.identity.provider.clone()
        },
        engine: EngineKind::Codex,
        effort: ctx.effort.sent.clone(),
        timeout_sec: ctx.r.timeout_sec as f64,
        mode: resolved_mode(&ctx.effective_mode, &ctx.parent_thread, &ctx.identity),
        sandbox,
        schema_path: schema_arg,
        extra_config: ctx.r.extra_config.clone(),
        output_last_message: Some(ctx.last_msg_path.clone()),
        prompt_file: None,
        max_model_steps: None,
    }
}

fn store_pending_path(store: &FilesStore, pending: &PendingRef) -> PathBuf {
    store.task_dir(&pending.task).join(pending.file_name())
}

/// Ingest the outcome, render, commit and print the summary.
fn finish(
    ctx: Context,
    store: FilesStore,
    pending: PendingRef,
    outcome: AttemptOutcome,
    base_record: PendingRecord,
) -> i32 {
    let mut bridge_outcome;
    let mut structured: Option<StructuredReply> = None;
    let mut raw_text = String::new();
    let mut usage: Option<Usage> = None;
    let mut wall_seconds = 0.0_f64;
    let mut thread = String::new();
    let mut thread_source = "unknown".to_string();
    let mut validation_error = String::new();
    let mut provider_failure = None;
    let mut usable = false;
    let mut main_timed_out = false;
    // Pids that survived a timeout kill: their recovery record is kept (`survivors`) so the
    // next run for this task is refused until they exit.
    let mut timeout_survivors: Vec<u32> = Vec::new();

    match outcome {
        AttemptOutcome::Completed(reply) => {
            raw_text = reply.raw_text.clone();
            usage = reply.usage.clone();
            wall_seconds = reply.wall_seconds;
            if let c3_core::engine::ConversationTrust::Verified(c) = &reply.conversation {
                thread = c.0.clone();
                thread_source = "events".into();
            }
            bridge_outcome = "usable reply".to_string();
            usable = true;
        }
        AttemptOutcome::TimedOut {
            survivors,
            wall_seconds: w,
            conversation,
            ..
        } => {
            wall_seconds = w;
            main_timed_out = true;
            // A killed main turn still emitted `thread.started`, so its events file carries the
            // thread; take it as the resume target (source `events`, like the plugin).
            if let c3_core::engine::ConversationTrust::Verified(c)
            | c3_core::engine::ConversationTrust::Candidate(c) = &conversation
            {
                if !c.0.is_empty() {
                    thread = c.0.clone();
                    thread_source = "events".into();
                }
            }
            bridge_outcome = if survivors.is_empty() {
                format!(
                    "failed: timeout after {} s (process tree killed)",
                    ctx.r.timeout_sec
                )
            } else {
                format!(
                    "failed: timeout after {} s (process tree killed; {} processes survived: pid {}; the next run for this task is refused until they exit)",
                    ctx.r.timeout_sec,
                    survivors.len(),
                    survivors
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            timeout_survivors = survivors;
        }
        AttemptOutcome::ProviderFailure { failure: pf, .. } => {
            bridge_outcome = format!("failed: {} - {}", pf.class, pf.message);
            provider_failure = Some(pf);
        }
        AttemptOutcome::LaunchFailed { message, .. } => {
            bridge_outcome = format!("failed: could not start codex - {message}");
        }
        AttemptOutcome::Cancelled => {
            bridge_outcome = "failed: cancelled".to_string();
        }
    }

    // Live drift (`Compare-TreeContent` + brief re-hash). The turn ran in `run_live` before
    // this; re-fingerprint the tree and re-hash the brief now. A file's CONTENT changing is a
    // tree change (warning); HEAD moving with identical content is only a `revision_moved`
    // note. The brief changing during the review is a warning too.
    let rev_after = revision::revision_info(&ctx.repo_root, Some(&ctx.collab_root));
    let tree_cmp = revision::compare_tree_content(&ctx.revision, &rev_after);
    let brief_sha_after = ctx
        .brief_path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| c3_core::sha256_hex(&b))
        .unwrap_or_default();
    let drift = Drift {
        tree_sha256_after: rev_after.tree_sha256.clone(),
        tree_changed: tree_cmp.changed,
        revision_moved: tree_cmp.revision_moved.clone(),
        brief_sha_after: brief_sha_after.clone(),
        brief_changed: !ctx.brief_sha.is_empty() && ctx.brief_sha != brief_sha_after,
    };

    // ------------------------------------------------------------- secondary turns
    // The two codex mechanisms (`codex-consult.ps1` waves 24/24b/24c): the timeout continuation
    // (one `resume <thread>` turn after the main turn was killed) and the format repair (one
    // `resume <thread>` turn that converts a prose reply into the object). Both run under the
    // same task lock and recovery record, at most once.
    let mut sec = Secondary::default();
    let main_events_text = std::fs::read_to_string(&ctx.events_path).unwrap_or_default();
    let main_stderr = std::fs::read_to_string(&ctx.stderr_path).unwrap_or_default();
    let main_event_error = crate::engines::codex::parse_error(&main_events_text);

    if main_timed_out {
        run_timeout_continuation(
            &ctx,
            &drift,
            &timeout_survivors,
            &main_stderr,
            &main_event_error,
            wall_seconds,
            &mut sec,
            &mut bridge_outcome,
            &mut raw_text,
            &mut thread,
            &mut thread_source,
            &mut usable,
            &mut provider_failure,
        );
    }

    // Structured / prose classification (`ConvertFrom-StructuredReply`), on a usable, non-raw
    // reply — the main reply, or a continuation that answered.
    if !ctx.r.raw && usable {
        match crate::engines::codex::parse_structured(&raw_text) {
            Some(s) => structured = Some(s),
            None => validation_error = ingest::first_validation_error(&raw_text),
        }
    }

    // The handoff's `Structured reply: INVALID (...)` line names the FIRST reply's parse error
    // (`$parse.ValidationError`), never the `(format repair …)` suffix the ledger's
    // `validation_error` carries — capture it before the repair turn augments the ledger value.
    let base_validation_error = validation_error.clone();

    // Format repair: a substantive prose reply on a verified thread earns ONE convert-only turn.
    if structured.is_none() {
        run_format_repair(
            &ctx,
            &store,
            &pending,
            &base_record,
            &thread,
            &thread_source,
            usable,
            &mut sec,
            &mut structured,
            &mut raw_text,
            &mut validation_error,
        );
    }

    // A failed codex run records a classified provider_failure derived from its evidence
    // (`codex-consult.ps1:4093`), unless a continuation already supplied one.
    if !usable && provider_failure.is_none() {
        provider_failure = Some(codex_failure_pf(
            &bridge_outcome,
            &main_event_error,
            &main_stderr,
        ));
    }

    // (`codex-consult.ps1:4106`) a usable reply produced by the continuation says so everywhere.
    if sec.continued && usable {
        bridge_outcome = "usable reply (after a timeout continuation)".to_string();
    }

    // Salvaged partial reply: a turn the bridge killed on its timeout with no usable
    // continuation leaves `handoffs/NN-codex-<slug>.partial.md` (`codex-consult.ps1:4108`).
    build_partial_reply(
        &ctx,
        &main_events_text,
        &mut sec,
        main_timed_out,
        wall_seconds,
    );

    // Build the finding delta + ids for a structured reply.
    let mut finding_ids: Vec<String> = Vec::new();
    let mut delta = FindingsDelta::default();
    let mut counts = FindingCounts::default();
    if let Some(s) = &structured {
        counts = render::severity_counts(s);
        let handoff_rel = format!("handoffs/{:02}-codex-{}.md", ctx.nn, ctx.reply_name);
        for (k, rf) in s.findings.iter().enumerate() {
            let id = c3_core::findings::finding_id(ctx.nn, k + 1);
            finding_ids.push(id.clone());
            delta
                .new
                .push(build_finding(&ctx, &id, rf, &handoff_rel, &thread));
        }
    }

    // Render the handoff markdown (and the salvaged partial file, when a turn was killed).
    let (handoff_md, partial_md) = render_handoff(
        &ctx,
        &bridge_outcome,
        wall_seconds,
        &usage,
        &thread,
        &thread_source,
        structured.as_ref(),
        &finding_ids,
        &base_validation_error,
        provider_failure.as_ref(),
        &raw_text,
        &drift,
        &sec,
        &main_event_error,
        &main_stderr,
    );
    let handoff_rel = format!("handoffs/{:02}-codex-{}.md", ctx.nn, ctx.reply_name);
    let events_rel = format!(
        "handoffs/{:02}-codex-{}.events.jsonl",
        ctx.nn, ctx.reply_name
    );
    // The plugin writes `.reply.json` (the byte-for-byte raw reply) at the handoff path
    // BEFORE the write lock is taken (README "Write order": `.reply.json` first), so a crash
    // during the commit still leaves the raw reply recoverable. C3 writes it here, before the
    // lock, at the handoff path — not staged under `.consult.reply.json`.
    let reply_json_rel = if !ctx.r.raw && !raw_text.is_empty() {
        format!("handoffs/{:02}-codex-{}.reply.json", ctx.nn, ctx.reply_name)
    } else {
        String::new()
    };
    if !reply_json_rel.is_empty() {
        let _ = c3_core::store::write_text_atomic(&ctx.reply_json_path, raw_text.as_bytes());
    }

    // Build the ledger entry.
    let entry = build_entry(
        &ctx,
        &bridge_outcome,
        wall_seconds,
        &usage,
        &thread,
        &thread_source,
        structured.as_ref(),
        &finding_ids,
        &counts,
        &validation_error,
        provider_failure.clone(),
        &handoff_rel,
        &reply_json_rel,
        &events_rel,
        &drift,
        &sec,
    );

    // Keep a copy for telemetry (the entry is moved into the commit below).
    let entry_for_telemetry = if ctx.telemetry_enabled {
        Some(entry.clone())
    } else {
        None
    };

    // Commit under the write lock: the handoff `.md` (and the salvaged `.partial.md`). The raw
    // `.reply.json` was already written above, before the lock (crash-safety), so it is not in
    // the commit `files` list.
    let mut files = vec![(handoff_rel.clone(), handoff_md.clone().into_bytes())];
    if let Some(pm) = &partial_md {
        files.push((sec.partial_rel.clone(), pm.clone().into_bytes()));
    }
    // Timeout survivors: keep the recovery record in the `survivors` state (a live process of
    // this run is still out there) so the commit does not delete it and the next run is
    // refused until they exit. Written before the write lock, as the plugin does after the
    // kill (`codex-consult.ps1:3375`).
    let disposition = if timeout_survivors.is_empty() {
        RecoveryDisposition::Remove
    } else {
        let mut survivor_rec = base_record.clone();
        survivor_rec.state = PendingState::Survivors;
        survivor_rec.survivors = survivor_entries(&timeout_survivors);
        let _ = store.write_pending(&pending, &survivor_rec);
        RecoveryDisposition::Retain
    };
    let write_lock = match store.take_write_lock(&ctx.task) {
        Ok(l) => l,
        Err(e) => return refuse(&format!("could not take the write lock: {e}")),
    };
    let commit = CommitRequest {
        entry,
        findings: delta,
        pending: &pending,
        disposition,
        files: &files,
        bootstrap_cwd: ctx.repo_root.to_string_lossy().to_string(),
        bootstrap_tool: ctx.codex_version.clone(),
    };
    let receipt = match store.commit(&write_lock, commit) {
        Ok(r) => r,
        Err(e) => return refuse(&format!("the commit failed: {e}")),
    };
    drop(write_lock);
    let _ = receipt;

    // Telemetry: record this consultation to the spool (errors ignored). The env switch is
    // re-checked inside `record_consultation`; a dry run never reaches this point.
    if let Some(entry) = &entry_for_telemetry {
        let _ = telemetry::record_consultation(entry, None);
    }

    // Summary.
    let reply_body = structured
        .as_ref()
        .map(|s| {
            if s.reply_markdown.trim().is_empty() {
                "_(empty reply_markdown)_".to_string()
            } else {
                s.reply_markdown.clone()
            }
        })
        .unwrap_or_else(|| raw_text.trim().to_string());
    // The `structured : INVALID (...)` summary line (a prose reply kept as the reply of record);
    // it uses the FULL ledger validation_error (with the repair suffix), unlike the handoff.
    let structured_invalid =
        if !ctx.r.raw && usable && structured.is_none() && !raw_text.trim().is_empty() {
            format!(
                "structured : INVALID ({validation_error}) - raw text kept; no findings recorded"
            )
        } else {
            String::new()
        };
    let section = structured
        .as_ref()
        .map(|s| render::format_structured_section(s, &finding_ids))
        .unwrap_or_default();

    let verdict_line = structured.as_ref().map(|s| {
        let v = match s.verdict {
            c3_core::engine::Verdict::Accept => "ACCEPT",
            c3_core::engine::Verdict::Hold => "HOLD",
            c3_core::engine::Verdict::Reject => "REJECT",
            c3_core::engine::Verdict::Advise => "ADVISE",
        };
        format!(
            "verdict    : {} - {}",
            v,
            c3_core::one_line(&s.verdict_reason)
        )
    });
    let findings_line = structured.as_ref().map(|_| {
        if finding_ids.is_empty() {
            "findings   : none".to_string()
        } else {
            format!(
                "findings   : {} -> {} in findings.json",
                render::format_severity_counts(&counts),
                render::format_id_range(&finding_ids)
            )
        }
    });

    // The `continued  :` console line (`codex-consult.ps1:4559`).
    let continue_line = sec.timeout_continue.as_ref().map(|tc| {
        if sec.continued {
            format!(
                "continued  : the main turn was killed at {} s of {} s; one continuation turn on thread {} answered in {} s",
                fmt_wall(wall_seconds),
                ctx.r.timeout_sec,
                sec.continue_thread,
                fmt_wall(sec.continue_wall),
            )
        } else {
            let mut l = format!("continued  : {}", tc.outcome);
            if sec.continue_wall > 0.0 || tc.events.is_some() {
                l.push_str(&format!(" (in {} s)", fmt_wall(sec.continue_wall)));
            }
            if !sec.continue_thread.is_empty() {
                l.push_str(&format!(" - thread {}", sec.continue_thread));
            }
            l
        }
    });
    let partial_abs = if sec.partial_needed {
        ctx.handoffs_dir
            .join(format!("{:02}-codex-{}.partial.md", ctx.nn, ctx.reply_name))
            .to_string_lossy()
            .to_string()
    } else {
        String::new()
    };
    let s = summary::SummaryInputs {
        bridge_outcome: bridge_outcome.clone(),
        usable,
        wall_seconds: fmt_wall(wall_seconds),
        lineage_shown: ctx.identity.lineage.clone(),
        mode: ctx.effective_mode.clone(),
        thread: thread.clone(),
        thread_source: thread_source.clone(),
        continue_line: continue_line.unwrap_or_default(),
        repair_console: sec.repair_console.clone(),
        repair_drift: sec.drift_notes.clone(),
        partial_path: partial_abs,
        partial_footer: sec.partial_footer.clone(),
        resume_command: sec.resume_command.clone(),
        verdict_line: verdict_line.unwrap_or_default(),
        findings_line: findings_line.unwrap_or_default(),
        structured_invalid,
        reply_path: ctx.reply_path.to_string_lossy().to_string(),
        reply_json_path: if reply_json_rel.is_empty() {
            String::new()
        } else {
            ctx.reply_json_path.to_string_lossy().to_string()
        },
        events_path: ctx.events_path.to_string_lossy().to_string(),
        reply_body,
        section,
        ..Default::default()
    };
    // Run warnings print before the summary block (`foreach ($rw in $runWarnings)`).
    for w in &ctx.run_warnings {
        println!("WARNING: {w}");
    }
    for line in summary::render_summary(&s) {
        println!("{line}");
    }
    // temp file cleanup
    let _ = std::fs::remove_file(&ctx.last_msg_path);
    let _ = std::fs::remove_file(&ctx.stderr_path);

    if usable {
        0
    } else {
        classify_exit(&bridge_outcome, provider_failure.as_ref())
    }
}

fn round1(s: f64) -> f64 {
    (s * 10.0).round() / 10.0
}

/// Give a recorded provider failure the plugin's full shape (`New-ProviderFailure` always
/// writes `kind` and `hint`, and stamps `when`).
fn finalize_pf(mut pf: c3_core::ledger::ProviderFailure) -> c3_core::ledger::ProviderFailure {
    if pf.kind.is_none() {
        pf.kind = Some(String::new());
    }
    if pf.hint.is_none() {
        pf.hint = Some(String::new());
    }
    if pf.when.is_empty() {
        pf.when = iso_now();
    }
    pf
}

/// Build a codex run's `provider_failure` from its evidence (`codex-consult.ps1:4093`): an SSE
/// `data:{...}` line on stderr, the event-stream error, the stderr tail, then the bridge's own
/// reason — the first non-empty through the one classifier.
fn codex_failure_pf(
    bridge_outcome: &str,
    event_error: &str,
    stderr_text: &str,
) -> c3_core::ledger::ProviderFailure {
    let sse_last = stderr_text
        .split(['\r', '\n'])
        .map(|l| l.trim())
        .rfind(|l| l.starts_with("data:") && l.contains('{'))
        .unwrap_or("");
    let stderr_tail = stderr_text
        .split(['\r', '\n'])
        .map(|l| l.trim())
        .rfind(|l| !l.is_empty())
        .unwrap_or("");
    let reason = bridge_outcome
        .strip_prefix("failed: ")
        .unwrap_or(bridge_outcome);
    let source = [sse_last, event_error, stderr_tail, reason]
        .into_iter()
        .find(|t| !t.is_empty())
        .unwrap_or("");
    let (code, message) = c3_core::health::convert_from_provider_error_text(source);
    let class = c3_core::health::provider_failure_class(&format!("{code} {message}"));
    let kind = c3_core::health::failure_kind(&class, &format!("{code} {message}"));
    finalize_pf(c3_core::ledger::ProviderFailure {
        class,
        kind: Some(kind),
        code,
        message,
        ..Default::default()
    })
}

/// The reply schema inlined into a prompt (the plugin's
/// `[IO.File]::ReadAllText(...).Trim() -replace "\r\n","\n" -replace "\n",$nl` with `$nl` = CRLF).
fn schema_text_crlf() -> String {
    c3_core::schema::REPLY_SCHEMA_V1
        .trim()
        .replace("\r\n", "\n")
        .replace('\n', "\r\n")
}

/// Run one codex secondary turn (`resume <thread>`) and return its outcome and measured wall.
#[allow(clippy::too_many_arguments)]
fn run_codex_secondary(
    ctx: &Context,
    kind: TurnKind,
    sandbox: &str,
    effort: Option<String>,
    schema_arg: Option<PathBuf>,
    thread: &str,
    prompt_text: &str,
    last_path: &Path,
    events_path: &Path,
    stderr_path: &Path,
    timeout_sec: f64,
    on_running: Option<std::sync::Arc<dyn Fn(u32, String) + Send + Sync>>,
) -> (AttemptOutcome, f64) {
    let mut request = Request {
        prompt: prompt_text.to_string(),
        brief_path: None,
        model: if ctx.identity.model_source == "unknown" {
            String::new()
        } else {
            ctx.identity.model.clone()
        },
        provider: if ctx.identity.provider_source.is_empty() {
            String::new()
        } else {
            ctx.identity.provider.clone()
        },
        engine: EngineKind::Codex,
        effort,
        timeout_sec,
        mode: Mode::New,
        sandbox: sandbox.to_string(),
        schema_path: schema_arg,
        extra_config: ctx.r.extra_config.clone(),
        output_last_message: Some(last_path.to_path_buf()),
        prompt_file: None,
        max_model_steps: None,
    };
    // The resume must carry the request's own lineage (the core refuses a cross-lineage resume).
    let lineage = request.lineage();
    request.mode = Mode::Resume {
        thread: thread.to_string(),
        lineage,
    };
    let codex = CodexEngine {
        launcher: ctx.launcher.clone(),
        cwd: ctx.repo_root.clone(),
        primary: TurnFiles::default(),
        secondary: TurnFiles {
            events: events_path.to_path_buf(),
            stderr: stderr_path.to_path_buf(),
        },
        on_running,
    };
    let turn = TurnRequest {
        request,
        consultation: ConsultationId(ctx.consult_id.clone()),
        attempt: c3_core::engine::AttemptId(ctx.consult_id.clone()),
        kind,
        continuation: Some(c3_core::engine::Continuation::Native(
            c3_core::engine::ConversationId(thread.to_string()),
        )),
    };
    let start = std::time::Instant::now();
    let outcome = codex
        .run(&turn)
        .unwrap_or_else(|e| AttemptOutcome::LaunchFailed {
            child_exists: false,
            message: format!("the secondary turn could not be planned ({e:?})"),
        });
    (outcome, round1(start.elapsed().as_secs_f64()))
}

/// The timeout continuation (`codex-consult.ps1:3626`): after a killed main turn, ONE
/// `resume <thread>` turn — the main turn's options — asks the reviewer to finish now. Gated by
/// the thread, survivors, files-changed and the killed turn's own failure evidence.
#[allow(clippy::too_many_arguments)]
fn run_timeout_continuation(
    ctx: &Context,
    drift: &Drift,
    survivors: &[u32],
    main_stderr: &str,
    main_event_error: &str,
    main_wall: f64,
    sec: &mut Secondary,
    bridge_outcome: &mut String,
    raw_text: &mut String,
    thread: &mut String,
    thread_source: &mut String,
    usable: &mut bool,
    provider_failure: &mut Option<c3_core::ledger::ProviderFailure>,
) {
    let continue_thread = thread.clone();
    sec.continue_thread = continue_thread.clone();
    let continue_events_name = format!(
        "{:02}-codex-{}.continue.events.jsonl",
        ctx.nn, ctx.reply_name
    );
    let continue_events_rel = format!("handoffs/{continue_events_name}");
    let continue_events_path = ctx.handoffs_dir.join(&continue_events_name);

    // Gate (the plugin's `$continueSkip`).
    let mut skip = String::new();
    if ctx.r.continue_sec <= 0 {
        skip = "-ContinueSec 0".to_string();
    } else if continue_thread.is_empty() {
        skip = "the thread of the killed turn is not known".to_string();
    } else if !survivors.is_empty() {
        skip = format!("{} process(es) survived the kill", survivors.len());
    } else {
        let mut moved: Vec<&str> = Vec::new();
        if drift.tree_changed {
            moved.push("the working tree");
        }
        if drift.brief_changed {
            moved.push("the brief");
        }
        if !moved.is_empty() {
            skip = format!("files changed during the run ({})", moved.join(", "));
        }
    }
    if skip.is_empty() {
        if let Some((class, text)) =
            super::secondary::get_killed_turn_failure(main_event_error, main_stderr)
        {
            skip = format!("the killed turn reported a {class} failure ({text})");
        }
    }
    if !skip.is_empty() {
        sec.timeout_continue = Some(c3_core::ledger::TimeoutContinue {
            thread: continue_thread,
            wall_seconds: 0.0,
            outcome: format!("not attempted: {skip}"),
            events: None,
            usage: None,
            ..Default::default()
        });
        return;
    }

    // The continuation prompt (the main turn's contract; prompt-only re-sends the schema).
    let mut parts: Vec<String> = Vec::new();
    if !ctx.r.raw {
        parts.push(prompt::FINAL_OUTPUT_CONTRACT.to_string());
    }
    parts.push(format!(
        "Your previous turn was stopped by a time limit after {} s. Do not start over and do not read more files than you must: finish now and output your final answer in the required format.",
        ctx.r.timeout_sec
    ));
    if !ctx.r.raw && ctx.transport.transport == "prompt-only" {
        parts.push(prompt::schema_lines(
            &ctx.o.purpose,
            ctx.open_findings_count > 0,
            true,
        ));
        parts.push(format!(
            "JSON Schema of the reply:\r\n{}",
            schema_text_crlf()
        ));
    }
    parts.push(format!("Consultation id: {}", ctx.consult_id));
    let continue_prompt = parts.join("\r\n\r\n");

    let tmp = std::env::temp_dir();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let continue_last = tmp.join(format!("codex-consult-continue-last-{id}.md"));
    let continue_stderr = tmp.join(format!("codex-consult-continue-stderr-{id}.txt"));
    let schema_arg = if !ctx.r.raw && ctx.transport.transport == "output-schema" {
        ctx.schema_path.clone()
    } else {
        None
    };

    println!(
        "{TOOL}: the main turn was killed at {} s of {} s; one continuation turn on thread {} (up to {} s)",
        fmt_wall(main_wall),
        ctx.r.timeout_sec,
        continue_thread,
        ctx.r.continue_sec
    );
    sec.continue_ran = true;
    // The continuation event stream is a further turn (added before the run, like the plugin).
    sec.continue_events_rel = Some(continue_events_rel.clone());

    let (outcome, wall) = run_codex_secondary(
        ctx,
        TurnKind::TimeoutContinuation,
        &sandbox_label(&ctx.o),
        ctx.effort.sent.clone(),
        schema_arg,
        &continue_thread,
        &continue_prompt,
        &continue_last,
        &continue_events_path,
        &continue_stderr,
        ctx.r.continue_sec as f64,
        None,
    );
    sec.continue_wall = wall;
    let events_field = if continue_events_path.is_file() {
        Some(continue_events_rel.clone())
    } else {
        None
    };

    let mut continue_problem = String::new();
    let mut cont_usage: Option<Usage> = None;
    match outcome {
        AttemptOutcome::Completed(reply) => {
            cont_usage = reply.usage.clone();
            let cont_raw = reply.raw_text.trim().to_string();
            let cont_thread = match &reply.conversation {
                c3_core::engine::ConversationTrust::Verified(c)
                | c3_core::engine::ConversationTrust::Candidate(c) => c.0.clone(),
                _ => String::new(),
            };
            if !cont_thread.is_empty() && cont_thread != continue_thread {
                continue_problem = format!(
                    "the continuation came back on thread {cont_thread}, not {continue_thread}"
                );
            } else if cont_raw.is_empty() {
                continue_problem = "empty reply".to_string();
            } else {
                let (ok, reason) = super::secondary::test_continuation_reply(&cont_raw, ctx.r.raw);
                if !ok {
                    continue_problem = format!("not a usable reply - {reason}");
                    sec.continue_rejected = true;
                    sec.continue_rejected_text = cont_raw.clone();
                    sec.continue_rejected_why = reason;
                }
            }
            if continue_problem.is_empty() {
                sec.continued = true;
                *bridge_outcome = "usable reply".to_string();
                *usable = true;
                *raw_text = reply.raw_text.clone();
                *thread = continue_thread.clone();
                *thread_source = "events".to_string();
            }
        }
        AttemptOutcome::TimedOut { .. } => {
            continue_problem = format!(
                "timeout after {} s (process tree killed)",
                ctx.r.continue_sec
            );
            sec.continue_killed = true;
        }
        AttemptOutcome::ProviderFailure {
            failure: pf,
            exit_code,
        } => {
            // The plugin frames a non-clean continuation exit as `codex exit N - <err>` (a bare
            // `codex exit N` when no error text), surfacing the raw process exit code.
            let n = exit_code.unwrap_or(-1);
            continue_problem = if pf.message.trim().is_empty() {
                format!("codex exit {n}")
            } else {
                format!("codex exit {n} - {}", pf.message)
            };
            *provider_failure = Some(finalize_pf(pf));
        }
        AttemptOutcome::LaunchFailed { message, .. } => {
            continue_problem = format!("could not start codex - {message}");
        }
        AttemptOutcome::Cancelled => {
            continue_problem = "cancelled".to_string();
        }
    }

    sec.continue_usage = cont_usage.clone();
    sec.timeout_continue = Some(c3_core::ledger::TimeoutContinue {
        thread: continue_thread,
        wall_seconds: wall,
        outcome: if sec.continued {
            "usable reply".to_string()
        } else {
            format!("failed: {continue_problem}")
        },
        events: events_field,
        usage: cont_usage,
        ..Default::default()
    });
}

/// The format repair (`codex-consult.ps1:3845`): a substantive prose reply on a verified thread
/// earns ONE convert-only `resume <thread>` turn at the lowest effort, no `--output-schema`.
#[allow(clippy::too_many_arguments)]
fn run_format_repair(
    ctx: &Context,
    store: &FilesStore,
    pending: &PendingRef,
    base_record: &PendingRecord,
    thread: &str,
    thread_source: &str,
    usable: bool,
    sec: &mut Secondary,
    structured: &mut Option<StructuredReply>,
    raw_text: &mut String,
    validation_error: &mut String,
) {
    let eligible = ctx.r.repair_enabled
        && structured.is_none()
        && usable
        && !ctx.r.raw
        && !thread.is_empty()
        && thread_source == "events";
    if !eligible {
        return;
    }
    let gate = super::secondary::prose_gate(raw_text);
    if !gate.substantive {
        // No repair turn: the validation_error names why (`(format repair not attempted: ...)`).
        *validation_error = format!(
            "{} (format repair not attempted: {})",
            validation_error, gate.reason
        );
        return;
    }

    let original_prose = raw_text.clone();
    let mut repair_reason = validation_error.clone();
    if repair_reason.chars().count() > 200 {
        repair_reason = repair_reason.chars().take(200).collect();
    }
    sec.repair_reason = repair_reason;

    // The original prose, byte for byte, kept next to the handoff BEFORE the repair process.
    let original_name = format!("{:02}-codex-{}.original.md", ctx.nn, ctx.reply_name);
    let original_full = ctx.handoffs_dir.join(&original_name);
    let _ = c3_core::store::write_text_atomic(&original_full, original_prose.as_bytes());
    sec.original_rel = format!("handoffs/{original_name}");
    sec.original_prose = original_prose;

    // The recovery record names the saved prose BEFORE the repair process exists (state
    // launching), then the callback flips it to running with the repair pid.
    let original_repo_rel = c3_core::paths::repo_relative(&ctx.repo_root, &original_full)
        .unwrap_or_else(|| original_full.to_string_lossy().to_string());
    let mut launching = base_record.clone();
    launching.state = PendingState::Launching;
    launching.original = Some(original_repo_rel.clone());
    launching.first_reply = Some("usable prose (format repair in progress)".to_string());
    launching.note = "format repair turn being started; its pid is not recorded yet".to_string();
    let _ = store.write_pending(pending, &launching);

    let rec_arc = std::sync::Arc::new(std::sync::Mutex::new(launching));
    let on_running: std::sync::Arc<dyn Fn(u32, String) + Send + Sync> = {
        let cb_store = store.clone();
        let cb_pending = pending.clone();
        let cb_rec = std::sync::Arc::clone(&rec_arc);
        std::sync::Arc::new(move |child_pid: u32, child_start: String| {
            if let Ok(mut r) = cb_rec.lock() {
                r.state = PendingState::Running;
                r.child_pid = Some(child_pid);
                r.child_start_time = child_start;
                r.note = "format repair turn".to_string();
                let _ = cb_store.write_pending(&cb_pending, &r);
            }
        })
    };

    let repair_timeout = ctx.r.timeout_sec.min(300);
    sec.repair_timeout = repair_timeout;
    let tmp = std::env::temp_dir();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let repair_last = tmp.join(format!("codex-consult-repair-last-{id}.md"));
    let repair_events = tmp.join(format!("codex-consult-repair-events-{id}.jsonl"));
    let repair_stderr = tmp.join(format!("codex-consult-repair-stderr-{id}.txt"));

    let repair_prompt = format!(
        "Your last message was prose, not the required JSON. Reply with exactly one bare JSON object satisfying the JSON Schema below - no fence, nothing before or after it. Convert, do not re-answer: copy your previous content unchanged (the same Q1..Qn answers verbatim inside reply_markdown, the same findings, the same Requested checks, the same prior-finding statuses and the same verdict); add or omit nothing.\r\n\r\nJSON Schema of the reply:\r\n{}\r\n\r\nConsultation id: {}",
        schema_text_crlf(),
        ctx.consult_id
    );

    let (outcome, wall) = run_codex_secondary(
        ctx,
        TurnKind::FormatRepair,
        "read-only",
        repair_effort(ctx),
        None, // never --output-schema on a codex repair turn (prompt-only)
        thread,
        &repair_prompt,
        &repair_last,
        &repair_events,
        &repair_stderr,
        repair_timeout as f64,
        Some(on_running),
    );

    let mut repair_problem = String::new();
    let mut repair_usage: Option<Usage> = None;
    let mut repair_thread = String::new();
    let mut repaired: Option<StructuredReply> = None;
    match outcome {
        AttemptOutcome::Completed(reply) => {
            repair_usage = reply.usage.clone();
            repair_thread = match &reply.conversation {
                c3_core::engine::ConversationTrust::Verified(c)
                | c3_core::engine::ConversationTrust::Candidate(c) => c.0.clone(),
                _ => String::new(),
            };
            let rr = reply.raw_text.trim();
            if rr.is_empty() {
                repair_problem = "empty reply".to_string();
            } else {
                match crate::engines::codex::parse_structured(rr) {
                    Some(s) => {
                        // .reply.json holds the repaired object, byte for byte.
                        *raw_text = reply.raw_text.clone();
                        repaired = Some(s);
                    }
                    None => {
                        repair_problem =
                            format!("still not valid: {}", ingest::first_validation_error(rr));
                    }
                }
            }
        }
        AttemptOutcome::TimedOut { .. } => {
            repair_problem = format!("timeout after {repair_timeout} s (process tree killed)");
            sec.repair_killed = true;
        }
        AttemptOutcome::ProviderFailure { exit_code, .. } => {
            // The plugin frames a non-clean repair exit as a bare `codex exit N`.
            repair_problem = format!("codex exit {}", exit_code.unwrap_or(-1));
        }
        AttemptOutcome::LaunchFailed { message, .. } => {
            repair_problem = format!("could not start codex - {message}");
        }
        AttemptOutcome::Cancelled => {
            repair_problem = "cancelled".to_string();
        }
    }

    // Drift notes: a different repair thread, then the prose-vs-object comparison.
    let mut drift_notes: Vec<String> = Vec::new();
    if !repair_thread.is_empty() && repair_thread != thread {
        drift_notes.push("repair returned a different thread id".to_string());
    }
    if let Some(s) = &repaired {
        for d in super::secondary::get_format_repair_drift(&sec.original_prose, s) {
            drift_notes.push(d);
        }
        *validation_error = String::new();
        sec.repaired_ok = true;
        *structured = repaired;
    } else {
        *validation_error = format!(
            "{} (format repair failed: {})",
            validation_error,
            c3_core::one_line(&repair_problem)
        );
    }
    sec.drift_notes = drift_notes.clone();

    sec.format_retry = Some(c3_core::ledger::FormatRetry {
        attempted: true,
        reason: sec.repair_reason.clone(),
        succeeded: sec.repaired_ok,
        thread: repair_thread,
        wall_seconds: wall,
        usage: repair_usage,
        drift: drift_notes
            .iter()
            .map(|d| serde_json::Value::String(d.clone()))
            .collect(),
        original: sec.original_rel.clone(),
        events: None, // codex's repair event stream is a temp file, removed → null
        schema_transport: "prompt-only".to_string(),
        ..Default::default()
    });
    sec.repair_wall = wall;
    sec.repair_console = format!(
        "format repair: {} in {} s; drift: {} note(s)",
        if sec.repaired_ok {
            "succeeded"
        } else {
            "failed"
        },
        fmt_wall(wall),
        drift_notes.len()
    );

    // temp cleanup
    let _ = std::fs::remove_file(&repair_last);
    let _ = std::fs::remove_file(&repair_events);
    let _ = std::fs::remove_file(&repair_stderr);
}

/// The lowest effort of the endpoint's vocabulary for a repair turn (`Get-RepairEffort`).
fn repair_effort(ctx: &Context) -> Option<String> {
    if ctx.effort.mapping == "native" {
        return ctx.effort.sent.clone();
    }
    let host = &ctx.identity.host;
    if !host.is_empty() {
        if let Some(cap) = caps(host) {
            if let Some((_, low)) = vocabulary_map(cap.vocabulary, "low") {
                return Some(low.to_string());
            }
        }
    }
    ctx.effort.sent.clone()
}

/// Build the salvaged `.partial.md` body, footer and resume command when a killed turn had no
/// usable continuation (`codex-consult.ps1:4108`).
fn build_partial_reply(
    ctx: &Context,
    main_events_text: &str,
    sec: &mut Secondary,
    main_timed_out: bool,
    main_wall: f64,
) {
    let partial_needed =
        (main_timed_out && !sec.continued) || sec.continue_killed || sec.repair_killed;
    if !partial_needed {
        return;
    }
    sec.partial_needed = true;
    sec.partial_rel = format!("handoffs/{:02}-codex-{}.partial.md", ctx.nn, ctx.reply_name);

    let mut turns: Vec<super::secondary::PartialTurn> = Vec::new();
    let mut killed_at: Vec<String> = Vec::new();
    turns.push(super::secondary::PartialTurn {
        label: "Turn 1 - the main turn".to_string(),
        note: if main_timed_out {
            format!(
                "killed at {} s of {} s",
                fmt_wall(main_wall),
                ctx.r.timeout_sec
            )
        } else {
            "it ended by itself".to_string()
        },
        salvage: super::secondary::read_codex_salvage(main_events_text),
    });
    if main_timed_out {
        killed_at.push(format!(
            "{} s of {} s (the main turn)",
            fmt_wall(main_wall),
            ctx.r.timeout_sec
        ));
    }
    if sec.continue_ran {
        let cont_events = ctx.handoffs_dir.join(format!(
            "{:02}-codex-{}.continue.events.jsonl",
            ctx.nn, ctx.reply_name
        ));
        let cont_text = std::fs::read_to_string(&cont_events).unwrap_or_default();
        let note = if sec.continue_killed {
            format!(
                "killed at {} s of {} s",
                fmt_wall(sec.continue_wall),
                ctx.r.continue_sec
            )
        } else if sec.continued {
            format!("it answered in {} s", fmt_wall(sec.continue_wall))
        } else {
            let why = sec
                .timeout_continue
                .as_ref()
                .map(|t| t.outcome.trim_start_matches("failed: ").to_string())
                .unwrap_or_default();
            format!("failed: {why}")
        };
        turns.push(super::secondary::PartialTurn {
            label: format!("Turn {} - the timeout continuation", turns.len() + 1),
            note,
            salvage: super::secondary::read_codex_salvage(&cont_text),
        });
        if sec.continue_killed {
            killed_at.push(format!(
                "{} s of {} s (the timeout continuation)",
                fmt_wall(sec.continue_wall),
                ctx.r.continue_sec
            ));
        }
    }

    let mut body = super::secondary::format_partial_body(&turns);
    // A continuation reply the checks REJECTED is not thrown away.
    if sec.continue_rejected && !sec.continue_rejected_text.trim().is_empty() {
        body = format!(
            "{}\n\n## continuation reply (rejected: {})\n\n{}\n",
            body.trim_end(),
            sec.continue_rejected_why,
            sec.continue_rejected_text.trim().replace("\r\n", "\n")
        );
        if let Some(tc) = &mut sec.timeout_continue {
            tc.outcome = format!(
                "{}; its text is kept in {} under \"continuation reply (rejected)\"",
                tc.outcome, sec.partial_rel
            );
        }
    }
    sec.partial_body = body;

    // The resume thread: this run's verified thread, else the killed turn's continuation thread.
    let resume_thread = if !sec.continue_thread.is_empty() {
        sec.continue_thread.clone()
    } else {
        String::new()
    };
    let killed_text = if killed_at.len() == 1 {
        // strip the trailing " (...)"
        let s = &killed_at[0];
        s.rsplit_once(" (")
            .map(|(a, _)| a.to_string())
            .unwrap_or_else(|| s.clone())
    } else {
        killed_at.join(", ")
    };
    if !resume_thread.is_empty() {
        let args = summary::build_resume_command(&summary::ResumeInputs {
            task: ctx.o.task.clone(),
            collab_dir: ctx.o.collab_dir.clone(),
            thread: resume_thread.clone(),
            no_roster: true,
            provider: ctx.identity.provider.clone(),
            model: ctx.identity.model.clone(),
            purpose: ctx.o.purpose.clone(),
            raw: ctx.r.raw,
            reply_name: ctx.o.reply_name.clone(),
            reply_name_given: !ctx.o.reply_name.is_empty(),
            timeout_source: ctx.r.timeout_source.clone(),
            timeout_sec: ctx.r.timeout_sec,
            continue_sec: ctx.r.continue_sec,
            effort: ctx.o.effort.clone(),
            native_effort: ctx.o.native_effort.clone(),
            max_words: ctx.o.max_words,
            transport_override: ctx.r.transport_override.clone(),
            codex_config: ctx.o.codex_config.clone(),
            artifacts: ctx.o.artifacts.clone(),
            range: ctx.o.range.clone(),
            sandbox: sandbox_label(&ctx.o),
            format_retry: ctx.o.format_retry,
            off_peak_only: ctx.o.off_peak_only,
            skip_preflight: ctx.o.skip_preflight,
            codex_exe: ctx.o.codex_exe.clone(),
        });
        sec.partial_footer =
            format!("killed at {killed_text}; thread {resume_thread} - continue with `{args}`");
        sec.resume_command = args;
    } else {
        sec.partial_footer = format!(
            "killed at {killed_text}; the thread of the killed turn is not known - no resume is possible (start again with -Mode new)"
        );
    }
}

fn classify_exit(outcome: &str, pf: Option<&c3_core::ledger::ProviderFailure>) -> i32 {
    // The plugin exits 1 for every non-usable outcome (`codex-consult.ps1` summary block);
    // the class-`transport` signal codes 130/143 (a codex process killed by SIGINT/SIGTERM)
    // are an M2c+ refinement, not the timeout-kill path (which is a plain failed run = 1).
    let _ = (outcome, pf);
    1
}

fn build_finding(
    ctx: &Context,
    id: &str,
    rf: &c3_core::engine::ReplyFinding,
    handoff_rel: &str,
    thread: &str,
) -> c3_core::findings::Finding {
    use c3_core::findings::{Evidence, Finding, FindingStatus, HistoryEvent, Location, Source};
    let mut f = Finding::default();
    f.id = id.to_string();
    f.severity = severity_token(rf.severity).to_string();
    f.claim = rf.claim.clone();
    f.trigger = rf.trigger.clone();
    f.verification = rf.verification.clone();
    f.remedy = rf.remedy.clone();
    f.supersedes = rf.supersedes.clone();
    f.locations = rf
        .locations
        .iter()
        .map(|l| Location {
            path: l.path.clone(),
            line: l.line,
            extra: Default::default(),
        })
        .collect();
    f.evidence = rf
        .evidence
        .iter()
        .map(|e| Evidence {
            kind: evidence_token(e.kind).to_string(),
            reference: e.reference.clone(),
            observation: e.observation.clone(),
            extra: Default::default(),
        })
        .collect();
    f.source = Source {
        consult: ctx.consult_n,
        reply: handoff_rel.to_string(),
        thread: thread.to_string(),
        base_commit: ctx.revision.base_commit.clone(),
        tree_sha256: ctx.revision.tree_sha256.clone(),
        ..Default::default()
    };
    // The finding is created `proposed` with one initial history event, as the plugin does.
    f.history.push(HistoryEvent {
        when: iso_now(),
        status: FindingStatus::Proposed,
        by: "codex-consult".into(),
        note: String::new(),
        evidence: String::new(),
        base_commit: ctx.revision.base_commit.clone(),
        tree_sha256: ctx.revision.tree_sha256.clone(),
        extra: Default::default(),
    });
    f
}

fn severity_token(s: c3_core::engine::Severity) -> &'static str {
    use c3_core::engine::Severity::*;
    match s {
        Blocker => "blocker",
        Major => "major",
        Minor => "minor",
        Note => "note",
    }
}

fn evidence_token(k: c3_core::engine::EvidenceKind) -> &'static str {
    use c3_core::engine::EvidenceKind::*;
    match k {
        ReadCode => "read-code",
        RanCommand => "ran-command",
        Inferred => "inferred",
        Assumed => "assumed",
    }
}

fn iso_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

pub(crate) fn fmt_wall(w: f64) -> String {
    if (w.fract()).abs() < f64::EPSILON {
        format!("{}", w as i64)
    } else {
        format!("{w}")
    }
}

#[allow(clippy::too_many_arguments)]
fn render_handoff(
    ctx: &Context,
    bridge_outcome: &str,
    wall: f64,
    usage: &Option<Usage>,
    thread: &str,
    thread_source: &str,
    structured: Option<&StructuredReply>,
    finding_ids: &[String],
    validation_error: &str,
    provider_failure: Option<&c3_core::ledger::ProviderFailure>,
    raw_text: &str,
    drift: &Drift,
    sec: &Secondary,
    main_event_error: &str,
    main_stderr: &str,
) -> (String, Option<String>) {
    let events_rel = format!(
        "handoffs/{:02}-codex-{}.events.jsonl",
        ctx.nn, ctx.reply_name
    );
    let effort_sent = ctx.effort.sent.clone().unwrap_or_else(|| "nothing".into());
    // codex reports usage; a turn that produced none (a killed main turn) is `unknown`, matching
    // `Format-Usage $null` — never "not reported by codex" (that is for agy/muse).
    let tokens = match usage {
        Some(u) => TokenReport::Reported {
            input: u.input_tokens,
            cached: u.cached_input_tokens,
            output: u.output_tokens,
            reasoning: u.reasoning_output_tokens,
        },
        None => TokenReport::Unknown,
    };
    let mut records = OptionalRecords {
        recovery_lines: ctx
            .recovery_lines
            .iter()
            .map(|l| format!("Recovery record: {l}"))
            .collect(),
        ..Default::default()
    };
    if !ctx.peak_warning.is_empty() {
        // The handoff records the past tense ('ran at') of the console warning, with the
        // same `WARNING: ` prefix the console line carries.
        records.peak_warning = Some(format!(
            "WARNING: {}",
            ctx.peak_warning
                .replace("this consultation runs at", "this consultation ran at")
        ));
    }
    // Drift lines (`$driftLines`): tree, HEAD move, brief, artifacts — in that order.
    if drift.tree_changed {
        records.drift_lines.push(
            "WARNING: working tree changed during the review (fingerprint before/after differ)."
                .to_string(),
        );
    }
    if !drift.revision_moved.is_empty() {
        records.drift_lines.push(revision::revision_moved_note(
            &drift.revision_moved,
            drift.tree_changed,
        ));
    }
    if drift.brief_changed {
        records.drift_lines.push(format!(
            "WARNING: the brief changed during the review (sha256 {} before, {} after).",
            short_hash(&ctx.brief_sha),
            short_hash(&drift.brief_sha_after)
        ));
    }
    if !ctx.run_warnings.is_empty() {
        records.warnings = Some(format!(
            "Warnings: {}.",
            ctx.run_warnings
                .iter()
                .map(|w| c3_core::one_line(w))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if let Some(pf) = provider_failure {
        let code = if pf.code.is_empty() {
            String::new()
        } else {
            format!(" ({})", pf.code)
        };
        records.provider_failure = Some(format!(
            "Provider failure: {}{} - {}.",
            pf.class, code, pf.message
        ));
    }
    // (18) Timeout continuation line.
    if let Some(tc) = &sec.timeout_continue {
        if sec.continued {
            records.timeout_continuation = Some(format!(
                "Timeout continuation: the main turn was killed at {} s of {} s; one continuation turn on thread `{}` answered in {} s. Tokens of that turn: {}.",
                fmt_wall(wall),
                ctx.r.timeout_sec,
                sec.continue_thread,
                fmt_wall(sec.continue_wall),
                usage_clause(&sec.continue_usage),
            ));
        } else {
            let mut h = format!("Timeout continuation: {}", tc.outcome);
            if sec.continue_wall > 0.0 || tc.events.is_some() {
                h.push_str(&format!(" (in {} s)", fmt_wall(sec.continue_wall)));
            }
            if !sec.continue_thread.is_empty() {
                h.push_str(&format!(" - thread `{}`", sec.continue_thread));
            }
            h.push('.');
            records.timeout_continuation = Some(h);
        }
    }
    // (19) Partial reply line.
    if sec.partial_needed {
        records.partial_reply = Some(format!(
            "Partial reply: `{}` - {}.",
            sec.partial_rel, sec.partial_footer
        ));
    }
    // (23) Format repair line.
    if let Some(fr) = &sec.format_retry {
        let drift_text = if sec.drift_notes.is_empty() {
            "none".to_string()
        } else {
            format!(
                "{} note(s): {}",
                sec.drift_notes.len(),
                sec.drift_notes.join("; ")
            )
        };
        records.format_repair = Some(if sec.repaired_ok {
            format!(
                "Format repair: succeeded in {} s - the first reply was prose ({}); one repair turn resumed thread `{}` and converted it. Drift: {}. The original prose follows the structured section and is kept as `{}`.",
                fmt_wall(fr.wall_seconds),
                c3_core::one_line(&sec.repair_reason),
                thread,
                drift_text,
                sec.original_rel,
            )
        } else {
            format!(
                "Format repair: failed in {} s - the first reply was prose ({}) and the repair turn did not produce a valid object; the prose is kept below (also `{}`).",
                fmt_wall(fr.wall_seconds),
                c3_core::one_line(&sec.repair_reason),
                sec.original_rel,
            )
        });
    }
    let reply_json_rel = if !ctx.r.raw && !raw_text.is_empty() {
        format!("handoffs/{:02}-codex-{}.reply.json", ctx.nn, ctx.reply_name)
    } else {
        String::new()
    };
    // Format-StructuredStatusLine.
    let verdict_line = structured.map(|s| {
        let v = match s.verdict {
            c3_core::engine::Verdict::Accept => "ACCEPT",
            c3_core::engine::Verdict::Hold => "HOLD",
            c3_core::engine::Verdict::Reject => "REJECT",
            c3_core::engine::Verdict::Advise => "ADVISE",
        };
        let reason = render::close_sentence(&s.verdict_reason);
        let mut vt = format!("Verdict: {v}");
        if reason.is_empty() {
            vt.push('.');
        } else {
            vt.push_str(&format!(" - {reason}"));
        }
        let counts = render::severity_counts(s);
        let ft = if s.findings.is_empty() {
            "Findings: none.".to_string()
        } else {
            format!(
                "Findings: {} ({}, tracked in `findings.json`).",
                render::format_severity_counts(&counts),
                render::format_id_range(finding_ids)
            )
        };
        let mut line = format!("{vt} {ft}");
        if !reply_json_rel.is_empty() {
            let label = if ctx.transport.transport == "prompt-only" {
                "Structured reply (prompt-only transport)"
            } else {
                "Structured reply"
            };
            line.push_str(&format!(" {label}: `{reply_json_rel}`."));
        }
        line
    });
    // The status line renders only when a reply was ingested (`if ($parse)` in the plugin) —
    // i.e. a usable, non-raw run; a failed run (a killed turn) has no reply and no line.
    let ingest_ran = !ctx.r.raw && c3_core::health::is_usable_outcome(bridge_outcome);
    let structured_status = if ingest_ran && structured.is_none() {
        let mut line = format!(
            "Structured reply: INVALID ({validation_error}) - raw text kept; no findings recorded."
        );
        if !reply_json_rel.is_empty() {
            line.push_str(&format!(" Raw last message: `{reply_json_rel}`."));
        }
        Some(line)
    } else {
        None
    };

    let header = HandoffHeader {
        nn: ctx.nn,
        engine_label: "Codex".into(),
        slug: ctx.reply_name.clone(),
        date: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        author: Author::Codex {
            model: model_label(&ctx.identity),
            effort: effort_sent.clone(),
            cli_version: ctx.codex_version.replace("codex-cli ", ""),
        },
        effort_sent: effort_sent.clone(),
        effort_requested: ctx.effort.requested.clone(),
        effort_mapping: ctx.effort.mapping.clone(),
        effort_basis: ctx.effort.basis.clone(),
        consult_id: ctx.consult_id.clone(),
        mode: ctx.effective_mode.clone(),
        sandbox: sandbox_label(&ctx.o),
        purpose: ctx.r.purpose_label.clone(),
        argv: format!("codex {}", ctx.argv_display.trim_start_matches("codex ")),
        prompt_via: "prompt on stdin".into(),
        bridge_outcome: bridge_outcome.to_string(),
        wall_seconds: fmt_wall(wall),
        tokens,
        events_rel: events_rel.clone(),
        further_turns: sec.continue_events_rel.iter().cloned().collect(),
        reviewer_line: reviewer_line(&ctx.identity, &ctx.codex_version),
        preflight_line: if ctx.preflight.is_empty() {
            "Preflight: not recorded (non-openai credential check deferred to M2c+).".into()
        } else {
            format!("Preflight: {}.", ctx.preflight)
        },
        roster_line: None,
        parent_result_line: {
            let parent_line = if !ctx.parent_thread.is_empty() {
                format!("Parent thread: `{}`.", ctx.parent_thread)
            } else if !ctx.parent_note.is_empty() {
                format!("Parent thread: (none - new thread; {}).", ctx.parent_note)
            } else {
                "Parent thread: (none - new thread).".to_string()
            };
            let result_thread = if thread.is_empty() {
                "(unknown)".to_string()
            } else {
                format!("`{thread}`")
            };
            format!("{parent_line} Result thread: {result_thread} (source: {thread_source}).")
        },
        brief_reviewed_line: brief_reviewed_line(ctx),
        timeout_line: {
            let mut t = format!(
                "Timeout: {} s ({}); continuation after a timeout kill: {}.",
                ctx.r.timeout_sec,
                if ctx.r.timeout_source == "purpose" {
                    format!("the default of purpose {}", ctx.r.purpose_label)
                } else {
                    "-TimeoutSec".to_string()
                },
                if ctx.r.continue_sec > 0 {
                    format!("up to {} s", ctx.r.continue_sec)
                } else {
                    "off (-ContinueSec 0)".to_string()
                }
            );
            if let Some(rr) = &ctx.range_record {
                t.push_str(&format!(
                    " Range: `{}` - {} ({} insertions, {} deletions).",
                    rr.spec, ctx.range_text, rr.insertions, rr.deletions
                ));
            }
            t
        },
        verdict_line: verdict_line.or(structured_status),
        records,
    };

    let header_str = header.render();
    // The verbatim reply, then the structured section for a structured reply. A run with no
    // captured reply (a killed turn, no usable continuation) prints the placeholder body and,
    // when it exists, the engine error and stderr tail (`codex-consult.ps1:4382`).
    let body = if raw_text.trim().is_empty() {
        let mut b = if sec.partial_needed {
            format!(
                "_(no reply captured - what the killed turn(s) produced is salvaged in `{}`)_",
                sec.partial_rel
            )
        } else {
            "_(no reply captured)_".to_string()
        };
        if !main_event_error.is_empty() {
            b.push_str(&format!("\n\nCodex reported: {main_event_error}"));
        }
        if !main_stderr.trim().is_empty() {
            b.push_str(&format!("\n\n```\n{}\n```", main_stderr.trim()));
        }
        b
    } else {
        structured
            .map(|s| {
                if s.reply_markdown.trim().is_empty() {
                    "_(empty reply_markdown)_".to_string()
                } else {
                    s.reply_markdown.clone()
                }
            })
            // The reply body is the trimmed reply (`$rawReply = ...Trim()`); the byte-for-byte
            // copy lives in `.reply.json`.
            .unwrap_or_else(|| raw_text.trim().to_string())
    };
    // The header ends `...---\n`; the plugin puts a blank line before the verbatim reply.
    let mut out = header_str.clone();
    out.push('\n');
    out.push_str(&body.replace("\r\n", "\n"));
    out.push('\n');
    if let Some(s) = structured {
        out.push_str("\n---\n\n");
        out.push_str(&render::format_structured_section(s, finding_ids));
        out.push('\n');
    }
    // (`codex-consult.ps1:4399`) a repaired reply keeps the original prose below the section.
    if sec.repaired_ok {
        out.push_str("\n---\n\n## Original reply (prose, before format repair)\n\n");
        out.push_str(&sec.original_prose.trim().replace("\r\n", "\n"));
        out.push('\n');
    }

    // The salvaged partial file (`codex-consult.ps1:4402`): the same metadata block (a new
    // title, no `Verbatim reply follows.`), then the turns and the footer.
    let partial_md = if sec.partial_needed {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!(
            "# Handoff {:02} - Codex: {} - partial reply (a turn was killed on its timeout)",
            ctx.nn, ctx.reply_name
        ));
        for l in header_str.lines().skip(1) {
            if l == "Verbatim reply follows." {
                break;
            }
            lines.push(l.to_string());
        }
        lines.push("What the reviewer produced before the kill follows (every agent message and reasoning text of each turn's event stream, in order, then its tool calls).".to_string());
        let p_header = lines.join("\n");
        Some(format!(
            "{}\n\n---\n\n{}\n\n---\n\n{}\n",
            p_header.trim_end(),
            sec.partial_body.trim_end(),
            sec.partial_footer
        ))
    } else {
        None
    };
    (out, partial_md)
}

/// `Format-Usage` for a tokens clause: the reported counts, or `unknown` when null.
fn usage_clause(u: &Option<Usage>) -> String {
    match u {
        Some(u) => format!(
            "in {} (cached {}), out {}, reasoning {}",
            u.input_tokens, u.cached_input_tokens, u.output_tokens, u.reasoning_output_tokens
        ),
        None => "unknown".to_string(),
    }
}

fn model_label(id: &ReviewerIdentity) -> String {
    if id.model_source == "unknown" {
        "unknown".to_string()
    } else {
        id.model.clone()
    }
}

fn sandbox_label(o: &Options) -> String {
    if o.sandbox.is_empty() {
        "read-only".to_string()
    } else {
        o.sandbox.clone()
    }
}

/// The first 12 hex chars of a hash (`Format-ShortHash`).
fn short_hash(h: &str) -> String {
    h.chars().take(12).collect()
}

/// `Reviewer: <lineage> (provider from <src>, model from <src>; endpoint <host>; provider
/// fingerprint <short>[; <note>]; harness <harness>).` (`$reviewerLine`).
/// The reviewer identity's `endpoint <base_url>, wire_api: <x>` display (`$identity.Display`):
/// the full canonical base_url with the query redacted (`(default)` when the endpoint is
/// Codex's own default), and the wire_api label (`(default)` when the table declares none).
pub(crate) fn identity_display(id: &ReviewerIdentity) -> String {
    let audit = if id.base_url.trim().is_empty() {
        "(default)".to_string()
    } else {
        crate::providers::strip_query(&id.base_url)
    };
    let wire_label = match id.wire_api.trim() {
        "" | "(built in)" => "(default)",
        other => other,
    };
    format!("endpoint {audit}, wire_api: {wire_label}")
}

pub(crate) fn reviewer_line(id: &ReviewerIdentity, codex_version: &str) -> String {
    let harness = format!("codex-cli {}", codex_version.replace("codex-cli ", ""));
    let mut line = format!(
        "Reviewer: {} (provider from {}, model from {}; {}",
        id.lineage,
        if id.provider_source.is_empty() {
            "codex default"
        } else {
            &id.provider_source
        },
        if id.model_source.is_empty() {
            "config"
        } else {
            &id.model_source
        },
        identity_display(id)
    );
    if id.resolved {
        line.push_str(&format!(
            "; provider fingerprint {}",
            short_hash(&id.fingerprint)
        ));
        if !id.note.is_empty() {
            line.push_str(&format!("; {}", id.note));
        }
        line.push_str(&format!("; harness {harness}).",));
    } else {
        line.push_str(&format!(
            "; identity UNRESOLVED - never a parent thread: {}; harness {harness}).",
            id.note
        ));
    }
    line
}

/// `Brief: ... Reviewed: ...` (`$briefLine $reviewedLine`).
fn brief_reviewed_line(ctx: &Context) -> String {
    let brief_line = if ctx.brief_ref.is_empty() {
        "Brief: (none, prompt only).".to_string()
    } else {
        format!(
            "Brief: `{}` (sha256 {}).",
            ctx.brief_ref,
            short_hash(&ctx.brief_sha)
        )
    };
    let tree = if ctx.revision.tree_sha256.is_empty() {
        "(none)".to_string()
    } else {
        short_hash(&ctx.revision.tree_sha256)
    };
    let reviewed_line = format!(
        "Reviewed: {}, base {}, tree sha256 {}, {} changed files.",
        ctx.revision.reviewed_revision, ctx.revision.base_commit, tree, ctx.revision.changed_files
    );
    format!("{brief_line} {reviewed_line}")
}

#[allow(clippy::too_many_arguments)]
fn build_entry(
    ctx: &Context,
    bridge_outcome: &str,
    wall: f64,
    usage: &Option<Usage>,
    thread: &str,
    thread_source: &str,
    structured: Option<&StructuredReply>,
    finding_ids: &[String],
    counts: &FindingCounts,
    validation_error: &str,
    provider_failure: Option<c3_core::ledger::ProviderFailure>,
    handoff_rel: &str,
    reply_json_rel: &str,
    events_rel: &str,
    drift: &Drift,
    sec: &Secondary,
) -> LedgerEntry {
    let mut e = LedgerEntry {
        n: ctx.consult_n,
        when: iso_now(),
        purpose: ctx.o.purpose.clone(),
        // `-Topics`/`-Role` are the roster/M4 surface; a single codex run writes the plugin's
        // empty defaults (`[]` / `""`) so the entry is byte-identical to the live plugin.
        topics: Some(Vec::new()),
        role: Some(String::new()),
        consult_id: ctx.consult_id.clone(),
        lineage: ctx.identity.lineage.clone(),
        preflight: ctx.preflight.clone(),
        preflight_warning: ctx.preflight_warning.clone(),
        parent_thread: ctx.parent_thread.clone(),
        thread: thread.to_string(),
        thread_source: thread_source.to_string(),
        mode: ctx.effective_mode.clone(),
        command: format!("codex {}", ctx.argv_display.trim_start_matches("codex ")),
        brief: ctx.brief_ref.clone(),
        prompt_chars: ctx.prompt_text.chars().count() as i64,
        reply: handoff_rel.to_string(),
        reply_json: reply_json_rel.to_string(),
        events: events_rel.to_string(),
        partial_reply: sec.partial_rel.clone(),
        model: model_label(&ctx.identity),
        effort: ctx.effort.sent.clone(),
        effort_requested: ctx.effort.requested.clone(),
        effort_sent: ctx.effort.sent.clone(),
        effort_mapping: ctx.effort.mapping.clone(),
        effort_caps: ctx.effort.caps.clone(),
        max_words: ctx.r.max_words as i64,
        sandbox: sandbox_label(&ctx.o),
        timeout_sec: ctx.r.timeout_sec,
        timeout_source: ctx.r.timeout_source.clone(),
        continue_sec: ctx.r.continue_sec,
        extra_config: ctx
            .r
            .extra_config
            .iter()
            .map(|s| serde_json::Value::String(s.clone()))
            .collect(),
        // Peak status evaluated at launch (`run_live` re-evaluated it as call 1).
        peak: ctx.peak,
        peak_schedule: ctx.peak_schedule.clone(),
        peak_source: ctx.peak_source.clone(),
        peak_evaluated_at: ctx.peak_evaluated_at.clone(),
        schema: if ctx.r.raw {
            String::new()
        } else {
            "consult-reply v1".into()
        },
        schema_transport: ctx.transport.transport.clone(),
        schema_transport_source: ctx.transport.source.clone(),
        validation_error: validation_error.to_string(),
        format_retry: sec.format_retry.clone(),
        timeout_continue: sec.timeout_continue.clone(),
        range: ctx.range_record.clone(),
        warnings: ctx
            .run_warnings
            .iter()
            .map(|w| serde_json::Value::String(w.clone()))
            .collect(),
        base_commit: ctx.revision.base_commit.clone(),
        reviewed_revision: ctx.revision.reviewed_revision.clone(),
        tree_sha256: ctx.revision.tree_sha256.clone(),
        tree_sha256_after: drift.tree_sha256_after.clone(),
        tree_changed_during_review: drift.tree_changed,
        revision_moved: Some(if drift.revision_moved.is_empty() {
            None
        } else {
            Some(drift.revision_moved.clone())
        }),
        changed_files: ctx.revision.changed_files,
        brief_sha256: ctx.brief_sha.clone(),
        brief_sha256_after: drift.brief_sha_after.clone(),
        brief_changed_during_review: drift.brief_changed,
        fingerprint_note: ctx.revision.fingerprint_note.clone(),
        bridge_outcome: bridge_outcome.to_string(),
        provider_failure,
        structured: structured.is_some(),
        findings: counts.clone(),
        finding_ids: finding_ids
            .iter()
            .map(|s| serde_json::Value::String(s.clone()))
            .collect(),
        usage: usage.clone(),
        wall_seconds: wall,
        finished_at: iso_now(),
        reviewer: build_reviewer(&ctx.identity, &ctx.codex_version),
        ..Default::default()
    };
    if let Some(s) = structured {
        e.verdict = match s.verdict {
            c3_core::engine::Verdict::Accept => "ACCEPT",
            c3_core::engine::Verdict::Hold => "HOLD",
            c3_core::engine::Verdict::Reject => "REJECT",
            c3_core::engine::Verdict::Advise => "ADVISE",
        }
        .to_string();
        e.verdict_reason = s.verdict_reason.clone();
    }
    e
}

fn build_reviewer(id: &ReviewerIdentity, codex_version: &str) -> Reviewer {
    // provider_config: `{builtin:"openai"}` for the built-in endpoint (no base_url), else the
    // user provider table's `{base_url,name,wire_api}` (`Resolve-ReviewerIdentity`).
    let provider_config = if id.base_url.is_empty() {
        serde_json::json!({ "builtin": "openai" })
    } else {
        let mut m = serde_json::Map::new();
        m.insert(
            "base_url".into(),
            serde_json::Value::String(id.base_url.clone()),
        );
        m.insert(
            "name".into(),
            serde_json::Value::String(id.provider.clone()),
        );
        if !id.wire_api.is_empty() {
            m.insert(
                "wire_api".into(),
                serde_json::Value::String(id.wire_api.clone()),
            );
        }
        serde_json::Value::Object(m)
    };
    Reviewer {
        provider: id.provider.clone(),
        provider_source: id.provider_source.clone(),
        model: id.model.clone(),
        model_source: id.model_source.clone(),
        engine: "codex".into(),
        harness: format!("codex-cli {}", codex_version.replace("codex-cli ", "")),
        provider_fingerprint: id.fingerprint.clone(),
        provider_config,
        identity_note: id.note.clone(),
        ..Default::default()
    }
}

#[cfg(test)]
mod telemetry_tests {
    use super::*;
    use c3_core::ledger::LedgerEntry;
    use std::sync::Mutex;

    // Serialises the env-mutating part of this file's tests against itself.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn spool_pending(home: &Path) -> usize {
        let p = home.join("c3").join("telemetry").join("spool.ndjson");
        std::fs::read_to_string(p)
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
    }

    #[test]
    fn telemetry_gates_govern_enqueue() {
        let _g = ENV_LOCK.lock().unwrap();

        // The consult flow enqueues only inside `finish()`, gated by `ctx.telemetry_enabled`
        // (= `is_enabled`), and `finish()` runs only for a real (non-dry) run. `run()` gates
        // the background flush by `is_enabled(cfg) && !dry_run`.
        let on = telemetry::Config {
            telemetry: Some(true),
        };
        // A dry run never flushes and never reaches the enqueue site.
        let dry_run = true;
        assert!(!(telemetry::is_enabled(&on) && !dry_run));
        // `--telemetry off` disables the enqueue gate regardless of the environment.
        assert!(!telemetry::is_enabled(&telemetry::Config {
            telemetry: Some(false),
        }));

        // env `CODEX_CONSULT_TELEMETRY=off`: `is_enabled` false AND `record_consultation`
        // enqueues nothing (observed on an isolated spool).
        let home = std::env::temp_dir().join(format!("c3-tele-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("CODEX_HOME", &home);
        std::env::set_var("CODEX_CONSULT_TELEMETRY", "off");
        let off_cfg = telemetry::Config::default();
        assert!(!telemetry::is_enabled(&off_cfg));
        let entry = LedgerEntry {
            n: 7,
            ..Default::default()
        };
        let _ = telemetry::record_consultation(&entry, None);
        assert_eq!(spool_pending(&home), 0, "env off must enqueue nothing");

        // Control: with the switch on, the same record DOES enqueue one event (proves the
        // guard suppresses, rather than the path being a no-op).
        std::env::remove_var("CODEX_CONSULT_TELEMETRY");
        assert!(telemetry::is_enabled(&telemetry::Config::default()));
        let _ = telemetry::record_consultation(&entry, None);
        assert_eq!(spool_pending(&home), 1, "switch on enqueues one event");

        std::env::remove_var("CODEX_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[cfg(test)]
mod parent_walk_tests {
    use super::*;
    use c3_core::ledger::{LedgerEntry, Reviewer};

    fn id(provider: &str, model: &str, fp: &str) -> ReviewerIdentity {
        ReviewerIdentity {
            provider: provider.into(),
            provider_source: "-Provider".into(),
            model: model.into(),
            model_source: "-Model".into(),
            lineage: c3_core::lineage::format_reviewer_lineage(provider, model, "codex"),
            resolved: true,
            note: String::new(),
            error: String::new(),
            fingerprint: fp.into(),
            compat_string: String::new(),
            host: String::new(),
            base_url: String::new(),
            wire_api: String::new(),
            engine: "codex".into(),
        }
    }

    fn entry(n: i64, thread: &str, provider: &str, model: &str, fp: &str) -> LedgerEntry {
        LedgerEntry {
            n,
            thread: thread.into(),
            reviewer: Reviewer {
                provider: provider.into(),
                model: model.into(),
                engine: "codex".into(),
                provider_fingerprint: fp.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn unknown_thread_refused() {
        let e =
            select_parent_thread(&[], &id("openai", "gpt-5.1", "fp1"), "fork", "abc").unwrap_err();
        assert_eq!(
            e,
            "thread abc has unknown provenance: it is not in this task's ledger; use -Mode new"
        );
    }

    #[test]
    fn cross_lineage_thread_refused() {
        let entries = vec![entry(1, "t-1", "ZAI", "glm-5.3", "fpz")];
        let e = select_parent_thread(&entries, &id("openai", "gpt-5.1", "fp1"), "fork", "t-1")
            .unwrap_err();
        assert!(
            e.contains(
                "belongs to lineage ZAI :: glm-5.3 (consult n=1); this run is openai :: gpt-5.1"
            ),
            "{e}"
        );
        assert!(
            e.ends_with("use -Mode new, or run as ZAI :: glm-5.3"),
            "{e}"
        );
    }

    #[test]
    fn legacy_entry_refused_for_thread() {
        // A pre-0.3 entry (no reviewer fields at all) recorded this thread.
        let mut leg = entry(1, "t-1", "", "", "");
        leg.reviewer = Reviewer::default();
        let e = select_parent_thread(&[leg], &id("openai", "gpt-5.1", "fp1"), "fork", "t-1")
            .unwrap_err();
        assert_eq!(
            e,
            "thread t-1 has unknown provenance (recorded before 0.3.0); use -Mode new"
        );
    }

    #[test]
    fn fork_needs_parent_when_none_of_lineage() {
        let entries = vec![entry(1, "t-1", "ZAI", "glm-5.3", "fpz")];
        let e = select_parent_thread(&entries, &id("openai", "gpt-5.1", "fp1"), "fork", "")
            .unwrap_err();
        assert!(e.starts_with("-Mode fork needs a parent thread: no thread of lineage openai :: gpt-5.1 in this task's ledger"), "{e}");
        assert!(e.contains("other lineage(s): ZAI :: glm-5.3"), "{e}");
        assert!(
            e.ends_with("Pass -Thread <uuid> of lineage openai :: gpt-5.1, or use -Mode new"),
            "{e}"
        );
    }

    #[test]
    fn auto_forks_newest_same_lineage() {
        let entries = vec![
            entry(1, "t-1", "openai", "gpt-5.1", "fp1"),
            entry(2, "t-2", "openai", "gpt-5.1", "fp1"),
        ];
        let r = select_parent_thread(&entries, &id("openai", "gpt-5.1", "fp1"), "", "").unwrap();
        assert_eq!(r.parent_thread, "t-2");
        assert_eq!(r.mode, "fork");
        assert_eq!(
            r.note,
            "newest thread of lineage openai :: gpt-5.1 (consult n=2)"
        );
    }

    #[test]
    fn thread_drift_refused() {
        let entries = vec![entry(1, "t-1", "openai", "gpt-5.1", "OLDfp")];
        let e = select_parent_thread(&entries, &id("openai", "gpt-5.1", "NEWfp"), "resume", "t-1")
            .unwrap_err();
        assert!(
            e.contains("endpoint or protocol of provider openai changed since thread t-1"),
            "{e}"
        );
        assert!(e.contains("start a new thread with -Mode new"), "{e}");
    }
}
