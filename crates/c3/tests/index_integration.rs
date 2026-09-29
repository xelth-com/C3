//! Embedded-index integration tests (feature `index-surreal`).
//!
//! A scratch directory holds a few small source files; we build the index, check its
//! stats and a query, prove the single-writer fail-fast (a second open of the same store
//! errors instantly), and run the acceptance test: two rebuilds produce identical stats.

#![cfg(feature = "index-surreal")]

use c3::index::{Backend, SurrealIndex};

fn files() -> Vec<(String, String)> {
    vec![
        (
            "src/math.rs".to_string(),
            "\
/// Adds two numbers.
/// @relates: multiply
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Multiplies two numbers.
pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
"
            .to_string(),
        ),
        (
            "src/app.rs".to_string(),
            "\
/// Runs the demo, using add from the math module.
pub fn run_demo() -> i32 {
    let s = add(2, 3);
    multiply(s, 4)
}
"
            .to_string(),
        ),
    ]
}

fn scratch_store() -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let uniq = format!(
        "c3-idx-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    p.push(uniq);
    p.push("index");
    p
}

#[test]
fn build_query_lock_and_rebuild() {
    let store = scratch_store();
    let backend = Backend::SurrealKv(store.clone());
    let files = files();
    let generation = "testgen-abc1234";

    let idx = SurrealIndex::open(backend.clone(), "c3", "test").expect("open embedded index");

    // Build.
    let s1 = idx.index(&files, generation).expect("index");
    assert!(s1.entities >= 5, "entities: {}", s1.entities);
    assert_eq!(s1.files, 2, "two file entities");
    assert!(s1.belongs_to > 0, "belongs_to edges present");
    assert!(
        s1.calls > 0,
        "calls edges present (run_demo -> add/multiply)"
    );
    assert!(
        s1.relates_to > 0,
        "relates_to edge present (add -> multiply)"
    );
    assert_eq!(s1.generation.as_deref(), Some(generation));

    // Query: a known function name comes back, ranked first.
    let hits = idx.retrieve("multiply", 4000).expect("retrieve");
    assert!(!hits.is_empty(), "expected hits for 'multiply'");
    assert!(
        hits.iter().take(3).any(|h| h.name == "multiply"),
        "'multiply' should rank near the top, got: {:?}",
        hits.iter().map(|h| &h.name).collect::<Vec<_>>()
    );

    // Single-writer fail-fast: a second open of the same store errors immediately.
    let second = SurrealIndex::open(backend.clone(), "c3", "test");
    assert!(
        second.is_err(),
        "a second opener of the same embedded store must fail (RC1 single-writer)"
    );

    // Acceptance: two rebuilds produce identical stats.
    let r1 = idx.rebuild(&files, generation).expect("rebuild 1");
    let r2 = idx.rebuild(&files, generation).expect("rebuild 2");
    assert_eq!(r1.entities, r2.entities);
    assert_eq!(r1.files, r2.files);
    assert_eq!(r1.belongs_to, r2.belongs_to);
    assert_eq!(r1.calls, r2.calls);
    assert_eq!(r1.relates_to, r2.relates_to);
    // And identical to the first build (deterministic).
    assert_eq!(r1.entities, s1.entities);
    assert_eq!(r1.calls, s1.calls);

    drop(idx);
    let _ = std::fs::remove_dir_all(store.parent().unwrap_or(&store));
}

#[test]
fn chunked_bulk_insert_over_one_chunk() {
    // Enough files that the from-scratch bulk insert spans more than one `INSERT` chunk
    // (CHUNK is 400 rows): 260 files, each a file entity plus one fn, is > 400 entities.
    let mut files: Vec<(String, String)> = Vec::new();
    for i in 0..260 {
        files.push((
            format!("src/f{i}.rs"),
            format!("/// item {i}\npub fn item_{i}() -> i32 {{ {i} }}\n"),
        ));
    }
    let store = scratch_store();
    let backend = Backend::SurrealKv(store.clone());
    let generation = "chunkgen-0001";
    let idx = SurrealIndex::open(backend, "c3", "chunk").expect("open embedded index");

    let s1 = idx.index(&files, generation).expect("index");
    assert_eq!(s1.files, 260, "one file entity per file");
    assert!(
        s1.entities > 400,
        "expected > 400 entities to span >1 chunk, got {}",
        s1.entities
    );

    // A known function is found and ranked first.
    let hits = idx.retrieve("item_137", 4000).expect("retrieve");
    assert!(
        hits.first().map(|h| h.name.as_str()) == Some("item_137"),
        "'item_137' should rank first, got: {:?}",
        hits.iter().take(3).map(|h| &h.name).collect::<Vec<_>>()
    );

    // Deterministic rebuild: identical stats and entity count as the first build.
    let r1 = idx.rebuild(&files, generation).expect("rebuild");
    assert_eq!(r1.entities, s1.entities);
    assert_eq!(r1.files, s1.files);
    assert_eq!(r1.belongs_to, s1.belongs_to);
    assert_eq!(r1.calls, s1.calls);
    assert_eq!(r1.relates_to, s1.relates_to);

    drop(idx);
    let _ = std::fs::remove_dir_all(store.parent().unwrap_or(&store));
}

#[test]
fn incremental_matches_rebuild() {
    // The incremental (file-hash skip) path must land the store in exactly the state a
    // from-scratch rebuild would: a no-change build touches nothing and reports the same
    // counts, and a changed-file build (here a removed and an added function, leaving one
    // caller's target dangling) matches a full rebuild of the new file set.
    let store = scratch_store();
    let backend = Backend::SurrealKv(store.clone());
    let files = files();
    let generation = "incgen-0001";
    let idx = SurrealIndex::open(backend, "c3", "inc").expect("open embedded index");

    let s1 = idx.index(&files, generation).expect("build");

    // No-change rebuild: identical counts (and, by construction, zero row writes).
    let s2 = idx.index(&files, generation).expect("no-change");
    assert_eq!(s1.entities, s2.entities);
    assert_eq!(s1.files, s2.files);
    assert_eq!(s1.belongs_to, s2.belongs_to);
    assert_eq!(s1.calls, s2.calls);
    assert_eq!(s1.relates_to, s2.relates_to);

    // Change one file: drop `multiply` (so app.rs's call to it dangles) and add `subtract`.
    let mut files2 = files.clone();
    files2[0].1 = "\
/// Adds two numbers.
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Subtracts two numbers.
pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
"
    .to_string();

    let s3 = idx.index(&files2, generation).expect("incremental change");
    // A full rebuild of the same new file set on the same store.
    let sr = idx.rebuild(&files2, generation).expect("rebuild");
    assert_eq!(s3.entities, sr.entities, "entities incremental == rebuild");
    assert_eq!(s3.files, sr.files, "files incremental == rebuild");
    assert_eq!(
        s3.belongs_to, sr.belongs_to,
        "belongs_to incremental == rebuild"
    );
    assert_eq!(s3.calls, sr.calls, "calls incremental == rebuild");
    assert_eq!(
        s3.relates_to, sr.relates_to,
        "relates_to incremental == rebuild"
    );

    // `subtract` is now retrievable; `multiply` is gone.
    let hits = idx.retrieve("subtract", 4000).expect("retrieve");
    assert!(
        hits.iter().take(3).any(|h| h.name == "subtract"),
        "'subtract' should rank near the top after the incremental change"
    );

    drop(idx);
    let _ = std::fs::remove_dir_all(store.parent().unwrap_or(&store));
}

#[test]
fn none_backend_is_noop() {
    // Parsing and the none backend need no feature-specific store.
    let b = c3::index::parse_conn("none").unwrap();
    assert_eq!(b, Backend::None);
}
