//! `c3 manifest` — atomic edits to a `.eck/<NAME>.md` manifest (port spec, Part A).
//! No git, no snapshot.

use std::path::PathBuf;

use clap::Args;

use crate::finish::manifest::Action;
use crate::finish::{self};
use crate::providers;

/// Arguments of `c3 manifest`.
#[derive(Args, Debug)]
pub struct ManifestArgs {
    /// The manifest file: a plain NAME.md directly under `.eck/`.
    #[arg(long)]
    pub file: String,
    /// replace_text | append_to_section | replace_file.
    #[arg(long)]
    pub action: String,
    /// The text to write / append / substitute.
    #[arg(long, allow_hyphen_values = true)]
    pub content: String,
    /// The section header line (required by append_to_section).
    #[arg(long, allow_hyphen_values = true)]
    pub section: Option<String>,
    /// The text to replace (required by replace_text).
    #[arg(long, allow_hyphen_values = true)]
    pub target: Option<String>,
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
}

/// Run `c3 manifest`. Returns the process exit code.
pub fn run(args: ManifestArgs) -> i32 {
    let action = match Action::parse(&args.action) {
        Some(a) => a,
        None => {
            eprintln!(
                "c3 manifest: --action must be one of replace_text, append_to_section, replace_file (got '{}')",
                args.action
            );
            return 1;
        }
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);

    match finish::run_manifest(
        &repo_root,
        &args.file,
        action,
        &args.content,
        args.section.as_deref(),
        args.target.as_deref(),
    ) {
        Ok(path) => {
            println!("Edited {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("c3 manifest: {e}");
            1
        }
    }
}
