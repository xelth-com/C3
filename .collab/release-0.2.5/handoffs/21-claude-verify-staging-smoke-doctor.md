# Handoff 21 - Claude: verification of the 0.2.5 review fixes, part 1 of 3 - staging, smoke, doctor

Date: 2026-11-23. Range: `4f43074..99e2da7` (base `4f43074`, head `99e2da7`; one commit).

You run in a throwaway checkout of the current head of the repository, which contains this commit
and the later ones; the change to verify is the patch file named in your prompt (`.collab/` and
`.eck/` left out). You may have no shell: read the patch and the files it names with your file
tools. Everything is in this repository.

## Question

Are the findings listed below really fixed by this commit, and did a fix introduce a new defect? A
finding is fixed only when the failure its trigger describes can no longer happen - not when a
comment or a CHANGELOG sentence says so.

## Delta since the last review

A first panel read the changes after tag `v0.2.4` in five parts; for this part it returned 32
findings (F10-1..7, F11-1..12, F12-1..4, F13-1..9). This commit is the fix wave. One line per item:
ids - defect - fix - main files.

- F11-1, F10-1, F10-2 - agy write passed as a read, or a read failed - parameters of all events of
  a step merged, unlisted tool = writing (`engines/agy.rs`, `tree_check.rs`)
- F11-2 - bound input changed during an agy run seen as drift - fails the outcome, keeps the reply
- F11-3 (blocker) - staging could write through a link out of the worktree - destination checked
  in the revision and on disk, exclusive create + rename (`engines/worktree.rs`, `reviewed.rs`)
- F11-4, F10-3 - `..` brief refused, unreadable brief failed, staging failure a provider failure
  - normalised, taken from HEAD, typed pre-start refusal (`consult/reviewed.rs`)
- F11-5 - prompt listed untracked names - tracked paths as data, a count for the rest
- F11-6, F10-4 - member judged against findings a sibling changed - frozen record per member
  (`orchestrate/member.rs`, `panel/member.rs`)
- F11-7, F10-5 - smoke passed without proof - `Unproven`, held like a failure (`maintain/smoke.rs`)
- F11-8 - rollout evidence unchecked - absolute Codex home, session id, terminal record, retries
  (`maintain/smoke.rs`, `c3-core/src/quota.rs`)
- F11-9 - turn exiting between polls escaped the sub-agent guard - one inspection after exit
  (`engines/subprocess.rs`)
- F11-10 - secondary prompts lacked the reviewer-role sentence - added (`secondary.rs`; the agy
  denial-retry prompt is left to the next commit)
- F11-11, F10-7 - death record named another run's stream - stream kept per turn
  (`consult/detach.rs`)
- F11-12 - `c3 providers` miscounted failed runs - `reply_kept` recorded (`providers.rs`)
- F13-1, F13-8, F12-1 (blocker) - `doctor --fix` removed through a link or a live worktree - link
  reported, owner record mandatory, 6-hour rule gone (`engines/worktree.rs`, `setup/branches.rs`)
- F13-2 - stray `.git` let the hook repair a non-repository - strict root test
  (`setup/repo_models.rs`)
- F13-3 - any date text made a branch record - `YYYY-MM-DD` required (`setup/branches.rs`)
- F13-4 - `c3 update` children unmarked - `C3_CHILD=1`; branch edits refuse in a child
  (`update/mod.rs`, `cli/branches.rs`)
- F13-5 - hook deadline left git running - registered children killed (`hook/mod.rs`)
- F13-6 - `AGENTS.md` followed to an older model - dated comparison (`setup/repo_models.rs`)
- F13-7 - failed `git cherry` looked checked - counted unchecked (`setup/branches.rs`)
- F12-2, F12-3, F12-4 - sentences claimed more than the code - code or wording corrected

Not fixed by decision:

- F10-6 (older Codex CLI may reject unknown `-c` keys) - rejected: Codex ignores unknown config
  keys; the keys were exercised on 0.169.0.
- F13-9 (window between the last re-read and the write) - decided boundary: closing it needs a
  local process with write access to the repository, which can edit the files directly.

## CURRENT invariants claimed

- Staging never writes outside the reviewer's worktree: no destination component is a link,
  junction or reparse point; any doubt refuses the run before a reviewer starts.
- An unproven smoke is treated like a failed one by every automatic path (update, recovery smoke,
  model swap).
- `c3 doctor --fix` never removes through a link, never removes a worktree without an owner record
  unless git calls it prunable, never removes when liveness is `cannot tell`; creating the
  worktree fails when its owner record cannot be written.
- The hook repair accepts a `.git` only when it holds `HEAD`, does nothing when `GIT_DIR` is set,
  and starts no process; its deadline kills and reaps the scan's git children.
- `C3_CHILD=1` marks every child `c3 update` starts; `c3 branches --set`, `--clear`, `--default`
  refuse in a child; only a `YYYY-MM-DD` record is C3's, other descriptions stay byte for byte.
- An agy write is never classified as a read; a changed bound input fails the outcome.
- A panel member is judged only against the findings frozen at panel start.
- The isolation paragraph names tracked paths as data and untracked ones as a count.
- `AGENTS.md` does not follow to a model dated older; a failed `git cherry` stays unchecked.

## Evidence

- Local checks: fmt, clippy `-D warnings`; c3-core and c3 all green (793 lib tests); c3-cli
  `kimi_engine` 18, `claude_engine` 18; on the repair half alone `branches` 14, `init_doctor` 15,
  `update` 12. The other c3-cli binaries were left to CI.
- CI for `99e2da7`: green on Linux and Windows at the second attempt; the first attempt's Windows
  job hung in its test step and left no log.
- NOT covered by a test: the bound-input rule for muse, grok and claude outside a worktree (still
  a warning); a panel re-seat after a staging refusal; agy's Cascade reader names against a fixture.

## Questions

- **Q1.** For each finding id in the table: `fixed`, `still-open` or `partly`, with `path:line` of
  the code that decides it. Do not report on ids outside this part.
- **Q2.** Can a staging or removal check now refuse a legitimate case (a brief behind a normal
  directory, a cloud-sync reparse point, a temp directory that is itself a link), or still pass a
  hostile one (a link made between check and write, Windows and Unix)?
- **Q3.** Does an error path leave something behind: a staged temp file, a worktree without its
  owner record, a half-written record, a smoke scratch directory, a lock?
- **Q4.** Does a fix change behaviour for callers it did not mean to touch: the agy allow-list
  failing a real reader, the mandatory owner record, `C3_CHILD` refusals hitting a normal
  coordinator, the hook repair going silent in a real repository?
- **Q5.** Is `Unproven` handled on every path that reads a smoke result, or does one still count
  it as a pass? Does a CHANGELOG sentence of this part claim more than the code does?
- **Q6.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
