Write in English.

# Handoff 17 - claude: wave 3a of the compatibility track (recovery hardening) - diff review

Date: 2026-10-09. Base commit: `5b6feb9` (branch `main`; code at 8ff53c4). The range under review is
`6a5f838..1081079` - wave 3a on its branch (954a5e6 code and tests, 1081079 doc), since merged as 8ff53c4; skip
`.collab/`. Plan: `state.md` gap row F (and F06-1 of your round 06).

## Question

Wave 3a ports the plugin's 0.6.0 wave 28e recovery decisions E1, E18, E19, E23, E25 (and E28's unknown-tree
release) and fixes F06-1 (Samoa). Faithful, and is the fail-closed invariant intact? ACCEPT, HOLD (blockers by id)
or ADVISE. (E2/E3/E20/E24/E26 - the not-spooled count, the fold and the marker - are wave 3b, on its branch now.)

## Delta

- **E1 / E18 / E23** (`c3-core/src/engine.rs` `KillCheck` carried by `AttemptOutcome::TimedOut`/`Stopped`;
  `c3-core/src/store.rs` `PendingRecord` gains `unverified[]` after `survivors` and `kill_unconfirmed`;
  `engines/subprocess.rs` `kill_tree_checked` = `Stop-ProcessTreeChecked`; `orchestrate.rs` the three kill sites,
  `summary.rs`): at every kill site the record (state `survivors`) is kept when the kill left survivors,
  unverified pids `{pid, why}`, or an unconfirmed kill naming no pid (`kill_unconfirmed`); with only unverified
  pids the outcome reads `(kill not confirmed: <why>; pid <u> may still run; the next run for this task is
  refused until it exits)`; `-List` names a `kill_unconfirmed` record; the summary line `pending    : recovery
  record kept`. Hooks `CODEX_CONSULT_TEST_SURVIVORS` (at every kill in C3; main turn only in the plugin - one
  test-only warning differs), `_UNVERIFIED`, `_START_UNREADABLE`, `_KILL_DENIED=1` (now stops the root and leaves
  its child as the orphan; before, the root stayed and was recorded as a survivor).
- **E19** (`liveness/pending.rs` `Test-PendingActive` rewritten after the plugin: `test_unverified_process`,
  `test_recorded_process`; `liveness/proc.rs` `pid_identity`, `process_info`, `terminate_pid`,
  `enumerate_processes_checked`; an access-denied start-time read is "unreadable", not "gone"): the re-check
  order - gone -> dropped; start unreadable -> running (fail-closed); started before the record -> dropped; the
  codex rule -> running; command line not readable (empty, `[name]`, a generic runtime with nothing after the
  executable) -> running `command line not readable - counted as running (fail-closed)`; a child of a recorded
  pid -> running; else dropped `not codex`.
- **E23 / E25 / E28:** such a record is released only after a clean by-parent scan AND a clean machine-wide
  scan, panel members included; refused outside Windows and from another host. The kept record is written once
  at the end of the run (the plugin writes at each kill site).
- **F06-1** (`c3-core/src/health.rs`): a candidate whose UTC instant falls in another calendar year than the wall
  time is classified by the zone's local-year rules; the synthetic Windows Samoa 2011/2012 zone (rules from this
  machine's registry) gives `2012-01-02T00:00:00+14:00` as the plugin, IANA `Pacific/Apia` agrees, the Berlin
  F04-2 edges unchanged; documented limit: showing an instant near New Year still uses the UTC-year rules, as
  Windows does.
- `crates/c3/tests/recovery_wave3a.rs` (the hooks plus three real tree kills); `docs/port/wave3a-recovery.md`.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test --workspace -j 2` | 954a5e6 | 0 | 616 (600 + 16); clean | completed |
| harness-fixes28e (shim v0.6.1, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only harness-fixes28e` | 954a5e6 | 1 | 31/34 -> 48/17: RECORD 17/19 green (the 2 left grep the script source - shim artifacts), NOTSPOOLED x13 + MARKER x2 = wave 3b | completed |
| harness-fixes | the same | 954a5e6 | 0 | 54/2 -> 56/0 (E28 x2 green) | completed |
| pending / panel / detach / 3b / fixes27c / format | the same | 954a5e6 | - | 26/0, 62/0, 50/1 (CARRY shim), 12/0, 34/2 (POINTER shim, ZCODE n/a), 37/0 - no regressions | completed |

Known risk (documented): the machine-wide scan counts real Codex sessions on this machine; a Codex run started by
another session inside a harness's few-second window would fail the release checks.

## Open findings

F06-1 `implemented` (954a5e6). Waves 2 (round 2, handoff 15) and 2b (round 2, handoff 16) await your verdicts in
parallel.

## Questions

- **Q1.** E19: a path that drops a process the task's reviewer could be, or blocks forever on a provably
  unrelated one - versus the plugin's order?
- **Q2.** The release rule (both scans clean, record written once at the end): a window in which a record is
  released or never written while a tree still runs?
- **Q3.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 600 words.
