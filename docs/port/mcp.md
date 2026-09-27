# MCP surface — `c3 mcp` (milestone 10)

`c3 mcp` runs a stdio MCP server: newline-delimited JSON-RPC 2.0 on stdin/stdout (the MCP
stdio transport). It exposes C3's **read-and-record** tools to a coordinator model (Claude
Code, or any MCP client) and nothing else. Per design §9 and invariant 3 (*C3 never
commits*), there is **no commit or finish tool**, and no interactive tool (`complain`,
`forget-me`) is exposed.

Implementation: `crates/c3/src/mcp/mod.rs` (the server) and `crates/c3/src/cli/mcp.rs` (the
subcommand). The server is dependency-light — a hand-rolled JSON-RPC loop over serde_json,
no async runtime.

## How each tool runs

Every tool re-invokes the running `c3` executable (`std::env::current_exe()`) as a child
process with the matching CLI arguments and captures its stdout / stderr / exit code. This
guarantees the files a tool writes are **byte-identical to a CLI run** and isolates each
tool's locks and timeouts in a child process. The child inherits the environment and the
server's working directory (the repository the coordinator runs in); its stdin is closed.

Path-shaped arguments (`collab_dir`, `brief`, `out`, `focus[]`, `artifact[]`) are rejected
if they are absolute or contain a `..` component that escapes the working directory
(`c3-core::task_slug::contained_join`).

Timeouts: `c3_consult` gets 3600 s (a consultation may take minutes); every other tool gets
120 s. A timeout kills the child and the tool result is flagged `isError`.

A tool result is `content: [{ "type": "text", "text": <console output + exit code> }]` plus
`isError` (true on a non-zero exit or a timeout). Logging goes to stderr only under
`C3_DEBUG=1`.

## Protocol methods

| Method | Behaviour |
|---|---|
| `initialize` | Echoes the client's `protocolVersion` (default `2025-06-18` if absent); capabilities `{ "tools": {} }`; serverInfo `{ name: "c3", version }`. |
| `notifications/initialized` | Notification (no `id`) — no response. |
| `tools/list` | The tool definitions below. |
| `tools/call` | Runs the named tool; result as described above. |
| `ping` | Empty result `{}`. |
| unknown method | JSON-RPC error `-32601` (method not found). |
| malformed line | JSON-RPC error `-32700` (parse error), `id: null`. |

## Tools

Each maps to a `c3` subcommand. `collab_dir` (default `.collab`) is accepted by every tool
that has one; omitted from the argument column below for brevity.

| Tool | Input schema (required in **bold**) | Runs |
|---|---|---|
| `c3_providers` | `provider?`, `short?`, `no_network?` | `c3 providers [--provider] [--short] [--no-network]` |
| `c3_consult` | **`task`**, **`purpose`**, **`brief`**, **`prompt`**, **`reply_name`**, `mode?`, `thread?`, `provider?`, `model?`, `effort?`, `max_words?`, `timeout_sec?`, `continue_sec?`, `range?`, `artifact?[]`, `raw?`, `dry_run?`, `skip_preflight?`, `off_peak_only?`, `codex_config?[]`, `schema_transport?`, `format_retry?`, `telemetry?` | `c3 consult …` (may take minutes; a background panel with detach/status is **not** available yet — milestone 4) |
| `c3_findings_list` | **`task`**, `all?` | `c3 findings --task … --list [--all]` |
| `c3_findings_stats` | **`task`** | `c3 findings --task … --stats` |
| `c3_findings_status` | **`task`**, **`id`**, **`status`**, `note?`, `evidence?` | `c3 findings --task … --id … --status … [--note] [--evidence]` |
| `c3_rate` | **`task`**, **`n`**, **`useful`**, `note?` | `c3 findings --task … --rate <n> --useful … [--note]` |
| `c3_scoreboard` | `task?`, `json?` | `c3 scoreboard [--task] [--json]` |
| `c3_pack` | **`brief`**, **`out`**, `focus?[]`, `budget?`, `task?` | `c3 pack --brief … --out … [--focus] [--budget] [--task]` |
| `c3_explain` | **`claim`**, `focus?[]`, `budget?`, `audience?`, `out?` | `c3 explain --claim … --yes [--focus] [--budget] [--audience] [--out]` (always passes `--yes`; the pack is meant to leave the machine) |
| `c3_snapshot` | `out?`, `delta?`, `depth?`, `budget?`, `focus?[]` | `c3 snapshot [--out] [--delta] [--depth] [--budget] [--focus]` |
| `c3_telemetry_status` | *(none)* | `c3 telemetry status` |

Not exposed: `complain` and `forget-me` (interactive), `panel`/`index` (not built), and
anything that commits — there is no such tool.

## Registering the server in Claude Code

Add a `.mcp.json` at the repository root (the server inherits that repo as its working
directory):

```json
{
  "mcpServers": {
    "c3": {
      "command": "c3",
      "args": ["mcp"]
    }
  }
}
```

If `c3` is not on `PATH`, use the absolute path to the built binary as `command`.

## Invariant

**No tool commits.** Every tool only reads the repository and records consultation
artifacts (briefs, replies, ledger entries, findings, packs, snapshots) as files. `git
add`/`commit` and the eck finish loop stay with the coordinator — C3 never commits.
