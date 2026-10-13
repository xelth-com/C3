# Wave 7c - copies that cannot be listed, a kick that killed nothing (handoff 40, F40-1, F40-2)

Branch `wave-7c-copy-enumeration` from main a5d5263 (code 098c29e). Worker: Opus.

## Done
- F40-1: `liveness::pending::kill_copy_paths` -> `Result<Vec<PathBuf>, String>`: only `NotFound`
  of `<codex home>/c3/pending-kill` is "no copies"; a failure to open it or to read an entry is the
  inspection failure `the recovery copies under <dir> cannot be listed (<error>): the task is
  refused until they can be read` (`copy_listing_refusal`). `pending_paths` / `pending_paths_with`
  -> `PendingPaths { paths, error }`. Consumers: `consult::recovery::assess` (error before any record
  is judged: real run, dry run, both panel passes), `read_task_pending_records` (findings status
  change and rating), `findings -List` (`pending: <failure>` line), `-Kick` (exit 1). Test hook
  `CODEX_CONSULT_TEST_COPY_DIR_FAIL=denied|iter` (`CopyDirFail`; `kill_copy_paths_as` for units).
- F40-2: `subprocess::acknowledge_taken_kick` - a kill with `KillCheck::not_killed` acknowledges
  `result=not killed` (+ `root=`, `reason=` lines; `KICK_NOT_KILLED`, `KickAck.root/.reason`,
  `write_kick_ack(.., left)`), else `stopped`. `consult::kick`: `not killed` -> stderr `-Kick: member
  <NN> of task <t> took the kick but was NOT stopped - not killed (root pid <pid>): <why>. Its
  engine's process tree (root pid <pid>) is left alive and the task is refused ...`, exit 1 (the
  ack still retired by its creator).
- Tests: RC1 = `kill_record_durable::rc1_f38_a_helper_below_a_dead_intermediate_is_refused_from_the_codex_home_copy`
  (extended: denied and iter -> run and dry run exit 1 with the failure, no turn marker, helper
  alive, copy + launch record kept; findings -List shows it; kick refuses; then the existing
  restore -> refusal naming the helper -> helper gone -> turn runs). RC2 =
  `kill_record_durable::rc2_f40_a_kick_whose_kill_record_is_written_nowhere_is_acknowledged_not_killed`.
  Units: `pending::tests::a_kill_copy_directory_that_cannot_be_listed_is_an_inspection_failure`,
  `subprocess::tests::a_kick_whose_kill_did_not_happen_is_acknowledged_not_killed`,
  `kick::tests::a_not_killed_acknowledgement_is_reported_as_no_stop`. Test helper `c3_cmd` /
  `c3_other` (launch_io refactored onto it).
- Targeted: c3 lib (pending, kick, subprocess, recovery) 32/32; kill_record_durable rc1 + rc2 2/2.
- clippy `--workspace --all-targets -j 2 -D warnings` and `cargo fmt --check`: clean.
- Full suite `cargo test -j 2 --no-fail-fast --workspace` (log `%TEMP%/c3-wave7c/test.log`): 793
  tests, **792 passed**, 1 failed = the known flaky
  `index::embed::tests::embed_reaches_a_loopback_fake_via_localhost`, green on its rerun
  (`index::embed` 6/6, `%TEMP%/c3-wave7c/embed-rerun.log`). Baseline 789 + 4: c3 lib 418 -> 421,
  kill_record_durable 11 -> 12; every other target unchanged.
- RC1 rerun with `--nocapture` (`%TEMP%/c3-wave7c/rc1-nocapture.log`): no "release not shown" -
  the release after the helper's stop was shown (copy consumed, turn taken).
- Harnesses through the shim (binary copy `%TEMP%/c3-wave7c/c3.exe` of target/debug/c3.exe 22:28,
  the wave-7 pinned v0.6.1 tree `%TEMP%/c3-wave7/pin`, driver `%TEMP%/c3-wave7c/drive.ps1`,
  HARNESS.lock per harness, Windows PowerShell 5.1): **harness-pending 26/0** (100 s),
  **harness-fixes 56/0** (231 s); logs `%TEMP%/c3-wave7c/logs/`.
- Docs: `docs/port/wave7-acceptance-fixes.md` "7c" (with its verification), CHANGELOG 0.2.0 line.
- Disk: deleted C3 `target/debug/incremental` (2.1 GB; standing permission).

## Decided
- The kick refuses on an inspection failure like every other consumer (the brief): a hanging
  member then ends at its own timeout, whose kill does not depend on listing the copies.
- Exit 1 for a `not killed` kick: the kick's existing failure code (plugin README: `1` no such
  member / not running; `3` stays "no acknowledgement", `4` misuse).
- The injected `iter` error comes before the first entry (deterministic); `denied` replaces the
  open whether the directory exists or not.
- The task directory's own listing keeps the plugin's `Get-PendingPaths` rule (unchanged, out of
  F40-1's scope).

## Next
- Supervisor: merge `wave-7c-copy-enumeration`, then the round-4 acceptance brief (RC1/RC2 of
  reply 40 are the two tests named above).
