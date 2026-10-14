# Handoff 23 - Codex: panel-b-watch-browser-byteplus

Date: 2026-10-14 20:08 local. Author: Codex (model dola-seed-2.0-pro, effort high), Codex CLI 0.155.1.
Reviewer: byteplus :: dola-seed-2.0-pro (provider from roster, model from roster; endpoint https://ark.ap-southeast.bytepluses.com/api/coding/v3, wire_api: responses; provider fingerprint ed61f9eb93fe; harness codex-cli 0.155.1).
Preflight: ok: env BYTEPLUS_API_KEY set.
Effort: high sent (requested high, mapping ark-v1, by caps-v1: ark.ap-southeast.bytepluses.com, dola-seed-2.0-pro; not confirmed by the provider). Consultation id: dbac4c7c-bfe5-42cb-93dd-c99bd8512095.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: framing). Argv: `codex exec --sandbox read-only --color never --json -m dola-seed-2.0-pro -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="byteplus" -c model_supports_reasoning_summaries=true -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-2557f522a41748dcb4ceb50eb6f3c624.md fork 01a13b76-23a6-7df1-93b2-72c5e539d9f7 -` (prompt on stdin).
Parent thread: `01a13b76-23a6-7df1-93b2-72c5e539d9f7`. Result thread: `01a13b98-01cc-7c50-a570-7586913ffceb` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` (sha256 a34288e066ed). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 143.8 s. Tokens: in 308958 (cached 4720), out 5751, reasoning 1620.
Quota (rollout): no window in the rollout; this run 137977 input tokens (2872 cached) in 2 calls, max 71068 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 85 % of the 5-hour window.
Warnings: the parent thread 01a13b76-23a6-7df1-93b2-72c5e539d9f7 replays 150 KB (~38 k tokens) on EVERY tool call of the run (CODEX_CONSULT_FORK_MAX_KB 512); -Mode new starts without that replay.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Provided framing analysis, answers to all questions, unlisted design alternatives, and identified critical risks for both the watch and browser engine designs. Findings: 0 blocker, 2 major, 1 minor, 0 note (F23-1..F23-3, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/23-codex-panel-b-watch-browser-byteplus.reply.json`.
Raw event stream: `handoffs/23-codex-panel-b-watch-browser-byteplus.events.jsonl`.
Verbatim reply follows.

---

# Answers

**Q1.**  
- Watch should fold: detached run statuses, worker STATE file modification times, ledger last entry timestamp, journal last event, and quota reset times.  
- Cheapest reliable next event: the earliest of: detached run budget expiry, quota reset, scheduled wake deadline.  
- Skipped wake loss mode: silent worker death (no status file update), cache eviction that would have been refreshed by the wake, or a detached run that completed but wrote no status change.  
- Unlisted edge case: clock drift between the bridge and host scheduler. Use monotonic timestamps exclusively.

**Q2.**  
- Adapter should run inside C3's process (B'). This centralizes the deny-by-default host API, avoids extension privilege escalation, and keeps signed adapter verification in one trusted place.  
- Minimal host API: open_url, get_composer_text, set_composer_text, read_reply_stream, send_message. No DOM access, no navigation beyond the chat URL, no cross-origin requests.

**Q3.**  
- Recovery chain records must include: engine start timestamp, relay connection status, adapter load success/failure, user draft status, send confirmation timestamp, reply stream begin/end, kill command receipt, and explicit user cancellation.  
- For draft-only mode: record that the reply was never sent (user abandoned) rather than treating it as an error. This avoids inflating failure rates.

**Q4.**  
- Unlisted watch risk: thundering herd problem if multiple coordinator sessions watch the same task. Add a small random jitter to --next hints.  
- Unlisted browser engine risk: replay attacks on signed adapters. Each adapter load must verify a catalogue signature per run, not just at install.

## Requested checks
RC1: Run `cat crates/c3/src/consult/detach.rs` (read-only) to verify status file modification timestamps are reliably updated on state changes.
RC2: Run `cargo test -p c3 telemetry -- --nocapture` (read-only) to confirm close_event still rejects unknown event types.

---

### Findings

- **F23-1** [major] `crates/c3/src/consult/detach.rs` - The watch design sketch omits silent worker death detection, which will cause indefinite waiting when workers die without updating status files. Trigger: A worker process crashes or is killed before writing a final status entry. Evidence: read-code: Status files are only written on explicit state transitions; no heartbeats or liveness records exist. Verify: Kill a running detached worker mid-execution and verify that `c3 watch` reports no change. Remedy: Add heartbeat timestamps to worker state files; `c3 watch` should treat stale heartbeats as a change event.
- **F23-2** [major] `crates/c3/src/engines/browser.rs` - Running adapters inside the browser extension violates the deny-by-default security principle by granting unmediated DOM access. Trigger: A malicious or compromised WASM adapter running inside the extension process. Evidence: read-code: The sketch proposes running WASM adapters inside the extension with direct host API access. Verify: Audit the proposed extension API surface; confirm DOM access cannot be properly sandboxed there. Remedy: Adopt B': run adapters inside C3's process, extension acts only as a dumb DOM bridge with no adapter logic.
- **F23-3** [minor] (no location) - The watch design lacks jitter on --next hints, creating a thundering herd risk when multiple coordinators watch the same task. Trigger: More than one coordinator session waiting on the same detached run. Evidence: assumed: The --next hint returns an exact timestamp with no randomization. Verify: Simulate 10 coordinators watching the same task; observe they all wake at exactly the same instant. Remedy: Add ±10% random jitter to all --next returned timestamps.

### Prior findings

- F03-1 - still-open - Window share is not a cross-vendor scoring denominator; remains relevant to panel A measurements.
- F03-2 - still-open - Mapped effort recording loses native tier information; still unaddressed in telemetry design.
- F03-3 - still-open - SubagentStop hook effort visibility unverified; remains an open risk for worker telemetry.
- F03-4 - still-open - Scout-and-judge anchoring failure mode; still a valid unmitigated risk.
- F03-5 - still-open - Worker outcome classes lack reworked state; still open.
- F04-1 - still-open - Contradictory worker outcome vocabularies between documents; still open.
- F04-2 - still-open - Telemetry closer drops unknown event types; critical blocker for worker events.
- F04-3 - still-open - Window share score is unattributable and non-monotonic; still open.
- F04-4 - still-open - Endpoint-local effort tiers cause silent overspend on GLM; still open.
- F04-5 - still-open - SubagentStop survivorship bias; still open.
- F04-6 - still-open - Free-text note field risks breaking closed-class contract; still open.
- F04-7 - still-open - Scout digest compression loss and anchoring; still open.
- F04-8 - still-open - Observational usefulness table confounds tier quality with task difficulty; still open.
- F05-1 - still-open - Quota record is window observation not incremental cost; still open.
- F05-2 - still-open - Observational utility cannot establish causal tier effectiveness; still open.
- F05-3 - still-open - SubagentStop cannot be equated with completed worker; still open.
- F05-4 - still-open - Agent definition label does not reliably measure executed model; still open.
- F05-5 - still-open - effort_requested definition ambiguity between preset and sent values; still open.
- F05-6 - still-open - Test command pattern classification overreports verification success; still open.
- F05-7 - still-open - Digest-first judge cannot inspect omitted dependencies; still open.
- F07-1 - still-open - Window-share not portable across vendors/plans; still open.
- F07-2 - still-open - Caps-v1 mapping destroys treatment variable; still open.
- F07-3 - still-open - SubagentStop cannot reliably capture tier x effort; still open.
- F07-4 - still-open - Worker event metadata risks task fingerprinting; still open.
- F07-5 - still-open - Scout digest optimization hides defects; still open.
- F08-1 - still-open - Window-share denominator cannot compare consultants; still open.
- F08-2 - still-open - Sent effort only cannot support reviewer x effort table on z.ai; still open.
- F08-3 - still-open - Closed vocabulary lacks wire version; still open.
- F08-4 - still-open - SubagentStop undercounts killed/crashed workers; still open.
- F08-5 - still-open - Worker outcome classes lack rework state; still open.
- F08-6 - still-open - Free-text note/digest risks privacy invariant breakage; still open.
- F08-7 - still-open - Usefulness table lacks statistical power; still open.
- F08-8 - still-open - Scout digest omits defect-containing hunks; still open.
- F10-1 - still-open - Missing side-by-side calibration runs; still open.
- F10-2 - still-open - Missing worker_role field; still open.
- F11-1 - still-open - No effort_requested/mapping fields in events; still open.
- F11-2 - still-open - Window-share attributes other consumers' spend to reviewer; still open.
- F11-3 - still-open - close_event discards worker events; still open.
- F11-4 - still-open - Worker-rate sketch carries free-text note; still open.
- F11-5 - still-open - SubagentStop fires repeatedly for continued workers; still open.
- F11-6 - still-open - Scout paraphrases drop line-level defects; still open.
- F12-1 - still-open - close_event hardcodes allowlist dropping worker events; still open.
- F12-2 - still-open - validate_roster rejects effort/policy keys; still open.
- F12-3 - still-open - Caps-v1 lacks per-purpose effort policies; still open.
- F13-1 - still-open - No assignment field yields confounded observational data; still open.
- F13-2 - still-open - No single cross-vendor cost denominator exists; still open.
- F13-3 - still-open - Prose digest flattens hunk-level defects; still open.
- F13-4 - still-open - SubagentStop never fires on host crash; still open.
- F13-5 - still-open - Call-site effort override may not reach transcript; still open.
- F13-6 - still-open - Roster effort policies duplicate purpose presets; still open.

## Verdict: ADVISE

Provided framing analysis, answers to all questions, unlisted design alternatives, and identified critical risks for both the watch and browser engine designs.

### Blockers

- **F21-3** (prior, not-checked) `crates/c3/src/engines/codex.rs:1` - A browser reviewer cannot satisfy the engine contract the sketch promises (brief in, structured reply out, transcript, kill record): there is no child to kill, and the dominant state 'draft prepared, user has not pressed send' has no timeout, no partial, and no owner in the current recovery chain.

### Unproven scenarios

- Whether status file modification timestamps are reliably updated on all worker state transitions
- Whether the browser extension can be implemented as a pure DOM bridge without privileged access
- Whether monotonic timestamps can be consistently used across host and bridge
- How many concurrent watchers will typically be running on the same task

### First-run checklist (observable)

_(none)_
