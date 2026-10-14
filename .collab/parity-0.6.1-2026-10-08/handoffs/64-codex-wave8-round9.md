# Handoff 64 - Codex: wave8-round9

Date: 2026-10-14 18:46 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 7a981894-dead-43da-bec8-b34e56b7c66d.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-33c6d64c9f59409c87b97c47c03931b0.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13b4f-2c01-7110-9650-9ea3a2cb6976` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/63-claude-wave8-round9.md` (sha256 749851ce6a6d). Reviewed: fd0ab41, base fd0ab41264578806d83179e1bf4bbd0314f99d27, tree sha256 853826ed89f2, 0 changed files.
Bridge outcome: usable reply. Wall time: 234.4 s. Tokens: in 256377 (cached 176768), out 3656, reasoning 642.
Quota (rollout): 5h 68% until 21:23; wk 76% until Oct 20; this run 256377 input tokens (176768 cached) in 6 calls, max 58641 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `a0404b1..409481f` - the range changes 16 files, 2168 lines (2105 insertions, 63 deletions).
Verdict: ACCEPT - The reviewed quota changes address F62-1 and make blocking-window selection order-independent at a fixed comparison instant; no new release-blocking defect was found. Findings: none. Structured reply: `handoffs/64-codex-wave8-round9.reply.json`.
WARNING: ACCEPT with 14 unchecked prior blocker(s) (F14-1, F22-1, F23-1, F23-2, F23-3, F24-1, F25-2, F30-1, F30-2, F31-1, F36-1, F38-1, F40-1, F42-1).
Raw event stream: `handoffs/64-codex-wave8-round9.events.jsonl`.
Verbatim reply follows.

---

1. **Q1:** I found no counterexample that hides a blocking window through input ordering under the documented policy. Within each known/legacy class, selection compares whole seconds and then the reset/usage rank. Keeping both class winners avoids the previous nontransitive cross-class fold. Recorded and live selection use the same comparison instant. Fractional timestamps survive persistence, while deliberately not affecting ordering within a second. F62-1 is fixed by inspection; the reviewed persistence and legacy-policy changes also address F60-1.

   The permutation tests exercise usage selection and blocking, not equality of every returned metadata field. Acceptance here concerns quota selection and blocking behavior, not a guarantee that provenance or exact timestamps are identical for rank-equivalent candidates.

2. **Q2: ACCEPT** the reviewed 8i/8j delta for v0.2.1. This is a code-review verdict, not an independently reproduced test certification. I inspected the quota diff, wave-note and changelog hunks, and targeted aggregation and endpoint-health code. The brief reports passing tests, lint checks, formatting, and harness checks; I did not rerun them in this read-only consultation.

   Unread: `.collab/` changes other than the supplied brief, README/plugin changes excluded by the brief, unchanged portions of the wave note and quota implementation, earlier handoffs, and the external plugin T17 implementation. Earlier unrelated findings were not re-audited.

---

### Findings

_(none)_

### Prior findings

- F02-1 - not-checked - Outside the quota delta reviewed.
- F02-3 - not-checked - Outside the quota delta reviewed.
- F02-4 - not-checked - Harness coverage was not re-audited.
- F02-5 - not-checked - Outside the quota delta reviewed.
- F02-6 - not-checked - Outside the quota delta reviewed.
- F02-7 - not-checked - Outside the quota delta reviewed.
- F02-8 - not-checked - No route benchmark reviewed.
- F04-1 - not-checked - Outside the quota delta reviewed.
- F04-2 - not-checked - Windows time-zone behavior was not re-audited.
- F04-3 - not-checked - Outside the quota delta reviewed.
- F06-1 - not-checked - Windows Samoa behavior was not re-audited.
- F09-1 - not-checked - Outside the quota delta reviewed.
- F09-2 - not-checked - Outside the quota delta reviewed.
- F09-3 - not-checked - Outside the quota delta reviewed.
- F09-4 - not-checked - Outside the quota delta reviewed.
- F09-5 - not-checked - Outside the quota delta reviewed.
- F09-6 - not-checked - Outside the quota delta reviewed.
- F11-1 - not-checked - Journal recovery was not re-audited.
- F11-2 - not-checked - Required-reviewer launcher resolution was not re-audited.
- F11-3 - not-checked - Shared-plan roster filtering was not re-audited.
- F11-4 - not-checked - Hook execution was outside the requested quota scope.
- F11-5 - not-checked - Outside the quota delta reviewed.
- F14-1 - not-checked - Outside the quota delta reviewed.
- F19-1 - not-checked - Outside the quota delta reviewed.
- F19-2 - not-checked - Outside the quota delta reviewed.
- F19-3 - not-checked - Outside the quota delta reviewed.
- F22-1 - not-checked - Journal retention and replay were not re-audited.
- F23-1 - not-checked - Outside the quota delta reviewed.
- F23-2 - not-checked - Outside the quota delta reviewed.
- F23-3 - not-checked - Outside the quota delta reviewed.
- F23-4 - not-checked - Recovery tests were not reviewed.
- F23-5 - not-checked - Windows Samoa fixtures were not reviewed.
- F24-1 - not-checked - Outside the quota delta reviewed.
- F24-2 - not-checked - Outside the quota delta reviewed.
- F24-3 - not-checked - Outside the quota delta reviewed.
- F24-4 - not-checked - Outside the quota delta reviewed.
- F24-5 - not-checked - Telemetry oracle and shared locking were not reviewed.
- F24-6 - not-checked - Outside the quota delta reviewed.
- F25-2 - not-checked - Outside the quota delta reviewed.
- F25-3 - not-checked - Outside the quota delta reviewed.
- F25-4 - not-checked - Outside the quota delta reviewed.
- F30-1 - not-checked - Outside the quota delta reviewed.
- F30-2 - not-checked - Outside the quota delta reviewed.
- F30-3 - not-checked - Recovery durability tests were not reviewed.
- F31-1 - not-checked - Outside the quota delta reviewed.
- F36-1 - not-checked - Outside the quota delta reviewed.
- F36-2 - not-checked - Outside the quota delta reviewed.
- F38-1 - not-checked - Outside the quota delta reviewed.
- F40-1 - not-checked - Outside the quota delta reviewed.
- F40-2 - not-checked - Outside the quota delta reviewed.
- F42-1 - not-checked - Outside the quota delta reviewed.
- F46-1 - not-checked - Replacement ranking was inspected, but the earlier finding's complete recorded/live verification was not repeated.
- F48-1 - not-checked - Per-window selection was inspected; earlier cross-route regression coverage was not fully re-audited.
- F50-1 - not-checked - Reset completeness participates in ranking; the full earlier verification matrix was not reviewed.
- F52-1 - not-checked - Reviewed endpoint and account folds share a comparison instant; all rollout aggregation paths were not re-audited.
- F52-2 - not-checked - Finite-percentage filtering was inspected, but the earlier malformed-input regression was not independently checked.
- F55-1 - not-checked - The cache retains raw observations and reselects at lookup time; the reset-boundary regression was not independently checked.
- F55-2 - not-checked - Candidate validity filtering was inspected; the earlier invalid-newer-record regression was not re-audited.
- F58-1 - not-checked - Per-window merge timestamps were inspected; the full earlier aggregation and partition matrix was not re-audited.
- F60-1 - fixed - By inspection: persisted per-window timestamps are read separately, and legacy windows compete with known windows by rank instead of promoted aggregate recency. Legacy-versus-legacy aggregate ordering remains the documented policy.
- F62-1 - fixed - By inspection: AutoSi preserves fractional per-window timestamps; whole-second comparisons prevent the specified older live observation from suppressing the recorded 99% window. The added test covers the real ledger round trip and live selection; it was not rerun.

## Verdict: ACCEPT

The reviewed quota changes address F62-1 and make blocking-window selection order-independent at a fixed comparison instant; no new release-blocking defect was found.

**WARNING: ACCEPT with 14 unchecked prior blocker(s) (F14-1, F22-1, F23-1, F23-2, F23-3, F24-1, F25-2, F30-1, F30-2, F31-1, F36-1, F38-1, F40-1, F42-1).**

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

- The reported test, clippy, formatting, and harness results were not independently executed or validated against their logs.
- Parity with the external plugin T17 implementation was not independently inspected.
- Permutation coverage checks selected percentages and blocking, not equality of all provenance and timestamp metadata.
- Changes outside the brief's quota scope and earlier unrelated findings were not re-audited.

### First-run checklist (observable)

_(none)_
