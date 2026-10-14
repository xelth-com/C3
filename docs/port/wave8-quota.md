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

## 8b - the equal-time tie (reviewer finding F46-1)

`endpoint_window` took a candidate only when its `observed_at` was STRICTLY newer, so at an equal
timestamp (the ledger writes `observed_at` without fractional seconds, ties are likely) the first
entry stayed: a 30 % observation could hide a 99 % one and the reviewer stayed available. The
recorded-observation selection and the live-rollout replacement now share ONE comparison
(`quota::window_obs_wins`): newer `observed_at` wins; at an equal instant the higher primary
`used_percent` wins, then the higher secondary (the rule `newer_wins` already applied to rollouts).
Tests: `endpoint_window_at_an_equal_time_keeps_the_fuller_window_in_both_orders` and
`the_live_window_replaces_a_lower_recorded_one_at_an_equal_time` (both failed before the fix).
`observed_at` comparison sites: `quota.rs` recorded selection (fixed), live replacement (fixed),
`newer_wins` (already correct); `health.rs` only consumes `endpoint_window` (n.a.); the machine-wide
record merge has no `observed_at` comparison (n.a.).

Plugin parity: `codex-consult-common.ps1` `Get-EndpointWindow` (lines ~6985 and ~6989) uses
`-gt $best.ObservedAt` for both the recorded and the live selection - the SAME tie. A plugin
TECH_DEBT candidate (not edited here).

## 8c - the equal-time merge per window (reviewer finding F48-1)

The 8b tuple rule (primary first) could hide a blocking weekly window: at an equal `observed_at`,
primary 40 % / secondary 99 % lost to primary 50 % / secondary 20 %. The coordinator's rule: at an
EQUAL `observed_at` the windows merge PER WINDOW - the endpoint's primary is the observation with
the higher primary `used_percent` (with that observation's reset), the secondary independently the
higher secondary (with its reset); a strictly newer `observed_at` still replaces both.
`quota::select_window` (recorded and live selection of `endpoint_window`, replacing
`window_obs_wins`) and `quota::merge_observation` (the sub-agent and account-rollout aggregation,
built on `newer_wins`, which still decides newer-or-not) apply it. Tests:
`endpoint_window_at_an_equal_time_merges_per_window_in_both_orders`,
`the_live_window_merges_per_window_with_a_crossing_recorded_one` (both assignments),
`merge_observation_keeps_each_fuller_window_at_an_equal_instant`; the 8b tests stay.
The plugin's T17 fix of the tie (see 8b) must use the same per-window rule.

## 8d - a total order for tied windows (reviewer finding F50-1)

At an equal `observed_at` and an equal `used_percent` the per-window pick kept the FIRST window's
reset, so a null or past reset could hide a future one by ordering. One comparator
(`quota::cmp_window_rank`, used by `select_window` for the recorded and live selections and by
`merge_observation`) now ranks two observations of the same window at the comparison time `now`:
(1) a known FUTURE reset beats a known PAST reset beats a missing (null) one; (2) the higher
`used_percent`; (3) the later `resets_at`; (4) identical - either. A strictly newer `observed_at`
still replaces both windows. Tests: `equal_shares_rank_by_the_reset_in_every_order`,
`the_live_window_ranks_a_future_reset_over_a_null_one_at_a_tie`,
`a_past_reset_never_hides_a_future_one_whatever_the_share`,
`the_tie_selection_is_the_same_for_every_permutation` (4 observations, all 24 orders).
The plugin's T17 fix must use this full ordering (future > past > null, then share, then later reset).

## 8e - one comparison instant, finite percentages (reviewer findings F52-1, F52-2)

The 8d rank depends on `now`, and each comparison sampled the clock on its own, so a reset crossing
from future to past DURING one aggregation (the rollout reads happen between merges) made the pick
order-dependent (A=(100 %, R), B=(99 %, S), C=(30 %, S), R < S). `now` is now sampled ONCE at the
top of each public entry point - `endpoint_window` (-> `endpoint_window_at`), `account_window`
(-> `account_window_at`), `codex_run_quota` - and passed as a parameter to `select_window`,
`merge_observation` and `cmp_window_rank` (no clock read inside the comparator); the health
evaluation hands the same `now_utc` to `endpoint_window_at` and to the "reset in the past" verdict.
F52-2: `finite_percent` treats a non-finite or out-of-range (outside 0..=100) share - a "NaN" /
"inf" text included - as an ABSENT window, in the window states (the block evaluation), the
selection and the rollout parser; the comparator uses `f64::total_cmp`. Tests:
`one_comparison_instant_makes_the_aggregation_order_independent` (A/B/C, all 6 orders, an instant
before R keeps A, one between R and S keeps B, both block),
`a_non_finite_percent_never_hides_a_valid_block`,
`the_tie_selection_is_the_same_for_every_order_of_six` (720 orders, a past reset and a NaN included).
The plugin's T17 fix needs the same single instant and the finite check.

## 8f - the account cache keeps raw observations, newer invalid windows do not replace (F55-1, F55-2)

F55-1: `account_window` cached only the WINNER picked at an earlier instant and returned it for 60 s,
so after that winner's reset a discarded equal-time observation that still blocked was lost. The
cache (per codex home) now holds the RAW observations of the account's rollouts
(`read_account_observations`); `account_window_at` runs the selection (`merge_observation`) at
every lookup with the requested `now`, the 60 s TTL only bounds how often the rollouts are read.
F55-2 (fail closed, per window): an observation with no VALID window is absent; a strictly newer
observation replaces a window only where its own window is valid, otherwise the older valid window
of that kind stays, and an older observation fills only the kinds the newer one lacks - in
`select_window` (recorded and live) and in `merge_observation`; at an equal instant the 8d rank
decides. Tests: `the_account_cache_selects_at_each_requested_instant`,
`a_newer_invalid_record_keeps_the_older_valid_window` (NaN / out of range, both orders, plus the
newer-partial case), and the permutation test now runs 5040 orders of seven observations (a
NaN-only newer record included). The tests that read `endpoint_window` are serialised with those
that switch the process-wide live source on (a flake of the shared global). The plugin's T17 fix
needs the same cache shape and the per-window validation.

## 8g - every window carries its OWN observation instant (reviewer finding F58-1)

Partial-window aggregation kept an older observation's window but stamped the aggregate with the
NEWER observation's instant, so a later comparison treated the retained window as newer than it
was: with t1 < t2 < t3, A = primary 30 % at t1, B = secondary 10 % at t3, C = primary 99 % at t2,
the fold A, B, C kept 30 % (B had promoted A's primary to t3) and A, C, B kept 99 %. Now
`Observation` carries `primary_at` / `secondary_at` (`window_at(kind)`: its own, else `at`),
`EndpointWindow` and `AccountWindow` carry `window_at: [primary, secondary]`, and a candidate window
is compared only with the instant of THAT retained window (strictly newer replaces, equal takes the
8d rank, older is kept; an invalid window has no instant and never replaces). The aggregate
`observed_at` / `at` is derived (the latest per-window instant) for display and the age of the
record, never for a replacement decision. The instants flow: rollout parse (`at` of the event) ->
`merge_observation` (per-window instants kept) -> `account_window_at` (`AccountWindow.window_at`
from the aggregate, over the raw cache) -> `endpoint_window_at` (recorded candidates carry their
entry instant on both windows, the live candidate its `window_at`) -> `select_window` (compares per
window with `EndpointWindow.window_at`). Tests: `partial_windows_keep_their_own_instants_in_the_recorded_selection`,
`partial_windows_keep_their_own_instants_in_merge_observation`,
`partial_windows_keep_their_own_instants_across_the_recorded_live_split`,
`distinct_instant_partial_windows_are_order_independent` (720 orders, distinct instants, partial
windows, a NaN-only one). The plugin's T17 fix needs per-window timestamps too.

## 8h - the per-window instants survive persistence (reviewer finding F60-1, supersedes F58-1)

8g kept each window's instant in memory, but `Observation::to_quota` wrote ONE aggregate
`observed_at`, so a recorded aggregate (the ledger's `quota`, the machine-wide health record - both
carry the quota object verbatim) lost the provenance: `endpoint_window_at` rebuilt both instants
from it, and an aggregate of A (primary 30 at t1) and B (secondary 10 at t3) labelled its primary
with t3 and beat C (primary 99 at t2). The quota record now carries two optional keys after
`observed_at`: `primary_observed_at` and `secondary_observed_at` (ISO, local offset), written by
`to_quota` only for a window that is present (a null quota keeps exactly `primary, secondary,
observed_at`). They are written wherever the quota is written (the ledger entry of a codex run, the
machine-wide record) and read by the recorded selection in `endpoint_window_at`; the aggregate
`observed_at` is only the LEGACY fallback of a record written before 8h (keys absent), and the
selected `EndpointWindow.quota` is re-stamped with them. Plugin compatibility: the oracle reads
only keys it knows; its one strict key check is on the NULL quota (`primary,secondary,observed_at`),
which is unchanged. Tests: `the_quota_record_carries_per_window_instants_through_ledger_and_health`,
`a_persisted_aggregate_cannot_promote_its_older_window` (both recorded orders and C live),
`a_legacy_record_without_window_instants_uses_the_aggregate_instant`. The plugin's T17 fix needs
the same two ledger fields.

## 8i - lossless instants, the imprecise rule, the legacy policy (reviewer finding F62-1)

8h stored each window's instant through a seconds-only formatter, so a recorded 99 % observed at
12:00:00.900 read back as 12:00:00 and an OLDER live 30 % at 12:00:00.100 looked strictly newer and
replaced it. Three rules now:
1. **Lossless**: `primary_observed_at` / `secondary_observed_at` are written as RFC3339 with the
   fractional seconds the instant has (`2026-10-13T12:00:00.900+00:00`; no fraction when it has none),
   in `to_quota` and in the re-stamp of `select_window`; read back exactly. The aggregate
   `observed_at` keeps its seconds-only format (compat).
2. **Imprecise compare** (`cmp_instants`): an instant without a fractional part is imprecise (a legacy
   aggregate, a second-precision record, a whole-second rollout stamp). If either side is, the two
   are compared at whole-second granularity: a recency win needs a strictly later whole second, the
   same second is a tie and the 8d rank decides (future reset > past > null, higher share, later
   reset). Both precise: exact. (A precise instant that happens to be exactly on a second reads as
   imprecise - the conservative side.)
3. **Legacy records** (no per-window keys): the window instants are UNKNOWN (`EndpointWindow.
   window_legacy`; the aggregate stands in only for the legacy-vs-legacy comparison). A legacy window
   never wins on recency against a window with a known instant - that pair is decided by the 8d rank
   alone; no per-window key is written for a legacy window.
Tests: `a_subsecond_instant_survives_the_record_and_beats_an_older_live_one`,
`a_second_precision_record_ties_with_a_live_instant_of_the_same_second`,
`a_legacy_record_never_wins_on_recency_against_a_known_instant` (replaces the 8h legacy test; the
health verdict sees the block). Pairwise comparisons mixing precise and imprecise instants are not
strictly transitive across three or more observations within one second; the rank keeps each pair
deterministic. The plugin's T17 fix needs the same three rules.

## 8j - ONE total order per window (supersedes the 8i pairwise precision rule)

The 8i rule (whole-second compare only when a side is imprecise) was not transitive across mixed
precisions inside one second, and a legacy window decided by rank against known ones broke the fold
the same way. Now every instant comparison in the selection and the merge is made at WHOLE-SECOND
granularity (`cmp_instants`: floor to the second; fractions are never an ordering key): a strictly
later second wins on recency; the same second is a tie that `cmp_window_rank` (8d) decides. The
fractional instants stay stored, re-stamped and round-tripped exactly (8i) - bookkeeping, not a
key. Within one kind of window (known instants, or legacy aggregates compared at the same
granularity) (floor second, rank) is a lexicographic total order. The legacy rule stays - an unknown
instant never wins on recency against a known one, the rank alone decides - and to keep the fold
order-independent `select_window` keeps the best KNOWN and the best LEGACY candidate per window
(`EndpointWindow` slots) and resolves the visible window from that pair (the legacy one only if it
outranks the known one by the rank), so the pick is a function of the candidate SET. Inside one
second the higher-usage window wins: the conservative side (usage only rises within a window; a
reset is already ranked by future/past). Tests: `whole_second_order_is_transitive_for_the_abc_precision_case`
(a imprecise :00 50 %, b :00.100 99 %, c :00.900 30 %, six orders -> 99 %),
`mixed_precision_candidates_give_one_pick_in_every_order` (six candidates - two imprecise, two with
fractions in one second, one in the next second, one legacy, partial windows - 720 orders through
the recorded selection and `merge_observation`, plus a recorded/live split); the 8i sub-second test
now passes by rank (99 % at :00.900 against live 30 % at :00.100). The plugin's T17 fix uses the
same total order.
