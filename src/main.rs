//! C3 — claude-codex-consult in Rust.
//!
//! The consultation bridge: a coordinator (Claude Code, Codex CLI, any shell)
//! asks a roster of read-only reviewers from different labs to look at a
//! one-page brief, records the consultation as files next to the code (the
//! brief, the reply verbatim, a JSON ledger, findings tracked by id) and can
//! run several reviewers as a panel.
//!
//! This is the Rust rewrite of the PowerShell plugin `codex-consult`
//! (github.com/xelth-com/claude-codex-consult), made from a finished copy of
//! it. The plugin stays and is the working implementation until this binary
//! reaches parity; README "Plan" lists the port order. Nothing below is
//! functional yet beyond `--version` and `--help`.

const HELP: &str = "\
c3 — claude-codex-consult in Rust (early port, not functional yet)

USAGE:
    c3 --version | -V        print the version
    c3 --help    | -h        this text

The working implementation today is the PowerShell plugin:
    /plugin marketplace add xelth-com/claude-codex-consult
    /plugin install codex-consult@claude-codex-consult

Page, install, roster, telemetry in the open:  https://xelth.com/C3/
Questions and complaints (forum section c3):    https://xelth.com/F/p/c3
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") | Some("-V") => println!("c3 {}", env!("CARGO_PKG_VERSION")),
        Some("--help") | Some("-h") | None => print!("{HELP}"),
        Some(other) => {
            eprintln!("c3: unknown argument `{other}` — the port is not functional yet; run `c3 --help`");
            std::process::exit(2);
        }
    }
}
