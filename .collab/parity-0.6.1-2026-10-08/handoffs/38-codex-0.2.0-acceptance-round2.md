# Handoff 38 - Codex: 0.2.0-acceptance-round2

Date: 2026-10-13 20:56 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: f9e9621b-4279-400d-b743-8570437c937e.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-64063261bd674a258f476357c3c417f8.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a1369f-8562-7fc1-a7cb-eb8bb4e52c59` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/37-claude-0.2.0-acceptance-round2.md` (sha256 2887a7fd63b2). Reviewed: 8580c65, base 8580c65ca31ad808ede99b4bcb65bf63b3fba2ed, tree sha256 139e1ae7a0da, 0 changed files.
Bridge outcome: usable reply. Wall time: 382.7 s. Tokens: in 640588 (cached 526976), out 7989, reasoning 1446.
Quota (rollout): 5h 44% until 23:37; wk 23% until Oct 20; this run 640588 input tokens (526976 cached) in 10 calls, max 92321 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 70 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s. Range: `09b9d25..9f1ebf4` - the range changes 40 files, 3793 lines (3731 insertions, 62 deletions).
Verdict: HOLD - F36-1 and F36-2 are addressed, but the remaining F30-2/F23-3 failure path can release a task while an orphaned helper survives. Findings: 1 blocker, 0 major, 0 minor, 0 note (F38-1, tracked in `findings.json`). Structured reply: `handoffs/38-codex-0.2.0-acceptance-round2.reply.json`.
Raw event stream: `handoffs/38-codex-0.2.0-acceptance-round2.events.jsonl`.
Verbatim reply follows.

---

1. **Q1 — Unicode guard:** F36-1 and F36-2 are fixed by inspection for their reported triggers. Prefix removal now follows character boundaries; transcript checks compare the original and NFC forms of both paths. The added fixtures exercise the Japanese-path panic and decomposed configuration path. I found no further concrete Unicode bypass in the inspected code; exhaustive Windows case-fold equivalence remains unproven.

2. **Q2 — Recovery:** Yes, a failing schedule remains. Both recovery-record writes fail; the kill removes the root and an intermediate process but leaves its helper alive; the bridge dies before recording the result. The surviving launch record names neither intermediate nor helper. Parent scans cannot cross the missing intermediate, and a helper with a readable, non-Codex command line escapes the machine-wide rule. Recovery then returns inactive. Finding #1 merges the remaining F30-2/F23-3 failure case. The new RC2 fixture leaves a direct child, so it does not exercise this schedule.

3. **Q3 — Documented limits:** Acceptable as explicitly scoped compatibility limitations: cross-bridge once-only replay is unsupported with plugin 0.6.1; `--version` establishes launcher availability rather than turn success; unresolved home-directory fallback refuses conservatively. Wrapper-modified transcript locations remain outside the preflight guarantee. These disclosures do not resolve Q2’s claimed fail-closed recovery invariant.

4. **Q4 — HOLD:** Do not tag v0.2.0 yet. The blocker is the remaining F30-2/F23-3 case, consolidated in finding #1. Require the regression below and observable refusal before acceptance.

**Coverage:** Read the brief, relevant handoff-33 sections, wave-7 note, changed release documentation, and all non-`.collab` code/configuration diff hunks. Followed recovery and transcript-guard dependencies through selected source ranges. Skipped `.collab` history/logs beyond the two briefs, unrelated earlier-release implementation, complete older wave documents, and raw verification logs. Source files inspected outside the diff match `9f1ebf4`; HEAD differs only in collaboration records. No files were written and no tests were run. Reported green runs are coordinator evidence, not independently reproduced results.

## Requested checks

- **RC1:** Working directory `C:\Users\Dmytro\C3`; permission **workspace-write**; budget **30 minutes, one regression**. Extend the RC2 fixture to leave `root → intermediate → helper`, fail both kill-record writes, remove root/intermediate while retaining a helper with a readable unrelated command line, and terminate the bridge at `kill,main`. Restart the same task after restoring write access. Finding #1 is settled only if recovery refuses before any new engine turn while the helper remains alive; verify the fixture actually preserves that helper.

---

### Findings

- **F38-1** [blocker] `crates/c3/src/consult/orchestrate.rs:687`, `crates/c3/src/engines/subprocess.rs:912`, `crates/c3/src/liveness/pending.rs:1026`, `crates/c3/src/liveness/proc.rs:878`, `crates/c3-cli/tests/kill_record_durable.rs:589` - The fallback narrows but does not close the recovery durability failure: if both pre-kill writes fail and the bridge dies after killing an unrecorded intermediate, recovery can release the task while that intermediate's non-Codex helper remains alive. The surviving launch record contains insufficient ancestry, and the in-memory KillRecord::Lost result does not survive the bridge. Trigger: Start a tree bridge → recorded root → unrecorded intermediate → helper. Make replacement of the pending record and creation of its fallback fail. Let the kill stop the root and intermediate while the helper survives, then terminate the bridge before any successful post-kill write. Restore filesystem access and restart the task with no other matching processes. The helper has a readable command line that matches neither the recorded launcher nor the Codex rules. Evidence: read-code: Failure of both writes returns KillRecord::Lost and saves the outcome only in memory; no additional durable refusal state is established.; read-code: The killer proceeds after receiving Lost. It kills the root and attempts descendant termination; marking the result unconfirmed occurs in memory.; read-code: Recovery scans under recorded pids and then applies the machine-wide classifier. Parent traversal needs surviving table links; a readable unrelated helper is not selected by the machine-wide rule. Empty scans produce an inactive result.; read-code: RC2 preserves the fake PowerShell directly below the recorded launcher. The wave note explicitly acknowledges that the launch record cannot find a non-Codex helper below a dead, unrecorded intermediate.; inferred: Combining the acknowledged ancestry gap with failure of both durable writes permits a new turn despite a surviving helper. This combined schedule was not executed during this read-only review. Verify: Run RC1 from Requested checks with an isolated process table or controlled Windows tree, proving the helper remains alive and requiring the restarted task to refuse before spawning another engine turn. Remedy: Make an interrupted launch remain conservatively blocked when complete kill evidence cannot be persisted. Establish that recovery obligation durably before spawning, or require explicit operator release of an unresolved launch record instead of treating these empty scans as proof of termination. Add the missing dead-intermediate regression. Supersedes: F30-2, F23-3.

### Prior findings

- F02-1 - not-checked - Telemetry spool concurrency was outside this delta review.
- F02-3 - not-checked - Deletion retry identity was not rechecked.
- F02-4 - not-checked - Read reported harness limitations, but did not audit the assertion coverage map.
- F02-5 - not-checked - Rating commit-before-spool behavior was not rechecked.
- F02-6 - not-checked - Prompt anchors were outside this delta review.
- F02-7 - not-checked - Shared-plan roster behavior was not rechecked.
- F02-8 - not-checked - No credit-saving or review-equivalence benchmark was inspected.
- F04-1 - not-checked - Located Unicode classifier tests but did not inspect the complete fix or run them.
- F04-2 - not-checked - Production Windows DST handling was not rechecked.
- F04-3 - not-checked - Required-light ledger metadata was not rechecked.
- F06-1 - not-checked - Historical Samoa offset handling was not rechecked.
- F09-1 - not-checked - Context-window configuration parsing was not rechecked.
- F09-2 - not-checked - Flush-versus-deletion locking was not rechecked.
- F09-3 - not-checked - Recoverable deletion cleanup was not rechecked.
- F09-4 - not-checked - Spool reread failure handling was not rechecked.
- F09-5 - not-checked - Legacy telemetry sanitization was not rechecked.
- F09-6 - not-checked - Rating actor model inference was not rechecked.
- F11-1 - not-checked - Health journal I/O failure behavior was not rechecked.
- F11-2 - not-checked - Required-reviewer launcher override handling was not rechecked.
- F11-3 - not-checked - Full-roster shared-plan quota evaluation was not rechecked.
- F11-4 - not-checked - PowerShell hook pointer execution was not rechecked.
- F11-5 - not-checked - Read AGY kill-hook signature changes only; tool-stall behavior was not rechecked.
- F14-1 - not-checked - Local-only deletion transaction persistence was not rechecked.
- F19-1 - not-checked - Complaint/deletion synchronization was not rechecked.
- F19-2 - not-checked - Unreadable-salt cleanup behavior was not rechecked.
- F19-3 - not-checked - Duplicate-key telemetry sanitization was not rechecked.
- F22-1 - not-checked - Read the mixed-bridge limitation disclosure; did not inspect C3 journal deduplication implementation.
- F23-1 - not-checked - Process metadata lookup failure handling was not rechecked.
- F23-2 - not-checked - Inspected scan branches for the current blocker, but did not verify all original unknown-start fixtures.
- F23-3 - still-open - Normal pre-kill persistence and fallback improve durability; the remaining dual-write-failure/crash case is consolidated in finding #1.
- F23-4 - not-checked - Original metadata and unknown-tree test corrections were not audited.
- F23-5 - not-checked - Production Windows Samoa evidence was not inspected.
- F24-1 - not-checked - Not-spooled lock-busy accounting was not rechecked.
- F24-2 - not-checked - Unterminated suffix accounting was not rechecked.
- F24-3 - not-checked - Concurrent not-spooled creation during deletion was not rechecked.
- F24-4 - not-checked - Latest unseen event reporting was not rechecked.
- F24-5 - not-checked - Plugin-home marker and shared-lock oracle coverage was not rechecked.
- F24-6 - not-checked - Second accounting-record rewrite failures were not rechecked.
- F25-2 - not-checked - Inspected derived-location checks, but did not retrace endpoint auth-status acquisition and the original reported-directory scenario.
- F25-3 - not-checked - Claude model case validation was not rechecked.
- F25-4 - not-checked - ChildEnv debug redaction was not rechecked.
- F30-1 - not-checked - Inspected subtree traversal and machine-wide filtering, but did not validate the complete original synthetic fixture.
- F30-2 - still-open - Fallback persistence fixes one-write failure; both-write failure followed by a bridge crash remains unsafe. Consolidated with F23-3 in finding #1.
- F30-3 - fixed - By source inspection, tests now cover main, continuation, repair, interruption before post-kill persistence, and write-failure refusal. Not rerun; the new combined scenario in finding #1 remains uncovered.
- F31-1 - not-checked - Live-to-gone producer accounting was not rechecked.
- F36-1 - fixed - Character-wise prefix removal eliminates the unrelated UTF-8 byte-offset slice; exact Japanese-path fixtures are present. Not rerun.
- F36-2 - fixed - Derived paths use NFC and containment checks compare both forms of candidate and root; unit and endpoint integration fixtures cover the reported trigger. Not rerun.

## Verdict: HOLD

F36-1 and F36-2 are addressed, but the remaining F30-2/F23-3 failure path can release a task while an orphaned helper survives.

### Blockers

- **F38-1** `crates/c3/src/consult/orchestrate.rs:687`, `crates/c3/src/engines/subprocess.rs:912`, `crates/c3/src/liveness/pending.rs:1026`, `crates/c3/src/liveness/proc.rs:878`, `crates/c3-cli/tests/kill_record_durable.rs:589` - The fallback narrows but does not close the recovery durability failure: if both pre-kill writes fail and the bridge dies after killing an unrecorded intermediate, recovery can release the task while that intermediate's non-Codex helper remains alive. The surviving launch record contains insufficient ancestry, and the in-memory KillRecord::Lost result does not survive the bridge. Verify: Run RC1 from Requested checks with an isolated process table or controlled Windows tree, proving the helper remains alive and requiring the restarted task to refuse before spawning another engine turn. Remedy: Make an interrupted launch remain conservatively blocked when complete kill evidence cannot be persisted. Establish that recovery obligation durably before spawning, or require explicit operator release of an unresolved launch record instead of treating these empty scans as proof of termination. Add the missing dead-intermediate regression.
- **F14-1** (prior, not-checked) `crates/c3/src/telemetry/complaint.rs:542`, `crates/c3/src/telemetry/complaint.rs:240`, `crates/c3/src/telemetry/complaint.rs:673` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending'). Verify: Add a test: forget_with with ForgetRequest{public_ref: None, local: true, yes: true} and an injected remove failing on spool.ndjson; assert whether forget-pending.json exists and whether a subsequent flush posts the retained events. Remedy: In the None arm, persist the transaction (phase cleaning) via write_transaction before calling finish_cleanup (or pass phase confirmed so run_local_cleanup writes it), so an interruption leaves the blocking, resumable record the docs and the CLI message promise.
- **F22-1** (prior, not-checked) `crates/c3-core/src/health.rs:1563`, `crates/c3-core/src/health.rs:1915`, `crates/c3-core/src/health.rs:1942`, `crates/c3-core/src/health.rs:1960`, `crates/c3-core/src/health.rs:2028` - After failed `.bad` archival, the retained journal is replayed against mutable endpoint state whose record keys can be removed by the 24-hour or 500-record retention rules. A journal record can therefore be applied, evicted, and re-applied with a different final effect, while a record already stale on a replay is parsed but never becomes visible. The record-key dedup is not durable, so the documented harmless/once replay invariant is false. Verify: Add a deterministic test with an unwritable `.bad` path, one recent journal record, and 500 newer endpoint records; run two updates and assert the journal record is neither resurrected nor allowed to displace another endpoint. Remedy: Persist a monotonic journal applied-sequence marker or equivalent durable consumed state, and truncate the journal by that sequence independently of endpoint retention; do not use the mutable endpoint set as the only replay ledger.
- **F23-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:97`, `crates/c3/src/liveness/proc.rs:100`, `crates/c3/src/liveness/pending.rs:283`, `crates/c3/src/liveness/pending.rs:344` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time. Verify: Inject `name_and_parent -> None` while `process_start_iso -> Some(start)` and require `test_unverified_process`/`test_recorded_process` to return running or an explicit inspection failure. Remedy: Distinguish process absence from inspection failure; return an unknown/running verdict when the pid exists but metadata cannot be read.
- **F23-2** (prior, not-checked) `crates/c3/src/liveness/proc.rs:753`, `crates/c3/src/liveness/proc.rs:775`, `crates/c3/src/liveness/proc.rs:827`, `crates/c3/src/liveness/pending.rs:790` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs. Verify: Run `test_pending_active` on a `kill_unconfirmed` record with a live scan row whose `created` is `None`; require refusal/failed verification, never release. Remedy: Treat unknown creation/start as blocking evidence or fail the scan; do not silently filter those rows.
- **F23-3** (prior, still-open) `crates/c3/src/consult/orchestrate.rs:5105`, `crates/c3/src/consult/orchestrate.rs:5143`, `docs/port/wave3a-recovery.md:59` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree. Verify: Add a pause between the kill and the end-of-run pending write, terminate the bridge there, and inspect the recovery record for the expected `unverified`/`kill_unconfirmed` fields. Remedy: Persist the kill disposition at each kill site, or write a durable kill journal before continuing the run.
- **F24-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:640`, `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:704` - The spool-lock fallback marks all non-legacy not-spooled lines as seen without folding or retaining per-file provenance; a later fold counts the same gone-producer files again, so the accounting can double-count and temporarily hide the count. Verify: Hold `spool.lock` for over one second during flush 1 with N gone-producer lines, then run flush 2 and compare both records; require exactly one accounting of N. Remedy: In the fallback, retain per-file folded entries or mark only positively kept files seen; never advance the global baseline for files that remain foldable.
- **F25-2** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:357`, `crates/c3/src/engines/claude_auth.rs:374` - Endpoint mode cannot enforce the transcript-location safety guard because it never populates `projectsDirectory`; transcripts may land inside the repository and alter the tree being reviewed. Verify: Make endpoint-mode `auth status` report an in-repository projects directory and require refusal before spawn. Remedy: Probe the status facts for endpoint mode too, without reading or logging the token, and reject unsafe transcript locations.
- **F30-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:815`, `crates/c3/src/liveness/pending.rs:297`, `crates/c3/src/liveness/pending.rs:902` - Unknown-tree release remains fail-open for a live reviewer helper whose creation time is unreadable and whose name or command line does not match the codex rule, especially when it is a grandchild below an unrecorded intermediate process. Verify: Run `test_pending_active_with` with the wrapper/helper synthetic table and require the record to remain active; repeat with a readable non-codex command line. Remedy: Scan full descendant relationships and treat unknown-start or command-line-unreadable/generic-runtime rows as blocking unless positive evidence proves they are unrelated.
- **F30-2** (prior, still-open) `crates/c3/src/engines/subprocess.rs:536`, `crates/c3/src/consult/orchestrate.rs:587`, `crates/c3/src/consult/orchestrate.rs:4927`, `crates/c3/src/consult/orchestrate.rs:5674` - Kill evidence is not durable at the actual kill: the processes are killed before the kept record is written, and write failures are ignored while execution continues. Verify: Add a pause between `kill_tree_checked` and `KillSite::write`, terminate there, and separately inject `write_pending` errors before continuation or commit. Remedy: Persist the kill record synchronously from the kill path or an append-only sidecar before returning, and fail closed when that persistence fails.
- **F31-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:886`, `crates/c3/src/telemetry/notspooled.rs:950` - The lock-busy fallback records live producers' existing lines as seen without recording per-file provenance; when such a producer later exits, the next fold counts its entire file again as a gone producer, double-counting those lines across the record history. Verify: Run the RC1 schedule and compare the first record’s `not_spooled_seen`, the second record’s fold note, and the total accounting for the N lines. Remedy: Persist per-file seen byte/line prefixes or carry the fallback’s kept-file provenance into the next fold, and subtract those already-seen lines when folding a formerly-live producer.

### Unproven scenarios

- The combined dual-write-failure, dead-intermediate, surviving-helper crash schedule has not been executed.
- No tests, builds, harnesses, or live consultations were independently run during this read-only review.
- Exhaustive equivalence between the new Unicode folding helper and Windows OrdinalIgnoreCase was not established.
- Fallback recovery interchange with the pinned PowerShell plugin was documented but not independently exercised.
- Earlier telemetry, health, rating, roster, and runtime findings marked not-checked were not re-certified.
- Wrapper-modified transcript locations and successful endpoint turns cannot be established by the version probe alone.

### First-run checklist (observable)

- [ ] Observe the tested revision and binary identity, an actual engine turn, a nonempty usable reply, and a matching committed ledger entry; a successful version probe or dry run is insufficient.
- [ ] With an in-repository NFC-equivalent Claude configuration path, observe an explicit refusal and no engine turn; with an outside configuration, verify the actual transcript destination and unchanged reviewed tree.
- [ ] At an interrupted kill, inspect the pending record or fallback before terminating the bridge: it must preserve the relevant process identities and unresolved-kill state.
- [ ] After the dual-write-failure regression, observe that the helper is still alive and the next consultation refuses before spawning any engine process.
- [ ] Once recovery is legitimately released, observe recovery consumption and fallback removal; confirm that a logged warning or skipped release assertion was not counted as a successful recovery check.
- [ ] For a normal live consultation, observe the expected ledger outcome and telemetry status showing delivery or an explicitly retained queue; exit code 0 alone does not prove those side effects.
