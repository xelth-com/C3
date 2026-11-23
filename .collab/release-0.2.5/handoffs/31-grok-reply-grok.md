# Handoff 31 - Grok (grok): reply-grok

Date: 2026-11-23 06:14 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (usable reply 17 min ago).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 00875f2c-b718-4d35-98c6-2cb3cf7d2dee.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-21e3157075b64e81bbfe452e2774ebe4.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-867d35a12bef458c96f993293926bf91` of revision fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f (--rev fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f; removed after the run).
Parent thread: (none - new thread). Result thread: `01a206c6-70ad-7c02-a33c-3a4d8313a9e5` (source: events).
Brief: `.collab/release-0.2.5/handoffs/23-claude-verify-ci-panel-hook.md` (sha256 6174098bcbc8). Reviewed: fdf193a, base fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f, tree sha256 77688b041006, 0 changed files. Live HEAD at launch: fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f.
Bridge outcome: usable reply. Wall time: 781.9 s. Tokens: in 153968 (cached 2483200), out 50315, reasoning 38345.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - This commit still leaves a Windows gh grandchild past the CI deadline, and with origin/HEAD unset it can still ask about a local main. Findings: 0 blocker, 2 major, 4 minor, 0 note (F31-1..F31-6, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/31-grok-reply-grok.reply.json`.
Raw event stream: `handoffs/31-grok-reply-grok.events.jsonl`.
Verbatim reply follows.

---

## Q1

- **F16-2 fixed** — `crates/c3/src/ci_check.rs:79`, `:166`. The line is a fixed template, a checked branch, and a digits-only id. `displayTitle` is not requested. Pull-request events are skipped. `c3 finish --json` stores `{branch, run_id, line}` (`ci_check.rs:87`, `cli/finish.rs:118`).
- **F16-3 still-open** — `crates/c3/src/bounded.rs:49`. Descendants are killed only while the direct child is alive. This commit's Windows job failed `the_checks_deadline_kills_gh_and_its_children` (a `gh.cmd` grandchild survived). The later snapshot-then-taskkill pass is `engines/subprocess.rs:1269` and is not in this patch. `within` also returns without joining its thread (`bounded.rs:148`).
- **F16-4 still-open** — `crates/c3/src/setup/branches.rs:343`, `:354`. A remote-only `origin/HEAD` is used, `createdAt` is parsed (`ci_check.rs:173`), and `gh` is asked for 20 runs. If `origin/HEAD` is unset, the check still falls through to local `main` or `master`.
- **F16-6 fixed** — `crates/c3/src/cli/router.rs:204`, `crates/c3/src/router/replay.rs:240`. Explain uses `panel_members` and `lab_stands`. Replay weighs `ext.c3.draw` when present and names rounded replays.
- **F16-7 fixed** — `crates/c3/src/panel/plan.rs:35`, `:58`, `:139`. Prefixes match only at a boundary; `minimax`, `meta-llama`, and `llama` are labs; aliases match only as the whole id; a declared lab is case-folded.
- **F16-8 fixed** — `crates/c3/src/panel/plan.rs:1207`. A missed upper-tier seat warns with purpose, lab, and entries. One-lab is kept.
- **F16-9 still-open** — `crates/c3/src/ci_check.rs:335`, `:440`. Each repo has its own file, so one session no longer erases another's timestamp. Two sessions in one repo can both pass `gate_due` and both call `gh`.
- **F16-10 fixed** — `plugin/hooks/c3-hook.ps1:39`, `:122`. The cut stops before a high surrogate, `catch` prints the waiting rule, and the byte-identical claim is withdrawn (`CHANGELOG.md:171`).
- **F17-1 fixed** — same as F16-2.
- **F17-2 fixed** — `cli/router.rs:204`. An unavailable entry is `not eligible`, not the lab pick.
- **F17-3 fixed** — same as F16-8.
- **F17-4 fixed** — `plan.rs:35`, `crates/c3-core/src/claude.rs:836`. `api.minimax.io` and a `minimax-` id share `minimax`.
- **F17-6 still-open** — `crates/c3/src/setup/rules.rs:66`. A title-line file is still C3's, and `--fix` still appends. The false "never edited" sentence is withdrawn (`rules.rs:8`); existing bytes are not rewritten.
- **F17-7 fixed** — `branches.rs:319`. `origin/HEAD` counts with no local twin. A `c3.defaultBranch` that is not a local branch is an error (`branches.rs:329`) and silences the check.
- **F17-8 fixed** — `ci_check.rs:222`. The host must be `github.com`. A path containing `github.com/` does not count.
- **F17-9 fixed** — same as F16-10.
- **F17-10 fixed** — `ci_check.rs:79`, `:155`. The sentence says "latest completed"; a newer in-progress run does not hide a red completed run.

F16-5 stays out by decision. F16-1 and F17-5 are not this part.

## Q2

Yes. A git-valid ref drops the line if it starts with `-`, is longer than 255 characters, or contains a format character outside the partial Cf list (`ci_check.rs:100`). One bad `createdAt` silences the other 19 runs (`ci_check.rs:173`). `ssh.github.com` used to match the old substring test and now does not (`ci_check.rs:222`); an ssh alias or a GitHub Enterprise host does not either. `minimax-` and `llama` (including `llama.cpp`, because `.` is a boundary) now share a lab, so one seat is kept out (`plan.rs:58`).

## Q3

Yes. The worker thread is left running (`bounded.rs:148`). A direct child that has already exited is not tree-killed (`bounded.rs:49`). `gate_record` removes its temp file only when rename fails after a successful write (`ci_check.rs:389`). A crash or a failed write leaves `ci-check-*.json.<pid>.<nanos>.tmp`, and the 30-day sweep ignores those names (`ci_check.rs:398`).

## Q4

Yes. `ci` in `c3 finish --json` is an object, not a string (`cli/finish.rs:118`). The branch line and the CI check share `bounded::within`, which on Windows clears inherit on this process's standard handles (`bounded.rs:167`). Explain always builds a routed panel (`cli/router.rs:214`) and calls `panel_members` with preflight on and `no_network` false (`cli/router.rs:204`, `providers.rs:448`), so it can run `codex login status` and other credential probes. The new lab table changes who is seated.

## Q5

For an ASCII line with no embedded CR, both scripts cut at 400 characters and append `...`. The shell script deletes every CR (`c3-hook.sh:66`); PowerShell only trims the end, so an embedded CR differs. `CHANGELOG.md:171` says ASCII lines are identical, which is slightly stronger than the scripts. It no longer claims byte-identical text for every input. The upper-tier sentence matches `plan.rs:1251`.

## Q6

No test here covers unset `origin/HEAD` with a local `main`, `ssh.github.com`, two hooks in one repository, or explain's login probes. The Windows descendant test was added and, per the brief, failed on this commit. I did not read the cli integration tests, the `prepare` body past the new hunks, or the hook-script tests.

## Requested checks

RC1. On a Windows checkout of `748ba3a`, run `cargo test -p c3 --lib ci_check::tests::the_checks_deadline_kills_gh_and_its_children -- --nocapture`. Permission: workspace-write. Observation: the test fails because a grandchild pid is still alive. Budget: 3 minutes. Settles finding #1.

RC2. In a scratch repo with only local `main`, a GitHub `origin`, and no `refs/remotes/origin/HEAD`, run the CI check against a fake `gh` that logs `--branch`. Permission: workspace-write. Observation: the argument is `main`. Budget: 10 minutes. Settles finding #2.

RC3. Start two `hook_line` calls together in one repository, with a fake `gh` that counts invocations, before any gate file exists. Permission: workspace-write. Observation: two `gh` runs and one gate file. Budget: 5 minutes. Settles finding #3.


---

### Findings

- **F31-1** [major] `crates/c3/src/bounded.rs:49`, `crates/c3/src/bounded.rs:148`, `crates/c3/src/engines/subprocess.rs:1262` - The CI and branch deadlines do not reap every descendant this commit starts. stop() calls kill_tree_by_pid only while the direct child is still running, and within() returns without joining the worker. On this commit the Windows CI job failed the_checks_deadline_kills_gh_and_its_children because a grandchild of gh.cmd survived. The later enumeration in subprocess.rs is not part of the patch. Trigger: SessionStart or c3 finish runs the CI check, gh is a .cmd shim, and the deadline fires while a grandchild such as ping is alive; or the shim has already exited and left that grandchild. Evidence: read-code: stop() tree-kills only when try_wait returns Ok(None). within() documents that the worker thread is left behind.; read-code: The current kill enumerates descendants before taskkill and says the 0.2.5 CI deadline test saw gh.cmd's ping outlive taskkill /T. That comment and the second pass are absent from the reviewed patch.; assumed: The brief says the Windows job failed the_checks_deadline_kills_gh_and_its_children because a grandchild survived. I did not re-run it. Verify: RC1: on Windows at 748ba3a, run the deadline test and confirm a grandchild pid is still alive after the check returns. Remedy: Always tree-kill from the registered pid, including after the direct child has exited, and do not return from the deadline until the enumerated descendants are reaped or the kill is reported as failed. Supersedes: F16-3.
- **F31-2** [major] `crates/c3/src/setup/branches.rs:343`, `crates/c3/src/setup/branches.rs:354`, `crates/c3/src/ci_check.rs:254` - When origin/HEAD is unset, the CI check still asks gh about a local main or master, so it can name a branch that is not the repository's GitHub default. Trigger: The GitHub default is develop, origin/HEAD is unset, init.defaultBranch does not select develop, and a local main exists. c3 finish or the SessionStart hook runs the CI check. Evidence: read-code: remote_default_branch uses origin/HEAD even without a local twin, then falls through to init.defaultBranch, main, and master, each only if that local branch exists.; read-code: ci_branch asks gh about remote_default_branch and no other source. Verify: RC2: in a scratch repo with local main, a GitHub origin, and no origin/HEAD, log the fake gh --branch argument and confirm it is main. Remedy: If origin/HEAD is missing, stay silent or resolve the remote default from the remote itself. Do not substitute a local main. Supersedes: F16-4.
- **F31-3** [minor] `crates/c3/src/ci_check.rs:440`, `crates/c3/src/ci_check.rs:444` - Two SessionStart hooks in one repository can both pass the daily gate and both call gh. The per-repo file removes the cross-repo clobber, but gate_due and gate_record are not an exclusive create. Trigger: Two sessions start in the same repository before either gate rename is visible. Evidence: read-code: hook_failure returns only when gate_due is false, then calls gate_record, then gh. Nothing creates the gate file exclusively between the read and the rename. Verify: RC3: start two hook_line calls together against one repository and count gh invocations. Remedy: Create the gate file with create_new (or an equivalent lock) before calling gh, and treat an already-created file as not due. Supersedes: F16-9.
- **F31-4** [minor] `crates/c3/src/ci_check.rs:222`, `crates/c3/src/ci_check.rs:248` - A remote whose host is ssh.github.com is no longer treated as GitHub. The old substring test matched github.com: inside git@ssh.github.com:org/repo; url_host now requires the host to equal github.com, so the red-CI line stays silent for GitHub's documented port-443 SSH host. Trigger: The only GitHub remote is git@ssh.github.com:org/repo.git, or ssh://git@ssh.github.com/org/repo.git, and the default branch has a failed completed run. Evidence: read-code: remotes_name_github keeps a remote only when url_host equals github.com. For git@ssh.github.com:org/repo the host is ssh.github.com.; inferred: The previous predicate matched any remote line containing github.com: or github.com/, which includes ssh.github.com. Verify: Call remotes_name_github with origin\tgit@ssh.github.com:org/repo.git (fetch) and confirm it returns false. Remedy: Treat github.com and ssh.github.com as GitHub hosts. Keep rejecting other hosts that merely contain the substring.
- **F31-5** [minor] `crates/c3/src/ci_check.rs:384`, `crates/c3/src/ci_check.rs:398` - A failed or interrupted gate write leaves a temp file that the 30-day sweep never removes. The sweep only deletes names that are exactly ci-check-<16 hex>.json. Trigger: gate_record's write fails after creating the temp file, or the process dies after the write and before the rename. Evidence: read-code: The temp path is the gate path with an extra extension. remove_file runs only when write succeeded and rename failed. is_gate_file_name rejects the .tmp name. Verify: Force fs::write to fail after creating the temp path, run the hook, and list the codex c3 directory for a leftover .tmp file. Remedy: Remove the temp file on every error path, and delete stale ci-check-*.tmp files in the same sweep.
- **F31-6** [minor] `crates/c3/src/cli/router.rs:214`, `crates/c3/src/cli/router.rs:204` - c3 router explain always judges lab seats as a routed panel and runs full credential preflight. A roster-order panel can therefore be described as held by a different entry, and explain can spawn codex login status or other auth probes. Trigger: Explain a purpose whose real panel uses roster order, or a roster whose best-weighted entry needs codex login status and is not logged in. Evidence: read-code: seat_marks calls panel_members with skip_preflight false and sets PanelRoutingInput.order to routed. Ctx::for_consult sets no_network to false.; read-code: With skip_preflight false, panel_members calls preflight, which can run codex login status for an openai-auth provider. Verify: Run c3 router explain on a roster with an unauthenticated OpenAI entry and confirm a login-status process starts and the row is marked not eligible. Repeat the lab mark against a roster-order dry run. Remedy: Pass the same order the panel will use, and document or bound the credential probes. Skip preflight only if explain is meant to stay offline.

### Prior findings

- F02-1 - not-checked - Outside this patch; not read.
- F02-2 - not-checked - Outside this patch; not read.
- F02-3 - not-checked - Outside this patch; not read.
- F02-4 - not-checked - Outside this patch; not read.
- F02-5 - not-checked - Outside this patch; not read.
- F02-6 - not-checked - The branch runner moved to bounded.rs; the original git-stall claim was not re-tested.
- F02-7 - not-checked - Outside this patch; not read.
- F02-8 - not-checked - Outside this patch; not read.
- F02-9 - not-checked - Outside this patch; not read.
- F02-10 - not-checked - Outside this patch; not read.
- F02-11 - not-checked - Outside this patch; not read.
- F02-12 - not-checked - Outside this patch; not read.
- F02-13 - not-checked - Outside this patch; not read.
- F02-14 - not-checked - Outside this patch; not read.
- F02-15 - not-checked - Outside this patch; not read.
- F02-16 - not-checked - Outside this patch; not read.
- F02-17 - not-checked - Outside this patch; not read.
- F02-18 - not-checked - Outside this patch; not read.
- F02-19 - not-checked - Outside this patch; not read.
- F02-20 - not-checked - Outside this patch; not read.
- F02-21 - not-checked - Outside this patch; not read.
- F03-1 - not-checked - Outside this patch; not read.
- F03-2 - not-checked - Outside this patch; not read.
- F03-3 - not-checked - Outside this patch; not read.
- F03-4 - not-checked - Outside this patch; not read.
- F03-5 - not-checked - Outside this patch; not read.
- F03-6 - not-checked - Outside this patch; not read.
- F04-1 - not-checked - Outside this patch; not read.
- F04-2 - not-checked - Outside this patch; not read.
- F04-3 - not-checked - Outside this patch; not read.
- F04-4 - not-checked - Outside this patch; not read.
- F04-5 - not-checked - Outside this patch; not read.
- F04-6 - not-checked - Outside this patch; not read.
- F04-7 - not-checked - Outside this patch; not read.
- F04-8 - not-checked - Outside this patch; not read.
- F04-9 - not-checked - Outside this patch; not read.
- F10-1 - not-checked - Outside this patch; not read.
- F10-2 - not-checked - Outside this patch; not read.
- F10-3 - not-checked - Outside this patch; not read.
- F10-4 - not-checked - Outside this patch; not read.
- F10-5 - not-checked - Outside this patch; not read.
- F10-7 - not-checked - Outside this patch; not read.
- F11-1 - not-checked - Outside this patch; not read.
- F11-2 - not-checked - Outside this patch; not read.
- F11-3 - not-checked - Outside this patch; not read.
- F11-4 - not-checked - Outside this patch; not read.
- F11-5 - not-checked - Outside this patch; not read.
- F11-6 - not-checked - Outside this patch; not read.
- F11-7 - not-checked - Outside this patch; not read.
- F11-8 - not-checked - Outside this patch; not read.
- F11-9 - not-checked - Outside this patch; not read.
- F11-10 - not-checked - Outside this patch; not read.
- F11-11 - not-checked - Outside this patch; not read.
- F11-12 - not-checked - Outside this patch; not read.
- F12-1 - not-checked - Outside this patch; not read.
- F12-2 - not-checked - Outside this patch; not read.
- F12-3 - not-checked - Outside this patch; not read.
- F12-4 - not-checked - Outside this patch; not read.
- F13-1 - not-checked - Outside this patch; not read.
- F13-2 - not-checked - Outside this patch; not read.
- F13-3 - not-checked - Outside this patch; not read.
- F13-4 - not-checked - Outside this patch; not read.
- F13-5 - not-checked - The branch deadline now uses bounded::stop; the Windows grandchild hole in finding #1 applies, and the branch path was not re-run.
- F13-6 - not-checked - Outside this patch; not read.
- F13-7 - not-checked - Outside this patch; not read.
- F13-8 - not-checked - Outside this patch; not read.
- F14-1 - not-checked - Outside this patch; not read.
- F14-2 - not-checked - Outside this patch; not read.
- F14-3 - not-checked - Outside this patch; not read.
- F14-4 - not-checked - Outside this patch; not read.
- F14-5 - not-checked - Outside this patch; not read.
- F15-1 - not-checked - Outside this patch; not read.
- F15-2 - not-checked - Outside this patch; not read.
- F15-3 - not-checked - Outside this patch; not read.
- F15-5 - not-checked - Outside this patch; not read.
- F15-6 - not-checked - Outside this patch; not read.
- F16-1 - not-checked - Assigned to part 2; not read here.
- F16-2 - fixed - The CI line no longer carries displayTitle; the id is digits only.
- F16-3 - still-open - Windows grandchild can survive; see finding #1. The later kill_tree pass is outside this patch.
- F16-4 - still-open - Time order and the 20-run limit are fixed; unset origin/HEAD still selects local main. See finding #2.
- F16-6 - fixed - Explain uses the panel election; replay uses ext.c3.draw when present.
- F16-7 - fixed - Boundary prefixes, new labs, and case-folded declared labs.
- F16-8 - fixed - A missed upper-tier seat warns; one-lab is kept.
- F16-9 - still-open - Cross-repo clobber is gone; same-repo double gh remains. See finding #3.
- F16-10 - fixed - ps1 no longer splits a surrogate, catch prints the waiting rule, and the byte-identical claim is withdrawn.
- F17-1 - fixed - Same title removal as F16-2.
- F17-2 - fixed - Explain marks unavailable entries from panel_members, not as the lab pick.
- F17-3 - fixed - Same warning as F16-8.
- F17-4 - fixed - minimax and llama/meta-llama share a lab with the host table.
- F17-5 - not-checked - Assigned to part 2; not read here.
- F17-6 - still-open - Docs now admit the append, but --fix still appends to a title-line file with a user body.
- F17-7 - fixed - origin/HEAD is used even when that branch is not local.
- F17-8 - fixed - A github.com path on another host no longer starts gh. ssh.github.com is a separate regression (finding #4).
- F17-9 - fixed - Same ps1 cut and catch-path fix as F16-10.
- F17-10 - fixed - The sentence says latest completed, and an in-progress run does not hide a red one.
- F18-1 - not-checked - Outside this patch; not read.
- F18-2 - not-checked - Outside this patch; not read.
- F18-3 - not-checked - Outside this patch; not read.
- F18-4 - not-checked - Outside this patch; not read.
- F18-5 - not-checked - Outside this patch; not read.
- F18-6 - not-checked - Outside this patch; not read.
- F18-7 - not-checked - Outside this patch; not read.
- F19-1 - not-checked - Outside this patch; not read.
- F19-2 - not-checked - Outside this patch; not read.
- F19-3 - not-checked - Outside this patch; not read.
- F19-4 - not-checked - Outside this patch; not read.
- F19-5 - not-checked - Outside this patch; not read.
- F19-6 - not-checked - Outside this patch; not read.
- F20-1 - not-checked - Outside this patch; not read.
- F20-2 - not-checked - Outside this patch; not read.
- F20-3 - not-checked - Outside this patch; not read.
- F20-4 - not-checked - Outside this patch; not read.
- F20-5 - not-checked - Outside this patch; not read.
- F20-6 - not-checked - Outside this patch; not read.
- F24-2 - not-checked - Outside this patch; not read.
- F24-4 - not-checked - Outside this patch; not read.
- F27-1 - not-checked - Outside this patch; not read.
- F27-2 - not-checked - Outside this patch; not read.
- F27-3 - not-checked - Outside this patch; not read.
- F28-1 - not-checked - Outside this patch; not read.
- F28-2 - not-checked - Outside this patch; not read.
- F28-3 - not-checked - Outside this patch; not read.
- F29-1 - not-checked - Outside this patch; not read.
- F29-3 - not-checked - Outside this patch; not read.

## Verdict: HOLD

This commit still leaves a Windows gh grandchild past the CI deadline, and with origin/HEAD unset it can still ask about a local main.

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

### Unproven scenarios

- Whether fdf193a's kill_tree makes the Windows deadline test pass; the new enumeration was read, not run.
- Whether two same-repo SessionStart hooks both invoke gh.
- Whether c3 router explain actually blocks on codex login status.
- Byte output of both hook scripts for a long ASCII line and for a line with an embedded CR.
- The prepare() body beyond the new warning and draw-record hunks, the cli integration tests, and the hook-script tests were not read.

### First-run checklist (observable)

_(none)_
