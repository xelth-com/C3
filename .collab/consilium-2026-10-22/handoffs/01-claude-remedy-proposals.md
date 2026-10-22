Write in English.

# Handoff 01 - claude: framing panel - reviewers that propose remedies, not only verdicts

Date: 2026-10-22. Base commit: C3 `23d49ad` (main; 0.2.3 under acceptance). C3 is a bridge that lets a
coordinating agent (Claude Code) consult a roster of reviewer models from different labs through their CLIs; every
consultation is recorded as files under `.collab/<task>/` (brief, verbatim reply, `sessions.json` ledger,
`findings.json` store); a panel runs several reviewers on one brief; a routed panel seats reviewers by track record
(`README.md` "Reviewer roster and panel"; `docs/DESIGN.md` for the invariants: files are the record, reviewers are
read-only, the coordinator decides).

## The situation that prompted this panel

The release 0.2.3 is in an acceptance loop with one standing reviewer (openai :: gpt-6-astra, "Astra"):

- pass 1 (`.collab/parity-0.6.1-2026-10-08/handoffs/89-codex-codex-0.2.3-acceptance-1.reply.json`): HOLD, 3
  blockers + 5 more findings in the new `c3 work` feature (a roster model run as a coding worker);
- the coordinator briefed a Claude Code worker, which fixed all eight (`docs/port/wave10a-work-acceptance.md`);
- pass 2 (`handoffs/91-codex-codex-0.2.3-acceptance-2.reply.json`): HOLD again - 4 findings (F91-1..F91-4), two of
  them "the fix closed the simple case, this is the remaining case" of the pass-1 blockers.

Each finding carries `claim`, `trigger`, `verification` and a short `remedy` (one sentence: "validate the permitted
state, exhaustion, owner and expected attempt generation under the admission lock"). The remedy is a direction, not
a design: the worker that implements it makes the design choices alone, and the next pass finds the case it missed.
Cost today: about 280 s and 9-16 % of Astra's 5-hour window per pass; a worker wave of 40-60 minutes between passes.

## The operator's proposal (to be challenged)

When a reviewer raises a blocker, the OTHER roster members - preferably models from other labs than the finder -
propose concrete remedies (design + where in the code + the test that proves it); the coordinator picks one (or
merges), the worker implements it, and the finding is verified. Rationale: the model that found the problem should
not be the one that designs the fix (it would confirm its own frame), independent solvers see the cases the finder
listed and the ones it did not, and vendor diversity turns one reviewer's taste into a decision between
alternatives. The roles would be: finder (raises, states acceptance criteria), solvers (propose), coordinator
(chooses), verifier (the finder, or a third member?).

## What exists in the bridge that this would build on

- The reply schema already has `findings[].remedy` and `prior_findings[]` with per-id status
  (`crates/c3/src/consult/` ingests it; `c3 findings` lists, sets status `proposed|implemented|verified|rejected`).
- Panels with roles: `--role <one for all>` / `--roles <by score rank>` (`crates/c3/src/panel/roles.rs`, Kuhn
  matching on willingness) - a seat can be told "you are a solver for F91-1".
- Routing by track record: `panel.routing`, ratings `useful yes|partly|no` per consultation keyed by
  `consult_id`, the router scores reviewer x purpose x topic (`crates/c3/src/router/`).
- Packs: `c3 pack` builds a token-budgeted, sanitised context of named files for a model that cannot read the
  repository (`crates/c3/src/pack/`); the `http` engine consults OpenRouter models with a pack.
- Workers: `c3 work` runs a roster model as a coding worker that edits the tree (`docs/work-mode.md`).

## The coordinator's sketch

A new purpose `remedy` (preset effort high): `c3 consult --remedy F91-1[,F91-2] --panel-size 3 --exclude-vendor
<finder's lab>`; the brief is generated from the finding record (claim, trigger, verification, remedy, locations)
plus a pack of the located code and the tests that exist; every member answers with `proposals[]` {approach, code
locations, patch sketch, test, risks, confidence}; the proposals are stored under the finding id in `findings.json`
(`findings[].proposals[]` with the proposer's identity); the coordinator marks one `adopted` and the rest
`rejected` with a reason; the finder verifies on the next pass; the adoption marks feed the router as a second
score dimension ("solver" next to "finder"), so a member can be routed as a good solver even if it is a mediocre
finder. Automatic trigger: on a HOLD with blockers, the coordinator is offered the exact `--remedy` line (it is not
run automatically - the window cost is the coordinator's to spend).

## Questions

- **Q1.** Is the separation finder / solver / verifier sound, and who should verify - the finder (it knows the
  trigger), a third member, or a test the solver must ship? Name the failure modes: a solver that does not know
  the codebase, a finder that rejects every foreign fix, the cost of three consultations per blocker.
- **Q2.** The design: what is the minimal `proposals[]` record and workflow (purpose, roles, exclusion of the
  finder's vendor, pack contents, how the coordinator records the choice) that fits the existing files-are-the-
  record ledger? What must NOT be automated?
- **Q3.** Live demonstration. Pick at least two of F91-1, F91-2, F91-3, F91-4 (read the reply.json of handoff 91;
  the code is `crates/c3/src/work/mod.rs` - `claim_for_spawn`, `resume_once`, `refresh_artifacts`, `classify_attempt`;
  `crates/c3/src/work/record.rs` - `read_record`, `run_dirs`, `live_run`, `admission_lock`;
  `crates/c3/src/work/engines.rs` - `terminal_outcome`) and propose a concrete remedy for each: the design, the
  exact function(s) to change, the invariant it establishes, and the test (name + what it drives) that proves it.
  If you ARE the finder (openai :: gpt-6-astra), do not propose solutions: state per finding the acceptance criteria
  - the evidence that closes it - so the solvers' proposals can be judged against them.
- **Q4.** Scoring: how should an adopted proposal and a verified fix be credited (to the solver, to the finder, to
  both), so the router learns "good solver" without the finder gaming it by raising easy blockers?

Answer by number. Put each recommendation and each Q3 proposal as a finding (`claim` = the recommendation or the
proposed remedy, `trigger` = the case it addresses, `verification` = the test or evidence, `remedy` = the design in
two to five sentences). Keep it under 700 words. Reading budget: at most 6 tool calls; never print a whole file.
