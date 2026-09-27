//! `c3 mcp`: run the stdio MCP server (milestone 10).
//!
//! The subcommand takes no arguments — it reads JSON-RPC frames from stdin and
//! writes them to stdout until stdin closes. The server itself lives in
//! [`crate::mcp`].

/// Run the stdio MCP server and return the process exit code.
pub fn run() -> i32 {
    crate::mcp::run()
}
