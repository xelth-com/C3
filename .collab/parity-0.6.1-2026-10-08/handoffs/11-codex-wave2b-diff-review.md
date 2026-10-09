# Handoff 11 - Codex: wave2b-diff-review

Date: 2026-10-09 03:54 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 04189e96-9098-4269-8d64-3f5924b328e2.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-f15d99a1d87b40449f0ea41ff3e49b99.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json fork 01a11e3b-363e-7951-a94f-4c1f3fa0f187 -` (prompt on stdin).
Parent thread: `01a11e3b-363e-7951-a94f-4c1f3fa0f187`. Result thread: `01a11e5e-ac5f-7b31-b3d9-49fd2cd68d88` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/10-claude-wave2b-diff-review.md` (sha256 eef8df46a74f). Reviewed: 6a5f838, base 6a5f838d8f15dcbca25f8e901561ebfe41975997, tree sha256 185ba27012b1, 0 changed files.
Bridge outcome: usable reply. Wall time: 306.9 s. Tokens: in 8727839 (cached 7684608), out 37920, reasoning 7796.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `0d78ec0..1fa1ba7` - the range changes 34 files, 3438 lines (3084 insertions, 354 deletions).
Verdict: HOLD - The health journal can discard records after I/O failures, and the new single-run requirement check evaluates availability with incomplete launcher and plan context. Findings: 0 blocker, 4 major, 1 minor, 0 note (F11-1..F11-5, tracked in `findings.json`). Structured reply: `handoffs/11-codex-wave2b-diff-review.reply.json`.
Raw event stream: `handoffs/11-codex-wave2b-diff-review.events.jsonl`.
Verbatim reply follows.

---

1. **The normal health write order is sound; its error paths are not.** One immutable record is attempted before the task lock, journaled after failure, and retried after commit. The health lock and exclusive journal handle serialize replay; record-key deduplication handles replay after a crash between health replacement and journal clearing. However, ignored read and suffix-write errors can discard unapplied journal bytes while reporting success (finding #1). Existing replay, malformed-line and `.bad` tests do not exercise those failures.

2. **The plain-token `raw_arg` change looks appropriately constrained.** Its whitelist excludes shell metacharacters; I found no new injection path. Some differences are intentional: `foo="%TEMP%"` can expand through the plugin’s batch path but remains literal through Rust’s escaping; CR/LF in batch arguments is refused; backslashes before quotes are escaped differently to preserve downstream arguments. Keep these protections and document them as compatibility differences. Ordinary multiline asks use stdin or a prompt file, so argument-level newline refusal does not reject those asks. The batch echo tests do not establish native argv preservation for percent expressions or backslash/quote boundaries.

3. **REFUSE D3 wording can be a documented difference.** `BriefPrefix` can be deferred if the release explicitly excludes it and users can supply equivalent brief content. The label resolver should remain a release dependency on wave 2d: F09-6 also affects coordinator attribution and self-review warnings, so it is not merely wording.

   Additional gaps surfaced here: single-run `--require` forgets the explicit engine launcher (#2), and filtering its roster before plan evaluation hides quota failures on sibling routes (#3). The latter also exists in the pinned plugin: faithful copying does not satisfy the promised roster-walk availability invariant. The default hook command fails PowerShell parsing (#4), while the harness overrides that command. AGY still lacks the plugin’s open-tool stall allowance (#5); this is an existing omission, not a new regression.

4. **HOLD wave 2b for findings #1–#3.** Fix the hook pointer and assign the AGY omission explicitly before claiming full compatibility. Wave 2d remains independently responsible for F09-1–F09-6, which are still present at the reviewed revision. Reported harness improvements are useful evidence, but I did not rerun builds or tests; this review used source inspection, pinned-plugin comparison and a read-only PowerShell parse check.

## Requested checks

- **RC1:** Cwd repository root; workspace-write; add journal fault-injection fixtures for a partial read failure and a retained-suffix write failure. Require preserved unapplied bytes and a failure result, then successful replay after removing the fault (#1). Budget: two fixtures, 25 minutes.
- **RC2:** Cwd repository root; workspace-write; extend the single-run requirement fixture with two cases: an AGY launcher available only through `--engine-exe`, and a required healthy route whose omitted plan sibling has an active quota failure. Require success for the first and exit 5 for the second (#2–#3). Budget: two cases, 25 minutes.
- **RC3:** Cwd repository root; workspace-write; forward batch-launcher arguments into a native argv-dumping helper using literal `%TEMP%`, backslash-before-quote, a spaced path ending in backslash, and CR/LF. Require exact supported argv and explicit newline refusal. Budget: one fixture matrix, 15 minutes.
- **RC4:** Cwd repository root; workspace-write; make a fake AGY emit an ACTIVE tool event, remain quiet for four seconds, then complete under `--stall-sec 3`; repeat beyond six seconds. Require success first and a labelled stall second (#5). Budget: two runs, 15 minutes.

---

### Findings

- **F11-1** [major] `crates/c3-core/src/health.rs:1839`, `crates/c3-core/src/health.rs:1968`, `crates/c3-core/src/health.rs:1971`, `crates/c3-core/src/health.rs:1972` - Ignored journal I/O errors can discard unapplied records or the suffix retained after a failed .bad archival, while the update reports HealthUpdate::Written. Trigger: Reading the journal fails after zero or partial bytes; alternatively, .bad archival fails and the subsequent retained-suffix rewrite fails before truncation succeeds. Evidence: read-code: The read result is discarded. Consumption is calculated from the bytes successfully read, and the journal can then be truncated to zero despite an unread tail.; read-code: Retained-suffix seek and write_all errors are ignored; set_len still runs and the closure returns success.; ran-command: The comparator script matched the pinned v0.6.1 version.; read-code: A journal-read exception exits before clearing; suffix Write and SetLength share a try block, so a write exception skips truncation. Verify: Inject partial journal-read and retained-suffix-write failures; require preserved unapplied bytes, a failure result, and successful later replay. Remedy: Propagate journal read and seek errors, and never truncate after unsuccessful rewriting. Prefer retaining the entire journal when archival fails and relying on deduplication, or implement recovery-safe atomic compaction.
- **F11-2** [major] `crates/c3/src/consult/orchestrate.rs:1630`, `crates/c3/src/providers.rs:396`, `crates/c3/src/providers.rs:847` - Single-run --require does not carry the resolved --engine-exe into its availability context, so it can reject a usable reviewer or inspect a different launcher. Trigger: A selected AGY reviewer is explicitly required, its working launcher is supplied through --engine-exe, and AGY is absent from normal launcher discovery or resolves there to another installation. Evidence: read-code: The explicit launcher is resolved and seeded into the roster-walk context, but the newly constructed requirement context receives no corresponding seed.; read-code: for_consult creates an empty engine-launcher cache; non-Codex discovery subsequently resolves with an empty explicit-launcher argument.; ran-command: The plugin seeds engineLaunchers with EngineExe and passes that map into the single-run required-reviewer selection.; read-code: The new requirement test exercises Codex providers and missing credentials, not a non-Codex explicit launcher. Verify: Run a required AGY reviewer with a fake working launcher supplied only through --engine-exe and absent from normal discovery; require the availability check to use that launcher. Remedy: Seed the requirement context with the resolved engine binding, or reuse the complete launcher context already built for the consultation.
- **F11-3** [major] `crates/c3/src/consult/orchestrate.rs:1626`, `crates/c3/src/providers.rs:994` - Single-run --require can report a required reviewer available despite an active quota failure on another route of its shared plan, because non-required roster entries are removed before plan quota evaluation. Trigger: The selected reviewer belongs to an unrelated plan; required entry #2 shares a plan with non-required entry #3; #2 has usable credentials and no own failure, while #3 has an active recorded usage limit. Evidence: read-code: The requirement context receives a roster retaining only required positions.; read-code: plan_quota constructs the plan's fingerprint set exclusively from that context's roster, so omitted sibling routes cannot contribute quota failures.; read-code: Required-member availability uses plan_verdict, but its plan evaluation has already lost the omitted route.; ran-command: The pinned plugin also filters the roster before this calculation; this is an inherited correctness gap rather than a mismatch with that implementation. Verify: Seed an active quota record for non-required entry #3 sharing required entry #2's plan, then run an unrelated provider with --require #2; require exit 5 naming the plan outage. Remedy: Retain the full roster for plan and identity resolution, and filter only the members whose availability results are required.
- **F11-4** [minor] `crates/c3/src/hook/mod.rs:46`, `plugin/hooks/c3-hook.ps1:55`, `tests/shim/codex-consult-hook.ps1:99` - The default hook pointer is not runnable as written in PowerShell because it places a quoted executable path directly before arguments without the call operator. Trigger: A Windows user or coordinator copies the production hook's command, such as "C:\Users\Dmytro\C3\target\debug\c3.exe" consult --explain coordinate, into PowerShell. Evidence: read-code: The default command is formatted as a quoted executable path followed by consult arguments, without '&'.; ran-command: Parsing returned UnexpectedToken at 'consult'; no command was executed.; read-code: The production wrapper uses the default command, while the harness shim supplies a different powershell -File command and therefore bypasses the defect. Verify: Parse and execute the production pointer command in PowerShell 5.1 and PowerShell 7, requiring the coordinate skill text to print successfully. Remedy: Generate a shell-appropriate invocation, including PowerShell's call operator and correct path quoting, and test the production default rather than only the shim override.
- **F11-5** [major] `crates/c3/src/engines/agy.rs:104`, `crates/c3/src/engines/subprocess.rs:592` - AGY tool calls still receive neither the plugin's extended stall allowance nor open-call attribution, allowing C3 to terminate a healthy quiet tool after stall_sec instead of twice that interval. Trigger: An AGY primary turn emits a step_update with step_type='tool', state='ACTIVE' and step_index=2, then remains quiet longer than stall_sec but less than twice stall_sec before completing. Evidence: read-code: AGY supplies its stall limit but passes tool_flight=None.; read-code: The doubled quiet-time bound and open-call labels apply only when the tool tracker is nonempty.; read-code: The plugin tracks AGY ACTIVE tool steps, closes them on other states, labels them by step index and applies the doubled bound.; ran-command: tool_delta=None becomes tool_flight=None, confirming an existing parity omission rather than a newly introduced regression. Verify: With stall_sec=3, emit an ACTIVE AGY tool event followed by four quiet seconds and successful completion; require success, then verify a longer-than-six-second case stalls with the tool label. Remedy: Implement an AGY ToolFlight classifier keyed by step_index and connect it to the shared subprocess tracker.

### Prior findings

- F02-1 - fixed - Retains the previous closure for the original concurrent-append overwrite; the telemetry implementation is unchanged in this range.
- F02-3 - fixed - Retains the previous closure for failed-DELETE retry identity; the separate F09 deletion defects remain open.
- F02-4 - fixed - The coverage map and telemetry shim remain present; harness passes still require interpretation by implementation coverage.
- F02-5 - fixed - The earlier ordering correction remains valid; this range does not change the rating commit-before-spool path.
- F02-6 - not-checked - The inline re-read anchor was not revalidated in this review.
- F02-7 - not-checked - Full cross-engine and cross-repository plan behavior was not revalidated. Finding #3 identifies a separate new requirement-check interaction with existing plan evaluation.
- F02-8 - fixed - Plan P4 continues to gate native Messages on a separate benchmark rather than inheriting the plugin's measured savings.
- F04-1 - not-checked - The process-name Unicode fix is outside this range and was not rerun.
- F04-2 - not-checked - Health-journal changes were reviewed, but Windows local-zone boundary parsing was not revalidated.
- F04-3 - not-checked - Required-light execution and ledger metadata were not revalidated.
- F06-1 - still-open - The state document retains the historical Samoa case for wave 3; this delta does not correct it.
- F09-1 - still-open - The reviewed revision still slices extra configuration strings at key.len() without a UTF-8 boundary check.
- F09-2 - still-open - The reviewed revision still checks pending deletion before acquiring flush.lock.
- F09-3 - still-open - Successful DELETE still removes the pending record before local cleanup; telemetry fixes are outside this range.
- F09-4 - still-open - The current-spool reread still uses unwrap_or_default before replacement.
- F09-5 - still-open - Legacy payload forwarding is unchanged; this range adds no backlog sanitization.
- F09-6 - still-open - Rating actor resolution still uses the incomplete coordinator parser; the brief assigns its correction to wave 2d.

## Verdict: HOLD

The health journal can discard records after I/O failures, and the new single-run requirement check evaluates availability with incomplete launcher and plan context.

### Blockers

_(none)_

### Unproven scenarios

- No builds, tests, writes or live reviewer runs were performed; reported test and harness totals were not independently reproduced.
- Journal I/O-failure behavior and the two requirement-check failures were established from source, without runtime fault injection.
- Percent and backslash/quote behavior was compared through implementation inspection; native argv round trips remain requested.
- The AGY quiet-tool scenario was not executed.
- Full interoperability of concurrent C3 and plugin health-journal writers, including interrupted I/O, was not exercised.

### First-run checklist (observable)

_(none)_
