# Consilium 2026-10-22 - decisions (coordinator, after the panel)

Panel: brief `handoffs/01-claude-remedy-proposals.md`, 13 of 13 roster entries asked, 10 usable (failed: openai
gpt-6.1-sol - not on the ChatGPT plan; alibaba qwen3.8-max - monthly quota until 2026-10-26; byteplus
deepseek-v4-1-flash-261017 - model drift refusal, the endpoint answered as deepseek-v4-1-flash). Digest with the
per-member positions: `notes-digest.md`. Run from a detached worktree at 23d49ad while wave 10b edited the tree.

## The operator's proposal, as the panel reshaped it

The idea "other models propose the remedy for a blocker" stands, but NOT as framed:

1. **The verifier is the test, not a third model.** 9 of 10: a remedy ships with a test that fails before and
   passes after; the coordinator re-runs it; the finder re-checks only its own trigger on the next pass. A third
   verifying model (dola alone) is not adopted.
2. **Diagnosis corrected** (deepseek, mimo, glm-5.3, gemini): the pass 1 -> pass 2 loop was an under-specified
   invariant plus a missing closing test, not a shortage of designs. So the FIRST intervention is a mandatory
   closing test per blocker and frozen acceptance criteria from the finder BEFORE anyone proposes; the remedy
   panel is the SECOND intervention, for genuinely plural design spaces and for reopened findings only.
3. **Vendor diversity is not independence.** Exclude the finder INSTANCE (and the previous fixer's vendor), not
   the finder's lab; require at least 2 eligible solvers after exclusion or skip and record the skip. Prefer
   repo-reading engines (claude, codex) as solvers - pack-fed solvers miss call sites.
4. **Window share is the scarce resource**, not wall clock: cap at 2 solvers per finding family; the `--remedy`
   line is offered after a HOLD, never run automatically.

## Adopted design (to implement as a wave, not now)

- `findings[].acceptance[]` - the finder's criteria, written before proposals exist and never amended after.
- `findings[].proposals[]` stored verbatim in `findings.json` (no new ledger): `{proposer{provider, model,
  consult_id}, approach, locations[], patch_sketch, test{name, drives, fails_before}, risks, confidence,
  pack_digest, status: adopted | rejected | merged + reason}`; status set only by the coordinator; an explicit
  "no proposal adopted" outcome; the exclusion and panel composition recorded in `sessions.json`.
- Purpose `remedy` (effort high): `c3 consult --remedy F<NN>-<k>[,..] --panel-size 2 --exclude <finder consult_id>`;
  brief generated from the finding record + a pack of the located code and nearby tests; adoption changes
  nothing in the finding's status - only a verified test does.
- Router: a `solver` dimension separate from `finder`, credited ONLY on `verified` (adoption = 0), signed (a later
  superseding finding reverses the credit), keyed by (purpose, topic), inactive below ~10 verified samples; the
  finder never rates the solver; the coordinator's `useful` mark stays the finder's score. Finder denominator
  counts rejected/duplicate findings. Track closure time and reopen rate.
- Never automated: panel dispatch on HOLD, adoption/merging, worker launch, the `verified` flip, vendor exclusion
  policy, the finder amending criteria after proposals.

## The live demonstration (Q3) against wave 10b

The panel's best remedies (glm-5.3/deepseek for F91-1, deepseek for F91-2, glm-5.3-flash for F91-3,
glm-5.3/mimo for F91-4) coincide with what the wave-10b worker implemented in parallel (2ceeead): claim under the
admission lock with the attempt derived from the locked re-read and the `waiting_quota` owner path kept; strict
reads that fail closed with a lenient display path; event-read errors as `artifact_error` with temp files kept
until the durable append; explicit `is_error: false` and `turn.completed`. Astra's acceptance criteria (F91-1/2
only; it deferred F91-3/4 to the handoff-91 recipes) add one check the worker's report does not name: that
`read_record`'s recovery WRITE is split from the read and serialised with admission. Carried into brief 92 as a
question to the finder rather than a new wave. No 10c.

## Deferred

The wave itself (schema, purpose, router dimension) goes to ROADMAP "Next" behind the 0.2.3 acceptance; first a
measurement: replay two past blockers to two labs and measure divergence (deepseek) before building the panel path.
