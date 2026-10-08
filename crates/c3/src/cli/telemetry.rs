//! `c3 complain`, `c3 forget-me` and `c3 telemetry status` (milestone 5). The argument
//! structs and entry points; the logic lives in [`crate::telemetry`].

use std::io::{self, Write};

use clap::{Args, Subcommand};

use crate::telemetry::{self, Config};

/// Arguments of `c3 complain "<text>"`.
#[derive(Args, Debug, Default)]
pub struct ComplainArgs {
    /// The complaint text (truncated to 8 KiB).
    pub text: String,

    /// Send without the confirmation prompt.
    #[arg(long)]
    pub yes: bool,

    /// Where consultations are stored (for the last-run summary). Relative paths resolve
    /// against the git repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
}

/// Arguments of `c3 forget-me`.
#[derive(Args, Debug, Default)]
pub struct ForgetMeArgs {
    /// Delete without the confirmation prompt.
    #[arg(long)]
    pub yes: bool,
}

/// Arguments of `c3 telemetry` - one form per call (the plugin's `codex-telemetry.ps1` forms):
/// `--status` (or the `status` subcommand) and `--backfill-ratings [--dry-run]`.
#[derive(Args, Debug, Default)]
#[command(args_conflicts_with_subcommands = true)]
pub struct TelemetryArgs {
    #[command(subcommand)]
    pub action: Option<TelemetryAction>,

    /// The switch and where it comes from, the intake, the spool, the instance id. Reads only.
    #[arg(long)]
    pub status: bool,

    /// (R24) Send the marks of this repository's tasks that were never sent (findings.json
    /// `ratings` without `telemetry_sent`) as rating events, once.
    #[arg(long)]
    pub backfill_ratings: bool,

    /// With `--backfill-ratings`: print what would be sent (classes only), write nothing.
    #[arg(long)]
    pub dry_run: bool,

    /// With `--backfill-ratings`: where consultations are stored (relative: to the git repo root).
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,

    /// With `--backfill-ratings`: on | off for this call; empty = `CODEX_CONSULT_TELEMETRY`.
    #[arg(long, default_value = "")]
    pub telemetry: String,
}

/// The `c3 telemetry` actions.
#[derive(Subcommand, Debug)]
pub enum TelemetryAction {
    /// Print the on/off status and the instance id.
    Status,
}

/// `c3 complain "<text>"`: print the exact payload, ask before sending, print the public
/// reference to quote on the forum.
pub fn run_complain(args: ComplainArgs) -> i32 {
    if args.text.trim().is_empty() {
        eprintln!("c3 complain: the complaint text is empty");
        return 2;
    }
    let last_run = telemetry::last_run_summary(&args.collab_dir);
    let yes = args.yes;
    let confirm = |payload: &str| -> bool {
        println!("Complaint payload:");
        println!("{payload}");
        if yes {
            return true;
        }
        prompt_yes_no("Send? [y/N] ")
    };
    match telemetry::complain(&args.text, last_run.as_deref(), confirm) {
        Ok(Some(reference)) => {
            println!("Sent. Public reference: {reference}");
            println!("Quote it on https://xelth.com/F/p/c3 or in a mail.");
            0
        }
        Ok(None) => {
            println!("Not sent.");
            0
        }
        Err(e) => {
            eprintln!("c3 complain: {e}");
            1
        }
    }
}

/// `c3 forget-me`: describe what will be deleted, ask, then send the DELETE and wipe local
/// telemetry state.
pub fn run_forget_me(args: ForgetMeArgs) -> i32 {
    let yes = args.yes;
    let confirm = |plan: &str| -> bool {
        println!("{plan}");
        if yes {
            return true;
        }
        prompt_yes_no("Delete now? [y/N] ")
    };
    match telemetry::forget_me(confirm) {
        Ok(outcome) if !outcome.confirmed => {
            println!("Cancelled. Nothing was deleted.");
            0
        }
        Ok(outcome) => {
            if outcome.server_requested {
                if outcome.server_deleted {
                    println!("The intake erased instance {}.", outcome.instance_id);
                } else {
                    println!(
                        "The intake could not be reached; local data was removed and the server \
                         drops it on the next successful DELETE."
                    );
                }
            }
            println!("Local telemetry salt, spool and references removed.");
            0
        }
        Err(e) => {
            eprintln!("c3 forget-me: {e}");
            1
        }
    }
}

/// `c3 telemetry`: exactly one form - `--status` / `status` (read-only: it never creates the
/// telemetry salt) or `--backfill-ratings [--dry-run]`.
pub fn run_telemetry(args: TelemetryArgs) -> i32 {
    const TOOL: &str = "codex-telemetry";
    let status = args.status || matches!(args.action, Some(TelemetryAction::Status));
    let forms = [status, args.backfill_ratings]
        .iter()
        .filter(|b| **b)
        .count();
    if forms != 1 {
        println!("{TOOL}: give exactly one of -Flush, -Status, -Complain \"<text>\", -Forget, -BackfillRatings (c3 telemetry --status | --backfill-ratings).");
        return 1;
    }
    let tele = args.telemetry.trim().to_lowercase();
    if !tele.is_empty() && tele != "on" && tele != "off" {
        println!("{TOOL}: -Telemetry must be on or off (got '{tele}').");
        return 1;
    }
    if args.dry_run && !args.backfill_ratings {
        println!("{TOOL}: -DryRun goes with -BackfillRatings.");
        return 1;
    }
    if args.backfill_ratings {
        let override_ = match tele.as_str() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let repo_root = crate::providers::resolve_repo_root(&cwd);
        let collab_root = crate::providers::resolve_collab_root(&repo_root, &args.collab_dir);
        return telemetry::backfill::run(&collab_root, &telemetry::switch(override_), args.dry_run);
    }
    let config = Config::default();
    println!("{}", telemetry::status(&config));
    match telemetry::instance_id_if_exists() {
        Some(id) => println!("instance: {id}"),
        None => println!("instance: (not created yet - created on the first consultation)"),
    }
    0
}

/// Print a prompt and read a `y`/`yes` answer from stdin; anything else is no.
fn prompt_yes_no(prompt: &str) -> bool {
    print!("{prompt}");
    let _ = io::stdout().flush();
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}
