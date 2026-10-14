# Handoff 07 - Meta Muse (muse): panel-a-measure-meta

Date: 2026-10-14 19:31 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.7-R6101.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.7-R6101.1).
Preflight: ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: 21d5020c-f95d-488e-846a-c4c397d09f68.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-3d46fc52f0b9444890bb8bb8403e6edc.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Parent thread: (none - new thread). Result thread: `01a13b77-15ad-7453-b9d3-915bd935e8f6` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md` (sha256 698cd5836539). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 117.8 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: muse: warning: rules file at \\?\C:\Users\Dmytro\C3\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Framing needs a routing-first redesign with a portable cost vector and Start/Stop worker join before any closed-schema freeze. Findings: 0 blocker, 4 major, 1 minor, 0 note (F07-1..F07-5, tracked in `findings.json`). Structured reply: `handoffs/07-muse-panel-a-measure-meta.reply.json`.
Raw event stream: `handoffs/07-muse-panel-a-measure-meta.events.jsonl`.
Verbatim reply follows.

---

Framing challenge: the brief asks how to measure tiers, but the decision that locks in value is routing under scarcity, not scoring after months. Both sketches assume one closed table can serve billing, privacy, and learning; those goals conflict. Decide the routing policy first (what the bridge does when the window is hot), then derive the minimal telemetry that can falsify it.

**Q1. Tiers and denominator.** Do not assign fixed tiers per purpose yet; assign a default plus an exploration budget (e.g. 10% of reviews run one tier lower/higher). Unlisted designs: (a) calibration set — rerun a frozen defect-seeded diff monthly at each tier; (b) token-denominated cost as the cross-vendor score, window-share only as a same-account constraint; (c) usefulness per confirmed finding, not per mark. To choose I need: purpose mix shares, GLM price vs subscription shadow price, and whether z.ai tiers are speed- or quality-ordered (finding #2).

**Q2. Worker capture.** SubagentStop alone is the wrong point; use Start (intent: tier, effort, task class) joined by agent id to Stop (outcome, usage) plus a coordinator rework flag (used-as-is | reworked | discarded). Keep out everything in finding #4. Attribution: credit = worker outcome class × coordinator disposition, never a single usefulness bit. To choose I need: five real hook stdin samples and the Codex/Kimi equivalent signal.

**Q3. Schema.** Add now or break later: `effort_requested`, `effort_sent`, `caps_version`, `purpose`, `task_class` (review/worker shared enum), `disposition` (rework flag), `judge_source`, and a `schema_version` + reserved `other` on every new enum. Keep consult and worker events disjoint; never extend `agent_report`.

**Q4. Scout-and-judge.** It fails exactly where quotas bite: the scout drops the hunk containing the defect and the judge, call-capped and anchored, never re-opens it. A byte cap is insufficient; require coverage attestation (hunk list + omitted ranges) plus one mandatory judge re-read outside the digest (finding #5). To choose I need: the miss rate on seeded diffs at scout-low vs scout-medium.

## Requested checks
RC1 (read-only): `cat` five real `SubagentStop` hook stdin payloads from `~/.claude/hooks/` session logs — settles finding #3 effort visibility. Budget: 30 min.
RC2 (read-only): run the same brief at z.ai requested `medium` vs `high`, diff sent tier in ledger `effort_sent` — settles tier collapse. Budget: 2 consults.

---

### Findings

- **F07-1** [major] `crates/c3-core/src/quota.rs`, `.collab/parity-0.6.1-2026-10-08/state.md:186` - Window-share is not a portable denominator: it is plan-specific (subscription 5h/weekly vs pay-per-token GLM), nonlinear within a window (n=30: 30->65% for 260k tokens), polluted by other consumers on the same account (n=33: 8->72% with no local runs), and invisible to z.ai; scoring tiers per window-share cannot compare Astra against GLM or Sonnet-max against Opus-max users. Trigger: Comparing an Astra high round (10-15% of 5h window) against a GLM max round (no window, money cost) or across users on different plans. Evidence: read-code: quota.rs derives cost from Codex rollout token_count + rate_limits primary/secondary windows only; no money or token-price path exists.; read-code: State log records nonlinear jumps, other-consumer pollution, and budget-stop events that confirm the denominator moves without local runs. Verify: Recompute the nine accepted rounds F46-1..F62-1 in both units (window-% vs tokens) and check whether the ranking of tiers flips. Remedy: Score with a vector, not one denominator: tokens in/out+cached, wall time, and window-share where defined, plus money where billable; normalize usefulness per 1M input tokens as the cross-vendor comparator and keep window-share as a routing constraint, not the score.
- **F07-2** [major] `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md:48` - The caps-v1 mapping (medium->high, xhigh->max on z.ai) destroys the treatment variable: logging only the sent tier confounds preset intent with endpoint capability, and three coarse buckets cannot detect whether z.ai has intermediate tiers or silent remapping. Trigger: Any per reviewer x effort usefulness split on z.ai where medium and high collapse to the same sent value. Evidence: read-code: Brief states effort vocabularies low|medium|high|xhigh vs low|high|max with medium->high and xhigh->max mapping, and proposes logging sent tier plus requested preset name.; inferred: With only sent-tier logged, GLM high mixes requested-medium and requested-high; no field records endpoint caps version or raw model string. Verify: Send the same brief at requested medium vs high via z.ai and confirm the two events are distinguishable in the intake table. Remedy: Log the triple (effort_requested preset, effort_mapped sent tier, caps_version + endpoint id); add a calibration probe set run at each tier so tier effects are identified without waiting months.
- **F07-3** [major] `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md:57` - SubagentStop alone cannot reliably capture tier x effort or attribute outcomes: effort frontmatter vs Agent-call override vs model alias may disagree, the hook fires on coordinator stops and errors identically, reworked waves misattribute credit, and Codex/Kimi hosts have no such hook so the table will be Claude-only while claiming cross-user comparability. Trigger: A worker whose definition says sonnet but whose transcript model resolved to haiku, or a wave the coordinator rewrote before success. Evidence: read-code: Worker sketch assumes hook sees agent type and effort reliably and lists attribution and cross-host reporting as open questions.; read-code: Coordinator sketch notes agent definition vs model alias ambiguity and rework attribution as unresolved. Verify: Sample five real SubagentStop stdin payloads and check tier, effort, and model fields are all present and agree. Remedy: Capture at SubagentStart (declared type+effort+task class) joined by agent id to SubagentStop (outcome+usage), coordinator rate carries --effort fallback and a rework flag (used-as-is|reworked|discarded); define a host-adapter interface so Codex/Kimi report the same closed event manually.
- **F07-4** [major] `crates/c3/src/telemetry/classes.rs:1` - The proposed worker event still leaks task shape: duration/token/tool/edit/verification buckets plus agent id and timing joinable to ledger consult_refs fingerprint task size and workflow, and the old agent_report free-text event is one misroute away from breaking the closed-class contract. Trigger: Joining a workers table row (duration+tokens+timestamp) against a consult event from the same task window. Evidence: read-code: Telemetry classes are closed by construction (table text or other/unknown, never labels/hosts/models as typed); worker sketch proposes buckets plus optional ~50-token rating note.; read-code: State sketch explicitly warns the worker event must not reuse the free-text agent_report event. Verify: Attempt the join on synthetic intake data and confirm no task can be re-identified beyond tier x effort x outcome class. Remedy: MUST stay out: prompts, paths, names, transcript excerpts, task_scope text, raw token counts, session/transcript paths; coarse-bucket counts only, agent id as a one-way pseudonym rotated per task, explicit blocklist test rejecting free-text fields at intake.
- **F07-5** [minor] `.collab/consilium-2026-10-14/handoffs/01-claude-measure-reviewers-and-workers.md:51` - Scout-and-judge without a falsification budget will hide defects: a cheap scout optimizes for plausible digests, the judge anchored on the digest under-checks omitted hunks, and a stale digest surviving a cold cache silently narrows the judge's scope; no output cap alone keeps the judge honest. Trigger: A defect in a hunk the scout summarized as clean and excluded from the digest. Evidence: read-code: Routing proposes a cheap tier writing a digest handoff consumed by the judge plus a call cap, with the digest surviving a cold cache.; inferred: Quota history shows cold-cache pressure is real (96% fork cost, 8-23% per round), so digest staleness and scope-narrowing are the likely failure mode, not cost. Verify: Seed one defect outside the digest on a test diff and check the judge still finds it under the cap. Remedy: Require the scout to emit diff-range coverage (files+hunk counts) plus verbatim suspicious hunks under a byte cap, and require the judge to re-read at least one unmentioned hunk and report coverage; measure miss rate on seeded-defect calibration diffs.

### Prior findings

_(none)_

## Verdict: ADVISE

Framing needs a routing-first redesign with a portable cost vector and Start/Stop worker join before any closed-schema freeze.

### Blockers

_(none)_

### Unproven scenarios

- Whether z.ai glm-5.3 exposes more than three effective tiers or silently remaps them.
- Whether coordinator usefulness marks correlate with findings-confirmed hits once the findings join exists.
- Whether a weekly optimiser note changes tier choice versus a static recommendation table.

### First-run checklist (observable)

_(none)_
