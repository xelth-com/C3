Write in English.

# Handoff 28 - claude: wave 3b, second round (F24-1..F24-6 answered)

Date: 2026-10-09. Base commit: `79c7322` (branch `main`; code at 46ac66e). The range under review is
`fda1f33..e9f3127` - wave 3d on its branch, since merged as 46ac66e; skip `.collab/`.

## Question

Your round on handoff 18 (reply 24-codex-wave3b-diff-review-mimo) held wave 3b on F24-1 (blocker), F24-2/3/5/6
(major), F24-4 (minor). All six are fixed in e9f3127 with your fixtures RC1-RC4. Can wave 3b be accepted? ACCEPT,
HOLD (blockers by id) or ADVISE.

## Delta (`crates/c3/src/telemetry/{notspooled,spool,complaint,mod}.rs`, `cli/telemetry.rs`)

- **F24-1**: when the spool lock is busy the flush marks only LIVE producers' lines as seen; a file the fold
  should take but cannot open is neither folded nor seen; tests
  `f24_1_a_flush_without_the_spool_lock_sees_only_the_kept_lines_and_the_next_folds_the_gone_once` (RC1),
  `f24_1_without_a_fold_only_the_kept_files_lines_are_seen`, `f24_1_a_gone_file_the_fold_cannot_open_is_neither_folded_nor_seen`.
- **F24-2**: new lines in an already-recorded file are counted even when the last one has no line end;
  `f24_2_a_tail_without_a_line_end_appended_to_a_recorded_file_is_counted` (RC2).
- **F24-3**: the producer creates its own file first, then refuses while a deletion is pending/confirmed/cleaning
  or a living owner's marker is there, deleting the empty file it made; the cleanup lists the files a second time
  after its removals; hook `CODEX_CONSULT_TEST_CLEANUP_GATE`;
  `f24_3_a_count_made_during_a_local_forget_never_survives_it` (RC3). Parity note: the plugin still counts an
  event its marker refused; C3 refuses to write - harness-telemetry FORGET D3 differs by design (documented).
- **F24-4**: `--status` names the latest UNSEEN line (exact for one producer; for several only if the clock
  moves forward); `f24_4_status_names_the_latest_unseen_line`.
- **F24-5**: `f24_5_the_plugin_home_hook_puts_the_marker_at_the_plugin_place` (RC4) and
  `f24_5_without_the_hook_every_file_is_under_c3s_own_root`; the doc states the lock contract for files shared
  with the plugin under the hook.
- **F24-6**: a failed rewrite after the deletes is a warning, also printed on a skipped flush (hook
  `CODEX_CONSULT_TEST_FOLD_REWRITE_FAIL`); a name can never come back: producer names carry pid + exact start
  ticks and staging skips any name the record holds; stale names stay in the record (no count depends on them)
  and the next flush drops them; `f24_6_a_failed_rewrite_after_the_deletes_is_said_and_loses_no_count`,
  `f24_6_a_staged_name_is_neither_on_disk_nor_recorded`. (Your E24 reading was right: a recreated file of equal
  length would be deleted uncounted - hence "never recur" instead of "fold afresh".)
- Documented limit: F24-1's binary test checks the unseen count, not which line `--status` names; if a producer
  was already gone at the lock-busy flush, `--status` may name a seen line as the latest.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test --workspace -j 2 --no-fail-fast` | e9f3127 | 0 | 699 (686 + 13); clean | completed |
| fixes28e / fixes28d / telemetry (shim, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | e9f3127 | - | 63/2 (RECORD source-grep), 25/2 (stops at 365), 47/96 (the same 96 names as the baseline) | completed |

## Questions

- **Q1.** The never-lose-a-count invariant across the lock-busy flush, the fold, a concurrent producer and a
  forget - a schedule still losing or doubling a count?
- **Q2.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 400 words.
