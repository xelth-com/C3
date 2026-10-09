Write in English.

# Handoff 18 - claude: wave 3b of the compatibility track (the not-spooled count, the fold, the marker) - diff review

Date: 2026-10-09. Base commit: `b079a1d` (branch `main`; code at d71ae01). The range under review is
`fda5af0..da7bfdd` - wave 3b on its branch, since merged as d71ae01; skip `.collab/`. Plan: `state.md` gap row G,
decisions P7 (own telemetry identity) and P8 (below).

## Question

Wave 3b ports the plugin's 0.6.0 wave 28e telemetry decisions E2, E3, E20, E24, E26 onto C3's own telemetry root,
on top of wave 2's outbox and deletion transaction. Faithful, and is the "never lose a count" invariant intact?
ACCEPT, HOLD (blockers by id) or ADVISE. Decision P8 to judge too: a TEST-MODE-ONLY switch
`C3_TEST_TELEMETRY_PLUGIN_HOME` (set by the shims) points only the not-spooled files, `.last` and the forgetting
marker at the plugin home, so the pinned harness-fixes28e NOTSPOOLED/MARKER checks run the plugin's library and
C3 on the same files; production paths and P7 unchanged.

## Delta

- `crates/c3/src/telemetry/notspooled.rs` (new): per-producer files `telemetry-not-spooled-<pid>-<start
  ticks>.ndjson`, the count, the fold, the legacy staging `telemetry-not-spooled-legacy-<utc ticks>.ndjson`, the
  marker with start ticks, the `.last` notes. `spool.rs`: the recorded flush (the fold + `last-flush.json` in the
  plugin's `.last` shape), the producer and sender marker checks, the HTTP status. `complaint.rs`: a local forget
  writes the marker and removes it last; the cleanup removes every not-spooled file. `mod.rs`:
  `note_not_spooled` writes this process's own file. `cli/telemetry.rs`: `--status` (the not-spooled sum, the
  marker owner, the notes), the `--flush` warning, the `.last` field list incl. `not_spooled_folded`.
- **The fold order:** (1) stage each legacy file, open the gone producers' files exclusively (delete-sharing
  only), count - nothing deleted yet; (2) save `last-flush.json` with the note, `not_spooled_seen` and
  `not_spooled_folded[]` as `{name, bytes}` - a failed save folds nothing and `--flush` warns; (3) delete under
  the held handles (`CODEX_CONSULT_TEST_FOLD_CRASH=1` exits 87 just before); (4) rewrite the record without the
  names now gone (`=2` exits 88 just before). A file already named in `not_spooled_folded[]` is deleted without
  counting again; a legacy line appended between a crash and the restarted flush counts once; a shorter file
  under a recorded name is folded afresh; a bare name is not counted; a legacy file a writer holds is skipped
  with a note. The marker: this pid + `start_ticks`; wrong ticks = gone (removed by a producer, `--status` says
  so); right ticks refuse; an unreadable start counts as alive; an older marker without ticks as before.
- Differences: the record is C3's `last-flush.json` in the plugin's shape (`result` in C3's wording, `rejected`
  always `[]`, `dropped` also counts discarded events); C3's old `not-spooled.ndjson` is a second legacy name; a
  sender finding the lock busy writes nothing (no "sender stuck" note); texts name `c3 telemetry --forget
  --local`; the marker sits on top of the `forget-pending.json` transaction.
- `crates/c3-cli/tests/notspooled_parity.rs` (11 tests, Windows); `docs/port/wave3b-notspooled.md`.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test --workspace -j 2` | da7bfdd | 0 | 631 (614 + 17); clean | completed |
| harness-fixes28e (shim, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only harness-fixes28e` | da7bfdd | 1 | 31/34 -> 46/19: NOTSPOOLED and MARKER all green; the 19 = RECORD (wave 3a, merged separately - the merged main is being verified now, expected ~63/2) | completed |
| harness-fixes28d | the same | da7bfdd | 1 | 23/4 -> 25/2 (MARKER "deletion fails halfway" and LOCK D3 "sender stuck" read the plugin's spool/lock owner = by design; stops at line 365 on `Add-KillCheck` pulled from the real script) | completed |
| harness-telemetry | the same | da7bfdd | 1 | 47/96 - the same set as before (83 by design P7, FORGET D3 shim + one wording "- counted" vs "- dropped" still open in orchestrate.rs, DRYRUN shim, COMPLAIN x10 older gap, DOCS) | completed |

## Open findings

None from this wave yet. Still open elsewhere: the "- dropped" wording, the sender's 429/400/413 handling (older
gap), `--brief-prefix`, REFUSE D3 wording - planned after wave 4 (the claude engine, running).

## Questions

- **Q1.** The fold: a schedule across two producers, a sender and a forget where a count is lost or doubled, or
  a file deleted that was not saved first?
- **Q2.** P8: is the test-mode-only path switch an acceptable oracle adapter, or does it hide a production path
  difference you would want tested?
- **Q3.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 600 words.
