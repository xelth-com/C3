# Panel A synthesis: measuring reviewers and workers (consilium 2026-10-14)

Members (cited by number): 03 kimi-k3, 04 mimo (claude), 05 gpt-6-astra, 07 muse-spark, 08 deepseek-v4.1-flash, 10 dola-seed-2.0-pro, 11 glm-5.3 (claude), 12 gemini-3.8-flash, 13 glm-5.3 (codex). All ADVISE.

## 1. Per question

### Q1: tiers and denominator
**Consensus.** Astra: chore low, checkpoint/reading medium (provisional), diff-review high, acceptance high, core-contract/stuck xhigh. GLM: chore low, checkpoint/framing/diff-review high, stuck/core-contract max. No universal denominator: window share is OpenAI-only, account-wide (8->72 % without our runs) and nonlinear, so it is a routing constraint, not the score (03, 04, 05, 07, 08, 11, 13). Numerator: confirmed findings, not marks.
**Dissent.** 08: nothing below high on Astra for a verdict-bearing run. 12: no chores to Astra; Astra acceptance xhigh. GLM acceptance: high (03, 04, 08, 11) vs max (05, 12, 13); 05, 12 also GLM diff-review max. 10, 12 keep window share as Astra's denominator. 07: no fixed tiers yet. Framing on Astra: high (05, 12) vs medium (11).
**New elements.**
- Marginal finding (no other seat produced it) per cost unit (03); paired high-vs-medium increment on one brief from natural panel pairs (13, 04, 08, 03).
- Exploration: 10-20 % of runs one tier off policy, flagged (04, 07; 11 low-stakes only).
- Escalation ladder: re-send higher only on an invalid reply (04), zero findings at acceptance (11), or an unverified ACCEPT (12).
- Frozen known-defect calibration cases at each tier (05, 07, 10).
- Escapes (defects found after an accept) as the recall signal (13).
- Own-run input tokens (cached counted) as the cross-vendor denominator (07, 11); `RunQuota.tokens` is already baseline-subtracted (11).
- Roster policy overrides a preset, never duplicates it (13; 04 floor/cap). GLM max as second judge on disagreement (04).
- Claimed: z.ai docs list exactly three glm-5.3 tiers; OpenAI lists Astra `max`, unproven on the subscription route (05).

### Q2: worker capture
**Consensus.** SubagentStop is the right closing point but insufficient: silent on killed/crashed workers, repeated on resume. Pair it with SubagentStart (04, 05, 07, 08, 12) or a SessionStart sweep (13); an unmatched start becomes `unknown`; dedupe on a minted worker ref and count resumes. The hook sees the agent name, not the model or call-site effort (04, 05, 07, 13).
**Dissent.** 11: Stop alone, duration from transcript timestamps. 10: Stop is authoritative "with no race conditions". 03: Stop is enough if a spawn registry supplies effort.
**New elements.**
- Spawn registry: coordinator writes worker-id -> {tier, effort} locally, the hook reads it (03).
- Provenance on every effort/model value; `rate --effort` never claims what the hook missed (04, 11, 13).
- Tier from the resolved model, not the agent name (05); model from transcript init events (11).
- Cited docs: hook input carries a conditional `effort.level`; Stop has no model field (05).
- A test-command pattern is not `verified`: split attempted / result / independent acceptance (05).
- Rework as a closed coordinator disposition, apart from the execution outcome; the wave as primary unit (08).
- Git hunk survival ratio: >= 80 % clean, 20-79 % assisted, < 20 % discarded (12).
- Handoff and state.md outcome lists conflict; reconcile first (04).

### Q3: schema
**Consensus.** Add `effort`, `effort_requested`, an effort source, a mapping version and a schema version; every enum ships with `unknown`/`other`; a separate `worker` event, never `agent_report`; no free text, no ratios. Prerequisite (04, 11, 12): `close_event` (classes.rs:870) discards every event_type except consultation|rating, extend closer, constructors and intake in the hook's release; old events must still close (04).
**Dissent.** `effort_requested` = preset name (07, the sketch) vs pre-mapping tier (05, 11; 03 adds `effort_native`). 08: anti-breakage is permissive ingest plus a version; the ledger already holds the effort fields. 05: promise versioned compatibility, not a frozen vocabulary.
**New elements.** Section 2, item 3; plus `validate_roster` refuses an `effort` key (12; checked: 14-key allowlist).

### Q4: scout and judge
**Consensus.** Fails by compression loss (the one-line ordering/precision defects of F58/F60/F62), anchoring, stale digests. The digest is a map, never a verdict; coverage is explicit; the judge keeps the diffstat and raw-hunk access.
**Dissent.** 05: no cap guarantees honesty; benchmark against independent review at equal cost. Caps: 40 lines/4 KB (04) to 1,500 tokens (12).
**New elements.** Suspicion 0-3 per hunk (03); machine-checked coverage, digest discarded if a diffstat file is missing (04); judge cap in calls, 1 + <= 2 escalations (04); 1-in-5 raw-diff audit arm (04, 08); claims quote raw path:line (04, 12); budget exhaustion = incomplete, never accept (05, 10); digest names its commit (08); counted `digest insufficient` escalation (08); judge re-reads one unmentioned hunk (07); scout and judge from different families (11, 13).

## 2. Decisions the coordinator can take now

1. **Effort policy** (roster overrides presets):

| purpose | gpt-6-astra | glm-5.3 |
|---|---|---|
| chore | low | low |
| checkpoint / reading | medium (paired-test) | high |
| framing / decision | high | high |
| diff-review | high | high |
| acceptance | high | high (4 vs 3 for max; first paired candidate) |
| core-contract / stuck | xhigh | max |

2. **Denominator.** No published ratio; the wire carries components (own input/cached/output tokens, wall seconds, bucketed window delta, `cost_basis`). Interim within-vendor score: confirmed findings per 1M own input tokens; window share only an Astra feasibility gate.
3. **Fields** (proposers in parentheses):
   - consult/rating: `effort` none|low|medium|high|xhigh|max|unknown; `effort_requested` same set, pre-mapping (05, 11); `effort_source` preset|policy|flag|native (11) + explore (04); `effort_mapping`/`vocab_version` (04, 08; 07 `caps_version`); `schema_v` (03); `cost_basis` window|money|none (03); `tokens_cached` (11); `quota_window_bucket` none|0-25|25-50|50-80|80-98|98-100 (12); `panel_role` solo|scout|judge|shadow (11); `assignment` routed|preset|alternate|random|unknown (13); `cohort_ref` pseudonymous same-brief group (13); rating copies the effort fields (05, 12) and adds `findings_confirmed`/`findings_implemented` counts (04).
   - worker: `worker_ref` random, mapping local (05, 11); `host` claude-code|codex-cli|other (13); `tier` haiku|sonnet|opus|fable|other; `model` closed list; `effort` + `effort_source` definition|override|coordinator|unknown (13); `outcome` completed|stopped|error:rate_limit|error:context|error:other|unknown (04); `disposition` as-delivered|reworked|superseded|unused (08; 05 splits minor/substantial rework); `verification` clean|failed|unrun (12) + unknown; `edits` read-only|writes (11); duration/token/tool buckets; `resumes`; `wave_ref` (04); mark + `rating_rev`.
4. **Capture.** SubagentStart + SubagentStop, silent and non-blocking, local lifecycle journal, orphans closed as `unknown`; effort from the spawn registry first. **Excluded:** prompts, briefs, transcripts, `agent_transcript_path`, raw hook JSON, paths/cwd/repo, session/agent ids (pseudonymise, rotate per task, 07), agent names as typed, tool args/outputs, diffs, errors, env, `--note`, digests, exact percentages and token counts (buckets only).
5. **Scout caps.** Per-file map (hunk ranges, suspicion 0-3, not-read list, commit), no verdict; <= 200 lines / ~1,000 tokens, prose <= 300 words dropped first; coverage machine-checked else raw; judge gets brief + diffstat + raw-hunk fetches, 1 + <= 2 calls; incomplete coverage -> incomplete; 1-in-5 audit arm.
6. **Before code:** extend `close_event` + intake for `worker`; intake accepts-and-ignores unknown keys (08); roster `effort` key; one outcome list.

## 3. Open questions (question -> settling experiment)

- glm-5.3 beyond three tiers? -> one chore at native xhigh, read the ledger (04); 05 says docs show three
- hook payload (model, `effort.level`, Stop on kill/resume)? -> stdin-dump hooks over completion, resume, cancel, kill, with an effort override (05, 04, 08, 11)
- Astra medium for reading; GLM acceptance high vs max -> 8 frozen known-defect cases x 2 efforts, blind, <= 20 window points (05); cheaper: one past wave twice (08, 11)
- attributable window share -> per-run deltas vs total movement in `~/.codex/sessions` (04); re-rank F46-1..F62-1 by window % vs tokens (07)
- intake on unknown types/keys, versioning -> read the xelth.com intake parser (04, 08)
- digest recall -> replay F60-1/F62-1's range through a scout, plus a clean range (03, 08, 11, 13)
- enough data? -> intake cell counts; under ~20 per cell (08) the table is noise
- mark vs hunk survival -> compute survival on past waves, compare with marks (12)

## 4. Ratings

| # | useful | why | best idea |
|---|---|---|---|
| 03 kimi | yes | `effort_native`, `cost_basis`, `reworked`, fixed-schema scout | coordinator spawn registry the hook reads |
| 04 mimo | yes | caught the outcome-list conflict and the `close_event` drop; four runnable checks | machine-checked digest coverage + 1-in-5 audit arm |
| 05 astra | yes | cited hook/vendor docs; verified != test-command; tier from resolved model | blind frozen known-defect benchmark at two efforts |
| 07 muse | partly | no tier table; RC1 cites nonexistent hook logs; RC2 "read-only" runs consults | timing+token buckets fingerprint tasks: rotate pseudonyms |
| 08 deepseek | yes | wire version, exact-bytes test, n=9 too small | accept-and-ignore unknown keys + `vocab_version` first |
| 10 dola | partly | mostly restates the sketch; "no race conditions" wrong; RCs are `cat` | omitted-hunks list the judge must acknowledge |
| 11 glm (claude) | yes | own-tokens denominator, pre-mapping `effort_requested`, `panel_role`, `--note` conflict | own input tokens as score, window as gate |
| 12 gemini | yes | exact enum sets, real roster-key refusal; RC test names invented | git hunk survival ratio |
| 13 glm (codex) | yes | `assignment`, `cohort_ref`, escapes as recall, SessionStart sweep | `assignment` + `cohort_ref` for causal pairs |
