Write in English.

# Handoff 65 - claude: wave 9 "consilium defects" - diff review before C3 0.2.2

Date: 2026-10-15. Base commit: `c62d81f` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `68774db..c62d81f` - wave 9; skip `.collab/`. The wave note: `docs/port/wave9-consilium-defects.md`
(one paragraph per defect: trigger, rule, test). Also in main since 0.2.1, not under review: the `index-surreal`
feature became opt-in (1246f02, docs only beyond two Cargo defaults).

## Question

Four defects seen while C3 0.2.1 ran two full-roster panels on 2026-10-14. Are the rules below sound and complete
enough for a patch release? ACCEPT (tag v0.2.2), HOLD (blockers by id) or ADVISE.

## Delta

- **D1 - a panel forked old threads by default.** A second panel on a task forked every codex member from the first
  panel's threads (0.8-1.2 MB each); the fork guard then refused four members. Now a panel defaults to `--mode new`
  for every member; `--mode fork` forks and the guard still applies; `--mode resume` stays refused; the dry run and
  the panel header print the mode. The plugin's harness pins fork-by-default for the PowerShell bridge, so the
  shim forwarders set `C3_PLUGIN_COMPAT=1`, which keeps the automatic mode (and the plugin's header) only under the
  shim. Test `wave9_consilium::a_panel_starts_new_threads_by_default_and_forks_only_when_asked`.
- **D2 - C3 printed the PowerShell spellings.** The detach come-back line, the hook/list line and the findings hint
  said `codex-consult.ps1 -Status -Id` etc. Now a `c3_core::spelling` module holds the c3 spellings (`c3 consult
  --task <t> --status --id <id>`, `--wait --id <id>`, `c3 providers`, `c3 findings --task <t> --list --all`); the
  ledger and handoff FILE formats are unchanged; under the shim `C3_PLUGIN_COMPAT=1` restores the `.ps1` texts the
  harness pins. Test: the detach output contains `c3 consult --task` and no `.ps1`.
- **D3 - detached single runs of one task serialised on the task lock** (two of four reruns were "skipped: not
  started"). Now a DETACHED single run holds the task lock only for its bookkeeping: it retries the lock up to 60 s,
  takes its numbers under it, writes a member-style `.consult.pending-<NN>.json`, releases it and runs the reviewer
  turn unlocked; the commit still goes through the write lock, so `n` stays strictly increasing and the stores are
  never written concurrently; another detached run's live record no longer blocks it (`Recovery.concurrent`). A
  foreground single run holds the lock for the whole run as before. Test
  `wave9_consilium::detached_single_runs_of_one_task_run_side_by_side_and_speak_c3` (two detached runs with a 25 s
  fake turn: overlapping `[when, finished_at]`, distinct `n`).
- **D4 - an instant 429 on a monthly-capped plan was a 10-minute burst.** Alibaba's plan at 100 % monthly quota
  answered `429` in 0 s twice; the bridge retried after 10 min. Now a roster entry may declare `ext.c3.plan_caps`
  (`monthly|weekly|daily`) and `ext.c3.reset_hint` (`monthly:27T00:00+08:00`) under the per-entry `ext` object the
  plugin validates as an object only; two instant 429s (wall under 2 s, no window text) within 60 minutes, newer than
  the newest usable reply, mark the entry `unavailable (plan quota: 429 twice in <n> min, no window text; retry after
  <iso>)` until the next reset per the hint, else +24 h; `c3 providers`, the roster walk, a panel and a direct run
  all see it through one `Ctx::plan_verdict`; an entry without the declaration keeps the 10-minute burst. Tests
  `plan::two_instant_429s_inside_an_hour_mark_a_capped_entry_out_until_the_reset`,
  `plan::the_cap_reset_hint_names_the_next_reset`, `roster::an_entry_declares_its_plan_cap_under_ext_c3`,
  `providers::two_instant_429s_on_a_capped_plan_mark_the_entry_out_until_the_reset`.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace` (default features); clippy `--all-targets -D warnings`; fmt | c62d81f | 0 | 823 passed; one known flaky `index::embed` loopback, green alone; clean | completed |
| plugin harness through the shim | `tests/run-all.ps1 -Only harness-0.3`, `harness-roster`, `harness-quota`, `harness-detach` (plugin HEAD tests, Windows PowerShell 5.1) | c62d81f vs 68774db | 0/1 | 0.3 229/0, roster 125/0, quota 57/0 on both builds; detach 50/1 on BOTH builds - CARRY F11-2 (the `starting` record of a detached run with an inline prompt) fails identically on the base, pre-existing, not of this wave | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff 68774db..c62d81f -- crates docs CHANGELOG.md` - read the hunks of
`panel/run.rs`, `consult/detach.rs`, `consult/orchestrate.rs`, `health.rs`/`plan.rs`, `spelling.rs`; at most 6 tool
calls; the verdict even on an incomplete read.

## Questions

- **Q1.** D3: a path where two detached runs of one task still corrupt a store, reuse an `n`, or where recovery
  releases a live run because the other's record is "concurrent"?
- **Q2.** D4: a case where the plan verdict hides an endpoint that is actually usable (a transient 429 pair), or
  keeps one in that is out (a cap without the hint)?
- **Q3.** D1/D2 under `C3_PLUGIN_COMPAT=1`: a risk that the compat switch leaks into a real (non-shim) run?
- **Q4.** Verdict: ACCEPT (tag v0.2.2), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 300 words.
