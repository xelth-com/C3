//! `c3 explain` — build an explainer pack for one claim (milestone 7, D6). Because the
//! file leaves the machine, the redaction count and size are printed and an explicit
//! `--yes` or a `y/N` confirmation is required before it is written (D8). The pipeline
//! lives in [`crate::pack::explain`].

use std::io::Write;
use std::path::PathBuf;

use clap::Args;

use crate::pack::explain::{self, ExplainOpts};
use crate::providers;

/// Arguments of `c3 explain`.
#[derive(Args, Debug, Default)]
pub struct ExplainArgs {
    /// The claim to explain, verbatim (marked unverified in the pack).
    #[arg(long)]
    pub claim: String,
    /// Focus file(s): path or glob, in full with line numbers (repeatable).
    #[arg(long)]
    pub focus: Vec<String>,
    /// Token budget for the periphery (0 = none).
    #[arg(long, default_value_t = 0)]
    pub budget: usize,
    /// Who the explanation is for (free text).
    #[arg(long, default_value = "")]
    pub audience: String,
    /// Output path. Default: `.c3/explains/<repo>_<ts>.md`.
    #[arg(long)]
    pub out: Option<String>,
    /// Skip the confirmation (the pack leaves the machine).
    #[arg(long)]
    pub yes: bool,
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
}

/// Build the explainer pack, confirm, and write. Returns the process exit code.
pub fn run(args: ExplainArgs) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &args.collab_dir);

    let into_state_dir = args.out.is_none();
    let out = args
        .out
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| explain::default_out(&repo_root));
    let state_repo = repo_root.clone();
    let state_collab = collab_root.clone();

    let opts = ExplainOpts {
        repo_root,
        collab_root,
        claim: args.claim,
        focus: args.focus,
        budget: args.budget,
        audience: args.audience,
        max_file_size: 2 * 1024 * 1024,
    };

    let pack = match explain::build(&opts) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("c3 explain: {e}");
            return 1;
        }
    };

    let kb = (pack.size_bytes / 1024).max(1);
    let toks = if pack.tokens < 1000 {
        pack.tokens.to_string()
    } else {
        format!("{:.1}k", pack.tokens as f64 / 1000.0)
    };
    eprintln!(
        "Explainer pack ready: {} focus file(s), {} periphery, {kb} KB, ~{toks} tokens, {} redaction(s).",
        pack.focus_files.len(),
        pack.neighbours,
        pack.redactions
    );
    eprintln!(
        "This file will leave your machine when you share it. Destination: {}",
        out.display()
    );

    if !args.yes && !confirm() {
        eprintln!("c3 explain: aborted (no confirmation).");
        return 1;
    }

    // The default output goes into C3's folder: make sure it is in place first (and write into the
    // folder it settled on - the legacy one for a run whose move had to wait).
    let mut out = out;
    let mut in_state: Option<(PathBuf, String)> = None;
    if into_state_dir {
        match crate::state_dir::ensure(&state_repo, Some(&state_collab)) {
            Ok(e) => {
                crate::state_dir::print_notes("c3 explain", &e.notes(), true);
                if let Some(name) = out.file_name().map(|n| n.to_string_lossy().to_string()) {
                    in_state = Some((e.dir.join("explains"), name));
                }
            }
            Err(e) => {
                eprintln!("c3 explain: {e}");
                return 1;
            }
        }
    }

    // (S2, F36-1/F36-3; 0.2.5 R7b, F48-2) into C3's folder the file is written through
    // `write_in` at the write itself: `explains/` at the folder's place NOW (a legacy path the
    // move carried is taken to `.c3/`, a legacy folder that is gone is never created again), never
    // through a link
    let written = match &in_state {
        Some((dir, name)) => {
            match crate::state_dir::write_in(&state_repo, dir, name, |_, _| {
                Ok(pack.content.as_bytes().to_vec())
            }) {
                Ok(p) => {
                    out = p;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        None => explain::write(&pack, &out),
    };
    match written {
        Ok(()) => {
            println!("Explainer pack: {}", out.display());
            0
        }
        Err(e) => {
            eprintln!("c3 explain: {e}");
            1
        }
    }
}

/// Ask `y/N` on the terminal; default No. A non-tty / closed stdin reads as No.
fn confirm() -> bool {
    eprint!("Write it? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(_) => matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
        Err(_) => false,
    }
}
