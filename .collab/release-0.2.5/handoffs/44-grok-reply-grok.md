# Handoff 44 - Grok (grok): reply-grok

Date: 2026-11-23 12:11 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (usable reply 4 min ago).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 16149089-2e57-4fa3-b7cd-840d805e9136.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-6c876b404ae543dc949ce44b20c4b1ab.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-7063699cbf254422abbb56cabc507151` of revision 272920dff3ea5a59d905541f017d8bdd7b8c914b (--rev 272920dff3ea5a59d905541f017d8bdd7b8c914b; removed after the run).
Parent thread: (none - new thread). Result thread: `01a2080b-da23-7d90-bd02-ced10594bbb9` (source: events).
Brief: `.collab/release-0.2.5/handoffs/40-claude-diff-review-handles.md` (sha256 1d73ac2bb0ac). Reviewed: 272920d, base 272920dff3ea5a59d905541f017d8bdd7b8c914b, tree sha256 974cd3dbaddc, 0 changed files. Live HEAD at launch: ac26f5a61672930bbb307273a29141acc14a7c44.
Bridge outcome: usable reply. Wall time: 915.1 s. Tokens: in 480852 (cached 4436096), out 52029, reasoning 43471.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The handle walk closes the directory-swap window, but a no-hard-link patch claim whose writer dies stays under the final name and blocks every later write of that patch. Findings: 0 blocker, 1 major, 2 minor, 0 note (F44-1..F44-3, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/44-grok-reply-grok.reply.json`.
Raw event stream: `handoffs/44-grok-reply-grok.events.jsonl`.
Verbatim reply follows.

---

## Q1
Still path-based after the handle work:

- `contain.rs:284` and `:346` use `std::fs` only to classify an open that already failed. `Root::open_anchor` (`:343`) follows the anchor (repo root or handoffs directory).
- `worktree.rs:577` `canonicalize`s each link to decide inside vs outside, then unlinks on the handle. `:658`, `:787` and `:804` canonicalize to compare paths or word a refusal. `:466` uses `symlink_metadata` only when doctor has no handle, then `Root::open`. `git -C` (`:34`) still runs `worktree add` and, after removal, `worktree prune`.
- `reviewed.rs:1028` `create_dir_all` plus `open_anchor`: a symlink `handoffs/` is followed and the patch is written in the target. `:907` copies the git index to a temp file by path (`:944` deletes it).
- `repo_models.rs:205`–`:234` and `rules.rs:126`, `:148`, `:278`, `:308` still `read`/`read_dir` the snapshot (those follow). The replace is `replace_file` on a handle and refuses a link. `record_throwaway_owner` (`worktree.rs:162`, `setup/branches.rs:900`) still `std::fs::write`s `c3-owner` at the `gitdir:` path.
- On Windows, cap-std `rename` / `remove_file` / `remove_dir` / `create_dir` rebuild a path from the handle (`concatenate` + `std::fs`) and depend on no `FILE_SHARE_DELETE`.

## Q2
Linux and macOS: cap-std 4.0.3 uses `openat` + `O_NOFOLLOW`. A symlink component (file or directory, absolute or relative, inside or outside) fails that open. C3 maps it to `Step::Link` and does not follow. If cap-std is asked to follow, an absolute target is `PrefixOrRootDir` ("a path led outside of the filesystem"). C3 does not ask that for a component.

Windows: `FollowSymlinks::No` sets `FILE_FLAG_OPEN_REPARSE_POINT`, then cap-std returns `ERROR_STOPPED_ON_SYMLINK` when `is_symlink` is true. Junctions and directory symlinks are name-surrogate reparse points, so the no-follow open fails; a junction target is absolute, so a follow is also an escape. C3 adds `kind_of`: every `FILE_ATTRIBUTE_REPARSE_POINT` is a link, including tags Rust does not call symlinks. `Root::open` is no-follow, so a link root is `RootError::Link`. `open_anchor` does not set the reparse flag and follows. Directories are opened without `FILE_SHARE_DELETE`; the held directory and its parent cannot be renamed (the proof test). The changelog's "every ancestor" is wider than that test.

## Q3
An in-repo directory symlink on the way to the brief now refuses the run (`worktree.rs:859`). The same link for `handoffs/` or `state.md` is skipped and counted (`reviewed.rs:410`), with a warning (`orchestrate/mod.rs:4327`). Those inside links used to be followed. A read-only worktree file is cleared and removed on Windows (`contain.rs:622`); a file another process holds stops removal there and leaves the rest. The worktree root handle stays open for the run, so on Windows that directory and at least its parent cannot be renamed.

## Q4
`publish` deletes its temp on a returned error and reports a failed delete (`contain.rs:541`). A crash after create and before `rename` leaves `.c3-stage-*.tmp` or `.c3-repair-*.tmp`, not the final name. `remove_contents` stops at the first error, so a locked file or a reviewer still inside leaves a half-deleted worktree and does not `git worktree prune` (`worktree.rs:479`). `remove_root` drops the worktree handle, then `remove_dir` (`contain.rs:836`). The test shows a link swapped in then is removed on Windows and refused on Unix, not followed. The empty patch claim is the final-name leftover (Q5).

## Q5
Two live runs of the same bytes finish as one file. `write_patch` stores sha256 only after `place_patch` is `Ok`, and `reuse_patch` requires the same bytes, so the ledger hash matches. A crash or killed owner between the exclusive empty claim and `rename` (`reviewed.rs:1002`) leaves that empty file under the patch name. `reuse_patch` (`:970`) waits 2s, then errors "already holds other bytes" and does not delete it. Later runs do the same, including one that still has the bytes. No wrong sha256 is committed. The empty name is a false patch in `handoffs/`.

## Q6
The three new variables go through `test_hooks::hook`. An inherited value is ignored unless `CODEX_CONSULT_TEST_MODE` is `1`/`true`/`yes`/`on`, and the run warns. No repository file is read. `set_thread_hook` skips the mode gate; only an in-process caller can set it.

## Q7
`CHANGELOG.md:371` says nothing user-visible changes except the races. Inside symlinks on the way to the brief and handoffs do change (`:379`; `:380` "as before" understates the new brief refusal). `:387` says no partial patch is left under its name; the empty claim is that name, and a dead owner leaves it. `contain.rs:23` matches the cap-std mechanisms and does not claim the anchor is no-follow.

## Q8
Untested: a dead owner's claim; Windows directory symlinks where the proof skips for lack of privilege; reparse points that are not name surrogates; macOS firmlinks; `MoveFileEx` when the destination is swapped for a symlink after `dest_replaceable`; the roster writer and `c3-owner`. The two-thread test only covers live racers that both finish. Not read: `maintain/models.rs`, most test bodies, `Cargo.lock` past cap-std 4.0.3, `branches.rs` except the owner write.

## Requested checks
RC1: From the repo root (workspace-write), a harness that calls the no-hard-link claim (`create_new` of the patch name) and exits before `rename`, then calls `place_patch` with the same bytes. Observation: the name is still an empty file and the second call errors with `already holds other bytes` (finding #1). Budget: one cargo test, under 2 minutes.

---

### Findings

- **F44-1** [major] `crates/c3/src/consult/reviewed.rs:1002`, `crates/c3/src/consult/reviewed.rs:970`, `CHANGELOG.md:387` - On a filesystem without hard links, place_without_link claims the final patch name with an empty exclusive file and only then renames the full temp onto it. If that process dies between the claim and the rename, the empty file stays under the content-addressed name. reuse_patch treats an empty file as in-progress for two seconds, then fails with 'already holds other bytes' and does not remove it, so every later write of that same patch fails and the handoffs directory keeps a patch file whose bytes are not the hash in its name. Trigger: No hard links (or CODEX_CONSULT_TEST_PATCH_NO_HARDLINK with test mode), place_patch has created the empty claim, and the process is killed before Dir::rename. A sibling or a later run calls place_patch with the same bytes. Evidence: read-code: place_without_link create_new's the final name, then rename's the temp onto it. remove_file(leaf) runs only if that rename returns an error.; read-code: reuse_patch returns Ok only when the bytes match. An empty file is slept on for 2s; after the deadline it hits the 'already holds other bytes' error. Nothing deletes the empty file.; read-code: plan.sha256 is assigned only after place_patch returns Ok, so a failed reuse does not record a hash. The empty file remains on disk.; read-code: The no-hard-link test covers a failed write, a live sibling, and two live racers. It does not kill the claimant between create_new and rename. Verify: Kill the process after the empty create_new and before rename, then call place_patch again with the same bytes. The name is still empty and the call errors with 'already holds other bytes'. Remedy: Do not treat a stale empty claim as foreign content. If the file is still empty after the wait, or its writer is gone, remove it and retry the claim once. Put the claim beside the final name (a lock file with pid and start time) so a crash never publishes an empty patch under the hash name.
- **F44-2** [minor] `CHANGELOG.md:371`, `crates/c3/src/engines/worktree.rs:859`, `crates/c3/src/consult/reviewed.rs:397` - A symlink directory that stays inside the repository on the way to the brief now refuses the run, and the same link for handoffs/ or state.md is skipped. Those paths were followed before. CHANGELOG.md:371 says nothing changes for a user except the race windows; line 380's 'refuses the run as before' understates the new brief refusal. Trigger: The brief, handoffs/, or state.md is reached through a directory symlink whose target is still inside the repository. Evidence: read-code: link_source_problem and read_live_source use Root::kind / Root::read, which return Step::Link for any component that is a link, including one that stays inside.; read-code: open_dir or read of handoffs/ and state.md maps Step::Link to links_skipped and does not copy the entry.; read-code: The entry says nothing changes for a user, then says an inside link was followed before and a linked brief refuses as before. Verify: Point a brief at docs/brief.md where docs is a symlink to another directory inside the same repo and run a worktree review. The run is refused. Repeat with handoffs as that symlink and confirm the files are not staged. Remedy: Either follow an inside directory symlink once while opening the anchor and then contain only below that handle, or change the changelog so the new refusal and skip are the user-visible exception.
- **F44-3** [minor] `crates/c3/src/contain.rs:223` - open_file_in decides the name is a regular file with symlink_metadata, then opens it. A local swap of that name for a FIFO between the two calls opens the FIFO and can block the consultation forever. The comment says a FIFO is never opened. Trigger: Between entry() and open_with, the name is replaced by a FIFO and the open is for reading. Evidence: read-code: The kind check returns before open only for the kind seen by symlink_metadata. The handle metadata check runs after open_with, so a FIFO swapped in is opened first. Verify: Race a FIFO onto a file name that open_file_in has already classified as a regular file and observe the read blocks until a writer opens the FIFO. Remedy: Open with a non-blocking flag, or open first with no-follow and reject from the handle metadata before any blocking read.

### Prior findings

_(none)_

## Verdict: HOLD

The handle walk closes the directory-swap window, but a no-hard-link patch claim whose writer dies stays under the final name and blocks every later write of that patch.

### Blockers

_(none)_

### Unproven scenarios

- Whether Windows MoveFileEx with MOVEFILE_REPLACE_EXISTING follows a destination symlink swapped in after dest_replaceable (cap-std rename rebuilds a path; not executed).
- Rename lock on ancestors above the held directory's parent (the proof only renames the held directory and its parent).
- Non-name-surrogate reparse points (cloud placeholders) when the open still hydrates despite FILE_OPEN_REPARSE_POINT.
- macOS firmlinks and the Windows directory-symlink half of the proof on a machine that cannot create them.
- A dead-owner patch claim was not executed; the conclusion is from the control flow.

### First-run checklist (observable)

_(none)_
