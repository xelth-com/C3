Write in English.

# Handoff 01 - claude: framing panel A - measuring reviewers and workers (usage tiers, worker telemetry)

Date: 2026-10-14. Base commit: C3 `v0.2.1` (8a133f8); the PowerShell plugin `claude-codex-consult` 0.6.1 (main
dcb9d59, wave 30 unreleased). Both are the same program: a bridge that lets a coordinating agent (Claude Code, Codex
CLI and other hosts) consult reviewer models through their CLIs, with a reviewer roster, a findings store, and an
anonymised telemetry intake at `xelth.com/T/`. Read `README.md` and `docs/port/wave8-quota.md` of this repository
for the current behaviour; the roster shape is in the README's "Reviewer roster and panel".

## Question

Before any of this is implemented, we want the panel's angle on two measurement designs that will shape the
roster, the telemetry schema and the routing for a long time: (A) usage TIERS for the two standing consultants
(OpenAI gpt-6-astra through the ChatGPT subscription, Zhipu glm-5.3 through z.ai) - which reasoning effort to
send for which purpose, and how to score a tier; (B) WORKER telemetry - the coordinator's own sub-agents (Claude
Code tiers haiku / sonnet / opus / fable AND their effort levels) counted and scored at near-zero token cost so the
tier choice can be improved from data. Both change a closed telemetry vocabulary that is hard to change once
published.

## Task state

- Done: the telemetry intake (consult and rating events; engine, vendor class, a closed list of model names, purpose,
  outcome class, counts - never a task, prompt, path or name); the quota windows (wave 8: the 5-hour and weekly
  ChatGPT windows read from the Codex rollouts; a round of review costs 8-23 % of the 5-hour window at effort high;
  a forked thread with sub-agents once cost 96 % in 186 s); effort vocabularies per endpoint (caps-v1: openai
  low|medium|high|xhigh; z.ai low|high|max with medium->high and xhigh->max); purpose presets (chore low, checkpoint
  medium, acceptance high, core-contract/stuck xhigh); the ratings (`rate`, `useful yes|partly|no`) per
  consultation; a routed panel seats reviewers by track record.
- Open: no telemetry event carries the effort; no event exists for workers at all; the roster has no per-purpose
  effort policy per entry; nothing measures a tier against its cost.
- Not in scope: new reviewer engines; the browser engine; the intake's hosting.

## Evidence

- `crates/c3/src/telemetry/classes.rs` - the closed vocabulary of the events (what can be added without breaking
  the published schema).
- `crates/c3-core/src/quota.rs` - what a run costs in window share and tokens (`tokens`, `quota`, `budget` in the
  ledger entry): the cost side of a score exists per consultation already.
- `.collab/parity-0.6.1-2026-10-08/state.md`, sections "Planned (2026-10-14 10:40): usage tiers" and "Planned
  after the tag: worker telemetry" - the coordinator's current sketch (below).
- Observed today: nine review rounds of one wave with gpt-6-astra at effort high found one real defect each
  (F46-1 .. F62-1), 183-234 s and 10-15 % of the 5-hour window per round; a GLM round at `max` is cheaper in money
  and has no 5-hour window; Claude Code workers (opus / sonnet) did each fix in 15-70 minutes.

## The coordinator's sketch (to be challenged)

(A) Usage tiers. Roster entries get an effort POLICY per purpose (e.g. gpt-6-astra: chore medium, reading pass
medium, diff-review high, core-contract xhigh; glm-5.3: chore low, review high, stuck max). The consult and
rating events carry `effort` (the tier SENT after the vocabulary mapping - closed: none|low|medium|high|xhigh|max)
and `effort_requested` (the preset name). The intake's usefulness table splits per reviewer x effort. Routing
"scout and judge": a cheap tier (GLM low, or a Haiku worker) reads the diff and writes a digest handoff; the judge
(Astra high, or GLM max) gets the digest plus a call cap; the digest survives a cold cache. Open: does z.ai expose
more than three tiers on glm-5.3; is `medium` on gpt-6-astra good enough for a reading pass; how to SCORE a tier -
usefulness per 5-hour-window share, not per run.

(B) Worker telemetry. Claude Code's `SubagentStop` hook (the plugin can ship it) fires at each worker's end with
the agent type; the bridge records one closed-class event per worker: tier (haiku|sonnet|opus|fable|other), effort
level if the host exposes it, duration class, outcome class (reported | stopped-by-coordinator | error | unknown),
and a coordinator-side mark after the fact (`c3 workers rate <id> useful yes|partly|no`) - never the brief, the
files or the repository. The intake gets a "workers" table (counts, error share, usefulness per tier x effort).
Cost: one hook invocation per worker, no model tokens. Open: can the hook see the effort and the tier reliably
(agent definition vs model alias); how to attribute a wave's success to a worker when the coordinator reworked it;
what a user of another host (Codex CLI agents, Kimi) can report.

## Alternatives weighed

1. **Effort policy in the roster + effort in the events (the sketch)** - small schema change now; the score needs
   months of data.
2. **No new telemetry; a local experiment first** (the coordinator runs the same brief at two efforts for a week and
   compares findings per window share) - no schema risk; nothing learned for other users.
3. **Score reviewers by findings confirmed, not by usefulness marks** - more objective (a finding later marked
   `implemented`/`verified` is a hit); needs the findings store joined to the events (ids are private today).
4. **Workers: no hook, the coordinator's own notes in STATE.md** - zero code; not comparable across users.

Current preference: 1 for reviewers with the window-share denominator; 3 as the long-term score once the findings
join exists; for workers the hook with closed classes, but only if the effort is visible to it.

## Questions

- **Q1.** For (A): which effort tiers would you send per purpose for gpt-6-astra and glm-5.3, and what is the right
  denominator to score a tier (window share, money, wall time, findings confirmed)? Name a design we did not list.
- **Q2.** For (B): is the SubagentStop hook the right capture point; what MUST stay out of the event for privacy;
  and how would you attribute a worker's result when the coordinator reworks it?
- **Q3.** The schema: which fields would you add to the closed vocabulary now so we never have to break it later?
- **Q4.** The "scout and judge" routing: where does it fail (a digest that hides the defect), and what cap on the
  scout's output keeps the judge honest?

Answer by number. Keep it under 600 words.
