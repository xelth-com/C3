# Handoff 08 - Claude: acceptance of 0.2.4

Date: 2026-11-15. Base: tag `v0.2.3` .. commit `d7ce658` (committed) + one uncommitted wave in the
working tree. `git diff 035d386 d7ce658` is wave 1, `git diff d7ce658` is wave 2 (the working tree).

## Question

Can 0.2.4 be accepted and tagged at the commit that will hold exactly this working tree? Name
anything that must be fixed first.

## Delta since the last review

First review of this task by the standing partner. Stand-in diff-reviews ran on both waves:
handoffs 02-04 (GLM-5.3 ACCEPT, Muse Wonder HOLD, Grok HOLD; 30 findings) and 06-07 (Grok HOLD,
Muse contributor HOLD; 7 findings). All 37 are closed; ids and closing tests below.

- **Wave 1 (`d7ce658`)**: `c3 maintain` (`crates/c3/src/maintain/`) - a weekly detached pass from the
  SessionStart hook that updates the engine CLIs behind a two-turn smoke with rollback and swaps a
  seated model only for a same-major successor that passed the smoke; Grok takes the reply schema
  in the prompt (`crates/c3-core/src/effort.rs`), a read-insist re-ask is a new conversation and a
  run that died in a re-ask is recorded under that re-ask (`crates/c3/src/consult/orchestrate/secondary.rs`,
  `crates/c3/src/engines/grok.rs`); live quota windows for z.ai and Muse (`crates/c3/src/budget/mod.rs`);
  the build commit in `c3 --version` (`crates/c3-cli/build.rs`).
- **Wave 2 (uncommitted)**: reviewer protection. Roster entry `ext.on_request` (never seated
  automatically) and `ext.reserve_percent` (a protected entry is held on non-standing purposes when
  `used% >= min(elapsed% + N, 100 - N)` on a long window; `--spend-reserve` overrides); a bare
  consultation is the standing partner only on acceptance / core-contract, otherwise one routed
  pick; the ChatGPT windows are read from rollouts up to 8 days back and the health record instead
  of being forgotten after two days (`crates/c3/src/providers.rs`, `crates/c3/src/panel/{plan,run}.rs`,
  `crates/c3/src/consult/orchestrate/mod.rs`, `crates/c3-core/src/{roster,purpose,quota}.rs`).

## CURRENT invariants claimed

- Nothing is installed without a passing smoke of the installed version; an update is pinned to the
  checked version; every update and rollback is confirmed by re-reading `--version`; a pending-update
  record written before an install lets the next pass verify or roll back after a crash.
- The pass lock is taken by atomic creation; an engine in use (image name or command line; an
  unreadable process table counts as in use) is skipped; `--dry-run` and `--smoke` leave no state.
- The hook never waits for the pass it starts (its own standard handles are not inherited).
- A keyed HTTP GET never follows a redirect; the keyless kimi download follows only the one listed
  mirror over https and is never run when it reached the size cap; update children get no provider keys.
- The roster is edited only by `maintain/models.rs` (backup once a day, one `model` value, strict id
  shape, re-read before the write, temp file + rename) and never for a protected or on-request entry.
- For grok a zero-tool reply, and a turn with tool calls but no text after the last one, is never
  usable on a non-raw purpose; an abandoned thread is never recorded for a resume.
- The hold never applies on acceptance / core-contract nor to an unprotected entry; with no budget
  data an entry is not held; an old reading may hold but never raises a weight.
- An explicit `--provider` run is never rerouted: it runs or is refused; an on-request entry runs
  only when named exactly.

## Closed findings (id -> closing test; full text in `findings.json`)

| Ids | Closing test or evidence |
|---|---|
| F03-1, F04-4 | `a_turn_that_reads_then_answers_nothing_is_not_usable_under_either_transport` |
| F04-5, F02-5 | `a_dead_reask_with_only_pre_tool_text_clears_the_reply_not_the_placeholder`, `salvage_is_the_text_after_the_last_tool_call` |
| F03-9 | launch-failure arm of `run_read_insist`; no test (the fake cannot fail the re-ask's launch alone) |
| F02-1, F04-7 | `zai_get` / `kimi_get` built with `redirects(0)`; no dedicated test |
| F03-10, F04-10 | `cache_path_is_keyed_by_source_and_plan`, `a_past_reset_live_window_is_dropped` |
| F03-2, F04-3 | `two_racing_lock_takers_only_one_wins`, `a_stale_lock_is_taken_over` |
| F03-3, F04-2, F02-6 | `in_use_detects_name_and_command_line_and_fails_closed_on_a_snapshot_error` |
| F04-1 | `a_pending_update_whose_install_never_took_is_cleared` and its two siblings |
| F02-2, F03-4 | `an_unparsable_installed_version_is_held` |
| F03-5, F04-6 | `the_grok_update_is_pinned_to_the_checked_version`, `an_update_that_does_not_take_is_reported_not_smoked`, `a_rollback_that_does_not_restore_is_reported_failed` |
| F03-6, F04-9 | `model_id_shape_guard`, `replace_refuses_a_hostile_successor_id`, `report_line_sanitises_dynamic_pieces` |
| F03-7 | `dotted_date_stamps_are_never_swapped` |
| F03-8, F02-7 | `commit_swap_aborts_on_a_concurrent_edit_and_writes_atomically` |
| F02-4 | swap smoke goes through `smoke::seat_saturated`; no dedicated test |
| F02-3 | `the_model_step_honours_the_engine_filter` |
| F03-11 | `update_children_do_not_see_provider_secrets`, `the_secret_env_filter` |
| F04-8 | direct `CreateProcessW`; `the_hook_returns_without_waiting_for_the_spawned_pass` (fails without the handle fix); live check |
| F04-11 | `a_truncated_kimi_download_is_never_written_or_run` |
| F03-12 | smoke repository under the OS temp directory; redirect allow-list `a_redirect_stays_on_https_and_on_the_same_site` |
| F06-1 | `a_light_on_request_entry_is_not_seated_as_a_stand_in`, `a_required_held_weighty_entry_on_a_light_purpose_refuses` |
| F06-2, F07-2 | `a_held_entry_is_refused_even_with_skip_preflight` |
| F06-3 | `a_fork_follow_up_continues_the_previous_reviewer_or_draws_when_held` |
| F06-4, F07-1 | `an_old_high_reading_still_holds_and_names_its_age`, `an_old_low_reading_neither_holds_nor_boosts`, `a_future_dated_observation_is_ignored` |
| F07-3 | one network mode per run for hold, draw and explain; no dedicated test |

## Evidence

- Full run on this exact tree (2026-11-15): `cargo test -p c3-core` 282 passed, `cargo test -p c3 --lib`
  644 passed, `cargo test -p c3-cli --no-fail-fast -- --test-threads=1` 196 passed in 20 binaries, 0 failed.
- Live on this machine with the d7ce658 build: `c3 maintain --run --dry-run` named the right versions
  of six CLIs and left no state; `c3 maintain --smoke --engine codex --engine kimi` passed; the hook
  with a due pass returned in 2.1 s while the pass ran 20 s and the next hook printed the report once.
- Live, grok-4.7: with `--json-schema` 3 of 5 fresh conversations ended on a placeholder with no
  tool call; without it 10 of 10 read; in the two stand-in panels Grok read on its first turn.
- NOT exercised live: a real CLI update, a real rollback, the kimi installer run, a real roster
  swap, a real hold on the ChatGPT account (it was reset today; the hold was exercised on a plan
  fed by `c3 budget set`).

## Questions

- **Q1.** ACCEPT, HOLD or REJECT for 0.2.4 as this tree, and the reason.
- **Q2.** `c3 maintain`: is there still a path that leaves an engine worse than it found it, or
  edits the roster wrongly?
- **Q3.** Reviewer protection: can a protected reviewer still be spent on a non-standing purpose
  without `--spend-reserve`, or a reviewer be held or refused wrongly (clock, window, stale data)?
- **Q4.** The grok record: can a placeholder or the wrong attempt still end up as the reply, the
  outcome or the resumable thread?
- **Q5.** What must be observed in the log on the first real update and the first real hold
  (first-run checklist)?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
