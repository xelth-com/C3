# Handoff 06 - Claude: diff-review of 0.2.5, part 2 of 5 - repository repair and c3 branches

Date: 2026-11-22. Range: `a115422..23b1455` (base `a115422`, head `23b1455`; one commit).

You run in a throwaway checkout of the head commit of this range; the change itself is the patch
file named in your prompt (`.collab/` and `.eck/` left out). You may have no shell: read the patch
and the files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, send something private off the machine, or report a
result that is not true (a failed reviewer shown as usable, a usable one shown as failed) - not
style.

## Delta since the last review

A first panel reviewed `v0.2.4..ba43ff6` and returned 36 findings. This range holds (a) the fixes
for those findings that concern the repository model repair, `C3_CHILD` and `c3 branches` (F02-1
to F02-11, F03-1, F03-3, F03-4, F03-6, F04-1, F04-4, F04-6, F04-7, F04-9) and (b) one tech-debt
item (a crashed kimi run's worktree). Two pieces:

- **P1. Repository repair and `C3_CHILD`** (`23b1455`; `crates/c3/src/setup/repo_models.rs`,
  `crates/c3/src/setup/doctor.rs`, `crates/c3/src/cli/doctor.rs`, `crates/c3/src/hook/mod.rs`,
  `crates/c3-core/src/claude.rs`, `crates/c3-core/src/host.rs`, `crates/c3/src/engines/mod.rs`,
  `crates/c3/src/engines/claude_auth.rs`, `crates/c3/src/engines/subprocess.rs`,
  `crates/c3/src/maintain/mod.rs`). The repair of `.claude/agents/*.md` pins and the root
  `AGENTS.md` writes through a temporary file and a rename, rewrites only a validated Claude id,
  starts no process and does nothing inside a C3 child; `C3_CHILD=1` is set on every child.
- **P2. `c3 branches`** (same commit; `crates/c3/src/setup/branches.rs`,
  `crates/c3/src/cli/branches.rs`, `crates/c3/src/cli/doctor.rs`, `crates/c3/src/hook/mod.rs`,
  `crates/c3/src/cli/finish.rs`, `crates/c3/src/engines/kimi.rs`). Descriptions are read
  byte-exact, git calls take no optional locks, the default branch no longer follows the
  checkout, and a stale kimi throwaway worktree is found and removed by `doctor --fix`.

Two conflicts between the pieces were resolved by hand: `hook/mod.rs` keeps P1's 2 s
`branch_line_within` wrapper around P2's `branches::hook_line`, and `cli/doctor.rs` keeps both
pieces' code, with the stale-worktree removal skipped inside a C3 child.

## CURRENT invariants claimed

- A repair writes a sibling temporary file, synced and read back; links are checked again, the
  original is re-read and compared with the snapshot the edit came from (a changed file is left
  as it is); only then a rename. A symlink, a junction, a file with more than one hard link and a
  file under a linked `.claude` / `.claude/agents` are never edited; each skip is reported.
- Only an id of the claude engine's model table or of the published shape
  `claude-(fable|opus|sonnet|haiku)-<digits>[-<digits>]` is rewritten to an alias; `[1m]` is kept;
  only the value changes (quotes and a trailing `# comment` stay).
- `AGENTS.md` changes only when exactly one successor id results from the active entries that are
  neither protected nor on request, each asked on its own.
- The hook's repair starts no process (root found by walking up to `.git`); the branch line's git
  queries share one 2 s deadline and never run `status` or `cherry`.
- `C3_CHILD=1` reaches the claude engine's turn and probes, workers and the detached `c3 maintain`;
  the hook repair, `c3 doctor --fix` and the stale-worktree removal do nothing under it;
  `C3_CHILD=0` / `false` / `off` / `no` does not mark a child.
- `--set` / `--clear` read the description NUL-framed and never trim it; a first line is C3's only
  when it parses as a full record; a value C3 cannot read exactly, or one changed between read and
  write, is never written over; a name git's `check-ref-format` rejects is refused.
- Every git call of the listing, doctor, the hook and `c3 finish` runs with `GIT_OPTIONAL_LOCKS=0`;
  `.git/index` stays byte-identical. The full listing runs `git cherry` for at most 50 branches.
- The default branch comes from `c3.defaultBranch`, `origin/HEAD`, `init.defaultBranch`, `main`,
  `master`, else it is unknown; no branch is called merged on a failed query.
- A worktree is C3's throwaway only when directly in the temp directory and named exactly
  `c3-kimi-wt-<32 hex>`; one whose owner record (pid and start) is gone, or with none older than
  6 hours, is stale and removed link-safely by `doctor --fix`.

## Evidence

- Local suite after `23b1455`: fmt, clippy `-D warnings`, every test binary of c3-core, c3 and
  c3-cli (40 binaries, 1331 passed, 0 failed). CI green on Linux and Windows (run 45036679436).
- Tests use a fake claude launcher and real git; one proves `.git/index` byte-identical on a stale
  stat cache.
- Known and left open: `find_repo_root` ignores `GIT_DIR` / `GIT_WORK_TREE`; the 2 s hook deadline
  could flake a hook test on a loaded runner; `child_env_allowed` now lists `C3_CHILD`; the pinned
  plugin harness, which may compare it, was not run.
- NOT exercised live: the hook repair in a real Claude session, `--set` on a real foreign
  description, a stale worktree from a real crashed kimi run. Nothing in this part was exercised
  against a real reviewer CLI.

## Questions

- **Q1. Repair writes (P1).** Name any path where the hook or `doctor --fix` writes outside the
  repository (a symlink, junction, `..`, a hard link made after the re-check, a rename over a file
  held open on Windows), loses a mode, BOM, encoding or line ending, edits more than one value, or
  leaves a temporary file on an error.
- **Q2. What is rewritten (P1).** Name any frontmatter or `AGENTS.md` shape that makes the repair
  rewrite a non-Claude or floating value, miss a real pin, edit a line that is not the `model:`
  key (indented, multi-line, in a body), or seat a wrong successor.
- **Q3. Child mark (P1).** Is `C3_CHILD=1` set on EVERY child C3 starts (reviewer, worker, probe,
  update, smoke, detached pass), on Windows and Unix? Can a child still repair or remove something?
- **Q4. Descriptions and races (P2).** Can `--set`, `--clear` or `--default` lose a foreign
  description, write the wrong branch key, or overwrite a concurrent writer when two `c3` processes
  or another session's commit run at once? Odd branch names, CRLF, non-UTF-8 values.
- **Q5. Read paths and classification (P2).** Does any read path write or lock (config, index,
  refs), or leave a git running past the hook's 2 s deadline? Wrong `merged` / unknown result:
  detached HEAD, no upstream, a tag named like a branch, default branch not `main`, many branches.
- **Q6. Stale worktrees (P2) and claims.** Can a user's or a live reviewer's worktree be taken for
  a stale throwaway and removed (pid reuse, clock, a temp directory that is a link)? Does a
  CHANGELOG sentence of these pieces claim more than the code does?
- **Q7.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
