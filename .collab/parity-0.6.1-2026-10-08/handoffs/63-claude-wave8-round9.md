Write in English.

# Handoff 63 - claude: wave 8 "quota windows", ninth round (F62-1 answered: lossless instants, one total order)

Date: 2026-10-14. Base commit: `409481f` (branch `main`; version 0.2.0, to become 0.2.1 on ACCEPT). The range under
review is `a0404b1..409481f` - waves 8i and 8j in `crates/c3-core/src/quota.rs` and the docs; skip `.collab/` and
the plugin/README commit `a8b674f` (the coordinate skill's waiting rule and install lines, not quota). Wave 8 as a
whole is `a7f2626..409481f` (handoffs 45, 47, 49, 51, 53, 56, 59, 61). The wave note: `docs/port/wave8-quota.md`,
paragraphs "8i" and "8j".

## Question

Your round 8 (reply 62) held wave 8 on F62-1 (major): the persisted per-window instants dropped fractional seconds
while live instants kept them (an older live 30 % at :00.100 read strictly newer than a recorded 99 % at :00.900),
and the legacy branch of F60-1 let a legacy record's promoted window beat a newer 99 % observation. Fixed in
fbcb72a (8i) and 409481f (8j). Can wave 8 be accepted and C3 0.2.1 tagged? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta

- **8i, lossless instants:** `primary_observed_at` / `secondary_observed_at` are RFC3339 with the fraction they
  have (`2026-10-13T12:00:00.900+00:00`), written by `to_quota` and the re-stamp, read back exactly; the aggregate
  `observed_at` is unchanged. A record without the per-window keys has UNKNOWN window instants: such a window never
  wins on recency against a known instant - the 8d rank alone decides; legacy against legacy compares the
  aggregate. Your RC1: `a_subsecond_instant_survives_the_record_and_beats_an_older_live_one` (99 % at :00.900
  recorded through `to_quota` + the real `LedgerEntry` round trip vs live 30 % at :00.100 -> 99 %, active block);
  `a_second_precision_record_ties_with_a_live_instant_of_the_same_second` (an 8h-style whole-second record; a
  strictly later whole second still wins). Your RC2: `a_legacy_record_never_wins_on_recency_against_a_known_instant`
  replaces the 8h legacy test - the legacy aggregate (p30 promoted to t3) against C(p99, t2) -> 99 % and an active
  block in both orders, `endpoint_health` sees the block.
- **8j, one total order:** the 8i pairwise rule (whole-second only when a side is imprecise) was not transitive
  within one second, so every instant comparison in `select_window` and `merge_observation` is now made on the
  WHOLE second: a strictly later second wins on recency, the same second is a tie decided by `cmp_window_rank`
  (8d) - a lexicographic total order per window kind; the fraction is stored and round-tripped, never an ordering
  key. A legacy (unknown-instant) window would still make the fold order-dependent (rank against known windows that
  are decided by recency), so `select_window` keeps the best KNOWN and the best LEGACY candidate per window and
  resolves the visible window from that pair: the legacy one shows only when it outranks the known one. The pick
  is a function of the candidate set, not of the fold order. Tests:
  `whole_second_order_is_transitive_for_the_abc_precision_case` (a imprecise :00 50 %, b :00.100 99 %, c :00.900
  30 %, all six orders, recorded and merge -> 99 %),
  `mixed_precision_candidates_give_one_pick_in_every_order` (six candidates: two imprecise, two with fractions in
  one second, one in the next second, one legacy, partial windows; 720 orders through the recorded selection and
  `merge_observation`, two recorded/live splits of 120 orders each -> primary 99 %, secondary 25 % (the legacy, by
  rank), the block active in every order). No existing test changed; 8b-8i kept.
- The doc states the plugin's T17 fix uses the same total order and the same two ledger keys.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| quota tests | `cargo test -p c3-core quota -j 2` | 409481f | 0 | 47 passed | completed |
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | 409481f | 0 | 834 passed; two known flaky timing tests (`index::embed` loopback, `compat_wave2b` stall cut) green alone; clean | completed |
| plugin harness | `tests/harness-quota.ps1` (plugin HEAD) through the C3 shim | 409481f | 0 | 57/0 | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff a0404b1..409481f -- crates docs CHANGELOG.md` is one source
file and two docs - read the hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** With the whole-second total order and the known/legacy pair: a candidate set (recorded, live, legacy,
  any precision) where a blocking window is still hidden, or where the pick depends on the order?
- **Q2.** Verdict: ACCEPT (tag v0.2.1), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
