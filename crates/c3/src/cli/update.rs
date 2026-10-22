//! `c3 update` — check for a newer C3 release and, only with `--apply`, install it (wave 11).
//! The logic lives in [`crate::update`]; this is the clap surface and the dispatch.

use chrono::Utc;
use clap::Args;

use crate::update;

/// Arguments of `c3 update` (see `docs/port/wave11-update.md`).
#[derive(Args, Debug, Default)]
pub struct UpdateArgs {
    /// Check only and report what `--apply` would do (the default; never changes anything).
    #[arg(long)]
    pub check: bool,
    /// Install the newer release: `cargo install` from the tag, an optional fast-forward of a
    /// known clean source checkout, and the plugin update. Never touches a dirty checkout.
    #[arg(long)]
    pub apply: bool,
}

/// Run `c3 update`. `--apply` installs (exit 0/1); otherwise report (exit 0 up to date, 3 an
/// update exists, 1 on a fetch failure).
pub fn run(args: UpdateArgs) -> i32 {
    let now = Utc::now();
    if args.apply {
        update::run_apply(now)
    } else {
        update::run_check_report(now)
    }
}
