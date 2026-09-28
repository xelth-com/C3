//! The review panel (`-Panel`/`-PanelAll`, plugin R14/wave 26) and the detached run (R12).
//!
//! This milestone (M4) ports the plugin's panel path from `codex-consult-common.ps1` and
//! `codex-consult.ps1`:
//!
//! - [`routing`] — the seeded weighted draw: `Get-RoutingScore` (with topic pooling),
//!   `Get-PanelSeed`, the `ConvertTo-Uniform53`/`Get-SlotUniforms` bit-exact uniforms and
//!   `Invoke-PanelDraw` (required seats, lab reserve, exploration).
//! - [`plan`] — `Get-PanelDefaultSize`, `Get-EntryLab`, `Get-EndpointGroups`/`Get-PanelPlan`
//!   (endpoint groups + the roster `parallel` cap + `-PanelConcurrency`),
//!   `Resolve-RequiredReviewers` and `Select-PanelRouting` (seat selection + the
//!   `panel.routing` ledger record).
//!
//! The member run (each seat a child process of the same binary), the byte-identical summary
//! block, and the detached status-file lifecycle build on these; see `docs/port/m4-status.md`.

pub mod member;
pub mod plan;
pub mod roles;
pub mod routing;
pub mod run;
