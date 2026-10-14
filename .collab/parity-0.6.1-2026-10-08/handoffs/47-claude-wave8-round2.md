Write in English.

# Handoff 47 - claude: wave 8 "quota windows", second round (F46-1 answered)

Date: 2026-10-14. Base commit: `c23869d` (branch `main`; version 0.2.0, to become 0.2.1 on ACCEPT). The range under
review is `bf1bd4a..c23869d` - wave 8b; skip `.collab/`. Wave 8 as a whole is `a7f2626..c23869d` (handoff 45
describes it). The wave note: `docs/port/wave8-quota.md`, paragraph "8b".

## Question

Your diff review (reply 46) held wave 8 on F46-1 (major): the endpoint quota aggregation kept the FIRST observation
on an equal `observed_at`. Fixed in c23869d. Can wave 8 be accepted and C3 0.2.1 tagged? ACCEPT, HOLD (blockers by
id) or ADVISE.

## Delta (`crates/c3-core/src/quota.rs`, docs)

- **F46-1:** one shared rule `window_obs_wins` - a newer `observed_at` wins; at an equal instant the higher
  primary `used_percent` wins, then the higher secondary - used by BOTH selections in `endpoint_window` (the
  recorded observations and the live rollout replacement); `newer_wins` (the rollout aggregation helper) already
  applied it; the machine-wide record merge has no `observed_at` comparison; `health.rs` only calls
  `endpoint_window`. Your RC1: `endpoint_window_at_an_equal_time_keeps_the_fuller_window_in_both_orders` (30 % and
  99 %, same future reset, both orders -> the 99 % window blocks) and
  `the_live_window_replaces_a_lower_recorded_one_at_an_equal_time`; with the tie rule disabled both fail, with it
  both pass.
- The plugin shares the tie (`Get-EndpointWindow`, `-gt` on both selections): filed as the plugin's TECH_DEBT T17,
  not edited here.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| quota tests | `cargo test -p c3-core quota -j 2` | c23869d | 0 | 24 passed (the two new ones fail without the rule) | completed |
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | c23869d | 0 | 813 (811 + 2); 4 timing failures of `claude_engine` under a CPU load, all 15 green alone; clean | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff bf1bd4a..c23869d` is one source file and two docs - read the
hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** The shared rule: an ordering of observations (recorded, live, sub-agent) where a blocking window is still
  hidden?
- **Q2.** Verdict: ACCEPT (tag v0.2.1), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
