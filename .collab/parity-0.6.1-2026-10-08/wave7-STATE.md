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
  (Windows, RC1 verbatim: root `C:epo`, `CLAUDE_CONFIG_DIR=C:\日本\.claude`, nothing reported
  -> `launch_problem_of` returns ""), `..::a_unicode_config_dir_outside_the_repository_does_not_panic`
  (portable).
- F36-2 (commit 1): `unicode-normalization` 0.1 (already in the lock via stringprep) added to the
  workspace and c3; `projects_directory` derives the NFC form (as Claude Code), and both checks of
  `launch_problem_of` use `path_inside_any_form` (as written and NFC, root and candidate, any pair
  inside refuses). Test `engines::claude_auth::tests::a_decomposed_config_dir_whose_nfc_form_is_in_the_repository_is_refused`
  (lexically outside per `repo_relative`, refused by the new guard; home NFD; reverse NFD repo;
  reported NFD; outside allowed). `cargo test -p c3 --lib claude_auth`: 8/8; `-p c3-core paths`: 2/2.

## Decided
- F36-1: `c3_core::paths::repo_relative` strips the root prefix char by char (never byte slicing);
  the Windows comparison folds case as .NET `OrdinalIgnoreCase` does (simple uppercase per char).
  The crate-wide grep for `[..x.len()]` / `split_at(` on strings found no other prefix-of-another-
  string slice (detach.rs, health.rs time-zone words, store.rs digits, roles.rs `role-` all slice a
  string at the length of its OWN prefix or of a `find` result).

## Next
- RC3 integration test (claude_engine, real binary, scratch repo `café`), F30-2 failure path (RC2), F29-1 / F32-2 docs, CHANGELOG,
  wave7 doc, full suite + clippy + fmt, harness-claude / harness-pending / harness-fixes via the shim.
