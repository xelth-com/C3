# Handoff 51 - Grok (grok): reply-grok

Date: 2026-11-24 09:43 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (usable reply 22 min ago).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 000a845b-430b-46a0-995d-510ec7ce039d.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-db9d6ba6425f42a1bb174b806de0588e.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-2809cd176f53436e97b8d5c7c6639e1f` of revision 2ffa02aa435c0942ffb9cba505eef58d358a58a1 (--rev 2ffa02aa435c0942ffb9cba505eef58d358a58a1; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20cab-1ed7-7f81-a07f-d70478770589` (source: events).
Brief: `.collab/release-0.2.5/handoffs/50-claude-diff-review-bridge-debt.md` (sha256 22b36ab257c1). Reviewed: 2ffa02a, base 2ffa02aa435c0942ffb9cba505eef58d358a58a1, tree sha256 946c1867ea9b, 0 changed files. Live HEAD at launch: 2ffa02aa435c0942ffb9cba505eef58d358a58a1.
Bridge outcome: usable reply. Wall time: 880.9 s. Tokens: in 188139 (cached 4472576), out 57841, reasoning 42606.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - An allow-listed agy reader can hide a command under a data-shaped key and stay usable, a --rev brief can be recorded under a hash the worktree did not get, and patch-claim cleanup can delete or replace another run's file. Findings: 0 blocker, 4 major, 2 minor, 0 note (F51-1..F51-6, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/51-grok-reply-grok.reply.json`.
Raw event stream: `handoffs/51-grok-reply-grok.events.jsonl`.
Verbatim reply follows.

---

## Q1
Byte hashes, not timestamps, decide a bound-input change (`orchestrate/mod.rs:446`; tree content at `revision.rs:409`). This run's patch is written before `collab_before` (`orchestrate/mod.rs:4055`) and only that file plus its temp name is exempt (`reviewed.rs:1503`). A sibling's other patch is a warning on a worktree and on muse, grok, claude and http; a complete live-tree agy reply with no writing step stays usable. A failed re-read is treated as a change: an empty hash or `missing` fails the reply (`orchestrate/mod.rs:446`). Codex in a worktree is exempt (`tree.rs:345`).

## Q2
The merge keeps both conflicting values (`agy.rs:470`). On an allow-listed reader only a Scanned string is tested (`tree_check.rs:241`). A key ending in `path`, `file`, `url`, `uri` or `dir` (and the other suffixes), or containing `query`, `pattern`, `regex`, `search`, `glob`, `include` or `exclude`, is data (`tree_check.rs:265`). Values such as `exec_path` = `rm -rf x`, `paths` = `bash -c rm`, `research` = `git clean -fd` (`research` contains `search`) and `global` = `rm -rf x` (`global` contains `glob`) are not writing. Any other tool name still counts as writing, so a complete agy reply is kept.

## Q3
Equal bytes share one content-addressed name. A crash after the synced temp and before publish leaves the temp, not a half-written final name. Without hard links the name is an empty claim, then a rename (`reviewed.rs:1268`); an empty file unchanged for 60s is taken over (`reviewed.rs:1188`). `drop_own_claim` and the takeover stat the file, then `remove_file` by name (`reviewed.rs:1232`, `reviewed.rs:1412`). A sibling that finishes in that gap is deleted. A live owner stalled past 60s can lose the claim and, where rename replaces, overwrite the other run, so the ledger sha256 is not the file. Excluded paths use a new index in the system temp directory (`reviewed.rs:1033`) via `GIT_INDEX_FILE` (`reviewed.rs:752`) and `GIT_OPTIONAL_LOCKS=0`. I saw no write to the repository index. A kill leaves that temp index in the temp directory.

## Q4
Without `--diff-tracked-only`, a one-revision patch still contains every untracked, not-ignored file (`reviewed.rs:978`). Gitignore and `.c3ignore` stay out. At most ten names are printed; a backtick or a control character is only counted (`reviewed.rs:1544`) while its bytes stay in the patch. The flag is on the CLI, detach (`detach.rs:220`) and panel members (`panel/run.rs:2290`, `member.rs:250`). MCP `c3_consult` forwards `diff` and has no flag (`mcp/mod.rs:318`, `mcp/mod.rs:971`).

## Q5
One start-of-run HEAD resolve is `rev_live_head` (`orchestrate/mod.rs:2725`). It withholds priors and is the handoff and ledger live HEAD, or unresolved when empty. Finding `source.base_commit` is the worktree fingerprint (`ledger.rs:51`). A later HEAD move does not change that launch value. A missing brief is hashed by `committed_brief`, which prefers the literal HEAD (`worktree.rs:934`). Staging reads again (`reviewed.rs:435`); drift skips the re-hash when `brief_from_revision` is set (`orchestrate/mod.rs:443`). The handoff can cite HEAD and a sha256 the worktree does not hold.

## Q6
`explain` elects with `skip_preflight` and `no_network` (`cli/router.rs:264`), reads records and searches PATH, and I saw no child process and no consultation write. Replay reads sessions only. Any invalid `ext.c3.draw` row, including a missing `mult`, drops the whole block (`replay.rs:291`), so a record that used to match on a mix of precise and rounded values can now exit 1. The file is left as it was.

## Q7
Not executed: a real agy, muse or grok CLI for the bound-input rule; http `--rev` end to end; a no-hardlink crash on the claim; the `remove_file` race; HEAD moving between the two brief reads. The changelog's tests were not run. The README diff, the consult skill reference, and most test bodies were not read.

## Requested checks
RC1. In `crates/c3`, assert `agy_tool_step_is_writing` for `read_file` with `exec_path` = `rm -rf x` and with `research` = `git clean -fd`. Read-only. Both are false today (finding #1). One unit test.
RC2. Disposable repo: delete the live brief, differ HEAD from an older commit, run `--rev` of the older commit, compare the handoff brief sha256 with the worktree file. Workspace-write. They differ if HEAD's blob changes after the plan-time read (finding #2). 10 minutes.
RC3. Two no-hardlink placements of one patch name; pause between `file_state` and `remove_file`. Workspace-write. The finished sibling file is gone (finding #3). One fixture.

---

### Findings

- **F51-1** [major] `crates/c3/src/engines/tree_check.rs:241`, `crates/c3/src/engines/tree_check.rs:265` - On an allow-listed agy reader, a command is not classified as writing when its key ends in a path/file/url/dir suffix or contains a search fragment such as search or glob, so a complete reply stays usable. Trigger: A read_file (or other AGY_READ_ONLY_TOOLS) step logs parameters such as exec_path = rm -rf x, paths = [bash -c rm], research = git clean -fd, or global = rm -rf x, and the tree check then sees no writing step. Evidence: read-code: Strings are command-scanned only for role Scanned. Write markers are tested first, then any key containing a search marker or ending in path/file/url/uri/dir/folder is Data and never scanned. research contains search; global contains glob.; read-code: An agy run with no writing step and a complete schema-valid reply is warned and the reply is kept even when the tree changed. Verify: RC1. Remedy: Treat only known file-name parameters of each allow-listed reader as data. Scan every other key, and do not use substring matches such as glob inside global or search inside research.
- **F51-2** [major] `crates/c3/src/engines/worktree.rs:934`, `crates/c3/src/consult/reviewed.rs:435`, `crates/c3/src/consult/orchestrate/mod.rs:443`, `crates/c3/src/consult/orchestrate/render.rs:626` - The brief hash and the 'taken from revision HEAD' line are fixed at plan time, while staging reads the brief again and drift does not re-hash it, so the handoff and ledger can name bytes the reviewer did not read. Trigger: The live brief is missing, committed_brief takes HEAD's blob, and before stage_bytes either the live file appears or HEAD's blob at that path changes. Under --rev the reviewed commit's own brief is skipped whenever HEAD has the path. Evidence: read-code: committed_brief returns the string HEAD plus the blob read at plan time, and only then the reviewed revision.; read-code: Staging calls brief_bytes again, which returns a live file first and otherwise re-reads HEAD.; read-code: When brief_from_revision is set, brief_sha_after is the plan-time hash and brief_changed stays false. Verify: RC2. Remedy: Hash the bytes staging actually wrote, name that commit sha rather than HEAD, and prefer the reviewed revision when --rev is set.
- **F51-3** [major] `crates/c3/src/consult/reviewed.rs:1232`, `crates/c3/src/consult/reviewed.rs:1412`, `crates/c3/src/consult/reviewed.rs:1291` - Claim cleanup removes the patch name after a stat, so a sibling's finished patch can be deleted, and a live owner stalled more than 60 seconds can later replace the file the other run published. Trigger: No hard links. One placement's rename fails, or an empty claim is older than 60s, and the other placement's rename lands between file_state and remove_file. Or the first process resumes and renames onto the name after the takeover. Evidence: read-code: drop_own_claim unlinks by leaf name once it has seen size 0 and the claim identity. It does not unlink the opened handle.; read-code: The 60s takeover uses the same stat-then-remove_file sequence and treats any unchanged empty file as abandoned, with no check that the owner has exited.; inferred: The owner's dir.rename runs after that window and, on a filesystem where rename replaces, writes its bytes over the name the other run already published. Verify: RC3. Remedy: Unlink only the opened claim handle (or rename over the claim from the same directory handle without a separate delete). Take over a claim only when its owner pid is dead, not merely when the empty file is 60s old. Supersedes: F43-3.
- **F51-4** [major] `crates/c3/src/consult/orchestrate/mod.rs:446`, `crates/c3/src/consult/orchestrate/mod.rs:457` - A live-tree reply is failed as a changed bound input when the brief or an artifact cannot be re-read, even if the bytes are unchanged. Trigger: At the post-turn re-hash the brief path is locked or otherwise unreadable, or an artifact read fails. brief_sha_after becomes empty, or the artifact after-hash becomes missing. Evidence: read-code: fs::read failure uses unwrap_or_default for the brief and unwrap_or_else missing for artifacts. Any difference from the pre-run hash sets brief_changed or artifacts_changed_paths.; read-code: A non-empty bound-changed list forces outcome failed and clears a usable reply. Verify: Make the brief unreadable at the end of a fake live-tree consult (revoke read for the re-hash only) and read bridge_outcome. Remedy: If the re-read fails, fail with 'could not be re-read' or retry. Do not report a content change unless two successful hashes differ.
- **F51-5** [minor] `crates/c3/src/mcp/mod.rs:318`, `crates/c3/src/mcp/mod.rs:971` - An MCP consult or panel can pass diff and cannot pass --diff-tracked-only, so a one-revision diff from MCP always embeds untracked file content. Trigger: c3_consult or c3_panel is called with diff set to one revision while the worktree has an untracked, not-ignored file. Evidence: read-code: The forwarded string flags include diff and do not include diff_tracked_only.; read-code: The c3_consult input schema documents diff and has no tracked-only property. Panel and detach paths do thread the CLI flag. Verify: Call the MCP consult tool schema and confirm diff_tracked_only is absent; run a one-revision diff through that builder and inspect the argv. Remedy: Add a boolean diff_tracked_only to the MCP consult and panel schemas and append --diff-tracked-only when it is true.
- **F51-6** [minor] `crates/c3/src/consult/detach.rs:1404` - When the pending stream's mtime is known and the noted stream's mtime is not, the death record keeps the pending path, which can be the older main stream. Trigger: The status note names a continuation events file that bases.join cannot see as a file, while the pending record still names the main stream and that file's mtime is readable. Evidence: read-code: The comment says the note wins when either time is unknown. The match returns false for (Some pending, None noted), so the pending path is kept. Verify: Build a death record whose note path does not resolve under bases and whose pending path does, and compare events_file with the note. Remedy: If the noted path does not resolve, keep the note when the pending file is the older name, or resolve both paths the same way the events file was stored.

### Prior findings

- F02-1 - not-checked - Outside this commit; not re-read.
- F02-2 - not-checked - Outside this commit; not re-read.
- F02-3 - not-checked - Outside this commit; not re-read.
- F02-4 - not-checked - Outside this commit; not re-read.
- F02-5 - not-checked - Outside this commit; not re-read.
- F02-6 - not-checked - Outside this commit; not re-read.
- F02-7 - not-checked - Outside this commit; not re-read.
- F02-8 - not-checked - Outside this commit; not re-read.
- F02-9 - not-checked - Outside this commit; not re-read.
- F02-10 - not-checked - Outside this commit; not re-read.
- F02-11 - not-checked - Outside this commit; not re-read.
- F02-12 - not-checked - Outside this commit; not re-read.
- F02-13 - not-checked - Outside this commit; not re-read.
- F02-14 - not-checked - Outside this commit; not re-read.
- F02-15 - not-checked - Outside this commit; not re-read.
- F02-16 - not-checked - Outside this commit; not re-read.
- F02-17 - not-checked - Outside this commit; not re-read.
- F02-18 - not-checked - Outside this commit; not re-read.
- F02-19 - not-checked - Outside this commit; not re-read.
- F02-20 - not-checked - Outside this commit; not re-read.
- F02-21 - not-checked - Outside this commit; not re-read.
- F03-1 - not-checked - Outside this commit; not re-read.
- F03-2 - not-checked - Outside this commit; not re-read.
- F03-3 - not-checked - Outside this commit; not re-read.
- F03-4 - not-checked - Outside this commit; not re-read.
- F03-5 - not-checked - Outside this commit; not re-read.
- F03-6 - not-checked - Outside this commit; not re-read.
- F04-1 - not-checked - Outside this commit; not re-read.
- F04-2 - not-checked - Outside this commit; not re-read.
- F04-3 - not-checked - Outside this commit; not re-read.
- F04-4 - not-checked - Outside this commit; not re-read.
- F04-5 - not-checked - Outside this commit; not re-read.
- F04-6 - not-checked - Outside this commit; not re-read.
- F04-7 - not-checked - Outside this commit; not re-read.
- F04-8 - not-checked - Outside this commit; not re-read.
- F04-9 - not-checked - Outside this commit; not re-read.
- F10-1 - not-checked - Outside this commit; not re-read.
- F10-2 - not-checked - Outside this commit; not re-read.
- F10-3 - fixed - A live-missing brief is accepted when committed_brief returns bytes (worktree.rs:930). The second read is finding #2.
- F10-4 - not-checked - Single-run snapshot is taken at start (orchestrate/mod.rs:2696); panel concurrency was not re-tested.
- F10-5 - not-checked - Outside this commit; not re-read.
- F10-7 - still-open - note_engine_stream still runs only when panel_member is None (orchestrate/mod.rs:4377 and 6692).
- F11-1 - not-checked - Classifier merge was read; the empty-ACTIVE write case was not re-run.
- F11-2 - not-checked - Bound-input withhold now covers every live-tree engine; a mid-run artifact swap was not executed.
- F11-3 - not-checked - Outside this commit; not re-read.
- F11-4 - fixed - Same committed_brief acceptance as F10-3. Staging still re-reads (finding #2).
- F11-5 - not-checked - Collab paths are omitted from the isolation paragraph; other uncommitted names are still listed.
- F11-6 - not-checked - Single-run judgement uses shown_priors (orchestrate/mod.rs:4950). A live status flip was not executed.
- F11-7 - not-checked - Outside this commit; not re-read.
- F11-8 - not-checked - Outside this commit; not re-read.
- F11-9 - not-checked - Outside this commit; not re-read.
- F11-10 - not-checked - Outside this commit; not re-read.
- F11-11 - fixed - A detached single run persists the stream and the reader loads record.stream (detach.rs:1551). Panel members remain F10-7.
- F11-12 - fixed - reply_outcomes counts a sidecar or partial only while present() sees the file (providers.rs:2781).
- F12-1 - not-checked - Outside this commit; not re-read.
- F12-2 - not-checked - Outside this commit; not re-read.
- F12-3 - not-checked - Outside this commit; not re-read.
- F12-4 - not-checked - Outside this commit; not re-read.
- F13-1 - not-checked - Outside this commit; not re-read.
- F13-2 - not-checked - Outside this commit; not re-read.
- F13-3 - not-checked - Outside this commit; not re-read.
- F13-4 - not-checked - Outside this commit; not re-read.
- F13-5 - not-checked - Outside this commit; not re-read.
- F13-6 - not-checked - Outside this commit; not re-read.
- F13-7 - not-checked - Outside this commit; not re-read.
- F13-8 - not-checked - Outside this commit; not re-read.
- F13-9 - not-checked - Outside this commit; not re-read.
- F14-1 - not-checked - Outside this commit; not re-read.
- F14-2 - not-checked - Outside this commit; not re-read.
- F14-3 - not-checked - Outside this commit; not re-read.
- F14-4 - not-checked - Outside this commit; not re-read.
- F14-5 - not-checked - Outside this commit; not re-read.
- F15-1 - not-checked - Outside this commit; not re-read.
- F15-2 - not-checked - Outside this commit; not re-read.
- F15-3 - not-checked - Outside this commit; not re-read.
- F15-5 - not-checked - Outside this commit; not re-read.
- F15-6 - not-checked - Outside this commit; not re-read.
- F16-1 - not-checked - Outside this commit; not re-read.
- F16-2 - not-checked - Outside this commit; not re-read.
- F16-3 - not-checked - Outside this commit; not re-read.
- F16-4 - not-checked - Outside this commit; not re-read.
- F16-5 - not-checked - Outside this commit; not re-read.
- F16-6 - not-checked - Outside this commit; not re-read.
- F16-7 - not-checked - Outside this commit; not re-read.
- F16-8 - not-checked - Outside this commit; not re-read.
- F16-9 - not-checked - Outside this commit; not re-read.
- F16-10 - not-checked - Outside this commit; not re-read.
- F17-1 - not-checked - Outside this commit; not re-read.
- F17-2 - not-checked - Explain availability is now record-only; the seat-mark mismatch was not re-tested.
- F17-3 - not-checked - The pins-fill-the-panel warning was read (F30-2). The empty-pool case was not re-run.
- F17-4 - not-checked - Outside this commit; not re-read.
- F17-5 - not-checked - Outside this commit; not re-read.
- F17-6 - not-checked - Outside this commit; not re-read.
- F17-7 - not-checked - Outside this commit; not re-read.
- F17-8 - not-checked - Outside this commit; not re-read.
- F17-9 - not-checked - Outside this commit; not re-read.
- F17-10 - not-checked - Outside this commit; not re-read.
- F18-1 - fixed - Under --rev, reviewed_info and finding source.base_commit come from the worktree fingerprint (ledger.rs:51).
- F18-2 - not-checked - git_cmd sets GIT_OPTIONAL_LOCKS=0. ThrowawayWorktree::create was not re-read in this pass.
- F18-3 - fixed - collab_before is taken after the patch write, and only that patch file is exempt (reviewed.rs:1503).
- F18-4 - still-open - A one-revision diff still embeds untracked content unless --diff-tracked-only is set (reviewed.rs:978).
- F18-5 - not-checked - Outside this commit; not re-read.
- F18-6 - not-checked - Outside this commit; not re-read.
- F18-7 - not-checked - Outside this commit; not re-read.
- F19-1 - not-checked - Outside this commit; not re-read.
- F19-2 - still-open - Same default as F18-4: untracked content is still in the patch. The notice and flag are new.
- F19-3 - fixed - The patch name includes the content sha8 and place_patch refuses other bytes (reviewed.rs:1604).
- F19-4 - not-checked - Outside this commit; not re-read.
- F19-5 - not-checked - Outside this commit; not re-read.
- F19-6 - fixed - Withholding uses the start-of-run HEAD, and new findings store the worktree base (orchestrate/mod.rs:2725, ledger.rs:51).
- F20-1 - not-checked - Outside this commit; not re-read.
- F20-2 - not-checked - Outside this commit; not re-read.
- F20-3 - fixed - Different bytes get a different content-addressed name (reviewed.rs:1604).
- F20-4 - not-checked - Outside this commit; not re-read.
- F20-5 - not-checked - Collab paths are left out of the isolation list. Other uncommitted names are still inserted.
- F20-6 - still-open - Same default as F18-4. MCP cannot set the new flag (finding #5).
- F24-1 - not-checked - Outside this commit; not re-read.
- F24-2 - fixed - When both mtimes resolve, the newer file wins (detach.rs:1403). An unresolvable note loses (finding #6).
- F24-4 - fixed - merge_parameters recurses into objects and keeps both scalars (agy.rs:447).
- F27-1 - not-checked - Outside this commit; not re-read.
- F27-2 - not-checked - Outside this commit; not re-read.
- F27-3 - fixed - is_run_patch_file matches the written name and a 32-hex temp only (reviewed.rs:1503).
- F28-1 - not-checked - Placement still removes the temp on the error path; a failed removal is reported. The no-hardlink claim race is finding #3.
- F28-2 - fixed - Excluded paths go through a temp index. The 500-path refusal is gone (reviewed.rs:873).
- F28-3 - fixed - Lossy UTF-8 is counted, stored as lossy_utf8, and named in the prompt (reviewed.rs:1600).
- F29-1 - not-checked - Outside this commit; not re-read.
- F29-2 - not-checked - Outside this commit; not re-read.
- F29-3 - fixed - rev_live_head is one resolve at start (orchestrate/mod.rs:2725).
- F30-1 - fixed - explain sets no_network and skip_preflight before panel_members (cli/router.rs:264).
- F30-2 - fixed - tier_missed_warning fires when required seats fill k (panel/plan.rs:1251).
- F30-3 - not-checked - Outside this commit; not re-read.
- F30-4 - fixed - Any invalid draw row yields Draw::Invalid for the whole block (replay.rs:291).
- F30-5 - not-checked - Outside this commit; not re-read.
- F30-6 - not-checked - Outside this commit; not re-read.
- F31-1 - not-checked - Outside this commit; not re-read.
- F31-2 - not-checked - Outside this commit; not re-read.
- F31-3 - not-checked - Outside this commit; not re-read.
- F31-4 - not-checked - Outside this commit; not re-read.
- F31-5 - not-checked - Outside this commit; not re-read.
- F31-6 - fixed - Same explain path as F30-1. seat_order_line names roster versus routed.
- F32-1 - not-checked - Outside this commit; not re-read.
- F32-2 - not-checked - Outside this commit; not re-read.
- F32-3 - fixed - A path-suffix value is not command-scanned, which covers a filename like notes&&draft.md. The rule is wider (finding #1).
- F32-4 - not-checked - Outside this commit; not re-read.
- F32-5 - not-checked - Outside this commit; not re-read.
- F36-1 - not-checked - Outside this commit; not re-read.
- F36-2 - not-checked - Outside this commit; not re-read.
- F36-3 - not-checked - Outside this commit; not re-read.
- F36-4 - not-checked - Outside this commit; not re-read.
- F36-5 - not-checked - Outside this commit; not re-read.
- F36-6 - not-checked - Outside this commit; not re-read.
- F36-7 - not-checked - Outside this commit; not re-read.
- F36-8 - not-checked - diff_excludes uses **/.c3/** and **/.eck/**. A nested leak was not executed.
- F38-1 - not-checked - Outside this commit; not re-read.
- F38-2 - not-checked - Outside this commit; not re-read.
- F38-3 - not-checked - Outside this commit; not re-read.
- F38-4 - not-checked - Outside this commit; not re-read.
- F38-5 - not-checked - Outside this commit; not re-read.
- F39-1 - not-checked - Outside this commit; not re-read.
- F39-2 - not-checked - Outside this commit; not re-read.
- F39-4 - not-checked - Outside this commit; not re-read.
- F43-1 - fixed - open_patch_dir creates and opens handoffs through Root::open_dir (reviewed.rs:1306).
- F43-2 - fixed - An empty claim unchanged for 60s is removed and the name is claimed again (reviewed.rs:1188).
- F43-3 - still-open - The identity check still calls remove_file by name. Finding #3.
- F43-4 - not-checked - Outside this commit; not re-read.
- F43-5 - not-checked - Outside this commit; not re-read.
- F43-6 - not-checked - Outside this commit; not re-read.
- F43-7 - not-checked - Outside this commit; not re-read.
- F43-8 - not-checked - Outside this commit; not re-read.
- F44-1 - fixed - The empty-claim block is replaced by the 60s takeover (reviewed.rs:1188). A live owner can still be overtaken (finding #3).
- F44-2 - not-checked - Outside this commit; not re-read.
- F44-3 - not-checked - Outside this commit; not re-read.
- F45-1 - not-checked - Outside this commit; not re-read.
- F45-2 - not-checked - Outside this commit; not re-read.
- F45-3 - not-checked - Outside this commit; not re-read.
- F45-4 - not-checked - Outside this commit; not re-read.
- F45-5 - not-checked - Outside this commit; not re-read.
- F45-6 - not-checked - Outside this commit; not re-read.
- F45-7 - not-checked - Outside this commit; not re-read.
- F45-8 - not-checked - Outside this commit; not re-read.
- F45-9 - not-checked - Outside this commit; not re-read.
- F46-1 - not-checked - Outside this commit; not re-read.
- F46-2 - not-checked - Outside this commit; not re-read.
- F46-3 - not-checked - Outside this commit; not re-read.
- F46-4 - not-checked - Outside this commit; not re-read.
- F46-5 - not-checked - Outside this commit; not re-read.
- F46-6 - not-checked - Outside this commit; not re-read.
- F46-7 - not-checked - Outside this commit; not re-read.
- F46-8 - not-checked - Outside this commit; not re-read.
- F47-1 - not-checked - Outside this commit; not re-read.
- F47-2 - not-checked - Outside this commit; not re-read.
- F47-3 - not-checked - Outside this commit; not re-read.
- F47-4 - not-checked - Outside this commit; not re-read.
- F48-1 - not-checked - Outside this commit; not re-read.
- F48-2 - not-checked - Outside this commit; not re-read.
- F48-3 - not-checked - Outside this commit; not re-read.
- F48-4 - not-checked - Outside this commit; not re-read.
- F48-5 - not-checked - Outside this commit; not re-read.

## Verdict: HOLD

An allow-listed agy reader can hide a command under a data-shaped key and stay usable, a --rev brief can be recorded under a hash the worktree did not get, and patch-claim cleanup can delete or replace another run's file.

### Blockers

- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, not-checked) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, not-checked) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, not-checked) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.
- **F19-1** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision.
- **F32-1** (prior, not-checked) `crates/c3/src/engines/worktree.rs:248` - Stage creation and publication use pathnames after validation, so a replaced directory component can send staged bytes outside the worktree and leave a temporary file there.

### Unproven scenarios

- No test in this review was executed. The changelog's library, panel, kimi and compat counts were not reproduced.
- A real agy, muse or grok CLI was not run against a bound input that changes mid-review.
- http --rev was not run end to end.
- The no-hardlink claim race and a kill during place_without_link were not executed.
- HEAD moving between committed_brief and stage_bytes was not executed.
- Whether a split index rewrites .git/sharedindex while GIT_INDEX_FILE points at the temp index was not checked.
- README.md and plugin/skills/consult/reference.md diffs were not read.

### First-run checklist (observable)

_(none)_
