# Wave 9 - the four defects of the 2026-10-14 consilium panels

Found in the consilium panels of task `consilium-2026-10-14`; fixed on main for the 0.2.2
candidate. The plugin is the oracle only where it does not contradict a fix: the plugin harness is
run through the shims, and the two places where it pins the old behaviour are kept behind the
shims' compat switch `C3_PLUGIN_COMPAT=1` (D1, D2) - a normal `c3` run never sets it.

## D1 - a panel with a new brief forked the previous threads

Trigger: `c3 consult --panel --panel-all` on a task that already had threads (second brief) forked
every codex member from the first panel's threads (0.8-1.2 MB each); the fork guard
(`CODEX_CONSULT_FORK_MAX_KB`) then refused four members. Rule: a panel defaults to `--mode new` for
every member unless `--mode fork` is given (then the guard applies as before; `--mode resume` stays
refused for a panel); the dry run and the panel header print `mode: new - every member starts a new
thread ...` / `mode: fork - ...`. The plugin forks the newest thread of each lineage by default and
its `harness-roster` (PANEL: "second panel: openai forks the openai thread, mimo the mimo thread")
pins that, so under `C3_PLUGIN_COMPAT=1` (set by `tests/shim/*.ps1`) the mode stays automatic and
the header is the plugin's. Test: `wave9_consilium::a_panel_starts_new_threads_by_default_and_forks_only_when_asked`
(panel B starts a new thread, `--mode fork` forks it, a 1 KB cap refuses the heavy 4 KB parent).

## D2 - the detach "come back" line printed PowerShell commands

Trigger: `codex-consult.ps1 -Task <t> -Status -Id <id>` / `-Wait` in the detach output. Rule: C3's
own lines name `c3`: `c3 consult --task <t> --status --id <id>` and `--wait --id <id>` (the `-List`
line of the SessionStart hook and `c3 findings --list`, the unknown-finding hint
`c3 findings --task <t> --list --all`, every "run codex-providers.ps1 for the full picture" as
`run c3 providers`). New module `c3_core::spelling`. The ledger/handoff FILE formats are untouched
(the handoff's `Invocation: codex-consult.ps1` line stays; the hook's `-Explain` pointer is the
plugin's). Pinned by the plugin harness (`harness-detach` SINGLE "come back  : codex-consult.ps1
-Task t -Status -Id ...", `-List` lines, `harness-roster` refusals), so under
`C3_PLUGIN_COMPAT=1` the old spelling is printed. Test:
`wave9_consilium::detached_single_runs_of_one_task_run_side_by_side_and_speak_c3` (the output has
`c3 consult --task t --status --id` and no `.ps1`).

## D3 - detached single runs of one task serialised on `.consult.lock`

Trigger: four `--detach` single runs of one task: two ended "skipped: not started: another
consultation is running". Rule: a DETACHED single run holds the task lock only for its bookkeeping
(it retries the lock for up to a minute, takes its numbers `n`/`nn` under it, writes a member-style
recovery record `.consult.pending-<NN>.json` and releases it), then runs the reviewer turn without
the lock, as a panel member does under its panel; the commit goes through the write lock
(`commit_wait_ms`) exactly as before, so the ledger numbering stays strictly increasing and
`sessions.json` / `findings.json` are never written concurrently. A live record of another
detached single run (a member-style name without a `parent_pid`) is no obstacle to a detached run
(`Recovery.concurrent`); a foreground run still refuses on it. Only `--detach` changes: a
foreground single run still holds the lock for the whole run (the plugin's behaviour; no harness
test runs two detached singles of one task). Test: `wave9_consilium::detached_single_runs_of_one_task_run_side_by_side_and_speak_c3`
(two detached runs with a 6 s fake turn: their `[when, finished_at]` intervals overlap, distinct `n`).

## D4 - an instant 429 on a plan with a monthly cap was a 10-minute burst

Trigger: alibaba (Qwen) answered `429 Too Many Requests` in 0 s twice while the plan's monthly quota
was at 100 % (reset 2026-10-27). Rule: a roster entry may declare its plan's cap under its
`ext.c3` (the plugin validates `ext` as an object and ignores it): `"ext": {"c3": {"plan_caps":
["monthly"], "reset_hint": "monthly:27T00:00+08:00"}}` (`plan_caps` of `monthly` | `weekly` |
`daily`; `reset_hint` `monthly:<DD>T<HH:MM><offset>`, `weekly:<Mon..Sun>T<HH:MM><offset>` or
`daily:<HH:MM><offset>`). Two instant 429s (a burst-class failure, wall under 2 s, no window text)
within 60 minutes of each other and newer than the endpoint's newest usable reply mark the entry
`unavailable (plan quota: 429 twice in <n> min, no window text; retry after <next reset>)` until the
hint's next reset, else 24 h after the second hit - instead of the 10-minute burst
(`c3_core::plan::plan_cap_verdict`, applied by `Ctx::plan_verdict`, so `c3 providers`, the walk, the
panel and a direct run all see it). An entry without the declaration keeps the 10-minute burst.
Tests: `plan::two_instant_429s_inside_an_hour_mark_a_capped_entry_out_until_the_reset`,
`plan::the_cap_reset_hint_names_the_next_reset`,
`roster::an_entry_declares_its_plan_cap_under_ext_c3`,
`providers::two_instant_429s_on_a_capped_plan_mark_the_entry_out_until_the_reset`.
