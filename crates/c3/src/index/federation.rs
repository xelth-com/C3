//! Index federation, opt-in and read-only (M11, DESIGN §7 "Federation, opt-in").
//!
//! A peer is ANOTHER index in the same schema — another project's embedded store, or the
//! user's `ws://` hub. v1 never writes into a peer and never syncs. A peer is used only when
//! the user's configuration allows it FOR THIS PROJECT and the command names it (`--peer` /
//! `--peers all`). A peer that does not open is skipped; a query never fails because of a peer.
//!
//! This module holds the connection-independent pieces (the selection, the resolved-peer
//! record and the cross-source reciprocal-rank fusion, all unit-testable without the store);
//! opening a peer and its availability probe need the `index-surreal` feature and live in the
//! feature-gated block at the end.

use super::{Backend, Hit};

/// Which peers a command asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerSelection {
    /// `--peers all`: every peer the project may use.
    pub all: bool,
    /// `--peer <name>` occurrences (each must resolve to an allowed peer).
    pub names: Vec<String>,
}

impl PeerSelection {
    /// No peer requested — the query/pack is local, exactly as before M11.
    pub fn is_empty(&self) -> bool {
        !self.all && self.names.is_empty()
    }
}

/// A peer resolved from the configuration for THIS project: its connection and the
/// namespace/database of its entry. `name` passed the config slug check, so it is the one
/// value a refusal may echo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPeer {
    pub name: String,
    pub backend: Backend,
    pub namespace: String,
    pub database: String,
    /// Whether the peer's entry allows its content into a pack (`use_in_packs`).
    pub use_in_packs: bool,
}

impl ResolvedPeer {
    /// A short, non-secret description of where the peer lives (the store path or the host),
    /// for `c3 index peers`. Never a userinfo or a full connection string.
    pub fn location(&self) -> String {
        match &self.backend {
            Backend::SurrealKv(path) => path.to_string_lossy().replace('\\', "/"),
            Backend::Ws { url } => host_of(url),
            Backend::None => "none".to_string(),
        }
    }
}

/// The host (and port) of a `ws://` url, without scheme, userinfo or path. Falls back to the
/// raw string's authority when it does not parse.
pub(crate) fn host_of(url: &str) -> String {
    if let Ok(u) = url::Url::parse(url) {
        let host = u.host_str().unwrap_or("");
        match u.port() {
            Some(p) => format!("{host}:{p}"),
            None => host.to_string(),
        }
    } else {
        url.trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_string()
    }
}

/// Fuse the local hit list and each peer's hit list into one ranked list by reciprocal rank,
/// tagging every hit with its source (`local` or `peer:<name>`). Each source contributes one
/// ranked list; ties are broken deterministically by the fused id. A peer hit keeps its path
/// as stored in the peer (relative to the PEER's repository) — it is never resolved against
/// the local repository. `budget` (0 = none) trims by the same token estimate the local path
/// uses, in fused order.
pub fn fuse_sourced(local: Vec<Hit>, peers: Vec<(String, Vec<Hit>)>, budget: usize) -> Vec<Hit> {
    use crate::pack::budget::estimate_tokens;
    use std::collections::BTreeMap;

    // Assign a stable id per hit and keep the hit keyed by it. Source prefixes keep ids from
    // different stores distinct even when two peers share a path.
    let mut by_id: BTreeMap<String, Hit> = BTreeMap::new();
    let mut rankings: Vec<Vec<String>> = Vec::new();

    let mut ingest = |source: String, hits: Vec<Hit>, rankings: &mut Vec<Vec<String>>| {
        let mut list = Vec::with_capacity(hits.len());
        for (rank, mut h) in hits.into_iter().enumerate() {
            h.source = source.clone();
            let id = format!(
                "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
                source, h.path, h.name, h.line_start, rank
            );
            list.push(id.clone());
            by_id.insert(id, h);
        }
        rankings.push(list);
    };

    ingest("local".to_string(), local, &mut rankings);
    for (name, hits) in peers {
        ingest(format!("peer:{name}"), hits, &mut rankings);
    }

    let fused = super::rrf(&rankings, 60.0);
    let mut out: Vec<Hit> = Vec::with_capacity(fused.len());
    let mut used = 0usize;
    for (id, score) in fused {
        if let Some(mut h) = by_id.remove(&id) {
            if budget > 0 {
                let ext = h.path.rsplit('.').next().unwrap_or("");
                let t = estimate_tokens(&h.snippet, ext);
                if used + t > budget && !out.is_empty() {
                    break;
                }
                used += t;
            }
            h.score = score;
            out.push(h);
        }
    }
    out
}

// --------------------------------------------------------------------------- feature-gated

/// Whether a peer opens right now, for `c3 index peers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Opens and carries the index schema.
    Available,
    /// An embedded store held by another process (RC1 fail-fast).
    Busy,
    /// Could not connect.
    Unreachable,
    /// Opened, but does not look like a C3 index.
    SchemaMismatch,
    /// The `index-surreal` feature is off, so no probe was done.
    FeatureOff,
}

impl Availability {
    pub fn as_str(self) -> &'static str {
        match self {
            Availability::Available => "available",
            Availability::Busy => "busy",
            Availability::Unreachable => "unreachable",
            Availability::SchemaMismatch => "schema mismatch",
            Availability::FeatureOff => "unknown (index feature off)",
        }
    }
}

#[cfg(feature = "index-surreal")]
mod backend_ops {
    use super::*;
    use crate::index::SurrealIndex;

    /// Probe a peer: open it read-only for the shortest span and classify the outcome. No retry
    /// loop; the handle is dropped before returning.
    pub fn probe(peer: &ResolvedPeer) -> Availability {
        match SurrealIndex::open_read(peer.backend.clone(), &peer.namespace, &peer.database) {
            Ok(idx) => {
                let ok = idx.schema_ok();
                drop(idx);
                if ok {
                    Availability::Available
                } else {
                    Availability::SchemaMismatch
                }
            }
            Err(reason) => classify_open_error(&reason),
        }
    }

    /// Open a peer read-only, run one retrieval, and close it before returning. `Ok(hits)` on a
    /// clean read (each hit's path is relative to the PEER's repository), `Err(one line)` when
    /// the peer did not open or the read failed — the caller skips it with one line on stderr.
    pub fn retrieve(peer: &ResolvedPeer, query: &str, budget: usize) -> Result<Vec<Hit>, String> {
        let idx = SurrealIndex::open_read(peer.backend.clone(), &peer.namespace, &peer.database)
            .map_err(|reason| classify_open_error(&reason).as_str().to_string())?;
        let hits = idx.retrieve(query, budget);
        drop(idx);
        hits
    }

    /// Map an open error to an availability class. The embedded store's RC1 lock violation
    /// (os error 32/33, surfaced in the message) is `busy`; anything else is `unreachable`.
    fn classify_open_error(reason: &str) -> Availability {
        let r = reason.to_ascii_lowercase();
        if r.contains("os error 32")
            || r.contains("os error 33")
            || r.contains("lock")
            || r.contains("in use")
            || r.contains("sharing violation")
        {
            Availability::Busy
        } else {
            Availability::Unreachable
        }
    }
}

/// Probe a peer's availability (`index-surreal` only; otherwise [`Availability::FeatureOff`]).
pub fn probe_peer(peer: &ResolvedPeer) -> Availability {
    #[cfg(feature = "index-surreal")]
    {
        backend_ops::probe(peer)
    }
    #[cfg(not(feature = "index-surreal"))]
    {
        let _ = peer;
        Availability::FeatureOff
    }
}

/// Retrieve from a peer, opening it read-only for the shortest span. `Err` (feature off, or a
/// peer that did not open / read) means the caller skips the peer; the query never fails.
pub fn retrieve_peer(peer: &ResolvedPeer, query: &str, budget: usize) -> Result<Vec<Hit>, String> {
    #[cfg(feature = "index-surreal")]
    {
        backend_ops::retrieve(peer, query, budget)
    }
    #[cfg(not(feature = "index-surreal"))]
    {
        let _ = (peer, query, budget);
        Err("index feature off".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(path: &str, name: &str) -> Hit {
        Hit {
            path: path.to_string(),
            name: name.to_string(),
            kind: "fn".to_string(),
            line_start: 1,
            line_end: 2,
            snippet: "code".to_string(),
            score: 0.0,
            relations: Vec::new(),
            source: String::new(),
        }
    }

    #[test]
    fn fuse_tags_source_and_orders_by_rank() {
        let local = vec![hit("a.rs", "a"), hit("b.rs", "b")];
        let peer = vec![hit("x.rs", "a"), hit("y.rs", "z")];
        let out = fuse_sourced(local, vec![("hub".to_string(), peer)], 0);
        // Every hit is tagged.
        assert!(out
            .iter()
            .all(|h| h.source == "local" || h.source == "peer:hub"));
        assert!(out
            .iter()
            .any(|h| h.source == "peer:hub" && h.path == "x.rs"));
        // The two rank-0 hits (local a.rs, peer x.rs) tie on RRF; both are near the top.
        let top2: Vec<&str> = out.iter().take(2).map(|h| h.path.as_str()).collect();
        assert!(top2.contains(&"a.rs"));
        assert!(top2.contains(&"x.rs"));
    }

    #[test]
    fn host_of_strips_scheme_and_userinfo() {
        assert_eq!(host_of("ws://127.0.0.1:8000"), "127.0.0.1:8000");
        assert_eq!(host_of("wss://hub.example/rpc"), "hub.example");
    }
}
