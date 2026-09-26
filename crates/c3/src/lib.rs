//! C3 runtime library.
//!
//! The runtime modules that build on `c3-core`'s types and formats. Only
//! [`providers`] is implemented at milestone 1 (providers and preflight); the rest
//! are named here so the crate's shape matches the design and later milestones drop
//! in without moving code.

pub mod providers;

pub mod engines {
    //! Engine adapters (`codex`, `agy`, `muse`, `http`): one attempt per call, the
    //! CLIs wrapped exactly as the plugin drives them, the `http` engine sending one
    //! OpenAI-compatible request. Milestone 2.
}

pub mod panel {
    //! Panel sizing (plugin R14), the endpoint-aware scheduler and detach/status
    //! (plugin R12). Milestone 4.
}

pub mod router {
    //! Routing policies (v1: smoothed score over a global prior; later Thompson
    //! sampling and brief similarity as new versions). Milestone 9.
}

pub mod index {
    //! The derived code index (`ContextIndex`): embedded surrealkv per project or a
    //! per-user SurrealDB server, behind the `index-surreal` feature. Milestone 8.
}

pub mod pack {
    //! The pack pipeline (`PackBuilder`): snapshots, reviewer packs and explainer
    //! packs from one discovery/redaction/budget pipeline. Milestone 7.
}

pub mod telemetry {
    //! The T-hub client: a typed payload allowlist, an NDJSON spool, background send
    //! with retry, `complain`, `forget_me` and prior download. Milestone 5.
}
