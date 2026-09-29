//! `c3 pack` — build a reviewer pack (and its `.pack.json` sidecar) for the `http` engine
//! (milestone 7; milestone 7b consumes it). The pipeline lives in
//! [`crate::pack::reviewer`].

use std::path::PathBuf;

use clap::Args;

use crate::index;
use crate::pack::reviewer::{self, PackOpts};
use crate::providers;

/// Arguments of `c3 pack`.
#[derive(Args, Debug, Default)]
pub struct PackArgs {
    /// The one-page brief file (path relative to the repo root, or absolute).
    #[arg(long)]
    pub brief: String,
    /// Focus file(s): path or glob, included in full (repeatable).
    #[arg(long)]
    pub focus: Vec<String>,
    /// Token budget for the periphery (0 = none).
    #[arg(long, default_value_t = 0)]
    pub budget: usize,
    /// Task slug whose `findings.json` supplies the open-findings snapshot.
    #[arg(long)]
    pub task: Option<String>,
    /// Output path for the pack; the `.pack.json` sidecar is written beside it.
    #[arg(long)]
    pub out: String,
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    /// Index connection for the periphery: `none` | `surrealkv:<path>` | `ws://host`. Default:
    /// the embedded store at `<collab>/.c3/index/` if it exists, else the lexical neighbourhood.
    /// The index is never required — any miss falls back to lexical.
    #[arg(long)]
    pub conn: Option<String>,
    /// Include excerpts from a federation peer by name (repeatable; M11). The peer must be
    /// allowed for this project in the configuration with `use_in_packs: true`.
    #[arg(long = "peer")]
    pub peer: Vec<String>,
    /// Include every peer this project may use in packs (`--peers all`); any other value refused.
    #[arg(long)]
    pub peers: Option<String>,
}

/// Build the reviewer pack and its sidecar. Returns the process exit code.
pub fn run(args: PackArgs) -> i32 {
    if args.out.trim().is_empty() {
        eprintln!("c3 pack: --out is required");
        return 1;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &args.collab_dir);

    // Default the index connection to the embedded store `c3 index` builds, so a pack uses the
    // index automatically when one is present; `--conn none` opts out. reviewer::build treats an
    // absent store, a held lock or an empty index as a lexical fallback.
    let conn = args.conn.or_else(|| {
        let dir = collab_root.join(".c3").join("index");
        Some(format!(
            "surrealkv:{}",
            dir.to_string_lossy().replace('\\', "/")
        ))
    });

    let out = PathBuf::from(&args.out);
    let opts = PackOpts {
        repo_root,
        collab_root,
        brief: PathBuf::from(&args.brief),
        focus: args.focus,
        budget: args.budget,
        task: args.task,
        out: out.clone(),
        max_file_size: 2 * 1024 * 1024,
        conn,
    };

    // Federation peers (M11): resolve the requested selection from the configuration, applying
    // the `use_in_packs` gate. No `--peer` → a local pack, exactly as before.
    let selection = match peer_selection(&args.peer, &args.peers) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("c3 pack: {e}");
            return 1;
        }
    };
    let peers = if selection.is_empty() {
        Vec::new()
    } else {
        match reviewer::resolve_pack_peers(&opts.repo_root, &selection) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("c3 pack: {e}");
                return 1;
            }
        }
    };

    let (pack, peer_stats) = match reviewer::build_with_peers(&opts, &peers) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("c3 pack: {e}");
            return 1;
        }
    };

    match reviewer::write(&pack, &out) {
        Ok(side) => {
            let kb = (pack.size_bytes / 1024).max(1);
            println!(
                "Reviewer pack: {} ({kb} KB | {} focus | {} periphery | {} redaction(s))",
                out.display(),
                pack.focus_files.len(),
                pack.periphery_shown,
                pack.redactions
            );
            for stat in &peer_stats {
                println!("  {}", reviewer::peer_dry_run_line(stat));
            }
            println!("Sidecar: {}", side.display());
            0
        }
        Err(e) => {
            eprintln!("c3 pack: {e}");
            1
        }
    }
}

/// Parse the peer selection from the flags, refusing an unknown `--peers` value.
fn peer_selection(peer: &[String], peers: &Option<String>) -> Result<index::PeerSelection, String> {
    let all = match peers.as_deref() {
        None => false,
        Some(v) if v.eq_ignore_ascii_case("all") => true,
        Some(_) => return Err("--peers accepts only 'all'".to_string()),
    };
    Ok(index::PeerSelection {
        all,
        names: peer.to_vec(),
    })
}
