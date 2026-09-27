//! Subcommand argument structs and entry points. Each submodule owns one `c3`
//! subcommand: its clap `Args` struct and a `run(args) -> i32` returning the process
//! exit code. The binary in `c3-cli` only parses and dispatches.

pub mod consult;
pub mod findings;
pub mod hook;
pub mod scoreboard;
