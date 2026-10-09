# Wave 6 of the compatibility track: the live matrix's differences (2026-10-09)

Branch `wave6-live-diffs`. The input is `rc3-live-matrix-2026-10-09.md`, "Differences from the
plugin" (conclusion 5 and the section "Other differences seen on the way"): items 1-4 below. Item 5
of that list - C3's own telemetry app id, salt and outbox - is decision P7 and stays. The
specification is the plugin at `v0.6.1`: `codex-consult-common.ps1` (`Get-CoordinatorHostHint`,
`Resolve-CoordinatorIdentity`, `Format-CoordinatorText`, `Read-AllTaskConsults`,
`Resolve-EngineIdentity`, `Test-TelemetrySenderEnvName`, `Get-TelemetrySenderEnvironment`,
`New-TelemetrySenderStartInfo`, `Start-NoInheritProcess`, `Start-TelemetrySender`),
`codex-consult.ps1` (the sender after the commit, the panel's one sender), `codex-findings.ps1`
(the rating's sender), `codex-providers.ps1` (the header's count) and `codex-telemetry.ps1`
(`-Flush` and its `CODEX_CONSULT_TEST_TELEMETRY_ENV` hook).

## 1 - the ledger's `coordinator` record and foreign entries on a rewrite

- **The record.** `Coordinator` (`c3-core/src/ledger.rs`) is now the plugin literal `{provider,
  model, engine, host, host_by, source, in_roster, unresolved}`: `host_by` sits after `host`, and
  `in_roster` / `unresolved` are always written (`null` when there is no roster, no identity or
  nothing unresolved) instead of being skipped. `build_coordinator` sets `host_by` as
  `Get-CoordinatorHostHint` would for C3's one host: `markers` for `claude-code` (C3 infers it from
  the markers only), `none` for `unknown`. A record written before `host_by` existed reads with
  `host_by` absent and keeps it absent. `format_coordinator_text` says `(inferred from the install
  path, a hint)` for a plugin record whose `host_by` is `path`, as `Format-CoordinatorText` does.
- **Foreign entries.** `LedgerBook.consults` is read through `deserialize_consults`: every entry
  keeps the JSON object it was parsed from (`LedgerEntry::source_json`, never serialized itself).
  `serialize_consults` writes an entry built in this run from its fields, and an entry read from
  disk as that object - verbatim when C3 changed nothing in it (its key order, its explicit nulls,
  the keys C3 does not know), else with exactly C3's changes laid onto it (`ledger::merge_onto`: a
  changed key takes C3's value - an object merged key by key -, a removed key goes, an added key is
  placed after the key that precedes it in C3's order; a typed default C3 did not touch is not
  added). The two writers of `sessions.json` - the commit (`store.rs`, appends) and the panel's
  counts (`panel/run.rs`, patches `panel.started` / `panel.usable`) - both go through it.
- Ratings write `findings.json` only (both sides), so "the fields the rating adds" are none in
  `sessions.json`: the tests check the ledger byte for byte across a rating too.

Tests: `c3-core` `ledger::tests::merge_onto_keeps_the_read_object_and_lays_only_the_changes`,
`a_read_entry_is_written_verbatim_and_a_new_one_from_its_fields`; `tests/store.rs`
`a_plugin_entry_survives_a_c3_commit_byte_for_byte` (the fixture
`tests/fixtures/plugin-0.6.1-sessions.json` is the plugin's own file from the RC3 interchange, step
(a), with the user name in its paths replaced) and
`a_changed_foreign_entry_keeps_its_shape_and_takes_only_the_change` (the panel counts' patch);
`c3-cli/tests/wave6_live_diffs.rs` `a_plugin_entry_survives_a_c3_rating_and_a_c3_consultation_byte_for_byte`
(the real binary: a C3 rating, a C3 consultation, a second rating - every byte of the plugin's
file through its entry unchanged, C3's coordinator with the eight keys) and
`a_coordinator_without_markers_records_host_by_none`.

## 2 - the `c3 providers` header

`endpoint health: <collab> (<k> task ledgers, <m> consultations), read at ... - the ledgers of THIS
repository` counted `read_all_task_consults_health` - the repository's consultations plus the
machine-wide health file's synthetic records. `m` is now the repository's consultations only
(`read_all_task_consults`, as `codex-providers.ps1` counts `Read-AllTaskConsults`); the machine-wide
records still join the list the verdicts are computed from. The JSON `health_source` is the same
string. Test: `the_providers_header_counts_this_repositorys_consultations_only` (a health file of
two records: `0 task ledgers, 0 consultations` with no ledger, `1 task ledger, 1 consultation` with
the plugin's ledger - it said 2 and 3 before).

## 3 - the agy and muse reviewer line

`identity_display` (`consult/orchestrate.rs`; the dry run's `reviewer :` and the reply header's
`Reviewer:`) special-cased the claude engine only; agy and muse fell through to the codex endpoint
text `endpoint (default), wire_api: (default)`. Every CLI engine of the plugin - agy, muse, claude -
now shows `Resolve-EngineIdentity`'s Display: `engine <name> (<launcher>)`, `<name> CLI not found`
without a launcher (the engine's command is its name for all three), and for claude's endpoint mode
the existing `, endpoint <base url> (token from env <NAME>)`. The codex engine and C3's own http
engine (its endpoint is a provider table) keep the endpoint text. Tests: `consult::orchestrate`
`identity_display_tests` (agy and muse with and without a launcher, through `reviewer_line` too;
claude, codex and http unchanged).

## 4 - the detached telemetry sender

Before: `consult::run` started an in-process flush thread at the START of every telemetry-on run
(joined with a 3 s cap at its end), and a rating / a backfill flushed in a thread joined with the
same cap - so a session's last event waited for the next run or `c3 telemetry --flush`.

Now, as `Start-TelemetrySender` (`telemetry/sender.rs`): right after the commit the run starts ONE
detached process, `c3 telemetry --flush --telemetry on`, and does not wait for it; that process
flushes once through `flush_now` (the sender lock, its owner record `flush.owner.json`, the last
flush's record - a second sender that finds the lock busy skips) and exits.

- **When** (the plugin's rules): a consultation whose event was spooled at its commit, unless it is a
  panel member (the panel run starts ONE sender after its counts, when at least one member
  started) or it keeps its recovery record (survivors, a failed registration - the next run's
  sender delivers); a rating whose event was spooled; a backfill that sent at least one event
  (`codex-telemetry: the sender started (detached) - c3 telemetry --status shows the result.`, or
  `the sender did not start (<why>) - the next consultation's sender, or c3 telemetry --flush,
  delivers the spool.`). A run whose event was not spooled starts none. A sender that cannot start
  never fails the run (the reason under `C3_DEBUG`, the plugin's `Write-Verbose`).
- **The process** holds none of the run's handles: on Windows `CreateProcessW` with
  `bInheritHandles = FALSE`, `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP |
  CREATE_UNICODE_ENVIRONMENT` - the helper of the `-Detach` background
  (`consult::detach::create_process_no_inherit`), which now takes an optional environment block;
  elsewhere `Command` with null standard streams, its own process group and a thread that reaps it
  (the MCP server keeps no zombie). It starts in the temporary directory.
- **Its environment** is an ALLOW list (`sender_environment`): the plugin's names and prefixes
  (`$script:TelemetrySenderEnvNames` / `...Prefixes`, compared ignoring case), in test mode
  `CODEX_CONSULT_TEST_MODE` and `CODEX_CONSULT_TEST_TELEMETRY_*`, and `CODEX_HOME` named even when
  it was derived - no provider key, no host marker, no other `CODEX_CONSULT_*`. **C3's additions:**
  `C3_TELEMETRY_HUB` (C3's own intake override - the sender must post where the run would have);
  and in test mode C3's plugin-home hook `C3_TEST_TELEMETRY_PLUGIN_HOME` travels as
  `CODEX_CONSULT_TEST_TELEMETRY_PLUGIN_HOME` (the one test name the allow list admits;
  `notspooled::local_paths` reads either), so the sender's last-flush record lands where the run's
  would.
- `c3 telemetry --flush` has the plugin's test hook `CODEX_CONSULT_TEST_TELEMETRY_ENV=<path>` (test
  mode only): the NAMES of every variable of its environment, sorted ignoring case, one per line.
- **The priors refresh** (M9 §3) ran inside the start-of-run flush thread; it stays at the start of a
  telemetry-on consultation on its own thread (`router::refresh_priors_in_background`, the same 3 s
  cap) - it is not the sender's job and needs `C3_PRIORS`, which the sender's allow list does not
  carry. A rating and a backfill no longer refresh the priors.
- `spool::flush_in_background` / `BackgroundFlush` are gone (no caller left).

Tests: `telemetry::sender::tests` (the allow list against the plugin's; the environment with the
codex home and the hook); `c3-cli/tests/wave6_live_diffs.rs`
`the_detached_sender_delivers_a_runs_event_without_another_run` - a loopback intake that ACCEPTS only
after the run returned (a run that waited for its sender, or a sender holding the run's output
pipes, would stall the test): the run's event arrives as one `POST /T/v2/events` within seconds,
the last flush's record says delivered with `http 200`, the outbox is empty, the sender's
environment dump holds only allow-listed names (no `CLAUDECODE`, `CODEX_THREAD_ID`, `RT_ZAI_KEY`;
`CODEX_HOME` and `C3_TELEMETRY_HUB` present); then a rating's event arrives the same way; and
`a_run_with_telemetry_off_starts_no_sender`.

## Harnesses through the shim (pinned v0.6.1)

Staging as in `harness-shim.md` section 4 (`%TEMP%\c3-wave6\stage`: the v0.6.1 plugin tree, the six
shims over it), `C3_EXE` = a copy of this branch's final build (`c3 0.2.0`, built after touching the
three crate roots; the copy's hash equals `target\debug\c3.exe` after the final `cargo test`), host
Windows PowerShell 5.1 with `PSModulePath` reset, one harness at a time under
`%TEMP%\codex-consult-tests\HARNESS.lock`, through `tests\run-all.ps1 -ScriptsDir ... -Only <h>`.
"Before" is main's (wave 5 report, RC5 verification).

| harness | before | after | |
|---|---|---|---|
| 0.3 | 229 / 0 | 229 / 0 | |
| roster | 125 / 0 | 125 / 0 | |
| engines | 97 / 0 | 97 / 0 | the first run's log directory was deleted under it by something outside the run (its numbers lost, exit 1 from the runner's log write); the rerun is green |
| muse | 73 / 1 | 73 / 1 | the same UNIT D2 (the plugin's own source scan) |
| companions | 42 / 0 | 42 / 0 | |
| telemetry | 47 / 96 | 47 / 96 | the same failure set (diffed line by line against a main run) |
| host | 59 / 6 | 59 / 6 | the six not applicable to C3 (`harness-shim.md`) |

**SEND x2 (and ENV) stay red in the run, for a reason of the SHIM, not of the sender:** the
`codex-consult.ps1` shim sets `CODEX_CONSULT_TELEMETRY=off` when the caller left it unset (its safety
net, `harness-shim.md` "The telemetry shim"), and the SEND case leaves it unset - so the run spools
no event and starts no sender ("0 request(s)"). An experiment with that one shim line removed
(`harness-telemetry.ps1 -Only SEND` on a copy of the staging, under the lock; not part of the table):
**SEND "delivered = deleted" PASS** (the plugin's spool empty, `.last` at the plugin's place through
the plugin-home hook: `delivered 1, kept 0, dropped 0`, `http 200`, the ten keys in order), **ENV
PASS** (the sender's dump holds only the plugin's allow list - no marker, no `RT_ZAI_KEY`, no
`CODEX_CONSULT_ROSTER`; `CODEX_HOME` and `CODEX_CONSULT_TELEMETRY_URL` present), and SEND "ONE
request" got exactly one `POST /T/v2/events` within the run's 3.7 s and fails only on the allowlist
walk of the event (`app_id` `c3`, not `codex-consult` - decision P7). The safety net stays: it keeps
every other harness's fake consultations from spooling.

## Tests and checks

`cargo test --workspace -j 2 --no-fail-fast` (after touching the three crate roots): 772 passed, 0
failed. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`: clean.
