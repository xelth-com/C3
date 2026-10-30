//! `c3 init` — provision a repository for the C3 workflow (port spec, Part C).
//! The logic lives in [`crate::setup::init`].

use std::path::PathBuf;

use clap::Args;

use crate::providers;
use crate::setup::init::{self, InitOpts};

/// Arguments of `c3 init`.
#[derive(Args, Debug, Default)]
pub struct InitArgs {
    /// Overwrite the `.eck/*.md` manifests with a fresh `[STUB]` even when they exist.
    #[arg(long)]
    pub force_stubs: bool,
    /// Emit a JSON object instead of the text summary.
    #[arg(long)]
    pub json: bool,
}

/// Resolve the absolute path of the running `c3` executable for the `.mcp.json` command.
fn c3_exe() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(&p).ok().or(Some(p)))
        .map(providers::dunce_simplify)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "c3".to_string())
}

/// Run `c3 init`. Returns the process exit code.
pub fn run(args: InitArgs) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);

    let opts = InitOpts {
        repo_root,
        force_stubs: args.force_stubs,
        c3_exe: c3_exe(),
    };

    match init::run(&opts) {
        Ok(r) => {
            if args.json {
                let obj = serde_json::json!({
                    "created": r.created,
                    "skipped": r.skipped,
                    "gitignore": r.gitignore,
                    "mcp": r.mcp,
                });
                println!("{obj}");
            } else {
                if r.created.is_empty() {
                    println!("Created nothing (everything already present).");
                } else {
                    println!("Created {} file(s):", r.created.len());
                    for c in &r.created {
                        println!("  {c}");
                    }
                }
                if !r.skipped.is_empty() {
                    println!("Skipped {} existing file(s):", r.skipped.len());
                    for s in &r.skipped {
                        println!("  {s}");
                    }
                }
                println!("{}", r.gitignore);
                println!("{}", r.mcp);
            }
            0
        }
        Err(e) => {
            eprintln!("c3 init: {e}");
            1
        }
    }
}
