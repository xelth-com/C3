//! The single-consultation flow behind `c3 consult` (milestone 2c): resolve the
//! task and collab root, preflight the reviewer, resolve lineage and parent thread
//! from the ledger, hash the brief and fingerprint the revision, assemble the
//! prompt with the open-findings snapshot, print the dry-run block or run the
//! engine, ingest the reply (structured or prose, format repair), write the handoff
//! files and commit the ledger entry and findings through `c3_core::store`, print
//! the summary block, return the exit code.
//!
//! The self-contained pieces of the flow are in the submodules below (each unit-tested):
//! [`args`] (validation + defaults), [`prompt`] (prompt assembly), [`ingest`] (the prose
//! gate + structured/prose classification), [`render`] (the handoff's findings sections),
//! [`revision`] (the git fingerprint, content-only tree check), [`summary`] (the resume
//! command + the console summary). [`orchestrate`] ties them to the identity/preflight/
//! launcher layer (reused from [`crate::providers`]) and the [`crate::engines::codex`]
//! adapter for the codex engine.

pub mod args;
pub mod detach;
pub mod detached;
pub mod dryrun;
pub mod ingest;
pub mod kick;
pub mod orchestrate;
pub mod prompt;
pub mod recovery;
pub mod render;
pub mod revision;
pub mod secondary;
pub mod semantics;
pub mod summary;

pub use args::Options;

/// Run one consultation and return the process exit code.
pub fn run(opts: Options) -> i32 {
    orchestrate::run(opts)
}
