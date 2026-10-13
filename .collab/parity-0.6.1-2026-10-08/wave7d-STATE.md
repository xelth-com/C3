# Wave 7d - metadata fail-closed and the sweep (handoff 42, F42-1)

Branch `wave-7d-metadata-fail-closed` from main 90c1bfc. Worker: Opus.

## Done
- F42-1: `liveness::pending::read_pending_file` - only `NotFound` (of the metadata lookup or of the
  read) is absent; any other error is the inspection failure `the recovery record <path> cannot be
  read (<error>): the task is refused until it can be read` (`record_read_refusal`; `exists: true`,
  no record). Empty / not UTF-8 / unparsable / not an object / unknown state stay unusable. Test
  hook `CODEX_CONSULT_TEST_COPY_META_FAIL=denied` (`PermissionDenied` at the metadata lookup of a
  file in `<codex home>/c3/pending-kill`; `read_pending_file_as` for units).
- The task directory's own listing (`pending_paths_with`): `NotFound` = no records, any other open
  or entry error = `the recovery records under <dir> cannot be listed (<error>): the task is
  refused until they can be read` (`record_listing_refusal`) in `PendingPaths::error`.
- Readers: consult recovery (real + dry), panel (both passes) via `recovery::assess`;
  `read_task_pending_records` (status change, rating); `-List` (error before absence); kick
  (`kick_record`: every record read, any error refuses, exit 1; a task directory metadata error is
  named). Panel unused-record pass and member record state: an unreadable record is kept, named and
  reported `unreadable`. `before_kill`: a record that cannot be inspected / read back is not
  replaced (the pre-kill record goes to the fallback, then the copy). `member_early_accept` /
  `member_accept`: read/parse failures refuse the member before launch (NotFound unchanged).
- Tests: RC1 = `kill_record_durable::rc1_f38_a_helper_below_a_dead_intermediate_is_refused_from_the_codex_home_copy`
  (third phase `CODEX_CONSULT_TEST_COPY_META_FAIL=denied`: run and dry run exit 1 with the refusal
  naming the copy, no turn marker, helper alive, copy + launch record kept; `-List` `pending:`;
  kick exit 1; then restore -> refusal naming the helper -> helper gone -> copy consumed, turn
  runs). Units: `pending::tests::a_recovery_record_that_cannot_be_inspected_is_never_absent`,
  `kick::tests::a_kick_refuses_on_a_record_it_cannot_inspect`.
- Targeted: c3 lib (pending, kick, recovery, kill_record, subprocess a_kick) 36/36; RC1 1/1 with
  `--nocapture` (`%TEMP%/c3-wave7d/rc1-nocapture.log`): no "release not shown".
- clippy `--workspace --all-targets -j 2 -D warnings`, `cargo fmt --check`: clean.
- Full suite `cargo test -j 2 --no-fail-fast --workspace` (`%TEMP%/c3-wave7d/test.log`): 795 tests
  (793 + 2: c3 lib 421 -> 423; kill_record_durable stays 12, RC1 extended), **794 passed**, 1
  failed = the known flaky `index::embed::tests::embed_reaches_a_loopback_fake_via_localhost`,
  green on its rerun (`index::embed` 6/6, `%TEMP%/c3-wave7d/embed-rerun.log`).
- Harnesses through the shim (binary copy `%TEMP%/c3-wave7d/c3.exe` of target/debug/c3.exe 23:22,
  the wave-7 pinned v0.6.1 tree `%TEMP%/c3-wave7/pin` with the current `tests/shim`, driver
  `%TEMP%/c3-wave7d/drive.ps1`, HARNESS.lock per harness, Windows PowerShell 5.1):
  **harness-pending 26/0** (112 s), **harness-fixes 56/0** (195 s); logs `%TEMP%/c3-wave7d/logs/`.
- Docs: `docs/port/wave7-acceptance-fixes.md` "7d" (with the sweep table), CHANGELOG 0.2.0 line.

## The sweep
Non-test code; patterns `.exists()` `.is_file()` `.is_dir()` `.ok()` `unwrap_or_default()`
`unwrap_or(Vec::new())` `if let Ok(` `.flatten()` `filter_map(Result::ok)` `metadata(..).is_ok()`,
plus the readers of `read_pending_file` found by reading ("read"). Line numbers at main 90c1bfc.
E = recovery evidence.

### liveness/pending.rs - 17 hits, 7 fixed, 10 left
- 105 `if let Ok(read_dir(task_dir))`, 106 `rd.flatten()` - E (the task directory's listing) -
  FIXED.
- 261 `fs::read(copy).ok()`, 262 parse `.ok()` (copy attribution) - E - FIXED (explicit: an
  unreadable/unparsable copy is attributed by its name `<task>-<uuid>.json`, listed, then refused by
  `read_pending_file`; a non-UUID name is never a C3 copy of this task - C3 always names copies
  with a UUID, so every copy is listed for its own task).
- 289 `if let (Ok, Ok) = canonicalize` (`same_dir`) - E (attribution) - FIXED (a non-NotFound
  resolve failure = the same directory, fail-closed).
- 306 `path.exists()` - E - FIXED (F42-1). 313 `read_to_string().unwrap_or_default()` - E - FIXED.
- 48, 1224 value parse `.ok()`; 120, 145, 223, 279 file-name `unwrap_or_default()` - not IO - left.
- 354 `from_str(&text).ok()` - unreachable (every parse failure is unusable earlier) - left.
- 485 `if let Ok(parse_from_rfc3339)`, 840 `.ok()`, 991 `unwrap_or_default()` - timestamp parses in
  the liveness rules, already fail-closed (unparsable: no age cut-off) - left.

### consult/recovery.rs - 6 hits + 1 read, 1 fixed, 6 left
- 88 (read) `rd.record None => continue` - E - FIXED through `read_pending_file` (an inspection
  failure is now `rd.error`, returned before any record is judged).
- 67/68/70 `read_sessions().ok().flatten()...unwrap_or_default()` - the sessions ledger: the
  recovered/cleared label only, not evidence - left.
- 96, 100, 109 field `unwrap_or_default()` - not IO - left.

### consult/kick.rs - 4 hits + 1 read, 2 fixed, 3 left
- 70 `!task_dir.is_dir()` - the directory holding the evidence - FIXED (a metadata error is named;
  NotFound / not a directory stays "no task directory"; exit 1 either way).
- 108-115 (read) record loop ignoring `rd.error` - E - FIXED (`kick_record`).
- 27, 53 parses - left. 159 `kick_path.exists()` - the kick request file - left.

### findings_tool/mod.rs - 17 hits + 2 read, 1 fixed, 18 left
- 298 (read) `!rd.exists` checked before the error - E - FIXED (the error first).
- 271-276 (read) `read_ledger` `Err(_) => Vec::new()` - the ledger (orphan flags) - left.
- 226 `exists()`, 229 `read_to_string().unwrap_or_default()`, 613, 720, 1344 `exists()` - the
  findings store (its own refusal: empty/unreadable refuses) - left.
- 345, 349 lock content `.ok()?` - the lock owner's message only - left.
- 996 `sessions_path.is_file()` - the ledger - left. 1240 test gate - left.
- 91, 390 parses; 656, 950, 1086, 1197 value defaults; 1295, 1316 `Option` flatten - left.

### findings_tool/revision.rs - 3 hits, 0 fixed, 3 left
- 37 git output `.ok()?`; 164, 166 `is_dir()/is_file()` (revision blobs) - not evidence - left.

### panel/ - 49 hits + 3 read, 2 fixed, 50 left
- run.rs 1322 (read) the unused-record pass `rd.error.is_some() || record None => continue` (the
  record kept but reported "stay unused") - E - FIXED (kept, named, state `unreadable`).
- run.rs 1729 (read) `record_state` "" on an error - E - FIXED (`unreadable`).
- run.rs 1165 (read) F15-3 - already refuses on `rd.error` - left (no change needed).
- run.rs (30): 61, 242 brief `is_file()`; 276 brief metadata; 97, 894 parses; 216 env; 322, 483,
  536, 559, 562, 564, 573, 596, 616, 694, 697, 702, 719 roster/plan defaults; 879, 880, 888 the
  findings (prior counts); 1033 launcher; 1105 child wait; 1179, 1547 defaults; 1375 the panel's
  temp directory; 1712 member out/err logs; 1747 the ledger's panel entry (2 hits); 1991 the
  ledger patch - left (not evidence).
- roles.rs (12): 77, 116, 121, 128, 133, 177, 189, 190, 205, 206, 207, 308 - plugin root, role
  templates, env - left.
- routing.rs (7): 53, 66, 67, 167, 176, 207, 526 - routing ratings over the ledger - left.
- member.rs, plan.rs, mod.rs: 0.

### consult/orchestrate.rs (recovery parts) - 7 hits, 6 fixed, 1 left
- 757-759 `before_kill` `read_pending_file(..).record...from_value().ok()...unwrap_or_else(on)` -
  the running record - FIXED (not replaced when it cannot be inspected; fallback, then copy).
- 1856 `member_early_accept` `!path.is_file()` - FIXED (a metadata error is named). 1921
  `from_value(val).unwrap_or_default()` - FIXED (unusable).
- 5161-5164 `member_accept` `read().ok()`, `from_slice().ok()`, `unwrap_or_default()` - FIXED (a
  read error or a parse error refuses the member before its launch; NotFound keeps the default).
- 828 `drop_copy` `c.exists()` - a removal guard; a lookup failure keeps the copy (the next run
  consumes it) - left.

### Total: 110 hits, 19 fixed, 91 left.

## Decided
- The kick reads every record and refuses on any inspection failure (the unreadable one may name
  the member), consistent with 7c's listing refusal.
- `before_kill`: a primary record that cannot be inspected is kept and the pre-kill record goes to
  the fallback / copy (its write "fails" with the inspection failure) - the kill is then not
  confirmed, as for any failed pre-kill write.
- A non-UTF-8 record keeps the old "empty or could not be read" unusable text.
- Outside the sweep's files: `c3_core::store::recover_pending` (numbering) already propagates
  non-NotFound errors.

## Next
- Supervisor: merge `wave-7d-metadata-fail-closed`, then the round-5 acceptance brief (RC1 of
  reply 42 = the test named above).
