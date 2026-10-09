# Wave 3a of the compatibility track: the recovery hardening of 0.6.0 wave 28e (2026-10-09)

Branch `wave3a-recovery`. The specification is the plugin at `v0.6.1`: CHANGELOG `[0.6.0]` wave 28e
(decisions E1, E18, E19, E23, E25; E28's unknown-tree release), `codex-consult-common.ps1`
(`Stop-ProcessTreeChecked`, `Get-ProcessInfo`, `Get-CommandLineGap`, `New-UnverifiedEntries`,
`Get-KillUnconfirmedWhy`, `Test-UnverifiedProcess`, `Test-RecordedProcess`, `Test-PendingActive`),
`codex-consult.ps1` (the three kill sites, `Format-KillText`, `Add-KillCheck`) and
`codex-findings.ps1` (the `-List` pending line); the oracles are `tests/harness-fixes28e.ps1` RECORD
and `tests/harness-fixes.ps1` E28. Plus astra's F06-1 (the Samoa fixture, handoff 06 of this task).
Not here (wave 3b, the telemetry files another branch is changing): the per-producer not-spooled
count, the `.last` fold and the forgetting marker (E2, E3, E20, E24, E26).

## 1. The kill, confirmed (`Stop-ProcessTreeChecked`; 27c D16, 28b D14, 28c D8, 28e E1/E23)

`engines::subprocess::kill_tree_checked` replaces the bare `taskkill /T` + `kill` (which reported no
survivor, ever): it reads the root's descendants from ONE process-table read, each with the start
time read at the enumeration (`Get-DescendantTree`); runs `taskkill /PID <root> /T /F` (Windows),
stops the root by its handle, then every descendant whose identity is confirmed (`Get-PidIdentity`
alive: the start time read now equals the enumeration's) by its pid; waits for the root (10 s) and
polls the killed pids (100 ms steps, up to 3 s). The result (`c3_core::engine::KillCheck`, carried by
`AttemptOutcome::TimedOut` / `Stopped` as `kill`, beside `survivors`):

- **survivors** - the root while it runs, and descendants alive with their enumerated start time;
- **unverified** - descendants whose start time cannot be read (never killed by pid, never counted
  gone): the kill is not confirmed, `why` = `start time of pid <n>[, <m>] unreadable`;
- **denied enumeration** (`the process table could not be read ...`, or the test hook
  `CODEX_CONSULT_TEST_KILL_DENIED=1`: `process inspection denied (test hook ...)`): taskkill once
  more while the root is there; the root still running - `the children could not be enumerated (<d>)
  and the root process did not exit`; the root gone but taskkill failed (or outside Windows) - `the
  children could not be enumerated (<d>) and taskkill /T /F failed (exit <n>) - the root exited, its
  children may not have`. `CODEX_CONSULT_TEST_KILL_DENIED=1` or `=taskkill` makes taskkill fail
  (exit 1) without running it. Before this wave the denied hook left the ROOT running and recorded it
  as a survivor; now the root is stopped (as the plugin's `$Process.Kill()`), its child is the orphan.

`proc::process_start_iso` (`Get-ProcessStartIso`) now tells "exists but unreadable" from "gone":
`OpenProcess` failing with access denied reads `""` (it read "gone"), and the test hook
`CODEX_CONSULT_TEST_START_UNREADABLE=<pid>[,<pid>]` (test mode only) reads `""` for those pids. The
scan's process table is not hooked (the plugin's scan reads `Win32_Process.CreationDate`).
New helpers: `pid_identity`, `process_info` (`Get-ProcessInfo`: name without `.exe`, command line
with `CODEX_CONSULT_TEST_CMDLINE_UNREADABLE`, start, parent), `terminate_pid`,
`enumerate_processes_checked` (a snapshot that fails or comes back empty is a failed scan).

## 2. What a kill keeps, at the three kill sites (E1, E18, E23)

The main turn (`finish`), the timeout continuation and the format repair (both `secondary_kill`):

- **The main turn's check** (`main_kill_check`): the kill's own, then (E1) the test hook
  `CODEX_CONSULT_TEST_UNVERIFIED=<pid>[,<pid>]` (test mode, main turn only; alive pids join
  `unverified`, the why becomes `start time of pid <all> unreadable`), C3's own
  `CODEX_CONSULT_TEST_KILL_UNCONFIRMED`, and survivors (the `CODEX_CONSULT_TEST_SURVIVORS` hook's
  too) make it unconfirmed.
- **The outcome** (`main_kill_outcome`, `Format-KillText`): survivors - `failed: <stop> (process
  tree killed; <n> processes survived: pid <s>[; <why>; pid <u> may still run]; the next run for this
  task is refused until they exit)`; (E18) only unverified pids - `failed: <stop> (kill not
  confirmed: <why>; pid <u> may still run; the next run for this task is refused until it exits|they
  exit)`; not confirmed, naming no pid - `failed: <stop> (kill not confirmed: <why>; pid <root> may
  still run)`; else `(process tree killed)`. The stall and the kick are told the same way (a kick
  names the kill only when it was not confirmed). A secondary turn: `<stop> <Format-KillText>`.
- **The record** (`kept_kill`): kept in state `survivors` when the kill left survivors OR
  unverified pids OR (E23, `Get-KillUnconfirmedWhy`) was not confirmed with neither - `survivors[]`
  `{pid, start_time, name}` (the name is now the process's, as `New-SurvivorEntries`),
  `unverified[]` `{pid, why}` (`New-UnverifiedEntries`), `kill_unconfirmed: "<why>"` (`the kill was
  not confirmed` when it has none). A secondary turn's kept lists replace the main turn's.
  `PendingRecord` carries `unverified: []` right after `survivors` in every record (the plugin's
  `New-PendingRecord`), `kill_unconfirmed` only when set.
- **Said:** the summary's `pending    : recovery record kept: <path> (state 'survivors'|'launching')`
  (the plugin's `$pendingNote`, which C3 never printed); the warnings `kill not confirmed (<turn>):
  ...` (`Add-KillCheck`, both forms, for the main turn, `timeout continuation`, `format repair`); the
  ledger's `kill_confirmed` false when ANY kill of the run (main, continuation, repair) was not
  confirmed or left survivors (a kick's kill now counts too); no continuation after an unconfirmed
  main kill (`... - pid <root> may still hold the thread`).
- `codex-findings -List`: a record with `kill_unconfirmed` adds `; the kill of that run was not
  confirmed (<why>): its process tree is unknown - the next consultation scans for it and is refused
  while one of it may run (outside Windows: delete this record by hand once none does)`.

## 3. The next run's re-check (`Test-PendingActive`; E1, E19, E23, E25, E28)

`liveness::pending::test_pending_active` is rewritten after the plugin's, in its order:

1. The writer (D1) - unchanged.
2. `reserved` - unchanged.
3. The recorded pids (`child_pid`, `survivors[]`) AND (E1) `unverified[]`, when the state is not
   `launching`: another host - active, naming all of them; each recorded pid through
   `test_recorded_process` (pid + start time - `pid + start time`, `pid reused (start time
   differs)`, `pid reused (now <name>)`; no start time, or unreadable now - the evidence rule
   below), each unverified pid through `test_unverified_process`. A blocking pid judged by the
   evidence rule says why (`pid <n> [<how>]`); an unverified one adds `; at the kill: <why>`; gone
   or dropped ones are named in the line that clears or recovers the record: `codex pid(s) <n>
   [<how>] no longer running` (the `[<how>]` is new: `[gone]`), `unverified pid(s) <n> [<how>]
   dropped`. Only unverified pids alive: `a previous consultation's codex run left a process its
   kill could not verify (<what>): pid <u> [...] - it blocks the task as a survivor does`.
4. **E19, the evidence rule** (`test_unverified_process`), in this order:
   - no such process - dropped, `gone`;
   - its start time still unreadable - running, `its start time still cannot be read - counted as
     running (fail-closed)`;
   - started before the record's `started` - dropped, `started <t>, before that run: not its
     process`;
   - the "looks like codex" rule matches - running, `start time readable now; <rule>`;
   - its command line not readable (`""`, ps's `[name]`, or a generic runtime - node, nodejs, bun,
     deno, cmd, powershell, pwsh, sh, bash, dash, zsh, python, python3 - with nothing after the
     executable) - running, `...; pid <n> runs <name>[ (a generic runtime, no arguments on its
     command line)]; command line not readable - counted as running (fail-closed)`;
   - a child of a recorded pid (the writer, the child, the survivors, the unverified) - running,
     `..., a child of the recorded pid <p> - counted as running`;
   - otherwise - dropped, `start time readable now; pid <n> runs <name>, not codex[ (excluded:
     <what>)]`.
5. **E23 / E25 / E28, the unknown tree** (`kill_unconfirmed`): another host - active; outside
   Windows - active (`this host cannot scan for its processes by parent pid`); no recorded pid -
   active; else the scan by parent pid under the writer, the child, the survivors and the unverified
   pids, THEN the machine-wide "looks like codex" scan - for every record, a panel member's too. A
   failed scan or a find refuses (a panel member's machine-wide find: `a codex-like process runs: pid
   <n> <name> (task not verifiable) - this panel member's unknown tree is released only when no such
   process runs`); both clean - released: `unknown tree after an unconfirmed kill: the scan found no
   codex-like process under pid <pids> since <started> - released (<checks>)`.
6. The descendant scan - unchanged, but the unverified pids are parents too, and a process table
   that cannot be read is a FAILED scan (active: `may have left a codex process running, and the
   check failed`) instead of an empty one.

## 4. F06-1: Windows Samoa across New Year 2012

`ResetZone::wall_offsets` (F04-2's round trip) took the UTC lookup as the authority for every
candidate offset. A Windows zone keeps one rule set per year, and chrono's Windows `Local` (0.4.45)
looks a UTC instant up with the rules of the UTC year while a wall time's classification uses the
local year (as .NET's `TimeZoneInfo`). Windows' `Samoa Standard Time` changes its base offset
between the years (registry Dynamic DST: 2011 Bias 660, DaylightBias -60; 2012 Bias -780,
DaylightBias -60), so 2012-01-01 00:00 at +14:00 (instant 2011-12-31T10:00Z, looked up with the 2011
rules: -10:00) lost its genuine offset; the artificial gap ended at 14:00, and `try again at 12:00
AM` at `2012-01-01T14:01:00+14:00` read `2012-01-01T14:00:00+14:00` (already passed).

**Corrected, not bounded:** a candidate whose round-trip instant `wall - o` falls in ANOTHER calendar
year than `wall` is judged by the zone's own (local-year) classification; within one year the UTC
round trip stays the authority. The F04-2 edges (Berlin's March and October transitions, in all
three zone implementations) are untouched - they never straddle a year. Test
`health::retry_after_tests::samoa_year_boundary_keeps_the_local_years_offset`: a synthetic
`WindowsLikeSamoa2012` zone with the registry's 2011/2012 rules and chrono's two lookups; 2012-01-01
00:00, 05:00, 14:00 and 2012-01-02 00:00 are `Single(+14:00)`, 2011-12-31 00:00 and 20:00
`Single(-10:00)`; the trigger returns `2012-01-02T00:00:00+14:00` (the pinned plugin's result) in it
and in IANA `Pacific/Apia`; the Berlin boundary samples still pass. It fails on the unfixed rule
(`2012-01-01T00:00:00` -> `None`).

What stays bounded: `to_zone_time` (an instant shown in the zone) still uses the UTC-year lookup -
an instant within the hours around New Year in a zone whose rules change between those years is
shown at the offset Windows' own `SystemTimeToTzSpecificLocalTime` gives. The time-only reference
day and every reset instant of the fixture are unaffected.

## Tests

- `consult::orchestrate::kill_record_tests` - the outcome texts of E1/E18/E23 (the harness's
  wording), `Format-KillText`, the `Get-KillUnconfirmedWhy` table (`w1||||the kill was not
  confirmed`), `New-UnverifiedEntries`, the keep decision, `Add-KillCheck`'s two warnings, a
  secondary kill keeping the record.
- `liveness::pending::tests` - the evidence order (gone, before the run, codex-like, read and not
  codex, a child of a recorded pid), `Test-RecordedProcess` (pid + start time, reused, a name, the
  evidence fallback), records: an unverified pid gone (dropped and named), alive and codex-like
  (blocks, `at the kill`), another host's unverified pids and unknown tree, an unknown tree released
  after both scans, refused with a live orphan of the recorded writer (Windows), a survivor without
  a start time read and not codex (Windows).
- `c3_core::store::tests::pending_record_carries_unverified_after_survivors`,
  `c3_core::health::retry_after_tests::samoa_year_boundary_keeps_the_local_years_offset`.
- `crates/c3/tests/recovery_wave3a.rs` - the hooks in one test (START_UNREADABLE and
  CMDLINE_UNREADABLE, honoured in test mode only) and, on Windows, three REAL tree kills of a `.cmd`
  -> `PING.EXE` tree: a clean kill (confirmed), a descendant whose start time cannot be read with
  taskkill denied (left alone, unverified, the why), the enumeration and taskkill denied (the root
  stopped, its child an orphan, the E23 why, nothing named).

## Harnesses through the shim (pinned v0.6.1, 2026-10-09)

Staging as in `harness-shim.md` section 4 (the whole plugin tree of `v0.6.1`, the six shims of
this branch over its scripts), `C3_EXE` a copy of this branch's build (commit `954a5e6`'s code),
the harnesses one at a time under `HARNESS.lock`, host **Windows PowerShell 5.1**. "Before" is the
brief's baseline (RC2 for fixes28e and fixes; wave 2b's for the rest).

| harness | before | after | went green / note |
|---|---|---|---|
| fixes28e | 31 / 34 | **48 / 17** | RECORD 17 of 19: E1 x5 (the mixed kill, still unreadable, started before, gone, live and codex-like), E18 x3, E19 x4, E23 x3, E25 x2 |
| fixes | 54 / 2 | **56 / 0** | E28 x2 (the unknown-tree recovery of the panel member's record: refused while the escaped-quote reviewers live, released with the app servers excluded) |
| pending | 26 / 0 | 26 / 0 | no regression |
| panel | 62 / 0 | 62 / 0 | no regression |
| detach | 50 / 1 | 50 / 1 | no regression (CARRY F11-2, below) |
| 3b | 12 / 0 | 12 / 0 | no regression (PowerShell library only) |
| fixes27c (extra: KILL D16 runs the denied kill) | 34 / 2 | 34 / 2 | no regression |
| format (extra: the repair turn's kill) | 37 / 0 | 37 / 0 | no regression |

### Every remaining failure, classified

- **Shim artifacts (code greps over `codex-consult.ps1`'s source, which is the shim here):**
  fixes28e RECORD "E1 the code" (`New-UnverifiedEntries -Check $turnKill|$mainKill|$repairKill`,
  `Get-TestHookValue 'CODEX_CONSULT_TEST_UNVERIFIED'`; its in-process half - the record's key order
  `...,survivors,unverified,note` - passes) and "E18, E23 the code" (the three `if (... -or
  ...Unverified... -or $...Unconfirmed)` lines, three `Add-Member 'kill_unconfirmed'`). C3's
  equivalent is `kill_record_tests` and the three call sites of `kept_kill` / `secondary_kill`.
  Likewise fixes27c POINTER D15 and detach CARRY F11-2 (as in wave 2b).
- **Wave 3b (not in this wave):** fixes28e NOTSPOOLED x13 (E2 x3, E20 x5, E24 x2, E26 x3) and MARKER
  x2 (E3) - the per-producer not-spooled files, the `.last` fold and the forgetting marker.
- **Not applicable (C3 is Claude Code only):** fixes27c ZCODE D20/D21.
- fixes28e DOCS and ANCHOR: green (the plugin tree is staged since wave 2b; the anchor since wave 1).
- Environment note: the machine-wide scan counts every codex-like process started after a record's
  `started` - a Codex CLI run by another session on this machine during a harness's few-second
  window would refuse the release checks (E23, E25, E28); it did not happen in these runs.

`cargo test --workspace -j 2`: 616 passed, 0 failed (600 + 16). `cargo clippy --workspace
--all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.

## What differs from the plugin

- The `CODEX_CONSULT_TEST_SURVIVORS` hook applies at every kill (C3 adds it inside the turn's kill,
  since wave 2), the plugin's at the main turn only; `CODEX_CONSULT_TEST_UNVERIFIED` is main-turn
  only in both. `Add-KillCheck`'s warning is formed with the hooked survivors (the plugin forms it
  before adding the hook's) - a test-hook-only difference in the warning's wording.
- The kept record is written once, at the end of the run (before the commit, as since M2), with the
  last kill site's lists; the plugin rewrites it at each kill site.
- A kicked continuation or repair keeps C3's own problem text (`stopped by the operator (-Kick)`);
  its kill check is recorded and keeps the record as the plugin's.
- `Get-ProcessInfo`'s name on Linux is `/proc/<pid>/comm` (15 characters at most), .NET's
  `ProcessName` there is the same source.
