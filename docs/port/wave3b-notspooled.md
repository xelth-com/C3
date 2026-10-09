# Wave 3b of the 0.6.1 parity: the not-spooled count, its fold and the forgetting marker (2026-10-09)

Branch `wave3b-notspooled`. Gap row G of `.collab/parity-0.6.1-2026-10-08/state.md`: the plugin's
0.6.0 wave 28e decisions **E2, E3, E20, E24, E26** (CHANGELOG `[0.6.0]`; handoff 28 of the plugin's
`claude-engine-2026-09-30`). The specification is the plugin at `v0.6.1`:
`codex-consult-common.ps1` (`Add-TelemetryNotSpooled`, `Get-TelemetryNotSpooledFiles`,
`Get-TelemetryNotSpooled`, `Get-TelemetryFoldedMap`, `Merge-TelemetryNotSpooled`,
`Complete-TelemetryNotSpooledFold`, the end of `Invoke-TelemetryFlush`, `Add-TelemetryLastNote`,
`Get-TelemetryForgettingOwner`, `Resolve-TelemetryForgetting`, `Invoke-TelemetryForget`),
`codex-consult-detached.ps1` (`Get-ProcessStartTicks`, `Get-PidIdentityTicks`) and
`codex-telemetry.ps1 -Status`.

## Where (P7)

Everything stays under C3's own telemetry root `<codex home>/c3/telemetry/` with the plugin's file
names and shapes:

| file | what |
|---|---|
| `telemetry-not-spooled-<pid>-<start ticks>.ndjson` | E2: one per producer process, one `{time, why}` line per event it could not spool; appended WITHOUT any lock (no other process appends to it) |
| `not-spooled.ndjson`, `telemetry-not-spooled.ndjson` | the legacy single files (C3's wave 2, the plugin's before its 28e): counted whole, staged and folded |
| `telemetry-not-spooled-legacy-<utc ticks>.ndjson` | E26: a legacy file's staged generation (no producer writes to one) |
| `last-flush.json` | C3's `.last`, now in the plugin's shape `{time, result, delivered, kept, dropped, rejected, http, not_spooled_seen, not_spooled_folded, notes}` (the field list is in `c3 telemetry --help` under `--flush`) |
| `telemetry-forgetting` | E3: the marker a local deletion holds `{pid, start_time, start_ticks, since}` |

`<start ticks>` is the process's start time in .NET ticks (100 ns since 0001-01-01 UTC) - read from
the kernel's creation `FILETIME` through `liveness::proc::process_start_iso`, so a file a
PowerShell producer names and one C3 names compare exactly. Code: `crates/c3/src/telemetry/notspooled.rs`
(new), `spool.rs` (the flush's record and the producers' marker check), `complaint.rs` (the
forget's marker and cleanup), `cli/telemetry.rs` (`--status`, `--flush`).

## The rules as implemented

- **E2 - the count.** `note_not_spooled` appends to this process's own file (`notspooled::append`,
  retried up to 1 s, never a lock; `note_not_spooled_checked` returns why a line was not written).
  `--status` sums the complete lines of every producer's file, the legacy files and the staged
  generations, less the record's `not_spooled_seen` - `not spooled: <n> event(s) since the last flush
  - the latest <time>: <why> (<total> line(s) in <files> file(s), one per producer - ...)`. A partial
  last line and a name that is not a producer's are not counted.
- **E2 - the fold.** A flush that holds the sender lock ends, under the spool lock (THE producers'
  lock, which every writer of the record holds), with the fold: the files of GONE producers (no
  process with that pid and those start ticks: a dead pid, a reused pid) and the staged legacy files
  go into ONE note `<time> folded <n> not-spooled line(s) of <m> gone producer(s)` and are removed; a
  live producer's file - or one whose identity cannot be confirmed - stays, its complete lines
  recorded as `not_spooled_seen`. A gone producer's last line without a line end counts too.
- **E20 - save before delete.** The fold order:
  1. stage the legacy files (E26), then open every gone file EXCLUSIVELY (read/write, sharing only
     delete - an appender waits) and count it; nothing is deleted;
  2. SAVE the record: the fold's note, the new `not_spooled_seen`, `not_spooled_folded[]` = `{name,
     bytes}` of every file the fold covers (its length under the handle);
  3. only then delete the files under the held handles;
  4. write the record once more without the names of the files now gone.
  A file `not_spooled_folded[]` already names (a crash between 2 and 3) is deleted without being
  counted again, and `--status` leaves it out meanwhile. A record that cannot be written (read-only)
  folds nothing: the files and the old baseline stay and `--flush` says `; warning: <record> could not
  be written (...) - nothing was folded: the not-spooled files and the last baseline stay, the next
  flush counts them`. TEST HOOK (test mode only): `CODEX_CONSULT_TEST_FOLD_CRASH=1` exits 87 between
  2 and 3, `=2` exits 88 between 3 and 4.
- **E24 - `{name, bytes}`.** A named file of the recorded length is deleted uncounted; a LONGER one
  has its complete lines beyond the recorded bytes counted as new (and `--status` counts them
  meanwhile); a SHORTER one is another file under that name, folded afresh; a bare name (bytes
  unknown) is deleted without counting; a named file that cannot be opened stays named.
- **E26 - legacy staging.** A legacy file is never counted under its own name: it is renamed first to
  `telemetry-not-spooled-legacy-<utc ticks>.ndjson` (a unique name, retried about 1 s), so `{name,
  bytes}` names one generation exactly and an older writer that recreates the legacy name writes a
  new generation the next fold stages. A legacy file a writer holds is skipped with a note `the legacy
  not-spooled file <path> could not be staged (...) - not folded this flush` (its lines are never
  "seen" - they count until staged). An entry naming the legacy file itself (an earlier build) is
  ignored.
- **E3 - the marker.** A local deletion (`--forget --local`, `c3 forget-me`, a resumed cleanup)
  writes `telemetry-forgetting` `{pid, start_time, start_ticks, since}` once it holds the sender lock
  and the spool lock, and removes it LAST (a drop guard, on every path). A marker is judged on its
  pid AND `start_ticks` when it has them (`Identity::Alive` only on exactly equal ticks on Windows -
  within a second elsewhere; a start that cannot be read is `Unknown` = alive, never removed on a
  guess), an older marker without them by its `start_time`, a marker that names no pid is not alive.
  A producer that meets a living owner's marker refuses AT ONCE (checked before every lock attempt:
  `c3 telemetry --forget --local is deleting the local telemetry data (pid <n>, since <t>; the marker
  <path>)` - the event is counted as not spooled); under the spool lock a gone owner's marker is
  removed with the note `removed the forgetting marker of pid <n> (gone) since <t> - a -Forget -Local
  that did not finish; ...` and the producer goes on. The sender does the same (a living owner:
  `skipped - ...`, nothing sent). `--status`: `forgetting : the marker <path> is there - its owner pid
  <n> lives (...)` / `... is gone (...): the next event or sender removes it`. TEST HOOK (test mode
  only): `CODEX_CONSULT_TEST_START_UNREADABLE=<pid>[,<pid>]`.
- **Forget.** `--forget --local` removes every not-spooled file (each producer's - a live one too -,
  the legacy files, the staged generations) and the record, named in `removed locally - ...`; a
  foreign name (`telemetry-not-spooled-notes.ndjson`) stays.
- **`--status`** prints the record's notes (`note       : <note>`), the marker line, and `last flush :
  never` for a record without a result (one only a note made).

C3 keeps what was C3's: the outbox rule, the sender lock and the spool lock (OS locks, never
stale), the deletion transaction `forget-pending.json` (F09-2/F09-3/F14-1) - the marker is in
addition to it; an interrupted cleanup is still blocked by the transaction record.

## The plugin-home test hook

`C3_TEST_TELEMETRY_PLUGIN_HOME=<codex home>` (honoured in TEST MODE only) puts the not-spooled
files, the marker and the record at the plugin's places (`<home>/telemetry-not-spooled-*.ndjson`,
`<home>/telemetry-forgetting`, `<home>/telemetry-spool/.last`); the `codex-telemetry`, `codex-consult`
and `codex-findings` shims set it to `CODEX_HOME` when test mode is on. This is how the plugin's
harnesses verify C3's count, fold and marker: the harness writes and reads those files with the
plugin's own library in-process and runs C3 through the shims on the same files - C3's record must be
one the plugin's `Invoke-TelemetryFlush` accepts and vice versa. C3's outbox, salt, references,
locks and deletion transaction stay under `<codex home>/c3/telemetry/` (P7); production never sets
the variable (without test mode it is ignored).

## Differences from the plugin

- The record is `last-flush.json` under C3's root (the plugin's `<home>/telemetry-spool/.last`); its
  `result` keeps C3's wording (`done`/`nothing to send - dropped N, kept M`, `skipped - <why>`);
  `dropped` counts the discarded (unclosable) events too; `rejected` is always `[]` (C3's sender has
  no per-event 400 handling - an older gap, see wave 2's sender-parity note).
- C3's own wave-2 `not-spooled.ndjson` is a second legacy name (staged and folded like the plugin's).
- The fold runs under C3's sender lock and spool lock (the plugin: its `.flush.lock` record and its
  `telemetry.lock`). A sender that finds the sender lock busy writes NO record (the plugin: none
  either, but a "sender stuck" note - C3's OS lock is never stale, so there is no stuck sender to
  report). With the spool lock busy for 1 s the record is written without a fold and every line but
  the legacy files' counts as seen (the plugin's fallback).
- The marker's refusal and the status line name `c3 telemetry --forget --local`, not
  `codex-telemetry.ps1`.
- A forget writes the marker only when it may delete local data (as the plugin's `-Local`); a forget
  of the intake's data alone holds the two locks (F09-2) without a marker.
- The consultation's console warning for an event not spooled stays C3's (`... - counted (c3
  telemetry --status)`; the plugin's run says `- dropped` for a marker refusal) - that line is in
  `consult/orchestrate.rs`, outside this wave.

## Tests

- `crates/c3/src/telemetry/notspooled.rs` unit tests (6): .NET ticks of a round-trip time, this
  process alive on its ticks and gone one tick off / unknown without ticks / gone for a pid that does
  not run, the file list (producers, both legacy names, staged; foreign and out-of-range names
  ignored), complete lines with and without the tail, the folded map (entries, bare names, bad
  bytes), the marker on pid and ticks / an older marker by `start_time` / garbage.
- `crates/c3-cli/tests/notspooled_parity.rs` (11, Windows; the real binary, this test process as the
  LIVE producer) - the mirror of harness-fixes28e NOTSPOOLED/MARKER and harness-fixes28d D4/D2:
  E2 status sums 7 lines in 4 files / the fold of 3 gone producers / the record's field list / a
  line after the flush / C3's legacy name / the count written while the spool lock is held; two
  producer PROCESSES appending 40 lines each at a barrier (two files, 80 lines, folded once both
  exited); `--forget --local` removes every file but a foreign name, no marker left; E20 crash 87
  (record saved, both files there, the legacy name free), `--status` meanwhile, the restart deletes
  without recounting, a read-only record folds nothing and warns, writable again folds; E24 a legacy
  line appended after the crash counted once, entries of an earlier build (the legacy name shorter /
  same length, a bare name), a shorter file under a recorded name folded afresh; E26 crash 88 and the
  legacy name recreated with equal (a) and longer (b) contents, a legacy file a writer holds skipped
  with a note; E3 the marker: wrong ticks gone (status, a producer removes it with the note), right
  ticks refuse at once (status lives, the sender sends nothing), start unreadable alive, an older
  marker by `start_time`, pid 999999 gone, the sender removes a gone marker; the forget's own marker
  with `start_ticks` seen during its cleanup and removed last; the test hook honoured in test mode
  only.

Workspace: 614 -> 631 tests (`cargo test --workspace -j 2`), clippy `-D warnings` and `cargo fmt
--check` clean.

## Harnesses through the shim (pinned v0.6.1, Windows PowerShell 5.1)

Staging as in `harness-shim.md` section 4 (the plugin tree of the tag, the six shims of this branch),
`C3_EXE` a copy of the branch's build, one harness at a time under `HARNESS.lock`. Before = main
fda5af0 with main's shims, same day, same host.

| harness | before | after | what changed |
|---|---|---|---|
| fixes28e | 31 / 34 (RECORD 19, NOTSPOOLED 13, MARKER 2) | 46 / 19 (RECORD 19) | NOTSPOOLED and MARKER all green |
| fixes28d | 23 / 4, stops at line 365 | 25 / 2, stops at line 365 | MARKER D2 `-Status` names the owner, NOTSPOOLED D4 green |
| telemetry | 47 / 96 | 47 / 96 | the same 96 checks (diffed line by line) |

### Every remaining failure, classified

- **fixes28e RECORD x19** - wave 3a's (recovery E1/E18/E19/E23/E25, branch `wave3a-recovery`).
- **fixes28d MARKER D2 "-Forget -Local whose deletion fails halfway"** - plugin path by design (P7):
  the file held open is the PLUGIN's spool (`<home>/telemetry-spool/2026-09-29.ndjson`); C3's forget
  never deletes the plugin's outbox, so it finishes (exit 0) where the plugin's stops.
- **fixes28d LOCK D3 "sender stuck"** - plugin path by design: the `sender     : sender stuck since`
  line reads the plugin's `.flush.lock` owner record; C3's sender lock is an OS lock (never stale, no
  record). The note part passes (C3's `--status` prints the record's notes).
- **fixes28d stops at line 365** - older, structural (RC2): the KILL section extracts `Add-KillCheck`
  from the real `codex-consult.ps1` source, which the shim replaces.
- **telemetry, plugin path by design (P7) - 83:** SPOOL x16 (incl. D6/D7, which wave 2 listed as
  wave 3: they hold the PLUGIN's spool file, C3's producer never meets it), RATE x14, BACKFILL x17,
  STATUS x2, FORGET x7, NOTICE, SEND x2, ENV, and the sender checks R429 x2, NONJSON x3, DROP, LOCK x2,
  BATCH, D8 x5, D2 x4, D4 x3, D5, URL - the plugin's outbox, salt, instance id and sender.
- **telemetry FORGET D3 (a living owner's marker)** - shim artifact plus one still-open wording: the
  consult shim forces `CODEX_CONSULT_TELEMETRY=off` when unset (its safety net), so the run spools
  nothing and says nothing; and C3's run warning ends `- counted (c3 telemetry --status)` where the
  check wants `- dropped` (`consult/orchestrate.rs`, wave 3a's file - not touched). The `--status`
  owner line and `--forget --local` removing the marker are covered by the Rust tests.
- **telemetry DRYRUN** - shim artifact (the same safety net: the dry run can only say off).
- **telemetry COMPLAIN x10** - older gap (C3's complaint payload/flags, wave 2's note).
- **telemetry DOCS** - documentation (the plugin's README text).
