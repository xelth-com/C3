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
  has its complete lines beyond the recorded bytes counted as new (wave 3d, F24-2: the fold counts
  a last piece without a line end too; `--status` counts the complete ones meanwhile); a SHORTER
  one is another file under that name, folded afresh; a bare name (bytes
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
  <path>)` - the event is dropped: since wave 3d (F24-3) its not-spooled count is not written
  either); under the spool lock a gone owner's marker is
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
  report). With the spool lock busy for 1 s the record is written without a fold; since wave 3f
  (F31-1) it sees no line - `not_spooled_seen` stays the last fold's (the plugin's fallback counts
  every line but the legacy file's as seen; wave 3d's rule, the lines of the files a fold would
  keep, is superseded - see "Wave 3d" and "Wave 3f" below).
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

## Wave 3d: the diff-review fixes F24-1..F24-6 (2026-10-09)

Branch `wave3d-notspooled-fixes`. The MiMo diff-review of this wave (handoff 24 of
`.collab/parity-0.6.1-2026-10-08/`, verdict HOLD) found one blocker, four majors and one minor; each
is fixed with the fixture the reviewer asked for (`RC1`..`RC4`). Where the fix departs from the
plugin it is said under "Differences".

- **F24-1 (blocker) - a line is never "seen" without its file being kept.** The flush without the
  spool lock (busy 1 s) no longer writes `not_spooled_seen = every line but the legacy files'`: it
  writes the lines of the files a fold would KEEP (`notspooled::kept_seen`: live producers - the
  fold's own `seen`; **superseded by wave 3f**, F31-1: no line at all - see "Wave 3f" below); a
  gone producer's, a staged, a legacy and a named file's lines stay unseen,
  `--status` counts them and the next fold counts them ONCE in its note. The same rule closes the
  narrower twin in the fold: a file the fold takes (gone, staged, named) but cannot open this time
  is neither folded nor seen (before: seen, then folded by the next flush). The names an earlier fold
  recorded stay while their files are there (as before). RC1:
  `f24_1_a_flush_without_the_spool_lock_sees_only_the_kept_lines_and_the_next_folds_the_gone_once`
  (the spool lock held through flush 1:
  `not_spooled_seen` 1 = the live line, no fold note, `--status` 3; flush 2: one note `folded 3`);
  unit `f24_1_without_a_fold_only_the_kept_files_lines_are_seen`,
  `f24_1_a_gone_file_the_fold_cannot_open_is_neither_folded_nor_seen`.
- **F24-2 (major) - a named file's tail is counted.** A file `not_spooled_folded[]` names that grew
  beyond its recorded bytes has its lines beyond them counted with a last piece WITHOUT a line end
  too (the file is deleted - its producer is gone, as for any gone file). RC2:
  `f24_2_a_tail_without_a_line_end_appended_to_a_recorded_file_is_counted` (crash 87, an unterminated
  line appended, the restart: notes `folded 2` and `folded 1`, the file gone, `not_spooled_folded`
  `[]`); unit `f24_2_a_recorded_files_tail_without_a_line_end_is_counted`.
- **F24-3 (major) - no count survives a local forget.** The producer (`notspooled::append`, under
  `note_not_spooled`) opens - makes - its own file FIRST and only then checks the deletion
  (`deletion_refusal`: `forget-pending.json` in any phase - pending, confirmed, cleaning - or one that
  cannot be read, and a forgetting marker whose owner lives); refused, it writes nothing and removes
  the file when it made it empty. A check that passes therefore means the file existed before the
  deletion's transaction or marker, and the cleanup lists the files only after writing both - it
  sees the file. Still no lock (fixes28d D4). The cleanup lists the not-spooled files a second time
  after its removals (before the salt and the record): a producer that does not check (the
  plugin's, an older C3) is caught too. TEST HOOK (test mode only, C3's):
  `CODEX_CONSULT_TEST_CLEANUP_GATE=<path>` - after the first listing the cleanup writes
  `<path>.waiting` and waits (60 s at most) for `<path>`. RC3:
  `f24_3_a_count_made_during_a_local_forget_never_survives_it` (paused at the gate: C3's append is
  refused and leaves no file; a file written directly after the listing is removed and named; no
  not-spooled file, marker or transaction left; afterwards a count is written again); unit
  `f24_3_no_count_is_written_while_the_local_data_is_being_deleted`.
- **F24-4 (minor) - `--status` names the latest UNSEEN line.** The seen lines are taken from the
  front of the producers' files the record does not name (each file in its own order - they only
  grow -, the files interleaved by time); every line of a legacy file, a staged generation and
  beyond a named file's bytes is unseen; the latest by time among the rest is named. Exact for one
  producer whatever the clock did, and for several under a clock that only moves forward; the one
  approximation left (the count is exact): a producer already gone at the last flush but not folded
  (a flush without the spool lock) may have its older lines taken for the seen ones. Tests:
  `f24_4_status_names_the_latest_unseen_line` (`not_spooled_seen` 1, a seen 2030 line before a new
  2020 one: the 2020 one), unit `f24_4_the_latest_named_is_an_unseen_line` (also: a new producer
  whose name sorts first; an old legacy line beside newer seen lines).
- **F24-5 (major) - P8 proven for the marker and the production layout.** RC4:
  `f24_5_the_plugin_home_hook_puts_the_marker_at_the_plugin_place` (under the hook: `--status` and
  the sender read `<home>/telemetry-forgetting` - a living owner stops the sender, a gone one is
  removed with its note in `<home>/telemetry-spool/.last`; a living marker at C3's place is NOT read;
  a forget writes its marker with its pid and `start_ticks` at the plugin place (not C3's), refuses
  C3's producer with it, and removes it last with the plugin's not-spooled files and `.last`; C3's
  own `last-flush.json` is never written) and
  `f24_5_without_the_hook_every_file_is_under_c3s_own_root` (test mode on without the variable, and
  the variable with test mode off: the fold's record and files and the forget's marker under
  `<codex home>/c3/telemetry/`; nothing but `c3/` appears under the codex home). The lock contract
  of the shared files: under the hook C3 and the plugin read and write the same not-spooled files,
  marker and `.last` under DIFFERENT locks (C3's sender and spool locks under its root, the plugin's
  `.flush.lock` and `telemetry.lock`); the hook is test-only and the harnesses run the two
  implementations one after the other, never concurrently - a concurrent C3/plugin flush on one home
  is not a supported layout (production never shares these files: P7).
- **F24-6 (major) - the rewrite after the deletes is never swallowed.** Its failure sets the flush's
  warning `<record> could not be rewritten after the fold's deletes (...) - it still names <n>
  deleted file(s); no count depends on those names and the next flush drops them` (printed by
  `--flush`, on a skipped flush too). The names stay: `--status` and the next fold only look at files
  that exist, and the next record keeps only names whose files are there. **E24 does NOT cover every
  recreated name** - a file recreated under a stale name SHORTER than the recorded bytes is folded
  afresh, but one of EQUAL length would be deleted uncounted and a LONGER one counted only beyond the
  bytes. So a name never recurs instead: a producer's name is its pid and exact start ticks (a gone
  process's name cannot come back), and a staged generation now takes a name that is neither on disk
  NOR named by the record (`staged_path`). TEST HOOK (test mode only, C3's):
  `CODEX_CONSULT_TEST_FOLD_REWRITE_FAIL=1`. Tests:
  `f24_6_a_failed_rewrite_after_the_deletes_is_said_and_loses_no_count` (the warning, two stale
  names, `--status` 0; then a new legacy generation, a new gone producer and the stale producer name
  recreated shorter: one note `folded 3 ... of 3`, the names dropped), unit
  `f24_6_a_staged_name_is_neither_on_disk_nor_recorded`.

### Differences from the plugin (added by 3d)

- The fallback's `not_spooled_seen` (F24-1; since 3f, F31-1, the last fold's baseline unchanged)
  and a taken-but-unopened file (neither seen nor folded): the plugin counts those lines as seen.
- A named file's unterminated tail is counted (F24-2); the plugin counts complete lines only there.
- No not-spooled line while a deletion is pending/confirmed/cleaning or a living owner's marker is
  there (F24-3); the plugin still counts an event its marker refused - harness-telemetry FORGET D3
  checks `-Status ... not spooled: 1 event(s)` with a living marker (that check failed before 3d for
  the shim reasons above and still does; it now also differs by design). The cleanup's second
  listing has no plugin counterpart.
- `--status` names the latest unseen line (F24-4); the plugin names the latest of all lines.
- The failed rewrite is said (F24-6); the plugin swallows it. The staged name also avoids the
  record's names.

Workspace: 686 -> 699 tests (`cargo test --workspace -j 2 --no-fail-fast`: 6 unit tests in
`notspooled.rs`, 7 in `notspooled_parity.rs`), `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo fmt --check` clean.

### Harnesses through the shim (3d)

Pinned v0.6.1, Windows PowerShell 5.1, staging as above, `C3_EXE` a copy of this branch's build
(checked to carry 3d's strings), one harness at a time under `HARNESS.lock`.

| harness | 3b (after) | 3d | what changed |
|---|---|---|---|
| fixes28e | 46 / 19 (RECORD 19) | 63 / 2 (RECORD 2) | NOTSPOOLED and MARKER all green; the 2 RECORD failures are outside this wave (recovery) |
| fixes28d | 25 / 2, stops at line 365 | 25 / 2, stops at line 365 | none - the same MARKER D2 halfway and LOCK D3 sender-stuck failures (plugin path by design) |
| telemetry | 47 / 96 | 47 / 96 | none - the same 96 checks by name as the same-day baseline run `run-all-20261009-091435` (diffed line by line); FORGET D3 keeps failing (shim) and now also differs by design (F24-3) |

## Wave 3f: the second-round blocker F31-1 (2026-10-09)

Branch `wave3f-fallback`. The MiMo second round on 3b/3d (handoff 31 of
`.collab/parity-0.6.1-2026-10-08/`, verdict HOLD) found one blocker, F31-1, which supersedes F24-1:
the flush without the spool lock recorded a LIVE producer's lines as seen; when that producer exited
before the next fold, the fold counted its whole file in its note - the same N lines were
`not_spooled_seen` N in one record and `folded N` in the next.

**The rule chosen: a flush without the spool lock counts nothing.** Its record carries the last
fold's `not_spooled_seen` unchanged (`notspooled::carried_seen`; no record or no number in it: 0)
and, as before, the names of `not_spooled_folded[]` whose files are there; it reads no not-spooled
file. Only a fold - under the spool lock - moves the baseline, so:

- a line appended since the last fold stays unseen (`--status` counts it) until a fold keeps its
  file (seen then) or takes it (counted ONCE, in that fold's note);
- a line the last fold saw stays seen (3d's rule also turned a kept producer's lines back into
  unseen ones when that producer was gone by the flush without the lock);
- the flush without the lock adds nothing to the notes and nothing to the baseline under every
  interleaving: a producer appending during it (it reads no file), the producer exiting between it
  and the fold (none of its lines was seen there: the fold counts the whole file once), the fold's
  own crash hooks (87/88 - the E20/E24 path is untouched; a flush without the lock after a crashed
  fold carries that fold's baseline and its names, and the restart deletes the named files
  uncounted).

Why not the other remedy, `{name, bytes}` per live file: `not_spooled_folded[]` means "in a fold's
note already" (E24). A live producer's entry there would make the next fold delete those bytes
UNCOUNTED - the lines would reach no note, only a `not_spooled_seen` the next record overwrites;
`--status` would subtract them twice (a named file counts only beyond its bytes, and the baseline
again) unless the baseline left them out; a fold under the lock would TAKE the live producer's file
(`foldable` takes every named file: opened exclusively and deleted while its producer still
appends); and under the plugin-home hook (P8) the plugin's fold reads the entry the same way. A
separate per-file seen list would be a record field the plugin does not know. Counting nothing needs
no new field and keeps the record in the plugin's shape.

What `not_spooled_seen` is, stated once: `--status`'s baseline - the complete lines of the files the
LAST FOLD kept -, not a count of its own. The count of record is the fold notes, and every line
reaches exactly one note. (A live producer a FOLD keeps is seen by that fold and counted in a later
fold's note once it is gone - E2's design, unchanged; the flush without the lock no longer adds a
second "seen" of its own.)

Fixtures - the reviewer's RC1 with a real producer PROCESS (`f31_child_producer`, stepped through
its batches by the test), so the producer is live at the flush without the lock and gone at the fold:

- `f31_1_a_producer_live_at_a_flush_without_the_spool_lock_and_gone_at_the_fold_is_counted_once`:
  N = 3 lines at a flush with `spool.lock` held - its record `not_spooled_seen` 0, no `{name,
  bytes}`, no fold note, `--status` 3; the producer exits; the fold under the lock - one note `folded
  3 not-spooled line(s) of 1 gone producer(s)`, `not_spooled_seen` 0, the file gone, `--status` 0;
  what the first flush saw plus every fold note = exactly 3. With M = 2 lines appended between that
  flush and the fold: 5. With crash 87 at the fold (the record saved, the file there and named with
  its bytes, `--status` 0) and its restart, and with crash 88 and its restart: still one note of 5,
  5 in total.
- `f31_1_a_flush_without_the_spool_lock_keeps_the_last_folds_baseline`: a fold keeps the live
  producer's 2 lines (`not_spooled_seen` 2); one line later the flush without the lock records 2 -
  not 3 (3d's rule), not 0 - and `--status` counts 1 and names it; one more line, the producer
  exits, the next fold: one note `folded 4`.
- The 3d fixture `f24_1_...` is renamed
  `f24_1_a_flush_without_the_spool_lock_sees_nothing_and_the_next_folds_the_gone_once` (after the
  flush without the lock: `not_spooled_seen` 0, `--status` 4; then `folded 3`, `not_spooled_seen` 1);
  the unit test `f24_1_without_a_fold_only_the_kept_files_lines_are_seen` is replaced by
  `f31_1_without_a_fold_the_last_folds_baseline_is_carried_and_no_line_is_seen`.

### Differences from the plugin (changed by 3f)

- The flush without the lock: the plugin writes `not_spooled_seen` = every line but the legacy
  file's; C3 carries the last fold's (3d: the lines of the files a fold would keep). The field's
  shape is unchanged - under the plugin-home hook either implementation reads the other's record.

Workspace: 717 -> 720 tests (`cargo test --workspace -j 2 --no-fail-fast`, on freshly touched sources
- the shared target dir otherwise reuses another worktree's newer artifacts: 1 unit test replaced, 2
fixtures and the producer process added in `notspooled_parity.rs`), `cargo clippy --workspace
--all-targets -- -D warnings` and `cargo fmt --check` clean.

### Harnesses through the shim (3f)

Pinned v0.6.1, Windows PowerShell 5.1, staging as above, `C3_EXE` a copy of this branch's build -
checked to carry 3f's rule (a record with `not_spooled_seen` 7, no not-spooled file, a flush with
`spool.lock` held: 7 carried; 3d's build writes 0) -, one harness at a time under `HARNESS.lock`.
The harnesses cannot hold C3's spool lock (it is under C3's own root), so no harness check reaches
the changed path; they guard against regressions.

| harness | 3d | 3f | what changed |
|---|---|---|---|
| fixes28e | 63 / 2 (RECORD 2) | 63 / 2 (RECORD 2) | none - NOTSPOOLED (22) and MARKER (6) all green; the 2 RECORD failures are the recovery code greps over the shim (outside this wave) |
| telemetry | 47 / 96 | 47 / 96 | none - the same 96 checks by name as the baseline run `run-all-20261009-091435` (diffed line by line) |
