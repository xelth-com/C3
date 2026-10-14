# Handoff 12 - Gemini (agy): panel-a-measure-gemini

Date: 2026-10-14 19:41 local. Author: Gemini (agy) (model gemini-3.8-flash-high, effort tier in the model id), agy-cli (version unknown).
Reviewer: gemini :: gemini-3.8-flash-high [agy] (provider from roster, model from roster; engine agy (C:\Users\Dmytro\AppData\Local\Microsoft\WinGet\Packages\Google.AntigravityCLI_Microsoft.Winget.Source_8wekyb3d8bbwe\agy.exe); provider fingerprint a612ba12e63e; harness agy-cli (version unknown)).
Preflight: ok: signed in (6 models).
Effort: nothing sent (requested high, mapping model-tier, by caps-v1: engine agy, the tier is part of the model id; not confirmed by the provider). Consultation id: e5e0699c-e2de-4ac7-a70a-aa72a847337b.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `agy -p= --input-format stream-json --output-format stream-json --model gemini-3.8-flash-high --json-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --print-timeout 0 --sandbox --disable-slash-commands` (prompt on stdin as one NDJSON line).
Parent thread: (none - new thread; no thread of lineage gemini :: gemini-3.8-flash-high in this task's ledger; other lineage(s): alibaba :: qwen3.8-max). Result thread: `eea5ab20-5d3a-4a0d-8678-3fdb7cda8198` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md` (sha256 698cd5836539). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 636.5 s. Tokens: in 545368 (cached 3561861), out 27648, reasoning 22358.
Engine turns: 1.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Scoring must decouple subscription window congestion from API token costs, replace prose scout digests with structural slices, and validate worker telemetry through git survival metrics before publishing the closed schema. Findings: 0 blocker, 1 major, 1 minor, 1 note (F12-1..F12-3, tracked in `findings.json`). Structured reply: `handoffs/12-agy-panel-a-measure-gemini.reply.json`.
Raw event stream: `handoffs/12-agy-panel-a-measure-gemini.events.jsonl`.
Verbatim reply follows.

---

## Framing Review: Measuring Reviewers and Workers

The brief assumes reviewer tiers can be scored under a common denominator and that scouts can reliably summarize diffs for judges. Both assumptions break under operational reality.

### Framing Challenges & Unlisted Options
1. **Asymmetric Currency Fallacy**: Astra is bottlenecked by a non-linear 5-hour rolling quota window under fixed subscription cost; GLM is bottlenecked by linear API credits without rolling limits. Dividing GLM by window share is undefined; dividing Astra by fiat cost yields zero marginal cost until abrupt 98% lockout.
2. **The Summarization Blindspot**: Subtle bugs (e.g. F46–F62 timestamp precision and byte-slicing) look like ordinary code. A cheap scout compresses away the exact edge case, blinding the judge.
3. **Unlisted Option: Deterministic AST Slicing**: Instead of prose digests, the scout produces structured slice manifests (call-chains, touched invariants, hunk boundaries).
4. **Unlisted Option: Code-Survival Attribution**: Instead of subjective coordinator marks, measure worker quality by git hunk survival rate in the final merge.

---

### Q1. Effort Tiers and Scoring Denominator
**Effort per purpose**:
- **gpt-6-astra**: 
  - *Chore*: **Never route to Astra** (burns 10–23% window for trivial queries; route to GLM low or local grep). If forced: `low`.
  - *Checkpoint*: `medium`.
  - *Framing / Decision / Diff-review*: `high` (wave 8 proved `high` is necessary to detect subtle invariants).
  - *Acceptance / Core-contract / Stuck*: `xhigh` (or `high` with strict reading discipline).
- **glm-5.3**:
  - *Chore*: `low`.
  - *Checkpoint / Framing / Decision*: `high` (caps-v1 maps medium -> high).
  - *Diff-review / Acceptance / Core-contract / Stuck*: `max` (GLM is cheap per token; reducing effort saves pennies while sacrificing defect discovery).

**Denominator**:
Use **Dual-Currency Cost per Verified Finding (CPF)**:
- Astra: Verified Findings / (Δ 5h-window share % × congestion factor).
- GLM: Verified Findings / Dollar spent.
Wall-time must act as an SLA timeout filter, not an efficiency denominator.

**Unlisted Design**: **Two-Pass Escalation Ladder**. Run GLM at `low` (or Astra `medium`) first; escalate to Astra `high`/`xhigh` only if diff-review yields an unverified `ACCEPT` or ambiguous warnings.

### Q2. Worker Telemetry & Attribution
- **Capture Point**: `SubagentStop` is necessary but insufficient: it misses context thrashing, mid-flight stall loops, and unhandled crashes. It must pair with `SubagentStart` to track orphaned workers.
- **Privacy Constraints**: Strictly exclude task slugs, prompt text, file paths, branch names, bash strings, diffs, and error logs. Emit only closed enums: `worker_tier`, `model_token`, `duration_bucket`, `tokens_bucket`, `outcome_class`, `edits_class`.
- **Attribution on Rework**: Replace subjective `c3 workers rate` with **Git Hunk Survival Ratio**:
  - *Clean*: ≥ 80% hunks survive in coordinator commit.
  - *Assisted*: 20–79% hunks survive.
  - *Discarded*: < 20% hunks survive (docking worker tier score).
  Correlate with subsequent reviewer findings on touched hunks.

### Q3. Closed Vocabulary Additions
Add immediately:
1. `Event` (consultation): `effort_sent` (`none|low|medium|high|xhigh|max`), `effort_requested`, `quota_window_bucket` (`none|0-25|25-50|50-80|80-98|98-100`), `cache_ratio_bucket` (`0-25|25-50|50-75|75-100`).
2. `RatingEvent`: `effort_sent` (enables rating analysis without joining).
3. `WorkerEvent`: `worker_tier` (`haiku|sonnet|opus|fable|other`), `worker_model`, `effort` (`none|low|medium|high|xhigh|max|unknown`), `task_purpose` (`chore|fix|feature|test|refactor|other`), `outcome` (`completed|stopped_coordinator|failed:timeout|failed:quota|failed:context|failed:other`), `rework_class` (`none|minor|major|discarded`), `verification_status` (`clean|failed|unrun`).

### Q4. Scout-and-Judge Routing
- **Failure Modes**: (1) *Compression loss*: scout omits subtle edge cases. (2) *Confirmation bias*: judge audits the scout's narrative rather than raw code.
- **Caps & Safeguards**:
  - Scout emits **structured AST index/slice maps only**, capped at 1,500 tokens / 100 lines.
  - **Judge Evidence Invariant**: Judge must read code via direct tool calls (`READING_DISCIPLINE`, max 200 lines/call); citing the digest as primary evidence fails validation.

## Requested checks

- **RC1**: Run `cargo test -p c3 telemetry::classes::tests::worker_event_dropped` from repo root (permission: read-only; budget: 30s) testing whether `close_event_body` drops `"event_type":"worker"` (finding #1). Settle when test confirms rejection.
- **RC2**: Run `cargo test -p c3-core roster::tests::reject_effort_key` from repo root (permission: read-only; budget: 30s) validating a roster entry with an `"effort"` key fails `validate_roster` with "unknown key 'effort'" (finding #2). Settle when refusal message is confirmed.

---

### Findings

- **F12-1** [major] `crates/c3/src/telemetry/classes.rs:870` - close_event in telemetry/classes.rs hardcodes an event_type check for consultation and rating only, silently discarding any queued worker event. Trigger: An event with event_type: "worker" is enqueued and processed by close_event_body during telemetry dispatch. Evidence: read-code: close_event returns None if event_type != "consultation" && event_type != "rating". Verify: Pass a valid JSON object with event_type: "worker" to close_event_body; verify that None is returned. Remedy: Add "worker" to the recognized event_type list and implement a dedicated closed-vocabulary classifier for worker telemetry events.
- **F12-2** [minor] `crates/c3-core/src/roster.rs:483` - validate_roster enforces a fail-closed allowlist of reviewer entry keys that rejects any effort or effort_policy property. Trigger: A reviewer entry in codex-consult-roster.json includes an "effort" or "effort_policy" key as proposed in the sketch. Evidence: read-code: iobj.keys() is strictly validated against an exact 14-string allowlist that does not include effort. Verify: Invoke validate_roster with a roster containing an "effort" dictionary on an entry; verify that an unknown key refusal is emitted. Remedy: Add "effort" to the allowed reviewer keys in validate_roster and define its schema parser.
- **F12-3** [note] `crates/c3-core/src/effort.rs:94` - Caps-v1 capability table defines static vocabulary identifiers without supporting per-purpose effort policies or fallback mappings. Trigger: Consultation orchestration attempts to resolve a per-purpose effort policy from caps-v1. Evidence: read-code: Caps struct only exposes vocabulary, models, and schema_transport without purpose-to-tier mappings. Verify: Inspect Caps definition in c3-core/src/effort.rs; verify absence of purpose-to-tier mappings. Remedy: Introduce purpose-aware effort resolution and fallback tables in c3_core::effort.

### Prior findings

_(none)_

## Verdict: ADVISE

Scoring must decouple subscription window congestion from API token costs, replace prose scout digests with structural slices, and validate worker telemetry through git survival metrics before publishing the closed schema.

### Blockers

_(none)_

### Unproven scenarios

_(none)_

### First-run checklist (observable)

_(none)_
