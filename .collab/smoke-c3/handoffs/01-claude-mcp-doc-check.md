# Handoff 01 - Claude: does docs/port/mcp.md match the MCP server code?

Date: 2026-09-27. Base commit: the current HEAD of this repository (see `git log -1`).

## Question

This is the first live consultation run through the Rust bridge `c3` itself (not the PowerShell plugin). The
question is small on purpose: does `docs/port/mcp.md` describe the MCP server in `crates/c3/src/mcp/mod.rs`
accurately - tool names, argument schemas, the `.mcp.json` registration snippet, and the "no tool commits" invariant?

## Delta since the last review

First review of this task.

## CURRENT invariants claimed

- Every MCP tool runs the same binary as a subprocess with the CLI flags, so its files are byte-identical to a CLI run.
- No tool commits to git, and `complain` / `forget-me` are not exposed.
- Path arguments that escape the working directory are rejected.

## Changed files

| File | Change |
|---|---|
| `crates/c3/src/mcp/mod.rs` | the server: JSON-RPC loop, tool definitions, argument mapping, subprocess runner |
| `docs/port/mcp.md` | the document under review |

## Open findings

_(none)_

## Evidence

- `crates/c3/src/mcp/mod.rs` - the `tools/list` definitions and the argv mapping per tool.
- `docs/port/mcp.md` - the tool table and schemas.

## Questions

- **Q1.** List every mismatch between the document and the code (tool names, argument names and types, defaults,
  descriptions, the `.mcp.json` snippet). If there is none, say so.
- **Q2.** Is the "no tool commits" invariant actually enforced by the code, or only by the tool list?

Answer by number. Keep it under 400 words.
