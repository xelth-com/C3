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
use std::time::Duration;

use c3_core::engine::{
    AttemptId, AttemptOutcome, ConsultationId, EngineKind, Mode, Request, TurnKind, TurnRequest,
};
use serde_json::Value;

use crate::http_engine::{HttpConfig, HttpEngine, DEFAULT_BASE_URL, DEFAULT_KEY_ENV};
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
fn build_pack(ctx: &Context, budget: usize) -> Result<ReviewerPack, String> {
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
    reviewer::build(&opts)
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

/// Run one http seat: resolve the config, run the billing/key guard (before the pack is written),
/// build the pack, and send the request. Returns the outcome and the ledger's provider_config, or
/// a refusal message (surfaced by `run_primary_turn` as the seat's refusal, before any request).
pub(crate) fn run_seat(ctx: &Context) -> Result<SeatRun, String> {
    let seat = resolve_seat(ctx)?;
    billing_precheck(&seat)?;
    let budget = resolve_pack_budget(ctx, &seat);
    let pack = build_pack(ctx, budget)?;
    let eng = engine(ctx, seat, pack);
    let turn = primary_turn(ctx);
    let att = eng
        .attempt(&turn)
        .map_err(|e| format!("the http request could not be planned: {e}"))?;

    let (bridge_outcome, reply_text) = match &att.outcome {
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
        outcome: att.outcome,
        provider_config: att.provider_config,
        bridge_outcome,
        reply_text,
        warnings: att.warnings,
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
    let plan_pack = build_pack(ctx, budget);
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
    // (S6) The billing guard verdict, else the per-token cost estimate (a dry run never refuses):
    // the pack (the user message) plus the reply-schema system message.
    if let Err(e) = billing_precheck(&seat) {
        println!("billing     : a real run is refused - {e}");
    } else if let Ok(p) = &plan_pack {
        let schema_tokens = reviewer::system_prompt().chars().count() / 4;
        println!(
            "billing     : per token - this request sends about {} tokens (pack {} + schema {})",
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
}
