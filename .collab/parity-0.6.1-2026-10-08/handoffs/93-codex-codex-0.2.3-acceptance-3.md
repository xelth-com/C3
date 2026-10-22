# Handoff 93 - Codex: codex-0.2.3-acceptance-3

Date: 2026-10-22 10:03 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - position 1 of 13.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: fc8cb0dc-2ef2-44d9-89ab-822eb3a2b86f.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-36ba379fe2e34aad9e771ecf45f39ca1.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a1629e-d00e-7692-b09e-6181885633a9` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/92-claude-0.2.3-acceptance-3.md` (sha256 74ebcc0b8a8f). Reviewed: 2ceeead, base 2ceeeadad57fca857d1bfa497b02c9f48ec2e20c, tree sha256 6d8163cfe721, 0 changed files.
Bridge outcome: usable reply. Wall time: 244.5 s. Tokens: in 329109 (cached 275584), out 6240, reasoning 880.
Quota (rollout): 5h 47% until 12:59; wk 34% until Oct 27; this run 329109 input tokens (275584 cached) in 6 calls, max 75093 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - Unlocked recovery writes can overwrite a live resume claim and permit another worker on the same task. Findings: 1 blocker, 0 major, 0 minor, 0 note (F93-1, tracked in `findings.json`). Structured reply: `handoffs/93-codex-codex-0.2.3-acceptance-3.reply.json`.
Raw event stream: `handoffs/93-codex-codex-0.2.3-acceptance-3.events.jsonl`.
Verbatim reply follows.

---

**Q1.** The specific F91-1 and F91-2 triggers are fixed in the inspected code: resume claims reread eligibility and generation under the lock, and admission propagates inspection errors. The broader serialization criterion remains unmet. F91-3 and F91-4 also appear fixed; tests were inspected, not executed.

**Q2. Blocker — finding #1.** Status does not merely display stale information: `read_record` writes it back. A delayed status read can overwrite a newly claimed live resume with `died`; subsequent admission trusts that overwritten disk record and launches another worker. Make reads pure, and perform recovery transitions under the admission lock with a fresh read and liveness check.

**Q3. HOLD.** Do not move/publish the acceptance tag yet. HEAD is `2ceeead`. Review was limited to selected hunks and relevant ranges; omitted code and runtime scenarios are listed in `unproven`. Reported suite results were not independently reproduced.

## Requested checks

- **RC1:** Working directory `C:\Users\Dmytro\C3`; workspace-write; add/run a barrier fixture for finding #1: capture running A in status, finish A as quota and exit, claim live resume B, then release status. Require B’s record unchanged and a fresh start refused without a launch marker. Budget: one deterministic integration fixture, 15 minutes.

---

### Findings

- **F93-1** [blocker] `crates/c3/src/work/record.rs:140`, `crates/c3/src/work/record.rs:157`, `crates/c3/src/work/detach.rs:310`, `crates/c3/src/work/mod.rs:830`, `crates/c3/src/work/mod.rs:1047`, `docs/port/wave10a-work-acceptance.md:131` - The status/wait reader performs an unlocked recovery write that can replace a newly claimed live resume with an old died record. A later admission then sees no pending worker and can launch another worker on the same task. Admission's strict locked reads do not protect against this external writer. Trigger: Status reads run A while its record is running and pauses before checking liveness. A finishes as quota and its owning process exits. Status determines that its captured owner is dead and pauses before write_record. Resume B claims the quota record under the admission lock, writes its new running owner and launches. Status resumes and atomically replaces B's record with its stale A snapshot marked died. A fresh same-task start then passes live_run while B remains alive. Evidence: read-code: read_record reads and parses a snapshot, checks that snapshot's owner, then writes the modified snapshot without acquiring admission_lock or rereading; the write result is ignored.; read-code: The status/wait listing calls the mutating read_record without an admission lock, despite describing itself as read-only.; read-code: claim_resume writes the new running owner under the lock, while live_run later relies on the persisted status and only treats pending states as active.; inferred: An atomic replacement prevents partial JSON but does not prevent a stale writer from replacing a newer claim; the resulting died status hides the live resumed worker. Verify: Run RC1 with barriers after the status snapshot read and before its recovery write; verify that a claimed live resume survives the delayed status operation and that a fresh same-task launch refuses before spawning. Remedy: Make read_record a pure fallible reader. If recovery must persist died, acquire the task admission lock, reread the current record, revalidate its owner/generation and liveness, and propagate write failures before completing the transition. Update the status-path documentation and add the interleaving regression.

### Prior findings

- F02-1 - not-checked - Outside the bounded work-mode review.
- F02-3 - not-checked - Outside the bounded work-mode review.
- F02-4 - not-checked - Outside the bounded work-mode review.
- F02-5 - not-checked - Outside the bounded work-mode review.
- F02-6 - not-checked - Outside the bounded work-mode review.
- F02-7 - not-checked - Outside the bounded work-mode review.
- F02-8 - not-checked - Outside the bounded work-mode review.
- F04-1 - not-checked - Outside the bounded work-mode review.
- F04-2 - not-checked - Outside the bounded work-mode review.
- F04-3 - not-checked - Outside the bounded work-mode review.
- F06-1 - not-checked - Outside the bounded work-mode review.
- F09-1 - not-checked - Outside the bounded work-mode review.
- F09-2 - not-checked - Outside the bounded work-mode review.
- F09-3 - not-checked - Outside the bounded work-mode review.
- F09-4 - not-checked - Outside the bounded work-mode review.
- F09-5 - not-checked - Outside the bounded work-mode review.
- F09-6 - not-checked - Outside the bounded work-mode review.
- F11-1 - not-checked - Outside the bounded work-mode review.
- F11-2 - not-checked - Outside the bounded work-mode review.
- F11-3 - not-checked - Outside the bounded work-mode review.
- F11-4 - not-checked - Outside the bounded work-mode review.
- F11-5 - not-checked - Outside the bounded work-mode review.
- F14-1 - not-checked - Outside the bounded work-mode review.
- F19-1 - not-checked - Outside the bounded work-mode review.
- F19-2 - not-checked - Outside the bounded work-mode review.
- F19-3 - not-checked - Outside the bounded work-mode review.
- F22-1 - not-checked - Outside the bounded work-mode review.
- F23-1 - not-checked - Outside the bounded work-mode review.
- F23-2 - not-checked - Outside the bounded work-mode review.
- F23-3 - not-checked - Outside the bounded work-mode review.
- F23-4 - not-checked - Outside the bounded work-mode review.
- F23-5 - not-checked - Outside the bounded work-mode review.
- F24-1 - not-checked - Outside the bounded work-mode review.
- F24-2 - not-checked - Outside the bounded work-mode review.
- F24-3 - not-checked - Outside the bounded work-mode review.
- F24-4 - not-checked - Outside the bounded work-mode review.
- F24-5 - not-checked - Outside the bounded work-mode review.
- F24-6 - not-checked - Outside the bounded work-mode review.
- F25-2 - not-checked - Outside the bounded work-mode review.
- F25-3 - not-checked - Outside the bounded work-mode review.
- F25-4 - not-checked - Outside the bounded work-mode review.
- F30-1 - not-checked - Outside the bounded work-mode review.
- F30-2 - not-checked - Outside the bounded work-mode review.
- F30-3 - not-checked - Outside the bounded work-mode review.
- F31-1 - not-checked - Outside the bounded work-mode review.
- F36-1 - not-checked - Outside the bounded work-mode review.
- F36-2 - not-checked - Outside the bounded work-mode review.
- F38-1 - not-checked - Outside the bounded work-mode review.
- F40-1 - not-checked - Outside the bounded work-mode review.
- F40-2 - not-checked - Outside the bounded work-mode review.
- F42-1 - not-checked - Outside the bounded work-mode review.
- F46-1 - not-checked - Outside the bounded work-mode review.
- F48-1 - not-checked - Outside the bounded work-mode review.
- F50-1 - not-checked - Outside the bounded work-mode review.
- F52-1 - not-checked - Outside the bounded work-mode review.
- F52-2 - not-checked - Outside the bounded work-mode review.
- F55-1 - not-checked - Outside the bounded work-mode review.
- F55-2 - not-checked - Outside the bounded work-mode review.
- F58-1 - not-checked - Outside the bounded work-mode review.
- F60-1 - not-checked - Outside the bounded work-mode review.
- F62-1 - not-checked - Outside the bounded work-mode review.
- F66-1 - not-checked - Outside the bounded work-mode review.
- F66-2 - not-checked - Outside the bounded work-mode review.
- F66-3 - not-checked - Outside the bounded work-mode review.
- F66-4 - not-checked - Outside the bounded work-mode review.
- F66-5 - not-checked - Outside the bounded work-mode review.
- F66-6 - not-checked - Outside the bounded work-mode review.
- F68-1 - not-checked - Outside the bounded work-mode review.
- F68-2 - not-checked - Outside the bounded work-mode review.
- F70-1 - not-checked - Outside the bounded work-mode review.
- F72-1 - not-checked - Outside the bounded work-mode review.
- F72-2 - not-checked - Outside the bounded work-mode review.
- F74-1 - not-checked - Outside the bounded work-mode review.
- F74-2 - not-checked - Outside the bounded work-mode review.
- F76-1 - not-checked - Outside the bounded work-mode review.
- F78-1 - not-checked - Outside the bounded work-mode review.
- F78-2 - not-checked - Outside the bounded work-mode review.
- F80-1 - not-checked - Outside the bounded work-mode review.
- F82-1 - not-checked - Outside the bounded work-mode review.
- F84-1 - not-checked - Outside the bounded work-mode review.
- F89-1 - not-checked - Owner/liveness helpers were read, but the original foreground lifetime trigger was not rechecked.
- F89-2 - not-checked - This review checked the later F91 admission changes, not the full original concurrent-resume and fresh-start fixture.
- F89-3 - not-checked - Durable append and error propagation were inspected; the original write-failure integration trigger was not independently verified.
- F89-4 - not-checked - The new grammar rejects the named bare-shell/interpreter examples; the complete Claude/Codex/Muse permission contract and runtime boundary were not revalidated.
- F89-5 - not-checked - Report/diff/argv sanitizer changes and explicit raw-stream documentation were inspected; all original usage-field and secret-persistence paths were not revalidated.
- F89-6 - not-checked - Persisted cap/window fields were observed, but plain-resume behavior was not independently verified.
- F89-7 - not-checked - Baseline-hash fields were observed; pre-dirty-file attribution was not rechecked.
- F89-8 - not-checked - The narrowed F91-4 terminal triggers were checked; the complete earlier error/quota/empty-stream matrix was not revalidated.
- F91-1 - fixed - For the stated stale-claim trigger, claim_resume now checks status, exhaustion and expected resume generation under the lock before deriving and writing the attempt. Static closure only; the separate unlocked recovery-write race is finding #1.
- F91-2 - fixed - Inspected admission paths propagate read, parse, metadata, enumeration and entry failures; confirmed NotFound remains absence. Runtime fault injection was not performed.
- F91-3 - fixed - Attempt reads now propagate failures and preserve temporary files; aggregate reads precede report replacement and propagate errors to artifact_error. Static inspection only.
- F91-4 - fixed - Claude now requires is_error=false; Codex requires turn.completed. The two incomplete-stream integration fixtures assert died/nonzero; not executed here.

## Verdict: HOLD

Unlocked recovery writes can overwrite a live resume claim and permit another worker on the same task.

### Blockers

- **F93-1** `crates/c3/src/work/record.rs:140`, `crates/c3/src/work/record.rs:157`, `crates/c3/src/work/detach.rs:310`, `crates/c3/src/work/mod.rs:830`, `crates/c3/src/work/mod.rs:1047`, `docs/port/wave10a-work-acceptance.md:131` - The status/wait reader performs an unlocked recovery write that can replace a newly claimed live resume with an old died record. A later admission then sees no pending worker and can launch another worker on the same task. Admission's strict locked reads do not protect against this external writer. Verify: Run RC1 with barriers after the status snapshot read and before its recovery write; verify that a claimed live resume survives the delayed status operation and that a fresh same-task launch refuses before spawning. Remedy: Make read_record a pure fallible reader. If recovery must persist died, acquire the task admission lock, reread the current record, revalidate its owner/generation and liveness, and propagate write failures before completing the transition. Update the status-path documentation and add the interleaving regression.
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
- **F91-1** (prior, fixed) `crates/c3/src/work/mod.rs:747` - Resume admission checks status and exhaustion before acquiring the admission lock, but the locked claim only rejects a live owner. A delayed resume can therefore restart an already completed or exhausted run, and can reuse an attempt number calculated from stale state.
- **F91-2** (prior, fixed) `crates/c3/src/work/record.rs:137` - The new admission lock does not enforce one live worker when recovery records cannot be inspected: record reads, directory enumeration failures, entry errors, and parse errors are treated as absent runs.

### Unproven scenarios

- No builds, tests, runtime launches or filesystem fault injections were run; the brief's 882-test, integration, library and clippy results remain reported evidence.
- Finding #1's status/completion/resume interleaving is supported by code tracing, not an executed barrier reproduction.
- The inspected stale-resume integration fixture covers completion as done, uses a fixed 1.5-second sleep without a barrier-arrival acknowledgement, and does not contain the separately claimed exhausted outcome.
- Admission metadata, enumeration, entry and parse failures were checked statically; the integration fixture replacing run.json with a directory does not exercise every error class.
- Successful detached-alarm resumption and cross-platform real-engine behavior were not executed.
- Reading was incomplete: selected portions of mod.rs and record.rs, capped work.rs hunks and selected test ranges were read; remaining hunks/context, complete engine permission construction, the redactor implementation, other work-mode call paths and all earlier-wave implementation files were not audited. One batched output was truncated; decisive recovery-reader code was subsequently read explicitly.
- F89-4 and F89-5 were only partially checked: the named grammar and sanitizer changes do not constitute independent validation of every permission or secret-persistence path.

### First-run checklist (observable)

- [ ] Observe run.json marked running with the actual owning PID/start identity and expected attempt before the child-launch marker; while the worker is alive, status must retain that owner and a competing same-task start/resume must refuse without another launch marker.
- [ ] For a resumed run, observe exactly one new attempt number and resume separator, and verify the retained events contain the complete attempt before its temporary event file disappears.
- [ ] Before believing exit 0/done, inspect an affirmative engine terminal: Claude result with is_error=false and success/absent subtype, Codex turn.completed, or Muse's explicit completion; ensure no rejecting quota/error outcome contradicts it.
- [ ] Open report.md and diff.txt and compare them with the successful attempt and actual repository changes; run.json must name the same run/session and must not contain artifact_error.
- [ ] On the first quota/resume cycle, observe the persisted resume count and configured cap, refusal after exhaustion, and no extra child-launch marker from a stale claimant.
- [ ] In the controlled first-run validation, observe a read-denied artifact produce nonzero/artifact_error with recoverable source bytes and the previous report preserved; observe a known sanitizer sentinel redacted from the report while raw events remain explicitly identified as raw.
