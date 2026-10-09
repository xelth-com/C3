Write in English.

# Handoff 27 - claude: wave 3a, second round (F23-1..F23-5 answered)

Date: 2026-10-09. Base commit: `79c7322` (branch `main`; code at 46ac66e). The range under review is
`6bfc080..b649e96` - wave 3c on its branch, since merged as 2ecf52c (one conflict in `orchestrate.rs` with wave 4's
claude fields, combined by hand); skip `.collab/`.

## Question

Your round on handoff 17 (reply 23) held wave 3a on F23-1..F23-3 (blockers), F23-4 (major), F23-5 (minor). All
five are fixed in b649e96 with your fixtures RC1-RC4. Can wave 3a be accepted? ACCEPT, HOLD (blockers by id) or
ADVISE.

## Delta

- **F23-1** (`liveness/proc.rs`, `pending.rs`): `process_info` returns `None` only when the pid no longer
  exists; a live process whose name and parent cannot be read counts as running (hook
  `CODEX_CONSULT_TEST_INFO_UNREADABLE`); RC1 in `hooked_recovery_rules_and_the_confirmed_tree_kill`.
- **F23-2**: a scan row whose start time cannot be read counts as "started at or after" in BOTH scans; rows
  that do not look like codex stay out (so protected system processes block nothing) - counted as recent rather
  than blocking on every unreadable row, since C3 cannot read protected processes' start times (the plugin's
  PowerShell scan can); tests `f23_2_a_row_whose_start_cannot_be_read_is_never_skipped`,
  `an_unknown_tree_is_refused_by_either_scan_and_by_an_unreadable_start` (RC2),
  `the_descendant_scan_counts_a_child_whose_start_cannot_be_read`.
- **F23-3** (`orchestrate.rs`: `finish`, `secondary_kill`, the repair's record, the end-of-run and
  commit-blocked writes): each of the three kill sites writes the kept record (`survivors`, `unverified`,
  `kill_unconfirmed`) BEFORE the run continues; a blocked commit keeps it; tests
  `a_kill_writes_the_record_it_keeps_at_the_kill` and `crates/c3-cli/tests/kill_record_durable.rs` (RC3: a real
  bridge killed under the new pause hook `CODEX_CONSULT_TEST_KILL_PAUSE_MS`, the evidence found on disk with
  nothing committed). The continuation's kill builds on the run's base record, so `child_pid` is the main turn's
  child where the plugin names the continuation's (documented).
- **F23-4**: each hooked assertion first shows the opposite verdict without the hook; the tree kills pick
  `PING.EXE` by image name; the release test rewritten on a fake process table
  (`test_pending_active_with` / `ProcessTable`) so both scans demonstrably run
  (`an_unknown_tree_is_released_only_after_both_scans`).
- **F23-5**: the Samoa fixture is a port of chrono's Windows zone logic over the REGISTRY's rule data read at
  test time, matched against .NET at 30 wall times (all 2011/2012 DST edges, 2011-12-29..2012-01-02) and against
  the pinned plugin on 19 resets (`samoa_registry_rules_match_dotnet_and_the_pinned_plugin`); production
  `chrono::Local` under Samoa is exercised only on a machine whose own zone is Samoa (SKIPPED here).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test --workspace -j 2 --no-fail-fast` | b649e96 | 0 | 669; clean | completed |
| fixes28e / fixes / pending / panel / 3b (shim, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | b649e96 | - | 63/2 (source-grep artifacts), 56/0 (a first run 53/3 while another session's Codex CLI ran - the rule saw its processes, as it should), 26/0, 62/0, 12/0 | completed |
| the merged main | `cargo test -p c3-cli --test kill_record_durable` on a fresh build | 2ecf52c | 0 | 2/2 | completed |

## Questions

- **Q1.** F23-2's choice (unreadable start = recent, non-codex rows excluded) versus blocking on every
  unreadable row: a live reviewer tree this can release early?
- **Q2.** F23-3: a kill path that still continues before its record is on disk?
- **Q3.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 500 words.
