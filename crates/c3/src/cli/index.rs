//! `c3 index` — build or query the derived context index (milestone 8).
//!
//! The pipeline lives in [`crate::index`]; this owns the clap surface, the embedded
//! single-writer lock (`<collab>/.c3/index.lock`, the same exclusive share-mode open as
//! `c3-core`'s `TaskLock`) and the one-line verdict. Any open failure — the lock is held or
//! the embedded store returns the surrealkv lock violation (os error 33) — is reported as
//! `index: none (<reason>)` with no retry loop, exiting 0 for a query and 1 for a build
//! (DESIGN §7, RC1). With `--no-default-features` the whole backend is compiled out and
//! every action reports `none`.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};

use crate::index::{self, IndexStats};
use crate::providers;

/// Arguments of `c3 index`.
#[derive(Args, Debug)]
pub struct IndexArgs {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Subcommand, Debug)]
pub enum Action {
    /// Discover, extract and upsert entities and relations (incremental by file hash).
    Build(BuildArgs),
    /// Drop the index and re-index from scratch (the acceptance-test path).
    Rebuild(BuildArgs),
    /// BM25 + reciprocal-rank retrieval with a bounded 1-hop expansion.
    Query(QueryArgs),
    /// Counts per table, the generation, the backend and its path.
    Stats(StatsArgs),
}

#[derive(Args, Debug, Default)]
pub struct BuildArgs {
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    /// Connection string: `none` | `surrealkv:<path>` | `ws://host`. Default: an embedded
    /// store at `<collab>/.c3/index/`.
    #[arg(long)]
    pub conn: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct QueryArgs {
    /// The search text. Several queries may be given; they share one store open, so each
    /// query after the first pays only its own retrieval, not another open.
    pub text: Vec<String>,
    /// A file of queries, one per line (blank lines and `#` comments ignored); appended after
    /// any positional queries. All share the single open.
    #[arg(long)]
    pub queries_file: Option<String>,
    /// Token budget for the returned hits (0 = no cap).
    #[arg(long, default_value_t = 4000)]
    pub budget: usize,
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    #[arg(long)]
    pub conn: Option<String>,
    /// Emit JSON instead of the rendered hits.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Default)]
pub struct StatsArgs {
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    #[arg(long)]
    pub conn: Option<String>,
    /// Emit JSON instead of the one-line summary.
    #[arg(long)]
    pub json: bool,
}

/// Resolved context common to every action.
struct Ctx {
    // Read only on the feature-on paths (open/lock/discover/generation); the feature-off
    // build parses `conn` and reports `none`, so these two are dead there.
    #[cfg_attr(not(feature = "index-surreal"), allow(dead_code))]
    repo_root: PathBuf,
    #[cfg_attr(not(feature = "index-surreal"), allow(dead_code))]
    collab_root: PathBuf,
    conn: String,
}

fn resolve(collab_dir: &str, conn: Option<String>) -> Ctx {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, collab_dir);
    let conn = conn.unwrap_or_else(|| {
        let dir = collab_root.join(".c3").join("index");
        format!("surrealkv:{}", dir.to_string_lossy().replace('\\', "/"))
    });
    Ctx {
        repo_root,
        collab_root,
        conn,
    }
}

/// A stable-ish database name from the repository directory basename.
#[cfg_attr(not(feature = "index-surreal"), allow(dead_code))]
fn repo_db_name(repo_root: &Path) -> String {
    let base = repo_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());
    let slug: String = base
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if slug.is_empty() {
        "repo".to_string()
    } else {
        slug
    }
}

pub fn run(args: IndexArgs) -> i32 {
    // The backend is async and we bridge it with a current-thread tokio runtime, so every
    // `block_on` runs SurrealDB's (deeply recursive) SQL parser and executor on the calling
    // thread. On Windows the process main thread has only a 1 MiB stack, which overflows on
    // a real repository; run the whole dispatch on a dedicated thread with a generous stack.
    std::thread::Builder::new()
        .name("c3-index".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || run_inner(args))
        .expect("spawn c3-index thread")
        .join()
        .expect("c3-index thread panicked")
}

fn run_inner(args: IndexArgs) -> i32 {
    match args.action {
        Action::Build(a) => build(a, false),
        Action::Rebuild(a) => build(a, true),
        Action::Query(a) => query(a),
        Action::Stats(a) => stats(a),
    }
}

fn build(args: BuildArgs, rebuild: bool) -> i32 {
    let ctx = resolve(&args.collab_dir, args.conn);
    let backend = match index::parse_conn(&ctx.conn) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    match backend {
        index::Backend::None => {
            println!(
                "{} {}",
                index::PREFIX,
                IndexStats::none("connection string is none").verdict()
            );
            0
        }
        _ => build_backend(&ctx, backend, rebuild),
    }
}

#[cfg(not(feature = "index-surreal"))]
fn build_backend(_ctx: &Ctx, _backend: index::Backend, _rebuild: bool) -> i32 {
    println!(
        "{} {}",
        index::PREFIX,
        IndexStats::none("index-surreal feature disabled").verdict()
    );
    1
}

#[cfg(feature = "index-surreal")]
fn build_backend(ctx: &Ctx, backend: index::Backend, rebuild: bool) -> i32 {
    // Embedded stores take the single-writer lock first (fail-fast, no retry). A `ws://`
    // server handles its own concurrency, so it needs no local lock.
    let _lock;
    if let index::Backend::SurrealKv(_) = &backend {
        match take_index_lock(&ctx.collab_root) {
            Ok(l) => _lock = l,
            Err(reason) => {
                println!("{} {}", index::PREFIX, IndexStats::none(reason).verdict());
                return 1;
            }
        }
    }

    let ns = "c3";
    let db_name = repo_db_name(&ctx.repo_root);
    let idx = match index::SurrealIndex::open(backend, ns, &db_name) {
        Ok(i) => i,
        Err(reason) => {
            println!("{} {}", index::PREFIX, IndexStats::none(reason).verdict());
            return 1;
        }
    };

    let generation = index::generation(&ctx.repo_root);
    let files = discover_files(&ctx.repo_root);
    let res = if rebuild {
        idx.rebuild(&files, &generation)
    } else {
        idx.index(&files, &generation)
    };
    match res {
        Ok(stats) => {
            println!("{} {}", index::PREFIX, stats.verdict());
            if let Some(g) = &stats.generation {
                println!("  generation: {g}");
            }
            0
        }
        Err(reason) => {
            println!("{} {}", index::PREFIX, IndexStats::none(reason).verdict());
            1
        }
    }
}

fn query(args: QueryArgs) -> i32 {
    let ctx = resolve(&args.collab_dir, args.conn);
    let mut queries: Vec<String> = args.text.clone();
    if let Some(path) = &args.queries_file {
        match std::fs::read_to_string(path) {
            Ok(body) => {
                for line in body.lines() {
                    let t = line.trim();
                    if !t.is_empty() && !t.starts_with('#') {
                        queries.push(t.to_string());
                    }
                }
            }
            Err(e) => {
                eprintln!("c3 index query: --queries-file {path}: {e}");
                return 2;
            }
        }
    }
    if queries.is_empty() {
        eprintln!("c3 index query: at least one query is required (positional or --queries-file)");
        return 2;
    }
    let backend = match index::parse_conn(&ctx.conn) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    query_backend(&ctx, backend, &queries, args.budget, args.json)
}

#[cfg(not(feature = "index-surreal"))]
fn query_backend(
    _ctx: &Ctx,
    _backend: index::Backend,
    queries: &[String],
    _budget: usize,
    json: bool,
) -> i32 {
    for _ in queries {
        report_none_query(json);
    }
    0
}

#[cfg(feature = "index-surreal")]
fn query_backend(
    ctx: &Ctx,
    backend: index::Backend,
    queries: &[String],
    budget: usize,
    json: bool,
) -> i32 {
    if backend == index::Backend::None {
        for _ in queries {
            report_none_query(json);
        }
        return 0;
    }
    let profile = std::env::var("C3_INDEX_PROFILE").is_ok();
    let ns = "c3";
    let db_name = repo_db_name(&ctx.repo_root);
    // A read opens the embedded store exclusively too (no shared mode), so an open failure
    // here means another process holds it — report none and exit 0, no retry. Reading opens
    // without re-running the schema `DEFINE` (the build already wrote it). Every query in this
    // invocation shares this one open.
    let t = std::time::Instant::now();
    let idx = match index::SurrealIndex::open_read(backend, ns, &db_name) {
        Ok(i) => i,
        Err(reason) => {
            for _ in queries {
                report_none_query_reason(json, &reason);
            }
            return 0;
        }
    };
    if profile {
        eprintln!(
            "[c3-index] open_read (no DEFINE): {:.3}s",
            t.elapsed().as_secs_f64()
        );
    }
    for text in queries {
        let t = std::time::Instant::now();
        match idx.retrieve(text, budget) {
            Ok(hits) => print_hits(&hits, json),
            Err(reason) => report_none_query_reason(json, &reason),
        }
        if profile {
            eprintln!(
                "[c3-index] retrieve({text:?}) total: {:.3}s",
                t.elapsed().as_secs_f64()
            );
        }
    }
    0
}

fn report_none_query(json: bool) {
    report_none_query_reason(json, "index unavailable");
}

fn report_none_query_reason(json: bool, reason: &str) {
    if json {
        println!(
            "{}",
            serde_json::json!({ "hits": [], "index": "none", "reason": reason })
        );
    } else {
        println!("{} {}", index::PREFIX, IndexStats::none(reason).verdict());
    }
}

#[cfg(feature = "index-surreal")]
fn print_hits(hits: &[index::Hit], json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({ "hits": hits })).unwrap_or_default()
        );
        return;
    }
    if hits.is_empty() {
        println!("{} index: 0 hits", index::PREFIX);
        return;
    }
    println!("{} {} hit(s):", index::PREFIX, hits.len());
    for h in hits {
        let rel = if h.relations.is_empty() {
            String::new()
        } else {
            format!("  [{}]", h.relations.join(", "))
        };
        println!(
            "  {} {} ({}:{}-{})  score {:.4}{}",
            h.kind, h.name, h.path, h.line_start, h.line_end, h.score, rel
        );
    }
}

fn stats(args: StatsArgs) -> i32 {
    let ctx = resolve(&args.collab_dir, args.conn);
    let backend = match index::parse_conn(&ctx.conn) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    stats_backend(&ctx, backend, args.json)
}

#[cfg(not(feature = "index-surreal"))]
fn stats_backend(_ctx: &Ctx, _backend: index::Backend, json: bool) -> i32 {
    print_stats(&IndexStats::none("index-surreal feature disabled"), json);
    0
}

#[cfg(feature = "index-surreal")]
fn stats_backend(ctx: &Ctx, backend: index::Backend, json: bool) -> i32 {
    if backend == index::Backend::None {
        print_stats(&IndexStats::none("connection string is none"), json);
        return 0;
    }
    let ns = "c3";
    let db_name = repo_db_name(&ctx.repo_root);
    let idx = match index::SurrealIndex::open_read(backend, ns, &db_name) {
        Ok(i) => i,
        Err(reason) => {
            print_stats(&IndexStats::none(reason), json);
            return 0;
        }
    };
    match idx.stats() {
        Ok(s) => {
            print_stats(&s, json);
            0
        }
        Err(reason) => {
            print_stats(&IndexStats::none(reason), json);
            0
        }
    }
}

fn print_stats(s: &IndexStats, json: bool) {
    if json {
        println!("{}", serde_json::to_string(s).unwrap_or_default());
    } else {
        println!("{} {}", index::PREFIX, s.verdict());
        if let Some(p) = &s.path {
            println!("  backend: {} at {}", s.backend, p);
        }
        if let Some(g) = &s.generation {
            println!("  generation: {g}");
        }
    }
}

/// Discover the repository files (same hard-ignores and walk as the pack pipeline) and read
/// each one, returning `(rel_posix_path, content)` pairs. Unreadable files are skipped.
#[cfg(feature = "index-surreal")]
fn discover_files(repo_root: &Path) -> Vec<(String, String)> {
    use crate::pack::discover::{discover, DiscoverOpts};
    use crate::pack::read_text;
    let entries = discover(repo_root, &DiscoverOpts::default());
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        if let Some(text) = read_text(&e.abs) {
            out.push((e.rel, text));
        }
    }
    out
}

// --------------------------------------------------------------------------- index lock

/// Take the embedded index lock exclusively, the same way `c3-core`'s `TaskLock` does.
/// `Ok(guard)` holds it until dropped; `Err(reason)` means another handle holds it.
#[cfg(feature = "index-surreal")]
fn take_index_lock(collab_root: &Path) -> Result<IndexLock, String> {
    let dir = collab_root.join(".c3");
    std::fs::create_dir_all(&dir).map_err(|e| format!("index.lock dir: {e}"))?;
    let path = dir.join("index.lock");
    match try_open_exclusive(&path) {
        Ok(Some(file)) => Ok(IndexLock { _file: file }),
        Ok(None) => Err(format!(
            "index.lock held by another process ({})",
            path.display()
        )),
        Err(e) => Err(format!("index.lock open failed: {e}")),
    }
}

#[cfg(feature = "index-surreal")]
struct IndexLock {
    _file: std::fs::File,
}

#[cfg(all(feature = "index-surreal", windows))]
fn try_open_exclusive(path: &Path) -> std::io::Result<Option<std::fs::File>> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_SHARE_READ only: a second read-write open hits a sharing/lock violation, the same
    // exclusion c3-core's TaskLock relies on.
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(FILE_SHARE_READ)
        .open(path)
    {
        Ok(f) => Ok(Some(f)),
        Err(e) => match e.raw_os_error() {
            Some(32) | Some(33) => Ok(None),
            _ => Err(e),
        },
    }
}

#[cfg(all(feature = "index-surreal", unix))]
fn try_open_exclusive(path: &Path) -> std::io::Result<Option<std::fs::File>> {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match FileExt::try_lock_exclusive(&f) {
        Ok(true) => Ok(Some(f)),
        Ok(false) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
    }
}
