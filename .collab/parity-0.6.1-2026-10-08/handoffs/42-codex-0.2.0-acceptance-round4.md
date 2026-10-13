# Handoff 42 - Codex: 0.2.0-acceptance-round4

Date: 2026-10-13 22:49 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: aa0f8dd5-cf42-45b8-b69d-08615c6c4430.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-a56346103e0849bab21d16aa89e99aeb.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13707-3dc5-7000-b663-35b94420ccc1` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/41-claude-0.2.0-acceptance-round4.md` (sha256 8588f3ed4d52). Reviewed: 43bc331, base 43bc331acc2020158da9cfa9f4ac291a4bedbb24, tree sha256 e2887e0353a2, 0 changed files.
Bridge outcome: usable reply. Wall time: 251.8 s. Tokens: in 436153 (cached 348032), out 5248, reasoning 672.
Quota (rollout): 5h 76% until 23:37; wk 28% until Oct 20; this run 436153 input tokens (348032 cached) in 9 calls, max 66857 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 85 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s. Range: `098c29e..98a8742` - the range changes 19 files, 2500 lines (2437 insertions, 63 deletions).
Verdict: HOLD - Directory enumeration and kick acknowledgement are fixed, but a copy-file metadata error can still hide durable recovery evidence and release the task. Findings: 1 blocker, 0 major, 0 minor, 0 note (F42-1, tracked in `findings.json`). Structured reply: `handoffs/42-codex-0.2.0-acceptance-round4.reply.json`.
Raw event stream: `handoffs/42-codex-0.2.0-acceptance-round4.events.jsonl`.
Verbatim reply follows.

---

**Q1.** Yes—finding #1. After successful directory enumeration, `read_pending_file` calls `Path::exists()`. A metadata error becomes `false`, producing no record and no error. `recovery::assess` silently skips that copy. If it alone names the surviving helper, recovery can release the task. This is distinct from F40-1’s directory-enumeration failure.

F40-1’s original trigger is fixed: directory-open and entry-iteration errors propagate to refusal. F40-2 is also fixed: both kick sites acknowledge `not killed`, and the caller reports the live root and exits 1. Existing test logs contain passing entries for both targeted integration tests and the three unit tests; I did not execute them.

**Q2. HOLD** on finding #1; do not tag yet.

I read handoffs 33/37/39/41, wave-note sections 7b/7c, the changelog delta, all non-`.collab` changed hunks, and targeted recovery/caller context. I did not review the `.collab` administrative delta, the whole release, older findings’ implementations, or complete verification logs. Context reads used the working tree, whose HEAD is `43bc331`; the requested diff was explicitly `098c29e..98a8742`.

## Requested checks

- **RC1:** In `C:\Users\Dmytro\C3`, extend the existing RC1 helper-survival fixture to inject `PermissionDenied` at the copy-file metadata lookup after successful enumeration, then restart with the same `CODEX_HOME`. Permission: workspace-write. Require an inspection refusal, retained copy, live helper, and no engine-turn marker; restore access and verify refusal names the helper. Settles finding #1. Budget: one fixture, 30 minutes.

---

### Findings

- **F42-1** [blocker] `crates/c3/src/liveness/pending.rs:306`, `crates/c3/src/consult/recovery.rs:88`, `crates/c3/src/liveness/pending.rs:380` - A metadata inspection error on an enumerated recovery copy is treated as absence. read_pending_file uses Path::exists(), which converts metadata errors to false, and returns exists=false with no error. Recovery and findings checks then skip the copy, potentially releasing a task while a helper named only in that durable copy remains alive. Trigger: The Codex-home copy directory enumerates successfully, but metadata lookup for a listed copy fails, for example through a transient access failure after enumeration. The copy alone names a surviving generic helper; the collab launch record names dead ancestors and no independent process match blocks recovery. Evidence: read-code: Copy discovery retains matching paths, including unreadable UUID-named copies, but read_pending_file subsequently returns an error-free absent result when path.exists() is false.; read-code: Recovery continues when the returned record is None; findings checks continue when exists is false.; inferred: A metadata failure is indistinguishable from absence at this branch. If the skipped copy contains the only surviving-helper evidence, its liveness is never checked. This filesystem failure was not reproduced during the read-only review. Verify: Extend the existing RC1 integration fixture with a metadata-error injection after successful copy enumeration; require restart refusal before any engine-turn marker while the helper remains alive. Remedy: Replace the boolean existence precheck with an error-preserving read or metadata operation. Treat only a confirmed NotFound as absence; propagate other errors as unusable-record inspection failures. Add the copy-file failure fixture alongside the directory-error tests.

### Prior findings

- F02-1 - not-checked - Older telemetry implementation outside this round's reviewed delta.
- F02-3 - not-checked - Older deletion implementation not revalidated.
- F02-4 - not-checked - Harness coverage map not revalidated.
- F02-5 - not-checked - Rating transaction ordering not revalidated.
- F02-6 - not-checked - Prompt anchor implementation not revalidated.
- F02-7 - not-checked - Plan identity implementation not revalidated.
- F02-8 - not-checked - Native-route benchmark not reviewed; handoff 33 excludes native Messages from this release.
- F04-1 - not-checked - Unicode process classifier not revalidated.
- F04-2 - not-checked - Production Windows DST behavior not revalidated.
- F04-3 - not-checked - Required-light ledger metadata not revalidated.
- F06-1 - not-checked - Historical Samoa normalization not revalidated.
- F09-1 - not-checked - Unicode configuration handling not revalidated.
- F09-2 - not-checked - Flush/deletion locking not revalidated.
- F09-3 - not-checked - Recoverable deletion cleanup not revalidated.
- F09-4 - not-checked - Spool reread failure handling not revalidated.
- F09-5 - not-checked - Legacy event sanitization not revalidated.
- F09-6 - not-checked - Rating actor inference not revalidated.
- F11-1 - not-checked - Health journal I/O failure handling not revalidated.
- F11-2 - not-checked - Required-reviewer launcher propagation not revalidated.
- F11-3 - not-checked - Shared-plan quota evaluation not revalidated.
- F11-4 - not-checked - PowerShell hook pointer not revalidated.
- F11-5 - not-checked - AGY tool stall allowance not revalidated.
- F14-1 - not-checked - Local-only deletion transaction not revalidated.
- F19-1 - not-checked - Complaint/deletion synchronization not revalidated.
- F19-2 - not-checked - Unreadable salt cleanup not revalidated.
- F19-3 - not-checked - Duplicate-key sanitization not revalidated.
- F22-1 - not-checked - Durable journal replay deduplication not revalidated.
- F23-1 - not-checked - Process metadata lookup failure handling not revalidated.
- F23-2 - not-checked - Unknown creation-time scan behavior not revalidated.
- F23-3 - not-checked - Pre-kill durability described in wave 7b; its implementation was not re-reviewed.
- F23-4 - not-checked - Earlier deterministic recovery fixtures not revalidated.
- F23-5 - not-checked - Production Samoa tests not reviewed.
- F24-1 - not-checked - Not-spooled fallback accounting not revalidated.
- F24-2 - not-checked - Unterminated suffix accounting not revalidated.
- F24-3 - not-checked - Concurrent not-spooled cleanup not revalidated.
- F24-4 - not-checked - Unseen-suffix status reporting not revalidated.
- F24-5 - not-checked - P8 marker, layout and mixed-bridge coverage not revalidated.
- F24-6 - not-checked - Second fold-record rewrite failure not revalidated.
- F25-2 - not-checked - Endpoint transcript-location guard not revalidated.
- F25-3 - not-checked - Claude model case normalization not revalidated.
- F25-4 - not-checked - Child environment Debug redaction not revalidated.
- F30-1 - not-checked - Unknown-tree helper classification not revalidated.
- F30-2 - not-checked - Pre-kill write policy documented, but all kill sites and failure branches were not re-reviewed.
- F30-3 - not-checked - Continuation and repair durability fixtures not revalidated.
- F31-1 - not-checked - Live-producer accounting not revalidated.
- F36-1 - not-checked - Handoff reports fixed; Unicode path implementation not re-reviewed.
- F36-2 - not-checked - Handoff reports fixed; NFC guard implementation not re-reviewed.
- F38-1 - not-checked - Wave 7b policy and RC1 context read; complete pre-kill implementation not revalidated.
- F40-1 - fixed - Original directory-open and entry-iteration failures now propagate through PendingPaths.error to refusal. Reviewed tests and existing passing log entries. Finding #1 concerns the separate copy-file metadata path.
- F40-2 - fixed - Both kick sites emit not killed for KillCheck.not_killed; the caller reports the live root and returns 1. Reviewed integration assertions and existing passing test log.

## Verdict: HOLD

Directory enumeration and kick acknowledgement are fixed, but a copy-file metadata error can still hide durable recovery evidence and release the task.

### Blockers

- **F42-1** `crates/c3/src/liveness/pending.rs:306`, `crates/c3/src/consult/recovery.rs:88`, `crates/c3/src/liveness/pending.rs:380` - A metadata inspection error on an enumerated recovery copy is treated as absence. read_pending_file uses Path::exists(), which converts metadata errors to false, and returns exists=false with no error. Recovery and findings checks then skip the copy, potentially releasing a task while a helper named only in that durable copy remains alive. Verify: Extend the existing RC1 integration fixture with a metadata-error injection after successful copy enumeration; require restart refusal before any engine-turn marker while the helper remains alive. Remedy: Replace the boolean existence precheck with an error-preserving read or metadata operation. Treat only a confirmed NotFound as absence; propagate other errors as unusable-record inspection failures. Add the copy-file failure fixture alongside the directory-error tests.
- **F14-1** (prior, not-checked) `crates/c3/src/telemetry/complaint.rs:542`, `crates/c3/src/telemetry/complaint.rs:240`, `crates/c3/src/telemetry/complaint.rs:673` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending'). Verify: Add a test: forget_with with ForgetRequest{public_ref: None, local: true, yes: true} and an injected remove failing on spool.ndjson; assert whether forget-pending.json exists and whether a subsequent flush posts the retained events. Remedy: In the None arm, persist the transaction (phase cleaning) via write_transaction before calling finish_cleanup (or pass phase confirmed so run_local_cleanup writes it), so an interruption leaves the blocking, resumable record the docs and the CLI message promise.
- **F22-1** (prior, not-checked) `crates/c3-core/src/health.rs:1563`, `crates/c3-core/src/health.rs:1915`, `crates/c3-core/src/health.rs:1942`, `crates/c3-core/src/health.rs:1960`, `crates/c3-core/src/health.rs:2028` - After failed `.bad` archival, the retained journal is replayed against mutable endpoint state whose record keys can be removed by the 24-hour or 500-record retention rules. A journal record can therefore be applied, evicted, and re-applied with a different final effect, while a record already stale on a replay is parsed but never becomes visible. The record-key dedup is not durable, so the documented harmless/once replay invariant is false. Verify: Add a deterministic test with an unwritable `.bad` path, one recent journal record, and 500 newer endpoint records; run two updates and assert the journal record is neither resurrected nor allowed to displace another endpoint. Remedy: Persist a monotonic journal applied-sequence marker or equivalent durable consumed state, and truncate the journal by that sequence independently of endpoint retention; do not use the mutable endpoint set as the only replay ledger.
- **F23-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:97`, `crates/c3/src/liveness/proc.rs:100`, `crates/c3/src/liveness/pending.rs:283`, `crates/c3/src/liveness/pending.rs:344` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time. Verify: Inject `name_and_parent -> None` while `process_start_iso -> Some(start)` and require `test_unverified_process`/`test_recorded_process` to return running or an explicit inspection failure. Remedy: Distinguish process absence from inspection failure; return an unknown/running verdict when the pid exists but metadata cannot be read.
- **F23-2** (prior, not-checked) `crates/c3/src/liveness/proc.rs:753`, `crates/c3/src/liveness/proc.rs:775`, `crates/c3/src/liveness/proc.rs:827`, `crates/c3/src/liveness/pending.rs:790` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs. Verify: Run `test_pending_active` on a `kill_unconfirmed` record with a live scan row whose `created` is `None`; require refusal/failed verification, never release. Remedy: Treat unknown creation/start as blocking evidence or fail the scan; do not silently filter those rows.
- **F23-3** (prior, not-checked) `crates/c3/src/consult/orchestrate.rs:5105`, `crates/c3/src/consult/orchestrate.rs:5143`, `docs/port/wave3a-recovery.md:59` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree. Verify: Add a pause between the kill and the end-of-run pending write, terminate the bridge there, and inspect the recovery record for the expected `unverified`/`kill_unconfirmed` fields. Remedy: Persist the kill disposition at each kill site, or write a durable kill journal before continuing the run.
- **F24-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:640`, `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:704` - The spool-lock fallback marks all non-legacy not-spooled lines as seen without folding or retaining per-file provenance; a later fold counts the same gone-producer files again, so the accounting can double-count and temporarily hide the count. Verify: Hold `spool.lock` for over one second during flush 1 with N gone-producer lines, then run flush 2 and compare both records; require exactly one accounting of N. Remedy: In the fallback, retain per-file folded entries or mark only positively kept files seen; never advance the global baseline for files that remain foldable.
- **F25-2** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:357`, `crates/c3/src/engines/claude_auth.rs:374` - Endpoint mode cannot enforce the transcript-location safety guard because it never populates `projectsDirectory`; transcripts may land inside the repository and alter the tree being reviewed. Verify: Make endpoint-mode `auth status` report an in-repository projects directory and require refusal before spawn. Remedy: Probe the status facts for endpoint mode too, without reading or logging the token, and reject unsafe transcript locations.
- **F30-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:815`, `crates/c3/src/liveness/pending.rs:297`, `crates/c3/src/liveness/pending.rs:902` - Unknown-tree release remains fail-open for a live reviewer helper whose creation time is unreadable and whose name or command line does not match the codex rule, especially when it is a grandchild below an unrecorded intermediate process. Verify: Run `test_pending_active_with` with the wrapper/helper synthetic table and require the record to remain active; repeat with a readable non-codex command line. Remedy: Scan full descendant relationships and treat unknown-start or command-line-unreadable/generic-runtime rows as blocking unless positive evidence proves they are unrelated.
- **F30-2** (prior, not-checked) `crates/c3/src/engines/subprocess.rs:536`, `crates/c3/src/consult/orchestrate.rs:587`, `crates/c3/src/consult/orchestrate.rs:4927`, `crates/c3/src/consult/orchestrate.rs:5674` - Kill evidence is not durable at the actual kill: the processes are killed before the kept record is written, and write failures are ignored while execution continues. Verify: Add a pause between `kill_tree_checked` and `KillSite::write`, terminate there, and separately inject `write_pending` errors before continuation or commit. Remedy: Persist the kill record synchronously from the kill path or an append-only sidecar before returning, and fail closed when that persistence fails.
- **F31-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:886`, `crates/c3/src/telemetry/notspooled.rs:950` - The lock-busy fallback records live producers' existing lines as seen without recording per-file provenance; when such a producer later exits, the next fold counts its entire file again as a gone producer, double-counting those lines across the record history. Verify: Run the RC1 schedule and compare the first record’s `not_spooled_seen`, the second record’s fold note, and the total accounting for the N lines. Remedy: Persist per-file seen byte/line prefixes or carry the fallback’s kept-file provenance into the next fold, and subtract those already-seen lines when folding a formerly-live producer.
- **F36-1** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:425`, `crates/c3/src/engines/claude_auth.rs:540`, `crates/c3-core/src/paths.rs:41` - The new Claude transcript guard can panic on a valid Unicode configuration path because repo_relative slices the candidate string at the byte length of an unrelated repository prefix without checking a UTF-8 boundary. Verify: Run RC1 and assert the guard returns normally for the stated paths. Remedy: Use boundary-safe prefix comparison, such as str::get before comparing, or an appropriate path-component comparison; add the Unicode regression.
- **F38-1** (prior, not-checked) `crates/c3/src/consult/orchestrate.rs:687`, `crates/c3/src/engines/subprocess.rs:912`, `crates/c3/src/liveness/pending.rs:1026`, `crates/c3/src/liveness/proc.rs:878`, `crates/c3-cli/tests/kill_record_durable.rs:589` - The fallback narrows but does not close the recovery durability failure: if both pre-kill writes fail and the bridge dies after killing an unrecorded intermediate, recovery can release the task while that intermediate's non-Codex helper remains alive. The surviving launch record contains insufficient ancestry, and the in-memory KillRecord::Lost result does not survive the bridge. Verify: Run RC1 from Requested checks with an isolated process table or controlled Windows tree, proving the helper remains alive and requiring the restarted task to refuse before spawning another engine turn. Remedy: Make an interrupted launch remain conservatively blocked when complete kill evidence cannot be persisted. Establish that recovery obligation durably before spawning, or require explicit operator release of an unresolved launch record instead of treating these empty scans as proof of termination. Add the missing dead-intermediate regression.

### Unproven scenarios

- The copy-file metadata-error schedule in finding #1 has not been executed.
- Tests were inspected and existing logs sampled, not rerun; the complete suite and reported flaky-test rerun were not independently verified.
- Panel passes and findings mutation consumers were traced through shared error handling, but no fresh end-to-end failure runs were performed.
- Older findings and the full compatibility release were not revalidated in this bounded review.
- Targeted surrounding-code reads used working-tree HEAD 43bc331; unchanged context was not independently compared with 98a8742.
- Documented mixed-bridge limits remain: plugin 0.6.1 does not recognize C3's not killed acknowledgement or discover Codex-home recovery copies.

### First-run checklist (observable)

- [ ] With a surviving helper and unreadable recovery evidence, observe an inspection refusal naming the affected directory or file, a nonzero exit, retained recovery evidence, and no engine-turn marker.
- [ ] After restoring access, observe refusal naming the surviving helper PID; only after that helper exits should recovery consume the copy and a new engine-turn marker appear.
- [ ] When all three pre-kill writes fail during a kick, observe result=not killed, the live root PID and failure reason, kick exit 1, root still alive, and no committed consultation.
- [ ] On the first ordinary successful run, verify the expected reviewer actually executed, produced a usable reply, and committed the corresponding ledger entry; exit 0 alone is insufficient.
