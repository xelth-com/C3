# Handoff 01 - Claude: diff-review of the 0.2.5 wave

Date: 2026-11-22. Base: tag `v0.2.4` (commit `88b9169`). Head: `ba43ff6` (committed; the working tree
you are in IS the change, nothing uncommitted belongs to it).

The whole change as one patch: `.collab/release-0.2.5/handoffs/01-claude-diff-review.patch`
(`git diff v0.2.4..HEAD`, `.collab/` and `.eck/` left out). You may have no shell: read the patch
file and the files it names with your file tools. Everything is in this repository.

## Question

Is this wave safe to release as 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, or send something private off the machine - not
style.

## Delta since the last review

First review of this task. None of these five commits has been read by any model. Four pieces:

- **A. Repository model repair** (`0a6ad4d`; `crates/c3/src/setup/repo_models.rs`,
  `crates/c3/src/setup/doctor.rs`, `crates/c3/src/cli/doctor.rs`, `crates/c3/src/hook/mod.rs`,
  `crates/c3-core/src/claude.rs`, `crates/c3/src/engines/mod.rs`, the `c3 init` templates in
  `crates/c3/templates/`). `c3 doctor` reports a `.claude/agents/*.md` whose frontmatter `model:` is
  a concrete Claude id instead of an alias; `c3 doctor --fix` and the SessionStart hook (default ON)
  rewrite such a pin to its alias and let the root `AGENTS.md` follow the roster's successor id.
  Every child process C3 starts carries `C3_CHILD=1`, and the hook repairs nothing when it sees it.
  A Claude id of the published shape `claude-(fable|opus|sonnet|haiku)-<digits>[-<digits>]` is
  accepted in a roster without a release.
- **B. `c3 branches`** (`a65aa10`; `crates/c3/src/setup/branches.rs`, `crates/c3/src/cli/branches.rs`,
  doctor, hook, `crates/c3/src/cli/finish.rs`, the `02-delegation.md` template). Lists every branch
  and worktree with a recorded state (stored as the git branch description), `merged` by ancestry or
  content-merged by `git cherry`; `--set` / `--clear` edit only C3's line of the description. Doctor
  counts unknown branches as leftovers, the hook prints one line, `c3 finish` names them after its
  commit and never refuses.
- **C. `c3 maintain` public catalogs** (`c7820be`, `b086cc1`; `crates/c3/src/maintain/{web,models,
  version,mod,env}.rs`, `crates/c3/src/cli/maintain.rs`, `crates/c3/src/c3config.rs`,
  `crates/c3/src/telemetry/classes.rs`, `crates/c3/src/engines/kimi.rs`). A keyless GET of
  `https://openrouter.ai/api/v1/models` (and optional configured pages) yields successor candidates;
  a successor may now cross a major version; a public candidate needs date evidence; it is seated
  only after a smoke on the entry's own plan; `c3 maintain --models [--dry-run]` runs only the model
  step. Telemetry sends a model id by name when the maintain state holds it from a PUBLIC source.
  The effort check accepts a newer member of a declared model family. The kimi engine now unlinks
  every symlink / junction in its throwaway worktree before removing it.
- **D. GPT-6.2 ids** (`ba43ff6`): three ids added to the telemetry `openai` class list; docs.

`CHANGELOG.md` section `[Unreleased]` describes every item in prose.

## CURRENT invariants claimed

- A. The hook never runs git, never writes outside the repository (a linked directory or file is
  skipped), never fails or blocks a session, and does nothing under `C3_CHILD=1` or with
  `C3_REPO_MODELS=off` / `"repo_models": false`. Each repair edit changes exactly one line and keeps
  the file's other bytes (line endings included). Only a Claude pin is rewritten to an alias; an
  `AGENTS.md` id changes only to the roster's successor of the same family.
- B. `c3 branches` without `--set`/`--clear`, the doctor check and the hook line are read-only. A
  foreign branch description survives `--set` and `--clear`. The hook runs no `git cherry`, nothing
  inside a C3 child or outside a git repository. `c3 finish` never refuses because of branches.
- C. The catalog GET carries no key or identifying header, is https only, follows same-host
  redirects only, stops at 15 s / 4 MB / 5000 ids; every id passes the strict model-id shape before
  it is stored, compared, printed or written. A candidate older than the seated model by the
  catalog's date, an undated one and a page id are announced only, never smoked or seated. A
  protected (`ext.reserve_percent`) or on-request entry is never smoked or edited. `--dry-run`
  writes no state and edits no roster; `--models` leaves `next_due` and the engine records alone.
  An endpoint's own model list never enters the telemetry "published" set.
- C. The kimi worktree removal never follows a link out of the worktree.

## Evidence

- CI on GitHub for `b086cc1` green (run 44305276506); the run for `ba43ff6` (45022742138) was in
  progress when this brief was written.
- Locally after `ba43ff6`: `cargo fmt --check`, clippy `-D warnings`, the telemetry unit tests and
  the c3-cli `telemetry_parity` test pass.
- Live, this machine: `c3 maintain --models --dry-run` collected 354 ids from openrouter.ai, offered
  `gpt-6-astra -> gpt-6.2-astra` and did not seat it (protected entry), and rejected
  `grok-4.20` as older than `grok-4.7` by the catalog's date. `c3 maintain --smoke --engine codex`
  passed for `openai::gpt-6.2-astra` and `openai::gpt-6.2-sol`. `c3 branches` on this repository:
  `no branches besides main`.
- NOT exercised live: a real roster swap by a maintain pass, a `model_pages` entry, the hook repair
  on an `AGENTS.md` across a major, `c3 branches --set` on a branch with a foreign description.

## Questions

- **Q1. Repair (A).** Name any path where the hook or `doctor --fix` writes outside the repository
  (a symlink, a junction, a `..` component, a linked `.claude` directory), edits more than one line,
  changes line endings / BOM / encoding, rewrites a non-Claude or already-floating `model:` line,
  seats a wrong id in `AGENTS.md`, acts inside a C3 child (is `C3_CHILD=1` set on EVERY child C3
  spawns - reviewer, worker, probe, the detached maintain pass?), or fails / delays a session start.
- **Q2. Branches (B).** Can `--set` / `--clear` lose or corrupt a foreign description, or write to
  the wrong branch? Does any read path write (git config, index refresh, lock files)? Wrong
  `merged` / `unknown` / content-merged classification: detached HEAD, no upstream, default branch
  not `main`, a branch name with odd characters, worktrees of the kimi engine vs a user's worktree
  with a similar path. Cost of `git cherry` per branch on a repository with many branches.
- **Q3. Catalog (C), security.** Redirect handling, size and time limits, JSON parsing, anything a
  hostile catalog body or page could inject into the roster, a command line, a file path, the
  report or the hook line. Any header, cookie or env value that would identify the user.
- **Q4. Catalog (C), correctness.** Any path that seats an older, unrelated or wrong-family model,
  swaps a protected or on-request entry, smokes without date evidence, writes state or the roster on
  `--dry-run`, moves `next_due` on `--models`, or crosses a major where the plan would refuse.
- **Q5. Telemetry privacy (C, D).** Any path where a private endpoint's model name (or any other
  private string) is sent by name: the published-set sourcing, case folding, the `[1m]` suffix, the
  Claude-shape rule, the effort-family acceptance.
- **Q6. kimi worktree (C).** Does the unlink-then-remove order hold on Windows junctions and
  directory symlinks, without descending into a link target, and with nothing removed outside the
  worktree on any error path?
- **Q7.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
