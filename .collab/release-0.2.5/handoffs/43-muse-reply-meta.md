# Handoff 43 - Meta Muse (muse): reply-meta

Date: 2026-11-23 11:59 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.25-R9361.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.25-R9361.1).
Preflight: ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: f3ff73a1-f838-4451-bdce-ce4edf37c4ec.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-f116136698ae40cab29ee8fc3da3a7d3.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-83f11af140c44475bd400e7b96087258` of revision 272920dff3ea5a59d905541f017d8bdd7b8c914b (--rev 272920dff3ea5a59d905541f017d8bdd7b8c914b; removed after the run).
Parent thread: (none - new thread). Result thread: `01a2080b-dd4d-7240-8876-cde7b1b3835b` (source: events).
Brief: `.collab/release-0.2.5/handoffs/40-claude-diff-review-handles.md` (sha256 1d73ac2bb0ac). Reviewed: 272920d, base 272920dff3ea5a59d905541f017d8bdd7b8c914b, tree sha256 974cd3dbaddc, 0 changed files. Live HEAD at launch: ac26f5a61672930bbb307273a29141acc14a7c44.
Bridge outcome: usable reply. Wall time: 118.7 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: muse: warning: rules file at \\?\C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-83f11af140c44475bd400e7b96087258\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules; muse: warning: project MCP servers from .mcp.json, if any, are not loaded because the workspace is untrusted; restart with --trust-workspace to load them.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - Handle containment is real but patch placement can strand/delete patches and directory setup still precedes the handles. Findings: 0 blocker, 3 major, 3 minor, 2 note (F43-1..F43-8, tracked in `findings.json`). Structured reply: `handoffs/43-muse-reply-meta.reply.json`.
Raw event stream: `handoffs/43-muse-reply-meta.events.jsonl`.
Verbatim reply follows.

---

## Q1. Remaining path names
Converted code still calls `std::fs` for classification, planning, and setup around the handle core: `contain.rs:285` (`classify_root` wording only), `worktree.rs:786-807` (`source_rel` fallback `canonicalize`, `linked_source_words` `canonicalize`), `worktree.rs:577-583` (`unlink_outside_links` `canonicalize` to classify inside/outside), `reviewed.rs:678-680` (`plan_diff` `canonicalize` for stem choice), `reviewed.rs:907,944-945` (temp-index `copy`/`remove_file`), `reviewed.rs:1028` (`create_dir_all` before the anchor open — the one that matters, see finding #1), `repo_models.rs:205,215,234` + `rules.rs:126,148,278,308` (discovery `read_dir`/`read` by path), plus `git` invocations with path args and `record_owner` in the git admin dir (admitted unconverted).

## Q2. `cap-std` per platform
Not verifiable read-only: no vendored sources, no shell. The proof test covers junctions everywhere and dir symlinks only where creatable (the Windows symlink half was skipped on the dev machine; CI is Linux+Windows, macOS behavior unexercised). The code's own comment admits `cap-std` itself may follow an inside-staying link (`contain.rs:1883-1889`); C3 avoids that call shape, but the claim rests on that discipline, not the crate. Treat junction/symlink/absolute-target refusal on macOS and non-surrogate reparse points as unproven.

## Q3. Legitimate use that changed
Inside-repo symlinks to the brief/`handoffs` are now skipped/counted (staged set shrinks) or refuse the run — previously followed. Any non-symlink Windows reparse point (cloud placeholders) now counts as a link. The held worktree handle blocking renames of the worktree and ancestors on Windows is the accepted side effect.

## Q4. Error paths
Half-removed worktree on first removal error; emptied-but-unremoved root on identity refusal; temp left with a message when its cleanup remove fails; stale empty claim after a crash (finding #2); reviewer running inside the tree makes listing/removal fail and the tree stays for `doctor --fix`.

## Q5. Patch placement
Two same-byte racers converge on one file (good). Different bytes under one name are refused (good). The holes are the stale-claim deadlock and the rename-failure `remove_file(leaf)` deleting a sibling's good file (findings #2–#3). No evidence of a wrong `sha256` in the ledger: `sha256` is computed from the in-memory bytes before placement.

## Q6. Test hooks
Thread hooks never come from the environment (only `set_thread_hook`), and env hooks need the mode gate — no inherited-env takeover read. Residual: hook shims ship in the release binary (`debug_assert` only), and with the mode variable set an inherited environment can still steer `PATCH_NO_HARDLINK`/`WRITE_FAIL`/`RACE` paths (availability/integrity of the run, not an escape).

## Q7. Claims
`CHANGELOG.md:9-30` and `contain.rs` header claim more than the code: 'never partial/temp left' ignores stale claims and reported-but-left temps; 'a root that is a link is refused' omits that `open_anchor` (repo/handoffs roots) follows by design; `publish` 'replacing only a regular file' is really 'replacing whatever entry survived the last check'.

## Q8. What is untested
Roster writer and git-admin owner record (admitted); macOS `cap-std` behavior; Windows dir-symlink half (skipped); crash-between-claim-and-rename and stale-claim recovery; read-only-directory removal; non-symlink reparse points as staging sources; permission-bit preservation; racers with differing bytes on the no-hardlink path.

## Requested checks
RC1 (read-only): inspect vendored `cap-std`/`cap-fs-ext` 4.0.3 sources for `open_nofollow`, `create_dir`, `rename`, `hard_link` semantics on Linux/macOS/Windows. RC2 (workspace-write): race two `place_without_link` on one name with one forced rename failure; observe survivor. RC3 (workspace-write): kill at `patch-claim`, rerun same-name placement; observe stale-claim error. RC4 (workspace-write): link `.claude/agents` outside and run repair scan; observe which bytes are read vs edited. RC5 (workspace-write): inject mid-removal unlink failure; list the half-removed remainder.

---

### Findings

- **F43-1** [major] `crates/c3/src/consult/reviewed.rs:1028` - place_patch creates directories by path before containment, so a link above the handoffs dir can redirect the create and the anchor open. Trigger: handoffs parent (or a component above it) is or becomes a symlink/junction when the patch is planned or placed. Evidence: read-code: place_patch calls std::fs::create_dir_all(dir_path) then Root::open_anchor(dir_path); the create follows links. Verify: Point the patch dir under a swapped link parent and show where the temp/claim file lands. Remedy: Open/create the parent through handles first (or refuse when any component above the leaf is a link) instead of create_dir_all by path; document the trust boundary for the handoffs parent.
- **F43-2** [major] `crates/c3/src/consult/reviewed.rs:995` - A crash between claim-create and rename (no-hardlink filesystem) leaves a permanent empty claim file that blocks every later patch with a misleading 'never overwritten' error; there is no reaper or expiry. Trigger: Process crash/kill between exclusive claim create and rename, then a later --diff run with the same patch name. Evidence: read-code: place_without_link creates empty leaf then renames tmp onto it; reuse_patch waits 2s on empty leaf then errors 'already holds other bytes'. No cleanup path for a dead owner. Verify: Kill a run at race point patch-claim, then rerun place_patch with the same name and read the error and directory listing. Remedy: Make claims self-identifying (pid/time in content, not empty) with stale-claim takeover, or always clean orphan claims older than N minutes.
- **F43-3** [major] `crates/c3/src/consult/reviewed.rs:1004` - place_without_link's rename-failure cleanup can delete a sibling's just-completed patch. Trigger: Two concurrent no-hardlink placements racing on the same patch name where one rename fails after the other completed. Evidence: read-code: On rename error it unconditionally removes leaf (the claimed name), even though leaf may now be another run's complete non-empty patch.; inferred: On Windows rename does not atomically replace, so a sibling completing between claim and rename makes the failing path remove a good file. Verify: Race two place_without_link calls on one name with injected rename failure and check the survivor file. Remedy: Only remove the claim when it is still empty/ours (check size/content through the handle first); never remove a non-empty leaf on failure.
- **F43-4** [note] `crates/c3/src/engines/worktree.rs:786`, `crates/c3/src/engines/worktree.rs:804`, `crates/c3/src/consult/reviewed.rs:678`, `crates/c3/src/engines/worktree.rs:577` - Several converted paths still resolve names after validation for classification or setup: canonicalize/symlink_metadata on live brief, handoffs plan, sweep classification, and repo-relative fallback. Trigger: A link swap between the classification canonicalize and the handle-based read/walk. Evidence: read-code: source_rel canonicalizes p and repo as fallback; linked_source_words canonicalizes for wording; plan_diff canonicalizes brief parent and handoffs dir.; read-code: unlink_outside_links canonicalizes each walked path to classify inside/outside; reviewed.rs plan_diff canonicalizes to decide stem. Verify: Swap a brief parent to a link between source_rel fallback and Root::read and confirm which bytes are staged. Remedy: State explicitly that these canonicalize calls only word messages or plan names and never select the bytes read/written/removed, or move them behind handles too.
- **F43-5** [minor] `crates/c3/src/setup/repo_models.rs:205`, `crates/c3/src/setup/rules.rs:126` - Repository repair discovery still enumerates and reads by path, so a linked .claude/agents supplies the snapshot bytes (edit later refused, but content is ingested). Trigger: .claude or .claude/agents replaced by a link to an outside directory before doctor/hook repair scans. Evidence: read-code: refresh_repo_models_at uses repo_root.join read_dir/read by path; refusal happens only in open_edit_dir/open_edit_file afterwards.; read-code: rules.rs scan/repair_state_sentences read by repo_root.join path. Verify: Link .claude/agents outside with a pinned-looking file and check whether its bytes enter the rewrite computation. Remedy: Enumerate and read the repair candidates through the repository handle, or document that discovery reads may cross a link while edits never do.
- **F43-6** [minor] `crates/c3/src/contain.rs:1646` - A failed worktree removal stops at the first error and leaves a half-emptied tree; remove_root's identity refusal likewise leaves an emptied directory in place with only prune run. Trigger: Read-only/locked file, denied unlink, or reviewer still inside the worktree during removal. Evidence: read-code: remove_contents returns on first error with remainder left; remove_worktree_dir propagates it and only doctor reports it later. Verify: Inject an unlink failure mid-tree and list what remains after the error. Remedy: Document the half-removed state as expected and ensure doctor --fix reliably finishes it; consider continuing the sweep to remove all it can before returning.
- **F43-7** [minor] `CHANGELOG.md:9`, `crates/c3/src/contain.rs:1339` - CHANGELOG overstates 'never' guarantees: stale claim files and failed-cleanup temp files do remain, and Root::open_anchor deliberately follows the repo root. Trigger: Reading the CHANGELOG as a security contract for patch and removal cleanup. Evidence: read-code: Temp/claim cleanup failures are appended to the message but the file stays; anchor open follows its path by design.; read-code: CHANGELOG claims 'never overwritten, never through a link, never partial, no temp left' without the crash/stale-claim and anchor exceptions. Verify: Compare each 'never' sentence against the crash/cleanup-failure code paths. Remedy: Qualify the entry: anchor roots are followed by design; temp/claim files can survive a crash or failed cleanup and are reported, not eliminated.
- **F43-8** [note] `crates/c3/src/consult/reviewed.rs:376`, `crates/c3/src/contain.rs:916` - Legitimate setups changed behavior: inside-repo symlinks to the brief/handoffs are now skipped/refused, and non-symlink reparse points (cloud placeholders) count as links; the held worktree handle blocks renames on Windows by design. Trigger: Repository with a checked-in or user-made inside symlink on the brief/handoffs path, or OneDrive-style placeholder files. Evidence: read-code: source_step counts Step::Link as skipped even when it stays inside; kind_of treats every Windows reparse point as Link.; read-code: contain.rs documents the held-handle rename block as the Windows protection. Verify: Stage a tree whose brief arrives via an inside symlink and record the new skip/refusal. Remedy: Call out the intentional break in release notes with a workaround (materialize the brief, avoid placeholder files in staged trees).

### Prior findings

_(none)_

## Verdict: HOLD

Handle containment is real but patch placement can strand/delete patches and directory setup still precedes the handles.

### Blockers

_(none)_

### Unproven scenarios

- cap-std/cap-fs-ext 4.0.3 no-follow, junction, absolute-target and root-link behavior on each of Linux, macOS, Windows (no sources or shell in this consultation).
- Whether Windows rename/replace semantics make the place_without_link cleanup delete a sibling's completed patch in practice.
- Whether a stale empty claim or half-removed worktree is reachable often enough to matter in production.

### First-run checklist (observable)

_(none)_
