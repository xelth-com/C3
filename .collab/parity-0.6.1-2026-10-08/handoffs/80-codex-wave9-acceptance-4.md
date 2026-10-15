# Handoff 80 - Codex: wave9-acceptance-4

Date: 2026-10-15 10:57 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - entry 1 of 10 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 2b9e8140-452e-49bf-895d-a09e9eafd0d0.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-dd2e67577f8545fab980dac256268b28.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13ec2-af48-7323-8a98-c4034658cb38` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/79-claude-wave9-acceptance-4.md` (sha256 d6074c047828). Reviewed: 96eec21, base 96eec2101e70cc05df5d22c2d05c506d4df21987, tree sha256 dfcc4e862d43, 0 changed files.
Bridge outcome: usable reply. Wall time: 359.1 s. Tokens: in 362777 (cached 278016), out 5280, reasoning 982.
Quota (rollout): 5h 38% until 14:18; wk 6% until Oct 22; this run 362777 input tokens (278016 cached) in 8 calls, max 56885 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s. Range: `c82d97d..51dfc13` - the range changes 12 files, 2134 lines (2116 insertions, 18 deletions).
Verdict: HOLD - The two prior blockers are addressed, but asymmetric retention can discard a later successful reply and resurrect an obsolete cap refusal. Findings: 1 blocker, 0 major, 0 minor, 0 note (F80-1, tracked in `findings.json`). Structured reply: `handoffs/80-codex-wave9-acceptance-4.reply.json`.
Raw event stream: `handoffs/80-codex-wave9-acceptance-4.events.jsonl`.
Verbatim reply follows.

---

1. **Yes—finding #1 is a blocker.** Machine-health retention preserves failure pairs for 35 days but can discard their later clearing success after roughly 24 hours or under record-cap pressure. Another repository then falsely reports cap exhaustion. F78-1 and F78-2’s original triggers are addressed.

2. **HOLD** for finding #1. Workspace and harness reruns remain required before tagging.

Read both source diffs, both documentation diffs, and targeted surrounding code. Skipped `.collab/` except the brief, unrelated historical implementations, and remaining source. Tests were reported by the brief, not independently executed.

## Requested checks

- **RC1:** Repository root; workspace-write; add/run a production-path regression with a machine-only pair followed by success, then separately 48-hour pruning and 500-row pressure. Both must remain available; budget 30 minutes.
- **RC2:** Repository root; workspace-write; rerun the full workspace suite and pinned harness on the corrected revision. Require zero failures; budget 60 minutes.

---

### Findings

- **F80-1** [blocker] `crates/c3-core/src/health.rs:2177`, `crates/c3-core/src/health.rs:2184`, `crates/c3-core/src/health.rs:2367`, `crates/c3-core/src/health.rs:2399`, `crates/c3-core/src/health.rs:1131` - The new retention policy preserves cap-failure evidence longer and with higher eviction priority than the successful reply that clears it. Once that success disappears, machine-only readers reconstruct the old pair and falsely mark a usable endpoint unavailable until its declared reset. Trigger: A monthly-capped endpoint has two identified instant 429s followed by a successful explicit run. Another repository has no ledger copies. An unrelated machine-health update more than 25 hours after the success retains the failures for 35 days but removes the success, which has no active quota window or mark. Alternatively, 500 retained cap-evidence rows plus the clearing success cause the new eviction rule to remove the success immediately. Evidence: read-code: Only identified quota/burst rows receive evidence status; cap_machine_endpoints removes non-evidence rows before any evidence row, including later successful replies.; read-code: Retention is recent || still_out || evidence_kept. Failure evidence receives 35 days, while an ordinary success receives only the recent window.; read-code: Cap candidates are suppressed only by the newest successful reply present in the supplied records; no durable clearing watermark is consulted here.; inferred: After pruning the clearing success, machine-only conversion supplies the surviving pair, and the current-period detector reinstates the hinted refusal. This schedule was traced through code, not executed. Verify: Run RC1 through production machine-health updates and conversion: assert zero qualifying old hits and an available cap verdict both after aging out the success and after record-cap pressure. Remedy: Preserve the latest successful clearing watermark for every endpoint while older cap evidence can survive, or durably eliminate evidence superseded by that success. Make time retention and record-cap eviction preserve this relationship together.

### Prior findings

- F02-1 - not-checked - Telemetry implementation outside this review.
- F02-3 - not-checked - Deletion retry implementation outside this review.
- F02-4 - not-checked - Harness coverage map not inspected.
- F02-5 - not-checked - Rating transaction ordering not inspected.
- F02-6 - not-checked - Prompt anchors not inspected.
- F02-7 - not-checked - Cross-engine plan configuration not inspected.
- F02-8 - not-checked - No route-equivalence benchmark reviewed.
- F04-1 - not-checked - Process-name handling not inspected.
- F04-2 - not-checked - Windows local-zone boundary behavior not tested.
- F04-3 - not-checked - Required-light ledger behavior not inspected.
- F06-1 - not-checked - Historical Samoa behavior not inspected.
- F09-1 - not-checked - Context configuration parsing not inspected.
- F09-2 - not-checked - Flush/deletion locking not inspected.
- F09-3 - not-checked - Deletion cleanup recovery not inspected.
- F09-4 - not-checked - Spool reread failure handling not inspected.
- F09-5 - not-checked - Legacy telemetry sanitization not inspected.
- F09-6 - not-checked - Rating actor inference not inspected.
- F11-1 - not-checked - Journal failure paths not sufficiently reviewed.
- F11-2 - not-checked - Required-reviewer launcher propagation not inspected.
- F11-3 - not-checked - Required-reviewer shared-plan evaluation not inspected.
- F11-4 - not-checked - PowerShell hook command not inspected.
- F11-5 - not-checked - AGY tool stall handling not inspected.
- F14-1 - not-checked - Local-only cleanup transaction not inspected.
- F19-1 - not-checked - Complaint/deletion serialization not inspected.
- F19-2 - not-checked - Unreadable salt cleanup not inspected.
- F19-3 - not-checked - Duplicate-key sanitization not inspected.
- F22-1 - not-checked - Durable journal deduplication not fully reviewed.
- F23-1 - not-checked - Process metadata failure handling not inspected.
- F23-2 - not-checked - Unknown-start recovery handling not inspected.
- F23-3 - not-checked - Kill evidence persistence not inspected.
- F23-4 - not-checked - Recovery test coverage not inspected.
- F23-5 - not-checked - Production Samoa tests not inspected.
- F24-1 - not-checked - Not-spooled fallback accounting not inspected.
- F24-2 - not-checked - Unterminated suffix accounting not inspected.
- F24-3 - not-checked - Concurrent cleanup accounting not inspected.
- F24-4 - not-checked - Unseen-suffix status selection not inspected.
- F24-5 - not-checked - Plugin-home and shared-lock coverage not inspected.
- F24-6 - not-checked - Fold record rewrite recovery not inspected.
- F25-2 - not-checked - Endpoint transcript guard not inspected.
- F25-3 - not-checked - Claude model case handling not inspected.
- F25-4 - not-checked - Child environment debug redaction not inspected.
- F30-1 - not-checked - Unknown-tree helper recovery not inspected.
- F30-2 - not-checked - Pre-kill durability and error handling not inspected.
- F30-3 - not-checked - Kill-site integration coverage not inspected.
- F31-1 - not-checked - Live-producer accounting provenance not inspected.
- F36-1 - not-checked - Unicode transcript-path handling not inspected.
- F36-2 - not-checked - Transcript-path NFC normalization not inspected.
- F38-1 - not-checked - Multiple persistence failure recovery not inspected.
- F40-1 - not-checked - Recovery-copy enumeration failures not inspected.
- F40-2 - not-checked - Kick acknowledgement on persistence failure not inspected.
- F42-1 - not-checked - Recovery-copy metadata failures not inspected.
- F46-1 - not-checked - Equal-time quota aggregation not inspected.
- F48-1 - not-checked - Secondary quota-window aggregation not inspected.
- F50-1 - not-checked - Quota reset completeness ordering not inspected.
- F52-1 - not-checked - Quota comparison-clock consistency not inspected.
- F52-2 - not-checked - Nonfinite quota input handling not inspected.
- F55-1 - not-checked - Account cache reset-boundary behavior not inspected.
- F55-2 - not-checked - Invalid newer quota observation selection not inspected.
- F58-1 - not-checked - Per-window observation timestamps not inspected.
- F60-1 - not-checked - Quota timestamp persistence not inspected.
- F62-1 - not-checked - Fractional quota timestamps not inspected.
- F66-1 - not-checked - Detached recovery admission not inspected.
- F66-2 - not-checked - Detached reservation durability not inspected.
- F66-3 - fixed - Read endpoint_health deduplication by nonempty consult_id; matching ledger/machine IDs count once. Not executed.
- F66-4 - not-checked - Reset-hint Unicode validation not inspected.
- F66-5 - not-checked - Shim environment restoration not inspected.
- F66-6 - still-open - The documented heuristic deliberately retains false-positive transient-pair risk and unhinted probing; these are policy residuals, not new blockers here.
- F68-1 - fixed - Read identity-only deduplication: distinct IDs survive equal timestamps, and no-ID rows do not enter the pair.
- F68-2 - not-checked - Portable fixture change is described in the note; implementation not inspected.
- F70-1 - fixed - No-ID rows are excluded from cap candidates, removing the legacy mirror matching ambiguity from this detector.
- F72-1 - fixed - No-ID rows cannot contribute to the cap pair, regardless of persisted age.
- F72-2 - fixed - Cap deduplication uses consultation identity, not truncated message text.
- F74-1 - still-open - Multiple attempts within one consultation still count once, explicitly documented as intended policy.
- F74-2 - not-checked - The note claims validation makes this unreachable; reset-hint validation was not inspected.
- F76-1 - fixed - Read per-entry current-period filtering; hinted identified evidence is no longer universally limited to 24 hours.
- F78-1 - fixed - The detector sorts and scans for the newest qualifying pair; the isolated-third-hit regression is present. Not executed.
- F78-2 - fixed - Original aging trigger addressed by 35-day machine retention and evidence-prioritized eviction; production-path regression is present. Clearing-success retention is a separate new defect.

## Verdict: HOLD

The two prior blockers are addressed, but asymmetric retention can discard a later successful reply and resurrect an obsolete cap refusal.

### Blockers

- **F80-1** `crates/c3-core/src/health.rs:2177`, `crates/c3-core/src/health.rs:2184`, `crates/c3-core/src/health.rs:2367`, `crates/c3-core/src/health.rs:2399`, `crates/c3-core/src/health.rs:1131` - The new retention policy preserves cap-failure evidence longer and with higher eviction priority than the successful reply that clears it. Once that success disappears, machine-only readers reconstruct the old pair and falsely mark a usable endpoint unavailable until its declared reset. Verify: Run RC1 through production machine-health updates and conversion: assert zero qualifying old hits and an available cap verdict both after aging out the success and after record-cap pressure. Remedy: Preserve the latest successful clearing watermark for every endpoint while older cap evidence can survive, or durably eliminate evidence superseded by that success. Make time retention and record-cap eviction preserve this relationship together.
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
- **F76-1** (prior, fixed) `crates/c3-core/src/health.rs:1143` - The new read-time age limit discards identified cap-pair evidence after approximately 24 hours even when a valid reset_hint declares a later weekly or monthly reset. plan_cap_verdict then returns the underlying verdict without enforcing that reset. This makes an exhausted endpoint eligible before the retry time previously displayed and contradicts the documented policy that hinted entries remain unavailable until their reset.
- **F78-1** (prior, fixed) `crates/c3-core/src/plan.rs:368` - The cap detector considers only pairs containing the newest qualifying hit. A later isolated instant 429 therefore suppresses an already established cap pair, allowing an exhausted endpoint to become eligible before its declared reset without any successful reply.
- **F78-2** (prior, fixed) `crates/c3-core/src/health.rs:100` - The 35-day cap-pair retention applies only to rows supplied to endpoint_health; machine-health writes still prune old rows unless their stored outage or quota window remains active. Consequently, cap evidence originating in another repository can disappear after roughly 24 hours, releasing a hinted exhausted endpoint before its declared reset.

### Unproven scenarios

- No tests were independently executed; reported crate-test, formatting, and clippy results were taken from handoff 79.
- The full workspace suite and pinned plugin harness have not been demonstrated green at the reviewed revision.
- Successful-reply clearing under time pruning and record-cap pressure lacks demonstrated regression coverage.
- The new retention regression evaluates plan_cap_verdict at base plus one hour after pruning at real time roughly 48 hours later; it does not directly assert the verdict at the post-prune instant.
- Historical implementations outside the two changed source files were not re-audited; their not-checked statuses are not acceptance evidence.
- Skipped collaboration artifacts other than the brief, unchanged source outside the targeted ranges, and the broader wave-9 diff.

### First-run checklist (observable)

- [ ] Record the tested commit and observe zero failures from the full workspace suite and pinned plugin harness on that same revision.
- [ ] For the 20:00/20:20/22:00 identified-failure fixture, observe a cap refusal at 22:11 with reset 2026-10-26T16:00Z and no engine-turn marker.
- [ ] After a machine-only pair is pruned at plus 48 hours, observe that another repository still reports the declared cap refusal before reset.
- [ ] After a later successful reply, observe that both 48-hour pruning and 500-row pressure preserve availability in another repository and do not restore the old cap refusal.
- [ ] At the declared reset, observe that the old pair no longer produces a cap refusal; repeated rows with one consultation ID must never produce a two-hit refusal.
