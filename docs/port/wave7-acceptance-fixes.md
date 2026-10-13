# Wave 7 - the 0.2.0 acceptance round 1 fixes (handoff 36, HOLD)

Branch `wave-7-acceptance-fixes`. The acceptance review of release 0.2.0 (Astra, gpt-6-astra,
`.collab/parity-0.6.1-2026-10-08/handoffs/36-codex-0.2.0-acceptance.md`, reviewed code 09b9d25)
held the tag on F36-1 (blocker) and the failure path of F30-2 / F23-3, asked for F36-2 (major), and
for F29-1 and the endpoint-preflight limits (F25-1, F32-1, F32-2) to be stated rather than claimed
closed. Each finding below: the fix, the proof, and what differs from the plugin.

## F36-1 (blocker) - a valid Unicode path panicked the Claude transcript guard

- **Fix.** `c3_core::paths::repo_relative` sliced the candidate at the byte length of the root
  prefix (`&hay[..prefix.len()]`): root `C:\repo` (8 bytes) against `C:\日本\.claude`, byte 8 is
  inside `本`. The root is now stripped char by char (`strip_prefix`, `fold`) - the rest always
  starts at a char boundary - and the Windows comparison folds every letter as the plugin's
  `OrdinalIgnoreCase` (simple uppercase per char; before: ASCII only). A grep of the crates for
  `[..x.len()]`, `[x.len()..]` and `split_at(` found no other slice of one string at another
  string's length (the rest slice at a `find` result or the string's own prefix).
- **Proof (RC1).** `c3_core::paths::tests::a_unicode_candidate_outside_the_root_is_not_sliced`
  (portable), `..::a_unicode_config_dir_outside_a_drive_root_is_not_sliced` (Windows),
  `engines::claude_auth::tests::rc1_a_unicode_config_dir_on_another_drive_path_does_not_panic`
  (Windows, RC1 verbatim: root `C:\repo`, `CLAUDE_CONFIG_DIR=C:\日本\.claude`, no reported
  projects directory - `launch_problem_of` returns `""`), `..::a_unicode_config_dir_outside_the_repository_does_not_panic`
  (portable).
- **Plugin.** None: `Get-RepoRelativePath` uses .NET `StartsWith(OrdinalIgnoreCase)`, which C3 now
  matches.

## F36-2 (major) - the guard ignored the NFC normalisation Claude Code applies

- **Fix.** Claude Code writes under `(CLAUDE_CONFIG_DIR ?? join(os.homedir(), ".claude"))
  .normalize("NFC")` + `/projects`. `projects_directory` now derives that NFC form, and both checks
  of `launch_problem_of` use `path_inside_any_form`: candidate and root each as written and in NFC,
  any pair inside refuses (fail-closed; the comparison is the strings', never the filesystem's -
  NTFS keeps the two spellings apart, APFS folds them). Crate `unicode-normalization` 0.1, already
  in the lock file (stringprep's).
- **Proof (RC3).** `engines::claude_auth::tests::a_decomposed_config_dir_whose_nfc_form_is_in_the_repository_is_refused`
  (the NFD configuration lexically outside per `repo_relative`, refused; NFD home; NFD repository
  with an NFC configuration; a reported NFD projectsDirectory; outside allowed) and
  `claude_engine::a_decomposed_config_dir_inside_an_nfc_repository_refuses_the_endpoint_run` (the
  real binary, a fake launcher: a scratch repository named NFC `café`, `CLAUDE_CONFIG_DIR` =
  `cafe` + U+0301 + `\.claude`, never created; the dry run says `a real run is refused`, the run
  exits 1 `nothing was started`, no turn, no `auth` start; an NFD directory outside runs).
- **Plugin.** The plugin checks the configuration as written (and the reported projectsDirectory
  only); C3 adds the NFC reading - only more refusals.

## F30-2 / F23-3 failure path (still open in round 1) - a failed write before the kill

- **Fix.** The hook's answer to `KillEvent::Before` is now a `subprocess::KillRecord` the killer
  receives before it stops anything. A failed write is retried ONCE into the fallback file next to
  the record (`.consult.pending-kill.json`; a member's `.consult.pending-<NN>-kill.json`), a name
  the recovery of both bridges reads as a record. Both failing (`Lost`): the kill still runs (a hung
  tree must not outlive the run), not confirmed, and the run is refused before its commit unless
  the end of the run writes the record. Test hook: `CODEX_CONSULT_TEST_KILL_FALLBACK_FAIL=1`.
- **Proof (RC2).** `kill_record_durable::rc2_a_lost_kill_record_leaves_its_fallback_and_the_next_run_is_refused`
  and `..::rc2_a_kill_record_lost_with_its_fallback_is_refused_from_the_launch_record_alone`: the
  read-only fixture, the kill unable to enumerate the children (`CODEX_CONSULT_TEST_KILL_DENIED=1`,
  so the fake's PowerShell under the killed launcher survives - the helper), the bridge terminated
  in the pause right after the main kill, access restored. On disk: the launch record unchanged and
  the fallback with the record before the kill (or, blocked, nothing else). The next run is refused
  naming the helper's pid (the dry run: every record would refuse); with the fallback, once the
  helper is gone the next run consumes both records and removes the fallback. Unit level:
  `kill_record_tests::a_kill_whose_record_cannot_be_written_keeps_it_in_its_fallback`,
  `..::a_kill_whose_record_cannot_be_written_is_not_confirmed_and_loses_nothing_silently`,
  `subprocess::tests::a_kill_whose_record_is_lost_still_kills_and_is_not_confirmed`,
  `pending::tests::a_kills_fallback_file_is_read_as_a_record`,
  `kill_record_durable::a_kill_record_that_cannot_be_written_is_kept_in_its_fallback`.
- **What RC2 showed.** The recovery already refused from the launch record alone for this helper:
  its parent is the recorded child pid, and the by-parent scan walks any name under it. The
  launch record alone cannot find a non-codex-like process below a dead intermediate the record
  never named; the record before the kill (and now its fallback) names every intermediate of the
  enumeration. Details: `wave3a-recovery.md`, F30-2, "(Wave 7 ...)".
- **Plugin.** The plugin writes after the kill only and warns and commits on a failed write; the
  fallback file is C3's (plugin 0.6.1 judges it as a member-named record).

## F29-1 (documented) - the shared health journal with plugin 0.6.1

Recorded as unsupported (RC4's second option): with plugin 0.6.1 on the same machine the once-only
replay holds only for C3's own updates; the plugin's updater can re-apply an evicted record until
plugin 0.6.2 (T14). `wave2b-compat.md`, "Limits"; README, "Known limits".

## F25-1 / F32-1 / F32-2 (documented, one fact verified)

The `--version` probe cannot prove a launcher takes a turn (unsupported, stated). The guard's home
is `home_var` - `USERPROFILE` on Windows, `HOME` elsewhere - the variable libuv's `uv_os_homedir`
reads first; its system fallback is not resolved, so an unset or blank variable refuses the run
(confirmed, unchanged). A wrapper launcher that sets `CLAUDE_CONFIG_DIR` inside itself stays
unverifiable before the spawn. `wave4-claude.md`, "Wave 7".

## Verification

See `.collab/parity-0.6.1-2026-10-08/wave7-STATE.md` for the counts and log paths of this wave's
suite, clippy, fmt and the three harnesses through the shim.
