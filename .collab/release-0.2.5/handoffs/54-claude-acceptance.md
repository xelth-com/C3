# Handoff 54 - Claude: acceptance of C3 0.2.5

Date: 2026-11-24. Scope: `git diff v0.2.4 HEAD` on `main` - 26 commits, about 47 000 added lines
in 143 files outside `.collab/`. A NEW thread. You run in a throwaway checkout of the head of
`main` (the commit this brief is in, version 0.2.5); that tree is the release.

**Sub-agents are off and guarded. Do not delegate.** The change is large: read
`git diff v0.2.4 HEAD --stat -- . ':!.collab'` first, then one area at a time; `CHANGELOG.md`
section `[0.2.5]` describes every item in prose and names the finding ids.

## What this release is

1. **Every reviewer reads the revision under review** - `c3 consult --rev`, `--diff`, a frozen
   tree for claude-engine reviews, agy and kimi in a throwaway worktree
   (`crates/c3/src/consult/reviewed.rs`, `engines/worktree.rs`, `consult/orchestrate/`).
2. **Containment by directory handles** - staging, the link sweep, the removal, the repository
   repair, the patch write and the project folder's own operations are relative to verified
   directory handles (`crates/c3/src/contain.rs`, on `cap-std`).
3. **C3 stands alone** - its project folder is `.c3/` (an existing `.eck/` is moved once, under a
   lock, the rename last); no hand-off snapshot by default (`crates/c3/src/state_dir.rs`).
4. **Reviewer reliability and the codex guard** - agy / kimi / grok outcomes, the bound-input
   rule, the findings snapshot, the maintain smoke that proves the sub-agent switch per turn or
   holds the update (`engines/`, `consult/orchestrate/`, `maintain/smoke.rs`,
   `crates/c3-core/src/quota.rs`).
5. **`c3 maintain`** - the public catalog under an address table, one deadline, a date gate, the
   entry-bound smoke with a fingerprint, the list-price hold (`crates/c3/src/maintain/`).
6. **Panels and routing** - one entry per lab, the upper tier, `c3 router explain` / `replay`
   (`panel/`, `router/`).
7. **Tooling** - `c3 branches` (states, `--close`), the repository repair and `C3_CHILD`, the CI
   check, the hook's deadline, pre-finish checks, the secret gate (`setup/`, `ci_check.rs`,
   `bounded.rs`, `finish/`, `pack/redact.rs`).

## How it was reviewed before you

Three rounds by six other models (gpt-6.2-sol, grok-4.7, muse-spark contributor, glm-5.3,
kimi k3.1-pro, deepseek) - about 40 consultations in this task, 225 findings in
`.collab/release-0.2.5/findings.json`: 210 implemented, 14 wontfix, 1 rejected, none open. Every
reply is in `.collab/release-0.2.5/handoffs/`; `state.md` is the coordinator's log with every
decision. The last round's fixes (commit `e599f27`: waves R7a, R7b, R8 and three corrections of
the coordinator) were NOT read by another model - you are their first reader.

## The blockers the earlier rounds found, and what closes each

| Id | Finding | Fix | Closing test |
|---|---|---|---|
| F02-1, F02-2 | the repository repair wrote through a hard link / truncated in place | temp file + rename relative to the repository root's handle; only a regular file is replaced | `setup::repo_models::tests::a_directory_swapped_for_a_link_before_the_replace_never_receives_the_repair` |
| F02-16 | the maintain smoke ran through another roster entry than the one edited | the smoke is bound to the roster position, the chosen model and a fingerprint of the whole entry | `maintain::smoke::tests::a_bound_smoke_refuses_an_entry_edited_in_any_field_since_it_was_chosen` |
| F02-19 | telemetry could send a private endpoint's model name | an id is published only from the built-in catalog URL or a URL declared public itself | `telemetry_parity.rs::two_catalogs_on_one_host_the_private_first_never_publish_the_private_id` |
| F02-21 | the worktree removal forced a delete after an incomplete unlink pass | one walker that never descends into a link; removal through handles, then `git worktree prune` | `engines::worktree::tests::a_directory_swapped_for_a_link_during_the_walk_is_never_descended` |
| F03-1 | the claude engine's allow-listed environment dropped `C3_CHILD` | the mark is added after the environment is rebuilt, for every child | `engines::subprocess::tests::every_turn_child_is_marked_c3_child`, `claude_engine.rs::every_claude_child_is_marked_c3_child` |
| F11-3, F32-1 | brief staging could write outside the reviewer's worktree | staging on directory handles, both sides | `engines::worktree::tests::staging_into_a_directory_swapped_for_a_link_never_leaves_the_worktree` |
| F11-7 | the maintain smoke passed without proof of the sub-agent switch | `Smoke::Unproven`, treated as a failure by every automatic path; evidence per turn | `maintain::smoke::tests::the_codex_smoke_proves_the_switch_for_each_turn_of_a_shared_rollout`, `maintain.rs::an_unproven_post_update_smoke_rolls_back_like_a_failure` |
| F13-1 | `c3 doctor --fix` could remove through a link named like a throwaway worktree | the path itself is read; a link is reported and never removed; the owner record is mandatory | `branches.rs::a_throwaway_path_that_is_a_link_is_reported_and_never_removed` |
| F19-1 | `--rev` on the http engine sent a pack of the live tree | the pack is built from a throwaway worktree of the revision | `consult::http::tests::an_http_pack_under_rev_is_built_from_the_revision_with_the_patch` |

## Decided, not fixed - say if any of it blocks

- **The roster file is written by compare-then-rename (F15-4, F25-1).** `~/.codex/codex-consult-roster.json`
  is re-read and compared immediately before the rename; a hand edit that lands between that
  comparison and the rename is lost. No lock: the file is the operator's own, edited by hand too.
- **No version gate on the two `-c` keys of a codex review turn (F10-6, rejected).** `c3 maintain`
  keeps the CLI current; the oldest version exercised is named in the comment on `REVIEW_ISOLATION`.
- **The SessionStart repair stops at the nearest `.git` entry and ignores `GIT_DIR` (F24-3).**
  `c3 doctor --fix` resolves the root through git.
- **`AGENTS.md` follows a numerically higher model id when neither id has a date (F25-2).** With
  dates the follow is refused.
- **The guard's 5 s slack for a child rollout without a readable timestamp (F25-3).** It errs
  toward stopping a turn.
- **A DNS lookup past the deadline leaves its thread to finish (F27-4).** It holds no connection.
- **The whole-entry fingerprint makes a benign concurrent roster edit hold an update (F38-3)** -
  the smoke is "not run", the update is tried again on the next pass.
- **The list-price hold needs a positive price for both ids on the same field (F39-3, F38-5).**
  Without it nothing is held; the date gate and the smoke still apply.
- **The codex smoke asks for switch evidence on every turn of the rollout file (F39-5)** - the
  stricter reading; a resumed thread with an older turn fails the smoke.
- **In test mode the in-use probe is answered by the test hook (F39-6).**
- **Names resolved after validation only classify (F43-4):** canonicalize / metadata calls that
  decide whether to act; every write, content read and removal in the converted paths goes
  through the handles.
- **A collab root outside the repository is followed (F52-3)** - the operator's own anchor.
- **A link on the way to a repair candidate is a note, not a finding** (the coordinator's
  correction of wave R7b): nothing behind it is read, the SessionStart line is silent, `c3 doctor`
  prints `[i] <file>: not checked` and stays clean - a repository whose `AGENTS.md` is a link to
  `CLAUDE.md` is an ordinary one.
- **A READ through a collab resolver into a linked `.c3` / `.eck` is not refused**, and a
  consultation still creates its task directory with `create_dir_all` after `begin_run` checked
  and created the collab root through handles (wave R7b, item 9 partly). Writes through a linked
  folder are refused.
- **A live but hung owner of the move lock blocks the move** (never taken over while its process
  lives); commands go on in `.eck/` and say so.
- **A descendant that left its process group AND was re-parented (a `setsid` daemon) survives the
  deadline stop**; on Windows an already reaped child's tree is not swept when the process table
  cannot be read. Said in the doc comment and the CHANGELOG.
- **`--diff-untracked` fails on Windows for an untracked name that holds U+202E** (Git for Windows
  cannot hash it); the default, tracked changes only, is not affected.
- **The CI Windows job hung twice in the `c3-cli` integration tests without a log** (first
  attempts of `99e2da7` and `0eea5f1`; the reruns were green; not reproduced locally). Since
  `e599f27` every test step has its own time limit, so the next occurrence fails with its log.

## What is NOT covered - say if any of it blocks

- **agy:** no live run in this release at all - the plan is out of quota until 2026-11-28. Its
  worktree isolation and the name-only step classifier (wave R8) ran against fake CLIs only.
- **kimi:** no live run after the rule that a housekeeping file in its working directory fails the
  run; resuming a thread from another directory was verified live for Claude Code only.
- **`c3 maintain`:** no real pass with this build - no catalog swap, no CLI update with a rollback.
  The next pass is due 2026-11-26 and will be watched.
- **The move of `.eck/` to `.c3/`** has run in tests only; this repository's own move happens at
  the install of 0.2.5.
- **http under `--rev`** end to end (unit level only); **a real file system without hard links**
  (a test hook forces that path); **macOS** (CI is Windows and Linux).
- The workers of the last round ran their tests only partly (this machine was short of memory);
  the coordinator ran them on the merged tree instead - see Evidence. Nobody but CI ran the Unix
  halves (the FIFO test, the process-group sweep, `/proc`).
- qwen (alibaba, monthly quota exhausted) and gemini took no part in the reviews.

## Evidence

- On the merged tree of `e599f27`, this machine (Windows), in the foreground: `cargo fmt --all --
  --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `c3` library
  tests 936 passed, `c3-core` 253; `c3-cli` targets `branches` 18, `finish` 27, `ci_check` 14,
  `hook_scripts` 5, `init_doctor` 22, `compat_wave2b` 21, `snapshot_partb` 7,
  `panel_light_required` 5, `mcp_integration` 3, `claude_engine` 19, `kimi_engine` 21 - all passed.
- CI (Windows, Linux, Linux with `index-surreal`): green for `2ffa02a`; the run for `e599f27` and
  the one for the head (the version bump on top of it) were running when this brief was handed
  over - `gh run list --branch main --limit 3` shows them. The tag waits for a green run.
- The head differs from `e599f27` by the version (`0.2.4` -> `0.2.5` in `Cargo.toml`, `Cargo.lock`,
  the two plugin manifests), the CHANGELOG heading and this brief.

## Questions

- **Q1.** Is 0.2.5 safe to tag? A blocker is a defect that damages a machine, a repository, a
  roster or a consultation record, loses or splits a project's records, sends something private
  off the machine, or reports a result that is not true.
- **Q2.** The move of a project's folder (`state_dir.rs`): name any sequence of commands, crashes
  or concurrent processes after which a project has two folders, lost records, or a folder moved
  through a link.
- **Q3.** The handle containment (`contain.rs` and its callers): name any operation in a
  converted path that still acts on a path name after its validation.
- **Q4.** The codex guard and the maintain smoke: can an automatic Codex update still be recorded
  verified without proof that the sub-agent switch held on every smoke turn?
- **Q5.** What leaves the machine: the `--diff` patch, the reviewer prompt's lists of paths,
  telemetry's model names, the catalog fetch. Name any private string that still goes out.
- **Q6.** The last round's fixes (commit `e599f27`): do they do what their CHANGELOG entries say?
  Look hardest at what no test on this machine could run: agy in a worktree, the claim-free patch
  placement, the deadline stop's rounds, the `.c3/layout` marker and the lock rules of the move.
- **Q7.** Anything in "Decided, not fixed" or "NOT covered" that you would not release with.

Answer by number. Cite `path:line` for every finding. For each blocker of the table: closed / not
closed. Keep it under 1200 words.
