# Handoff 12 - Claude: acceptance of 0.2.4, third pass (delta since reply 11)

Date: 2026-11-16. Scope: `git diff 08121e5 HEAD` on `main` (clean tree) - the fixes of your four
findings in reply 11 (`handoffs/11-codex-acceptance-2.md`). A NEW thread: your reply 11 thread sits
at 131 k tokens per call. Everything you closed in reply 11 (R2-R6, the twelve prior findings) stands
unless this delta touches it; read `git diff 08121e5 HEAD --stat` first, then one path at a time.

**Sub-agents are off and guarded, as in pass 2. Do not delegate.**

## Delta

| Id | Your finding | Fix | Closing test (your RC) |
|---|---|---|---|
| F11-1 | recovery returned "resolved" after a failed rollback / with no rollback path, so the record was cleared | `handle_pending` clears only after a passing smoke or a rollback whose command succeeded AND restored the previous version; the other two branches return unresolved and report `pending update unresolved: ...`, never `recovered` (`crates/c3/src/maintain/mod.rs`) | RC1: `a_pending_recovery_whose_rollback_keeps_failing_stays_unresolved` (three forced passes, record kept, seen failing before the fix); `a_pending_recovery_whose_smoke_passes_clears_the_record` (`crates/c3-cli/tests/maintain.rs`) |
| F11-2 | kimi published pre-tool text as a usable, verified reply; salvage kept it | `KimiEvents.final_text` is reset at every tool call and tool result; tools ran + empty tail = a failed turn; the reply and `kimi_salvage` are the tail only (`crates/c3/src/engines/kimi.rs`) | RC2: `a_pre_tool_verdict_with_no_post_tool_answer_is_rejected`, `a_post_tool_answer_is_the_reply_not_the_pre_tool_text` (unit tests on the event stream, not through the fake process) |
| F11-3 | only the primary kimi turn staged the brief | both secondary constructors pass the same `BriefCopy`; a staging failure is a `capability` failure before the reviewer launches, for every turn (`consult/orchestrate/secondary.rs`, `engines/kimi.rs`) | RC3: `the_uncommitted_brief_reaches_the_read_insist_worktree_too` (`crates/c3-cli/tests/kimi_engine.rs`) |
| F11-4 | the guard counted a historical `spawn_agent` call | `rollout_spawn_agent_since`: a call counts only when its line `timestamp` >= the turn's launch time; child-rollout detection unchanged (`crates/c3-core/src/quota.rs`) | RC4: `the_subagent_guard_ignores_a_historical_spawn_call` (unit); `a_resumed_thread_with_an_old_spawn_call_is_not_stopped`, `a_resumed_thread_that_makes_a_new_spawn_is_stopped` (`crates/c3-cli/tests/subagent_guard.rs`) |

F11-4 also changed one older fixture: `the_subagent_guard_stops_a_turn_that_spawns_a_sub_agent`
(`crates/c3/tests/pending_liveness.rs`) wrote its call line undated and before the turn started, so
the bounded guard no longer saw it; the line is now dated a minute after the fixture is written.

Beyond your findings, found while reading the F11-2 fix: in the grok adapter (and the kimi copy of
it) the "tools ran, no answer" failure carried the reviewer's interim text as its first evidence,
so the failure message and its class (`provider_failure_class`) were derived from the reviewer's own
prose - a placeholder mentioning "quota" would have been recorded as a quota failure of the provider.
Both adapters now pass the reason alone (`engines/grok.rs`, test
`tool_calls_then_no_post_tool_text_is_not_a_usable_reply`; the kimi test above asserts the same).

## What is NOT covered - say if any of it blocks

- **F11-1, no rollback path:** no integration test (the fixtures drive grok and codex; only agy
  lacks a rollback); it shares the unresolved return, by inspection only.
- **F11-1, consequence:** while a record is unresolved that engine's update check is skipped, so it
  stays on the failing version until a later smoke passes, a rollback works, or the operator
  installs a working version by hand. The recovery smoke runs only on a due or forced pass.
- **F11-4, fork:** observed on codex-cli 0.169.0 only for resume (every rollout line carries its own
  `timestamp`). No forked rollout was left on this machine to read; if a fork re-stamps the history
  it copies, an old call would stop that fork. A `spawn_agent` line without a parseable timestamp is
  not counted (the child-rollout check is the backstop).
- **F06-4, the open part** (rollout reading returned before the recorded one; provider, not account,
  scope): not changed. The recorded quota is written from the same rollouts of the same Codex home,
  so it cannot be newer than them here, and this machine has one ChatGPT account. It is recorded as
  technical debt.
- As before: no real CLI update, rollback, kimi installer run, roster swap or real hold.

## Evidence

Test counts of the full run on this tree, and the CI run of HEAD, are in the prompt (measured after
this file was written). `cargo fmt --all -- --check` and clippy `-D warnings` clean.

## Questions

- **Q1.** ACCEPT, HOLD or REJECT for 0.2.4 at HEAD, and the reason.
- **Q2.** F11-1..F11-4: is each closed by its fix and its test? Name any that is not.
- **Q3.** Does the open part of F06-4, or any item under "NOT covered", block the tag `v0.2.4`?
- **Q4.** Any defect this delta introduces.

Answer by number. Cite `path:line` for every finding. Keep it under 600 words.
