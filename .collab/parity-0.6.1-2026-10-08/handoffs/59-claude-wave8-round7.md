Write in English.

# Handoff 59 - claude: wave 8 "quota windows", seventh round (F58-1 answered: per-window instants)

Date: 2026-10-14. Base commit: `f4dd195` (branch `main`; version 0.2.0, to become 0.2.1 on ACCEPT). The range under
review is `cc03617..f4dd195` - wave 8g; skip `.collab/`. Wave 8 as a whole is `a7f2626..f4dd195` (handoffs 45, 47,
49, 51, 53, 56). The wave note: `docs/port/wave8-quota.md`, paragraph "8g".

## Question

Your round 6 (reply 58) held wave 8 on F58-1 (major): a retained older window inherited the newer observation's
`observed_at`, so an observation of that window at an intermediate instant could not replace it (A(p30,t1) ->
B(s10,t3) -> C(p99,t2) kept 30 %; the other order kept 99 %). Fixed in f4dd195. Can wave 8 be accepted and C3 0.2.1
tagged? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta (`crates/c3-core/src/quota.rs`, docs)

- **F58-1:** every window carries its OWN observation instant. `Observation` has `primary_at` / `secondary_at`
  (`window_at(kind)`: the window's own instant, else `at`); `EndpointWindow` and `AccountWindow` expose
  `window_at`. `merge_observation` compares a candidate window ONLY with the instant of the retained window of
  that kind: strictly newer replaces, equal takes the 8d rank (`cmp_window_rank`), older is kept; each kept window
  keeps its instant; the aggregate `at` is derived (the latest of them) and used for display only, never for a
  replacement decision. `account_window_at` folds the raw cache through that merge; `endpoint_window_at` gives
  each recorded entry its own instant on both windows and the live candidate its `window_at`; `select_window`
  compares per window against `EndpointWindow.window_at`. An invalid window has no instant and never replaces
  (8f kept).
- Your RC1: `partial_windows_keep_their_own_instants_in_the_recorded_selection` and
  `..._in_merge_observation` (all six orders of A/B/C), `..._across_the_recorded_live_split` (A,B recorded with C
  live, and C recorded with A,B live, both orders): every result keeps primary 99 % at t2, secondary 10 % at t3,
  the block active. `distinct_instant_partial_windows_are_order_independent`: 720 orders with distinct instants,
  partial windows and a NaN-only record. The 8b-8f tests kept.
- The doc states the plugin's T17 fix must mirror all of it: per-window rank, one instant, finite percentages,
  the raw account cache and per-window instants.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| quota tests | `cargo test -p c3-core quota -j 2` | f4dd195 | 0 | 40 passed | completed |
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | f4dd195 | 0 | 827 passed; two timing failures under a CPU load (`index::embed` loopback, `compat_wave2b` stall cut), both unrelated to this wave; clean | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff cc03617..f4dd195` is one source file and two docs - read the
hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** With per-window instants: a sequence of observations (recorded, live, account) where a blocking window
  is still hidden, or where the pick depends on the order?
- **Q2.** Verdict: ACCEPT (tag v0.2.1), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
