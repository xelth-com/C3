//! M11 federation integration tests (feature `index-surreal`).
//!
//! Two temporary embedded stores stand in for "this project" and a peer. No network, no real
//! configuration, no real project: a peer is a scratch store and the configuration is a temp
//! file read with `c3config::load_from`.
//!
//! RC1 caveat (surrealkv on Windows): an embedded store holds its OS lock for the LIFETIME of
//! the process, so a store path can be opened only once per process. A test that must READ a
//! peer store therefore builds it in a CHILD process (which exits, and only then is the path
//! free for the parent's single open) — exactly as real federation opens a peer store built by
//! another project's process. Tests that only resolve configuration, or that deliberately hold
//! the store to prove the busy path, open in-process.

#![cfg(feature = "index-surreal")]

use std::path::{Path, PathBuf};

use c3::c3config;
use c3::index::federation::{self, Availability};
use c3::index::{Backend, PeerSelection, SurrealIndex};
use c3::pack::reviewer::{self, PackOpts};

fn scratch(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "c3-fed-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

fn posix(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Build an embedded peer store at `store` from `files`, IN THIS PROCESS. Only for tests that
/// never re-open the store (RC1: the path stays locked for the process). Callers that must read
/// the store use [`build_peer_store_in_child`] instead.
fn build_peer_store_in_process(store: &Path, files: &[(&str, &str)]) {
    let idx = SurrealIndex::open(Backend::SurrealKv(store.to_path_buf()), "c3", "peer")
        .expect("open peer store");
    let files: Vec<(String, String)> = files
        .iter()
        .map(|(p, c)| (p.to_string(), c.to_string()))
        .collect();
    idx.index(&files, "peergen-0001").expect("index peer");
    drop(idx);
}

/// Build the peer store in a CHILD process (this test binary re-executed to run the
/// `helper_build_peer_store` test), so the store's lock is released when the child exits and the
/// parent may open the path for the first time.
fn build_peer_store_in_child(store: &Path, files: &[(&str, &str)]) {
    let spec = serde_json::json!({
        "store": store.to_string_lossy(),
        "files": files.iter().map(|(p, c)| vec![*p, *c]).collect::<Vec<_>>(),
    })
    .to_string();
    let exe = std::env::current_exe().expect("current exe");
    let status = std::process::Command::new(exe)
        .args(["helper_build_peer_store", "--exact", "--nocapture"])
        .env("C3_PEER_BUILD_SPEC", spec)
        .status()
        .expect("spawn peer-store builder");
    assert!(status.success(), "child peer-store build failed");
}

/// The child entry point invoked by [`build_peer_store_in_child`]. A normal test run (no env
/// var) is a no-op; when re-executed with `C3_PEER_BUILD_SPEC` it builds the store and exits so
/// the OS lock is released for the parent.
#[test]
fn helper_build_peer_store() {
    let Ok(spec) = std::env::var("C3_PEER_BUILD_SPEC") else {
        return;
    };
    let v: serde_json::Value = serde_json::from_str(&spec).unwrap();
    let store = PathBuf::from(v["store"].as_str().unwrap());
    let files: Vec<(String, String)> = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f[0].as_str().unwrap().to_string(),
                f[1].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let idx = SurrealIndex::open(Backend::SurrealKv(store), "c3", "peer").expect("open peer store");
    idx.index(&files, "peergen-0001").expect("index peer");
    drop(idx);
    // Exit cleanly so surrealkv releases the store's lock for the parent process.
    std::process::exit(0);
}

// -------------------------------------------------------------------- item 2: cross-project

#[test]
fn peer_configured_for_another_project_is_invisible() {
    let root = scratch("invisible");
    let proj_a = root.join("projA");
    let proj_b = root.join("projB");
    let peer_store = root.join("peerstore").join("index");
    std::fs::create_dir_all(&proj_a).unwrap();
    std::fs::create_dir_all(&proj_b).unwrap();
    build_peer_store_in_process(&peer_store, &[("lib.rs", "pub fn hello() {}\n")]);

    // The peer is allowed for projB only.
    let cfg_body = format!(
        r#"{{"config_version":1,"index":{{"peers":[
            {{"name":"hub","conn":"surrealkv:{}","namespace":"c3","database":"peer",
              "projects":["{}"],"use_in_packs":true}}
        ]}}}}"#,
        posix(&peer_store),
        posix(&proj_b),
    );
    let cfg_path = root.join("config.json");
    std::fs::write(&cfg_path, cfg_body).unwrap();
    let cfg = c3config::load_from(&cfg_path).unwrap().unwrap();

    // From projA the peer is not listed and naming it is refused with the exact wording.
    assert!(cfg.peers_for_project(&proj_a).is_empty());
    let sel = PeerSelection {
        all: false,
        names: vec!["hub".to_string()],
    };
    let err = cfg.resolve_selection(&proj_a, &sel, false).unwrap_err();
    assert_eq!(err, "no peer named hub is allowed for this project");
    // `--peers all` from projA yields nothing (its existence is never revealed).
    let all = PeerSelection {
        all: true,
        names: vec![],
    };
    assert!(cfg
        .resolve_selection(&proj_a, &all, false)
        .unwrap()
        .is_empty());

    // From projB it resolves.
    assert_eq!(cfg.peers_for_project(&proj_b).len(), 1);
    assert_eq!(
        cfg.resolve_selection(&proj_b, &sel, false).unwrap().len(),
        1
    );

    let _ = std::fs::remove_dir_all(&root);
}

// -------------------------------------------------------------------- item 3: use_in_packs

#[test]
fn use_in_packs_false_is_not_used_in_a_pack() {
    let root = scratch("nopacks");
    let proj = root.join("proj");
    let peer_store = root.join("peerstore").join("index");
    std::fs::create_dir_all(&proj).unwrap();
    build_peer_store_in_process(&peer_store, &[("lib.rs", "pub fn hello() {}\n")]);

    let cfg_body = format!(
        r#"{{"config_version":1,"index":{{"peers":[
            {{"name":"hub","conn":"surrealkv:{}","namespace":"c3","database":"peer",
              "projects":["{}"],"use_in_packs":false}}
        ]}}}}"#,
        posix(&peer_store),
        posix(&proj),
    );
    let cfg_path = root.join("config.json");
    std::fs::write(&cfg_path, cfg_body).unwrap();
    let cfg = c3config::load_from(&cfg_path).unwrap().unwrap();

    let sel = PeerSelection {
        all: false,
        names: vec!["hub".to_string()],
    };
    // A plain query MAY use it, but a pack (for_packs = true) refuses because use_in_packs=false.
    assert_eq!(cfg.resolve_selection(&proj, &sel, false).unwrap().len(), 1);
    let err = cfg.resolve_selection(&proj, &sel, true).unwrap_err();
    assert!(err.contains("use_in_packs: false"), "{err}");

    let _ = std::fs::remove_dir_all(&root);
}

// -------------------------------------------------------------------- item 8: busy peer (RC1)

#[test]
fn a_held_peer_is_busy_and_the_query_goes_on_without_it() {
    let root = scratch("busy");
    let peer_store = root.join("peerstore").join("index");
    // Open and hold the store in this process (the "other process" of RC1).
    let held = SurrealIndex::open(Backend::SurrealKv(peer_store.clone()), "c3", "peer")
        .expect("open and hold the store");
    held.index(&[("lib.rs".into(), "pub fn hello() {}\n".into())], "gen")
        .expect("index");

    let peer = federation::ResolvedPeer {
        name: "hub".to_string(),
        backend: Backend::SurrealKv(peer_store.clone()),
        namespace: "c3".to_string(),
        database: "peer".to_string(),
        use_in_packs: true,
    };
    // A second opener of the same embedded store is busy (fail-fast, RC1).
    assert_eq!(
        federation::probe_peer(&peer),
        Availability::Busy,
        "a store held by another opener is busy per RC1"
    );
    // A federated retrieval skips the busy peer (Err) rather than failing the query.
    assert!(federation::retrieve_peer(&peer, "hello", 0).is_err());

    drop(held);
    let _ = std::fs::remove_dir_all(&root);
}

// -------------------------------------------------------------------- item 4: pack peer paths

#[test]
fn peer_tool_state_excluded_and_paths_never_joined_to_local_root() {
    let root = scratch("packpaths");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    // A local focus file and brief sharing the query identifier `open_ledger`.
    std::fs::write(proj.join("brief.md"), "# Brief\n1. Trace open_ledger().\n").unwrap();
    std::fs::write(proj.join("store.rs"), "pub fn open_ledger() {}\n").unwrap();

    // The peer carries a normal file (with a seeded secret), a tool-state file and an escaping
    // path, all matching the query. Built in a CHILD process so the parent can read it (RC1).
    let peer_store = root.join("peerstore").join("index");
    build_peer_store_in_child(
        &peer_store,
        &[
            (
                "lib.rs",
                "pub fn open_ledger() { let k = \"AKIAIOSFODNN7EXAMPLE\"; /* PEER_NORMAL */ }\n",
            ),
            (
                ".collab/notes.rs",
                "fn open_ledger() { /* PEER_TOOLSTATE */ }\n",
            ),
            (
                "../../etc/passwd",
                "open_ledger PEER_TRAVERSAL not-a-real-passwd\n",
            ),
        ],
    );

    let peer = federation::ResolvedPeer {
        name: "hub".to_string(),
        backend: Backend::SurrealKv(peer_store.clone()),
        namespace: "c3".to_string(),
        database: "peer".to_string(),
        use_in_packs: true,
    };

    let opts = PackOpts {
        repo_root: proj.clone(),
        collab_root: proj.join(".collab"),
        brief: PathBuf::from("brief.md"),
        focus: vec!["store.rs".into()],
        budget: 0,
        task: None,
        out: proj.join("pack.md"),
        max_file_size: 2 * 1024 * 1024,
        conn: Some("none".to_string()),
    };
    let (pack, stats) = reviewer::build_with_peers(&opts, std::slice::from_ref(&peer)).unwrap();

    // The peer section exists with its own header.
    assert!(
        pack.content
            .contains("## Peripheral context from other indexes"),
        "{}",
        pack.content
    );
    // The normal peer file entered, headed by its peer path.
    assert!(
        pack.content.contains("### peer:hub / lib.rs"),
        "{}",
        pack.content
    );
    assert!(pack.content.contains("PEER_NORMAL"));
    // Item 7 (peer side): a secret seeded into a peer entity is redacted in the pack.
    assert!(
        !pack.content.contains("AKIAIOSFODNN7EXAMPLE"),
        "peer secret leaked"
    );
    assert!(pack.content.contains("[REDACTED:aws-key]"));
    // The tool-state peer path never enters a pack.
    assert!(!pack.content.contains(".collab/notes.rs"));
    assert!(!pack.content.contains("PEER_TOOLSTATE"));
    // The escaping path is used only as a heading (the PEER's stored text), never joined to the
    // local root: the excerpt is the peer body, and no local file was read for it.
    assert!(pack.content.contains("### peer:hub / ../../etc/passwd"));
    assert!(pack.content.contains("PEER_TRAVERSAL"));

    // The sidecar records the peer with a slug name and counts only.
    let side: serde_json::Value = serde_json::from_str(&pack.sidecar).unwrap();
    let peers = side["peers"].as_array().unwrap();
    assert_eq!(peers[0]["name"], "hub");
    assert!(peers[0]["excerpts"].as_u64().unwrap() >= 2);
    // The dry-run line names the peer and the count, never a path.
    let line = reviewer::peer_dry_run_line(&stats[0]);
    assert!(line.contains("hub"));
    assert!(!line.contains("passwd"));

    let _ = std::fs::remove_dir_all(&root);
}

// -------------------------------------------------------------------- item 1: no peers = local

#[test]
fn no_peers_pack_is_byte_identical_to_local() {
    let root = scratch("identical");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("brief.md"), "# Brief\n1. Trace open_ledger().\n").unwrap();
    std::fs::write(proj.join("store.rs"), "pub fn open_ledger() {}\n").unwrap();
    std::fs::write(proj.join("client.rs"), "fn c() { open_ledger(); }\n").unwrap();
    let opts = PackOpts {
        repo_root: proj.clone(),
        collab_root: proj.join(".collab"),
        brief: PathBuf::from("brief.md"),
        focus: vec!["store.rs".into()],
        budget: 0,
        task: None,
        out: proj.join("pack.md"),
        max_file_size: 2 * 1024 * 1024,
        conn: Some("none".to_string()),
    };
    let local = reviewer::build(&opts).unwrap();
    let (with_empty_peers, stats) = reviewer::build_with_peers(&opts, &[]).unwrap();
    assert_eq!(local.content, with_empty_peers.content);
    assert!(stats.is_empty());
    assert!(!local
        .content
        .contains("## Peripheral context from other indexes"));
    let _ = std::fs::remove_dir_all(&root);
}
