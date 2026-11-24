# Handoff 68 - Claude: acceptance of C3 0.2.5, the delta after your third REJECT

Date: 2026-11-24. A NEW thread. You run in a throwaway checkout of the head of `main`.
Scope: `git diff c4a74af HEAD` - one code commit, `808a1bb`, the fix for the ONE blocker and the
three minors of your review of `c4a74af` (handoff 67,
`.collab/release-0.2.5/handoffs/67-codex-acceptance-delta-2.md`: seven rows closed, the two path
limits said not to block the tag, F67-1 blocking) - plus this brief and the coordinator's records.
Everything else is as you read it an hour ago.

**Budget: the account's week is at 93 %; this is the last pass it has.** The thread is replayed
on every tool call: read narrowly (`rg -n`, `sed -n` ranges,
`git diff c4a74af HEAD -- <one file> | head -n ...`), never a whole large file. The diff is five
files; about twenty-five tool calls should do. If you run short, give the verdict on what you
read and say what you did not read.

**Sub-agents are off and guarded. Do not delegate.** `CHANGELOG.md`, section `[0.2.5]` /
`### Fixed`: search `F67-`.

## Your findings and what was done

| Finding | Fix (where) | Closing test | Limit that remains |
|---|---|---|---|
| **F67-1** the ordinary update path credits the target without reading the version after its smoke | the recovery's after-smoke identity check is now ONE function, `changed_during_smoke` (`maintain/mod.rs`), and the ordinary target path of `update_and_verify` calls it after a passing smoke. A release that changed or became unreadable during the smoke gets no success attribution: the line and the state's action are `<engine> <prev> -> <target> unresolved: the installed version changed during the smoke (<target> -> <now or unknown>), so its pass is credited to no version`, the pending record is KEPT, nothing is rolled back (the smoke passed - on an unknown release). The next pass's recovery reads what is installed first and smokes it for itself (the third-version path you closed). The "another release was left instead of the target" case of handoff 65 goes through `handle_pending_smoke`, which has the same check | RC1: `maintain.rs::a_target_replaced_before_its_smoke_is_credited_nothing_and_keeps_the_record` (the fake reports the target to the post-install probe, then its version file switches before the smoke: an unresolved action, the record kept, no `-> 1.0.80 ok` in the output or the state, no rollback; a second run smokes 1.0.81 for itself and clears the record); `a_target_that_becomes_unreadable_during_its_smoke_is_credited_nothing` | a change and a change back inside one smoke is not seen (both paths, as before); the changed release serves unsmoked until the next pass |
| **F67-2** numeric prerelease identifiers beyond u64 sort by text | numeric identifiers are recognised by their digits alone and compared by the count of significant digits (leading zeros stripped), then digit by digit; numbers below words, `rc.2 < rc.10` as before (`maintain/version.rs`) | RC2: `maintain::version::tests::numeric_pre_release_identifiers_compare_numerically_at_any_length` (your two identifiers, both directions) | `rc.007` and `rc.7` tie in the ordering and stay two different releases |
| **F67-3** exchange tests unwrap success where the contract refuses; a first write refused without an exchange | PRODUCTION: a write whose file is not there needs no exchange - it is created by a hard link from its temporary name (fails when a file appeared meanwhile), with or without an exchange; a system without one used to answer "unsupported" before looking at the file (`contain::publish_exchange`). The no-exchange test hook now emulates an unsupported exchange instead of refusing at once. TESTS: the ones that need a real exchange return on a target that has none (`HAS_EXCHANGE`); the Unix twin is compiled only where the exchange exists | `contain::tests::without_an_exchange_the_write_is_refused_and_the_file_is_as_it_was` makes its first write UNDER the hook and asserts `Publication::Created` unconditionally | your RC3 - the suite on a real file system without an exchange - was not run: no such target here or in CI |
| **F67-4** "can swap a directory" is not "has the operator's rights" | the CHANGELOG states the operating assumption instead: a run's later path-based writes need trusted writers on the directories they pass through; a shared checkout, or a temp directory other accounts can write without a sticky bit, is outside what this release promises. The admission-only guarantee stands as written | - | - |

## Evidence

- This machine (Windows), in the foreground, on the tree of `808a1bb` after a cold build: fmt
  clean; clippy `-D warnings` clean; `c3` library tests 968 passed, `c3-core` 257; `c3-cli`
  `maintain` 56 passed. The other `c3-cli` files were not run locally in this round; CI runs them.
- CI: `82d2abe` and `c4a74af` (the head you reviewed) - GREEN on all three jobs, first attempt.
  CI for `808a1bb` was started when this brief was handed over. (You have no network.)
- The F67-1 and F67-2 code was written by a worker in an isolated worktree with a type-check
  only; the numbers above are the coordinator's, on the merged tree.

## Questions

- **Q1.** For each row: closed / not closed, with `path:line` when not closed.
- **Q2.** Does this one commit open anything new that blocks?
- **Q3.** Is 0.2.5 safe to tag at this head, given a green three-job CI run for the tagged
  commit? Say ACCEPT or REJECT.

Answer by number. Cite `path:line` for every finding. Keep it under 500 words.
