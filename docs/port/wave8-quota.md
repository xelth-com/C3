# Wave 8 - quota windows (the plugin's wave 30, TECH_DEBT T15)

Spec: the plugin's `bade7cb..a162b8d` (`[Unreleased]` 0.6.2) - README "Quota windows and the reading
budget", CHANGELOG "Wave 30", TECH_DEBT T15, `tests/harness-quota.ps1` (the oracle) and
`tests/fake-rollout.ps1`. Branch `wave-8-quota`.

## What was ported

| plugin (PowerShell) | C3 |
|---|---|
| `Get-RolloutTokenEvents`, `Get-RolloutUsage`, `Read-RolloutText/Head` (the LAST `token_count` naming a window, one call per changed total, the baseline before the run, the tail read) | `c3_core::quota::{rollout_usage, read_rollout_head}` (each `token_count` line parsed as JSON, `payload.type == token_count`) |
| `Get-CodexRolloutPath` (UUIDv7 day +-1, then the whole tree), `Get-CodexSubagentRollouts` (`parent_thread_id`, recursive), `Get-CodexRunQuota` (fork / resume baseline) | `codex_rollout_path`, `subagent_rollouts`, `codex_run_quota` |
| `Select-NewerWindow`, `Get-QuotaWindowStates`, `Format-QuotaWindows`, `Format-QuotaLowWarning`, `Format-QuotaReset`, `Format-QuotaPercent` | `newer_wins`, `window_states`, `format_windows`, `format_low_warning`, `format_reset`, `format_percent` (banker's rounding as `[Math]::Round`) |
| `Get-CodexAccountWindow` (today + yesterday, 16 newest, openai sessions, cached 60 s), `Get-EndpointWindow`, `New-QuotaWindowRecord` | `account_window`, `endpoint_window`, `health::quota_window_record`; `EndpointHealth {window, window_text, window_low}`, `Record.failure_kind = "window"` |
| `Get-PreflightVerdict` (window: reason/refusal; `QuotaWarning`), `Format-RowVerdict`, the `-Provider` exit code | `verdict_with_credential` (`quota_warning`), `providers::format_row_verdict`, `exit_code_for_verdict` (`available (` = 0) |
| `New-MachineHealthRecord -Quota` (class `''`), `ConvertTo-MachineHealthEntries`, the prune | `health::new_machine_health_record_quota`, `MachineEndpoint.quota`, `machine_consults_filtered` (class `""` -> outcome `""` with `quota`), the prune keeps a record whose window still blocks |
| `Get-ForkReplayGuard` (512 / 128 KB), the refusal / warning / dry-run line, the partial's `-Mode new` hint | `fork_replay_guard` in `build_context` (before the argv), `build_partial_reply` + `summary::build_resume_command {budget, new_thread_partial, brief}` |
| `Wait-EngineProcess -BudgetPercent` (15 s, `CODEX_CONSULT_TEST_BUDGET_POLL_MS`), the budget stop (no continuation, partial, class operator, hint), the continuation skip at the budget | `subprocess::BudgetWatch` (`TurnStop::Budget`), `StopKind::Budget`, `finish()` / `run_timeout_continuation` |
| `-c features.multi_agent=false` right after `-m` (main, continuation, repair), `subagents_allowed` | `Request.agent_config` (`SubprocessEngine::plan_codex`), `orchestrate::agent_config_for` |
| ledger `quota`, `tokens`, `subagents_allowed`, `budget` after `commit_wait_ms`; the summary `quota      :` line, the handoff `Quota (rollout):` line, the dry run's `quota/budget/sub-agents/replay` lines and preview | `LedgerEntry` (tri-state, declaration order), `quota_summary_line`, `OptionalRecords.quota`, `dryrun::quota_lines` |
| `-Budget` / `CODEX_CONSULT_QUOTA_BUDGET` (refusals), a panel's `budget` / `budget_source` to its members, a detached run's `-Budget` | `--budget`, `args::resolve_run_budget`, the member spec args, `DetachArgs.budget`; the consult shim forwards `-Budget` when bound |
| `Get-TelemetryOutcome` (a budget stop = class `quota`) | `telemetry::classes::outcome_class` |
| the reading discipline of checkpoint, core-contract, acceptance, diff-review | `prompt::presets::text` + `c3_core::quota::READING_DISCIPLINE` |
| `codex-providers.ps1` QUOTA column (last), `-Json` `quota {text, primary, secondary, observed_at, source}` | `providers.rs` (`Row.quota`, `quota_json`) |

The live source (the codex home's newest rollout for the built-in openai endpoint) is turned on once
per process by `c3` (`providers::enable_quota_live_source`) when the machine-wide health is on
(`CODEX_CONSULT_HEALTH` not `none`), exactly the plugin's condition; the c3-core unit tests never turn
it on (the ledgers alone).

## What differs

- The rollout reader parses each `token_count` line as JSON (the plugin matches regexes on the raw
  line); on every shape codex-cli 0.155 and the fakes write the results are the same, and a line whose
  text merely contains `token_count` is not taken for an event.
- A kicked codex turn's partial footer now names the run's own thread for a resume (the plugin's
  `$threadId` first) - C3 named none before unless a continuation ran; the budget stop needs it.
- The machine-wide records keep C3's `when` in UTC (+00:00), as before this wave.

## Verification (2026-10-14)

Harnesses through the shim against the plugin's CURRENT tests (HEAD `33ba041`, archived to
`%TEMP%\c3-wave8\pin`; the six shims of `tests/shim/` over its scripts dir, the HEAD
`codex-consult-common.ps1` and `codex-consult-detached.ps1` kept for the in-process UNIT checks;
binary copy `%TEMP%\c3-wave8\c3.exe`; Windows PowerShell 5.1; one at a time under HARNESS.lock;
driver `%TEMP%\c3-wave8\drive.ps1`, logs `%TEMP%\c3-wave8\logs\`):

| harness | wave 8 | expected | note |
|---|---|---|---|
| harness-quota | **57/0** | 57/0 | the oracle of this wave (first run) |
| harness-0.3 | **229/0** | 229/0 | LEDGER's HEAD order ends `commit_wait_ms,quota,tokens,subagents_allowed,budget` |
| harness-claude | **87/0** | 87/0 | also 87/0 run from the plugin checkout's own `tests/` (RC7's first-pass setup) |
| harness-roster | **125/0** | 125/0 | |
| harness-host | **59/6** | 59/6 | the six Z Code / Codex-host checks, not applicable (`harness-shim.md`) |
| harness-fixes | **56/0** | 56/0 | |
| harness-telemetry | **47/96** | 47/96 | P7 by design (C3's own telemetry root) |

RC7's harness-claude E2 at plugin HEAD (65/1, "usable reply", then the harness died): not
reproducible - harness-claude.ps1 and fake-claude.ps1 are byte-identical at v0.6.1 and HEAD (the
HEAD test changes are the codex fakes, fake-rollout, harness-quota, harness-0.3's order and run-all),
and two HEAD runs (the pinned copy and the checkout itself) give 87/0. The symptom was the drift run
writing NO ledger entry (Last-Entry returned the previous usable entry), then the next run in another
repository writing none either and Last-Entry of an empty ledger throwing - a transient refusal in that
pass, not a T15 behaviour; nothing in wave 8 targets it.

Cargo: `cargo test -j 2 --no-fail-fast --workspace` = **811 tests** (baseline 795 + 16). The first
run 809/1 (telemetry_parity's LEDGER_ORDER constant, updated to the HEAD order; 7/7 after). The final
run 800/11 on a machine at 100 % CPU (four cargo jobs of other sessions and a stray `find`): every one
of the 11 a timing failure (`claude --version` did not finish within 15 s, the linear-cost bound of
`ingest`, the kill-record and stall-cut timings) - all green when re-run one binary at a time
(`claude_engine` 15/15, `compat_wave2b` 17/17, `kill_record_durable` 12/12 with `--test-threads=1`,
`consult::ingest` 20/20). The C: drive was down to 0.8 GB during that run (LNK1318 PDB LIMIT on the
first attempt); 12 GB of idle 9eck.com build artifacts (deps older than 2026-10-07) and C3's own
`target/debug/incremental` were deleted under the standing permission for build artifacts. Clippy
`--workspace --all-targets -D warnings` clean, rustfmt clean. New tests: `c3_core::quota` (9: the
reader on a fork + parent + sub-agent, null windows, the views at a fixed clock, newer-wins, the
replay guard, the budget resolution, the budget watch rule, multi_agent, UUIDv7), `health::
quota_window_tests` (4: 99 % unavailable with its reset and the refusal, 84 % low quota, a past reset
and a 0.6.1 entry and newest-wins, the machine record class `""` round trip), `ledger` (the key order
and tri-states), `summary` (the `-Budget` and the new-thread hint), `telemetry::classes` (a budget stop
is class quota), `pending_liveness::the_budget_watch_stops_the_turn_at_the_quota_budget` (the real
spawn/kill path, a 200 ms poll).
