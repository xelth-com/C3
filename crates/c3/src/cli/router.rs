//! `c3 router` (milestone 9): the routing subcommands — `simulate` (RC2), `replay`,
//! `priors` (status/fetch/clear) and `explain`. The logic lives in [`crate::router`]; this
//! file is only the clap surface and the thin glue, following the neighbouring style.

use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};

use crate::router::{self, download, Lineage};

/// Arguments of `c3 router <action>`.
#[derive(Args, Debug)]
pub struct RouterArgs {
    #[command(subcommand)]
    pub action: RouterAction,
}

/// The `c3 router` actions.
#[derive(Subcommand, Debug)]
pub enum RouterAction {
    /// Run the offline RC2 routing simulation (deterministic, no network).
    Simulate(SimulateArgs),
    /// Replay a routed panel from the ledger and confirm the seats match.
    Replay(ReplayArgs),
    /// Priors cache: status, fetch (once per 24 h) or clear.
    Priors(PriorsArgs),
    /// The score table the next draw would use, one line per roster lineage.
    Explain(ExplainArgs),
}

#[derive(Args, Debug, Default)]
pub struct SimulateArgs {
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    #[arg(long, default_value_t = 200)]
    pub runs: u64,
    #[arg(long, default_value_t = 300)]
    pub rounds: u64,
    /// Write the report to a file instead of stdout.
    #[arg(long)]
    pub out: Option<String>,
    /// Emit JSON instead of the markdown report.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Default)]
pub struct ReplayArgs {
    /// The task (its directory under `<collab-dir>`).
    #[arg(long)]
    pub task: String,
    /// Replay only this consult number; default replays every distinct routed panel.
    #[arg(long)]
    pub nn: Option<i64>,
    /// Where consultations are stored. Relative paths resolve against the git repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
}

#[derive(Args, Debug)]
pub struct PriorsArgs {
    #[command(subcommand)]
    pub action: PriorsAction,
}

#[derive(Subcommand, Debug)]
pub enum PriorsAction {
    /// Print source, version, generated, age, cell count, key id, and why priors are off.
    Status,
    /// Fetch once now (respects the 24 h cadence and every switch).
    Fetch,
    /// Remove the cached priors.
    Clear,
}

#[derive(Args, Debug, Default)]
pub struct ExplainArgs {
    /// The consult purpose.
    #[arg(long)]
    pub purpose: String,
    /// A topic tag; repeatable.
    #[arg(long)]
    pub topic: Vec<String>,
    /// Where consultations are stored (for the ratings). Relative paths resolve against root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    /// The order the panel would seat in: `routed` (the panel's default - it falls back to the
    /// roster order without ratings) or `roster`, as `c3 consult --panel-order`.
    #[arg(long, default_value = "")]
    pub panel_order: String,
}

/// Dispatch `c3 router`.
pub fn run(args: RouterArgs) -> i32 {
    match args.action {
        RouterAction::Simulate(a) => router::simulate::run(&router::simulate::SimOptions {
            seed: a.seed,
            runs: a.runs,
            rounds: a.rounds,
            json: a.json,
            out: a.out,
        }),
        RouterAction::Replay(a) => router::replay::run(&a.collab_dir, &a.task, a.nn),
        RouterAction::Priors(a) => run_priors(a.action),
        RouterAction::Explain(a) => run_explain(a),
    }
}

fn run_priors(action: PriorsAction) -> i32 {
    match action {
        PriorsAction::Status => {
            match download::resolve_mode() {
                download::Mode::Off => {
                    if crate::telemetry::env_off() {
                        println!("priors: off (CODEX_CONSULT_TELEMETRY=off)");
                    } else {
                        println!("priors: off (C3_PRIORS=off)");
                    }
                    return 0;
                }
                download::Mode::Refused(u) => {
                    println!("priors: off (only https URLs are allowed; got '{u}')");
                    return 0;
                }
                download::Mode::File(p) => println!("priors source: file {}", p.display()),
                download::Mode::Hub(u) => println!("priors source: hub {u}"),
            }
            match download::load_cached(&download::priors_dir(), Utc::now()) {
                Some((priors, src)) => {
                    println!("verified: yes ({})", src.source);
                    println!("version: {}", src.version);
                    println!(
                        "generated: {}",
                        if src.generated.is_empty() {
                            "-"
                        } else {
                            &src.generated
                        }
                    );
                    if !src.key_id.is_empty() {
                        println!("key id: {}", src.key_id);
                    }
                    println!("cells: {}", priors.cells.len());
                    if let Some(m) = read_meta_age() {
                        println!("age: {m}");
                    }
                }
                None => println!(
                    "verified priors: none cached (a 404 from the hub is the normal answer today)"
                ),
            }
            0
        }
        PriorsAction::Fetch => {
            let outcome =
                download::refresh(&download::priors_dir(), Utc::now(), &download::UreqFetcher);
            println!("priors fetch: {outcome:?}");
            0
        }
        PriorsAction::Clear => {
            let dir = download::priors_dir();
            let mut removed = 0;
            for f in ["priors.json", "priors.json.sig", "priors.meta.json"] {
                if std::fs::remove_file(dir.join(f)).is_ok() {
                    removed += 1;
                }
            }
            println!("priors cleared: {removed} file(s) removed");
            0
        }
    }
}

/// The cached copy's age, from `priors.meta.json`'s `fetched`.
fn read_meta_age() -> Option<String> {
    let bytes = std::fs::read(download::priors_dir().join("priors.meta.json")).ok()?;
    let meta: download::Meta = serde_json::from_slice(&bytes).ok()?;
    let t = DateTime::parse_from_rfc3339(&meta.fetched).ok()?;
    let age = Utc::now().signed_duration_since(t.with_timezone(&Utc));
    Some(if age.num_days() > 0 {
        format!("{} day(s)", age.num_days())
    } else {
        format!("{} hour(s)", age.num_hours().max(0))
    })
}

/// (0.2.5 D1, F30-1 / F31-6) What `c3 router explain` knows of an entry's availability without
/// starting anything: the verdict its RECORDED health gives (an identity error, an auth failure,
/// a usage limit, a model the login does not offer, a plan cap or a plan quota), or - when no
/// record says anything about it - that its availability was not checked. A credential is never
/// probed (no `login status`, no `auth status`, no key lookup): the probe is the panel's, at its
/// start.
#[derive(Debug, Clone, PartialEq)]
enum RecordedAvailability {
    /// The records show it available (a usable reply, no failure since).
    Available,
    /// The records show it unavailable: the reason, as the panel's preflight words it.
    Unavailable(String),
    /// No record says anything: availability not checked.
    NotChecked,
}

/// (0.2.5 D1, F30-1 / F31-6) [`RecordedAvailability`] from the identity and the recorded health of
/// one entry - a pure function of the records: the panel's own verdict rules
/// ([`c3_core::verdict`]) with the credential taken as not probed.
fn recorded_availability(
    id: &c3_core::lineage::ReviewerIdentity,
    health: Option<&c3_core::health::EndpointHealth>,
) -> RecordedAvailability {
    if let Some(v) = c3_core::verdict::verdict_pre_credential(id) {
        return if v.state == "unavailable" {
            RecordedAvailability::Unavailable(v.reason)
        } else {
            RecordedAvailability::NotChecked
        };
    }
    let v = c3_core::verdict::verdict_with_credential(
        id,
        health,
        c3_core::credential::CredentialResult::new(
            c3_core::credential::State::Ok,
            "availability not checked",
        ),
        true,
    );
    if v.state != "available" {
        return RecordedAvailability::Unavailable(v.reason);
    }
    match health {
        Some(h) if h.recent_usable.is_some() => RecordedAvailability::Available,
        _ => RecordedAvailability::NotChecked,
    }
}

/// (0.2.5 R4, F16-6 / F17-2) The seat marks of the explain table, by position: the panel's own
/// member election (`Select-PanelMembers`: refusals, the weighty / light gates, the reserve hold -
/// no brief, so no context-window skip) and the roster's required entries for the purpose, judged
/// by the panel's own preparation and the draw's own pools ([`crate::panel::plan::lab_seat_marks`])
/// for a panel of the purpose's default size.
///
/// (0.2.5 D1, F30-1 / F31-6) It starts no process and uses no network: the election runs without
/// its credential preflight and with cache-only budget windows, availability comes from the
/// recorded health only ([`recorded_availability`]: an entry the records show unavailable is not
/// eligible; one they say nothing about stays eligible and is marked `availability not checked`),
/// and the seats are judged in the order the real panel would use (`--panel-order`, default
/// routed with the panel's "no ratings" fallback to the roster order) - which one is returned.
fn seat_marks(
    roster: &c3_core::roster::Roster,
    ratings: &[crate::panel::routing::Rating],
    args: &ExplainArgs,
    ctx: &router::RouterContext,
    collab_root: &std::path::Path,
    now: DateTime<Utc>,
) -> crate::panel::plan::SeatMarks {
    let launcher = crate::providers::resolve_codex_launcher("").unwrap_or_default();
    let consults = crate::providers::read_all_task_consults_health(collab_root);
    let mut pctx = crate::providers::Ctx::for_consult(
        crate::providers::read_codex_config(&crate::providers::get_codex_config_path()),
        consults.clone(),
        roster.clone(),
        launcher,
        std::env::var("OPENAI_BASE_URL").unwrap_or_default(),
        now,
    );
    pctx.no_network = true;
    let mut selection = pctx.panel_members("", "", &args.purpose, false, true, 0, false);
    let mut not_checked: Vec<i64> = Vec::new();
    for m in selection.members.iter_mut().filter(|m| m.state == "run") {
        let health = m
            .identity
            .resolved
            .then(|| c3_core::health::endpoint_health(&consults, &m.identity.fingerprint, now));
        let recorded = match recorded_availability(&m.identity, health.as_ref()) {
            // a plan cap or a plan quota of the entry's plan, from the records too
            RecordedAvailability::Unavailable(r) => RecordedAvailability::Unavailable(r),
            other => {
                let mut v = c3_core::verdict::PreflightVerdict::available();
                v = pctx.plan_verdict(v, &m.entry, &m.identity, true);
                if v.state == "unavailable" {
                    RecordedAvailability::Unavailable(v.reason)
                } else {
                    other
                }
            }
        };
        match recorded {
            RecordedAvailability::Unavailable(reason) => {
                m.state = "skipped".into();
                m.reason = reason;
                m.skip_kind = "unavailable".into();
            }
            RecordedAvailability::NotChecked => not_checked.push(m.entry.position as i64),
            RecordedAvailability::Available => {}
        }
    }
    let members = crate::panel::run::member_inputs(&selection, now, false);
    let required =
        crate::panel::plan::resolve_required_reviewers(roster, &[], &args.purpose, false, true);
    let order = if args.panel_order.trim().is_empty() {
        "routed".to_string()
    } else {
        args.panel_order.trim().to_lowercase()
    };
    let mut out = crate::panel::plan::lab_seat_marks(
        ctx,
        &crate::panel::plan::PanelRoutingInput {
            members: &members,
            size: crate::panel::plan::default_size(&args.purpose),
            size_source: "purpose",
            order: &order,
            ratings,
            purpose: &args.purpose,
            topics: &args.topic,
            utc_now: now,
            task: "",
            brief_sha: "",
            nonce: "",
            nonce_source: "",
            required: &required.positions,
        },
    );
    for p in not_checked {
        out.marks.push((p, "availability not checked".to_string()));
    }
    out
}

/// (0.2.5 D1, F31-6) The line that says in which order `explain` judged the seats.
fn seat_order_line(marks: &crate::panel::plan::SeatMarks, asked: &str) -> String {
    let asked = if asked.trim().is_empty() {
        "routed (the panel's default)".to_string()
    } else {
        format!("{} (--panel-order)", asked.trim().to_lowercase())
    };
    if marks.mode == "roster" && !marks.fallback.is_empty() {
        format!(
            "seats: judged in roster order - order asked {asked}, fallen back ({}) as the panel would",
            marks.fallback
        )
    } else {
        format!(
            "seats: judged in {} order - order asked {asked}",
            marks.mode
        )
    }
}

fn run_explain(args: ExplainArgs) -> i32 {
    if args.purpose.trim().is_empty() {
        eprintln!("c3 router explain: --purpose is required");
        return 2;
    }
    let order = args.panel_order.trim().to_lowercase();
    if !order.is_empty() && order != "routed" && order != "roster" {
        eprintln!("c3 router explain: --panel-order must be roster or routed (got '{order}')");
        return 2;
    }
    let roster = match crate::providers::read_reviewer_roster() {
        Ok(r) if r.exists && r.error.is_empty() => r,
        Ok(r) if !r.error.is_empty() => {
            eprintln!("c3 router explain: {}", r.error);
            return 2;
        }
        Ok(_) => {
            eprintln!("c3 router explain: no reviewer roster found");
            return 2;
        }
        Err(e) => {
            eprintln!("c3 router explain: {e}");
            return 2;
        }
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let repo_root = crate::providers::resolve_repo_root(&cwd);
    let collab_root = crate::providers::resolve_collab_root(&repo_root, &args.collab_dir);
    let ratings = crate::panel::routing::read_all_task_ratings(&collab_root);
    let ctx = router::load_context();
    let now = Utc::now();

    // (wave 9) budget-aware routing: each plan group's multiplier and governing window. (0.2.5 D1,
    // F30-1) From the cached windows only: a display command makes no network call.
    let active: Vec<c3_core::roster::RosterEntry> = roster.active_entries().cloned().collect();
    let budget_mults = crate::budget::multipliers_for(&active, now, false);
    // the governing-window cell and multiplier per budget key (recomputed once per key)
    let mut budget_cells: std::collections::HashMap<String, (f64, String)> =
        std::collections::HashMap::new();
    for e in &active {
        let key = crate::budget::budget_key(e);
        if key.is_empty() || budget_cells.contains_key(&key) {
            continue;
        }
        let group: Vec<c3_core::roster::RosterEntry> = active
            .iter()
            .filter(|x| crate::budget::budget_key(x) == key)
            .cloned()
            .collect();
        let plan_arg = if key == "openai-account" {
            ""
        } else {
            e.plan.as_str()
        };
        let windows = crate::budget::plan_windows(plan_arg, &group, now, false);
        let (m, gov) = crate::budget::budget_multiplier(&windows);
        budget_cells.insert(key, (m, crate::budget::explain_budget_cell(&windows, &gov)));
    }

    println!("priors: {} (v{})", ctx.source.source, ctx.source.version);
    println!(
        "{:<40} {:>6} {:>7} {:>5} {:<16} {:>16} {:>10} prior(mean/support/m)",
        "lineage", "score", "weight", "mult", "budget", "basis", "local n"
    );
    // (0.2.5, wave C1; R4) one seat per lab: which entry holds each lab's seat, judged as the panel
    // judges it.
    let marks = seat_marks(&roster, &ratings, &args, &ctx, &collab_root, now);
    println!("{}", seat_order_line(&marks, &args.panel_order));
    // (ext.c3.inactive) an inactive entry is not a routing candidate.
    for e in roster.active_entries() {
        let engine = if e.engine.is_empty() {
            "codex"
        } else {
            &e.engine
        };
        let lineage = c3_core::lineage::format_reviewer_lineage(&e.provider, &e.model, engine);
        let scored = router::score(
            &ratings,
            Lineage {
                provider: &e.provider,
                model: &e.model,
                engine,
            },
            &args.purpose,
            &args.topic,
            now,
            &ctx,
        );
        let prior = scored
            .prior
            .map(|p| format!("{}/{}/{:.1}/{:.2}", p.prior_basis, p.support, p.m, p.mean))
            .unwrap_or_else(|| "-".to_string());
        let key = crate::budget::budget_key(e);
        let mult = crate::budget::effective_multiplier(
            scored.base.score,
            crate::panel::routing::ROUTING_NEUTRAL,
            budget_mults.get(&key).copied().unwrap_or(1.0),
        );
        let budget_cell = budget_cells
            .get(&key)
            .map(|(_, cell)| cell.clone())
            .unwrap_or_else(|| "-".to_string());
        let weight = scored.base.score * mult;
        // (0.2.4, reviewer-protection) mark an on-request entry and a protected entry held by its
        // reserve on this purpose (the routed draw would skip both).
        let mut tail = prior;
        if e.on_request {
            tail.push_str("  [on request]");
        }
        if !c3_core::purpose::is_standing(&args.purpose)
            && e.reserve_percent > 0
            && crate::budget::entry_reserve_hold(&roster, e, now, false).is_some()
        {
            tail.push_str("  [held: reserve]");
        }
        for (_, mark) in marks.marks.iter().filter(|(p, _)| *p == e.position as i64) {
            tail.push_str(&format!("  [{mark}]"));
        }
        println!(
            "{:<40} {:>6.3} {:>7.3} {:>5.2} {:<16} {:>16} {:>10.2} {}",
            lineage,
            scored.base.score,
            weight,
            mult,
            budget_cell,
            scored.base.basis,
            scored.base.ratings,
            tail
        );
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> c3_core::lineage::ReviewerIdentity {
        let cfg = c3_core::config::scan_config_text("", "model = \"gpt-5.1\"\n");
        c3_core::lineage::resolve_reviewer_identity(&cfg, "openai", "gpt-5.1", "", "codex", "")
    }

    fn consult(
        fp: &str,
        when: &str,
        outcome: &str,
        failure: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "n": 1, "when": when, "finished_at": when, "bridge_outcome": outcome,
            "reviewer": {"provider": "openai", "model": "gpt-5.1", "provider_fingerprint": fp},
            "provider_failure": failure
        })
    }

    // (0.2.5 D1, F30-1 / F31-6) explain reads availability from the recorded health only: no record
    // is "availability not checked" (never a probe), a usable reply is available, a recorded auth
    // failure makes the entry unavailable with the panel's own reason, and an identity that does
    // not resolve is not checked either.
    #[test]
    fn explain_takes_availability_from_the_recorded_health_only() {
        let id = identity();
        assert!(id.resolved, "{:?}", id.note);
        let now = chrono::Utc::now();
        let when = (now - chrono::Duration::minutes(5)).to_rfc3339();
        assert_eq!(
            recorded_availability(&id, None),
            RecordedAvailability::NotChecked
        );
        let empty = c3_core::health::endpoint_health(&[], &id.fingerprint, now);
        assert_eq!(
            recorded_availability(&id, Some(&empty)),
            RecordedAvailability::NotChecked
        );
        let ok = vec![consult(
            &id.fingerprint,
            &when,
            "usable reply",
            serde_json::Value::Null,
        )];
        let usable = c3_core::health::endpoint_health(&ok, &id.fingerprint, now);
        assert_eq!(
            recorded_availability(&id, Some(&usable)),
            RecordedAvailability::Available
        );
        let denied = vec![consult(
            &id.fingerprint,
            &when,
            "failed: auth (401 unauthorized)",
            serde_json::json!({"class": "auth", "code": "401", "message": "401 unauthorized", "when": when, "retry_after": null}),
        )];
        let auth = c3_core::health::endpoint_health(&denied, &id.fingerprint, now);
        match recorded_availability(&id, Some(&auth)) {
            RecordedAvailability::Unavailable(r) => assert!(r.starts_with("auth failed"), "{r}"),
            other => panic!("{other:?}"),
        }
        let mut unresolved = identity();
        unresolved.resolved = false;
        assert_eq!(
            recorded_availability(&unresolved, None),
            RecordedAvailability::NotChecked
        );
    }

    // (0.2.5 D1, F31-6) explain says in which order it judged the seats: the panel's routed
    // default, its "no ratings" fallback to the roster order, or the order asked for.
    #[test]
    fn explain_says_in_which_order_it_judged_the_seats() {
        let m = |mode: &str, fallback: &str| crate::panel::plan::SeatMarks {
            marks: Vec::new(),
            mode: mode.into(),
            fallback: fallback.into(),
        };
        assert_eq!(
            seat_order_line(&m("routed", ""), ""),
            "seats: judged in routed order - order asked routed (the panel's default)"
        );
        assert_eq!(
            seat_order_line(&m("roster", "no ratings"), ""),
            "seats: judged in roster order - order asked routed (the panel's default), fallen back (no ratings) as the panel would"
        );
        assert_eq!(
            seat_order_line(&m("roster", ""), "Roster"),
            "seats: judged in roster order - order asked roster (--panel-order)"
        );
    }
}
