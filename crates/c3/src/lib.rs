//! C3 runtime library.
//!
//! The runtime modules that build on `c3-core`'s types and formats. [`providers`]
//! is implemented (milestone 1); [`cli`] holds the clap argument structs and the
//! `run` entry point of every subcommand so the binary in `c3-cli` only dispatches;
//! the other modules are named here so the crate's shape matches the design and
//! later milestones drop in without moving code.

// The dry-run `sessions.json` entry preview is one large `json!{...}` literal whose macro
// expansion needs more than the default 128-deep recursion budget.
#![recursion_limit = "256"]

pub mod providers;

pub mod cli;

pub mod engines;

pub mod http_engine;

pub mod liveness;

pub mod consult;

pub mod findings_tool;

pub mod scoreboard;

pub mod hook;

pub mod panel;

pub mod router {
    //! Routing policies (v1: smoothed score over a global prior; later Thompson
    //! sampling and brief similarity as new versions). Milestone 9.
}

pub mod index;

pub mod pack;

pub mod telemetry;

pub mod mcp;
