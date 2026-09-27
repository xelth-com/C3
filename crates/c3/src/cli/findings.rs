//! `c3 findings` — list, move and rate tracked findings (milestone 3). The
//! argument surface mirrors `codex-findings.ps1` (see `docs/port/cli-surface.md`);
//! the logic lives in [`crate::findings_tool`].
//!
//! Note: the reference `codex-findings.ps1` has no `-Json` switch (confirmed against its
//! whole param block; see `docs/port/cli-surface.md` open question 1), so this faithful
//! port does not add one either.

use clap::Args;

/// Arguments of `c3 findings`.
#[derive(Args, Debug, Default)]
pub struct FindingsArgs {
    /// The task slug.
    #[arg(long)]
    pub task: String,

    /// Where consultations are stored. Relative paths resolve against the git repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,

    /// Print open findings (proposed | implemented).
    #[arg(long)]
    pub list: bool,

    /// With `--list`, print every finding.
    #[arg(long)]
    pub all: bool,

    /// One line per consultation from `sessions.json`, with its findings by status.
    #[arg(long)]
    pub stats: bool,

    /// Finding id (`F<NN>-<k>`) whose status to set with `--status`.
    #[arg(long, default_value = "")]
    pub id: String,

    /// proposed | implemented | verified | rejected | wontfix | superseded.
    #[arg(long, default_value = "")]
    pub status: String,

    /// Why (required for `rejected` and for a reopen to `proposed`).
    #[arg(long, default_value = "")]
    pub note: String,

    /// What was run / where the proof is (required for `verified`).
    #[arg(long, default_value = "")]
    pub evidence: String,

    /// Consultation n (a ledger entry of the task) to mark with `--useful`.
    #[arg(long)]
    pub rate: Option<i64>,

    /// yes | partly | no — the judge's mark for the consultation `--rate` names.
    #[arg(long, default_value = "")]
    pub useful: String,
}

/// Run the findings tool and return the process exit code.
pub fn run(args: FindingsArgs) -> i32 {
    crate::findings_tool::run(args)
}
