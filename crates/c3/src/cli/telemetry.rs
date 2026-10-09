//! `c3 complain`, `c3 forget-me` and `c3 telemetry` (milestone 5; the plugin's
//! `codex-telemetry.ps1` forms since wave 2). The argument structs and entry points; the logic
//! lives in [`crate::telemetry`].

use std::io::{self, Write};

use clap::{Args, Subcommand};

use crate::telemetry;

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

/// Arguments of `c3 forget-me`: ask the intake to erase everything this installation sent (the
/// newest stored complaint reference, or a pending deletion's, proves ownership), and remove the
/// local telemetry files once the intake CONFIRMED it.
#[derive(Args, Debug, Default)]
pub struct ForgetMeArgs {
    /// Delete without the confirmation prompt.
    #[arg(long)]
    pub yes: bool,

    /// The public reference to prove ownership with (default: a pending deletion's, else the
    /// newest one a complaint stored).
    #[arg(long, default_value = "")]
    pub public_ref: String,

    /// Without any public reference: remove the LOCAL data only (the intake keeps what was sent).
    #[arg(long)]
    pub local: bool,
}

/// Arguments of `c3 telemetry` - one form per call (the plugin's `codex-telemetry.ps1` forms):
/// `--status` (or the `status` subcommand), `--flush`, `--forget [--public-ref <ref>] [--local]
/// [--yes]` and `--backfill-ratings [--dry-run]`.
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

    /// With `--backfill-ratings` and `--flush`: on | off for this call; empty =
    /// `CODEX_CONSULT_TELEMETRY`.
    #[arg(long, default_value = "")]
    pub telemetry: String,

    /// THE sender, once: deliver the spool (the outbox rule - delivered lines removed, lines
    /// appended meanwhile kept) and write the last flush's record `last-flush.json` {time, result,
    /// delivered, kept, dropped, rejected, http, not_spooled_seen, not_spooled_folded, notes} -
    /// folding the not-spooled files of gone producers into one of its notes (the record saved
    /// BEFORE the files are deleted; not_spooled_folded {name, bytes} names the files a fold
    /// covers; a legacy file is first staged as telemetry-not-spooled-legacy-<utc ticks>.ndjson).
    #[arg(long)]
    pub flush: bool,

    /// Delete my data: with `--public-ref <ref>` at the intake, with `--local` here (both: the
    /// intake FIRST, the local files only after it confirmed).
    #[arg(long)]
    pub forget: bool,

    /// With `--forget`: the public_ref a delivered complaint printed (the intake's proof of
    /// ownership); empty = a pending deletion's.
    #[arg(long, default_value = "")]
    pub public_ref: String,

    /// With `--forget`: remove the local spool, the salt, the references and the counters.
    #[arg(long)]
    pub local: bool,

    /// With `--forget --local` alone: remove without asking.
    #[arg(long)]
    pub yes: bool,
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

/// `c3 forget-me`: the C3 one-step forget - the intake's DELETE (proved by `--public-ref`, else a
/// pending deletion's reference, else the newest stored one) and, once the intake CONFIRMED it, the
/// local files. A DELETE that is not confirmed deletes nothing and records the pending deletion
/// for the retry (F02-3). Without any reference: `--local` removes the local data only.
pub fn run_forget_me(args: ForgetMeArgs) -> i32 {
    let dir = telemetry::telemetry_dir();
    // (F09-3) a deletion the intake confirmed whose local cleanup did not finish: only that is
    // left - nothing is asked of the intake, and nothing needs asking here
    if telemetry::pending_deletion_in(&dir).is_some_and(|p| p.cleanup_due()) {
        let out = telemetry::forget(
            &telemetry::ForgetRequest {
                public_ref: None,
                local: true,
                yes: true,
            },
            prompt_yes_no,
        );
        for l in &out.lines {
            println!("{l}");
        }
        return out.exit;
    }
    let explicit = args.public_ref.trim().to_string();
    let reference = if !explicit.is_empty() {
        Some(explicit)
    } else {
        telemetry::pending_deletion_in(&dir)
            .map(|p| p.public_ref)
            .filter(|r| !r.is_empty())
            .or_else(|| telemetry::newest_ref(&dir))
    };
    if reference.is_none() && !args.local {
        println!("c3 forget-me: no public reference is stored (a delivered complaint's public_ref proves ownership of this instance), so the intake cannot be asked to delete anything; nothing was sent and nothing removed. --public-ref <ref> names one; --local removes the LOCAL data only (the intake keeps what was sent).");
        return 1;
    }
    if reference.is_some() && !args.yes {
        let id = telemetry::pending_deletion_in(&dir)
            .map(|p| p.instance_id)
            .or_else(telemetry::instance_id_if_exists)
            .unwrap_or_else(|| "(none)".into());
        println!(
            "This asks {} to erase every event and complaint of instance {id}, then - once the intake confirmed it - deletes the local telemetry salt, spool and references under {}.",
            telemetry::hub().base,
            dir.display()
        );
        if !prompt_yes_no("Delete now? [y/N] ") {
            println!("Cancelled. Nothing was deleted.");
            return 0;
        }
    }
    let req = telemetry::ForgetRequest {
        public_ref: reference,
        local: true,
        yes: args.yes,
    };
    let out = telemetry::forget(&req, prompt_yes_no);
    for l in &out.lines {
        println!("{l}");
    }
    out.exit
}

/// `c3 telemetry`: exactly one form - `--status` / `status` (read-only: it never creates the
/// telemetry salt), `--flush`, `--forget` or `--backfill-ratings [--dry-run]`.
pub fn run_telemetry(args: TelemetryArgs) -> i32 {
    const TOOL: &str = "codex-telemetry";
    let status = args.status || matches!(args.action, Some(TelemetryAction::Status));
    let forms = [status, args.flush, args.forget, args.backfill_ratings]
        .iter()
        .filter(|b| **b)
        .count();
    if forms != 1 {
        println!("{TOOL}: give exactly one of -Flush, -Status, -Complain \"<text>\", -Forget, -BackfillRatings (c3 telemetry --status | --flush | --forget | --backfill-ratings; c3 complain \"<text>\").");
        return 1;
    }
    let tele = args.telemetry.trim().to_lowercase();
    if !tele.is_empty() && tele != "on" && tele != "off" {
        println!("{TOOL}: -Telemetry must be on or off (got '{tele}').");
        return 1;
    }
    let override_ = match tele.as_str() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    };
    if args.dry_run && !args.backfill_ratings {
        println!("{TOOL}: -DryRun goes with -BackfillRatings.");
        return 1;
    }
    if !args.forget {
        if args.local {
            println!("{TOOL}: -Local goes with -Forget.");
            return 1;
        }
        if !args.public_ref.trim().is_empty() {
            println!("{TOOL}: -PublicRef goes with -Forget.");
            return 1;
        }
    }
    if args.backfill_ratings {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let repo_root = crate::providers::resolve_repo_root(&cwd);
        let collab_root = crate::providers::resolve_collab_root(&repo_root, &args.collab_dir);
        return telemetry::backfill::run(&collab_root, &telemetry::switch(override_), args.dry_run);
    }
    if args.flush {
        // TEST HOOK (test mode only): the NAMES of every variable this process inherited (never a
        // value) - (wave 6) a run starts its sender with the allow-listed environment only
        telemetry::sender::dump_env_names_hook();
        let sw = telemetry::switch(override_);
        if !sw.on {
            println!(
                "{TOOL}: telemetry is off ({}): nothing is sent; the spool stays as it is.",
                sw.source
            );
            return 0;
        }
        // the intake URL refused (CODEX_CONSULT_TELEMETRY_URL not https, plain http not to a
        // loopback host in test mode): refused before any connection
        let hub = telemetry::hub();
        if !hub.error.is_empty() {
            println!("{TOOL}: not delivered: {}", hub.error);
            return 1;
        }
        return match telemetry::flush_now() {
            Ok(r) if !r.skipped.is_empty() => {
                // (wave 3d, F24-6) the record's warning is said on a skipped flush too
                println!(
                    "{TOOL}: skipped - {}{}",
                    r.skipped,
                    if r.last_warning.is_empty() {
                        String::new()
                    } else {
                        format!("; warning: {}", r.last_warning)
                    }
                );
                if r.skipped.starts_with("another flush") {
                    2
                } else {
                    1
                }
            }
            Ok(r) => {
                // (wave 5) the plugin's line: `codex-telemetry: <result>` - the same text the last
                // flush's record keeps; exit 1 when the send stopped before everything was sent
                println!(
                    "{TOOL}: {}{}",
                    r.result_text(),
                    if r.last_warning.is_empty() {
                        String::new()
                    } else {
                        format!("; warning: {}", r.last_warning)
                    }
                );
                if r.stopped.is_empty() {
                    0
                } else {
                    1
                }
            }
            Err(e) => {
                println!("{TOOL}: not delivered - {e}");
                1
            }
        };
    }
    if args.forget {
        let req = telemetry::ForgetRequest {
            public_ref: Some(args.public_ref.trim().to_string()).filter(|r| !r.is_empty()),
            local: args.local,
            yes: args.yes,
        };
        let out = telemetry::forget(&req, prompt_yes_no);
        for l in &out.lines {
            println!("{l}");
        }
        return out.exit;
    }
    print_status();
    0
}

/// `c3 telemetry --status`: the switch and its source, the intake, the spool's counts, the events
/// not spooled since the last flush, a pending deletion, the last flush, the instance id and the
/// notice - in the plugin's `-Status` layout. Reads only (never creates the salt).
fn print_status() {
    let sw = telemetry::switch(None);
    let hub = telemetry::hub();
    let dir = telemetry::telemetry_dir();
    println!(
        "codex-telemetry: telemetry {} ({}) - CODEX_CONSULT_TELEMETRY=off switches it off, --telemetry off for one run; README \"Telemetry\"",
        sw.text(),
        sw.source
    );
    if c3_core::test_hooks::mode_on() {
        println!("test mode  : ON - CODEX_CONSULT_TEST_MODE=1: test hooks are honoured and plain http to a loopback intake is accepted");
    }
    if !hub.error.is_empty() {
        println!("intake     : REFUSED - {}", hub.error);
    } else if hub.source == "the default" {
        println!(
            "intake     : {} (the default; CODEX_CONSULT_TELEMETRY_URL overrides)",
            hub.base
        );
    } else {
        println!("intake     : {} ({})", hub.base, hub.source);
    }
    let spool = telemetry::Spool::new(&dir, hub.base.clone());
    let (events, other, bad, oldest) = spool.counts();
    let files = usize::from(dir.join("spool.ndjson").is_file());
    let oldest = oldest
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
        .map(|d| {
            format!(
                "; oldest queued {}",
                d.with_timezone(&chrono::Local)
                    .format("%Y-%m-%dT%H:%M:%S%:z")
            )
        })
        .unwrap_or_default();
    println!(
        "spool      : {} - {events} event(s), {other} complaint(s){} in {files} file(s){oldest}",
        dir.join("spool.ndjson").display(),
        if bad > 0 {
            format!(", {bad} unreadable line(s)")
        } else {
            String::new()
        }
    );
    // (wave 3b, E2) the events not spooled since the last flush, summed over every producer's file
    // and the legacy files
    let paths = telemetry::local_paths(&dir);
    let ns = telemetry::notspooled::count(&paths, false);
    let files_text = if ns.files > 0 {
        format!(
            " ({} line(s) in {} file(s), one per producer - a flush folds those of gone producers into the last flush's record)",
            ns.total, ns.files
        )
    } else {
        String::new()
    };
    if ns.count > 0 {
        println!(
            "not spooled: {} event(s) since the last flush - the latest {}: {}{files_text}",
            ns.count, ns.when, ns.last
        );
    } else {
        println!("not spooled: none since the last flush{files_text}");
    }
    // (wave 3b, E3) a local deletion that runs, or that died halfway: its marker and its owner
    let fm = telemetry::notspooled::forgetting_owner(&paths.marker);
    if fm.there {
        if fm.alive {
            println!(
                "forgetting : the marker {} is there - its owner pid {} lives (a -Forget -Local runs now{}): events are dropped until it finishes",
                paths.marker.display(),
                fm.pid,
                if fm.since.is_empty() { String::new() } else { format!(", since {}", fm.since) }
            );
        } else {
            println!(
                "forgetting : the marker {} is there - its owner {} (a -Forget -Local that did not finish): the next event or sender removes it; run c3 telemetry --forget --local again to finish the deletion",
                paths.marker.display(),
                if fm.pid > 0 { format!("pid {} is gone", fm.pid) } else { "is not named".to_string() }
            );
        }
    }
    // (wave 5; the plugin's wave 28d D3) the sender lock as its owner record says (read only):
    // busy, stuck (30 minutes), or a record its owner left behind
    if let Some(line) = telemetry::sender_status(&dir) {
        println!("sender     : {line}");
    }
    match telemetry::pending_deletion_in(&dir) {
        Some(p) if p.cleanup_due() => println!("forgetting : the local deletion of instance {} did not finish (phase {}, since {}) - nothing is spooled or sent until it is; the next flush or c3 forget-me finishes it", p.instance_id, p.phase, p.since),
        Some(p) => println!("forgetting : a deletion of instance {} is pending at the intake since {} ({} attempt(s), the last: {}) - nothing is spooled or sent until it is confirmed; c3 forget-me retries it", p.instance_id, p.since, p.attempts, p.last_error),
        None if dir.join("forget-pending.json").exists() => println!("forgetting : the deletion record {} cannot be read - nothing is spooled or sent until it is removed or a forget rewrites it", dir.join("forget-pending.json").display()),
        None => {}
    }
    let last = std::fs::read_to_string(&paths.last)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let result = last
        .as_ref()
        .and_then(|v| v.get("result"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    if result.is_empty() {
        println!("last flush : never");
    } else {
        println!(
            "last flush : {} - {result}",
            last.as_ref()
                .and_then(|v| v.get("time"))
                .and_then(|x| x.as_str())
                .unwrap_or("?")
        );
    }
    // (wave 3b) what the telemetry client did or saw on its own (the record's notes)
    let notes = last
        .as_ref()
        .and_then(|v| v.get("notes"))
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    for n in notes
        .iter()
        .filter_map(|n| n.as_str())
        .filter(|n| !n.is_empty())
    {
        println!("note       : {n}");
    }
    match telemetry::instance_id_if_exists() {
        Some(id) => println!(
            "instance id: {id} (SHA-256 of the salt {} and the machine name; a new salt makes a new instance)",
            dir.join("salt").display()
        ),
        None => println!("instance id: (none yet - the first event creates the salt)"),
    }
    let notice = telemetry::notice_marker();
    if notice.exists() {
        println!("notice     : shown ({})", notice.display());
    } else {
        println!("notice     : not shown yet - the next consultation with telemetry on prints it");
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
