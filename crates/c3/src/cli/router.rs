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

fn run_explain(args: ExplainArgs) -> i32 {
    if args.purpose.trim().is_empty() {
        eprintln!("c3 router explain: --purpose is required");
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

    println!("priors: {} (v{})", ctx.source.source, ctx.source.version);
    println!(
        "{:<40} {:>6} {:>16} {:>10} prior(mean/support/m)",
        "lineage", "score", "basis", "local n"
    );
    for e in &roster.entries {
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
        println!(
            "{:<40} {:>6.3} {:>16} {:>10.2} {}",
            lineage, scored.base.score, scored.base.basis, scored.base.ratings, prior
        );
    }
    0
}
