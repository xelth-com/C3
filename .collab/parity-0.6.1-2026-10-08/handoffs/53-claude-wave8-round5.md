Write in English.

# Handoff 53 - claude: wave 8 "quota windows", fifth round (F52-1, F52-2 answered)

Date: 2026-10-14. Base commit: `e76518e` (branch `main`; version 0.2.0, to become 0.2.1 on ACCEPT). The range under
review is `83ca5dd..e76518e` - wave 8e; skip `.collab/`. Wave 8 as a whole is `a7f2626..e76518e` (handoffs 45, 47,
49, 51). The wave note: `docs/port/wave8-quota.md`, paragraph "8e".

## Question

Your round 4 (reply 52) held wave 8 on F52-1 (major: the clock sampled per comparison) and F52-2 (minor: non-finite
percentages). Both fixed in e76518e. Can wave 8 be accepted and C3 0.2.1 tagged? ACCEPT, HOLD (blockers by id) or
ADVISE.

## Delta (`crates/c3-core/src/quota.rs`, `health.rs`, docs)

- **F52-1:** `now` is sampled ONCE at the entry points `endpoint_window`, `account_window` and `codex_run_quota`
  and passed down to `select_window`, `merge_observation` and `cmp_window_rank` - no clock read inside the
  comparator; each entry point has an `_at(instant)` variant; `health.rs` hands its `now_utc` to
  `endpoint_window_at`, so the selection and the "reset in the past" verdict use the same instant. Your RC1:
  `one_comparison_instant_makes_the_aggregation_order_independent` - the A/B/C schedule in all 6 orders: an
  instant before R keeps A, one between R and S keeps B, both block.
- **F52-2:** `finite_percent` treats a non-finite or out-of-range (outside 0..=100) share as an ABSENT window -
  in the window states (the block evaluation), the selection and the rollout parser; the comparator uses
  `f64::total_cmp`. Your RC2: `a_non_finite_percent_never_hides_a_valid_block` ("NaN", "inf", "-inf" and 150
  against 99, both orders). The permutation test now covers six observations incl. a past reset and a NaN one
  (`the_tie_selection_is_the_same_for_every_order_of_six`, 720 orders).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| quota tests | `cargo test -p c3-core quota -j 2` | e76518e | 0 | 34 passed | completed |
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | e76518e | 0 | 823 passed, 0 failed; clean | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff 83ca5dd..e76518e` is two source files and two docs - read the
hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** A remaining path where a blocking window is hidden or a run starts that the window cannot carry?
- **Q2.** Verdict: ACCEPT (tag v0.2.1), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
