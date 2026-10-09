# Handoff 30 - Codex: wave3a-round2-mimo

Date: 2026-10-09 11:23 local. Author: Codex (model mimo-v2.6-pro, effort high), Codex CLI 0.155.1.
Reviewer: mimo :: mimo-v2.6-pro (provider from -Provider, model from roster; endpoint https://token-plan-ams.xiaomimimo.com/v1, wire_api: responses; provider fingerprint 47cd6ee7e4ff; harness codex-cli 0.155.1).
Preflight: ok: env MIMO_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 3 of 12 for -Provider mimo (model, codex_config applied).
Effort: high sent (requested high, mapping mimo-v1, by caps-v1: token-plan-ams.xiaomimimo.com, mimo-v2.6-pro; not confirmed by the provider). Consultation id: d02f7c74-83e5-464d-aeab-43f4a9f2eaab.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m mimo-v2.6-pro -c model_reasoning_effort="high" -c model_provider="mimo" -c model_catalog_json="C:/Users/Dmytro/.codex/model-catalogs.json" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-985b24a1750d45d399812872c4aa7110.md fork 01a11fe6-04c9-7fd3-9523-f023d1e4f653 -` (prompt on stdin).
Parent thread: `01a11fe6-04c9-7fd3-9523-f023d1e4f653`. Result thread: `01a11ff9-827f-7623-9c7e-575292039954` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/27-claude-wave3a-round2.md` (sha256 6f1111c75c17). Reviewed: c8f0997, base c8f099712ae2aa9127a1c7b73cc997e77441ef55, tree sha256 56d7ef6cea2e, 0 changed files.
Bridge outcome: usable reply. Wall time: 1372.5 s. Tokens: in 25154601 (cached 18100480), out 83856, reasoning 51411.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `6bfc080..b649e96` - the range changes 7 files, 1868 lines (1701 insertions, 167 deletions).
Verdict: HOLD - HOLD — findings #1 and #2 keep the fail-closed recovery invariant broken. Findings: 2 blocker, 1 major, 0 minor, 0 note (F30-1..F30-3, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/30-codex-wave3a-round2-mimo.reply.json`.
Raw event stream: `handoffs/30-codex-wave3a-round2-mimo.events.jsonl`.
Verbatim reply follows.

---

**Q1.** Yes. Treating an unreadable start time as recent is safe only for rows the scans can attribute. A live reviewer helper can still be released early when it is a grandchild below an unrecorded wrapper and its name/command line do not look codex-like. The by-parent passes miss it, and the machine-wide pass excludes non-codex rows even when `created` is unknown. Blocking every unknown-start row would avoid that release but would also block protected system processes; the safer rule is to block unknown-start rows unless readable identity and command-line evidence proves they are unrelated.

**Q2.** Yes. The tree kill happens inside `run_turn`; the kept record is written only after the engine outcome returns to `KillSite::write` or `secondary_kill`. A bridge termination in that interval leaves only the old running record. Additionally, several `write_pending` errors are ignored, so failed persistence does not stop continuation, repair, or commit.

**Q3.** HOLD. F23-1 is fixed, but F23-2, F23-3, and the recovery proof remain incomplete.

## Requested checks
`RC1` (finding #1) — In `C:\Users\Dmytro\C3`, workspace-write, run `test_pending_active_with` with `kill_unconfirmed`, a dead recorded root, and an unrecorded wrapper plus `node.exe` child whose `created` is `None`; require active, never release. Budget: 20 minutes.

`RC2` (finding #2) — In `C:\Users\Dmytro\C3`, workspace-write, pause immediately after `kill_tree_checked` and before `KillSite::write`, terminate the bridge, and require the pending file to contain `survivors`, `unverified`, or `kill_unconfirmed`. Budget: 20 minutes.

`RC3` (finding #3) — In `C:\Users\Dmytro\C3`, workspace-write, extend `kill_record_durable` to continuation and format-repair kills and inject `write_pending` failure; require no further turn or commit without durable evidence. Budget: 30 minutes.

`RC4` (`F23-5`) — On Windows configured for Samoa Standard Time, workspace-write, run the production `chrono::Local` Samoa boundary and reset fixtures against the pinned plugin. Budget: 20 minutes.

---

### Findings

- **F30-1** [blocker] `crates/c3/src/liveness/proc.rs:815`, `crates/c3/src/liveness/pending.rs:297`, `crates/c3/src/liveness/pending.rs:902` - Unknown-tree release remains fail-open for a live reviewer helper whose creation time is unreadable and whose name or command line does not match the codex rule, especially when it is a grandchild below an unrecorded intermediate process. Trigger: A `kill_unconfirmed` record has a dead recorded root; the process table contains an unrecorded wrapper and a live `node.exe` or `python.exe` grandchild with `created: None` and an unreadable or unrelated command line. Evidence: read-code: Unknown-start rows are considered recent, but only codex-like rows enter `found`; non-codex rows are excluded.; read-code: The unknown-tree branch releases after the parent and machine-wide scans return no matching rows.; inferred: An unrecorded grandchild is invisible to the parent passes and can be excluded by the name rule despite being part of the reviewer tree. Verify: Run `test_pending_active_with` with the wrapper/helper synthetic table and require the record to remain active; repeat with a readable non-codex command line. Remedy: Scan full descendant relationships and treat unknown-start or command-line-unreadable/generic-runtime rows as blocking unless positive evidence proves they are unrelated. Supersedes: F23-2.
- **F30-2** [blocker] `crates/c3/src/engines/subprocess.rs:536`, `crates/c3/src/consult/orchestrate.rs:587`, `crates/c3/src/consult/orchestrate.rs:4927`, `crates/c3/src/consult/orchestrate.rs:5674` - Kill evidence is not durable at the actual kill: the processes are killed before the kept record is written, and write failures are ignored while execution continues. Trigger: Terminate the bridge after `kill_tree_checked` returns but before `KillSite::write`, or force `write_pending` to fail and continue to a later turn or commit. Evidence: read-code: The tree kill completes inside `run_turn` before outcome handling returns.; read-code: `secondary_kill` performs the record write after the turn returns and discards its error.; read-code: The main kill write occurs only after outcome handling, and failures only warn. Verify: Add a pause between `kill_tree_checked` and `KillSite::write`, terminate there, and separately inject `write_pending` errors before continuation or commit. Remedy: Persist the kill record synchronously from the kill path or an append-only sidecar before returning, and fail closed when that persistence fails. Supersedes: F23-3.
- **F30-3** [major] `crates/c3-cli/tests/kill_record_durable.rs:68`, `crates/c3-cli/tests/kill_record_durable.rs:180` - The recovery proof covers only a successful main-turn write followed by a pause; it does not prove continuation, format-repair, write-failure, or pre-write crash durability. Trigger: Exercise a killed continuation or repair turn, or terminate the bridge before `KillSite::write` completes. Evidence: read-code: The bridge is killed only after the pending record already reports `survivors`.; read-code: Only main-turn unconfirmed and unverified cases are asserted. Verify: Add fixtures for each kill site plus injected `write_pending` failure and assert durable evidence before any subsequent turn or commit. Remedy: Extend RC3 to all three kill sites and persistence failures before claiming the invariant proven. Supersedes: F23-4.

### Prior findings

- F02-1 - not-checked - Outside 6bfc080..b649e96.
- F02-3 - not-checked - Outside this range.
- F02-4 - not-checked - Outside this range.
- F02-5 - not-checked - Outside this range.
- F02-6 - not-checked - Outside this range.
- F02-7 - not-checked - Outside this range.
- F02-8 - not-checked - Outside this range.
- F04-1 - fixed - Unicode-safe process-name handling is present.
- F04-2 - not-checked - Outside this range.
- F04-3 - not-checked - Outside this range.
- F06-1 - fixed - Samoa local-year normalization is implemented and exercised with registry-derived fixtures.
- F09-1 - not-checked - Outside this range.
- F09-2 - not-checked - Outside this range.
- F09-3 - not-checked - Outside this range.
- F09-4 - not-checked - Outside this range.
- F09-5 - not-checked - Outside this range.
- F09-6 - not-checked - Outside this range.
- F11-1 - not-checked - Outside this range.
- F11-2 - not-checked - Outside this range.
- F11-3 - not-checked - Outside this range.
- F11-4 - not-checked - Outside this range.
- F11-5 - not-checked - Outside this range.
- F14-1 - not-checked - Outside this range.
- F19-1 - not-checked - Outside this range.
- F19-2 - not-checked - Outside this range.
- F19-3 - not-checked - Outside this range.
- F22-1 - fixed - The applied-key replay guard addresses the C3-local eviction replay.
- F23-1 - fixed - Process lookup failure now returns `inspected: false` and counts as running.
- F23-2 - fixed - Unknown-start rows are no longer skipped; the corrected residual claim is finding #1.
- F23-3 - fixed - Records are written after each kill site; the corrected durability claim is finding #2.
- F23-4 - fixed - The specific vacuous and timing-dependent fixtures were rewritten; remaining proof gaps are finding #3.
- F23-5 - still-open - Registry-derived Samoa fixtures are stronger, but production `chrono::Local` remains unrun except on a Samoa-zone machine.
- F24-1 - not-checked - Outside this range.
- F24-2 - not-checked - Outside this range.
- F24-3 - not-checked - Outside this range.
- F24-4 - not-checked - Outside this range.
- F24-5 - not-checked - Outside this range.
- F24-6 - not-checked - Outside this range.
- F25-1 - not-checked - Outside this range.
- F25-2 - not-checked - Outside this range.
- F25-3 - not-checked - Outside this range.
- F25-4 - not-checked - Outside this range.
- F29-1 - not-checked - Outside this range.

## Verdict: HOLD

HOLD — findings #1 and #2 keep the fail-closed recovery invariant broken.

### Blockers

- **F30-1** `crates/c3/src/liveness/proc.rs:815`, `crates/c3/src/liveness/pending.rs:297`, `crates/c3/src/liveness/pending.rs:902` - Unknown-tree release remains fail-open for a live reviewer helper whose creation time is unreadable and whose name or command line does not match the codex rule, especially when it is a grandchild below an unrecorded intermediate process. Verify: Run `test_pending_active_with` with the wrapper/helper synthetic table and require the record to remain active; repeat with a readable non-codex command line. Remedy: Scan full descendant relationships and treat unknown-start or command-line-unreadable/generic-runtime rows as blocking unless positive evidence proves they are unrelated.
- **F30-2** `crates/c3/src/engines/subprocess.rs:536`, `crates/c3/src/consult/orchestrate.rs:587`, `crates/c3/src/consult/orchestrate.rs:4927`, `crates/c3/src/consult/orchestrate.rs:5674` - Kill evidence is not durable at the actual kill: the processes are killed before the kept record is written, and write failures are ignored while execution continues. Verify: Add a pause between `kill_tree_checked` and `KillSite::write`, terminate there, and separately inject `write_pending` errors before continuation or commit. Remedy: Persist the kill record synchronously from the kill path or an append-only sidecar before returning, and fail closed when that persistence fails.
- **F14-1** (prior, not-checked) `crates/c3/src/telemetry/complaint.rs:542`, `crates/c3/src/telemetry/complaint.rs:240`, `crates/c3/src/telemetry/complaint.rs:673` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending'). Verify: Add a test: forget_with with ForgetRequest{public_ref: None, local: true, yes: true} and an injected remove failing on spool.ndjson; assert whether forget-pending.json exists and whether a subsequent flush posts the retained events. Remedy: In the None arm, persist the transaction (phase cleaning) via write_transaction before calling finish_cleanup (or pass phase confirmed so run_local_cleanup writes it), so an interruption leaves the blocking, resumable record the docs and the CLI message promise.
- **F24-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:640`, `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:704` - The spool-lock fallback marks all non-legacy not-spooled lines as seen without folding or retaining per-file provenance; a later fold counts the same gone-producer files again, so the accounting can double-count and temporarily hide the count. Verify: Hold `spool.lock` for over one second during flush 1 with N gone-producer lines, then run flush 2 and compare both records; require exactly one accounting of N. Remedy: In the fallback, retain per-file folded entries or mark only positively kept files seen; never advance the global baseline for files that remain foldable.
- **F25-1** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:197`, `crates/c3/src/engines/claude_auth.rs:311` - Endpoint-auth preflight reports available without proving the configured Claude launcher can run; only the token variable is checked, so a wrong or non-runnable launcher passes preflight and fails later. Verify: Run endpoint preflight with a non-executable launcher and require unavailable before the turn starts. Remedy: Run a cheap launcher probe such as `--version` or `auth status` in the endpoint child environment before returning available.
- **F25-2** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:357`, `crates/c3/src/engines/claude_auth.rs:374` - Endpoint mode cannot enforce the transcript-location safety guard because it never populates `projectsDirectory`; transcripts may land inside the repository and alter the tree being reviewed. Verify: Make endpoint-mode `auth status` report an in-repository projects directory and require refusal before spawn. Remedy: Probe the status facts for endpoint mode too, without reading or logging the token, and reject unsafe transcript locations.
- **F29-1** (prior, not-checked) `crates/c3-core/src/health.rs:1682`, `crates/c3-core/src/health.rs:2017`, `crates/c3-core/src/health.rs:2045`, `crates/c3-core/src/health.rs:2189` - The applied-key list is only a C3-local replay guard: it is silently ignored when missing or malformed, and the plugin’s shared journal updater neither reads nor preserves it, so a retained journal record can still be replayed after endpoint-key eviction and applied twice. Verify: Create a kept journal record, evict its endpoint through the cap, remove or invalidate `.journal.applied`, run the plugin journal updater and then an update, and assert the record is neither re-added nor allowed to displace a newer endpoint. Remedy: Make consumed-journal state authoritative in the shared journal protocol, or fail closed when the retained journal has no trustworthy ledger; add a mixed C3/plugin crash-and-eviction fixture.

### Unproven scenarios

- No tests were run in this read-only consultation.
- The unrecorded-grandchild release path and the pre-write kill window were reasoned from code, not executed.
- Continuation and format-repair kill durability remains unproved.
- Production Samoa `chrono::Local` behavior remains unproved on a non-Samoa machine.

### First-run checklist (observable)

_(none)_
