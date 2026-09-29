//! Routing policies (M9): router v1 — a strict generalisation of the plugin's R15 smoothed
//! score with an optional global prior from a signed `priors.json` — the RC2 offline
//! simulation, the ledger replay and the `explain` table.
//!
//! One code path, the plugin's numbers by default: router v1 replaces only the *weight
//! source* of the seeded draw ([`crate::panel::routing`]), never the draw itself. With no
//! priors loaded and the default [`RouterParams`], [`score`] returns the same `f64` bits as
//! `panel::routing::routing_score` and [`routing_ext`] writes no `ext` key, so the ledger
//! record is byte-identical to the plugin's. The design decisions are made in `docs/DESIGN.md`
//! §5/§8/§11 and `docs/port/m9-spec.md`; this module executes them, fail-closed and offline.

pub mod download;
pub mod params;
pub mod priors;
pub mod replay;
pub mod score;
pub mod sign;
pub mod simulate;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::panel::routing::{routing_score, Rating, RoutingScore};

pub use params::RouterParams;
pub use priors::{PriorBasis, PriorHit, Priors, PriorsError};

/// A reviewer lineage `(provider, model, engine)`, the unit a score is computed for.
#[derive(Debug, Clone, Copy)]
pub struct Lineage<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub engine: &'a str,
}

/// The prior a position's score used, recorded in `ext.c3.router.prior_use[]`.
#[derive(Debug, Clone, Serialize)]
pub struct PriorUse {
    pub position: i64,
    pub prior_basis: &'static str,
    pub mean: f64,
    pub support: f64,
    pub m: f64,
}

/// A v1 score: the plugin's base record (with `score`/`basis` set to the v1 result) plus the
/// prior it used, if any.
#[derive(Debug, Clone)]
pub struct Scored {
    pub base: RoutingScore,
    pub prior: Option<PriorUse>,
}

/// The verified priors and parameters a draw runs with. Built by [`load_context`]; never
/// fails and never blocks.
#[derive(Debug, Clone, Default)]
pub struct RouterContext {
    pub params: RouterParams,
    pub priors: Option<Priors>,
    pub source: download::PriorsSource,
}

/// The v1 score for one lineage (M9 §2): the local basis exactly as R15, generalised with a
/// global prior mean `pi` and strength `m`. With no priors and the default params this is
/// `routing_score` bit for bit.
pub fn score(
    ratings: &[Rating],
    lineage: Lineage,
    purpose: &str,
    topics: &[String],
    now: DateTime<Utc>,
    ctx: &RouterContext,
) -> Scored {
    let mut base = routing_score(
        ratings,
        lineage.provider,
        lineage.model,
        lineage.engine,
        purpose,
        topics,
        now,
        false,
    );

    let hit = match &ctx.priors {
        // The prior lookup matches topic cells by the fixed vocabulary (M9 §7 D-b); a local free
        // slug outside the vocabulary has no topic cell and falls to the purpose cell. Local
        // rating evidence (routing_score above) still matches the raw slug exactly (as R15 does).
        Some(p) => {
            let prior_topics: Vec<String> = topics
                .iter()
                .filter_map(|t| crate::telemetry::topic_slug(t).map(|s| s.to_string()))
                .collect();
            p.lookup(
                lineage.provider,
                lineage.model,
                lineage.engine,
                purpose,
                &prior_topics,
            )
        }
        None => PriorHit::neutral(),
    };
    let m = ctx.params.prior_strength(hit.support);
    base.score = score::smoothed_score(base.yes, base.partly, base.ratings, hit.mean, m);
    // No local evidence: the basis becomes `prior` when the prior came from a cell.
    if base.basis == "neutral" && hit.basis != PriorBasis::None {
        base.basis = "prior".into();
    }

    let prior = if ctx.priors.is_some() || !ctx.params.is_default() {
        Some(PriorUse {
            position: 0,
            prior_basis: hit.basis.as_str(),
            mean: hit.mean,
            support: hit.support,
            m,
        })
    } else {
        None
    };
    Scored { base, prior }
}

/// C3's additions under one trailing key of `panel.routing`: `ext.c3.router` (M9 §2). Returns
/// `None` — write no `ext` key — when no priors are loaded and the params are the R15
/// defaults, so the record stays byte-identical to the plugin's.
pub fn routing_ext(ctx: &RouterContext, uses: &[PriorUse]) -> Option<serde_json::Value> {
    if ctx.priors.is_none() && ctx.params.is_default() {
        return None;
    }
    let mut router = serde_json::json!({
        "router": "v1",
        "params": ctx.params,
        "prior_use": uses,
    });
    if ctx.priors.is_some() {
        router["priors"] = serde_json::json!({
            "version": ctx.source.version,
            "generated": ctx.source.generated,
            "sha256": ctx.source.sha256,
            "source": ctx.source.source,
            "key_id": ctx.source.key_id,
        });
    }
    Some(serde_json::json!({ "c3": { "router": router } }))
}

/// Build the routing context: the default params plus the verified priors from the cache, or
/// none. Never fetches, never blocks, never fails (M9 §8).
pub fn load_context() -> RouterContext {
    let params = RouterParams::default();
    match download::load_cached(&download::priors_dir(), Utc::now()) {
        Some((priors, source)) => RouterContext {
            params,
            priors: Some(priors),
            source,
        },
        None => RouterContext {
            params,
            priors: None,
            source: download::PriorsSource {
                source: "none",
                ..Default::default()
            },
        },
    }
}

/// Refresh the priors cache once, where the telemetry flush runs (M9 §3). Ignores every
/// failure and never blocks a consultation; a disabling switch skips the fetch entirely.
pub fn maybe_refresh_priors() {
    let outcome = download::refresh(&download::priors_dir(), Utc::now(), &download::UreqFetcher);
    crate::telemetry::debug_log(&format!("priors refresh: {outcome:?}"));
}
