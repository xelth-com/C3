# Handoff 05 - Codex: panel-a-measure-openai

Date: 2026-10-14 19:33 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 27fa2dd2-050c-44ab-b4bf-93f4a0cc7a06.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-709ddf0a083b45ce8f52c5a4e0fe232c.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13b76-2a1b-7000-acd4-a39e3cb797fe` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md` (sha256 698cd5836539). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 251.7 s. Tokens: in 575705 (cached 474624), out 5573, reasoning 362.
Quota (rollout): 5h 83% until 21:23; wk 78% until Oct 20; this run 575705 input tokens (474624 cached) in 10 calls, max 87115 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 85 % of the 5-hour window.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Proceed with versioned measurement and controlled experiments, but correct quota attribution, worker identity, and outcome semantics before using telemetry for routing. Findings: 0 blocker, 6 major, 1 minor, 0 note (F05-1..F05-7, tracked in `findings.json`). Structured reply: `handoffs/05-codex-panel-a-measure-openai.reply.json`.
Raw event stream: `handoffs/05-codex-panel-a-measure-openai.events.jsonl`.
Verbatim reply follows.

---

The framing should optimize verified task outcomes under resource constraints, rather than assume a universal tier leaderboard. Missing options include deterministic scouting, local aggregation before upload, and randomized shadow evaluations that leave production routing unchanged. Choosing requires the task mix, tolerated misses, latency target, binding subscription limits, and coordinator rework cost.

**Q1.** Initial experimental defaults—not measured optima:

| Purpose | Astra | GLM-5.3 |
|---|---|---|
| Chore/extraction | low | low |
| Reading/checkpoint | medium | high |
| Framing/decision | high | high |
| Diff-review/acceptance | high | max |
| Core-contract/stuck | xhigh | max |

Treat reading as a stage, not another purpose. Escalate on unresolved evidence or risk, not just the purpose label. [Z.ai documents](https://docs.z.ai/guides/llm/glm-5.3) exactly three tiers. [OpenAI documents](https://developers.openai.com/api/docs/models/gpt-6-astra) Astra `max` too; that does not establish support through this bridge’s subscription route.

There is no universal denominator. Confirmed findings are benefits, not costs. Compare independently verified task quality against quota consumption, elapsed time, paid usage/points, and rework separately. Within an isolated subscription experiment, use total quality benefit divided by total attributable quota consumption, including failed attempts; track both windows. Current quota snapshots are not attributable consumption (finding #1). Deduplicate confirmed findings and measure misses on known-defect cases; `implemented` alone is not confirmation. Randomize paired trials by task class and report uncertainty and rating coverage.

**Q2.** Use command hooks at Start and Stop, plus reconciliation for missing endings; define an execution episode separately from a resumed worker. Current [hook documentation](https://code.claude.com/docs/en/hooks#common-input-fields) includes conditional `effort.level`, but Stop lacks a model field. Validate the installed host and resolve actual model identity locally; retain unknown effort instead of dropping workers.

Upload only reconstructed allowlisted classes/counts. Exclude raw hook JSON, prompts, messages, transcripts, paths, commands, errors, agent names, session/thread/agent IDs, credentials, and notes. Mint random worker references; keep their mapping local. Apply existing telemetry opt-out and deletion behavior. Other hosts can submit the same adapter-neutral record with explicit provenance and missingness.

Rate worker contribution separately from final wave success: accepted unchanged, minor rework, substantial rework, discarded, or unknown; record rework duration and independent verification separately. Command execution does not prove verification passed.

**Q3.** Promise versioned compatibility, not a vocabulary frozen forever. Add schema/capability/policy versions; requested and sent effort separately; confirmation state; policy source; resolved model class; work class/stage; measurement source/completeness; cost basis and attribution quality. Keep `none`, `unknown`, and `unsupported` distinct. Worker records need random reference, episode/revision, lifecycle outcome, duration/token buckets, rework class, verification result, and rating revision/judge. Copy effort metadata from the original run when rating. Aggregate finding outcomes locally against existing `consult_ref`; private finding IDs need not leave the machine.

**Q4.** No digest cap guarantees honesty. Start at 1,000 tokens containing evidence pointers, invariants, hypotheses, omissions, and uncertainty. Give the judge the original brief, diff inventory, and direct source access; require independent inspection of relevant boundaries. Budget exhaustion means incomplete coverage, never automatic acceptance. Compare digest-assisted review against independent review.

## Requested checks

- **RC1:** In a disposable workspace, capture local Start/Stop payloads for completion, resume, cancellation, and forced termination with differing parent/child settings. Permission: workspace-write. Budget: four cases/20 minutes. Settle findings #3–4 through event counts and model/effort provenance; upload nothing.
- **RC2:** In a disposable checkout, run eight frozen known-defect cases at two efforts in randomized order with isolated account usage and blind adjudication. Permission: workspace-write. Budget: 16 reviews, capped at 20 quota percentage points. Settle findings #1–2 through attributable cost, misses, precision, and rework.

---

### Findings

- **F05-1** [major] `crates/c3-core/src/quota.rs:830`, `.collab/parity-0.6.1-2026-10-08/state.md:208` - The existing quota record is an account-window observation, not the incremental quota cost of a consultation; using it directly, or taking uncontrolled before/after differences, can misrank effort tiers. Trigger: Other consumers use the account during a review, a reset occurs, or observations are stale or missing. Evidence: read-code: codex_run_quota merges window observations and serializes the selected observation. Baseline subtraction is applied to token totals, not quota percentages.; read-code: The coordinator records the account window rising from 8% to 72% without its runs and attributes this to another consumer. Verify: Run RC2 with before/after observations from the same reset interval and account isolation; separately classify reset crossings, missing observations, and concurrent usage. Remedy: Separate quota state from measured consumption. Record attribution quality and window identity locally; exclude contaminated or unavailable deltas from cost ratios rather than treating them as zero. Retain token, latency, and rework measurements.
- **F05-2** [major] `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md:51`, `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md:72` - A reviewer-by-effort usefulness table, or a count of implemented findings, cannot by itself establish which effort causes better outcomes. Trigger: Higher efforts receive harder tasks, routing preferentially selects past winners, only some runs receive ratings, or repeated findings and coordinator fixes receive credit. Evidence: read-code: The sketch assigns effort by purpose, splits usefulness by reviewer and effort, and proposes treating implemented or verified findings as hits.; inferred: Task difficulty and selection affect both effort assignment and observed outcomes; implementation status does not independently prove the original finding was correct. Verify: Run RC2 and compare the blind paired results with the observational ordering; include missed known defects and unrated runs. Remedy: Use randomized shadow comparisons on frozen inputs, stratify by work class and stage, deduplicate independently confirmed findings, and report rating coverage, uncertainty, false positives, and misses. Keep production telemetry descriptive until calibration supports routing.
- **F05-3** [major] `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md:57` - One SubagentStop invocation cannot safely be equated with one completed worker or used alone to estimate worker failure rates. Trigger: A worker resumes, a stop is blocked, an internal agent emits a stop event, or a process disappears without an observed ending. Evidence: read-code: The proposal describes one event and one hook invocation per worker.; ran-command: The documentation describes stops after responses, internal-agent stop events, resumable agents, and hooks that can prevent stopping.; inferred: Without a start inventory, missing stop events cannot be distinguished from workers that never existed. Verify: Run RC1 and compare observed starts, stops, and resumes with the four deliberately controlled execution histories. Remedy: Maintain a local lifecycle journal with execution episodes, deduplicate delivery, reconcile unmatched starts as unknown endings, and distinguish coordinator-spawned workers from other agent activity. Keep hooks silent and non-blocking.
- **F05-4** [major] `.collab/parity-0.6.1-2026-10-08/state.md:125` - Classifying model tier from names such as opus-worker measures the agent definition's label rather than reliably measuring the model that executed. Trigger: An agent definition changes model, inherits a model, uses an override, or runs through a floating alias. Evidence: read-code: The sketch derives tier from plugin agent names and separately proposes an actual-model field.; ran-command: agent_type identifies the agent name. SubagentStop has no model field; effort.level is conditional. Verify: In RC1, run a deliberately mismatched agent name and model, then compare the proposed tier with locally observed resolved model identity. Remedy: Keep role classification separate from model family. Derive tier from confirmed resolved model identity, with source and completeness fields; use unknown when unavailable. Verify whether observed effort belongs to the child execution before attributing it.
- **F05-5** [minor] `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md:51`, `crates/c3-core/src/ledger.rs:197` - The sketch ambiguously defines effort_requested as a preset name, whereas the existing ledger represents requested effort separately from sent effort and capability metadata. Trigger: A purpose default is overridden by roster policy or an explicit effort, or mapping sends a different endpoint tier. Evidence: read-code: The event proposal calls effort_requested the preset name.; read-code: The ledger distinguishes requested, sent, mapping, caps, and confirmation; the handoff example uses high as the requested value. Confirmation is reserved and currently null. Verify: Trace a checkpoint preset with an explicit effort override through launch, ledger, consultation event, and later rating; verify each field retains one consistent meaning. Remedy: Define effort_requested as the pre-mapping level, effort_sent as the emitted level, and policy_source/purpose separately. Preserve unknown confirmation and copy immutable execution metadata into rating events.
- **F05-6** [major] `.collab/parity-0.6.1-2026-10-08/state.md:130` - Recognizing a test-command pattern and labeling the worker verified can report successful verification when tests failed, never completed, or did not cover the result. Trigger: A transcript contains cargo test, pytest, or another recognized command whose result is failing, cancelled, or irrelevant to the delivered work. Evidence: read-code: The sketch maps the presence of verification-command patterns to verified.; inferred: A command name establishes an attempted action, not its exit status or adequacy. Verify: Feed a local classifier fixture containing a recognized test command with a failing exit status and require attempted/failed rather than verified. Remedy: Separate verification attempted, observed result, and independent acceptance. Use unknown for missing results and retain coordinator rework as a separate outcome.
- **F05-7** [major] `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md:52` - A digest-first judge constrained by a call cap can inherit the scout's omissions and issue an unsupported acceptance; reducing digest length does not resolve that risk. Trigger: The defect lies in an omitted dependency, invariant, error path, or unchanged caller, and the judge lacks sufficient source inspection budget. Evidence: read-code: The proposed judge receives a scout digest plus a call cap.; inferred: A lossy summary cannot establish absence of defects in evidence it omits; the cap can prevent independent recovery. Verify: Use a known-defect case whose relevant caller is omitted from the digest and check whether the judge independently finds it or explicitly reports incomplete coverage. Remedy: Provide the original brief, diff inventory, source access, explicit omissions, and evidence pointers. Require independent boundary checks and an incomplete outcome when the budget prevents them. Evaluate the combined workflow against independent review at equal total cost.

### Prior findings

_(none)_

## Verdict: ADVISE

Proceed with versioned measurement and controlled experiments, but correct quota attribution, worker identity, and outcome semantics before using telemetry for routing.

### Blockers

_(none)_

### Unproven scenarios

- No controlled effort comparison was run; the recommended defaults are experimental policies, not measured optima.
- The installed Claude Code version's hook payloads, cancellation behavior, and child-effort provenance were not exercised.
- Provider documentation does not prove effective effort handling on the exact installed CLI, subscription, and endpoint routes.
- The intake server's schema migration and unknown-field behavior were not inspected.
- No files were written, no implementation changes were made, and no live model calls or tests were launched.

### First-run checklist (observable)

_(none)_
