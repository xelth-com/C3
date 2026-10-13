# Wave 7b - no kill without a record (handoff 38, F38-1, HOLD on 0.2.0)

Branch `wave-7b-fail-closed-kill` from main 24585b6 (reviewed code 9f1ebf4). Worker: Opus.

## Done
- Disk C had 3.8 GB free: deleted C3's own `target/debug/incremental` (139 dirs, 4.2 GB; the
  operator's standing permission for build artifacts). No stale 9eck deps older than 2026-10-02 left.
- Third location: `liveness::pending::{kill_copy_home, kill_copy_dir, kill_copy_path,
  kill_copy_paths}` - `<codex home>/c3/pending-kill/<task>-<run id>.json` (codex home =
  `providers::get_codex_home`, run id = the consultation id). `pending_paths` (via
  `pending_paths_with(task_dir, home)`) lists the copies whose `task` and `collab_dir` match the
  task directory (directories compared canonically, else lexically), plus an unreadable
  `<task>-<uuid>.json` (fail-closed). Every consumer (consult recovery, panel recovery, findings
  refusal/-List, kick) sees them; the recovery consumes (removes) them as the fallback.
- `KillSite`: field `copy` (computed at `new`; `None` under `cfg(test)` - unit tests never write
  into the machine's codex home), `before_kill` tries pending -> fallback -> copy
  (`write_kill_copy`, test hook `CODEX_CONSULT_TEST_KILL_THIRD_FAIL=1`), `Lost` carries the three
  errors; `PreKill.copy`; `drop_copy` after the end-of-run write succeeds; `kill_record_warning`
  names the copy (not lost).
- `subprocess::kill_tree_checked`: `KillRecord::Lost` -> NO KILL (`left_alive`: survivors = root +
  live descendants, unverified = unreadable ones, `KillCheck::not_killed`, why `the kill record could
  not be written anywhere (...): the reviewer tree (pid N) is left alive as the evidence; the task
  is refused until it is gone or recorded`). Test hook `CODEX_CONSULT_TEST_KILL_SPARE=<pid file>`
  (a helper the kill cannot stop: no taskkill /T, not by pid).
- `c3_core::engine::KillCheck::not_killed` (new field). Outcome texts: `main_kill_outcome`,
  `format_kill_text` (`(not killed: <why>)`), `kill_check_warning` (`not killed (<turn>): ...`); the
  end-of-run refusal names the tree left running.
- Tests: kill_record_durable (a) `rc1_f38_a_helper_below_a_dead_intermediate_is_refused_from_the_codex_home_copy`,
  (b) `f38_a_kill_record_written_nowhere_leaves_the_tree_alive_and_the_task_refused` (replaces
  `a_kill_record_that_cannot_be_written_ends_the_run_refused`), (c)
  `f38_the_codex_home_copy_is_removed_once_the_record_is_written_clean`; RC2's launch-record-alone
  variant became `rc2_a_kill_record_lost_with_its_fallback_is_kept_in_the_codex_home_and_refuses`.
  Unit: `pending::tests::a_kill_copy_in_the_codex_home_is_read_as_a_record_of_its_task_only`,
  `kill_record_tests::a_kill_whose_collab_writes_fail_keeps_its_record_in_the_codex_home`,
  `kill_record_tests::a_kill_that_did_not_happen_is_told_as_such`, subprocess
  `a_kill_whose_record_is_lost_does_not_kill_and_is_not_confirmed` (renamed, inverted). The fake
  codex gained `FAKE_TURN_FILE` (turn marker) and `FAKE_HELPER_PID_FILE` (the helper).
- Targeted runs: `cargo test -p c3-cli --test kill_record_durable` 11/11 (rc1 showed the release:
  the copy consumed and removed, the turn taken); c3 lib kill/pending/subprocess filters green.
- Docs: `docs/port/wave7-acceptance-fixes.md` "7b: no kill without a record" (+ the F30-2 bullet
  marked superseded), README "Known limits" line, CHANGELOG 0.2.0 Fixed addendum.
- clippy `--workspace --all-targets -j 2 -D warnings` (log `%TEMP%/c3-wave7b/clippy.log`; one
  `doc list item without indentation` fixed) and `cargo fmt --check`: clean.
- Full suite `cargo test -j 2 --no-fail-fast --workspace` (log `%TEMP%/c3-wave7b/test.log`):
  **789 passed, 0 failed** (baseline 784 + 5: c3 lib 415 -> 418, kill_record_durable 9 -> 11; the
  flaky embed loopback test passed first time). Counts: c3 lib 418, exit_codes 1, http_engine 21,
  index_embeddings 5, index_federation 6, index_integration 4, pending_liveness 3,
  recovery_wave3a 1, router 12, scrub_markers 1, telemetry 39, telemetry_peers 1, claude_engine 15,
  compat_wave2b 17, compat_wave5 5, kill_record_durable 11, mcp_integration 1, notspooled_parity
  21, panel_light_required 1, telemetry_parity 7, wave6_live_diffs 5, c3-core lib 151, contracts
  8, formats 26, store 9.
- Harnesses through the shim (binary copy `%TEMP%/c3-wave7b/c3.exe` of target/debug/c3.exe 21:44,
  the wave-7 pinned v0.6.1 tree `%TEMP%/c3-wave7/pin`, driver `%TEMP%/c3-wave7b/drive.ps1`,
  HARNESS.lock per harness, Windows PowerShell 5.1): **harness-pending 26/0** (127 s),
  **harness-fixes 56/0** (254 s); logs `%TEMP%/c3-wave7b/logs/`.

## Decided
- Implemented the coordinator's decision as specified (three locations, fail closed). Kept
  `KillRecord::Durable { fallback: bool }` (true = outside the recovery record: fallback OR copy).
- The registration-failure stop (`kill_tree_by_pid`, rows (e)) is left unchanged (not a
  `KillRecord` site; the child just spawned is stopped by `taskkill /T`); stated in the doc.
- The no-kill run whose end-of-run write succeeds commits (outcome failed: timeout/stall, the
  record names the live tree); one whose end-of-run write fails is refused uncommitted (as before).
- RC1's helper survival is forced with `CODEX_CONSULT_TEST_KILL_SPARE` (enumeration intact, so the
  record names the tree); with the enumeration denied (RC2 style) no record could name a helper
  below a dead intermediate - that is the documented external-death / unknown-tree limit.

## Next
- Nothing left in this wave's brief. For the coordinator: merge `wave-7b-fail-closed-kill`, then the
  acceptance re-review (round 3) of F38-1 against RC1 (`kill_record_durable::rc1_f38_*`).
- Not done (out of scope, noted): the kick acknowledgement still says `stopped` when a kicked
  turn's tree was left alive (all three writes failed); the outcome says `not killed`.
