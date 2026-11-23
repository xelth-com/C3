# Handoff 30 - Claude (claude): reply-zai-claude

Date: 2026-11-23 06:10 local. Author: Claude (claude) (model glm-5.3, effort high), claude-cli 2.1.330.
Reviewer: ZAI-claude :: glm-5.3 [claude] (provider from roster, model from roster; engine claude (C:\users\dmytro\.local\bin\claude.exe), endpoint https://api.z.ai/api/anthropic (token from env ZAI_API_KEY); provider fingerprint 944a94557723; harness claude-cli 2.1.330).
Preflight: ok: env ZAI_API_KEY set.
Effort: high sent (requested high, mapping claude-v1, by caps-v1: engine:claude, any model; not confirmed by the provider). Consultation id: 87ceb90e-5427-4b1f-8348-310750430e2f.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `claude -p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model glm-5.3 --effort high --json-schema "{"$schema":"http://json-schema.org/draft-07/schema#","title":"codex-consult reply v1","type":"object","additionalProperties":false,"required":["schema_version","verdict","verdict_reason","reply_markdown","findings","prior_findings","unproven","first_run_checklist"],"properties":{"schema_version":{"type":"string","enum":["1"]},"verdict":{"type":"string","enum":["ACCEPT","HOLD","REJECT","ADVISE"]},"verdict_reason":{"type":"string"},"reply_markdown":{"type":"string"},"findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["severity","locations","claim","trigger","evidence","verification","remedy","supersedes"],"properties":{"severity":{"type":"string","enum":["blocker","major","minor","note"]},"locations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["path","line"],"properties":{"path":{"type":"string"},"line":{"type":["integer","null"]}}}},"claim":{"type":"string"},"trigger":{"type":"string"},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","reference","observation"],"properties":{"kind":{"type":"string","enum":["read-code","ran-command","inferred","assumed"]},"reference":{"type":"string"},"observation":{"type":"string"}}}},"verification":{"type":"string"},"remedy":{"type":"string"},"supersedes":{"type":"array","items":{"type":"string"}}}}},"prior_findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","status","note"],"properties":{"id":{"type":"string"},"status":{"type":"string","enum":["fixed","still-open","not-checked","unknown-id"]},"note":{"type":"string"}}}},"unproven":{"type":"array","items":{"type":"string"}},"first_run_checklist":{"type":"array","items":{"type":"string"}}}}" --session-id bfe87bbb-5358-498f-93c1-257062e310df` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-3120ccb1aaa5462d85485b6aefc272bd` of revision fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f (--rev fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f; removed after the run).
Parent thread: (none - new thread). Result thread: `bfe87bbb-5358-498f-93c1-257062e310df` (source: events).
Brief: `.collab/release-0.2.5/handoffs/23-claude-verify-ci-panel-hook.md` (sha256 6174098bcbc8). Reviewed: fdf193a, base fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f, tree sha256 77688b041006, 0 changed files. Live HEAD at launch: fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f.
Bridge outcome: usable reply. Wall time: 461.5 s. Tokens: in 2383755 (cached 2257472), out 28437, reasoning 0.
Engine turns: 1 (claude -p, auth endpoint; model glm-5.3; init tools Glob, Grep, Read, StructuredOutput; permission denials 0).
Warnings: ACCEPT with 10 unchecked prior blocker(s) (F02-1, F02-2, F02-16, F02-19, F02-21, F03-1, F11-3, F11-7, F13-1, F19-1)..
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: ACCEPT - All 17 findings of this part are fixed in the code at HEAD (F16-3 completing only at fdf193a's tree kill), and the residual issues found are minor/note-level, none blocking. Findings: 0 blocker, 0 major, 2 minor, 4 note (F30-1..F30-6, tracked in `findings.json`). Structured reply: `handoffs/30-claudecode-reply-zai-claude.reply.json`.
Raw event stream: `handoffs/30-claudecode-reply-zai-claude.events.jsonl`.
Verbatim reply follows.

---

## Q1 — per-finding verdicts (code at HEAD fdf193a)

- **F16-2 / F17-1 / F17-10 fixed** — `crates/c3/src/ci_check.rs:52` (`RUN_FIELDS` asks no `displayTitle`), `:78-84` fixed-template line ("latest completed run"), `:100` `branch_text_ok`, `:110` `is_format_char`, `:128` `run_id` (digits-only or drop).
- **F16-3 fixed** — `ci_check.rs:297` / `:418` run the whole check inside `bounded::within` (10 s / 3 s); `crates/c3/src/bounded.rs:45` `stop()` now kills child + descendants via `crates/c3/src/engines/subprocess.rs:1262` `kill_tree_by_pid`. Caveat: complete only at fdf193a; 748ba3a alone killed no descendants (its Windows CI failed, as the brief admits).
- **F16-4 / F17-7 fixed** — `crates/c3/src/setup/branches.rs:319` `remote_default_branch` (origin/HEAD without a local twin), used by `ci_check.rs:254` `ci_branch`; time-parsed ordering `ci_check.rs:170-177`; limit 20 `:49`.
- **F17-8 fixed** — `ci_check.rs:217` `remotes_name_github` + `:229` `url_host` (host equality, ssh/scp/https parsed).
- **F16-9 fixed** — per-repo gate file `ci_check.rs:341`, `repo_key` `:322`, `gate_due` `:351`, `gate_record` `:374` (temp+rename, legacy shared file removed, 30-day prune).
- **F16-6 / F17-2 fixed** — `crates/c3/src/cli/router.rs:187-229` `seat_marks` uses `pctx.panel_members` (`:204`), `crates/c3/src/panel/run.rs:214` `member_inputs`, `crates/c3/src/panel/plan.rs:1263` `lab_seat_marks`, `crates/c3/src/panel/routing.rs:818` `lab_stands`; replay weighs `ext.c3.draw` (`crates/c3/src/router/replay.rs:240`, `:106`) and notes rounded replays (`:365`).
- **F16-7 / F17-4 fixed** — `plan.rs:26` table (+`minimax`/`meta-llama`/`llama`), `:49` aliases whole-id only, `:58` `prefix_at_boundary`, `:136` case-folded `entry_lab`, `:151` `member_lab`; host/prefix agreement unit-tested.
- **F16-8 / F17-3 fixed** — `plan.rs:1207` `tier_missed_warning`, wired `:1067`; one-lab rule kept, warning names purpose/lab/entries. One untested edge: finding #2.
- **F16-10 / F17-9 fixed** — `plugin/hooks/c3-hook.ps1:39-44` `Get-CutText` (used `:103`, `:114`, `:120`); catch prints `$WaitRule` (`:121-122`).
- **F17-6 fixed** — `crates/c3/src/setup/rules.rs:96-115` `with_branches_section` appends only (no existing byte changes); docs now state the title-line-alone rule (`rules.rs:66`); proven by `crates/c3-cli/tests/delegation_rule.rs:247`.

## Q2 — legitimate refusals
The branch filter drops only what git ref names cannot contain (control/space/format), so no valid ref loses the line; a `c3.defaultBranch` value with a space drops it silently (safe direction). Remote-host equality loses `https://www.github.com/...` remotes (matched by the old substring test) and ssh-alias remotes (`gh:owner/repo` via ~/.ssh/config — silent before too): finding #5. Remote-only defaults now work (tested). The boundary rule can split one maker into two labs (`seed` vs `seedream`, both ByteDance) so such siblings may share a panel; no seat a roster needs is kept out.

## Q3 — leftovers
Descendants are killed at HEAD (tree enumerated before `taskkill /T`, identity-checked loop `subprocess.rs:1273-1283`); a grandchild spawned after enumeration can still escape — inherent. Temp gate files are removed on rename failure but survive a crash mid-write and are never pruned (`ci_check.rs:399` name filter) — finding #6. Stale gate files are pruned only when some repository records. The worker thread is abandoned past the deadline (documented, `bounded.rs:146`); it ends once its children die.

## Q4 — unintended callers
`"ci"` in `c3 finish --json` changed from string to object (`crates/c3/src/cli/finish.rs:118`) — breaking for old consumers, documented. The branch line shares `bounded` unchanged and gains the tree kill. Panels: same-maker families split by boundary labs (above). `router explain` now runs the panel's election with preflight and network budget reads and no deadline (`cli/router.rs:195-205`) — finding #1.

## Q5 — Windows vs Unix
The scripts agree for ASCII lines (ps1 `Get-CutText` equals `Substring` for ASCII; awk bytes == chars). CHANGELOG sentences of this part now match the code, including the withdrawn byte-identical claim and the append-only delegation wording; "at most once a day per repository" still has the same-repo concurrent-start exception (finding #3).

## Q6 — untested
Same-repo concurrent gate race; `www.github.com` remotes; end-to-end `router explain` with real preflight (only `lab_seat_marks` is unit-tested; the .ps1 surrogate tests run on Windows only); the pins-fill-panel tier-miss edge (finding #2); malformed `ext.c3.draw` rows (finding #4); real `gh` `event` value variants.

## Requested checks
- **RC1** — In a scratch repo, configure `--require` entries of two labs, panel size 2, plus a `panel: weighty` entry of a third lab, on a weighty purpose; run `c3 consult --panel --dry-run` (workspace-write). Observation: no `upper tier missed` warning appears although no weighty entry is seated (finding #2). Budget: ~10 min.
- **RC2** — Start two `c3 hook` processes simultaneously in one fresh repo with a logging fake `gh` on PATH (read-only fixture). Observation: two gh invocations the same day for one repository (finding #3). Budget: ~10 min.
- **RC3** — Run `c3 router explain` on a roster with one unauthenticated claude-engine entry; list spawned processes and elapsed time. Observation: credential probes spawned by a display command, unbounded duration (finding #1). Budget: ~10 min.
- **RC4** — Scratch repo whose origin is `https://www.github.com/example/c3.git`, fake `gh` logging calls; run `c3 hook`. Observation: zero gh calls, no CI line (finding #5). Budget: ~5 min.

---

### Findings

- **F30-1** [minor] `crates/c3/src/cli/router.rs:195`, `crates/c3/src/cli/router.rs:204` - c3 router explain, previously a purely local computation, now builds the panel's member election with skip_preflight=false and reads budget multipliers in network mode, so a display command can spawn credential/auth probes for every active roster entry and has no deadline bounding it. Trigger: A roster with claude-engine or codex entries whose credentials need probing (no cached health) when the operator runs c3 router explain. Evidence: read-code: seat_marks calls Ctx::for_consult, pctx.panel_members("", "", purpose, false, false, 0, false) (skip_preflight=false) and member_inputs(..., !pctx.no_network).; read-code: panel_members with skip_preflight=false runs preflight per entry, which for claude/http/other engines calls engine_credential probes. Verify: Run c3 router explain on a roster with one unauthenticated claude entry while listing spawned processes and timing the command. Remedy: Pass skip_preflight=true (or cache-only budget mode) in explain's election, or document and deadline-bound the probes; the marks only need relative eligibility.
- **F30-2** [minor] `crates/c3/src/panel/plan.rs:1230` - tier_missed_warning drops the warning entirely when any eligible weighty candidate's lab holds no seat, including the case where required pins already fill all k seats so no tier seat could be drawn: an eligible weighty entry is seated, no required entry is weighty, and the plan stays silent, contrary to the stated invariant. Trigger: A weighty purpose, panel size k equal to the number of required pins (labs A and B), and a panel:weighty entry of a third lab C: invoke_lab_draw skips the tier seat because seats.len() >= k, and seated_at(lab C) is None so the `?` returns no warning. Evidence: read-code: `let by = seated_at(&c.lab)?;` inside the loop over tier candidates returns None from the whole function when the lab is unseated.; read-code: The tier seat is only drawn when seats.len() < k, so pins filling the panel skip it without a tier rule ever appearing (which would have suppressed the warning at plan.rs:1216 anyway). Verify: Unit-test select_panel_routing with two required pins filling size 2 and a weighty third-lab entry on a weighty purpose; assert a warning or its documented absence. Remedy: When a tier candidate's lab is unseated, still warn (naming no 'held by' lab, e.g. 'no room beside the required seats') instead of returning None.
- **F30-3** [note] `crates/c3/src/ci_check.rs:440` - Two SessionStart hooks starting concurrently in the SAME repository both pass gate_due before either writes, so both call gh and both print: 'at most once a day per repository' does not hold for simultaneous starts (the fix removed only the cross-repository read-modify-write). Trigger: Two sessions start in one repository within the window between gate_due's read and gate_record's rename. Evidence: read-code: gate_due reads the per-repo file, then gate_record writes it; no create-exclusive or lock guards the same-key window.; read-code: The concurrency test covers two different repositories only. Verify: Start two c3 hook processes together in one fresh repo with a logging fake gh and count gh invocations. Remedy: Accept and document the same-repo window, or create the gate file exclusively (create_new) and let the loser skip.
- **F30-4** [note] `crates/c3/src/router/replay.rs:240` - recorded_draw silently drops malformed rows (filter_map), so a partially valid ext.c3.draw makes the replay weigh precise values for some candidates and rounded values for others without saying so; only a fully absent block produces the rounded note. Trigger: A ledger record whose ext.c3.draw has a row missing position/score or with mult <= 0 or non-finite. Evidence: read-code: Rows are filter_map'd; per-candidate fallback to e.score * recorded_mult(e) is silent. Verify: Feed replay_one a record with one valid and one malformed draw row and inspect whether any note or failure is produced. Remedy: Treat a draw block with any invalid row as absent (note the rounding) or fail the replay with a message.
- **F30-5** [note] `crates/c3/src/ci_check.rs:229` - Host-equal recognition narrows coverage: a remote at https://www.github.com/... (previously matched by the substring test, and a real GitHub alias) is now silent, as are ssh-alias remotes (gh:owner/repo via ~/.ssh/config) - the alias case was silent before too. Trigger: A repository whose only remote URL host is www.github.com (or an ssh alias whose configured hostname is github.com) with a red default-branch run. Evidence: read-code: url_host returns the literal host and remotes_name_github compares == "github.com"; no www or alias handling.; inferred: The previous contains("github.com/") matched www.github.com URLs. Verify: Point a scratch repo's origin at https://www.github.com/example/c3.git with a fake gh and confirm no CI line and zero gh calls. Remedy: Also accept www.github.com (a one-line host check), or document the exact-host rule.
- **F30-6** [note] `crates/c3/src/ci_check.rs:382` - A gate temp file whose process dies between write and rename is never cleaned: its name (json.<pid>.<nanos>.tmp) fails is_gate_file_name, and the 30-day prune only touches gate-named files. Trigger: c3 hook is killed in the milliseconds between fs::write of the temp file and the rename. Evidence: read-code: Tmp is removed only when rename fails; the prune loop skips non-gate names. Verify: Kill a hook mid gate_record in a fixture and list the c3 directory afterwards. Remedy: Also remove stale ci-check-*.tmp files older than GATE_KEEP_DAYS during the prune.

### Prior findings

- F02-1 - not-checked - Part 1 scope; not verified in this consultation.
- F02-2 - not-checked - Part 1 scope.
- F02-3 - not-checked - Part 1 scope.
- F02-4 - not-checked - Part 1 scope.
- F02-5 - not-checked - Part 1 scope.
- F02-6 - not-checked - Part 1 scope.
- F02-7 - not-checked - Part 1 scope.
- F02-8 - not-checked - Part 1 scope.
- F02-9 - not-checked - Part 1 scope.
- F02-10 - not-checked - Part 1 scope.
- F02-11 - not-checked - Part 1 scope.
- F02-12 - not-checked - Part 1 scope.
- F02-13 - not-checked - Part 1 scope.
- F02-14 - not-checked - Part 1 scope.
- F02-15 - not-checked - Part 1 scope.
- F02-16 - not-checked - Part 1 scope.
- F02-17 - not-checked - Part 1 scope.
- F02-18 - not-checked - Part 1 scope.
- F02-19 - not-checked - Part 1 scope.
- F02-20 - not-checked - Part 1 scope.
- F02-21 - not-checked - Part 1 scope.
- F03-1 - not-checked - Part 1 scope.
- F03-2 - not-checked - Part 1 scope.
- F03-3 - not-checked - Part 1 scope.
- F03-4 - not-checked - Part 1 scope.
- F03-5 - not-checked - Part 1 scope.
- F03-6 - not-checked - Part 1 scope.
- F04-1 - not-checked - Part 1 scope; the moved runner keeps GIT_OPTIONAL_LOCKS=0 in ci_check::git_out.
- F04-2 - not-checked - Part 1 scope.
- F04-3 - not-checked - Part 1 scope.
- F04-4 - not-checked - Part 1 scope.
- F04-5 - not-checked - Part 1 scope.
- F04-6 - not-checked - Part 1 scope.
- F04-7 - not-checked - Part 1 scope; default-branch logic touched here only additively (remote_default_branch).
- F04-8 - not-checked - Part 1 scope.
- F04-9 - not-checked - Part 1 scope.
- F10-1 - not-checked - Other part.
- F10-2 - not-checked - Other part.
- F10-3 - not-checked - Other part.
- F10-4 - not-checked - Other part.
- F10-5 - not-checked - Other part.
- F10-7 - not-checked - Other part.
- F11-1 - not-checked - Other part.
- F11-2 - not-checked - Other part.
- F11-3 - not-checked - Other part.
- F11-4 - not-checked - Other part.
- F11-5 - not-checked - Other part.
- F11-6 - not-checked - Other part.
- F11-7 - not-checked - Other part.
- F11-8 - not-checked - Other part.
- F11-9 - not-checked - Other part.
- F11-10 - not-checked - Other part.
- F11-11 - not-checked - Other part.
- F11-12 - not-checked - Other part.
- F12-1 - not-checked - Other part.
- F12-2 - not-checked - Other part.
- F12-3 - not-checked - Other part.
- F12-4 - not-checked - Other part.
- F13-1 - not-checked - Other part.
- F13-2 - not-checked - Other part.
- F13-3 - not-checked - Other part.
- F13-4 - not-checked - Other part.
- F13-5 - fixed - Mechanism moved to crates/c3/src/bounded.rs and shared; branch line behavior preserved (hook/mod.rs:30 branch_line_within now calls bounded::within).
- F13-6 - not-checked - Other part.
- F13-7 - not-checked - Other part.
- F13-8 - not-checked - Other part.
- F14-1 - not-checked - Other part.
- F14-2 - not-checked - Other part.
- F14-3 - not-checked - Other part.
- F14-4 - not-checked - Other part.
- F14-5 - not-checked - Other part.
- F15-1 - not-checked - Other part.
- F15-2 - not-checked - Other part.
- F15-3 - not-checked - Other part.
- F15-5 - not-checked - Other part.
- F15-6 - not-checked - Other part.
- F16-2 - fixed - No title requested (RUN_FIELDS ci_check.rs:52); line is C3's template (ci_check.rs:78) with digits-only run_id (:128) and branch_text_ok (:100).
- F16-3 - fixed - finish_failure (ci_check.rs:297) and hook_line (:418) run under bounded::within; stop() kills child + descendants (bounded.rs:45-55, subprocess.rs:1262 kill_tree_by_pid) - complete only at fdf193a; 748ba3a alone did not kill descendants.
- F16-4 - fixed - remote_default_branch (branches.rs:319) used via ci_branch (ci_check.rs:254); parsed-time ordering (ci_check.rs:170-177); RUN_LIMIT 20 (ci_check.rs:49).
- F16-6 - fixed - seat_marks builds the panel election (cli/router.rs:187-229, panel/run.rs:214, plan.rs:1263, routing.rs:818); replay weighs ext.c3.draw (replay.rs:240, :106) and notes rounded replays (:365). Residual: malformed draw rows (finding #4).
- F16-7 - fixed - LAB_VENDORS extended (plan.rs:35-39), prefix_at_boundary (plan.rs:58), aliases whole-id (plan.rs:49), entry_lab case-folded (plan.rs:136), member_lab (plan.rs:151); lab_of_host agreement tested.
- F16-8 - fixed - tier_missed_warning (plan.rs:1207) wired at plan.rs:1067, tests cover the required-lab case. Residual edge: pins filling the panel (finding #2).
- F16-9 - fixed - One file per repository (ci_check.rs:341), temp+rename (gate_record :374), legacy shared file removed (:390). Residual: same-repo concurrent start (finding #3).
- F16-10 - fixed - Get-CutText never splits a surrogate pair (c3-hook.ps1:39-44, used :103/:114/:120); byte-identical claim withdrawn in CHANGELOG/README.
- F17-1 - fixed - Same as F16-2: no GitHub text reaches the line; branch_text_ok blocks control/format/whitespace (ci_check.rs:100-126).
- F17-2 - fixed - Explain now judges with panel_members + lab_seat_marks/lab_stands (cli/router.rs:204-208); unavailable entries marked 'not eligible', not lab picks (plan.rs:1263-1290).
- F17-3 - fixed - Same as F16-8; see finding #2 for an untested edge.
- F17-4 - fixed - minimax/meta-llama/llama prefixes in LAB_VENDORS (plan.rs:35-39); lab_of_host/prefix-table agreement unit-tested (plan.rs tests).
- F17-6 - fixed - Wording now matches status_of's title-line rule (rules.rs:66-73, docs :4-11); with_branches_section appends only (rules.rs:96-115); append-only proven by delegation_rule.rs test.
- F17-7 - fixed - ci_branch uses remote_default_branch (ci_check.rs:254, branches.rs:319-345); e2e test the_check_asks_about_the_remotes_default_branch_without_a_local_one.
- F17-8 - fixed - remotes_name_github compares the parsed URL host (ci_check.rs:217-252); gitlab.example/github.com/mirror no longer starts gh (tested).
- F17-9 - fixed - Get-CutText used on the catch path too and the catch prints $WaitRule (c3-hook.ps1:118-122); tested by the_ps1_catch_path_prints_the_waiting_rule.
- F17-10 - fixed - Line template says 'latest completed run' (ci_check.rs:78-84); in-progress run test updated accordingly.
- F18-1 - not-checked - Other part.
- F18-2 - not-checked - Other part.
- F18-3 - not-checked - Other part.
- F18-4 - not-checked - Other part.
- F18-5 - not-checked - Other part.
- F18-6 - not-checked - Other part.
- F18-7 - not-checked - Other part.
- F19-1 - not-checked - Other part.
- F19-2 - not-checked - Other part.
- F19-3 - not-checked - Other part.
- F19-4 - not-checked - Other part.
- F19-5 - not-checked - Other part.
- F19-6 - not-checked - Other part.
- F20-1 - not-checked - Other part.
- F20-2 - not-checked - Other part.
- F20-3 - not-checked - Other part.
- F20-4 - not-checked - Other part.
- F20-5 - not-checked - Other part.
- F20-6 - not-checked - Other part.
- F24-2 - not-checked - Proposed finding outside this part; not verified.
- F24-4 - not-checked - Proposed finding outside this part; not verified.
- F27-1 - not-checked - Proposed finding outside this part; not verified.
- F27-2 - not-checked - Proposed finding outside this part; not verified.
- F27-3 - not-checked - Proposed finding outside this part; not verified.
- F28-1 - not-checked - Proposed finding outside this part; not verified.
- F28-2 - not-checked - Proposed finding outside this part; not verified.
- F28-3 - not-checked - Proposed finding outside this part; not verified.
- F29-1 - not-checked - Proposed finding outside this part; not verified.
- F29-3 - not-checked - Proposed finding outside this part; not verified.

## Verdict: ACCEPT

All 17 findings of this part are fixed in the code at HEAD (F16-3 completing only at fdf193a's tree kill), and the residual issues found are minor/note-level, none blocking.

**WARNING: ACCEPT with 10 unchecked prior blocker(s) (F02-1, F02-2, F02-16, F02-19, F02-21, F03-1, F11-3, F11-7, F13-1, F19-1).**

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

- CI greenness of fdf193a (including the Windows the_checks_deadline_kills_gh_and_its_children test) is taken from the brief's evidence; I ran no tests.
- No end-to-end run of c3 router explain with a real roster/preflight was performed; only the lab_seat_marks/lab_stands unit tests were read.
- The .ps1 surrogate-pair and catch-path tests execute only on Windows; I read them but did not run them.
- Real gh reply shapes (event field variants, non-RFC3339 createdAt, string databaseId) were verified against unit fixtures only, not against live GitHub.
- Same-repo concurrent gate behaviour (finding #3) and the pins-fill-panel tier edge (finding #2) are reasoned from code, not executed.

### First-run checklist (observable)

_(none)_
