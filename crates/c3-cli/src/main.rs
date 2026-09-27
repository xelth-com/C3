//! C3 — claude-codex-consult in Rust.
//!
//! The consultation bridge: a coordinator (Claude Code, Codex CLI, any shell)
//! asks a roster of read-only reviewers from different labs to look at a
//! one-page brief, records the consultation as files next to the code (the
//! brief, the reply verbatim, a JSON ledger, findings tracked by id) and can
//! run several reviewers as a panel.
//!
//! This is the Rust rewrite of the PowerShell plugin `codex-consult`
//! (github.com/xelth-com/claude-codex-consult), made from a finished copy of
//! it. The plugin stays and is the working implementation until this binary
//! reaches parity; the design of record is `docs/DESIGN.md`, and its milestone
//! table lists the port order. At milestone 1 only `c3 providers` is functional.

use clap::{Parser, Subcommand};

use c3::providers;

#[derive(Parser)]
#[command(
    name = "c3",
    version,
    about = "C3 — claude-codex-consult in Rust",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
// The `Consult`/`Findings` variants carry the filled clap arg structs (many flags), which
// are larger than the unit variants; boxing a clap `Args` variant is awkward, so the size
// spread is accepted here rather than in the dispatch.
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// List the Codex model providers and whether each one is usable right now.
    Providers(ProvidersArgs),
    /// Ask one reviewer for a one-page consultation (milestone 2).
    Consult(c3::cli::consult::ConsultArgs),
    /// Run several reviewers on one brief as a panel (milestone 4).
    Panel,
    /// List, move and rate tracked findings (milestone 3).
    Findings(c3::cli::findings::FindingsArgs),
    /// Per-reviewer usefulness scoreboard (milestone 3).
    Scoreboard(c3::cli::scoreboard::ScoreboardArgs),
    /// The one-line SessionStart availability summary (milestone 3).
    Hook(c3::cli::hook::HookArgs),
    /// File a complaint to the maintainer's intake, with a public reference (milestone 5).
    Complain(c3::cli::telemetry::ComplainArgs),
    /// Delete every event and complaint this installation ever sent (milestone 5).
    ForgetMe(c3::cli::telemetry::ForgetMeArgs),
    /// Telemetry on/off status and the instance id (milestone 5).
    Telemetry(c3::cli::telemetry::TelemetryArgs),
    /// Build a reviewer pack for the http engine, with a `.pack.json` sidecar (milestone 7).
    Pack(c3::cli::pack::PackArgs),
    /// Build an explainer pack for one claim (milestone 7).
    Explain(c3::cli::explain::ExplainArgs),
    /// Take a repository snapshot, optionally as a git delta (milestone 7).
    Snapshot(c3::cli::snapshot::SnapshotArgs),
    /// Build or query the code index (milestone 8).
    Index,
    /// Run the stdio MCP server (milestone 10).
    Mcp,
}

#[derive(Parser)]
struct ProvidersArgs {
    /// Report this provider only (case-sensitive, as in the config); sets the exit code.
    #[arg(long)]
    provider: Option<String>,
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    collab_dir: String,
    /// Emit an array of objects instead of the table.
    #[arg(long)]
    json: bool,
    /// Explicit path to the codex launcher (env override: CODEX_CONSULT_EXE).
    #[arg(long)]
    codex_exe: Option<String>,
    /// Explicit path to a non-codex engine launcher (env: CODEX_CONSULT_AGY_EXE / _MUSE_EXE).
    #[arg(long)]
    engine_exe: Option<String>,
    /// No network call at all (an agy sign-in reads "not checked"; muse stays local).
    #[arg(long)]
    no_network: bool,
    /// One line: what is OUT, per roster entry, and how many reviewers are available.
    #[arg(long)]
    short: bool,
}

fn stub(name: &str, milestone: u8) -> i32 {
    eprintln!("c3 {name}: not implemented yet (milestone {milestone})");
    2
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Some(Commands::Providers(a)) => providers::run(providers::Options {
            provider: a.provider.unwrap_or_default(),
            collab_dir: a.collab_dir,
            json: a.json,
            codex_exe: a.codex_exe.unwrap_or_default(),
            engine_exe: a.engine_exe.unwrap_or_default(),
            no_network: a.no_network,
            short: a.short,
        }),
        Some(Commands::Consult(a)) => c3::cli::consult::run(a),
        Some(Commands::Panel) => stub("panel", 4),
        Some(Commands::Findings(a)) => c3::cli::findings::run(a),
        Some(Commands::Scoreboard(a)) => c3::cli::scoreboard::run(a),
        Some(Commands::Hook(a)) => c3::cli::hook::run(a),
        Some(Commands::Complain(a)) => c3::cli::telemetry::run_complain(a),
        Some(Commands::ForgetMe(a)) => c3::cli::telemetry::run_forget_me(a),
        Some(Commands::Telemetry(a)) => c3::cli::telemetry::run_telemetry(a),
        Some(Commands::Pack(a)) => c3::cli::pack::run(a),
        Some(Commands::Explain(a)) => c3::cli::explain::run(a),
        Some(Commands::Snapshot(a)) => c3::cli::snapshot::run(a),
        Some(Commands::Index) => stub("index", 8),
        Some(Commands::Mcp) => stub("mcp", 10),
        None => {
            // No subcommand: print help (clap prints to stderr on error; here to stdout).
            use clap::CommandFactory;
            Cli::command().print_help().ok();
            println!();
            0
        }
    };
    std::process::exit(code);
}
