# Wave 7 - acceptance round 1 fixes (handoff 36, HOLD on 0.2.0)

Branch `wave-7-acceptance-fixes` from main 086c03d (reviewed code = 09b9d25). Worker: Opus.

## Done
- Disk C had 1.5 GB free: deleted the C3 target's stale incremental dirs (before 2026-10-09 15:31)
  and the superseded workspace test artifacts in target/debug/deps (2026-10-09 15:00-16:59, 10.4 GB;
  all relinked anyway by this wave's c3-core change). Dependencies' artifacts untouched.
- F36-1 (commit 1): `c3_core::paths::repo_relative` strips the root char by char (`strip_prefix`,
  `fold`); tests `paths::tests::a_unicode_candidate_outside_the_root_is_not_sliced` (portable),
  `paths::tests::a_unicode_config_dir_outside_a_drive_root_is_not_sliced` (Windows, RC1 shape),
  `engines::claude_auth::tests::rc1_a_unicode_config_dir_on_another_drive_path_does_not_panic`
  (Windows, RC1 verbatim: root `C:\repo`, `CLAUDE_CONFIG_DIR=C:\日本\.claude`, nothing reported
  -> `launch_problem_of` returns ""), `..::a_unicode_config_dir_outside_the_repository_does_not_panic`
  (portable).
- F36-2 (commit 1): `unicode-normalization` 0.1 (already in the lock via stringprep) added to the
  workspace and c3; `projects_directory` derives the NFC form (as Claude Code), and both checks of
  `launch_problem_of` use `path_inside_any_form` (as written and NFC, root and candidate, any pair
  inside refuses). Test `engines::claude_auth::tests::a_decomposed_config_dir_whose_nfc_form_is_in_the_repository_is_refused`
  (lexically outside per `repo_relative`, refused by the new guard; home NFD; reverse NFD repo;
  reported NFD; outside allowed). `cargo test -p c3 --lib claude_auth`: 8/8; `-p c3-core paths`: 2/2.

- F30-2 failure path (commit 2): `KillRecord` answer of the kill hook to
  `kill_tree_checked` (Lost -> the kill still runs, not confirmed, why "the kill's record could not
  be written before the kill (...)"); `KillSite::before_kill` retries a failed write ONCE into
  `liveness::pending::kill_fallback_path` = `.consult.pending-kill.json` (member
  `.consult.pending-<NN>-kill.json`, read by `pending_paths` and the plugin's Get-PendingPaths as a
  member-named record; consumed and removed by the recovery when released); `kill_record_warning`
  names the fallback (not lost); test hook `CODEX_CONSULT_TEST_KILL_FALLBACK_FAIL=1`.
  Tests added: orchestrate `a_kill_whose_record_cannot_be_written_keeps_it_in_its_fallback`,
  subprocess `a_kill_whose_record_is_lost_still_kills_and_is_not_confirmed`, pending
  `a_kills_fallback_file_is_read_as_a_record`, kill_record_durable
  `a_kill_record_that_cannot_be_written_is_kept_in_its_fallback`, `rc2_a_lost_kill_record_leaves_its_fallback_and_the_next_run_is_refused`,
  `rc2_a_kill_record_lost_with_its_fallback_is_refused_from_the_launch_record_alone`; the read-only
  test now blocks the fallback (hook). RC3 integration test
  `claude_engine::a_decomposed_config_dir_inside_an_nfc_repository_refuses_the_endpoint_run`.
- Docs written: wave2b-compat.md "Limits" (F29-1), README "Known limits", wave4-claude.md "Wave 7",
  wave3a-recovery.md F30-2 wave 7 bullet + difference, CHANGELOG 0.2.0 Fixed entry,
  docs/port/wave7-acceptance-fixes.md.
- Disk C ran out again (0.88 GB, a build failed with os error 1455 - paging file): deleted
  9eck.com/target/debug/deps files older than 2026-10-02 (21.4 GB, the stale 09-27..10-01 sets; the
  10-06 set kept; no 9eck process running) under the operator's standing permission for build
  artifacts. Peer cargo runs at the time: xelixir `server` / `xelixir_agent` only - none of mine.

- Targeted runs green: `cargo test -p c3 --lib -- kill_record_tests ...pending::tests::a_kills
  ...subprocess::tests::a_kill_whose engines::claude_auth` 20/20 (after fixing two expectations of
  the new orchestrate test: a dead root gets no survivor entry; the warning names the prior's child);
  `cargo test -p c3-cli --test kill_record_durable --test claude_engine`: 9/9 and 15/15; the RC2
  fallback test showed the release (no "release not shown").
- RC2 answer: against the PRE-CHANGE binary (`%TEMP%\c3-rc6\c3.exe`, code 09b9d25) via
  `C3_TEST_EXE`: `rc2_a_kill_record_lost_with_its_fallback_is_refused_from_the_launch_record_alone`
  PASSES (the recovery already refused from the launch record alone - by-parent scan under the
  recorded child finds the orphaned fake PowerShell), `rc2_a_lost_kill_record_leaves_its_fallback...`
  FAILS (no fallback file within 90 s - the new durable evidence). No processes leaked.

## Decided
- F36-1: `c3_core::paths::repo_relative` strips the root prefix char by char (never byte slicing);
  the Windows comparison folds case as .NET `OrdinalIgnoreCase` does (simple uppercase per char).
  The crate-wide grep for `[..x.len()]` / `split_at(` on strings found no other prefix-of-another-
  string slice (detach.rs, health.rs time-zone words, store.rs digits, roles.rs `role-` all slice a
  string at the length of its OWN prefix or of a `find` result).

## Next
- RC3 integration test (claude_engine, real binary, scratch repo `café`), F30-2 failure path (RC2), F29-1 / F32-2 docs, CHANGELOG,
  wave7 doc, full suite + clippy + fmt, harness-claude / harness-pending / harness-fixes via the shim.
