//! `ContextIndex` — the derived, rebuildable code index (DESIGN §4, §7; invariant 6).
//!
//! The connection string decides the backend:
//! - `none` — always available, every call a no-op returning empty results and
//!   [`IndexStats::none`]. Present in every build, including `--no-default-features`.
//! - `surrealkv:<path>` — an embedded SurrealDB store (feature `index-surreal`). Strictly
//!   single-process (RC1): only the task-lock holder opens it, and any open failure yields
//!   `Index: none` without a retry loop.
//! - `ws://host[:port]` — a per-user SurrealDB server shared by several projects. Compiled
//!   and its connection string parsed here; not exercised live in this milestone.
//!
//! The index is derived: `index(repo, generation)` builds it from discovered files plus the
//! repository, `retrieve(query, budget)` runs BM25 over four indexes fused by reciprocal
//! rank and a bounded 1-hop expansion, `rebuild()` drops and re-indexes, `stats()` reports
//! counts. An index failure never fails a consultation.

use std::collections::BTreeMap;
use std::path::PathBuf;

pub mod extract;
#[cfg(feature = "index-surreal")]
mod surreal;

pub use extract::{
    derive_relations, extract_file, file_hash, Entity, EntityKind, Extraction, Relation,
    RelationKind,
};

/// Refusal / status prefix, matching the rest of C3.
pub const PREFIX: &str = "codex-consult:";

/// The backend chosen by a connection string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    /// No index; every operation is a no-op.
    None,
    /// Embedded surrealkv at this filesystem path.
    SurrealKv(PathBuf),
    /// A per-user SurrealDB server. `namespace`/`database` are derived from repo identity.
    Ws { url: String },
}

impl Backend {
    /// The short backend name used in `stats` output.
    pub fn name(&self) -> &'static str {
        match self {
            Backend::None => "none",
            Backend::SurrealKv(_) => "surrealkv",
            Backend::Ws { .. } => "ws",
        }
    }
}

/// Error parsing a connection string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnError(pub String);

impl std::fmt::Display for ConnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{PREFIX} index: {}", self.0)
    }
}
impl std::error::Error for ConnError {}

/// Parse a connection string into a [`Backend`].
///
/// Accepts `none`, `surrealkv:<path>` (or the alias `surrealkv://<path>`), and
/// `ws://host[:port]` / `wss://host[:port]`. Anything else is an error.
pub fn parse_conn(conn: &str) -> Result<Backend, ConnError> {
    let c = conn.trim();
    if c.is_empty() || c.eq_ignore_ascii_case("none") {
        return Ok(Backend::None);
    }
    if let Some(rest) = c.strip_prefix("surrealkv://") {
        return kv_path(rest);
    }
    if let Some(rest) = c.strip_prefix("surrealkv:") {
        return kv_path(rest);
    }
    if c.starts_with("ws://") || c.starts_with("wss://") {
        // Minimal validation: scheme plus a non-empty host.
        let host = c
            .trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .split('/')
            .next()
            .unwrap_or("");
        if host.is_empty() {
            return Err(ConnError(format!("ws connection string has no host: {c}")));
        }
        return Ok(Backend::Ws { url: c.to_string() });
    }
    Err(ConnError(format!(
        "unknown connection string {c:?} (expected none | surrealkv:<path> | ws://host)"
    )))
}

fn kv_path(rest: &str) -> Result<Backend, ConnError> {
    let p = rest.trim();
    if p.is_empty() {
        return Err(ConnError("surrealkv connection string has no path".into()));
    }
    Ok(Backend::SurrealKv(PathBuf::from(p)))
}

/// Counts and identity reported by `stats`.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct IndexStats {
    /// `none` | `surrealkv` | `ws`.
    pub backend: String,
    /// The store path or server url, when there is one.
    pub path: Option<String>,
    /// Why the index is `none` (lock held, open failed, feature off), when it is.
    pub reason: Option<String>,
    pub entities: u64,
    pub files: u64,
    pub belongs_to: u64,
    pub calls: u64,
    pub relates_to: u64,
    /// The evidence generation the rows carry (git HEAD + dirty digest).
    pub generation: Option<String>,
}

impl IndexStats {
    /// The stats of an unavailable index.
    pub fn none(reason: impl Into<String>) -> Self {
        IndexStats {
            backend: "none".to_string(),
            reason: Some(reason.into()),
            ..Default::default()
        }
    }

    /// The one-line verdict C3 prints (`index: none (<reason>)` or a count summary).
    pub fn verdict(&self) -> String {
        if self.backend == "none" {
            match &self.reason {
                Some(r) => format!("index: none ({r})"),
                None => "index: none".to_string(),
            }
        } else {
            format!(
                "index: {} ({} entities | {} files | {} belongs_to | {} calls | {} relates_to)",
                self.backend,
                self.entities,
                self.files,
                self.belongs_to,
                self.calls,
                self.relates_to
            )
        }
    }
}

/// One retrieval hit, trimmed to the token budget by the caller.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Hit {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub line_start: usize,
    pub line_end: usize,
    pub snippet: String,
    pub score: f64,
    /// Relation labels for rows pulled in by expansion ("derived: calls", ...).
    pub relations: Vec<String>,
}

/// Reciprocal rank fusion of several ranked id lists (best first).
///
/// `score(id) = Σ 1 / (k + rank)` over every list the id appears in (rank 0-based). Returns
/// `(id, score)` sorted by score descending, ties broken by id for determinism. `k` is the
/// usual RRF constant (60).
pub fn rrf(rankings: &[Vec<String>], k: f64) -> Vec<(String, f64)> {
    let mut scores: BTreeMap<String, f64> = BTreeMap::new();
    for list in rankings {
        for (rank, id) in list.iter().enumerate() {
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank as f64 + 1.0);
        }
    }
    let mut v: Vec<(String, f64)> = scores.into_iter().collect();
    v.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    v
}

/// Compute the evidence generation: `<HEAD7>[+dirty:<n>@<digest8>]` (git HEAD plus a digest
/// of the porcelain status). A repo with no git yields `nogit@<n>`.
pub fn generation(repo_root: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let head = git_line(repo_root, &["rev-parse", "HEAD"]);
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    match head {
        Some(h) => {
            let head7: String = h.chars().take(7).collect();
            if status.trim().is_empty() {
                head7
            } else {
                let mut hasher = Sha256::new();
                hasher.update(status.as_bytes());
                let digest = format!("{:x}", hasher.finalize());
                let n = status.lines().filter(|l| !l.trim().is_empty()).count();
                format!("{head7}+dirty:{n}@{}", &digest[..8])
            }
        }
        None => {
            let mut hasher = Sha256::new();
            hasher.update(status.as_bytes());
            format!("nogit@{}", &format!("{:x}", hasher.finalize())[..8])
        }
    }
}

fn git_line(root: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(feature = "index-surreal")]
pub use surreal::SurrealIndex;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_backends() {
        assert_eq!(parse_conn("none").unwrap(), Backend::None);
        assert_eq!(parse_conn("").unwrap(), Backend::None);
        assert_eq!(parse_conn("NONE").unwrap(), Backend::None);
        assert_eq!(
            parse_conn("surrealkv:/tmp/idx").unwrap(),
            Backend::SurrealKv(PathBuf::from("/tmp/idx"))
        );
        assert_eq!(
            parse_conn("surrealkv://C:/x/idx").unwrap(),
            Backend::SurrealKv(PathBuf::from("C:/x/idx"))
        );
        assert_eq!(
            parse_conn("ws://localhost:8000").unwrap(),
            Backend::Ws {
                url: "ws://localhost:8000".to_string()
            }
        );
        assert!(parse_conn("ws://").is_err());
        assert!(parse_conn("surrealkv:").is_err());
        assert!(parse_conn("mysql://x").is_err());
    }

    #[test]
    fn rrf_fuses_and_orders() {
        // id "a" is top of two lists; "b" second in one; "c" only appears low.
        let lists = vec![
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            vec!["a".to_string(), "d".to_string()],
        ];
        let fused = rrf(&lists, 60.0);
        assert_eq!(fused[0].0, "a");
        // a: 1/61 + 1/61; b: 1/62; d: 1/62 -> a first, then b/d tie broken by id (b<d).
        let names: Vec<_> = fused.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names[0], "a");
        assert!(fused[0].1 > fused[1].1);
    }

    #[test]
    fn stats_none_verdict() {
        let s = IndexStats::none("lock held");
        assert_eq!(s.verdict(), "index: none (lock held)");
    }
}
