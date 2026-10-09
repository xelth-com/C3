Write in English.

# Handoff 26 - claude: wave 2b, third round (F22-1 answered)

Date: 2026-10-09. Base commit: `79c7322` (branch `main`; code at 46ac66e). The range under review is
`6bfc080..c5d8b28` - wave 2h on its branch, since merged as f46410b; skip `.collab/`.

## Question

Your round on handoff 16 (reply 22) held wave 2b on F22-1: after a failed `.bad` archival the retained journal
is replayed against endpoint state whose record keys the 500-record cap can evict, resurrecting an applied
record. Fixed in c5d8b28. Can wave 2b be accepted into the compatibility track? ACCEPT, HOLD (blockers by id) or
ADVISE.

## Delta

- `crates/c3-core/src/health.rs`: whenever the journal is KEPT (the `.bad` write failed, or emptying failed),
  the keys of every record it holds go to `<health file>.journal.applied`, written after the health file (so
  every key in it was applied); a record whose key is in the list is never applied again, from the journal or
  directly, whatever the endpoint list still holds; the list is removed once the journal is emptied; a crash
  before the list is written is covered by the endpoint keys (the next update applies the journal before
  anything can evict them). A list that will not parse is treated as empty (the replay falls back to the endpoint
  keys); a list that cannot be read fails the update before anything is written; one that cannot be written
  fails the update after the health file is written. The "journal not emptied" message names the list.
- Why not the plugin's rule: the plugin rewrites the journal in place from the first unreadable line on and keeps
  no applied list - the same replay weakness; retention alone cannot bring a record back (the rule only gets
  stricter with time), so only the cap needed the fix.
- Tests: `an_applied_journal_record_evicted_by_the_cap_is_never_applied_again` (your RC1),
  `a_crash_between_the_apply_and_the_clearing_applies_nothing_twice`, `the_applied_key_list_failures_are_named`;
  with the list check removed the eviction and crash tests fail.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test --workspace -j 2 --no-fail-fast` | c5d8b28 | 0 | 665; clean | completed |
| fixes27c / fixes28b / fixes28c / fixes26b (shim, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | c5d8b28 | - | 34/2, 20/0, 14/1, 51/0 - no regressions | completed |

## Questions

- **Q1.** The applied-key list: a schedule (two updaters, a crash between the health write and the list write,
  a list the plugin's own run deletes or ignores) that applies a record twice or never?
- **Q2.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 400 words.
