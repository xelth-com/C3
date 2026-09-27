//! The SurrealDB-backed `ContextIndex` (feature `index-surreal`).
//!
//! One embedded `surrealkv` store per project, opened with the direct local engine exactly
//! as xelth.rs does (`Surreal::new::<SurrealKv>(path)` — reliable on Windows, and the RC1
//! single-writer open that fails fast with os error 33 when the store is already held), or a
//! `ws://` per-user server reached through the `any` engine. Both engines implement
//! `surrealdb::Connection`, so the query logic is written once, generic over the engine, and
//! a small [`Handle`] enum dispatches. The schema is the xelth.rs shape without
//! embeddings/HNSW: a schemaless `entity` table with four BM25 full-text indexes (code,
//! name, path, summary) on one code analyser, the `belongs_to` / `calls` / `relates_to`
//! relation tables, a `file_hash` table for incremental skip and a `meta` table for the
//! generation. Retrieval runs the four BM25 legs, fuses them by reciprocal rank in Rust
//! ([`super::rrf`]) and does a bounded 1-hop expansion.
//!
//! The API is async; C3 is otherwise synchronous, so we own a current-thread tokio runtime
//! and `block_on` each operation.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use surrealdb::engine::any::{self, Any};
use surrealdb::engine::local::{Db, SurrealKv};
use surrealdb::types::SurrealValue;
use surrealdb::{Connection, Surreal};

use super::extract::{derive_relations, extract_file, file_hash};
use super::{Backend, Hit, IndexStats};
use crate::pack::budget::estimate_tokens;

/// How many fused primary entities to open a retrieval on.
const TOP_PRIMARY: usize = 12;
/// Neighbours pulled per relation table per primary during 1-hop expansion.
const EXPAND_PER_RELATION: usize = 2;
/// Overall cap on expansion hits regardless of primary count.
const EXPAND_TOTAL_CAP: usize = 24;
/// BM25 candidates fetched per field before fusion.
const BM25_LIMIT: usize = 30;

/// A handle on an open store: the embedded local engine or a remote `any` connection.
enum Handle {
    Local(Surreal<Db>),
    Any(Surreal<Any>),
}

/// A handle on an open SurrealDB index.
pub struct SurrealIndex {
    rt: tokio::runtime::Runtime,
    handle: Handle,
    backend: Backend,
    endpoint: String,
}

/// An entity row read back from the store.
#[derive(Debug, Deserialize, SurrealValue)]
struct Row {
    #[allow(dead_code)]
    id: String,
    kind: String,
    path: String,
    name: String,
    #[serde(default)]
    line_start: i64,
    #[serde(default)]
    line_end: i64,
    #[serde(default)]
    code: String,
    #[serde(default)]
    summary: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct IdRow {
    id: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct CountRow {
    count: i64,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct MetaRow {
    value: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct FhRow {
    path: String,
    hash: String,
}

impl SurrealIndex {
    /// Connect to `backend`, select the namespace/database and define the schema.
    ///
    /// `ns`/`db` are the repo identity the caller derives. For `ws://`, credentials come
    /// from `C3_INDEX_USER` / `C3_INDEX_PASS` in the environment only (never printed). On
    /// any failure the caller falls back to `Index: none`.
    pub fn open(backend: Backend, ns: &str, db_name: &str) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| redact_err(&e.to_string()))?;

        let ns = ns.to_string();
        let db_name = db_name.to_string();
        let (handle, endpoint) = match &backend {
            Backend::SurrealKv(path) => {
                let endpoint = path.to_string_lossy().replace('\\', "/");
                let ep = endpoint.clone();
                let handle = rt.block_on(async move {
                    let db = Surreal::new::<SurrealKv>(ep)
                        .await
                        .map_err(|e| redact_err(&e.to_string()))?;
                    db.use_ns(&ns)
                        .use_db(&db_name)
                        .await
                        .map_err(|e| redact_err(&e.to_string()))?;
                    define_schema(&db).await.map_err(|e| redact_err(&e.to_string()))?;
                    Ok::<_, String>(Handle::Local(db))
                })?;
                (handle, endpoint)
            }
            Backend::Ws { url } => {
                let endpoint = url.clone();
                let ep = endpoint.clone();
                let handle = rt.block_on(async move {
                    let db = any::connect(ep)
                        .await
                        .map_err(|e| redact_err(&e.to_string()))?;
                    if let (Ok(user), Ok(pass)) =
                        (std::env::var("C3_INDEX_USER"), std::env::var("C3_INDEX_PASS"))
                    {
                        db.signin(surrealdb::opt::auth::Root {
                            username: user,
                            password: pass,
                        })
                        .await
                        .map_err(|e| redact_err(&e.to_string()))?;
                    }
                    db.use_ns(&ns)
                        .use_db(&db_name)
                        .await
                        .map_err(|e| redact_err(&e.to_string()))?;
                    define_schema(&db).await.map_err(|e| redact_err(&e.to_string()))?;
                    Ok::<_, String>(Handle::Any(db))
                })?;
                (handle, endpoint)
            }
            Backend::None => return Err("backend is none".to_string()),
        };

        Ok(SurrealIndex {
            rt,
            handle,
            backend,
            endpoint,
        })
    }

    /// Index `files` (each `(rel_posix_path, content)`) at `generation`.
    pub fn index(&self, files: &[(String, String)], generation: &str) -> Result<IndexStats, String> {
        let name = self.backend.name().to_string();
        let endpoint = self.endpoint.clone();
        self.rt
            .block_on(async {
                match &self.handle {
                    Handle::Local(db) => do_index(db, files, generation, &name, &endpoint).await,
                    Handle::Any(db) => do_index(db, files, generation, &name, &endpoint).await,
                }
            })
            .map_err(|e| redact_err(&e))
    }

    /// Drop everything, then index — the acceptance-test path.
    pub fn rebuild(&self, files: &[(String, String)], generation: &str) -> Result<IndexStats, String> {
        let name = self.backend.name().to_string();
        let endpoint = self.endpoint.clone();
        self.rt
            .block_on(async {
                match &self.handle {
                    Handle::Local(db) => {
                        drop_all(db).await?;
                        do_index(db, files, generation, &name, &endpoint).await
                    }
                    Handle::Any(db) => {
                        drop_all(db).await?;
                        do_index(db, files, generation, &name, &endpoint).await
                    }
                }
            })
            .map_err(|e| redact_err(&e))
    }

    /// BM25 retrieval fused by RRF plus a bounded 1-hop expansion, trimmed to `budget`.
    pub fn retrieve(&self, query: &str, budget: usize) -> Result<Vec<Hit>, String> {
        self.rt
            .block_on(async {
                match &self.handle {
                    Handle::Local(db) => do_retrieve(db, query, budget).await,
                    Handle::Any(db) => do_retrieve(db, query, budget).await,
                }
            })
            .map_err(|e| redact_err(&e))
    }

    /// Counts per table, the generation and backend identity.
    pub fn stats(&self) -> Result<IndexStats, String> {
        let name = self.backend.name().to_string();
        let endpoint = self.endpoint.clone();
        self.rt
            .block_on(async {
                match &self.handle {
                    Handle::Local(db) => do_stats(db, &name, &endpoint).await,
                    Handle::Any(db) => do_stats(db, &name, &endpoint).await,
                }
            })
            .map_err(|e| redact_err(&e))
    }
}

// --------------------------------------------------------------------------- generic core

async fn drop_all<C: Connection>(db: &Surreal<C>) -> Result<(), String> {
    db.query("DELETE entity; DELETE belongs_to; DELETE calls; DELETE relates_to; DELETE file_hash; DELETE meta;")
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn do_index<C: Connection>(
    db: &Surreal<C>,
    files: &[(String, String)],
    generation: &str,
    backend_name: &str,
    endpoint: &str,
) -> Result<IndexStats, String> {
    // 1. Extract every file in memory (deterministic, lexical), derive relations.
    let mut entities = Vec::new();
    let mut current_paths: BTreeSet<String> = BTreeSet::new();
    let mut hash_by_path: BTreeMap<String, String> = BTreeMap::new();
    for (rel, content) in files {
        current_paths.insert(rel.clone());
        hash_by_path.insert(rel.clone(), file_hash(content));
        entities.extend(extract_file(rel, content, generation));
    }
    let (dir_entities, relations) = derive_relations(&entities);
    entities.extend(dir_entities);

    // 2. Read stored file hashes to skip unchanged files.
    let stored = stored_file_hashes(db).await?;

    // 3. Orphan cleanup: entities whose path is gone.
    let mut orphan_paths: Vec<String> = stored
        .keys()
        .filter(|p| !current_paths.contains(*p))
        .cloned()
        .collect();
    orphan_paths.sort();
    for p in &orphan_paths {
        db.query("DELETE entity WHERE path = $p; DELETE file_hash WHERE path = $p;")
            .bind(("p", p.clone()))
            .await
            .map_err(|e| e.to_string())?
            .check()
            .map_err(|e| e.to_string())?;
    }

    // 4. Upsert entities of changed/new files (and all synthetic dir entities). A file whose
    //    content hash is unchanged is skipped — its rows are already present and identical.
    for e in &entities {
        let is_dir = e.lang == "dir";
        if !is_dir {
            if let Some(h) = stored.get(&e.path) {
                if hash_by_path.get(&e.path).map(|x| x.as_str()) == Some(h.as_str()) {
                    continue;
                }
            }
        }
        let _: Option<serde_json::Value> = db
            .upsert(("entity", e.id.as_str()))
            .content(serde_json::json!({
                "kind": e.kind.as_str(),
                "path": e.path,
                "name": e.name,
                "lang": e.lang,
                "line_start": e.line_start as i64,
                "line_end": e.line_end as i64,
                "summary": e.summary,
                "code": e.code,
                "content_hash": e.content_hash,
                "generation": generation,
            }))
            .await
            .map_err(|e| e.to_string())?;
    }

    // 5. Rewrite all relation edges (deterministic; cheap for a lexical index).
    db.query("DELETE belongs_to; DELETE calls; DELETE relates_to;")
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    for r in &relations {
        let stmt = format!(
            "RELATE (type::record('entity', $a)) -> {} -> (type::record('entity', $b))",
            r.kind.table()
        );
        db.query(stmt)
            .bind(("a", r.from.clone()))
            .bind(("b", r.to.clone()))
            .await
            .map_err(|e| e.to_string())?
            .check()
            .map_err(|e| e.to_string())?;
    }

    // 6. Record file hashes and the generation.
    for (p, h) in &hash_by_path {
        let _: Option<serde_json::Value> = db
            .upsert(("file_hash", p.as_str()))
            .content(serde_json::json!({ "path": p, "hash": h }))
            .await
            .map_err(|e| e.to_string())?;
    }
    let _: Option<serde_json::Value> = db
        .upsert(("meta", "generation"))
        .content(serde_json::json!({ "value": generation }))
        .await
        .map_err(|e| e.to_string())?;

    do_stats(db, backend_name, endpoint).await
}

async fn stored_file_hashes<C: Connection>(
    db: &Surreal<C>,
) -> Result<BTreeMap<String, String>, String> {
    let mut resp = db
        .query("SELECT path, hash FROM file_hash")
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<FhRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| (r.path, r.hash)).collect())
}

async fn do_retrieve<C: Connection>(
    db: &Surreal<C>,
    query: &str,
    budget: usize,
) -> Result<Vec<Hit>, String> {
    let mut rankings: Vec<Vec<String>> = Vec::new();
    for field in ["code", "name", "path", "summary"] {
        rankings.push(bm25_leg(db, field, query).await?);
    }
    let fused = super::rrf(&rankings, 60.0);
    let primary_ids: Vec<String> = fused.iter().take(TOP_PRIMARY).map(|(id, _)| id.clone()).collect();
    let score_of: BTreeMap<String, f64> = fused.into_iter().collect();

    let mut hits: Vec<Hit> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for id in &primary_ids {
        if let Some(row) = fetch_row(db, id).await? {
            seen.insert(id.clone());
            hits.push(row_to_hit(row, *score_of.get(id).unwrap_or(&0.0), Vec::new()));
        }
    }

    // 1-hop expansion over the three relation tables (outgoing edges).
    let mut expanded = 0usize;
    'outer: for id in &primary_ids {
        for (table, label) in [
            ("calls", "derived: calls"),
            ("belongs_to", "derived: belongs_to"),
            ("relates_to", "derived: relates_to"),
        ] {
            let neighbours = neighbours(db, table, id).await?;
            for nid in neighbours.into_iter().take(EXPAND_PER_RELATION) {
                if seen.contains(&nid) {
                    continue;
                }
                if let Some(row) = fetch_row(db, &nid).await? {
                    seen.insert(nid.clone());
                    let score = score_of.get(id).map(|s| s * 0.5).unwrap_or(0.0);
                    hits.push(row_to_hit(row, score, vec![label.to_string()]));
                    expanded += 1;
                    if expanded >= EXPAND_TOTAL_CAP {
                        break 'outer;
                    }
                }
            }
        }
    }

    // Budget trim (0 = no cap): primaries are ordered by score, expansions follow.
    if budget > 0 {
        let mut used = 0usize;
        let mut kept = Vec::new();
        for h in hits {
            let ext = h.path.rsplit('.').next().unwrap_or("");
            let t = estimate_tokens(&h.snippet, ext);
            if used + t > budget && !kept.is_empty() {
                break;
            }
            used += t;
            kept.push(h);
        }
        hits = kept;
    }
    Ok(hits)
}

async fn bm25_leg<C: Connection>(db: &Surreal<C>, field: &str, query: &str) -> Result<Vec<String>, String> {
    // The `@1@` reference number is an inline literal (v3 Rule 2/6); the search text is
    // bound so it is never concatenated into the SQL. `field` is from a fixed allowlist.
    let sql = format!(
        "SELECT record::id(id) AS id, search::score(1) AS s FROM entity WHERE {field} @1@ $q ORDER BY s DESC LIMIT {BM25_LIMIT}"
    );
    let mut resp = db
        .query(sql)
        .bind(("q", query.to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<IdRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

async fn neighbours<C: Connection>(db: &Surreal<C>, table: &str, id: &str) -> Result<Vec<String>, String> {
    let sql = format!("SELECT record::id(out) AS id FROM {table} WHERE in = type::record('entity', $id)");
    let mut resp = db
        .query(sql)
        .bind(("id", id.to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<IdRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

async fn fetch_row<C: Connection>(db: &Surreal<C>, id: &str) -> Result<Option<Row>, String> {
    let mut resp = db
        .query("SELECT record::id(id) AS id, kind, path, name, line_start, line_end, code, summary FROM (type::record('entity', $id))")
        .bind(("id", id.to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<Row> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().next())
}

async fn do_stats<C: Connection>(db: &Surreal<C>, backend_name: &str, endpoint: &str) -> Result<IndexStats, String> {
    let entities = count(db, "SELECT count() FROM entity GROUP ALL").await?;
    let files = count(db, "SELECT count() FROM entity WHERE kind = 'file' GROUP ALL").await?;
    let belongs_to = count(db, "SELECT count() FROM belongs_to GROUP ALL").await?;
    let calls = count(db, "SELECT count() FROM calls GROUP ALL").await?;
    let relates_to = count(db, "SELECT count() FROM relates_to GROUP ALL").await?;
    let generation = read_generation(db).await?;
    Ok(IndexStats {
        backend: backend_name.to_string(),
        path: Some(endpoint.to_string()),
        reason: None,
        entities,
        files,
        belongs_to,
        calls,
        relates_to,
        generation,
    })
}

async fn count<C: Connection>(db: &Surreal<C>, sql: &str) -> Result<u64, String> {
    let mut resp = db.query(sql).await.map_err(|e| e.to_string())?;
    let rows: Vec<CountRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.first().map(|r| r.count.max(0) as u64).unwrap_or(0))
}

async fn read_generation<C: Connection>(db: &Surreal<C>) -> Result<Option<String>, String> {
    let mut resp = db
        .query("SELECT `value` FROM meta:generation")
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<MetaRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().next().map(|r| r.value))
}

fn row_to_hit(row: Row, score: f64, relations: Vec<String>) -> Hit {
    Hit {
        path: row.path,
        name: row.name,
        kind: row.kind,
        line_start: row.line_start.max(0) as usize,
        line_end: row.line_end.max(0) as usize,
        snippet: row.code,
        score,
        relations,
    }
}

/// Define the lexical schema (four BM25 indexes on one analyser; no HNSW/embeddings).
async fn define_schema<C: Connection>(db: &Surreal<C>) -> Result<(), surrealdb::Error> {
    db.query(
        "DEFINE TABLE IF NOT EXISTS entity SCHEMALESS;
         DEFINE ANALYZER IF NOT EXISTS code_analyzer TOKENIZERS blank,class,camel,punct FILTERS lowercase,ascii;
         DEFINE INDEX IF NOT EXISTS code_search ON entity FIELDS code FULLTEXT ANALYZER code_analyzer BM25;
         DEFINE INDEX IF NOT EXISTS name_search ON entity FIELDS name FULLTEXT ANALYZER code_analyzer BM25;
         DEFINE INDEX IF NOT EXISTS path_search ON entity FIELDS path FULLTEXT ANALYZER code_analyzer BM25;
         DEFINE INDEX IF NOT EXISTS summary_search ON entity FIELDS summary FULLTEXT ANALYZER code_analyzer BM25;
         DEFINE TABLE IF NOT EXISTS belongs_to TYPE RELATION;
         DEFINE TABLE IF NOT EXISTS calls TYPE RELATION;
         DEFINE TABLE IF NOT EXISTS relates_to TYPE RELATION;
         DEFINE TABLE IF NOT EXISTS file_hash SCHEMALESS;
         DEFINE TABLE IF NOT EXISTS meta SCHEMALESS;",
    )
    .await?
    .check()?;
    Ok(())
}

/// Redact a store/connection error before it is shown (invariant 4/5): drop any credential
/// that a `ws://` URL or driver message might echo.
fn redact_err(msg: &str) -> String {
    let (red, _) = crate::pack::redact::redact(msg);
    red
}
