# Handoff 97 - Codex: codex-wave11-update-review

Date: 2026-10-22 12:30 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - position 1 of 13.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 116f2058-c317-4f26-95a8-e3f984b6583d.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-d03fb49b866548dab40c1b6dc05c7f28.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a16326-74f6-7970-8b6d-0157921f2566` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/96-claude-wave11-update-diff-review.md` (sha256 be6e95032eed). Reviewed: 3e8763e, base 3e8763eb9fc0e4b78e0095a3e17eacb6ff3a33a4, tree sha256 039d8d7a9fad, 0 changed files.
Bridge outcome: usable reply. Wall time: 224.5 s. Tokens: in 321110 (cached 248960), out 6467, reasoning 950.
Quota (rollout): 5h 70% until 12:59; wk 38% until Oct 27; this run 321110 input tokens (248960 cached) in 6 calls, max 71243 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - Checkout synchronization does not bind the target to the checked release or revalidate checkout state after installation, and failed automatic checks bypass the advertised daily limit. Findings: 0 blocker, 2 major, 1 minor, 0 note (F97-1..F97-3, tracked in `findings.json`). Structured reply: `handoffs/97-codex-codex-wave11-update-review.reply.json`.
Raw event stream: `handoffs/97-codex-codex-wave11-update-review.events.jsonl`.
Verbatim reply follows.

---

1. **Q1:** Explicit unsigned HTTPS installation is acceptable with the disclosed trust model. SHA pinning provides consistency, not independent authenticity; `cargo install --git … --rev <validated-sha>` avoids needing access to Cargo’s checkout. However, checkout synchronization can update a different branch after installation and merge an unrelated local tag: findings #1–2 block acceptance.

2. **Q2:** Stable installed versions ahead of the latest tag are protected against downgrade; prerelease tags are ignored. Deleted tags can remain advertised indefinitely during outages. Failed checks repeat on every hook invocation, contradicting the daily limit. Default-on with a three-second network timeout is reasonable only with failure backoff and honest stale-cache messaging.

3. **Q3:** **HOLD** for findings #1–2; also fix #3. Tests were inspected, not executed. Reading omissions are listed in `unproven`. F95-1 remains open.

## Requested checks

- **RC1:** Repository root; workspace-write; pause a mocked Cargo install, switch branches and dirty the checkout, then resume apply. Require checkout skip. Budget: 10 minutes.
- **RC2:** Repository root; workspace-write; apply against a scratch fork with a conflicting release-tag SHA. Require rejection before merge. Budget: 10 minutes.
- **RC3:** Repository root; workspace-write; invoke two hooks with an expired cache and counting failing fetcher. Require backoff and no unqualified stale offer. Budget: 5 minutes.

---

### Findings

- **F97-1** [major] `crates/c3/src/update/mod.rs:764`, `crates/c3/src/update/mod.rs:513`, `crates/c3/src/update/mod.rs:650` - Checkout safety is checked before the potentially lengthy Cargo installation and never rechecked before synchronization. The plan discards the inspected branch identity, so apply can fast-forward a different branch, a detached HEAD, or a checkout that has become dirty. Trigger: The checkout is initially clean on main. While cargo install runs, the operator switches to another branch whose HEAD is an ancestor of the release, detaches HEAD, or introduces nonconflicting edits. GitSync subsequently fetches and merges without reinspection. Evidence: read-code: run_apply resolves checkout state before executing the plan; CargoInstall precedes GitSync, whose stored fields contain only path and tag.; read-code: GitSync performs fetch and merge directly, without checking cleanliness, symbolic HEAD, or whether HEAD changed since planning.; read-code: Tests cover static checkout inspection and isolated fast-forward behavior, not changes between inspection and execution. Verify: Run RC1, including a branch switch to another ancestor of the release, and assert that neither that branch nor its working tree is updated. Remedy: Carry expected branch and HEAD into the plan, then revalidate repository identity, branch, HEAD, and cleanliness immediately before merge after fetching; skip synchronization when any precondition changed.
- **F97-2** [major] `crates/c3/src/update/mod.rs:419`, `crates/c3/src/update/mod.rs:575`, `crates/c3/src/update/mod.rs:652`, `crates/c3/src/update/mod.rs:764` - Source synchronization is not bound to the release identified by the official tags API. It fetches an unchecked origin and merges a local tag by name without comparing its commit to latest_sha, so a fork or stale local tag can update the checkout to different code while reporting the advertised release. Trigger: A configured clean checkout points origin at a fork whose v<latest> tag names a different descendant commit from the API's latest_sha. Alternatively, origin lacks that tag while an unrelated local tag with that name already exists. Fetch succeeds and the local tag fast-forwards HEAD. Evidence: read-code: Explicit checkout paths are accepted based on Git work-tree, cleanliness, and branch checks; origin and release identity are not validated.; read-code: Apply passes only the tag name into the plan, fetches origin, and merges that name. latest_sha is not part of execution. Verify: Run RC2 with two distinct descendant commits sharing the release tag spelling in the official fixture and scratch fork; verify that the checkout cannot move to the fork's commit. Remedy: Fetch the selected release from the canonical repository into a dedicated ref, validate its peeled commit against the API SHA, and merge the verified commit. Pin Cargo installation to that same validated commit for consistent installation and checkout contents.
- **F97-3** [minor] `crates/c3/src/update/mod.rs:328`, `crates/c3/src/update/mod.rs:341`, `docs/port/wave11-update.md:32` - Failed automatic checks neither record an attempt time nor suppress stale offers. Consequently, an outage causes a network attempt on every hook invocation and can keep advertising a deleted release indefinitely, contrary to the documented daily-attempt and silent-on-error behavior. Trigger: The cache is absent or older than 24 hours and the fetch fails repeatedly. If the expired cache contains a newer release, that release continues to be advertised without a stale qualifier. Evidence: read-code: Only successful fetches write the cache. The error branch reconstructs status from the old cache and then prints an update offer whenever its version is newer.; read-code: Documentation promises attempts at most once per 24 hours and no hook line on errors.; read-code: The offline hook test uses a fresh home; it does not exercise an expired newer-version cache or repeated failures. Verify: Run RC3 with an expired newer-version cache and two failing fetch attempts scheduled within one day; observe request count and emitted hook lines. Remedy: Persist last-attempt/backoff state separately from the last successful response, preserving useful cached data while throttling failures. Suppress expired offers on fetch failure or explicitly label them stale and document that policy.

### Prior findings

- F02-1 - not-checked - Outside this update review.
- F02-3 - not-checked - Outside this update review.
- F02-4 - not-checked - Outside this update review.
- F02-5 - not-checked - Outside this update review.
- F02-6 - not-checked - Outside this update review.
- F02-7 - not-checked - Outside this update review.
- F02-8 - not-checked - Outside this update review.
- F04-1 - not-checked - Outside this update review.
- F04-2 - not-checked - Outside this update review.
- F04-3 - not-checked - Outside this update review.
- F06-1 - not-checked - Outside this update review.
- F09-1 - not-checked - Outside this update review.
- F09-2 - not-checked - Outside this update review.
- F09-3 - not-checked - Outside this update review.
- F09-4 - not-checked - Outside this update review.
- F09-5 - not-checked - Outside this update review.
- F09-6 - not-checked - Outside this update review.
- F11-1 - not-checked - Outside this update review.
- F11-2 - not-checked - Outside this update review.
- F11-3 - not-checked - Outside this update review.
- F11-4 - not-checked - Outside this update review.
- F11-5 - not-checked - Outside this update review.
- F14-1 - not-checked - Outside this update review.
- F19-1 - not-checked - Outside this update review.
- F19-2 - not-checked - Outside this update review.
- F19-3 - not-checked - Outside this update review.
- F22-1 - not-checked - Outside this update review.
- F23-1 - not-checked - Outside this update review.
- F23-2 - not-checked - Outside this update review.
- F23-3 - not-checked - Outside this update review.
- F23-4 - not-checked - Outside this update review.
- F23-5 - not-checked - Outside this update review.
- F24-1 - not-checked - Outside this update review.
- F24-2 - not-checked - Outside this update review.
- F24-3 - not-checked - Outside this update review.
- F24-4 - not-checked - Outside this update review.
- F24-5 - not-checked - Outside this update review.
- F24-6 - not-checked - Outside this update review.
- F25-2 - not-checked - Outside this update review.
- F25-3 - not-checked - Outside this update review.
- F25-4 - not-checked - Outside this update review.
- F30-1 - not-checked - Outside this update review.
- F30-2 - not-checked - Outside this update review.
- F30-3 - not-checked - Outside this update review.
- F31-1 - not-checked - Outside this update review.
- F36-1 - not-checked - Outside this update review.
- F36-2 - not-checked - Outside this update review.
- F38-1 - not-checked - Outside this update review.
- F40-1 - not-checked - Outside this update review.
- F40-2 - not-checked - Outside this update review.
- F42-1 - not-checked - Outside this update review.
- F46-1 - not-checked - Outside this update review.
- F48-1 - not-checked - Outside this update review.
- F50-1 - not-checked - Outside this update review.
- F52-1 - not-checked - Outside this update review.
- F52-2 - not-checked - Outside this update review.
- F55-1 - not-checked - Outside this update review.
- F55-2 - not-checked - Outside this update review.
- F58-1 - not-checked - Outside this update review.
- F60-1 - not-checked - Outside this update review.
- F62-1 - not-checked - Outside this update review.
- F66-1 - not-checked - Outside this update review.
- F66-2 - not-checked - Outside this update review.
- F66-3 - not-checked - Outside this update review.
- F66-4 - not-checked - Outside this update review.
- F66-5 - not-checked - Outside this update review.
- F66-6 - not-checked - Outside this update review.
- F68-1 - not-checked - Outside this update review.
- F68-2 - not-checked - Outside this update review.
- F70-1 - not-checked - Outside this update review.
- F72-1 - not-checked - Outside this update review.
- F72-2 - not-checked - Outside this update review.
- F74-1 - not-checked - Outside this update review.
- F74-2 - not-checked - Outside this update review.
- F76-1 - not-checked - Outside this update review.
- F78-1 - not-checked - Outside this update review.
- F78-2 - not-checked - Outside this update review.
- F80-1 - not-checked - Outside this update review.
- F82-1 - not-checked - Outside this update review.
- F84-1 - not-checked - Outside this update review.
- F89-1 - not-checked - Outside this update review.
- F89-2 - not-checked - Outside this update review.
- F89-3 - not-checked - Outside this update review.
- F89-4 - not-checked - Outside this update review.
- F89-5 - not-checked - Outside this update review.
- F89-6 - not-checked - Outside this update review.
- F89-7 - not-checked - Outside this update review.
- F89-8 - not-checked - Outside this update review.
- F91-1 - not-checked - Outside this update review.
- F91-2 - not-checked - Outside this update review.
- F91-3 - not-checked - Outside this update review.
- F91-4 - not-checked - Outside this update review.
- F93-1 - fixed - Static inspection of record.rs:185-215 shows recovery acquires the admission lock, rereads the current record, and checks generation, pending state, and liveness before writing. Runtime race proof remains limited by F95-1.
- F95-1 - still-open - work.rs:1850 still waits a fixed 1500 ms; record.rs:168-179 provides no barrier-entry acknowledgement. The regression can still miss the intended stale snapshot schedule.

## Verdict: HOLD

Checkout synchronization does not bind the target to the checked release or revalidate checkout state after installation, and failed automatic checks bypass the advertised daily limit.

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

- No builds, tests, network requests, or apply operations were executed; the brief's reported passing checks were not independently reproduced.
- Within the five-call reading budget, omitted wiring diffs in crates/c3-cli/src/main.rs, crates/c3/src/lib.rs, and crates/c3/src/cli/mod.rs; omitted compat_wave2b.rs and the formatting-only work diffs apart from targeted recovery excerpts.
- Did not read the final update-module fetch/cache unit tests around lines 1048-1117 or the small TestFetcher implementation gap; read the relevant production cache and runner paths.
- No successful end-to-end apply test was observed. Inspected CLI tests exercise checking and refusal; unit tests exercise plan values, static checkout inspection, and isolated Git merges.
- Concurrent cache writers, production timeout behavior, tag-list completeness, Windows executable replacement, and actual Claude plugin launcher behavior remain unverified.
- Historical findings other than F93-1 and F95-1 were not re-audited.

### First-run checklist (observable)

_(none)_
