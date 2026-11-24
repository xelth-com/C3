# Handoff 60 - Grok (grok): run-dirs-review-grok

Date: 2026-11-24 17:56 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from -Provider, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (usable reply 35 min ago).
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - entry 16 of 17 for -Provider grok (engine, model applied).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: d87044ed-efcf-447d-a96d-186028e31f7a.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-e70ceddcdb5b435e9eb40897b0c590fc.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-4cc300ebed6947d0a86286c12310bc77` of revision 8087bad24a97d6958cda5506aa51810b9811cb41 (--rev HEAD; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20e71-21ec-7d80-a72a-6a144c02cdc8` (source: events).
Brief: `.collab/release-0.2.5/handoffs/59-claude-diff-review-run-dirs.md` (sha256 aaee6bd45393). Reviewed: 8087bad, base 8087bad24a97d6958cda5506aa51810b9811cb41, tree sha256 2e2d8f0ea769, 0 changed files. Live HEAD at launch: 8087bad24a97d6958cda5506aa51810b9811cb41.
Bridge outcome: usable reply. Wall time: 753.4 s. Tokens: in 239641 (cached 874496), out 51671, reasoning 41386.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Warnings: the live collab directory changed during the run (1 file: .collab/release-0.2.5/handoffs/61-claudecode-run-dirs-review-glm54.events.jsonl) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - A task-directory link is refused, but consult, panel, and detach still follow a linked in-repo collab root because they canonicalize it before run_dirs. Findings: 0 blocker, 1 major, 2 minor, 0 note (F60-1..F60-3, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/60-grok-run-dirs-review-grok.reply.json`.
Raw event stream: `handoffs/60-grok-run-dirs-review-grok.events.jsonl`.
Verbatim reply follows.

---

## 1
A link at `<collab>/<task>` or at `handoffs/` / `work/` is not followed. Both branches open those names with `run_dir` → `open_child` (nofollow) at `state_dir.rs:1803` and `contain.rs:198`.

Case, `..`, a trailing separator, and a different drive-letter case stay on the inside branch. `lexical` (`state_dir.rs:1863`) drops `.` and resolves `..`; `repo_relative` (`paths.rs:27`) trims a trailing separator and, on Windows, folds case (`paths.rs:14`). That match returns before `canonicalize` (`state_dir.rs:1853`). An 8.3 name that is still a string under `repo_root` does the same, and the nofollow walk then refuses the link.

Two ways still take the outside branch (`state_dir.rs:1794`) and follow a link that is the collab root or an ancestor:

- Consult, panel, and detach call `resolve_collab_root` (`providers.rs:2562`, `orchestrate/mod.rs:1516`). `canonicalize` follows a symlink or junction `.collab`, or any ancestor, and passes the target in. When that target is outside the repository, `repo_relative_normal` is `None`, and `create_dir_all` plus `Dir::open_ambient_dir` create the task there. `c3 work` joins the path and does not canonicalize (`work/mod.rs:191`), so the same link is refused.
- Inside `run_dirs`, a spelling that is not a lexical prefix — a verbatim `\\?\` path, or an 8.3 name of an ancestor that is not a prefix of `repo_root` — falls through to `canonical_prefix` (`state_dir.rs:1856`, `1901`). `canonicalize` follows links. A symlink on that path whose target lies outside selects the same outside branch.

## 2
A repository reached through a link, a worktree, and an absolute path of a real directory inside the repository still run. `resolve_repo_root` canonicalizes, `open_anchor` follows that root (`contain.rs:393`), and the walk opens real children without following them. A task directory that is itself a link is refused; the changelog names that. `c3 work` also refuses a collab directory that is a link even when the target is still inside the repository. Consult follows that link (finding 1). A cloud-sync placeholder is a reparse point and is refused; the entry counts that as a link.

## 3
Nothing wrong found on the refusal path. An existing link is not created and its target is not written; the parent already exists, so no new directory is left behind. `shown` (`state_dir.rs:2121`) names the path the walk built. The `Dir` handles are locals and drop when `run_dirs` returns. `RunStart` keeps only the move lock.

## 4
The new assertions fail on the old code: `.collab/t` (`tests.rs:923`), `.collab/t/handoffs` (`932`), a linked `.collab2` (`939`), and task `x` under an outside anchor (`958`) all `unwrap_err`. The old code followed those and returned `Ok`. The `.c3` / `.eck` refusals, both normal starts, and “the outside anchor is followed” (`952`) already held before this commit. The test calls `begin_run` on the link path and never `resolve_collab_root`, so it stays green while consult follows a linked `.collab`. If the first `dir_link` fails, the test returns at `tests.rs:868` and the new assertions never run.

The tests clause at `CHANGELOG.md:764` still says a linked task directory under `.collab/` is followed. The paragraph above and the test say it is refused.

Not read: the c3-cli tests named in the changelog, `state_dir.rs` outside `run_dirs` / `repo_relative_normal` / `folder_link_problem` / `open_write_dir`, and the cap-std Windows open implementation past `open_child`.

---

### Findings

- **F60-1** [major] `crates/c3/src/providers.rs:2562`, `crates/c3/src/consult/orchestrate/mod.rs:1516`, `crates/c3/src/state_dir.rs:1794`, `crates/c3/src/work/mod.rs:191` - Consult, panel, and detach follow a symlink or junction used as an in-repository collab root (or as an ancestor of it) and create the task directory at the link target. resolve_collab_root canonicalizes before run_dirs, so repo_relative_normal sees the target; when that target is outside the repository the outside branch uses create_dir_all and Dir::open_ambient_dir. c3 work does not canonicalize, and the same link is refused there. Trigger: The default .collab directory, or a directory on the way to it, is a symlink or junction to a directory outside the repository, and the operator runs c3 consult, a panel, or a detached run. Evidence: read-code: resolve_collab_root_with ends with std::fs::canonicalize, which follows every symlink, then dunce_simplify.; read-code: build_context passes that canonical path to the Context used by begin_run and by a panel member's run_dirs.; read-code: When repo_relative_normal returns None, run_dirs create_dir_all's and open_ambient_dir's the collab path, then creates the task under that opened directory.; read-code: prepare_paths_with joins the collab path and does not canonicalize, so run_dirs still sees the in-repo link and run_dir refuses it.; read-code: The new .collab2 case calls begin_run on the link path itself and never resolve_collab_root. Verify: In a scratch git repo, make .collab a symlink to an outside directory, run c3 consult with a fake reviewer, and see whether the task directory appears at the link target. Repeat with c3 work and confirm that command refuses. Remedy: Decide inside versus outside on the path before following links, and open each component from the repository root with run_dir. Have resolve_collab_root leave an in-repo link for run_dirs to refuse.
- **F60-2** [minor] `crates/c3/src/state_dir.rs:1856`, `crates/c3/src/state_dir.rs:1901`, `crates/c3/src/state_dir.rs:1794` - A collab path that is inside the repository but is not a lexical prefix of repo_root is classified as outside when canonicalize follows a symlink on it. The verbatim \\?\ form, and an 8.3 short name of an ancestor that is not a string prefix of repo_root, skip the lexical match. canonical_prefix then follows the symlink, repo_relative_normal returns None, and the outside branch creates the task at the target. Trigger: run_dirs is given \\?\C:\repo\.collab, or C:\Users\SHORT~1\repo\.collab, and .collab or an ancestor on that spelling is a symlink or junction to a directory outside the repository. Evidence: read-code: The lexical repo_relative result is returned immediately when it matches. Only a miss calls canonical_prefix on both paths and compares those.; read-code: canonical_prefix uses std::fs::canonicalize, which follows symlinks and junctions, including the last component.; read-code: repo_relative is a string prefix. A \\?\ prefix or a different 8.3 ancestor does not match a normal repo_root even when the directory entry is inside the repo.; inferred: Drive-letter case, a trailing separator, and a lexical .. that stays under repo_root match before canonicalize, so those spellings do not select the outside branch. Verify: Call run_dirs with the repo root and a \\?\ spelling of an in-repo .collab that is a symlink to an outside directory, and check whether the task directory is created at the target. Remedy: Strip the verbatim prefix before the lexical test, and treat a symlink met while resolving an in-repo spelling as a refusal instead of as an outside anchor.
- **F60-3** [minor] `CHANGELOG.md:764` - The F57-6 tests clause says the unit test still follows a linked task directory under .collab. The same entry and the test expect that link to refuse the run. Trigger: A reader uses the tests clause to see what a_linked_task_directory_inside_the_folder_refuses_the_run_and_creates_nothing covers. Evidence: read-code: The clause ends with “a linked task directory under .collab/ still followed”.; read-code: begin_run on .collab/t, on .collab/t/handoffs, and on a linked .collab2 is unwrap_err, and the error must name the link. Verify: Read CHANGELOG.md:764 beside tests.rs:923 and confirm the clause says followed while the assertion requires an error. Remedy: Replace “still followed” with the refusal the test asserts.

### Prior findings

- F02-1 - not-checked - Not in this diff.
- F02-2 - not-checked - Not in this diff.
- F02-3 - not-checked - Not in this diff.
- F02-4 - not-checked - Not in this diff.
- F02-5 - not-checked - Not in this diff.
- F02-6 - not-checked - Not in this diff.
- F02-7 - not-checked - Not in this diff.
- F02-8 - not-checked - Not in this diff.
- F02-9 - not-checked - Not in this diff.
- F02-10 - not-checked - Not in this diff.
- F02-11 - not-checked - Not in this diff.
- F02-12 - not-checked - Not in this diff.
- F02-13 - not-checked - Not in this diff.
- F02-14 - not-checked - Not in this diff.
- F02-15 - not-checked - Not in this diff.
- F02-16 - not-checked - Not in this diff.
- F02-17 - not-checked - Not in this diff.
- F02-18 - not-checked - Not in this diff.
- F02-19 - not-checked - Not in this diff.
- F02-20 - not-checked - Not in this diff.
- F02-21 - not-checked - Not in this diff.
- F03-1 - not-checked - Not in this diff.
- F03-2 - not-checked - Not in this diff.
- F03-3 - not-checked - Not in this diff.
- F03-4 - not-checked - Not in this diff.
- F03-5 - not-checked - Not in this diff.
- F03-6 - not-checked - Not in this diff.
- F04-1 - not-checked - Not in this diff.
- F04-2 - not-checked - Not in this diff.
- F04-3 - not-checked - Not in this diff.
- F04-4 - not-checked - Not in this diff.
- F04-5 - not-checked - Not in this diff.
- F04-6 - not-checked - Not in this diff.
- F04-7 - not-checked - Not in this diff.
- F04-8 - not-checked - Not in this diff.
- F04-9 - not-checked - Not in this diff.
- F10-1 - not-checked - Not in this diff.
- F10-2 - not-checked - Not in this diff.
- F10-3 - not-checked - Not in this diff.
- F10-4 - not-checked - Not in this diff.
- F10-5 - not-checked - Not in this diff.
- F10-7 - not-checked - Not in this diff.
- F11-1 - not-checked - Not in this diff.
- F11-2 - not-checked - Not in this diff.
- F11-3 - not-checked - Not in this diff.
- F11-4 - not-checked - Not in this diff.
- F11-5 - not-checked - Not in this diff.
- F11-6 - not-checked - Not in this diff.
- F11-7 - not-checked - Not in this diff.
- F11-8 - not-checked - Not in this diff.
- F11-9 - not-checked - Not in this diff.
- F11-10 - not-checked - Not in this diff.
- F11-11 - not-checked - Not in this diff.
- F11-12 - not-checked - Not in this diff.
- F12-1 - not-checked - Not in this diff.
- F12-2 - not-checked - Not in this diff.
- F12-3 - not-checked - Not in this diff.
- F12-4 - not-checked - Not in this diff.
- F13-1 - not-checked - Not in this diff.
- F13-2 - not-checked - Not in this diff.
- F13-3 - not-checked - Not in this diff.
- F13-4 - not-checked - Not in this diff.
- F13-5 - not-checked - Not in this diff.
- F13-6 - not-checked - Not in this diff.
- F13-7 - not-checked - Not in this diff.
- F13-8 - not-checked - Not in this diff.
- F13-9 - not-checked - Not in this diff.
- F14-1 - not-checked - Not in this diff.
- F14-2 - not-checked - Not in this diff.
- F14-3 - not-checked - Not in this diff.
- F14-4 - not-checked - Not in this diff.
- F14-5 - not-checked - Not in this diff.
- F15-1 - not-checked - Not in this diff.
- F15-2 - not-checked - Not in this diff.
- F15-3 - not-checked - Not in this diff.
- F15-5 - not-checked - Not in this diff.
- F15-6 - not-checked - Not in this diff.
- F16-1 - not-checked - Not in this diff.
- F16-2 - not-checked - Not in this diff.
- F16-3 - not-checked - Not in this diff.
- F16-4 - not-checked - Not in this diff.
- F16-5 - not-checked - Not in this diff.
- F16-6 - not-checked - Not in this diff.
- F16-7 - not-checked - Not in this diff.
- F16-8 - not-checked - Not in this diff.
- F16-9 - not-checked - Not in this diff.
- F16-10 - not-checked - Not in this diff.
- F17-1 - not-checked - Not in this diff.
- F17-2 - not-checked - Not in this diff.
- F17-3 - not-checked - Not in this diff.
- F17-4 - not-checked - Not in this diff.
- F17-5 - not-checked - Not in this diff.
- F17-6 - not-checked - Not in this diff.
- F17-7 - not-checked - Not in this diff.
- F17-8 - not-checked - Not in this diff.
- F17-9 - not-checked - Not in this diff.
- F17-10 - not-checked - Not in this diff.
- F18-1 - not-checked - Not in this diff.
- F18-2 - not-checked - Not in this diff.
- F18-3 - not-checked - Not in this diff.
- F18-4 - not-checked - Not in this diff.
- F18-5 - not-checked - Not in this diff.
- F18-6 - not-checked - Not in this diff.
- F18-7 - not-checked - Not in this diff.
- F19-1 - not-checked - Not in this diff.
- F19-2 - not-checked - Not in this diff.
- F19-3 - not-checked - Not in this diff.
- F19-4 - not-checked - Not in this diff.
- F19-5 - not-checked - Not in this diff.
- F19-6 - not-checked - Not in this diff.
- F20-1 - not-checked - Not in this diff.
- F20-2 - not-checked - Not in this diff.
- F20-3 - not-checked - Not in this diff.
- F20-4 - not-checked - Not in this diff.
- F20-5 - not-checked - Not in this diff.
- F20-6 - not-checked - Not in this diff.
- F24-1 - not-checked - Not in this diff.
- F24-2 - not-checked - Not in this diff.
- F24-4 - not-checked - Not in this diff.
- F27-1 - not-checked - Not in this diff.
- F27-2 - not-checked - Not in this diff.
- F27-3 - not-checked - Not in this diff.
- F28-1 - not-checked - Not in this diff.
- F28-2 - not-checked - Not in this diff.
- F28-3 - not-checked - Not in this diff.
- F29-1 - not-checked - Not in this diff.
- F29-2 - not-checked - Not in this diff.
- F29-3 - not-checked - Not in this diff.
- F30-1 - not-checked - Not in this diff.
- F30-2 - not-checked - Not in this diff.
- F30-3 - not-checked - Not in this diff.
- F30-4 - not-checked - Not in this diff.
- F30-5 - not-checked - Not in this diff.
- F30-6 - not-checked - Not in this diff.
- F31-1 - not-checked - Not in this diff.
- F31-2 - not-checked - Not in this diff.
- F31-3 - not-checked - Not in this diff.
- F31-4 - not-checked - Not in this diff.
- F31-5 - not-checked - Not in this diff.
- F31-6 - not-checked - Not in this diff.
- F32-1 - not-checked - Not in this diff.
- F32-2 - not-checked - Not in this diff.
- F32-3 - not-checked - Not in this diff.
- F32-4 - not-checked - Not in this diff.
- F32-5 - not-checked - Not in this diff.
- F36-1 - not-checked - Not in this diff.
- F36-2 - not-checked - Not in this diff.
- F36-3 - not-checked - Not in this diff.
- F36-4 - not-checked - Not in this diff.
- F36-5 - not-checked - Not in this diff.
- F36-6 - not-checked - Not in this diff.
- F36-7 - not-checked - Not in this diff.
- F36-8 - not-checked - Not in this diff.
- F38-1 - not-checked - Not in this diff.
- F38-2 - not-checked - Not in this diff.
- F39-1 - not-checked - Not in this diff.
- F39-2 - not-checked - Not in this diff.
- F39-4 - not-checked - Not in this diff.
- F43-1 - not-checked - Not in this diff.
- F43-2 - not-checked - Not in this diff.
- F43-3 - not-checked - Not in this diff.
- F43-5 - not-checked - Not in this diff.
- F43-6 - not-checked - Not in this diff.
- F43-7 - not-checked - Not in this diff.
- F43-8 - not-checked - Not in this diff.
- F44-1 - not-checked - Not in this diff.
- F44-2 - not-checked - Not in this diff.
- F44-3 - not-checked - Not in this diff.
- F45-1 - not-checked - Not in this diff.
- F45-2 - not-checked - Not in this diff.
- F45-3 - not-checked - Not in this diff.
- F45-4 - not-checked - Not in this diff.
- F45-5 - not-checked - Not in this diff.
- F45-6 - not-checked - Not in this diff.
- F45-7 - not-checked - Not in this diff.
- F45-8 - not-checked - Not in this diff.
- F45-9 - not-checked - Not in this diff.
- F46-1 - not-checked - Not in this diff.
- F46-2 - not-checked - Not in this diff.
- F46-3 - not-checked - Not in this diff.
- F46-4 - not-checked - Not in this diff.
- F46-5 - not-checked - Not in this diff.
- F46-6 - not-checked - Not in this diff.
- F46-7 - not-checked - Not in this diff.
- F46-8 - not-checked - Not in this diff.
- F47-1 - not-checked - Not in this diff.
- F47-2 - not-checked - Not in this diff.
- F47-3 - not-checked - Not in this diff.
- F47-4 - not-checked - Not in this diff.
- F48-1 - not-checked - Not in this diff.
- F48-2 - not-checked - Not in this diff.
- F48-3 - not-checked - Not in this diff.
- F48-4 - not-checked - Not in this diff.
- F48-5 - not-checked - Not in this diff.
- F51-1 - not-checked - Not in this diff.
- F51-2 - not-checked - Not in this diff.
- F51-3 - not-checked - Not in this diff.
- F51-4 - not-checked - Not in this diff.
- F51-5 - not-checked - Not in this diff.
- F51-6 - not-checked - Not in this diff.
- F52-1 - not-checked - Not in this diff.
- F52-2 - not-checked - Not in this diff.
- F52-4 - not-checked - Not in this diff.
- F53-1 - not-checked - Not in this diff.
- F53-2 - not-checked - Not in this diff.
- F53-3 - not-checked - Not in this diff.
- F53-4 - not-checked - Not in this diff.
- F53-5 - not-checked - Not in this diff.
- F53-6 - not-checked - Not in this diff.
- F53-7 - not-checked - Not in this diff.
- F57-1 - not-checked - Not in this diff.
- F57-2 - not-checked - Not in this diff.
- F57-3 - not-checked - Not in this diff.
- F57-4 - not-checked - Not in this diff.
- F57-5 - not-checked - Not in this diff.
- F57-6 - fixed - A task component under a real collab directory is opened nofollow in run_dir (state_dir.rs:1803), including under .c3/collab. The listed trigger is refused. A linked collab root on the consult path is a separate hole (finding 1).
- F57-7 - not-checked - Not in this diff.
- F57-8 - not-checked - Not in this diff.
- F57-9 - not-checked - Not in this diff.
- F57-10 - not-checked - Not in this diff.

## Verdict: HOLD

A task-directory link is refused, but consult, panel, and detach still follow a linked in-repo collab root because they canonicalize it before run_dirs.

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
- **F57-1** (prior, not-checked) `crates/c3/src/maintain/web.rs:209` - Changing a page's URL and public flag while retaining its name can publish IDs from the previously non-public page in telemetry.
- **F57-2** (prior, not-checked) `crates/c3-core/src/quota.rs:941` - An uninterpretable nested tool inventory counts as positive switch evidence for a Codex smoke turn.
- **F57-3** (prior, not-checked) `crates/c3/src/maintain/mod.rs:740` - Pending recovery can record the pending target version as verified after smoking a different installed version.
- **F57-4** (prior, not-checked) `crates/c3/src/state_dir.rs:745` - A live lock creator can return success after another process reclaimed its still-empty file and acquired the same move lock.
- **F57-5** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:1451` - Two different patches can publish successfully at one short-hash path, leaving one consultation's full digest bound to the other consultation's bytes.
- **F57-6** (prior, fixed) `crates/c3/src/state_dir.rs:1544` - A linked task directory below a real project collab directory can redirect handoff directory creation and consultation output.

### Unproven scenarios

- No command was run. The consult follow and the \\?\ / 8.3 classification are from reading the path code.
- cap-std's Windows nofollow open was taken from open_child and kind_of, not from a junction run.
- The c3-cli project-folder tests named in the changelog were not read.

### First-run checklist (observable)

_(none)_
