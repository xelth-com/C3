# Handoff 18 - Grok (grok): reply-grok

Date: 2026-11-23 01:25 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (usable reply 21 min ago).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 6678f76a-c3c0-4e92-84ca-48a6aad0b42e.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-1a6b5cfce1814d1ea8fddd4b9050abf3.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-b6c098276ce246b5be9f79fa2db7f149` of revision e4f42a8da3b138e4061a9f8f76d977b1117ecadf (--rev e4f42a8da3b138e4061a9f8f76d977b1117ecadf; removed after the run).
Parent thread: (none - new thread). Result thread: `01a205bd-666f-7f52-a602-de1596f2618a` (source: events).
Brief: `.collab/release-0.2.5/handoffs/09-claude-diff-review-reviewed-revision.md` (sha256 05e9583d36cf). Reviewed: 4f43074, base 4f430740c2ce1936fdff3649d67ed21d7ca1e5d4, tree sha256 24740c634eea, 0 changed files.
Bridge outcome: usable reply. Wall time: 813.3 s. Tokens: in 348237 (cached 2205056), out 47750, reasoning 35158.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Warnings: the live collab directory changed during the run (1 file: .collab/release-0.2.5/handoffs/09-claude-diff-review-reviewed-revision.patch) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The sha is pinned and a destination link in the checkout is refused, but a --rev review is recorded as the live HEAD, staging follows source links, --diff can publish untracked secrets and mark a clean run warned, and worktree setup can rewrite the live index. Findings: 0 blocker, 6 major, 1 minor, 0 note (F18-1..F18-7, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/18-grok-reply-grok.reply.json`.
Raw event stream: `handoffs/18-grok-reply-grok.events.jsonl`.
Verbatim reply follows.

---

## Q1
A branch or tag is resolved with `rev^{commit}` to a full sha before `git worktree add`, and a panel passes that sha once (`panel/run.rs:2248`). A leading `-`, whitespace, or `..` is refused; a blob or tree fails `^{commit}`. The worktree sha in `extra.worktree` matches the checkout. The handoff `Reviewed:` line, ledger `base_commit` / `reviewed_revision`, and each finding's `source.base_commit` are still the live HEAD (`orchestrate/mod.rs:3806`, `ledger.rs:48`, `render.rs:632`). Those findings are stored on the live task and later reviews of HEAD judge them. If the planned sha is empty, create re-resolves `HEAD`. Ambiguous names fail `rev-parse`. A shallow clone is refused only when the object is absent.

## Q2
Staging copies a stageable brief, `state.md`, and `handoffs/` except names ending in `.events.jsonl`. A symlink, junction, or linked ancestor at the destination is refused (`worktree.rs:154`, `worktree.rs:217`). A source symlink or junction is followed: `handoffs.is_dir()` and `fs::copy` read the target, so an outside secret becomes a normal file in the worktree. Hard-link bytes are copied; a hard-linked destination is not treated as a link. `..` and absolute paths are not staged. Tracked paths inside the worktree are overwritten on purpose. The prompt says handoffs were copied whenever the collab path is stageable, even when `handoffs/` is absent (`worktree.rs:641`).

## Q3
The working-tree form sets `GIT_INDEX_FILE` to a temp copy; the unit test shows the real index bytes unchanged on that path. A leading dash cannot be a revision token, and the range is one argv after `diff`, so it is not `--output`. A single revision intent-to-adds every non-ignored untracked file (`reviewed.rs:639`), so a secret that is not gitignored is written into the patch and named in the prompt. The patch is written after `collab_before` (`mod.rs:3810` then `mod.rs:3988`). Two members can race on one patch path; each ledger stores the hash it rendered.

## Q4
`ReviewTree` drops the worktree on return, including a failed stage. A hard kill skips `Drop`. The `c3-kimi-wt-<32 hex>` name and owner file let `c3 branches` / `c3 doctor --fix` see a dead pid. The owner write is best effort. Removal unlinks links first and aborts if that pass fails or the root is a link. `create` runs `git status --porcelain` on the live repo with no `--no-optional-locks` (`worktree.rs:91`), so the live index can be refreshed and locked. A link created after the last unlink pass and before `remove_dir_all` is not covered. Ctrl-c unwind was not read.

## Q5
A live-tree or sibling change only warns. Kimi, and agy with a writing tool, fail on a change inside their own worktree. Claude, grok, and muse only warn, so a write that stays in the worktree is deleted with it and the reply stays usable. A write through a symlink is not a worktree change; the live change is labeled not the reviewer's and the reply stays usable (`tree.rs:193`). C3's own patch makes `tree_check` `warned` on a clean isolated `--diff` run; a live-tree agy reply with no tools and no structured parse is failed for that file. Two runs get distinct uuid directories.

## Q6
Http with `--rev` gets no worktree. The operator warning says the pack is the live tree; the model prompt does not. The frozen-HEAD prompt lists the first 20 uncommitted paths, including names outside the review (F11-5). MCP passes `rev` and `diff` as flag values; the CLI rejects a leading dash and whitespace before git. `C3_CHILD` does not suppress `--rev` or `--diff`. The changelog's untouched-index sentence is about the temp index, which that test supports; it does not mention `git status` in `create`. "Event streams left out" is only `*.events.jsonl`.

## Q7
Happy-path `--rev`, frozen HEAD, patch sha256, a quiet index-byte compare, and a reviewer-left junction are tested. Not covered: moving ref vs checkout, source symlink, symlink write, index lock, finding provenance, `--diff` vs `tree_check`, an untracked secret, the http pack, ctrl-c, a shallow clone, a hard link, and two members racing one patch. Not read: most test bodies, the http pack builder, signal handling, and the prior-finding files outside this diff.

## Requested checks
RC1. In a scratch repo, `stat` the index, run a fake-engine `c3 consult --rev HEAD` that creates a worktree, `stat` again. Workspace-write, 2 min. Finding #2: mtime changes or `index.lock` appears.
RC2. Same repo, review an old sha and print the ledger `worktree.sha`, `base_commit`, and `findings.json` `source.base_commit`. Read-only after the run, 2 min. Finding #1: finding commit equals live HEAD, not the old sha.
RC3. Claude `diff-review` with `--diff HEAD` on a clean tree plus one untracked `secret.txt` not in gitignore. Workspace-write, 2 min. Finding #3 and #4: `tree_check.outcome` is `warned` and the patch contains `secret.txt`.
RC4. Make `handoffs` a symlink to a directory outside the repo that holds `sentinel.txt`, run an isolated consult. Workspace-write, 2 min. Finding #5: the worktree copy contains the sentinel bytes.
RC5. Commit a symlink `out` pointing at a file in the main checkout; fake the reviewer writing through `out`. Workspace-write, 2 min. Finding #6: the target changes and the ledger outcome stays usable.

---

### Findings

- **F18-1** [major] `crates/c3/src/consult/orchestrate/mod.rs:3806`, `crates/c3/src/consult/orchestrate/ledger.rs:48`, `crates/c3/src/consult/orchestrate/ledger.rs:251`, `crates/c3/src/consult/orchestrate/render.rs:632` - Under --rev the reviewer reads the pinned worktree sha, but the handoff Reviewed line, the ledger base_commit and reviewed_revision, and every new finding's source.base_commit are the live HEAD taken before the run. Those findings are stored on the live task and a later review of HEAD treats them as findings about HEAD. Trigger: c3 consult --rev <old-sha> on a dirty live checkout whose HEAD is a different commit, and the reply files a finding. Evidence: read-code: ctx.revision is revision_info of the live repo_root, not of the worktree.; read-code: build_finding copies ctx.revision.base_commit and tree_sha256 into source and the first history event.; read-code: The Reviewed line prints ctx.revision.reviewed_revision and base_commit.; read-code: Members receive the resolved sha as rev, so the worktree itself is pinned; only the recorded provenance stays on the live HEAD. Verify: RC2. Remedy: Stamp finding source, history, and the Reviewed line with the worktree sha when isolation is present, and keep the live fingerprint in a separate field.
- **F18-2** [major] `crates/c3/src/engines/worktree.rs:91` - Creating a review worktree runs git status --porcelain on the live repository with no --no-optional-locks and no GIT_OPTIONAL_LOCKS=0. That refresh can lock and rewrite the live index while a consult is starting. Trigger: Any kimi run, claude diff-review or acceptance, or --rev consult, while the live index stat cache is stale or another git process wants the index. Evidence: read-code: create calls git_capture(repo, ["status", "--porcelain"]) on the live repo before worktree add. git_capture does not set GIT_OPTIONAL_LOCKS.; read-code: The later uncommitted-path status does pass --no-optional-locks; the create call does not. Verify: RC1. Remedy: Pass --no-optional-locks on that status, or drop it and use the already lock-avoiding uncommitted_paths result.
- **F18-3** [major] `crates/c3/src/consult/orchestrate/mod.rs:3810`, `crates/c3/src/consult/orchestrate/mod.rs:3988`, `crates/c3/src/consult/reviewed.rs:675`, `crates/c3/src/consult/orchestrate/tree.rs:234` - The --diff patch is written into the live handoffs after collab_before is taken, and the patch path is not on the run's ignored handoff prefix. An isolated run that was otherwise clean is recorded tree_check warned. A live-tree agy reply with no tool call and no structured parse is failed for that write. Trigger: c3 consult --diff HEAD on a claude diff-review, or on agy with no --rev, when the reply has no tool call and does not parse as structured. Evidence: read-code: collab_before is snapshotted before prepare_review_inputs.; read-code: write_patch runs inside prepare_review_inputs, after that snapshot.; read-code: A collab path outside the reply prefix becomes a failure reason, or only a warning when the run is worktree-isolated.; read-code: agy warns on a change only when the reply is complete or the log names a tool; otherwise the change fails the run. Verify: RC3, and repeat on agy with a no-tool unstructured reply. Remedy: Take collab_before after the patch write, or add the patch path to the ignore prefixes.
- **F18-4** [major] `crates/c3/src/consult/reviewed.rs:615`, `crates/c3/src/consult/reviewed.rs:639` - A single-revision --diff marks every non-ignored untracked file intent-to-add and puts its contents in the patch that the prompt tells the reviewer to read. A secret that is untracked but not gitignored leaves the machine with a remote reviewer. Trigger: c3 consult --diff HEAD while an untracked file such as secret.txt is not matched by exclude rules, .collab, or .eck. Evidence: read-code: WorkingTree runs ls-files --others --exclude-standard and git add --intent-to-add on that list, then git diff of that revision.; read-code: Excludes are only .collab, .eck, and the collab directory.; read-code: The prompt names that patch as the change under review. Verify: RC3. Remedy: Intent-to-add only paths the operator named, or require an explicit path list before untracked contents are written into the patch.
- **F18-5** [major] `crates/c3/src/consult/reviewed.rs:299`, `crates/c3/src/engines/worktree.rs:161`, `crates/c3/src/engines/worktree.rs:169` - Staging follows a symlink or junction used as the brief or as handoffs/. is_dir and fs::copy read the target, and the bytes are written into the worktree as a normal file. A link in the checked-out revision at the destination is refused, but the source link is not. Trigger: The live handoffs directory, or the brief path, is a symlink or junction to a directory or file outside the repository. Evidence: read-code: Staging runs only if handoffs.is_dir(), which follows a directory symlink or junction, then stage_dir walks that target.; read-code: stage_file copies with fs::copy, which follows a symlink at the source.; read-code: Nested links inside the walked directory are skipped; the top-level follow has already happened. Verify: RC4. Remedy: Open the brief and handoffs with symlink_metadata, refuse a link, and copy with a non-following read.
- **F18-6** [major] `crates/c3/src/consult/orchestrate/tree.rs:193`, `crates/c3/src/consult/reviewed.rs:361` - A reviewer that writes through a symlink checked out in the throwaway worktree changes the link target, which can be the live checkout. changes() only sees files in the worktree, and a live-tree change is recorded as not the reviewer's, so the reply stays usable. Trigger: The reviewed commit contains a symlink whose target is a file or directory in the live checkout, and the reviewer writes through that symlink. Evidence: read-code: changes() compares git content and the staged collab snapshot inside the worktree only.; read-code: When the run is isolated, a live working-tree change is pushed only onto main_warn with NOT_THE_WORKTREE.; read-code: Warnings do not clear a usable reply; only check.problem does. Verify: RC5. Remedy: Treat a live-tree change that touches a path reached by a worktree symlink as the reviewer's write, and fail the engines that must not write.
- **F18-7** [minor] `crates/c3/src/engines/worktree.rs:641`, `crates/c3/src/consult/reviewed.rs:279` - The --rev prompt says the task handoffs were copied whenever the collab path is stageable, including when handoffs/ does not exist. Staging also skips only filenames ending in .events.jsonl, so a differently named event stream is copied in. Trigger: A --rev consult whose task directory has no handoffs directory, or a handoff file named events.jsonl rather than *.events.jsonl. Evidence: read-code: rev_prompt_line appends the copied-handoffs sentence from task_rel alone.; read-code: The directory is staged only when handoffs.is_dir().; read-code: skip_staging matches only a filename that ends with .events.jsonl. Verify: Run --rev in a task with no handoffs directory and read the prompt; add handoffs/events.jsonl and see it in the worktree. Remedy: Claim the copy only after staging succeeds, and skip every events stream the engines write.

### Prior findings

- F02-1 - not-checked - Not in this diff; repo_models was not re-read.
- F02-2 - not-checked - Not in this diff; repo_models was not re-read.
- F02-3 - not-checked - Not in this diff; repo_models was not re-read.
- F02-4 - not-checked - Not in this diff; repo_models was not re-read.
- F02-5 - not-checked - Not in this diff; auth and maintain launch were not re-read.
- F02-6 - not-checked - Not in this diff; hook git calls were not re-read.
- F02-7 - not-checked - Not in this diff; branch description parsing was not re-read.
- F02-8 - not-checked - Not in this diff. A new live-index status is finding #2, not this site.
- F02-9 - not-checked - Not in this diff; default-branch discovery was not re-read.
- F02-10 - not-checked - Not in this diff; cherry content check was not re-read.
- F02-11 - not-checked - Not in this diff; user worktree prefix filtering was not re-read.
- F02-12 - not-checked - Not in this diff; catalog redirects were not re-read.
- F02-13 - not-checked - Not in this diff; catalog body limits were not re-read.
- F02-14 - not-checked - Not in this diff; catalog dates were not re-read.
- F02-15 - not-checked - Not in this diff; endpoint model lists were not re-read.
- F02-16 - not-checked - Not in this diff; smoke binding was not re-read.
- F02-17 - not-checked - Not in this diff; roster writer was not re-read.
- F02-18 - not-checked - Not in this diff; maintain --models was not re-read.
- F02-19 - not-checked - Not in this diff; telemetry catalogs were not re-read.
- F02-20 - not-checked - Not in this diff; Claude-shaped telemetry was not re-read.
- F02-21 - not-checked - Removal now lives in worktree.rs and aborts when the unlink pass fails; the old kimi.rs site was not compared line by line.
- F03-1 - not-checked - child_environment was not re-read. This diff does not gate --rev on C3_CHILD.
- F03-2 - not-checked - The moved unlink walk treats Windows reparse points as links; the old junction case was not re-run.
- F03-3 - not-checked - Not in this diff; branch config keys were not re-read.
- F03-4 - not-checked - Not in this diff; model-line parsing was not re-read.
- F03-5 - not-checked - Not in this diff; telemetry suffix case was not re-read.
- F03-6 - not-checked - Not in this diff; branch listing cost was not re-read.
- F04-1 - not-checked - Not in this diff. Finding #2 is a new status call in worktree create.
- F04-2 - not-checked - Not in this diff; swap date gate was not re-read.
- F04-3 - not-checked - Not in this diff; catalog created stamps were not re-read.
- F04-4 - not-checked - Not in this diff; doctor --fix child guard was not re-read.
- F04-5 - not-checked - Current remove_worktree_dir returns before --force when the unlink pass fails; the Windows hold-open case was not run.
- F04-6 - not-checked - Not in this diff; auth and detached maintain env were not re-read.
- F04-7 - not-checked - Not in this diff; default-branch fallback was not re-read.
- F04-8 - not-checked - Not in this diff; telemetry shape and catalog caps were not re-read.
- F04-9 - not-checked - Not in this diff; --clear description handling was not re-read.
- F10-1 - not-checked - Not in this diff; agy classifier was not re-read.
- F10-2 - not-checked - Not in this diff; agy ACTIVE/DONE merge was not re-read.
- F10-3 - still-open - A brief that is committed at the rev is still admitted, and a later stage failure is a capability error. is_preflight_refusal does not match that text, so the panel does not re-seat.
- F10-4 - not-checked - Not in this diff; findings snapshot races were not re-read.
- F10-5 - not-checked - Not in this diff; smoke sub-agent checks were not re-read.
- F10-7 - not-checked - Not in this diff; panel death records were not re-read.
- F11-1 - not-checked - Not in this diff; agy write classification was not re-read.
- F11-2 - not-checked - Bound-input drift during an agy run was not re-read beyond the new worktree warning path.
- F11-3 - fixed - stage_file refuses a symlink or junction at the destination or an ancestor. Source links are a separate defect (finding #5).
- F11-4 - still-open - A path with .. is now refused. A destination that is a directory, or a source that disappears after the preflight, still fails as capability and is not re-seated.
- F11-5 - still-open - uncommitted_paths still feeds the first 20 live names into the frozen-HEAD prompt, now also for claude diff-review and acceptance.
- F11-6 - not-checked - Not in this diff; panel acceptance vs shown blockers was not re-read.
- F11-7 - not-checked - Not in this diff; smoke sub-agent pass was not re-read.
- F11-8 - not-checked - Not in this diff; rollout parsing was not re-read.
- F11-9 - not-checked - Not in this diff; spawn attribution was not re-read.
- F11-10 - not-checked - Secondary prompts were not re-read for the role constraint. They do use engine_cwd.
- F11-11 - not-checked - Not in this diff; death-record stream identity was not re-read.
- F11-12 - not-checked - Not in this diff; providers reply_outcomes was not re-read.
- F12-1 - not-checked - stale_verdict still calls pid_alive; process_start_iso failure behavior was not re-read.
- F12-2 - not-checked - Not in this diff; resolve_repo_root env was not re-read.
- F12-3 - not-checked - Not in this diff; OneDrive reparse classification was not re-read.
- F12-4 - not-checked - Not in this diff; description write race was not re-read.
- F13-1 - fixed - If the worktree path itself is a link, unlink_links_inside_with errors and remove_worktree_dir returns before git worktree remove or remove_dir_all.
- F13-2 - not-checked - Not in this diff; .git walk for the hook was not re-read.
- F13-3 - not-checked - Not in this diff; BranchRecord::parse was not re-read.
- F13-4 - not-checked - Not in this diff; update and branches child mark were not re-read.
- F13-5 - not-checked - Not in this diff; hook deadline was not re-read.
- F13-6 - not-checked - Not in this diff; AGENTS.md numeric seating was not re-read.
- F13-7 - not-checked - Not in this diff; git cherry failure was not re-read.
- F13-8 - still-open - record_throwaway_owner still ignores write errors, and a missing owner still goes stale after 6 hours.
- F14-1 - not-checked - Not in this diff; catalog deadlines were not re-read.
- F14-2 - not-checked - Not in this diff; older-pair cap was not re-read.
- F14-3 - not-checked - Not in this diff; smoke roster symlink was not re-read.
- F14-4 - not-checked - Not in this diff; bound_roster types were not re-read.
- F14-5 - not-checked - Not in this diff; future catalog dates were not re-read.
- F15-1 - not-checked - Not in this diff; resolved peer checks were not re-read.
- F15-2 - not-checked - Not in this diff; older-pair eviction was not re-read.
- F15-3 - not-checked - Not in this diff; smoke roster position was not re-read.
- F15-5 - not-checked - Not in this diff; pending recovery version was not re-read.
- F15-6 - not-checked - Not in this diff; maintain off writes were not re-read.
- F16-1 - not-checked - Not in this diff; catalog host trust was not re-read.
- F16-2 - not-checked - Not in this diff; CI title forwarding was not re-read.
- F16-3 - not-checked - Not in this diff; CI deadline coverage was not re-read.
- F16-4 - not-checked - Not in this diff; default-branch CI line was not re-read.
- F16-6 - not-checked - Not in this diff; router explain was not re-read.
- F16-7 - not-checked - Not in this diff; lab identity was not re-read.
- F16-8 - not-checked - Not in this diff; weighty seat guarantee was not re-read.
- F16-9 - not-checked - Not in this diff; CI daily gate race was not re-read.
- F16-10 - not-checked - Not in this diff; hook script truncation was not re-read.
- F17-1 - not-checked - Not in this diff; CI displayTitle was not re-read.
- F17-2 - not-checked - Not in this diff; router explain seats were not re-read.
- F17-3 - not-checked - Not in this diff; weighty tier skip was not re-read.
- F17-4 - not-checked - Not in this diff; MiniMax lab split was not re-read.
- F17-5 - not-checked - Not in this diff; catalog host trust was not re-read.
- F17-6 - not-checked - Not in this diff; delegation rule rewrite was not re-read.
- F17-7 - not-checked - Not in this diff; CI default branch was not re-read.
- F17-8 - not-checked - Not in this diff; github.com remote match was not re-read.
- F17-9 - not-checked - Not in this diff; hook script bytes were not re-read.
- F17-10 - not-checked - Not in this diff; CI latest-run wording was not re-read.

## Verdict: HOLD

The sha is pinned and a destination link in the checkout is refused, but a --rev review is recorded as the live HEAD, staging follows source links, --diff can publish untracked secrets and mark a clean run warned, and worktree setup can rewrite the live index.

### Blockers

- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, not-checked) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, fixed) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, fixed) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.

### Unproven scenarios

- Whether a warm index makes the new git status a no-op, or it always takes index.lock.
- Ctrl-c and SIGKILL: Drop removal was read, the signal path was not.
- Http pack bytes under --rev were not read; only the operator warning and the empty isolation line were.
- A hard-linked destination appearing between checkout and fs::copy.
- A symlink created after the last unlink pass and before remove_dir_all.
- Shallow-clone and ambiguous-name behavior beyond the rev-parse arguments.

### First-run checklist (observable)

_(none)_
