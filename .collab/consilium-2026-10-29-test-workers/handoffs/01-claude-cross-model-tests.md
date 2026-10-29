# Handoff 01 - Claude: cross-model tests for `c3 work`

Date: 2026-10-29. Base commit: `0b712c4` (clean).

## Question

The operator wants GLM (`ZAI-claude :: glm-5.3`) as the main code author through `c3 work`, and
wants the TESTS for a change written by a model of a different family than the author. Should c3
itself grow support for this (and if so what exactly), or does it stay a coordinator convention
over two plain `c3 work` runs? Whatever we build becomes the default pipeline for every worker
task, so the shape is expensive to change later.

## Task state

- Done: `c3 work` (design `docs/work-mode.md`, README "Worker runs"). Engines claude (GLM-5.3,
  GLM-5.3-flash, DeepSeek-4.1-flash), codex (gpt-6.1-sol), muse (spark-1.3-contributor). One live
  work run per task. Record `.collab/<task>/work/<NN>-<provider>-<model>/` with brief.md,
  events.jsonl, report.md, diff.txt (git status before/after + diff --stat, computed by c3),
  run.json. Default allow list `Bash(cargo test|check|clippy|fmt:*)`; git refused; `--unrestricted`
  opt-in. First real GLM run (`.collab/c3-work/work/01-ZAI-claude-glm-5.3/report.md`) wrote 4
  integration tests and found/fixed a real guard-ordering bug; workspace tests/clippy/fmt green.
- Open: nothing in c3 today knows that two work runs belong together, which run is "author" and
  which is "tester", or which files each may touch.
- Not in scope: reviews/panels (unchanged), multi-host, cloud mode.
- Constraints: Windows host; full cargo test only with `-j 2` and never beside another build (OOM);
  z.ai/Muse/Grok expose no quota API; C3 never commits.

## Coordinator's position (one opinion on the table, not the answer)

One task = one author. Different models add value in review and in an INDEPENDENT oracle (tests),
not in relay-editing the same code. Fixes go back to the same author session.

## Alternatives weighed

1. **Convention only** - coordinator runs `c3 work` twice (author, then tester on another
   provider) and arbitrates by hand. Zero code; no enforcement, no linked record, no telemetry.
2. **Sighted tester** - `c3 work --role tests --of <work-id>`: the tester gets the author's brief
   + diff, c3 refuses a same-family tester, a path guard limits it to test files (`tests/`,
   `#[cfg(test)]` modules are a problem: they live inside src files).
3. **Blind / spec-first** - tester writes tests from the SPEC before or without seeing the
   implementation; author must make them pass and may not edit the test files (c3 checks
   diff.txt against a path scope and refuses/flags the run).
4. **Validity gate** - whichever of 2/3, c3 also proves the tests bite: they must fail on the
   base commit (or with the author's change reverted in a scratch worktree) and pass after.

Current preference: 3 + 4 for features with a clear contract, 2 for bug fixes / refactors;
minimal c3 surface (linkage + path scope + validity gate), arbitration stays with the coordinator.

## Questions

- **Q1.** Is cross-model test authoring worth building into c3 at all, vs alternative 1? What
  evidence would change your answer?
- **Q2.** Blind (spec-first) vs sighted tester: which catches more real bugs, and where does each
  fail (over-fitting to the implementation vs testing a misread spec)?
- **Q3.** When a cross-model test fails, who decides "code bug" vs "test bug", and what must c3
  record so that decision is auditable? Can c3 help (e.g. re-run, revert-check) without deciding?
- **Q4.** The minimal c3 surface: flags, record linkage, path scope, validity gate - what is
  essential for v1 and what is gold-plating? How does it interact with "one live work run per
  task" and with `#[cfg(test)]` modules inside source files?
- **Q5.** Family-diversity rule: which pairings on this roster (GLM, DeepSeek, gpt-6.1-sol, Muse)
  make sense as author/tester, and should c3 enforce "different family" or only warn?
- **Q6.** Risks we have not listed (cost on monthly-capped plans, tests that pass trivially,
  tester rewriting the API, flaky tests on Windows, prompt-injection through the author's diff).

Answer by number. Keep it under 900 words.
