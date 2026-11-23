# Changelog

All notable changes to C3 are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the versions follow
[Semantic Versioning](https://semver.org/). The specification of every compatibility wave is the
PowerShell plugin [`claude-codex-consult`](https://github.com/xelth-com/claude-codex-consult) at its
tag `v0.6.1`; the finding ids (Fnn-k, En, Dn) are those of the parity task's reviews and of the
plugin's own waves. Each wave's details are in `docs/port/wave*.md`.

## [Unreleased]

### Added

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
  section, taken from the embedded template, in the file's own line ending, through a temp file and a
  rename (a link and a hard-linked file are refused and reported; the file is read again immediately
  before the rename and left as it is when it changed - the moment between that re-read and the
  rename itself remains). `c3 doctor` recognises a C3-written rule by its title line alone (a file
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
- **A `--diff` patch never carries a secret off the machine (F19-2, F18-4, F20-6).** A one-revision
  `--diff` put every untracked, not-ignored file into the patch handed to the reviewer, a remote one
  included. Every patch now passes through the redactor packs for remote models use: a
  secret-shaped value is replaced by its `[REDACTED:<kind>]` marker, the rest is kept, the sha256 is
  of the file as written, the ledger's `diff` record gains `redactions` and the prompt says the
  patch is for reading and how many values were redacted. Untracked files stay in for the
  one-revision form (they are the change), but a file a `.eckignore` excludes, or a TRACKED file the
  repository's ignore rules match, never enters a patch of either form. Test: a patch with an
  OpenRouter-shaped key, an untracked note, an `.eckignore`d file and a force-added ignored file.
- **One patch file per content (F19-3, F20-3).** The patch was named after the brief only, so two
  runs (or two panel members) with different ranges wrote the same path and a ledger `diff.sha256`
  could describe bytes another run wrote. The file is now `<stem>.<sha8>.patch`, written by an
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
  before the write, and the write is skipped when it changed; the moment between that re-read and
  the write itself remains - a decided boundary (closing it needs a local process with write access
  to the repository, which can edit the files directly).
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
  F10-2).** The parameters of all events of one tool step (ACTIVE / DONE / ERROR, also when one of
  them carries no name) are merged, so an empty first object no longer hides the command a later
  event logs; a tool not on the reader allow-list counts as writing whatever its parameters are
  named, a reader whose parameters carry a command at any depth (or a command-shaped value outside
  a search term) too, and agy's Cascade-vocabulary readers (`list_dir`, `grep_search`,
  `find_by_name`, ...) joined the allow-list, so a pure read with them no longer fails a complete
  reply. The hand-over exemption needs a verified `finish` call (named `finish`, closed by a finish
  event), never a step index alone. A tree change agy's log does not explain is reported as
  "unattributed", never as someone else's; the integration test whose launcher writes the file
  itself now says so.
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
  the current store under the task lock.
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
  meanwhile failed the post-check. A tool step is now judged from agy's own tool log: it is a write
  only when it names a path, file, directory or command and is not a known reader, or when its
  parameters were not logged; the `finish` hand-over and a tool given no path or command are not.
  When the reply file and `reply.json` are complete and schema-valid, a tree or collab change the
  log does not explain is a warning (`... - reply kept`) and the ledger entry and scoreboard mark
  are written; a real writing call still fails the run. `c3 providers` now prints, per reviewer over
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
  computed from (a changed file is left as it is; an edit landing between that re-read and the
  rename itself is not seen); only then does the temporary file replace the original by rename. A file with more
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
