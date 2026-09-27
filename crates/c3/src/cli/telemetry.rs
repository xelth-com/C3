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

/// Arguments of `c3 telemetry <action>`.
#[derive(Args, Debug)]
pub struct TelemetryArgs {
    #[command(subcommand)]
    pub action: TelemetryAction,
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

/// `c3 telemetry status`: the dry-run status sentence plus the instance id.
pub fn run_telemetry(args: TelemetryArgs) -> i32 {
    match args.action {
        TelemetryAction::Status => {
            let config = Config::default();
            println!("{}", telemetry::status(&config));
            println!("instance: {}", telemetry::instance_id());
            0
        }
    }
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
