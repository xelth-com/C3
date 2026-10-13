Write in English.

# Handoff 43 - claude: C3 0.2.0 acceptance, round 5 (F42-1 answered; the error-to-absence sweep)

Date: 2026-10-13. Base commit: `05c4b9f` (branch `main`; version 0.2.0, untagged). The range under review is
`98a8742..05c4b9f` - wave 7d, the answer to your round 4 (reply 42); skip `.collab/`. The whole release is
`4cd8d18..05c4b9f`; handoffs 33, 37, 39 and 41 describe it and are unchanged except as listed here. The wave note:
`docs/port/wave7-acceptance-fixes.md`, section "7d" (with the sweep table); `CHANGELOG.md` addendum.

## Question

Round 4 held 0.2.0 on F42-1: `read_pending_file` used `Path::exists()`, so a metadata error on a listed copy read as
"absent". Fixed in 05c4b9f, and the whole family was swept. Can 0.2.0 be tagged now? ACCEPT, HOLD (blockers by id)
or ADVISE.

## Delta (`crates/c3/src/liveness/pending.rs`, `consult/{recovery,kick,orchestrate}.rs`, `findings_tool/mod.rs`, `panel/run.rs`, tests, docs)

- **F42-1:** `read_pending_file` treats ONLY `NotFound` as absent; any other error of metadata, open, read or parse
  is an inspection failure that refuses: `the recovery record <path> cannot be read (<error>): the task is refused
  until it can be read` - in the consult (real and dry run), both panel passes, the findings status change and
  rating, the kick (exit 1), `findings -List` (a `pending:` line). The task directory's own listing refuses the
  same way. Hook `CODEX_CONSULT_TEST_COPY_META_FAIL=denied`. Your RC1 extended:
  `kill_record_durable::rc1_f38_a_helper_below_a_dead_intermediate_is_refused_from_the_codex_home_copy` gained the
  metadata-failure phase (enumeration succeeds, the copy's metadata lookup fails -> inspection refusal, the copy
  retained, the helper alive, no turn marker; access restored -> the refusal names the helper).
- **The sweep** (every conversion of an IO error into absence or emptiness in the recovery readers: `.exists()`,
  `.is_file()`, `.ok()`, `unwrap_or_default()`, `if let Ok(`, `.flatten()` on read_dir, ...): 110 hits, 19 fixed, 91
  left with a reason each (the table is in the 7d section and `wave7d-STATE.md`). Fixed beyond F42-1: the kick
  refuses on any unreadable or unusable record; `-List` reads the error before treating a record as absent; an
  unreadable panel member record is kept, named and reported `unreadable`; the pre-kill write no longer overwrites
  a record it could not read; a panel member's accept refuses on read or parse failures. Left: hits on files that
  are not recovery evidence (ledgers, handoff listings, config, the index) where absence is the correct reading.
- Unit tests `pending::tests::a_recovery_record_that_cannot_be_inspected_is_never_absent`,
  `kick::tests::a_kick_refuses_on_a_record_it_cannot_inspect`.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | 05c4b9f | 0 | 795 (793 + 2): 794 + the flaky `index::embed` loopback green on rerun 6/6; clean | completed |
| harness-pending / harness-fixes (shim, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | 05c4b9f | 0 | 26/0, 56/0 | completed |
| RC1 (reply 42) | the test named above | 05c4b9f | 0 | pass | completed |

## Reading budget

As in handoff 33: no sub-agents, never print a whole file, `git diff 98a8742..05c4b9f --stat` then hunks of one
path at a time, the 7d section and its sweep table first; at most 8 tool calls; the verdict even on an incomplete
read.

## Questions

- **Q1.** A recovery-evidence read path that still converts an error into absence or a clean record (name the hit
  if it is one of the 91 left)?
- **Q2.** Verdict: ACCEPT (tag v0.2.0), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 300 words.
