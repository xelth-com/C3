# Handoff 25 - Codex: wave4-diff-review-mimo

Date: 2026-10-09 09:53 local. Author: Codex (model mimo-v2.6-pro, effort high), Codex CLI 0.155.1.
Reviewer: mimo :: mimo-v2.6-pro (provider from -Provider, model from roster; endpoint https://token-plan-ams.xiaomimimo.com/v1, wire_api: responses; provider fingerprint 47cd6ee7e4ff; harness codex-cli 0.155.1).
Preflight: ok: env MIMO_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 3 of 12 for -Provider mimo (model, codex_config applied).
Effort: high sent (requested high, mapping mimo-v1, by caps-v1: token-plan-ams.xiaomimimo.com, mimo-v2.6-pro; not confirmed by the provider). Consultation id: 5d4d3ea9-89b2-49c9-841c-bf709a3462ab.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m mimo-v2.6-pro -c model_reasoning_effort="high" -c model_provider="mimo" -c model_catalog_json="C:/Users/Dmytro/.codex/model-catalogs.json" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-4d78d04d4254441b82c858c017d13557.md fork 01a11f84-5109-72a2-82b2-9bc3b94cd00a -` (prompt on stdin).
Parent thread: `01a11f84-5109-72a2-82b2-9bc3b94cd00a`. Result thread: `01a11fa7-8a2c-7322-90b3-9a5b8710f70d` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/24-claude-wave4-diff-review.md` (sha256 04669c584cff). Reviewed: fda1f33, base fda1f33669e0f7c1e6bd1363341545617f8d1dfe, tree sha256 c5d196dca1bf, 0 changed files.
WARNING: working tree changed during the review (fingerprint before/after differ).
Note: HEAD moved during the review (fda1f33 -> f7105f3).
Bridge outcome: usable reply. Wall time: 1124.3 s. Tokens: in 17957569 (cached 12190976), out 59945, reasoning 36339.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `6bfc080..1fdb8bd` - the range changes 32 files, 6214 lines (6092 insertions, 122 deletions).
Verdict: HOLD - HOLD — findings #1 and #2 leave endpoint-mode availability and transcript safety unproven. Findings: 2 blocker, 0 major, 2 minor, 0 note (F25-1..F25-4, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/25-codex-wave4-diff-review-mimo.reply.json`.
Raw event stream: `handoffs/25-codex-wave4-diff-review-mimo.events.jsonl`.
Verbatim reply follows.

---

**Q1.** No direct endpoint-token value path into files, logs, or the ledger was found; the allow list stores names only. However, `ChildEnv` derives `Debug` while holding the endpoint token, creating a latent leak if debug logging or panic output prints it. The tree-check gap is concrete: endpoint mode never resolves `projectsDirectory`, so transcripts can be written inside the repository and evade the intended launch guard.

**Q2.** The init proof, E13 model check, killed-turn proof, and quota-mark mapping are present in the reviewed code paths, but the endpoint preflight is not faithful: it accepts a token variable without proving the launcher runs, and it skips the transcript-location check. Model validation also rejects case variants that the normalization rules accept.

**Q3.** HOLD until findings #1 and #2 are fixed and the endpoint-mode safety checks are exercised.

## Requested checks
`RC1` (finding #1) — In `C:\Users\Dmytro\C3`, workspace-write, configure valid endpoint auth with a non-executable or wrong launcher and run the preflight; require unavailable before any turn starts. Budget: 20 minutes.

`RC2` (finding #2) — In `C:\Users\Dmytro\C3`, workspace-write, make endpoint-mode `claude auth status` report `projectsDirectory` inside the repository and require launch refusal. Budget: 20 minutes.

`RC3` (finding #3) — In `C:\Users\Dmytro\C3`, workspace-write, validate roster models `OPUS` and `CLAUDE-HAIKU-5-5` and compare with the pinned plugin rules. Budget: 10 minutes.

`RC4` (finding #4) — In `C:\Users\Dmytro\C3`, workspace-write, enable debug logging around endpoint child-environment construction and require no token value in output. Budget: 10 minutes.

---

### Findings

- **F25-1** [blocker] `crates/c3/src/engines/claude_auth.rs:197`, `crates/c3/src/engines/claude_auth.rs:311` - Endpoint-auth preflight reports available without proving the configured Claude launcher can run; only the token variable is checked, so a wrong or non-runnable launcher passes preflight and fails later. Trigger: Valid `auth: endpoint` and token variable, but `--engine-exe` or the configured launcher points to a non-executable or unrelated binary. Evidence: read-code: Endpoint mode returns `ok(env <NAME> set)` before any launcher probe.; read-code: Subscription and api-key modes execute `claude auth status`, proving the launcher runs. Verify: Run endpoint preflight with a non-executable launcher and require unavailable before the turn starts. Remedy: Run a cheap launcher probe such as `--version` or `auth status` in the endpoint child environment before returning available.
- **F25-2** [blocker] `crates/c3/src/engines/claude_auth.rs:357`, `crates/c3/src/engines/claude_auth.rs:374` - Endpoint mode cannot enforce the transcript-location safety guard because it never populates `projectsDirectory`; transcripts may land inside the repository and alter the tree being reviewed. Trigger: Endpoint auth with `CLAUDE_CONFIG_DIR` outside the repository but Claude's configured projects directory inside it. Evidence: read-code: Endpoint mode returns before parsing `claude auth status` facts.; read-code: The launch guard consults only `CLAUDE_CONFIG_DIR` and cached sign-in information. Verify: Make endpoint-mode `auth status` report an in-repository projects directory and require refusal before spawn. Remedy: Probe the status facts for endpoint mode too, without reading or logging the token, and reject unsafe transcript locations.
- **F25-3** [minor] `crates/c3-core/src/claude.rs:130`, `crates/c3-core/src/claude.rs:139` - Claude model validation is case-sensitive even though the model normalization and alias rules are case-insensitive, rejecting valid spellings such as `OPUS`. Trigger: A subscription or api-key roster entry uses `OPUS`, `CLAUDE-HAIKU-5-5`, or another case variant. Evidence: read-code: The non-endpoint path exact-matches the lower-case model table after only `[1m]` stripping.; read-code: The normalization helpers lowercase aliases and model bases. Verify: Compare `model_problem` results for case variants against the pinned plugin's model rules. Remedy: Validate the lower-cased stripped base while preserving the original spelling in messages.
- **F25-4** [minor] `crates/c3-core/src/claude.rs:392` - The endpoint child environment derives `Debug` while containing `ANTHROPIC_AUTH_TOKEN`, so accidental debug or panic output can expose the credential value. Trigger: Any future logging, tracing, or error formatting that prints the `ChildEnv` debug representation during endpoint auth. Evidence: read-code: `ChildEnv` stores token values and derives `Debug`; the ledger contract stores names only. Verify: Enable debug logging around child-environment construction and assert no token value appears. Remedy: Redact secret values in `Debug` or wrap the token in a secret type that never formats its value.

### Prior findings

- F02-1 - fixed - Spool rewrite synchronization is implemented.
- F02-3 - fixed - Deletion retry identity is retained.
- F02-4 - still-open - Implementation coverage map remains incomplete.
- F02-5 - fixed - Rating mark ordering is implemented.
- F02-6 - fixed - Context-budgeted prompt anchor is implemented.
- F02-7 - fixed - Claude roster plan identity is accepted and scheduled.
- F02-8 - not-checked - Outside this range.
- F04-1 - fixed - Unicode-safe process-name handling is present.
- F04-2 - fixed - Local-zone boundary normalization is present.
- F04-3 - not-checked - Panel gate behavior outside this range.
- F06-1 - fixed - Samoa local-year handling is implemented.
- F09-1 - fixed - Unicode-safe configuration handling is present.
- F09-2 - fixed - Deletion checks occur under both locks.
- F09-3 - fixed - Cleanup ordering preserves retry identity.
- F09-4 - fixed - Reread failure preserves the spool.
- F09-5 - fixed - Legacy events pass through sanitization.
- F09-6 - not-checked - Rating actor resolution outside this range.
- F11-1 - fixed - Journal I/O failures are surfaced and preserved.
- F11-2 - fixed - Resolved engine launcher reaches requirement checks.
- F11-3 - fixed - Full roster plan quota evaluation is implemented.
- F11-4 - fixed - Hook pointer is runnable in PowerShell.
- F11-5 - fixed - AGY tool-flight tracking is implemented.
- F14-1 - fixed - Local-only cleanup records its transaction.
- F19-1 - fixed - Complaint submission participates in deletion gating.
- F19-2 - fixed - Unreadable salt aborts cleanup safely.
- F19-3 - not-checked - Duplicate-key sanitizer behavior outside this range.
- F22-1 - still-open - Journal eviction/replay invariant remains open.
- F23-1 - still-open - Process inspection failure can still be treated as gone.
- F23-2 - still-open - Unknown creation rows can still be skipped.
- F23-3 - still-open - Recovery evidence is still written late.
- F23-4 - still-open - Fail-closed recovery tests remain incomplete.
- F23-5 - still-open - Production Samoa behavior remains unproven.
- F24-1 - still-open - Not-spooled fallback accounting remains open.
- F24-2 - still-open - Recorded suffix-tail handling remains open.
- F24-3 - still-open - Cleanup versus concurrent count writes remains open.
- F24-4 - still-open - Latest not-spooled reporting remains open.
- F24-5 - still-open - P8 marker and production-path proof remains open.
- F24-6 - still-open - Post-fold second rewrite error remains unhandled.

## Verdict: HOLD

HOLD — findings #1 and #2 leave endpoint-mode availability and transcript safety unproven.

### Blockers

- **F25-1** `crates/c3/src/engines/claude_auth.rs:197`, `crates/c3/src/engines/claude_auth.rs:311` - Endpoint-auth preflight reports available without proving the configured Claude launcher can run; only the token variable is checked, so a wrong or non-runnable launcher passes preflight and fails later. Verify: Run endpoint preflight with a non-executable launcher and require unavailable before the turn starts. Remedy: Run a cheap launcher probe such as `--version` or `auth status` in the endpoint child environment before returning available.
- **F25-2** `crates/c3/src/engines/claude_auth.rs:357`, `crates/c3/src/engines/claude_auth.rs:374` - Endpoint mode cannot enforce the transcript-location safety guard because it never populates `projectsDirectory`; transcripts may land inside the repository and alter the tree being reviewed. Verify: Make endpoint-mode `auth status` report an in-repository projects directory and require refusal before spawn. Remedy: Probe the status facts for endpoint mode too, without reading or logging the token, and reject unsafe transcript locations.
- **F22-1** (prior, still-open) `crates/c3-core/src/health.rs:1563`, `crates/c3-core/src/health.rs:1915`, `crates/c3-core/src/health.rs:1942`, `crates/c3-core/src/health.rs:1960`, `crates/c3-core/src/health.rs:2028` - After failed `.bad` archival, the retained journal is replayed against mutable endpoint state whose record keys can be removed by the 24-hour or 500-record retention rules. A journal record can therefore be applied, evicted, and re-applied with a different final effect, while a record already stale on a replay is parsed but never becomes visible. The record-key dedup is not durable, so the documented harmless/once replay invariant is false. Verify: Add a deterministic test with an unwritable `.bad` path, one recent journal record, and 500 newer endpoint records; run two updates and assert the journal record is neither resurrected nor allowed to displace another endpoint. Remedy: Persist a monotonic journal applied-sequence marker or equivalent durable consumed state, and truncate the journal by that sequence independently of endpoint retention; do not use the mutable endpoint set as the only replay ledger.
- **F23-1** (prior, still-open) `crates/c3/src/liveness/proc.rs:97`, `crates/c3/src/liveness/proc.rs:100`, `crates/c3/src/liveness/pending.rs:283`, `crates/c3/src/liveness/pending.rs:344` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time. Verify: Inject `name_and_parent -> None` while `process_start_iso -> Some(start)` and require `test_unverified_process`/`test_recorded_process` to return running or an explicit inspection failure. Remedy: Distinguish process absence from inspection failure; return an unknown/running verdict when the pid exists but metadata cannot be read.
- **F23-2** (prior, still-open) `crates/c3/src/liveness/proc.rs:753`, `crates/c3/src/liveness/proc.rs:775`, `crates/c3/src/liveness/proc.rs:827`, `crates/c3/src/liveness/pending.rs:790` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs. Verify: Run `test_pending_active` on a `kill_unconfirmed` record with a live scan row whose `created` is `None`; require refusal/failed verification, never release. Remedy: Treat unknown creation/start as blocking evidence or fail the scan; do not silently filter those rows.
- **F23-3** (prior, still-open) `crates/c3/src/consult/orchestrate.rs:5105`, `crates/c3/src/consult/orchestrate.rs:5143`, `docs/port/wave3a-recovery.md:59` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree. Verify: Add a pause between the kill and the end-of-run pending write, terminate the bridge there, and inspect the recovery record for the expected `unverified`/`kill_unconfirmed` fields. Remedy: Persist the kill disposition at each kill site, or write a durable kill journal before continuing the run.
- **F24-1** (prior, still-open) `crates/c3/src/telemetry/spool.rs:640`, `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:704` - The spool-lock fallback marks all non-legacy not-spooled lines as seen without folding or retaining per-file provenance; a later fold counts the same gone-producer files again, so the accounting can double-count and temporarily hide the count. Verify: Hold `spool.lock` for over one second during flush 1 with N gone-producer lines, then run flush 2 and compare both records; require exactly one accounting of N. Remedy: In the fallback, retain per-file folded entries or mark only positively kept files seen; never advance the global baseline for files that remain foldable.

### Unproven scenarios

- No tests were run in this read-only consultation.
- Endpoint-mode launcher probing and transcript-location refusal were not executed.
- The init proof, E13 model check, E12-E15 killed-turn proof, and quota-mark persistence were not independently exercised.
- No endpoint-mode live run or token-safety logging test was performed.

### First-run checklist (observable)

_(none)_
