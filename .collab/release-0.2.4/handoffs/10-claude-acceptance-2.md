# Handoff 10 - Claude: acceptance of 0.2.4, second pass

Date: 2026-11-16. Base: tag `v0.2.3` .. HEAD of `main` (clean tree). This continues handoff 08 /
reply 09 in a NEW thread: reply 09 was stopped at 332 s by the quota budget before you wrote your
answer. Read `handoffs/08-claude-acceptance.md` for the full scope and the invariants claimed; this
brief is the delta.

## What happened to reply 09

Your run delegated to three sub-agents (`maintain_review`, `budget_protection`, `kimi_roster`)
although the bridge had passed `-c features.multi_agent=false`: on codex-cli 0.169.0 that flag no
longer removes the multi-agent v2 tools. Together with a second run of another session the 5-hour
window went from 0 to 93 % in five minutes. Your findings were recovered from the rollouts (the two
sub-agents that reported, and your own messages) as R1-R7 below; `kimi_roster` never reported.

**This run has sub-agents off (`-c agents.enabled=false`) and a guard that stops the turn at the
first `spawn_agent` call. Do not delegate; review in this thread, and read narrowly - the fixes
below first, then Q0.**

## Delta since reply 09 (`git diff 1a26e0d HEAD`)

| Id | Your finding | Fix | Closing test |
|---|---|---|---|
| R1 | failed install / rollback still cleared the pending record, pass returned success | `update_and_verify` returns `resolved`; the record is cleared only when the target smoked clean or the previous version is verified restored (`maintain/mod.rs`) | `a_failed_installer_that_mutated_keeps_the_pending_record`, `a_failed_rollback_keeps_the_pending_record` |
| R2 | `write_pending` / `set_pending` swallowed write errors | both return `io::Result`; no installer runs when the record was not persisted | `a_pending_record_that_cannot_be_persisted_holds_the_install` |
| R3 | protection read before the smoke, swap basis after it | the roster is re-read right before each candidate's smoke; that snapshot is both the gate and the basis, and `apply_swap` refuses when the file changed since (`maintain/models.rs`) | `apply_swap_aborts_when_the_roster_changed_since_the_pre_smoke_snapshot` |
| R4 | `on_request` + held reserve ran under an exact `--require` | the reserve hold is evaluated before the on-request gate (`providers.rs`) | `a_required_on_request_and_held_entry_refuses_without_spend_reserve` |
| R5 | one aggregate `observed_at` made an old weekly reading look fresh | each window takes its own `primary_observed_at` / `secondary_observed_at` (`budget/mod.rs`) | `a_merged_old_weekly_window_keeps_its_own_age_not_the_aggregate` |
| R6 | a future-dated candidate was merged before validation (open part of F06-4) | future-dated per-window candidates are dropped before the account merge, in both scans (`c3-core/src/quota.rs`) | `a_future_dated_observation_never_evicts_a_valid_account_window` (the selection path, not the helpers) |
| R7a | can kimi read a brief that is only in the caller's tree? | it could not (brief by reference into a HEAD worktree); the brief is now staged into the worktree at its repo-relative path (`engines/kimi.rs`) | `an_uncommitted_brief_is_readable_by_kimi_in_its_worktree` |
| R7b | can a skipped unknown-engine entry shift `#n`, the first entry or `--provider`? | refuted by us: the entry stays in `entries` with its file position and `inactive`, `active_entries()` filters it (`c3-core/src/roster.rs:531, 980, 1005`) | `unknown_engine_is_skipped_not_refused` |

Not adopted from R1/R2: the pass still exits 0 after an unresolved update (`maintain --run` is
documented "exit 0 always"; the report line and the retained record carry the failure). Say if you
hold that this must be an exit status.

**Sub-agent switch (the cause of the stop):** `c3_core::quota::SUBAGENT_OFF` = both
`features.multi_agent=false` and `agents.enabled=false` on every codex turn unless `--codex-config`
turns sub-agents on; `quota::subagent_in_run` on the budget watch stops a main, continuation or
repair turn that spawns one anyway (`StopKind::Subagent`, class `operator`, partial kept). Tests:
`agent_config_passes_both_off_switches_unless_opted_in`, `the_subagent_guard_sees_a_spawn_agent_call`,
`the_subagent_guard_sees_a_child_rollout`, `the_guard_stops_a_run_that_spawns_a_sub_agent_and_records_it_operator`,
`an_opted_in_run_is_not_stopped_by_the_guard`. Proven live only on a GLM model forced to v2 through a
custom codex provider; this run is the first on the ChatGPT provider.

## Evidence

- Full run on this exact tree: the counts are in the prompt of this consultation (they are measured
  after this file is written). `cargo fmt --all -- --check` and clippy `-D warnings` clean.
- GitHub CI for HEAD (ubuntu default + index-surreal, windows): named in the prompt.
- NOT exercised live, as in brief 08: a real CLI update, rollback, kimi installer run, roster swap;
  a real hold on the ChatGPT account.

## Questions

- **Q0.** The part `kimi_roster` never answered (brief 08, Q0): the kimi engine - worktree cleanup,
  what a failed or partial run leaves, how the reply is told from tool output; and R7a's fix.
- **Q1.** ACCEPT, HOLD or REJECT for 0.2.4 at HEAD, and the reason.
- **Q2.** R1-R6: is each one closed by its fix and its test? Name any that is not.
- **Q3.** Anything from brief 08's Q2-Q4 you had found but that is not in R1-R7.
- **Q4.** First-run checklist: what must be seen in the log on the first real update and the first
  real hold.

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
