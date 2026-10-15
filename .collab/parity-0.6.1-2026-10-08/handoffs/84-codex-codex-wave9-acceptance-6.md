# Handoff 84 - Codex: codex-wave9-acceptance-6

Date: 2026-10-15 11:35 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - entry 1 of 10 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 39a17803-7161-4cad-b9a3-b3bacb2b7d0b.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-27b957a7a132478fa79788b1988e879d.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13ee6-1afd-7331-ba45-6465ab61375a` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/83-claude-wave9-acceptance-6.md` (sha256 052ec8c68814). Reviewed: 3b636f8, base 3b636f8d51ac74db89f69ad3ed4d0225e80d0ced, tree sha256 b951a66c4254, 0 changed files.
Bridge outcome: usable reply. Wall time: 326.9 s. Tokens: in 312102 (cached 243456), out 5867, reasoning 1356.
Quota (rollout): 5h 61% until 14:18; wk 10% until Oct 22; this run 312102 input tokens (243456 cached) in 6 calls, max 67675 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s. Range: `2bbd27e..8ab6cea` - the range changes 10 files, 2033 lines (1949 insertions, 84 deletions).
Verdict: HOLD - Equal-time success rows can exhaust the protected tier, evict a clearing anchor, and subsequently allow delayed evidence to resurrect its cleared refusal. Findings: 1 blocker, 0 major, 0 minor, 0 note (F84-1, tracked in `findings.json`). Structured reply: `handoffs/84-codex-codex-wave9-acceptance-6.reply.json`.
Raw event stream: `handoffs/84-codex-codex-wave9-acceptance-6.events.jsonl`.
Verbatim reply follows.

---

1. The original F82-1 fixture is addressed, but closure is incomplete: finding #1 describes resurrection after tied anchors cause eviction. Otherwise, repeated skipped updates preserve the anchor; readable journal/direct inputs merge before superseding; at the 35-day boundary, older cleared evidence expires no later than its success. These conclusions are code-derived, not executed.

2. **HOLD** for finding #1, superseding F82-1. Workspace and harness results also remain outstanding. Read the production diff, relevant documentation, and added tests; skipped `.collab/` changes, unrelated prior implementations, and remaining unchanged code. Checkout HEAD was `3b636f8`; the diff explicitly targeted `8ab6cea`.

## Requested checks

- **RC1:** In `C:\Users\Dmytro\C3`, workspace-write, extend the skipped-journal fixture with finding #1’s schedule; require zero old hits and availability. Budget: 30 minutes.
- **RC2:** Same directory, workspace-write: `cargo test -j 2 --no-fail-fast --workspace` on the candidate revision; require zero failures. Budget: 30 minutes.

---

### Findings

- **F84-1** [blocker] `crates/c3-core/src/health.rs:2231`, `crates/c3-core/src/health.rs:2287`, `crates/c3-core/src/health.rs:2457`, `docs/port/wave9-consilium-defects.md:222` - The clearing-anchor protection is not durable under tied-success cap pressure. is_anchor protects every success row sharing an endpoint's newest timestamp, rather than one row. Those duplicates can force eviction of another endpoint's clearing anchor. A later success makes the duplicates unprotected, allowing delayed older evidence for the evicted endpoint to survive and restore a cleared cap refusal. Trigger: Keep X's two identified instant-429 records in an exclusively held journal, with a still-future monthly reset. The machine file contains 497 recent success anchors for distinct other endpoints and three distinct success records for Y sharing its newest timestamp. Add X's clearing success, timestamped after its pair but before all filler successes: all 501 rows qualify as anchors, so tier 3 evicts X. While the journal remains held, add a newer Y success: one old Y row is evicted as unprotected, leaving two. Release and apply the journal: the two arriving X failures displace those remaining unprotected Y rows, leaving 498 anchors plus X's obsolete pair. X has no surviving success to suppress its two hits. Evidence: read-code: The anchor map stores only each endpoint's maximum timestamp; is_anchor accepts every success row equal to that timestamp and does not select a unique record.; read-code: The cap removes unprotected rows first, evidence second, and oldest anchors last, permitting X's eviction when all 501 rows qualify as anchors.; read-code: Superseding uses only successes in the currently merged endpoint rows; no retained X watermark remains after X's anchor is evicted.; inferred: After Y advances, its tied older rows become unprotected and absorb the overflow from X's delayed pair, allowing both failures to remain. This schedule was not executed. Verify: Extend machine_health_anchor_supersedes_evidence_from_a_skipped_journal with the trigger schedule, using distinct record keys for the three equal-time Y successes and the real exclusive journal lock. Assert the persisted rows after each update, then require machine-only instant_429s.len() == 0 and an available cap verdict after replay. Remedy: Select one deterministic clearing-anchor record per endpoint, treating tied redundant successes as ordinary rows, and add this multi-update cap-pressure regression. Ensure eviction cannot discard the sole clearing proof while duplicate proofs consume protected capacity. Supersedes: F82-1.

### Prior findings

- F02-1 - not-checked - Outside this delta; not revalidated.
- F02-3 - not-checked - Outside this delta; not revalidated.
- F02-4 - not-checked - Outside this delta; not revalidated.
- F02-5 - not-checked - Outside this delta; not revalidated.
- F02-6 - not-checked - Outside this delta; not revalidated.
- F02-7 - not-checked - Outside this delta; not revalidated.
- F02-8 - not-checked - Benchmark evidence not reviewed.
- F04-1 - not-checked - Outside this delta; not revalidated.
- F04-2 - not-checked - Windows transition behavior not revalidated.
- F04-3 - not-checked - Outside this delta; not revalidated.
- F06-1 - not-checked - Samoa behavior not revalidated.
- F09-1 - not-checked - Outside this delta; not revalidated.
- F09-2 - not-checked - Outside this delta; not revalidated.
- F09-3 - not-checked - Outside this delta; not revalidated.
- F09-4 - not-checked - Outside this delta; not revalidated.
- F09-5 - not-checked - Outside this delta; not revalidated.
- F09-6 - not-checked - Outside this delta; not revalidated.
- F11-1 - not-checked - Read-error handling inspected incidentally; full failure schedules not revalidated.
- F11-2 - not-checked - Outside this delta; not revalidated.
- F11-3 - not-checked - Outside this delta; not revalidated.
- F11-4 - not-checked - Outside this delta; not revalidated.
- F11-5 - not-checked - Outside this delta; not revalidated.
- F14-1 - not-checked - Outside this delta; not revalidated.
- F19-1 - not-checked - Outside this delta; not revalidated.
- F19-2 - not-checked - Outside this delta; not revalidated.
- F19-3 - not-checked - Outside this delta; not revalidated.
- F22-1 - not-checked - Journal marks inspected only partially; failure schedules not revalidated.
- F23-1 - not-checked - Outside this delta; not revalidated.
- F23-2 - not-checked - Outside this delta; not revalidated.
- F23-3 - not-checked - Outside this delta; not revalidated.
- F23-4 - not-checked - Outside this delta; not revalidated.
- F23-5 - not-checked - Production Samoa checks not reviewed.
- F24-1 - not-checked - Outside this delta; not revalidated.
- F24-2 - not-checked - Outside this delta; not revalidated.
- F24-3 - not-checked - Outside this delta; not revalidated.
- F24-4 - not-checked - Outside this delta; not revalidated.
- F24-5 - not-checked - Outside this delta; not revalidated.
- F24-6 - not-checked - Outside this delta; not revalidated.
- F25-2 - not-checked - Outside this delta; not revalidated.
- F25-3 - not-checked - Outside this delta; not revalidated.
- F25-4 - not-checked - Outside this delta; not revalidated.
- F30-1 - not-checked - Outside this delta; not revalidated.
- F30-2 - not-checked - Outside this delta; not revalidated.
- F30-3 - not-checked - Outside this delta; not revalidated.
- F31-1 - not-checked - Outside this delta; not revalidated.
- F36-1 - not-checked - Outside this delta; not revalidated.
- F36-2 - not-checked - Outside this delta; not revalidated.
- F38-1 - not-checked - Outside this delta; not revalidated.
- F40-1 - not-checked - Outside this delta; not revalidated.
- F40-2 - not-checked - Outside this delta; not revalidated.
- F42-1 - not-checked - Outside this delta; not revalidated.
- F46-1 - not-checked - Quota aggregation not reviewed.
- F48-1 - not-checked - Quota aggregation not reviewed.
- F50-1 - not-checked - Quota aggregation not reviewed.
- F52-1 - not-checked - Quota aggregation not reviewed.
- F52-2 - not-checked - Quota aggregation not reviewed.
- F55-1 - not-checked - Quota cache not reviewed.
- F55-2 - not-checked - Quota aggregation not reviewed.
- F58-1 - not-checked - Quota aggregation not reviewed.
- F60-1 - not-checked - Quota persistence not reviewed.
- F62-1 - not-checked - Quota timestamp persistence not reviewed.
- F66-1 - not-checked - Detached admission not reviewed.
- F66-2 - not-checked - Detached reservation not reviewed.
- F66-3 - not-checked - Identity deduplication read, but production ledger/machine conversion not revalidated.
- F66-4 - not-checked - Reset-hint parser not reviewed.
- F66-5 - not-checked - Shim environment restoration not reviewed.
- F66-6 - not-checked - Documented policy read; controlled-clock scenarios not revalidated.
- F68-1 - not-checked - Current identity filter read; complete production scenarios not revalidated.
- F68-2 - not-checked - Portable process fixture not inspected.
- F70-1 - fixed - The inspected endpoint_health filter excludes no-ID rows from cap-pair evidence, eliminating this legacy matching ambiguity.
- F72-1 - fixed - The inspected reader excludes legacy no-ID rows from the pair regardless of persisted age.
- F72-2 - fixed - Cap-pair selection excludes no-ID rows and deduplicates identified rows by consult_id rather than truncated message text.
- F74-1 - still-open - Confirmed as intentional policy: all attempts sharing one consult_id count once; the wave note explicitly accepts ordinary burst handling for this case.
- F74-2 - not-checked - Documentation says roster validation prevents the trigger; validator not inspected.
- F76-1 - not-checked - The 35-day reader bound was inspected; downstream hinted-period evaluation not revalidated.
- F78-1 - not-checked - Newest qualifying-pair selection in plan_cap_verdict not inspected.
- F78-2 - fixed - Inspected retention explicitly preserves identified quota/burst evidence for 35 days independently of expired burst until; record-cap loss remains an acknowledged policy limit.
- F80-1 - fixed - For evidence already merged with its clearing success, the inspected supersede step durably removes the older pair before retention and capping.
- F82-1 - still-open - The original evidence-filler fixture is addressed; finding #1 supersedes the claim with a tied-anchor eviction and delayed-replay schedule.

## Verdict: HOLD

Equal-time success rows can exhaust the protected tier, evict a clearing anchor, and subsequently allow delayed evidence to resurrect its cleared refusal.

### Blockers

- **F84-1** `crates/c3-core/src/health.rs:2231`, `crates/c3-core/src/health.rs:2287`, `crates/c3-core/src/health.rs:2457`, `docs/port/wave9-consilium-defects.md:222` - The clearing-anchor protection is not durable under tied-success cap pressure. is_anchor protects every success row sharing an endpoint's newest timestamp, rather than one row. Those duplicates can force eviction of another endpoint's clearing anchor. A later success makes the duplicates unprotected, allowing delayed older evidence for the evicted endpoint to survive and restore a cleared cap refusal. Verify: Extend machine_health_anchor_supersedes_evidence_from_a_skipped_journal with the trigger schedule, using distinct record keys for the three equal-time Y successes and the real exclusive journal lock. Assert the persisted rows after each update, then require machine-only instant_429s.len() == 0 and an available cap verdict after replay. Remedy: Select one deterministic clearing-anchor record per endpoint, treating tied redundant successes as ordinary rows, and add this multi-update cap-pressure regression. Ensure eviction cannot discard the sole clearing proof while duplicate proofs consume protected capacity.
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
- **F76-1** (prior, not-checked) `crates/c3-core/src/health.rs:1143` - The new read-time age limit discards identified cap-pair evidence after approximately 24 hours even when a valid reset_hint declares a later weekly or monthly reset. plan_cap_verdict then returns the underlying verdict without enforcing that reset. This makes an exhausted endpoint eligible before the retry time previously displayed and contradicts the documented policy that hinted entries remain unavailable until their reset.
- **F78-1** (prior, not-checked) `crates/c3-core/src/plan.rs:368` - The cap detector considers only pairs containing the newest qualifying hit. A later isolated instant 429 therefore suppresses an already established cap pair, allowing an exhausted endpoint to become eligible before its declared reset without any successful reply.
- **F78-2** (prior, fixed) `crates/c3-core/src/health.rs:100` - The 35-day cap-pair retention applies only to rows supplied to endpoint_health; machine-health writes still prune old rows unless their stored outage or quota window remains active. Consequently, cap evidence originating in another repository can disappear after roughly 24 hours, releasing a hinted exhausted endpoint before its declared reset.
- **F80-1** (prior, fixed) `crates/c3-core/src/health.rs:2177` - The new retention policy preserves cap-failure evidence longer and with higher eviction priority than the successful reply that clears it. Once that success disappears, machine-only readers reconstruct the old pair and falsely mark a usable endpoint unavailable until its declared reset.
- **F82-1** (prior, still-open) `crates/c3-core/src/health.rs:2288` - The supersede drop does not prevent an obsolete cap refusal when older evidence remains in a skipped journal until after the clearing success is evicted. Journal replay protection covers records actually read, not unread records in a busy journal, and no durable success watermark remains to reject their later application.

### Unproven scenarios

- Finding #1's multi-update schedule is inferred from the reviewed code and has not been executed.
- No tests were run during this read-only consultation; the brief's reported test results were not independently verified.
- Repeated journal skips, journal-delivered success with directly delivered failures, and exact 35-day boundaries lack executed evidence in this review.
- The plugin's independent retention behavior on the shared machine-health file remains outside this delta.
- Final workspace-suite and pinned harness results for the acceptance candidate remain unverified.
- The checkout was at 3b636f8; explicit diff evidence targets 8ab6cea, while supplementary line-range reads used the checkout.
- Changed .collab bookkeeping and unrelated prior implementations were not read.

### First-run checklist (observable)

- [ ] Record the tested revision and observe the workspace suite finish with zero failed tests; identify any rerun explicitly.
- [ ] Observe the pinned roster harness finish with zero failures and confirm its adapter invokes the candidate C3 binary.
- [ ] In the held-journal scenario, observe successful health writes, retained clearing evidence through successive updates, and at most 500 persisted endpoint rows.
- [ ] After journal release, observe zero obsolete X hits and an available machine-only verdict; the no-success control must retain two qualifying hits and the declared-reset refusal.
- [ ] For finding #1's tied-success schedule, inspect persisted rows after every update and confirm X's clearing proof is preserved or its delayed pair is otherwise rejected.
