//! M11 local-embeddings integration tests (feature `index-surreal`).
//!
//! The embedder is an in-process fake server on loopback — never a real embedder and never the
//! network. It answers the OpenAI `/v1/embeddings` shape and records the request bodies so a
//! test can prove no secret is sent. A scratch embedded store holds a few entities.

#![cfg(feature = "index-surreal")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use c3::index::{Backend, Embedder, SurrealIndex};

fn scratch_store(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "c3-emb-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p.push("index");
    p
}

fn files() -> Vec<(String, String)> {
    vec![
        (
            "src/math.rs".to_string(),
            "/// Adds.\npub fn add(a: i32, b: i32) -> i32 { a + b }\n/// Multiplies.\npub fn multiply(a: i32, b: i32) -> i32 { a * b }\n".to_string(),
        ),
        (
            "src/app.rs".to_string(),
            "/// Runs the demo.\npub fn run_demo() -> i32 { let s = add(2, 3); multiply(s, 4) }\n".to_string(),
        ),
    ]
}

/// A fake loopback embedder. `resp_dim` is the dimension every returned vector has; set it
/// different from the configured dimension to exercise the "wrong dimension → store nothing"
/// path. Every request body is pushed to `bodies`. Returns the `/v1/embeddings` URL.
fn spawn_embedder(resp_dim: usize, bodies: Arc<Mutex<Vec<String>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut content_length = 0usize;
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut buf = vec![0u8; content_length];
            let _ = reader.read_exact(&mut buf);
            let body = String::from_utf8_lossy(&buf).to_string();
            // Count the inputs.
            let n = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("input").and_then(|i| i.as_array()).map(|a| a.len()))
                .unwrap_or(0);
            bodies.lock().unwrap().push(body);
            let vec: Vec<f64> = vec![0.5; resp_dim];
            let data: Vec<serde_json::Value> = (0..n)
                .map(|i| serde_json::json!({ "index": i, "embedding": vec }))
                .collect();
            let payload = serde_json::json!({ "data": data }).to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    format!("http://{addr}/v1/embeddings")
}

// ------------------------------------------------------------- embed stores vectors; stats

#[test]
fn embed_stores_vectors_and_stats_report_them() {
    let store = scratch_store("stores");
    let idx = SurrealIndex::open(Backend::SurrealKv(store.clone()), "c3", "emb").unwrap();
    idx.index(&files(), "gen-1").unwrap();

    let bodies = Arc::new(Mutex::new(Vec::new()));
    let url = spawn_embedder(4, bodies.clone());
    let embedder = Embedder::new(url, "fake-embed", 4);

    let s = idx.embed(&embedder, None).unwrap();
    assert!(s.considered > 0);
    assert_eq!(s.embedded, s.considered, "every candidate embedded");
    assert_eq!(s.failed_batches, 0);

    let stats = idx.stats().unwrap();
    assert!(stats.vectors > 0, "stats report stored vectors");
    assert_eq!(stats.vector_model.as_deref(), Some("fake-embed"));
    assert_eq!(stats.vector_dim, Some(4));

    // A second embed is a no-op (nothing changed) — considered drops to zero.
    let s2 = idx.embed(&embedder, None).unwrap();
    assert_eq!(s2.considered, 0, "no entity needs re-embedding");

    drop(idx);
    let _ = std::fs::remove_dir_all(store.parent().unwrap_or(&store));
}

// ------------------------------------------------------------- item 6: bad response, nothing

#[test]
fn wrong_dimension_response_stores_nothing() {
    let store = scratch_store("wrongdim");
    let idx = SurrealIndex::open(Backend::SurrealKv(store.clone()), "c3", "emb").unwrap();
    idx.index(&files(), "gen-1").unwrap();

    // Configured dimension 4, but the server returns dimension 3: the batch is discarded whole.
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let url = spawn_embedder(3, bodies.clone());
    let embedder = Embedder::new(url, "fake-embed", 4);

    let s = idx.embed(&embedder, None).unwrap();
    assert!(s.considered > 0);
    assert_eq!(s.embedded, 0, "a wrong-dimension batch stores nothing");
    assert!(s.failed_batches >= 1);
    assert_eq!(idx.stats().unwrap().vectors, 0, "no vector was stored");

    drop(idx);
    let _ = std::fs::remove_dir_all(store.parent().unwrap_or(&store));
}

// ------------------------------------------------------------- item 7: no secret to embedder

#[test]
fn a_seeded_secret_is_redacted_before_it_reaches_the_embedder() {
    let store = scratch_store("secret");
    let idx = SurrealIndex::open(Backend::SurrealKv(store.clone()), "c3", "emb").unwrap();
    // A secret in the source: extraction already redacts stored code, and embed re-runs the
    // single sanitizer, so the embedder must never see it.
    let secret = "AKIAIOSFODNN7EXAMPLE";
    let src = format!("/// A key.\npub fn danger() {{ let k = \"{secret}\"; }}\n");
    idx.index(&[("src/s.rs".to_string(), src)], "gen-1")
        .unwrap();

    let bodies = Arc::new(Mutex::new(Vec::new()));
    let url = spawn_embedder(4, bodies.clone());
    let embedder = Embedder::new(url, "fake-embed", 4);
    idx.embed(&embedder, None).unwrap();

    let sent = bodies.lock().unwrap().join("\n");
    assert!(!sent.is_empty(), "the embedder received at least one batch");
    assert!(
        !sent.contains(secret),
        "the secret reached the embedder: {sent}"
    );

    drop(idx);
    let _ = std::fs::remove_dir_all(store.parent().unwrap_or(&store));
}

// ------------------------------------------------------------- fifth retrieval leg + fallback

#[test]
fn embedding_leg_contributes_and_missing_embedder_falls_back() {
    let store = scratch_store("fifthleg");
    let idx = SurrealIndex::open(Backend::SurrealKv(store.clone()), "c3", "emb").unwrap();
    idx.index(&files(), "gen-1").unwrap();

    let bodies = Arc::new(Mutex::new(Vec::new()));
    let url = spawn_embedder(4, bodies.clone());
    let embedder = Embedder::new(url, "fake-embed", 4);
    idx.embed(&embedder, None).unwrap();

    // A term that no BM25 field contains: BM25 alone finds nothing.
    let bm25_only = idx.retrieve("zzznotpresentterm", 0).unwrap();
    assert!(bm25_only.is_empty(), "BM25 finds nothing for a novel term");

    // With the embedder, the nearest-neighbour leg contributes hits (all stored vectors are
    // equidistant here, so every embedded entity is a neighbour).
    let with_nn = idx
        .retrieve_embedded("zzznotpresentterm", 0, &embedder)
        .unwrap();
    assert!(
        !with_nn.is_empty(),
        "the embedding leg joins retrieval as a fifth leg"
    );

    // A missing embedder (a closed port) falls back to the four BM25 legs, byte for byte.
    let dead = Embedder::new("http://127.0.0.1:1/v1/embeddings", "fake-embed", 4);
    let bm25 = idx.retrieve("multiply", 0).unwrap();
    let fell_back = idx.retrieve_embedded("multiply", 0, &dead).unwrap();
    let names = |h: &[c3::index::Hit]| h.iter().map(|x| x.name.clone()).collect::<Vec<_>>();
    assert_eq!(
        names(&bm25),
        names(&fell_back),
        "an unreachable embedder yields the plain BM25 retrieval"
    );

    drop(idx);
    let _ = std::fs::remove_dir_all(store.parent().unwrap_or(&store));
}

// ------------------------------------------------------------- rebuild + embed acceptance

#[test]
fn rebuild_then_embed_reproduces_the_vector_count() {
    let store = scratch_store("rebuild");
    let idx = SurrealIndex::open(Backend::SurrealKv(store.clone()), "c3", "emb").unwrap();
    idx.index(&files(), "gen-1").unwrap();

    let bodies = Arc::new(Mutex::new(Vec::new()));
    let url = spawn_embedder(4, bodies.clone());
    let embedder = Embedder::new(url, "fake-embed", 4);
    idx.embed(&embedder, None).unwrap();
    let first = idx.stats().unwrap();
    assert!(first.vectors > 0);

    // Rebuild drops everything (vectors included); a fresh embed reproduces the same set.
    idx.rebuild(&files(), "gen-1").unwrap();
    assert_eq!(idx.stats().unwrap().vectors, 0, "rebuild clears vectors");
    idx.embed(&embedder, None).unwrap();
    let second = idx.stats().unwrap();
    assert_eq!(first.entities, second.entities);
    assert_eq!(
        first.vectors, second.vectors,
        "same vector count after rebuild+embed"
    );

    drop(idx);
    let _ = std::fs::remove_dir_all(store.parent().unwrap_or(&store));
}
