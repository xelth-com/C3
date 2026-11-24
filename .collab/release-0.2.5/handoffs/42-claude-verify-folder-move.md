# Handoff 42 - Claude: diff-review of 0.2.5 - verification of the folder-move fixes

Date: 2026-11-23. Range: `0eea5f1..ac26f5a` (base `0eea5f1`, head `ac26f5a`; one commit). The
commit holds only this piece of work; its files are `crates/c3/src/state_dir.rs` and
`state_dir/tests.rs`, `crates/c3/src/{providers,work/mod}.rs`,
`crates/c3/src/consult/{orchestrate/mod,detach,reviewed}.rs`, `crates/c3/src/panel/run.rs`,
`crates/c3/src/finish/mod.rs`, `crates/c3/src/pack/{snapshot,scout}.rs`,
`crates/c3/src/cli/{explain,index,doctor}.rs`, README, and the tests `finish.rs` and
`init_doctor.rs`.

You run in a throwaway checkout of the head of this range; the change is the patch file named in
your prompt (`.collab/`, `.c3/` and `.eck/` left out). You may have no shell: read the patch and the
files it names with your file tools. Everything is in this repository.

## Question

Are the findings F36-1 .. F36-8 really fixed by this commit, and did a fix introduce a new defect? A
finding is fixed only when the failure its trigger describes can no longer happen - not when a
comment or a CHANGELOG sentence says so.

## Delta since the last review

C3 inherited its per-project folder `.eck/` from another tool and now keeps everything in `.c3/`;
the first writing command moves an existing `.eck/` there with one rename. A first review returned
eight findings, six major: what surrounds the rename could split a project's journal and reports
between two folders, write through a link, or be set off by a command that only reads. This commit
fixes all eight. The rule: when anything is in doubt, C3 does not move, creates no second folder,
and says why.

| Finding | The defect | The fix | Main file |
| --- | --- | --- | --- |
| F36-1 | Two writers pass the checks; the loser treats a rename error as "use `.eck/`" and recreates it | Exclusive lock file in git's common directory (pid, start time, token; stale when its owner is gone or after 10 minutes); after waiting, or any rename error, the mover re-reads the state and uses `.c3/` when there | `state_dir.rs` |
| F36-2 | Leftovers were relocated only in the run that renamed; an interrupted move left the old `anchor` atop `.c3/` | Leftovers to `.eck/legacy/`, old report to `.eck/last-report.md`, inside `.eck/` first, each step repeatable; rename last; a half-done `.c3/` is finished by the next writing command | `state_dir.rs` |
| F36-3 | A `.c3` link, a linked `lastsnapshot` or a collab directory behind a linked `.eck` was followed | `.c3` link refused by every writing command, reported by `c3 doctor`; links inside `.eck/` never followed (a linked directory there blocks the move); per-component `symlink_metadata` and reparse-point test before each operation | `state_dir.rs`, `cli/doctor.rs` |
| F36-4 | `pack`, `explain`, `index` and others moved the folder while resolving a collab path | Two resolvers: the default never moves; only a real consultation, panel or work start and the finish family may | `state_dir.rs`, `providers.rs` |
| F36-5 | Classified before `.` and `..` were resolved: `sub/../.eck/collab` made a new `.eck/` | Normalised first (`.`, `..`, absolute paths, the file system's case rule), then classified | `state_dir.rs` |
| F36-6 | The live-run guard failed open and was not held to the rename | Unlistable directory, unreadable record, held `.consult.lock` or unreadable start time mean "may be live"; repeated under the move lock; a run starting under `.eck/` takes the same lock while creating its records | `state_dir.rs`, `consult/detach.rs` |
| F36-7 | README said the switch means C3 never touches `.eck/`; a collab directory under it was still used | Wording says what it does: with `C3_STATE_MIGRATE=off` C3 does not move `.eck/`, uses `.c3/` for its own files, and uses a collab directory under `.eck/` as given | README |
| F36-8 | A `--diff` patch excluded `.c3/` and `.eck/` only at the root | Excluded at any depth, under the pack walk's name rule | `consult/reviewed.rs` |

## CURRENT invariants claimed

- Two processes moving at once end with one `.c3/` and no `.eck/`; a process that loses the lock, or
  gets a rename error, reads the state again and uses `.c3/` when it is there.
- No path under the legacy name is created after a move; every writer takes its folder from the
  resolved state.
- The rename is the last step of the move; whatever is interrupted, `.c3/` never holds the old
  tool's `anchor`, and a half-done `.c3/` is finished by the next writing command.
- A `.c3` that is a link is refused by every writing command and reported by `c3 doctor`; a link
  inside `.eck/` is never followed.
- A command that only reads, and the default resolver, never move or create the folder.
- Every spelling of a collab path (`.`, `..`, absolute, another case where the file system ignores
  case) resolves to the same folder before it is classified.
- The live-run guard fails closed, is repeated under the lock, and a run that starts under `.eck/`
  takes the same lock while it creates its records.
- The lock file is stale when its owner is gone or after 10 minutes, and never lives inside `.eck/`
  or `.c3/`.
- A `--diff` patch holds no nested `.c3/` or `.eck/` path.

## Evidence

- Local: fmt clean, workspace clippy clean; the author ran the c3 library tests of the touched modules
  (169) and the c3-cli targets finish (25), init_doctor (20), snapshot_partb (7), work (29),
  claude_engine (18), mcp_integration (2). CI for `ac26f5a`: green on Linux and Windows.
- Tests are named in the CHANGELOG entry for F36-1 .. F36-8: units in `state_dir/tests.rs` and
  `finish.rs` / `init_doctor.rs` (two writers at once, a kill before the rename, links inside
  `.eck/`, every read-only command with `--collab-dir .c3/collab`, every collab spelling).
- NOT covered: the link checks use `symlink_metadata` plus the reparse-point test on each component
  right before each operation, NOT the directory-handle module of an earlier wave (`contain.rs`), so
  a small check-then-act window remains there: a component swapped for a link between the check and
  the rename is not caught.

## Questions

- **Q1. Two folders.** Name any path where two processes still end with two folders: a lock taken
  over while its owner lives (the 10-minute rule, pid reuse), a loser whose re-read comes before the
  winner's rename, a write under `.eck/` just before the rename.
- **Q2. Crash points.** Name a crash point, before or after the rename, that leaves a state the next
  writing command does not repair (an old `anchor` read as C3's, a leftover that stops the move for
  ever).
- **Q3. Read-only commands.** Name a read-only command (`pack`, `explain`, `index`, `providers`,
  `findings`, `consult --status`, `--dry-run`, `work --status`, `doctor`, an MCP tool) that still
  moves or creates something.
- **Q4. Collab spellings.** Name a spelling that still escapes the normalisation: a trailing dot or
  space on Windows, an 8.3 short name, mixed case, a UNC or drive-relative form, a path through a
  link.
- **Q5. Live runs and stuck moves.** A live run that is still moved (a record the guard does not
  list, a start between the lock and the rename); or a move that can now never happen (a stale lock
  never taken over, a forgotten link inside `.eck/`, the `.consult.lock` of a dead run).
- **Q6. Links.** Name a link still followed: a component swapped between check and use, a hard
  link, a `.git` that is a file, a common directory behind a link.
- **Q7. Claims.** Do the README and CHANGELOG sentences for the switch and for the move say exactly
  what the code does?
- **Q8.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
