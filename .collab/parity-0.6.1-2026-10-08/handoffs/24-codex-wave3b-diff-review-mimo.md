# Handoff 24 - Codex: wave3b-diff-review-mimo

Date: 2026-10-09 09:15 local. Author: Codex (model mimo-v2.6-pro, effort high), Codex CLI 0.155.1.
Reviewer: mimo :: mimo-v2.6-pro (provider from -Provider, model from roster; endpoint https://token-plan-ams.xiaomimimo.com/v1, wire_api: responses; provider fingerprint 47cd6ee7e4ff; harness codex-cli 0.155.1).
Preflight: ok: env MIMO_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 3 of 12 for -Provider mimo (model, codex_config applied).
Effort: high sent (requested high, mapping mimo-v1, by caps-v1: token-plan-ams.xiaomimimo.com, mimo-v2.6-pro; not confirmed by the provider). Consultation id: 061ca452-d052-49e8-b55d-af05c6daca4b.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m mimo-v2.6-pro -c model_reasoning_effort="high" -c model_provider="mimo" -c model_catalog_json="C:/Users/Dmytro/.codex/model-catalogs.json" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-de3669442cef4bb8a9bdfff81673b1a2.md fork 01a11f6d-21b1-7052-b198-b30a2a3549c7 -` (prompt on stdin).
Parent thread: `01a11f6d-21b1-7052-b198-b30a2a3549c7`. Result thread: `01a11f84-5109-72a2-82b2-9bc3b94cd00a` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/18-claude-wave3b-diff-review.md` (sha256 8d87474a3538). Reviewed: 6bfc080, base 6bfc080c950585319ace0bc376f31e046f94b233, tree sha256 aea88395eb5d, 0 changed files.
Bridge outcome: usable reply (after a timeout continuation). Wall time: 1942 s. Tokens: unknown.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `fda5af0..da7bfdd` - the range changes 12 files, 2921 lines (2823 insertions, 98 deletions).
Timeout continuation: the main turn was killed at 1942 s of 2400 s; one continuation turn on thread `01a11f84-5109-72a2-82b2-9bc3b94cd00a` answered in 225.3 s. Tokens of that turn: in 14021214 (cached 9123712), out 52487, reasoning 33480.
Verdict: HOLD - HOLD — findings #1-#3 violate the never-lose-a-count invariant or leave deletion cleanup incomplete. Findings: 1 blocker, 4 major, 1 minor, 0 note (F24-1..F24-6, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/24-codex-wave3b-diff-review-mimo.reply.json`.
Raw event stream: `handoffs/24-codex-wave3b-diff-review-mimo.events.jsonl`; further turns: `handoffs/24-codex-wave3b-diff-review-mimo.continue.events.jsonl`.
Verbatim reply follows.

---

**Q1.** The normal two-producer/sender/forget schedule is serialized and saves `last-flush.json` before deleting folded files. However, the invariant is not intact. If the spool lock is busy, `record_flush` marks every non-legacy line seen without folding it; a later successful fold counts and reports those same lines again. Also, a suffix appended after a recorded fold is counted only when newline-terminated, then deleted, so an interrupted tail disappears. Finally, `note_not_spooled` appends without synchronization while `run_local_cleanup` enumerates files once, so a concurrent count can survive a local forget.

**Q2.** P8 is an acceptable test-only adapter only if the production layout is tested separately. It redirects not-spooled files, `.last`, and the marker, while locks and the outbox remain under C3’s telemetry root; the tests do not exercise the plugin-home marker path or concurrent plugin/C3 flushes sharing those files. The hook can therefore hide path and lock-topology differences.

**Q3.** HOLD until the fallback accounting, suffix handling, and cleanup race are fixed and the P8 marker/production-path cases are proven.

## Requested checks
`RC1` (finding #1) — In `C:\Users\Dmytro\C3`, workspace-write, hold `spool.lock` through one flush with N gone-producer lines, release it, run a second flush, and inspect both `last-flush.json` records; require one accounting of N, never N twice. Budget: 20 minutes.

`RC2` (finding #2) — In `C:\Users\Dmytro\C3`, workspace-write, run `CODEX_CONSULT_TEST_FOLD_CRASH=1`, append an unterminated line to a recorded producer file, restart the fold, and require the suffix to be counted or retained. Budget: 20 minutes.

`RC3` (finding #3) — In `C:\Users\Dmytro\C3`, workspace-write, pause `run_local_cleanup` after `list_files`, append from a second producer, complete `--forget --local`, and require no not-spooled file to survive. Budget: 20 minutes.

`RC4` (finding #5) — In `C:\Users\Dmytro\C3`, workspace-write, run the plugin-home marker fixture and one no-hook production-path fixture; require the marker to be read/written/removed at the selected root. Budget: 20 minutes.

---

### Findings

- **F24-1** [blocker] `crates/c3/src/telemetry/spool.rs:640`, `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:704` - The spool-lock fallback marks all non-legacy not-spooled lines as seen without folding or retaining per-file provenance; a later fold counts the same gone-producer files again, so the accounting can double-count and temporarily hide the count. Trigger: A gone producer leaves N lines, the spool lock remains busy during one recorded flush, then a later flush succeeds after the lock is released. Evidence: read-code: On lock timeout the fallback writes `not_spooled_seen = total - legacy_lines` and preserves only pre-existing folded entries.; read-code: A later merge counts the gone files and records them as newly folded.; inferred: The same N lines can appear as seen and later as a fold note. Verify: Hold `spool.lock` for over one second during flush 1 with N gone-producer lines, then run flush 2 and compare both records; require exactly one accounting of N. Remedy: In the fallback, retain per-file folded entries or mark only positively kept files seen; never advance the global baseline for files that remain foldable.
- **F24-2** [major] `crates/c3/src/telemetry/notspooled.rs:728`, `crates/c3/src/telemetry/notspooled.rs:775` - For an already-recorded file, the fold counts only newline-complete suffix lines and then deletes the file, so an appended tail without a newline is discarded despite the fold’s gone-producer path counting tails. Trigger: After `CODEX_CONSULT_TEST_FOLD_CRASH=1`, append an unterminated line to a file already named in `not_spooled_folded`, then resume the fold. Evidence: read-code: The recorded suffix is parsed with `complete_lines(..., false)`.; read-code: The file is deleted after the record is saved. Verify: Append an unterminated suffix to a recorded file and assert the resumed fold either counts it or retains the bytes; require no silent zero. Remedy: Count tails consistently for gone files or retain incomplete suffix bytes until they become complete.
- **F24-3** [major] `crates/c3/src/telemetry/notspooled.rs:397`, `crates/c3/src/telemetry/complaint.rs:386` - Local cleanup can miss a concurrently created not-spooled file because cleanup enumerates files once while `note_not_spooled` appends without holding the deletion locks or checking the marker. Trigger: Pause cleanup after `list_files`; a second producer is refused by the live forgetting marker and writes its own not-spooled file before cleanup completes. Evidence: read-code: The append path uses no marker or deletion synchronization.; read-code: Cleanup takes one file snapshot before removals. Verify: Inject a pause after enumeration, append from a second producer, complete the local forget, and assert no not-spooled file remains. Remedy: Serialize count writes with cleanup or rescan immediately before completing deletion and remove newly discovered files.
- **F24-4** [minor] `crates/c3/src/telemetry/notspooled.rs:575`, `crates/c3/src/telemetry/notspooled.rs:617` - `count` reports the latest line from all parsed lines rather than the latest line since the baseline, so `--status` can name an already-seen older event while a newer unseen count is present. Trigger: Set `not_spooled_seen=1`, place an already-seen line dated 2030 before a new line dated 2020, then read `--status`. Evidence: read-code: The `latest` candidate is updated for every parsed line regardless of whether it contributes to `count`. Verify: Use out-of-order timestamps and assert the reported latest why/time belongs to the unseen suffix. Remedy: Track the latest line only among lines contributing to the post-baseline count.
- **F24-5** [major] `crates/c3/src/telemetry/notspooled.rs:100`, `crates/c3/src/telemetry/notspooled.rs:121`, `crates/c3-cli/tests/notspooled_parity.rs:1019` - P8 is under-proven as an oracle adapter: tests cover plugin-home not-spooled and `.last` paths but not the plugin-home forgetting marker or the production no-hook layout, while shared files use different lock domains for C3 and the plugin. Trigger: Run the marker fixture with `C3_TEST_TELEMETRY_PLUGIN_HOME`, then run concurrent plugin and C3 flushes against the same home. Evidence: read-code: The hook redirects the marker and `.last` only in test mode.; read-code: The hook test does not exercise marker ownership/removal at the plugin home.; read-code: The documentation notes separate C3 and plugin lock domains despite shared files. Verify: Add plugin-home marker ownership/removal tests, a no-hook production-path test, and a concurrent plugin/C3 flush test. Remedy: Prove all three redirected artifacts and production paths; coordinate concurrent implementations or document the shared-file lock contract.
- **F24-6** [major] `crates/c3/src/telemetry/spool.rs:722` - The second record rewrite after fold deletion ignores its error, so stale `not_spooled_folded` names can remain and later suppress accounting for a recreated file with the same name. Trigger: Crash or fail `write_last` after folded files are deleted but before their names are removed from the record, then recreate a matching filename. Evidence: read-code: The post-deletion `write_last` result is discarded. Verify: Inject a second rewrite failure and verify the next status/fold reports the stale names and preserves counts for a recreated file. Remedy: Handle the second rewrite failure explicitly and validate folded entries by file identity, not name alone.

### Prior findings

- F02-1 - fixed - Current remove_lines rereads the live spool under the lock and preserves appended lines.
- F02-3 - fixed - The pending deletion record is written before DELETE and retained on failure.
- F02-4 - still-open - The post-P8 implementation coverage map and independent oracle proof remain incomplete.
- F02-5 - not-checked - Outside this range.
- F02-6 - not-checked - Outside this range.
- F02-7 - not-checked - Outside this range.
- F02-8 - not-checked - Outside this range.
- F04-1 - fixed - Unicode-safe name handling and fixtures are present.
- F04-2 - fixed - The local-zone boundary normalization is implemented and covered.
- F04-3 - not-checked - Outside this range.
- F06-1 - fixed - The Samoa local-year correction is implemented; production registry coverage remains tracked by F23-5.
- F09-1 - not-checked - Outside this range.
- F09-2 - fixed - Deletion state is checked under the sender and spool locks before posting.
- F09-3 - fixed - Cleanup keeps the transaction until queued data, references, and salt are removed.
- F09-4 - fixed - A failed reread prevents spool replacement and preserves bytes.
- F09-5 - fixed - Legacy events are closed through the sanitizer before sending.
- F09-6 - not-checked - Outside this range.
- F11-1 - not-checked - Outside this range.
- F11-2 - not-checked - Outside this range.
- F11-3 - not-checked - Outside this range.
- F11-4 - not-checked - Outside this range.
- F11-5 - not-checked - Outside this range.
- F14-1 - fixed - A local-only deletion records its cleaning transaction before cleanup.
- F19-1 - fixed - Complaint submission is gated by transaction and marker checks under the locks.
- F19-2 - fixed - Unreadable salt is tri-state and aborts cleanup without deleting the transaction; a residual salt-creation race remains untested.
- F19-3 - not-checked - Duplicate-key sanitizer behavior is outside this range.
- F22-1 - still-open - The health-journal retention/replay defect remains open.
- F23-1 - still-open - Process-info lookup failure can still be conflated with process absence.
- F23-2 - still-open - Unknown creation/start rows can still be skipped by recovery scans.
- F23-3 - still-open - Recovery evidence is still written only at the end of the run.
- F23-4 - still-open - Recovery fail-closed test coverage remains incomplete.
- F23-5 - still-open - Production Samoa registry and transition-edge behavior remains unproven.

## Verdict: HOLD

HOLD — findings #1-#3 violate the never-lose-a-count invariant or leave deletion cleanup incomplete.

### Blockers

- **F24-1** `crates/c3/src/telemetry/spool.rs:640`, `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:704` - The spool-lock fallback marks all non-legacy not-spooled lines as seen without folding or retaining per-file provenance; a later fold counts the same gone-producer files again, so the accounting can double-count and temporarily hide the count. Verify: Hold `spool.lock` for over one second during flush 1 with N gone-producer lines, then run flush 2 and compare both records; require exactly one accounting of N. Remedy: In the fallback, retain per-file folded entries or mark only positively kept files seen; never advance the global baseline for files that remain foldable.
- **F22-1** (prior, still-open) `crates/c3-core/src/health.rs:1563`, `crates/c3-core/src/health.rs:1915`, `crates/c3-core/src/health.rs:1942`, `crates/c3-core/src/health.rs:1960`, `crates/c3-core/src/health.rs:2028` - After failed `.bad` archival, the retained journal is replayed against mutable endpoint state whose record keys can be removed by the 24-hour or 500-record retention rules. A journal record can therefore be applied, evicted, and re-applied with a different final effect, while a record already stale on a replay is parsed but never becomes visible. The record-key dedup is not durable, so the documented harmless/once replay invariant is false. Verify: Add a deterministic test with an unwritable `.bad` path, one recent journal record, and 500 newer endpoint records; run two updates and assert the journal record is neither resurrected nor allowed to displace another endpoint. Remedy: Persist a monotonic journal applied-sequence marker or equivalent durable consumed state, and truncate the journal by that sequence independently of endpoint retention; do not use the mutable endpoint set as the only replay ledger.
- **F23-1** (prior, still-open) `crates/c3/src/liveness/proc.rs:97`, `crates/c3/src/liveness/proc.rs:100`, `crates/c3/src/liveness/pending.rs:283`, `crates/c3/src/liveness/pending.rs:344` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time. Verify: Inject `name_and_parent -> None` while `process_start_iso -> Some(start)` and require `test_unverified_process`/`test_recorded_process` to return running or an explicit inspection failure. Remedy: Distinguish process absence from inspection failure; return an unknown/running verdict when the pid exists but metadata cannot be read.
- **F23-2** (prior, still-open) `crates/c3/src/liveness/proc.rs:753`, `crates/c3/src/liveness/proc.rs:775`, `crates/c3/src/liveness/proc.rs:827`, `crates/c3/src/liveness/pending.rs:790` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs. Verify: Run `test_pending_active` on a `kill_unconfirmed` record with a live scan row whose `created` is `None`; require refusal/failed verification, never release. Remedy: Treat unknown creation/start as blocking evidence or fail the scan; do not silently filter those rows.
- **F23-3** (prior, still-open) `crates/c3/src/consult/orchestrate.rs:5105`, `crates/c3/src/consult/orchestrate.rs:5143`, `docs/port/wave3a-recovery.md:59` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree. Verify: Add a pause between the kill and the end-of-run pending write, terminate the bridge there, and inspect the recovery record for the expected `unverified`/`kill_unconfirmed` fields. Remedy: Persist the kill disposition at each kill site, or write a durable kill journal before continuing the run.

### Unproven scenarios

- No tests were run in this read-only consultation.
- The fallback double-count and suffix-tail behavior have not been reproduced dynamically.
- Concurrent cleanup and lockless not-spooled writes have not been reproduced.
- P8 plugin-home marker behavior, production-path behavior, and cross-implementation locking remain unverified.

### First-run checklist (observable)

_(none)_
