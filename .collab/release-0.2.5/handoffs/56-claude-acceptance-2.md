# Handoff 56 - Claude: acceptance of C3 0.2.5, second run

Date: 2026-11-24. Scope: `git diff v0.2.4 HEAD` on `main` - 29 commits, about 48 000 added lines
in 148 files outside `.collab/`. A NEW thread. You run in a throwaway checkout of the head of
`main` (the commit this brief is in, version 0.2.5); that tree is the release.

**Your first run (handoff 55) was stopped after four minutes by the bridge's quota budget** - the
5-hour window of the account was nearly full when it started. It left no findings, only one
sentence: "I found two places where the final round still relies on a check before a separate
write. I'm tracing their effects on the move lock and on the patch bytes named in the ledger."
What I did with it is under "Since your first run". This brief replaces brief 54; you need
neither 54 nor the partial 55.

**Budget: one 5-hour window, and the thread is replayed on every tool call.** Read narrowly
(`rg -n`, `sed -n` ranges, `git diff <a> <b> -- <one file> | head`), never a whole large file or
the whole diff. Decide early which areas matter and spend the calls there; about sixty tool calls
should do. If you run short, give the verdict on what you read and say plainly what you did not
read - a clear partial verdict is worth more than a run that is stopped again.

**Sub-agents are off and guarded. Do not delegate.** `CHANGELOG.md` section `[0.2.5]` describes
every item in prose and names the finding ids.

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
`.collab/release-0.2.5/findings.json`: 210 implemented, 14 wontfix, 1 rejected, none open.
`state.md` is the coordinator's log with every decision. **No other model has read the last three
code commits** - `e599f27` (waves R7a, R7b, R8: the fixes of the last review round), `d476520`
and `0f52246` (below). You are their first reader.

## Since your first run

- **The move lock's takeover was compare-then-remove - fixed (`d476520`).** Of two processes that
  judged the same stale record, the first could remove it and create its own lock between the
  second's compare and its removal; the second then removed THAT lock, and both ran the move. The
  compare and the removal - the takeover and the owner's release - now happen under an OS lock on
  the gate file `<lock>.gate` (`takeover_gate`, `crates/c3/src/state_dir.rs`); the OS drops it when
  its holder dies; the gate file is never removed. Test:
  `the_move_lock_is_exclusive_and_a_dead_owners_lock_is_taken_over`.
- **The patch placement without hard links is judge-then-rename - NOT changed, said as a limit.**
  With hard links the publication fails on an existing name and the name is judged again. Without
  them the rename replaces: a FOREIGN file with other bytes that appears at the content-addressed
  name between the comparison and the rename is replaced (a sibling's file has the same bytes).
  The claim protocol that closed this window had races of its own and was removed on purpose in
  wave R8. Say if this limit blocks.
- **Small debts closed in `d476520` (wave D5):** one table of Unicode format characters
  (`crates/c3/src/unicode.rs`); `write_in` sweeps its own temp files older than a day
  (`contain::sweep_stale_temps`, shared with the patch sweep); the "move lock is held" message and
  `c3 doctor` name the owner; an untracked file git cannot read is left out of a
  `--diff-untracked` patch with its reason instead of failing the run; a Claude id spelled with
  dots is matched in its dash form in telemetry (nothing outside the closed list and the published
  set is sent).
- **`0f52246` (wave D6): the http engine under `--rev` is proved end to end**
  (`wave6_live_diffs.rs::an_http_review_under_rev_sends_the_revision_not_the_live_tree`: the real
  binary, a loopback endpoint that records the request). For that, `parse_base_url`
  (`crates/c3-core/src/roster_ext.rs`) accepts plain `http://` ONLY to a loopback host
  (127.0.0.0/8, ::1, `localhost` - the parser's host) and ONLY in test mode
  (`CODEX_CONSULT_TEST_MODE=1`), the rule the telemetry intake already follows. **This is a new
  product exception - judge it.**
- A test fixture named `<U+202E>txt.exe` was quarantined by Windows Defender in the middle of the
  test (by the name alone); that, not Git for Windows, was the "git cannot read a name with
  U+202E" of earlier notes. The fixture is `a<U+202E>b.txt` now.

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
| F19-1 | `--rev` on the http engine sent a pack of the live tree | the pack is built from a throwaway worktree of the revision | `consult::http::tests::an_http_pack_under_rev_is_built_from_the_revision_with_the_patch`, `wave6_live_diffs.rs::an_http_review_under_rev_sends_the_revision_not_the_live_tree` |

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
- **A READ through a collab resolver into a linked `.c3` / `.eck` follows the link**; every write
  through a linked folder is refused, and a consultation creates its task directory with
  `create_dir_all` only after `begin_run` checked and created the collab root through handles.
- **A live owner of the move lock is never displaced, however long it holds it**; commands go on
  in `.eck/` and say so, naming the owner (pid, host, since when).
- **A descendant that left its process group AND was re-parented (a `setsid` daemon) survives the
  deadline stop**; on Windows an already reaped child's tree is not swept when the process table
  cannot be read. Said in the doc comment and the CHANGELOG.
- **"Cannot tell" about a pid holds a record open only while a live process holds that pid**
  (detached-run decisions, kick requests, member recovery records); no age rule there.
- **A NEWER version of a known Claude family than the closed list knows stays `other` in
  telemetry** - an id outside the closed list and the published set is never sent.
- **The rename of `.eck` to `.c3` on Unix could replace an EMPTY directory made at `.c3` in the
  same instant** (`rename_folder`, said in its doc comment); the mover holds the move lock from
  its last re-read of the state through the rename. Say whether a C3 writer can make that
  directory without the lock.
- **The CI Windows job is not steady**: two hangs in the `c3-cli` integration tests without a log
  (first attempts of `99e2da7` and `0eea5f1`), and one failure with a log - `d476520`,
  `claude_engine.rs::a_decomposed_config_dir_inside_an_nfc_repository_refuses_the_endpoint_run`:
  the fake launcher's `claude --version` "did not finish within 15 s" on the runner. Every rerun
  so far was green; never reproduced locally. Since `e599f27` every test step has its own time
  limit, so a hang fails with its log.

## What is NOT covered - say if any of it blocks

- **agy:** no live run in this release at all - the plan is out of quota until 2026-11-28. Its
  worktree isolation and the name-only step classifier (wave R8) ran against fake CLIs only.
- **kimi:** no live run after the rule that a housekeeping file in its working directory fails the
  run; resuming a thread from another directory was verified live for Claude Code only.
- **`c3 maintain`:** no real pass with this build - no catalog swap, no CLI update with a rollback.
  The next pass is due 2026-11-26 and will be watched.
- **The move of `.eck/` to `.c3/`** has run in tests only; this repository's own move happens at
  the install of 0.2.5.
- **A real file system without hard links** (a test hook forces that path); **macOS** (CI is
  Windows and Linux).
- The workers of the last rounds ran their tests only partly (this machine was short of memory);
  the coordinator ran them on the merged tree instead - see Evidence. Nobody but CI ran the Unix
  halves (the FIFO test, the process-group sweep, `/proc`).
- qwen (alibaba, monthly quota exhausted) and gemini took no part in the reviews.

## Evidence

- This machine (Windows), in the foreground, on the merged trees: `cargo fmt --all -- --check`
  clean; `cargo clippy --workspace --all-targets -- -D warnings` clean at `0f52246`; `c3` library
  tests 940 passed, `c3-core` 254; `c3-cli` targets at `e599f27`: `branches` 18, `finish` 27,
  `ci_check` 14, `hook_scripts` 5, `init_doctor` 22, `compat_wave2b` 21, `snapshot_partb` 7,
  `panel_light_required` 5, `mcp_integration` 3, `claude_engine` 19, `kimi_engine` 21; again at
  `d476520`: `init_doctor`, `ci_check`, `telemetry_parity` 12, `snapshot_partb`, `finish`; at
  `0f52246`: `wave6_live_diffs` 6 - all passed.
- CI (Windows, Linux, Linux with `index-surreal`): green for `e599f27` and `adac2ed`; `d476520`
  green on both Linux jobs, its Windows job failed once (the launcher probe above; the rerun was
  running when this brief was handed over); `0f52246` and the head were running. The tag waits for
  a green run of the tagged commit. (You have no network: `gh` is not available to you.)
- The head differs from `0f52246` by this brief, the coordinator's records and one step of the CI
  workflow (the Windows job excludes its build and scratch directories from the virus scanner).

## Questions

- **Q1.** Is 0.2.5 safe to tag? A blocker is a defect that damages a machine, a repository, a
  roster or a consultation record, loses or splits a project's records, sends something private
  off the machine, or reports a result that is not true.
- **Q2.** The move of a project's folder (`state_dir.rs`): name any sequence of commands, crashes
  or concurrent processes after which a project has two folders, lost records, or a folder moved
  through a link. Does the gate close the takeover you were tracing? What was the SECOND place?
- **Q3.** The handle containment (`contain.rs` and its callers): name any operation in a
  converted path that still acts on a path name after its validation.
- **Q4.** The codex guard and the maintain smoke: can an automatic Codex update still be recorded
  verified without proof that the sub-agent switch held on every smoke turn?
- **Q5.** What leaves the machine: the `--diff` patch, the reviewer prompt's lists of paths,
  telemetry's model names, the catalog fetch, and now plain http to a loopback host in test mode.
  Name any private string that still goes out, or any way that exception reaches another host.
- **Q6.** The last three code commits (`e599f27`, `d476520`, `0f52246`): do they do what their
  CHANGELOG entries say? Look hardest at what no test on this machine could run: agy in a
  worktree, the claim-free patch placement, the deadline stop's rounds, the `.c3/layout` marker
  and the lock rules of the move.
- **Q7.** Anything in "Decided, not fixed" or "NOT covered" that you would not release with.

Answer by number. Cite `path:line` for every finding. For each blocker of the table: closed / not
closed. Keep it under 1200 words.
