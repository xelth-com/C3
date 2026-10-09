# Handoff 31 - Codex: wave3b-round2-mimo

Date: 2026-10-09 11:46 local. Author: Codex (model mimo-v2.6-pro, effort high), Codex CLI 0.155.1.
Reviewer: mimo :: mimo-v2.6-pro (provider from -Provider, model from roster; endpoint https://token-plan-ams.xiaomimimo.com/v1, wire_api: responses; provider fingerprint 47cd6ee7e4ff; harness codex-cli 0.155.1).
Preflight: ok: env MIMO_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 3 of 12 for -Provider mimo (model, codex_config applied).
Effort: high sent (requested high, mapping mimo-v1, by caps-v1: token-plan-ams.xiaomimimo.com, mimo-v2.6-pro; not confirmed by the provider). Consultation id: baeec94c-985e-4fae-aa6a-8a93a557db79.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m mimo-v2.6-pro -c model_reasoning_effort="high" -c model_provider="mimo" -c model_catalog_json="C:/Users/Dmytro/.codex/model-catalogs.json" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-7782bd2778184b45bb4e8924bcd93e12.md fork 01a11ff9-827f-7623-9c7e-575292039954 -` (prompt on stdin).
Parent thread: `01a11ff9-827f-7623-9c7e-575292039954`. Result thread: `01a1200e-f244-7f73-b835-3d55a903f967` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/28-claude-wave3b-round2.md` (sha256 ba8486c57689). Reviewed: c8f0997, base c8f099712ae2aa9127a1c7b73cc997e77441ef55, tree sha256 56d7ef6cea2e, 0 changed files.
Bridge outcome: usable reply. Wall time: 766.9 s. Tokens: in 28504604 (cached 20952576), out 91610, reasoning 55733.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `fda1f33..e9f3127` - the range changes 7 files, 1166 lines (1108 insertions, 58 deletions).
Verdict: HOLD - HOLD — finding #1 supersedes F24-1: a producer that is live during a lock-busy flush and gone at the next fold can still be accounted twice. Findings: 1 blocker, 0 major, 0 minor, 0 note (F31-1, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/31-codex-wave3b-round2-mimo.reply.json`.
Raw event stream: `handoffs/31-codex-wave3b-round2-mimo.events.jsonl`.
Verbatim reply follows.

---

**Q1.** Yes. During a lock-busy flush, `kept_seen` correctly records the live producer’s lines as seen and leaves gone files unseen. If that producer exits before the next successful fold, however, `merge` treats its whole file as a gone file and counts every line in the fold note, including the lines already recorded as seen by the earlier fallback. The same N events are therefore represented once in `not_spooled_seen` and again in `folded N ...`; the count is not lost, but it is doubled in the accounting history.

**Q2.** HOLD until the live-to-gone transition is accounted exactly once. F24-2, F24-3, F24-4, F24-5, and F24-6 otherwise look materially improved.

## Requested checks
`RC1` (finding #1) — In `C:\Users\Dmytro\C3`, workspace-write, hold `spool.lock` through flush 1 with one live producer file containing N lines, let that producer exit, then run flush 2 and inspect `last-flush.json`; require N total accounting, not `not_spooled_seen=N` plus `folded N`. Budget: 20 minutes.

---

### Findings

- **F31-1** [blocker] `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:886`, `crates/c3/src/telemetry/notspooled.rs:950` - The lock-busy fallback records live producers' existing lines as seen without recording per-file provenance; when such a producer later exits, the next fold counts its entire file again as a gone producer, double-counting those lines across the record history. Trigger: A producer has N not-spooled lines while `spool.lock` is busy during flush 1, then exits before flush 2 obtains the lock and folds its file. Evidence: read-code: The no-lock fallback writes only `not_spooled_seen` for files a fold would keep, with no durable per-file prefix record.; read-code: A gone producer file is counted in full by `merge`, regardless of lines previously included in `not_spooled_seen`.; inferred: The live-to-gone transition makes the same N lines appear in both the earlier seen baseline and the later fold note. Verify: Run the RC1 schedule and compare the first record’s `not_spooled_seen`, the second record’s fold note, and the total accounting for the N lines. Remedy: Persist per-file seen byte/line prefixes or carry the fallback’s kept-file provenance into the next fold, and subtract those already-seen lines when folding a formerly-live producer. Supersedes: F24-1.

### Prior findings

- F02-1 - not-checked - Outside fda1f33..e9f3127.
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
- F22-1 - not-checked - Outside this range.
- F23-1 - not-checked - Outside this range.
- F23-2 - not-checked - Outside this range.
- F23-3 - not-checked - Outside this range.
- F23-4 - not-checked - Outside this range.
- F23-5 - not-checked - Outside this range.
- F24-1 - fixed - The original fallback-wide seen bug is fixed; the corrected live-to-gone claim is finding #1, which supersedes it.
- F24-2 - fixed - Recorded-file tails are counted.
- F24-3 - fixed - Counts are gated during deletion and cleanup rescans files.
- F24-4 - fixed - Status selects an unseen line.
- F24-5 - fixed - Plugin-home marker and production-root fixtures are added.
- F24-6 - fixed - Post-delete rewrite failures are surfaced and staged names avoid stale names.
- F25-1 - not-checked - Outside this range.
- F25-2 - not-checked - Outside this range.
- F25-3 - not-checked - Outside this range.
- F25-4 - not-checked - Outside this range.
- F29-1 - not-checked - Outside this range.
- F30-1 - not-checked - Outside this range.
- F30-2 - not-checked - Outside this range.
- F30-3 - not-checked - Outside this range.

## Verdict: HOLD

HOLD — finding #1 supersedes F24-1: a producer that is live during a lock-busy flush and gone at the next fold can still be accounted twice.

### Blockers

- **F31-1** `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:886`, `crates/c3/src/telemetry/notspooled.rs:950` - The lock-busy fallback records live producers' existing lines as seen without recording per-file provenance; when such a producer later exits, the next fold counts its entire file again as a gone producer, double-counting those lines across the record history. Verify: Run the RC1 schedule and compare the first record’s `not_spooled_seen`, the second record’s fold note, and the total accounting for the N lines. Remedy: Persist per-file seen byte/line prefixes or carry the fallback’s kept-file provenance into the next fold, and subtract those already-seen lines when folding a formerly-live producer.
- **F14-1** (prior, not-checked) `crates/c3/src/telemetry/complaint.rs:542`, `crates/c3/src/telemetry/complaint.rs:240`, `crates/c3/src/telemetry/complaint.rs:673` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending'). Verify: Add a test: forget_with with ForgetRequest{public_ref: None, local: true, yes: true} and an injected remove failing on spool.ndjson; assert whether forget-pending.json exists and whether a subsequent flush posts the retained events. Remedy: In the None arm, persist the transaction (phase cleaning) via write_transaction before calling finish_cleanup (or pass phase confirmed so run_local_cleanup writes it), so an interruption leaves the blocking, resumable record the docs and the CLI message promise.
- **F22-1** (prior, not-checked) `crates/c3-core/src/health.rs:1563`, `crates/c3-core/src/health.rs:1915`, `crates/c3-core/src/health.rs:1942`, `crates/c3-core/src/health.rs:1960`, `crates/c3-core/src/health.rs:2028` - After failed `.bad` archival, the retained journal is replayed against mutable endpoint state whose record keys can be removed by the 24-hour or 500-record retention rules. A journal record can therefore be applied, evicted, and re-applied with a different final effect, while a record already stale on a replay is parsed but never becomes visible. The record-key dedup is not durable, so the documented harmless/once replay invariant is false. Verify: Add a deterministic test with an unwritable `.bad` path, one recent journal record, and 500 newer endpoint records; run two updates and assert the journal record is neither resurrected nor allowed to displace another endpoint. Remedy: Persist a monotonic journal applied-sequence marker or equivalent durable consumed state, and truncate the journal by that sequence independently of endpoint retention; do not use the mutable endpoint set as the only replay ledger.
- **F23-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:97`, `crates/c3/src/liveness/proc.rs:100`, `crates/c3/src/liveness/pending.rs:283`, `crates/c3/src/liveness/pending.rs:344` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time. Verify: Inject `name_and_parent -> None` while `process_start_iso -> Some(start)` and require `test_unverified_process`/`test_recorded_process` to return running or an explicit inspection failure. Remedy: Distinguish process absence from inspection failure; return an unknown/running verdict when the pid exists but metadata cannot be read.
- **F23-2** (prior, not-checked) `crates/c3/src/liveness/proc.rs:753`, `crates/c3/src/liveness/proc.rs:775`, `crates/c3/src/liveness/proc.rs:827`, `crates/c3/src/liveness/pending.rs:790` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs. Verify: Run `test_pending_active` on a `kill_unconfirmed` record with a live scan row whose `created` is `None`; require refusal/failed verification, never release. Remedy: Treat unknown creation/start as blocking evidence or fail the scan; do not silently filter those rows.
- **F23-3** (prior, not-checked) `crates/c3/src/consult/orchestrate.rs:5105`, `crates/c3/src/consult/orchestrate.rs:5143`, `docs/port/wave3a-recovery.md:59` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree. Verify: Add a pause between the kill and the end-of-run pending write, terminate the bridge there, and inspect the recovery record for the expected `unverified`/`kill_unconfirmed` fields. Remedy: Persist the kill disposition at each kill site, or write a durable kill journal before continuing the run.
- **F25-1** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:197`, `crates/c3/src/engines/claude_auth.rs:311` - Endpoint-auth preflight reports available without proving the configured Claude launcher can run; only the token variable is checked, so a wrong or non-runnable launcher passes preflight and fails later. Verify: Run endpoint preflight with a non-executable launcher and require unavailable before the turn starts. Remedy: Run a cheap launcher probe such as `--version` or `auth status` in the endpoint child environment before returning available.
- **F25-2** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:357`, `crates/c3/src/engines/claude_auth.rs:374` - Endpoint mode cannot enforce the transcript-location safety guard because it never populates `projectsDirectory`; transcripts may land inside the repository and alter the tree being reviewed. Verify: Make endpoint-mode `auth status` report an in-repository projects directory and require refusal before spawn. Remedy: Probe the status facts for endpoint mode too, without reading or logging the token, and reject unsafe transcript locations.
- **F29-1** (prior, not-checked) `crates/c3-core/src/health.rs:1682`, `crates/c3-core/src/health.rs:2017`, `crates/c3-core/src/health.rs:2045`, `crates/c3-core/src/health.rs:2189` - The applied-key list is only a C3-local replay guard: it is silently ignored when missing or malformed, and the plugin’s shared journal updater neither reads nor preserves it, so a retained journal record can still be replayed after endpoint-key eviction and applied twice. Verify: Create a kept journal record, evict its endpoint through the cap, remove or invalidate `.journal.applied`, run the plugin journal updater and then an update, and assert the record is neither re-added nor allowed to displace a newer endpoint. Remedy: Make consumed-journal state authoritative in the shared journal protocol, or fail closed when the retained journal has no trustworthy ledger; add a mixed C3/plugin crash-and-eviction fixture.
- **F30-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:815`, `crates/c3/src/liveness/pending.rs:297`, `crates/c3/src/liveness/pending.rs:902` - Unknown-tree release remains fail-open for a live reviewer helper whose creation time is unreadable and whose name or command line does not match the codex rule, especially when it is a grandchild below an unrecorded intermediate process. Verify: Run `test_pending_active_with` with the wrapper/helper synthetic table and require the record to remain active; repeat with a readable non-codex command line. Remedy: Scan full descendant relationships and treat unknown-start or command-line-unreadable/generic-runtime rows as blocking unless positive evidence proves they are unrelated.
- **F30-2** (prior, not-checked) `crates/c3/src/engines/subprocess.rs:536`, `crates/c3/src/consult/orchestrate.rs:587`, `crates/c3/src/consult/orchestrate.rs:4927`, `crates/c3/src/consult/orchestrate.rs:5674` - Kill evidence is not durable at the actual kill: the processes are killed before the kept record is written, and write failures are ignored while execution continues. Verify: Add a pause between `kill_tree_checked` and `KillSite::write`, terminate there, and separately inject `write_pending` errors before continuation or commit. Remedy: Persist the kill record synchronously from the kill path or an append-only sidecar before returning, and fail closed when that persistence fails.

### Unproven scenarios

- No tests were run in this read-only consultation.
- The live-to-gone double-accounting schedule was reasoned from code and documentation, not executed.
- Concurrent legacy/plugin producers during cleanup remain outside the C3-only append contract.

### First-run checklist (observable)

_(none)_
