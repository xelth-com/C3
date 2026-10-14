Write in English.

# Handoff 49 - claude: wave 8 "quota windows", third round (F48-1 answered: per-window merge)

Date: 2026-10-14. Base commit: `0f2cddd` (branch `main`; version 0.2.0, to become 0.2.1 on ACCEPT). The range under
review is `c23869d..0f2cddd` - wave 8c; skip `.collab/`. Wave 8 as a whole is `a7f2626..0f2cddd` (handoffs 45, 47).
The wave note: `docs/port/wave8-quota.md`, paragraph "8c".

## Question

Your round 2 (reply 48) held wave 8 on F48-1 (major): the equal-time rule compared primary first, so a blocking
weekly window could be hidden, and it differed from the rollout/sub-agent aggregation. Fixed in 0f2cddd. Can wave 8
be accepted and C3 0.2.1 tagged? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta (`crates/c3-core/src/quota.rs`, docs)

- **F48-1:** at an equal `observed_at` the windows are merged PER WINDOW: the primary window is the observation
  with the higher primary `used_percent` (with that observation's primary reset), the secondary window
  independently the higher secondary (with its reset); a strictly newer observation still replaces both.
  `select_window` replaces `window_obs_wins` for both selections of `endpoint_window` (recorded and live); a new
  `merge_observation` applies the same rule to the rollout and sub-agent aggregation, so the three places agree;
  `newer_wins` unchanged. The doc states the plugin's T17 fix must use the same per-window rule.
- Tests: `endpoint_window_at_an_equal_time_merges_per_window_in_both_orders` (40/99 and 50/20 -> primary 50,
  secondary 99, the weekly window blocks in both orders), `the_live_window_merges_per_window_with_a_crossing_recorded_one`
  (both live/recorded assignments), `merge_observation_keeps_each_fuller_window_at_an_equal_instant` (and a newer
  observation replaces both); the two 8b tests kept.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| quota tests | `cargo test -p c3-core quota -j 2` | 0f2cddd | 0 | 27 passed | completed |
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | 0f2cddd | 0 | 814 (813 + 1 net); two timing failures under a CPU load, green alone (`a_bridge_that_dies_during_the_format_repairs_kill_leaves_its_record` is load-sensitive, unrelated to this wave); clean | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff c23869d..0f2cddd` is one source file and two docs - read the
hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** The per-window merge: an ordering of observations (recorded, live, sub-agent) where a blocking window of
  either kind is still hidden, or a reset time attached to the wrong window?
- **Q2.** Verdict: ACCEPT (tag v0.2.1), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
