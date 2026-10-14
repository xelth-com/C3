# Handoff 13 - Codex: panel-a-measure-zai

Date: 2026-10-14 19:46 local. Author: Codex (model glm-5.3, effort high), Codex CLI 0.155.1.
Reviewer: ZAI :: glm-5.3 (provider from roster, model from roster; endpoint https://api.z.ai/api/v1, wire_api: responses; provider fingerprint 29edbb79ed7d; harness codex-cli 0.155.1).
Preflight: ok: env ZAI_API_KEY set.
Effort: high sent (requested high, mapping zai-v1, by caps-v1: api.z.ai, glm-5.3; not confirmed by the provider). Consultation id: f93539f5-2a97-4326-8b77-2e6bd870490d.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `codex exec --sandbox read-only --color never --json -m glm-5.3 -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="ZAI" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-7090593d60954ff5b3226d2295367e07.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread; no thread of lineage ZAI :: glm-5.3 in this task's ledger; other lineage(s): gemini :: gemini-3.8-flash-high [agy], ZAI-claude :: glm-5.3 [claude], byteplus :: dola-seed-2.0-pro, byteplus :: deepseek-v4.1-flash, meta :: muse-spark-1.3-contributor [muse], alibaba :: qwen3.8-max, openai :: gpt-6-astra, mimo-claude :: mimo-v2.6-pro [claude], kimi :: k3). Result thread: `01a13b82-9ec4-7220-96db-d27c885f98b8` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md` (sha256 698cd5836539). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 235.9 s. Tokens: in 577363 (cached 476992), out 9731, reasoning 5112.
Quota (rollout): no window in the rollout; this run 577363 input tokens (476992 cached) in 8 calls, max 90081 per call of 996147; sub-agents 0; sub-agents off (features.multi_agent=false); budget 85 % of the 5-hour window.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - This is a framing consultation on measurement designs not yet implemented, so advice rather than acceptance. Findings: 0 blocker, 3 major, 3 minor, 0 note (F13-1..F13-6, tracked in `findings.json`). Structured reply: `handoffs/13-codex-panel-a-measure-zai.reply.json`.
Raw event stream: `handoffs/13-codex-panel-a-measure-zai.events.jsonl`.
Verbatim reply follows.

---

## Framing first

Three designs the brief does not list:

1. **Assignment, not just effort, in the event.** Without a field saying how the tier was chosen (`routed|preset|alternate|random`), the data is observational and confounded: the routed panel already seats strong reviewers on hard tasks, so high effort will look inefficient per unit cost. Alternating two efforts inside one purpose is the only causal design.
2. **Marginal, paired value.** A panel already puts several reviewers on the same brief — free natural pairs. Score the *increment* of high over medium on the same brief, not absolute usefulness per cost.
3. **Recall ground truth.** Your own waves show one escaped defect per round (F46-1…F62-1). An "escaped defect" — found by a later round after an accept — is the false-negative signal; usefulness marks cannot measure recall.

To choose I need: z.ai's real tier surface; the exact SubagentStop payload; whether astra `medium` survives a reading pass (only a paired run answers); expected event volume per install.

## Q1 — tiers and denominator

- **gpt-6-astra**: chore `low`; checkpoint/reading pass `medium` (until paired data says otherwise); diff-review and acceptance `high`; core-contract/stuck `xhigh`.
- **glm-5.3**: chore `low` (or a flash model — cheaper than any effort knob); checkpoint `high` (z.ai has no medium, the mapping makes "medium" meaningless there); diff-review `high`; acceptance and stuck `max`.
- Roster entries should **override** the existing purpose presets, never duplicate them, or two sources of truth drift.
- **Denominator**: there is no single one. Window share exists only for the ChatGPT endpoint; GLM's scarce resource is money; workers' is coordinator wall time and quota. Record the raw components per event (window-share delta, tokens, wall seconds — money is not observable client-side) and compute ratios at the intake. Numerator: confirmed findings (implemented/verified), severity-weighted, plus escapes as recall. Window share is a routing constraint, not a universal score.

## Q2 — worker capture, privacy, attribution

SubagentStop is the right primary point — the only zero-token, host-native one — but it fires per stop (dedupe by agent id; count resumes) and never fires on process kill or crash, so the worst outcomes vanish unless a SessionStart backfill sweeps interrupted agents and emits `outcome: unknown`. Privacy: never the brief, transcript/session ids, paths, cwd, repo or agent names as typed, tool outputs, free-text notes (drop `--note` from the event), or unbucketed token counts. Effort must carry its source (`definition|override|coordinator|unknown`) — the agent file's `effort:` may not be what was sent; `unknown` is data, not failure. Attribution under rework: never binary. Closed classes `first-attempt accepted | accepted after rework | findings survived | superseded`, the coordinator's mark, and `rating_rev` (already in the rating event) for revisions; join to the findings store to measure survival of the worker's own findings.

## Q3 — schema fields to add now

Consult/rating: `effort_sent` (none|low|medium|high|xhigh|max|unknown), `effort_requested`, `effort_source`, `assignment`, a pseudonymous `cohort_ref` (random, consult_ref-style) grouping same-brief panel members, and the quota delta components. Worker event: `host` (claude-code|codex-cli|other), `tier`, `model` (closed list), `effort`+source, `task_class` (closed), `outcome`, duration/token/tool buckets, `resumes`, `rework`, `verified`, mark+`rating_rev`. Principles: components not ratios; every enum ships with `unknown`/`other` from day one; version the vocabularies.

## Q4 — scout and judge

It fails when the digest is semantic: a summary states what code "does" and drops line-level facts — precision, ordering, error-to-absence — exactly where your real defects lived (F58/F60/F62 were ordering and sub-second precision details a summary flattens). The scout is also a correlated single point of failure: its blind spot becomes the judge's. Cap the structure, not just words: fixed slots per file (path, hunk ranges, stat deltas, "what I did not read"), a token cap that drops prose before it drops hunk references, and the judge always keeps read access to the raw diff — the digest prioritizes, it must never replace the code.

## Requested checks

- **RC1** (repo root, read-only, 5 min): `curl -s https://docs.z.ai/api | Select-String -Pattern "thinking|effort" -Context 0,2` — whether glm-5.3 exposes more than low/high/max (Q1).
- **RC2** (repo root, read-only, 5 min): `curl -s https://docs.claude.com/en/docs/claude-code/hooks | Select-String -Pattern "SubagentStop" -Context 2,12` — the exact stdin fields, effort visibility (Q2, finding #5).
- **RC3** (repo root, read-only, 2 min): `c3 findings` — per-finding status/rating availability for the confirmed-findings join (finding #2).
- **RC4** (repo root, read-only, 2 min): `Get-ChildItem .collab -Recurse -Filter sessions.json | Select-String -Pattern '"quota"|"tokens"' | Select-Object -First 10` — which cost components are already recorded (finding #2).

---

### Findings

- **F13-1** [major] `crates/c3/src/telemetry/event.rs:58`, `crates/c3/src/telemetry/classes.rs:323` - Adding effort to the events without an assignment field yields confounded observational data: the routed panel assigns harder briefs to stronger reviewers, so tier-versus-cost comparisons will systematically penalize the higher effort. Trigger: Any intake table computed from effort-tagged events once the routed panel is the default seating. Evidence: read-code: Details carries engine/provider/model/purpose/outcome/tokens but no assignment or routing-provenance field.; read-code: The panel seats reviewers by track record, so task difficulty and reviewer/tier choice are correlated by construction. Verify: Compare two same-brief panel members at different efforts via cohort_ref over ~20 rounds; without an assignment field the intake cannot distinguish routed from randomized pairs. Remedy: Add a closed `assignment` field (routed|preset|alternate|random|unknown) now, and run alternating-effort pairs inside one purpose for the first months.
- **F13-2** [major] `crates/c3/src/telemetry/event.rs:58`, `crates/c3-core/src/quota.rs:45` - No single cost denominator exists: window share applies only to the ChatGPT/codex endpoint, money is not observable client-side, and workers consume wall time and coordinator quota; baking any one ratio into the schema or score is wrong for the other classes. Trigger: Scoring GLM (no 5-hour window) or Claude workers with the window-share denominator. Evidence: read-code: Quota constants and rollout reading model the ChatGPT 5-hour/weekly windows only.; read-code: The event already carries raw tokens and wall seconds — components, not ratios. Verify: Attempt to compute one comparable score for one astra and one glm consultation from the current ledger fields; the GLM row has no window component. Remedy: Record components per event (primary/secondary window-share delta, tokens, wall seconds) and compute per-vendor ratios only at the intake.
- **F13-3** [major] `crates/c3-core/src/quota.rs`, `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md` - A semantic scout digest structurally hides the defects this project actually produces: the wave's confirmed findings were ordering, precision and error-to-absence details that live at hunk level and are flattened by any summary. Trigger: A scout digest written as prose about what the diff does, fed to a judge with no raw-diff access. Evidence: read-code: The bridge's own reading discipline mandates hunk-by-hunk reading precisely because whole-file/summary reading misses such defects.; read-code: Confirmed defects F58-1/F60-1/F62-1 were sub-second-instant and ordering details a summary would drop. Verify: Feed one past wave's digest-only summary of the 8i bug to a judge without the diff; it cannot reproduce F62-1. Remedy: Cap the digest's structure (path, hunk ranges, stat deltas, unread list) with prose dropped before hunk references; the judge keeps read access to the raw diff.
- **F13-4** [minor] `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md` - SubagentStop alone undercounts the worst worker outcomes: it never fires on process kill, crash or host exit, so error/abandoned runs are silently missing unless a later SessionStart backfill emits them as outcome unknown. Trigger: A host crash or forced tree kill while workers run. Evidence: read-code: The sketch records one event per subagent end via the hook but specifies no backfill for missed stops.; inferred: Stop-type hooks fire on normal subagent completion, not on host process death. Verify: Kill the host mid-worker-run, restart, and check that a backfilled unknown-outcome event exists. Remedy: Ship a SessionStart sweep of unreported agent ids that emits a closed `outcome: unknown` event before new work starts.
- **F13-5** [minor] `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md` - The agent definition's effort frontmatter may not equal the effort actually sent (a call-site override can win and may not reach the transcript), so a bare effort field would silently mislabel rows; the sketch's coordinator-side --effort fallback is post-hoc and unverifiable. Trigger: A worker spawned with an explicit effort override differing from its agent definition. Evidence: read-code: The plan itself flags 'an explicit effort override of the Agent call may not be in the subagent transcript' as an open question. Verify: Run one subagent with a call-site effort override and diff the hook's inferred effort against the CLI's resolved effort line. Remedy: Record `effort` plus a closed `effort_source` (definition|override|coordinator|unknown); treat unknown as a valid dimension value, never coerce it.
- **F13-6** [minor] `README.md` - Per-purpose effort policies on roster entries duplicate the existing purpose presets, creating two sources of truth that will drift (e.g. a preset raised to high while a roster entry still says medium). Trigger: Any later change to the purpose presets after roster entries carry their own per-purpose effort maps. Evidence: read-code: Purpose presets and per-entry effort both exist as separate configuration surfaces today. Verify: Change one preset and grep the roster file for stale per-entry values that now disagree. Remedy: Roster entries may only override a preset for a named purpose, with the preset as the single default and the override surfaced in `c3 providers`.

### Prior findings

_(none)_

## Verdict: ADVISE

This is a framing consultation on measurement designs not yet implemented, so advice rather than acceptance.

### Blockers

_(none)_

### Unproven scenarios

- z.ai's exposed effort tier surface for glm-5.3 beyond low/high/max
- the exact Claude Code SubagentStop stdin payload and whether it exposes tier/effort reliably
- whether astra medium is sufficient for reading passes (needs paired runs, not read-only inspection)
- intake-side capacity and retention for the proposed worker-event volume

### First-run checklist (observable)

_(none)_
