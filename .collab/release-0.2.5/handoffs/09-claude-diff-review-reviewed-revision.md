# Handoff 09 - Claude: diff-review of 0.2.5, part 5 of 5 - every reviewer reads the revision under review

Date: 2026-11-22. Range: `08ae5d5..B1_COMMIT` (base `08ae5d5`, head `B1_COMMIT`; one commit).

You run in a throwaway checkout of the head commit of this range; the change itself is the patch
file named in your prompt (`.collab/` and `.eck/` left out). You may have no shell: read the patch
and the files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, send something private off the machine, or report a
result that is not true (a failed reviewer shown as usable, a usable one shown as failed) - not
style.

## Delta since the last review

A first panel reviewed `v0.2.4..ba43ff6` and returned 36 findings. This range holds (a) no fix
for them, and (b) one tech-debt item (a reviewer in a throwaway worktree was failed by a change of
the live tree). No model has read this change. One piece:

- **B1. Reviewed revision** (`B1_COMMIT`; new `crates/c3/src/consult/reviewed.rs` and
  `crates/c3/src/engines/worktree.rs`; changed `crates/c3/src/engines/{kimi,mod}.rs`,
  `crates/c3/src/consult/{args,mod,detach,dryrun,prompt}.rs`,
  `crates/c3/src/consult/orchestrate/{mod,engine,ledger,member,render,secondary,tree}.rs`,
  `crates/c3/src/cli/consult.rs`, `crates/c3/src/mcp/mod.rs`, `crates/c3/src/panel/run.rs`,
  `crates/c3/src/setup/branches.rs`).
  - `c3 consult --rev <rev>` resolves the revision to a full sha and runs every local-cwd engine
    (codex, agy, muse, claude, grok, kimi) in a throwaway detached worktree of it, with the brief,
    the task's `handoffs/` and `state.md` staged in. A panel resolves the rev once.
  - A claude-engine `diff-review` / `acceptance` without `--rev` runs in a worktree of HEAD and
    the prompt lists the paths uncommitted at launch.
  - `--diff <range>` (`a..b`, `a...b`, or one revision against the working tree, new files
    included through a temporary copy of the index) writes the patch to
    `handoffs/<NN>-<prefix>-<slug>.patch`. A brief naming a git range, given to any engine but
    codex without `--diff`, prints a warning.
  - `KimiWorktree` moved into `engines/worktree.rs`. `c3_consult` over MCP gained `rev` and `diff`.
  - A reviewer in a throwaway worktree is now checked against that worktree, not the live tree.

Motivation: a claude-engine reviewer answered HOLD from documents alone for a branch in a sibling
worktree; a grok diff-review spent ~15 tool calls on `.git` archaeology; kimi was failed by a
live-tree edit it did not make.

## CURRENT invariants claimed

- `<rev>` is resolved to a full sha and an unknown revision is refused before anything starts; one
  worktree per run (not per turn); a panel resolves the rev once, each member gets its own
  worktree of that commit; the http engine with `--rev` only warns.
- Staging copies the brief, the task's `handoffs/` (`*.events.jsonl` left out) and `state.md` at
  their repo-relative paths and never writes through a link the checked-out revision carries; a
  relative brief path outside the repository is refused for a claude diff-review / acceptance.
- The prompt, the handoff's `Isolation:` line and the ledger `worktree {reason, rev, sha}` name the
  full sha; a resume or fork of a thread recorded on another revision is warned about; findings
  stay bound to the live HEAD even under `--rev`.
- A worktree or patch that cannot be made is a capability failure; no reviewer starts.
- A reviewer in a throwaway worktree is checked against THAT worktree only: kimi (and agy with a
  writing tool in its log) fail on a write there; claude, grok and muse only warn; a live-tree
  change only warns and never fails the run or skips the timeout continuation.
- `--diff` leaves `.collab/` and `.eck/` out; the real index is untouched (new files are marked
  intent-to-add in a temporary copy); the ledger `diff` record describes the file written.
- The worktree is removed link-safely (every link unlinked first, never `--force` over a link) and
  is named so `c3 branches` recognises it as C3's.
- Without `--rev`, only a claude-engine `diff-review` / `acceptance` uses a worktree of HEAD.

## Evidence

- Local suite and CI: see the launch note.
- The kimi removal this part reuses was hardened in `08ae5d5` (part 4); the worktree name and owner
  record come from `23b1455` (part 2).
- NOT exercised live: no run of `--rev`, `--diff` or the HEAD worktree against a real reviewer CLI
  is on record; nothing in this part was exercised against a real reviewer CLI.

## Questions

- **Q1. Same revision.** Name any path where a reviewer reads a different revision than the one
  recorded: a branch or tag that moves between the panel's resolve and a member's checkout, an
  ambiguous name, a sha naming a blob or tree, a name starting with `-`, a shallow clone. Can
  findings bound to the live HEAD be applied to a review of another revision?
- **Q2. Staging.** What does staging copy, and can it copy a secret (an ignored or untracked
  file, a brief outside `.collab/`, an events stream) or follow a link out of the repository: a
  brief or `handoffs/` that is a symlink, junction or hard link, a `..` component, another drive,
  a `.collab` the revision itself carries as a link? Can it overwrite a tracked file?
- **Q3. `--diff` and its index.** Does the temporary index leave the real index and its lock
  untouched on every path, an error and a concurrent commit included? Can a range or revision name
  become a git option (`--output=`, a leading dash), the patch carry an untracked secret into the
  prompt, or the ledger record differ from the file written?
- **Q4. Cleanup.** Is the worktree with its owner record removed on EVERY error path (failed
  staging or launch, a dying panel member, a timeout, `--detach` killed, ctrl-c) and found by
  `c3 branches` / `c3 doctor` after a crash? Can the removal follow a link a reviewer made?
- **Q5. Classification and races.** Is a reviewer failed for a change it did not make (the live
  tree, the staged copy, a sibling's worktree), or kept usable after a write that should fail it
  (claude, grok and muse only warn)? Two `c3` processes or panel members on one rev: name
  collisions, owner records. Windows vs Unix: paths, links.
- **Q6. Privacy and entry points.** Does the http engine with `--rev` send the live tree to a
  remote model while the prompt implies otherwise? Do the uncommitted-path list and the ledger put
  private names off the machine? Are `rev` and `diff` from an MCP caller validated before they
  reach a git argument or a path? Behaviour in a C3 child (`C3_CHILD=1`). Does a CHANGELOG
  sentence claim more than the code does?
- **Q7.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
