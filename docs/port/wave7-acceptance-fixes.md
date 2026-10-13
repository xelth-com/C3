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
  (Superseded by 7b below: no kill without a record - a third location, else no kill.)
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

## 7b: no kill without a record (handoff 38, F38-1)

The round-2 acceptance (`handoffs/38-codex-0.2.0-acceptance-round2.md`, HOLD on F38-1 only) found
the last gap of the failure path above: both writes before the kill failing, the kill still ran
and `KillRecord::Lost` lived in memory only. A bridge that died right after killing an unrecorded
intermediate left a launch record naming neither it nor its helper: the by-parent scans cannot
cross the dead intermediate, the machine-wide rule passes a helper with a readable non-codex
command line, and the next run released the task. The coordinator's decision, implemented here:
fail closed - NO KILL WITHOUT A RECORD.

- **Three locations.** Before any tree kill (the main turn's timeout, stall or kick, the timeout
  continuation's and the format repair's - all through `KillSite::before_kill` and
  `subprocess::kill_tree_checked`) the record that names the tree must be durable in at least one
  of, tried in this order: (1) the recovery record (`.consult.pending.json`, a member's
  `.consult.pending-<NN>.json`); (2) its fallback (`.consult.pending-kill.json`,
  `.consult.pending-<NN>-kill.json`); (3) the codex home's copy, outside the collab directory:
  `<codex home>/c3/pending-kill/<task>-<run id>.json` (`<codex home>` = `CODEX_HOME`, else
  `~/.codex` - the bridge's resolver; the run id = the consultation id), the fallback's JSON plus
  `collab_dir` and `task`. Test hook: `CODEX_CONSULT_TEST_KILL_THIRD_FAIL=1` - the copy fails too.
- **The recovery reads all three.** `liveness::pending::pending_paths` lists, after the task's own
  records, the copies whose `task` is the task directory's name and whose `collab_dir` is its
  parent (compared as directories, not as spellings); a file named `<task>-<uuid>.json` whose task
  and collab directory cannot be read is listed too (fail-closed: it refuses as unusable). A copy
  is judged, refused on and consumed exactly as the fallback - by the run's recovery, the panel's,
  the findings and rating refusal and `-List`. A run whose end writes the recovery record clean (it
  names what the kills left) removes its copy; a copy a dead bridge left is removed when the next
  run consumes it.
- **All three failing: no kill.** `KillRecord::Lost` carries the three errors and the killer
  leaves the tree ALIVE as the evidence (`KillCheck::not_killed`; the survivors are the root and
  its live descendants). The turn ends failed, its stop unchanged (class `timeout` or `stall`,
  never usable, no continuation): `failed: timeout after N s (not killed: the kill record could
  not be written anywhere (<record error>; its fallback <path>: <error>; its copy <path>:
  <error>): the reviewer tree (pid <root>) is left alive as the evidence; the task is refused until
  it is gone or recorded)`. The launch record on disk names the root as its `child_pid`: the next
  run of the task is refused while it lives. An end of the run that cannot write the record either
  is refused uncommitted and names the tree it left running.
- **Proof (RC1 of reply 38, as far as the decision allows).**
  `kill_record_durable::rc1_f38_a_helper_below_a_dead_intermediate_is_refused_from_the_codex_home_copy`:
  the tree cmd (root) -> the fake's PowerShell (intermediate) -> a helper PowerShell that only
  sleeps (its command line unrelated to codex); the record read-only and the fallback blocked, the
  copy written; the kill stops the root and the intermediate, the helper survives
  (`CODEX_CONSULT_TEST_KILL_SPARE=<pid file>`, test mode only, stands for a helper the kill cannot
  stop: no `taskkill /T`, not by pid - the test asserts it alive), the bridge dies at `kill,main`.
  On disk: the launch record unchanged, no fallback, the copy naming root, intermediate and helper
  with their start times. The dry run shows the copy refusing on the helper; the next run is
  refused before any engine turn (the fake's turn marker is never written); the helper gone, the
  next run consumes the copy, removes it and takes its turn.
  `..::f38_a_kill_record_written_nowhere_leaves_the_tree_alive_and_the_task_refused`: all three
  blocked - after the run the root and the intermediate still run, the message as above, nothing
  committed, no continuation; the next run is refused from the launch record naming the root,
  before any engine turn; then the tree is cleaned up.
  `..::f38_the_codex_home_copy_is_removed_once_the_record_is_written_clean`: access restored
  during the pause after the kill, the end of the run writes the record clean and the copy is
  gone. `..::rc2_a_kill_record_lost_with_its_fallback_is_kept_in_the_codex_home_and_refuses`:
  RC2's launch-record-alone variant became the copy variant (with both collab writes failing a
  kill now always has the copy, or does not happen). Unit level:
  `pending::tests::a_kill_copy_in_the_codex_home_is_read_as_a_record_of_its_task_only` (the
  resolver, the listing, the judging), `kill_record_tests::a_kill_whose_collab_writes_fail_keeps_its_record_in_the_codex_home`,
  `..::a_kill_that_did_not_happen_is_told_as_such`,
  `subprocess::tests::a_kill_whose_record_is_lost_does_not_kill_and_is_not_confirmed`.
- **Documented limit.** An EXTERNAL death of the recorded ancestors - not by C3: the operator or
  another tool stopping the root and an unrecorded intermediate - that leaves a generic helper (a
  readable command line unrelated to codex) is not detectable by either bridge: no record names
  the helper and no scan reaches it. Parity with the plugin. The copy is read from the codex home
  the next run resolves: a next run under another `CODEX_HOME` does not see it.
- **Unchanged.** The registration failure's stop (`kill_tree_by_pid` of a child whose `running`
  record could not be written, rows (e)) stays the plugin's: the child that just spawned is stopped
  at once by `taskkill /T`, its record left at `launching`.
- **Plugin.** The plugin kills after its write whatever it came to and reads no codex home copy:
  a plugin 0.6.1 bridge running the next consultation of a task judges the collab records only
  (the launch record; the fallback as a member-named record).

## 7c: copies that cannot be listed, a kick that killed nothing (handoff 40, F40-1, F40-2)

The round-3 acceptance (`handoffs/40-codex-0.2.0-acceptance-round3.md`, HOLD) found two gaps of 7b.

- **F40-1 (blocker): an unlisted copy is no absent copy.** `kill_copy_paths` treated a failure to
  open `<codex home>/c3/pending-kill` as "no copies" and dropped the entries it could not read
  (`rd.flatten()`), so the one record naming a surviving helper - 7b's copy, the collab records
  naming only the dead ancestors - could be skipped and the task released. Now the directory
  absent (`NotFound`) is the only empty listing; any other error opening it or reading one of its
  entries is an INSPECTION FAILURE: `kill_copy_paths` returns `Err`, and `pending_paths` carries it
  beside the task's own records (`PendingPaths { paths, error }`). Every consumer refuses on it
  before any record is judged - the consult recovery (real run and dry run), the panel's (both
  passes), the findings status change and rating (`read_task_pending_records`), the kick (exit 1);
  `findings -List` prints it as a `pending:` line. The text: `the recovery copies under <dir> cannot
  be listed (<error>): the task is refused until they can be read`. Test hook (test mode only):
  `CODEX_CONSULT_TEST_COPY_DIR_FAIL=denied` (opening the directory fails with `PermissionDenied`,
  whether it exists or not) or `=iter` (it opens, reading its entries fails).
- **F40-2 (minor): a kick that killed nothing says so.** The kicked turn acknowledged `stopped`
  after `kill_now` whatever the kill did, so a kick whose kill record could be written nowhere
  (7b: the tree left alive) made the `-Kick` command report the tree "being stopped" and exit 0.
  Now the turn acknowledges by what its kill did (`acknowledge_taken_kick`): a kill with
  `KillCheck::not_killed` writes `result=not killed` with `root=<pid>` and `reason=<why>` into
  `<kick>.ack`; the command prints `-Kick: member <NN> of task <t> took the kick but was NOT
  stopped - not killed (root pid <pid>): <why>. Its engine's process tree (root pid <pid>) is left
  alive and the task is refused until it is gone or recorded; ...` to stderr, claims no stop and exits `1`
  (the failure code of a kick that is not carried out: "no such member / not running"; `3` stays
  "no acknowledgement", `4` misuse). The member's own outcome was already right (`failed: stopped
  by the operator (-Kick) (not killed: ...)`, refused uncommitted when its record cannot be
  written). A plugin 0.6.1 `-Kick` reads `stopped`|`late` only: it would not recognise a C3
  member's `not killed` (the C3 kick does).
- **Proof (RC1 and RC2 of reply 40).**
  `kill_record_durable::rc1_f38_a_helper_below_a_dead_intermediate_is_refused_from_the_codex_home_copy`
  is extended: after the dry run shows the copy refusing on the helper, the same codex home is
  read under `CODEX_CONSULT_TEST_COPY_DIR_FAIL=denied` and `=iter` - the next run exits 1 with the
  inspection failure naming the directory and the error, before any engine turn (no turn marker),
  the helper alive, the copy and the launch record kept; the dry run refuses the same,
  `findings -List` shows it as `pending:`, the kick refuses on it. Enumeration restored, the run is
  refused naming the helper; the helper stopped, the copy is consumed and the turn runs.
  `..::rc2_f40_a_kick_whose_kill_record_is_written_nowhere_is_acknowledged_not_killed`: the
  all-three-writes-fail fixture with the operator's kick instead of the timeout - the kick exits 1
  naming the live root and the left-alive refusal, never "is being stopped"; root and intermediate
  alive after the kick and after the run; the run failed `stopped by the operator (-Kick) (not
  killed: ...)`, nothing committed. Unit level:
  `pending::tests::a_kill_copy_directory_that_cannot_be_listed_is_an_inspection_failure` (absent:
  empty; injected `PermissionDenied` with the directory absent and present; injected entry error;
  a real one - the directory's path is a file), `subprocess::tests::a_kick_whose_kill_did_not_happen_is_acknowledged_not_killed`,
  `kick::tests::a_not_killed_acknowledgement_is_reported_as_no_stop`.
- **Unchanged.** The task directory's own listing (`.consult.pending*.json`) keeps the plugin's
  rule (`Get-PendingPaths`); a run under another `CODEX_HOME` still does not see the copy (7b's
  documented limit).
- **Verified.** `cargo test -j 2 --no-fail-fast --workspace`: 793 tests (789 + 4: c3 lib 418 -> 421,
  kill_record_durable 11 -> 12), 792 passed, the known flaky `index::embed` loopback test green on
  its rerun; RC1 showed the release (the copy consumed, the turn taken); clippy `--all-targets -D
  warnings` and `cargo fmt --check` clean; through the shim (the wave-7 pinned v0.6.1 tree, Windows
  PowerShell 5.1, `HARNESS.lock`): harness-pending 26/0, harness-fixes 56/0. Counts and log paths:
  `.collab/parity-0.6.1-2026-10-08/wave7c-STATE.md`.

## Verification

- `cargo test -j 2 --no-fail-fast --workspace`: 784 tests (baseline 772 + 12 new), 783 passed; the
  one failure was the known flaky `index::embed::tests::embed_reaches_a_loopback_fake_via_localhost`,
  green on its rerun (`index::embed` 6/6).
- `cargo clippy --workspace --all-targets -j 2 -- -D warnings` and `cargo fmt --check`: clean.
- RC2 against the pre-change binary (code 09b9d25, via `C3_TEST_EXE`): the launch-record fixture
  passes, the fallback fixture fails (no fallback file) - the refusal from the launch record held
  before, the fallback is what wave 7 adds.
- Through the shim (pinned v0.6.1 tree, Windows PowerShell 5.1, one harness at a time under
  `HARNESS.lock`): harness-claude 87/0, harness-pending 26/0, harness-fixes 56/0.

Counts and log paths: `.collab/parity-0.6.1-2026-10-08/wave7-STATE.md`.
