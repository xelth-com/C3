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
use surrealdb::types::{RecordId, SurrealValue};
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
/// Rows per bulk `INSERT` statement (entities, edges and file hashes). One `await` per chunk
/// instead of one per row is the difference between a full-repo build in seconds and in
/// minutes; the value is a compromise between round trips and per-statement value size.
const CHUNK: usize = 400;

/// The four BM25 full-text indexes, `(name, field)`. Defined once at open (so a query-only
/// process works) and, on a from-scratch build, dropped before the bulk load and rebuilt in
/// one pass afterwards — far cheaper than maintaining them incrementally per inserted row.
const BM25_INDEXES: [(&str, &str); 4] = [
    ("code_search", "code"),
    ("name_search", "name"),
    ("path_search", "path"),
    ("summary_search", "summary"),
];

/// An entity row as written to the store (the `id` becomes the record id).
#[derive(Debug, Clone, SurrealValue)]
struct EntityIn {
    id: RecordId,
    kind: String,
    path: String,
    name: String,
    lang: String,
    line_start: i64,
    line_end: i64,
    summary: String,
    code: String,
    content_hash: String,
    generation: String,
}

/// A relation edge for a bulk `INSERT RELATION`.
#[derive(Debug, Clone, SurrealValue)]
struct EdgeIn {
    #[surreal(rename = "in")]
    in_: RecordId,
    out: RecordId,
}

/// A `file_hash` row for the incremental-skip table (the `id` is the path).
#[derive(Debug, Clone, SurrealValue)]
struct FhIn {
    id: RecordId,
    path: String,
    hash: String,
}

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
#[derive(Debug, Clone, Deserialize, SurrealValue)]
struct Row {
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

/// A `(src, dst)` edge id pair read back for batched 1-hop expansion.
#[derive(Debug, Deserialize, SurrealValue)]
struct EdgeRow {
    src: String,
    dst: String,
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
    /// Connect to `backend` and select the namespace/database.
    ///
    /// `ns`/`db` are the repo identity the caller derives. For `ws://`, credentials come
    /// from `C3_INDEX_USER` / `C3_INDEX_PASS` in the environment only (never printed). On
    /// any failure the caller falls back to `Index: none`.
    ///
    /// A build opens with the cheap base-schema definition ([`open`]: the tables and the
    /// analyser, `IF NOT EXISTS` and idempotent); the expensive BM25 full-text indexes are
    /// defined once by the from-scratch load, not on every open. A plain read opens without
    /// any `DEFINE` at all ([`open_read`]) — the tables, analyser and BM25 indexes are already
    /// present from the build, so a query-only process must not spend its budget re-running
    /// `DEFINE`. Both take the same fail-fast single-writer open (RC1).
    pub fn open(backend: Backend, ns: &str, db_name: &str) -> Result<Self, String> {
        Self::open_with(backend, ns, db_name, true)
    }

    /// Open for reading only: no schema/index `DEFINE` on the connection (see [`open`]).
    pub fn open_read(backend: Backend, ns: &str, db_name: &str) -> Result<Self, String> {
        Self::open_with(backend, ns, db_name, false)
    }

    fn open_with(backend: Backend, ns: &str, db_name: &str, define: bool) -> Result<Self, String> {
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
                    if define {
                        define_base_schema(&db)
                            .await
                            .map_err(|e| redact_err(&e.to_string()))?;
                    }
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
                    if let (Ok(user), Ok(pass)) = (
                        std::env::var("C3_INDEX_USER"),
                        std::env::var("C3_INDEX_PASS"),
                    ) {
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
                    if define {
                        define_base_schema(&db)
                            .await
                            .map_err(|e| redact_err(&e.to_string()))?;
                    }
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
    pub fn index(
        &self,
        files: &[(String, String)],
        generation: &str,
    ) -> Result<IndexStats, String> {
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
    pub fn rebuild(
        &self,
        files: &[(String, String)],
        generation: &str,
    ) -> Result<IndexStats, String> {
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

    // 2. Read stored file hashes to skip unchanged files. An empty table means a from-scratch
    //    build (a first `index` or the `rebuild` path, which cleared it) — then there is
    //    nothing to skip and we can drop the BM25 indexes for a one-pass bulk load.
    let stored = stored_file_hashes(db).await?;
    if stored.is_empty() {
        return index_from_scratch(
            db,
            &entities,
            &relations,
            &hash_by_path,
            generation,
            backend_name,
            endpoint,
        )
        .await;
    }

    // 3. Incremental. Detect the files whose content changed and the files that disappeared;
    //    everything else is byte-identical and its rows are left untouched.
    let changed: Vec<String> = current_paths
        .iter()
        .filter(|p| hash_by_path.get(*p).map(|x| x.as_str()) != stored.get(*p).map(|x| x.as_str()))
        .cloned()
        .collect();
    let deleted: Vec<String> = stored
        .keys()
        .filter(|p| !current_paths.contains(*p))
        .cloned()
        .collect();

    // 4. Nothing changed on disk: the derived entities and every edge are identical to what is
    //    stored, so a rebuild must touch no row. Only the generation string can differ (a git
    //    op with no indexed-file change); update that one meta row when it does, nothing else.
    if changed.is_empty() && deleted.is_empty() {
        let stored_gen = read_generation(db).await?;
        if stored_gen.as_deref() != Some(generation) {
            let _: Option<serde_json::Value> = db
                .upsert(("meta", "generation"))
                .content(serde_json::json!({ "value": generation }))
                .await
                .map_err(|e| e.to_string())?;
        }
        return do_stats(db, backend_name, endpoint).await;
    }

    // A changed file rewrites only its own entities and the edges that start or end in them;
    // a deleted file removes its rows. Directory entities are diffed separately (they hang off
    // the file set, not off any single file hash).
    let mut dirty: BTreeSet<String> = BTreeSet::new();
    dirty.extend(changed.iter().cloned());
    dirty.extend(deleted.iter().cloned());
    let dirty_vec: Vec<String> = dirty.iter().cloned().collect();

    // 4a. The entity ids whose path is dirty (fetched before any delete, so edges into a
    //     deleted file's now-removed entities are still resolvable).
    let affected_ids: Vec<RecordId> = {
        let mut resp = db
            .query("SELECT record::id(id) AS id FROM entity WHERE path IN $paths")
            .bind(("paths", dirty_vec.clone()))
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<IdRow> = resp.take(0).map_err(|e| e.to_string())?;
        rows.into_iter()
            .map(|r| RecordId::new("entity", r.id))
            .collect()
    };

    // 4b. Delete the edges that touch any dirty entity (only those can have changed: an edge
    //     changes iff a referenced name in it appeared, disappeared or moved, which makes one
    //     of its endpoint files dirty). Then delete the dirty files' entity and hash rows.
    db.query("DELETE belongs_to WHERE in IN $ids OR out IN $ids; DELETE calls WHERE in IN $ids OR out IN $ids; DELETE relates_to WHERE in IN $ids OR out IN $ids;")
        .bind(("ids", affected_ids.clone()))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    db.query("DELETE entity WHERE path IN $paths; DELETE file_hash WHERE path IN $paths;")
        .bind(("paths", dirty_vec.clone()))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;

    // 4c. Insert the current entities of the changed files (non-directory rows).
    let changed_set: BTreeSet<&str> = changed.iter().map(|s| s.as_str()).collect();
    let new_rows: Vec<EntityIn> = entities
        .iter()
        .filter(|e| e.lang != "dir" && changed_set.contains(e.path.as_str()))
        .map(|e| entity_in(e, generation))
        .collect();
    for chunk in new_rows.chunks(CHUNK) {
        let _: Vec<serde_json::Value> = db
            .insert("entity")
            .content(chunk.to_vec())
            .await
            .map_err(|e| e.to_string())?;
    }

    // 4d. Reconcile directory entities against the current file set (a new directory to add, a
    //     directory whose last file went away to remove). Cheap: there are only a handful.
    let target_dirs: Vec<&super::extract::Entity> =
        entities.iter().filter(|e| e.lang == "dir").collect();
    let target_dir_ids: BTreeSet<&str> = target_dirs.iter().map(|e| e.id.as_str()).collect();
    let stored_dirs: BTreeSet<String> = {
        let mut resp = db
            .query("SELECT record::id(id) AS id FROM entity WHERE lang = 'dir'")
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<IdRow> = resp.take(0).map_err(|e| e.to_string())?;
        rows.into_iter().map(|r| r.id).collect()
    };
    for id in &stored_dirs {
        if !target_dir_ids.contains(id.as_str()) {
            let _: Option<serde_json::Value> = db
                .delete(("entity", id.as_str()))
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    let mut add_dirs: Vec<EntityIn> = Vec::new();
    for &e in &target_dirs {
        if !stored_dirs.contains(e.id.as_str()) {
            add_dirs.push(entity_in(e, generation));
        }
    }
    for chunk in add_dirs.chunks(CHUNK) {
        let _: Vec<serde_json::Value> = db
            .insert("entity")
            .content(chunk.to_vec())
            .await
            .map_err(|e| e.to_string())?;
    }

    // 4e. Re-insert exactly the edges that start or end in a dirty file (the ones deleted in
    //     4b). The full relation set was derived over every current entity, so an edge from an
    //     unchanged file into a renamed symbol is present here and correctly reattached.
    let path_of: BTreeMap<&str, &str> = entities
        .iter()
        .map(|e| (e.id.as_str(), e.path.as_str()))
        .collect();
    let touches_dirty = |r: &super::extract::Relation| -> bool {
        path_of
            .get(r.from.as_str())
            .is_some_and(|p| dirty.contains(*p))
            || path_of
                .get(r.to.as_str())
                .is_some_and(|p| dirty.contains(*p))
    };
    for table in ["belongs_to", "calls", "relates_to"] {
        let edges: Vec<EdgeIn> = relations
            .iter()
            .filter(|r| r.kind.table() == table && touches_dirty(r))
            .map(|r| EdgeIn {
                in_: RecordId::new("entity", r.from.clone()),
                out: RecordId::new("entity", r.to.clone()),
            })
            .collect();
        for chunk in edges.chunks(CHUNK) {
            let _: Vec<serde_json::Value> = db
                .insert(table)
                .relation(chunk.to_vec())
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    // 4f. Record the changed files' hashes and the generation.
    for p in &changed {
        if let Some(h) = hash_by_path.get(p) {
            let _: Option<serde_json::Value> = db
                .upsert(("file_hash", p.as_str()))
                .content(serde_json::json!({ "path": p, "hash": h }))
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    let _: Option<serde_json::Value> = db
        .upsert(("meta", "generation"))
        .content(serde_json::json!({ "value": generation }))
        .await
        .map_err(|e| e.to_string())?;

    do_stats(db, backend_name, endpoint).await
}

/// The from-scratch bulk load: drop the BM25 indexes, chunk-`INSERT` every entity, edge and
/// file hash, then rebuild the four indexes in a single pass. One `await` per chunk.
async fn index_from_scratch<C: Connection>(
    db: &Surreal<C>,
    entities: &[super::extract::Entity],
    relations: &[super::extract::Relation],
    hash_by_path: &BTreeMap<String, String>,
    generation: &str,
    backend_name: &str,
    endpoint: &str,
) -> Result<IndexStats, String> {
    // Ensure the tables and analyser exist (idempotent) before the bulk load, independent of
    // how the handle was opened.
    define_base_schema(db).await.map_err(|e| e.to_string())?;
    remove_bm25_indexes(db).await?;
    let rows: Vec<EntityIn> = entities.iter().map(|e| entity_in(e, generation)).collect();
    for chunk in rows.chunks(CHUNK) {
        let _: Vec<serde_json::Value> = db
            .insert("entity")
            .content(chunk.to_vec())
            .await
            .map_err(|e| e.to_string())?;
    }
    define_bm25_indexes(db).await?;

    db.query("DELETE belongs_to; DELETE calls; DELETE relates_to;")
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    for table in ["belongs_to", "calls", "relates_to"] {
        let edges: Vec<EdgeIn> = relations
            .iter()
            .filter(|r| r.kind.table() == table)
            .map(|r| EdgeIn {
                in_: RecordId::new("entity", r.from.clone()),
                out: RecordId::new("entity", r.to.clone()),
            })
            .collect();
        for chunk in edges.chunks(CHUNK) {
            let _: Vec<serde_json::Value> = db
                .insert(table)
                .relation(chunk.to_vec())
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    let fh: Vec<FhIn> = hash_by_path
        .iter()
        .map(|(p, h)| FhIn {
            id: RecordId::new("file_hash", p.clone()),
            path: p.clone(),
            hash: h.clone(),
        })
        .collect();
    for chunk in fh.chunks(CHUNK) {
        let _: Vec<serde_json::Value> = db
            .insert("file_hash")
            .content(chunk.to_vec())
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

/// Build the store row for an entity.
fn entity_in(e: &super::extract::Entity, generation: &str) -> EntityIn {
    EntityIn {
        id: RecordId::new("entity", e.id.clone()),
        kind: e.kind.as_str().to_string(),
        path: e.path.clone(),
        name: e.name.clone(),
        lang: e.lang.clone(),
        line_start: e.line_start as i64,
        line_end: e.line_end as i64,
        summary: e.summary.clone(),
        code: e.code.clone(),
        content_hash: e.content_hash.clone(),
        generation: generation.to_string(),
    }
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

/// The three relation tables walked in 1-hop expansion, in fixed order, with the label a
/// pulled-in hit carries.
const EXPAND_TABLES: [(&str, &str); 3] = [
    ("calls", "derived: calls"),
    ("belongs_to", "derived: belongs_to"),
    ("relates_to", "derived: relates_to"),
];

async fn do_retrieve<C: Connection>(
    db: &Surreal<C>,
    query: &str,
    budget: usize,
) -> Result<Vec<Hit>, String> {
    let profile = std::env::var("C3_INDEX_PROFILE").is_ok();

    // 1. The four BM25 legs in one round trip, fused by RRF.
    let t = std::time::Instant::now();
    let rankings = bm25_legs(db, query).await?;
    prof(profile, "bm25_legs (4-in-1)", t);

    let fused = super::rrf(&rankings, 60.0);
    let primary_ids: Vec<String> = fused
        .iter()
        .take(TOP_PRIMARY)
        .map(|(id, _)| id.clone())
        .collect();
    let score_of: BTreeMap<String, f64> = fused.into_iter().collect();

    // 2. All primary rows in one round trip.
    let t = std::time::Instant::now();
    let primary_rows = fetch_rows(db, &primary_ids).await?;
    prof(profile, "fetch_primary", t);

    let mut hits: Vec<Hit> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for id in &primary_ids {
        if let Some(row) = primary_rows.get(id) {
            seen.insert(id.clone());
            hits.push(row_to_hit(
                row.clone(),
                *score_of.get(id).unwrap_or(&0.0),
                Vec::new(),
            ));
        }
    }

    // 3. Outgoing neighbours of every primary, one round trip per relation table (three).
    let t = std::time::Instant::now();
    let primary_recs: Vec<RecordId> = primary_ids
        .iter()
        .map(|id| RecordId::new("entity", id.clone()))
        .collect();
    let mut adjacency: Vec<BTreeMap<String, Vec<String>>> = Vec::with_capacity(EXPAND_TABLES.len());
    for (table, _) in EXPAND_TABLES {
        let sql = format!(
            "SELECT record::id(in) AS src, record::id(out) AS dst FROM {table} WHERE in IN $ids ORDER BY src, dst"
        );
        let mut resp = db
            .query(sql)
            .bind(("ids", primary_recs.clone()))
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<EdgeRow> = resp.take(0).map_err(|e| e.to_string())?;
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for r in rows {
            m.entry(r.src).or_default().push(r.dst);
        }
        adjacency.push(m);
    }

    // Choose expansion targets in the deterministic primary-major, table-order walk (up to
    // EXPAND_PER_RELATION per table per primary, capped at EXPAND_TOTAL_CAP).
    let mut expand: Vec<(String, &'static str, String)> = Vec::new();
    let mut expanded = 0usize;
    'outer: for id in &primary_ids {
        for (i, (_, label)) in EXPAND_TABLES.iter().enumerate() {
            if let Some(list) = adjacency[i].get(id) {
                for nid in list.iter().take(EXPAND_PER_RELATION) {
                    if seen.contains(nid) {
                        continue;
                    }
                    seen.insert(nid.clone());
                    expand.push((nid.clone(), label, id.clone()));
                    expanded += 1;
                    if expanded >= EXPAND_TOTAL_CAP {
                        break 'outer;
                    }
                }
            }
        }
    }

    // 4. All expansion rows in one round trip.
    let expand_ids: Vec<String> = expand.iter().map(|(nid, _, _)| nid.clone()).collect();
    let expand_rows = fetch_rows(db, &expand_ids).await?;
    for (nid, label, primary) in &expand {
        if let Some(row) = expand_rows.get(nid) {
            let score = score_of.get(primary).map(|s| s * 0.5).unwrap_or(0.0);
            hits.push(row_to_hit(row.clone(), score, vec![label.to_string()]));
        }
    }
    prof(profile, "expansion (3+1 round trips)", t);

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

/// The BM25 legs as one multi-statement query — one round trip. SurrealDB's `@@` match is
/// conjunctive (a document must contain every term in the searched text), so a multi-word
/// query is split into terms and each `(term × field)` pair is its own ranked leg; RRF
/// ([`super::rrf`]) then fuses them, giving OR-style relevance where a document scores for
/// each term it matches in any field. A single-term query is exactly the original four legs.
/// `search::score(1)` references the inline `@1@` match within its own statement; a secondary
/// `id` sort makes ties deterministic.
async fn bm25_legs<C: Connection>(
    db: &Surreal<C>,
    query: &str,
) -> Result<Vec<Vec<String>>, String> {
    let terms = tokenize_query(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    // The `@N@` MATCHES clause needs a plain bound param on its right (an indexed array access
    // is not recognised), so each term is its own named param `$tK`.
    let fields = ["code", "name", "path", "summary"];
    let keys: Vec<String> = (0..terms.len()).map(|i| format!("t{i}")).collect();
    let mut sql = String::new();
    for key in &keys {
        for field in fields {
            sql.push_str(&format!(
                "SELECT record::id(id) AS id, search::score(1) AS s FROM entity WHERE {field} @1@ ${key} ORDER BY s DESC, id ASC LIMIT {BM25_LIMIT};"
            ));
        }
    }
    let mut q = db.query(sql);
    for (key, term) in keys.iter().zip(terms.iter()) {
        q = q.bind((key.as_str(), term.clone()));
    }
    let mut resp = q.await.map_err(|e| e.to_string())?;
    let leg_count = terms.len() * fields.len();
    let mut out = Vec::with_capacity(leg_count);
    for i in 0..leg_count {
        let rows: Vec<IdRow> = resp.take(i).map_err(|e| e.to_string())?;
        out.push(rows.into_iter().map(|r| r.id).collect());
    }
    Ok(out)
}

/// Split a query into distinct search terms: maximal `[A-Za-z0-9_]` runs, lowercased, order
/// preserved, deduped, and capped so a long natural-language brief cannot explode the number
/// of legs. A single identifier tokenises to itself, so a one-word query is unchanged.
fn tokenize_query(query: &str) -> Vec<String> {
    const MAX_TERMS: usize = 32;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<String>, seen: &mut BTreeSet<String>| {
        if !cur.is_empty() {
            let t = std::mem::take(cur).to_lowercase();
            if seen.insert(t.clone()) {
                out.push(t);
            }
        }
    };
    for c in query.chars() {
        if c.is_alphanumeric() || c == '_' {
            cur.push(c);
        } else {
            flush(&mut cur, &mut out, &mut seen);
        }
        if out.len() >= MAX_TERMS {
            return out;
        }
    }
    flush(&mut cur, &mut out, &mut seen);
    out
}

/// Fetch the entity rows for a set of ids in one round trip, keyed by id.
async fn fetch_rows<C: Connection>(
    db: &Surreal<C>,
    ids: &[String],
) -> Result<BTreeMap<String, Row>, String> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let recs: Vec<RecordId> = ids
        .iter()
        .map(|id| RecordId::new("entity", id.clone()))
        .collect();
    let mut resp = db
        .query("SELECT record::id(id) AS id, kind, path, name, line_start, line_end, code, summary FROM entity WHERE id IN $ids")
        .bind(("ids", recs))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<Row> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| (r.id.clone(), r)).collect())
}

/// Emit a phase timing line to stderr when `C3_INDEX_PROFILE` is set (measurement only).
fn prof(on: bool, label: &str, since: std::time::Instant) {
    if on {
        eprintln!("[c3-index] {label}: {:.3}s", since.elapsed().as_secs_f64());
    }
}

async fn do_stats<C: Connection>(
    db: &Surreal<C>,
    backend_name: &str,
    endpoint: &str,
) -> Result<IndexStats, String> {
    let entities = count(db, "SELECT count() FROM entity GROUP ALL").await?;
    let files = count(
        db,
        "SELECT count() FROM entity WHERE kind = 'file' GROUP ALL",
    )
    .await?;
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

/// Define the base lexical schema — the tables and the code analyser only (no BM25 indexes,
/// no HNSW/embeddings). `IF NOT EXISTS` throughout, so it is idempotent and cheap; a build
/// runs it at open, and the far more expensive BM25 full-text indexes are defined once by the
/// from-scratch load ([`define_bm25_indexes`]) rather than on every open.
async fn define_base_schema<C: Connection>(db: &Surreal<C>) -> Result<(), surrealdb::Error> {
    db.query(
        "DEFINE TABLE IF NOT EXISTS entity SCHEMALESS;
         DEFINE ANALYZER IF NOT EXISTS code_analyzer TOKENIZERS blank,class,camel,punct FILTERS lowercase,ascii;
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

/// Define the four BM25 full-text indexes (idempotent). On a from-scratch build this runs
/// once after the bulk load, so each index is built in a single pass instead of being
/// maintained row by row.
async fn define_bm25_indexes<C: Connection>(db: &Surreal<C>) -> Result<(), String> {
    define_bm25_indexes_e(db).await.map_err(|e| e.to_string())
}

async fn define_bm25_indexes_e<C: Connection>(db: &Surreal<C>) -> Result<(), surrealdb::Error> {
    let mut sql = String::new();
    for (name, field) in BM25_INDEXES {
        sql.push_str(&format!(
            "DEFINE INDEX IF NOT EXISTS {name} ON entity FIELDS {field} FULLTEXT ANALYZER code_analyzer BM25;"
        ));
    }
    db.query(sql).await?.check()?;
    Ok(())
}

/// Drop the four BM25 indexes before a bulk load so inserts do not maintain them.
async fn remove_bm25_indexes<C: Connection>(db: &Surreal<C>) -> Result<(), String> {
    let mut sql = String::new();
    for (name, _) in BM25_INDEXES {
        sql.push_str(&format!("REMOVE INDEX IF EXISTS {name} ON entity;"));
    }
    db.query(sql)
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Redact a store/connection error before it is shown (invariant 4/5): drop any credential
/// that a `ws://` URL or driver message might echo.
fn redact_err(msg: &str) -> String {
    let (red, _) = crate::pack::redact::redact(msg);
    red
}
