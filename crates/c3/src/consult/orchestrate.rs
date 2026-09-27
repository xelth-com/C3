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
use super::ingest::{self, Ingestion};
use super::prompt::{self, OpenFinding, PromptInputs};
use super::render;
use super::revision::{self, RevisionInfo};
use super::summary;

const TOOL: &str = "c3 consult";

/// `-Range` size-warning thresholds (`$rangeWarnLines` / `$rangeWarnTimeout`).
const RANGE_WARN_LINES: i64 = 1500;
const RANGE_WARN_TIMEOUT: i64 = 2400;

/// Print a refusal (`Stop-WithError`) and return the usage exit code (1).
fn refuse(msg: &str) -> i32 {
    eprintln!("{TOOL}: {msg}");
    1
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
fn resolve_preflight(id: &ReviewerIdentity, launcher: &str) -> (String, Option<(String, i32)>) {
    // The plugin refuses a preflight through `Stop-WithError` (exit 1, nothing written,
    // `codex-consult.ps1:304`); c3 matches that, not cli-surface.md's aspirational exit 2/3.
    if let Some(v) = verdict_pre_credential(id) {
        return (v.preflight, Some((v.refusal, 1)));
    }
    // Only the built-in openai endpoint's `codex login status` is checked in M2c.
    let builtin_openai = id.provider == "openai" && id.base_url.is_empty();
    if !builtin_openai {
        return (String::new(), None);
    }
    let timeout = std::env::var("CODEX_CONSULT_TEST_LOGIN_TIMEOUT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(20);
    let cred = providers::get_codex_login_status(launcher, timeout);
    let v = verdict_with_credential(id, None, cred, false);
    if v.state == "available" {
        (v.preflight, None)
    } else {
        (v.preflight, Some((v.refusal, 1)))
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
    /// The preflight string recorded in the ledger / handoff (`""` when not evaluated).
    pub(crate) preflight: String,
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
    let (preflight, preflight_refusal) = if o.skip_preflight {
        ("skipped".to_string(), None)
    } else {
        resolve_preflight(&identity, &launcher)
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
        preflight,
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
        mode: mode_of(o),
        sandbox,
        schema_path: schema_arg,
        extra_config: r.extra_config.clone(),
        output_last_message: Some(last_msg_path.to_path_buf()),
        prompt_file: None,
        max_model_steps: None,
    }
}

fn mode_of(o: &Options) -> Mode {
    // M2c: fork/resume lineage resolution from the ledger is deferred; a bare -Mode
    // fork/resume with -Thread uses the resolved identity's lineage as the key.
    match o.mode.as_str() {
        "fork" if !o.thread.is_empty() => Mode::Fork {
            thread: o.thread.clone(),
            lineage: c3_core::engine::Lineage(String::new()),
        },
        "resume" if !o.thread.is_empty() => Mode::Resume {
            thread: o.thread.clone(),
            lineage: c3_core::engine::Lineage(String::new()),
        },
        _ => Mode::New,
    }
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
        mode: mode_of(o),
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
    let bridge_outcome;
    let mut structured: Option<StructuredReply> = None;
    let mut raw_text = String::new();
    let mut usage: Option<Usage> = None;
    let mut wall_seconds = 0.0_f64;
    let mut thread = String::new();
    let mut thread_source = "unknown".to_string();
    let mut validation_error = String::new();
    let mut provider_failure = None;
    let mut usable = false;
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
            if ctx.r.raw {
                bridge_outcome = "usable reply".to_string();
                usable = true;
            } else {
                match ingest::classify(&raw_text) {
                    Ingestion::Structured(s) => {
                        structured = Some(*s);
                        bridge_outcome = "usable reply".to_string();
                        usable = true;
                    }
                    Ingestion::Prose(gate) => {
                        // M2c: format repair is deferred; a substantive prose reply is kept as
                        // the reply of record (usable), a non-substantive one records why.
                        bridge_outcome = "usable reply".to_string();
                        usable = true;
                        validation_error = if gate.substantive {
                            "the reply is prose, not a JSON object (format repair deferred to M2c+)"
                                .to_string()
                        } else {
                            format!(
                                "the reply is not a valid consult-reply v1 object {}",
                                ingest::not_attempted_suffix(&gate)
                            )
                        };
                    }
                }
            }
        }
        AttemptOutcome::TimedOut { survivors, .. } => {
            wall_seconds = ctx.r.timeout_sec as f64;
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
        AttemptOutcome::ProviderFailure(pf) => {
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

    // Render the handoff markdown.
    let handoff_md = render_handoff(
        &ctx,
        &bridge_outcome,
        wall_seconds,
        &usage,
        &thread,
        &thread_source,
        structured.as_ref(),
        &finding_ids,
        &validation_error,
        provider_failure.as_ref(),
        &raw_text,
        &drift,
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
    );

    // Keep a copy for telemetry (the entry is moved into the commit below).
    let entry_for_telemetry = if ctx.telemetry_enabled {
        Some(entry.clone())
    } else {
        None
    };

    // Commit under the write lock: the handoff `.md`. The raw `.reply.json` was already
    // written above, before the lock (crash-safety), so it is not in the commit `files` list.
    let files = vec![(handoff_rel.clone(), handoff_md.clone().into_bytes())];
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
        .unwrap_or_else(|| raw_text.clone());
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

    let s = summary::SummaryInputs {
        bridge_outcome: bridge_outcome.clone(),
        usable,
        wall_seconds: fmt_wall(wall_seconds),
        lineage_shown: ctx.identity.lineage.clone(),
        mode: mode_str(&ctx.o),
        thread: thread.clone(),
        thread_source: thread_source.clone(),
        verdict_line: verdict_line.unwrap_or_default(),
        findings_line: findings_line.unwrap_or_default(),
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

fn classify_exit(outcome: &str, pf: Option<&c3_core::ledger::ProviderFailure>) -> i32 {
    // The plugin exits 1 for every non-usable outcome (`codex-consult.ps1` summary block);
    // the class-`transport` signal codes 130/143 (a codex process killed by SIGINT/SIGTERM)
    // are an M2c+ refinement, not the timeout-kill path (which is a plain failed run = 1).
    let _ = (outcome, pf);
    1
}

fn mode_str(o: &Options) -> String {
    if o.mode.is_empty() {
        "new".to_string()
    } else {
        o.mode.clone()
    }
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
) -> String {
    let events_rel = format!(
        "handoffs/{:02}-codex-{}.events.jsonl",
        ctx.nn, ctx.reply_name
    );
    let effort_sent = ctx.effort.sent.clone().unwrap_or_else(|| "nothing".into());
    let tokens = match usage {
        Some(u) => TokenReport::Reported {
            input: u.input_tokens,
            cached: u.cached_input_tokens,
            output: u.output_tokens,
            reasoning: u.reasoning_output_tokens,
        },
        None => TokenReport::NotReported {
            engine: "codex".into(),
        },
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
    let structured_status = if !ctx.r.raw && structured.is_none() {
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
        mode: mode_str(&ctx.o),
        sandbox: sandbox_label(&ctx.o),
        purpose: ctx.r.purpose_label.clone(),
        argv: format!("codex {}", ctx.argv_display.trim_start_matches("codex ")),
        prompt_via: "prompt on stdin".into(),
        bridge_outcome: bridge_outcome.to_string(),
        wall_seconds: fmt_wall(wall),
        tokens,
        events_rel: events_rel.clone(),
        further_turns: vec![],
        reviewer_line: reviewer_line(&ctx.identity, &ctx.codex_version),
        preflight_line: if ctx.preflight.is_empty() {
            "Preflight: not recorded (non-openai credential check deferred to M2c+).".into()
        } else {
            format!("Preflight: {}.", ctx.preflight)
        },
        roster_line: None,
        parent_result_line: format!(
            "Parent thread: (none - new thread). Result thread: `{}` (source: {}).",
            thread, thread_source
        ),
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

    let mut out = header.render();
    // The verbatim reply, then the structured section for a structured reply.
    let body = structured
        .map(|s| {
            if s.reply_markdown.trim().is_empty() {
                "_(empty reply_markdown)_".to_string()
            } else {
                s.reply_markdown.clone()
            }
        })
        .unwrap_or_else(|| raw_text.to_string());
    // The header ends `...---\n`; the plugin puts a blank line before the verbatim reply.
    out.push('\n');
    out.push_str(&body);
    if let Some(s) = structured {
        out.push_str("\n\n---\n\n");
        out.push_str(&render::format_structured_section(s, finding_ids));
        out.push('\n');
    }
    out
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
pub(crate) fn reviewer_line(id: &ReviewerIdentity, codex_version: &str) -> String {
    let harness = format!("codex-cli {}", codex_version.replace("codex-cli ", ""));
    let host = if id.host.is_empty() {
        "builtin:openai"
    } else {
        &id.host
    };
    let mut line = format!(
        "Reviewer: {} (provider from {}, model from {}; endpoint {}",
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
        host
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
) -> LedgerEntry {
    let mut e = LedgerEntry {
        n: ctx.consult_n,
        when: iso_now(),
        purpose: ctx.o.purpose.clone(),
        consult_id: ctx.consult_id.clone(),
        lineage: ctx.identity.lineage.clone(),
        preflight: ctx.preflight.clone(),
        parent_thread: String::new(),
        thread: thread.to_string(),
        thread_source: thread_source.to_string(),
        mode: mode_str(&ctx.o),
        command: format!("codex {}", ctx.argv_display.trim_start_matches("codex ")),
        brief: ctx.brief_ref.clone(),
        prompt_chars: ctx.prompt_text.chars().count() as i64,
        reply: handoff_rel.to_string(),
        reply_json: reply_json_rel.to_string(),
        events: events_rel.to_string(),
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
