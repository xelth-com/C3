# Handoff 58 - Codex: wave8-round6

Date: 2026-10-14 15:59 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 63a5a6ef-9331-4cdc-bf5e-f9fc01918426.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-c61b4a588d334421b67f6919af9f47d1.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13ab5-c462-7962-8f48-21984326887a` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/56-claude-wave8-round6.md` (sha256 5d534d4a4f1f). Reviewed: 1949e02, base 1949e02b01c028f59fdaa152d3a9de5204bae3fd, tree sha256 de846f00ec0c, 0 changed files.
Bridge outcome: usable reply. Wall time: 182.8 s. Tokens: in 247059 (cached 193280), out 4210, reasoning 473.
Quota (rollout): 5h 86% until 16:14; wk 65% until Oct 20; this run 247059 input tokens (193280 cached) in 6 calls, max 52948 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 97 % of the 5-hour window.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `e76518e..cc03617` - the range changes 16 files, 2315 lines (2192 insertions, 123 deletions).
Verdict: HOLD - The partial-window merge loses per-window observation timestamps, allowing input order to suppress an active quota block. Findings: 0 blocker, 1 major, 0 minor, 0 note (F58-1, tracked in `findings.json`). Structured reply: `handoffs/58-codex-wave8-round6.reply.json`.
Raw event stream: `handoffs/58-codex-wave8-round6.events.jsonl`.
Verbatim reply follows.

---

1. **Yes: finding #1 identifies a remaining replacement failure.** With observation times t1 < t2 < t3, use A=(primary 30%, t1), B=(secondary 10%, t3), and C=(primary 99%, t2), all with future resets. Folding A→B→C retains primary 30%; A→C→B retains 99%. Merging B promotes A’s retained primary to t3, so C cannot replace it. Both endpoint selection and rollout aggregation use this logic.

   F55-1’s cache redesign fixes its reported reset-crossing case. F55-2’s original invalid-only replacement case is also fixed. The new tests cover two-record partial merges and permutations with an invalid-only newer record; they do not cover three distinct timestamps with a valid partial newest observation. A small in-memory model reproduced the divergent results; Rust tests were not executed.

2. **HOLD tagging v0.2.1 pending finding #1.** Preserve each window’s observation timestamp throughout aggregation, including the account-to-endpoint handoff.

Read: the handoff, wave note, complete quota diff, both documentation diffs, and targeted surrounding code. Skipped: the 13 changed `.collab` artifacts as instructed, historical handoffs, upstream oracle sources, and unchanged implementations outside those excerpts. Unrelated prior findings remain unverified.

## Requested checks

- **RC1:** From `C:\Users\Dmytro\C3`, add and run a regression covering all six A/B/C permutations through endpoint selection and rollout aggregation, plus a recorded/live split. Permission: workspace-write. Require primary 99% and an active block in every case. Budget: one focused test, 15 minutes; confirms finding #1.

---

### Findings

- **F58-1** [major] `crates/c3-core/src/quota.rs:680`, `crates/c3-core/src/quota.rs:697`, `crates/c3-core/src/quota.rs:1108`, `crates/c3-core/src/quota.rs:1152`, `crates/c3-core/src/quota.rs:1206` - Partial-window aggregation retains older windows but assigns the aggregate the newest observation timestamp. Subsequent comparisons therefore treat an older retained window as newer than it actually is, making aggregation order-dependent and allowing a valid blocking window to be hidden. This affects both recorded endpoint selection and rollout/account aggregation, and the aggregate timestamp is also passed into recorded/live selection. Trigger: For one endpoint, use three observations with t1 < t2 < t3: A has primary=30 at t1 and no secondary; B has secondary=10 at t3 and no primary; C has primary=99 at t2 and no secondary. Every present window has a future reset. A→B→C produces primary=30 and secondary=10, while A→C→B produces primary=99 and secondary=10. Evidence: read-code: merge_observation fills missing newer windows from the older observation but stores only newer.at; later replacement compares that shared timestamp.; read-code: select_window likewise retains older window values while advancing out.observed_at to the maximum observation time.; read-code: Endpoint selection folds consultations in their input order, then merges the account result using its single aggregate observed_at.; ran-command: The model returned {at:3,p:30,s:10} for A/B/C and {at:3,p:99,s:10} for A/C/B. This was a branch model, not execution of the Rust implementation.; read-code: The expanded permutation fixture adds an invalid-only newer record, while the partial-window regression merges only two observations; neither exercises retained-window timestamp promotion followed by an intermediate-time observation. Verify: Run RC1 with all six permutations and a recorded/live partition; every result must retain the primary 99% window observed at t2 and report an active block. Remedy: Track observation timestamps separately for primary and secondary windows, comparing each candidate against the timestamp of that specific retained window. Preserve that provenance across account aggregation and recorded/live merging, or retain raw candidates until final per-window selection.

### Prior findings

- F02-1 - not-checked - Telemetry implementation is outside this diff review.
- F02-3 - not-checked - Deletion implementation is outside this diff review.
- F02-4 - not-checked - Harness coverage mapping was not audited.
- F02-5 - not-checked - Rating transaction ordering was not audited.
- F02-6 - not-checked - Prompt anchors were not audited.
- F02-7 - not-checked - Roster plan compatibility was not audited.
- F02-8 - not-checked - No three-route benchmark evidence was examined.
- F04-1 - not-checked - Process classification is outside this diff.
- F04-2 - not-checked - Windows timezone behavior was not exercised.
- F04-3 - not-checked - Panel ledger metadata was not audited.
- F06-1 - not-checked - Historical Samoa handling was not audited.
- F09-1 - not-checked - Configuration parsing is outside this diff.
- F09-2 - not-checked - Telemetry deletion locking was not audited.
- F09-3 - not-checked - Deletion cleanup durability was not audited.
- F09-4 - not-checked - Spool reread failure handling was not audited.
- F09-5 - not-checked - Legacy telemetry sanitization was not audited.
- F09-6 - not-checked - Rating actor inference was not audited.
- F11-1 - not-checked - Health journal I/O recovery was not audited.
- F11-2 - not-checked - Required-reviewer launcher resolution was not audited.
- F11-3 - not-checked - Shared-plan availability evaluation was not audited.
- F11-4 - not-checked - PowerShell hook commands were not exercised.
- F11-5 - not-checked - AGY stall handling was not audited.
- F14-1 - not-checked - Local deletion transaction persistence was not audited.
- F19-1 - not-checked - Complaint/deletion synchronization was not audited.
- F19-2 - not-checked - Salt read failure handling was not audited.
- F19-3 - not-checked - Duplicate-key telemetry sanitization was not audited.
- F22-1 - not-checked - Journal replay deduplication was not audited.
- F23-1 - not-checked - Process metadata failure handling was not audited.
- F23-2 - not-checked - Unknown-start recovery scans were not audited.
- F23-3 - not-checked - Kill evidence durability was not audited.
- F23-4 - not-checked - Recovery test coverage was not audited.
- F23-5 - not-checked - Production Windows Samoa behavior was not exercised.
- F24-1 - not-checked - Not-spooled fallback accounting was not audited.
- F24-2 - not-checked - Unterminated suffix accounting was not audited.
- F24-3 - not-checked - Concurrent not-spooled cleanup was not audited.
- F24-4 - not-checked - Not-spooled status timestamps were not audited.
- F24-5 - not-checked - Plugin-home adapter coverage was not audited.
- F24-6 - not-checked - Fold record rewrite failures were not audited.
- F25-2 - not-checked - Endpoint transcript safety was not audited.
- F25-3 - not-checked - Claude model case handling was not audited.
- F25-4 - not-checked - Child environment redaction was not audited.
- F30-1 - not-checked - Unknown helper recovery was not audited.
- F30-2 - not-checked - Pre-kill persistence failures were not audited.
- F30-3 - not-checked - Kill-site integration coverage was not audited.
- F31-1 - not-checked - Live-producer fallback accounting was not audited.
- F36-1 - not-checked - Unicode transcript-path handling was not audited.
- F36-2 - not-checked - Transcript-path normalization was not audited.
- F38-1 - not-checked - Recovery after failed persistence was not audited.
- F40-1 - not-checked - Recovery-copy enumeration failures were not audited.
- F40-2 - not-checked - Kick acknowledgements were not audited.
- F42-1 - not-checked - Recovery-copy metadata failures were not audited.
- F46-1 - fixed - Read the equal-time per-window ranking and regression coverage; the original lower-first tie case is addressed. Tests were not run.
- F48-1 - fixed - Equal-time selection ranks primary and secondary independently, addressing the original crossing-window case. Tests were not run.
- F50-1 - fixed - The equal-time comparator ranks reset completeness before percentage, preserving a future reset over a missing one. Tests were not run.
- F52-1 - not-checked - Endpoint and account paths pass a fixed comparison instant, but the complete rollout/sub-agent entry path was not inspected.
- F52-2 - fixed - Finite percentage validation and invalid-only rejection address the reported NaN-first recorded case. Tests were not run.
- F55-1 - fixed - The cache stores rollout observations and reruns selection at each requested instant; the warm-cache/fresh-home regression directly covers the original trigger. Tests were not run.
- F55-2 - fixed - Invalid-only recorded candidates are rejected before replacement; the added regression covers both orders. Finding #1 is a distinct three-observation regression introduced by retaining partial windows without their timestamps.

## Verdict: HOLD

The partial-window merge loses per-window observation timestamps, allowing input order to suppress an active quota block.

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

- The Rust regressions and reported workspace checks were not rerun in this read-only consultation.
- Three distinct observation timestamps with a valid partial newest observation are absent from the inspected regression coverage.
- The broader wave a7f2626..e76518e, historical handoffs, upstream oracle sources, and unrelated prior findings were not re-audited.
- The 13 changed .collab artifacts were skipped as directed by the brief.

### First-run checklist (observable)

_(none)_
