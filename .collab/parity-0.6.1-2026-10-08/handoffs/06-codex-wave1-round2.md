# Handoff 06 - Codex: wave1-round2

Date: 2026-10-08 22:13 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 915a9525-065a-434d-9088-096229935bfd.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-3a24b40d2fb94e81861ca310a13aa94c.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json fork 01a11cd9-cf5b-7ba1-84e8-5e5cef6ccc9d -` (prompt on stdin).
Parent thread: `01a11cd9-cf5b-7ba1-84e8-5e5cef6ccc9d`. Result thread: `01a11d26-1feb-79f1-b2b2-b755bd6bb393` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/05-claude-wave1-round2.md` (sha256 765345da2da3). Reviewed: add198b, base add198b7b149356a21a6ec4e7e4cec8665475ad5, tree sha256 ef3ddd0088d7, 0 changed files.
Bridge outcome: usable reply. Wall time: 316.8 s. Tokens: in 5020697 (cached 4421888), out 19635, reasoning 3332.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `bd96c44..add198b` - the range changes 5 files, 620 lines (564 insertions, 56 deletions).
Verdict: ACCEPT - F04-1 through F04-3 are fixed; the remaining historical Windows time-zone edge case is a nonblocking limitation of the normalization. Findings: 0 blocker, 0 major, 1 minor, 0 note (F06-1, tracked in `findings.json`). Structured reply: `handoffs/06-codex-wave1-round2.reply.json`.
Raw event stream: `handoffs/06-codex-wave1-round2.events.jsonl`.
Verbatim reply follows.

---

**Q1.** The round-trip rule fixes F04-2’s Berlin boundaries and preserves genuine repeated-hour candidates when the UTC lookup is accurate. The new synthetic and production-Local tests address the previous coverage gap.

There is one historical counterexample: Windows Samoa rules around New Year 2012 can make the UTC lookup disagree with a valid local offset. The new normalization then invents a gap and selects an already-passed reset instead of tomorrow’s midnight (finding #1). I found no specific current or future instance, so this does not block the 2026 wave. Avoid claiming unrestricted historical-zone equivalence.

**Q2.** I found no remaining pre-routing-state serialization in the requested paths. Ledger member records and `roster.skipped` now use routed states joined by position. Detached status starts from selected runners, receives execution-state updates, and `--status` renders that saved record. The timeout exceptions likewise follow selected seat order.

The new real fake-CLI test checks the required light reviewer in every member’s ledger entry and checks that `roster.skipped` is empty. It does not itself exercise detach/status or an unselected member, but the inspected paths use the correct sources. F04-3 is fixed. F04-1’s boundary-safe suffix check and direct classifier/scan fixtures also resolve the original panic.

**Q3. ACCEPT** wave 1 into the compatibility track. Close F04-1, F04-2, and F04-3. The deferred telemetry, oracle, and shared-plan findings remain open and are outside this acceptance scope.

This is based on source review, the checked-in regressions, and the supplied run results. I did not rebuild or rerun the Rust suites. The Windows Local test can skip on other zones, so its explicit execution evidence matters more than the aggregate passing count.

## Requested checks

- **RC1:** CWD repository root; **workspace-write**; add a synthetic `ResetZone` reproducing Windows Samoa’s 2011/2012 local-year versus UTC-year lookup, then test `retry_after_in("try again at 12:00 AM", "2012-01-01T14:01:00+14:00", zone)`. Require valid midnight to retain `+14:00` and the result to be `2012-01-02T00:00:00+14:00`, matching the pinned plugin (finding #1). Budget: one fixture, 15 minutes; follow-up, not an acceptance prerequisite.

---

### Findings

- **F06-1** [minor] `crates/c3-core/src/health.rs:253`, `crates/c3-core/src/health.rs:350` - The new normalization can discard a genuine historical Windows local offset when chrono's UTC lookup selects a different year's rules, causing a valid reset time to be treated as a gap. Trigger: Using Windows Samoa Standard Time with reference 2012-01-01T14:01:00+14:00, parse 'try again at 12:00 AM'. This requires a historical or simulated reference; no specific current or future trigger was established. Evidence: read-code: Normalization rejects an offset unless the UTC lookup returns it. If every candidate is rejected, the caller advances minute by minute until it finds a supposedly valid wall time.; read-code: The Windows UTC lookup selects time-zone rules using the UTC year, which can differ from the local year used to classify the original wall time.; ran-command: The 2011 record has Bias 660 and daylight bias -60; the 2012 record has Bias -780 and daylight bias -60. .NET classifies 2012-01-01 midnight as valid and unambiguous at +14:00.; ran-command: The reference implementation returned 2012-01-02T00:00:00.0000000+14:00 for the trigger.; inferred: The genuine +14 midnight candidate maps to UTC in 2011 and is rejected against the -10 rules. The alternative -10 candidate maps into 2012 and is also rejected. The artificial gap ends at 14:00; the five-minute allowance accepts that already-passed instant at reference 14:01, yielding 2012-01-01T14:00:00+14:00 instead of tomorrow's midnight. Verify: Add a source-faithful synthetic Samoa year-boundary fixture and assert both the valid +14 midnight classification and the pinned plugin's next-midnight result. Remedy: Correct the Windows UTC lookup used as the normalization authority for cross-year rule changes, or explicitly bound the supported historical scope and retain the counterexample as a regression fixture; blindly retaining raw candidates would reintroduce F04-2.

### Prior findings

- F02-1 - still-open - Spool synchronization is unchanged in this range and remains assigned to wave 2.
- F02-2 - still-open - Closed telemetry classifiers are unchanged and remain assigned to wave 2.
- F02-3 - still-open - Failed-deletion retry identity is unchanged and remains assigned to wave 2.
- F02-4 - still-open - The complete harness assertion coverage map and telemetry shim are not delivered by this range.
- F02-5 - fixed - The state still correctly acknowledges the existing commit-before-spool ordering.
- F02-6 - fixed - The previously reviewed anchor implementation is unchanged.
- F02-7 - still-open - Shared plan infrastructure is assigned to wave 1b and is outside the reviewed range.
- F02-8 - fixed - Decision P4 retains spawned Claude for parity and gates native Messages on its own benchmark.
- F04-1 - fixed - str::get guards the suffix boundary before slicing; new fixtures cover Unicode names in the helper, classifier, and both scan variants.
- F04-2 - fixed - The reported Berlin boundary errors are corrected and covered by synthetic, IANA, and conditional production-Local tests; the supplied report confirms the production test ran. Finding #1 records a distinct historical limitation.
- F04-3 - fixed - Member and skipped records now use final routed states. The real fake-CLI integration test asserts correct state and empty skip records in every member's ledger entry.

## Verdict: ACCEPT

F04-1 through F04-3 are fixed; the remaining historical Windows time-zone edge case is a nonblocking limitation of the normalization.

### Blockers

_(none)_

### Unproven scenarios

- The reported 542 passing tests and harness results were not independently rerun during this read-only review.
- Detached-panel status and unselected-member serialization were traced in code but not exercised live.
- The historical Samoa C3 result is inferred from the changed code and locked chrono implementation; the pinned plugin result was executed in memory.
- The production Windows Local regression test conditionally skips outside the matching zone; general passing test totals do not establish that it ran on every machine.

### First-run checklist (observable)

_(none)_
