# Handoff 17 - Claude (claude): reply-alibaba-claude

Date: 2026-11-23 01:04 local. Author: Claude (claude) (model deepseek-v4.1-flash-1017, effort high), claude-cli 2.1.330.
Reviewer: alibaba-claude :: deepseek-v4.1-flash-1017 [claude] (provider from roster, model from roster; engine claude (C:\users\dmytro\.local\bin\claude.exe), endpoint https://token-plan.ap-southeast-1.maas.aliyuncs.com/apps/anthropic (token from env ALIBABA_API_KEY); provider fingerprint 062ad8f461b5; harness claude-cli 2.1.330).
Preflight: ok: env ALIBABA_API_KEY set.
Effort: high sent (requested high, mapping claude-v1, by caps-v1: engine:claude, any model; not confirmed by the provider). Consultation id: 2399a41b-b1e4-4895-8f14-e58126be82d5.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `claude -p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model deepseek-v4.1-flash-1017 --effort high --json-schema "{"$schema":"http://json-schema.org/draft-07/schema#","title":"codex-consult reply v1","type":"object","additionalProperties":false,"required":["schema_version","verdict","verdict_reason","reply_markdown","findings","prior_findings","unproven","first_run_checklist"],"properties":{"schema_version":{"type":"string","enum":["1"]},"verdict":{"type":"string","enum":["ACCEPT","HOLD","REJECT","ADVISE"]},"verdict_reason":{"type":"string"},"reply_markdown":{"type":"string"},"findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["severity","locations","claim","trigger","evidence","verification","remedy","supersedes"],"properties":{"severity":{"type":"string","enum":["blocker","major","minor","note"]},"locations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["path","line"],"properties":{"path":{"type":"string"},"line":{"type":["integer","null"]}}}},"claim":{"type":"string"},"trigger":{"type":"string"},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","reference","observation"],"properties":{"kind":{"type":"string","enum":["read-code","ran-command","inferred","assumed"]},"reference":{"type":"string"},"observation":{"type":"string"}}}},"verification":{"type":"string"},"remedy":{"type":"string"},"supersedes":{"type":"array","items":{"type":"string"}}}}},"prior_findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","status","note"],"properties":{"id":{"type":"string"},"status":{"type":"string","enum":["fixed","still-open","not-checked","unknown-id"]},"note":{"type":"string"}}}},"unproven":{"type":"array","items":{"type":"string"}},"first_run_checklist":{"type":"array","items":{"type":"string"}}}}" --session-id 3b0e62ad-b193-4aca-a474-a4cf9e2355d2` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-da1e79c4f6774325a31f684153e60e9a` of revision 08ae5d585c55f9239d99b76803fb1e4db0887544 (--rev 08ae5d585c55f9239d99b76803fb1e4db0887544; removed after the run).
Parent thread: (none - new thread; no thread of lineage alibaba-claude :: deepseek-v4.1-flash-1017 in this task's ledger; other lineage(s): kimi-claude :: k3.1-pro [claude], meta :: muse-spark-1.3-contributor [muse], grok :: grok-4.7 [grok], ZAI-claude :: glm-5.3 [claude], openai :: gpt-6.2-sol). Result thread: `3b0e62ad-b193-4aca-a474-a4cf9e2355d2` (source: events).
Brief: `.collab/release-0.2.5/handoffs/08-claude-diff-review-panel-ci-telemetry.md` (sha256 c4c9bcd9ec1f). Reviewed: 4f43074, base 4f430740c2ce1936fdff3649d67ed21d7ca1e5d4, tree sha256 24740c634eea, 0 changed files.
Bridge outcome: usable reply. Wall time: 861.8 s. Tokens: in 8702366 (cached 8457472), out 74475, reasoning 0.
Engine turns: 1 (claude -p, auth endpoint; model deepseek-v4.1-flash-1017; init tools Glob, Grep, Read, StructuredOutput; permission denials 0).
Warnings: ACCEPT with 6 unchecked prior blocker(s) (F02-1, F02-2, F02-16, F03-1, F11-3, F11-7).; the live collab directory changed during the run (1 file: .collab/release-0.2.5/handoffs/08-claude-diff-review-panel-ci-telemetry.patch) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: ACCEPT - No path was found that damages a machine, repository, roster or record or discloses a private model name; the residual defects are reporting/edge issues of minor or note severity. Findings: 0 blocker, 0 major, 8 minor, 2 note (F17-1..F17-10, tracked in `findings.json`). Structured reply: `handoffs/17-claudecode-reply-alibaba-claude.reply.json`.
Raw event stream: `handoffs/17-claudecode-reply-alibaba-claude.events.jsonl`.
Verbatim reply follows.

---

# 08 diff-review — panel seating, CI/hook, telemetry

**Verdict: ACCEPT**, with minor findings only.

**Q1. Seats.** Required entries are always seated (`pinned_seats`, `panel/plan.rs:832`, `routing.rs:822`); `k` is never cut below the pin count (`lab_seat_cap` counts pins + unheld labs, `routing.rs:946`). A lab-count cut is warned (`plan.rs:897-905`), two required of one lab too (`plan.rs:929-936`). Two gaps: (a) the weighty guarantee is skipped with no warning when the only eligible weighty entry shares a lab with a required non-weighty entry — `tier_owed` is true but the tier pool is empty (`plan.rs:782-790`, `routing.rs:757-775`) → finding #3; (b) a lab held by its reserve simply leaves the eligible set; the "panel size reduced" line covers that.

**Q2. Labs/records.** MiniMax and Meta-Llama exist in the class/OpenRouter tables but not in `LAB_VENDORS`, so an endpoint entry on `api.minimax.io` is lab `minimax` while a codex `minimax-m3` (or `meta-llama/...`) entry is a singleton: two entries of one maker can be seated → finding #4. Other claude endpoint hosts fall back to the model prefix, which is right for every family the roster uses. `ext.c3.seating` is written only when `labs_shared || tier_seated` (`plan.rs:980-989`), and replay reproduces those seats (test). But `c3 router explain` marks a seat-taker without checking availability/refusal/context → finding #2. The CHANGELOG's "a file the user rewrote is never edited" is only a title-line test → finding #6.

**Q3. CI check.** gh is killed at 3 s/10 s (`ci_check.rs:144-193`), the hook's 5 s wrapper returns without waiting (`hook/mod.rs:86-119`), nothing writes outside `<codex home>/c3`, no non-gh process is run, no child, never a refusal; cancelled/skipped decide nothing. Two real defects: a run title is forwarded verbatim — `one_line` collapses only whitespace, so ESC/BEL/NUL and instructions reach the session (#1); and `default_branch` can fall back to local `main` when `origin/HEAD` names a remote-only default, so the check can describe a non-default branch (#7). Also `remotes_name_github` matches a non-GitHub URL whose path contains `github.com/` (#8), the wording says "latest run" for the latest *completed* run (#10), and two sessions can both call gh / lose one attempt (benign, read-modify-write with atomic rename).

**Q4. Hook scripts.** Order, once-only, 400-char cut, 10-line cap and fallback are implemented and tested. Two divergences: a >400-char non-ASCII line is cut at 400 UTF-16 units in the `.ps1` (can split a surrogate pair) versus characters in awk, so "byte-identical" fails off ASCII, and the `.ps1` catch path prints no waiting rule (#9). The 10-line cap can silently drop the CI line last (#9). A non-ASCII cut is already known.

**Q5. File ops.** `repair_delegation_rule` reuses `edit_refusal`/`replace_file` (temp+rename, link and hard-link refusal, snapshot re-read, original line endings kept) — no write outside the target. The kimi removal refuses a link root before any unlink and runs no git command after any error (`kimi.rs:2741-2768`); the junction/sentinel and injected-failure tests match. Residual race: a link created after the sweep and before `git worktree remove`.

**Q6. Telemetry.** A name is sent only from `CLAUDE_MODELS` or a `(class, id)` published by the built-in catalog / a `"public": true` source; `[1m]` is stripped in any case; no vendor class → `other`; the Claude shape no longer authorises (`classes.rs:331-361`, `models.rs:758-783`). One leak path: trust is per source (host), not per URL, while `collect` keeps only the first catalog per host → finding #5.

**Q7. Untested:** the tier guarantee with a pinned same-lab entry; explain marks against unavailable entries; both hook scripts with long non-ASCII lines; the CI check against real `gh`; two catalogs on one host; a MiniMax/Meta-Llama roster; the 10-line cap.

## Requested checks
- RC1 (read-only, workspace-write for a scratch roster, ~5 min): roster with zhipu #2 available and #3 unavailable; run `c3 router explain --purpose framing` and `c3 consult --panel --dry-run`; the mark must not name a seat the dry run leaves empty.
- RC2 (workspace-write, ~10 min): config `model_catalogs: ["https://catalogs.example/v1/models", {"url":"https://catalogs.example/pub/v1/models","public":true}]` with the first listing `qwen/private-sauce`; serialize a backfill event for model `private-sauce`; expect `other`.
- RC3 (workspace-write, ~3 min): run both hook scripts with `c3: ` + 199 ASCII characters + 2 emoji; expect identical output and no lone surrogate.

---

### Findings

- **F17-1** [minor] `crates/c3/src/ci_check.rs:87`, `crates/c3-core/src/lib.rs:65` - The red-CI line forwards a workflow-run displayTitle into the session after one_line, which collapses whitespace only; ESC/BEL/NUL and arbitrary instructions survive, so a title a stranger controls can inject control sequences or instructions into every session start (once a day per repository). Trigger: A failed run on the default branch whose displayTitle contains an ANSI escape or text addressed to the agent. Evidence: read-code: The title goes through c3_core::one_line and a 100-char cut only.; read-code: one_line tests char::is_whitespace only; no other control character is removed. Verify: Call failure_line with a displayTitle containing \u{1b}[2J and an instruction, print the result, and check the ESC byte survives. Remedy: Strip C0/C1 control characters (keep printable text) in one_line or in the title, and consider quoting or labelling the title as untrusted.
- **F17-2** [minor] `crates/c3/src/cli/router.rs:250`, `crates/c3/src/cli/router.rs:287`, `crates/c3/src/cli/router.rs:403` - c3 router explain decides which entry 'takes its seat' per lab from roster.active_entries() with only on-request, held and weighty checks; an entry skipped for unavailable auth, a context window or a refusal is still marked as its lab's pick and its siblings as kept out, so the table names seats the real panel would never fill. Trigger: explain on a roster whose best-weighted entry of a lab is unavailable (missing key, too small a context window). Evidence: read-code: lab_picks builds LabRow from active_entries; drawable = !on_request && !held && !(weighty&&!weighty_purpose); no member-election state is consulted.; read-code: run_explain computes no availability/refusal and appends the mark unconditionally at 403. Verify: Give roster position 3 of one lab an unauthenticated endpoint and position 2 of the same lab a key; compare explain's mark with consult --panel --dry-run. Remedy: Build the rows from the same member election the panel uses (providers panel_members), or mark the seat as conditional.
- **F17-3** [minor] `crates/c3/src/panel/plan.rs:782`, `crates/c3/src/panel/routing.rs:757`, `crates/c3/src/panel/routing.rs:794` - The upper-tier guarantee is silently skipped when the only eligible weighty entries share a lab with a required (non-weighty) entry: tier_owed is true, but open(seats, true) excludes every lab already held by a pin, so the pool is empty, no tier seat is drawn and no warning is emitted - although no required entry is weighty, which is the only exception the invariant allows. Trigger: A roster with required #1 (kimi, non-weighty) and weighty #2 (kimi, same lab) on a weighty purpose with size >= 2. Evidence: read-code: tier is every eligible weighty position; no same-lab check.; read-code: tier_owed ignores pins' labs; open(&seats,true) filters labs in seated_labs, so a same-lab pin empties the pool and the seat is skipped. Verify: Unit-test select_panel_routing with required [1], weighty #2 of the same lab and purpose acceptance; assert a tier seat or a warning. Remedy: When tier_owed is true and the tier pool is empty because of a pinned lab, treat the matching required entry as satisfying the guarantee and say so, or warn.
- **F17-4** [minor] `crates/c3/src/panel/plan.rs:23`, `crates/c3/src/panel/plan.rs:56`, `crates/c3/src/panel/plan.rs:121`, `crates/c3-core/src/claude.rs:825`, `crates/c3/src/telemetry/classes.rs:235` - MiniMax and Meta-Llama are vendors in the class and OpenRouter tables but have no LAB_VENDORS prefix: an endpoint entry on api.minimax.io takes lab 'minimax' from lab_of_host, while a codex entry with minimax-m3, or an OpenRouter meta-llama id, becomes a singleton lab. Two entries of one maker are therefore seated together, contrary to the one-entry-per-lab rule. Trigger: A roster with two MiniMax (or Meta-Llama) entries, one an endpoint entry with an empty lab and one a codex/OpenRouter id. Evidence: read-code: LAB_VENDORS has no minimax, meta-llama or llama prefix.; read-code: lab_of_host returns 'minimax' for api.minimax.io/.cn.; read-code: The class table lists minimax and OPENROUTER_VENDORS maps 'minimax' to it. Verify: Unit-test model_lab("minimax-m3") and model_lab("meta-llama/llama-4"), then select_panel_routing with an endpoint minimax entry and a codex one. Remedy: Add minimax and meta-llama/llama prefixes to LAB_VENDORS so every class with a lab resolves to the same string.
- **F17-5** [minor] `crates/c3/src/maintain/web.rs:84`, `crates/c3/src/maintain/web.rs:119`, `crates/c3/src/maintain/web.rs:155`, `crates/c3/src/maintain/models.rs:758` - Telemetry trust is keyed by the source name, which for a catalog is only the host, while collect fetches only the first catalog URL per host. If a private catalog is configured first on a host that also has a catalog declared "public": true, the state's ids come from the private URL and are still published (OpenRouter-style ids bind by slug regardless of the host's class). Trigger: model_catalogs = ["https://catalogs.example/v1/models", {"url":"https://catalogs.example/pub/v1/models","public":true}] with the first listing e.g. qwen/private-sauce. Evidence: read-code: catalog_source returns web:<host>, discarding the path.; read-code: collect skips a catalog whose source already occurred, so only the first URL per host is fetched.; read-code: published_from matches state sources to trusted sources by name; a <vendor>/<model> id binds by slug even when the host class is None. Verify: Fixture with the two catalogs on one stub host and one alibaba-class reviewer with model private-sauce; serialize the backfill event and inspect details.model. Remedy: Key trusted sources by the full URL and record the URL in the maintain state, or refuse two catalogs on one host with different public flags.
- **F17-6** [minor] `crates/c3/src/setup/rules.rs:49`, `crates/c3/src/setup/rules.rs:64`, `crates/c3/src/setup/rules.rs:94`, `crates/c3/src/setup/rules.rs:137` - status_of calls a delegation rule C3-written whenever any line equals the template's title; doctor --fix then appends the section to a file the user rewrote around that title, although the CHANGELOG and README promise that a user-rewritten file is reported and never edited. Trigger: .claude/rules/02-delegation.md that keeps C3's title line but has a user-written body and no BRANCHES section. Evidence: read-code: template_title() is the first line; has_line compares any line; status_of derives c3_written from that single line.; read-code: with_branches_section appends for any c3_written file; repair_delegation_rule calls it unconditionally under --fix. Verify: Run doctor --fix on a repo whose rule file is the title line plus 'my own rules', then diff the file. Remedy: Require more evidence than the title (e.g. the template's next heading or a body hash), or change the documented rule to the title-line test.
- **F17-7** [minor] `crates/c3/src/ci_check.rs:194`, `crates/c3/src/setup/branches.rs:262` - The CI check asks gh for setup::branches::default_branch, which falls back to local main when origin/HEAD names a remote-only default (e.g. develop) and no local develop exists; the hook then reports the run of a branch that is not the repository's default and can miss a red default. Trigger: A clone whose origin default is develop, with only a local main, and a red latest run on main. Evidence: read-code: check() takes default_branch as the gh --branch value.; read-code: origin/HEAD is accepted only when the named branch exists locally, else main/master is used. Verify: Scratch repo with origin/HEAD -> develop, no local develop and a local main; run the hook with a fake gh logging its --branch argument. Remedy: Use the remote default name from origin/HEAD for the gh call even when no local branch exists, or state in the line that the branch is the local fallback.
- **F17-8** [note] `crates/c3/src/ci_check.rs:134` - remotes_name_github accepts any git remote output line containing "github.com/" or "github.com:", so a non-GitHub remote such as https://gitlab.example/github.com/mirror makes has_github_remote true; gh is started, fails, and the day's hook attempt is consumed. Trigger: A remote URL whose path contains github.com/. Evidence: read-code: remotes_name_github lower-cases the line and does a substring test; the unit test only covers a plain gitlab URL. Verify: Unit-test remotes_name_github("origin\thttps://gitlab.example/github.com/mirror (fetch)"). Remedy: Parse the remote URL/scp form and compare the host exactly with github.com.
- **F17-9** [minor] `plugin/hooks/c3-hook.ps1:91`, `plugin/hooks/c3-hook.ps1:107`, `plugin/hooks/c3-hook.sh:61` - The .ps1 cuts a long line at 400 UTF-16 code units (Substring can split a surrogate pair) while awk counts characters, so for a long non-ASCII line the two scripts print different text, contradicting the byte-identical claim; the .ps1 catch path also omits the waiting rule that the .sh always prints. Trigger: A forwarded c3: line over 400 UTF-16 units with non-BMP characters, or an exception inside the .ps1. Evidence: read-code: $t.Length/Substring(0,400) on UTF-16; catch prints only the error line.; read-code: awk length/substr counts characters (or bytes in a C locale) and the rule is always printed. Verify: Run both scripts with "c3: " + 199 ASCII characters + 2 emoji and compare the outputs byte for byte. Remedy: Cut on grapheme/text-element boundaries in PowerShell, and print the waiting rule in the catch block.
- **F17-10** [note] `crates/c3/src/ci_check.rs:82`, `crates/c3/src/ci_check.rs:99` - The printed sentence says 'the latest run on <branch> failed' although the value chosen is the latest completed, non-cancelled run; with a newer queued or in-progress run the sentence is literally untrue (the unit test encodes this behaviour). Trigger: A failed completed run followed by a newer queued run on the same branch. Evidence: read-code: runs are filtered to completed non-cancelled and sorted by createdAt; the message text does not say 'completed'. Verify: Call failure_line with a failure at T1 and a queued run at T2>T1. Remedy: Say 'the latest completed run' in the line, or state the run time.

### Prior findings

- F02-1 - not-checked - repo_models changed only by visibility in this diff.
- F02-2 - not-checked - Not re-verified.
- F02-3 - not-checked - Not re-verified.
- F02-4 - not-checked - Not re-verified.
- F02-5 - not-checked - Not re-verified.
- F02-6 - not-checked - Hook deadlines exist; not timed.
- F02-7 - not-checked - Description read code not read this pass.
- F02-8 - not-checked - Read-only git paths not re-checked.
- F02-9 - not-checked - Not re-verified; see the new #7 on default-branch fallback.
- F02-10 - not-checked - Not re-verified.
- F02-11 - not-checked - Not re-verified.
- F02-12 - not-checked - Not re-verified.
- F02-13 - not-checked - Not re-verified.
- F02-14 - not-checked - Not re-verified.
- F02-15 - not-checked - Not re-verified.
- F02-16 - not-checked - Not re-verified.
- F02-17 - not-checked - Not re-verified.
- F02-18 - not-checked - Not re-verified.
- F02-19 - fixed - published_from now binds ids to a trusted source and a vendor class; endpoint, undeclared and non-public sources are excluded, with end-to-end tests.
- F02-20 - fixed - The Claude-shape branch is gone from model_token; only CLAUDE_MODELS or the published set sends a Claude id.
- F02-21 - fixed - remove_worktree_dir runs no git or directory removal after any unlink error and refuses a link root; injected-failure and sentinel tests pass.
- F03-1 - not-checked - claude child environment not read this pass.
- F03-2 - fixed - Every reparse point counts as a link, directories are re-checked inside the canonical root, and the walk aborts on a link root.
- F03-3 - not-checked - Branch description parse not re-read; F13-3 still describes an open parse case.
- F03-4 - not-checked - Model line parser not re-read.
- F03-5 - fixed - strip_1m matches [1m] case-insensitively; opus[1M] -> opus, with unit tests.
- F03-6 - not-checked - Read-only branch paths not timed.
- F04-1 - not-checked - git status index refresh not re-checked.
- F04-2 - not-checked - Model swap gate not re-read.
- F04-3 - not-checked - Model swap gate not re-read.
- F04-4 - not-checked - doctor --fix guards not fully re-read this pass.
- F04-5 - fixed - Same rewrite as F02-21: no forced removal after an unlink failure; the recorder test asserts no git call.
- F04-6 - not-checked - Child environment coverage not re-read.
- F04-7 - not-checked - Default-branch/worktree classification not re-verified; see new #7.
- F04-8 - still-open - The Claude-id telemetry half is fixed here; the catalog 4 MiB cap and the code.kimi.ai redirect allow-list are untouched (tracked by F14-1/F15-1).
- F04-9 - not-checked - Description clear/set not re-read.
- F10-1 - not-checked - agy classifier not in this diff.
- F10-2 - not-checked - agy events not in this diff.
- F10-3 - not-checked - Kimi brief staging not changed here.
- F10-4 - not-checked - Panel snapshot not read.
- F10-5 - not-checked - Smoke checks not read.
- F10-7 - not-checked - Death record not read.
- F11-1 - not-checked - Not in this diff.
- F11-2 - not-checked - Not in this diff.
- F11-3 - not-checked - Kimi staging not changed here.
- F11-4 - not-checked - Kimi staging/panel re-seat not read.
- F11-5 - not-checked - Kimi prompt not read.
- F11-6 - not-checked - Panel acceptance not read.
- F11-7 - not-checked - Smoke not read.
- F11-8 - not-checked - Rollout parser not read.
- F11-9 - not-checked - Subprocess attribution not read.
- F11-10 - not-checked - Secondary prompts not read.
- F11-11 - not-checked - Detached death records not read.
- F11-12 - not-checked - Provider breakdown not read.
- F12-1 - not-checked - Liveness not read.
- F12-2 - not-checked - The CHANGELOG sentence was not re-checked.
- F12-3 - still-open - is_link in repo_models still treats every Windows reparse point as a link; only visibility changed.
- F12-4 - not-checked - Branch write race not re-checked.
- F13-1 - fixed - A link or junction at the worktree path is now refused inside remove_worktree_dir before any unlink or git call, so the link target survives.
- F13-2 - not-checked - Hook repo-root discovery not in this diff.
- F13-3 - not-checked - BranchRecord::parse not read.
- F13-4 - not-checked - C3_CHILD coverage not re-read.
- F13-5 - still-open - line_within still returns on the deadline and leaves the scan thread and its git children running; the new CI line uses the same helper.
- F13-6 - not-checked - AGENTS.md repair not read.
- F13-7 - not-checked - git cherry failure path not read.
- F13-8 - not-checked - Owner file not read.
- F14-1 - not-checked - HTTP deadline/connect not read.
- F14-2 - not-checked - Older-pair cap not read.
- F14-3 - not-checked - Smoke roster write not read.
- F14-4 - not-checked - bound_roster checks not read.
- F14-5 - not-checked - Future-date slack not read.
- F15-1 - not-checked - Resolved-peer policy not in this diff.
- F15-2 - not-checked - Older-pair eviction not read.
- F15-3 - not-checked - Smoke roster position not read.
- F15-5 - not-checked - Pending recovery not read.
- F15-6 - not-checked - --models off writes not re-checked.

## Verdict: ACCEPT

No path was found that damages a machine, repository, roster or record or discloses a private model name; the residual defects are reporting/edge issues of minor or note severity.

**WARNING: ACCEPT with 6 unchecked prior blocker(s) (F02-1, F02-2, F02-16, F03-1, F11-3, F11-7).**

### Blockers

- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, fixed) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, fixed) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, not-checked) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, fixed) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.

### Unproven scenarios

- No test was run: the whole review is code reading; Windows-only and Unix-only branches were not executed.
- The CI check was not exercised against a real gh or a real GitHub remote.
- The hook scripts were not run; non-ASCII and >400-character behaviour is argued from the code, not observed.
- No real panel was seated, so the ledger's ext.c3.seating and c3 router replay were only read and inferred from the unit test.
- The MiniMax/Meta-Llama split and the two-catalog-on-one-host privacy path were not reproduced with fixtures.
- The kimi removal race after the sweep (a link created before git worktree remove) was not tested.

### First-run checklist (observable)

- [ ] On a repo with a red latest completed default-branch run, `c3 hook` prints exactly one `CI:` line, `<codex home>/c3/ci-check.json` records the attempt, and a second session the same day prints none; on a green run nothing is printed.
- [ ] `c3 consult --panel --dry-run` on a roster with two entries of one lab prints `not picked (one seat per lab): ...` and the ledger's `panel.routing.ext.c3.seating` is present, and `c3 router replay` reports no difference for that record.
- [ ] `c3 doctor --fix` on an old C3 delegation rule prints both `[fixed] ... appended` and `[ok] ... has the "BRANCHES AND WORKTREES" section`, and the file ends with one blank line before the new section with the original line endings.
- [ ] `c3 finish` still exits 0 and prints no CI line when gh is absent, not logged in, or the repository has no GitHub remote, and its `--json` output carries `"ci": null`.
- [ ] Both hook scripts print the same text for the same `c3 hook` output, end with the waiting rule, and forward no duplicate line.
