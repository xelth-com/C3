# Handoff 66 - Claude: acceptance of C3 0.2.5, the delta after your second REJECT

Date: 2026-11-24. A NEW thread. You run in a throwaway checkout of the head of `main`.
Scope: `git diff ea30cce HEAD` - three code commits: `f78901b`, the fixes for the six findings of
your review of `ea30cce` (handoff 62, `.collab/release-0.2.5/handoffs/62-codex-acceptance-delta.md`:
verdict REJECT, five blockers, one minor) and for the limits your answer 3 called blocking;
`2d99f72` and `82d2abe`, what two cheaper diff reviews of `f78901b` found before this pass
(handoffs 64 and 65, both ACCEPT, one note and six small findings) - plus this brief and the
coordinator's records. Everything else is as you read it.

**Budget: the account's week is at 90 %; this pass runs on the operator's word and is the last
one this week.** The thread is replayed on every tool call: read narrowly (`rg -n`, `sed -n`
ranges, `git diff ea30cce HEAD -- <one file> | head -n ...`), never a whole large file. Judge each
fix against YOUR remedy; about forty tool calls should do. If you run short, give the verdict on
what you read and say what you did not read.

**Sub-agents are off and guarded. Do not delegate.** `CHANGELOG.md`, section `[0.2.5]` /
`### Fixed`, has one entry per finding (search `F62-<n>`).

## Your findings and what was done

| Finding | Fix (where) | Closing test | Limit that remains |
|---|---|---|---|
| **F62-1** a prerelease smoke credits the final release | a version read from a CLI carries its exact release IDENTITY (only surrounding whitespace, a leading `v`, a trailing `.` and punctuation after the token are dropped; `1.0.80-rc.1` is not `1.0.80`; build metadata is part of it) and, separately, a semver ORDERING used only for "is it newer" (a prerelease sorts before its final release). The type has no `<` / `>` any more: every caller says which question it asks (`maintain/version.rs`: `same_release`, `is_newer_than`). The recovery's comparisons (previous, target, after the smoke, after a rollback) use the identity; so do "is the previous version back", "up to date" and the cool-down's release-date lookup (`maintain/mod.rs`, `engine.rs`). After an install the version is read again: the target -> smoked as before; ANOTHER readable release (same numbers with another identity, a newer one) -> smoked in the same pass, credited to itself, the target not verified, rolled back on a failed smoke (the recovery's own code path, `handle_pending_smoke`); still the previous one, or unreadable -> "update did not take", no smoke, the record kept | RC1: `maintain.rs::a_pending_recovery_never_credits_a_prerelease_smoke_to_the_final_release`, the mirror `a_pending_recovery_never_credits_a_final_release_smoke_to_a_prerelease_target`, `an_installed_prerelease_is_updated_to_its_final_release`, `an_update_that_leaves_a_prerelease_of_the_target_smokes_it_for_itself`, `an_update_that_leaves_a_failing_prerelease_rolls_it_back`, `a_rollback_to_a_same_numbers_release_is_not_a_restore`, `an_update_that_leaves_an_unreadable_version_smokes_nothing_and_keeps_the_record`, `a_prerelease_cool_down_reads_the_prereleases_own_date`; `maintain::version::tests::equal_numbers_with_different_identities_are_different_releases`, `a_pre_release_sorts_before_its_final_release`, `parses_the_first_semver_token` | a version token followed by other characters (`1.0.80(rc1)`), or a dotted number that does not fit a u64, is unreadable - no smoke, nothing verified, a later number on the line is not taken; a pending record written by an earlier build holds bare numbers, so a prerelease installed then recovers as a third version; an unreadable version after an install waits unsmoked for the next pass; the state's `installed` field shows the version read at the start of the engine's turn (the action line names what the updater left) |
| **F62-2** `link/../.collab` taken as an external anchor | the commands' resolver resolves `.` and `..` by TEXT before its link check and before its canonical form (`providers.rs` `resolve_collab_root_with`, `state_dir::lexical`); `c3 work` already did (F61-3). Consult, panel, detach and work name one directory, the textual one | RC2: `state_dir::tests::a_linked_task_directory_inside_the_folder_refuses_the_run_and_creates_nothing` (`l/../.collab4` through `resolve_collab_root` and handed straight to `begin_run`: the records land in the repository's `.collab4`, nothing beside the link's target); `work.rs::consult_takes_a_collab_path_with_dot_dot_after_a_link_by_text` (`c3 consult --dry-run` through the resolver, Windows and Unix: the task's place is the repository's own `.collab4`, a collab directory inside the repository); `work.rs::a_collab_path_with_dot_dot_after_a_link_is_taken_by_text` (`c3 work`, F61-3) | a panel and a detached run have no end-to-end test of this spelling; they call the same resolver as the consult test |
| **F62-3** a catalog declared public later publishes ids cached while private | a catalog record stores `url` and `public` as a page record does; one rule for both (`web::sources_of`, `web::recorded`, `web::provenance`): an id is published only when its record says the URL was public WHEN READ and the configuration trusts exactly that URL now. A custom catalog's record of an earlier build (no status) publishes nothing until refreshed; the built-in catalog's legacy record is kept only under its exact canonical URL (`web::is_builtin_catalog_source`) - its trust always came from the code, never from the configuration | RC3: `telemetry_parity.rs::a_catalog_declared_public_later_never_publishes_an_id_cached_while_it_was_private` (the serialized model field before the declaration, right after it with the old cache, with a legacy record, after a refresh of the cleaned catalog); `maintain::web::tests::a_catalog_declared_public_later_never_publishes_ids_cached_while_it_was_private`; `maintain::models::tests::a_catalog_id_is_published_only_when_its_record_was_public_when_read`; the built-in legacy exception through the binary: `telemetry_parity.rs::a_published_claude_id_spelled_with_dots_is_sent_by_its_dash_name` | a URL declared public later is published only after the next successful refresh; the recorded status is what the configuration declared at collection time |
| **F62-4** two displaced copies can share a name | the displaced version's name is TAKEN by a hard link - which fails when the name exists - first `<file>.displaced-<UTC>`, then `-2`, `-3` ...; the temporary name is removed only after the link exists (`contain::place_displaced`). No displaced version replaces a file; the unchecked thousandth candidate is gone | RC4: `contain::tests::two_displaced_versions_never_share_a_name_and_nothing_is_replaced` (the same stem twice beside an earlier file of that name: three paths, three byte sequences; the no-hard-link branch) | where no hard link can be made, and after a thousand taken names, the name gets 32 random hex digits, is checked free and taken by rename - a look followed by a rename |
| **F62-5** a namespace nested in `collaboration` loses the ancestry | a list inside the `collaboration` namespace stays inside it at any depth (`quota::walk_tool_list`) | RC5: `quota::tests::a_namespace_nested_in_the_multi_agent_namespace_keeps_its_ancestry` (two and three levels, a hosted tool by kind, the same nesting outside the namespace still complete, an unreadable entry below still unproven) | - |
| **F62-6** the full digest makes a patch name too long | the readable stem is cut to 120 bytes at a character boundary before any name is built (`consult::reviewed::PATCH_STEM_MAX`, `bounded_stem`); the digest is never cut | `consult::reviewed::tests::a_long_brief_stem_is_bounded_so_every_patch_name_fits` (your 160-byte stem; two-byte characters across the cut; every built name at most 255 bytes) | two briefs whose stems differ only after byte 120 share a readable stem (their patches still differ by digest) |
| **Answer 3: the rename fallback** (F57-10's limit) | gone. Where the system or the file system has no atomic exchange the write is REFUSED: the temporary file removed, the file as it was, the command says why (`Publication::Rename` no longer exists; `contain::publish_exchange`). `c3 maintain` seats no model there and the repair of agent files changes nothing there | `contain::tests::without_an_exchange_the_write_is_refused_and_the_file_is_as_it_was` (an edit saved at that moment is still the file) | on such a file system the operator makes these edits by hand; the refusal is proved through the test hook, not on a real file system without an exchange |
| **Answer 3: later path-based writes** (F57-6's limit); **F57-7's Unix window** | NOT done, and now said as what is promised, in both CHANGELOG entries: the refusal is about a link that EXISTS when the run starts (arrived with a clone, left in the tree, named by the operator). Containment by directory identity for the whole run is not promised: whoever can swap a directory of the working tree, or of the operator's own temp directory, while a run writes already has the operator's rights over the repository and its records. On Linux and macOS a swapped worktree directory is caught by the identity check after `git worktree add`, not prevented (git takes a pathname) | - | binding every write of a run to directory handles is a rewrite of the run's output path; it is on the tech-debt list for the release after this one |

## Evidence

- This machine (Windows), in the foreground, on the tree of `82d2abe` after a cold build without
  an incremental cache: fmt clean; clippy `-D warnings` clean; `c3` library tests 967 passed,
  `c3-core` 257; `c3-cli`: `maintain` 54, `telemetry_parity` 14, `work` 32 - all passed. The
  other `c3-cli` test files were not run locally in this round (the disk was nearly full); CI
  runs them.
- CI for `f78901b` (run 45476534138): GREEN on all three jobs - Windows, Linux, Linux with
  `index-surreal` - first attempt; the Unix assertions of this round ran there. CI for `2d99f72`
  (run 45482456435): GREEN on all three jobs, first attempt. CI for `82d2abe` was started when
  this brief was handed over. (You have no network: `gh` is not available to you.)
- Two diff reviews of `f78901b` in new threads (brief 63): grok-4.7 (handoff 64) ACCEPT with one
  note - a dotted number beyond u64 let the version scan go on - fixed in `2d99f72`; glm-5.4
  (handoff 65) ACCEPT with three minors and three notes - stale wording about the rename fallback
  in eight places, the same-pass smoke lost after a mismatched update (a regression of the F62-1
  fix), no test through the consult entry point, two test gaps, a misplaced doc comment - all
  worked in, `82d2abe`. Two workers wrote F62-1 and F62-3 in isolated worktrees with a type-check
  only; every number above is the coordinator's, on the merged tree.
- The Windows CI flake (`claude_engine.rs`, `a_decomposed_config_dir_inside_an_nfc_repository_refuses_the_endpoint_run`,
  "`claude --version` did not finish within 15 s") was measured with a hand-started workflow: the
  test alone 0 failed of 45, its whole binary 0 of 9, the sequence of binaries as `ci` runs them 0
  of 12; in `ci` it failed 2 of about 18 Windows runs since `d476520`, green on each rerun. Its
  cause is not known; it is on the tech-debt list with what to try.
- The "Decided, not fixed" and "NOT covered" lists of brief 56 stand, minus what is fixed here.

## Questions

- **Q1.** For each row of the table: closed / not closed, with `path:line` when not closed.
- **Q2.** Does this delta open anything new that blocks: the version identity and its parser, the
  same-pass smoke of a release that is not the target, the catalog status and the legacy rule for
  the built-in catalog, the hard-link naming of the displaced copy, the refusal where no exchange
  exists?
- **Q3.** The last row: with the promise stated as it now is in the CHANGELOG, do the two limits
  still block the tag? If yes, say which sentence of the promise you do not accept.
- **Q4.** Is 0.2.5 safe to tag at this head, given a green three-job CI run for the tagged
  commit? Say ACCEPT or REJECT.

Answer by number. Cite `path:line` for every finding. Keep it under 800 words.
