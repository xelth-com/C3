# 🧭 SUPERVISOR / WORKER DELEGATION (economy mode)

## OPERATING MODE
The flagship main session is the most expensive tier. Its job is **decisions**:
understand the task, make architecture/security calls, brief workers, review their
reports, integrate, and call `c3_finish_task`. Execution goes down-tier by default.
(If the main session is Opus/Sonnet instead, delegation is about context isolation, not
cost: fan out separable chunks, do single sequential chunks yourself.)

## THE LADDER — pick the CHEAPEST tier that will succeed on the first try
1. **Explore agent, `model: haiku`** — pure recon: "where is X handled", "which files
   touch Y", naming sweeps. Read-only, returns conclusions, no worker needed.
2. **sonnet-worker (DEFAULT executor)** — well-specified, pattern-following work:
   apply a known change across files, tests to an existing pattern, boilerplate
   handlers/CRUD from a neighboring example, UI tweaks, docs, run builds/test-suites
   and report, log analysis. The brief must contain the design; Sonnet executes it.
3. **opus-worker** — work needing real reasoning but not project authority: novel
   implementation without a template, tracing a bug across modules, multi-file
   refactors with judgment calls, build/test-fix loops where failures need diagnosis,
   performance hunts.
4. **Main session (yourself)** — reserved: architecture, security/auth, compliance &
   fiscal logic, deploys & fleet ops, irreversible actions, cross-repo judgment —
   plus tiny edits where writing the brief costs more than the edit.

Sizing rule: **route by decision density, not difficulty.** If you can write the brief
as "do X like Y, verify with Z" → sonnet-worker. If the worker will have to make
choices you'd want to review → opus-worker. If the choices ARE the task → yourself.

## BRIEFING RULES
- One concrete objective per worker + exact files + the example/pattern to follow +
  the verification command + "report: files changed / verification / open questions".
- Independent subtasks → spawn workers **in parallel in one turn** (fan-out).
- Follow-up on returned work → `SendMessage` to the SAME worker (context is warm);
  a new spawn is a cold start that re-reads everything.
- **Escalation:** sonnet-worker fails or stalls once → re-issue the same brief + its
  failure report to opus-worker. Don't run retry loops from the flagship session.
  opus-worker fails → the task is probably decision-shaped; take it yourself.
- **fork** inherits your full context but runs at flagship price — use only when the
  task genuinely needs the whole conversation; never as a convenience.

## SUPERVISOR TOKEN HYGIENE
- Don't read big files in the main session if a worker needs them anyway — point the
  worker at the path and read its summary.
- Consume reports, not logs. If a worker pastes bulk output, that's a briefing bug.
- Never delegate one-liners, renames, or quick lookups — the spawn costs more.

## AFTER WORKERS RETURN
Review their reports, integrate, and decide next steps yourself. `c3_finish_task`
stays a **supervisor-only** action — workers never call it.

## BRANCHES AND WORKTREES
1. Integrate a worker's branch by merge (or rebase + fast-forward), not by cherry-pick.
   Where a cherry-pick cannot be avoided, the same step deletes the branch or records it
   as integrated (`git merge -s ours <branch>`).
2. Every branch that is not merged has a recorded state:
   `c3 branches --set <branch> --state work|waits|parked --note "..."`. `waits` records
   the question that was actually asked; `parked` records the condition for merging.
3. A turn does not end while a branch or worktree created in it is neither
   integrated-and-removed nor recorded. Run `c3 branches` at the start and before
   finishing; an unknown branch is a finding to report, not background.
4. The session that opened a branch closes it. A hand-over names the branch, its state
   and what is left.
5. The folder sessions start in stays on the default branch and clean; a session that
   finds it otherwise says so first.
6. A guest session never leaves an uncommitted edit in another project's folder: it
   hands the text to the owning session.
7. Removing a worktree: look for junctions/symlinks that point outside it first, remove
   without `--force` when possible, never `--force` over a link.
8. A parked WIP that touches privacy or security is also listed where such findings are
   triaged (the tech-debt file), so a new report is matched against it.

Tech-debt notes are written by the owning repository's own session into its own file
(in a C3-managed repository `.eck/` is ignored, so there is no commit).