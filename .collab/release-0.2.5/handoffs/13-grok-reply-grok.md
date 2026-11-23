# Handoff 13 - Grok (grok): reply-grok

Date: 2026-11-23 00:28 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (usable reply 0 min ago).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 62a08f1c-9d67-4a61-bc87-160f87d0b9c8.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-66f828763f5f4db49a75d6eb1198c054.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-8354efbfa43946e184bbbc4bd2af9a92` of revision 23b14557cc4e2ad0b24e2d95d68d6a4029b9e795 (--rev 23b14557cc4e2ad0b24e2d95d68d6a4029b9e795; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20585-94a4-7390-b7b7-3875d47efe47` (source: events).
Brief: `.collab/release-0.2.5/handoffs/06-claude-diff-review-repair-branches.md` (sha256 c7fef837a89d). Reviewed: 4f43074, base 4f430740c2ce1936fdff3649d67ed21d7ca1e5d4, tree sha256 24740c634eea, 0 changed files.
Bridge outcome: usable reply. Wall time: 1033.4 s. Tokens: in 164600 (cached 2758528), out 71080, reasoning 58031.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - doctor --fix can treat a temp path that is a link as a stale kimi worktree and force-remove it, and the SessionStart hook can rewrite model files under a directory that only has a stray .git entry. Findings: 1 blocker, 3 major, 5 minor, 0 note (F13-1..F13-9, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/13-grok-reply-grok.reply.json`.
Raw event stream: `handoffs/13-grok-reply-grok.events.jsonl`.
Verbatim reply follows.

---

## Q1
The repair writes a sibling temp file, syncs it, reads it back, rechecks links and the snapshot, then renames (`crates/c3/src/setup/repo_models.rs:324-365`). A second hard link at that recheck is refused. Quotes, a trailing comment, a BOM, and CRLF outside the value span stay (`crates/c3/src/setup/doctor.rs:99-172`). Non-UTF-8 files are skipped.

`find_repo_root` (`repo_models.rs:481-488`) treats any `.git` entry as a repository. The hook (`:531-540`) then rewrites models there and does not ask git. A crash after the temp file is created (`:312`) and before `remove_file` (`:318`) leaves `.name.c3-repair-*.tmp`. The recheck (`:356`) and `rename` (`:364`) are separate calls, so a replaced ancestor can still redirect the write. Unix copies mode bits; Windows ACLs on the old file are not copied.

## Q2
An agent pin changes only for a claude-table id or `claude-(fable|opus|sonnet|haiku)-<digits>[-<digits>]`, and `[1m]` is kept (`doctor.rs:201-232`). `opus[1m]`, `claude-opus-foo`, and `CLAUDE-OPUS-4-8` stay. Only an unindented `model` key is used (`doctor.rs:99-141`), so a body or indented line is not edited. A folded YAML value is missed.

`AGENTS.md` changes only when exactly one active, unprotected, not-on-request roster id is a successor (`repo_models.rs:112-158`). That test is numeric (`crates/c3/src/maintain/version.rs:251-269`), so a sole roster id `grok-4.20` replaces `grok-4.7`.

## Q3
`C3_CHILD=1` is set on the engine launcher (`crates/c3/src/engines/mod.rs:54`), the claude allow list (`crates/c3-core/src/claude.rs:503`), auth probes (`crates/c3/src/engines/claude_auth.rs:105`), smoke (`crates/c3/src/maintain/env.rs:124`), and detached maintain (`crates/c3/src/maintain/mod.rs:1627-1639`, Windows block `:1761`). `0`, `false`, `off`, and `no` do not mark (`engines/mod.rs:74-80`). The hook, model repair, and stale removal honor the mark (`crates/c3/src/cli/doctor.rs:119`).

`c3 update` runs `claude plugin update` through `stream` (`crates/c3/src/update/mod.rs:798-800`, `:867`) without the mark, so that process can still repair. `c3 branches --set` and `--clear` never check the mark (`crates/c3/src/setup/branches.rs:1186`).

## Q4
`--set` and `--clear` use NUL-framed `git config --local -z --get-all`, refuse non-UTF-8 and multiple values, and re-read before writing (`branches.rs:1109-1257`). `c3: hello` is kept. `foo.bar` addresses `branch.foo.bar.description`. Two processes can both pass `unchanged_since` (`:1168`).

`BranchRecord::parse` (`:135-148`) accepts any third field. `c3: work | alice | tomorrow | keep me` is therefore C3's record: `--set` replaces it and `--clear` deletes it. `write_config` (`:1137`) passes the value with no `--`.

## Q5
Listing, doctor, the hook, and the finish note set `GIT_OPTIONAL_LOCKS=0` (`branches.rs:197-200`). The hook and finish note do not run `status` or `cherry`. `c3 branches` and `c3 doctor` still run `git status` for the dirty count. Refs are `refs/heads/<name>`. No default, or a failed ancestry query, leaves `merged` null (`:532-534`). Detached HEAD is not the default. Cherry runs for at most 50 unmerged branches.

A failed cherry still sets `patches_checked` (`:645`) while the result stays null. The 2s deadline (`crates/c3/src/hook/mod.rs:91-104`) stops waiting and does not kill git. With no `origin/HEAD` and no `init.defaultBranch`, local `main` still beats `develop` (`branches.rs:285-294`).

## Q6
Only a direct child of the temp directory named `c3-kimi-wt-` plus 32 lowercase hex digits is C3's (`branches.rs:745-767`). Staleness uses `std::fs::metadata` (`:825`), which follows a symlink or junction. No owner file and an age past 6 hours is enough even while a review runs (`:816-818`). `doctor --fix` then calls `remove` (`crates/c3/src/engines/kimi.rs:211-224`): best-effort unlink, `git worktree remove --force`, then `remove_dir_all`.

README:668-672 and CHANGELOG:174 say a live run is never a finding and a forced removal cannot empty a linked checkout. CHANGELOG:27 says every child is marked. CHANGELOG:35 still says cherry reports content on the default branch; the code reports patch equivalence.

## Q7
Not run here: the claimed 1331 tests, a real SessionStart, a real foreign description, a crashed kimi worktree, and the stale-index byte comparison. Also open: git's handling of a config value that starts with `-`, and whether `claude plugin update` fires SessionStart.

## Requested checks
RC1. Finding #1. Workspace-write, 10 min, disposable repo: point `$TEMP/c3-kimi-wt-<32 hex>` at a sentinel directory older than 6 hours via a junction or symlink, register it as a worktree, run `c3 doctor --fix`. Settled if the sentinel file is gone, or if it remains.
RC2. Finding #2. Workspace-write, 5 min: a non-repo directory with a regular file `.git` and `AGENTS.md` model `GLM-5.3`, roster successor `glm-5.4`, cwd there, `c3 hook`. Settled if `AGENTS.md` changes to `GLM-5.4`.
RC3. Finding #3. Workspace-write, 5 min: set `branch.topic.description` to `c3: work | alice | tomorrow | keep me`, run `c3 branches --clear topic`. Settled if that text is gone.
RC4. Finding #4. Workspace-write, 10 min: fake `claude` on PATH logs `C3_CHILD` and exits 0; run `c3 update --apply` in a repo with a stale pin. Settled if the log lacks `C3_CHILD=1` or the pin changes.
RC5. Finding #5. Workspace-write, 1 min: shim `git` so `for-each-ref` sleeps 30s, run `c3 hook`, then list processes. Settled if git is still running after the hook returns.

---

### Findings

- **F13-1** [blocker] `crates/c3/src/setup/branches.rs:825`, `crates/c3/src/setup/branches.rs:758`, `crates/c3/src/setup/branches.rs:849`, `crates/c3/src/engines/kimi.rs:211`, `README.md:668`, `CHANGELOG.md:174` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target. Trigger: A worktree path directly under the system temp directory has the exact kimi name, the path is a symlink or junction to another directory that is older than 6 hours, and that target has no readable c3-owner file. c3 doctor --fix is run in the repository that lists the worktree. Evidence: read-code: is_c3_throwaway_worktree checks only the parent directory and the exact file name. throwaway_stale_reason uses std::fs::metadata, which follows links, and treats a missing owner plus age over STALE_AFTER as stale.; read-code: remove always calls unlink_links_inside, then git worktree remove --force, then remove_dir_all, and ignores failures of the unlink pass.; read-code: Both say a forced removal cannot empty a checkout a link points into, and that a live run's worktree is never a finding. Verify: In a disposable repository, register a worktree at $TEMP/c3-kimi-wt-<32 hex> that is a junction or symlink to a sentinel directory older than 6 hours, run c3 doctor --fix, and see whether the sentinel file still exists. Remedy: Before removal, require symlink_metadata to show a real directory, require the .git gitdir file to point at this repo's worktrees admin directory, and abort the whole removal if any link cannot be unlinked. Do not call remove_dir_all on a path that fails those checks.
- **F13-2** [major] `crates/c3/src/setup/repo_models.rs:481`, `crates/c3/src/setup/repo_models.rs:531`, `CHANGELOG.md:25` - The SessionStart repair treats any directory that contains a .git entry as the repository root. A stray file, a broken symlink, or a non-repository .git is enough, and the hook then rewrites AGENTS.md and .claude/agents/*.md there. Trigger: c3 hook runs with cwd inside a directory that is not a git work tree but has an ancestor containing a non-git .git entry, and a model pin there matches the rewrite rules. Evidence: read-code: find_repo_root returns the first ancestor for which symlink_metadata of .git succeeds, with no check that the entry is a git directory or a gitdir file.; read-code: hook_line uses that root and calls refresh_repo_models. It does not run git rev-parse. Verify: Create a non-repo directory with a regular file named .git and an AGENTS.md model GLM-5.3, point the roster at glm-5.4, and run c3 hook with that cwd. Compare AGENTS.md before and after. Remedy: Accept .git only when it is a directory or a gitdir file whose target resolves inside a real git common dir. Otherwise skip the repair.
- **F13-3** [major] `crates/c3/src/setup/branches.rs:135`, `crates/c3/src/setup/branches.rs:1186`, `crates/c3/src/setup/branches.rs:1238` - BranchRecord::parse treats any four-field line c3: <state> | <owner> | <anything> | <text> as C3's record. A foreign note whose third field is not a YYYY-MM-DD date is replaced by --set and deleted by --clear. Trigger: The branch description is the single line c3: work | alice | tomorrow | keep me, and the operator runs c3 branches --set or --clear on that branch. Evidence: read-code: parse requires a known state and four pipe fields, then stores the date with trim only. There is no calendar check.; read-code: A parsed description contributes only the bytes after the first newline as the foreign part. A one-line note therefore has no foreign part, so --set writes a new record and --clear unsets the key. Verify: Set branch.topic.description to c3: work | alice | tomorrow | keep me and run c3 branches --clear topic. Read the key back with git config --local -z --get-all. Remedy: Accept a record only when the date field matches YYYY-MM-DD. Anything else stays foreign and is preserved byte for byte.
- **F13-4** [major] `crates/c3/src/update/mod.rs:798`, `crates/c3/src/update/mod.rs:867`, `crates/c3/src/setup/branches.rs:1186`, `CHANGELOG.md:27` - C3_CHILD=1 is not set on every child C3 starts. c3 update launches claude plugin update with the inherited environment, so a SessionStart hook in that process can still repair the repository. A child can also rewrite or clear branch descriptions because --set and --clear do not check the mark. Trigger: c3 update --apply runs while claude is on PATH, or a process that already has C3_CHILD=1 runs c3 branches --set or --clear. Evidence: read-code: stream builds Command::new(program) with args and optional cwd only. PluginUpdate calls stream("claude", ["plugin", "update", "c3@c3"], None).; read-code: The child guard covers refresh_repo_models and remove_stale_worktrees. set_state and clear_state have no in_c3_child check.; read-code: The changelog says every child C3 starts is marked, listing reviewer CLIs, probes, and detached maintain, and that doctor --fix does nothing under the mark. Verify: Put a fake claude on PATH that prints C3_CHILD and exits 0, run c3 update --apply, and inspect that log. Separately run c3 branches --set with C3_CHILD=1 and see whether the description changes. Remedy: Set C3_CHILD=1 in update's stream for claude, and refuse --set, --clear, and --default inside a C3 child.
- **F13-5** [minor] `crates/c3/src/hook/mod.rs:91`, `crates/c3/src/hook/mod.rs:104` - The branch-line deadline stops waiting after 2 seconds and does not kill the scan thread or its git children, so a stalled git keeps running after c3 hook returns. Trigger: SessionStart runs the branch line and git for-each-ref, symbolic-ref, or rev-list is slower than 2 seconds. Evidence: read-code: line_within spawns a detached thread and returns None on recv_timeout. The thread and any Command::output git it started are not killed. On process exit those git processes are reparented and continue. Verify: Shim git so for-each-ref sleeps 30 seconds, run c3 hook, and list processes after the hook exits. Remedy: Track the git child pids and kill them when the deadline fires, before the hook returns.
- **F13-6** [minor] `crates/c3/src/setup/repo_models.rs:129`, `crates/c3/src/maintain/version.rs:251` - The AGENTS.md repair seats a numerically higher id with no catalog date gate. A sole followable roster model grok-4.20 is written over grok-4.7. Trigger: AGENTS.md frontmatter model is grok-4.7, no active followable entry equals that id, and exactly one followable entry has model grok-4.20. Evidence: read-code: Each roster id is passed alone to successor, and a single distinct successor is written back with the original prefix casing.; read-code: The first dotted number is major.minor, so 4.20 compares greater than 4.7. Stamped ids are skipped; grok-4.20 is not stamped. Verify: Call rewrite_agents_md_model on model grok-4.7 with roster [grok-4.20] and inspect the returned edit. Remedy: Do not treat a higher minor as a successor when a catalog date says it is older, or refuse known crossed pairs such as grok-4.20 versus grok-4.7.
- **F13-7** [minor] `crates/c3/src/setup/branches.rs:641`, `crates/c3/src/setup/branches.rs:645` - A failed git cherry still marks the branch patches_checked, with patches_on_default left null. The listing then omits the patches-not-checked phrase, so a failed check looks like a completed one with an unknown result. Trigger: c3 branches runs the patch check and git cherry exits non-zero or fails to spawn for an unmerged branch inside the cap of 50. Evidence: read-code: patches_on_default is the map of git_out, which is None on failure. patches_checked is then set true unconditionally.; read-code: The text adds patches not checked only when patches_checked is false. Verify: Force git cherry to exit 1 for one unmerged branch and inspect patches_checked and patches_on_default in c3 branches --json. Remedy: Set patches_checked only when both git calls succeed. On failure leave the branch unchecked and count it in patches_unchecked.
- **F13-8** [minor] `crates/c3/src/setup/branches.rs:816`, `crates/c3/src/engines/kimi.rs:185`, `README.md:670` - A live kimi review whose owner file was not written becomes a stale worktree after 6 hours, and c3 doctor --fix removes it. The owner write is best effort. Trigger: record_throwaway_owner returns without writing c3-owner, the worktree directory is older than 6 hours, and the review process is still running when doctor --fix runs. Evidence: read-code: The owner file is written with let _ = std::fs::write. A failed read of the gitdir file returns before any write.; read-code: With no owner record, age greater than STALE_AFTER is stale. A live pid is not consulted on that path.; read-code: The README says the worktree is never a finding while its run lives, then says no record and older than 6 hours is stale. Verify: Start a kimi worktree, delete its c3-owner file, set the directory mtime back 7 hours, and run c3 doctor --fix while the owner process is alive. Remedy: If the owner file is missing, do not remove the worktree until it is also absent from the live process table by command line, or until it is prunable because the directory is gone.
- **F13-9** [minor] `crates/c3/src/setup/repo_models.rs:356`, `crates/c3/src/setup/repo_models.rs:364` - After the last link and snapshot check, rename still uses a path. An ancestor directory replaced by a symlink or junction in that gap can make the replacement land outside the repository. A hard link created earlier no longer receives the new bytes, because rename replaces the directory entry. Trigger: A repairable agent file passes edit_refusal, and before std::fs::rename the parent .claude or .claude/agents directory is replaced by a link to another directory. Evidence: read-code: edit_refusal and the snapshot re-read happen, then std::fs::rename(tmp, edit.path) uses the path. There is no directory file descriptor or renameat.; inferred: Rename replaces the name. Another hard link of the previous inode keeps the old bytes, which closes the original hard-link write-through. Verify: In a fixture, pause between the recheck and rename, swap .claude/agents for a symlink to an outside directory, and compare the outside file with the new bytes. Remedy: Open the parent directory, recheck the file with openat and O_NOFOLLOW or FILE_FLAG_OPEN_REPARSE_POINT, and rename with renameat in that directory. Supersedes: F02-1.

### Prior findings

- F02-1 - still-open - In-place hard-link rewrite and truncation are gone. A link swapped after the recheck can still redirect rename. Narrowed by the new rename-race finding.
- F02-2 - fixed - The destination is replaced by rename after a temp write and a snapshot compare. A crash can leave the temp file; the original is not truncated.
- F02-3 - fixed - rewrite_agent_model accepts only a table id or the published shape and keeps [1m]. opus[1m] and claude-opus-foo are unchanged.
- F02-4 - fixed - AGENTS.md follows one unprotected, not-on-request entry. The numeric grok-4.20 case is a separate finding.
- F02-5 - fixed - Auth probes and detached maintain set C3_CHILD=1. The update launcher is a separate gap.
- F02-6 - fixed - The repair walks to .git and starts no process. The hook no longer waits past 2s; orphaned git is a new finding.
- F02-7 - fixed - NUL-framed read and byte-preserving foreign parts cover trailing CRLF, whitespace, and a non-record c3: line.
- F02-8 - fixed - Read-path git commands set GIT_OPTIONAL_LOCKS=0. The index-byte test was not executed in this review.
- F02-9 - fixed - No main, master, origin/HEAD, or init.defaultBranch reports the default unknown, and a failed ancestry query does not mark branches merged.
- F02-10 - fixed - The JSON field is patches_on_default and the listing text says patch equivalence. CHANGELOG:35 still says content.
- F02-11 - fixed - A user worktree is hidden only for the exact temp name. Removal of a link at that name is a new finding.
- F02-12 - not-checked - Catalog redirects are outside this repair and branches diff.
- F02-13 - not-checked - Catalog body caps are outside this diff.
- F02-14 - not-checked - Catalog date validation is outside this diff.
- F02-15 - not-checked - Per-endpoint model lists are outside this diff.
- F02-16 - not-checked - Smoke roster selection is outside this diff.
- F02-17 - not-checked - Roster extension writes are outside this diff.
- F02-18 - not-checked - C3_MAINTAIN_MODELS=off fetch behavior is outside this diff.
- F02-19 - not-checked - Telemetry disclosure of catalog model names is outside this diff.
- F02-20 - not-checked - Anthropic telemetry shaping is outside this diff.
- F02-21 - still-open - unlink_links_inside still skips errors and remove still force-deletes afterwards. This diff adds doctor --fix as another caller.
- F03-1 - fixed - child_environment sets C3_CHILD=1 itself, including when the parent value is 0.
- F03-2 - still-open - The walk still descends only when is_symlink is false, and remove_dir_all still runs after a failed unlink.
- F03-3 - fixed - check-ref-format gates the name, and tests cover foo.bar. A non-date c3: line is a new finding.
- F03-4 - fixed - The value span keeps quotes and comments, and model : is recognized.
- F03-5 - not-checked - Telemetry [1M] handling is outside this diff.
- F03-6 - fixed - Cherry is capped at 50 and read paths set GIT_OPTIONAL_LOCKS=0.
- F04-1 - fixed - Status still runs on the listing and doctor, with GIT_OPTIONAL_LOCKS=0. The hook and finish note do not run status.
- F04-2 - not-checked - Maintain swap_plan is outside this diff. AGENTS.md still uses numeric successor.
- F04-3 - not-checked - Catalog created stamps are outside this diff.
- F04-4 - fixed - refresh_repo_models returns immediately when C3_CHILD marks the process, and doctor says fix_skipped.
- F04-5 - still-open - remove still ignores unlink failures and always runs worktree remove --force and remove_dir_all.
- F04-6 - fixed - Probes and the detached pass set the mark explicitly. c3 update's claude child does not.
- F04-7 - still-open - The checkout is no longer the default, but main still wins over develop when origin/HEAD and init.defaultBranch are unset. The worktree-name half is fixed.
- F04-8 - not-checked - Telemetry shape and catalog redirect limits are outside this diff.
- F04-9 - fixed - A non-record c3: line is kept, and a failed read aborts the write. A four-field line with a non-date is a new finding.
- F10-1 - not-checked - Agy tool classification is outside this diff.
- F10-2 - not-checked - Agy ACTIVE/DONE merge is outside this diff.
- F10-3 - not-checked - Kimi brief preflight versus staging is outside this diff.
- F10-4 - not-checked - Panel finding snapshots are outside this diff.
- F10-5 - not-checked - Smoke sub-agent exemptions are outside this diff.
- F10-6 - not-checked - Codex -c keys are outside this diff.
- F10-7 - not-checked - Panel death records are outside this diff.
- F11-1 - not-checked - Agy write classification is outside this diff.
- F11-2 - not-checked - Agy bound-input races are outside this diff.
- F11-3 - not-checked - Kimi staging symlink overwrite is outside this diff.
- F11-4 - not-checked - Kimi brief availability is outside this diff.
- F11-5 - not-checked - Kimi prompt filename disclosure is outside this diff.
- F11-6 - not-checked - Panel blocker snapshots are outside this diff.
- F11-7 - not-checked - Codex sub-agent smoke is outside this diff.
- F11-8 - not-checked - Rollout parser evidence is outside this diff.
- F11-9 - not-checked - Spawn attribution windows are outside this diff.
- F11-10 - not-checked - Secondary-turn prompts are outside this diff.
- F11-11 - not-checked - Detached death-record identity is outside this diff.
- F11-12 - not-checked - Providers reply_outcomes is outside this diff.

## Verdict: HOLD

doctor --fix can treat a temp path that is a link as a stale kimi worktree and force-remove it, and the SessionStart hook can rewrite model files under a directory that only has a stray .git entry.

### Blockers

- **F13-1** `crates/c3/src/setup/branches.rs:825`, `crates/c3/src/setup/branches.rs:758`, `crates/c3/src/setup/branches.rs:849`, `crates/c3/src/engines/kimi.rs:211`, `README.md:668`, `CHANGELOG.md:174` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target. Verify: In a disposable repository, register a worktree at $TEMP/c3-kimi-wt-<32 hex> that is a junction or symlink to a sentinel directory older than 6 hours, run c3 doctor --fix, and see whether the sentinel file still exists. Remedy: Before removal, require symlink_metadata to show a real directory, require the .git gitdir file to point at this repo's worktrees admin directory, and abort the whole removal if any link cannot be unlinked. Do not call remove_dir_all on a path that fails those checks.
- **F02-1** (prior, still-open) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, fixed) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, still-open) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, fixed) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, not-checked) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.

### Unproven scenarios

- Whether git config treats a value that starts with - as an option when write_config passes it without --.
- Whether claude plugin update starts a SessionStart hook that would repair the cwd.
- Whether git worktree remove --force or Rust remove_dir_all follows a junction placed at the worktree path; the code sends that path and this review did not execute it.
- Whether every supported git still leaves .git/index unchanged when GIT_OPTIONAL_LOCKS=0 and the stat cache is stale.
- The pinned plugin harness comparison of child_env_allowed, which the brief says was not run.

### First-run checklist (observable)

_(none)_
