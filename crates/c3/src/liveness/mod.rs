//! Process liveness and recovery-record semantics shared across `c3` subcommands.
//!
//! [`proc`] ports the plugin's `Get-ProcessStartIso` / `Test-PidAlive` /
//! `Test-SameStartTime`; [`pending`] ports `Read-PendingFile` /
//! `Read-TaskPendingRecords` / `Test-PendingActive` / `Get-PendingOriginalNote`. Both
//! were first written for `c3 findings` (`findings_tool`) and moved here (M2d) because
//! `c3 consult`'s pending-recovery path (reserved → launching → running → survivors) and
//! its refusal of a new run while an interrupted run's process is alive need the identical
//! liveness rules. `findings_tool` re-imports them from this module.

pub mod pending;
pub mod proc;
