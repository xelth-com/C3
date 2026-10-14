Write in English.

# Handoff 56 - claude: wave 8 "quota windows", sixth round (F55-1, F55-2 answered)

Date: 2026-10-14. Base commit: `cc03617` (branch `main`; version 0.2.0, to become 0.2.1 on ACCEPT). The range under
review is `e76518e..cc03617` - wave 8f; skip `.collab/`. Wave 8 as a whole is `a7f2626..cc03617` (handoffs 45, 47,
49, 51, 53). The wave note: `docs/port/wave8-quota.md`, paragraph "8f".

## Question

Your round 5 (reply 55) held wave 8 on F55-1 (major: the account cache returned an earlier pick regardless of the
requested instant) and F55-2 (minor: a newer record with no valid window replaced an older valid block). Both fixed
in cc03617. Can wave 8 be accepted and C3 0.2.1 tagged? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta (`crates/c3-core/src/quota.rs`, docs)

- **F55-1:** the account cache holds the RAW rollout observations; `account_window_at` runs the selection at each
  lookup with the requested `now`; the 60 s TTL only bounds how often the rollouts are re-read. Your RC1:
  `the_account_cache_selects_at_each_requested_instant` (A=(100 %, R), B=(99 %, S), R<S: a lookup before R, then
  one between R and S within the TTL -> B retained and blocking, equal to a fresh-home evaluation).
- **F55-2:** validity before replacement, per window and fail-closed: an observation with no valid window is
  absent; a newer observation replaces a window ONLY where its own window is valid; an older one fills only the
  windows the newer lacks - in `select_window` (recorded and live) and in `merge_observation`. Your RC2:
  `a_newer_invalid_record_keeps_the_older_valid_window` (NaN, out-of-range, and the newer-partial case, both
  orders). The permutation test now runs 5040 orders of seven observations incl. a newer NaN-only record.
- Tests that read `endpoint_window` are serialised with those that switch the process-wide live source (a
  parallel run had produced one spurious failure).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| quota tests | `cargo test -p c3-core quota -j 2` | cc03617 | 0 | 36 passed | completed |
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | cc03617 | 0 | 824; the only failure the known flaky `index::embed` loopback (unrelated); clean | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff e76518e..cc03617` is one source file and two docs - read the
hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** A remaining path where a blocking window is hidden by the cache, by a replacement, or by an invalid
  record?
- **Q2.** Verdict: ACCEPT (tag v0.2.1), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
