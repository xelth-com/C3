# Handoff 23 - Codex: wave3a-diff-review-mimo

Date: 2026-10-09 08:50 local. Author: Codex (model mimo-v2.6-pro, effort high), Codex CLI 0.155.1.
Reviewer: mimo :: mimo-v2.6-pro (provider from -Provider, model from roster; endpoint https://token-plan-ams.xiaomimimo.com/v1, wire_api: responses; provider fingerprint 47cd6ee7e4ff; harness codex-cli 0.155.1).
Preflight: ok: env MIMO_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 3 of 12 for -Provider mimo (model, codex_config applied).
Effort: high sent (requested high, mapping mimo-v1, by caps-v1: token-plan-ams.xiaomimimo.com, mimo-v2.6-pro; not confirmed by the provider). Consultation id: 37cd6a9c-f9e5-4c0d-ae9e-5c5c5a20d89c.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m mimo-v2.6-pro -c model_reasoning_effort="high" -c model_provider="mimo" -c model_catalog_json="C:/Users/Dmytro/.codex/model-catalogs.json" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-3cc92e64087e4d9b99a23a8bbcd0adbe.md fork 01a11f5c-bb07-7bd0-b5fe-55b02fbe4c10 -` (prompt on stdin).
Parent thread: `01a11f5c-bb07-7bd0-b5fe-55b02fbe4c10`. Result thread: `01a11f6d-21b1-7052-b198-b30a2a3549c7` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/17-claude-wave3a-diff-review.md` (sha256 43c1bddfff78). Reviewed: a2943d8, base a2943d8a447c643d25fca481351693f6493508ab, tree sha256 1ac9e910cd6a, 0 changed files.
WARNING: working tree changed during the review (fingerprint before/after differ).
Note: HEAD moved during the review (a2943d8 -> 6bfc080).
Bridge outcome: usable reply. Wall time: 1501.9 s. Tokens: in 5296312 (cached 2558464), out 35736, reasoning 24210.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `6a5f838..1081079` - the range changes 15 files, 2679 lines (2406 insertions, 273 deletions).
Verdict: HOLD - HOLD — findings #1-#3 leave E19 and the recovery release rule fail-open, and the kept recovery evidence is not durable at the kill. Findings: 3 blocker, 1 major, 1 minor, 0 note (F23-1..F23-5, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/23-codex-wave3a-diff-review-mimo.reply.json`.
Raw event stream: `handoffs/23-codex-wave3a-diff-review-mimo.events.jsonl`.
Verbatim reply follows.

---

**Q1.** The documented E19 order matches the plugin, but C3 has an additional fail-open path: `process_info` treats a Toolhelp/name-and-parent lookup failure as `None`, and `test_unverified_process`/`test_recorded_process` then return `gone`. A live reviewer with a readable start time can therefore be dropped when the second inspection fails transiently. A process whose start time remains unreadable correctly blocks rather than being dropped.

**Q2.** Yes. `find_codex_processes` skips rows whose creation/start time is unknown (`created: None`), so both release scans can be clean while such a process still runs. Separately, the `survivors`/`unverified`/`kill_unconfirmed` record is written only at the end of the run; a bridge crash after the tree kill but before that write leaves the on-disk record without the new evidence. Combined with the scan gap, the record can be released or inadequately recorded while an orphan remains.

**Q3.** HOLD. The Samoa change addresses F06-1, but findings #1-#3 violate the fail-closed recovery invariant and must be fixed before acceptance.

## Requested checks
`RC1` (finding #1) — In `C:\Users\Dmytro\C3`, workspace-write, add a fault-injection test where `process_start_iso` returns `Some(start)` but `name_and_parent` returns `None`; call `test_unverified_process` and require a blocking/running verdict, not `gone`. Budget: 20 minutes.

`RC2` (finding #2) — In `C:\Users\Dmytro\C3`, workspace-write, run a pending/unknown-tree fixture containing a live process row with `created: None`; require `test_pending_active` to refuse or fail the scan rather than release. Budget: 20 minutes.

`RC3` (finding #3) — In `C:\Users\Dmytro\C3`, workspace-write, pause after `kill_tree_checked` and before `write_pending`, terminate the bridge, then inspect `.consult.pending.json`; require persisted `unverified`/`kill_unconfirmed` evidence. Budget: 30 minutes.

`RC4` (finding #5) — On Windows with Samoa Standard Time, workspace-write, run the production `chrono::Local` fixture for 2012-01-01/02 and the Samoa DST edges and compare with the pinned plugin. Budget: 20 minutes.

---

### Findings

- **F23-1** [blocker] `crates/c3/src/liveness/proc.rs:97`, `crates/c3/src/liveness/proc.rs:100`, `crates/c3/src/liveness/pending.rs:283`, `crates/c3/src/liveness/pending.rs:344` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time. Trigger: A recorded or unverified pid remains alive and its start time is readable, but `CreateToolhelp32Snapshot`/`name_and_parent` fails transiently or cannot list the pid. Evidence: read-code: `process_info` returns `None` on `name_and_parent(pid)?`, conflating lookup failure with process absence.; read-code: Both E19 re-check entry points map `process_info == None` directly to `gone`.; inferred: A readable live start followed by failed inspection must remain blocking, but this path drops it. Verify: Inject `name_and_parent -> None` while `process_start_iso -> Some(start)` and require `test_unverified_process`/`test_recorded_process` to return running or an explicit inspection failure. Remedy: Distinguish process absence from inspection failure; return an unknown/running verdict when the pid exists but metadata cannot be read.
- **F23-2** [blocker] `crates/c3/src/liveness/proc.rs:753`, `crates/c3/src/liveness/proc.rs:775`, `crates/c3/src/liveness/proc.rs:827`, `crates/c3/src/liveness/pending.rs:790` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs. Trigger: A kill leaves `kill_unconfirmed`, and an orphan or descendant is live but its creation/start time cannot be read, yielding a scan row with `created: None`. Evidence: read-code: Both scan branches `continue` when `p.created` is `None`.; read-code: The process-table builders map an unreadable start time to `created: None`.; inferred: Two clean scans can therefore coexist with a live unverifiable process. Verify: Run `test_pending_active` on a `kill_unconfirmed` record with a live scan row whose `created` is `None`; require refusal/failed verification, never release. Remedy: Treat unknown creation/start as blocking evidence or fail the scan; do not silently filter those rows.
- **F23-3** [blocker] `crates/c3/src/consult/orchestrate.rs:5105`, `crates/c3/src/consult/orchestrate.rs:5143`, `docs/port/wave3a-recovery.md:59` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree. Trigger: The bridge is killed after `kill_tree_checked` returns but before `store.write_pending(...)` writes the survivor record. Evidence: read-code: The survivor/unknown-tree record is assembled and written in `finish`, not at the kill sites.; read-code: The documentation explicitly records this once-at-end difference from the plugin.; inferred: A process death in the interval preserves only the older running/launching record. Verify: Add a pause between the kill and the end-of-run pending write, terminate the bridge there, and inspect the recovery record for the expected `unverified`/`kill_unconfirmed` fields. Remedy: Persist the kill disposition at each kill site, or write a durable kill journal before continuing the run.
- **F23-4** [major] `crates/c3/tests/recovery_wave3a.rs:35`, `crates/c3/tests/recovery_wave3a.rs:100`, `crates/c3/tests/recovery_wave3a.rs:163`, `crates/c3/src/liveness/pending.rs:1162` - The new tests do not prove the fail-closed recovery invariant: the start-unreadable assertion can be vacuous, descendant selection is unfiltered and timing-dependent, and the unknown-tree release test uses a future `started` timestamp that filters every candidate. Trigger: Run the tests on a machine without readable `/proc`/btime, with multiple descendants, or with either scan accidentally skipped. Evidence: read-code: The post-hook Alive assertion is skipped when `real` is empty.; read-code: `first_descendant()` takes the first descendant without checking its image or role.; read-code: The release fixture sets `started` one day in the future, making the scan assertions vacuous. Verify: Add deterministic fixtures for metadata lookup failure, `created: None`, and non-empty recorded start with current unreadable start; assert blocking behavior and actual pending-record writes. Remedy: Make the fixtures identify their target pid deterministically and assert both scan passes plus persisted record state.
- **F23-5** [minor] `crates/c3-core/src/health.rs:3123`, `crates/c3-core/src/health.rs:3157`, `crates/c3-core/src/health.rs:3204` - F06-1 is fixed in the synthetic path but production Samoa behavior remains unproven: the fixture hard-codes registry-derived rules, samples no DST edges, and marks the exact fall-back end ambiguous despite the F04-2 boundary semantics. Trigger: Run against a real Windows Samoa Standard Time registry or at the synthetic zone's exact transition boundaries. Evidence: read-code: The Samoa fixture hard-codes 2011/2012 rules and has an `Ambiguous` exact `to_std` case.; read-code: Only one retry-after trigger and one IANA comparison are asserted; production `chrono::Local` is not exercised. Verify: Run a production Windows `chrono::Local` Samoa test at the 2012 year boundary and both DST edges and compare with the pinned plugin. Remedy: Validate against the actual registry rules and add exact edge assertions consistent with F04-2.

### Prior findings

- F02-1 - not-checked - Outside this recovery range.
- F02-3 - not-checked - Outside this recovery range.
- F02-4 - not-checked - Outside this recovery range.
- F02-5 - not-checked - Outside this recovery range.
- F02-6 - not-checked - Outside this recovery range.
- F02-7 - not-checked - Outside this recovery range.
- F02-8 - not-checked - Outside this recovery range.
- F04-1 - fixed - Unicode-safe `strip_exe` and classifier/scan fixtures are present.
- F04-2 - fixed - Boundary normalization and Berlin edge fixtures address the original issue.
- F04-3 - not-checked - Panel light-gate behavior is outside this range.
- F06-1 - fixed - The Samoa local-year rule is implemented and has a synthetic fixture.
- F09-1 - not-checked - Outside this recovery range.
- F09-2 - not-checked - Outside this recovery range.
- F09-3 - not-checked - Outside this recovery range.
- F09-4 - not-checked - Outside this recovery range.
- F09-5 - not-checked - Outside this recovery range.
- F09-6 - not-checked - Outside this recovery range.
- F11-1 - not-checked - Outside this recovery range.
- F11-2 - not-checked - Outside this recovery range.
- F11-3 - not-checked - Outside this recovery range.
- F11-4 - not-checked - Outside this recovery range.
- F11-5 - not-checked - Outside this recovery range.
- F14-1 - not-checked - Outside this recovery range.
- F19-1 - not-checked - Outside this recovery range.
- F19-2 - not-checked - Outside this recovery range.
- F19-3 - not-checked - Outside this recovery range.
- F22-1 - still-open - The journal retention/replay issue remains unaddressed by this recovery range.

## Verdict: HOLD

HOLD — findings #1-#3 leave E19 and the recovery release rule fail-open, and the kept recovery evidence is not durable at the kill.

### Blockers

- **F23-1** `crates/c3/src/liveness/proc.rs:97`, `crates/c3/src/liveness/proc.rs:100`, `crates/c3/src/liveness/pending.rs:283`, `crates/c3/src/liveness/pending.rs:344` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time. Verify: Inject `name_and_parent -> None` while `process_start_iso -> Some(start)` and require `test_unverified_process`/`test_recorded_process` to return running or an explicit inspection failure. Remedy: Distinguish process absence from inspection failure; return an unknown/running verdict when the pid exists but metadata cannot be read.
- **F23-2** `crates/c3/src/liveness/proc.rs:753`, `crates/c3/src/liveness/proc.rs:775`, `crates/c3/src/liveness/proc.rs:827`, `crates/c3/src/liveness/pending.rs:790` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs. Verify: Run `test_pending_active` on a `kill_unconfirmed` record with a live scan row whose `created` is `None`; require refusal/failed verification, never release. Remedy: Treat unknown creation/start as blocking evidence or fail the scan; do not silently filter those rows.
- **F23-3** `crates/c3/src/consult/orchestrate.rs:5105`, `crates/c3/src/consult/orchestrate.rs:5143`, `docs/port/wave3a-recovery.md:59` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree. Verify: Add a pause between the kill and the end-of-run pending write, terminate the bridge there, and inspect the recovery record for the expected `unverified`/`kill_unconfirmed` fields. Remedy: Persist the kill disposition at each kill site, or write a durable kill journal before continuing the run.
- **F14-1** (prior, not-checked) `crates/c3/src/telemetry/complaint.rs:542`, `crates/c3/src/telemetry/complaint.rs:240`, `crates/c3/src/telemetry/complaint.rs:673` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending'). Verify: Add a test: forget_with with ForgetRequest{public_ref: None, local: true, yes: true} and an injected remove failing on spool.ndjson; assert whether forget-pending.json exists and whether a subsequent flush posts the retained events. Remedy: In the None arm, persist the transaction (phase cleaning) via write_transaction before calling finish_cleanup (or pass phase confirmed so run_local_cleanup writes it), so an interruption leaves the blocking, resumable record the docs and the CLI message promise.
- **F22-1** (prior, still-open) `crates/c3-core/src/health.rs:1563`, `crates/c3-core/src/health.rs:1915`, `crates/c3-core/src/health.rs:1942`, `crates/c3-core/src/health.rs:1960`, `crates/c3-core/src/health.rs:2028` - After failed `.bad` archival, the retained journal is replayed against mutable endpoint state whose record keys can be removed by the 24-hour or 500-record retention rules. A journal record can therefore be applied, evicted, and re-applied with a different final effect, while a record already stale on a replay is parsed but never becomes visible. The record-key dedup is not durable, so the documented harmless/once replay invariant is false. Verify: Add a deterministic test with an unwritable `.bad` path, one recent journal record, and 500 newer endpoint records; run two updates and assert the journal record is neither resurrected nor allowed to displace another endpoint. Remedy: Persist a monotonic journal applied-sequence marker or equivalent durable consumed state, and truncate the journal by that sequence independently of endpoint retention; do not use the mutable endpoint set as the only replay ledger.

### Unproven scenarios

- No tests or harnesses were run in this read-only consultation.
- The process-info lookup-failure and created-unknown scan paths have not been reproduced dynamically.
- The end-of-run recovery-record crash window has not been reproduced.
- Production Windows Samoa registry behavior and transition edges remain unverified.

### First-run checklist (observable)

_(none)_
