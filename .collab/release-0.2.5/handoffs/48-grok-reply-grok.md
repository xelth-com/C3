# Handoff 48 - Grok (grok): reply-grok

Date: 2026-11-24 09:21 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (~/.grok/auth.json) - subscription (grok.com oauth).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 0ce1d200-db1f-4c80-82eb-71baa298b2d6.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-b8722d1eb6324e3f9f6538c09ad76852.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-8cc3d46d790d45fb8bb074d513d45a0b` of revision ac26f5a61672930bbb307273a29141acc14a7c44 (--rev ac26f5a61672930bbb307273a29141acc14a7c44; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20c95-4b97-77b3-8e36-3c037f618986` (source: events).
Brief: `.collab/release-0.2.5/handoffs/42-claude-verify-folder-move.md` (sha256 e566f99886d1). Reviewed: ac26f5a, base ac26f5a61672930bbb307273a29141acc14a7c44, tree sha256 13727672ad5e, 0 changed files. Live HEAD at launch: ac26f5a61672930bbb307273a29141acc14a7c44.
Bridge outcome: usable reply. Wall time: 1029.2 s. Tokens: in 178552 (cached 2754048), out 64358, reasoning 47822.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The finish rewrite and the pre-rename leftover move close the original split, but a lock waiter can still create a second .eck and a symlinked .c3 is still written through by consult. Findings: 0 blocker, 3 major, 2 minor, 0 note (F48-1..F48-5, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/48-grok-reply-grok.reply.json`.
Raw event stream: `handoffs/48-grok-reply-grok.events.jsonl`.
Verbatim reply follows.

---

## Q1. Two folders

Two `c3 finish` processes no longer recreate `.eck/`. `write_answer` goes through `write_in` (`crates/c3/src/finish/mod.rs:182`), which remaps a legacy path once `.eck` is gone (`crates/c3/src/state_dir.rs:390`, `crates/c3/src/state_dir.rs:1543`).

A waiter that gives up while the holder is still before the rename can. `MoveLock::acquire` waits 10s (`crates/c3/src/state_dir.rs:105`, `crates/c3/src/state_dir.rs:488`) and `after_failure` (`crates/c3/src/state_dir.rs:784`) keeps `.eck` when `.c3` is absent. The holder is still inside `migrate_legacy` (`crates/c3/src/state_dir.rs:713` through `crates/c3/src/state_dir.rs:735`). `c3 explain` with no `--out`, and `c3 index` when the Surreal feature is on, then `create_dir_all` that captured path (`crates/c3/src/pack/explain.rs:291`, `crates/c3/src/cli/index.rs:823`) and create a new `.eck/` beside `.c3/`.

`take_over_if_stale` (`crates/c3/src/state_dir.rs:626`) also drops a lock older than 600s while `pid_alive` is still true (`crates/c3/src/state_dir.rs:107`). A `begin_run` stuck in `create_dir_all` (`crates/c3/src/consult/orchestrate/mod.rs:3775`) loses the lock; the new mover can rename, and the first process creates `.eck/` again. Pid reuse with a recorded start time counts as gone (`crates/c3/src/liveness/proc.rs:149`). An empty recorded start time matches any live pid, so that reuse waits out the 10 minutes.

## Q2. Crash points

A kill before `std::fs::rename` (`crates/c3/src/state_dir.rs:735`) leaves leftovers under `.eck/legacy/`; the next `migrate_legacy` renames. A kill after the rename leaves them already moved.

`finish_half_done` (`crates/c3/src/state_dir.rs:930`) runs only when `update_seq` or `telemetry_queue.json` is still at the top (`crates/c3/src/state_dir.rs:73`). An S1 tree whose markers were already moved and whose `anchor` rename failed, or a tree that only had `anchor`, keeps that file at `.c3/anchor`. `read_anchor` (`crates/c3/src/pack/snapshot.rs:657`) treats it as C3's baseline. A linked `lastsnapshot` is noted and does not block the rename (`crates/c3/src/state_dir.rs:896`), so the old report can stay under `lastsnapshot/` and is not retried once the markers are gone. A busy leftover, or a `legacy` that is a link, defers the rename and the next writing command retries; it stops the move only while that obstacle remains.

## Q3. Read-only commands

`c3 pack`, `c3 explain --out`, `c3 index` query/stats, `c3 providers`, `c3 findings`, `c3 consult --status`, `--dry-run`, `c3 work --status`, and `c3 doctor` without `--fix` use `collab_dir` or `read_path` and do not call `migrate_legacy`. `c3 explain` without `--out`, `c3 index` build, `c3 doctor --fix`, finish, init, snapshot, scout, and fetch still move; those are writers. No MCP read path in this diff calls `ensure`.

## Q4. Collab spellings

`sub/../.eck/collab`, `./.eck/collab`, an absolute in-repo path, and a different ASCII case classify as the legacy folder (`crates/c3/src/state_dir.rs:1237`, `crates/c3/src/state_dir.rs:153`).

`repo_relative_normal` returns the lexical path as soon as it lies in the repository (`crates/c3/src/state_dir.rs:1245`), so an in-repo 8.3 name such as `ECK~1/collab` is never canonicalized. A Windows trailing dot or space (`.eck.` or `.eck `) is a different component, and Win32 then creates or opens `.eck`. A first component that is a symlink (`alias/collab` pointing at `.eck`) is not classified; `create_dir_all` follows it. `resolve_collab_root` then canonicalizes (`crates/c3/src/providers.rs:2557`) and follows that link.

## Q5. Live runs and stuck moves

An unlistable directory, an unreadable record, a held `.consult.lock`, and an unreadable start time all fail closed (`crates/c3/src/state_dir.rs:971`, `crates/c3/src/state_dir.rs:1094`, `crates/c3/src/consult/detach.rs:1644`). `begin_run` holds the same lock while the first records are created (`crates/c3/src/state_dir.rs:1196`), so a consultation or work start cannot enter between the check and the rename unless the lock is stolen (Q1).

A directory symlink or junction outside `snapshots`, `lastsnapshot`, `scouts`, `index`, `explains`, and `legacy` makes `walk_runs` report a live run (`crates/c3/src/state_dir.rs:998`) and the move never proceeds. A dead run's ordinary `.consult.lock` is not held, so the move is allowed (`crates/c3/src/state_dir.rs:1173`). The walk is three levels (`crates/c3/src/state_dir.rs:972`); a task at `.eck/a/b/c/<task>` is not inspected. A lock whose owner is gone is removed (`crates/c3/src/state_dir.rs:611`).

## Q6. Links

`link_problem` (`crates/c3/src/state_dir.rs:197`) is enforced by `ensure` and `c3 doctor` only. `c3 consult`, `c3 panel`, and `c3 work` never call it. With `--collab-dir .c3/collab` and `.c3` a symlink, `canonicalize` follows it (`crates/c3/src/providers.rs:2557`) and `create_dir_all` follows it (`crates/c3/src/consult/orchestrate/mod.rs:3775`). `read_path` uses `Path::is_dir` (`crates/c3/src/state_dir.rs:139`), so a symlinked `.eck` is read through. A component swapped for a link between the metadata check and `rename` (`crates/c3/src/state_dir.rs:705`, `crates/c3/src/state_dir.rs:735`) is not caught; on Unix the rename moves the link, so `.c3` becomes that link. A hard link is not `is_link` (`crates/c3/src/setup/repo_models.rs:463`); `write_in` truncates every name. `git_common_dir` uses following `metadata` (`crates/c3/src/state_dir.rs:534`), so a symlinked `.git` directory receives the lock file.

## Q7. Claims

The switch matches the code: with `C3_STATE_MIGRATE=off`, C3 does not rename `.eck/`, creates `.c3/` for its own files, and uses a collab path under `.eck/` as given (README.md:707, `crates/c3/src/state_dir.rs:1383`).

CHANGELOG.md:518 says the lock is taken over only when the owner is gone; the 600s rule also takes it from a live owner. README.md:697 says any leftover `anchor` is repaired; that happens only while a marker file is still at the top. README.md:710 and CHANGELOG.md:527 say every writing command refuses a `.c3` link and that C3 never reads through a link; consult and `read_path` still follow one.

## Q8. Untested

Nothing holds the mover past the lock wait and then checks `c3 explain` or `c3 index` for a second `.eck/`. Nothing points `c3 consult` at a symlinked `.c3`. Nothing is a half-done `.c3/` that contains only `anchor`. Nothing covers a Windows trailing dot, an 8.3 component, or a symlink component. `nested_state_folders_are_kept_out_of_a_diff_patch` covers a committed nested `--diff` exclusion; I did not run it. The check-then-rename swap is untested.

## Requested checks

RC1. In a disposable repo, hold `CODEX_CONSULT_TEST_STATE_MOVE_HOLD` past 10s inside `migrate_legacy` and run `c3 explain` with no `--out`. Workspace-write, about 1 minute. Finding #2: a new `.eck/` appears beside `.c3/` and the explainer file is in the new folder.

RC2. Point `<repo>/.c3` at an outside directory that already has `collab/`, then run `c3 consult --collab-dir .c3/collab` with a fake engine that returns immediately. Workspace-write, about 1 minute. Finding #1: the outside `collab/` gains a task directory and the command does not refuse.

RC3. Put a non-empty `anchor` at the top of `.c3/` and omit `update_seq` and `telemetry_queue.json`, then run `c3 snapshot --delta`. Workspace-write, about 1 minute. Finding #3: the snapshot uses that file as C3's baseline.

---

### Findings

- **F48-1** [major] `crates/c3/src/providers.rs:2557`, `crates/c3/src/state_dir.rs:139`, `crates/c3/src/state_dir.rs:197`, `crates/c3/src/consult/orchestrate/mod.rs:3775`, `README.md:710` - A symlinked .c3 or .eck is still followed by consult, panel, and work. link_problem refuses only inside ensure and c3 doctor. resolve_collab_root canonicalizes the collab path, and create_dir_all follows an intermediate symlink, so --collab-dir .c3/collab writes the task directory into the link target. read_path uses Path::is_dir, so read-only commands also read through a symlinked .eck. README.md:710 says C3 never reads or writes through a link and that every writing command refuses a .c3 link. Trigger: The repository's .c3 is a symlink to a directory, and c3 consult, c3 panel, or c3 work is started with --collab-dir .c3/collab. The same follow happens for --collab-dir .eck/collab when .eck itself is a symlink. Evidence: read-code: After collab_dir or collab_dir_moving returns, resolve_collab_root_with always std::fs::canonicalize's the path, which follows symlinks, and falls back to the uncanonical path when canonicalize fails.; read-code: link_problem is implemented and called from ensure and doctor. A repo-wide search shows no other caller, so consult, panel, and work never refuse a .c3 link.; read-code: read_path rejects a .c3 link, then returns .eck when Path::is_dir is true. Path::is_dir follows a symlink, so the returned path is the link and later reads follow it. Verify: RC2: point .c3 at an outside directory and run c3 consult --collab-dir .c3/collab; the outside collab directory receives the task files and the process does not print the link refusal. Remedy: Before any writing command uses a collab path, refuse when .c3 or .eck is a link, using the same link_problem test as ensure. Do that on the path before canonicalize, and make read_path use symlink_metadata for .eck the way it already does for .c3. Supersedes: F36-3.
- **F48-2** [major] `crates/c3/src/state_dir.rs:488`, `crates/c3/src/state_dir.rs:626`, `crates/c3/src/state_dir.rs:784`, `crates/c3/src/pack/explain.rs:291`, `crates/c3/src/cli/index.rs:823`, `CHANGELOG.md:518` - A process that loses the move lock, or has the lock taken over while it still lives, can create a second .eck after the winner renames. after_failure adopts .eck whenever .c3 is not there yet. c3 explain (no --out) and c3 index then create_dir_all that path with no later remap. take_over_if_stale removes a lock older than 600 seconds even when the owner is alive, which CHANGELOG.md:518 does not say. Trigger: One process holds the move lock inside migrate_legacy for more than 10 seconds before renaming (a large live-run walk, or a leftover rename that blocks and then succeeds), while c3 explain with no --out or c3 index build is waiting. Separately, a begin_run that still holds the lock after 10 minutes has it removed and the folder renamed underneath its create_dir_all. Evidence: read-code: Acquire returns the holder text after lock_wait (default 10s). after_failure sets deferred only when .c3 is absent and .eck exists, so the caller is told to use .eck while the holder has not renamed yet.; read-code: explain prepares the legacy explains directory and then create_dir_all's that parent and writes the file. It does not call write_in or moved_with_folder at the write.; read-code: A parsed lock is stale when the owner is gone or when file age is greater than LOCK_STALE_SECS (600). The age clause does not require the owner to be dead. Verify: RC1: hold CODEX_CONSULT_TEST_STATE_MOVE_HOLD past 10s and run c3 explain with no --out; after the holder renames, compare the explainer path with .c3/explains and see whether a new .eck/ exists. Remedy: Have explain and index write through write_in or prepare_dir at the write itself, and refuse to create .eck when it is absent. Take a lock over only when the owner is gone, or keep the wait until the holder drops the lock instead of proceeding on the legacy path.
- **F48-3** [major] `crates/c3/src/state_dir.rs:73`, `crates/c3/src/state_dir.rs:930`, `crates/c3/src/pack/snapshot.rs:657`, `README.md:697` - A half-done .c3 that has the old tool's anchor and no longer has update_seq or telemetry_queue.json at its top is never repaired. finish_half_done requires one of those two markers, and read_anchor then uses the leftover anchor as C3's snapshot baseline. README.md:697 says any anchor an earlier build left at the top is put right. Trigger: An S1 move already renamed .eck to .c3, moved update_seq and telemetry_queue.json into legacy/, and failed or was killed before moving anchor. Or the old folder contained only anchor. A later c3 snapshot --delta or c3 finish --snapshot runs. Evidence: read-code: HALF_DONE_MARKERS lists only update_seq and telemetry_queue.json. finish_half_done returns immediately when neither name exists at the top, so a lone anchor is left in place. The comment says C3's own anchor must stay, which is why anchor alone is not a marker.; read-code: The S1 loop renamed the folder first and then moved leftovers one by one, continuing after a single rename failure. A failed anchor rename after the markers had moved leaves exactly the state finish_half_done now ignores.; read-code: read_anchor reads state_dir/anchor and accepts any non-empty trimmed string, with no check that the file lives under legacy/. Verify: RC3: write only anchor at the top of .c3 and run c3 snapshot --delta; the delta base is that file's contents. Remedy: Treat a top-level anchor as half-done when legacy/ already holds the old markers, or when anchor's bytes are not a C3 snapshot id. Keep a C3-written anchor where it is when those signs are absent. Supersedes: F36-2.
- **F48-4** [minor] `crates/c3/src/state_dir.rs:1245`, `crates/c3/src/state_dir.rs:153`, `crates/c3/src/state_dir.rs:1474` - Collab spellings that the file system treats as .eck are not classified, so after the move a writer can create a new .eck. An in-repo 8.3 component is returned before canonicalization. A Windows trailing dot or space is a different component and Win32 maps it back to .eck. A symlink used as the first component is not a state path, and prepare_dir's create_dir_all follows it. Trigger: After .eck has been renamed to .c3, a command is given --collab-dir .eck. or .eck /collab on Windows, or ECK~1/collab where that is the short name of the old folder, or alias/collab where alias is a symlink to .eck. Evidence: read-code: repo_relative_normal returns the lexical repo-relative path as soon as repo_relative succeeds. canonical_prefix runs only when the lexical path is outside the repository, so an in-repo short name is never expanded.; read-code: names_equal folds ASCII case on Windows and macOS and otherwise compares bytes. .eck. and ECK~1 are not equal to .eck, so split_state_path returns None and resolve_collab returns the path unchanged.; read-code: A path that does not classify as .c3 or .eck is created with create_dir_all, which follows intermediate links and, on Windows, the trailing-dot normalization of the API. Verify: On Windows, after a completed move, run a writer with --collab-dir .eck./collab and list the repository root for a new .eck directory. Remedy: Canonicalize with the platform's full-path rules, including trailing dots and short names, before classification, and treat a symlink component that resolves inside .c3 or .eck as that folder.
- **F48-5** [minor] `crates/c3/src/state_dir.rs:735`, `crates/c3/src/state_dir.rs:998`, `crates/c3/src/setup/repo_models.rs:463` - Two link cases remain around the move. A directory symlink or junction anywhere in the three-level walk, outside the six skipped names, blocks the move for good and every later writing command reports that a run may be live. Between the metadata check and rename, a component swapped for a link is not rechecked; a hard link is not classified as a link, so write_in updates every name that shares the file. Trigger: .eck contains a directory junction that is not snapshots, lastsnapshot, scouts, index, explains, or legacy, and any writing command runs. Or a local writer replaces .eck with a symlink after the locked metadata check and before rename, or hard-links .c3/JOURNAL.md outside the repository before a finish write. Evidence: read-code: A link that is not in NOT_RUN_PLACES and whose followed metadata is a directory, or whose metadata cannot be read, returns immediately as a possible live run. Nothing later distinguishes a user junction from a task.; read-code: The locked re-check uses symlink_metadata, then relocate_leftovers, then std::fs::rename on the same path. There is no directory handle tying the rename to the directory that was checked.; read-code: is_link is symlink on Unix and any reparse point on Windows. A hard link has neither, so the write path does not refuse it. Verify: Put a directory junction under .eck/collab-link and run c3 finish; the command keeps using .eck and the note says a run may be live. Repeat with the junction removed and the move completes. Remedy: Name the blocking link in the deferral and document that removing it unblocks the move. For the rename, open the directory and rename that handle, and refuse a destination whose link count is greater than one when the bytes would change.

### Prior findings

- F02-1 - not-checked - Not in this commit's diff; not re-checked.
- F02-2 - not-checked - Not in this commit's diff; not re-checked.
- F02-3 - not-checked - Not in this commit's diff; not re-checked.
- F02-4 - not-checked - Not in this commit's diff; not re-checked.
- F02-5 - not-checked - Not in this commit's diff; not re-checked.
- F02-6 - not-checked - Not in this commit's diff; not re-checked.
- F02-7 - not-checked - Not in this commit's diff; not re-checked.
- F02-8 - not-checked - Not in this commit's diff; not re-checked.
- F02-9 - not-checked - Not in this commit's diff; not re-checked.
- F02-10 - not-checked - Not in this commit's diff; not re-checked.
- F02-11 - not-checked - Not in this commit's diff; not re-checked.
- F02-12 - not-checked - Not in this commit's diff; not re-checked.
- F02-13 - not-checked - Not in this commit's diff; not re-checked.
- F02-14 - not-checked - Not in this commit's diff; not re-checked.
- F02-15 - not-checked - Not in this commit's diff; not re-checked.
- F02-16 - not-checked - Not in this commit's diff; not re-checked.
- F02-17 - not-checked - Not in this commit's diff; not re-checked.
- F02-18 - not-checked - Not in this commit's diff; not re-checked.
- F02-19 - not-checked - Not in this commit's diff; not re-checked.
- F02-20 - not-checked - Not in this commit's diff; not re-checked.
- F02-21 - not-checked - Not in this commit's diff; not re-checked.
- F03-1 - not-checked - Not in this commit's diff; not re-checked.
- F03-2 - not-checked - Not in this commit's diff; not re-checked.
- F03-3 - not-checked - Not in this commit's diff; not re-checked.
- F03-4 - not-checked - Not in this commit's diff; not re-checked.
- F03-5 - not-checked - Not in this commit's diff; not re-checked.
- F03-6 - not-checked - Not in this commit's diff; not re-checked.
- F04-1 - not-checked - Not in this commit's diff; not re-checked.
- F04-2 - not-checked - Not in this commit's diff; not re-checked.
- F04-3 - not-checked - Not in this commit's diff; not re-checked.
- F04-4 - not-checked - Not in this commit's diff; not re-checked.
- F04-5 - not-checked - Not in this commit's diff; not re-checked.
- F04-6 - not-checked - Not in this commit's diff; not re-checked.
- F04-7 - not-checked - Not in this commit's diff; not re-checked.
- F04-8 - not-checked - Not in this commit's diff; not re-checked.
- F04-9 - not-checked - Not in this commit's diff; not re-checked.
- F10-1 - not-checked - Not in this commit's diff; not re-checked.
- F10-2 - not-checked - Not in this commit's diff; not re-checked.
- F10-3 - not-checked - Not in this commit's diff; not re-checked.
- F10-4 - not-checked - Not in this commit's diff; not re-checked.
- F10-5 - not-checked - Not in this commit's diff; not re-checked.
- F10-7 - not-checked - Not in this commit's diff; not re-checked.
- F11-1 - not-checked - Not in this commit's diff; not re-checked.
- F11-2 - not-checked - Not in this commit's diff; not re-checked.
- F11-3 - not-checked - Not in this commit's diff; not re-checked.
- F11-4 - not-checked - Not in this commit's diff; not re-checked.
- F11-5 - not-checked - Not in this commit's diff; not re-checked.
- F11-6 - not-checked - Not in this commit's diff; not re-checked.
- F11-7 - not-checked - Not in this commit's diff; not re-checked.
- F11-8 - not-checked - Not in this commit's diff; not re-checked.
- F11-9 - not-checked - Not in this commit's diff; not re-checked.
- F11-10 - not-checked - Not in this commit's diff; not re-checked.
- F11-11 - not-checked - Not in this commit's diff; not re-checked.
- F11-12 - not-checked - Not in this commit's diff; not re-checked.
- F12-1 - not-checked - Not in this commit's diff; not re-checked.
- F12-2 - not-checked - Not in this commit's diff; not re-checked.
- F12-3 - not-checked - Not in this commit's diff; not re-checked.
- F12-4 - not-checked - Not in this commit's diff; not re-checked.
- F13-1 - not-checked - Not in this commit's diff; not re-checked.
- F13-2 - not-checked - Not in this commit's diff; not re-checked.
- F13-3 - not-checked - Not in this commit's diff; not re-checked.
- F13-4 - not-checked - Not in this commit's diff; not re-checked.
- F13-5 - not-checked - Not in this commit's diff; not re-checked.
- F13-6 - not-checked - Not in this commit's diff; not re-checked.
- F13-7 - not-checked - Not in this commit's diff; not re-checked.
- F13-8 - not-checked - Not in this commit's diff; not re-checked.
- F13-9 - not-checked - Not in this commit's diff; not re-checked.
- F14-1 - not-checked - Not in this commit's diff; not re-checked.
- F14-2 - not-checked - Not in this commit's diff; not re-checked.
- F14-3 - not-checked - Not in this commit's diff; not re-checked.
- F14-4 - not-checked - Not in this commit's diff; not re-checked.
- F14-5 - not-checked - Not in this commit's diff; not re-checked.
- F15-1 - not-checked - Not in this commit's diff; not re-checked.
- F15-2 - not-checked - Not in this commit's diff; not re-checked.
- F15-3 - not-checked - Not in this commit's diff; not re-checked.
- F15-5 - not-checked - Not in this commit's diff; not re-checked.
- F15-6 - not-checked - Not in this commit's diff; not re-checked.
- F16-1 - not-checked - Not in this commit's diff; not re-checked.
- F16-2 - not-checked - Not in this commit's diff; not re-checked.
- F16-3 - not-checked - Not in this commit's diff; not re-checked.
- F16-4 - not-checked - Not in this commit's diff; not re-checked.
- F16-5 - not-checked - Not in this commit's diff; not re-checked.
- F16-6 - not-checked - Not in this commit's diff; not re-checked.
- F16-7 - not-checked - Not in this commit's diff; not re-checked.
- F16-8 - not-checked - Not in this commit's diff; not re-checked.
- F16-9 - not-checked - Not in this commit's diff; not re-checked.
- F16-10 - not-checked - Not in this commit's diff; not re-checked.
- F17-1 - not-checked - Not in this commit's diff; not re-checked.
- F17-2 - not-checked - Not in this commit's diff; not re-checked.
- F17-3 - not-checked - Not in this commit's diff; not re-checked.
- F17-4 - not-checked - Not in this commit's diff; not re-checked.
- F17-5 - not-checked - Not in this commit's diff; not re-checked.
- F17-6 - not-checked - Not in this commit's diff; not re-checked.
- F17-7 - not-checked - Not in this commit's diff; not re-checked.
- F17-8 - not-checked - Not in this commit's diff; not re-checked.
- F17-9 - not-checked - Not in this commit's diff; not re-checked.
- F17-10 - not-checked - Not in this commit's diff; not re-checked.
- F18-1 - not-checked - Not in this commit's diff; not re-checked.
- F18-2 - not-checked - Not in this commit's diff; not re-checked.
- F18-3 - not-checked - Not in this commit's diff; not re-checked.
- F18-4 - not-checked - Not in this commit's diff; not re-checked.
- F18-5 - not-checked - Not in this commit's diff; not re-checked.
- F18-6 - not-checked - Not in this commit's diff; not re-checked.
- F18-7 - not-checked - Not in this commit's diff; not re-checked.
- F19-1 - not-checked - Not in this commit's diff; not re-checked.
- F19-2 - not-checked - Not in this commit's diff; not re-checked.
- F19-3 - not-checked - Not in this commit's diff; not re-checked.
- F19-4 - not-checked - Not in this commit's diff; not re-checked.
- F19-5 - not-checked - Not in this commit's diff; not re-checked.
- F19-6 - not-checked - Not in this commit's diff; not re-checked.
- F20-1 - not-checked - Not in this commit's diff; not re-checked.
- F20-2 - not-checked - Not in this commit's diff; not re-checked.
- F20-3 - not-checked - Not in this commit's diff; not re-checked.
- F20-4 - not-checked - Not in this commit's diff; not re-checked.
- F20-5 - not-checked - Not in this commit's diff; not re-checked.
- F20-6 - not-checked - Not in this commit's diff; not re-checked.
- F24-1 - not-checked - Not in this commit's diff; not re-checked.
- F24-2 - not-checked - Not in this commit's diff; not re-checked.
- F24-4 - not-checked - Not in this commit's diff; not re-checked.
- F27-1 - not-checked - Not in this commit's diff; not re-checked.
- F27-2 - not-checked - Not in this commit's diff; not re-checked.
- F27-3 - not-checked - Not in this commit's diff; not re-checked.
- F28-1 - not-checked - Not in this commit's diff; not re-checked.
- F28-2 - not-checked - Not in this commit's diff; not re-checked.
- F28-3 - not-checked - Not in this commit's diff; not re-checked.
- F29-1 - not-checked - Not in this commit's diff; not re-checked.
- F29-2 - not-checked - Not in this commit's diff; not re-checked.
- F29-3 - not-checked - Not in this commit's diff; not re-checked.
- F30-1 - not-checked - Not in this commit's diff; not re-checked.
- F30-2 - not-checked - Not in this commit's diff; not re-checked.
- F30-3 - not-checked - Not in this commit's diff; not re-checked.
- F30-4 - not-checked - Not in this commit's diff; not re-checked.
- F30-5 - not-checked - Not in this commit's diff; not re-checked.
- F30-6 - not-checked - Not in this commit's diff; not re-checked.
- F31-1 - not-checked - Not in this commit's diff; not re-checked.
- F31-2 - not-checked - Not in this commit's diff; not re-checked.
- F31-3 - not-checked - Not in this commit's diff; not re-checked.
- F31-4 - not-checked - Not in this commit's diff; not re-checked.
- F31-5 - not-checked - Not in this commit's diff; not re-checked.
- F31-6 - not-checked - Not in this commit's diff; not re-checked.
- F32-1 - not-checked - Not in this commit's diff; not re-checked.
- F32-2 - not-checked - Not in this commit's diff; not re-checked.
- F32-3 - not-checked - Not in this commit's diff; not re-checked.
- F32-4 - not-checked - Not in this commit's diff; not re-checked.
- F32-5 - not-checked - Not in this commit's diff; not re-checked.
- F36-1 - fixed - Two c3 finish processes now write through write_in, which remaps .eck after the rename. A second folder can still be created by explain and index; that is finding #2.
- F36-2 - still-open - A kill with the markers still at the top is repaired. A lone anchor, after the markers were already moved, is not. Finding #3 replaces that remainder.
- F36-3 - still-open - ensure, the cache move, and a linked lastsnapshot no longer follow. consult still follows a .c3 symlink. Finding #1 replaces that remainder.
- F36-4 - fixed - pack, explain --out, index query, and the other named readers use collab_dir or read_path and do not call migrate_legacy.
- F36-5 - fixed - sub/../.eck/collab is classified after lexical normalization. Trailing-dot, 8.3, and symlink-component spellings remain as finding #4.
- F36-6 - fixed - Unreadable directories, records, and start times fail closed, and begin_run holds the move lock across the first record write. A directory junction can block the move forever, and a 10-minute takeover can reopen the gap; those are findings #2 and #5.
- F36-7 - fixed - README and the config comment now say the switch does not move .eck, uses .c3 for C3's files, and still uses a collab path under .eck.
- F36-8 - fixed - diff_excludes uses :(exclude,glob)**/.c3/** and the same for .eck, with icase on Windows and macOS. The nested unit test is in source; it was not executed.
- F38-1 - not-checked - Not in this commit's diff; not re-checked.
- F38-2 - not-checked - Not in this commit's diff; not re-checked.
- F38-3 - not-checked - Not in this commit's diff; not re-checked.
- F38-4 - not-checked - Not in this commit's diff; not re-checked.
- F38-5 - not-checked - Not in this commit's diff; not re-checked.
- F39-1 - not-checked - Not in this commit's diff; not re-checked.
- F39-2 - not-checked - Not in this commit's diff; not re-checked.
- F39-4 - not-checked - Not in this commit's diff; not re-checked.
- F43-1 - not-checked - Not in this commit's diff; not re-checked.
- F43-2 - not-checked - Not in this commit's diff; not re-checked.
- F43-3 - not-checked - Not in this commit's diff; not re-checked.
- F43-4 - not-checked - Not in this commit's diff; not re-checked.
- F43-5 - not-checked - Not in this commit's diff; not re-checked.
- F43-6 - not-checked - Not in this commit's diff; not re-checked.
- F43-7 - not-checked - Not in this commit's diff; not re-checked.
- F43-8 - not-checked - Not in this commit's diff; not re-checked.
- F44-1 - not-checked - Not in this commit's diff; not re-checked.
- F44-2 - not-checked - Not in this commit's diff; not re-checked.
- F44-3 - not-checked - Not in this commit's diff; not re-checked.
- F45-1 - not-checked - Not in this commit's diff; not re-checked.
- F45-2 - not-checked - Not in this commit's diff; not re-checked.
- F45-3 - not-checked - Not in this commit's diff; not re-checked.
- F45-4 - not-checked - Not in this commit's diff; not re-checked.
- F45-5 - not-checked - Not in this commit's diff; not re-checked.
- F45-6 - not-checked - Not in this commit's diff; not re-checked.
- F45-7 - not-checked - Not in this commit's diff; not re-checked.
- F45-8 - not-checked - Not in this commit's diff; not re-checked.
- F45-9 - not-checked - Not in this commit's diff; not re-checked.
- F46-1 - not-checked - Not in this commit's diff; not re-checked.
- F46-2 - not-checked - Not in this commit's diff; not re-checked.
- F46-3 - not-checked - Not in this commit's diff; not re-checked.
- F46-4 - not-checked - Not in this commit's diff; not re-checked.
- F46-5 - not-checked - Not in this commit's diff; not re-checked.
- F46-6 - not-checked - Not in this commit's diff; not re-checked.
- F46-7 - not-checked - Not in this commit's diff; not re-checked.
- F46-8 - not-checked - Not in this commit's diff; not re-checked.

## Verdict: HOLD

The finish rewrite and the pre-rename leftover move close the original split, but a lock waiter can still create a second .eck and a symlinked .c3 is still written through by consult.

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

- No test or command was executed; the nested diff test and the library tests named in the brief were read only as source.
- Windows trailing-dot and 8.3 creation was not executed; the classification gap is from the path code, and the Win32 mapping is from the platform rule.
- How long live_run_under takes on a real large .eck was not measured.
- On Windows, whether rename of a junction that replaced .eck moves the link or the target was not executed.
- Untracked nested .c3/.eck files in a working-tree --diff were not executed; the unit test covers a committed range.
- MCP tool bodies outside this diff were not traced past the shared resolve_collab_root and ensure entry points.

### First-run checklist (observable)

_(none)_
