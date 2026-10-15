# Handoff 68 - Codex: wave9-round2

Date: 2026-10-15 02:18 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 9 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 7e3e9311-6a26-4a09-8ec1-6303064da172.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-fb28bafc3bc74a85b3354fdfc6375aa7.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13cea-4ebd-72d2-84da-6d4849cb3a61` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/67-claude-wave9-round2.md` (sha256 8f5c8359a65c). Reviewed: a0a5c50, base a0a5c5058206b4a5b4315bd3f63ab44bf7b3dc02, tree sha256 4348dec2e2bc, 0 changed files.
Bridge outcome: usable reply. Wall time: 187.4 s. Tokens: in 365418 (cached 293248), out 6348, reasoning 1062.
Quota (rollout): 5h 69% until 02:23; wk 91% until Oct 20; this run 365418 input tokens (293248 cached) in 7 calls, max 71057 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `c62d81f..f2ff103` - the range changes 17 files, 753 lines (728 insertions, 25 deletions).
Verdict: HOLD - Time-based deduplication still merges distinct consultations and suppresses the declared-cap verdict; the new recovery test also introduces an unconditional PowerShell dependency. Findings: 0 blocker, 1 major, 1 minor, 0 note (F68-1..F68-2, tracked in `findings.json`). Structured reply: `handoffs/68-codex-wave9-round2.reply.json`.
Raw event stream: `handoffs/68-codex-wave9-round2.events.jsonl`.
Verbatim reply follows.

---

1. **Q1:** I found no remaining instance of the original F66-1/F66-2 paths in the reviewed control flow: concurrent admission requires a live writer and ordinary recovery state; allocation or reservation failure returns before releasing the task lock. The reservation test exercises the helper sequentially, not two complete detached runs under injected failure.

2. **Q2:** Yes—finding #1. Different consultation IDs with identical hit timestamps collapse into one hit; the new `twin` test explicitly expects this incorrect result. The legacy five-second match also cannot distinguish a mirror from an independent failure in another repository. F66-3’s original double-counting trigger is fixed, but the replacement undercounts distinct failures.

3. **Q3:** **HOLD v0.2.2 for finding #1.** Also fix the portable-test regression in finding #2. F66-4/F66-5 are addressed in code. F66-6 is now an explicit conservative policy, not proof of exhaustion.

Read all non-`.collab` changed hunks, the brief, handoff 65, and targeted surrounding code. Skipped `.collab` state diffs and the store implementation. No tests were run; older findings remain unchecked unless stated otherwise.

## Requested checks

- **RC1:** Repository root; workspace-write; extend the health fixture with distinct IDs at identical timestamps and independent legacy/ledger failures within five seconds. Require two hits and the cap verdict, while genuine mirrors count once. Budget: 30 minutes.
- **RC2:** Repository root on Linux without `powershell`; workspace-write; run `cargo test -p c3 a_detached_run_admits_only_records_of_a_live_owning_bridge`. Require a passing portable fixture. Budget: 10 minutes after compilation.

---

### Findings

- **F68-1** [major] `crates/c3-core/src/health.rs:1149`, `crates/c3-core/src/health.rs:1161`, `crates/c3-core/src/plan.rs:497` - Time-based deduplication merges distinct consultations and can suppress the declared-cap verdict. Even different nonempty consultation IDs count once when their hit timestamps match. An unidentified legacy machine record is also discarded solely because an endpoint-matching ledger failure finished nearby, although it can represent an independent consultation. Trigger: Two fast 429 consultations for the same capped endpoint have different consult_id values but identical hit timestamps; alternatively, an independent legacy machine-health failure from another repository occurs within five seconds of a local ledger failure. Evidence: read-code: The legacy match uses endpoint and elapsed time without consultation identity; the final hit insertion suppresses equal timestamps regardless of distinct IDs.; read-code: The new test constructs c-1 and c-2 as separate ledger consultations with matching timestamps and asserts that they count as one.; read-code: The cap override requires at least two retained hits, so the collapsed pair cannot trigger it. Verify: Run RC1, asserting two retained hits and a cap-unavailable verdict for distinct consultations, and one hit for a genuine ledger/machine mirror. Remedy: Deduplicate known consultations by stable identity and preserve distinct IDs even at identical timestamps. Treat legacy time proximity as uncertain correlation rather than proven identity; use stronger provenance where available and explicitly define the fallback policy.
- **F68-2** [minor] `crates/c3/src/consult/recovery.rs:310`, `crates/c3/src/consult/recovery.rs:320` - The new recovery unit test unconditionally launches the Windows PowerShell executable, making the c3 unit suite fail on non-Windows systems without an executable named powershell. Trigger: Run the c3 unit tests on Linux or macOS without Windows PowerShell's powershell executable. Evidence: read-code: The test module is guarded only by cfg(test); the new test uses Command::new("powershell") and expect("a sleeping child") without a platform guard or alternative.; read-code: The separate PowerShell-dependent integration tests correctly declare cfg(windows); the new recovery unit test does not. Verify: Run RC2 on a Linux host without powershell and observe whether the fixture can create its sleeping child. Remedy: Use a portable sleeping-child fixture or platform-specific launchers; if the fixture is intentionally Windows-only, guard it and retain deterministic cross-platform coverage of admission decisions.

### Prior findings

- F02-1 - not-checked - Outside this review range.
- F02-3 - not-checked - Outside this review range.
- F02-4 - not-checked - Harness coverage map not audited.
- F02-5 - not-checked - Rating transaction not reviewed.
- F02-6 - not-checked - Prompt anchor not reviewed.
- F02-7 - not-checked - Cross-engine plan compatibility not reviewed.
- F02-8 - not-checked - No route benchmark examined.
- F04-1 - not-checked - Process-name classification not reviewed.
- F04-2 - not-checked - Windows local-zone boundaries not tested.
- F04-3 - not-checked - Panel ledger metadata not reviewed.
- F06-1 - not-checked - Historical Windows offsets not reviewed.
- F09-1 - not-checked - Context-window configuration not reviewed.
- F09-2 - not-checked - Telemetry deletion locking not reviewed.
- F09-3 - not-checked - Deletion cleanup durability not reviewed.
- F09-4 - not-checked - Spool reread failure handling not reviewed.
- F09-5 - not-checked - Legacy telemetry sanitization not reviewed.
- F09-6 - not-checked - Rating actor inference not reviewed.
- F11-1 - not-checked - Journal I/O recovery not reviewed.
- F11-2 - not-checked - Required-reviewer launcher propagation not reviewed; handoff harness results are not independent verification.
- F11-3 - not-checked - Required-reviewer shared-plan evaluation not reviewed.
- F11-4 - not-checked - Production hook pointer execution not checked.
- F11-5 - not-checked - AGY stall behavior not reviewed.
- F14-1 - not-checked - Local-only deletion transaction not reviewed.
- F19-1 - not-checked - Complaint/deletion concurrency not reviewed.
- F19-2 - not-checked - Unreadable salt cleanup not reviewed.
- F19-3 - not-checked - Duplicate-key sanitization not reviewed.
- F22-1 - not-checked - Journal replay retention not reviewed.
- F23-1 - not-checked - Metadata lookup failure handling not audited.
- F23-2 - not-checked - Unknown creation-time scans not audited.
- F23-3 - not-checked - Kill-site persistence not reviewed.
- F23-4 - not-checked - Earlier recovery fixtures not audited.
- F23-5 - not-checked - Production Samoa behavior not tested.
- F24-1 - not-checked - Not-spooled accounting not reviewed.
- F24-2 - not-checked - Unterminated fold suffix handling not reviewed.
- F24-3 - not-checked - Cleanup/producer race not reviewed.
- F24-4 - not-checked - Status suffix timestamps not reviewed.
- F24-5 - not-checked - Telemetry oracle adapter paths and locks not audited.
- F24-6 - not-checked - Post-fold record rewrite not reviewed.
- F25-2 - not-checked - Claude transcript guard not reviewed.
- F25-3 - not-checked - Claude model validation not reviewed.
- F25-4 - not-checked - Child environment debug redaction not reviewed.
- F30-1 - not-checked - Unknown helper ancestry not audited.
- F30-2 - not-checked - Pre-kill durability not audited.
- F30-3 - not-checked - Continuation and repair kill fixtures not reviewed.
- F31-1 - not-checked - Live-producer accounting transition not reviewed.
- F36-1 - not-checked - Unicode transcript path handling not reviewed.
- F36-2 - not-checked - Transcript NFC normalization not reviewed.
- F38-1 - not-checked - All-copy kill persistence failure not audited.
- F40-1 - not-checked - Recovery propagates listing errors, but discovery implementation and failure injection were not checked.
- F40-2 - not-checked - Kick acknowledgement behavior not reviewed.
- F42-1 - not-checked - Recovery propagates read errors, but metadata-error handling in read_pending_file was not audited.
- F46-1 - not-checked - Quota observation aggregation not reviewed.
- F48-1 - not-checked - Secondary quota-window selection not reviewed.
- F50-1 - not-checked - Reset-completeness ranking not reviewed.
- F52-1 - not-checked - Quota comparison-clock behavior not reviewed.
- F52-2 - not-checked - NaN quota handling not reviewed.
- F55-1 - not-checked - Quota account-cache behavior not reviewed.
- F55-2 - not-checked - Invalid newer quota observations not reviewed.
- F58-1 - not-checked - Per-window observation timestamps not reviewed.
- F60-1 - not-checked - Quota persistence round-trip not reviewed.
- F62-1 - not-checked - Fractional quota timestamps not reviewed.
- F66-1 - fixed - Source review confirms concurrent admission requires the live-writer verdict, an ordinary lifecycle state, and no survivor/unverified/kill evidence. No runtime test executed.
- F66-2 - fixed - Allocation errors and failed reservation persistence/read-back now refuse before releasing the task lock. Full concurrent fault-injection behavior remains unproven.
- F66-3 - fixed - The original mirrored-row double count is addressed by propagated consult_id and legacy matching. Finding #1 describes a separate undercounting defect introduced by time-based deduplication.
- F66-4 - fixed - ASCII validation precedes byte slicing; malformed Unicode cases are covered in the added roster fixture.
- F66-5 - fixed - All four changed shims restore the previous compatibility variable in finally; the added persistent-session fixture covers the initially absent case.
- F66-6 - fixed - The transient-pair hold and no-hint 24-hour eligibility are explicitly documented and pinned as policy. This resolves the policy-disclosure issue, not the heuristic's inherent uncertainty.

## Verdict: HOLD

Time-based deduplication still merges distinct consultations and suppresses the declared-cap verdict; the new recovery test also introduces an unconditional PowerShell dependency.

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
- **F66-1** (prior, fixed) `crates/c3/src/consult/recovery.rs:129` - Detached admission treats active recovery records as harmless concurrency without establishing that their owning bridge is still alive. Records retained because an interrupted run left surviving or unverified processes are therefore bypassed, allowing a new engine turn while recovery still considers the previous tree active.

### Unproven scenarios

- No tests, builds, harnesses, or live fault-injection procedures were executed; the handoff's reported results were not independently verified.
- Two complete detached runs with a reservation-write failure and synchronization barrier were not exercised.
- The underlying store allocation and atomic-write implementations were located but not read within the six-call budget.
- The .collab state.md and wave9-STATE.md diffs were skipped as directed; handoff 65 was read as a referenced document rather than audited as a diff.
- Earlier findings outside the changed behavior were not revalidated.
- Restoration of a pre-existing compatibility value and exceptional shim exits were inspected in code but not executed.

### First-run checklist (observable)

_(none)_
