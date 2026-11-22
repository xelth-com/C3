# Handoff 05 - Claude: diff-review of 0.2.5, part 1 of 5 - reviewer reliability and the codex guard

Date: 2026-11-22. Range: `ba43ff6..a115422` (base `ba43ff6`, head `a115422`; one commit).

You run in a throwaway checkout of the head commit of this range; the change itself is the patch
file named in your prompt (`.collab/` and `.eck/` left out). You may have no shell: read the patch
and the files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, send something private off the machine, or report a
result that is not true (a failed reviewer shown as usable, a usable one shown as failed) - not
style.

## Delta since the last review

A first panel reviewed `v0.2.4..ba43ff6` and returned 36 findings. This range holds (a) fixes for
those findings: none, this part is not one of them; and (b) tech-debt items, two waves written
for problems seen in earlier reviews and runs. No model has read either. They are:

- **A1. Reviewer reliability** (`a115422`; `crates/c3/src/engines/agy.rs`,
  `crates/c3/src/engines/tree_check.rs`, `crates/c3/src/engines/kimi.rs`,
  `crates/c3/src/consult/orchestrate/{mod,tree,engine}.rs`, `crates/c3/src/consult/prompt.rs`,
  `crates/c3/src/providers.rs`, `crates/c3/src/panel/run.rs`, `crates/c3-core/src/lineage.rs`).
  agy's tool steps are judged from its own log; a complete reply survives a tree change as a
  warning; a kimi seat with an unreachable brief is refused before launch; panel priors come from
  the pre-panel snapshot; a death record is completed.
- **A2. Codex guard** (same commit; `crates/c3/src/maintain/smoke.rs`, `crates/c3-core/src/quota.rs`,
  `crates/c3-core/src/engine.rs`, `crates/c3/src/engines/subprocess.rs`,
  `crates/c3/src/consult/prompt.rs`). The codex smoke reads thread rollouts and fails on a
  sub-agent switch that is not off; the sub-agent guard is bounded by the turn's launch; every
  codex REVIEW turn drops plugins and skill listings and every review prompt says the reviewer is
  not a coordinator.

`CHANGELOG.md` section `[Unreleased]` describes each item in prose.

## CURRENT invariants claimed

- A tool step of agy is a write only when it names a path, file, directory or command and is not a
  known reader, or when its parameters were not logged; the `finish` hand-over and a step with no
  path or command are not writes. A real writing call still fails the run.
- When the reply file and `reply.json` are complete and schema-valid, a tree or collab change the
  log does not explain is a warning ("reply kept"); the ledger entry and scoreboard mark are
  written.
- A kimi seat whose brief is neither committed at HEAD nor copyable (a relative path leaving the
  repository) is refused before the child starts, also under `--skip-preflight`. A reply that only
  says it cannot read the brief is NOT reclassified.
- A panel member's prior-blocker lines, ACCEPT checks and `unchecked_prior_blockers` are judged
  against the pre-panel snapshot it was shown, never the store at render time.
- A death record names the events stream of the turn that ended the run; a zero-tool guard
  failure's wall is added to the run's wall. `c3 providers` reports per reviewer, over the 14-day
  ledger window, `usable / failed with a reply / failed without a reply`.
- The codex smoke fails on a thread whose rollout shows `multi_agent_version` other than
  `disabled`, offers or calls a multi-agent tool, is missing, or names neither a version nor a tool
  list.
- The sub-agent guard takes its own boundary just before the spawn; for a fork without
  `history_base` it starts at the rollout's last `task_started`.
- Every codex review turn passes `-c features.plugins=false -c skills.include_instructions=false`
  unless the operator's `--codex-config` names that key; `c3 work` worker turns are unchanged.
  Every review prompt on every engine ends its Constraints paragraph with the
  reviewer-is-not-a-coordinator sentence.

## Evidence

- Local suite after `a115422`: fmt, clippy `-D warnings`, every test binary of c3-core, c3 and
  c3-cli (40 binaries, 1298 passed, 0 failed). CI green on Linux and Windows (run 45031254736).
- Live: `c3 maintain --smoke --engine codex` passed on `ZAI::glm-5.3-flash` (debug build); an exec
  fork was observed on ZAI; a probe showed 30 skills listed before the two switches, none after
  (input tokens 74.6k -> 66.8k).
- Known and left open: a gap of about 0.8 s on a sub-agent fork in the guard; priors of a single
  (non-panel) run still read the store at render time; kimi still fails on a main-tree change.
- NOT exercised live: no real agy, kimi or grok run through the A1 changes is on record; a real
  Codex update rolled back by the new smoke check; a real sub-agent fork against the guard.

## Questions

- **Q1. agy outcome (A1).** Name any path where a real write by agy is judged a reader or a
  reply-kept warning (a tool with an unusual name, parameters logged in another shape, a command
  that writes), or where a complete reply is thrown away. Can "reply kept" keep a reply that the
  tree change corrupted, and does a warning still reach the ledger and `c3 providers`?
- **Q2. kimi seat (A1).** Can the brief check be bypassed or wrongly refuse (a symlink, junction,
  `..` component, other drive, a brief committed at HEAD but dirty)? Does the refusal re-seat a
  panel member without dropping a required reviewer?
- **Q3. Snapshot and races (A1).** Does the pre-panel snapshot stay true when two members or two
  `c3` processes write the findings store at once? Can a prior blocker be missed or invented?
- **Q4. Smoke proof (A2).** Name any way the smoke passes while a sub-agent switch is on (a
  rollout read before it is complete, the rollout of another thread, a partial last line, a
  tool list in another field) or fails a sound update. Does a failure always reach the rollback,
  and can the rollback itself be skipped or loop? Windows vs Unix: rollout path, line endings,
  file locking.
- **Q5. Guard boundary and flags (A2).** Is the boundary clock correct across a slow
  `on_running`, a resumed turn and a fork? Can `--codex-config` or a missing key turn the two
  review switches off silently, break an older codex that rejects the key, or leak into worker
  turns? Could an operator's global file still make a reviewer a coordinator?
- **Q6. Truthful reporting.** Anywhere a failed reviewer is shown as usable or the reverse, a
  ledger or death record is wrong or doubled, or something private is written outside the
  repository or sent off the machine? Does a CHANGELOG sentence claim more than the code does?
- **Q7.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
