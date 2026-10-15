# Handoff 74 - Claude (claude): wave9-round5-glm

Date: 2026-10-15 03:01 local. Author: Claude (claude) (model glm-5.3, effort high), claude-cli 2.1.300.
Reviewer: ZAI-claude :: glm-5.3 [claude] (provider from -Provider, model from roster; engine claude (C:\users\dmytro\.local\bin\claude.exe), endpoint https://api.z.ai/api/anthropic (token from env ZAI_API_KEY); provider fingerprint 944a94557723; harness claude-cli 2.1.300).
Preflight: ok: env ZAI_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 2 of 9 for -Provider ZAI-claude (engine, model applied).
Effort: high sent (requested high, mapping claude-v1, by caps-v1: engine:claude, any model; not confirmed by the provider). Consultation id: 064b9e27-e491-4a80-9e32-4244b1e71865.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `claude -p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model glm-5.3 --effort high --json-schema "{"$schema":"http://json-schema.org/draft-07/schema#","title":"codex-consult reply v1","type":"object","additionalProperties":false,"required":["schema_version","verdict","verdict_reason","reply_markdown","findings","prior_findings","unproven","first_run_checklist"],"properties":{"schema_version":{"type":"string","enum":["1"]},"verdict":{"type":"string","enum":["ACCEPT","HOLD","REJECT","ADVISE"]},"verdict_reason":{"type":"string"},"reply_markdown":{"type":"string"},"findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["severity","locations","claim","trigger","evidence","verification","remedy","supersedes"],"properties":{"severity":{"type":"string","enum":["blocker","major","minor","note"]},"locations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["path","line"],"properties":{"path":{"type":"string"},"line":{"type":["integer","null"]}}}},"claim":{"type":"string"},"trigger":{"type":"string"},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","reference","observation"],"properties":{"kind":{"type":"string","enum":["read-code","ran-command","inferred","assumed"]},"reference":{"type":"string"},"observation":{"type":"string"}}}},"verification":{"type":"string"},"remedy":{"type":"string"},"supersedes":{"type":"array","items":{"type":"string"}}}}},"prior_findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","status","note"],"properties":{"id":{"type":"string"},"status":{"type":"string","enum":["fixed","still-open","not-checked","unknown-id"]},"note":{"type":"string"}}}},"unproven":{"type":"array","items":{"type":"string"}},"first_run_checklist":{"type":"array","items":{"type":"string"}}}}" --session-id cca1159b-d281-479d-b95b-891241f9677a` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `cca1159b-d281-479d-b95b-891241f9677a` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/73-claude-wave9-round5.md` (sha256 0040bb871094). Reviewed: 960d54e, base 960d54ebe7c32196423885a6767fc15038947943, tree sha256 c5656daf4060, 0 changed files.
Bridge outcome: usable reply. Wall time: 302.1 s. Tokens: in 142691 (cached 103360), out 18436, reasoning 0.
Engine turns: 1 (claude -p, auth endpoint; model glm-5.3; init tools Glob, Grep, Read, StructuredOutput; permission denials 0).
Warnings: ACCEPT with 15 unchecked prior blocker(s) (F14-1, F22-1, F23-1, F23-2, F23-3, F24-1, F25-2, F30-1, F30-2, F31-1, F36-1, F38-1, F40-1, F42-1, F66-1)..
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `891ea63..6a462f0` - the range changes 18 files, 12999 lines (12941 insertions, 58 deletions).
Verdict: ACCEPT - The 9d removal of the legacy no-id fallback fixes F72-1 and F72-2 outright: the cap pair is built only from id-carrying rows deduped by exact consult id with the 24 h bound applied at read time inside endpoint_health, the negatives (>1 h apart, slow 429, no-id rows, 25 h-old hits) are pinned by tests, and the only residuals found are documented design policy. Findings: 0 blocker, 0 major, 0 minor, 2 note (F74-1..F74-2, tracked in `findings.json`). Structured reply: `handoffs/74-claudecode-wave9-round5-glm.reply.json`.
Raw event stream: `handoffs/74-claudecode-wave9-round5-glm.events.jsonl`.
Verbatim reply follows.

---

## Answers

**Q1 — paths beyond the documented residual.** None found. Verified in code: the pair is built only from rows with a non-empty `consult_id` (`crates/c3-core/src/health.rs:1142`), deduped by exact id (`health.rs:1148-1154`), and every hit must be younger than 24 h at read time (`health.rs:1143`) — the filter lives inside `endpoint_health`, so `plan_cap_verdict` and every provider/preflight read inherit it (the providers test goes through `endpoint_health`, `crates/c3/src/providers.rs:3375`). The 100-character text comparison is gone. Negatives are pinned: no-id rows never complete a pair (`plan.rs:478-497`), hits 25 h old uncounted at read time (`plan.rs:517-536`), >1 h apart (`plan.rs:654`), slow 5 s 429 (`plan.rs:661`). Two consequences remain, both design rather than defects (filed as notes): (a) repeats inside ONE consultation share the id and count once, so exhaustion observed only within a single consultation stays on the plain 10-minute burst; (b) a non-empty `reset_hint` that never resolves (`next_cap_reset` → None, e.g. `monthly:99…`) falls back to second hit + 24 h while the pair actually dissolves at first hit + 24 h — only the refusal text can overstate the outage, by at most the pair gap.

**Q2 — ACCEPT (tag v0.2.2).** F72-1 (read-time bound) and F72-2 (identity, not text prefix) are fixed by removal, not refinement; the test suite covers the identity rule, the read-time bound and every negative; the requested checks (workspace tests 828 passed, clippy/fmt clean, harness 125/0) are green. Findings #1 and #2 are notes, not blockers.

---

### Findings

- **F74-1** [note] `crates/c3-core/src/health.rs:1150`, `crates/c3-core/src/plan.rs:344` - All instant-429 rows from one consultation share its consult_id and count as a single hit, so cap exhaustion that only ever surfaces inside a single consultation (retries or multiple reviewer attempts of one run) can never complete the pair and stays on the plain 10-minute burst. Trigger: One consultation makes two instant-429 attempts on a capped-plan endpoint; no second consultation fails. Evidence: read-code: Dedup skips any row whose consult_id is already in seen_ids, so same-id rows contribute one hit regardless of how many attempts they represent.; read-code: The machine record's consult_id is copied from the ledger entry's consultation id, so every row of one consultation carries the same id. Verify: Run a single consultation whose two reviewer attempts both fail as instant 429s on a capped entry and assert the entry keeps ordinary burst handling (no 'plan quota' verdict). Remedy: None required - it is the documented 'same id = one hit' rule; optionally state this consequence in the wave note's residual paragraph.
- **F74-2** [note] `crates/c3-core/src/plan.rs:361` - When a non-empty reset_hint never resolves (next_cap_reset returns None, e.g. 'monthly:99T00:00+08:00'), the displayed until falls back to second hit + 24 h while the read-time bound dissolves the pair at first hit + 24 h, so the refusal text can overstate the outage by up to the pair gap (max 60 min); the verdict itself flips to available correctly because the aged-out hit leaves len < 2. Trigger: A roster entry carries a syntactically accepted reset_hint for which next_cap_reset always returns None, and two id-carrying instant 429s land within an hour. Evidence: read-code: unhinted branch uses first + 24 h (matching the read-time bound), but the None fallback of the hinted branch uses second_utc + 24 h.; read-code: next_cap_reset returns None for 'monthly:99T00:00+08:00' and 'hourly:1'.; assumed: Not checked whether roster validation rejects such never-resolving hints; if it does, the path is unreachable. Verify: Check validate_roster against 'monthly:99T00:00+08:00'; if accepted, run plan_cap_verdict with two hits 60 min apart and compare v.until with the instant the pair ages out at read time. Remedy: Use first.with_timezone(&Utc) + Duration::hours(24) in the None fallback too, matching the unhinted branch.

### Prior findings

- F72-1 - fixed - health.rs:1143 applies CAP_PAIR_MAX_AGE_MINUTES inside endpoint_health, so every reader applies the 24 h bound at read time; pinned by the 25 h-old test (plan.rs:517-536).
- F72-2 - fixed - Pair dedup compares exact consult_id only (health.rs:1148-1154); no message-prefix comparison remains in the pair logic.
- F70-1 - fixed - Superseded by removal: rows without a consult_id take no part in the pair at all (health.rs:1142); pinned by plan.rs:478-497.
- F68-1 - fixed - Distinct consult ids count as two hits whatever their instants or texts (plan.rs:498-506); same-id ledger/machine mirror counts once (plan.rs:473-477).
- F68-2 - not-checked - Addressed in 9c per its commit message (admission test needs no PowerShell); not re-verified this round.
- F66-6 - fixed - Documented policy, pinned by the_cap_policy_is_inferred_exhaustion_with_a_24_hour_probe (plan.rs:539-582), updated for the read-time bound.
- F66-3 - fixed - Identity counting makes a ledger/machine mirror of one consultation one hit; pinned by plan.rs:473-477.
- F66-1 - not-checked - Outside the 9d diff; not re-verified this round.
- F66-2 - not-checked - Outside the 9d diff; not re-verified this round.
- F66-4 - not-checked - Outside the 9d diff; not re-verified this round.
- F66-5 - not-checked - Outside the 9d diff; not re-verified this round.
- F62-1 - not-checked - Quota persistence; outside the 9d diff.
- F60-1 - not-checked - Quota persistence; outside the 9d diff.
- F58-1 - not-checked - Quota aggregation; outside the 9d diff.
- F55-1 - not-checked - Quota cache; outside the 9d diff.
- F55-2 - not-checked - Quota validation; outside the 9d diff.
- F52-1 - not-checked - Quota ordering; outside the 9d diff.
- F52-2 - not-checked - Quota NaN handling; outside the 9d diff.
- F50-1 - not-checked - Quota merge; outside the 9d diff.
- F48-1 - not-checked - Quota merge; outside the 9d diff.
- F46-1 - not-checked - Quota aggregation; outside the 9d diff.
- F42-1 - not-checked - Recovery; outside the 9d diff.
- F40-1 - not-checked - Recovery; outside the 9d diff.
- F40-2 - not-checked - Kick acknowledgement; outside the 9d diff.
- F38-1 - not-checked - Recovery durability; outside the 9d diff.
- F36-1 - not-checked - Transcript guard; outside the 9d diff.
- F36-2 - not-checked - Transcript guard; outside the 9d diff.
- F31-1 - not-checked - Not-spooled accounting; outside the 9d diff.
- F30-1 - not-checked - Unknown-tree release; outside the 9d diff.
- F30-2 - not-checked - Kill evidence durability; outside the 9d diff.
- F30-3 - not-checked - Kill-record proof coverage; outside the 9d diff.
- F25-2 - not-checked - Claude transcript guard; outside the 9d diff.
- F25-3 - not-checked - Model validation; outside the 9d diff.
- F25-4 - not-checked - ChildEnv Debug; outside the 9d diff.
- F24-1 - not-checked - Not-spooled accounting; outside the 9d diff.
- F24-2 - not-checked - Not-spooled fold; outside the 9d diff.
- F24-3 - not-checked - Cleanup enumeration; outside the 9d diff.
- F24-4 - not-checked - Not-spooled status; outside the 9d diff.
- F24-5 - not-checked - P8 oracle adapter; outside the 9d diff.
- F24-6 - not-checked - Fold record rewrite; outside the 9d diff.
- F23-1 - not-checked - Liveness; outside the 9d diff.
- F23-2 - not-checked - Liveness; outside the 9d diff.
- F23-3 - not-checked - Recovery evidence durability; outside the 9d diff.
- F23-4 - not-checked - Recovery test determinism; outside the 9d diff.
- F23-5 - not-checked - Samoa zone; outside the 9d diff.
- F22-1 - not-checked - Journal replay; outside the 9d diff.
- F19-1 - not-checked - Complaint transaction; outside the 9d diff.
- F19-2 - not-checked - Cleanup salt handling; outside the 9d diff.
- F19-3 - not-checked - Sanitizer duplicate keys; outside the 9d diff.
- F14-1 - not-checked - Local-only forget record; outside the 9d diff.
- F11-1 - not-checked - Journal I/O errors; outside the 9d diff.
- F11-2 - not-checked - --require engine-exe; outside the 9d diff.
- F11-3 - not-checked - --require plan quota; outside the 9d diff.
- F11-4 - not-checked - Hook pointer syntax; outside the 9d diff.
- F11-5 - not-checked - AGY stall allowance; outside the 9d diff.
- F09-1 - not-checked - context_window_config panic; outside the 9d diff.
- F09-2 - not-checked - Flush vs deletion race; outside the 9d diff.
- F09-3 - not-checked - Cleanup ordering; outside the 9d diff.
- F09-4 - not-checked - Spool reread failure; outside the 9d diff.
- F09-5 - not-checked - Upgrade backlog privacy; outside the 9d diff.
- F09-6 - not-checked - Rating actor inference; outside the 9d diff.
- F06-1 - not-checked - Zone offset normalization; outside the 9d diff.
- F04-1 - not-checked - strip_exe panic; outside the 9d diff.
- F04-2 - not-checked - Windows zone boundary; outside the 9d diff.
- F04-3 - not-checked - Required-light ledger metadata; outside the 9d diff.
- F02-1 - not-checked - Spool flush race; outside the 9d diff.
- F02-3 - not-checked - Forget-me retry identity; outside the 9d diff.
- F02-4 - not-checked - Harness coverage map; outside the 9d diff.
- F02-5 - not-checked - Gap E label; outside the 9d diff.
- F02-6 - not-checked - Gap D anchor; outside the 9d diff.
- F02-7 - not-checked - Plan identity; outside the 9d diff.
- F02-8 - not-checked - Native Messages proposal assumption; a proposal, not part of the 9d diff.

## Verdict: ACCEPT

The 9d removal of the legacy no-id fallback fixes F72-1 and F72-2 outright: the cap pair is built only from id-carrying rows deduped by exact consult id with the 24 h bound applied at read time inside endpoint_health, the negatives (>1 h apart, slow 429, no-id rows, 25 h-old hits) are pinned by tests, and the only residuals found are documented design policy.

**WARNING: ACCEPT with 15 unchecked prior blocker(s) (F14-1, F22-1, F23-1, F23-2, F23-3, F24-1, F25-2, F30-1, F30-2, F31-1, F36-1, F38-1, F40-1, F42-1, F66-1).**

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

- Test, clippy, fmt and harness results are taken from the brief's requested-checks table; I could not run commands in this consultation.
- The ledger write path that stamps consult_id on new repository rows was not read; only the machine mirror site (health.rs:2559-2565) was.
- Newest-first ordering of health records (which plan.rs:348 relies on for hits[0] being the newest) is inferred from the 'more than an hour apart' test (plan.rs:654-660), not from reading the sort code.
- Whether validate_roster rejects never-resolving reset hints such as 'monthly:99T00:00+08:00' (reachability of finding #2) was not checked.
- The docs hunks of the range (README, CHANGELOG, the 9d paragraph of docs/port/wave9-consilium-defects.md) were not read; conclusions rest on the code hunks and tests.
- Pre-9b LEDGER rows also lack consult_id, so the upgrade residual applies to them exactly as to pre-9b machine rows; the brief words the residual in terms of machine rows only - same consequence, but I did not verify which rows the residual text claims to cover.

### First-run checklist (observable)

- [ ] A fresh 429 failure appears in both the repository ledger and the machine health file carrying the same consult_id (the machine record mirrors the ledger id).
- [ ] After upgrading, a retained pre-9b no-id machine row alone never produces a 'plan quota: 429 twice' outage in providers list or panel output.
- [ ] Two genuinely distinct consultations hitting instant 429s within an hour on a capped entry produce 'plan quota: 429 twice in N min, no window text; retry after <instant>' with the retry instant matching reset_hint (or first hit + 24 h when no hint is set).
- [ ] A successful explicit run after such an outage clears it (the operator override), and an unhinted entry becomes eligible again 24 h after the pair's first hit.
