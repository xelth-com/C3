# Handoff 58 - Claude: acceptance of C3 0.2.5, the delta after your REJECT

Date: 2026-11-24. A NEW thread. You run in a throwaway checkout of the head of `main`.
Scope: `git diff 5031982 HEAD` - three code commits: `ea74a4d`, the fixes for the ten findings of
your review of `5031982` (handoff 57, `.collab/release-0.2.5/handoffs/57-codex-acceptance-2.md`:
verdict REJECT, six blockers, three majors, one minor); `8087bad`, which widens the F57-6 fix to
every collab root; and `fe877b2`, which closes what two cheaper diff reviews of `8087bad` found
before this pass (handoffs 60 and 61: the commands' resolver followed a linked collab root) -
plus this brief and the coordinator's records. Everything else is as you read it.

**Budget: this is the last pass the account has this week.** The thread is replayed on every tool
call: read narrowly (`rg -n`, `sed -n` ranges, `git diff 5031982 HEAD -- <one file> | head -n ...`),
never a whole large file. Judge each fix against YOUR remedy; about fifty tool calls should do. If
you run short, give the verdict on what you read and say what you did not read.

**Sub-agents are off and guarded. Do not delegate.** `CHANGELOG.md`, section `[0.2.5]` /
`### Fixed`, has one entry per finding (search `F57-<n>`).

## Your findings and what was done

| Finding | Fix (where) | Closing test | Limit that remains |
|---|---|---|---|
| **F57-1** page label reused for another URL publishes the earlier page's ids | a page record carries the canonical URL its ids were read from and whether it was public then (`SourceList.url` / `.public`, `maintain/models.rs`); trust and publication go by that URL, one rule for catalogs and pages (`web::canonical_url`, `web::provenance`); a label-only record of an earlier build is published by nothing | RC1: `telemetry_parity.rs::a_page_label_reused_for_a_public_url_never_publishes_the_earlier_pages_id`; `maintain::models::tests::a_page_id_is_published_only_by_the_exact_public_url_it_was_read_from`; `maintain::web::tests::a_reused_page_label_never_lends_its_trust_to_ids_of_the_earlier_url` | a URL declared public later is published only after the next refresh |
| **F57-2** an unreadable nested tool inventory counts as switch evidence | `c3_core::quota::tool_inventory` answers complete / breached / unproven; only a complete list is evidence; an unreadable list makes its turn and the smoke unproven (`TurnEvidence::unreadable`, `proven()`; `maintain/smoke.rs`) | RC2, first half: `maintain::smoke::tests::an_inventory_that_cannot_be_read_in_full_is_no_switch_evidence`; `quota::tests::a_tool_inventory_is_complete_breached_or_unproven`, `an_unreadable_tool_list_leaves_its_turn_unproven` | hosted tools identified by their type alone are accepted from a fixed list (`local_shell`, `web_search`, `web_search_preview`, `image_generation`, `file_search`, `code_interpreter`, `computer_use_preview`) - without it every real rollout would hold every update; a new tool kind of a later CLI holds the update |
| **F57-3** a recovery credits the target after smoking a third version | recovery reads the installed version first (`maintain/mod.rs`): target -> smoke, and verified only if the version read again after the smoke is still the target; previous -> as before; a third version -> smoked for itself, the action and the state name it, the target is "not verified"; unreadable -> no smoke, nothing verified, the record kept | RC2, second half: `maintain.rs::a_pending_recovery_never_credits_a_third_installed_version_to_the_target`, `a_pending_recovery_with_an_unreadable_installed_version_verifies_nothing` | a change and change-back inside one smoke is not seen |
| **F57-4** a creator suspended after the exclusive create returns as a second owner | the gate is taken BEFORE the exclusive create and held through the complete, flushed record and a read-back; only a file that holds exactly this process's record makes it the owner (`try_create_lock`, `state_dir.rs`) | RC3: `state_dir::tests::a_lock_creator_paused_after_its_exclusive_create_is_never_a_second_owner` | another host on a file system without working OS locks has only the age rule; a c3 older than 0.2.5 takes no gate |
| **(your answer 2)** a writer with the switch off creates `.c3` beside a move | one function creates `.c3/` (`state_dir::state_folder`): while `.eck/` is a real directory, only under the move lock after re-reading the state, whatever the switch says; `ensure`, `prepare_dir` / `write_in`, `begin_run` / `run_dirs`, the cache move all go through it | `state_dir::tests::a_fresh_folder_beside_the_old_one_is_created_only_under_the_move_lock`; `finish.rs::a_writer_with_the_switch_off_never_splits_a_move` | something outside C3 can still put a `.c3` there |
| **F57-5** short-hash names collide; the no-hard-link rename replaces | the name carries the full sha256 (`<stem>.<64 hex>.patch`); with hard links: link, judged again on "exists"; WITHOUT hard links: an exclusive per-run name `<stem>.<64 hex>.<32 hex>.patch`, checked free through the handle, and that path is what the plan, the prompt and the ledger carry; no publication replaces a file; the `Empty` branch is gone (`place_patch`, `consult/reviewed.rs`) | RC4: `consult::reviewed::tests::two_different_patches_forced_to_one_name_never_share_a_path`; `a_foreign_file_appearing_at_the_patch_name_survives_byte_for_byte`; `a_patch_without_hard_links_is_published_under_the_runs_own_name` | without hard links every run writes its own copy; no real file system without hard links was used (a test hook) |
| **F57-6** a linked task directory below a real collab directory | one function makes the task directory and `handoffs/` (`work/` for a work run) ready (`state_dir::run_dirs`, called by `begin_run` for single, detached, panel and work runs and by each panel member); the callers' `create_dir_all` is gone. Inside C3's project folder (`.c3/`, legacy `.eck/`): through handles from the collab root. Under a collab directory inside the repository (the default `.collab/`): EVERY component from the repository root down is opened without following it. The collab root arrives there as it is NAMED: the commands' resolver (`providers.rs` `resolve_collab_root_with`) no longer canonicalises a path with a link below the repository root (`state_dir::linked_below_root`; `repo_relative_normal` uses it too, for a spelling that is not a textual prefix of the root) - before `fe877b2` a linked `.collab` or `.c3/collab` was resolved to its target, read as a directory outside the repository and followed by consult, panel and detach (F60-1, F61-1). `c3 work` resolves `.`/`..` in its collab path by text before use (F61-3). A collab directory outside the repository: the root itself is the operator's anchor (F52-3, followed), the task directory and `handoffs/` below it are not followed. A link at any of them refuses the run in one line before anything is written | RC5: `kimi_engine.rs::a_linked_task_directory_in_the_project_folder_refuses_the_run_and_writes_nothing_there` (consult and `--detach`), `panel_light_required.rs::a_panel_with_a_linked_task_directory_in_the_project_folder_is_refused`, `work.rs::a_linked_task_directory_in_the_project_folder_refuses_the_work_run`, `state_dir::tests::a_linked_task_directory_inside_the_folder_refuses_the_run_and_creates_nothing` (also `.collab/t`, `.collab/t/handoffs`, a linked default collab root - by the unresolved path, through `resolve_collab_root`, through a link above the repository root and as a verbatim path - a linked `.c3/collab3`, and a task directory linked below an anchor outside the repository), `kimi_engine.rs::a_linked_collab_root_refuses_the_run_and_writes_nothing_there` (`.collab` and `.c3/collab` as junctions; consult and `--detach`) | the run's LATER writes still go by path into the directories checked at the start - a directory swapped for a link after the check, while the run writes, is not caught. Your remedy's second half (outputs bound to handles through publication) is not done |
| **F57-7** the resume claim is not bound to the claimed directory | the path is built from the temp directory (opened once) + the validated basename; the recorded parent must be the temp directory by file identity; the directory is created exclusively through that handle, opened without following, the handle kept; after `git worktree add` the path must still be that directory before anything is staged; a mismatch refuses, removes nothing, prunes the registration (`create_at`, `engines/worktree.rs`) - for every worktree, fresh or resumed | `engines::worktree::tests::a_claimed_worktree_directory_replaced_before_the_check_refuses_and_is_kept`, `a_reusable_path_is_built_from_the_temp_directory_by_identity`; the three D7 tests in `kimi_engine.rs` | between the exclusive create and the open no handle is held: an EMPTY directory swapped in at that instant cannot be told apart |
| **F57-8** format characters in the isolation prompt | `prompt_safe_path` uses `crate::unicode::is_format_char` | `the_isolation_prompt_names_tracked_changes_and_counts_the_rest` (extended) | - |
| **F57-9** `localhost` by name | plain http in test mode only to a loopback IP LITERAL (`is_loopback_literal`, `c3-core/src/roster_ext.rs`); the telemetry intake's test-mode rule follows (`telemetry::hub`) | `roster_ext::tests::s2_plain_http_only_to_a_loopback_host_in_test_mode` | - |
| **F57-10** the roster write can lose a hand edit; **F02-2** the repair's rename | both publish by an atomic EXCHANGE (`contain::publish_exchange`: `ReplaceFileW` on Windows, `renameat2(RENAME_EXCHANGE)` / `renamex_np(RENAME_SWAP)` through rustix); the displaced file is read back - equal to the basis: removed; different: kept as `<file>.displaced-<UTC time>` and reported (`c3 maintain`: `ATTENTION: ...` in its output and report; the hook line; `c3 doctor`) | your Verify: `maintain::models::tests::a_roster_edit_saved_at_the_publication_boundary_is_kept_byte_for_byte`; `contain::tests::publish_exchange_keeps_an_edit_saved_before_the_exchange_and_nothing_without_one`, `without_an_exchange_publication_falls_back_to_the_rename_and_says_so`, `publish_exchange_on_unix_swaps_relative_to_the_directory_descriptor`; `setup::repo_models::tests::an_edit_saved_during_the_repair_is_kept_beside_the_file_and_reported` | where no exchange exists (another Unix, a file system that answers `EINVAL` / `ENOSYS` / `ENOTSUP`) the old compare-then-rename is used and the report says so |

## Also in the commit

- **A one-revision patch could be EMPTY for a changed file** (found through CI, not by a
  reviewer): the temporary index was a copy of the real one with a NEW modification time, so git
  no longer treated a same-size edit made in the same instant as the index as "racily clean" and
  trusted the stat data. `copy_index` (`consult/reviewed.rs`) keeps the real index's modification
  time. Test: `a_racily_clean_change_is_in_a_one_revision_patch`. The range form (`git read-tree`,
  `--cached`) was never affected.
- New direct dependencies, both already in the lock file through cap-std's stack: `windows-sys`
  (Windows, `ReplaceFileW`) and `rustix` (Unix, `renameat_with`).

## Evidence

- This machine (Windows), in the foreground, on `ea74a4d`, after a forced rebuild of the workspace
  crates: fmt clean; clippy `-D warnings` clean; `c3` library tests 961 passed, `c3-core` 256;
  `c3-cli`: `finish` 28, `kimi_engine` 25, `panel_light_required` 6, `work` 30, `maintain` 46,
  `telemetry_parity` 13, `init_doctor` 22, `delegation_rule` 7, `wave6_live_diffs` 6,
  `claude_engine` 19, `compat_wave2b` 21, `branches` 18, `ci_check` 14, `hook_scripts` 5,
  `snapshot_partb` 7, `mcp_integration` 3, `compat_wave5` 5, `grok_engine` 18,
  `kill_record_durable` 12, `notspooled_parity` 21 - all passed.
- On `fe877b2`, same machine, foreground: fmt clean; clippy `-D warnings` clean; `c3` library
  961, `c3-core` 256; `c3-cli`: `kimi_engine` 26, `work` 30, `panel_light_required` 6 - all
  passed. (On `8087bad` before it also `claude_engine` 19, `grok_engine` 18, `compat_wave2b` 21,
  `wave6_live_diffs` 6.)
- CI for `ea74a4d` (run 45418818364): GREEN on all three jobs - Windows, Linux, Linux with
  `index-surreal` - so the Unix exchange code and the `cfg(unix)` tests compiled and passed.
  CI for `8087bad` (run 45438780916): both Linux jobs green; the Windows job FAILED in one test,
  `claude_engine.rs::a_decomposed_config_dir_inside_an_nfc_repository_refuses_the_endpoint_run`,
  at its last step ("`claude --version` did not finish within 15 s" - the fake launcher's start).
  This is the known Windows flake of the tech-debt list, the second log of the same test at the
  same line (the first: run 45389095446, green on its rerun); the commit does not touch what the
  test exercises. Its cause is not known yet.
  CI for `fe877b2`: run 45446676389 - GREEN on all three jobs (the first attempt), with the new unit assertions on Linux (symlinks) and Windows (junctions, the verbatim path).
  The head adds only this brief, the records and a hand-started measuring workflow for that
  flake (`.github/workflows/flake-probe.yml`). (You have no network: `gh` is not available to
  you.)
- The four workers shared one build directory and their own test runs could mix builds; the
  numbers above are the coordinator's, on the merged tree.
- Seen live AFTER the fixes, with the head's build and the real kimi CLI (task `smoke-kimi`,
  `.collab/smoke-kimi/handoffs/08-kimi-live-5.md`, `09-kimi-live-6.md`): a new thread with
  `--rev HEAD --diff HEAD~2..HEAD` - usable; the patch file's sha256 equals the 64 hex digits in
  its name and the ledger names that path. `--mode resume` - usable, the SAME worktree path, the
  same session. A resume while a directory with a file sits at that path - refused before launch
  ("the worktree path ... of the thread it resumes is taken"), the directory and its file
  untouched, no handoff written, no worktree registered. (Part of your first-run checklist.)
- Seen live with the build of `fe877b2`: in a scratch repository whose `.collab` is a junction
  to a directory outside it, `c3 consult --task t --provider kimi ...` answered `.collab is a link
  (a symlink or junction) - C3 never writes a run's records through a link (its target is
  untouched); nothing was started`, and the target held only its own file afterwards; the build of
  `8087bad` said "collab dir outside the repository" in a dry run of the same command. The same
  refusal for `.collab/t` as the junction.
- THIS repository's `.eck/` was moved to `.c3/` by the head build's `c3 finish` (at the commit
  `8087bad`): the old tool's `anchor`, `update_seq`, `telemetry_queue.json` went to
  `.c3/legacy/`, `lastsnapshot/AnswerToSA.md` to `.c3/last-report.md`, C3's cache
  `.collab/.c3/{anchor, snapshot_seq}` into `.c3/`, one missing stub was written. Compared by
  sha256 with an inventory taken before: 17 files with the same name and hash, 3 in `legacy/`,
  the two edited since present, nothing lost, `.eck/` gone. (The folder is git-ignored: you do
  not see it.)
- The two diff reviews of `8087bad` (grok-4.7, glm-5.4; brief 59) also gave: F60-3, a stale
  clause of the CHANGELOG (corrected); F61-2, rejected (the work record allocation skips an
  existing name, it never returns it); F61-4, decided (every reparse point counts as a link - the
  accepted limit of F43-8 - now also on the default `.collab` chain; an ordinary process on this
  machine reads the OneDrive root as a plain directory).
- The "Decided, not fixed" and "NOT covered" lists of brief 56 stand, minus what is fixed here;
  your answer 7 objected to nothing else in them (agy live, a real file system without hard
  links and macOS are gaps you named as gaps).

## Questions

- **Q1.** For each row of the table: closed / not closed, with `path:line` when not closed.
- **Q2.** Does the delta open anything new that blocks: the exchange and its displaced file, the
  per-run patch name, `run_dirs` / `state_folder`, the resolver's link rule
  (`linked_below_root`), the three-answer inventory with its hosted-tool
  list, the recovery's three cases?
- **Q3.** The limits in the last column (for F57-6: a collab root outside the repository is
  followed itself; a run's later writes go by path): does any of them block the tag?
- **Q4.** Is 0.2.5 safe to tag at this head, given a green three-job CI run for the tagged
  commit? Say ACCEPT or REJECT.

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
