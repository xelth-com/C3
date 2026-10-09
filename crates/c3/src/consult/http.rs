//! The `http` seat's run path (M7b-b): the OpenAI-compatible reviewer wired into `c3 consult`.
//!
//! The `http` engine adapter (`crate::http_engine`) is engine-agnostic: it holds a resolved
//! [`HttpConfig`], a retained [`ReviewerPack`] and a handoff stem, and it sends one
//! `chat/completions` request. This module is the orchestrator side of the wiring the adapter's
//! doc calls for: it resolves the seat's config (from the roster's `ext.c3.reviewers` entry, or
//! from `--engine http --provider ... --model ...` with the OpenRouter defaults), runs the
//! billing/key guard, builds the reviewer pack from the brief and the bound artifacts, and hands
//! `run_primary_turn` the outcome plus the `provider_config` the ledger records. It also renders
//! the `--dry-run` block (endpoint, model, key status, pack size, and the request plan with the
//! `Authorization` header redacted); a dry run makes no network call and writes nothing.
//!
//! Key contract (DESIGN §3 invariant 4): the credential is read from the environment only, at run
//! time; it is never a flag, a config value, a roster field, or anything this module prints. The
//! billing guard refuses a subscription provider outright and a lab label unless the roster
//! entry accepted per-token billing (`api_billing: accepted`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use c3_core::engine::{
    AttemptId, AttemptOutcome, ConsultationId, Continuation, EngineKind, Mode, Request, TurnKind,
    TurnRequest,
};
use serde_json::Value;

use crate::http_engine::{HttpAuth, HttpConfig, HttpEngine, DEFAULT_BASE_URL, DEFAULT_KEY_ENV};
use crate::pack::reviewer::{self, PackOpts, ReviewerPack};

use super::orchestrate::Context;

/// Lab labels that also sell a signed-in subscription: sending an API key here bills per token
/// where the subscription may already cover it, so the guard refuses unless the roster entry says
/// `"api_billing": "accepted"` (M7b-b decision 3). The subscription-engine labels themselves
/// (`codex`, `chatgpt`, `muse`, `agy`, `antigravity`) are refused unconditionally by the adapter's
/// own `precheck` (reused via [`crate::http_engine::SUBSCRIPTION_PROVIDERS`]).
const LAB_LABELS: [&str; 4] = ["openai", "gemini", "google", "meta"];

/// A resolved http seat: the request config, whether the roster accepted per-token billing, and
/// the roster entry's periphery token budget (`-1` when it names none).
struct Seat {
    config: HttpConfig,
    api_billing_accepted: bool,
    pack_tokens: i64,
}

/// (S6) Resolve the pack periphery budget for this run: `--pack-budget` (when given) wins over the
/// roster entry's `pack_tokens`, which wins over the default; clamped to `0..=MAX_PACK_TOKENS`.
fn resolve_pack_budget(ctx: &Context, seat: &Seat) -> usize {
    let n = if ctx.o.pack_budget >= 0 {
        ctx.o.pack_budget
    } else if seat.pack_tokens >= 0 {
        seat.pack_tokens
    } else {
        c3_core::roster_ext::DEFAULT_PACK_TOKENS
    };
    n.clamp(0, c3_core::roster_ext::MAX_PACK_TOKENS) as usize
}

/// What one http seat run yields to `run_primary_turn`.
pub(crate) struct SeatRun {
    pub outcome: AttemptOutcome,
    /// `reviewer.provider_config` for the ledger (`{engine, base_url, model, pack, pack_sha256}`).
    pub provider_config: Value,
    /// The engine bridge-outcome text (used by `finish` on a provider failure).
    pub bridge_outcome: String,
    /// The reply text a failing turn produced (kept as `.reply.json`); empty on success.
    pub reply_text: String,
    /// (item 2) Engine warnings — the `reply normalised: <list>` line when a near-valid reply was
    /// locally repaired; empty otherwise. Recorded in the ledger `warnings[]` and printed.
    pub warnings: Vec<String>,
    /// (STEP 2) The result of a secondary turn (a format-repair replay or a timeout retry) when one
    /// ran; `None` when the primary turn was the only one. The orchestrator copies it into the
    /// secondary record so the ledger shows `engine_turns: 2` and the `format_repair` fields.
    pub secondary: Option<HttpSecondary>,
}

/// (STEP 2) What a secondary http turn produced, for the orchestrator to record via the existing
/// secondary-turn ledger fields.
#[derive(Default, Clone)]
pub(crate) struct HttpSecondary {
    /// The engine turns run (2 when a secondary turn ran).
    pub engine_turns: i64,
    /// A format-repair record when a repair turn ran (`None` for a timeout retry).
    pub format_retry: Option<c3_core::ledger::FormatRetry>,
    pub repaired_ok: bool,
    pub repair_reason: String,
    pub original_rel: String,
    pub original_prose: String,
    pub drift_notes: Vec<String>,
}

/// Resolve the seat's config: a roster `ext.c3.reviewers` entry matching the resolved identity
/// wins; otherwise the direct-run defaults with the `--base-url` / `--key-env` overrides.
fn resolve_seat(ctx: &Context) -> Result<Seat, String> {
    let provider = ctx.identity.provider.clone();
    let model = ctx.identity.model.clone();

    // A roster entry for this http reviewer (matched by provider + model) carries the full config.
    let roster_hit = crate::providers::read_reviewer_roster().ok().and_then(|r| {
        r.http_reviewers
            .into_iter()
            .find(|h| h.provider == provider && h.model == model)
    });

    let (base_url, key_env, json_object, headers, api_billing_accepted, pack_tokens) =
        if let Some(h) = roster_hit {
            (
                h.base_url,
                h.key_env,
                h.json_object,
                h.headers,
                h.api_billing_accepted,
                h.pack_tokens,
            )
        } else {
            let base_url = if ctx.o.base_url.trim().is_empty() {
                DEFAULT_BASE_URL.to_string()
            } else {
                ctx.o.base_url.trim().to_string()
            };
            let key_env = if ctx.o.key_env.trim().is_empty() {
                DEFAULT_KEY_ENV.to_string()
            } else {
                ctx.o.key_env.trim().to_string()
            };
            (base_url, key_env, true, Vec::new(), false, -1)
        };

    let timeout = Duration::from_secs(ctx.r.timeout_sec.max(1) as u64);
    Ok(Seat {
        config: HttpConfig {
            base_url,
            model,
            key_env,
            headers,
            timeout,
            provider_label: provider,
            json_object,
            repo_root: Some(ctx.repo_root.clone()),
        },
        api_billing_accepted,
        pack_tokens,
    })
}

/// The billing/key guard (M7b-b decision 3), run before every turn. Refuses a subscription
/// provider outright, a lab label unless per-token billing was accepted in the roster, and a
/// launch with no key in the environment. Never reads or prints the key value.
fn billing_precheck(seat: &Seat) -> Result<(), String> {
    let label = seat.config.provider_label.trim().to_ascii_lowercase();
    if crate::http_engine::SUBSCRIPTION_PROVIDERS
        .iter()
        .any(|p| p.eq_ignore_ascii_case(&label))
    {
        return Err(format!(
            "refusing the http engine for provider `{}`: it is a subscription engine that would be billed per token on the API path; use its own engine instead.",
            seat.config.provider_label
        ));
    }
    if LAB_LABELS.contains(&label.as_str()) && !seat.api_billing_accepted {
        return Err(format!(
            "refusing the http engine for the lab `{}`: an API key here bills per token where a signed-in subscription may already cover it; set \"api_billing\": \"accepted\" in the roster's ext.c3.reviewers entry to allow it.",
            seat.config.provider_label
        ));
    }
    // The proxy auth mode (`C3_HTTP_AUTH_PROXY` lists this host) reads no key: the egress proxy
    // attaches the credential and no Authorization header is sent.
    if seat.config.auth_mode() == HttpAuth::Proxy {
        return Ok(());
    }
    // The key is read from the environment only; report only whether the variable is set.
    if std::env::var(&seat.config.key_env)
        .ok()
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        return Err(format!(
            "env {} not set: the http engine reads its key from the environment only (never a flag or a config value).",
            seat.config.key_env
        ));
    }
    Ok(())
}

/// Build the reviewer pack from the brief and the bound artifacts (the focus files). The http
/// reviewer receives only this pack (DESIGN §3 invariant 2); it needs a brief and at least one
/// artifact to review.
fn build_pack(
    ctx: &Context,
    budget: usize,
) -> Result<(ReviewerPack, Vec<reviewer::PeerPackStat>), String> {
    let brief = match &ctx.brief_path {
        Some(p) => p.clone(),
        None => {
            return Err(
                "the http engine builds a reviewer pack: it needs -Brief <path> (the ask a pack is built around).".to_string(),
            )
        }
    };
    // The bound artifacts are the focus files. The raw `--artifact` values are the focus globs
    // (the same repo-relative form `c3 pack --focus` matches against `discover`'s output);
    // `ArtifactHash.path`/`.full` are the absolute/canonical paths kept for the ledger and drift
    // check, not for glob matching.
    let focus: Vec<String> = ctx
        .o
        .artifacts
        .iter()
        .filter(|a| !a.trim().is_empty())
        .cloned()
        .collect();
    if focus.is_empty() {
        return Err(
            "the http engine builds a reviewer pack: it needs at least one -Artifact <path> (the file(s) to review, shown in full).".to_string(),
        );
    }
    // Mirror the `c3 pack` CLI: default the index connection to the embedded store `c3 index`
    // builds, so a pack uses it automatically when present; an absent store, a held lock or an
    // empty index falls back to the lexical neighbourhood (the http engine works identically with
    // no index).
    let conn = Some(format!(
        "surrealkv:{}",
        ctx.collab_root
            .join(".c3")
            .join("index")
            .to_string_lossy()
            .replace('\\', "/")
    ));
    let opts = PackOpts {
        repo_root: ctx.repo_root.clone(),
        collab_root: ctx.collab_root.clone(),
        brief,
        focus,
        budget,
        task: Some(ctx.task.as_str().to_string()),
        out: pack_stem(ctx).with_extension("pack.md"),
        max_file_size: 2 * 1024 * 1024,
        conn,
    };
    // (M11) federation peers requested by `--peer`/`--peers all`. `resolve_pack_peers` gates them
    // (`use_in_packs: true` AND named), refusing an unusable peer by name; with no peers requested
    // it returns an empty list and the pack is byte-for-byte the local one.
    let selection = crate::index::PeerSelection {
        all: ctx
            .o
            .peers
            .as_deref()
            .map(|v| v.eq_ignore_ascii_case("all"))
            .unwrap_or(false),
        names: ctx.o.peer.clone(),
    };
    let peers = reviewer::resolve_pack_peers(&ctx.repo_root, &selection)?;
    reviewer::build_with_peers(&opts, &peers)
}

/// The handoff stem (`handoffs/NN-http-<reply>`); `.pack.md` / `.pack.json` are appended by the
/// adapter. Reconstructed from the components so a `reply_name` with a dot is handled exactly as
/// `build_context` builds the reply path.
fn pack_stem(ctx: &Context) -> PathBuf {
    ctx.handoffs_dir.join(format!(
        "{:02}-{}-{}",
        ctx.nn, ctx.file_prefix, ctx.reply_name
    ))
}

/// The `TurnRequest` for the primary http turn. The pack (not this prompt) is what the reviewer
/// sees on a primary turn; the request still carries the resolved identity, the effort and the
/// timeout.
fn primary_turn(ctx: &Context) -> TurnRequest {
    TurnRequest {
        request: Request {
            prompt: ctx.prompt_text.clone(),
            brief_path: ctx.brief_path.clone(),
            model: ctx.identity.model.clone(),
            provider: ctx.identity.provider.clone(),
            engine: EngineKind::Http,
            effort: ctx.effort.sent.clone(),
            timeout_sec: ctx.r.timeout_sec as f64,
            mode: Mode::New,
            sandbox: String::new(),
            schema_path: None,
            extra_config: Vec::new(),
            output_last_message: None,
            prompt_file: None,
            max_model_steps: None,
            new_thread: None,
            add_dirs: Vec::new(),
        },
        consultation: ConsultationId(ctx.consult_id.clone()),
        attempt: AttemptId(ctx.consult_id.clone()),
        kind: TurnKind::Primary,
        continuation: None,
    }
}

/// Build the runtime engine for this seat (config + retained pack + handoff stem).
fn engine(ctx: &Context, seat: Seat, pack: ReviewerPack) -> HttpEngine {
    HttpEngine {
        config: seat.config,
        pack,
        handoff_stem: pack_stem(ctx),
    }
}

/// The plain inputs the two-turn flow needs from the [`Context`], so it is testable with a real
/// [`HttpEngine`] and no full orchestrator context.
struct SeatContext {
    repair_enabled: bool,
    continue_sec: i64,
    raw: bool,
    consult_id: String,
    /// The absolute `<stem>.original.md` path (the first prose kept before a format repair).
    original_md: PathBuf,
    /// `handoffs/<stem>.original.md` (repo-relative), for the ledger record.
    original_rel: String,
    /// `handoffs/<stem>.events.jsonl` (repo-relative), for the format-retry record.
    events_rel: String,
    transport: String,
}

/// Run the primary turn and, when warranted, ONE secondary turn — a format-repair replay
/// (`Continuation::Replay` with the convert-only prompt) or a timeout retry (the same request
/// resent). Returns the final outcome, the ledger `provider_config`, the merged warnings and the
/// secondary record. Split from [`run_seat`] so the flow is unit-testable against the fake server.
fn drive_seat_turns(
    eng: &HttpEngine,
    primary: TurnRequest,
    sc: &SeatContext,
) -> Result<(AttemptOutcome, Value, Vec<String>, Option<HttpSecondary>), String> {
    let first = eng
        .attempt(&primary)
        .map_err(|e| format!("the http request could not be planned: {e}"))?;
    let provider_config = first.provider_config.clone();
    let mut warnings = first.warnings.clone();
    let mut outcome = first.outcome;
    let mut secondary: Option<HttpSecondary> = None;

    // --- Format repair (replay): a substantive prose reply the normaliser could not structure.
    if let AttemptOutcome::Completed(reply) = &outcome {
        if reply.structured.is_none()
            && sc.repair_enabled
            && !sc.raw
            && crate::consult::ingest::prose_gate(&reply.raw_text).substantive
        {
            let prose = reply.raw_text.clone();
            // (F05-1) Persist the first reply byte for byte as <stem>.original.md BEFORE the repair
            // request. If it cannot be kept, send NO repair request: the first reply stays the
            // reply-of-record as prose and the run carries the reason.
            if let Err(e) = c3_core::store::write_text_atomic(&sc.original_md, prose.as_bytes()) {
                warnings.push(format!(
                    "format repair not attempted: the first reply could not be kept ({})",
                    e.kind()
                ));
            } else {
                let mut reason = crate::consult::ingest::first_validation_error(&prose);
                if reason.chars().count() > 200 {
                    reason = reason.chars().take(200).collect();
                }
                let mut repair_turn = primary.clone();
                repair_turn.request.prompt =
                    super::orchestrate::format_repair_prompt(&sc.consult_id);
                repair_turn.kind = TurnKind::FormatRepair;
                repair_turn.continuation = Some(Continuation::Replay {
                    pack_hash: String::new(),
                    prior_reply: prose.clone(),
                });
                let started = Instant::now();
                let repair = eng
                    .attempt(&repair_turn)
                    .map_err(|e| format!("the http repair request could not be planned: {e}"))?;
                let wall = (started.elapsed().as_secs_f64() * 10.0).round() / 10.0;
                warnings.extend(repair.warnings.clone());
                let repaired_ok = matches!(
                    &repair.outcome,
                    AttemptOutcome::Completed(r) if r.structured.is_some()
                );
                if repaired_ok {
                    // The repaired object (raw_text = repaired JSON) becomes the reply-of-record.
                    outcome = repair.outcome;
                }
                // else: the first prose stays the reply of record, exactly as today.
                secondary = Some(HttpSecondary {
                    engine_turns: 2,
                    repaired_ok,
                    repair_reason: reason.clone(),
                    original_rel: sc.original_rel.clone(),
                    original_prose: prose,
                    drift_notes: Vec::new(),
                    format_retry: Some(c3_core::ledger::FormatRetry {
                        attempted: true,
                        reason,
                        succeeded: repaired_ok,
                        thread: String::new(),
                        wall_seconds: wall,
                        events: Some(sc.events_rel.clone()),
                        schema_transport: sc.transport.clone(),
                        ..Default::default()
                    }),
                });
            }
        }
    }

    // --- Timeout retry: an `unavailable` failure (a request timeout, a 5xx, or an overloaded /
    // unavailable answer) is retried once, unless `--no-continue` (continue_sec 0). `auth`, `quota`
    // and `burst` are not retried (the class gate in `retry_pause`).
    if secondary.is_none() && sc.continue_sec > 0 {
        let (class, retry_after) = match &outcome {
            AttemptOutcome::TimedOut { .. } => (Some("unavailable".to_string()), None),
            AttemptOutcome::ProviderFailure { failure, .. } => {
                (Some(failure.class.clone()), failure.retry_after.clone())
            }
            _ => (None, None),
        };
        if let Some(class) = class {
            if let Some(pause) = crate::http_engine::retry_pause(&class, retry_after.as_deref()) {
                if !pause.is_zero() {
                    std::thread::sleep(pause);
                }
                let mut retry_turn = primary.clone();
                retry_turn.kind = TurnKind::TimeoutContinuation;
                retry_turn.continuation = None; // the same request, resent
                let retry = eng
                    .attempt(&retry_turn)
                    .map_err(|e| format!("the http retry request could not be planned: {e}"))?;
                warnings.extend(retry.warnings.clone());
                warnings.push(format!("retried once after {class}"));
                outcome = retry.outcome;
                secondary = Some(HttpSecondary {
                    engine_turns: 2,
                    ..Default::default()
                });
            }
        }
    }

    Ok((outcome, provider_config, warnings, secondary))
}

/// `handoffs/NN-http-<reply>.<ext>` as an absolute path (mirrors `Context::hpath`).
fn handoff_path(ctx: &Context, ext: &str) -> PathBuf {
    ctx.handoffs_dir.join(format!(
        "{:02}-{}-{}.{}",
        ctx.nn, ctx.file_prefix, ctx.reply_name, ext
    ))
}

/// `handoffs/NN-http-<reply>.<ext>` repo-relative (mirrors `Context::hf`).
fn handoff_rel(ctx: &Context, ext: &str) -> String {
    format!(
        "handoffs/{:02}-{}-{}.{}",
        ctx.nn, ctx.file_prefix, ctx.reply_name, ext
    )
}

/// Run one http seat: resolve the config, run the billing/key guard (before the pack is written),
/// build the pack, and send the request. On a substantive prose reply the normaliser could not
/// structure, ONE format-repair replay is sent; on an `unavailable` failure the request is retried
/// once (STEP 2). Returns the final outcome and the ledger's provider_config, or a refusal message
/// (surfaced by `run_primary_turn` as the seat's refusal, before any request).
pub(crate) fn run_seat(ctx: &Context) -> Result<SeatRun, String> {
    let seat = resolve_seat(ctx)?;
    billing_precheck(&seat)?;
    let budget = resolve_pack_budget(ctx, &seat);
    let (pack, peer_stats) = build_pack(ctx, budget)?;
    let eng = engine(ctx, seat, pack);

    let sc = SeatContext {
        repair_enabled: ctx.r.repair_enabled,
        continue_sec: ctx.r.continue_sec,
        raw: ctx.r.raw,
        consult_id: ctx.consult_id.clone(),
        original_md: handoff_path(ctx, "original.md"),
        original_rel: handoff_rel(ctx, "original.md"),
        events_rel: handoff_rel(ctx, "events.jsonl"),
        transport: ctx.transport.transport.clone(),
    };
    let (outcome, mut provider_config, warnings, secondary) =
        drive_seat_turns(&eng, primary_turn(ctx), &sc)?;
    // (M11) the ledger's `reviewer.provider_config` gains `peers: <count>` (an integer, never a
    // name) when federation peers were brought into the pack; the telemetry `peers_used` reads it.
    if !peer_stats.is_empty() {
        if let Value::Object(map) = &mut provider_config {
            map.insert(
                "peers".to_string(),
                Value::Number(serde_json::Number::from(peer_stats.len())),
            );
        }
    }

    let (bridge_outcome, reply_text) = match &outcome {
        AttemptOutcome::Completed(_) => ("usable reply".to_string(), String::new()),
        AttemptOutcome::ProviderFailure { failure, .. } => (
            format!(
                "failed: http {} - {}",
                failure.class,
                c3_core::one_line(&failure.message)
            ),
            String::new(),
        ),
        AttemptOutcome::TimedOut { .. } => (
            "failed: the http request timed out".to_string(),
            String::new(),
        ),
        AttemptOutcome::LaunchFailed { message, .. } => {
            (format!("failed: http - {message}"), String::new())
        }
        other => (format!("failed: {other:?}"), String::new()),
    };

    Ok(SeatRun {
        outcome,
        provider_config,
        bridge_outcome,
        reply_text,
        warnings,
        secondary,
    })
}

/// Render the `--dry-run` block for an http seat: the endpoint, the model, the key status, the
/// pack size (tokens and files) and the request plan with the `Authorization` header redacted. A
/// dry run makes no network call and writes nothing (the pack is built in memory only).
pub(crate) fn render_dry_run(ctx: &Context) {
    println!("DRY RUN - nothing was executed and no file was written.");
    println!();
    let task_dir = ctx.collab_root.join(ctx.task.as_str());
    println!("repo root   : {}", ctx.repo_root.display());
    println!("task dir    : {}", task_dir.display());
    let engine_from = if ctx.engine_from.is_empty() {
        "-Engine"
    } else {
        &ctx.engine_from
    };
    println!("engine      : http - HTTP (from {engine_from})");
    println!(
        "lineage     : {}",
        c3_core::lineage::format_reviewer_lineage(
            &ctx.identity.provider,
            &ctx.identity.model,
            "http"
        )
    );

    let seat = match resolve_seat(ctx) {
        Ok(s) => s,
        Err(e) => {
            println!("seat        : a real run is refused - {e}");
            return;
        }
    };
    println!("endpoint    : {}", seat.config.completions_url());
    println!("model       : {}", seat.config.model);

    // Build the engine (config + a placeholder pack is fine for the key status and the plan; the
    // real pack below fills the size line). The key value is never printed.
    let budget = resolve_pack_budget(ctx, &seat);
    let (plan_pack, plan_stats): (Result<ReviewerPack, String>, Vec<reviewer::PeerPackStat>) =
        match build_pack(ctx, budget) {
            Ok((p, s)) => (Ok(p), s),
            Err(e) => (Err(e), Vec::new()),
        };
    let eng = HttpEngine {
        config: seat.config.clone(),
        pack: match &plan_pack {
            Ok(p) => p.clone(),
            Err(_) => empty_pack(),
        },
        handoff_stem: pack_stem(ctx),
    };
    println!("key         : {}", eng.key_status());

    match &plan_pack {
        Ok(p) => println!(
            "pack        : {} tokens, {} focus file(s), {} periphery file(s) (budget {} tokens)",
            p.tokens,
            p.focus_files.len(),
            p.periphery_shown,
            budget
        ),
        Err(e) => println!("pack        : a real run is refused - {e}"),
    }
    // (M11) one line per federation peer brought into the pack, right after the pack line.
    for stat in &plan_stats {
        println!("{}", reviewer::peer_dry_run_line(stat));
    }
    // (S6) The billing guard verdict, else the per-token cost estimate (a dry run never refuses):
    // the pack (the user message) plus the reply-schema system message.
    if let Err(e) = billing_precheck(&seat) {
        println!("billing     : a real run is refused - {e}");
    } else if let Ok(p) = &plan_pack {
        let schema_tokens = reviewer::system_prompt().chars().count() / 4;
        println!(
            "billing     : per token - this request sends about {} tokens (pack {} + schema {}); a format repair or a retry sends them once more",
            p.tokens + schema_tokens,
            p.tokens,
            schema_tokens
        );
    }
    println!(
        "schema      : {}",
        if ctx.r.raw {
            "raw text (no schema)".to_string()
        } else {
            "consult-reply v1 (prompt-only + JSON mode)".to_string()
        }
    );
    println!("timeout     : {} s", ctx.r.timeout_sec);
    println!("consult id  : {}", ctx.consult_id);
    println!("handoff     : {:02}", ctx.nn);
    println!("reply file  : {}", ctx.reply_path.display());
    println!(
        "pack file   : {}",
        pack_stem(ctx).with_extension("pack.md").display()
    );
    println!();
    println!("request plan :");
    for line in eng.request_plan(&primary_turn(ctx)).to_string().lines() {
        println!("    {line}");
    }
}

/// An empty pack used only to render the request plan / key status when the real pack could not
/// be built (a dry run reports the pack error on its own line, never a panic).
fn empty_pack() -> ReviewerPack {
    ReviewerPack {
        content: String::new(),
        sidecar: String::new(),
        redactions: 0,
        tokens: 0,
        size_bytes: 0,
        focus_files: Vec::new(),
        periphery_shown: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(label: &str, key_env: &str, api_billing_accepted: bool) -> Seat {
        Seat {
            config: HttpConfig {
                base_url: DEFAULT_BASE_URL.to_string(),
                model: "openai/gpt-5".to_string(),
                key_env: key_env.to_string(),
                headers: Vec::new(),
                timeout: Duration::from_secs(60),
                provider_label: label.to_string(),
                json_object: true,
                repo_root: None,
            },
            api_billing_accepted,
            pack_tokens: -1,
        }
    }

    #[test]
    fn subscription_labels_are_refused_regardless_of_key() {
        // The key is present, but a subscription engine label is refused outright.
        std::env::set_var("C3_HTTP_BILL_SUB", "sk-or-v1-xxxxxxxxxxxxxxxx");
        for label in ["muse", "MUSE", "codex", "agy", "antigravity", "chatgpt"] {
            let err = billing_precheck(&seat(label, "C3_HTTP_BILL_SUB", false)).unwrap_err();
            assert!(err.contains("subscription engine"), "label {label}: {err}");
        }
        std::env::remove_var("C3_HTTP_BILL_SUB");
    }

    #[test]
    fn lab_labels_need_api_billing_accepted() {
        std::env::set_var("C3_HTTP_BILL_LAB", "sk-or-v1-xxxxxxxxxxxxxxxx");
        for label in ["openai", "gemini", "google", "meta", "OpenAI"] {
            let err = billing_precheck(&seat(label, "C3_HTTP_BILL_LAB", false)).unwrap_err();
            assert!(err.contains("bills per token"), "label {label}: {err}");
            assert!(err.contains("api_billing"), "names the override: {err}");
            // With api_billing accepted, the same lab passes.
            assert!(billing_precheck(&seat(label, "C3_HTTP_BILL_LAB", true)).is_ok());
        }
        std::env::remove_var("C3_HTTP_BILL_LAB");
    }

    #[test]
    fn concentrator_passes_and_missing_key_is_refused() {
        // A concentrator label (openrouter) is neither a subscription nor a lab: it passes when
        // the key is set and is refused (naming the variable, never a value) when it is not.
        std::env::remove_var("C3_HTTP_BILL_OR");
        let err = billing_precheck(&seat("openrouter", "C3_HTTP_BILL_OR", false)).unwrap_err();
        assert!(err.contains("C3_HTTP_BILL_OR not set"), "{err}");
        assert!(!err.contains("sk-or-"));
        std::env::set_var("C3_HTTP_BILL_OR", "sk-or-v1-xxxxxxxxxxxxxxxx");
        assert!(billing_precheck(&seat("openrouter", "C3_HTTP_BILL_OR", false)).is_ok());
        std::env::remove_var("C3_HTTP_BILL_OR");
    }

    #[test]
    fn proxy_auth_host_needs_no_key_and_others_still_do() {
        // A host that `C3_HTTP_AUTH_PROXY` lists passes without any key in the environment (the
        // proxy attaches it); the subscription guard still applies; an unlisted host still needs
        // its key.
        std::env::remove_var("C3_HTTP_BILL_PX");
        let mut listed = seat("openrouter", "C3_HTTP_BILL_PX", false);
        listed.config.base_url = "https://proxy-auth-seat.test/v1".to_string();
        let prev = std::env::var(crate::http_engine::AUTH_PROXY_ENV).ok();
        std::env::set_var(
            crate::http_engine::AUTH_PROXY_ENV,
            "other.test, proxy-auth-seat.test",
        );
        assert!(billing_precheck(&listed).is_ok());
        let mut sub = seat("muse", "C3_HTTP_BILL_PX", false);
        sub.config.base_url = "https://proxy-auth-seat.test/v1".to_string();
        assert!(billing_precheck(&sub)
            .unwrap_err()
            .contains("subscription engine"));
        let mut unlisted = seat("openrouter", "C3_HTTP_BILL_PX", false);
        unlisted.config.base_url = "https://keyed-seat.test/v1".to_string();
        let err = billing_precheck(&unlisted).unwrap_err();
        assert!(err.contains("C3_HTTP_BILL_PX not set"), "{err}");
        match prev {
            Some(v) => std::env::set_var(crate::http_engine::AUTH_PROXY_ENV, v),
            None => std::env::remove_var(crate::http_engine::AUTH_PROXY_ENV),
        }
    }

    // ------------------------------------------------------ STEP 2: format repair + timeout retry

    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    const FAKE_KEY: &str = "sk-or-v1-0123456789abcdef0123456789abcdef";

    /// A minimal OpenAI-compatible mock: answers each queued response in turn.
    fn start_mock(responses: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for resp in responses {
                let (mut stream, _) = match listener.accept() {
                    Ok(s) => s,
                    Err(_) => break,
                };
                read_request_body(&mut stream);
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });
        format!("http://{addr}")
    }

    fn read_request_body(stream: &mut TcpStream) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let l = line.trim_end();
            if l.is_empty() {
                break;
            }
            if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
        if content_length > 0 {
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
        }
    }

    fn http_response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
        let mut s = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (k, v) in headers {
            s.push_str(&format!("{k}: {v}\r\n"));
        }
        s.push_str("\r\n");
        s.push_str(body);
        s
    }

    fn completion(content: &str) -> String {
        let body = serde_json::json!({
            "id": "gen-1",
            "choices": [{ "message": { "role": "assistant", "content": content } }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 }
        })
        .to_string();
        http_response("200 OK", &[("content-type", "application/json")], &body)
    }

    const PROSE: &str = "Q1. The change looks consistent with the surrounding module and does not obviously regress existing behaviour, but the error path is untested and one edge case around empty input is not covered by the current suite so far as I can tell from reading the diff and the neighbouring tests today.";
    const VALID: &str = r#"{"schema_version":"1","verdict":"ACCEPT","verdict_reason":"ok","reply_markdown":"body","findings":[],"prior_findings":[],"unproven":[],"first_run_checklist":[]}"#;

    fn sample_pack() -> ReviewerPack {
        ReviewerPack {
            content: "# C3 reviewer pack\n\nReply as one JSON object.\n".to_string(),
            sidecar: r#"{"pack_version":1,"kind":"reviewer","files":[]}"#.to_string(),
            redactions: 0,
            tokens: 20,
            size_bytes: 60,
            focus_files: vec!["store.rs".to_string()],
            periphery_shown: 0,
        }
    }

    fn mk_engine(base_url: &str, key_env: &str, dir: &std::path::Path) -> HttpEngine {
        HttpEngine {
            config: HttpConfig {
                base_url: base_url.to_string(),
                model: "openai/gpt-5".to_string(),
                key_env: key_env.to_string(),
                headers: Vec::new(),
                timeout: Duration::from_millis(800),
                provider_label: "openrouter".to_string(),
                json_object: true,
                repo_root: None,
            },
            pack: sample_pack(),
            handoff_stem: dir.join("01-http-slug"),
        }
    }

    fn mk_primary() -> TurnRequest {
        TurnRequest {
            request: Request {
                prompt: "review; consultation id: C-1".to_string(),
                brief_path: None,
                model: "openai/gpt-5".to_string(),
                provider: "openrouter".to_string(),
                engine: EngineKind::Http,
                effort: None,
                timeout_sec: 0.8,
                mode: Mode::New,
                sandbox: String::new(),
                schema_path: None,
                extra_config: Vec::new(),
                output_last_message: None,
                prompt_file: None,
                max_model_steps: None,
                new_thread: None,
                add_dirs: Vec::new(),
            },
            consultation: ConsultationId("C-1".to_string()),
            attempt: AttemptId("C-1".to_string()),
            kind: TurnKind::Primary,
            continuation: None,
        }
    }

    fn mk_sc(dir: &std::path::Path, repair_enabled: bool, continue_sec: i64) -> SeatContext {
        SeatContext {
            repair_enabled,
            continue_sec,
            raw: false,
            consult_id: "C-1".to_string(),
            original_md: dir.join("01-http-slug.original.md"),
            original_rel: "handoffs/01-http-slug.original.md".to_string(),
            events_rel: "handoffs/01-http-slug.events.jsonl".to_string(),
            transport: "prompt-only".to_string(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("c3-seat-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn no_key_leak(dir: &std::path::Path) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let body = std::fs::read_to_string(e.path()).unwrap_or_default();
            assert!(
                !body.contains(FAKE_KEY),
                "seeded key leaked into {:?}",
                e.path()
            );
        }
    }

    #[test]
    fn format_repair_prose_then_valid_is_structured_with_two_turns() {
        std::env::set_var("C3_SEAT_REPAIR_OK", FAKE_KEY);
        let base = start_mock(vec![completion(PROSE), completion(VALID)]);
        let d = scratch("repair-ok");
        let eng = mk_engine(&base, "C3_SEAT_REPAIR_OK", &d);
        let (outcome, _pc, warnings, secondary) =
            drive_seat_turns(&eng, mk_primary(), &mk_sc(&d, true, 900)).unwrap();

        match &outcome {
            AttemptOutcome::Completed(reply) => {
                assert!(
                    reply.structured.is_some(),
                    "repair produced a structured reply"
                )
            }
            other => panic!("expected Completed, got {other:?}"),
        }
        let hs = secondary.expect("a secondary turn ran");
        assert_eq!(hs.engine_turns, 2);
        assert!(hs.repaired_ok);
        // The first prose is kept byte for byte as .original.md.
        let kept = std::fs::read_to_string(d.join("01-http-slug.original.md")).unwrap();
        assert_eq!(kept, PROSE);
        // The events file has two request/response pairs; the second is marked format-repair.
        let ev = std::fs::read_to_string(eng.events_path()).unwrap();
        assert_eq!(ev.matches("\"event\":\"request\"").count(), 2, "{ev}");
        assert!(ev.contains("\"turn\":\"format-repair\""), "{ev}");
        assert!(!warnings.iter().any(|w| w.contains("retried")));
        no_key_leak(&d);
        std::env::remove_var("C3_SEAT_REPAIR_OK");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn format_repair_second_invalid_keeps_the_prose() {
        std::env::set_var("C3_SEAT_REPAIR_BAD", FAKE_KEY);
        let base = start_mock(vec![
            completion(PROSE),
            completion("still just prose, no JSON"),
        ]);
        let d = scratch("repair-bad");
        let eng = mk_engine(&base, "C3_SEAT_REPAIR_BAD", &d);
        let (outcome, _pc, _w, secondary) =
            drive_seat_turns(&eng, mk_primary(), &mk_sc(&d, true, 900)).unwrap();

        match &outcome {
            AttemptOutcome::Completed(reply) => {
                assert!(reply.structured.is_none(), "stays prose");
                assert_eq!(
                    reply.raw_text, PROSE,
                    "the first prose is the reply of record"
                );
            }
            other => panic!("expected Completed prose, got {other:?}"),
        }
        let hs = secondary.expect("a repair was attempted");
        assert_eq!(hs.engine_turns, 2);
        assert!(!hs.repaired_ok);
        assert_eq!(
            std::fs::read_to_string(d.join("01-http-slug.original.md")).unwrap(),
            PROSE
        );
        no_key_leak(&d);
        std::env::remove_var("C3_SEAT_REPAIR_BAD");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn timeout_retry_503_then_200_is_usable_with_a_note() {
        std::env::set_var("C3_SEAT_RETRY", FAKE_KEY);
        // A 503 carrying Retry-After: 0 (so the test does not actually pause), then a 200.
        let base = start_mock(vec![
            http_response("503 Service Unavailable", &[("Retry-After", "0")], "down"),
            completion(VALID),
        ]);
        let d = scratch("retry");
        let eng = mk_engine(&base, "C3_SEAT_RETRY", &d);
        let (outcome, _pc, warnings, secondary) =
            drive_seat_turns(&eng, mk_primary(), &mk_sc(&d, true, 900)).unwrap();

        assert!(
            matches!(outcome, AttemptOutcome::Completed(_)),
            "usable after retry"
        );
        assert_eq!(secondary.unwrap().engine_turns, 2);
        assert!(
            warnings
                .iter()
                .any(|w| w == "retried once after unavailable"),
            "warnings: {warnings:?}"
        );
        let ev = std::fs::read_to_string(eng.events_path()).unwrap();
        assert!(ev.contains("\"turn\":\"retry\""), "{ev}");
        no_key_leak(&d);
        std::env::remove_var("C3_SEAT_RETRY");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn auth_401_is_not_retried() {
        std::env::set_var("C3_SEAT_401", FAKE_KEY);
        let base = start_mock(vec![http_response(
            "401 Unauthorized",
            &[("content-type", "application/json")],
            r#"{"error":{"message":"bad key"}}"#,
        )]);
        let d = scratch("noauth");
        let eng = mk_engine(&base, "C3_SEAT_401", &d);
        let (outcome, _pc, _w, secondary) =
            drive_seat_turns(&eng, mk_primary(), &mk_sc(&d, true, 900)).unwrap();
        match outcome {
            AttemptOutcome::ProviderFailure { failure, .. } => assert_eq!(failure.class, "auth"),
            other => panic!("expected auth failure, got {other:?}"),
        }
        assert!(secondary.is_none(), "auth is never retried");
        std::env::remove_var("C3_SEAT_401");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn format_repair_not_attempted_when_the_first_reply_cannot_be_kept() {
        // (F05-1) A directory occupying the .original.md path makes the write fail: NO repair
        // request is sent (the mock only ever answers once), the prose stays the reply-of-record,
        // and the run carries the reason.
        std::env::set_var("C3_SEAT_ORIGFAIL", FAKE_KEY);
        let base = start_mock(vec![completion(PROSE)]);
        let d = scratch("origfail");
        let eng = mk_engine(&base, "C3_SEAT_ORIGFAIL", &d);
        let sc = mk_sc(&d, true, 900);
        // Occupy the .original.md path with a directory so the atomic write cannot rename onto it.
        std::fs::create_dir_all(&sc.original_md).unwrap();

        let (outcome, _pc, warnings, secondary) =
            drive_seat_turns(&eng, mk_primary(), &sc).unwrap();
        match &outcome {
            AttemptOutcome::Completed(reply) => {
                assert!(
                    reply.structured.is_none(),
                    "the prose stays the reply-of-record"
                );
                assert_eq!(reply.raw_text, PROSE);
            }
            other => panic!("expected Completed prose, got {other:?}"),
        }
        assert!(secondary.is_none(), "no repair turn ran");
        assert!(
            warnings.iter().any(|w| w
                .starts_with("format repair not attempted: the first reply could not be kept (")),
            "warnings: {warnings:?}"
        );
        std::env::remove_var("C3_SEAT_ORIGFAIL");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_continue_suppresses_the_retry() {
        std::env::set_var("C3_SEAT_NOCONT", FAKE_KEY);
        let base = start_mock(vec![http_response(
            "503 Service Unavailable",
            &[("Retry-After", "0")],
            "down",
        )]);
        let d = scratch("nocont");
        let eng = mk_engine(&base, "C3_SEAT_NOCONT", &d);
        // continue_sec 0 == `--no-continue`.
        let (outcome, _pc, _w, secondary) =
            drive_seat_turns(&eng, mk_primary(), &mk_sc(&d, true, 0)).unwrap();
        assert!(matches!(outcome, AttemptOutcome::ProviderFailure { .. }));
        assert!(secondary.is_none(), "no retry when --no-continue");
        std::env::remove_var("C3_SEAT_NOCONT");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn without_peers_the_pack_is_byte_identical_to_the_local_build() {
        // (M11) `--peer`/`--peers` change nothing when absent: `build_with_peers(opts, &[])` is the
        // local `build(opts)` byte for byte (content + sidecar). Guards the http path's default.
        let d = std::env::temp_dir().join(format!("c3-http-peers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("brief.md"), "# Brief\n1. Check it.\n").unwrap();
        std::fs::write(d.join("store.rs"), "pub fn open() {}\npub fn close() {}\n").unwrap();
        let opts = PackOpts {
            repo_root: d.clone(),
            collab_root: d.join(".collab"),
            brief: std::path::PathBuf::from("brief.md"),
            focus: vec!["store.rs".into()],
            budget: 0,
            task: None,
            out: d.join("pack.md"),
            max_file_size: 2 * 1024 * 1024,
            conn: None,
        };
        let plain = reviewer::build(&opts).unwrap();
        let (with_none, stats) = reviewer::build_with_peers(&opts, &[]).unwrap();
        assert!(stats.is_empty(), "no peers -> no peer stats");
        // The pack CONTENT (what the reviewer sees) is byte-for-byte the local build; the sidecar is
        // identical too apart from its `generated` timestamp, so compare it with that line dropped.
        assert_eq!(plain.content, with_none.content, "pack content unchanged");
        assert_eq!(plain.tokens, with_none.tokens);
        assert_eq!(plain.periphery_shown, with_none.periphery_shown);
        let drop_ts = |s: &str| {
            s.lines()
                .filter(|l| !l.trim_start().starts_with("\"generated\":"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(
            drop_ts(&plain.sidecar),
            drop_ts(&with_none.sidecar),
            "pack sidecar unchanged (apart from the timestamp)"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
