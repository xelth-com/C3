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

use c3_core::effort::{caps, Models};
use c3_core::engine::{
    AttemptOutcome, ConsultationId, Engine, EngineKind, Mode, Request, StructuredReply, TurnKind,
    TurnRequest,
};
use c3_core::handoff::{Author, HandoffHeader, OptionalRecords, TokenReport};
use c3_core::ledger::{FindingCounts, LedgerEntry, Reviewer, Usage};
use c3_core::lineage::{resolve_reviewer_identity, ReviewerIdentity};
use c3_core::store::{
    CommitRequest, EvidenceStore, FilesStore, FindingsDelta, LockRecord, PendingRecord, PendingRef,
    PendingState, RecoveryDisposition,
};
use c3_core::task_slug::TaskSlug;
use c3_core::verdict::{verdict_pre_credential, verdict_with_credential};

use crate::engines::codex::{CodexEngine, TurnFiles};
use crate::providers;

use super::args::{self, Options, Resolved};
use super::ingest::{self, Ingestion};
use super::prompt::{self, OpenFinding, PromptInputs};
use super::render;
use super::revision::{self, RevisionInfo};
use super::summary;

const TOOL: &str = "c3 consult";

/// Print a refusal (`Stop-WithError`) and return the usage exit code (1).
fn refuse(msg: &str) -> i32 {
    eprintln!("{TOOL}: {msg}");
    1
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
    pub(crate) schema_path: Option<PathBuf>,
    pub(crate) open_findings_count: usize,
    /// The preflight string recorded in the ledger / handoff (`""` when not evaluated).
    pub(crate) preflight: String,
    /// A preflight refusal `(message, exit_code)` for a real run; `None` = available/skipped.
    pub(crate) preflight_refusal: Option<(String, i32)>,
    pub(crate) revision: RevisionInfo,
    // paths
    pub(crate) handoffs_dir: PathBuf,
    pub(crate) reply_path: PathBuf,
    pub(crate) reply_json_path: PathBuf,
    pub(crate) events_path: PathBuf,
    pub(crate) last_msg_path: PathBuf,
    pub(crate) stderr_path: PathBuf,
}

/// Run one consultation; return the exit code.
pub fn run(o: Options) -> i32 {
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
    let last_msg_path = handoffs_dir.join(format!("{stem}.last.txt"));
    let stderr_path = handoffs_dir.join(format!("{stem}.stderr.txt"));

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

    // The reply schema path (shipped with the tool; in prompt-only its text is inlined).
    let schema_path = if r.raw { None } else { schema_file(&repo_root) };
    let schema_text = if transport.transport == "prompt-only" {
        schema_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| s.trim().replace("\r\n", "\n").replace('\n', "\r\n"))
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Open findings snapshot.
    let (open_findings, open_findings_count) = read_open_findings(&store, &task);

    let consult_id = uuid::Uuid::new_v4().to_string();

    // Range (measured once). M2c: the record/measurement is deferred; the prompt line uses
    // the spec text only when a range was given (already validated as a pair).
    let range = None;

    let prompt_text = prompt::assemble(&PromptInputs {
        raw: r.raw,
        purpose: &o.purpose,
        prompt: &o.prompt,
        brief_ref: &brief_ref,
        range,
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
        schema_path,
        open_findings_count,
        preflight,
        preflight_refusal,
        revision,
        handoffs_dir,
        reply_path,
        reply_json_path,
        events_path,
        last_msg_path,
        stderr_path,
    })
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

fn schema_file(repo_root: &Path) -> Option<PathBuf> {
    // The reply schema shipped with the plugin; in C3 it is looked up relative to the repo.
    for cand in [
        repo_root.join("schemas/consult-reply.schema.json"),
        repo_root.join("crates/c3/schemas/consult-reply.schema.json"),
    ] {
        if cand.is_file() {
            return Some(cand);
        }
    }
    // Fall back to a stable path even when absent (dry run still names it).
    Some(repo_root.join("schemas/consult-reply.schema.json"))
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

    // Reserve the recovery record.
    let reply_rel = format!("handoffs/{:02}-codex-{}.md", ctx.nn, ctx.reply_name);
    let mut rec = PendingRecord {
        state: PendingState::Reserved,
        ..Default::default()
    };
    rec.n = ctx.consult_n;
    rec.nn = format!("{:02}", ctx.nn);
    rec.reply = reply_rel.clone();
    rec.consult_id = ctx.consult_id.clone();
    rec.launcher = ctx.launcher.clone();
    rec.engine = "codex".into();
    let _ = store.write_pending(&pending, &rec);

    // The plugin fingerprints the tree AFTER the lock and recovery record exist, so the
    // `.consult.*` files under the collab dir are counted among the excluded entries
    // (`fingerprint_note`). Re-fingerprint here to match (`$revBefore` in the plugin flow).
    ctx.revision = revision::revision_info(&ctx.repo_root, Some(&ctx.collab_root));

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

    finish(ctx, store, pending, outcome)
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
fn finish(ctx: Context, store: FilesStore, pending: PendingRef, outcome: AttemptOutcome) -> i32 {
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
            bridge_outcome = format!(
                "failed: timeout after {} s (process tree killed{})",
                ctx.r.timeout_sec,
                if survivors.is_empty() {
                    String::new()
                } else {
                    format!("; {} processes survived", survivors.len())
                }
            );
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
    );
    let handoff_rel = format!("handoffs/{:02}-codex-{}.md", ctx.nn, ctx.reply_name);
    let events_rel = format!(
        "handoffs/{:02}-codex-{}.events.jsonl",
        ctx.nn, ctx.reply_name
    );
    // The plugin writes `.reply.json` (the byte-for-byte raw reply) at the handoff path for
    // every non-raw run. M2c commits it via the commit `files` list; the store's
    // `write_raw_reply` staging path (`.consult.reply.json`) is not used, so no stray staging
    // file is left (the F04-11 "raw reply before the lock" crash-safety nuance is deferred).
    let reply_json_rel = if !ctx.r.raw && !raw_text.is_empty() {
        format!("handoffs/{:02}-codex-{}.reply.json", ctx.nn, ctx.reply_name)
    } else {
        String::new()
    };

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
    );

    // Commit under the write lock: the handoff `.md` and, for a non-raw run, the raw
    // `.reply.json` at the handoff path (byte-for-byte).
    let mut files = vec![(handoff_rel.clone(), handoff_md.clone().into_bytes())];
    if !reply_json_rel.is_empty() {
        files.push((reply_json_rel.clone(), raw_text.clone().into_bytes()));
    }
    let write_lock = match store.take_write_lock(&ctx.task) {
        Ok(l) => l,
        Err(e) => return refuse(&format!("could not take the write lock: {e}")),
    };
    let commit = CommitRequest {
        entry,
        findings: delta,
        pending: &pending,
        disposition: RecoveryDisposition::Remove,
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
    let mut records = OptionalRecords::default();
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
        timeout_line: format!(
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
        ),
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
fn reviewer_line(id: &ReviewerIdentity, codex_version: &str) -> String {
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
        let sha = ctx
            .brief_path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| short_hash(&c3_core::sha256_hex(&b)))
            .unwrap_or_default();
        format!("Brief: `{}` (sha256 {}).", ctx.brief_ref, sha)
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
        // No peak evaluation in M2c: no declared window → the plugin's
        // `peak: null, peak_source: "none", peak_evaluated_at: <iso>` shape.
        peak: None,
        peak_source: "none".into(),
        peak_evaluated_at: iso_now(),
        schema: if ctx.r.raw {
            String::new()
        } else {
            "consult-reply v1".into()
        },
        schema_transport: ctx.transport.transport.clone(),
        schema_transport_source: ctx.transport.source.clone(),
        validation_error: validation_error.to_string(),
        base_commit: ctx.revision.base_commit.clone(),
        reviewed_revision: ctx.revision.reviewed_revision.clone(),
        tree_sha256: ctx.revision.tree_sha256.clone(),
        changed_files: ctx.revision.changed_files,
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
        revision_moved: Some(None),
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
