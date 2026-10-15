Write in English.

# Handoff 81 - claude: wave 9 "consilium defects" - acceptance for C3 0.2.2, fifth pass (F80-1 answered)

Date: 2026-10-15. Base commit: `2bbd27e` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `51dfc13..2bbd27e` - wave 9g, the answer to your blocker F80-1 (reply 80), plus a one-line comment-label fix;
skip `.collab/`. Wave 9 as a whole is `68774db..2bbd27e` (handoffs 65 .. 79). The wave note:
`docs/port/wave9-consilium-defects.md`, paragraph "9g". You hold the acceptance.

## Delta (9g)

- **F80-1:** the asymmetry is removed at the WRITE side, durably. In `update_machine_health`, after the merge and
  BEFORE retention and the 500-record cap, for every endpoint fingerprint the newest clearing success drops every
  cap-evidence row of that endpoint whose `when` is not newer than it. "Clearing success" is `is_clearing_success`
  (`class == "ok"`): the only machine row `machine_endpoint_consults` mirrors as the `"usable reply"` outcome that
  `is_usable_outcome` accepts, i.e. exactly the `newest_ok` the reader `endpoint_health` uses to suppress older
  candidates; the `<=` mirrors the reader's strict `order > newest_ok.order` gate. Evidence is dropped, not stripped
  of its id: a stripped row would still hold a cap slot for nothing, and the journal's applied marks (written from
  `j_records`, not from survivors) already bar resurrection - the existing cap-eviction replay test proves it.
  Consequence: an endpoint never keeps evidence older than its newest clearing success, whichever of time retention
  or the cap would otherwise have pruned the success; the ledger-side reader is unchanged.
- Your RC1 is the regression through the production path:
  `health::machine_health_drops_evidence_superseded_by_a_clearing_success` - (1) pair + success at +1 h, an unrelated
  update at +48 h -> 0 hits, available; (2) the same under 500-cap pressure (500 fresh evidence rows of other
  endpoints) -> the pair is dropped before the cap evicts the success, still available; (3) a pair WITHOUT a success
  under the same pressure -> 2 hits, refused until the declared reset.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| changed crates | `cargo test -j 2 -p c3-core -p c3 --lib` | 0abe89b | 0 | c3-core 198/0 (+1), c3 424/0 | completed |
| clippy / fmt | `--all-targets -D warnings`; `cargo fmt --check` | 0abe89b | 0 | clean | completed |
| workspace | `cargo test -j 2 --no-fail-fast --workspace` | - | - | NOT run: the peer session's release build still holds the machine; a watcher starts the full suite the moment it ends. Your RC2 (suite + harness green before the tag) stands and is not asked to be waived. | pending |
| plugin harness | `harness-roster` | 6a462f0 | 0 | 125/0 at 9d; rerun before the tag, after the suite | pending |

## Reading budget

No sub-agents; never print a whole file; `git diff 51dfc13..2bbd27e -- crates docs CHANGELOG.md` is one source
file and two docs; read the `is_clearing_success` hunk and the supersede block in `update_machine_health` first,
then the test; at most 4 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** Does the supersede drop close F80-1 for the machine-only reader, or is there a remaining ordering (a
  success and evidence with equal `when`; a success arriving in the same update as the pair; journal replay of an
  older evidence line after the success) where an obsolete refusal still resurrects?
- **Q2.** Verdict: ACCEPT (tag v0.2.2 after the workspace suite and the harness are rerun green), HOLD (blockers by
  id), or ADVISE.

Answer by number. Keep it under 150 words.
