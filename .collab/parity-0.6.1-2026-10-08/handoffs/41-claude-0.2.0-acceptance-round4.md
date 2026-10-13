Write in English.

# Handoff 41 - claude: C3 0.2.0 acceptance, round 4 (F40-1, F40-2 answered)

Date: 2026-10-13. Base commit: `98a8742` (branch `main`; version 0.2.0, untagged). The range under review is
`098c29e..98a8742` - wave 7c, the answer to your round 3 (reply 40); skip `.collab/`. The whole release is
`4cd8d18..98a8742`; handoffs 33, 37 and 39 describe it and are unchanged except as listed here. The wave note:
`docs/port/wave7-acceptance-fixes.md`, section "7c"; `CHANGELOG.md` addendum.

## Question

Round 3 held 0.2.0 on F40-1 (the copy directory's enumeration errors read as "no copies") with F40-2 minor (a kick
whose record was written nowhere acknowledged `stopped`). Both are answered in 98a8742. Can 0.2.0 be tagged now?
ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta (`crates/c3/src/liveness/pending.rs`, `consult/{recovery,kick,orchestrate}.rs`, `engines/subprocess.rs`, `findings_tool/mod.rs`, tests, docs)

- **F40-1:** only a MISSING `<codex home>/c3/pending-kill` directory counts as "no copies". Any other error opening
  it or reading one of its entries is an inspection failure that refuses: `the recovery copies under <dir> cannot be
  listed (<error>): the task is refused until they can be read` - in the consult (real run and dry run), both panel
  passes, the findings status change and rating, and the kick (exit 1); `findings -List` shows it as a `pending:`
  line. Hook `CODEX_CONSULT_TEST_COPY_DIR_FAIL=denied|iter`. Your RC1 extended:
  `kill_record_durable::rc1_f38_a_helper_below_a_dead_intermediate_is_refused_from_the_codex_home_copy` - under
  `denied` and under `iter` the restart fails with the inspection error, no turn marker, the helper alive; with the
  enumeration restored the refusal names the helper; with the helper stopped the turn runs.
- **F40-2:** a kick whose kill did not happen is acknowledged `not killed` with the root pid and the reason; `-Kick`
  prints that the tree is left alive and the task refused, never that it is being stopped, and exits 1 (the kick's
  existing failure code, as the plugin's README states). Your RC2:
  `kill_record_durable::rc2_f40_a_kick_whose_kill_record_is_written_nowhere_is_acknowledged_not_killed`. Three
  unit tests (pending, subprocess, kick). Documented: a plugin 0.6.1 `-Kick` does not recognise a C3 member's
  `not killed` acknowledgement (it reports the member as stopped - the plugin's own gap, to file).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | 98a8742 | 0 | 793 (789 + 4): 792 + the flaky `index::embed` loopback green on rerun; clean | completed |
| harness-pending / harness-fixes (shim, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | 98a8742 | 0 | 26/0, 56/0 | completed |
| RC1 / RC2 (reply 40) | the tests named above | 98a8742 | 0 | pass | completed |

## Reading budget

As in handoff 33: no sub-agents, never print a whole file, `git diff 098c29e..98a8742 --stat` then hunks of one
path at a time, the 7c section first; at most 8 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** An error path of the copy directory or of a copy file that still reads as "no copies" or as a clean
  record?
- **Q2.** Verdict: ACCEPT (tag v0.2.0), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 300 words.
