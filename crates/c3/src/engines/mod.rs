//! Engine adapters (`codex`, `agy`, `muse`, `http`): one attempt per call, the CLIs
//! wrapped exactly as the plugin drives them (argv from `c3_core::engine`, prompt
//! delivery per engine, events capture, timeout kill and continuation, denial
//! retry, format repair, partial salvage), the `http` engine sending one
//! OpenAI-compatible request.
//!
//! Milestone 2c implements the codex adapter ([`codex`]) end to end on top of the shared
//! subprocess launcher ([`subprocess`]); agy/muse reuse the same launcher and land next,
//! http at milestone 7.

pub mod agy;
pub mod codex;
pub mod muse;
pub mod subprocess;
pub mod tree_check;

pub use agy::AgyEngine;
pub use codex::CodexEngine;
pub use muse::MuseEngine;
