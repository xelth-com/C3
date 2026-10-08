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
| E | 0.6.1 telemetry: `consult_ref` per ledger entry (consultation + rating events), rating `judge {provider, model, source}` resolved at rating time and saved in the mark, `rating_rev` under the task lock, client_time = the mark's `when`, `telemetry_sent`, `-BackfillRatings` | absent (`telemetry/mod.rs:171`, `findings_tool/mod.rs:960-1121`); the mark IS written before `record_rating` already (F02-5) |
| F | recovery E1/E18/E19/E23/E25: `unverified[]` beside `survivors[]`, fail-closed re-check (unreadable command line = running), `kill_unconfirmed` released only after both scans, a kill with only unverified descendants keeps the record | partial (`confirm_tree_kill`, `kill_unconfirmed` exist; no unverified path) |
| G | E2/E3/E20/E24/E26: per-producer not-spooled files, the `.last` fold saving before deleting (`{name, bytes}`), legacy staging, the forgetting marker with start ticks | absent (one `spool.ndjson`) |
| H | the `claude` engine: `claude -p` with `--json-schema`, auth `subscription` / `api-key` / `endpoint {base_url, env_key, timeout_ms}` + `plan`, the closed model table (incl. `claude-haiku-5-5`), child-env allow-list, strict tree check, E12-E17 (killed-turn proof, `message.model` check, `rate_limit_event`, plan wait), coordinator rule, vendor classes `anthropic`/`minimax`, fake CLI + harness | absent |

Reverse (C3 beyond the plugin): the `http` engine (OpenRouter / any OpenAI-compatible endpoint, packs instead of
tools), context packs and `c3 explain`, the SurrealDB index, the router, the scoreboard, the MCP server, Linux.

## Plan (framed with Astra, handoff 02 - ADVISE, F02-1..F02-8)

Decisions (coordinator, 2026-10-08 20:20):
- P1. Two tracks. The COMPATIBILITY release (C3 0.2.0 = plugin 0.6.1 behaviour, measured by the pinned v0.6.1
  harnesses through the shim plus live runs) and a separately measured IMPROVEMENT track (native Messages API,
  SurrealDB-derived views, an outbox) - a passing harness, a native reply and a better review are three outcomes.
- P2. Waves of the compatibility track: 1 = A + B + C + D (running, branch `wave1-parity`); 1b = the roster
  `plan` key and the machine-wide running records / quota propagation / plan wait as SHARED infrastructure
  (F02-7; today the roster rejects `plan`, so a 0.6 plugin roster refuses every C3 run); 2 = telemetry: a
  durable spool (F02-1 lost-update race), closed vendor/model classifiers for reviewers and judges (F02-2), the
  forget-me retry identity (F02-3), consult_ref / saved judge / rating_rev / telemetry_sent / backfill, the
  telemetry shim; 3 = recovery F + G; 4 = H, the `claude` engine SPAWNED (all three auth modes, as the plugin).
  Each wave: one worker with tests, the pinned harnesses through the shim, then a diff-review by Astra.
- P3. Oracle: RC2 - the pinned v0.6.1 scripts dir, all 22 harnesses sequentially, every assertion mapped to
  "C3 executes" / "copied PowerShell helper" / "documentation" (F02-4), helper-only fixtures ported to Rust tests;
  runs after wave 1 merges. RC3 - the live matrix on this machine (codex, agy, muse, claude subscription,
  api-key, each endpoint plan; a mixed panel; alternating plugin/C3 writes on one task) before the release.
- P4. The endpoint route stays spawned Claude Code for parity; native Messages is an improvement candidate
  gated on RC4 (a three-route benchmark; the plugin's 3-6x measured codex vs Claude Code, not native - F02-8).
- P5. Files stay authoritative for shared recovery and health (the plugin reads them); SurrealDB stays derived.
- P6. Findings F02-1..F02-4, F02-7 are assigned to the waves above (not fixed in wave 1).

## Log

- 2026-10-08 19:50: task opened; build/test/shim recon of main 4cd8d18 on Windows running (worker); framing brief 01.
- 2026-10-08 20:20: Astra's framing (02) rated; plan P1-P6 above; wave 1 running (worker, worktree c3-wave1);
  C3 builds on Windows (522 tests, clippy clean, `-j 2`), shim verified on harness-lock2.
- 2026-10-08 22:50: wave 1 ACCEPTED by Astra (handoff 06; F04-1..3 closed, F06-1 minor Samoa-2012 fixture = follow-up in wave 3); wave 1b (plan key, c6ac728) merged 832aaf0 - verification of the merged main running; RC2 partial (its scripts dir broke mid-run), report pending.
