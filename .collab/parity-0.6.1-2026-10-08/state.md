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
- 2026-10-08 23:00: RC2 done (docs/port/rc2-oracle-2026-10-08.md, binary bd96c44): real C3 oracles = roster, panel, 0.3, engines, muse, format, fixes, detach, pending, companions, fixes26b/27c/28b/28c + the C3 parts of claude/telemetry/visibility; lock2 and 3b test the PowerShell library only; ~100 checks need a TELEMETRY SHIM (telemetry 48, fixes28e 24, fixes28d 25, detach 4); visibility/fixes28d crash because they extract functions from the real codex-consult.ps1 source; host hung once (30 min) on a shimmed -DryRun. Failures mapped: wave 2 = ledger field order (0.3 LEDGER, roster WALK/RATE, companions ROLE/RATE, engines/muse RUN, telemetry SPOOL/RATE); wave 3 = fixes28e RECORD/NOTSPOOLED/MARKER, fixes E28, fixes28d; wave 4 = claude DRYRUN/ENGINEEXE. NOT attributable (pre-0.6 gaps, need triage = wave 2b): agy, muse DRYRUN/RUN/PANEL, companions routing/roles, fixes27c/28b/28c, detach SINGLE/CARRY, panel SPEC, pending (e), format CONTR, roster TIMEONLY (pwsh 7 date artifact), 0.3 CFG.
- 2026-10-08 23:25: RC2 triage (docs/port/rc2-triage-2026-10-08.md): ~52 C3-gaps, ~21 shim artifacts, 4 harness-env. Wave 2b (after wave 2 merges) = the test-mode warning + CODEX_CONSULT_TEST_* scrub from children (~15 checks), the health journal at a failed commit + orphan replay + .bad, small compat bugs (kick during continuation, pending (e) outcome, store rename retry, -MaxModelSteps on a codex member, launch-mark hooks, STREAM D6), -Require on a single run and the role/topic slug validation, the hook second line + -Explain; shim fixes (optional -Task, -By, -CodexConfig whole, the plugin tree staged, the schema path). Decisions: claude wording = wave 4; the Z Code host checks are NOT applicable (C3 is Claude Code only - record as n/a in the shim note); the ledger fields (consult_ref, context_window, compactions, preview topics/role/format_retry) go with wave 2a.
- 2026-10-09 02:00: wave 2 (telemetry: 3d33a54 consult_ref/judge/rating_rev/backfill + ledger order; 136cbf8 the outbox + forget-me identity; b1ba064 closed classes + the telemetry shim) merged 3605395 - 576 tests; harness-telemetry 45/98 (81 of the 98 read the PLUGIN own spool/salt/instance: C3 keeps its own telemetry root <codex home>/c3/telemetry and app id c3 - the identity question goes to Astra, handoff 07); roster 124/1, 0.3 227/2, format 37/0, fixes28c 13/2. Astra diff-review of wave 2 running (5bdfbe48). Wave 2b (the triage gaps, launcher quoting via raw_arg, shim fixes) running on branch wave2b-compat.
- 2026-10-09 03:50: Astra on wave 2 (handoff 09): HOLD F09-1..F09-5 (+F09-6 minor). P7 (decided): C3 KEEPS its own telemetry app id, root and outbox; the plugin harness-telemetry checks that read the plugin paths are adapted as fixtures later (storage/app differ, semantics asserted); a shared logical identity across bridges is not promised - the owner of a consultation telemetry is the bridge that recorded it. Wave 2d (F09 fixes) starts now in parallel with 2b.
- 2026-10-09 04:10: wave 2b (2ebddcb, 460f00b, 7dace6e) merged 1fa1ba7 - 600 tests; 13 baselined harnesses 806/59 -> 853/12, host 55/10 (no hang). Astra diff-review of 2b running (handoff 10). Wave 2d (F09 fixes) running.
- 2026-10-09 04:20: Astra on wave 2b (handoff 11): HOLD F11-1..F11-3 (+F11-4 minor, F11-5 major agy stall) - wave 2e starts now; F11-3 is INHERITED from the plugin (the plugin filters the roster before plan evaluation too) -> file it for the plugin 0.6.2 as well. Wave 3a (recovery) running; 2d (F09) running.
- 2026-10-09 05:00: wave 2d (F09-1..6, bc8d605) merged 1c8c6f8 (with 2b; conflict-free, verification of the merged main running); Astra round 2 on wave 2 (handoff 12) running. Branches live: wave2e-fixes (F11), wave3a-recovery.
- 2026-10-09 05:20: Astra out until 08:16 (second window hit of the night); Kimi k3 second opinion on wave 2d (handoff 14): HOLD F14-1 blocker (local-only forget writes no transaction record), F14-2 minor (case-sensitive provider match) -> wave 2f fix now; Astra confirms 2d+2f after 08:16.
- 2026-10-09 05:45: wave 2f (F14-1 fixed; F14-2 wontfix - the plugin matches providers case-sensitively, the doc corrected) merged 5c9f8a0; 614 tests on the branch.
- 2026-10-09 06:20: merged main 1c8c6f8 verified (610 tests, clippy/fmt clean; 0.3 229/0, companions 42/0, panel 62/0, roster 124/1 FILE, telemetry 46/97 by design P7, host 55/10). Host PREFIX D6 x2 (--brief-prefix never ported) and REFUSE D3 (coordinator refusal wording) stay open -> port --brief-prefix and the refusal text in wave 4 or a small 2g. Harness runs need PSModulePath reset when inherited from pwsh 7 (Get-FileHash).
- 2026-10-09 06:40: wave 2e (F11-1..5, 4b51226) merged 4597ec5 (611 tests on the branch); awaiting Astra (08:16) for: wave 2 round 2 (2d+2f) and wave 2b round 2 (2e). Running: 3a, 3b.
