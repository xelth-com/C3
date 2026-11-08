//! `c3 budget` (wave 9): the operator's hand-written budget snapshots for plans without a live
//! usage API. `set` records a window read from the vendor's web console; `show` prints every
//! snapshot with its live-computed elapsed share and age. The multiplier and the live sources
//! (openai codex rollouts, kimi `/usages`) live in [`crate::budget`].

use chrono::Utc;
use clap::{Args, Subcommand};

/// Arguments of `c3 budget <action>`.
#[derive(Args, Debug)]
pub struct BudgetArgs {
    #[command(subcommand)]
    pub action: BudgetAction,
}

/// The `c3 budget` actions.
#[derive(Subcommand, Debug)]
pub enum BudgetAction {
    /// Record a plan's cap window read by hand from the vendor's console.
    Set(BudgetSetArgs),
    /// Show every recorded snapshot (used / elapsed / resets / age).
    Show,
}

#[derive(Args, Debug)]
pub struct BudgetSetArgs {
    /// The roster `plan` slug this snapshot belongs to.
    pub plan: String,
    /// The window label: `month` (default), `week`, `5h`, ...
    #[arg(long, default_value = "month")]
    pub window: String,
    /// The used share: `42%` or `0.42`.
    #[arg(long)]
    pub used: String,
    /// When the window resets: `2026-11-26` or `2026-11-26T00:00Z`.
    #[arg(long)]
    pub resets: String,
    /// The window's period in days; omit to compute it (a calendar month / 7 days / 5 h).
    #[arg(long)]
    pub period_days: Option<i64>,
}

pub fn run(args: BudgetArgs) -> i32 {
    match args.action {
        BudgetAction::Set(a) => run_set(a),
        BudgetAction::Show => run_show(),
    }
}

/// Parse `42%` / `0.42` / `42` into a 0..1 ratio.
fn parse_used(s: &str) -> Option<f64> {
    let t = s.trim();
    let had_pct = t.ends_with('%');
    let n: f64 = t.trim_end_matches('%').trim().parse().ok()?;
    if !n.is_finite() || n < 0.0 {
        return None;
    }
    let r = if had_pct || n > 1.0 { n / 100.0 } else { n };
    Some(r.clamp(0.0, 1.0))
}

fn run_set(a: BudgetSetArgs) -> i32 {
    let used = match parse_used(&a.used) {
        Some(u) => u,
        None => {
            eprintln!("c3 budget set: --used must be a percentage (42%) or ratio (0.42)");
            return 2;
        }
    };
    let resets = match crate::budget::parse_when(&a.resets).map(|r| r.with_timezone(&Utc)) {
        Some(r) => r,
        None => {
            eprintln!("c3 budget set: --resets must be a date (2026-11-26) or ISO instant");
            return 2;
        }
    };
    match crate::budget::write_snapshot(
        &a.plan,
        &a.window,
        used,
        resets,
        a.period_days,
        Utc::now(),
    ) {
        Ok(path) => {
            println!(
                "budget: {} {} {}% resets {} -> {}",
                a.plan,
                a.window,
                (used * 100.0).round() as i64,
                resets.format("%Y-%m-%d"),
                path.display()
            );
            0
        }
        Err(e) => {
            eprintln!("c3 budget set: {e}");
            1
        }
    }
}

fn run_show() -> i32 {
    let now = Utc::now();
    let snaps = crate::budget::all_snapshots(now);
    if snaps.is_empty() {
        println!("budget: no snapshots (write one with `c3 budget set <plan> --window ... --used ... --resets ...`)");
        return 0;
    }
    println!(
        "{:<14} {:<8} {:>6} {:>8} {:>12} {}",
        "plan", "window", "used", "elapsed", "resets", "age"
    );
    for w in snaps {
        let age_days = (now - w.observed_at).num_days().max(0);
        let age = if w.stale {
            format!("{age_days}d (stale)")
        } else {
            format!("{age_days}d")
        };
        println!(
            "{:<14} {:<8} {:>5}% {:>7}% {:>12} {}",
            w.plan,
            w.name,
            (w.used_ratio * 100.0).round() as i64,
            (w.elapsed_ratio * 100.0).round() as i64,
            w.resets_at.format("%Y-%m-%d"),
            age
        );
    }
    0
}
