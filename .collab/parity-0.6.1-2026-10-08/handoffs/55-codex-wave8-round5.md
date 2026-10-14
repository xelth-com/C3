# Handoff 55 - Codex: wave8-round5

Date: 2026-10-14 11:13 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: be7dd93a-8ea4-4eca-be9a-ca03d8d6ffc8.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-8a6181d55516468db1c8c06bb7204102.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a139b0-4728-7a92-8124-fec6c8c0f2d9` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/53-claude-wave8-round5.md` (sha256 05c728156504). Reviewed: fe4c477, base fe4c4779a3957e58b8112a3633b5cf7a78456995, tree sha256 a66a41ccf536, 0 changed files.
Bridge outcome: usable reply. Wall time: 261.6 s. Tokens: in 257019 (cached 199040), out 5515, reasoning 1229.
Quota (rollout): 5h 8% until 16:14; wk 53% until Oct 20; this run 257019 input tokens (199040 cached) in 6 calls, max 57138 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 70 % of the 5-hour window.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `83ca5dd..e76518e` - the range changes 13 files, 2001 lines (1970 insertions, 31 deletions).
Verdict: HOLD - The targeted fixes address the reported comparisons, but cached selection can still hide an active blocking window, and malformed newer records can erase valid blocking evidence. Findings: 0 blocker, 1 major, 1 minor, 0 note (F55-1..F55-2, tracked in `findings.json`). Structured reply: `handoffs/55-codex-wave8-round5.reply.json`.
Raw event stream: `handoffs/55-codex-wave8-round5.events.jsonl`.
Verbatim reply follows.

---

1. **Q1:** Two remaining paths hide blocking windows. Finding #1: the account cache retains a time-dependent winner across its reset, losing another observation that still blocks. Finding #2: a newer record containing only an invalid percentage replaces valid blocking evidence before percentage validation. The new permutation tests cover neither path.

F52-1’s per-comparison clock sampling and F52-2’s equal-time NaN case are fixed by inspection. F46-1, F48-1 and F50-1’s specific cases are also addressed. Other prior findings were not rechecked.

2. **Q2: HOLD** tagging v0.2.1 on finding #1; also address finding #2.

Read: the brief, wave note, all four non-`.collab` diff paths, and targeted surrounding implementation/tests. Unread: the nine `.collab` diff paths, earlier handoffs, external plugin oracle and broader wave-8 implementation. Tests were not executed; reported pass counts remain coordinator evidence.

## Requested checks

- **RC1:** Working directory `C:\Users\Dmytro\C3`; workspace-write; budget 20 minutes. Add/run a test with two same-time openai rollouts: A=(100%, R), B=(99%, S), R<S. Call `account_window_at` before R, then between R/S within the cache TTL. Require the second result to retain B and block, matching a fresh-home evaluation (finding #1).
- **RC2:** Same directory; workspace-write; budget 10 minutes. Add/run an `endpoint_window_at` test with an older 99% future-reset record and a newer NaN-only record, in both orders. Require the valid block to survive (finding #2).

---

### Findings

- **F55-1** [major] `crates/c3-core/src/quota.rs:992`, `crates/c3-core/src/quota.rs:998`, `crates/c3-core/src/quota.rs:1038`, `crates/c3-core/src/quota.rs:1175` - The account cache stores only the winner selected at an earlier comparison instant and returns it for 60 seconds without considering the requested instant. After that winner resets, a discarded equal-time observation can still block, but endpoint health sees only the expired cached window and can report availability. Trigger: Two account rollouts have the same observed_at and primary windows A=(100%, reset R), B=(99%, reset S), with R<S. A lookup before R caches A. Another lookup in the same process and home occurs after R but before S, within the cache TTL, with no independent recorded block. It returns expired A instead of still-blocking B. Evidence: read-code: Ranking depends on now; account aggregation retains only the selected observation. Cache hits return v.clone() based solely on elapsed monotonic time, without using the supplied now.; read-code: Endpoint selection consumes the cached account window, while blocking evaluation treats resets at or before the current comparison instant as expired.; read-code: The new clock regression test recomputes from all observations at each instant; it does not exercise the account cache across a reset.; inferred: A warm-cache lookup between R and S returns nonblocking A, whereas an uncached aggregation at that same instant selects blocking B. Verify: Run RC1: compare warm-cache and fresh-home account_window_at results across R, requiring the 99% window ending at S to survive and block. Remedy: Cache the underlying observations and select them using each caller's instant, or invalidate the cached aggregate whenever a reset boundary that can change selection is crossed.
- **F55-2** [minor] `crates/c3-core/src/quota.rs:1094`, `crates/c3-core/src/quota.rs:1149`, `crates/c3-core/src/quota.rs:872`, `crates/c3-core/src/quota.rs:2138` - Invalid percentages are not treated as absent before recorded-observation selection. A strictly newer record with no valid windows can replace an older valid blocking record; subsequent window evaluation discards the invalid percentage and produces no block. Trigger: For one endpoint, an older observation has primary used_percent=99 with a future reset. A newer observation has primary used_percent="NaN" (or another rejected percentage) and secondary=null. Both record orders select the malformed newer observation and lose the valid block. Evidence: read-code: Recorded candidates are excluded only when quota is not an object or both window fields are null; a window object containing an invalid percentage passes this gate.; read-code: A strictly newer candidate replaces the current observation before quota_used applies finite_percent; validation occurs only in the equal-time merge.; read-code: Window evaluation skips invalid percentages, while the new NaN regression test gives both observations the same timestamp.; inferred: The malformed newer record becomes the selected quota but contributes no window state, suppressing the older future-reset block. Verify: Run RC2 with a newer NaN-only record and an older valid blocking record in both input orders; require the valid block to remain. Remedy: Normalize and validate recorded windows before candidate admission, and skip observations with no valid windows, consistently with the documented absent-window treatment.

### Prior findings

- F02-1 - not-checked - Telemetry spool is outside this review range.
- F02-3 - not-checked - Deletion retry behavior was not rechecked.
- F02-4 - not-checked - Harness implementation coverage was not rechecked.
- F02-5 - not-checked - Rating transaction ordering was not rechecked.
- F02-6 - not-checked - Prompt anchors were not rechecked.
- F02-7 - not-checked - Shared-plan identity was not rechecked.
- F02-8 - not-checked - No native Messages benchmark was reviewed.
- F04-1 - not-checked - Process-name Unicode handling was not rechecked.
- F04-2 - not-checked - Windows timezone boundary behavior was not rechecked.
- F04-3 - not-checked - Required-light panel metadata was not rechecked.
- F06-1 - not-checked - Historical Samoa timezone behavior was not rechecked.
- F09-1 - not-checked - Context-window configuration Unicode handling was not rechecked.
- F09-2 - not-checked - Flush/deletion synchronization was not rechecked.
- F09-3 - not-checked - Recoverable deletion cleanup was not rechecked.
- F09-4 - not-checked - Spool reread failure handling was not rechecked.
- F09-5 - not-checked - Legacy telemetry sanitization was not rechecked.
- F09-6 - not-checked - Rating actor inference was not rechecked.
- F11-1 - not-checked - Health journal I/O failure handling was not rechecked.
- F11-2 - not-checked - Required-reviewer launcher propagation was not rechecked.
- F11-3 - not-checked - Required-reviewer shared-plan evaluation was not rechecked.
- F11-4 - not-checked - PowerShell hook pointer execution was not rechecked.
- F11-5 - not-checked - AGY tool stall handling was not rechecked.
- F14-1 - not-checked - Local-only deletion transaction persistence was not rechecked.
- F19-1 - not-checked - Complaint/deletion synchronization was not rechecked.
- F19-2 - not-checked - Unreadable-salt cleanup behavior was not rechecked.
- F19-3 - not-checked - Duplicate-key telemetry sanitization was not rechecked.
- F22-1 - not-checked - Durable health-journal deduplication was not rechecked.
- F23-1 - not-checked - Process metadata lookup failure handling was not rechecked.
- F23-2 - not-checked - Unknown creation-time recovery behavior was not rechecked.
- F23-3 - not-checked - Kill evidence persistence was not rechecked.
- F23-4 - not-checked - Recovery test coverage was not rechecked.
- F23-5 - not-checked - Production Windows Samoa behavior was not rechecked.
- F24-1 - not-checked - Not-spooled fallback accounting was not rechecked.
- F24-2 - not-checked - Unterminated not-spooled suffix accounting was not rechecked.
- F24-3 - not-checked - Concurrent not-spooled cleanup was not rechecked.
- F24-4 - not-checked - Not-spooled status baseline behavior was not rechecked.
- F24-5 - not-checked - Plugin-home adapter and lock-domain coverage was not rechecked.
- F24-6 - not-checked - Post-fold record rewrite failure handling was not rechecked.
- F25-2 - not-checked - Claude endpoint transcript guard was not rechecked.
- F25-3 - not-checked - Claude model case normalization was not rechecked.
- F25-4 - not-checked - Child environment credential redaction was not rechecked.
- F30-1 - not-checked - Unknown-tree helper detection was not rechecked.
- F30-2 - not-checked - Pre-kill evidence durability was not rechecked.
- F30-3 - not-checked - Continuation and repair kill-site coverage was not rechecked.
- F31-1 - not-checked - Live-producer fallback accounting was not rechecked.
- F36-1 - not-checked - Transcript-path Unicode slicing was not rechecked.
- F36-2 - not-checked - Transcript-path NFC normalization was not rechecked.
- F38-1 - not-checked - Recovery after multiple persistence failures was not rechecked.
- F40-1 - not-checked - Kill-copy directory enumeration failures were not rechecked.
- F40-2 - not-checked - Kick acknowledgement after persistence failure was not rechecked.
- F42-1 - not-checked - Recovery-copy metadata failures were not rechecked.
- F46-1 - fixed - By inspection, equal-time valid observations merge per window rather than retaining the first; relevant regression tests were read, not run.
- F48-1 - fixed - By inspection, primary and secondary windows are selected independently, preserving the stated crossing-window case.
- F50-1 - fixed - By inspection, future reset information outranks missing reset information for tied valid observations.
- F52-1 - fixed - The reported within-fold clock sampling is removed: aggregation passes one instant, also shared with health evaluation. Finding #1 concerns separate cache reuse across instants.
- F52-2 - fixed - The reported equal-time NaN case is addressed by finite_percent before tie comparison. Finding #2 concerns strictly newer malformed observations.

## Verdict: HOLD

The targeted fixes address the reported comparisons, but cached selection can still hide an active blocking window, and malformed newer records can erase valid blocking evidence.

### Blockers

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
- **F40-1** (prior, not-checked) `crates/c3/src/liveness/pending.rs:170`, `crates/c3/src/liveness/pending.rs:173`, `crates/c3/src/consult/recovery.rs:71` - Recovery silently treats failure to enumerate the Codex-home kill-copy directory, or errors while iterating it, as absence of recovery copies. A durable copy naming a live helper can therefore be omitted and the task released. Verify: Extend the existing RC1 fixture to inject PermissionDenied and an entry-iteration error during kill-copy discovery; require recovery to refuse before writing the engine-turn marker while the helper remains alive. Remedy: Return discovery errors to all recovery consumers and refuse on non-NotFound directory or entry errors. Distinguish a genuinely absent copy directory from failed inspection; do not flatten away iteration errors.
- **F42-1** (prior, not-checked) `crates/c3/src/liveness/pending.rs:306`, `crates/c3/src/consult/recovery.rs:88`, `crates/c3/src/liveness/pending.rs:380` - A metadata inspection error on an enumerated recovery copy is treated as absence. read_pending_file uses Path::exists(), which converts metadata errors to false, and returns exists=false with no error. Recovery and findings checks then skip the copy, potentially releasing a task while a helper named only in that durable copy remains alive. Verify: Extend the existing RC1 integration fixture with a metadata-error injection after successful copy enumeration; require restart refusal before any engine-turn marker while the helper remains alive. Remedy: Replace the boolean existence precheck with an error-preserving read or metadata operation. Treat only a confirmed NotFound as absence; propagate other errors as unusable-record inspection failures. Add the copy-file failure fixture alongside the directory-error tests.

### Unproven scenarios

- Neither new finding was reproduced by executing Rust code; both follow from inspected control flow.
- The reported 34 quota tests and 823 workspace tests were not independently executed.
- The nine .collab paths in the diff, earlier handoffs, external plugin oracle, and broader wave-8 implementation were not reviewed.
- Earlier findings outside quota selection were not revalidated.

### First-run checklist (observable)

_(none)_
