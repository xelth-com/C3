//! `c3 doctor` — report what still needs doing in a C3 workspace (port spec, Part C).
//! The scan lives in [`crate::setup::doctor`]. Exit 0 when clean, 1 otherwise.

use std::path::PathBuf;

use clap::Args;

use crate::providers;
use crate::setup::doctor;

/// Arguments of `c3 doctor`.
#[derive(Args, Debug, Default)]
pub struct DoctorArgs {
    /// Emit a JSON object instead of the text report.
    #[arg(long)]
    pub json: bool,
}

fn human_bytes(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

/// Run `c3 doctor`. Returns the process exit code (0 clean, 1 with findings).
pub fn run(args: DoctorArgs) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let report = doctor::scan(&repo_root);
    let clean = report.is_clean();

    if args.json {
        let obj = serde_json::json!({
            "clean": clean,
            "stub_files": report.stub_files,
            "mcp_registered": report.mcp_registered,
            "eck_core_in": report.eck_core_in,
            "eck_rule_files": report.eck_rule_files,
            "snapshot_count": report.snapshot_count,
            "snapshot_bytes": report.snapshot_bytes,
        });
        println!("{obj}");
    } else {
        if report.stub_files.is_empty() {
            println!("[ok] no [STUB] markers left in .eck/*.md");
        } else {
            println!(
                "[!] {} manifest(s) still carry a [STUB] marker:",
                report.stub_files.len()
            );
            for f in &report.stub_files {
                println!("    {f}");
            }
        }

        if report.mcp_registered {
            println!("[ok] c3 mcp is registered in .mcp.json");
        } else {
            println!("[!] c3 mcp is NOT registered in .mcp.json (run `c3 init`)");
        }

        if report.eck_core_in.is_empty() && report.eck_rule_files.is_empty() {
            println!("[ok] no eck leftovers");
        } else {
            for loc in &report.eck_core_in {
                println!("[!] eck-core server still referenced in {loc}");
            }
            for f in &report.eck_rule_files {
                println!("[!] rule still names an eck_* tool: {f}");
            }
        }

        println!(
            "[i] .eck/snapshots/: {} file(s), {}",
            report.snapshot_count,
            human_bytes(report.snapshot_bytes)
        );

        println!(
            "{}",
            if clean {
                "doctor: clean"
            } else {
                "doctor: issues found (see above)"
            }
        );
    }

    if clean {
        0
    } else {
        1
    }
}
