# Changelog

All notable changes to C3 are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the versions follow
[Semantic Versioning](https://semver.org/). The specification of every compatibility wave is the
PowerShell plugin [`claude-codex-consult`](https://github.com/xelth-com/claude-codex-consult) at its
tag `v0.6.1`; the finding ids (Fnn-k, En, Dn) are those of the parity task's reviews and of the
plugin's own waves. Each wave's details are in `docs/port/wave*.md`.

## [Unreleased]

## [0.2.5] - 2026-11-24

### Added

- **`c3 finish` runs the repository's own checks first.** CI sat red for a week on a formatting
  failure nobody had run locally. A repository lists its checks in `.c3/config.json` as
  `"finish_checks": ["cargo fmt --all -- --check", ...]` (per repository; C3's own configuration is
  per machine, and the file is only read, never created). `c3 finish` runs them before it writes or
  stages anything - in order, in the repository root, through the shell, each with `C3_CHILD=1` and
  under its own 120 s deadline through the shared deadline-bound runner - and the first that fails
  refuses the finish with its name, its exit code and the last 20 lines of its output: nothing is
  written, staged or committed; the whole phase stops at 10 minutes (see the fix below).
  `--skip-checks` (the MCP tool: `skip_checks`) goes on without them and says so in the output and
  in the journal entry; a list that does not parse refuses (fix it, or skip); no list, no checks.
  Tests: `finish::checks` unit tests (the list, the tail, the message, a check run in the repository
  as a child, a check stopped at its deadline) and `finish.rs`'s
  `a_failing_pre_finish_check_refuses_with_nothing_staged` and
  `passing_pre_finish_checks_run_in_the_repository_as_a_child`.
- **`c3 branches --record-unknown` and `--close`.** The two leftovers of the branch rules of
  2026-11-18. `--record-unknown --state <s> --note "<text>"` records that state on every branch the
  listing calls unknown (not merged, no recorded state), one line per branch, nothing else - for a
  project whose first listing shows every old branch as unknown. `--close <branch>` does what a
  session does by hand after a merge: the branch must be merged into the default branch by ancestry
  (or have every patch there), `git branch -d` itself must accept it (checked before anything is
  touched), and its worktree, when it has one, must hold nothing outside the committed tree (no
  change, no untracked and no ignored file - see the fix below), not be locked, not be the start
  folder nor the checkout C3 runs in, and be free of links that lead outside it; then `git worktree remove`
  without `--force` and `git branch -d`. Anything else is refused with the reason, nothing is ever
  forced, and both refuse inside a C3 child. Tests: `branches.rs`'s
  `record_unknown_records_every_unknown_branch_and_nothing_else` and
  `close_removes_a_merged_branch_and_its_clean_worktree_and_refuses_the_rest`.
- **`c3 doctor` checks the git version.** Since wave R5 revisions reach git after
  `--end-of-options`, which needs git 2.30. `c3 doctor` reports an older git as a problem - one line
  naming the installed and the required version (`--json`: `git {version, problem}`) - and `c3
  consult --rev` / `--diff` on an older git refuse before anything starts instead of failing inside
  git. Tests: the parser unit test in `setup/doctor.rs` and `init_doctor.rs`'s
  `an_old_git_is_a_doctor_problem_and_refuses_consult_rev_and_diff` (the installed version from a
  test hook).

- **The operator sees which untracked files a one-revision `--diff` patch carries or leaves out
  (F18-4 / F19-2 / F20-6).** Before the reviewer starts, the run says how many untracked files the
  patch includes - (0.2.5 R8) or, by default, left out - and names up to ten of them (as data: a
  name with a backtick, a control character or a format character is counted, never shown), and
  the ledger's `diff` record keeps the same list (`untracked` / `untracked_count`, or
  `untracked_left_out` / `untracked_left_out_count`). Wave D1 carried the untracked files by
  default and added `--diff-tracked-only` to leave them out; since wave R8 the default is the
  tracked changes only, `--diff-untracked` carries them, and `--diff-tracked-only` only names the
  default (see Changed). A panel and a detached run carry the flags to their members. Test:
  `a_one_revision_patch_leaves_untracked_files_out_unless_asked`.
- **Panel integration tests for `--rev` and for the re-seat after a staging refusal.** A real panel
  of two fake reviewers of different labs under `--rev` proves each member runs in its OWN
  throwaway worktree of the same sha (two working directories, neither the live checkout), both
  ledger entries and handoffs name that sha, both worktrees are gone afterwards and the task lock
  is released; a real panel whose optional kimi member cannot stage its brief (HEAD commits a link
  at the brief's path) replaces it from the reserve and finishes with the reserve's reply. Tests:
  `a_rev_panel_runs_each_member_in_its_own_worktree_of_the_same_sha`,
  `a_staging_refusal_is_reseated_from_the_reserve`.
- **A roster id spelled differently from the catalog is still matched.** A plan may write a model
  with a leading token the public catalog leaves out (`kimi-code/k3.1-pro` or `kimi-k3.1-pro` on
  the roster, `moonshotai/k3.1-pro` in OpenRouter's list), so the entry never matched and got an
  announcement at best. When no public source lists a roster id as it is, `c3 maintain` tries it
  once more without that token - only when the token names the maker of the rest or the plan's
  vendor by the panel's lab table (the endpoint host's lab alone does not count), the rest is a known maker's id, and
  the catalog lists exactly that rest under a vendor slug of that maker; nothing else is guessed. A
  match found that way is used for the dates and the successors exactly like a direct one, and the
  successor is written in the plan's own spelling (`kimi-code/k3.2-pro`: the token put back), then
  smoked on the entry's plan as always. Tests: the unit test of the alias (both spellings, the
  date gate, an unlisted seated id, a token naming nothing, the plan's vendor) and an end-to-end
  pass that seats `kimi-k3.2-pro` from `moonshotai/k3.2-pro`.
- **A new Claude model is known to telemetry without a release.** OpenRouter spells a Claude id
  with dots (`anthropic/claude-opus-4.5`), the claude engine with dashes (`claude-opus-4-5`), so a
  published Claude model stayed `other` until a release added it to the closed list. An id a
  TRUSTED public source lists under the `anthropic/` slug is now published in its dash form too
  (dots to dashes in the version part) when that form is of the Claude shape
  (`claude-(fable|opus|sonnet|haiku)-<digits>[-<digits>]`), and is sent by name; the OpenRouter
  route keeps sending the catalog's own form. An untrusted source, another slug or an id of another
  shape (`claude-3.5-sonnet`) changes nothing. Tests: unit tests in `maintain/models.rs` and
  `telemetry/classes.rs`, and `telemetry_parity`'s backfill of a claude reviewer on
  `claude-opus-9-3`.

- **`c3 consult --rev <rev>`: every reviewer reads the revision under review.** The bridge resolves
  `<rev>` to its full sha (a revision git does not know is refused before anything starts), makes a
  throwaway detached worktree of it under the temp directory, stages the brief and the task's
  `handoffs/` (event streams left out) and `state.md` into it at their repo-relative paths, and runs
  every turn of the reviewer there - for every engine that runs in a local working directory
  (codex, agy, muse, claude, grok, kimi; the http engine sends a pack of the live tree and is told
  so in a warning). The prompt says which revision the working directory holds, the handoff's
  `Isolation:` line and the ledger's new `worktree {reason, rev, sha}` record name the full sha, a
  panel resolves `<rev>` once and gives every member its own worktree of that same commit, and a
  `--mode resume|fork` of a thread recorded on another revision is warned about before the run. The
  worktree is the kimi engine's `KimiWorktree`, generalised (`engines::worktree`): same
  `c3-kimi-wt-<32 hex>` name that `c3 branches` recognises, same owner record, same link-safe
  removal (every link inside unlinked first, never `--force` over a link); staging never writes
  through a link the checked-out revision carries. A claude-engine reviewer reviewing a branch
  checked out in a sibling worktree answered HOLD from documents alone (xelixir acceptance panel
  741be853): `--restricted` kept it out of that worktree.
- **`c3 consult --diff <range>` hands the reviewer the change as a patch.** `git diff <range>`
  (`base..head`, `base...head`, or one revision diffed against the working tree with new files
  included - marked intent-to-add in a temporary copy of the index, the real one untouched; `.collab/`
  and `.eck/` left out) is written into the task's handoffs as `<NN>-<prefix>-<slug>.patch` next to a
  conventionally named brief (else `<nn>-<prefix>-diff.patch`), the prompt says "The change under
  review is the patch at <path>", and the ledger's new `diff {range, patch, sha256, files, bytes}`
  record binds the review to it. A brief (or ask) that names a git range, given to an engine without
  a shell (every engine but codex) and without `--diff`, now draws a printed warning: a grok
  diff-review spent ~15 tool calls on `.git` archaeology following `git diff main...branch`.

- **A red CI run is named, not left for someone to notice.** `c3 finish`, after its commit, and the
  SessionStart hook ask the GitHub CLI for the latest completed run of the default branch (`gh run
  list --branch <default> --limit 20 --json status,conclusion,databaseId,createdAt,event`) and print
  one line when it failed: `CI: the latest completed run on main failed (run <id>) - gh run view
  <id>` (`c3 finish --json` carries `"ci": {branch, run_id, line}`). A cancelled or skipped run and a
  pull request's run decide nothing; a green one prints nothing. It is a note, never a refusal;
  every git and `gh` process of the check runs under one deadline (10 s for `finish`, 3 s in the
  hook) and is killed past it; it is silent without `gh` on PATH, a login, a GitHub remote or a
  default branch. The hook asks at most once a day per repository (the attempt is recorded in the
  repository's own `<codex home>/c3/ci-check-<hash>.json`, written before the call, so a failing
  `gh` costs one try a day too) and never inside a C3 child. Default ON; `C3_CI_CHECK=off` or
  `"ci_check": false` in the C3 config turns it off. CI sat red for a week (2026-11-08..15) with
  nobody looking.
- **`c3 doctor` reports - and `--fix` completes - a delegation rule written before a section
  existed.** `c3 init` writes `.claude/rules/02-delegation.md` only when it is missing, so a project
  set up before the "BRANCHES AND WORKTREES" section never got it. Doctor now reports such a file
  (exit 1) when it still carries C3's own title line, and `c3 doctor --fix` appends exactly that
  section, taken from the embedded template, in the file's own line ending, through a temp file (a
  link and a hard-linked file are refused and reported; the file is read again immediately before
  the publication and left as it is when it changed; the temp file then takes its place by an atomic
  exchange that keeps an edit saved after that re-read next to the file - see F02-2 under Fixed).
  `c3 doctor` recognises a C3-written rule by its title line alone (a file
  that keeps that line counts as C3's, whatever its body); `--fix` only ever appends the missing
  section and changes no existing byte; a file without the title line is reported as a warning and
  never edited. The SessionStart hook does not do this, and nothing happens inside a C3 child.
- **`c3 doctor` now flags a pinned agent model.** Each `.claude/agents/*.md` whose frontmatter
  `model:` names a concrete id (e.g. `claude-opus-4-8`) instead of a floating alias is reported,
  one line per file, with the alias to write instead (derived from the id's family). The aliases
  `opus` / `sonnet` / `haiku` / `fable`, `inherit`, and a missing `model:` line are fine. A pin
  makes the workspace dirty (exit 1); `--json` carries the same data.
- **C3 now repairs a repository's model settings, not only reports them.** `c3 doctor --fix` and the
  SessionStart hook (default ON; off with `C3_REPO_MODELS=off` or `"repo_models": false` in the C3
  config) rewrite a stale `.claude/agents/*.md` Claude pin to its alias, and let the repository-root
  `AGENTS.md` follow the roster's successor (same family/major, higher minor, keeping the original
  casing — e.g. `GLM-5.3` -> `GLM-5.4`). Each edit changes one value and is left uncommitted for
  `c3 finish`; the repair starts no process at all (no git: it finds the repository root by walking
  up to `.git`), never writes outside the repository (a linked directory or file, or a file with a
  second hard link, is reported and left alone), and never fails a session.
- **Every child C3 starts that could run a Claude Code hook or a repair is marked with
  `C3_CHILD=1`**: reviewer CLIs, worker CLIs, the claude engine's allow-listed environment, launcher
  probes, the engine CLIs' installs and updates of `c3 maintain`, the detached `c3 maintain` pass and
  every child of `c3 update` (`cargo install`, `claude plugin update`, its git calls). Not marked:
  C3's own re-runs of itself as the bridge on the coordinator's behalf (a detached `c3 consult` /
  `c3 work`, a panel member, the telemetry sender, the MCP server's tool calls - their reviewer and
  worker children are marked), git elsewhere, `gh` and the OS's kill and link tools. The hook's
  repair above, `c3 doctor --fix` and `c3 branches --set` / `--clear` do nothing when they see the
  mark: a reviewer on the `claude` engine runs the same SessionStart hook in the repository under
  review and must stay read-only, and a worker's patch must hold only its own change. Any non-empty
  value marks a child except an explicit off (`0`, `false`, `off`, `no`).
- **`c3 branches`: every branch's state is visible and recorded in the repository.** It lists every
  local branch but the default one and every worktree other than the default branch's, with the
  recorded state, `merged` by ancestry or — when not — whether the content is already on the default
  branch (`git cherry`: the cherry-pick case), commits ahead, last commit date and worktree path, plus
  the start folder (branch, commits behind the default branch's upstream as the local refs know it,
  the number of uncommitted changes to tracked files - untracked files are not counted, a C3-managed
  repository always has some); `--json` gives the same data. The throwaway worktree of a running
  `kimi` review is C3's own and is left out. `--set <branch> --state
  work|waits|parked --note "<text>" [--owner <name>]` stores one line `c3: <state> | <owner> | <date>
  | <text>` as the branch description (shared by all worktrees, gone with `git branch -d`; a foreign
  description is kept below it); `--clear` removes only C3's line. A branch neither merged nor
  recorded is **unknown**.
- **Branch checks in `c3 doctor`, the SessionStart hook and `c3 finish`.** Doctor reports unknown
  branches and worktrees without a recorded state as leftovers (exit 1) and warns, without changing
  the exit code, when the start folder is off the default branch or dirty; `--json` carries the same.
  The hook prints one line only when something is unknown or the start folder is off the default
  branch (ancestry only, no `git cherry`; read-only; nothing inside a C3 child or outside a git
  repository; default ON, off with `C3_BRANCH_CHECK=off` or `"branch_check": false`). `c3 finish`
  names them after its commit (also in `--json`) and never refuses because of them.
- **Branch and worktree rules** in the `02-delegation.md` template, this repository's own rule file
  and the `coordinate` skill: merge instead of cherry-pick, a recorded state for every unmerged
  branch, no turn ends with an unrecorded branch or worktree, the opener closes, the start folder
  stays on the default branch and clean, guests leave no uncommitted edits, worktree removal never
  `--force`s over a link, a parked privacy/security WIP is listed in the tech-debt file.
- **`c3 maintain` reads public model catalogs.** Inside the weekly pass (or on request) the model
  step also fetches `https://openrouter.ai/api/v1/models` with a plain keyless GET (https,
  same-host redirects only, 15 s, 4 MB / 5000 ids at most; `:variant` ids skipped, every id through
  the strict model-id shape) and records it under its URL (`web:<url>`). A catalog id is a successor CANDIDATE
  for every active, unprotected entry - also an endpoint entry whose endpoint gives no
  `/v1/models` (Alibaba, Xiaomi MiMo, BytePlus) - and is seated only after the smoke passed on that
  entry's own plan; a failing one is reported "seen in the catalog, not served by this plan" and
  tried again on a later pass. Switches: `C3_MODEL_WEB=off` or `"model_web": false` (default ON;
  `C3_MAINTAIN_MODELS=off` fetches nothing either); `"model_catalogs"` replaces the default list;
  `"model_pages": [{"name","url","pattern"}]` adds vendor pages read with a one-group regex
  (`page:<name>`).
- **`c3 maintain` announces what is new for the roster.** The pass report names a seated successor
  (`model alibaba-claude :: qwen3.8-max -> qwen4-max, smoke passed`), a successor not seated and
  why (protected / on-request entry, smoke failed, mode `announce`), and "new in the family": a
  catalog id not seen before with a seated id's prefix, a version not lower and another suffix
  (seated `gemini-3.8-flash-high`, catalog `gemini-4-hydrogen`) - at most 5 per entry, and on a
  catalog's first collection only ids created in the last 30 days. The hook line stays short; the
  full list is stored and printed by `c3 maintain`.
- **`c3 maintain --models [--dry-run]`** runs only the model step now: the engine, endpoint and
  public lists, the announcements and the swaps per mode - no CLI checked or updated, `next_due`
  and the engine records untouched. `--dry-run` collects and says what it would announce and swap,
  with no smoke, no roster edit and no state written.
- **The codex smoke checks the sub-agent switch.** A Codex CLI update can silently break the
  bridge's off switches (codex-cli 0.169.0 did, and two runs spent 14 % of a ChatGPT week in five
  minutes), and `c3 maintain` would have installed it: its smoke only checked that a reply came
  back. The codex smoke now reads the rollout of every thread its two turns recorded and fails when a
  thread shows a `multi_agent_version` other than `disabled`, offers or calls a multi-agent tool
  (`spawn_agent`, `send_message`, `wait_agent`, `list_agents`, `close_agent`, `interrupt_agent`, or
  anything in the `collaboration` namespace), so a post-update smoke rolls the update back. A smoke
  that cannot make that check for both turns is "unproven", never a pass, and an automatic update
  holds on it exactly as on a failure (see Fixed: the smoke counts as a proof only when it is one).

### Changed

- **agy reviews a throwaway worktree, and its tool log is judged by the tool's name (F51-1, F52-1,
  F53-2).** The agy step classifier guessed from parameter names and values whether a step wrote,
  and every review round found another shape it got wrong in both directions (a launcher prefix, a
  pipe, a command under a path key, a file named like a command). The guess is replaced by the
  fact: the agy engine is worktree-isolated like kimi (`worktree_isolated` in its engine spec, for
  every purpose) - it runs in a throwaway worktree of HEAD (or of `--rev`) with the brief and the
  task's handoffs staged in, and that worktree's before / after comparison is the evidence: a
  change there fails the run (with or without a logged writer), a change of the live tree is at
  most a warning. The classifier is the tool's name: a tool on the reader allow-list is a reader
  whatever its parameters carry, every other tool a writer; it is recorded
  (`tree_check.writing_tools` in the ledger), and a writing tool in the log beside an unchanged
  worktree is a warning, not a failure. Removed: the parameter scanning (`command_shaped`, the
  key-role table `agy_param_role` / `AGY_DATA_PARAM_SUFFIXES`, the write markers and command words
  for readers) and the parameter merge of a step's events (the step merge stays - the tool name can
  arrive late). NOT YET RUN against the real agy CLI: its plan is out of quota until 2026-11-28, so
  the isolation must be watched on agy's first live run. Tests:
  `agy_steps_are_judged_by_the_tools_name_and_an_unchanged_worktree_keeps_the_reply`,
  `an_agy_write_in_its_worktree_fails_the_run`,
  `a_live_tree_change_during_an_agy_run_is_only_a_warning`,
  `agy_tool_step_classifier_reads_the_tools_name`,
  `agy_readers_are_readers_whatever_their_parameters_say`, `worktree_isolated_is_kimi_and_agy`.
- **A patch name is its content: no claims (F51-3, F52-2, F53-6).** The `--diff` patch file is
  named after its own bytes ((0.2.5 A4, F57-5) since then `<stem>.<sha256>.patch`, the full
  digest; this wave still used `<stem>.<sha8>.patch`), yet where hard links were unavailable the
  placement claimed the name with an empty file and renamed onto it, with a takeover of old claims
  and a cleanup by name - and each step had a race (a sibling's finished patch removed, a stalled
  owner replacing another run's file). The claim protocol is removed. The bytes go into a temporary
  file in the handoffs directory through its handle; if the name exists its bytes are compared -
  equal: the temporary file is removed and the patch reused; different: refused ("holds other
  bytes"), nothing overwritten; if it does not exist the patch is published by a hard link. Where
  there are none this wave published by ONE atomic rename onto the name and replaced an empty file
  there (an earlier build's claim) - a rename replaces, so a foreign file with other bytes that
  appeared at the name between the comparison and the rename was overwritten. (0.2.5 A4, F57-5)
  That rename and the empty-claim branch are gone: without hard links the patch is published under
  a name of the run's own (`<stem>.<sha256>.<32 hex>.patch`), and an empty file at the name is a
  file with other bytes (refused). Nothing ever removes the final name. (F53-4, F43-7) Write-temp
  files of C3's own pattern (`<stem>.<sha256>.<32 hex>.tmp` since A4) older than a day are removed
  when a patch is placed in that directory. Removed: `reuse_patch`, `place_without_link`,
  `drop_own_claim`, the stale-claim age `PATCH_CLAIM_STALE_SECS` and the hooks
  `CODEX_CONSULT_TEST_PATCH_RENAME_FAIL` and `CODEX_CONSULT_TEST_PATCH_CLAIM_STALE_SEC` (with their
  tests); `CODEX_CONSULT_TEST_PATCH_NO_HARDLINK` stays. Tests:
  `stale_patch_temps_are_swept_when_a_patch_is_placed` and (renamed and rewritten by A4)
  `an_empty_file_at_the_patch_name_is_refused_and_a_short_name_is_left_alone`,
  `a_patch_without_hard_links_is_published_under_the_runs_own_name`.
- **A one-revision `--diff` carries the tracked changes only unless asked (F18-4, F19-2, F20-6,
  F51-5, F53-5, F53-7).** `--diff <one revision>` puts the tracked changes into the patch; the
  untracked files are left out and the run says so before the reviewer starts - how many, up to
  ten names (a name with a control or a format character, Unicode category Cf included, is counted,
  not shown) - on stderr, in the handoff's warnings and in the ledger (the run's `warnings` and the
  `diff` record's `untracked_left_out` / `untracked_left_out_count`); the prompt line says how many
  were left out. `--diff-untracked` includes them (through the redactor, named the same way as
  carried). `--diff-tracked-only` stays accepted and names the default; the two together are
  refused. The MCP tools `c3_consult` and `c3_panel` take `diff_untracked` (and `c3_panel` takes
  `diff`). README and the consult skill's reference say it. Tests:
  `a_one_revision_patch_leaves_untracked_files_out_unless_asked`,
  `a_one_revision_diff_leaves_untracked_files_out_and_says_so`,
  `consult_and_panel_pass_diff_untracked`.
- **`c3 hook` caps its own lines; the wrapper scripts forward them as they are.** The two
  SessionStart scripts each cut a forwarded line at 400 - the `.ps1` by UTF-16 units, the `.sh` by
  bytes under `mawk` - so they printed different text for a long non-ASCII line. The cap is now
  `c3 hook`'s own (Rust): at most 400 characters per printed line, cut at a character boundary with
  `...` appended (397 kept), carriage returns removed and trailing white space trimmed. Both
  scripts lost their cutting of forwarded lines (they keep only their prefix filter and the order,
  de-duplication and count rules; their own fallback line is cut at 400 again, see the fix below)
  and print the same text again, non-ASCII lines included. Tests: the unit test
  `hook_lines_are_capped_at_400_characters_on_a_character_boundary` and `hook_scripts.rs` extended
  by a long non-ASCII line and a line longer than the cap (both forwarded unchanged by both
  scripts).
- **`c3 router explain` starts nothing and judges the seats in the panel's order (F31-6, F30-1).**
  Since 0.2.5 R4 the display command ran the panel's member election with the full credential
  preflight, so it could spawn `codex login status` or another auth probe for every roster entry,
  with no deadline, read the budget windows over the network, and it always judged the seats as a
  routed panel. It now starts no process and makes no network call: availability comes from the
  recorded health only (an entry the records show unavailable - an auth failure, a usage limit, a
  model the login does not offer, a plan cap - is "not eligible" with the panel's own reason; an
  entry the records say nothing about stays eligible and is marked `[availability not checked]`),
  the budget columns use the cached windows, and the seats are judged in the order the real panel
  would use - routed by default with the panel's "no ratings" fallback to the roster order, or the
  new `--panel-order roster|routed` - and a `seats:` line says which. Tests:
  `explain_takes_availability_from_the_recorded_health_only`,
  `explain_says_in_which_order_it_judged_the_seats`.
- **The normaliser's timing test measures linearity, not wall time.**
  `normalise_is_linear_on_a_large_reply` asserted a 4 MB reply normalises in under 5 s and failed at
  9.7 s while other builds ran. It now compares the fastest of three runs at N and at 8N strings
  and asserts the ratio stays far below the quadratic 64x (under 24x), with a three-minute ceiling
  only as a backstop.
- **C3's project folder is `.c3/`, and `c3 finish` writes no snapshot by default.** C3 inherited its
  per-project folder `.eck/` from another tool; it now keeps everything in a lowercase `.c3/` at the
  repository root, git-ignored as a whole: the manifests (same names) and `profiles.json`,
  `last-report.md` (the report of the last `c3 finish` / `c3 fail`, formerly
  `lastsnapshot/AnswerToSA.md`), `scouts/`, the hand-off `snapshots/` / `lastsnapshot/`, and the
  cache that used to live under `<collab>/.c3/` (`anchor`, `snapshot_seq`, `index/`, `explains/`) -
  now one per repository. Every command that writes into the folder (`c3 init`, `c3 finish`,
  `c3 fail`, `c3 manifest` and their MCP tools, `c3 scout`, `c3 fetch`, `c3 snapshot`, `c3 index
  build`, `c3 explain` without `--out`) creates it when missing, with a `[STUB: ...]` stub for each
  missing manifest and a `.c3/` line in `.gitignore` when no line ignores it; the SessionStart hook,
  a read-only `c3 doctor` and a reviewer or worker C3 started (`C3_CHILD`) never do. **Upgrading:**
  the first writing command (or `c3 doctor --fix`) renames an existing `.eck/` to `.c3/` with one
  rename - if that fails nothing else is tried, the command says why and goes on with `.eck/` for
  that run - moves the old tool's
  `anchor`, `update_seq` and `telemetry_queue.json` into `.c3/legacy/`, the old report to
  `last-report.md`, and a `<collab>/.c3/` cache into `.c3/` entry by entry (an existing entry wins).
  A `.eck` that is a symlink or junction is never moved; when both folders exist nothing is merged
  and `c3 doctor` says so; until the move, `c3 doctor` and the manifest digest of `c3 explain` read
  `.eck/`. `C3_STATE_MIGRATE=off` (or `"state_migrate": false` in the C3 config) keeps `.eck/`
  where it is for a repository where the old tool is still used - C3 then does not move it, creates
  and uses `.c3/` for its own files, and still uses a collab directory the caller places under
  `.eck/` as given.
  `c3 finish` / `c3 fail` (and `c3_finish_task` / `c3_fail_task`) now write the report, the journal
  entry and (finish) the commit, and no snapshot: `--snapshot` (MCP `"snapshot": true`) asks for the
  hand-off snapshot, `--keep` applies to it, `--no-snapshot` is still accepted and does nothing;
  `c3 snapshot --state` (old name `--eck`, kept as a hidden alias) writes one on its own. The
  "never put in AI context" list is now `.c3ignore`; a directory without one still has its
  `.eckignore` read. Reviewer packs, snapshots, `c3 fetch` and `--diff` patches leave out both
  `.c3/` and `.eck/` (case-insensitively on Windows and macOS). `c3 doctor` reports a C3-written
  rules file that still carries an old template sentence naming `.eck/`, and `c3 doctor --fix`
  replaces exactly that line (the same link-safe replace as the delegation-rule repair); text the
  user wrote is reported and left alone. The stub detector accepts `[STUB]` as well as `[STUB:` at
  the start of a line. The root `AGENTS.md` is a short C3-only worker instruction file now. A collab
  directory kept inside the folder (`--collab-dir .eck/collab`, as some skills pass it) follows the
  move: once `.c3/` exists, a `.eck/...` path that no longer exists is read as the same path under
  `.c3/` (said in one line, and no new `.eck/` is ever created for it), and a `.c3/...` path on a
  repository that still has `.eck/` makes a real consultation or work start move the folder first
  (any other command reads the legacy path and moves nothing); the move is never attempted while a
  detached consultation or work run is live under `.eck/`, and when it cannot be made now (a live
  run, a failed rename) the command goes on with the legacy folder for that run and says so - one
  rename or nothing.
- **A swap across a major is held when the catalog lists it at more than twice the price.** A
  successor of a higher major (`qwen3.8-max` -> `qwen4-max`) used to be seated after its smoke with
  nothing weighing that it may cost more on the same plan. OpenRouter's catalog carries a list price
  per model (`pricing.prompt`, `pricing.completion`); when it prices both the seated id and the
  candidate and the candidate's is more than twice the seated one's in either field, the swap is
  announced only: "not seated (list price N times higher - seat it by hand)". Within one major, or
  without prices for both ids, nothing changes. A list price is a signal of what the model costs
  elsewhere, not the plan's own price - a plan may charge the same for both - which is why the swap
  is left to the operator rather than refused. Only the two numbers per id are read, for the pass
  that fetched them; nothing of the pricing is written to the state file. Tests: the catalog parser
  (positive prices only, never serialised), the unit test of the price rule (prompt or completion
  over twice, exactly twice, a missing price, within a major, an own-list id) and an end-to-end
  `--models` pass.
- **CI names the crate a hung or cancelled test run was in.** The test step of both runners and of
  the index-surreal job is split into one step per crate (`c3-core`, `c3`, `c3-cli`), each with
  `--no-fail-fast` and `if: success() || failure()`, so all three always run and the run's API shows
  the step that was running - twice on 2026-11-22/23 the Windows job was cancelled or hung without
  a usable log. The same tests run as before; the job timeouts stay.
- **A claude-engine reviewer on a diff-review or acceptance reads a frozen tree of HEAD.** Without
  `--rev`, the claude engine's stand-ins (GLM, Kimi, Qwen, DeepSeek through Anthropic-compatible
  endpoints) now run in a throwaway worktree of HEAD on those two purposes, like the kimi engine,
  with the brief and the task's handoffs staged in - so a parallel session editing the tree can no
  longer change what the reviewer reads mid-review. The prompt names the paths that were
  uncommitted at launch (and so are not visible there), as wave A1 does for kimi; pass `--diff HEAD`
  to hand such a reviewer the uncommitted change as a patch. Every turn of a run (primary,
  read-insist, continuation, repair) now shares the run's one worktree, kimi's included (it used to
  make a fresh one per turn). Other purposes keep reading the live tree.
- **The hook's waiting rule says why and says it applies to every session.** The old one-liner
  ("c3: coordinator rules - the c3:coordinate skill; ... schedule a wakeup ... about 20 min") read
  as a pointer to a skill and as a rule about c3 work, and a coordinator waiting on ordinary
  subagents filed it away; the `ScheduleWakeup` tool's own description ("/loop only", "never keep
  the cache warm") won, and every completion notice then re-read a several-hundred-thousand-token
  context at full price. `plugin/hooks/c3-hook.ps1` and `c3-hook.sh` now print `c3: WAITING RULE (the
  operator's standing instruction, for every session, with or without c3)`: before ending a turn
  while anything is running in the background, call `ScheduleWakeup` with `delaySeconds` 1800 and a
  one-line prompt; why (a wake-up inside the cache lifetime keeps the context cached); what to do at
  the wake-up; and that it overrides the tool's own advice. The delay is one number (1800 s), no
  range. The `coordinate` skill opens with the same rule (stating it is not limited to c3 work), and
  `c3 consult --detach` / `c3 work --detach` print one sentence of it on stderr when they start.
  The hook scripts also stopped dropping most of what `c3 hook` says: the maintenance, branch,
  model-repair and red-CI lines never reached a plugin session, because the scripts kept only a
  whitelist of four lines. Both now forward every line of `c3 hook` that starts with `c3: `,
  `codex-consult: ` or `CI: ` - in the order printed, each once, at most 10, each cut to 400
  characters (`...` appended) - and then the waiting rule. The two print identical text for ASCII
  lines; a longer non-ASCII line may be cut at a different point (the `.sh` counts bytes under
  `mawk`, the `.ps1` UTF-16 code units) (tested with a fake `c3`, the `.ps1` under Windows
  PowerShell 5.1 and the `.sh` under `sh`).
- **CI no longer stops at the first failed test binary.** The `ci` workflow's test steps (Windows and
  Linux, default features and `index-surreal`) run with `--no-fail-fast`, so one failed binary does
  not hide the rest; `kill_record_durable`'s `git()` helper now panics with git's exit code and
  stderr (a Windows runner flake at `git init` once left no message).
- **`C3_MAINTAIN_MODELS=off` holds for `c3 maintain --models` too (F02-18).** An explicit `--models`
  used to turn `off` into `announce` and fetch the engine, endpoint and public lists anyway. With
  `off` it now fetches nothing, writes nothing and says so; set `announce` or `swap` to run it.
- **An unresolved pending update no longer freezes its engine.** When the installed version of an
  interrupted update still fails its smoke and the rollback fails or does not exist, a pass skipped
  the engine's update check for as long as the record stayed. It now asks the vendor: a release
  NEWER than the failing version is tried with the same cool-down (unless `--force`), in-use check,
  write-before-install record and post-update smoke, keeping the record's original `previous` as the
  rollback target; resolved when it smokes clean, otherwise the record stays with the new target.
  The no-rollback-path branch (agy) has an integration test now.
- **A panel seats one entry per lab.** A 2026-11-20 framing panel seated `glm-5.3` and
  `glm-5.3-flash` side by side - a model and its light sibling repeat each other, so their agreement
  looked like corroboration and a seat was spent. The routed draw and the roster order (also the
  "no ratings" fallback, where that panel came from) now seat at most one roster entry per lab: the
  model's maker (the entry's `lab`, case-folded; a claude endpoint's host; else the model id's
  vendor prefix, matched only at a boundary; an unknown one counts as a lab of its own), never the
  provider label - `kimi` and `kimi-claude` are one lab when their models, endpoint hosts or declared
  labs say Moonshot, Qwen and DeepSeek behind `alibaba-claude` are two. Each lab is represented by its
  best-weighted eligible entry (the first in roster order on a tie), then the draw runs among labs;
  a size above the lab count is cut to one seat per lab with a warning, and the reserve a lost seat
  is refilled from holds only labs without a seat. Every `--require`d entry is still seated (two of
  one lab with a warning); `--panel-all` seats every eligible entry and warns once which labs hold
  several seats; the routed bare default (one reviewer) and the plugin-compat run
  (`C3_PLUGIN_COMPAT=1`) keep the plugin's draw. When the rules could change the seats the ledger's
  `panel.routing` carries `ext.c3.seating`, and `c3 router replay` replays such a record with them.
- **On a weighty purpose a panel holds the upper tier.** On framing, decision, core-contract,
  acceptance and stuck, a panel of two or more seats now gives one seat to a `"panel": "weighty"`
  entry when one is eligible (available, not held by its reserve, not on-request) and no required
  entry already is one - drawn first among the weighty entries whose lab holds no seat yet (rule
  `tier-draw` / `tier-explore`, `tier` in roster order); when every such entry shares a lab with a
  required seat, the plan warns instead (the one-lab rule is kept). The same 2026-11-20 run had
  seated three low-cost entries for a framing question with weighty entries available.
- **`c3 router explain` and the panel plan say when a lab already holds the seat.** A member kept
  out by the one-seat-per-lab rule shows `not picked: lab zhipu already seated by #3` in the plan
  (dry run and real run) and in `panel.members`, and the summary lists it on its own line
  `not picked (one seat per lab): ...` instead of under the panel size; `c3 router explain` marks
  an entry whose lab's seat another entry holds (`[lab zhipu: #3 takes its seat - one seat per
  lab]`), judged with the panel's own member election and draw.
- **OpenAI's GPT-6.2 models are known to telemetry.** The `openai` class's closed model list now
  also holds `gpt-6.1-sol`, `gpt-6.2-astra` and `gpt-6.2-sol` (released 2026-11, listed for
  ChatGPT accounts by Codex). Before this a judge record naming them was sent as `other`. The
  docs now name `openai :: gpt-6.2-sol` as the codex worker.
- **A new Claude model id is accepted without a C3 release.** The closed `CLAUDE_MODELS` table is
  kept (it documents the known ids and the plugin parity), but a roster / `--model` of a `claude`
  entry (auth subscription or api-key) now also accepts any id of the published shape
  `claude-(fable|opus|sonnet|haiku)-<digits>[-<digits>]` (case-sensitive, an optional `[1m]`). The
  shape only admits the id: telemetry sends a Claude id by name only when it is in `CLAUDE_MODELS`
  or published for the `anthropic` class (below), else as `other` (F02-20, F04-8).
- **`c3 maintain`: a successor may cross a major version.** `version::successor` keeps the same
  prefix and suffix but now accepts a higher major too and picks the highest version above the
  seated one; an id without a minor counts as `.0` (`qwen4-max` is 4.0, `gpt-6-astra` 6.0,
  `glm-6` 6.0), so `qwen3.8-max -> qwen4-max` and `glm-5.3 -> glm-6` are swaps. Everything else of
  the swap is unchanged (smoke on the entry's own plan first, roster backup, only that `model`
  edited, protected / on-request entries never smoked or edited). The repository `AGENTS.md`
  follows the roster across a major the same way (`Qwen3.8-Max` -> `Qwen4-Max`).
- **Telemetry: a published model id is sent as itself.** A model outside its vendor's closed list
  that the maintain state holds from a TRUSTED public source, attributed to that same vendor, is
  sent by name instead of `other`: the built-in catalog (OpenRouter, whose `<vendor>/<model>` ids
  bind to the vendor slug's class), or a configured catalog or page whose config entry says
  `"public": true` (a page's plain ids bind to the class of its own host). Never an endpoint's own
  list, an engine's list or an undeclared configured source, which may be private. Without a
  maintain state the closed lists alone decide, as before.
- **A newer member of a declared model family passes the effort check.** A codex custom provider
  or the muse engine refused any id outside the host's caps-v1 list before launch ("no effort
  vocabulary declared for model ..."). An id with the same prefix and suffix as a listed one and a
  version not lower (`qwen4-max` beside `qwen3.8-max`, `glm-6` beside `glm-5.3`,
  `muse-spark-2.0` beside `muse-spark-1.3`) is now accepted with the host's vocabulary, so a
  successor `c3 maintain` found can be smoked; any other unlisted id is still refused.
- **A codex reviewer no longer inherits the operator's plugins and skills.** `codex exec` loaded the
  plugins enabled in `~/.codex/config.toml` and listed their skills - the coordinator skills
  `c3:coordinate` and `codex-consult:coordinate` among them - to the reviewer, which once read one
  and acted as a coordinator. Every codex review turn now also passes `-c features.plugins=false -c
  skills.include_instructions=false` (whatever the sub-agent opt-in; a key the operator's
  `--codex-config` names is left to them); agent roles were already off with
  `agents.enabled=false`. Checked on codex-cli 0.169.0 (ZAI): 30 skills listed before, 14 with
  plugins off, none with both. `c3 work` worker turns are unchanged. The global
  `~/.codex/AGENTS.md` still reaches a codex reviewer (switching project docs off would also drop
  the reviewed repository's own AGENTS.md), so every review prompt, on every engine, now ends its
  Constraints paragraph with: the reviewer is not a coordinator and ignores instructions it finds
  about delegating, spawning workers or sub-agents, or finishing or committing tasks.
- **`c3 branches` names `git cherry`'s answer for what it is (F02-10).** The JSON field
  `content_on_default` is now `patches_on_default` (with `patches_checked`), and the listing says
  "patches already on main (git cherry: patch equivalence, not the current content)": `git cherry`
  compares commits' patch ids, so a cherry-picked commit that was later reverted still counts. Both
  refs are passed fully qualified (`refs/heads/<default>`, `refs/heads/<branch>`), so a tag of the
  same name cannot be taken instead. `merged` is null in the JSON when the ancestry is not known.

### Fixed

- **A tool inventory the codex smoke cannot read in full is no longer switch evidence (F57-2,
  found by the acceptance review; makes F11-8's tool-list rule strict).** The rollout reader
  marked a turn as having evidence as soon as it saw an `additional_tools` record with a `tools`
  array, and the walk through that array skipped every entry it did not understand - a tool
  without a name, a namespace whose inner `tools` was not an array - without saying so, so a changed
  rollout schema read as a clean inventory and could pass the smoke: the false pass `unproven`
  exists to prevent. The inventory is now read by `c3_core::quota::tool_inventory`, which answers
  one of three. COMPLETE: every entry is of a supported shape - a namespace with a name and a
  `tools` array (read the same way, nested), a `function` or `custom` tool with a name (or the chat
  form's `function.name`), or a hosted tool identified by its kind alone (the closed list
  `local_shell`, `web_search`, `web_search_preview`, `image_generation`, `file_search`,
  `code_interpreter`, `computer_use_preview`). BREACHED: a multi-agent tool is offered - a breach
  (a failed smoke) whatever else in the list is unreadable. UNPROVEN: anything else - a missing or
  non-array list, an entry that is not an object, an entry without a string `type` or of an unknown
  kind, a tool without a name, a `name` that is not a string, a namespace without a name or whose
  inner list is missing or not an array. Only a COMPLETE list is evidence for its turn. An UNPROVEN
  list is counted against its turn - a `multi_agent_version` beside it does not make that turn
  proven - and anywhere in the rollout (outside a turn too, as the breach check reads the whole
  rollout) it makes the smoke unproven, which holds an automatic update or rolls it back like a
  failure; the reason names the line, the turn and the entry's position and kind or key, never its
  content (``the tool list at line 7 (turn 2): entry 2 (a namespace) holds `tools` as an object,
  not an array``). The smoke's per-turn check now asks `TurnEvidence::proven()` (evidence of its own
  and no unreadable list); `SubagentSurface::tool_lists` counts complete lists only. Limit: the
  supported shapes are codex-cli 0.169.0's as C3's fixtures record them, not re-checked against a
  fresh live rollout here; a later CLI that adds a tool kind holds its update as unproven until the
  list is extended. Tests: `a_tool_inventory_is_complete_breached_or_unproven`,
  `an_unreadable_tool_list_leaves_its_turn_unproven` (c3-core), and the reviewer's RC2 first half,
  `an_inventory_that_cannot_be_read_in_full_is_no_switch_evidence`: a two-turn smoke rollout whose
  second turn's only evidence is a namespace with an object `tools` is unproven and the update is
  held; a nameless tool is unproven; a fully readable list without multi-agent tools still proves
  its turn; a multi-agent tool beside an unreadable entry is a breach.
- **A pending-update recovery never credits a smoke to a version that did not run (F57-3, found by
  the acceptance review).** An update interrupted between install and smoke leaves a pending record
  that the next pass recovers. The record was matched by engine; the recovery treated "installed ==
  previous" specially and otherwise smoked the launcher and recorded a pass as `update to <target>
  verified` - also when a THIRD version was installed (by hand or by another updater meanwhile) or
  when the installed version could not be read. The installed version, measured first, now decides.
  The previous version: cleared as before. The target: smoked, and only a pass with `--version`
  read again after the smoke still showing the target records `recovered: update to <target>
  verified after an interrupted pass`. A third version: smoked for itself; a pass is recorded as
  `pending update to <target> not verified: <installed> is installed instead (changed since the
  update was recorded); the smoke passed on <installed>` - the state's `installed` is that version,
  and the record is cleared with that truth in the state and the report; a failure names it too
  (`recovered (<installed> is installed, not the pending target <target>): smoke failed, rolled
  back to <previous>`, or unresolved as before when the rollback does not restore). A version that
  cannot be read: no smoke is run (its result could not be attributed to a version), the action says
  `pending update to <target> unresolved: the installed version cannot be read, so the update is not
  verified (no smoke run)`, and the record is kept. A passing smoke during which the installed
  version changed is credited to no version and the record is kept. Limit: the version is read
  before and after the smoke, not during it, so a change and a change back inside one smoke is not
  seen. Tests: the reviewer's RC2 second half,
  `a_pending_recovery_never_credits_a_third_installed_version_to_the_target` (previous 1.0.77,
  target 1.0.80, installed 1.0.79: the action and the state name 1.0.79, the target is not verified;
  a failing smoke of 1.0.79 is named and rolled back), and
  `a_pending_recovery_with_an_unreadable_installed_version_verifies_nothing`; the existing
  `a_pending_update_whose_install_never_took_is_cleared` and
  `a_pending_update_whose_new_binary_smokes_clean_is_recorded_verified` keep their meaning.
- **Plain http in test mode goes only to a loopback IP literal (F57-9, found by the acceptance
  review).** Wave D6 accepted `http://` under `CODEX_CONSULT_TEST_MODE=1` for `127.0.0.0/8`, `::1`
  and the NAME `localhost`; a name is resolved by the system resolver at connect time, which need not
  answer with a loopback address, so the plaintext request - with a configured bearer credential -
  could reach another machine. The exception is now loopback IP literals only (an IPv4 address in
  `127.0.0.0/8` or the IPv6 `[::1]`, as the URL parser reads the host): `http://localhost/...` is
  refused with `base_url must be https://` like any other host name, in test mode or not. No
  connection to a name is made over plain http any more, so no resolver answer is trusted.
  `https://localhost` is unaffected (TLS checks the peer). The end-to-end test already uses
  `127.0.0.1`. The telemetry intake's own test-mode rule (`telemetry::hub`) follows: a loopback
  literal only, the name `localhost` refused. Test: `s2_plain_http_only_to_a_loopback_host_in_test_mode`
  (`localhost`, `LocalHost` and `localhost.` refused in both states; `https://localhost` accepted).
- **Two different patches never share a path, and no patch publication replaces a file (F57-5,
  found by the acceptance review).** The `--diff` patch was `<stem>.<sha8>.patch`: only 32 bits of
  its sha256 were in the name, so two different patches with the same stem could meet at one name;
  and where hard links were unavailable the publication was a rename, which replaces - a different
  file that appeared at the name after the placement looked (a colliding sibling's patch, or a
  foreign file) was overwritten, and one consultation's ledger then named the other's bytes. The
  name now carries the full digest, `<stem>.<sha256>.patch`; the write-temp name
  (`<stem>.<sha256>.<32 hex>.tmp`), the stale-temp sweep, the collab exemption, the prompt's patch
  line, the ledger's `diff.patch`, the dry run, the CLI help, README and the consult reference
  follow. With hard links (the normal case) the publication stays the hard link, which fails on an
  existing name: the name is judged again - the same bytes are reused, other bytes refused.
  Without hard links the replacing rename is gone: the patch is published under a name of the
  run's own, `<stem>.<sha256>.<32 hex>.patch` (an id drawn for the placement; when the directory
  handle shows anything at that name another id is drawn), by a rename onto that fresh name, and
  THAT path is the one the plan, the prompt, the ledger and the collab exemption carry. The
  empty-claim branch is removed, not kept: an empty file at a full-digest name is a file with
  other bytes and is refused (no build ever left one there). A patch an earlier build wrote under
  the short name is another file - never reused, never removed. The invariant is stated on
  `place_patch` and proved by the tests: after a successful placement the file at the recorded path
  holds exactly the bytes whose sha256 the ledger records, and no file that existed before was
  overwritten or removed (C3's own write-temp debris older than a day excepted, as before). What
  remains: without hard links every run writes its own copy (panel members with equal bytes no
  longer share one file; in that mode a sibling's own name for the same stem and digest is exempt
  from the collab comparison, so a reviewer file of exactly that shape is not seen there - as with
  the write-temp name); the rename onto the run's own name assumes nobody creates a file at a name
  of 128 random bits, shown nowhere, in the instant between the look and the rename; no real
  file system without hard links ran (the hook stands in for one). Tests:
  `two_different_patches_forced_to_one_name_never_share_a_path` (RC4: two payloads forced to one
  name, the second placed at the first one's race point `patch-place`, without and with hard links;
  equal bytes with hard links share one file),
  `a_foreign_file_appearing_at_the_patch_name_survives_byte_for_byte` (both modes),
  `a_patch_without_hard_links_is_published_under_the_runs_own_name`,
  `an_empty_file_at_the_patch_name_is_refused_and_a_short_name_is_left_alone` (replaces
  `an_empty_leftover_claim_is_replaced_and_other_bytes_are_refused`),
  `the_runs_own_patch_is_the_only_exempt_file`, `stale_patch_temps_are_swept_when_a_patch_is_placed`,
  `a_patch_holds_the_change_and_new_files_and_leaves_collab_out`, and the c3-cli test
  `diff_hands_the_reviewer_a_patch_and_a_named_range_without_it_is_warned`.
- **A resumed worktree is the directory that was claimed (F57-7, found by the acceptance
  review).** D7 reused the parent's recorded worktree path verbatim: the claim proved only that the
  name was free at `mkdir` time; `mkdir`, `git worktree add` and the failure cleanup each resolved
  the path again; the parent was compared as text after canonicalisation (on Unix a literal
  backslash counted as a separator); and the first handle C3 kept was opened after the checkout.
  The path is now BUILT: the current temp directory (`std::env::temp_dir()`, opened once as a
  directory handle) joined with the validated name (`c3-kimi-wt-` and 32 lower-case hex). The
  recorded parent must be that directory by native file identity (volume and file index on
  Windows, device and inode elsewhere, of the two opened directories) - never by string, with no
  separator rewriting; anything else is not reusable (the fresh-path fallback and its warning, as
  before). Every throwaway worktree - a resumed one and a fresh one - is claimed through the temp
  directory's handle: made exclusively, opened through it without following it, and that handle
  kept (a link or a non-empty directory found there refuses). After `git worktree add` the path is
  opened again by name without following it and must be the same directory as the kept handle
  BEFORE anything is staged or a reviewer starts; the root the run uses is that verified directory.
  A mismatch refuses the seat in one preflight-class line, removes NOTHING at the path and prunes
  the registration git made. A failed `git worktree add` removes only the claimed directory -
  through the temp directory's handle, while the name still opens to that same directory, and only
  when empty. What remains: between the exclusive create and the open there is no handle yet, so an
  EMPTY directory swapped in at that instant cannot be told from the one made (it holds nothing to
  lose; one with content, or a link, is refused); from the claim on, Windows refuses any rename of
  the held directory, while on Linux and macOS a swap is caught by the identity check. Tests:
  `a_claimed_worktree_directory_replaced_before_the_check_refuses_and_is_kept` (a replacement
  directory with content swapped in at the race points `worktree-claim`, `worktree-add` and
  `worktree-checkout`: refused, the replacement and its content preserved, nothing staged, no
  registration left; where the system refuses the swap the worktree is used and removed as usual),
  `a_reusable_path_is_built_from_the_temp_directory_by_identity` (a directory that only looks like
  temp is not reused, an alias of temp yields the path built from temp, on Unix a directory named
  `a\b` is not `a/b`), `a_reusable_worktree_path_has_exactly_c3s_shape`, the D7 unit tests and the
  three D7 integration tests in `kimi_engine.rs`.
- **The isolation prompt never shows a tracked name with a format character (F57-8).** The HEAD
  isolation paragraph listed a modified tracked file's name when it held no backtick and no control
  character, so a bidi override (U+202E) or another invisible format character could reorder or
  hide what the name shows. Its filter now uses the shared `crate::unicode::is_format_char` (D5):
  such a name is counted, never shown - in the sentence on links removed after the checkout too,
  which uses the same filter. The `--rev` paragraph names no live paths and is unchanged. Test:
  `the_isolation_prompt_names_tracked_changes_and_counts_the_rest` (a tracked name holding U+202E
  is counted and none of its bytes is in the line).
- **A `--diff` patch is never empty for a changed file edited in the same instant as the index
  (found in CI run 45402117704).** A one-revision patch is rendered with a temporary COPY of the
  real index; `std::fs::copy` gives that copy a new modification time on Linux (it keeps it on
  Windows). Git tells a "racily clean" entry - a file changed to the same size in the same instant
  (on a git that compares whole seconds: the same second) as the index was written - by the index
  file's own time: an entry recorded at or after it is checked by content, every other one is
  trusted by its stat data. In the copy such an entry looked clean, and the patch came out without
  that change - in the CI run an empty patch (`e3b0c442...`, the sha256 of nothing) for a changed
  file. The copy now keeps the real index's modification time (read from the same open handle as
  its bytes), so git judges it exactly as the real index and smudges those entries when it writes
  the copy. Every temporary index of the patch was checked: the render's and the untracked listing's
  copies and the D5 per-file probe all come from that one copy function (changed); the range form
  builds its index with `git read-tree` and diffs `--cached`, which never reads the working tree's
  stat data (safe); no other code makes an index. Test: `a_racily_clean_change_is_in_a_one_revision_patch`
  (the racy condition made by hand - the entry's and the index file's time set to one past instant,
  ctime not trusted - the change is in the patch; on Linux a plain copy shows the empty diff).
- **A kimi thread can be resumed: the resumed run works at its parent's worktree path (found in a
  live run).** A kimi review runs in a throwaway git worktree whose path was random per run
  (`<temp>/c3-kimi-wt-<32 hex>`), and the kimi CLI binds a session to the directory it was created
  in, so every `--mode resume` of a kimi thread failed at once with `Session "<id>" was created under
  a different directory.` - already so in 0.2.4. The ledger's `worktree` record now names the
  worktree's directory (`path`), and a run that RESUMES a thread of a worktree-isolated engine (kimi,
  agy; a panel member too) makes its throwaway worktree at the path of the run it resumes. That path
  is taken only when it has C3's own shape - directly under the current system temp directory,
  named `c3-kimi-wt-` and 32 lower-case hex digits ((0.2.5 A4, F57-7) the parent decided by file
  identity, not canonical text, and the path BUILT from the temp directory and the recorded name,
  never the recorded spelling - see the F57-7 entry); another path gives a
  fresh one and a run warning that the parent's path could not be reused. The path must be free: its
  directory is made exclusively, and when anything is there (a live run of the same thread, a
  crashed run's leftover) the seat is refused before any reviewer starts, in one line that names the
  path and says what to do (`c3 doctor --fix` removes a leftover; a live run must finish first) -
  nothing there is removed or reused. A parent entry without a recorded path (an older build) gets a
  fresh path as before; when kimi then refuses the session, the failure text adds that the thread
  was started where its directory is not known and a new thread is needed (`--mode new`). `--mode
  new` is unchanged, kimi and agy have no fork, and the claude engine's frozen tree is not touched
  (Claude Code resumes across directories). The fake kimi CLI of the tests binds its sessions to
  their directory like the real one. Tests: `a_resumed_thread_runs_at_the_worktree_path_of_the_run_it_resumes`,
  `a_resume_while_a_directory_sits_at_its_worktree_path_is_refused`,
  `a_resume_of_a_parent_without_a_worktree_path_gets_a_fresh_path_and_the_hint`,
  `a_reusable_worktree_path_has_exactly_c3s_shape`,
  `create_at_uses_a_free_path_and_refuses_an_existing_one`,
  `a_failed_checkout_at_a_claimed_path_leaves_nothing_there`,
  `a_resume_takes_the_parent_worktree_path_only_when_it_has_c3s_shape`,
  `a_taken_resume_path_refuses_the_seat_and_a_free_one_is_used`,
  `a_different_directory_refusal_carries_the_resume_note`.
- **Two takers of a stale move lock can no longer both hold it (found by the acceptance review).**
  The lock that keeps the move of `.eck/` to `.c3/` single is taken over when its owner is gone:
  the taker read the record, judged it stale, read it again and removed the file - two separate
  steps. Of two processes that judged the same stale record, the first could remove it and create
  its own lock between the second's compare and its removal; the second then removed THAT lock
  and created another, and both ran the move. The compare and the removal are now one step among
  C3 processes: they happen under an OS lock on the gate file `<lock>.gate` next to the lock (in
  git's directory), and the owner's release goes through the same gate - and (F57-4, entry below)
  so does the lock's creation, from before the exclusive create through the complete record. The
  OS drops that lock when its holder dies, so the gate has no stale state; the gate file is never
  removed. A taker that cannot get the gate within a second removes nothing. Test:
  `the_move_lock_is_exclusive_and_a_dead_owners_lock_is_taken_over` (nothing is taken while the
  gate is held; a taker that judged the dead record does not remove the lock that replaced it).
- **The move lock is owned from its creation: a creator paused before its record is never a second
  owner (F57-4).** The gate above covered the takeover and the release, not the creation: a process
  created the lock file exclusively and THEN wrote its record. Suspended in between for longer than
  the garbage interval (10 s), its still-empty file was taken over by another process as garbage;
  when it resumed, its write into the unlinked file succeeded (Unix) and it returned as a second
  owner of the lock - two movers again. Now the creator takes the gate BEFORE the exclusive create
  and holds it through the complete, flushed record (or, when the write fails, the removal of the
  file), and before it returns it reads the file back, still under the gate: only exactly its own
  record makes the lock its own; another record means "held" (the caller waits or reports the
  holder), a vanished file means "try again". A creator that cannot get the gate within a second
  creates nothing and waits like a taker. So an empty lock file exists only while its creator holds
  the gate, and the garbage rule applies only to a creator that died. What remains: the gate is an
  OS lock - a process of ANOTHER host on a file system without working OS locks is held only by the
  age rules (the read-back still keeps such a creator from returning as an owner when its file was
  replaced), and a c3 older than 0.2.5 takes no gate at all. Tests:
  `a_lock_creator_paused_after_its_exclusive_create_is_never_a_second_owner` (a creator paused
  inside the gate past the interval while a second acquirer runs with the interval shortened to
  zero by the new test hook `CODEX_CONSULT_TEST_STATE_LOCK_GARBAGE_SECS`: the empty file is not
  taken over, the paused creator completes as the owner, the second gets the lock only after it,
  never two owners at once), the existing move-lock tests.
- **No writer creates `.c3/` while another process moves `.eck/` (found by the acceptance review,
  its answer 2).** A process with the migration switch off (`C3_STATE_MIGRATE=off` or
  `"state_migrate": false`) reached `ensure` and created a fresh `.c3/` with its marker while
  another process moved `.eck/` to `.c3/`; the mover's rename then failed on the non-empty folder
  and the project had two folders. Every place that can create `.c3/` - `ensure`, a writer's
  directory (`prepare_dir` / `write_in`), a run start whose collab directory lies in `.c3/`, the
  move of a legacy collab cache - now goes through one function that applies the rule the module
  states: while `.eck/` exists as a real directory, a fresh `.c3/` is created only under the move
  lock, after the state was read again under it, whatever the migration switch says. The mover holds
  that lock from its re-read through its rename, so no `.c3/` can appear between them. The switch's
  promise is kept: with it off, `.eck/` is not moved and C3 creates and uses `.c3/` for its own files
  - under the lock. A writer that finds the lock held goes on in `.eck/` for that run and says so
  (`.c3/ could not be created now (<who holds the lock>); using .eck/ for this run`) - a write it
  makes there moves with the folder; a run start refuses with one line, nothing started; the cache
  move leaves its entries. A writer that waits for the lock sees the moved `.c3/` and uses it. With
  no `.eck/` (or a `.eck` that is a link, which is never moved) no lock is taken. Inside a C3 child
  `.c3/` is never created by any of these paths (a run start there refuses like `ensure`). The
  `rename_folder` doc no longer counts on an empty `.c3/` made in the same instant: no C3 writer can
  make one; only something outside C3 can. Tests: `a_writer_with_the_switch_off_never_splits_a_move`
  (c3-cli: a mover held before its rename while a switch-off `c3 manifest` writer waits for the lock,
  and while it finds the lock held - one folder, both records, the second says it used `.eck/`),
  `a_fresh_folder_beside_the_old_one_is_created_only_under_the_move_lock`.
- **A linked task directory refuses the run (F57-6).** A run start checked
  and prepared the collab ROOT through handles; the callers then appended `<task>/handoffs` and
  created it with `create_dir_all`, so a task directory that already was a link (no race needed)
  sent the handoff, the reply, the events, the task lock and the ledger wherever it pointed. Now one
  function (`state_dir::run_dirs`, called by the run start of a single run, a detached run, a panel
  and a work run, and by every panel member itself) makes the task directory and its `handoffs/`
  (`work/` for a work run) ready: inside C3's project folder - the collab directory lies in `.c3/` or
  in the legacy `.eck/` - they are created and opened through handles from the collab directory, no
  component followed, and a link (a symlink, a junction, another reparse point) or anything else that
  is not a real directory at either refuses the run before anything is written, in one line that
  names it (`.c3/collab/t is a link (a symlink or junction) - C3 never writes a run's records through
  a link (its target is untouched); nothing was started`). The callers no longer create these
  directories themselves. The same holds OUTSIDE the project folder for what lies below the collab
  directory - a linked task directory is no safer under the default `.collab/`, which is tracked
  content and can arrive with a clone: inside the repository every component from the repository
  root down to the collab directory, the task directory and `handoffs/` is opened without
  following it, and a link at any of them refuses the run; a collab directory outside the
  repository (a `--collab-dir` elsewhere) is the operator's own anchor - created by path and
  followed as given (F52-3) - while the task directory and `handoffs/` below it are not followed.
  A setup that reached its task directory through a link must name the real place with
  `--collab-dir`. A run whose collab directory names `.eck/` while neither folder exists is refused instead of
  creating a new `.eck/` (C3 never creates it again). What remains: the run's later writes (the
  handoff, the reply, the events, the lock, the ledger) still go by path into the directories
  checked at the start - a component swapped for a link AFTER the check, while the run writes, is
  not caught; binding every write of a run to those handles is not in this release. Tests:
  `a_linked_task_directory_inside_the_folder_refuses_the_run_and_creates_nothing` (the task
  directory, `handoffs/` and `work/` as links under `.c3/collab` and, under the move lock,
  `.eck/collab`; a panel member's check; a normal start; a linked task directory under `.collab/`
  still followed), the c3-cli tests
  `a_linked_task_directory_in_the_project_folder_refuses_the_run_and_writes_nothing_there` (a kimi
  consultation and `--detach`, the task directory and `handoffs/` as junctions; a normal run lands
  its reply), `a_panel_with_a_linked_task_directory_in_the_project_folder_is_refused`,
  `a_linked_task_directory_in_the_project_folder_refuses_the_work_run` (Windows and Unix).
- **One table of the Unicode format characters.** The untracked-file names of a `--diff` patch
  (`consult/reviewed.rs`) and the branch name of the red-CI line (`ci_check.rs`) each had their own
  table of the format characters (category Cf) that keep a name from being shown; the CI line's
  lacked U+0890-0891, U+08E2 and the supplementary-plane ones (U+110BD, U+110CD, U+13430-1343F,
  U+1BCA0-1BCA3, U+1D173-1D17A, U+E0001, U+E0020-E007F). Both now use one function
  (`unicode::is_format_char`, the full table), so a branch name holding one of those is no longer
  printed. Test: `the_format_characters_are_exactly_category_cf`.
- **The project folder's writer sweeps its own stale temporary files.** `write_in` writes a file as
  `.NAME.c3-<pid>-<nanos>.tmp` and renames it into place; a writer killed between the two left the
  temporary file for ever. Every write now first removes the files of exactly that pattern in its
  directory that have not changed for a day - through the directory's handle, regular files only,
  never through a link, every failure ignored. The patch placement's sweep of its own temporary
  files is the same helper (`contain::sweep_stale_temps`), which now also leaves any reparse point
  alone. Test: `write_in_sweeps_its_own_stale_temp_files`.
- **"The move lock is held" names the process that holds it.** A live owner's move lock is never
  taken over, so a hung owner keeps `.eck/` from moving to `.c3/` while every command goes on in
  `.eck/` and says so. The line now names what the lock records - the owner's pid, its host when it
  is another host's lock, what it holds the lock for and since when (UTC, to the second) - and `c3
  doctor`'s line for the folder says the same (`[warn] ... stays in use while the move lock is held
  by c3 pid N (move) since ...`; `state_folder.move_lock` in `--json`) while the lock is held. Test:
  `a_live_owners_old_lock_is_not_taken_over`.
- **An untracked file git cannot read no longer fails a `--diff-untracked` run.** An untracked
  file that a virus scanner takes away or holds while the patch is made (or that another process
  holds locked) made `git diff` fail (`open(...): Function not implemented` on Windows) and the
  whole run with it. It was found through C3's own test: its fixture `<U+202E>txt.exe` has the name
  shape of a spoofed executable, Windows Defender quarantined it in the middle of the test
  (`Trojan:Win32/Artoelo.A`, by the name alone), and the failure was first taken for a limit of Git
  for Windows; the fixture is now `a<U+202E>b.txt`. When the batch of all untracked files fails and the
  tracked changes alone render, each untracked file is now tried on its own; the ones git cannot
  read are left out, counted among the left-out untracked files with their reason ("git could not
  read it") in the notice on stderr before the reviewer starts, the handoff header, the prompt line
  and the ledger (`untracked_left_out`, `untracked_left_out_count`, `untracked_left_out_reason`) -
  named only when the name holds no control or format character - and the patch carries the rest.
  Tests: `a_one_revision_patch_leaves_untracked_files_out_unless_asked` (the file with U+202E in its
  name is kept),
  `an_untracked_file_git_cannot_read_is_left_out_and_the_run_goes_on`.
- **A Claude id spelled with dots is its listed id in telemetry.** OpenRouter spells Claude ids with
  dots (`anthropic/claude-opus-4.7`); an id the closed list holds in the claude engine's dash
  spelling (`claude-opus-4-7`) was still sent as `other` unless a catalog had published the dotted
  form. A dotted Claude id of the published Claude shape is now matched against the closed list and
  the published set in its dash form and sent as they spell it; nothing outside them is ever sent.
  Test: `a_claude_id_spelled_with_dots_is_its_listed_dash_id`.
- **CI: the Windows job keeps the virus scanner off the build and scratch directories.** The
  first log of the unsteady Windows job (run 45389095446) showed a fake launcher whose
  `claude --version` "did not finish within 15 s" - a process start, not a test's logic. The
  `c3-cli` tests start hundreds of freshly written scripts and binaries, and a real-time scan of
  each is the first suspect, so the job now excludes the workspace, the temp directories and the
  toolchain from Defender before it builds (`continue-on-error`: an experiment, not a proven cure;
  the hangs stay recorded as open).
- **A patch is still placed when Windows refuses the rename for an instant.** Renaming the
  patch's temporary file onto a name that a virus scanner or an indexer holds open fails on
  Windows with "access denied" - seen on 2026-11-24 on an earlier build's empty claim written a
  moment before, where it failed the placement and with it the run. That one failure is now tried
  again (six tries, 50 ms apart) before it is reported; every other error is reported at once.
  ((0.2.5 A4, F57-5) The one rename left is onto the run's own fresh name, where there are no hard
  links; the retry stays for it.)
- **A hung CI step fails with its log, and the steps after it still run (F38-1).** The per-crate
  test steps of the `ci` workflow ran with `if: success() || failure()` under a job-level limit
  only: a step that hung was cancelled with the whole job, its log was lost and the remaining
  crates never ran - the Windows job hung twice in the `c3-cli` step on 2026-11-23/24 and left
  nothing to read. Every test step now has its own `timeout-minutes` (15 / 15 / 25 minutes; the
  Windows `c3-cli` step takes about 13), so a hung step is stopped as failed with its log kept; the
  job limits are 60 and 50 minutes.
- **The record names the brief bytes the reviewer read (F51-2).** The brief's sha256 in the handoff
  and the ledger, and the line "taken from the committed revision", were decided at plan time while
  the staging read the brief again, so they could name bytes the reviewer never saw. They now come
  from the bytes the staging actually wrote into the worktree and where it took them from - the live
  file, or the full sha of the commit whose file it materialised. Under `--rev` the reviewed
  revision's own copy of the brief is preferred to HEAD's when the live file is missing. Tests:
  `a_committed_brief_unreadable_live_is_materialised_from_head`,
  `a_brief_committed_at_head_but_not_in_the_live_tree_is_taken_from_the_revision`.
- **A file that cannot be re-read is not a changed file (F51-4).** The post-run re-hash of the brief
  and the bound artifacts turned a failed read (a lock, a scanner) into a changed hash and withheld a
  usable reply. A failed read is now retried three times, 200 ms apart; an input that still cannot
  be read is a warning naming it ("could not be re-read"; an artifact's `sha256_after` is
  `unreadable`) and the reply stays usable. A different hash withholds it, and so does an input that
  was there before the run and is gone after it, or was missing and is there now (`sha256_after` is
  then `missing` or the new hash): a deleted input is a changed input, not an unreadable one. Test:
  `a_bound_input_that_cannot_be_re_read_is_a_warning_not_a_change`.
- **A single run that cannot read the findings store at its start says so (F53-3).** A failed read
  froze an empty snapshot, so a reply that reported a real prior finding was rejected at the finish.
  The read is retried once; if it still fails the run lists no prior findings, records a warning
  that the store was unreadable, and the reply's prior report is checked against the store as read
  at the finish (the behaviour before wave D1). Test:
  `a_single_run_whose_findings_store_cannot_be_read_is_not_judged_against_an_empty_snapshot`.
- **The death record: the note wins when the times cannot be compared, and panel members note their
  stream (F51-6, F52-4, F10-7).** The code now agrees with its comment: when either stream's time is
  unknown (a noted file that does not resolve included) or the two are equal, the stream named by
  the status note wins. A panel member - a process of its own - notes its current stream on every
  turn transition too, keyed by its handoff number (`.consult.stream-<NN>.json` in the task
  directory, removed when the member ends normally), so a panel's death record describes a member
  that died in a later turn by that member's own stream. Tests:
  `the_note_wins_when_the_stream_times_cannot_be_compared`,
  `a_panel_death_record_takes_the_stream_its_dead_member_noted`.
- **A reviewer in a throwaway worktree is always told so (F53-1).** Since wave D1 left the collab
  paths out of the uncommitted-paths list, a run whose live tree was otherwise clean got no
  isolation paragraph at all. The paragraph "your working directory is a throwaway git worktree of
  the committed HEAD ..., not the live checkout" (and that the brief was copied in) is now always
  there for an isolated run; only the list of uncommitted paths depends on the live tree. Test:
  `the_isolation_prompt_names_tracked_changes_and_counts_the_rest`.
- **A file name is data, not a command, for an allow-listed agy reader (F32-3).** Since 0.2.5 R1 the
  step classifier looked for command-shaped strings at every depth, so a `read_file` of a file named
  `notes&&draft.md` was classified as writing and a complete review was thrown away. (0.2.5 R8)
  The parameter reading by role that wave D1 put here, and the recursive parameter merge of F24-4,
  are gone: a step is now judged by its tool's name alone and agy reviews a throwaway worktree
  (see Changed). Test: `agy_readers_are_readers_whatever_their_parameters_say`.
- **A changed bound input withholds the reply of every engine that reads the live tree.** Wave R1
  did it for agy only: a brief or a bound artifact that changes during the run is not ambient drift
  - the reply may answer other bytes than the review is bound to - so it is kept (the raw reply as
  `.reply.json`, a codex run's salvage as `.partial.md`) and the outcome is the failed read-only
  check naming the input, class `permission`. The same rule now applies to muse, grok, claude, kimi
  and codex runs on the live tree (codex had no tree check: the brief and the artifacts are
  re-hashed after its last turn). A run in a throwaway worktree read staged copies and stays
  unaffected; the http reviewer reads only its pack. Tests:
  `a_changed_bound_input_withholds_the_reply_of_every_live_tree_engine`,
  `a_codex_run_whose_brief_changed_is_not_recorded_as_usable` (and its `--rev` counterpart in the
  same test).
- **A single run is judged against the findings it was shown.** A panel member carried the
  immutable records of the prior findings it was listed; a single run read the findings store again
  at its finish for the prior-blocker lines, the prior-blocker semantics and the validation of its
  prior report. A single run now takes the same snapshot once at its start, and one code path
  judges both: a blocker resolved meanwhile stays the open blocker it was shown, a finding filed
  meanwhile is not reportable. Test: `a_single_run_is_judged_against_the_findings_it_was_shown`.
- **One HEAD value decides what a `--rev` run is compared with (F29-3).** Whether the reviewed
  revision is the live HEAD (and the task's open findings are withheld) and the `live_head` the
  records name came from two resolutions of HEAD taken at different moments. Both now come from ONE
  resolution at the start of the run; a HEAD that cannot be resolved withholds the findings (the
  safe side) and says so - a run warning, `live_head_unresolved: true` in the ledger, "Live HEAD at
  launch: could not be resolved" in the handoff. Test:
  `one_head_value_decides_what_a_rev_run_is_compared_with`.
- **The death record prefers the newest stream of its own run (F24-2, F10-7, F11-11).** A death record
  took the events path of the run's own pending record and used the stream the run noted in its
  status record only when that field was empty, so a pending record that still named the main
  stream (its rewrite failed, or a later rewrite kept its old `events`) hid a later continuation
  stream. When the two name different streams, the one written last now wins (the newer events
  file; the note on a tie or an unknown time, as it carries the turn transitions). Test:
  `the_death_record_prefers_the_newest_stream_of_its_own_run`.
- **The uncommitted-paths paragraph leaves the collab directory out.** The prompt of a reviewer in
  a throwaway worktree of HEAD named, among the tracked paths that differ from the revision, paths
  under the collab directory - whose task handoffs are staged into the worktree - and counted the
  untracked collab directory among the paths HEAD does not hold. Every path under the collab root
  is now left out of the list and out of both counts. Test:
  `the_isolation_prompt_leaves_the_collab_directory_out`.
- **Only the run's own patch is exempt from the collab comparison (F27-3).** Every collab path that
  started with `<task>/handoffs/<stem>.` was ignored, so a reviewer could write
  `handoffs/<stem>.notes.md` and the run still looked clean. Exactly the patch file the run wrote
  and its write-temp name (`<stem>.<sha8>.<32 hex>.tmp`; (0.2.5 A4, F57-5) since then
  `<stem>.<sha256>.<32 hex>.tmp`, and without hard links the run's own name and a sibling's own name
  for the same bytes) are exempt now, nothing else with that prefix. Test:
  `the_runs_own_patch_is_the_only_exempt_file`.
- **The `--diff` patch placement is contained from the start (F43-1).** The handoffs directory was
  created by path name (`create_dir_all`) before the anchor was opened on it, so a link above it
  redirected the create; it is now opened - and what is missing of it created - through handles
  from a root on the repository (or on the collab directory when that lies outside it), and a link
  anywhere above `handoffs/` refuses the placement. (0.2.5 R8) The claim protocol this wave added
  for file systems without hard links (F43-2, F43-3: the stale-claim takeover, the removal of the
  own claim) is gone - a patch name is its content (see Changed; (0.2.5 A4, F57-5) published by a
  hard link, or without hard links under a name of the run's own - never by a replacing rename).
  Test: `a_link_above_the_handoffs_directory_refuses_the_patch`.
- **Many excluded paths no longer refuse a review (F28-2).** A diff with more than 500 changed paths
  excluded by the ignore rules or `.c3ignore` / `.eckignore` was refused outright, because each
  became an `:(exclude)` pathspec on git's command line (`git diff` takes no pathspec file). The
  excluded paths now go through a temporary index fed on stdin: a range is diffed as
  `git diff --cached <base>` against an index of its `to` side where every excluded path carries
  its `base` version (or is absent), and a one-revision diff's temporary index gives each excluded
  path the revision's version marked skip-worktree; with nothing excluded the command is the plain
  `git diff` it was. The count no longer matters; only a real git failure refuses. Test:
  `many_excluded_paths_do_not_refuse_a_review` (501 excluded paths, modified, deleted and added,
  in a `..` and a `...` range and the working tree; the rest of the patch byte for byte the plain
  `git diff`).
- **A patch that is not valid UTF-8 is said to be altered (F28-3).** The patch text goes through a
  lossy UTF-8 conversion before redaction (the redactor works on text), which silently replaced
  invalid byte sequences. The conversion stays and is recorded: the ledger's `diff` record gains
  `lossy_utf8: true` (and `lossy_utf8_sequences`), and the prompt line says that N byte sequences
  were not valid UTF-8 and were replaced, so the reviewer knows the patch is not byte-exact there.
  Test: `a_patch_that_is_not_utf8_is_said_to_be_altered`.
- **A missed upper-tier seat is warned about when the pins fill the panel (F30-2).**
  `tier_missed_warning` stayed silent when the required entries took every seat while an eligible
  weighty entry of another lab existed. It now warns "upper tier missed: the required entries fill
  all N seats", naming the purpose and the weighty entries kept out. Test:
  `a_missed_upper_tier_is_warned_about_when_the_pins_fill_the_panel`.
- **A replay never mixes precise and rounded values (F30-4).** A recorded `ext.c3.draw` block with an
  invalid row was filtered row by row, so the replay weighed precise values for some candidates and
  rounded ones for others without saying so. A block with any invalid row (no position or score, a
  multiplier that is not a positive finite number, or a block that is not a list) is now treated as
  absent: the rounded values for every candidate, and the "values are rounded" note with the
  reason. Test: `a_replay_never_mixes_precise_and_rounded_values`.
- **A brief committed at the reviewed revision need not exist in the live tree (F10-3, F11-4).** Wave
  R1 materialised such a brief during the staging, but the entry check still refused the run because
  the file was not on disk. When the run reads a throwaway worktree, the entry check now accepts a
  brief path that the staging can take from a committed revision (HEAD, else the worktree's own),
  hashes those committed bytes, and the handoff's `Brief:` line says "Taken from the committed
  revision ..."; without a throwaway worktree, or committed nowhere, it is refused as before. Test:
  `a_brief_committed_at_head_but_not_in_the_live_tree_is_taken_from_the_revision`.
- **`c3 providers` counts a kept reply only while it is still there (F11-12).** The reply-outcome
  breakdown counted a failed run by the kind of reply its entry recorded, so a sidecar or a partial
  deleted since still counted as kept. Such a run is now counted in a new column, "failed, kept
  reply recorded but its file gone" (`failed_reply_gone` in `--json`), found through each
  consultation's task directory; an older entry without a consult id counts as before. Test:
  `a_kept_reply_counts_only_while_its_file_is_still_there`.
- **The deadline stop leaves nobody behind (F46-1, F45-2, F45-7).** `ScanChildren::stop` ended its
  sweep at the first round that terminated nobody, so a grandchild forked after that read survived;
  it skipped the tree of a registered child whose start time could not be read (already a zombie);
  when the process table could not be read it swept only the children still running; and it waited
  for a killed child with an unbounded `wait()` while holding the set's lock. The stop now takes the
  registered children out of the set first (the lock is not held past that), reads the table once
  before the kills and then sweeps round after round - at most five, 50 ms apart - until a round
  after the kills finds no live descendant, also following the children of what an earlier round
  terminated; a child whose start time is unknown is swept from the time the set spawned it (which
  the set now records; 2 s slack for Linux's whole-second boot time); on Unix every registered
  child's process group is killed, and when the table cannot be read the group of a child already
  reaped too (while no process holds its pid again); the wait for a killed child is bounded by the
  same 5 s as the wait for what was terminated, and whatever is still alive at the end is reported
  (`StopReport`; the caller of `within` prints one `c3:` line naming the pids). The remaining limit:
  a descendant that left the child's process group AND was re-parented - a daemon made with `setsid`
  after a double fork - is reachable by neither net and is not swept; and on Windows a child already
  reaped leaves no net when the table cannot be read (`taskkill /T` walks only from a live root, and
  an exited child's bare pid may belong to another process). Tests: `bounded.rs`'s
  `a_zombie_child_is_swept_from_the_time_it_was_spawned`,
  `a_grandchild_forked_after_a_read_is_caught_by_the_next_round`,
  `the_sweep_says_when_its_rounds_ran_out` and (Unix) `a_background_grandchild_of_a_running_child_is_reaped`.
- **The secret rule is strict again, with one narrow exception (F46-2, F45-1).** The first cut of the
  plain-word exemption let any single-case word under 24 letters pass under EVERY keyword and asked
  for a key shape under `auth`, so `password = "hunterhunterhunter"`, `password = "troubadour"` and
  `auth = "correct horse battery staple"` were neither refused by `c3 finish` nor redacted in packs
  and `--diff` patches. Under `api_key`, `secret`, `password`, `passwd`, `pwd`, `token`,
  `credential` and `authorization` a quoted value of 8+ characters is a secret again, plain word or
  not. Only under the BARE keyword `auth` is a value that is one single word of letters (one case,
  or one leading capital: `endpoint`, `subscription`, `oauth`) a setting; every other value of 8+
  characters there - a passphrase with spaces, a digit string, a key-shaped value - is a secret.
  A value that is C3's own redaction marker (`[REDACTED:<kind>]`, as a saved `--diff` patch holds
  it) is never a secret, under any keyword, so the gate passes such a patch and redacting twice
  changes nothing. Tests: `redact.rs`'s `a_plain_word_under_the_bare_auth_is_not_taken_for_a_secret`,
  `values_that_can_be_secrets_are_still_redacted` (the reviewers' cases among them) and
  `a_redaction_marker_is_never_taken_for_a_secret`, and `secret_gate.rs`'s
  `a_plain_word_under_auth_passes_the_gate` extended (a saved patch with markers among them).
- **`c3 branches --close` never deletes a file git does not show (F46-3, F45-8).** The cleanliness
  check used `git status --porcelain`, which lists no ignored file, and `git worktree remove` then
  deleted the worktree with its `.env` or build output unlisted. The check is now `git status
  --porcelain --ignored --untracked-files=all`: a worktree holding any file outside the committed
  tree - changed, untracked or ignored - is refused with the count and the first five names, and
  nothing is removed. What `git branch -d` would refuse is still found out before the worktree is
  touched; the remaining error texts now say exactly what was done (a `git worktree remove` that
  failed part-way says the worktree may be partly removed and the branch kept; a `git branch` that
  could not run after the removal says the worktree was removed). Test: `branches.rs`'s
  `close_never_deletes_an_ignored_file_of_the_worktree` (and the dirty case of
  `close_removes_a_merged_branch_and_its_clean_worktree_and_refuses_the_rest` now names the file).
- **The CI check asks about THIS repository (F46-4, F45-3).** gh inferred the repository from
  `GH_REPO` or from the remotes by its own rules, so another repository's default branch could be
  remembered for a day and its runs reported as this checkout's CI. Every gh call now names the
  repository - `gh repo view <owner/name>`, `gh run list ... -R <owner/name>` - taken from the
  `origin` remote's URL (no check when `origin` is not on GitHub), or without an `origin` from the
  one GitHub repository the remotes name (several different ones: no check, gh is not started), and
  `GH_REPO` and `GH_HOST` are removed from gh's environment. The remembered default branch lives in
  a file of its own (`ci-check-<hash>.branch.json`, written whole), so it and the daily attempt
  record can no longer drop each other's fields. Tests: `ci_check.rs`'s
  `gh_is_asked_about_this_checkouts_repository` and
  `the_branch_memory_and_the_attempt_do_not_drop_each_other`, and the end-to-end
  `the_check_asks_about_this_repository_whatever_gh_repo_says`.
- **`--skip-checks` tells the truth, a stopped check shows what it printed, and the checks phase
  has a ceiling (F46-6, F45-6, F45-5, F45-9).** With `--skip-checks` and a `.c3/config.json` that
  could not be read or parsed, the output and the journal said "none configured"; they now say that
  the configuration could not be read (with why) and that no check was run (`--json`:
  `checks.config_error`). A check stopped at its deadline was reported as having printed nothing:
  its output is now copied as it is read, and the refusal shows its last 20 lines before it was
  stopped. The whole checks phase has a ceiling of 10 minutes: a check runs under 120 s or what is
  left of the ceiling, whichever is shorter, and a check stopped at the ceiling - or one the ceiling
  leaves no time to start - refuses like a failed check and names the checks that did not run.
  Tests: `finish::checks`'s `a_stopped_check_shows_what_it_printed_before_it_was_stopped` and
  `the_checks_phase_refuses_at_its_ceiling_and_names_what_did_not_run`, and `finish.rs`'s
  `passing_pre_finish_checks_run_in_the_repository_as_a_child` extended by the skip over a broken
  file.
- **The hook scripts cap the one line they print themselves again, and lines that differ past the
  cut stay two lines (F45-4, F46-8).** When no line carried a known prefix, both wrapper scripts
  forwarded the first non-empty line uncapped - a panic or library line that bypassed `c3 hook`'s
  cap reached the session whole; that line is cut at 400 again (`cut -c1-400`, `Get-CutText ...
  400`). And two hook lines that shared their first 397 characters became one string after the cap,
  so the scripts' de-duplication printed only one: `c3 hook` now de-duplicates on the whole line
  before the cap, and a cut line ends in `... #` and the first 8 hex digits of the SHA-256 of the
  whole line (387 characters kept), so lines that differ past the cut print as different lines and
  the same line always prints the same. Tests: `hook::tests`'s
  `lines_that_differ_past_the_cut_are_both_printed` and
  `hook_lines_are_capped_at_400_characters_on_a_character_boundary` updated, and `hook_scripts.rs`'s
  `the_scripts_keep_their_fallbacks` (a 5000-character unprefixed line) and
  `the_ps1_forwards_a_long_line_whole` updated (the fallback stops before a surrogate pair).
- **"Cannot tell" no longer keeps a dead process alive (F46-7).** On Linux a pid whose
  `/proc/<pid>` does not exist read as "cannot tell" whenever the read failed with anything but
  "not found" (a permission error among them), and `pid_alive` counts "cannot tell" as alive, so a
  dead holder could keep a lock or a recovery record. A missing `/proc/<pid>` is now gone whatever
  the error, unless `/proc` hides pids (then `kill -0` decides, as before); "cannot tell" is left
  for an entry that exists and cannot be read. The maintenance pass's spawn gate (`lock_held`) also
  applies the lock's own 6-hour stale rule, which only the take-over path applied, so an owner whose
  identity cannot be read does not block a new pass longer than that. Tests:
  `an_unreadable_proc_entry_is_cannot_tell_never_gone` (Linux) extended and `maintain`'s
  `an_owner_that_cannot_be_told_dead_holds_the_lock_only_until_it_is_stale`.
- **A public catalog source is no longer read from an IPv6 address that embeds or tunnels an IPv4
  (F39-1, F38-2).** Only IPv4-mapped and NAT64 addresses were judged by the IPv4 they carry, so
  `::10.0.0.1` (IPv4-compatible), `2002:a00:1::1` (6to4) and Teredo `2001::/32` passed the lookup
  and peer checks, as did the benchmarking range `2001:2::/48`. All four ranges are now in the
  non-global table and refused outright, at the name lookup and at the peer of every hop; a real
  global address still passes. Test: `the_address_classes_of_a_public_source` has a row per range
  for the address check, the lookup check and the peer check.
- **The price comparison of a swap across a major uses one vendor's rows only (F39-2).** When the
  candidate's vendor had no row for the seated id, the ratio fell back to the first row of that name
  from any vendor, so a decoy row with a high price made an expensive successor look cheap and it
  was seated. The ratio now comes only from the seated id's and the candidate's rows of the same
  vendor slug in the same source; without a priced seated row under that vendor there is no ratio
  and nothing is held. Test:
  `a_successor_across_a_major_priced_over_twice_the_seated_one_is_announced_only` (decoy row, and a
  candidate vendor without a seated row).
- **A roster token is an alias only when it names the model's maker or the plan (F39-4).** The
  leading token of a roster id was also dropped when it only matched the lab of the endpoint host,
  so `kimi-qwen3.8-max` on a moonshot host was matched to the catalog's `qwen/qwen3.8-max` and its
  successor written back as `kimi-qwen4-max`. The token is now dropped only when it names the maker
  of the rest of the id or the plan's vendor. Test:
  `a_roster_id_with_a_lab_token_is_matched_through_the_catalog_spelling` (the host-only case has no
  alias; `kimi-k3.1-pro` still works).
- **What C3 does inside a reviewer's worktree and inside the repository can no longer be redirected
  by a directory swapped for a link (F32-1, F24-1, F29-2, F18-5, F27-2, F16-5, F13-9, F28-1).**
  Staging into the throwaway worktree (both the files written and the brief, `handoffs/` and
  `state.md` read), the post-checkout sweep of links that lead out, the worktree's removal, the
  repository repair's file replacement, the rules-file rewrite and the `--diff` patch now work
  relative to directory handles opened once and verified on the handle (the new module `contain`,
  on the `cap-std` crate), never by a path name checked first and resolved again - a directory
  replaced by a junction or symlink after it was checked can no longer send a write, a read or a
  removal outside, and the three "decided boundaries" (a local process racing the window) are
  withdrawn. What changes for a user (F43-7, F44-2): every path below a root is opened one
  component at a time without following any, so a symlink or junction INSIDE the repository on the
  way to the brief now refuses the run, and one on the way to `handoffs/` or `state.md` is skipped
  and counted - before, such a link was followed when its target stayed inside; every Windows
  reparse point counts as a link (a cloud-sync placeholder too); and the worktree's directory cannot
  be renamed while a reviewer runs in it. Two roots ARE followed by design: the repository root and
  a task's handoffs directory when their own path is a link (where they live is not C3's to judge;
  only what lies below them is contained) - ((0.2.5, F57-6) a run now refuses at its start when its
  task directory or `handoffs/` is a link; of a collab directory only one OUTSIDE the repository is
  followed). A staged or repaired file is written to a new temporary
  name and renamed relative to its directory's handle, and a temporary file whose removal fails is
  reported instead of ignored; the removal unlinks and deletes through the handles and then runs
  only `git worktree prune` (no `git worktree remove --force`, no `remove_dir_all` on a path); the
  patch is always written to a temporary file first and placed from it - a hard link, or
  ((0.2.5 R8) instead of the claim of this wave) on a file system without hard links one atomic
  rename ((0.2.5 A4, F57-5) onto a fresh name of the run's own, `<stem>.<sha256>.<32 hex>.patch` -
  no longer onto the content-addressed name, where a rename replaced a file that appeared there) -
  so an interrupted placement never leaves a partial patch under its name. What can remain: after a crash, a temporary file (C3's own patch temp files older
  than a day are removed when a patch is placed in that directory); after a failed removal, the
  entries that could not be removed (named in the error, the worktree kept for `c3 doctor --fix`).
  On Windows the protection rests on
  the handles C3 holds: a directory opened by cap-std cannot be renamed or deleted, nor can any of
  its ancestors, while the handle is open. Tests: `contain::tests::cap_std_refuses_a_link_as_a_path_component` (the proof:
  a junction, and a directory symlink where one can be made, refuses as a path component, pointing
  out or in; a root that is a link is refused; a held directory and its ancestors cannot be renamed
  on Windows), and the race tests driven by the new hook `CODEX_CONSULT_TEST_CONTAIN_RACE` (a
  directory swapped for a link between validation and use) for staging a file and publishing it,
  reading the brief and `handoffs/`, descending in the walk and replacing a repository file, plus
  the patch placement without hard links (`CODEX_CONSULT_TEST_PATCH_NO_HARDLINK`,
  `CODEX_CONSULT_TEST_CONTAIN_WRITE_FAIL`). `c3_core::test_hooks::set_thread_hook` lets an
  in-process test set a hook for its own thread only.
- **The handle containment now covers the project folder, the repair's discovery and a removal
  that fails part way (F47-2, F43-5, F46-5, F43-6, F44-3).** (1) What `.c3/` and the move do inside
  the repository goes through handles from the repository root: `.c3/` and its stubs are created
  relative to its own handle, a writer's directory is reached one component at a time without
  following any and its file is published by a temporary file and a rename (so a second hard link
  of `JOURNAL.md` or a manifest no longer sees the write), the old tool's leftovers and the legacy
  cache are renamed between directory handles, and `.eck` is renamed to `.c3` relative to the
  repository root's handle, never over an existing name. The move lock stays a file in git's
  directory; `.gitignore` and the files a caller writes into a prepared directory (scouts,
  explains, the index store, the copy into `lastsnapshot/`) stay path-based. (2) The repository
  repair finds its candidates the way it edits them: `.claude/agents/*.md`, `AGENTS.md` and the
  rules files are listed and read through handles; nothing is read through a link on the way
  (before, the bytes behind a linked `.claude/agents` were read and only the edit was refused).
  Such a link is not a failed repair - no repair is known to be due behind it: the SessionStart
  line says nothing about it, and `c3 doctor` names it in an `[i] <file>: not checked - <why>`
  line (`not_checked` in `--json`) and stays clean, so a repository whose `AGENTS.md` is a link to
  `CLAUDE.md` is an ordinary one. (3) The hook's repository
  test reads `HEAD` as git does: a symref under `refs/` or an object id, else no repository; a
  `.git` that is a symlink stays refused on purpose. (4) A worktree removal that meets an entry it
  cannot remove goes on, removes everything else, names up to ten entries left and keeps the
  worktree's `.git` file, so `c3 doctor` lists it and `--fix` finishes it once the obstacle is gone.
  (5) On Unix a file is opened for reading without blocking, so a FIFO swapped in after the kind
  check is refused on its handle instead of hanging the run (Windows has no FIFO in the file
  system). Tests: `state_dir::tests::a_directory_in_the_old_folder_swapped_for_a_link_takes_nothing_outside`,
  `the_folder_swapped_for_a_link_before_a_write_never_writes_outside`,
  `repo_models::tests::discovery_never_reads_through_a_link_on_the_way`,
  `rules::tests::a_linked_rules_directory_is_never_read_and_its_repairs_are_refused`,
  `git_dir_tests::a_head_that_git_would_not_take_is_no_repository`,
  `contain::tests::a_failed_removal_removes_what_it_can_and_names_what_is_left`,
  `a_fifo_swapped_in_after_the_kind_check_is_refused_without_blocking` (Unix), and `branches.rs`'s
  `a_half_removed_throwaway_is_listed_and_finished_once_the_obstacle_is_gone`.
- **A linked `.c3` or `.eck` is refused by every run that reaches into it (F48-1).** A consultation,
  a panel or a work run whose collab directory lies in a `.c3` or `.eck` that is a symlink, junction
  or other reparse point - or reaches the folder through another link in the repository - is
  refused at its start with one line and creates nothing, also when the path was already resolved
  through the link; a collab directory inside `.c3/` is created through handles. The read-only
  commands no longer read through a linked `.eck` either. Test:
  `a_collab_directory_under_a_linked_folder_refuses_the_run_and_creates_nothing`.
- **Nothing creates the legacy folder `.eck/` again, and a live owner's move lock is never taken
  over (F48-2, F48-4, F47-3).** A command that went on with `.eck/` because the move had to wait
  writes at the folder's place at the moment of the write (`c3 explain` included; the embedded
  index refuses a `.eck` that is gone), so a move that completes meanwhile never leads to a second
  `.eck/`. The lock of an owner alive on this machine is waited for and then the command goes on
  without moving, whatever the lock's age. A collab path is recognised as inside the folder also by
  identity - an 8.3 short name such as `ECK~1`, an alias that is a link to the folder (refused) -
  and on Windows a name with trailing dots or spaces (`.eck.`, `.c3 `) is the folder. Tests:
  `a_writer_that_lost_the_lock_ends_in_the_new_folder_and_no_old_folder_returns`,
  `a_live_owners_old_lock_is_not_taken_over`, `short_names_and_aliases_resolve_to_the_folder`,
  `collab_paths_are_normalised_before_they_are_classified`.
- **The old tool's `anchor` is never taken for C3's own (F48-3, F47-1).** `.c3/` now carries a marker
  file (`layout`), written last by a fresh creation, by the move and by the finish of an unfinished
  folder. A `.c3/` without it is unfinished: the next writing command sends the old tool's
  leftovers at its top - a lone `anchor` too - to `legacy/` before it writes the marker, and no
  snapshot anchor is read from it meanwhile; the legacy cache move (C3's own `anchor`) runs after
  that rule. A `.c3/` made by an earlier 0.2.5 build has no marker, so its `anchor` goes to
  `legacy/` once and the next snapshot is a full one. Test:
  `a_folder_without_its_marker_sends_a_lone_anchor_to_legacy`.
- **The secret gate no longer takes a plain word under the bare keyword `auth` for a secret; the
  MCP finish tool takes `allow_secret`.** `c3 finish` refused three commits on 2026-11-22/23 over
  the test-fixture line `m.entry.auth = "endpoint".into();` and a saved patch holding it: the
  keyword-assignment rule took any quoted value of 8+ characters after `auth`. Under the bare
  keyword `auth` a value that is one single word of letters (one case, or one leading capital,
  fewer than 24 of them) is now a setting; every other value of 8+ characters under `auth`, and
  every such value under `auth_token`, `authorization` (now a keyword of its own), `api_key`,
  `password` and the rest, is a secret as before (the first cut of this change exempted a plain word
  under every keyword and asked for a key shape under `auth`; see the entry on the strict rule
  below), and a value an earlier rule already replaced keeps that rule's kind. Packs and `--diff`
  patches use the same rules; every service-shaped key is still caught by its own rule. The MCP tool
  `c3_finish_task` gains `allow_secret` (repo-relative paths, the CLI's `--allow-secret`; a path
  that leaves the working directory is refused). Tests: `redact.rs`'s
  `a_plain_word_under_the_bare_auth_is_not_taken_for_a_secret` and
  `values_that_can_be_secrets_are_still_redacted`, `secret_gate.rs`'s
  `a_plain_word_under_auth_passes_the_gate`, the MCP unit test
  `finish_task_passes_allow_secret_paths` and `mcp_integration.rs`'s
  `mcp_finish_task_lets_an_allowed_secret_file_through`.
- **A deadline stop reaps what an exited child left behind, and a child stays under the deadline
  until it has exited (F31-1, F32-4).** `ScanChildren::stop` killed the tree of a registered child
  only while that direct child still ran, so a `gh.cmd` shim that had already exited left its `ping`
  alive; and the shared runner took a child out of the registry as soon as both its pipes reached
  their end, so a wrapper that closed them and kept running was no longer stopped. The set now
  remembers every child it started (pid, start time, when it was reaped) and keeps each one
  registered until its exit is confirmed (polled under the lock); the stop reads the process table
  once per round (now up to five, until a round finds nothing - see the fix below), kills the
  registered children by their handles and terminates every
  descendant of every child the set started - found by parent pid and, on Unix, by the child's own
  process group (each registered child leads one; an orphan keeps it) - only when it started at or
  after that child did (for a child already reaped, not after it was reaped) and is still the same
  process; it waits (at most 5 s) for what it terminated to leave, and the caller of `within` gives
  the worker thread 1.5 s to end and joins it. Tests: `bounded.rs`'s
  `the_deadline_reaps_what_an_exited_shim_left_behind`, `a_child_stays_under_the_deadline_until_it_has_exited`,
  `the_sweep_takes_only_what_the_children_started` and `an_orphan_is_found_by_its_process_group`.
- **The CI check learns the default branch from GitHub when the clone does not know it (F31-2).**
  With `origin/HEAD` unset and no `c3.defaultBranch` the check asked gh about a local `main` or
  `master`, which need not be the repository's default. It now asks gh once
  (`gh repo view <owner/name> --json defaultBranchRef`, under the same deadline), remembers the
  answer for a day (`default_branch`, `default_branch_at`; in a file of its own since the fix below,
  `ci-check-<hash>.branch.json`, so the attempt record and the memory cannot drop each other), and
  stays silent when gh cannot say - it never guesses. Tests:
  `ci_check.rs`'s `the_check_asks_github_for_the_default_branch_when_the_clone_does_not_know` and the
  end-to-end `without_origin_head_the_check_asks_github_for_the_default_branch`.
- **Two session starts in one repository ask once (F31-3, F30-3, F31-5, F30-6).** The hook's daily
  gate is now taken by creating the day's claim file `ci-check-<hash>.<YYYYMMDD>.claim` exclusively
  before gh starts: the loser of the race prints nothing, and a claim left by a killed hook holds
  only for the rest of that UTC day. The gate record's temp file is removed on every failed write,
  and the prune removes claim and temp files older than 30 days too (a `.tmp` left by a killed hook
  was never removed). Tests: `the_daily_gate_is_taken_by_one_session_only`,
  `stale_claim_and_temp_files_are_pruned` and the end-to-end
  `two_hooks_starting_together_in_one_repository_ask_once`.
- **GitHub's own host names are GitHub (F31-4, F30-5).** A remote whose host is `github.com`,
  `www.github.com` or `ssh.github.com` (GitHub's port-443 SSH host) is a GitHub remote; nothing
  else - no substring match, no other subdomain. Tests: `github_remotes_are_recognised_by_their_host`
  extended and the end-to-end `githubs_own_host_names_are_github_remotes`.
- **The hook's branch-line test no longer depends on the 2-second deadline.**
  `hook_prints_one_line_only_when_something_is_unknown` came back empty once on a loaded machine.
  The deadline has a test-mode override (`CODEX_CONSULT_TEST_BRANCH_DEADLINE_MS`, honoured only with
  `CODEX_CONSULT_TEST_MODE`), and the branch tests that are not about the deadline run with 60 s.
  Test: `the_branch_deadline_is_two_seconds_unless_a_test_overrides_it`.
- **"Cannot tell" on Linux too (F12-1).** A `/proc/<pid>` that exists but cannot be read
  (permission, `hidepid=1`) now reads as "cannot tell" (a blank start time), never "gone"; so does a
  pid `/proc` hides altogether (`hidepid=2` / `invisible`) unless `kill -0` reports no such
  process; a missing `/proc/<pid>` without hidden pids is gone whatever error the read gave (see
  the fix below); and a zombie reads as gone, as the documentation always said. Test:
  `an_unreadable_proc_entry_is_cannot_tell_never_gone` (Linux).
- **A repository is recognised the way git recognises one (F13-2).** The hook's own root search (no
  process started) took any `.git` directory holding `HEAD`. It now applies git's own test: `HEAD`,
  and in the common directory (the one a `commondir` file names, else the directory itself) an
  `objects` and a `refs` directory - for a `.git` directory and for the target of a `gitdir:` file
  alike (a linked worktree's `.git/worktrees/<name>` has `HEAD` and `commondir`). Tests:
  `a_repository_is_recognised_the_way_git_recognises_one` and `init_doctor.rs`'s
  `the_hook_repairs_nothing_behind_a_git_directory_git_would_not_take`.
- **The one-time move of `.eck/` to `.c3/` can no longer lose, split or leak a project's records
  (F36-1 .. F36-8).** A review of the S1 move found that what surrounds its one rename could split
  a project's journal and reports between two folders, write through a link, or be set off by a
  command that only reads. Now: (F36-1) the move is made under an exclusive lock file in the
  repository's git directory (pid and start time inside; taken over when its owner is gone, when
  its owner cannot be shown alive - another host's - and it is older than 10 minutes, or when it
  does not parse and is older than 10 seconds; since R7b never while its owner lives); a
  command that cannot take it within a few seconds, and any rename error, reads the state again -
  `.c3/` there and `.eck/` gone means the move happened and `.c3/` is used - and every writer takes
  its folder from the state as it is now, so no path under `.eck/` is ever created again after the
  move. (F36-2) The old tool's `anchor`, `update_seq` and `telemetry_queue.json` go to `legacy/`
  and `lastsnapshot/AnswerToSA.md` to `last-report.md` inside `.eck/` first, each step safe to
  repeat, and the rename is the last step; a leftover that cannot be relocated stops the move, and
  a `.c3/` an S1 build left half done (`update_seq` or `telemetry_queue.json` at its top) is put
  right by the next writing command, its `anchor` included. (F36-3) `.c3` must be a real
  directory: one that is a symlink, junction or other reparse point is refused by every writing
  command and reported by `c3 doctor`; a linked `legacy/` or `lastsnapshot/` stops that step, a
  directory behind a link is never walked, the cache is never moved out of a collab directory
  reached through a link, and no file is written through a link (each component is checked with
  `symlink_metadata` and the reparse-point test right before the operation). (F36-4) A command
  that only reads never moves the folder: `resolve_collab_root` / `state_dir::collab_dir` are the
  non-moving default and only a real consultation or work start uses the moving resolver.
  (F36-5) A collab path is normalised (`.`/`..` resolved lexically, an absolute path made
  relative, the file system's case rule) before it is classified. (F36-6) The live-run check fails
  closed (a directory or record that cannot be read, an owner whose start time cannot be read, a
  held task lock all count as live), is repeated under the lock right before the rename, and a
  consultation or work run that starts under `.eck/` creates its records under the same lock.
  (F36-7) The README and this file say what the switch does. (F36-8) A `--diff` patch leaves out a
  `.c3/` or `.eck/` directory at any depth (`:(exclude,glob)**/.c3/**`), as packs and snapshots do.
  Tests: `state_dir` units for the lock, the re-read, run start against move, the relocation, the
  half-done finish, links, normalisation, the read-only resolver, the fail-closed guard and the
  nested patch exclusion; `finish.rs` two writers at once, a kill before the rename, the half-done
  folder, links inside `.eck/`, every read-only command with `--collab-dir .c3/collab`, every
  collab spelling, unreadable run directory and start time, the switch; `init_doctor.rs` a `.c3`
  link refused by the writers and reported by `c3 doctor`.
- **The `c3 maintain` tests no longer read the machine's process table.** A pass skips an engine
  whose CLI is running ("grok skipped: in use"), and the integration tests drove that probe against
  the real machine: on 2026-11-23 six to nine of them failed whenever another session's real
  `grok.exe` was running. Under `CODEX_CONSULT_TEST_MODE` the probe is now answered by the test
  hook `CODEX_CONSULT_TEST_MAINTAIN_IN_USE` (the engine names to report as in use; none by
  default) and the process table is never read. Tests: a pass with a stand-in engine process
  running updates its engine, and the hook drives the "in use - skipped" path (nothing installed,
  due again the next day).
- **A public model source is read only from a global unicast address (F27-1, F15-1).** The address
  check let through `0.0.0.0/8` (a local route on Linux), the CGNAT range `100.64.0.0/10` (also
  overlays such as Tailscale), benchmarking `198.18.0.0/15`, the documentation ranges and IPv6
  site-local `fec0::/10`. It is now one table of every non-global range - IPv4 `0/8`, `10/8`,
  `100.64/10`, `127/8`, `169.254/16`, `172.16/12`, `192.0.0.0/24`, `192.0.2.0/24`, `192.88.99.0/24`,
  `192.168/16`, `198.18/15`, `198.51.100.0/24`, `203.0.113.0/24`, `224/4`, `240/4` and the broadcast
  address; IPv6 `::`, `::1`, `64:ff9b:1::/48`, `100::/64`, `2001:db8::/32`, `fc00::/7`,
  `fe80::/10`, `fec0::/10`, `ff00::/8` - with an IPv4-mapped address (and one of the NAT64 prefix
  `64:ff9b::/96`) judged as the IPv4 address it carries, at the name lookup and at the peer of every
  hop. Test: one row per range, the neighbours just outside them, the lookup and the peer check.
- **A `c3 maintain` fetch never goes through a proxy it did not check (F29-1).** ureq 2.12 takes a
  proxy from `ALL_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY` only with its `proxy-from-env` feature
  (off in C3's build) or when asked (the http engine asks, on purpose, for its own requests); the
  maintain client relied on that default. It now refuses the environment's proxy explicitly, so a
  dependency that turns the feature on through cargo's feature unification cannot route a public
  fetch through a proxy - through which the peer check would see the proxy, never the origin. C3
  has no proxy setting for `c3 maintain`. Test: with `HTTPS_PROXY`, `HTTP_PROXY` and `ALL_PROXY` on
  a dead local port, a maintain GET still connects to its origin. (A name lookup that stalls past
  the deadline still leaves its thread to finish on its own: a recorded boundary.)
- **The connect gets the time left after the name lookup (F14-1).** Each hop of a maintain GET now
  resolves its name first, under the fetch's one deadline, and only then builds its client: the
  connect and the request get the time that remains AFTER the lookup, never the budget computed
  before it, and they connect to the very addresses the lookup vetted (no second lookup). Test: a
  lookup that takes 800 ms of a 1 s deadline leaves the connect at most 200 ms; one that outlasts
  the deadline never reaches a connect.
- **An engine smoke bound to a roster entry refuses any edit of it (F15-3).** The bound smoke
  checked only that the position still held the provider and the model it was chosen with, so an
  edit of the endpoint, plan, key variable or `ext` that kept both was smoked on the new entry. A
  target now carries the fingerprint of the whole entry as it was chosen (SHA-256 of its canonical
  JSON: keys sorted, no white space - a reformatted file keeps it) and the smoke is "not run: the
  roster entry changed" when the entry at that position no longer has it. Tests: the unit tests of
  the fingerprint and of `bound_roster` (each field edited, a reformatted file, an empty
  fingerprint) and an end-to-end engine update whose seat's plan is edited between its two smokes:
  the post-update smoke is not run and the update is rolled back.
- **The codex smoke proves the sub-agent switch for each of its turns (F32-2, supersedes F11-8).**
  The check grouped the smoke's consults by thread, counted the terminal records and tested ONE
  rollout-wide evidence flag, so two turns on a shared rollout counted as proven when recognized
  switch evidence existed for the first turn only. The rollout reader now binds the evidence (a
  `multi_agent_version` in a `turn_context`, or a tool list) to the turn it was recorded in - the
  `task_started` / `task_complete` pair around it - and the smoke passes only when every turn has
  its own evidence between its own boundaries; a turn without it, or a rollout whose boundaries do
  not pair up (a terminal record with no turn open, a turn started inside another), is unproven,
  which holds an update like a failure. Tests: the rollout reader's per-turn evidence, and the
  reviewer's shared-rollout fixture - evidence in the first turn only is unproven, in both is a
  pass.
- **`--rev` and `--diff` reach the http reviewer (F19-1, F20-1).** Under `--rev` the http engine
  got no worktree, printed a warning and sent a pack of the LIVE tree - uncommitted files included -
  to a remote model while the operator asked for a frozen revision; with `--diff` its prompt said
  "read the patch" while the pack, the only thing an http reviewer receives, held no patch. Now an
  http run under `--rev` gets the same throwaway worktree of that revision as every other engine
  and its pack is built from it (the revision's files, the brief as staged there, no index - the
  index describes the live tree; no worktree made means nothing is sent, never the live tree). A
  `--diff` patch is a bound part of the pack, in full after the focus files through the pack's
  sanitizer (sidecar role `patch`); a patch that does not fit the pack budget refuses the run before
  any request, naming the sizes. The dry run says the pack is built from the worktree when the run
  starts instead of measuring the live tree. Test: a repository whose revision, HEAD and uncommitted
  file differ - the pack shows the revision's file, never the live one, carries the patch, and is
  refused with a too-small budget or without a worktree.
- **The http engine under `--rev` is proved end to end (F19-1).** The fix above was covered at unit
  level only: the pack builder was shown to read the revision's worktree, but no test drove the
  `c3` binary and looked at what reaches the reviewer's endpoint. The test
  `an_http_review_under_rev_sends_the_revision_not_the_live_tree` does: a scratch repository whose
  committed file differs from the live one, plus an untracked file, is reviewed with `c3 consult
  --engine http --rev HEAD` and with `--diff HEAD~1..HEAD` against a loopback listener of the test
  that records the request. The body holds the revision's content and the patch of the range and
  nothing of the live tree, the request carries no `Authorization` header (the proxy auth mode for
  that host), the pack kept in the handoffs is the request's user message, and the ledger names the
  reviewed revision (`worktree.sha`, `reviewed_revision`, the `diff` record). For that, `base_url`
  (`--base-url` and a roster entry alike) accepts plain `http://` in one case only: a loopback IP
  literal (`127.0.0.0/8`, `[::1]` - the URL parser's host, so `127.0.0.1.example.com` is not one;
  since F57-9 the name `localhost` is not one either) while `CODEX_CONSULT_TEST_MODE=1` is set, a
  narrower form of the rule the telemetry intake follows. It is a
  test harness's own listener on the same machine, not a way to reach any other host over plain
  http: without test mode, or for any other host, the URL is refused as before (`base_url must be
  https://`). Unit test: `s2_plain_http_only_to_a_loopback_host_in_test_mode`.
- **A `--diff` patch never carries a secret off the machine (F19-2, F18-4, F20-6).** A one-revision
  `--diff` put every untracked, not-ignored file into the patch handed to the reviewer, a remote one
  included. Every patch now passes through the redactor packs for remote models use: a
  secret-shaped value is replaced by its `[REDACTED:<kind>]` marker, the rest is kept, the sha256 is
  of the file as written, the ledger's `diff` record gains `redactions` and the prompt says the
  patch is for reading and how many values were redacted. (0.2.5 R8) Untracked files enter the
  one-revision form only with `--diff-untracked` (see Changed); a file a `.eckignore` excludes, or a
  TRACKED file the repository's ignore rules match, never enters a patch of either form. Test: a
  patch with an OpenRouter-shaped key, an untracked note, an `.eckignore`d file and a force-added
  ignored file.
- **One patch file per content (F19-3, F20-3).** The patch was named after the brief only, so two
  runs (or two panel members) with different ranges wrote the same path and a ledger `diff.sha256`
  could describe bytes another run wrote. The file is now `<stem>.<sha8>.patch` ((0.2.5 A4,
  F57-5) `<stem>.<sha256>.patch`: eight hex digits let two different patches meet), written by an
  exclusive create (a new temp file hard-linked to the name, so a sibling never reads half a file):
  the same bytes reuse the existing file, other bytes get their own name, and a file of that name
  holding other bytes - or a link - is never overwritten (the run fails instead). Panel members
  share a stem (a member without a conventional brief names it after the panel's first number).
  Test: two ranges sharing one stem, a sibling's reuse, a foreign file left untouched.
- **Under `--rev` the records name the revision that was read (F18-1, F19-6).** The handoff's
  `Reviewed:` line, the ledger's `base_commit` / `reviewed_revision` and every new finding's
  `source.base_commit` named the live HEAD while the reviewer read an older revision, so later
  reviews judged those findings against code nobody reviewed. They now name the worktree's
  revision (its fingerprint taken right after the staging); the live HEAD at launch is recorded
  beside it as `live_head` (ledger, finding source, `Live HEAD at launch:` in the handoff) with the
  live tree's `live_tree_sha256`. When the reviewed revision is not the live HEAD the task's open
  findings are not offered for re-checking: the prompt (and an http pack) says in one sentence how
  many were recorded against another revision, the reply's prior findings are ignored for the store
  (no `reviewer_checks`, ledger `prior_findings_withheld: n`) and no prior blocker applies (seen
  live 2026-11-22: reviewers of an old revision marked 34 already-fixed findings still-open). Test:
  a kimi `--rev` review of an older commit after a finding was filed on HEAD.
- **A review never takes the live repository's index lock (F18-2, F20-2).** Making a throwaway
  worktree ran `git status --porcelain` on the live repository without `--no-optional-locks`, so a
  stale stat cache made it rewrite the index and hold `index.lock` while a parallel session worked.
  Every git call of the worktree set-up and check (and of the `--diff` patch) now runs with
  `GIT_OPTIONAL_LOCKS=0`, the status call also with `--no-optional-locks`. Test: a stale index
  (a tracked file's mtime moved, content unchanged) keeps its bytes through a worktree's creation,
  staging and checks, and no `index.lock` is left.
- **The bridge's own patch is not a tree change (F18-3).** The `--diff` patch was written into the
  live handoffs after the collab snapshot, so an otherwise clean isolated run was recorded
  `tree_check: warned` and a live-tree agy run without tool calls could be failed for the bridge's
  write. The snapshot is now taken after the patch is written, and the patch's stem (any hash, and
  its write temp) is among the run's own files. Test: a claude framing run and a frozen-HEAD
  diff-review with `--diff` both record `clean`.
- **Staging never reads through a link (F18-5).** A brief or a `handoffs/` directory that is a
  symlink or junction was followed, and its target's bytes - from outside the repository - copied
  into the worktree. A brief that is a link, or reached through one from outside the repository,
  now refuses the run before anything starts (preflight and staging alike, preflight-class wording);
  a `handoffs/` directory or `state.md` that is a link, and every link below `handoffs/`, is
  skipped and counted in a run warning. Test: a junction'd `handoffs/`, a junction below it and a
  brief behind a junction, with an outside sentinel that never reaches the worktree.
- **A link in the reviewed revision cannot lead out of the worktree (F18-6).** A reviewer that
  wrote through a link the revision carries could change its target - the live checkout included -
  and the run stayed usable. Right after the checkout every link in the throwaway worktree whose
  target does not resolve inside it (outside, or dangling) is unlinked, never followed or descended;
  a link that cannot be removed makes the worktree unusable. The prompt names the removed entries
  as data (at most 20, the rest counted) and the handoff's `Isolation:` line counts them. Test: a
  committed link and a planted junction to the live checkout are removed, a link inside stays.
- **The worktree check sees git-ignored paths (F20-4).** The before / after comparison of a
  throwaway worktree used `git status` without `--ignored`, so a kimi or codex reviewer's write
  into an ignored path went unseen while the doc claimed "gitignored or not". The comparison now
  includes every ignored file (`--ignored`, each file listed); the records' fingerprint still leaves
  them out, and the doc says exactly what is covered. Tests: a unit write into `build/` and an
  ignored file; a kimi run writing an ignored file fails.
- **The `--rev` prompt claims only what was staged (F18-7).** It said the task's handoffs were
  copied whenever the collab directory was stageable, even without a `handoffs/` directory; the
  prompt is now completed after the staging and names the handoffs only when files were copied.
  An event stream is recognised by both names the bridge writes - `*.events.jsonl` and a bare
  `events.jsonl` - and neither is staged. Tests: no `handoffs/` means no claim; `events.jsonl` is
  left out.
- **`--rev` and `--diff` values can never become git options (F19-5, second half).** The CLI and
  the MCP tool now check both values with the same functions (one token, no blank or control
  character - a line break included -, not starting with `-`, at most 256 characters), the MCP tool
  before it starts anything; and every git call that receives one passes it after
  `--end-of-options`. Test: `--output=<file>`, `-c` and values with a line break or a control
  character sent through the MCP tool are refused by the tool with the CLI's text, no file is
  created and no consultation starts.
- **The agy denial-retry prompt carries the reviewer role sentence (R1 leftover).** The other
  secondary prompts got the reviewer-is-not-a-coordinator sentence in wave R1; the agy denial retry
  now carries it too (structured and raw), asserted in
  `the_secondary_prompts_carry_the_role_constraint`.
- **Nothing GitHub-controlled reaches the session as text (F16-2, F17-1, F17-10).** The red-CI line
  of `c3 finish` and the SessionStart hook carried the failed run's `displayTitle` after only
  whitespace collapsing: an escape sequence, a bidi override or a sentence addressed to the agent in
  a commit title reached every session start. The line is now C3's own words, the branch C3
  resolved (a ref name without control, format or white-space characters, else no line) and the
  run's id (digits only, else no line): `CI: the latest completed run on <branch> failed (run
  <id>) - gh run view <id>`; the title is no longer even asked for. A pull request's run (`event`
  `pull_request` / `pull_request_target` - a fork's branch can carry the default branch's name) is
  not the default branch's run and decides nothing. `c3 finish --json` carries `"ci": {branch,
  run_id, line}`, no title. The sentence says "latest completed run", which is what is chosen.
- **One deadline covers everything the CI check starts (F16-3).** `c3 finish` ran the check's git
  calls with no deadline before `gh`'s 10 s, and the hook's 5 s wrapper abandoned its thread without
  killing a `gh` it had started. Every git and `gh` process of the check now runs under one
  deadline - 10 s in `c3 finish`, 3 s in the hook - through the mechanism the hook's branch line
  already used (wave R2's `run_registered` / `ScanChildren`, moved to `crates/c3/src/bounded.rs`
  and shared); when it fires each running child is killed together with its descendants (a `.cmd`
  shim, the git a `gh` runs) and reaped, and no further one starts, so nothing the check started
  outlives `c3 finish` or `c3 hook`.
- **The CI check asks about the right branch and the right run (F16-4, F17-7).** When `origin/HEAD`
  names a branch with no local counterpart (`develop` only as `origin/develop`, beside a local
  `main`), the check asked gh about the local fallback; it now asks about the remote's default
  (`c3.defaultBranch` still wins; with no default at all it stays silent). The runs are ordered by
  `createdAt` read as a time, not as text (mixed offsets sorted an older run first), a timestamp that
  does not parse silences the check, and gh is asked for 20 runs instead of 5.
- **A GitHub remote is recognised by its host (F17-8).** Any `git remote -v` line containing
  `github.com/` counted, so `https://gitlab.example/github.com/mirror` started gh and spent the
  day's hook attempt. The remote URL is now parsed (https, ssh, scp-like `git@host:path`) and its
  host compared with `github.com`.
- **One daily-gate file per repository (F16-9).** The hook's "asked today" record was one shared
  `ci-check.json`, read, changed and renamed back by every session: two sessions starting together
  dropped each other's attempt, and a repository was asked again at once. Each repository now has
  its own small file, `<codex home>/c3/ci-check-<16 hex of sha256(canonical path)>.json`, written
  through a temp file and a rename - no read-modify-write of a shared file. A shared `ci-check.json`
  left by an older build is ignored and removed; gate files older than 30 days are removed.
- **`c3 router explain` and `c3 router replay` show what the panel does (F16-6, F17-2).** Explain
  picked each lab's seat among the roster's entries with its own on-request / held / weighty checks,
  so an unavailable entry (a missing key, a refused engine) was marked as its lab's pick and a
  required or upper-tier sibling's seat was ignored. It now builds the panel's own member election
  (`panel_members`: availability, refusals, the weighty and light gates, the reserve hold) and the
  roster's required entries, runs the panel's own preparation, and names the seat holder from the
  draw's own pools (`routing::lab_stands`, shared with the draw): `[not eligible: <reason>]`, `[lab
  zhipu: #1 takes its seat (required) - ...]`, `[... (upper tier) ...]`, or both candidates when
  the routed upper-tier draw decides. Replay rebuilt a routed draw from scores and multipliers
  rounded to 4 decimals, so a near tie could replay to another winner: the record now also carries
  the draw's unrounded values when rounding changed them (`ext.c3.draw = [{position, score,
  mult}]`; the rounded `score` / `budget_mult` stay for display, and a record rounding did not change
  stays the plugin's), the replay weighs those, and a routed record without them (an older
  build's) replays as before with a note that its values are rounded.
- **Every maker of the roster has a lab (F16-7, F17-4).** MiniMax and Meta's Llama ids had no vendor
  prefix, so an `api.minimax.io` endpoint entry (lab `minimax` by its host) and a `minimax-m3` codex
  entry, or two `meta-llama/...` ids, were seated side by side. `minimax`, `meta-llama` and `llama`
  joined the prefix table, which now names the same lab as `lab_of_host` for every host that function
  knows (tested); a prefix matches only at a boundary (the id ends, or `-`, `_`, `.`, `/`, `:` or a
  digit follows), the claude aliases `opus` / `sonnet` / `haiku` / `fable` only as the whole id (so
  `opus-custom` is not Anthropic's - only a `claude-` id is), and a declared `lab` is case-folded
  (`OpenAI` and `openai` are one lab).
- **A missed upper-tier guarantee is said out loud (F16-8, F17-3).** On a weighty purpose, when every
  eligible `panel: weighty` entry shared a lab with a required (non-weighty) entry, the panel ran
  without a weighty member and said nothing. The plan now warns `upper tier missed: purpose <p> owes
  a seat to a weighty entry, but every eligible one shares a lab with a seat already taken (lab <l>
  is held by #<n>, keeping out #<m> <lineage>) - one seat per lab`. The one-lab rule is not relaxed.
- **The hook script's cut and catch path (F16-10, F17-9).** `c3-hook.ps1` cut a long line at 400
  UTF-16 code units and could split an emoji's surrogate pair, printing half a character; it now
  cuts before such a pair (forwarded lines, the no-prefix fallback and the error line). Its `catch`
  path printed only the error line; it now prints the waiting rule after it, exactly as the normal
  path does (the `.sh` always did). The claim that the two scripts print byte-identical text for
  every input is withdrawn: identical for ASCII lines, a longer non-ASCII line may be cut at a
  different point (the `.sh` counts bytes under `mawk`).
- **The delegation rule's check, said as it is (F17-6).** The CHANGELOG and README promised that a
  delegation rule "the user rewrote" is never edited, but `c3 doctor` recognises a C3-written rule by
  its title line alone - a file that keeps that line around a body of the user's own counts as
  C3's. They now say exactly that, and that `c3 doctor --fix` only ever appends the missing
  "BRANCHES AND WORKTREES" section and changes no existing byte (tested on such a file).

- **`c3 doctor --fix` never removes through a link and never removes a worktree a live review may
  own (F13-1, F13-8, F12-1).** Staleness read the throwaway path with `std::fs::metadata`, which
  follows a link: a junction or symlink named `c3-kimi-wt-<32 hex>` in the temp directory, pointing
  at an old directory without an owner record, was classified stale and handed to the removal (the
  removal itself has refused a linked root since wave P4, so the target survived - by the second
  line of defence only). The path is now read itself (`symlink_metadata`): a link - on Windows any
  reparse point - is reported "a link, left alone" and never handed to the removal. The owner record
  is no longer best effort: when it cannot be written, making the reviewer's throwaway worktree
  fails, the worktree made so far is removed link-safely, and the run records the same capability
  failure as a worktree that could not be made. The 6-hour rule is gone: a throwaway worktree
  without an owner record is removed only when git itself calls it prunable (its directory is
  gone), else reported "no owner record - remove it by hand" whatever its age. An owner's liveness
  has three answers now - alive (no finding), gone (removable) and cannot tell: a process holds the
  pid but its start time cannot be read (another user's or an elevated process), which is reported
  and left alone, never removed. `c3 branches --json` / `c3 doctor --json` carry `removable` per
  reported worktree; doctor prints a left-alone one as `[!] c3 worktree <path>: <why> (c3 doctor
  --fix does not remove it)`, still a finding. Tests: the reviewer's scenario (a registered
  worktree whose path is a junction - a symlink elsewhere - to an outside sentinel, with a gone
  owner: reported as a link, `doctor --fix` attempts nothing, the target and the link survive), no
  owner record until the directory goes, an owner whose start time is unreadable (test hook), an
  injected owner-record failure.
- **A stray `.git` entry is no longer taken for a repository (F13-2).** The SessionStart repair
  found the root by walking up to the nearest `.git` entry of any kind, so an empty file, a broken
  link or a non-repository directory named `.git` let the hook rewrite agent files and `AGENTS.md`
  in a directory that is no repository. The nearest `.git` entry now counts only when it is a real
  directory holding `HEAD`, or a regular file whose first line is `gitdir: <path>` naming a
  directory that holds `HEAD`; anything else means no repair, silently (the walk does not go on past
  it). Still no process is started. With `GIT_DIR` set in the environment the hook's repair does
  nothing (the walk cannot know which repository git would take).
- **A branch record needs a real date (F13-3).** `c3: work | alice | tomorrow | keep me` parsed as
  C3's record, so `--set` replaced that note and `--clear` deleted it. A line is C3's record only
  when its date field is `YYYY-MM-DD` (four, two and two digits, month 01-12, day 01-31); anything
  else is a foreign description, kept byte for byte by `--set` and left alone by `--clear`.
- **`C3_CHILD=1` on the children of `c3 update`, and no branch edits from a child (F13-4).** `c3
  update --apply` started `claude plugin update c3@c3` (and `cargo install`, and its git calls) with
  the inherited environment, so the SessionStart hook of that Claude Code could repair the
  repository it ran in. Every child `c3 update` starts is now marked. Inside a C3 child `c3 branches
  --set`, `--clear` and `--default` refuse - exit 1, one line saying why - and write nothing; the
  listing stays available.
- **The hook's branch-line deadline stops its git children (F13-5).** Past the 2-second deadline the
  hook stopped waiting but left the scan's git running, and it outlived `c3 hook`. Every git call of
  the branch line - the root's `rev-parse` included - now goes through one runner that registers
  the running child; when the deadline fires, the registered children are killed and reaped and any
  later start of that scan is refused, before the hook goes on. Other callers of the scan are
  unchanged.
- **`AGENTS.md` never follows to an older model (F13-6).** The repair took the roster's
  numerically higher id for the successor: a sole `grok-4.20` entry rewrote `grok-4.7`, although
  4.20 is the vendor's 2025 model. When the maintain state dates both ids (a public catalog's
  `created`) and the candidate is not newer, or an earlier pass recorded the pair as older, that
  candidate is not followed (the swap gate's comparison). Without dates for both ids the behaviour
  stays as it was: the version rule alone decides.
- **A failed `git cherry` is "not checked" (F13-7).** A patch check whose `git cherry` or `git
  rev-list --count` failed still set `patches_checked`, so the listing dropped "patches not
  checked" and the failure looked like a finished check. The branch now counts as checked only when
  both calls succeeded; a failed check stays unchecked and is counted in `patches_not_checked` (the
  listing's line now says it counts failed checks as well as the cap of 50).
- **Four sentences now say what is true (F12-2, F12-3, F12-4, F13-9).** `providers::
  resolve_repo_root`'s `git rev-parse --show-toplevel` runs with `GIT_OPTIONAL_LOCKS=0` too, so
  "every git call of the listing, doctor, the hook and the finish note runs with
  `GIT_OPTIONAL_LOCKS=0`" holds literally. The repair's refusal names a Windows reparse point that
  is no symlink or junction (a cloud-sync placeholder) "a reparse point" instead of "a link". The
  sentences that said a concurrent edit is "never written over" (`c3 branches --set` / `--clear`,
  the repair's rename) now say what is checked: the description or file is read again immediately
  before the write, and the write is skipped when it changed. For `c3 branches` the moment between
  that re-read and the write itself remains - a decided boundary (closing it needs a local process
  with write access to the repository, which can edit the files directly). For the repair (and the
  roster swap of `c3 maintain`) that boundary is withdrawn: an edit saved in that moment is kept
  (F02-2, F57-10 below).
- **Brief staging never writes outside the reviewer's throwaway worktree (F11-3).** The brief and
  the task's handoffs are copied into the worktree at their repo-relative paths, and the checked-out
  revision can carry a link there. A staging destination is now checked twice: in the revision's own
  tree (no link, submodule or file on the way, no directory or link at the path - whatever the
  platform's checkout makes of a link) and on disk (no component may be a symlink, junction or other
  reparse point); the bytes go into a new file created exclusively beside the destination, which
  replaces it by a rename only when it is absent or a regular file, so the destination is never
  opened for writing. Any doubt refuses the run before the reviewer starts; a committed link at the
  brief's path pointing outside leaves its target's bytes untouched.
- **The worktree seat's availability check and re-seat (F11-4, F10-3).** An in-repo brief path with
  `..` (`docs/sub/../brief.md`) is normalised to the same file instead of being refused as leaving
  the repository; a brief committed at HEAD that the live tree cannot read is materialised from
  HEAD (also into the worktree of an older `--rev`) instead of failing; and a staging failure is a
  typed pre-start refusal with the preflight wording - the reservation is withdrawn, a panel member
  leaves its reserved record untouched, so the panel re-seats an optional seat from its reserve
  exactly as for a preflight refusal - not a provider capability failure. A stageable brief is
  always staged or the run refused, so the prompt says "copied in" only when the copy was made.
- **The agy write classifier neither passes a real write nor fails a pure read (F11-1, F10-1,
  F10-2).** All events of one tool step (ACTIVE / DONE / ERROR, also when one of them carries no
  name) are merged, so a name that arrives late is not lost; a tool not on the reader allow-list
  counts as writing, and agy's Cascade-vocabulary readers (`list_dir`, `grep_search`,
  `find_by_name`, ...) joined the allow-list, so a pure read with them no longer fails a complete
  reply. The hand-over exemption needs a verified `finish` call (named `finish`, closed by a finish
  event), never a step index alone. (0.2.5 R8) The parameter scanning this wave added is gone: a
  step is judged by its tool's name, the classification is recorded, and agy's throwaway worktree
  decides the run (see Changed).
- **An agy reply whose brief or artifact changed during the run is not recorded as usable
  (F11-2).** A change of a BOUND input while agy read it is classified apart from ambient drift: the
  outcome fails (class `permission`, as a failed read-only check) with a message naming the changed
  input, and the raw reply is kept. A change of the tree or the collab directory stays a warning.
- **The isolation paragraph no longer leaks file names (F11-5).** The prompt of a reviewer in a
  throwaway worktree of HEAD named the first twenty paths of `git status` verbatim, untracked files
  included. It now names only the tracked paths of HEAD that differ in the live checkout (names the
  reviewer can see in the revision anyway; a rename by its original path), each as data - a name
  with a backtick, a control character or a line break is counted, not shown - and gives the paths
  HEAD does not hold (untracked or newly added) as a count only.
- **A panel member is judged against the findings it was shown (F11-6, F10-4).** The panel freezes
  an immutable record (id, status, severity, the text shown) of every listed finding into each
  member's spec at panel start; the member's prompt, its prior-blocker semantics and the validation
  of its prior report use that record, so a sibling (or another process) changing a listed finding
  mid-panel can no longer erase or create a blocker the member never saw in that form, and an id
  that was not in the member's list is not accepted as a prior report. Its checks still merge into
  the current store under the task lock. The spec travels to the member in a file in the panel's own
  temp directory (`--panel-spec-file`), not on the command line: with the findings' text in it, a
  task with about a hundred open findings could not start a single member (Windows: "os error 206",
  the command line limit of 32 767 characters).
- **The smoke counts as a proof only when it is one (F11-7, F10-5).** A codex smoke whose consult
  recorded no thread id, a turn missing from the ledger, a seat that opts into sub-agents, and a
  rollout that cannot be found, read or identified are now "unproven", not a pass: an automatic
  update holds on it exactly like a failed smoke (rolled back where supported, the pending record
  kept unless the previous version is verified back), a recovery smoke never records it as
  verified, a model swap is not seated, and `c3 maintain --smoke` reports `unproven`. A multi-agent
  tool counts by its name in any namespace, an MCP server's (`mcp__*`) included.
- **The smoke's rollout evidence is checked (F11-8).** The Codex home is resolved to an absolute
  path once and handed to the consult children, so a relative `CODEX_HOME` no longer names another
  directory from the scratch repo; a rollout's `session_meta` id must be the thread it was looked up
  for; a tool list counts only once its `tools` array is there; a rollout without a `task_complete`
  for its turn (one per consult on a shared thread), or with an unreadable record, is unproven; a
  rollout that cannot be found or read is looked for again a few times before it is.
- **The sub-agent guard sees a turn that exits between two polls (F11-9).** After the child exits
  on its own, the guard inspects the rollout once more before the turn is accepted as normal, so a
  spawn call made after the last poll still stops it. A child rollout is attributed to the current
  turn by its own creation (its head's timestamp at or after the launch), so an old child of a
  resumed thread written to within the five-second slack no longer stops it; a head without a
  timestamp still falls back to that slack.
- **The secondary prompts carry the reviewer instruction (F11-10).** The timeout continuation, the
  format repair and the same-thread read-insist re-asks now carry the same "you are a reviewer, not
  a coordinator" sentence as every review prompt (an instruction to the reviewer, not an enforced
  capability limit).
- **A death record names the run's own stream (F11-11, F10-7).** Every turn transition (main,
  continuation, read-insist, format repair, denial retry) now names its stream in the run's own
  recovery record - a panel member's, keyed by its seat - and, for a detached single run, in its
  status file; the continuation's running record names the continuation's stream. A death record
  reads only evidence of its own run (a sibling's pending record is never taken for it), names the
  turn that ended the run, and a reader of a run whose bridge was killed falls back to the persisted
  stream, then to the newest events file of the run's handoffs.
- **`c3 providers` counts failed runs by the reply they kept (F11-12).** The breakdown counted a
  failed run as "with a reply" only when it named a `.reply.json` sidecar, so a run that kept only a
  Markdown partial read as "no reply". A failed run's ledger entry now records the kind it kept
  (`reply_kept`: `sidecar` / `partial` / `none`), the count goes by that (an older entry by its
  `reply_json` / `partial_reply`), and the label and the JSON keys (`failed_reply_json`,
  `failed_partial`, `failed_no_reply`; `failed_with_reply` is the sum of the first two) say what is
  counted.
- **A reviewer in a throwaway worktree is no longer failed by a change of the live tree.** The kimi
  engine ran in its own worktree, yet the read-only check compared the MAIN tree and failed the run on
  any change there - a parallel session's edit, not the reviewer's (the false negative the agy fix
  removed). For every reviewer in a throwaway worktree (kimi, `--rev`, a claude diff-review /
  acceptance) the check now watches that worktree - its tracked and untracked files and the staged
  collab copy - for the reviewer's own writes and decides on those (kimi still fails a run that
  writes there; it never saw such a write before), while a change of the live tree, its collab
  directory, the brief or an artifact is a warning only; a live-tree change no longer skips the
  timeout continuation either.
- **Every model family of the roster resolves to its lab.** The vendor prefix table missed `grok`
  (now xai) and the OpenAI `o<digit>` ids (`o3`, `o4-mini`: openai), so a Grok entry drew the "no lab
  known" warning and counted as a lab of its own; an OpenRouter-style `vendor/model` id that does not
  resolve whole is now tried by its last segment (`x-ai/grok-5` xai, `google/gemini-4` google).
- **Telemetry never sends a private model name (F02-19, F02-20, F04-8, F03-5).** Any configured
  catalog or page made the ids it listed "published", in one global set: a keyless internal catalog
  listing a private endpoint's model would have let telemetry send that name, even for a reviewer
  of an unknown vendor; and the `anthropic` class sent every id of the Claude shape. Now only the
  built-in catalog (openrouter.ai) and a configured catalog or page declared `"public": true`
  (`"model_catalogs"` takes `{"url": ..., "public": true}` beside a bare URL; a page takes
  `"public"`) confer published status, decided at send time from the code and the current config,
  and a published id counts only for the vendor class its source attributes it to (OpenRouter's
  `<vendor>/` slug, else the source host's class) - never for another class or a reviewer without
  one. A Claude-shaped id goes out only from `CLAUDE_MODELS` or that set. The `[1m]` suffix is
  stripped in any case (`opus[1M]` is `opus`), as the roster rule does. End-to-end tests through
  `c3 telemetry --backfill-ratings` cover the built-in catalog, an undeclared and a declared
  catalog, a page, an endpoint list, an unpublished Claude id and no maintain state.
- **The kimi throwaway worktree removal never reaches outside it (F02-21, F03-2, F04-5).** The
  unlink pass before `git worktree remove --force` treated only what Rust reports as a symlink as a
  link, swallowed every error, reopened directories by path, and the forced removal and
  `remove_dir_all` ran whatever it found. Now every reparse point counts as a link
  (`FILE_ATTRIBUTE_REPARSE_POINT`, as `panel/roles.rs`; a symlink elsewhere) and is never descended
  into; each directory is re-checked before it is read and before its entries are acted on to be a
  real directory whose canonical path lies inside the root, else the pass aborts; every error is
  collected. If any link could not be removed, any error occurred, or the root itself is a link,
  nothing destructive runs (no `worktree remove`, no prune, no `remove_dir_all`): the directory is
  left, one line says why, and `c3 doctor --fix` - which now reports such a refusal as not removed
  - retries it later. The directory fallback after git runs only after the same checks pass again.
  Tests: a junction to an outside sentinel on a real worktree and through the real `c3` binary with
  a fake kimi, an injected unlink failure (a git recorder shows `worktree remove` was not called),
  and a root replaced by a junction.
- **`c3 branches --set` / `--clear` keep a foreign description byte for byte (F02-7, F03-3,
  F04-9).** The description is now read from the repository's own config with NUL framing
  (`git config --local -z --get-all`) and never trimmed, so trailing line breaks, CRLF and a
  whitespace-only note survive a set and a clear exactly. A first line is C3's only when it parses as
  a full record; a note that merely starts with `c3: ` is foreign and `--clear` leaves it alone. A
  value C3 cannot read exactly (git fails, not UTF-8, several values under the key) aborts the
  command instead of being written over, and the value is read again immediately before the write:
  when another writer changed it meanwhile, nothing is written (a writer landing between that
  re-read and git's own write is not seen). A name git would not accept as a branch
  name is refused; a valid one with dots, slashes or non-ASCII characters (`foo.bar`, `feat/x-y`)
  addresses its own `branch.<name>.description` key, which the new tests prove.
- **The branch read paths no longer refresh or lock the index (F02-8, F04-1, F03-6).** `c3 branches`
  and `c3 doctor` ran `git status` in the start folder, and git rewrites the index (taking
  `index.lock`) when its stat cache is stale - a "read-only" check could collide with a commit in
  another session. Every git call of the listing, doctor, the SessionStart hook and the `c3 finish`
  note now runs with `GIT_OPTIONAL_LOCKS=0`; a test proves `.git/index` stays byte-identical on a
  stale stat cache. The per-branch patch check stays but is capped: the full listing runs
  `git cherry` and `git rev-list --count` (two git processes, and `git cherry` reads every commit's
  diff since the merge base) for the first 50 unmerged branches - those without a recorded state
  first - and lists the rest as "patches not checked" with a count.
- **The default branch no longer follows the checkout C3 runs in (F02-9, F04-7).** It fell back to
  the invoking branch, so from a topic checkout in a repository without `origin/HEAD`, `main` or
  `master` that topic became the "default" and the work on it was hidden. The order is now
  `c3.defaultBranch` in the local config (set with the new `c3 branches --default <name>`; it wins
  over everything), `origin/HEAD`, `init.defaultBranch`, `main`, `master` - each only when that
  local branch exists - and otherwise "default branch unknown; set it with `c3 branches --default
  <name>`", reported by the listing, `c3 doctor` (a finding), the hook line and the finish note. A
  failed ancestry query now leaves every branch unknown instead of merged.
- **A user's worktree named like kimi's is no longer hidden, and a crashed kimi run's worktree is
  found (F02-11, F04-7).** The branch checks skipped every worktree whose folder name started with
  `c3-kimi-wt-`, wherever it was; one left by a crashed run was reported by nothing. A worktree is
  now C3's throwaway only when it lies directly in the system temp directory and is named exactly
  `c3-kimi-wt-<32 hex digits>`. The kimi engine records the owning process (pid and start time) in
  the worktree's git admin directory, outside the reviewer's files; a throwaway whose owner is gone,
  or whose folder is gone (git calls it prunable), is listed as a "stale c3 worktree" by `c3
  branches` and `c3 doctor` (a finding), and `c3 doctor --fix` removes it with the kimi engine's own
  link-safe removal. A live run's worktree is still never a finding; one whose path is a link, one
  without an owner record and one whose owner cannot be checked are reported and left alone (see
  F13-1 above).

- **`c3 maintain` no longer offers an older catalog model as a successor.** The version rule alone
  took the vendor's 2025 `grok-4.20` for a successor of `grok-4.7` (4.20 > 4.7), and a plan that
  still serves it would have passed the smoke and been downgraded. A public candidate now needs
  date evidence: created after the seated id when the catalog lists both, else created within the
  last 180 days; an undated catalog entry or a page id is announced only, never smoked. The
  entry's own model list needs no date (but see the date gate below). The catalog dates are kept in
  the maintain state (an older state loads without them); a rejected id is named with its reason in
  the full report only; "new in the family" skips an id older than the seated one by date and names
  each id once per provider.
- **`c3 maintain`: the successor date gate lets negative evidence win (F02-14, F04-2, F04-3).** A
  catalog id dated more than a day in the future is refused; if ANY public source dates a candidate
  not after the seated id it lists too, the candidate is refused whatever another source says (a
  second catalog's recent, unanchored copy no longer outvotes it); such a pair is recorded in the
  maintain state (`models.older`) and stays refused in every later pass while its seated model is
  in the roster, so a catalog that re-stamps an old model later cannot bring it back (at most 500
  pairs are kept, but the cap never evicts a pair whose seated id is still an active roster model:
  it applies to the other pairs, oldest first, and when the protected pairs alone exceed it they are
  all kept). An id from the entry's own list (`grok
  models`, `agy models`, an endpoint's list) still needs no date, but goes through the same refusals
  when a public source dates both ids - `grok-4.20` beside `grok-4.7` in `grok models` is no longer
  smoked when the catalog dates it older. Only ids the version rule takes for a successor are gated.
- **`c3 maintain`: public catalog and page fetches have their own, tighter policy (F02-12, F02-13,
  F04-8).** They shared the kimi installer's redirect rule, so a catalog on `code.kimi.ai` could be
  sent on to `cdn.kimi.ai`, a redirect could change the port or add userinfo, every hop restarted the
  15 s clock, and the 4 MiB cap was checked only after up to 8 MiB had been read. A public GET now
  follows at most 3 redirects, each to the start's own host and port with no userinfo (re-checked at
  every hop; the start URL too), under ONE absolute 15 s deadline that also bounds the body read,
  and the reader stops at 4 MiB + 1 bytes. The installer keeps its `code.kimi.ai` -> `cdn.kimi.ai`
  pair (now also port-pinned and userinfo-free); a key-carrying endpoint request still follows no
  redirect. The User-Agent stays `c3/<version>`.
- **`c3 maintain`: an endpoint's model list is keyed by the endpoint, its key variable and its plan
  (F02-15).** The list was recorded as `endpoint:<host>` and the first response won, so two entries
  on one host with another path or another key shared it, and a model private to one could be offered
  to - and smoked on - the other without any date check. The source is now `endpoint:<scheme>://
  <host>:<port><path>|<key variable>|<plan>` and its ids are candidates only for entries with that
  key. The first pass after the change reads the old host-only record, so the list is not announced
  again as new.
- **`c3 maintain`: the smoke and the swap bind to the exact roster entry (F02-16).** The swap smoke
  ran `c3 consult --provider <label> --model <candidate>`, which resolved the label again: with the
  candidate model in no entry, the label's FIRST entry was used - possibly an on-request or protected
  one, with another endpoint, auth, plan and quota policy. Both smoke turns now run against a
  one-entry roster (`CODEX_CONSULT_ROSTER`, written beside the scratch repository and removed with
  it) holding exactly the entry at the candidate's position, the operator's own JSON for it with only
  the model overridden; the position must still be that provider and old model, and a protected,
  on-request or inactive entry is refused even there. The engine-update smokes bind to their roster
  seat the same way; a codex-config custom provider's smoke runs with no roster; an explicit
  `C3_MAINTAIN_SMOKE_<ENGINE>` is still resolved by its label. A failed turn is now reported with the
  consult's `codex-consult:` outcome line rather than its first stderr line. An `ext.c3.reviewers`
  http entry is announced only. Tested end to end with two same-label stub endpoints, the on-request
  one first: both turns reach the candidate's endpoint, the other none.
- **`c3 maintain`: the roster writer edits the parsed entry and verifies the result (F02-17).** It
  searched the text for `"model": "<old>"` and accepted a sole match anywhere, so with a JSON-escaped
  model in the entry it could edit an `ext` object's plain `"model"` instead while reporting a seat.
  It now locates the string value of `reviewers[<position>].model` by parsing (a repeated key on the
  path is refused), checks it still decodes to the old model, replaces the whole literal, and
  verifies that the re-parsed roster equals the old one except that one field.
- **`c3 maintain`: a public catalog or page is never read from a local or private address
  (F15-1).** The URL checks constrained the URL text only: an IP literal or `localhost` was
  accepted, and a public host name that resolved - or was rebound - to `127.0.0.1` or a private
  network received the keyless GET, on every redirect hop. A catalog or page URL whose host is an IP
  literal (any form the URL parser reads as one) or `localhost` (any case, trailing dot included)
  is now refused when the C3 config is loaded and again before the fetch. The name lookup of a
  public GET refuses a name of which any address is loopback, private (RFC 1918, unique local
  `fc00::/7`), link-local, unspecified or multicast, so no connection is made to it; and after the
  connection, an answer whose peer address (ureq's `remote_addr`) is in those classes - or unknown -
  is discarded before its body is read. The same checks run at every redirect hop. An endpoint's
  keyed model list and the kimi installer are not public sources and keep their own rules.
- **`c3 maintain`: one absolute deadline really bounds the whole public fetch (F14-1).** The 15 s
  deadline was checked only between redirect hops: every hop's connect phase got the full 15 s again
  (ureq's connect timeout outranks the request's own), and the body was read in one call with no
  deadline check, so a trickling body or a stalled later hop ran past it. Each hop's client now gets
  only the time that remains for its connect, the name lookup runs on its own thread under the same
  deadline (ureq cannot interrupt it), and the body is read in 8 KiB chunks with the deadline checked
  between them; a stalled connect, lookup or trickling body ends with the deadline's error at 15 s
  overall. The same reader and per-hop client serve the installer and endpoint GETs under their own
  deadlines.
- **`c3 maintain`: a recorded "older" refusal is not forgotten while its seated model is in the
  roster (F15-2, F14-2).** The refusals were a 500-pair FIFO: after 500 newer pairs the oldest was
  evicted, and a catalog that then re-dated that candidate newer could make it a swap candidate
  again. Eviction at the cap now never drops a pair whose seated id is still an active roster model;
  the cap applies to the other pairs, oldest first, and when the protected pairs alone exceed it,
  they are all kept.
- **`c3 maintain`: an engine smoke runs only on the roster entry it was chosen for (F15-3).** The
  entry-bound smoke checked only that the position still named the provider: an entry edited in the
  meantime to another model, endpoint or plan of the same provider was smoked - with the stale model
  forced onto the new entry's endpoint and quota. The target now carries the model the entry held
  when it was chosen (the old id for a model swap), and the smoke is "not run: the roster entry
  changed" - a failure, so the update or the swap is held - when the position no longer holds that
  provider AND that model. The check runs before anything is created, in test mode too. Tested end
  to end: a fake update check edits the seat's model during the pass, the update is held and
  nothing is installed; without the edit the same pass updates.
- **`c3 maintain`: the smoke's one-entry roster is created safely (F14-3).** It was written with a
  plain `fs::write` to a predictable sibling of the scratch repository in the shared temp directory,
  so a link planted there redirected the write. The smoke now creates its own scratch directory with
  an exclusive create (a path that already exists fails the smoke closed and is left as it is),
  puts the scratch repository in `repo/` inside it and the one-entry roster beside it, created with
  `create_new`, and removes the directory when the smoke ends; nothing is written at a sibling path.
- **`c3 maintain`: the entry-bound smoke reads the protection fields as the router does (F14-4).**
  `bound_roster` read `ext.reserve_percent` with `as_i64` (a float or string counted as 0),
  `ext.on_request` as a literal boolean and the inactive list by its own exact match, so an entry
  the router treats differently could be smoked. It now loads the roster with the router's own
  loader (`validate_roster`) and uses that entry's `reserve_percent`, `on_request` and `inactive`
  marks; a roster the loader refuses (a non-integer or out-of-range `reserve_percent`, a non-boolean
  `on_request`, a malformed `ext.c3.inactive`) refuses the smoke too, with the loader's reason.
- **`c3 maintain`: a candidate dated in the future is not evidence (F14-5).** Only a date more than
  a day ahead was refused; a catalog stamp up to 24 h in the future still qualified a candidate,
  anchored or not. A `created` stamp later than now plus ten minutes now never qualifies a
  candidate in any branch of the date gate - it is announced only ("dated in the future by the
  catalog - announce only"), never smoked and never recorded as older; beyond a day it stays
  negative evidence that refuses the candidate whatever another source says.
- **`c3 maintain`: pending recovery never installs an older release than the one installed
  (F15-5).** The "newer release" step compared the vendor's latest with the version read BEFORE the
  recovery smoke and rollback: a failed rollback (or a manual update meanwhile) that left 1.0.90 on
  disk after a failing 1.0.80 still let C3 install the vendor's 1.0.82. The installed version is now
  read again after the smoke and any rollback, and a release that is not newer than what is
  installed NOW is not installed (`<engine> 1.0.82 is not tried: 1.0.90 is installed now`); the
  pending record stays unresolved. Tested end to end with a fake rollback that leaves 1.0.90.
- **`C3_MAINTAIN_MODELS=off` really writes nothing for `c3 maintain --models` (F15-6).** The switch
  was read after the pass lock was taken, so the maintain state directory and a lock file were
  created - and a stale lock could be removed - before the "the model step is off" line. The switch
  is now read before the lock and before the state directory: an explicit `--models` with the switch
  off prints its one line and touches no file.
- **Telemetry trust is per catalog URL, not per host (F16-1, F17-5).** The maintain state keyed a
  catalog by `web:<host>`, the fetch kept only the first configured URL of a host, and a host was
  trusted when ANY of its URLs was declared `"public": true` (the built-in catalog's host always) -
  so a non-public catalog listed first on that host supplied the ids telemetry then sent by name. A
  catalog's identity is now its full URL (scheme, host, port, path, query - `web:https://openrouter.ai:443/api/v1/models`):
  every configured URL is fetched and recorded under its own key, and an id counts as published
  only when the URL it was read from is the built-in catalog or is itself declared public. A state
  file of an older build (host keys) is read without error; its host-keyed ids are not trusted until
  a fetch of a catalog on that host rewrites the record. Wherever a catalog is NAMED in output (the
  pass's "model id(s) collected" line, which the detached pass also writes to `last-run.log`) it is
  shown without its query - which may hold a token - as scheme, host, the port only when it is not
  443, and the path, with ` (query not shown)` appended when a query was dropped; the stored key
  keeps the full URL. Tested end to end through `c3 maintain
  --models` and `c3 telemetry --backfill-ratings`: two catalogs on one host, the private one first -
  its id goes out as `other`, the declared one's by name.

- **The `kimi` engine's throwaway worktree removal could follow a link.** The reviewer can run
  commands in that worktree; a junction or symlink it left there (e.g. a `node_modules` junction into
  the main checkout) could be followed by `git worktree remove --force`. Every symlink and junction
  inside the worktree is now unlinked first, without descending into it, and only then is the
  worktree removed.
- **`c3 init` templates pinned old models.** `opus-worker.md` / `sonnet-worker.md` pinned
  `claude-opus-4-8` / `claude-sonnet-5`; they now write the floating aliases `opus` / `sonnet`
  (and the worker prose no longer names a model version), so a freshly-initialised worker follows
  the newest model.
- **`eck_finish_task` in the worker templates** is now `c3_finish_task` (the tool a C3 workspace
  actually exposes).
- **`claude-fable-5-2` (the current Fable) was missing from the closed Claude model table**, so a
  roster entry naming it was refused. It is added to the table in `c3-core` and the telemetry class
  list, and to the model lists in `README.md` and the setup-providers skill.
- **agy's (Gemini's) reply is no longer thrown away by the tree check.** Every agy panel seat of
  2026-11-07 was marked FAILED although its reply was complete: agy's `finish` call - its normal
  reply hand-over - was taken for a writing tool, and files that workers and the coordinator edited
  meanwhile failed the post-check. The `finish` hand-over is not a writing call. (0.2.5 R8) agy now
  reviews a throwaway worktree of HEAD: a change of the live tree is only a warning, a change of
  its worktree fails the run, and a writing tool in its log beside an unchanged worktree is a
  warning (see Changed). `c3 providers` now prints, per reviewer over
  its 14-day ledger window, `usable / failed with a reply / failed without a reply`, and `--json`
  carries it as `reply_outcomes` (per row, and per entry with `--short`).
- **A kimi seat no longer answers without its brief, and a panel member no longer sees a sibling's
  finding as "prior".** kimi reviews a throwaway worktree of HEAD; since 0.2.4 the brief is copied
  in at its repo-relative path. A brief that is neither committed at HEAD nor copyable (a relative
  path leaving the repository) now refuses the seat before the child starts, worded as a
  preflight-class refusal so a panel re-seats it. The kimi prompt names the paths that were
  uncommitted in the live tree at launch (at most 20, then a count), not only the handoff header.
  A panel member's prior-blocker lines, ACCEPT checks and `unchecked_prior_blockers` are judged
  against the pre-panel snapshot it was shown, not the findings store at render time - a sibling's
  new blocker no longer shows up as `(prior, not-checked)` in members that were already running.
  A reply that only says it cannot read the brief is NOT reclassified: kimi's stream carries no
  error flag on a tool result, so no robust signal exists.
- **A detached run that died in a read-insist re-ask gets a complete death record.** The death
  record's `events_file` / `last_event` / `events_mtime` / `partial_hint` were read only from
  `.consult.pending*` records, which the commit removes and a re-ask never writes, so they stayed
  empty; the run now names the stream of the turn that ended it (the summary's `events file:`) and
  where its reply lies, and the death record falls back to that. A zero-tool guard failure's own
  wall is folded into the run's wall (it was 0 s with no re-ask). The consult reference now states
  that grok's read-only tools can read outside the repository (a sibling worktree,
  `~/.cargo/registry`) - documented, not restricted.
- **The sub-agent guard is bounded by the turn's launch (F13-1).** It counted a `spawn_agent` call
  from the time the budget watch starts, taken after the spawn, a slow `on_running` and the stdin
  write and set two seconds back: a historical call from the last two seconds before a resumed turn
  still stopped it, and a current call made before a slow `on_running` returned was not counted. The
  guard now takes its own boundary just before the spawn; the budget watch keeps its backdated one.
- **The sub-agent guard on a forked thread.** Observed on codex-cli 0.169.0: a `codex exec ... fork`
  rollout holds only the fork's own lines (its `session_meta` references the parent's history through
  `history_base`), so the time bound holds. A history copied into a rollout - as in multi-agent v2's
  inline-forked sub-agent rollouts - is re-stamped with the fork's time, so for a fork without
  `history_base` the guard now also starts its scan at the rollout's last `task_started` line, the
  current turn's first line: a copied call no longer stops it.
- **The repository model repair never writes through a file (F02-1, F02-2).** It truncated and
  rewrote the file in place, so a pinned agent file hard-linked to a file outside the checkout
  edited that file too, a failed or interrupted write could leave a truncated file, and an editor's
  save between the read and the write was lost. The new content now goes to a sibling temporary file
  in the same directory, which is synced and read back; the links are checked again and the
  original is re-read immediately before the rename and compared with the snapshot the edit was
  computed from (a changed file is left as it is); only then does the temporary file replace the
  original - by rename in this fix; an edit landing between that re-read and the rename was still
  lost, which the acceptance review judged open: since the F02-2 entry below the replacement is an
  atomic exchange that keeps such an edit. A file with more
  than one hard link is not edited, as before neither is a symlink, a junction or a file under a
  linked `.claude` / `.claude/agents`. Every repair not made is reported with its reason - in the
  hook line (`c3: model settings NOT refreshed in this repository - <file> <old> -> <new>: <why>`)
  and in `c3 doctor --fix` (`[!] ... not repaired - <why>`, exit 1; `--json` `fix_problems`).
- **The model repair rewrites only a validated concrete Claude id, and only its value (F02-3,
  F03-4).** Any value starting with a family prefix was taken for a pin, so a custom
  `claude-opus-foo` became `opus` and an already floating `opus[1m]` lost its modifier. Now only an
  id of the claude engine's model table or of the published shape is rewritten, a `[1m]` modifier
  kept on the alias (`claude-opus-4-8[1m]` -> `opus[1m]`); `c3 doctor` still reports any other
  pin, with no alias to suggest. Only the value is replaced: its quotes and a trailing `# comment`
  stay. `c3 doctor` and the repair read the frontmatter key by one rule: an un-indented `model`,
  spaces before the `:` allowed (`model :`), the `:` followed by white space or the line end.
- **`AGENTS.md` follows one roster entry, never the highest id of all (F02-4).** The successor was
  the highest id among every active entry's model, protected and on-request entries and other
  providers included. Now only an active entry that is neither protected nor on request counts, and
  each entry is asked on its own: an entry whose model is still the file's keeps it, and the file
  is rewritten only when exactly one successor id results (two entries with different successors
  leave it alone).
- **`c3 doctor --fix` repairs nothing inside a C3 child (F04-4).** The `C3_CHILD` guard was in the
  hook only, so a reviewer or worker that ran `c3 doctor --fix` edited the repository under review.
  The repair itself now returns at once under the mark, and doctor prints `[i] --fix skipped inside
  a C3 child` (`--json`: `"fix_skipped"`); the report still runs.
- **The hook's repair starts no process, and the branch line has one deadline (F02-6).** The repair
  found the repository root with `git rev-parse`; it now walks up to the nearest `.git` (a
  directory, or the file of a linked worktree). The branch line's read-only git queries
  (`rev-parse --show-toplevel`, `symbolic-ref`, `show-ref`, `config --get-regexp`, `for-each-ref`,
  `worktree list --porcelain`, `rev-list --count` - never `status` or `cherry`) now share one
  2-second deadline: past it the line is dropped and the hook ends without waiting (the hook's own
  output handles are no longer inherited by those git children, so a stalled one cannot hold the
  session's hook output open).
- **`C3_CHILD=1` reaches the reviewer, worker and maintenance children (F03-1, F02-5, F04-6).** The claude engine's
  allow-listed child environment did not carry the mark (a turn got it only because the launcher
  added it after the allow list), its `auth status` / `--version` probes did not get it at all, and
  the detached `c3 maintain` pass only inherited it from a session that already had it. The allow
  list now sets `C3_CHILD=1` itself (the ledger's `child_env_allowed` names it), the probes set it,
  and the detached pass gets an explicit environment with the mark (a `CreateProcessW` environment
  block on Windows). Tests start a fake launcher that records the variable for a claude reviewer
  turn and its probes, a codex-style and a claude-style turn, a `c3 work` worker on each engine path
  and the detached spawn.
- **A page's ids are published only by the exact public URL they were read from (F57-1, found by
  the acceptance review; it supersedes F02-19 for pages).** The maintain state recorded a configured
  page's ids under its label (`page:<name>`) and telemetry trusted a page by the same label, so a
  configuration that later gave the label to another URL declared `"public": true` made telemetry
  publish ids read from the earlier, non-public page - a plain id even took the new URL's vendor
  class. A page record now carries the canonical URL its ids were read from (`url`, in the form the
  trust decision uses: https, lower-case host, port, path, query) and whether that URL was declared
  public when they were read (`public`). A page is trusted by `page:<canonical URL>`, never by its
  label, and its ids count only when the CURRENT configuration declares exactly that URL public AND
  the record says it was public when read. A page record of an older build (label only, no URL) is
  withheld until the next `c3 maintain` refresh rewrites it. Catalogs keep their per-URL rule
  (F16-1); both kinds now go through one rule (`maintain::web::canonical_url` for the URL,
  `maintain::web::provenance` for a record's trust key), so they cannot drift apart. What remains: a
  page whose URL stays the same and is declared public only later is published after the next
  refresh, not at once. Tests: `maintain::models::tests::a_page_id_is_published_only_by_the_exact_public_url_it_was_read_from`,
  `maintain::web::tests::a_reused_page_label_never_lends_its_trust_to_ids_of_the_earlier_url`,
  `a_page_source_is_named_by_its_label` (extended), and `telemetry_parity.rs`'s
  `a_page_label_reused_for_a_public_url_never_publishes_the_earlier_pages_id` (the reviewers' RC1,
  offline through the real binary: `c3 maintain --models` records an id from a non-public page, the
  label then names a public `docs.z.ai` page - the backfilled event's model is `other`; an older
  build's label-only record - `other`; after a refresh from the public URL - the id by name).
- **A roster edit saved while `c3 maintain` writes the roster is kept (F57-10, found by the
  acceptance review).** The swap re-read the roster, compared it with the snapshot the smoke was
  judged on, and then replaced it by a separate temp-file-and-rename: an edit saved between that
  last comparison and the rename was lost - and not even the day's backup (`.bak-maintain-<date>`,
  the older basis) had it. The roster is now published by an atomic EXCHANGE of the temp file and the
  roster (`contain::publish_exchange`: Windows `ReplaceFileW`, Linux `renameat2(RENAME_EXCHANGE)`,
  macOS `renamex_np(RENAME_SWAP)`), after the same comparison. The version it displaces is read
  back: when it is the compared snapshot it is removed; otherwise it is kept next to the roster as
  `<roster>.displaced-<UTC time>`, and `c3 maintain` prints `c3 maintain: ATTENTION: a hand edit of
  the roster <path> landed during C3's write: it is kept at <path>; the file now holds C3's version`
  and records that line in its report. The roster's directory (the codex home) is reached by its
  path and opened once as a handle; a roster that is a link is followed to the file it names, which
  is the file exchanged; the new roster keeps the old one's permission bits on Unix. On Windows a
  sharing violation (an editor holding the file) is retried up to 8 times, 250 ms apart, as the
  earlier write did. What remains: where the system has no exchange (a Unix other than Linux and
  macOS, a file system that refuses it - `EINVAL`, `ENOSYS`, `ENOTSUP` - or `ReplaceFileW`
  unsupported) the roster is replaced by the earlier compare-then-rename and the report says so
  (`... was replaced by rename, not by an atomic exchange (<why>)`); a kept file is the operator's to
  compare and remove. Tests: `maintain::models::tests::a_roster_edit_saved_at_the_publication_boundary_is_kept_byte_for_byte`
  (the reviewer's Verify: an unrelated roster change saved at the pre-publication boundary stays
  recoverable byte for byte), `contain::tests::publish_exchange_keeps_an_edit_saved_before_the_exchange_and_nothing_without_one`,
  `contain::tests::without_an_exchange_publication_falls_back_to_the_rename_and_says_so` (the new
  hook `CODEX_CONSULT_TEST_CONTAIN_NO_EXCHANGE`; a first write too) and, on Unix,
  `contain::tests::publish_exchange_on_unix_swaps_relative_to_the_directory_descriptor`. The race
  hook `CODEX_CONSULT_TEST_CONTAIN_RACE` takes `<point>|write:<file>|<text>` for an editor's save at
  a named point.
- **A repository repair never loses an editor's save (F02-2, judged still open by the acceptance
  review).** The model repair - and the rules repair, which takes the same path - re-read the file,
  compared it with the snapshot the edit was computed from, and then renamed the temp file over it:
  a save between that comparison and the rename was lost. The file is now published by the same
  atomic exchange (F57-10), relative to the directory handle the repair already holds; the link and
  hard-link checks before it are unchanged. A displaced version that is not the snapshot is kept
  next to the file as `<file>.displaced-<UTC time>`; the SessionStart hook line ends with `;
  ATTENTION: a hand edit of <file> landed during C3's write: it is kept at <file>.displaced-<time>;
  the file now holds C3's version`, `c3 doctor --fix` prints the same as `[!]` (JSON
  `fixed[].notice`, `rule_notice`, `state_sentence_notices`), and `c3 doctor` names every kept
  `.displaced-` file in the repository root, `.claude/agents` and `.claude/rules` until it is
  removed (`[!] <file>: a version a C3 write displaced (a hand edit saved during it) is kept here -
  compare it with the file and remove it`, JSON `displaced_files`; not a finding). On Windows a file
  held open by an editor is retried as for the roster. What remains: as for the roster, a system
  without an exchange falls back to compare-then-rename and the line says so; a kept file lies in the
  working tree, so `c3 finish` commits it unless the operator removes it first. Tests:
  `setup::repo_models::tests::an_edit_saved_during_the_repair_is_kept_beside_the_file_and_reported`,
  the `contain` tests of F57-10, and the earlier repair tests (an edit before the comparison, a held
  file, a directory swapped for a link, the ordinary replace) unchanged.

## [0.2.4] - 2026-11-18

### Added

- **`c3 finish` / `c3 fail` / `c3 manifest` and the MCP tools `c3_finish_task`, `c3_fail_task`,
  `c3_manifest_edit`** (eck port Part A, `docs/eck-port.md`): C3 now commits, but only through
  `c3 finish` - `git add -A` minus a never-commit set (raw event streams, `.collab` locks and
  pending files, work-run `.log`/`.stderr`), a secret gate on the staged content (path and kind
  named, never the value; `--allow-secret`), the staged list printed, then the commit; the report
  goes to `.eck/lastsnapshot/AnswerToSA.md` and newest-first into `.eck/JOURNAL.md`; a delta
  snapshot (full when no anchor) lands in `.eck/snapshots/` and `.eck/lastsnapshot/` with the
  report embedded, pruned beyond `--keep` (10), the never-commit set left out of its body.
  `c3 fail` does the same without committing (`--fail` delta to the working tree). `c3 manifest`
  edits `.eck/*.md` only (no path traversal; `append_to_section` appends at the section's end).
  The MCP tools take eck's argument names.
- **Snapshot additions (eck port Part B):** `c3 snapshot --base <commit | snapshot file>` (the
  header now records `- Anchor: <sha>`; an ad-hoc base does not move the anchor);
  `.eckignore` (gitignore syntax) honoured by every snapshot and pack; `--profile a,b,-c,<glob>`
  from `.eck/profiles.json`; the `finish`/`fail` hand-off snapshot drops every hidden path
  except the `.eck/*.md` manifests (JOURNAL.md excluded) - the first full hand-off of this
  repository was 19.5 MB with every `.collab` brief inside.
- **Project setup (eck port Part C):** `c3 init` creates the missing `.eck` manifests as
  `[STUB]`s, adds `.eck/` to `.gitignore`, writes `CLAUDE.md`, `.claude/rules/01-c3-protocol.md`,
  `02-delegation.md` and the sonnet/opus/haiku worker agents only when missing, and registers
  `c3 mcp` in `.mcp.json` beside the other servers; `c3 doctor` reports leftover stubs, the MCP
  registration, eck leftovers and the snapshot folder size; `c3 scout <dir> [--depth]` and
  `c3 fetch <glob>...` write into `.eck/scouts/`; plugin skills `c3-scout` and `c3-fetch`. The
  `finish` secret gate now scans only the ADDED lines (an old test fixture no longer re-blocks
  every commit touching its file). Fixed: `c3 snapshot --depth 0` printed an empty tree.
- **Dead detached runs leave a death record:** written by the background, or once by the next
  `--status`/`--wait` reader (which side vanished, child exit, elapsed, last engine event, a
  sanitized log tail); `--status`/`--wait` print it and the raw-stream path when it may hold a
  usable reply.
- **Per-model health mark and panel re-seat:** a "model not supported / does not exist /
  unknown model id" failure marks that roster entry (not the provider) unavailable for 24 h, a
  usable reply clears it; a panel seat lost at preflight is re-seated from the same draw.
- **Cross-model tests in `c3 work`** (consilium 2026-10-29, `.collab/consilium-2026-10-29-test-workers`):
  every work record holds `patch.diff` (the run's own change from each file's start content;
  pre-dirty baselines kept as loose git objects); `--role tests` inlines another author's change
  (the working tree against HEAD, or `--of <id>`'s `patch.diff`) into the prompt with a
  cases-first test-writer preamble; `--scope <glob>` plus the test-path defaults is checked after
  the run (`scope_violations` in `run.json`, `OUT OF SCOPE`, exit 1, also on a resumed run); a
  tests run is refused on the muse worker; `--status` prints the role and the violations. The
  tests were written by a GLM worker run through `--role tests` itself.

- **`c3 update` — self-update check and apply** (wave 11, `docs/port/wave11-update.md`): C3 now
  checks whether a newer release of itself exists and offers it; it applies only on an explicit
  flag. The source of truth is the git tags of `xelth-com/C3`, read once a day through the
  unauthenticated GitHub tags API (highest `v<major>.<minor>.<patch>`, compared with
  `CARGO_PKG_VERSION`); the answer is cached at `<codex home>/c3/update-check.json`
  (`{checked_at, latest, latest_sha, installed, source}`, atomic write, a failed fetch kept silent
  and the previous cache kept). The SessionStart hook prints one extra line when a newer version is
  on record — `c3: update available v<latest> (installed <installed>) - review:
  https://github.com/xelth-com/C3/blob/v<latest>/CHANGELOG.md ; apply: c3 update --apply` —
  refreshed within a 3 s budget and never failing a session; startup/resume/compact output is
  otherwise unchanged, and `plugin/hooks/c3-hook.{ps1,sh}` forward the line. `c3 update`
  (`--check`) forces a check and reports installed vs latest, whether an update exists, the
  changelog URL, the detected install kind and the exact commands `--apply` would run, changing
  nothing (exit 0 up to date, 3 an update exists, 1 on a fetch failure). `c3 update --apply` runs
  them, stopping at the first failure: `cargo install --git … --rev <latest_sha> --locked c3-cli`
  (+ `--features index-surreal` when built with the indexed bundle); then, if a **clean** source
  checkout on a branch is known (config `source_checkout`, env `C3_SOURCE_CHECKOUT`, or the git work
  tree around `CLAUDE_PLUGIN_ROOT`), the release tag is fetched from the canonical URL, verified
  against the API commit and fast-forwarded to it (a dirty tree / detached HEAD / mismatch /
  non-fast-forward is printed and skipped, never failing the install); then
  `claude plugin update c3@c3` when `claude` is on PATH, else the command is printed. The automatic
  check is ON by default and silenced with `C3_UPDATE_CHECK=off` or `"update_check": false` in the
  C3 config; in `CODEX_CONSULT_TEST_MODE=1` no network is ever touched. There is no release
  signature yet: the install trusts GitHub's HTTPS transport and the API's release commit.
- **Grok reading duty on every non-raw purpose (C3):** a live test showed Grok answering a brief in
  one model call without opening a file and INVENTING code with line citations (a `RetryPolicy`
  field and a loop that do not exist), while the same brief by reference drew 5 tool calls and a
  grounded answer. For grok, the brief is now referenced by path on every non-raw purpose (never
  inlined), and a primary turn with zero tool calls fails `capability` - "it cannot have read the
  brief" on the non-review purposes (framing, decision, stuck, chore), the existing review wording
  on the four review purposes. The failure drives the existing read-insist re-ask (up to
  `CODEX_CONSULT_READ_INSIST`, default 2; `0` = fail at once; the non-review re-ask tells it to open
  the brief), and a non-review prompt carries a `Reading duty` paragraph. `raw` grok consultations
  keep the old inline and are exempt on non-review purposes; kimi and every other engine are
  untouched.
- **`c3 maintain` - C3 keeps the engine CLIs and the seated model ids fresh by itself:** once a
  week the SessionStart hook starts a detached pass (never blocking the hook) and the next session
  prints its one-line report (`c3: maintenance 2026-11-15 - codex 0.155.1 -> 0.169.0 ok; grok up
  to date; ...`). Per engine: installed and latest version, and when an update exists - a smoke of
  the INSTALLED version first, the update, the version re-read, a second smoke, and a rollback
  when that fails. The smoke is two real turns through `c3 consult` in a throwaway repository
  (one plain, one that must read a file holding a random token). codex updates through npm
  (rollback by version), grok through `grok update` (`--version` pins), kimi through the vendor
  installer (`KIMI_VERSION` pins; https only, a redirect only to the vendor's one listed mirror); agy has
  neither a check nor a pin and is report-only; muse and claude update themselves and are only
  recorded. An engine with a live run or process is skipped and retried the next day. A version
  released less than `C3_MAINTAIN_MIN_AGE_DAYS` (3) days ago is held when its date is known. The
  smoke never spends the ChatGPT allowance when a custom codex provider can carry it, and never a
  plan whose window is at 90 % or more. Models: the lists of `grok models`, `agy models` and the
  plan endpoints are stored and new ids announced; a seated id is replaced only by a successor
  with the same prefix and suffix, the same major and a higher minor, only after it passed the
  smoke, with the roster backed up (`.bak-maintain-<date>`); stamped ids, major jumps and new
  families are announced only. `c3 maintain` reports, `--run [--engine] [--dry-run] [--force]`
  runs a pass now, `--smoke [--engine] [--dry-run]` runs only the smoke. Hardened after the
  wave's diff-review (GLM-5.3, Muse Wonder, Grok; 30 findings): the pass lock is taken by atomic
  creation; an engine is re-checked for use right before its update, by command line as well as
  image name, and an unreadable process table counts as in use; a pending-update record written
  before every install lets the next pass verify or roll back an update interrupted by a crash;
  an unparsable installed version is never updated; every update is pinned to the checked
  version and every update and rollback is confirmed by re-reading `--version`; model ids are
  accepted only in a strict shape, a dotted date stamp also makes an id unswappable, the roster
  is written through a temp file and not at all when it changed during the pass; update and
  installer children run without the provider keys in their environment; a download that
  reaches the size cap is never run; the detached pass is started without a shell. ON by default;
  `C3_MAINTAIN=off` or `"maintain": false` in the C3 config stops the hook's passes,
  `C3_MAINTAIN_DAYS`, `C3_MAINTAIN_SKIP`, `C3_MAINTAIN_MODELS=swap|announce|off` and
  `C3_MAINTAIN_SMOKE_<ENGINE>=provider::model` tune it.
- **Budget-aware panel routing (`c3 budget`):** every roster lineage's panel draw weight is now
  scaled by a plan multiplier - the plan's remaining quota share divided by the remaining share of
  its period, the most constraining window of the plan governing, clamped to 0.25..2.0 (an idle
  plan at most doubles a seat, and the boost never lifts a lineage already scored below neutral -
  a penalty, `mult < 1`, always applies). Multipliers are read live for the `openai` account (codex
  rollouts), for kimi (`GET api.kimi.ai/coding/v1/usages`, cached 5 minutes) and for z.ai and Muse
  (see "Live quota" below); every other plan reads the operator's own snapshot from `c3 budget set <plan> --window <w> --used <pct> --resets
  <iso>` (`c3 budget show` lists them all; a snapshot is `stale` past 7 days or once its own window
  has reset, whichever comes first). `CODEX_CONSULT_BUDGET_ROUTING=off` pins every multiplier to
  1.0. `c3 router explain` gains `weight`, `mult` and `budget` columns (the latter naming the
  governing window), the per-member multiplier is recorded so `c3 router replay` reproduces a seat
  offline, and `c3 providers` fills the QUOTA column for every plan that has windows, not only the
  built-in ones.
- **The native `kimi` engine:** drives Moonshot's Kimi CLI headless (`kimi -p <pointer>
  --output-format stream-json -m <model>`), resumed with `-S <session id>` read from the CLI's own
  `session.resume_hint` line; the assembled prompt goes to a temp file and only a short pointer
  travels on argv (refused past 30000 chars). Model ids are whatever the CLI's own `config.toml`
  declares (`kimi-code/k3` default, `kimi-code/k3-256k`, `kimi-code/k3.1-pro`,
  `kimi-code/kimi-for-coding`, ...; `kimi-code/k3.1-pro` confirmed live by the 2026-11-08 smoke in
  `.collab/smoke-kimi`). The CLI has no read-only flag and can write, so every kimi turn runs with
  its cwd set to a throwaway detached git worktree of HEAD (removed after the run, recorded on the
  handoff's `Isolation:` line), with the strict main-tree check as a second guard - any
  working-tree or `.collab` change there FAILS the run. Roster: `engine: kimi`; replies are
  `handoffs/<NN>-kimi-<slug>.*`.
- **Read-insist re-asks a zero-tool review:** a grok or kimi reviewing primary turn that made no
  tool calls is re-asked up to `CODEX_CONSULT_READ_INSIST` times (default 2; `0` fails at once)
  before the existing zero-tool-calls `capability` guard fails the run; a re-ask that reads (tool
  calls > 0) becomes the primary reply - kimi's re-ask resumes the same thread (grok's re-ask is a
  new conversation, see below). Both engines' review prompts now say outright that a reply with no
  tool calls is rejected. The handoff header gets a `Read insist: <n> turn(s), tool calls 0 -> <k>`
  line and the ledger a `read_insist: {attempts, final_tool_calls}` entry.
- **Live quota for z.ai and Muse:** the budget module reads the z.ai plan's 5-hour token window
  from `GET https://api.z.ai/api/monitor/usage/quota/limit` (the plan key; the monthly tools
  allowance is ignored) and the Muse 5-hour and weekly windows from `muse usage --json` (no model
  turn; "no usage yet" yields nothing), cached like the kimi source, ahead of the operator's
  snapshot. Both show in the QUOTA column of `c3 providers` and weigh the panel draw.
- **`c3 --version` names the build commit:** `c3 0.2.4 (1a2b3c4)`, with `-dirty` when tracked
  files differed from HEAD at build time, so an installed binary can be told from the tree.

### Changed

- **The bare default reviewer depends on the purpose, and two roster `ext` settings protect scarce
  reviewers.** A single consultation with no `--provider` is no longer always the roster's first
  entry: on the standing purposes **acceptance** and **core-contract** it stays the first available
  entry (the standing partner); on every other purpose it is ONE reviewer picked by the same seeded
  weighted draw a panel uses (usefulness score x effective budget multiplier), among the entries a
  panel of that purpose would seat, so repeated bare consultations spread instead of draining the
  first entry. The bare run prints the pick, its weight, the candidate count and why the first entry
  was not it; `--panel-seed` reproduces the draw (now accepted on a single consult too). Two new
  per-entry `ext` settings: `reserve_percent: N` (1..50) makes an entry **protected** - HELD on a
  non-standing purpose once `used% >= min(elapsed% + N, 100 - N)` on any long budget window (24 h+,
  non-stale, reset ahead), i.e. it may not run more than N points ahead of the calendar and keeps
  its last N % for acceptance / core-contract; it is then skipped by the bare default and the panel
  (skip kind `reserve`), refused when named explicitly, and an outage when `--require`d - unless
  `--spend-reserve` (new flag, single and panel). `on_request: true` makes an entry **on-request** -
  never seated automatically (not by the bare default, a panel draw or `--panel-all`); it runs only
  when named exactly by `--provider`/`--model` or a `--require` matcher (skip kind `request`, "on
  request only"). `c3 providers` notes the hold on the entry's row (VERDICT stays `available`) and
  `c3 router explain` marks entries `[held: reserve]` / `[on request]`. `c3 maintain`'s smoke never
  targets a protected or an on-request seat. After the wave's diff-review (Grok, Muse contributor;
  7 findings): the hold and the on-request rule are evaluated before the light/weighty gates, so
  neither a stand-in promotion nor a `--require` can seat such an entry; the hold also applies with
  `--skip-preflight` and to a reviewer pinned by `--thread` or a resume/fork; a bare `--mode
  resume|fork` continues the reviewer of the task's latest verified consultation when it is still
  eligible and not held, and otherwise draws and starts a new thread, saying so; an old quota
  reading is a floor - it may hold a protected reviewer but never raises a weight; the hold, the
  draw and `router explain` read one set of windows per run.
- **The ChatGPT account's budget window is no longer forgotten after two idle days.** When today's
  and yesterday's codex rollouts give nothing, `budget::openai_windows` falls back to the newest
  observation in the rollouts of the last 8 days, then to the recorded quota of the machine-wide
  health file / ledgers, keeping only windows whose reset is still ahead and showing the real
  observation age ("seen Nd ago"). The health verdict's own two-day rule is unchanged.
- **Grok takes the reply schema in the prompt, not as `--json-schema`:** under the native flag
  every text Grok writes must be schema-shaped, so its opening remark came out as a schema-valid
  placeholder answer ("I am reading the brief before answering") and, carrying no tool call,
  ended the turn - the root of the "Grok answered without reading" failures. Live on one fixture
  brief (grok-4.7, CLI 1.0.77): with the flag 3 of 5 fresh conversations ended on a placeholder
  with 0 tool calls; without it every conversation read the files and closed with a valid JSON
  answer. `--schema-transport native` restores the flag for one run.
- **A grok read-insist re-ask is a new conversation:** it carries the whole original prompt with
  one insisting line in front instead of resuming the thread that did not read; the recorded
  thread is the accepted turn's, and a run whose every attempt read nothing records none. The
  ledger's `read_insist` gains `new_conversation`. kimi keeps the same-thread re-ask.
- **The roster loader skips an entry with an unknown engine instead of refusing the whole file:**
  an entry whose `engine` is a well-formed name this build does not recognize is kept at its
  roster position, marked `inactive` and `unknown_engine` (so `#n` lookups still line up, but it
  is never selected, scheduled or probed), and reported once as a roster warning - `entry <n>:
  engine "<name>" is unknown to this c3 build - entry skipped (upgrade c3 to use it)` - surfaced by
  `c3 providers`. A malformed engine value (not even a well-formed name) still refuses the file as
  before. Motivated by a field report: the native kimi entry in a live roster made an
  older, not-yet-upgraded c3 refuse every consultation on that machine.

### Fixed

- **A run that died in a re-ask is described by that re-ask** (field report, xelixir, detached
  run a7dde5eb): a grok diff-review whose re-ask made 57 tool calls over ten minutes and then
  stopped without a final answer was recorded as "answered in one model call ... 0 tool calls
  (wall 0 s)" with the first attempt's placeholder as the reply. Now the outcome is the last
  attempt's own failure, classified like a primary turn's and named `read-insist attempt N after
  a zero-tool primary; K tool calls`; the summary's events file is that attempt's stream; its
  salvaged text is the reply; the member wall is the total of the attempts. The grok answer is
  the text after the last tool call, so an interim note is never taken for it, and the grok
  prompt says the structured answer is written once, at the end. From the wave's diff-review: a
  grok turn that wrote a note, read files and ended without any text after its last tool call is
  a failed turn (the note is kept as salvage, never as the answer); a re-ask that could not even
  start keeps the zero-tool outcome and names the launch failure; the reply of a run that died in
  a re-ask is that attempt's text after its last tool call, or empty - never the first attempt's
  placeholder.
- **Quota reads (from the same review):** a quota GET that carries a plan key (z.ai, kimi) never
  follows a redirect; the live-source caches are keyed by source and plan; a live window whose
  reset has passed is dropped instead of feeding the draw its pre-reset usage.
- **`c3 budget set --resets` accepts minute-precision instants:** `2026-11-16T15:18Z` and
  `2026-11-16T15:18+01:00` - the shape `--help` advertises - were refused as "must be a date or ISO
  instant"; only the form with seconds parsed.
- **A grok code review that read nothing is no longer usable:** on `acceptance`, `core-contract`,
  `diff-review` and `checkpoint`, a grok reply with zero tool calls (one model call, e.g. "HOLD -
  I am still reading", 13.8 s) fails with class `capability` (no outage mark); framing and decision
  answers without tool calls stay usable.
- **Review-2026-10-30 fixes:** the `finish` secret gate fails closed (unreadable list or diff),
  reads `+` lines inside hunks only, scans typechanges, binary blobs and the commit message,
  refuses key-container files; agy writing-tool calls always fail the drift check, tool names come
  from every turn; a claude init listing a writing tool is not write-disabled; panel re-seat exit
  and summary over the final seats, a new seat number, the endpoint concurrency group,
  `routing.picked` rewritten; no death record while the bridge lives (task lock around the final
  write); a model-unsupported mark survives later failures; detached children try
  `CREATE_BREAKAWAY_FROM_JOB`.

- **grok reviewer answered an invented brief** — a grok `--json-schema` turn can answer in one
  model call without calling `read_file`; it then made up the brief from the question numbers
  (panel 2026-10-29, handoff 05). The brief's text is now inlined into the prompt after the
  "Read the brief at" line for engines whose spec sets the new `EngineSpec::inline_brief` (grok
  only). Tests in `consult::prompt` and `lineage` written by a GLM worker run (`c3 work`).

- **Wave 11b (`c3 update` hardening, Astra handoff 97)** — the self-update now binds to the GitHub
  tags API's release **commit**, not the tag name: `--apply` installs with
  `cargo install --git … --rev <latest_sha>` and synchronises a source checkout by fetching the
  release tag from the canonical `https://github.com/xelth-com/C3.git` into a dedicated ref (never
  the checkout's `origin`), verifying its peeled commit equals `latest_sha` before any merge and
  fast-forwarding to that sha — a fork or stale local tag with the release's name can no longer move
  the checkout (F97-2). The checkout's identity (work-tree root, branch, HEAD, cleanliness) is
  captured in the plan and **revalidated immediately before the merge**, after the fetch, so a
  branch switch, a detached HEAD or a tree that became dirty during the lengthy `cargo install`
  skips the sync instead of fast-forwarding the wrong thing (F97-1). The cache now records
  `attempted_at` (every attempt) separately from `checked_at` (last success): the hook backs off for
  1 h after a failed attempt and 24 h after a success, offers only from a success younger than 7
  days (a stale answer that may name a deleted release is dropped silently), and `c3 update` prints
  the cached data with a `refresh failed` note when a forced refresh fails (F97-3).
- **Wave 11c (Astra acceptance minor, handoff 99)** — the self-update backoff test now observes the
  fetch directly: the test-mode fetcher counts each actual attempt into
  `C3_UPDATE_TEST_FETCH_COUNT_FILE` (test-mode only, no network) and the test asserts exactly one
  fetch across two hook runs, so a throttle miss can no longer hide behind whole-second timestamps
  (F99-2).
- **Codex sub-agents stay off on codex-cli 0.169.0 (and a guard catches it if they don't):** the
  CLI update moved the live subsystem to multi-agent v2, whose `spawn_agent` tool surface
  `-c features.multi_agent=false` no longer removes (a model's `multi_agent_version` metadata
  outranks the feature flag), so an acceptance run still spawned three sub-agents and burned ~14 %
  of the ChatGPT weekly allowance in minutes. Every codex turn now also passes
  `-c agents.enabled=false` (proven live on a GLM reviewer with a forced-v2 model: the tool surface
  is present without it and gone with it); the legacy switch is kept for older CLIs and both are
  harmless on a CLI that does not know the key (codex ignores an unknown `-c` key without
  `--strict-config`, which the bridge never passes). The user's opt-in still wins: a
  `--codex-config` that turns sub-agents ON (`agents.enabled=true`, or `features.multi_agent=true`
  with `agents.enabled` unnamed) passes none of the off switches; one that turns them OFF by a
  single key still gets the switch it did not name, so "off" means off on every CLI.
  As defence in depth, the watch that already polls a run's rollout for the quota budget (every
  15 s) now also notices a sub-agent - a `spawn_agent` call in the rollout, or a child rollout
  naming this thread as parent - and stops the main, continuation or repair turn at once, keeping
  the partial exactly as the budget stop does; the outcome names the codex version, and the failure
  is class `operator` (a tooling failure, not counted against the reviewer's quota health or its
  capability).
- **Acceptance findings of the standing partner (task release-0.2.4, handoff 09, recovered from
  its rollouts after the run above was stopped; R1-R7):**
  - `c3 maintain` keeps its recovery journal after a failed install or rollback (R1): a nonzero
    installer, an update that did not take, or a rollback that did not restore now retains the
    pending-update record, so the next pass re-probes and smokes or rolls back the result instead
    of reading a half-installed CLI as "up to date".
  - `c3 maintain` fails closed when the pending record cannot be written (R2): `write_pending` /
    `set_pending` propagate directory, write and rename errors, and the pass holds without running
    the installer when the write-before-install record was not persisted.
  - The `c3 maintain` model swap is immune to a roster edit during the smoke (R3): the protection
    and saturation gate and the swap's comparison basis are read from one fresh pre-smoke
    snapshot, and `apply_swap` refuses to write when the roster on disk changed since it, so
    protecting the entry or re-pointing its old model during the smoke aborts the swap.
  - A protected on-request reviewer is no longer spent by an exact `--require` (R4): the reserve
    hold is evaluated before the on-request gate, so an entry that is both `on_request` and over
    its `reserve_percent` carries the hard `reserve` skip and a non-standing panel that requires
    it refuses (exit 5) unless `--spend-reserve`.
  - Budget windows keep their own observation age (R5): a merged quota reading uses each window's
    own `primary_observed_at` / `secondary_observed_at`, so a recent 5-hour reading no longer makes
    a retained old weekly reading look fresh and raise a routing weight above 1.
  - A future-dated quota observation can no longer evict a real one (R6, the open part of F06-4):
    future-dated per-window candidates are dropped before the account merge, so a clock-skewed
    reading cannot win on recency and discard a genuine held reading (a 97 % weekly whose reserve
    hold would vanish).
  - An uncommitted brief reaches the `kimi` reviewer (R7): the brief is staged into kimi's
    throwaway HEAD worktree at its repo-relative path, so a brief that exists only in the caller's
    working tree is the one the reviewer reads. The other R7 question was refuted: a skipped
    unknown-engine roster entry keeps its position, so `#n`, the first entry and `--provider`
    resolve as the file shows them.
- **Second acceptance pass of the standing partner (task release-0.2.4, handoff 11, F11-1..F11-4):**
  - `c3 maintain` no longer clears a pending update whose recovery failed (F11-1): after a failed
    recovery smoke, only a rollback that both runs AND restores the previous version clears the
    record. A failed rollback, a rollback that did not restore, or no rollback path now reports
    `pending update unresolved: ...` (never `recovered`) and keeps the record, so a later due pass
    smokes the still-broken CLI again instead of reading it as up to date. The recovery smoke stays
    behind the normal due/forced gate (the SessionStart hook spawns a pass only when one is due), so
    a retained record never spends a consultation on every session start.
  - The `kimi` reviewer's reply is only the answer after its last tool call (F11-2): text a kimi
    turn emits BEFORE a tool call (an early verdict or placeholder) is no longer taken as the usable,
    verified reply when the turn ran tools but produced no answer after the last one - that turn now
    fails like a stream that ended without an answer, and salvage keeps only the post-tool tail, as
    the grok adapter already does. In both adapters that failure is now recorded with its reason as
    the message: the reviewer's interim text is no longer the failure evidence, so a placeholder that
    mentions "quota" or "timeout" cannot be classed as a quota or transport failure of the provider.
  - Every `kimi` turn stages the operator brief into its worktree (F11-3): read-insist, continuation
    and repair turns create a fresh HEAD worktree too, so an uncommitted or locally modified brief is
    present at its repo-relative path on the very turn a zero-tool primary is re-asked to read it; a
    staging failure now refuses the turn before the reviewer launches.
  - The sub-agent guard no longer kills a compliant resumed run (F11-4): the scan for a `spawn_agent`
    call in the run's own rollout is scoped to the current turn (a call counts only when its line
    timestamp is at/after the turn's launch), so a resumed thread whose history holds an old spawn
    call is not stopped when the current turn spawns nothing; a new spawn still stops it, and the
    child-rollout detection is unchanged. A forked thread is covered only as long as Codex keeps the
    original timestamps on the history it copies - not observed on codex-cli 0.169.0.
- **Third acceptance pass (handoff 13): ACCEPT at 7c1e91f.** One minor finding is left open, F13-1: the
  time bound of the sub-agent guard is the start of the budget watch - taken after the reviewer process
  is launched and set two seconds back - not the launch itself, so a `spawn_agent` call from the last
  two seconds before a resumed turn would still stop it. F06-4 stays open in one part (the rollout
  reading is returned before the recorded one; the quota scan is scoped by provider, not by account).
  No real CLI update, rollback, roster swap or reserve hold has run live yet.

## [0.2.3] - 2026-10-22

### Added

- **R30 wave-boundary compaction** (`plugin/skills/coordinate/SKILL.md`): a Claude Code coordinator
  now compacts right after a wave launches, not mid-review, and recovers its identity afterwards.
  A detached launch (`c3 consult --detach`, `c3 consult --panel --detach`, the MCP `c3_panel`
  tool) prints one extra stderr line after its `status`/`come back` lines — `compact now (wave
  boundary): /compact keep the task ..., the detach id ... and the coordinator record; then
  \`c3 consult --task ... --status --id ...\` on wake` — not in `--dry-run`, not from the
  background child, not from panel members. The SessionStart hook now also fires on the `compact`
  source (`plugin/hooks/hooks.json` matcher, `c3 hook --source`): after a compaction it prints one
  re-hydration line restoring the coordinator identity (`CODEX_CONSULT_COORDINATOR`, resolved
  against the roster) when a coordinator record exists, so the session picks its detached runs (the
  availability line already names them) back up; `startup`/`resume` output is unchanged.
- **`c3 work` — worker runs** (`docs/work-mode.md`): a roster model (engine claude, role
  `"worker"`) as an ordinary coding worker for low-stakes or test work — it edits the tree and
  reports; not a consultation (no evidence check, no findings, no rating, no ledger entry). The
  prompt is a fixed preamble (never git, never commit, a final report) + the brief + `--prompt`;
  the child runs with the write tools under an allow list (`Bash(cargo test|check|clippy|fmt:*)`
  by default, `--allow` to replace, git refused); the record is
  `<collab>/<task>/work/<NN>-<provider>-<model>/` with `brief.md`, `events.jsonl`, `report.md`,
  `diff.txt` (git status/diff computed by c3, never by the worker) and `run.json` (status, turns,
  tool calls, files changed, usage — no secret values anywhere). One live run per task;
  `--detach`/`--status`/`--wait` as the consult flags (status exit 0 done / 1 died / 2 running).
- **`c3 work` — quota, resume and the alarm** (`docs/work-mode.md`): a worker that runs out of its
  provider's usage limit no longer just dies. The run is classified `quota` (a rejecting
  `rate_limit_event`, or the limit wording — the engine's shared `quota_evidence`), `run.json`
  records `quota {text, reset_at, resume_at, resumes, exhausted}`, and the SAME child session is
  resumed with `claude --resume` when the window reopens: automatically in a detached run (the
  background parks `waiting_quota` and sleeps in 30 s slices until `resume_at`, waking on a kick
  file), or on demand with `c3 work --resume <id>` / `--kick`. A foreground quota run prints the
  resume time and the exact resume command and exits 3; resumes are capped by `--max-resumes`
  (default 3), after which the run is `quota` with `exhausted: true` and needs a human. The
  resume window when the provider gives no reset time is `--quota-window-sec` (default 18000 = 5 h)
  or a roster entry's `ext.quota_window_sec`.
- **`c3 work` — the codex and muse engines** (`docs/work-mode.md` "v1.1 engines"): a worker run
  may now drive the `codex` engine (`openai :: gpt-6.1-sol`, `codex exec --sandbox workspace-write`
  — for complex programs) and the `muse` engine (`meta :: muse-spark-1.3-contributor`, near-free on
  the Muse Code subscription) beside the existing `claude` engine. `select_worker` accepts a worker
  entry on any of the three engines; the refusal names all three. Per engine: codex takes its prompt
  on stdin and names its thread in the stream (a resume uses `codex exec resume <thread>`); muse
  takes a `--prompt-file`, writes with `--approval-mode never --trust-workspace --disable-sandbox`
  (shell needs the sandbox off on Windows), and mints its `--session-id` up front so a resume
  continues it. `--allow` applies to claude only; codex/muse write through the sandbox/workspace.
  `run.json`'s `engine` and the `--status` line's `[engine]` suffix name the engine. A second
  byteplus worker entry (`deepseek-v4-1-flash-261017`) was added to the roster.


### Changed

- Tech-debt audit (2026-10-22, `.eck/TECH_DEBT.md`): `c3_core::engine::SubprocessEngine::{run,
  continue_turn}` no longer panic with a stale "milestone 2b" `unimplemented!` - they return the new
  `EngineError::PlanOnly` (that type only plans launches; execution lives in the `c3` adapters);
  stale `allow(dead_code)` and the `_wall` import shim removed; `.gitattributes` gains `* text=auto`
  so line endings normalise the same on every machine (the byte-exact fixture rules are unchanged);
  `docs/port/harness-results.md` points at the RC5-RC7 gate documents for every run after Run 17.
- Phase 2 switch done on the operator's machine (`docs/port/phase2-switch.md`): the `c3` plugin is
  the only enabled reviewer bridge; the operator's `astra`/`workers` skills name `c3:*`.


### Fixed

- **muse-wonder-1.0 refused before reaching Meta**: the caps-v1 muse catalog (`c3-core` effort
  table, telemetry classes) declared only the two spark ids, so every `--model muse-wonder-1.0`
  run (and the weighty roster seat) failed with "no effort vocabulary declared for model" while
  the muse CLI itself answered. The id is declared now. The consult reference documents the Muse
  priority: spark-contributor by default, spark-1.3 only when the contributor is down, wonder only
  for hard reviews and consilia.


- **`c3 work` wave 10c — Astra acceptance pass 3 (handoff 93, F93-1)** (`docs/port/wave10a-work-acceptance.md`):
  - **F93-1 (blocker)** the status/`--wait` reader no longer performs an unlocked recovery write:
    `read_record` is now a pure fallible reader, and the dead→`died` transition is done under the
    task admission lock via `recover_dead_run`, which re-reads and writes only when the record is
    still the same generation (pid + pid_start + attempt) and its owner is still dead — so a delayed
    status read can no longer overwrite a newly claimed live resume. `live_run` now checks liveness
    directly; the display/resume listings go through the locked `read_for_display`. Regression:
    `work.rs::a_delayed_status_recovery_cannot_overwrite_a_live_resume_claim`.
- **`c3 work` wave 10b — Astra acceptance pass 2 (handoff 91, F91-1..F91-4, F89-4/F89-5 remainders)** (`docs/port/wave10a-work-acceptance.md`):
  - **F91-1 (blocker)** the resume claim now validates permitted status, exhaustion, owner and the
    attempt generation under the admission lock and computes the attempt from the locked re-read;
    a stale claim (now done/exhausted/advanced) refuses before any spawn.
  - **F91-2 (blocker)** admission fails closed on inspection errors: `run_dirs`/`read_record`/
    `live_run`/`other_live_run` return errors, only a confirmed `NotFound` is absence, and an
    unreadable or replaced record refuses a second launch (naming the directory, not contents).
  - **F91-3 (major)** event-stream READ failures are artifact errors: attempt temp files are kept
    until read and durably appended; `refresh_artifacts` never replaces `report.md` when
    `events.jsonl` cannot be read; the run becomes `artifact_error`.
  - **F91-4 (major)** success needs affirmative terminal evidence: claude a `result` with
    `is_error` present and false (subtype success/absent), codex a `turn.completed` (an
    `agent_message` alone is not success), muse its completion event — extraction kept separate.
  - **F89-4 remainder** the restricted grammar refuses a bare shell/exec tool (a bare tool is
    allowed only for a read/write whitelist) and denies shells, interpreters and git by exe
    basename (trailing `.exe`/`.cmd`/`.bat`/`.ps1` stripped); path separators are refused.
  - **F89-5 remainder** `report.md`, `diff.txt` and the persisted argv pass the project's single
    pack sanitizer (secret-shaped values redacted); `events.jsonl` stays raw; the doc says so.
- **`c3 work` wave 10a — Astra acceptance pass (handoff 89, F89-1..F89-8)** (`docs/port/wave10a-work-acceptance.md`):
  - **F89-1 (blocker)** worker liveness: every `running`/`waiting_quota` record now carries the
    OWNING process's pid + start time (a foreground run writes its own pid BEFORE spawning the
    child); liveness is that pid-with-start-time being alive, and the 120 s start grace only covers
    the window before any pid is written. A foreground run past 120 s stays `running` and still
    makes a second same-task launch refuse.
  - **F89-2 (blocker)** admission: one task-level admission lock covers starts, resumes and the
    detached alarm — re-read under the lock, refuse if another run is live, durably claim the
    record (`running`, new pid/start, attempt number) before spawning; per-attempt event/stderr
    file names (`.events-resume-<n>.jsonl`). Two concurrent `--resume` of one record: exactly one
    spawns.
  - **F89-3 (blocker)** artifact durability: `append_resume`/`refresh_artifacts` return `Result`;
    the resume's attempt files are deleted only after the append succeeded and was flushed; a
    `report.md`/`diff.txt`/`events.jsonl` write failure makes the run `artifact_error` (exit
    non-zero) with the error in `run.json`, never `done`.
  - **F89-4 (major)** a real allow-list grammar (`<Tool>`, `<Tool>(<exe>)`, `<Tool>(<exe>:*)`, no
    shells/git) replaces the substring check; the new `--unrestricted` opt-in (flag or roster
    `ext.unrestricted`) lifts it and is required for the muse worker's `--disable-sandbox`; codex
    keeps `--sandbox workspace-write`; `run.json` records `unrestricted` and `permission`; the doc
    states the boundary precisely.
  - **F89-5 (major)** `run.json` `usage` is rebuilt from an allow list of numeric counters (never a
    cloned object); the record contract in the doc is made precise (events.jsonl is the raw stream
    retained verbatim; run.json/report.md/diff.txt carry no secret values).
  - **F89-6 (major)** the effective `max_resumes` and `quota_window_sec` are persisted at the first
    run and reused by a later plain `--resume` unless the operator passes the flag.
  - **F89-7 (minor)** `files_changed` compares content identity (`git hash-object`) before/after,
    so a pre-dirty file the worker edited further is listed.
  - **F89-8 (major)** success requires the engine's successful terminal event in addition to exit
    0; an error terminal or rejecting rate-limit takes precedence over exit 0; an empty stream with
    exit 0 is `died` with reason `no terminal event`.

## [0.2.2] - 2026-10-15

- Build: the `index-surreal` feature (embedded SurrealDB code index) is now opt-in - the default `cargo install` is the light bundle; `--features index-surreal` for the http/browser packs (README 'Light bundle or indexed bundle').

### Fixed

- **Wave 9 - the four consilium defects (`docs/port/wave9-consilium-defects.md`)**: (D1) a panel starts new threads for every member unless `--mode fork` is given, and the header prints the mode; (D2) C3's own lines name `c3 consult --task <t> --status --id <id>` / `--wait --id <id>` / `c3 providers` / `c3 findings`, not the plugin's `.ps1` scripts (the shims keep the plugin spelling for the harness oracle via `C3_PLUGIN_COMPAT=1`); (D3) a detached single run holds the task lock only for its bookkeeping, so detached runs of one task run side by side; (D4) a roster entry may declare `ext.c3.plan_caps` / `reset_hint`, and two instant 429s inside an hour mark it out until the cap's reset instead of the 10-minute burst.
- **Wave 9b - the review of wave 9**: a detached run is admitted only beside records of a live owning bridge (never survivors, unverified processes or a dead bridge); the reservation is durable and verified before the task lock is released; the cap pair counts distinct consultations (the machine record carries `consult_id`); a non-ASCII `reset_hint` is a validation error, not a panic; the shims set `C3_PLUGIN_COMPAT` for their child only; the cap policy is documented as inferred exhaustion (`docs/port/wave9-consilium-defects.md`).
- **Wave 9c - the round-2 review**: the cap pair is counted by consultation identity only (same non-empty consult id = one hit; different ids are distinct at any instant; a legacy row matches a ledger row only when endpoint, failure text and the second all agree), and the admission test no longer needs PowerShell (`docs/port/wave9-consilium-defects.md`).
- **Wave 9d - the acceptance review**: the cap pair is built from id-carrying rows only (a row without a consult id keeps the plain 10-minute burst) and only from hits younger than 24 h at READ time, so neither a legacy mirror nor an unpruned old row can produce or prolong the cap verdict (`docs/port/wave9-consilium-defects.md`).
- **Wave 9e - the second acceptance pass (F76-1)**: the read-time bound on a cap-pair hit is now the entry's current cap period, not a flat 24 h, so a hinted entry stays out until its declared reset (a hit counts while `next_cap_reset(hint, hit) > now`) while an unhinted entry keeps the 24 h bound; `endpoint_health` retains id-carrying hits for 35 days and `plan_cap_verdict` applies the per-entry rule, keeping the verdict identical across every reader (`docs/port/wave9-consilium-defects.md`).
- **Wave 9f - the third acceptance pass (F78-1, F78-2)**: the cap detector takes the newest qualifying pair of in-period hits (a later isolated 429 no longer hides an established pair), and machine-health retention keeps id-carrying instant-429 cap evidence for 35 days regardless of its expired burst `until` with the 500-record cap evicting such evidence last, so a reader in another repository still sees the pair until the declared reset (`docs/port/wave9-consilium-defects.md`).
- **Wave 9g - the fourth acceptance pass (F80-1)**: `update_machine_health` now durably drops cap evidence superseded by a newer clearing success (a usable reply) before retention and the record cap run, so the asymmetric lifetimes of evidence (35 days, evicted last) and the success (24 h, evicted first) can no longer resurrect an obsolete refusal for a machine-only reader (`docs/port/wave9-consilium-defects.md`).
- **Wave 9h - the fifth acceptance pass (F82-1)**: the newest clearing success of an endpoint is now a durable anchor - kept 35 days and evicted only in the record cap's last tier, after cap evidence - so a success can no longer be discarded while its cap pair waits in a busy (exclusively held) journal; evidence arriving late from a skipped journal or a delayed retry is still superseded (`docs/port/wave9-consilium-defects.md`).
- **Wave 9i - the sixth acceptance pass (F84-1)**: exactly one clearing anchor per endpoint (the newest success, ties broken by record key) is protected, so redundant equal-time successes are ordinary rows and can no longer fill the cap's protected tier and evict another endpoint's only clearing proof (`docs/port/wave9-consilium-defects.md`).


## [0.2.1] - 2026-10-14

- Plugin: the coordinate skill carries the waiting rule (wakeups instead of blocking waits); the SessionStart hook prints it; README: the local-checkout install path.

### Fixed

- **Wave 8j - one total order per window (8i caveat)**: every quota instant comparison is made at whole-second granularity and a tie goes to the rank, with the legacy windows resolved from a separate slot, so the pick is the same for every candidate order (`docs/port/wave8-quota.md`).
- **Wave 8i - lossless quota instants (F62-1)**: the per-window instants keep their fractional seconds, a comparison involving a whole-second instant is made at second granularity (a tie goes to the rank), and a legacy record's unknown window instants never win on recency against a known one (`docs/port/wave8-quota.md`).
- **Wave 8h - per-window instants persisted (F60-1)**: the quota record (ledger entry, machine-wide health record) carries `primary_observed_at` / `secondary_observed_at` for each present window, read by the recorded selection; the aggregate `observed_at` is only the fallback for older records (`docs/port/wave8-quota.md`).
- **Wave 8g - per-window observation instants (F58-1)**: each retained quota window keeps its own observation instant through the rollout, account and recorded/live aggregation, so a partial newer observation can no longer promote an older window (`docs/port/wave8-quota.md`).
- **Wave 8f - account cache and fail-closed windows (F55-1, F55-2)**: the account cache keeps the raw rollout observations and the selection runs at each lookup's instant; a newer observation replaces a window only where its own is valid (`docs/port/wave8-quota.md`).
- **Wave 8e - quota clock and finite percentages (F52-1, F52-2)**: one comparison instant per aggregation (sampled at the entry point, passed down) and a non-finite or out-of-range percent is an absent window, so neither a reset crossing mid-fold nor a "NaN" share changes the pick (`docs/port/wave8-quota.md`).
- **Wave 8d - quota tie order (F50-1)**: tied per-window observations are ranked by one total order - a future reset beats a past one beats a missing one, then the higher share, then the later reset - so the result no longer depends on the order (`docs/port/wave8-quota.md`).
- **Wave 8c - quota merge per window (F48-1)**: at an equal `observed_at` the endpoint window (recorded, live, sub-agent and rollout aggregation) merges per window - the higher primary and, independently, the higher secondary - so a blocking weekly window is never hidden (`docs/port/wave8-quota.md`).
- **Wave 8b - quota tie (F46-1)**: the endpoint window selection (recorded and live) picks the newer `observed_at`, and at an equal instant the fuller primary (then secondary) window, so a 30 % observation can no longer hide a 99 % one (`docs/port/wave8-quota.md`).

### Added

- **Wave 8 - quota windows** (`docs/port/wave8-quota.md`; the plugin's wave 30 / TECH_DEBT T15 of
  its `[Unreleased]` 0.6.2, `bade7cb..a162b8d`): after every codex run the ledger records `quota`
  `{primary, secondary, observed_at}`, `tokens` `{input, cached, output, thread_input, calls,
  max_input, context_window, subagents {count, input, calls}}`, `subagents_allowed` and `budget`
  after `commit_wait_ms`, read from the thread's rollout and its sub-agent rollouts
  (`c3_core::quota`); the endpoint health takes the newest observed window (ledgers, machine-wide
  records - which now carry `quota`, a run without an outcome record writing class `""` -, the
  codex home's newest rollout for the built-in openai endpoint): >= 98 % with its reset ahead is
  `unavailable (5-hour window 99 % used, resets <iso>)` like a usage limit, >= 80 % says `low
  quota: ...`, a past reset shows `5h reset (last 96% until 18:32)`; `c3 providers` has a QUOTA
  column and `--json` rows a `quota` object; `--budget <percent>` (env
  `CODEX_CONSULT_QUOTA_BUDGET`, default 90, 0 off) stops the main turn at the budget (partial kept,
  no continuation, class operator, telemetry class quota; detached runs and panel members the
  same); the replay guard `CODEX_CONSULT_FORK_MAX_KB` (default 512; warned from 128 KB) and the
  partial's `--mode new` hint; `-c features.multi_agent=false` on every codex turn unless the
  user's config names `features.multi_agent`; the reading discipline in the purpose paragraphs of
  acceptance, diff-review, checkpoint and core-contract; the dry run's `quota`, `budget`,
  `sub-agents` and `replay` lines; the consult shim forwards `-Budget`.

## [0.2.0] - 2026-10-14

The compatibility release: parity with claude-codex-consult 0.6.1, measured by the plugin's pinned
v0.6.1 harnesses through the shim (`docs/port/harness-shim.md`).

### Added

- Wave 1 (`wave1-parity.md`): the 0.6.1 rules of wave 28 - the time-only reset parse, the Codex
  app-server exclusion, the panel light, the re-read anchor.
- Wave 1b (`wave1b-plan.md`): the roster `plan` - quota propagation, the scheduling group and the
  machine-wide plan wait as shared code (F02-7).
- Wave 1c: F04-1..F04-3 - a Unicode-safe `.exe` strip, DST edges by a UTC round trip, the panel
  records from the final routing.
- Wave 2 (`wave2-telemetry.md`): 2a `consult_ref`, the rating judge and `rating_rev`, the backfill,
  the ledger order; 2b a durable telemetry outbox and the forget-me retry identity (F02-1, F02-3);
  2c the closed telemetry classes and the telemetry shim (F02-2, F02-4); C3 keeps its own app id,
  salt and outbox (decision P7).
- Wave 2b (`wave2b-compat.md`): the pre-0.6 gaps of the RC2 triage - the test-mode warning and
  the child scrub, the launcher quoting, the health journal, kick during a timeout continuation,
  the pending outcome, the store rename retry, the member step cap, the launch-mark hooks, STREAM
  D6, the hook's pointer line and `--explain`, `--require` on a single run, the role and topic
  validation, the shim fixes.
- Wave 3a (`wave3a-recovery.md`): the recovery hardening of the plugin's wave 28e (E1, E18, E19,
  E23, E25, E28) and F06-1.
- Wave 3b (`wave3b-notspooled.md`): one not-spooled file per producer, the fold that saves its
  record before it deletes, the legacy staging, the forgetting marker on pid and start ticks (E2,
  E3, E20, E24, E26; decision P8, the test-mode plugin-home switch).
- Wave 4 (`wave4-claude.md`): the `claude` engine - roster keys, the model table, the preflight,
  the turn, the panel seat, the endpoint mode; the not-spooled warning says "- dropped".
- Wave 5 (`wave5-leftovers.md`): `--brief-prefix` (`CODEX_CONSULT_BRIEF_PREFIX`, the plugin's
  wave 27c D6) with the reply-prefix and slug refusals and the dry run's `brief prefix:` line; the
  telemetry sender's intake handling - every batch of at most 100, the 429 `Retry-After` rule, the
  400 `events[i]` drop, the 413 halving, the 403 stop, the 60 s flush deadline and the 8 s request
  bound, the record's `rejected[]` and the plugin's result line; the sender lock's owner record
  (`flush.owner.json`) behind "sender busy" / "sender stuck" in the refusal, the notes and
  `--status`.

### Fixed

- Wave 2d: F09-1..F09-6 - the deletion transaction, the reread rule, closed legacy events, one
  coordinator resolver.
- Wave 2e: F11-1..F11-5 (the wave 2b review).
- Wave 2f: F14-1 - the local-only forget records its cleaning transaction before the first
  removal (F14-2 stays as the plugin: the provider match is case-sensitive).
- Wave 2g: F19-1..F19-3 - the complaint joins the deletion protocol, an unreadable salt never ends
  a cleanup, every outbound event is re-serialised.
- Wave 2h: F22-1 - a kept health journal lists its applied record keys, so a replay after an
  eviction never applies a record twice.
- Wave 3c: F23-1..F23-5 - the recovery invariant fail-closed (a failed lookup is never "gone", an
  unreadable start time never skips a scan row, the kept evidence is written at the kill).
- Wave 3d: F24-1..F24-6 (the wave 3b review).
- Wave 4f: F25-1..F25-4 - the endpoint launcher probe, the transcript guard in every auth mode, the
  plugin's model case rule, a redacted child-environment debug print.
- Wave 5: `CODEX_CONSULT_COORDINATOR` is refused with the plugin's character rule
  (`Get-IdentityStringProblem`: "the provider 'open::ai' must not contain '::'") instead of a
  provider-label pattern the plugin does not have; interior blanks are accepted as the plugin does.
  The coordinator's dry-run line and the "not in the roster" console line follow the plugin's
  `Format-CoordinatorText` / `Format-CoordinatorId` (no ` [codex]` suffix, "(model not named)", an
  unresolved `#n` shown as such).
- Wave 6 (`wave6-live-diffs.md`, the RC3 live matrix): the ledger's `coordinator` record carries
  `host_by`, `in_roster` and `unresolved` in the plugin's order, and a rewrite of `sessions.json`
  keeps every entry it read verbatim (key order, explicit nulls) apart from what C3 changed in it.
- Wave 6: the `c3 providers` header counts this repository's consultations only, not the
  machine-wide health records.
- Wave 6: the agy and muse engines' reviewer line (dry run, reply header) is `engine <name>
  (<launcher>)` as the plugin's, not the codex endpoint text.
- Wave 6: telemetry is sent by a detached `c3 telemetry --flush` started right after a
  consultation's, a panel's, a rating's or a backfill's commit (not waited for, allow-listed
  environment), not at the start of the next run.
- Wave 7 (`wave7-acceptance-fixes.md`), acceptance round 1 fixes: F36-1, F36-2, F30-2 failure
  path, F29-1/F32-2 documented. F36-1 - a repository-relative path strips the root char by char
  (a Unicode `CLAUDE_CONFIG_DIR` outside the repository no longer panics the Claude transcript
  guard; Windows folds every letter as `OrdinalIgnoreCase`). F36-2 - the guard compares the NFC
  form Claude Code writes under as well as the path as written, the root likewise; any form inside
  refuses. F30-2 - the write before a kill reports its result to the killer; a failed one is
  retried once into `.consult.pending-kill.json`, which the recovery reads as a record; both
  failing, the kill still runs, not confirmed, and the next run is refused from the launch record
  and its fail-closed scans. F29-1 (the shared journal with plugin 0.6.1) and F25-1/F32-1/F32-2
  (what the endpoint preflight and the transcript guard cannot see) are stated as known limits.
- Wave 7b (`wave7-acceptance-fixes.md`, "7b"), acceptance round 2 (F38-1): no kill without a
  record - the record before a kill goes to the recovery record, else its fallback, else a third
  location outside the collab directory, `<codex home>/c3/pending-kill/<task>-<run id>.json`, which
  the recovery reads and consumes as the fallback (and the end of a run that writes the record
  clean removes); all three failing, the tree is left alive, the run ends failed naming its root
  and the three errors, and the next run is refused from the launch record while it lives.
- Wave 7c (`wave7-acceptance-fixes.md`, "7c"), acceptance round 3 (F40-1, F40-2): the codex home's
  kill records that cannot be listed (the directory cannot be opened, an entry cannot be read) are
  an inspection failure, never "no records" - the consult and panel recovery, the dry run, the
  findings refusal and `-List` and the kick refuse naming the directory and the error (only an
  absent directory is empty); a kick whose kill did not happen (its record written nowhere) is
  acknowledged `not killed` - the kick names the live root and why, says the tree is left alive and
  the task refused, and exits 1.
- Wave 7d (`wave7-acceptance-fixes.md`, "7d"), acceptance round 4 (F42-1) and a sweep of the
  family: a recovery record - the record, its fallback, a codex home copy, the running record -
  whose metadata lookup, read or parse fails is an inspection failure, never an absent record (only
  `NotFound` is): every reader refuses naming the file and the error (`the recovery record <path>
  cannot be read (<error>): the task is refused until it can be read`; `-List` prints it as
  `pending:`, the kick exits 1); the task directory's own listing that fails refuses the same way;
  the record before a kill never replaces a record that cannot be read.

### Changed

- The workspace version is 0.2.0 (`c3 --version` prints `c3 0.2.0`); the Claude Code plugin and
  its marketplace entry carry 0.2.0.

## [0.1.0]

The parity port of the plugin's 0.5.x surface (milestones 1-6) and the subsystems after it: packs
and the `http` engine (milestone 7), the index (8), the router (9), the stdio MCP server (10) and
the index federation (11).
