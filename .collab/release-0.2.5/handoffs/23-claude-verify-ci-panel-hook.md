# Handoff 23 - Claude: verification of the 0.2.5 review fixes, part 3 of 3 - CI line, panel labs, hook

Date: 2026-11-23. Range: `5d92ca7..748ba3a` (base `5d92ca7`, head `748ba3a`; one commit).

You run in a throwaway checkout of the current head of the repository, which contains this commit
and the later ones; the change to verify is the patch file named in your prompt (`.collab/` and
`.eck/` left out). You may have no shell: read the patch and the files it names with your file
tools. Everything is in this repository.

## Question

Are the findings listed below really fixed by this commit, and did a fix introduce a new defect? A
finding is fixed only when the failure its trigger describes can no longer happen - not when a
comment or a CHANGELOG sentence says so.

## Delta since the last review

A first panel read the changes after tag `v0.2.4` in five parts. For the CI check, the panel routing
and the hook it returned 20 findings (F16-n, F17-n); this commit fixes 17 of them (F16-1 and F17-5
belong to part 2 of this verification, F16-5 is below). One line per item: ids - defect - fix -
main files.

- F16-2, F17-1, F17-10 - the red-CI line carried a GitHub run title into every session - C3's own
  words: local branch, digits-only run id; pull-request runs ignored; `--json` `ci` is an object
  (`ci_check.rs`, `cli/finish.rs`, `hook/mod.rs`)
- F16-3 - git and `gh` children could outlive `c3 finish` or `c3 hook` - one deadline (10 s / 3 s)
  for every process of the check, the whole process tree killed (`bounded.rs`, `ci_check.rs`)
- F16-4, F17-7 - wrong branch when `origin/HEAD` has no local twin, runs ordered as text, 5 runs
  - remote default branch, order by parsed time, 20 runs, silent without a default
- F17-8 - any remote line containing `github.com/` started `gh` - the URL's host is compared
- F16-9 - one shared "asked today" file raced between sessions - one file per repository, written
  by temp file and rename
- F16-6, F17-2 - `router explain` re-derived eligibility, `replay` used rounded draw values - the
  panel's own member election; full-precision `ext.c3.draw` in the record (`cli/router.rs`,
  `panel/routing.rs`, `router/replay.rs`)
- F16-7, F17-4 - makers without a lab, vendor prefix matched anywhere - labs added, match at a
  boundary, declared lab case-folded (`panel/plan.rs`)
- F16-8, F17-3 - a missed upper-tier seat was silent - the plan warns; one-lab rule kept
  (`panel/plan.rs`)
- F16-10, F17-9 - `.ps1` could cut a surrogate pair and its `catch` path lacked the waiting rule;
  "byte-identical scripts" claimed - fixed, claim withdrawn (`plugin/hooks/c3-hook.ps1`)
- F17-6 - "a rewritten delegation rule is never edited" claimed - wording: `--fix` only appends
  the missing section (`setup/rules.rs`)

Not fixed by decision:

- F16-5 (a throwaway worktree path replaced by a junction after the non-following check, before
  the removal) - decided boundary: it needs a local process with the user's own rights racing a
  check-then-act window.

## CURRENT invariants claimed

- No GitHub-controlled text reaches a session: the CI line is a fixed template with the local
  branch (a valid ref name, else no line) and a digits-only run id; no title is requested.
- A pull-request run (`pull_request`, `pull_request_target`) decides nothing.
- Every git and `gh` process of the check, and of the hook's branch line, runs under one deadline;
  when it fires each child and its descendants are killed and reaped and none starts later.
- The check asks about the remote's default branch when it has no local twin (`c3.defaultBranch`
  wins); with no default it is silent; runs are ordered by parsed time; an unparseable stamp
  silences it; 20 runs are asked for.
- A remote is GitHub's only by its host; the gate is one file per repository, never read-modify-
  write, and an older shared file is ignored.
- `router explain` seats a lab by the panel's own election and draw pools; `replay` weighs the
  unrounded values when the record has them and says so when it has not.
- A vendor prefix matches only at a boundary; `opus` / `sonnet` / `haiku` / `fable` only as the
  whole id; `lab_of_host` and the prefix table agree for every host the table knows.
- On a weighty purpose a missed upper-tier seat is a warning naming purpose, lab and entries.
- `c3-hook.ps1` never splits a surrogate pair and its `catch` path prints the waiting rule;
  `c3 doctor --fix` only appends the missing delegation section and changes no existing byte.

## Evidence

- Local checks: fmt, clippy `-D warnings`; c3-core 251; c3 lib 828; every other c3 test binary
  green; c3-cli `ci_check` 10, `hook_scripts` 5, `delegation_rule` 6, `finish` 8,
  `panel_light_required` 2; `branches`, `init_doctor`, `update`, `wave9_consilium`,
  `notspooled_parity`, `wave6_live_diffs` green. Some binaries were still running at commit time.
- CI: see the launch note.
- NOT covered by a test: the sources record no gap for this part beyond the binaries that had not
  finished locally.

## Questions

- **Q1.** For each finding id in the table: `fixed`, `still-open` or `partly`, with `path:line` of
  the code that decides it. Do not report on ids outside this part.
- **Q2.** Can a refusal now fire for a legitimate case (a valid but unusual branch name dropping
  the line, a GitHub remote reached through an ssh alias or an enterprise host, a default branch
  that exists only on the remote, a lab prefix that now keeps out a seat the roster needs)?
- **Q3.** Does an error path leave something behind: a killed child's descendants on Windows (a
  `.cmd` shim) or Unix, a temp gate file, a stale gate file, a thread past the deadline?
- **Q4.** Does a fix change behaviour for callers it did not mean to touch: the branch line that
  shares the moved process runner, consumers of `c3 finish --json` reading `ci`, panels drawn by
  the new lab rules, `router explain` now running login and preflight checks?
- **Q5.** Windows vs Unix: do the `.ps1` and `.sh` hook scripts still agree for ASCII lines, and
  does a CHANGELOG sentence of this part claim more than the code does?
- **Q6.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
