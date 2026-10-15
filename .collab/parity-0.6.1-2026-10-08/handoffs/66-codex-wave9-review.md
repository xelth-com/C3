# Handoff 66 - Codex: wave9-review

Date: 2026-10-15 00:43 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 9 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 4cb3a6d2-b689-4a89-b666-54dcf1142ad0.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-df66c0c2461f4f96943dd192c3ca14cb.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13c91-dd77-7ed1-90ad-1466ab821cab` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/65-claude-wave9-review.md` (sha256 c668ceb1e28d). Reviewed: 89cc1f4, base 89cc1f489ad66ee8a47427ec4ac0452e80b387f3, tree sha256 3e5cdc0ad442, 0 changed files.
Bridge outcome: usable reply. Wall time: 252.9 s. Tokens: in 395156 (cached 305152), out 9092, reasoning 1392.
Quota (rollout): 5h 50% until 02:23; wk 88% until Oct 20; this run 395156 input tokens (305152 cached) in 7 calls, max 88996 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `68774db..c62d81f` - the range changes 23 files, 1101 lines (1066 insertions, 35 deletions).
Verdict: HOLD - D3 bypasses recovery protection for surviving processes, and reservation, quota-counting, and reset-hint parsing defects also need correction before release. Findings: 1 blocker, 3 major, 1 minor, 1 note (F66-1..F66-6, tracked in `findings.json`). Structured reply: `handoffs/66-codex-wave9-review.reply.json`.
Raw event stream: `handoffs/66-codex-wave9-review.events.jsonl`.
Verbatim reply follows.

---

1. **Q1:** Yes. Detached admission ignores active orphan/survivor records merely because their filename is non-single and `parent_pid` is absent (finding #1). Failed reservation writes also allow number reuse after releasing the task lock (#2). The write lock and duplicate-number rejection protect ledger commits, but cannot make the reservation safe.

2. **Q2:** The counter can mistake one consultation represented in both local and machine health for two failures (#3). Even with accurate counting, transient 429 pairs cause long false outages, while a genuinely exhausted monthly plan without a hint reopens after 24 hours (#6). Malformed Unicode hints can panic during roster validation (#4).

3. **Q3:** Yes. Shims set a process environment variable without restoring it; subsequent native C3 invocations in that PowerShell session inherit compatibility behavior (#5).

4. **Q4: HOLD.** Finding #1 is a blocker; #2–#4 also require fixes. All non-`.collab` diff hunks were read. `.collab` changes and unrelated prior-finding implementations were not reviewed. Tests were inspected, not executed; surrounding implementation was read selectively.

## Requested checks

- **RC1:** Repository root; workspace-write; extend the detached fixture with a dead bridge and live helper in a member-style survivor record, then request another detached run. Require refusal before engine spawn (#1). Budget: one fixture, 5 minutes.
- **RC2:** Repository root; workspace-write; fail the first reservation write, pause before later writes, and start a second detached run. Require the first to abort without number reuse (#2). Budget: one fixture, 5 minutes.
- **RC3:** Repository root; workspace-write; feed one failure through local and machine-health inputs, then evaluate cap verdicts. Require one counted attempt (#3). Budget: one fixture, 5 minutes.
- **RC4:** Repository root; workspace-write; validate `reset_hint="daily:00:00日abcd"`. Require a validation error without panic (#4). Budget: one test.
- **RC5:** Repository root; read-only; invoke a shim with `&` in PowerShell, then inspect `C3_PLUGIN_COMPAT`. Require restoration of its original value (#5). Budget: 1 minute.

---

### Findings

- **F66-1** [blocker] `crates/c3/src/consult/recovery.rs:129`, `crates/c3/src/consult/recovery.rs:62`, `crates/c3/src/consult/orchestrate.rs:4482` - Detached admission treats active recovery records as harmless concurrency without establishing that their owning bridge is still alive. Records retained because an interrupted run left surviving or unverified processes are therefore bypassed, allowing a new engine turn while recovery still considers the previous tree active. Trigger: A detached single run leaves .consult.pending-<NN>.json with no parent_pid; its bridge exits, but a recorded survivor remains alive. Another detached single run targets the same task. Evidence: read-code: The concurrency flag depends only on filename and absence/nullness of parent_pid. active_message_for_detached_single excludes every active record with that flag, regardless of the reason it is active.; read-code: The liveness checker distinguishes a live writer from surviving and unverified processes after interruption; both conditions produce active records.; read-code: Detached runs use the filtered admission result and skip processing active concurrent records. Verify: Run RC1 with a dead owning bridge and a live recorded helper; assert that a new detached request refuses before writing an engine-turn marker. Remedy: Permit concurrency only for an explicitly identified detached run with a positively verified live owning bridge and an appropriate active lifecycle state. Keep orphan, survivor, unverified, kill-unconfirmed, and uncertain recovery states blocking.
- **F66-2** [major] `crates/c3/src/consult/orchestrate.rs:4444`, `crates/c3/src/consult/orchestrate.rs:4507`, `crates/c3/src/consult/orchestrate.rs:4522` - Detached runs release the task lock without requiring successful number allocation and durable reservation. A failed reservation write can leave the allocated n invisible to the next run, allowing duplicate allocation and eventual commit failure; matching pending filenames can also overwrite each other's recovery evidence. Trigger: The first detached run's reservation write fails transiently. It releases the task lock and pauses before any later successful pending write or commit. A second detached run allocates its numbers during that interval. Evidence: read-code: next_numbers errors retain the earlier context numbers, write_pending's result is discarded, and task_lock is then dropped.; read-code: Number allocation derives n from persisted sessions, findings, and recovery reservations; there is no independent allocation counter.; read-code: Commit rejects duplicate n rather than renumbering it, so serialization of commits does not repair duplicate allocation. Verify: Run RC2 with a one-shot reservation-write failure and a barrier after lock release; verify that no second run can receive the first run's allocated n. Remedy: Propagate next_numbers and reservation-write failures. Release the task lock only after the reservation has been successfully persisted.
- **F66-3** [major] `crates/c3-core/src/health.rs:1122`, `crates/c3-core/src/plan.rs:340`, `crates/c3/src/providers.rs:172`, `crates/c3/src/providers.rs:2327` - The two-hit cap detector counts health rows rather than distinct consultations. A single instant 429 represented in both the repository ledger and machine health can satisfy the two-failure threshold and incorrectly disable the endpoint until its declared reset. Trigger: One fast 429 is committed locally and recorded in machine health for a capped endpoint. Providers or consultation preflight combines both sources without a newer usable reply. Evidence: read-code: Both availability and consultation health inputs append machine-health synthetic consultations to repository consultations.; read-code: Each qualifying input row contributes a hit; the collection does not deduplicate consultation identity.; read-code: Any two hits within the hour qualify, including two hits at the same instant.; inferred: Two representations of one actual failed attempt can therefore become the required pair. The new unit tests use distinct synthetic consultations and do not exercise this production input combination. Verify: Run RC3 through production health conversion with exactly one committed fast failure and its machine-health record; assert that the endpoint retains ordinary burst handling. Remedy: Carry a stable consultation identity through machine health and deduplicate before counting attempts. Do not use timestamp equality alone as attempt identity.
- **F66-4** [major] `crates/c3-core/src/plan.rs:253`, `crates/c3-core/src/roster.rs:606` - Reset-hint validation can panic on malformed Unicode instead of rejecting the roster because split_clock splits a UTF-8 string at an unchecked byte offset. Trigger: A roster supplies ext.c3.reset_hint="daily:00:00日abcd". The clock suffix has 12 bytes, and splitting at byte 6 cuts inside 日. Evidence: read-code: The parser checks only byte length before calling s.split_at(s.len() - 6).; read-code: Roster validation directly invokes next_cap_reset on the supplied string.; inferred: The computed split offset is not a character boundary, so Rust's string split panics before normal validation can return None. Verify: Run RC4 through validate_roster and require a normal validation error rather than process termination. Remedy: Validate the ASCII grammar before slicing, or use checked string ranges and return None for invalid boundaries.
- **F66-5** [minor] `tests/shim/codex-consult.ps1:210`, `tests/shim/codex-providers.ps1:136`, `tests/shim/codex-findings.ps1:104`, `tests/shim/codex-consult-hook.ps1:102`, `crates/c3-core/src/spelling.rs:9` - The compatibility switch can escape a shim invocation into subsequent native C3 runs in the same PowerShell process, restoring fork-by-default behavior and PowerShell command spellings unexpectedly. Trigger: Invoke a shim with PowerShell's call operator in a persistent session, then run native c3 from that session. Evidence: read-code: The four shims assign the process environment variable without restoring its previous value.; read-code: Native behavior trusts the inherited variable directly; panel default mode remains automatic when it equals 1. Verify: Run RC5 from a persistent PowerShell session with the variable initially absent, then confirm a native panel dry run still defaults to new. Remedy: Set the compatibility variable only in the launched child's environment, or save and restore it with guaranteed cleanup around every shim exit path.
- **F66-6** [note] `crates/c3-core/src/plan.rs:340`, `crates/c3-core/src/plan.rs:353`, `docs/port/wave9-consilium-defects.md:51` - The declared-cap heuristic cannot distinguish cap exhaustion from ordinary transient rate limiting: two genuine fast transient 429s can disable a usable endpoint until a distant monthly reset. Conversely, a truly exhausted monthly plan with no reset hint becomes eligible after 24 hours even if its cap remains exhausted. Trigger: A monthly-capped endpoint emits two transient fast 429s within an hour and then recovers; alternatively, its monthly quota remains exhausted more than 24 hours after the second failure and reset_hint is absent. Evidence: read-code: The decision uses declared cap presence and the failure pair, then applies the hint or a fixed 24-hour fallback. It has no independent exhaustion confirmation.; read-code: Tests establish the chosen policy, including the 24-hour fallback, but do not establish that a fast 429 pair identifies actual cap exhaustion. Verify: With a controlled clock, evaluate a transient pair before a distant monthly reset and an exhausted no-hint pair after 24 hours; explicitly assess whether both resulting availability decisions are acceptable release policy. Remedy: Label this as inferred exhaustion and define a bounded probe or override policy. Prefer authoritative quota/reset evidence when available, and document the no-hint retry as a probe rather than evidence that capacity returned.

### Prior findings

- F02-1 - not-checked - Telemetry spool fix is outside this diff review.
- F02-3 - not-checked - Deletion retry behavior was not reviewed.
- F02-4 - not-checked - Full harness coverage map was not reviewed.
- F02-5 - not-checked - Rating transaction ordering was not reviewed.
- F02-6 - not-checked - Prompt anchors were not reviewed.
- F02-7 - not-checked - Cross-engine plan identity was not revalidated.
- F02-8 - not-checked - Native Messages benchmark evidence was not reviewed.
- F04-1 - not-checked - Process-name Unicode handling was not reviewed.
- F04-2 - not-checked - Windows DST production behavior was not tested.
- F04-3 - not-checked - Required-light ledger metadata was not reviewed.
- F06-1 - not-checked - Historical Samoa handling was not reviewed.
- F09-1 - not-checked - Context configuration parsing was not reviewed.
- F09-2 - not-checked - Flush/deletion locking was not reviewed.
- F09-3 - not-checked - Recoverable deletion cleanup was not reviewed.
- F09-4 - not-checked - Spool reread failure handling was not reviewed.
- F09-5 - not-checked - Legacy telemetry sanitization was not reviewed.
- F09-6 - not-checked - Rating actor inference was not reviewed.
- F11-1 - not-checked - Journal helpers were encountered, but full failure propagation was not revalidated.
- F11-2 - not-checked - Required AGY launcher selection was not reviewed.
- F11-3 - not-checked - Required-reviewer shared-plan evaluation was not revalidated.
- F11-4 - not-checked - Production hook pointer execution was not tested.
- F11-5 - not-checked - AGY tool stall allowance was not reviewed.
- F14-1 - not-checked - Local-only deletion persistence was not reviewed.
- F19-1 - not-checked - Complaint/deletion synchronization was not reviewed.
- F19-2 - not-checked - Unreadable salt cleanup was not reviewed.
- F19-3 - not-checked - Duplicate-key telemetry sanitization was not reviewed.
- F22-1 - not-checked - Journal replay durability was not revalidated.
- F23-1 - not-checked - Process metadata failure classification was not reviewed.
- F23-2 - not-checked - Unknown creation-time scan handling was not reviewed.
- F23-3 - not-checked - Kill-site persistence ordering was not revalidated.
- F23-4 - not-checked - Earlier recovery test coverage was not audited.
- F23-5 - not-checked - Production Samoa boundary behavior was not tested.
- F24-1 - not-checked - Not-spooled fallback accounting was not reviewed.
- F24-2 - not-checked - Unterminated suffix accounting was not reviewed.
- F24-3 - not-checked - Concurrent not-spooled cleanup was not reviewed.
- F24-4 - not-checked - Not-spooled status timestamp selection was not reviewed.
- F24-5 - not-checked - Telemetry oracle path and lock-domain coverage was not reviewed.
- F24-6 - not-checked - Fold record rewrite failures were not reviewed.
- F25-2 - not-checked - Endpoint transcript safety was not revalidated.
- F25-3 - not-checked - Claude model case normalization was not reviewed.
- F25-4 - not-checked - Credential Debug redaction was not reviewed.
- F30-1 - not-checked - Unknown-tree helper classification was not revalidated.
- F30-2 - not-checked - Kill evidence write failure handling was not revalidated.
- F30-3 - not-checked - Continuation and repair kill durability tests were not reviewed.
- F31-1 - not-checked - Live-producer fallback accounting was not reviewed.
- F36-1 - not-checked - Transcript path Unicode handling was not reviewed.
- F36-2 - not-checked - Transcript path NFC normalization was not reviewed.
- F38-1 - not-checked - Multi-write failure recovery durability was not revalidated.
- F40-1 - not-checked - Kill-copy enumeration failures were not revalidated.
- F40-2 - not-checked - Kick acknowledgement on persistence failure was not reviewed.
- F42-1 - not-checked - Recovery-copy metadata error handling was not revalidated.
- F46-1 - not-checked - Equal-time quota aggregation was not reviewed.
- F48-1 - not-checked - Secondary-window quota ordering was not reviewed.
- F50-1 - not-checked - Quota reset-completeness ties were not reviewed.
- F52-1 - not-checked - Quota aggregation clock consistency was not reviewed.
- F52-2 - not-checked - Nonfinite quota percentage handling was not reviewed.
- F55-1 - not-checked - Account cache reset-boundary behavior was not reviewed.
- F55-2 - not-checked - Invalid newer quota observation selection was not reviewed.
- F58-1 - not-checked - Per-window observation timestamp aggregation was not reviewed.
- F60-1 - not-checked - Per-window timestamp persistence was not reviewed.
- F62-1 - not-checked - Fractional timestamp persistence was not reviewed.

## Verdict: HOLD

D3 bypasses recovery protection for surviving processes, and reservation, quota-counting, and reset-hint parsing defects also need correction before release.

### Blockers

- **F66-1** `crates/c3/src/consult/recovery.rs:129`, `crates/c3/src/consult/recovery.rs:62`, `crates/c3/src/consult/orchestrate.rs:4482` - Detached admission treats active recovery records as harmless concurrency without establishing that their owning bridge is still alive. Records retained because an interrupted run left surviving or unverified processes are therefore bypassed, allowing a new engine turn while recovery still considers the previous tree active. Verify: Run RC1 with a dead owning bridge and a live recorded helper; assert that a new detached request refuses before writing an engine-turn marker. Remedy: Permit concurrency only for an explicitly identified detached run with a positively verified live owning bridge and an appropriate active lifecycle state. Keep orphan, survivor, unverified, kill-unconfirmed, and uncertain recovery states blocking.
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

### Unproven scenarios

- No tests or runtime reproductions were executed; the findings are based on source inspection and stated deductions.
- The reported workspace, clippy, formatting, and plugin harness results were not independently reproduced.
- The changed .collab/wave9-STATE.md was skipped as requested; unrelated prior-finding implementations and most unchanged code were not read.
- The full machine-health conversion implementation, cross-repository cap retention, and shared-plan propagation of inferred cap outages were not completely traced.
- The new detached test demonstrates successful overlap and distinct numbers under normal I/O, not reservation failure, abandoned-tree recovery, or adversarial commit schedules.

### First-run checklist (observable)

_(none)_
