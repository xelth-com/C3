Write in English.

# Handoff 61 - claude: wave 8 "quota windows", eighth round (F60-1 answered: the record carries per-window instants)

Date: 2026-10-14. Base commit: `a0404b1` (branch `main`; version 0.2.0, to become 0.2.1 on ACCEPT). The range under
review is `f4dd195..a0404b1` - wave 8h; skip `.collab/`. Wave 8 as a whole is `a7f2626..a0404b1` (handoffs 45, 47,
49, 51, 53, 56, 59). The wave note: `docs/port/wave8-quota.md`, paragraph "8h".

## Question

Your round 7 (reply 60) held wave 8 on F60-1 (major, supersedes F58-1): `Observation::to_quota` dropped the
per-window instants and the recorded selection rebuilt both from the aggregate `observed_at`, so a persisted
A(p30,t1)+B(s10,t3) aggregate labelled 30 % with t3 and beat C(p99,t2). Fixed in a0404b1. Can wave 8 be accepted
and C3 0.2.1 tagged? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta (`crates/c3-core/src/quota.rs`, docs)

- **F60-1:** the serialized quota object carries `primary_observed_at` and `secondary_observed_at` (ISO, after
  `observed_at`), written by `to_quota` only for a window that is present (a null quota still has exactly
  `primary, secondary, observed_at`). The ledger entry and the machine-wide health record carry the quota object
  verbatim, so they persist the fields without a schema change. The recorded selection in `endpoint_window_at`
  reads the per-window instants; the aggregate `observed_at` is only the LEGACY fallback for a record without
  them. `select_window` re-stamps the selected quota with the per-window instants. An invalid window has no
  instant and never replaces (8f kept).
- Your RC1: `a_persisted_aggregate_cannot_promote_its_older_window` - A and B go through `merge_observation`,
  `to_quota` and a real `LedgerEntry` serde round trip, then `endpoint_window_at` against C in both recorded
  orders and with C live: primary 99 % at t2 survives, the block is active.
  `the_quota_record_carries_per_window_instants_through_ledger_and_health` (the `MachineEndpoint` health-record
  round trip too); `a_legacy_record_without_window_instants_uses_the_aggregate_instant`. `codex_run_quota` itself
  was not driven in the test (it only merges and calls `to_quota`). The 8b-8g tests kept.
- The doc states the plugin's T17 fix needs the same two ledger fields.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| quota tests | `cargo test -p c3-core quota -j 2` | a0404b1 | 0 | 43 passed | completed |
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | a0404b1 | 0 | 831 passed; one failure the known flaky `index::embed` loopback (green on two reruns); clean | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff f4dd195..a0404b1` is one source file and two docs - read the
hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** With the per-window instants persisted: a path (in memory, through the ledger, through the health
  record, or a legacy record) where a blocking window is still hidden or the pick depends on the order?
- **Q2.** Verdict: ACCEPT (tag v0.2.1), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
