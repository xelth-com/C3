# Handoff 41 - Claude: diff-review of 0.2.5 - the tooling debt

Date: 2026-11-23. Range: `272920d..0eea5f1` (base `272920d`, head `0eea5f1`; one commit). The commit
holds only this piece of work; its files are `crates/c3/src/{bounded,ci_check}.rs`,
`crates/c3/src/finish/{checks,mod,secret_gate}.rs`, `crates/c3/src/pack/redact.rs`,
`crates/c3/src/setup/{branches,doctor,repo_models}.rs`, `crates/c3/src/cli/{branches,doctor,finish}.rs`,
`crates/c3/src/{hook/mod,liveness/proc,mcp/mod,consult/args}.rs`, the two hook scripts
`plugin/hooks/c3-hook.{ps1,sh}`, the plugin skill text, README and the tests in `crates/c3-cli/tests/`.

You run in a throwaway checkout of the head of this range; the change is the patch file named in
your prompt (`.collab/`, `.c3/` and `.eck/` left out). You may have no shell: read the patch and the
files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, lose or split a project's records, send something
private off the machine, or report a result that is not true - not style.

## Delta since the last review

The last open debt of the tooling (`c3 finish`, the SessionStart hook, `c3 doctor`, `c3 branches`),
fifteen items, closed in one commit. One bullet per item:

1. **Secret gate** (`pack/redact.rs`, `finish/secret_gate.rs`, `mcp/mod.rs`): a value counts as a
   secret only when it can be one - a single dictionary-shaped word (letters only, one case, fewer
   than 24) never does, and under the bare keyword `auth` only a key-shaped value does (16+
   characters, no white space, two of lower case, upper case, digits, symbols); `authorization` is a
   keyword of its own. The MCP tool `c3_finish_task` gains `allow_secret`. Packs and `--diff`
   patches share the rules.
2. **Pre-finish checks** (`finish/checks.rs`, `finish/mod.rs`): a repository lists commands in
   `.c3/config.json` under `finish_checks`; `c3 finish` runs them first, in the repository root,
   through the shell, with `C3_CHILD=1` and 120 s each; a failing one refuses with its name, exit
   code and last 20 lines, nothing written, staged or committed. `--skip-checks` goes on and says so
   in the output and the journal; a list that does not parse refuses.
3. **`c3 branches`** (`cli/branches.rs`, `setup/branches.rs`): `--record-unknown` records a state on
   every unmerged branch with none; `--close <branch>` needs a branch merged by ancestry, one that
   `git branch -d` accepts, and a clean, unlocked worktree that is not the start folder, then
   removes without `--force`. Both refuse inside a C3 child.
4. **Hook line cap** (`hook/mod.rs`, both scripts): `c3 hook` cuts a printed line at 400
   characters on a character boundary; the scripts forward lines as they are.
5. **Hook branch deadline** (`hook/mod.rs`, tests): a test-mode override of the 2 s deadline.
6. **Git version** (`setup/doctor.rs`, `consult/args.rs`): doctor reports git older than 2.30;
   `consult --rev` / `--diff` refuse before starting on such a git.
7. **Knowledge-base paragraph** in the plugin skills (text only).
8. **Memory-reaper paragraph** in the coordinate skill (text only).
9. **Deadline stop, F31-1** (`bounded.rs`): `ScanChildren` remembers every child it started; the stop
   reads the process table and ends descendants by parent pid and, on Unix, by process group, only
   when they started at or after the child; the caller of `within` joins the worker.
10. **CI check, default branch, F31-2** (`ci_check.rs`): without `origin/HEAD` and
    `c3.defaultBranch` it asks `gh repo view` once, remembers the answer for a day, and otherwise
    stays silent.
11. **Daily gate, F31-3, F30-3, F31-5, F30-6** (`ci_check.rs`): the day's claim file is created
    exclusively before `gh` starts; the loser prints nothing; the prune removes claim and temp files
    older than 30 days.
12. **GitHub hosts, F31-4, F30-5** (`ci_check.rs`): only `github.com`, `www.github.com` and
    `ssh.github.com` are GitHub remotes.
13. **Registered until exit, F32-4** (`bounded.rs`): a child stays under the deadline until its exit
    is confirmed, not until its pipes close.
14. **Cannot tell, F12-1** (`liveness/proc.rs`): an unreadable `/proc/<pid>` reads as "cannot tell",
    never "gone".
15. **Repository recognition, F13-2** (`hook/mod.rs`, `setup/repo_models.rs`): git's own test -
    `HEAD`, plus `objects` and `refs` in the common directory, also behind a `gitdir:` file.

## CURRENT invariants claimed

- A deadline stop leaves no descendant of a registered child alive and takes no process that started
  before its child.
- A failing pre-finish check stages, writes and commits nothing; a list that does not parse refuses.
- `--close` removes nothing that is not merged and never forces; `--record-unknown` records only on
  branches with no state.
- The CI check never guesses a default branch and asks GitHub at most once a day per repository.
- A plain word is not a secret, while a key-shaped value and every service-shaped key still are.
- `--close` and `--record-unknown` refuse inside a C3 child; the branch and CI notes never refuse a
  finish, and the hook's CI check never runs inside a C3 child.
- A claim file held by a killed hook blocks only for the rest of that UTC day.
- The two hook scripts print the same text, non-ASCII lines included.
- An unreadable process entry is never reported as a dead process.

## Evidence

- On the development branch: fmt clean, `cargo clippy --workspace --all-targets -- -D warnings`
  clean. The tests of the combination of this commit with the previous one were left to CI.
- CI: the next commit (`ac26f5a`), which contains this one, is green on Linux and Windows - the Linux-only
  paths of this wave ran there for the first time.
- Tests are named in the CHANGELOG entries for this commit, for example
  `the_deadline_reaps_what_an_exited_shim_left_behind` in `bounded.rs`.
- NOT exercised live: the Linux-only paths (process groups in the deadline stop, `/proc` liveness)
  had never been run before CI.

## Questions

- **Q1. Deadline stop.** Name any path where it kills a process that is not C3's (pid reuse, a
  process started before the child, a process group shared with something else), or leaves one
  alive (a descendant that changes its parent or group, a table read that races a fork, a child
  already reaped).
- **Q2. Secret gate.** Name three shapes of real keys (a cloud access key, a token with a prefix, a
  long password under `auth`) and check each against the new rules: does a real secret now pass? Does
  a plain word still refuse? Does `--diff` redaction lose anything?
- **Q3. Pre-finish checks.** Can a check be bypassed, run after something was staged, hang `c3
  finish` past its deadline, or leave a child running? Can `--skip-checks` be set without being said?
- **Q4. `--close` and `--record-unknown`.** Name any path that removes or records the wrong thing: a
  branch checked out in another worktree, a worktree with ignored files, the start folder, a branch
  with a foreign description.
- **Q5. CI check.** Name any path where a wrong default branch is remembered for a day, two
  repositories share one gate file, or a claim is never released; and any case where `gh` output
  reaches the terminal unfiltered.
- **Q6. Hook cap.** Does the 400-character cut remove a line, a prefix or a marker the scripts'
  filters, order, de-duplication or count rules depend on?
- **Q7. Recognition and liveness.** Name a repository git accepts that the hook now rejects, or one
  git rejects that it accepts; a process entry that now reads "cannot tell" where the old code
  gave a safe answer.
- **Q8.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
