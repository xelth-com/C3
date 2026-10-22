# Handoff 95 - Codex: codex-0.2.3-acceptance-4

Date: 2026-10-22 10:39 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - position 1 of 13.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 0bac4237-427a-41fa-9526-3965d27088ed.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-d4ffec76b1224c1ebb6cd45f7666c0f1.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a162c0-8261-77c1-8bae-fc0e0e9a2bdc` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/94-claude-0.2.3-acceptance-4.md` (sha256 13acaef1a4d5). Reviewed: 8098c05, base 8098c05fb55a973add5d3dae3b2762542f247190, tree sha256 9f5fe287c168, 0 changed files.
Bridge outcome: usable reply. Wall time: 197.7 s. Tokens: in 236715 (cached 177792), out 4984, reasoning 554.
Quota (rollout): 5h 58% until 12:59; wk 36% until Oct 27; this run 236715 input tokens (177792 cached) in 5 calls, max 58239 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Warnings: ACCEPT with 26 unchecked prior blocker(s) (F14-1, F22-1, F23-1, F23-2, F23-3, F24-1, F25-2, F30-1, F30-2, F31-1, F36-1, F38-1, F40-1, F42-1, F66-1, F76-1, F78-1, F78-2, F80-1, F82-1, F84-1, F89-1, F89-2, F89-3, F91-1, F91-2)..
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s.
Verdict: ACCEPT - The scoped change closes F93-1 through locked recovery with current-record revalidation; the remaining regression-test synchronization weakness is nonblocking. Findings: 0 blocker, 0 major, 1 minor, 0 note (F95-1, tracked in `findings.json`). Structured reply: `handoffs/95-codex-codex-0.2.3-acceptance-4.reply.json`.
Raw event stream: `handoffs/95-codex-codex-0.2.3-acceptance-4.events.jsonl`.
Verbatim reply follows.

---

1. **Q1: Yes.** (a) `read_record` performs no writes. (b) Recovery locks, re-reads, checks generation, pending status and liveness, and propagates write errors. (c) The documentation describes that contract. (d) The regression checks preservation of the resume claim and refusal of another start; its fixed-sleep synchronization deserves hardening (finding #1).

2. **Q2: ACCEPT `8098c05` for this scoped acceptance pass.** No blocking findings remain in the reviewed change. I read all seven non-`.collab` diffs and selected surrounding code. I did not audit all unchanged writers or historical fixes, inspect test logs, or rerun tests; the reported passing suites remain coordinator evidence.

## Requested checks

- **RC1:** In the repository root, with workspace-write permission, add an explicit barrier-entry acknowledgement to the regression, then run it with recovery guards enabled and reverted. Require pass and failure respectively after acknowledged overlap. Budget: 15 minutes.

---

### Findings

- **F95-1** [minor] `crates/c3-cli/tests/work.rs:1778`, `crates/c3/src/work/record.rs:169` - The regression can pass without exercising delayed recovery because it waits a fixed 1.5 seconds instead of observing that the status process captured the stale record and entered the barrier. Trigger: The status process is delayed until after the test replaces the original running record with quota or the new resume's running record. It then never captures the stale dead-owner snapshot, yet the final assertions can pass. Evidence: read-code: The test spawns status, sleeps 1500 ms, replaces the original record, and later releases the gate without checking a barrier-entry acknowledgement.; read-code: The barrier only waits for a release path; it emits no signal proving that the stale snapshot was captured.; inferred: A late status read can observe quota or the live resume, skip recovery entirely, and leave all existing assertions satisfied. Verify: Run RC1 with an explicit barrier-entry acknowledgement and confirm that restoring the stale unconditional recovery write reliably fails the regression. Remedy: Have the test hook signal barrier entry after capturing the dead-owner snapshot; wait for that signal before replacing the record, with a bounded timeout that fails the test.

### Prior findings

- F02-1 - not-checked - Outside the scoped wave 10c review.
- F02-3 - not-checked - Outside the scoped wave 10c review.
- F02-4 - not-checked - Outside the scoped wave 10c review.
- F02-5 - not-checked - Outside the scoped wave 10c review.
- F02-6 - not-checked - Outside the scoped wave 10c review.
- F02-7 - not-checked - Outside the scoped wave 10c review.
- F02-8 - not-checked - Outside the scoped wave 10c review.
- F04-1 - not-checked - Outside the scoped wave 10c review.
- F04-2 - not-checked - Outside the scoped wave 10c review.
- F04-3 - not-checked - Outside the scoped wave 10c review.
- F06-1 - not-checked - Outside the scoped wave 10c review.
- F09-1 - not-checked - Outside the scoped wave 10c review.
- F09-2 - not-checked - Outside the scoped wave 10c review.
- F09-3 - not-checked - Outside the scoped wave 10c review.
- F09-4 - not-checked - Outside the scoped wave 10c review.
- F09-5 - not-checked - Outside the scoped wave 10c review.
- F09-6 - not-checked - Outside the scoped wave 10c review.
- F11-1 - not-checked - Outside the scoped wave 10c review.
- F11-2 - not-checked - Outside the scoped wave 10c review.
- F11-3 - not-checked - Outside the scoped wave 10c review.
- F11-4 - not-checked - Outside the scoped wave 10c review.
- F11-5 - not-checked - Outside the scoped wave 10c review.
- F14-1 - not-checked - Outside the scoped wave 10c review.
- F19-1 - not-checked - Outside the scoped wave 10c review.
- F19-2 - not-checked - Outside the scoped wave 10c review.
- F19-3 - not-checked - Outside the scoped wave 10c review.
- F22-1 - not-checked - Outside the scoped wave 10c review.
- F23-1 - not-checked - Outside the scoped wave 10c review.
- F23-2 - not-checked - Outside the scoped wave 10c review.
- F23-3 - not-checked - Outside the scoped wave 10c review.
- F23-4 - not-checked - Outside the scoped wave 10c review.
- F23-5 - not-checked - Outside the scoped wave 10c review.
- F24-1 - not-checked - Outside the scoped wave 10c review.
- F24-2 - not-checked - Outside the scoped wave 10c review.
- F24-3 - not-checked - Outside the scoped wave 10c review.
- F24-4 - not-checked - Outside the scoped wave 10c review.
- F24-5 - not-checked - Outside the scoped wave 10c review.
- F24-6 - not-checked - Outside the scoped wave 10c review.
- F25-2 - not-checked - Outside the scoped wave 10c review.
- F25-3 - not-checked - Outside the scoped wave 10c review.
- F25-4 - not-checked - Outside the scoped wave 10c review.
- F30-1 - not-checked - Outside the scoped wave 10c review.
- F30-2 - not-checked - Outside the scoped wave 10c review.
- F30-3 - not-checked - Outside the scoped wave 10c review.
- F31-1 - not-checked - Outside the scoped wave 10c review.
- F36-1 - not-checked - Outside the scoped wave 10c review.
- F36-2 - not-checked - Outside the scoped wave 10c review.
- F38-1 - not-checked - Outside the scoped wave 10c review.
- F40-1 - not-checked - Outside the scoped wave 10c review.
- F40-2 - not-checked - Outside the scoped wave 10c review.
- F42-1 - not-checked - Outside the scoped wave 10c review.
- F46-1 - not-checked - Outside the scoped wave 10c review.
- F48-1 - not-checked - Outside the scoped wave 10c review.
- F50-1 - not-checked - Outside the scoped wave 10c review.
- F52-1 - not-checked - Outside the scoped wave 10c review.
- F52-2 - not-checked - Outside the scoped wave 10c review.
- F55-1 - not-checked - Outside the scoped wave 10c review.
- F55-2 - not-checked - Outside the scoped wave 10c review.
- F58-1 - not-checked - Outside the scoped wave 10c review.
- F60-1 - not-checked - Outside the scoped wave 10c review.
- F62-1 - not-checked - Outside the scoped wave 10c review.
- F66-1 - not-checked - Outside the scoped wave 10c review.
- F66-2 - not-checked - Outside the scoped wave 10c review.
- F66-3 - not-checked - Outside the scoped wave 10c review.
- F66-4 - not-checked - Outside the scoped wave 10c review.
- F66-5 - not-checked - Outside the scoped wave 10c review.
- F66-6 - not-checked - Outside the scoped wave 10c review.
- F68-1 - not-checked - Outside the scoped wave 10c review.
- F68-2 - not-checked - Outside the scoped wave 10c review.
- F70-1 - not-checked - Outside the scoped wave 10c review.
- F72-1 - not-checked - Outside the scoped wave 10c review.
- F72-2 - not-checked - Outside the scoped wave 10c review.
- F74-1 - not-checked - Outside the scoped wave 10c review.
- F74-2 - not-checked - Outside the scoped wave 10c review.
- F76-1 - not-checked - Outside the scoped wave 10c review.
- F78-1 - not-checked - Outside the scoped wave 10c review.
- F78-2 - not-checked - Outside the scoped wave 10c review.
- F80-1 - not-checked - Outside the scoped wave 10c review.
- F82-1 - not-checked - Outside the scoped wave 10c review.
- F84-1 - not-checked - Outside the scoped wave 10c review.
- F89-1 - not-checked - Earlier worker fix not independently revalidated in this scoped review.
- F89-2 - not-checked - Selected admission code was read, but the complete earlier fix and runtime scenarios were not revalidated.
- F89-3 - not-checked - Earlier worker fix not independently revalidated in this scoped review.
- F89-4 - not-checked - Earlier worker fix not independently revalidated in this scoped review.
- F89-5 - not-checked - Earlier worker fix not independently revalidated in this scoped review.
- F89-6 - not-checked - Persistence fields and option helpers were visible, but the full resume scenario was not revalidated.
- F89-7 - not-checked - Earlier worker fix not independently revalidated in this scoped review.
- F89-8 - not-checked - Earlier worker fix not independently revalidated in this scoped review.
- F91-1 - not-checked - Locked status and exhaustion checks were partially inspected; the complete claim and concurrent terminal-state scenarios were not revalidated.
- F91-2 - not-checked - Strict record reading was inspected, but the complete enumeration and inspection-failure scenarios were not revalidated.
- F91-3 - not-checked - Earlier artifact-read fix not independently revalidated in this scoped review.
- F91-4 - not-checked - Earlier terminal-evidence fix not independently revalidated in this scoped review.
- F93-1 - fixed - Verified in the diff: read_record is pure; recover_dead_run acquires the admission lock, re-reads the current record, revalidates generation/pending/liveness, and propagates write errors. Status and resume listing use this recovery path. The regression and documentation cover the intended race, with a nonblocking synchronization weakness recorded separately.

## Verdict: ACCEPT

The scoped change closes F93-1 through locked recovery with current-record revalidation; the remaining regression-test synchronization weakness is nonblocking.

**WARNING: ACCEPT with 26 unchecked prior blocker(s) (F14-1, F22-1, F23-1, F23-2, F23-3, F24-1, F25-2, F30-1, F30-2, F31-1, F36-1, F38-1, F40-1, F42-1, F66-1, F76-1, F78-1, F78-2, F80-1, F82-1, F84-1, F89-1, F89-2, F89-3, F91-1, F91-2).**

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
- **F76-1** (prior, not-checked) `crates/c3-core/src/health.rs:1143` - The new read-time age limit discards identified cap-pair evidence after approximately 24 hours even when a valid reset_hint declares a later weekly or monthly reset. plan_cap_verdict then returns the underlying verdict without enforcing that reset. This makes an exhausted endpoint eligible before the retry time previously displayed and contradicts the documented policy that hinted entries remain unavailable until their reset.
- **F78-1** (prior, not-checked) `crates/c3-core/src/plan.rs:368` - The cap detector considers only pairs containing the newest qualifying hit. A later isolated instant 429 therefore suppresses an already established cap pair, allowing an exhausted endpoint to become eligible before its declared reset without any successful reply.
- **F78-2** (prior, not-checked) `crates/c3-core/src/health.rs:100` - The 35-day cap-pair retention applies only to rows supplied to endpoint_health; machine-health writes still prune old rows unless their stored outage or quota window remains active. Consequently, cap evidence originating in another repository can disappear after roughly 24 hours, releasing a hinted exhausted endpoint before its declared reset.
- **F80-1** (prior, not-checked) `crates/c3-core/src/health.rs:2177` - The new retention policy preserves cap-failure evidence longer and with higher eviction priority than the successful reply that clears it. Once that success disappears, machine-only readers reconstruct the old pair and falsely mark a usable endpoint unavailable until its declared reset.
- **F82-1** (prior, not-checked) `crates/c3-core/src/health.rs:2288` - The supersede drop does not prevent an obsolete cap refusal when older evidence remains in a skipped journal until after the clearing success is evicted. Journal replay protection covers records actually read, not unread records in a busy journal, and no durable success watermark remains to reject their later application.
- **F84-1** (prior, not-checked) `crates/c3-core/src/health.rs:2231` - The clearing-anchor protection is not durable under tied-success cap pressure. is_anchor protects every success row sharing an endpoint's newest timestamp, rather than one row. Those duplicates can force eviction of another endpoint's clearing anchor. A later success makes the duplicates unprotected, allowing delayed older evidence for the evicted endpoint to survive and restore a cleared cap refusal.
- **F89-1** (prior, not-checked) `crates/c3/src/work/mod.rs:730` - A foreground worker is treated as dead after 120 seconds because its running record never receives an owning PID. Reading status or admitting another run can then rewrite the still-active run as died and allow another worker on the same task.
- **F89-2** (prior, not-checked) `crates/c3/src/work/mod.rs:698` - Resume admission is not exclusive: it neither checks the task's other live runs nor atomically claims the selected run and marks it running before spawning. Two resume commands can execute the same session concurrently and overwrite the same temporary event files; a fresh worker can also start while a foreground resume still has status quota.
- **F89-3** (prior, not-checked) `crates/c3/src/work/mod.rs:676` - Artifact failures can silently destroy a resumed report while C3 records done and returns success. append_resume ignores open/write errors, after which resume_once deletes the attempt files; refresh_artifacts also ignores report.md and diff.txt write failures.
- **F91-1** (prior, not-checked) `crates/c3/src/work/mod.rs:747` - Resume admission checks status and exhaustion before acquiring the admission lock, but the locked claim only rejects a live owner. A delayed resume can therefore restart an already completed or exhausted run, and can reuse an attempt number calculated from stale state.
- **F91-2** (prior, not-checked) `crates/c3/src/work/record.rs:137` - The new admission lock does not enforce one live worker when recovery records cannot be inspected: record reads, directory enumeration failures, entry errors, and parse errors are treated as absent runs.
- **F93-1** (prior, fixed) `crates/c3/src/work/record.rs:140` - The status/wait reader performs an unlocked recovery write that can replace a newly claimed live resume with an old died record. A later admission then sees no pending worker and can launch another worker on the same task. Admission's strict locked reads do not protect against this external writer.

### Unproven scenarios

- The reported 883-test workspace result, 22 work integration tests, 25 work library tests, clippy result, and fails-before experiment were not independently executed or checked against raw logs.
- The regression does not prove barrier entry before the competing claim under slow process scheduling.
- Recovery write-failure propagation was verified in code, but no injected runtime write failure was exercised.
- All seven scoped non-.collab diffs were read; unchanged code was inspected selectively. The complete external-writer audit and historical findings were not independently revalidated.
- No live-provider worker run or release publication was performed.

### First-run checklist (observable)

- [ ] Confirm the running binary was built from accepted commit 8098c05; retain the build identity with the run output.
- [ ] While a resumed worker is alive, observe run.json retaining its new PID, PID start time and incremented attempt after status and wait queries; it must remain running rather than become died.
- [ ] A concurrent same-task start must exit nonzero with 'already has a live work run', produce no second run directory, and launch no second child.
- [ ] For a genuinely dead owner, status must persist died; a failed recovery write must surface an inspection/recovery error rather than silently report successful recovery.
- [ ] Before trusting a worker exit code 0, observe affirmative successful terminal evidence in events.jsonl, status done in run.json, and readable report.md and diff.txt consistent with the run.
