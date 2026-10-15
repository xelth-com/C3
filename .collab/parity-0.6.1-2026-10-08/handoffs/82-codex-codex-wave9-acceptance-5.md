# Handoff 82 - Codex: codex-wave9-acceptance-5

Date: 2026-10-15 11:13 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - entry 1 of 10 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 315a233b-7abf-4caf-b714-c62a9e543ee1.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-93fabf364d24448cb99898977d442d8e.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13ed2-4f7b-7551-99ce-7af49a1430ba` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/81-claude-wave9-acceptance-5.md` (sha256 21567f2d7d4f). Reviewed: c56e656, base c56e656904535e07c86aa7cb5b8ee2b0d8c65af7, tree sha256 34c845dbb866, 0 changed files.
Bridge outcome: usable reply. Wall time: 305.9 s. Tokens: in 385891 (cached 320768), out 5584, reasoning 965.
Quota (rollout): 5h 49% until 14:18; wk 8% until Oct 22; this run 385891 input tokens (320768 cached) in 8 calls, max 63969 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s. Range: `51dfc13..2bbd27e` - the range changes 10 files, 2190 lines (2189 insertions, 1 deletions).
Verdict: HOLD - The supersede drop handles evidence already merged with a success, but a skipped journal can introduce older cap evidence after that success has been evicted. Findings: 1 blocker, 0 major, 0 minor, 0 note (F82-1, tracked in `findings.json`). Structured reply: `handoffs/82-codex-codex-wave9-acceptance-5.reply.json`.
Raw event stream: `handoffs/82-codex-codex-wave9-acceptance-5.events.jsonl`.
Verbatim reply follows.

---

1. **F80-1 is only partially closed.** Equal timestamps and a success merged alongside the pair are handled correctly. Applied journal records receive replay protection. However, an unread busy journal receives no marks: the success can be evicted before its older pair is subsequently applied, restoring the obsolete refusal (finding #1).

2. **HOLD** for finding #1. Workspace and harness reruns also remain release gates. No tests ran. Read: source/document diffs and targeted health context. Skipped: other `.collab` changes and earlier-wave implementations. Context reads used working HEAD `c56e656`, ahead of `2bbd27e`; exact-target reproduction remains required.

## Requested checks

- **RC1:** Repository root, workspace-write, 30 minutes: at `2bbd27e`, seed 500 evidence rows and a journal-only older pair; hold the journal through a success update, release, replay. Require zero old hits and availability.
- **RC2:** Repository root, workspace-write, 60 minutes: run `cargo test -j 2 --no-fail-fast --workspace`, then pinned `harness-roster`; require zero failures.

---

### Findings

- **F82-1** [blocker] `crates/c3-core/src/health.rs:2288`, `crates/c3-core/src/health.rs:2390`, `crates/c3-core/src/health.rs:2456`, `crates/c3-core/src/health.rs:2476`, `docs/port/wave9-consilium-defects.md:207` - The supersede drop does not prevent an obsolete cap refusal when older evidence remains in a skipped journal until after the clearing success is evicted. Journal replay protection covers records actually read, not unread records in a busy journal, and no durable success watermark remains to reject their later application. Trigger: The machine file contains 500 fresh cap-evidence rows for other endpoints. Its journal contains two distinct identified instant-429 records for the target endpoint, followed in event time by a successful consultation. Hold the journal exclusively beyond the update's open retries while adding that success. The update proceeds without reading the pair, and the record cap removes the non-evidence success. Release the journal and apply it: the older pair is appended, survives the cap, and can restore the monthly refusal despite the intervening success. Evidence: read-code: The added supersede block derives its newest-success map solely from the currently merged endpoints and drops matching older evidence before retention and capping. It persists no clearing watermark.; read-code: Working-tree context permits the update to continue when all journal opens fail; cap_machine_endpoints removes non-evidence rows first; applying marks are written only when a journal handle was acquired.; read-code: The new regression seeds the pair and success together in the health file. It does not exercise evidence arriving from a journal skipped during the clearing update.; inferred: Once the success has been evicted, the later merge has neither a success to suppress the pair nor applied marks for those previously unread records. The appended pair survives removal of earlier evidence rows.; ran-command: Working HEAD was c56e656904535e07c86aa7cb5b8ee2b0d8c65af7, ahead of the requested target. The added logic was read from the exact range diff; unchanged surrounding context was read from the working tree and was not independently compared with the target. Verify: Run RC1 on 2bbd27e with the production exclusive journal lock. Assert the clearing update's result, the success's removal under the cap, and the machine-only hit count and cap verdict after releasing and applying the journal; the required result is zero old hits and availability. Remedy: Preserve clearing information durably until older evidence can no longer arrive. At minimum, prevent a successful health update from discarding its clearing success when an existing journal could not be inspected; also account for delayed direct retries or later journal appends. Add the skipped-journal regression before claiming durable suppression. Supersedes: F80-1.

### Prior findings

- F02-1 - not-checked - Outside this delta; not rechecked.
- F02-3 - not-checked - Outside this delta; not rechecked.
- F02-4 - not-checked - Outside this delta; not rechecked.
- F02-5 - not-checked - Outside this delta; not rechecked.
- F02-6 - not-checked - Outside this delta; not rechecked.
- F02-7 - not-checked - Outside this delta; not rechecked.
- F02-8 - not-checked - Benchmark assumption not reassessed.
- F04-1 - not-checked - Outside this delta; not rechecked.
- F04-2 - not-checked - Windows timezone behavior not exercised.
- F04-3 - not-checked - Outside this delta; not rechecked.
- F06-1 - not-checked - Historical timezone behavior not exercised.
- F09-1 - not-checked - Outside this delta; not rechecked.
- F09-2 - not-checked - Outside this delta; not rechecked.
- F09-3 - not-checked - Outside this delta; not rechecked.
- F09-4 - not-checked - Outside this delta; not rechecked.
- F09-5 - not-checked - Outside this delta; not rechecked.
- F09-6 - not-checked - Outside this delta; not rechecked.
- F11-1 - not-checked - Journal context read, but original fault scenarios not reassessed.
- F11-2 - not-checked - Outside this delta; not rechecked.
- F11-3 - not-checked - Outside this delta; not rechecked.
- F11-4 - not-checked - Outside this delta; not rechecked.
- F11-5 - not-checked - Outside this delta; not rechecked.
- F14-1 - not-checked - Outside this delta; not rechecked.
- F19-1 - not-checked - Outside this delta; not rechecked.
- F19-2 - not-checked - Outside this delta; not rechecked.
- F19-3 - not-checked - Outside this delta; not rechecked.
- F22-1 - not-checked - Applied-mark implementation and cap regression read as context; original full scenario not rechecked.
- F23-1 - not-checked - Outside this delta; not rechecked.
- F23-2 - not-checked - Outside this delta; not rechecked.
- F23-3 - not-checked - Outside this delta; not rechecked.
- F23-4 - not-checked - Outside this delta; not rechecked.
- F23-5 - not-checked - Production Samoa behavior not exercised.
- F24-1 - not-checked - Outside this delta; not rechecked.
- F24-2 - not-checked - Outside this delta; not rechecked.
- F24-3 - not-checked - Outside this delta; not rechecked.
- F24-4 - not-checked - Outside this delta; not rechecked.
- F24-5 - not-checked - Outside this delta; not rechecked.
- F24-6 - not-checked - Outside this delta; not rechecked.
- F25-2 - not-checked - Outside this delta; not rechecked.
- F25-3 - not-checked - Outside this delta; not rechecked.
- F25-4 - not-checked - Outside this delta; not rechecked.
- F30-1 - not-checked - Outside this delta; not rechecked.
- F30-2 - not-checked - Outside this delta; not rechecked.
- F30-3 - not-checked - Outside this delta; not rechecked.
- F31-1 - not-checked - Outside this delta; not rechecked.
- F36-1 - not-checked - Outside this delta; not rechecked.
- F36-2 - not-checked - Outside this delta; not rechecked.
- F38-1 - not-checked - Outside this delta; not rechecked.
- F40-1 - not-checked - Outside this delta; not rechecked.
- F40-2 - not-checked - Outside this delta; not rechecked.
- F42-1 - not-checked - Outside this delta; not rechecked.
- F46-1 - not-checked - Quota aggregation not rechecked.
- F48-1 - not-checked - Quota aggregation not rechecked.
- F50-1 - not-checked - Quota aggregation not rechecked.
- F52-1 - not-checked - Quota aggregation not rechecked.
- F52-2 - not-checked - Quota aggregation not rechecked.
- F55-1 - not-checked - Quota cache behavior not rechecked.
- F55-2 - not-checked - Quota aggregation not rechecked.
- F58-1 - not-checked - Quota aggregation not rechecked.
- F60-1 - not-checked - Quota persistence not rechecked.
- F62-1 - not-checked - Quota timestamp persistence not rechecked.
- F66-1 - not-checked - Outside this delta; not rechecked.
- F66-2 - not-checked - Outside this delta; not rechecked.
- F66-3 - not-checked - Identity filtering read as context; production mirror fixture not rechecked.
- F66-4 - not-checked - Outside this delta; not rechecked.
- F66-5 - not-checked - Outside this delta; not rechecked.
- F66-6 - not-checked - Documented inferred-exhaustion policy read; policy fixtures not rechecked.
- F68-1 - not-checked - Identity filtering read as context; complete earlier scenario not rechecked.
- F68-2 - not-checked - Portable recovery fixture not inspected.
- F70-1 - not-checked - Documented replacement of legacy matching read; original scenarios not rerun.
- F72-1 - not-checked - Read-time filter inspected as context; persisted legacy fixture not rerun.
- F72-2 - not-checked - Documented replacement of legacy matching read; original scenarios not rerun.
- F74-1 - not-checked - Documented consultation-level policy read; multi-attempt fixture not run.
- F74-2 - not-checked - Reset-hint validation and fallback not inspected.
- F76-1 - not-checked - Period-policy documentation read; plan implementation and controlled-clock fixture not rechecked.
- F78-1 - not-checked - Pair-selection documentation read; implementation not rechecked.
- F78-2 - not-checked - Retention and cap context inspected; exact-target production regression not run.
- F80-1 - still-open - Original co-resident pair/success ordering is addressed; finding #1 replaces it with the skipped-journal ordering that can still restore an obsolete refusal.

## Verdict: HOLD

The supersede drop handles evidence already merged with a success, but a skipped journal can introduce older cap evidence after that success has been evicted.

### Blockers

- **F82-1** `crates/c3-core/src/health.rs:2288`, `crates/c3-core/src/health.rs:2390`, `crates/c3-core/src/health.rs:2456`, `crates/c3-core/src/health.rs:2476`, `docs/port/wave9-consilium-defects.md:207` - The supersede drop does not prevent an obsolete cap refusal when older evidence remains in a skipped journal until after the clearing success is evicted. Journal replay protection covers records actually read, not unread records in a busy journal, and no durable success watermark remains to reject their later application. Verify: Run RC1 on 2bbd27e with the production exclusive journal lock. Assert the clearing update's result, the success's removal under the cap, and the machine-only hit count and cap verdict after releasing and applying the journal; the required result is zero old hits and availability. Remedy: Preserve clearing information durably until older evidence can no longer arrive. At minimum, prevent a successful health update from discarding its clearing success when an existing journal could not be inspected; also account for delayed direct retries or later journal appends. Add the skipped-journal regression before claiming durable suppression.
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
- **F80-1** (prior, still-open) `crates/c3-core/src/health.rs:2177` - The new retention policy preserves cap-failure evidence longer and with higher eviction priority than the successful reply that clears it. Once that success disappears, machine-only readers reconstruct the old pair and falsely mark a usable endpoint unavailable until its declared reset.

### Unproven scenarios

- The skipped-journal reproduction has not been executed, and surrounding working-tree context was not independently compared against 2bbd27e.
- The handoff reports passing library tests, clippy and formatting; those results were not independently reproduced.
- The full workspace suite and pinned plugin harness remain pending according to the handoff.
- Equal-time success/evidence and same-update journal merging were assessed by code inspection, not dedicated executed regressions.
- Delayed direct retries or journal appends after clearing information has disappeared remain untested.
- Other .collab changes, earlier handoffs, earlier-wave implementations, and source ranges outside the targeted health reads were not reviewed.

### First-run checklist (observable)

- [ ] Record the tested commit and executable revision; confirm the workspace suite and pinned harness report zero failures on the release candidate.
- [ ] For RC1, capture the clearing update result, health-file rows before and after capping, journal contents before replay, and machine-only verdict after replay. Require zero superseded hits and availability.
- [ ] With a pair and clearing success merged together, observe zero old qualifying hits after both age-based retention and record-cap pressure.
- [ ] With no clearing success, observe two qualifying distinct-consultation hits and an unavailable verdict carrying the declared reset.
- [ ] For a retained, already-applied journal, observe applying/applied marks and confirm repeated replay does not restore deleted evidence.
