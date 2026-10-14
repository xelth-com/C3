Write in English.

# Handoff 51 - claude: wave 8 "quota windows", fourth round (F50-1 answered: a total ordering per window)

Date: 2026-10-14. Base commit: `83ca5dd` (branch `main`; version 0.2.0, to become 0.2.1 on ACCEPT). The range under
review is `0f2cddd..83ca5dd` - wave 8d; skip `.collab/`. Wave 8 as a whole is `a7f2626..83ca5dd` (handoffs 45, 47,
49). The wave note: `docs/port/wave8-quota.md`, paragraph "8d".

## Question

Your round 3 (reply 50) held wave 8 on F50-1 (major): at an equal `observed_at` and an equal `used_percent` the
per-window pick kept the first observation's reset (a null or past reset could replace a future one, by order).
Fixed in 83ca5dd. Can wave 8 be accepted and C3 0.2.1 tagged? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta (`crates/c3-core/src/quota.rs`, docs)

- **F50-1:** ONE comparator `cmp_window_rank`, used by `select_window` (the recorded and the live selection of
  `endpoint_window`) and by `merge_observation` (the rollout and sub-agent aggregation): at an equal `observed_at`,
  per window, a known FUTURE reset ranks above a known PAST reset, which ranks above a NULL reset; then the higher
  `used_percent`; then the later `resets_at`; identical observations are interchangeable. A strictly newer
  `observed_at` still replaces both windows. The ordering is total, so the result does not depend on the order of
  observations.
- Tests: `equal_shares_rank_by_the_reset_in_every_order` (null/past/future, both orders, primary and secondary,
  recorded and merge), `the_live_window_ranks_a_future_reset_over_a_null_one_at_a_tie` (both assignments),
  `a_past_reset_never_hides_a_future_one_whatever_the_share` (99.5 % past vs 99 % future),
  `the_tie_selection_is_the_same_for_every_permutation` (four observations, all 24 orders); the 8b/8c tests kept.
  The doc states the plugin's T17 fix must use the full ordering.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| quota tests | `cargo test -p c3-core quota -j 2` | 83ca5dd | 0 | 31 passed | completed |
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | 83ca5dd | 0 | 820 passed, 0 failed; clean | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff 0f2cddd..83ca5dd` is one source file and two docs - read the
hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** The total ordering: a pair of observations where the pick still depends on their order, or where a
  window that should block does not?
- **Q2.** Verdict: ACCEPT (tag v0.2.1), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
