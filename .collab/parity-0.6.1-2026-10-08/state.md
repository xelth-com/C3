# parity-0.6.1 - C3 to the plugin's 0.6.1 behaviour, and beyond (2026-10-08)

Goal (the operator, 2026-10-08 19:50): bring the Rust bridge to parity with claude-codex-consult 0.6.1 and, where
C3 can do more (the http engine, packs, the SurrealDB index, the router, MCP), do better. Coordinator: Claude
Fable 5.1 in Claude Code. Acceptance authority for releases: openai :: gpt-6-astra (as for the plugin).

## Baseline (gap recon, 2026-10-08 19:40)

C3 main 4cd8d18 (the cloud session's Linux port merged; verified live only through OpenRouter) implements the
plugin up to 0.5.0 wave 26b plus the 0.5.1 rating event (without the 0.6.1 keys). Absent from 0.6.0/0.6.1:

| # | plugin behaviour | C3 |
|---|---|---|
| A | time-only reset parse (today/tomorrow candidates, 5-min allowance, DST, UTC/GMT/Z and numeric offsets, other zone words decline) | absent (`c3-core/src/health.rs` parses month-name and ISO forms only) |
| B | E27/E28: the machine-wide codex scan excludes the Codex desktop app's servers (`app-server`, `exec-server`, `mcp-server`, `login`, `app`, `codex-computer-use*`, `--parent-pid` without `exec`), never a command line with the word `exec`, unreadable/ambiguous counted as codex | absent (`liveness/proc.rs:174-212`) |
| C | roster `panel: light` (joins light purposes; stands in on a weighty one only when no sibling of its label runs) | absent (`providers.rs:667`) |
| D | E4 re-read anchor: first line whole, 300-char cut, `(+n more lines)` | not checked |
| E | 0.6.1 telemetry: `consult_ref` per ledger entry (consultation + rating events), rating `judge {provider, model, source}` resolved at rating time and saved in the mark, `rating_rev` under the task lock, client_time = the mark's `when`, the mark committed BEFORE its event is spooled, `telemetry_sent`, `-BackfillRatings` | absent (`telemetry/mod.rs:171`, `findings_tool/mod.rs:960-1121`) |
| F | recovery E1/E18/E19/E23/E25: `unverified[]` beside `survivors[]`, fail-closed re-check (unreadable command line = running), `kill_unconfirmed` released only after both scans, a kill with only unverified descendants keeps the record | partial (`confirm_tree_kill`, `kill_unconfirmed` exist; no unverified path) |
| G | E2/E3/E20/E24/E26: per-producer not-spooled files, the `.last` fold saving before deleting (`{name, bytes}`), legacy staging, the forgetting marker with start ticks | absent (one `spool.ndjson`) |
| H | the `claude` engine: `claude -p` with `--json-schema`, auth `subscription` / `api-key` / `endpoint {base_url, env_key, timeout_ms}` + `plan`, the closed model table (incl. `claude-haiku-5-5`), child-env allow-list, strict tree check, E12-E17 (killed-turn proof, `message.model` check, `rate_limit_event`, plan wait), coordinator rule, vendor classes `anthropic`/`minimax`, fake CLI + harness | absent |

Reverse (C3 beyond the plugin): the `http` engine (OpenRouter / any OpenAI-compatible endpoint, packs instead of
tools), context packs and `c3 explain`, the SurrealDB index, the router, the scoreboard, the MCP server, Linux.

## Plan (proposed; framing handoff 01)

Wave 1: A + B + C + D. Wave 2: E (the intake at xelth.com already reads consult_ref and rating_rev). Wave 3: F + G.
Wave 4: H - with the question whether the endpoint route (Anthropic-compatible endpoints of z.ai, MiMo, Kimi)
should be a native Messages-API path of the `http` engine rather than a spawned Claude Code. Each wave: one
worker with tests, the plugin's harness through the shim where one exists, then a diff-review by Astra.

## Log

- 2026-10-08 19:50: task opened; build/test/shim recon of main 4cd8d18 on Windows running (worker); framing brief 01.
