# Handoff 40 - Claude: diff-review of 0.2.5 - containment by directory handles

Date: 2026-11-23. Range: `84dbca4..272920d` (base `84dbca4`, head `272920d`; one commit). The commit
holds only this piece of work; its files are `crates/c3/src/contain.rs` (new),
`crates/c3/src/engines/worktree.rs`, `crates/c3/src/consult/reviewed.rs`,
`crates/c3/src/setup/{repo_models,rules}.rs`, `crates/c3/src/lib.rs`,
`crates/c3-core/src/test_hooks.rs` and the two `Cargo.toml` files (new dependencies `cap-std` and
`cap-fs-ext`, version 4.0.3 each).

You run in a throwaway checkout of the head of this range; the change is the patch file named in
your prompt (`.collab/`, `.c3/` and `.eck/` left out). You may have no shell: read the patch and the
files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, lose or split a project's records, send something
private off the machine, or report a result that is not true - not style.

## Delta since the last review

Eight earlier review findings (F32-1, F24-1, F29-2, F18-5, F27-2, F16-5, F13-9, F28-1 in the
project's own numbering) were one defect class: C3 checks a path name (no link in any component,
inside the root) and then uses the SAME path name, so a directory swapped for a link between the
check and the use sends the operation elsewhere. Three of them had been recorded as accepted
boundaries (a local process racing a small window) until a reviewer rated the staging one a blocker;
the boundary is withdrawn. The new module `contain` opens a root once as a directory handle,
verifies it on the handle, and does every later operation relative to an open handle.

- **Module** (`contain.rs`, on `cap-std`): a root is a real directory, not a link, junction or
  reparse point; every component below it is opened one at a time without following a link; a link
  refuses, also one that stays inside the root.
- **Staging, destination side** (`engines/worktree.rs`): a file is written to a new temporary name in
  its directory and renamed relative to that directory's handle; a failed write removes what the
  call created through the same handles, and a removal that fails is reported, not ignored.
- **Staging, source side** (`consult/reviewed.rs`): the brief, the task's `handoffs/` and `state.md`
  are read through a handle on the repository root. A link on the way is now refused (brief) or
  skipped and counted (other entries); before, a link that stayed inside the repository was followed.
- **Link sweep and removal** (`engines/worktree.rs`): the post-checkout sweep of links that lead out
  of the worktree and the removal are one walker that never descends into a link; the worktree is
  removed through handles, then only `git worktree prune` runs - no `git worktree remove --force`,
  no recursive delete by path.
- **Repository repair and rules rewrite** (`setup/repo_models.rs`, `setup/rules.rs`): the file
  replacement is a temp file renamed relative to a handle on the repository root.
- **`--diff` patch write** (`consult/reviewed.rs`): the patch is written to a temporary file first
  and placed without replacing anything - by a hard link, or on a file system without hard links by
  a claim of the name (an empty file created exclusively, which a sibling run waits for) replaced
  by a rename. No partial patch is left under its name.
- **Test hooks** (`c3-core/src/test_hooks.rs`): `CODEX_CONSULT_TEST_CONTAIN_RACE` swaps a directory
  for a link between validation and use; two more force the no-hard-link and failed-write paths;
  `set_thread_hook` sets a hook for one thread only.
- **Accepted side effect:** the worktree's root handle stays open during a run, so on Windows that
  directory cannot be renamed while a reviewer runs.

## CURRENT invariants claimed

- No staging write, staging read, link sweep, removal or repository repair can leave its root,
  whatever happens to path names between the check and the use.
- A junction or symlink as a path component is refused or reported as a link, never followed; a root
  that is itself a link is refused.
- A failed write leaves no temporary or partial file; a failed cleanup is reported.
- The worktree's final removal never uses `git worktree remove --force` and never a recursive delete
  by path; only `git worktree prune` touches git's registration.
- A `--diff` patch file is never overwritten, never written through a link, and never visible
  partial under its name, also on a file system without hard links.
- Behaviour without a race is unchanged, except the link cases named above.

## Evidence

- CI for `272920d`: green on Linux and Windows (all three jobs). The commit was not rebuilt locally
  on the development branch; its files equal the tree the change was built and tested in.
- Proof test: `contain::tests::cap_std_refuses_a_link_as_a_path_component` - a junction and a
  directory symlink (where one can be made) refuse as a path component, pointing out or in; a root
  that is a link is refused; on Windows a held directory and its ancestors cannot be renamed.
- Race tests through the hooks cover staging, publishing, reading the brief and `handoffs/`, the
  walk, replacing a repository file, and the patch placement without hard links.
- NOT exercised live: the directory-symlink half of the Windows proof was skipped on the development
  machine (no privilege) and runs where symlinks can be made.
- NOT covered: the roster writer of `c3 maintain` (`crates/c3/src/maintain/models.rs`) and the owner
  record in git's admin directory are still path-based.

## Questions

- **Q1. Remaining path names.** Name any operation in the converted code paths that still resolves a
  path name after validation: find every remaining `std::fs::` call (and `exists`, `is_dir`,
  `canonicalize`, a `Command` with a path argument) in the files above and say what each acts on.
- **Q2. `cap-std` on each platform.** For the cases C3 relies on - a junction, a directory symlink,
  a root that is itself a link, an absolute link target - what do `cap-std` and `cap-fs-ext` 4.0.3
  do on Linux, macOS and Windows, and does C3's own code add the refusal where the crate follows?
- **Q3. Legitimate use that changed.** Name a behaviour that broke for a legitimate case: a
  repository with a symlink on the way to the brief, a locked or read-only file in the worktree, a
  held handle that blocks the operator.
- **Q4. Error paths.** Name any path that leaves a temporary file, a claim file or a half-removed
  worktree: a failed rename or cleanup, a crash between two steps, a reviewer still running inside
  the worktree during removal.
- **Q5. Patch placement.** Without hard links: two runs at once, a crash between the claim and the
  rename, a sibling waiting on a claim whose owner is gone. Can two runs end with one patch, a wrong
  sha256 in the ledger, or a claim that never goes?
- **Q6. Test hooks.** Can any of the new hooks take effect outside test mode, from an inherited
  environment or a repository file?
- **Q7. Claims.** Does the CHANGELOG entry or a comment in `contain.rs` claim more than the code
  does?
- **Q8.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
