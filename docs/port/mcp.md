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
guarantees **the same implementation and the same file set as a CLI run** — not byte-identical
artifacts across separate runs, since some written fields (timestamps, ids, snapshot anchors)
are run-dependent by design — and isolates each tool's locks and timeouts in a child process.
The child inherits the environment and the server's working directory (the repository the
coordinator runs in); its stdin is closed.

Path-shaped arguments (`collab_dir`, `brief`, `out`, `focus[]`, `artifact[]`) must be
relative to the working directory: an absolute path or any `..` component is rejected
outright, even one that would normalize back inside (`foo/../bar`) — the rule is lexical
containment, not "escapes after normalization" (`c3-core::task_slug::contained_join`).
The working directory and the joined argument are then both canonicalized (symlinks and
junctions resolved; for a path that does not exist yet, its deepest existing ancestor is
resolved and the missing tail re-appended), and the argument is rejected unless its
resolved form still starts with the resolved working directory — so a normal-looking path
component that is actually a symlink or junction pointing elsewhere is refused too.

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
| `c3_consult` | **`task`**, **`purpose`**, **`brief`**, **`prompt`**, **`reply_name`**, `mode?`, `thread?`, `provider?`, `model?`, `effort?`, `max_words?`, `timeout_sec?`, `continue_sec?`, `range?`, `artifact?[]`, `raw?`, `dry_run?`, `skip_preflight?`, `off_peak_only?`, `codex_config?[]`, `schema_transport?`, `format_retry?`, `telemetry?` | `c3 consult …` (may take minutes) |
| `c3_panel` | **`task`**, **`brief`**, `artifacts?[]`, `purpose?`, `size?`, `order?`, `require?[]`, `role?`, `roles?[]`, `topics?[]`, `dry_run?`, `detach?`, `timeout_sec?` | `c3 consult --panel …` — a background review panel. `detach` **DEFAULTS TO TRUE** (a panel takes minutes; the call returns at once with the three lines `c3 consult --status`/`--wait`/`--id` print). `dry_run` prints the plan and writes nothing, and never carries `--detach` (the CLI refuses the two together). |
| `c3_status` | **`task`**, `id?`, `wait?`, `wait_timeout_sec?` | `c3 consult --status` or, with `wait: true`, `c3 consult --wait`. Read-and-record: with `wait: true` it blocks until the run finishes or `wait_timeout_sec` elapses (default 300, 1..=3600). The **exit code is the answer**, returned alongside the text: `0` usable, `1` failed or refused, `2` still running, `3` wait timeout, `4` ambiguous id, `5` a required reviewer missing. |
| `c3_router_explain` | **`purpose`**, `topic?[]` | `c3 router explain --purpose … [--topic]...` — the score table the next routed draw would use. |
| `c3_router_replay` | **`task`**, `nn?` | `c3 router replay --task … [--nn]` — replays a routed panel from the ledger and confirms the seats match. |
| `c3_findings_list` | **`task`**, `all?` | `c3 findings --task … --list [--all]` |
| `c3_findings_stats` | **`task`** | `c3 findings --task … --stats` |
| `c3_findings_status` | **`task`**, **`id`**, **`status`**, `note?`, `evidence?` | `c3 findings --task … --id … --status … [--note] [--evidence]` |
| `c3_rate` | **`task`**, **`n`**, **`useful`**, `note?` | `c3 findings --task … --rate <n> --useful … [--note]` |
| `c3_scoreboard` | `task?`, `json?` | `c3 scoreboard [--task] [--json]` |
| `c3_pack` | **`brief`**, **`out`**, `focus?[]`, `budget?`, `task?` | `c3 pack --brief … --out … [--focus] [--budget] [--task]` |
| `c3_explain` | **`claim`**, `focus?[]`, `budget?`, `audience?`, `out?` | `c3 explain --claim … --yes [--focus] [--budget] [--audience] [--out]` (always passes `--yes`; the pack is meant to leave the machine) |
| `c3_snapshot` | `out?`, `delta?`, `depth?`, `budget?`, `focus?[]` | `c3 snapshot [--out] [--delta] [--depth] [--budget] [--focus]` (a full, non-delta snapshot also updates the anchor and sequence state under `<collab>/.c3/`) |
| `c3_telemetry_status` | *(none)* | `c3 telemetry status` |
| `c3_index_stats` | `collab_dir?`, `conn?` | `c3 index stats --json [--collab-dir] [--conn]` |
| `c3_index_query` | **`query`**, `budget?`, `json?`, `collab_dir?`, `conn?` | `c3 index query <query> [--budget] [--json] [--collab-dir] [--conn]` |

Not exposed: `complain` and `forget-me` (interactive), `index build` and `index rebuild`
(they take the index lock for minutes and stay CLI-only), anything that stops or prunes a
detached run (`--kick`, `--prune`), anything that fetches priors from the network or edits a
roster, and anything that commits — there is no such tool.

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
artifacts (briefs, replies, ledger entries, findings, packs, snapshots) as files; no tool
dispatched here runs a Git write command (`add`/`commit`/etc.), and the eck finish loop
stays with the coordinator. This is a statement about what C3's own tools do, not a
sandbox around a reviewer process a tool launches: `c3_consult` runs each reviewer in its
own sandbox or read-only mode, and the engine tree check reports any working-tree or HEAD
change it observes around a run, but neither is a preventive guarantee against a
descendant process that chooses to run Git itself.
