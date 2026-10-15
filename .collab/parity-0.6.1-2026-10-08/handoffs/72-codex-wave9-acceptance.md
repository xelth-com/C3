# Handoff 72 - Codex: wave9-acceptance

Date: 2026-10-15 02:40 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 9 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: eace5b54-01c1-4238-884c-21197e84e57c.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-0b2e3217372d473db5bf3901ad8d7c8f.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13cfe-e9a6-7a51-b163-114339ae061c` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/71-claude-wave9-acceptance.md` (sha256 357897e01925). Reviewed: d142600, base d142600bf1f00b43211892a2f279843c8fd33e2b, tree sha256 1f3a00962c57, 0 changed files.
Bridge outcome: usable reply. Wall time: 155.4 s. Tokens: in 296173 (cached 226560), out 6123, reasoning 846.
Quota (rollout): 5h 29% until 07:26; wk 96% until Oct 20; this run 296173 input tokens (226560 cached) in 6 calls, max 68741 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s. Range: `f2ff103..891ea63` - the range changes 14 files, 3109 lines (3038 insertions, 71 deletions).
Verdict: HOLD - The legacy fallback has additional identity loss, and its claimed 24-hour upgrade-risk bound is not enforced on reads. Findings: 0 blocker, 2 major, 0 minor, 0 note (F72-1..F72-2, tracked in `findings.json`). Structured reply: `handoffs/72-codex-wave9-acceptance.reply.json`.
Raw event stream: `handoffs/72-codex-wave9-acceptance.events.jsonl`.
Verbatim reply follows.

---

1. **Q1:** I would not ship on the stated residual-risk justification. Legacy records are pruned during health updates, not reads; providers can therefore keep applying a false cap verdict beyond 24 hours. Also, failure text is truncated to 100 characters before identity comparison, so distinct request IDs after that prefix cannot prevent incorrect merging. These extend F70-1 beyond the described mitigation.

2. **Q2: HOLD**, on findings #1 and #2. F68-1’s explicit-ID correction and F68-2’s portable launcher are fixed by inspection. No tests were executed; the 829-pass result is supplied evidence.

Read all five non-collaboration diffs and selected surrounding code, plus relevant portions of handoffs 65/67/69/71. Skipped collaboration bookkeeping, unrelated prior implementations, and full test logs. HEAD had advanced; inspected source paths had no differences from 891ea63.

## Requested checks

- **RC1:** At repository root, workspace-write, extend the cap fixture with a legacy mirror stamped one second later; evaluate through machine-health conversion after 48 hours without updates. Require no legacy-induced cap block. Budget: 20 minutes.
- **RC2:** At repository root, workspace-write, extend the identity fixture with same-second messages sharing 100 characters but differing afterward in request ID. Require two hits and cap-unavailable. Budget: 15 minutes.

---

### Findings

- **F72-1** [major] `crates/c3-core/src/health.rs:1127`, `crates/c3-core/src/health.rs:2340`, `crates/c3-core/src/health.rs:2556`, `crates/c3-core/src/plan.rs:360`, `crates/c3/src/providers.rs:2316` - The claimed 24-hour bound on legacy identity ambiguity is not enforced when health is read. An old no-ID machine row and its ledger mirror can continue producing a false declared-cap outage until the configured reset if no successful machine-health update prunes the row. Trigger: A pre-9b machine row mirrors one ledger failure but has a timestamp one second later. More than 24 hours pass without a successful health update. A capped entry has a future monthly reset, no newer usable reply, and providers or preflight reads the retained machine file. Evidence: read-code: Age retention runs inside update_machine_health; it is not a read-time expiration rule.; read-code: Provider health reads convert machine endpoints directly, filtering fingerprints but not age.; read-code: Instant-429 candidates have no age limit. A qualifying pair remains blocking until its reset hint.; inferred: Without a health update, elapsed time alone does not remove the legacy row; the false pair can outlive the stated upgrade window. Verify: Run RC1 with a persisted legacy machine row and its ledger twin, comparing health immediately and 48 hours later without rewriting the machine file. Remedy: Enforce the intended legacy eligibility bound during cap-evidence reads or selection, independent of successful health writes, and add the controlled-clock regression.
- **F72-2** [major] `crates/c3-core/src/health.rs:1109`, `crates/c3-core/src/health.rs:1153`, `crates/c3-core/src/plan.rs:477`, `docs/port/wave9-consilium-defects.md:115` - F70-1's legacy identity residual is broader than missing request IDs: deduplication compares only the first 100 characters of failure text. Independent same-second failures with different request IDs after that prefix are merged, hiding the cap verdict despite distinguishable source messages. The previously identified split of genuine mirrors at different seconds also remains. Trigger: A ledger failure and an independent legacy machine failure share an endpoint and completion second. Their messages have an identical first 100 characters and different request IDs afterward. Both are otherwise qualifying instant 429s. Evidence: read-code: Record.message is truncated to 100 characters before candidate deduplication.; read-code: The legacy fallback compares the truncated message, fingerprint, and completion second, then drops the matching machine row.; read-code: The identity tests use short messages whose request IDs survive truncation; the different-second legacy case explicitly expects two hits.; inferred: Different request IDs beyond character 100 become indistinguishable, reducing two genuine failures to one. Verify: Run RC2 with distinct request IDs beyond the shared 100-character prefix and assert two retained hits and an unavailable cap verdict. Remedy: Keep full failure identity separately from display text, or use a full-message digest for matching. Preserve the remaining ambiguity as an explicit legacy policy with bounded eligibility. Supersedes: F70-1.

### Prior findings

- F02-1 - not-checked - Outside the reviewed delta; implementation not revalidated.
- F02-3 - not-checked - Outside the reviewed delta; implementation not revalidated.
- F02-4 - not-checked - Harness coverage map not inspected.
- F02-5 - not-checked - Rating transaction not inspected.
- F02-6 - not-checked - Prompt anchors not inspected.
- F02-7 - not-checked - Cross-engine plan compatibility not revalidated.
- F02-8 - not-checked - No three-route benchmark evidence reviewed.
- F04-1 - not-checked - Process-name classifier not inspected.
- F04-2 - not-checked - Windows DST behavior not tested.
- F04-3 - not-checked - Required-light ledger behavior not inspected.
- F06-1 - not-checked - Historical Samoa behavior not tested.
- F09-1 - not-checked - Context configuration not inspected.
- F09-2 - not-checked - Telemetry deletion synchronization not inspected.
- F09-3 - not-checked - Recoverable deletion cleanup not inspected.
- F09-4 - not-checked - Spool reread failure handling not inspected.
- F09-5 - not-checked - Legacy telemetry sanitization not inspected.
- F09-6 - not-checked - Rating actor resolution not inspected.
- F11-1 - not-checked - Journal I/O failure handling not revalidated.
- F11-2 - not-checked - Required AGY launcher propagation not inspected; harness shorthand does not establish its status.
- F11-3 - not-checked - Required-reviewer shared-plan evaluation not inspected.
- F11-4 - not-checked - PowerShell hook pointer not executed.
- F11-5 - not-checked - AGY tool stall handling not inspected.
- F14-1 - not-checked - Local-only deletion transaction not inspected.
- F19-1 - not-checked - Complaint/deletion synchronization not inspected.
- F19-2 - not-checked - Unreadable salt cleanup not inspected.
- F19-3 - not-checked - Duplicate-key sanitization not inspected.
- F22-1 - not-checked - Journal replay durability not revalidated.
- F23-1 - not-checked - Process metadata failure classification not inspected.
- F23-2 - not-checked - Unknown creation-time scans not inspected.
- F23-3 - not-checked - Kill-site persistence not inspected.
- F23-4 - not-checked - Recovery failure-injection coverage not revalidated.
- F23-5 - not-checked - Production Samoa tests not run.
- F24-1 - not-checked - Not-spooled lock fallback not inspected.
- F24-2 - not-checked - Unterminated suffix accounting not inspected.
- F24-3 - not-checked - Concurrent cleanup accounting not inspected.
- F24-4 - not-checked - Unseen-suffix status reporting not inspected.
- F24-5 - not-checked - Plugin-home marker and shared-lock tests not inspected.
- F24-6 - not-checked - Second accounting-record rewrite not inspected.
- F25-2 - not-checked - Endpoint transcript-location guard not inspected.
- F25-3 - not-checked - Claude model case handling not inspected.
- F25-4 - not-checked - Credential debug representation not inspected.
- F30-1 - not-checked - Unknown-tree helper recovery not inspected.
- F30-2 - not-checked - Pre-kill evidence durability not inspected.
- F30-3 - not-checked - Kill-site integration coverage not inspected.
- F31-1 - not-checked - Live-producer accounting provenance not inspected.
- F36-1 - not-checked - Unicode transcript paths not inspected.
- F36-2 - not-checked - NFC transcript derivation not inspected.
- F38-1 - not-checked - All-write-failure recovery not inspected.
- F40-1 - not-checked - Kill-copy directory enumeration failure not inspected.
- F40-2 - not-checked - Kick acknowledgement after persistence failure not inspected.
- F42-1 - not-checked - Recovery-copy metadata failure not inspected.
- F46-1 - not-checked - Equal-time quota aggregation not inspected.
- F48-1 - not-checked - Secondary quota-window aggregation not inspected.
- F50-1 - not-checked - Reset-completeness tie handling not inspected.
- F52-1 - not-checked - Quota comparison-clock consistency not inspected.
- F52-2 - not-checked - Nonfinite quota inputs not inspected.
- F55-1 - not-checked - Quota-cache reset-boundary behavior not inspected.
- F55-2 - not-checked - Malformed newer quota selection not inspected.
- F58-1 - not-checked - Per-window observation ordering not inspected.
- F60-1 - not-checked - Per-window persistence not inspected.
- F62-1 - not-checked - Fractional timestamp persistence not inspected.
- F66-1 - not-checked - Read the updated fixture and remediation description; production admission was not revalidated.
- F66-2 - not-checked - Read the remediation description; reservation implementation and concurrency schedule were not inspected.
- F66-3 - fixed - Inspected same-nonempty-ID deduplication and its mirrored-row regression. Legacy ambiguity remains separately tracked.
- F66-4 - not-checked - Read the remediation description; Unicode validation was not revalidated.
- F66-5 - not-checked - Read the remediation description; persistent PowerShell behavior was not tested.
- F66-6 - still-open - The heuristic tradeoff remains explicitly documented policy; documentation does not make transient 429s distinguishable from exhaustion.
- F68-1 - fixed - Different nonempty IDs are retained at identical timestamps, and the five-second fallback is removed. Additional legacy defects are reported separately.
- F68-2 - fixed - The fixture now uses ping on Windows and sleep elsewhere; PowerShell dependency is removed by inspection. Linux execution remains unverified.
- F70-1 - still-open - Both original legacy ambiguities remain. Finding #2 expands the identity-loss claim to request IDs removed by truncation; finding #1 challenges the claimed temporal bound.

## Verdict: HOLD

The legacy fallback has additional identity loss, and its claimed 24-hour upgrade-risk bound is not enforced on reads.

### Blockers

- **F14-1** (prior, not-checked) `crates/c3/src/telemetry/complaint.rs:542` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending').
- **F22-1** (prior, not-checked) `crates/c3-core/src/health.rs:1563` - After failed `.bad` archival, the retained journal is replayed against mutable endpoint state whose record keys can be removed by the 24-hour or 500-record retention rules. A journal record can therefore be applied, evicted, and re-applied with a different final effect, while a record already stale on a replay is parsed but never becomes visible. The record-key dedup is not durable, so the documented harmless/once replay invariant is false.
- **F23-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:97` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time.
- **F23-2** (prior, not-checked) `crates/c3/src/liveness/proc.rs:753` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs.
- **F23-3** (prior, not-checked) `crates/c3/src/consult/orchestrate.rs:5105` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree.
- **F24-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:640` - The spool-lock fallback marks all non-legacy not-spooled lines as seen without folding or retaining per-file provenance; a later fold counts the same gone-producer files again, so the accounting can double-count and temporarily hide the count.
- **F25-2** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:357` - Endpoint mode cannot enforce the transcript-location safety guard because it never populates `projectsDirectory`; transcripts may land inside the repository and alter the tree being reviewed.
- **F30-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:815` - Unknown-tree release remains fail-open for a live reviewer helper whose creation time is unreadable and whose name or command line does not match the codex rule, especially when it is a grandchild below an unrecorded intermediate process.
- **F30-2** (prior, not-checked) `crates/c3/src/engines/subprocess.rs:536` - Kill evidence is not durable at the actual kill: the processes are killed before the kept record is written, and write failures are ignored while execution continues.
- **F31-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:668` - The lock-busy fallback records live producers' existing lines as seen without recording per-file provenance; when such a producer later exits, the next fold counts its entire file again as a gone producer, double-counting those lines across the record history.
- **F36-1** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:425` - The new Claude transcript guard can panic on a valid Unicode configuration path because repo_relative slices the candidate string at the byte length of an unrelated repository prefix without checking a UTF-8 boundary.
- **F38-1** (prior, not-checked) `crates/c3/src/consult/orchestrate.rs:687` - The fallback narrows but does not close the recovery durability failure: if both pre-kill writes fail and the bridge dies after killing an unrecorded intermediate, recovery can release the task while that intermediate's non-Codex helper remains alive. The surviving launch record contains insufficient ancestry, and the in-memory KillRecord::Lost result does not survive the bridge.
- **F40-1** (prior, not-checked) `crates/c3/src/liveness/pending.rs:170` - Recovery silently treats failure to enumerate the Codex-home kill-copy directory, or errors while iterating it, as absence of recovery copies. A durable copy naming a live helper can therefore be omitted and the task released.
- **F42-1** (prior, not-checked) `crates/c3/src/liveness/pending.rs:306` - A metadata inspection error on an enumerated recovery copy is treated as absence. read_pending_file uses Path::exists(), which converts metadata errors to false, and returns exists=false with no error. Recovery and findings checks then skip the copy, potentially releasing a task while a helper named only in that durable copy remains alive.
- **F66-1** (prior, not-checked) `crates/c3/src/consult/recovery.rs:129` - Detached admission treats active recovery records as harmless concurrency without establishing that their owning bridge is still alive. Records retained because an interrupted run left surviving or unverified processes are therefore bypassed, allowing a new engine turn while recovery still considers the previous tree active.

### Unproven scenarios

- No tests or harnesses were executed during this read-only review; reported pass counts were not independently verified.
- Linux execution of the portable recovery fixture and its process-inspection assumptions.
- Legacy cap behavior after 24 hours without a successful machine-health update.
- Legacy identity matching when distinguishing request IDs occur after the first 100 characters.
- Earlier implementations marked not-checked, collaboration bookkeeping diffs, and complete historical verification logs.
- The supplied detach harness's two failures were described as pre-existing; their underlying logs were not inspected.

### First-run checklist (observable)

- [ ] Confirm the tested binary corresponds to the accepted revision and reports the intended release version.
- [ ] Observe one consultation mirrored under the same nonempty ID produce one cap hit, while two distinct IDs at the same instant produce two hits and the declared reset in provider output.
- [ ] With legacy scratch health data, observe that a single mirrored failure does not create a persistent monthly cap outage after the intended legacy eligibility window.
- [ ] Observe native panel output select mode new by default and detached follow-up commands use c3 syntax.
- [ ] For two detached runs, observe overlapping execution intervals, distinct committed numbers, and both replies present in the ledger.
- [ ] With a dead bridge and a live recorded survivor, observe explicit recovery refusal and no new engine-turn marker.
- [ ] On Linux, observe the named recovery test pass without PowerShell; inspect test counts and harness failure details rather than relying only on aggregate exit status.
