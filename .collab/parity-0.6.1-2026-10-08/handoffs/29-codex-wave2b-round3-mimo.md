# Handoff 29 - Codex: wave2b-round3-mimo

Date: 2026-10-09 11:02 local. Author: Codex (model mimo-v2.6-pro, effort high), Codex CLI 0.155.1.
Reviewer: mimo :: mimo-v2.6-pro (provider from -Provider, model from roster; endpoint https://token-plan-ams.xiaomimimo.com/v1, wire_api: responses; provider fingerprint 47cd6ee7e4ff; harness codex-cli 0.155.1).
Preflight: ok: env MIMO_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 3 of 12 for -Provider mimo (model, codex_config applied).
Effort: high sent (requested high, mapping mimo-v1, by caps-v1: token-plan-ams.xiaomimimo.com, mimo-v2.6-pro; not confirmed by the provider). Consultation id: 3a82326d-2e9c-4ca8-b02f-4b8b5d538b6b.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m mimo-v2.6-pro -c model_reasoning_effort="high" -c model_provider="mimo" -c model_catalog_json="C:/Users/Dmytro/.codex/model-catalogs.json" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-6e21ce56ae6e4564ac1c82ef7bb689e6.md fork 01a11fa7-8a2c-7322-90b3-9a5b8710f70d -` (prompt on stdin).
Parent thread: `01a11fa7-8a2c-7322-90b3-9a5b8710f70d`. Result thread: `01a11fe6-04c9-7fd3-9523-f023d1e4f653` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/26-claude-wave2b-round3.md` (sha256 bfa2a35d629f). Reviewed: 70e74c7, base 70e74c76faf9bc9b765569a5ca5ce195a3a4028d, tree sha256 700af582d85d, 0 changed files.
WARNING: working tree changed during the review (fingerprint before/after differ).
Note: HEAD moved during the review (70e74c7 -> c8f0997).
Bridge outcome: usable reply. Wall time: 1235.2 s. Tokens: in 20634684 (cached 14354176), out 71684, reasoning 44478.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `6bfc080..c5d8b28` - the range changes 2 files, 485 lines (459 insertions, 26 deletions).
Verdict: HOLD - HOLD — finding #1 supersedes F22-1: the applied-key ledger is not durable across the plugin’s shared-journal writer. Findings: 1 blocker, 0 major, 0 minor, 0 note (F29-1, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/29-codex-wave2b-round3-mimo.reply.json`.
Raw event stream: `handoffs/29-codex-wave2b-round3-mimo.events.jsonl`.
Verbatim reply follows.

---

**Q1.** Two C3 updaters are serialized by the health lock, and a crash between the health write and list write is safe for C3 because the next update applies the journal before retention and the endpoint key suppresses a duplicate. However, the schedule fails across writers: after C3 lists a kept journal record and the 500-record cap evicts its endpoint, a plugin update that ignores or deletes `.journal.applied` can replay the retained journal record as new. A missing or malformed list is also treated as empty, producing the same resurrection. This can apply the record twice and displace a newer endpoint. A stale surviving list can conversely suppress a later same-key record until it is removed.

**Q2.** HOLD until the ledger is authoritative across C3/plugin updates or mixed-writer replay is otherwise prevented and tested.

---

### Findings

- **F29-1** [blocker] `crates/c3-core/src/health.rs:1682`, `crates/c3-core/src/health.rs:2017`, `crates/c3-core/src/health.rs:2045`, `crates/c3-core/src/health.rs:2189` - The applied-key list is only a C3-local replay guard: it is silently ignored when missing or malformed, and the plugin’s shared journal updater neither reads nor preserves it, so a retained journal record can still be replayed after endpoint-key eviction and applied twice. Trigger: A journal is kept and its key list written; the endpoint cap evicts the corresponding endpoint; a plugin update then deletes or ignores `.journal.applied` and processes the retained journal, or the list becomes unreadable-as-valid while the journal remains. Evidence: read-code: Missing or unparsable applied lists are converted to an empty set.; read-code: Replay suppression depends solely on that optional list plus mutable endpoint keys.; read-code: The documentation states that the plugin neither reads nor writes the applied-key list. Verify: Create a kept journal record, evict its endpoint through the cap, remove or invalidate `.journal.applied`, run the plugin journal updater and then an update, and assert the record is neither re-added nor allowed to displace a newer endpoint. Remedy: Make consumed-journal state authoritative in the shared journal protocol, or fail closed when the retained journal has no trustworthy ledger; add a mixed C3/plugin crash-and-eviction fixture. Supersedes: F22-1.

### Prior findings

- F02-1 - not-checked - Outside this two-file range.
- F02-3 - not-checked - Outside this range.
- F02-4 - not-checked - Outside this range.
- F02-5 - not-checked - Outside this range.
- F02-6 - not-checked - Outside this range.
- F02-7 - not-checked - Outside this range.
- F02-8 - not-checked - Outside this range.
- F04-1 - not-checked - Outside this range.
- F04-2 - not-checked - Outside this range.
- F04-3 - not-checked - Outside this range.
- F06-1 - not-checked - Outside this range.
- F09-1 - not-checked - Outside this range.
- F09-2 - not-checked - Outside this range.
- F09-3 - not-checked - Outside this range.
- F09-4 - not-checked - Outside this range.
- F09-5 - not-checked - Outside this range.
- F09-6 - not-checked - Outside this range.
- F11-1 - not-checked - Outside this range.
- F11-2 - not-checked - Outside this range.
- F11-3 - not-checked - Outside this range.
- F11-4 - not-checked - Outside this range.
- F11-5 - not-checked - Outside this range.
- F14-1 - not-checked - Outside this range.
- F19-1 - not-checked - Outside this range.
- F19-2 - not-checked - Outside this range.
- F19-3 - not-checked - Outside this range.
- F22-1 - fixed - C3-only cap-eviction replay is addressed; the corrected mixed-writer claim is finding #1, which supersedes it.
- F23-1 - not-checked - Outside this range.
- F23-2 - not-checked - Outside this range.
- F23-3 - not-checked - Outside this range.
- F23-4 - not-checked - Outside this range.
- F23-5 - not-checked - Outside this range.
- F24-1 - not-checked - Outside this range.
- F24-2 - not-checked - Outside this range.
- F24-3 - not-checked - Outside this range.
- F24-4 - not-checked - Outside this range.
- F24-5 - not-checked - Outside this range.
- F24-6 - not-checked - Outside this range.
- F25-1 - not-checked - Outside this range.
- F25-2 - not-checked - Outside this range.
- F25-3 - not-checked - Outside this range.
- F25-4 - not-checked - Outside this range.

## Verdict: HOLD

HOLD — finding #1 supersedes F22-1: the applied-key ledger is not durable across the plugin’s shared-journal writer.

### Blockers

- **F29-1** `crates/c3-core/src/health.rs:1682`, `crates/c3-core/src/health.rs:2017`, `crates/c3-core/src/health.rs:2045`, `crates/c3-core/src/health.rs:2189` - The applied-key list is only a C3-local replay guard: it is silently ignored when missing or malformed, and the plugin’s shared journal updater neither reads nor preserves it, so a retained journal record can still be replayed after endpoint-key eviction and applied twice. Verify: Create a kept journal record, evict its endpoint through the cap, remove or invalidate `.journal.applied`, run the plugin journal updater and then an update, and assert the record is neither re-added nor allowed to displace a newer endpoint. Remedy: Make consumed-journal state authoritative in the shared journal protocol, or fail closed when the retained journal has no trustworthy ledger; add a mixed C3/plugin crash-and-eviction fixture.
- **F14-1** (prior, not-checked) `crates/c3/src/telemetry/complaint.rs:542`, `crates/c3/src/telemetry/complaint.rs:240`, `crates/c3/src/telemetry/complaint.rs:673` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending'). Verify: Add a test: forget_with with ForgetRequest{public_ref: None, local: true, yes: true} and an injected remove failing on spool.ndjson; assert whether forget-pending.json exists and whether a subsequent flush posts the retained events. Remedy: In the None arm, persist the transaction (phase cleaning) via write_transaction before calling finish_cleanup (or pass phase confirmed so run_local_cleanup writes it), so an interruption leaves the blocking, resumable record the docs and the CLI message promise.
- **F23-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:97`, `crates/c3/src/liveness/proc.rs:100`, `crates/c3/src/liveness/pending.rs:283`, `crates/c3/src/liveness/pending.rs:344` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time. Verify: Inject `name_and_parent -> None` while `process_start_iso -> Some(start)` and require `test_unverified_process`/`test_recorded_process` to return running or an explicit inspection failure. Remedy: Distinguish process absence from inspection failure; return an unknown/running verdict when the pid exists but metadata cannot be read.
- **F23-2** (prior, not-checked) `crates/c3/src/liveness/proc.rs:753`, `crates/c3/src/liveness/proc.rs:775`, `crates/c3/src/liveness/proc.rs:827`, `crates/c3/src/liveness/pending.rs:790` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs. Verify: Run `test_pending_active` on a `kill_unconfirmed` record with a live scan row whose `created` is `None`; require refusal/failed verification, never release. Remedy: Treat unknown creation/start as blocking evidence or fail the scan; do not silently filter those rows.
- **F23-3** (prior, not-checked) `crates/c3/src/consult/orchestrate.rs:5105`, `crates/c3/src/consult/orchestrate.rs:5143`, `docs/port/wave3a-recovery.md:59` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree. Verify: Add a pause between the kill and the end-of-run pending write, terminate the bridge there, and inspect the recovery record for the expected `unverified`/`kill_unconfirmed` fields. Remedy: Persist the kill disposition at each kill site, or write a durable kill journal before continuing the run.
- **F24-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:640`, `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:704` - The spool-lock fallback marks all non-legacy not-spooled lines as seen without folding or retaining per-file provenance; a later fold counts the same gone-producer files again, so the accounting can double-count and temporarily hide the count. Verify: Hold `spool.lock` for over one second during flush 1 with N gone-producer lines, then run flush 2 and compare both records; require exactly one accounting of N. Remedy: In the fallback, retain per-file folded entries or mark only positively kept files seen; never advance the global baseline for files that remain foldable.
- **F25-1** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:197`, `crates/c3/src/engines/claude_auth.rs:311` - Endpoint-auth preflight reports available without proving the configured Claude launcher can run; only the token variable is checked, so a wrong or non-runnable launcher passes preflight and fails later. Verify: Run endpoint preflight with a non-executable launcher and require unavailable before the turn starts. Remedy: Run a cheap launcher probe such as `--version` or `auth status` in the endpoint child environment before returning available.
- **F25-2** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:357`, `crates/c3/src/engines/claude_auth.rs:374` - Endpoint mode cannot enforce the transcript-location safety guard because it never populates `projectsDirectory`; transcripts may land inside the repository and alter the tree being reviewed. Verify: Make endpoint-mode `auth status` report an in-repository projects directory and require refusal before spawn. Remedy: Probe the status facts for endpoint mode too, without reading or logging the token, and reject unsafe transcript locations.

### Unproven scenarios

- No tests were run in this read-only consultation.
- The mixed C3/plugin deletion-or-ignoring schedule was reasoned from code and documentation, not executed.
- The crash-between-write schedule was not dynamically reproduced.

### First-run checklist (observable)

_(none)_
