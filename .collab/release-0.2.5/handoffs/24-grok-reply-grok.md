# Handoff 24 - Grok (grok): reply-grok

Date: 2026-11-23 05:33 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (~/.grok/auth.json) - subscription (grok.com oauth).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: a38aec59-a721-401c-b33c-7b32f520acaa.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-9b63dfc7f2aa4df2b5c348c44a31b4ce.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-3bbd832021b94fe7b53c8522f739722f` of revision b9f665a9ea46e50d1cad137d8072b3258eacc8d7 (--rev b9f665a9ea46e50d1cad137d8072b3258eacc8d7; removed after the run).
Parent thread: (none - new thread). Result thread: `01a2069e-b0c1-72c3-8d2b-77754d33f3e6` (source: events).
Brief: `.collab/release-0.2.5/handoffs/21-claude-verify-staging-smoke-doctor.md` (sha256 117034b5c3ae). Reviewed: b9f665a, base b9f665a9ea46e50d1cad137d8072b3258eacc8d7, tree sha256 93a2e63d1804, 0 changed files. Live HEAD at launch: b9f665a9ea46e50d1cad137d8072b3258eacc8d7.
WARNING: the main working tree changed during the review (fingerprint before/after differ); the reviewer read its throwaway worktree, not the live tree.
Bridge outcome: usable reply. Wall time: 970.1 s. Tokens: in 182212 (cached 6127488), out 53314, reasoning 40396.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Warnings: the main working tree changed during the run (1 file: crates/c3/src/engines/subprocess.rs) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - F13-6 still rewrites grok-4.7 to grok-4.20 when the maintain state has no dates for both ids, so this wave does not close a finding it claims to close. Findings: 0 blocker, 0 major, 3 minor, 1 note (F24-1..F24-4, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/24-grok-reply-grok.reply.json`.
Raw event stream: `handoffs/24-grok-reply-grok.events.jsonl`.
Verbatim reply follows.

---

## Q1

**fixed** unless noted.

- F10-1, F10-2, F11-1 — `tree_check.rs:209` (unlisted name is writing; parameters walked at every depth) and `agy.rs:440` (one index's events merged). A later command under a key that already holds a non-empty object is dropped (finding #4).
- F11-2 — `tree.rs:258` fails agy when a bound brief or artifact changes; `orchestrate/mod.rs:5580` forces class `permission` and clears usable. muse, grok, http and claude stay warnings.
- F11-3 — `worktree.rs:256` exclusive-creates a temp; `worktree.rs:275` renames only after a second check. A link planted in that gap is finding #1.
- F11-4, F10-3 — `worktree.rs:705` keeps an in-repo `..`; `worktree.rs:866` falls back to HEAD; `reviewed.rs:394` returns preflight `Refused`, not a capability failure.
- F11-5 — `worktree.rs:1012` counts paths HEAD does not hold; tracked names are data, and backtick or control names are only counted.
- F11-6, F10-4 — `orchestrate/mod.rs:4817` uses the spec's frozen status and severity; an id outside that list is not a prior report.
- F11-7, F10-5 — `smoke.rs:674` is `Unproven` without proof for both turns; a multi-agent name counts in any namespace. Holds at `maintain/mod.rs:925`, `:1050` and `:1202`; no seat at `:1609`.
- F11-8 — session id, a real `tools` array, malformed lines and a terminal `task_complete` are required; the Codex home is made absolute.
- F11-9 — one more rollout read after a voluntary exit; a child rollout is dated by its head timestamp.
- F11-10 — `secondary.rs:35`, `:49` and `:135` add the reviewer sentence to format repair, read-insist and the timeout continuation.
- F11-11, F10-7 — `note_turn_stream` records the turn; `detach.rs:1311` skips another handoff. A pending record that still names an older `events` path beats the noted stream (finding #2).
- F11-12 — `ledger.rs:376` stores `reply_kept`; `providers.rs:2714` counts by it.
- F12-1, F13-1, F13-8 — `branches.rs:958` is `CannotTell` when the live start time is empty; `branches.rs:1001` never removes a link; no owner record is left alone; `worktree.rs:131` fails creation if the record cannot be written.
- F13-2 — `repo_models.rs:530` accepts `.git` only with `HEAD` or a `gitdir:` file whose target has one. It stops at the first `.git` even when that entry is not a repository (finding #3).
- F13-3 — `branches.rs:188` (`YYYY-MM-DD`, month 01-12, day 01-31).
- F13-4 — `update/mod.rs:710` marks every update child; `branches.rs:1375` refuses `--set`, `--clear` and `--default` inside a child.
- F13-5 — `hook/mod.rs:102` via `bounded.rs:179` kills the registered git child.
- F13-6 — **partly, still open.** `repo_models.rs:142` skips a successor only when `dated_older` (`models.rs:429`) is true. With no dates for both ids, `grok-4.7` is still rewritten to `grok-4.20`. The unit test asserts that edit.
- F13-7 — `branches.rs:613` requires both git calls.
- F12-2 — `providers.rs:2489` sets `GIT_OPTIONAL_LOCKS=0`.
- F12-3 — refusal text names a non-link reparse point.
- F12-4 — wording now says the re-read-to-write gap remains (F13-9, decided).

## Q2

A brief behind ordinary directories is staged when the revision has a file or nothing on the path and the disk components are real directories. On Windows every reparse point is a link (`worktree.rs:500`), so a cloud-sync placeholder is refused for staging and for doctor removal. A worktree path that is itself a link is left alone (`branches.rs:1001`); an ancestor symlink does not make that entry a link. Finding #1 is the remaining hostile case.

## Q3

A kill between create and rename leaves `.c3-stage-*.tmp` in the worktree. Owner-record failure removes the worktree, or leaves it for hand removal if removal is refused. The smoke scratch directory is removed on drop (`smoke.rs:390`). The hook reaps the git it registered, not that git's grandchildren. Branch edits are one `git config`.

## Q4

An agy name missing from the reader list fails a changed tree, including a real reader the list omits. Creation fails closed when the owner record cannot be written. `C3_CHILD` refusals apply only when the mark is set and is not `0`, `false`, `off` or `no`. Repair is silent when `GIT_DIR` is set (`repo_models.rs:626`) or the nearest `.git` is junk (finding #3), including a symlink `.git`.

## Q5

`Unproven` holds the pre-update smoke, the post-update smoke and recovery, and it does not seat a model; `c3 maintain --smoke` prints it. A non-codex smoke still returns `Pass` at `smoke.rs:591` because that proof is codex-only. The CHANGELOG heading "AGENTS.md never follows to an older model" is stronger than the next sentences, which keep the version rule when either id is undated.

## Q6

No test re-seats a panel after a staging refusal. No test fails muse, grok or claude on a changed bound input. Cascade reader names are not checked against a live agy log. The Windows rename onto a late symlink, and a death record after a failed pending-events rewrite, were not run.

## Requested checks

RC1. Directory `crates/c3`, workspace-write, under one minute. Unit-test `read_agy_events` for an allow-listed reader: ACTIVE `{"config":{"cwd":"."}}`, DONE `{"config":{"action":"rm -rf x"}}`, same index. Finding #4 holds if `writing_tool_names` is empty.

RC2. Disposable repo, workspace-write, one run. Break at `worktree.rs:256` during `stage_bytes` of a new path, replace the parent with a junction to a sentinel, continue. Finding #1 holds if the sentinel gains `.c3-stage-*.tmp`.

---

### Findings

- **F24-1** [minor] `crates/c3/src/engines/worktree.rs:256`, `crates/c3/src/engines/worktree.rs:275`, `crates/c3/src/engines/worktree.rs:340` - stage_bytes can create a file outside the worktree if a parent directory is replaced by a symlink or junction after real_parent_inside returns and before the exclusive create. The recheck then deletes that file only when remove_file succeeds. A swap in the gap between the second check and rename is not checked again. On Windows, rename onto a symlink destination can also follow the link. Trigger: During staging, a local writer replaces a checked parent, or the destination, with a symlink or junction before open or rename. Evidence: read-code: The parent is checked, then OpenOptions::create_new writes .c3-stage-*.tmp, then parent_still_inside and dest_replaceable run, then std::fs::rename. real_parent_inside canonicalizes the worktree root and follows it, so a root that is already a symlink passes the first check.; read-code: is_link treats every Windows reparse point as a link, but only at the moments it is called. Verify: RC2: breakpoint at the create_new open, swap the parent for a junction to a sentinel, continue, and see whether the sentinel receives .c3-stage-*.tmp. Remedy: Open the temp file with directory file descriptors (openat / NT relative open) taken while the parent is known to be a real directory inside the worktree, and replace the destination with a no-follow rename.
- **F24-2** [minor] `crates/c3/src/consult/detach.rs:1324`, `crates/c3/src/consult/detach.rs:1335`, `crates/c3/src/consult/orchestrate/mod.rs:4234` - A death record keeps the first pending events path of the same handoff and uses the noted stream only when that field is empty. A pending record that still names the main stream therefore hides a later continuation stream stored in the status note. Trigger: A timeout continuation updates the status note but the pending record's events field stays on the main stream (the pending rewrite failed or was not the file the reader sees). Evidence: read-code: Same-handoff pending records fill events_file. The noted stream is copied only inside if ev.events_file.is_empty().; read-code: note_turn_stream writes the pending record and, for a detached non-member, the status note, and ignores write errors. Verify: Leave a same-handoff pending record whose events path is the main stream, set the status note to continue.events.jsonl, build the death record, and compare events_file. Remedy: When the noted stream names a different file than the pending record, prefer the noted stream for the turn that ended the run.
- **F24-3** [note] `crates/c3/src/setup/repo_models.rs:530`, `crates/c3/src/setup/repo_models.rs:626` - The SessionStart repair stops at the nearest .git entry. If that entry is not a repository, the walk does not continue to a real parent, and the repair does nothing. GIT_DIR set has the same silence. A symlink .git is not accepted. Trigger: cwd is inside a real git repository whose nearer directory contains a stray .git file, a broken link, or a directory without HEAD, or the environment has GIT_DIR set. Evidence: read-code: On any symlink_metadata hit for .git, find_repo_root returns is_git_entry(...).then(...), which is None for a non-repository, and does not try parents.; read-code: hook_line returns None when GIT_DIR is set. Verify: In a real repo, add an empty file sub/.git, run c3 hook with cwd in sub, and confirm AGENTS.md is unchanged while a parent with a valid .git exists. Remedy: If the nearest .git entry is not a repository, keep walking. Keep refusing a gitdir target that is not a directory holding HEAD.
- **F24-4** [minor] `crates/c3/src/engines/agy.rs:442`, `crates/c3/src/engines/tree_check.rs:209` - merge_parameters does not merge into a nested object that already has keys. A later event's command under that key is discarded. If the tool name is on the reader allow-list, the step is classified as not writing. Trigger: An allow-listed reader logs ACTIVE parameters {"config":{"cwd":"."}} and DONE parameters {"config":{"action":"rm -rf x"}} at the same step index, and the tree changes. Evidence: read-code: For two objects, a key whose current value is non-empty is left unchanged. An object with any key is not empty, so the whole nested object from the later event is dropped.; read-code: A name on the reader list is writing only when agy_params_show_write sees the surviving parameters. Verify: RC1: feed that two-event step to read_agy_events and check writing_tool_names. Remedy: Merge objects recursively, and let a non-empty later value replace an empty or placeholder value at any depth.

### Prior findings

- F02-1 - not-checked - Outside this fix wave; not re-read.
- F02-2 - not-checked - Outside this fix wave; not re-read.
- F02-3 - not-checked - Outside this fix wave; not re-read.
- F02-4 - not-checked - Outside this fix wave; not re-read.
- F02-5 - not-checked - Outside this fix wave; not re-read.
- F02-6 - not-checked - Outside this fix wave; not re-read.
- F02-7 - not-checked - Outside this fix wave; not re-read.
- F02-8 - not-checked - Outside this fix wave; not re-read.
- F02-9 - not-checked - Outside this fix wave; not re-read.
- F02-10 - not-checked - Outside this fix wave; not re-read.
- F02-11 - not-checked - Outside this fix wave; not re-read.
- F02-12 - not-checked - Outside this fix wave; not re-read.
- F02-13 - not-checked - Outside this fix wave; not re-read.
- F02-14 - not-checked - Outside this fix wave; not re-read.
- F02-15 - not-checked - Outside this fix wave; not re-read.
- F02-16 - not-checked - Outside this fix wave; not re-read.
- F02-17 - not-checked - Outside this fix wave; not re-read.
- F02-18 - not-checked - Outside this fix wave; not re-read.
- F02-19 - not-checked - Outside this fix wave; not re-read.
- F02-20 - not-checked - Outside this fix wave; not re-read.
- F02-21 - not-checked - Outside this fix wave; not re-read.
- F03-1 - not-checked - Outside this fix wave; not re-read.
- F03-2 - not-checked - Outside this fix wave; not re-read.
- F03-3 - not-checked - Outside this fix wave; not re-read.
- F03-4 - not-checked - Outside this fix wave; not re-read.
- F03-5 - not-checked - Outside this fix wave; not re-read.
- F03-6 - not-checked - Outside this fix wave; not re-read.
- F04-1 - not-checked - Outside this fix wave; not re-read.
- F04-2 - not-checked - Outside this fix wave; not re-read.
- F04-3 - not-checked - Outside this fix wave; not re-read.
- F04-4 - not-checked - Outside this fix wave; not re-read.
- F04-5 - not-checked - Outside this fix wave; not re-read.
- F04-6 - not-checked - Outside this fix wave; not re-read.
- F04-7 - not-checked - Outside this fix wave; not re-read.
- F04-8 - not-checked - Outside this fix wave; not re-read.
- F04-9 - not-checked - Outside this fix wave; not re-read.
- F10-1 - fixed - A single event with a nested command is classified as writing. The new multi-event nested-merge hole is finding #4, not this trigger.
- F10-2 - fixed - Parameter-less ACTIVE and named DONE of one index are merged before classification.
- F10-3 - fixed - Staging failure is SetupError::Refused with preflight wording, so the seat is not a capability failure.
- F10-4 - fixed - member_prior_view uses the frozen listed records for status and severity.
- F10-5 - fixed - A multi-agent tool name counts in any namespace, including mcp__.
- F10-7 - fixed - Turns note their own stream. Finding #2 is the leftover case where a stale pending events path wins.
- F11-1 - fixed - Empty ACTIVE plus a later command, and an unlisted tool, count as writing.
- F11-2 - fixed - Agy bound-input change fails the outcome and forces permission. Other engines still warn, as the brief says.
- F11-3 - fixed - A link present at the check is refused. The remaining race is finding #1.
- F11-4 - fixed - In-repo .. is normalised; an unreadable live brief is taken from HEAD; staging failure is a pre-start refusal.
- F11-5 - fixed - Untracked names are a count; tracked names with backtick or controls are counted, not shown.
- F11-6 - fixed - Prompt and prior-report validation use the records frozen at panel start.
- F11-7 - fixed - Missing thread, opted-in seat, and unreadable rollout are Unproven and held like a failure.
- F11-8 - fixed - Absolute Codex home, session id, tools array, malformed lines and task_complete are checked.
- F11-9 - fixed - Post-exit inspection catches a spawn between polls; child attribution uses the head timestamp.
- F11-10 - fixed - Format repair, read-insist and timeout continuation include the reviewer sentence. Denial-retry also has it at this revision.
- F11-11 - fixed - Same-run stream is recorded and other handoffs are ignored. Finding #2 covers a stale pending events field.
- F11-12 - fixed - Failed runs record reply_kept and the providers count uses it, including a partial.
- F12-1 - fixed - An empty live start time is CannotTell and the worktree is left alone.
- F12-2 - fixed - resolve_repo_root sets GIT_OPTIONAL_LOCKS=0.
- F12-3 - fixed - A non-link Windows reparse point is described as a reparse point.
- F12-4 - fixed - The sentences now say the gap between re-read and write remains.
- F13-1 - fixed - symlink_metadata classifies a link path as not removable, so doctor --fix does not hand it to removal.
- F13-2 - fixed - A stray .git file is not a root. Finding #3 is the side effect of not walking past it.
- F13-3 - fixed - A non-date third field is a foreign note and is kept.
- F13-4 - fixed - c3 update marks its children, and branch edits refuse when C3_CHILD marks the process.
- F13-5 - fixed - The deadline calls ScanChildren::stop, which kills and reaps registered git children.
- F13-6 - still-open - Without dates for both ids, rewrite_agents_md_model still returns an edit from grok-4.7 to grok-4.20; the unit test asserts is_some.
- F13-7 - fixed - A failed cherry or rev-list leaves patches_checked false and increments patches_unchecked.
- F13-8 - fixed - Creation fails if the owner record cannot be written, and a directory with no record is not removed by age.
- F14-1 - not-checked - Outside this fix wave; not re-read.
- F14-2 - not-checked - Outside this fix wave; not re-read.
- F14-3 - not-checked - Outside this fix wave; not re-read.
- F14-4 - not-checked - Outside this fix wave; not re-read.
- F14-5 - not-checked - Outside this fix wave; not re-read.
- F15-1 - not-checked - Outside this fix wave; not re-read.
- F15-2 - not-checked - Outside this fix wave; not re-read.
- F15-3 - not-checked - Outside this fix wave; not re-read.
- F15-5 - not-checked - Outside this fix wave; not re-read.
- F15-6 - not-checked - Outside this fix wave; not re-read.
- F16-1 - not-checked - Outside this fix wave; not re-read.
- F16-2 - not-checked - Outside this fix wave; not re-read.
- F16-3 - not-checked - Outside this fix wave; not re-read.
- F16-4 - not-checked - Outside this fix wave; not re-read.
- F16-6 - not-checked - Outside this fix wave; not re-read.
- F16-7 - not-checked - Outside this fix wave; not re-read.
- F16-8 - not-checked - Outside this fix wave; not re-read.
- F16-9 - not-checked - Outside this fix wave; not re-read.
- F16-10 - not-checked - Outside this fix wave; not re-read.
- F17-1 - not-checked - Outside this fix wave; not re-read.
- F17-2 - not-checked - Outside this fix wave; not re-read.
- F17-3 - not-checked - Outside this fix wave; not re-read.
- F17-4 - not-checked - Outside this fix wave; not re-read.
- F17-5 - not-checked - Outside this fix wave; not re-read.
- F17-6 - not-checked - Outside this fix wave; not re-read.
- F17-7 - not-checked - Outside this fix wave; not re-read.
- F17-8 - not-checked - Outside this fix wave; not re-read.
- F17-9 - not-checked - Outside this fix wave; not re-read.
- F17-10 - not-checked - Outside this fix wave; not re-read.
- F18-1 - not-checked - Outside this fix wave; not re-read.
- F18-2 - fixed - At this revision worktree.rs:121 passes --no-optional-locks. That flag is not in the 99e2da7 hunk; a later commit added it.
- F18-3 - not-checked - Outside this fix wave; not re-read.
- F18-4 - not-checked - Outside this fix wave; not re-read.
- F18-5 - still-open - stage_file at worktree.rs:228 still uses std::fs::read, which follows a symlink used as the brief source.
- F18-6 - not-checked - unlink_outside_links exists at this revision; the reviewer write-through case was not re-tested.
- F18-7 - not-checked - Outside this fix wave; not re-read.
- F19-1 - not-checked - Outside this fix wave; not re-read.
- F19-2 - not-checked - Outside this fix wave; not re-read.
- F19-3 - not-checked - Outside this fix wave; not re-read.
- F19-4 - not-checked - Outside this fix wave; not re-read.
- F19-5 - not-checked - Outside this fix wave; not re-read.
- F19-6 - not-checked - Outside this fix wave; not re-read.
- F20-1 - not-checked - Outside this fix wave; not re-read.
- F20-2 - fixed - Same as F18-2: the live index status in create uses --no-optional-locks at this revision.
- F20-3 - not-checked - Outside this fix wave; not re-read.
- F20-4 - not-checked - Outside this fix wave; not re-read.
- F20-5 - fixed - The isolation paragraph now names tracked paths only and counts untracked ones. This is the F11-5 fix, present in worktree.rs:1040.
- F20-6 - not-checked - Outside this fix wave; not re-read.

## Verdict: HOLD

F13-6 still rewrites grok-4.7 to grok-4.20 when the maintain state has no dates for both ids, so this wave does not close a finding it claims to close.

### Blockers

- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, not-checked) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, fixed) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, fixed) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, fixed) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.
- **F19-1** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision.

### Unproven scenarios

- Windows MoveFileEx behaviour when the destination becomes a symlink after dest_replaceable was not run.
- Whether every real Cascade reader name is on AGY_READ_ONLY_TOOLS.
- A panel actually drawing a reserve after a staging refusal was not executed; only the preflight wording is asserted.
- A failed pending-events rewrite versus the status note was not executed.
- Findings outside F10-F13 of this wave were not re-read, except the spots named in prior_findings.

### First-run checklist (observable)

_(none)_
