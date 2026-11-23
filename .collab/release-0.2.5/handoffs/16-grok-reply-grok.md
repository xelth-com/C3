# Handoff 16 - Grok (grok): reply-grok

Date: 2026-11-23 01:04 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (usable reply 33 min ago).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 5450ad14-d73f-40c6-a102-2b780b6e31f6.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-d29328afc9504dba965f833845ae0118.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-3af805e8d89846c082fc9935159cb262` of revision 08ae5d585c55f9239d99b76803fb1e4db0887544 (--rev 08ae5d585c55f9239d99b76803fb1e4db0887544; removed after the run).
Parent thread: (none - new thread). Result thread: `01a205a6-a170-7f12-84ab-154070888a97` (source: events).
Brief: `.collab/release-0.2.5/handoffs/08-claude-diff-review-panel-ci-telemetry.md` (sha256 c4c9bcd9ec1f). Reviewed: 4f43074, base 4f430740c2ce1936fdff3649d67ed21d7ca1e5d4, tree sha256 24740c634eea, 0 changed files.
Bridge outcome: usable reply. Wall time: 949.4 s. Tokens: in 333612 (cached 2954368), out 62143, reasoning 50203.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Warnings: the live collab directory changed during the run (1 file: .collab/release-0.2.5/handoffs/08-claude-diff-review-panel-ci-telemetry.patch) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The one-lab draw and the Claude-id telemetry gate are in place, but a same-host catalog can still publish a private model name, the CI check can hang or inject a run title, and kimi removal can still act on a path replaced by a link. Findings: 0 blocker, 4 major, 5 minor, 1 note (F16-1..F16-10, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/16-grok-reply-grok.reply.json`.
Raw event stream: `handoffs/16-grok-reply-grok.events.jsonl`.
Verbatim reply follows.

---

**Q1.** A required reviewer is not dropped. Pins are seated first, including two of one lab, with a warning (`crates/c3/src/panel/routing.rs:743`, `crates/c3/src/panel/plan.rs:929`). A reserve-held required entry aborts the run (`crates/c3/src/panel/run.rs:336`). Size above the lab count is cut and warned (`plan.rs:776`, `plan.rs:897`). The bare default and `C3_PLUGIN_COMPAT=1` skip the rule (`plan.rs:713`). If every eligible `panel: weighty` entry shares a lab with a required entry that is not weighty, the tier pool is empty (`routing.rs:800`) and a panel of two or more can run with no weighty seat. That entry is reported as `lab … already seated by #…` (`plan.rs:883`). Nothing says the upper tier was missed.

**Q2.** A lab is the declared `lab` string, else a claude endpoint host (`plan.rs:121`, `crates/c3-core/src/claude.rs:825`), else a model prefix (`plan.rs:64`). Prefixes have no boundary, so `opus`, `sonnet`, `haiku`, `fable`, `seed`, `k3` and `o` plus a digit capture unrelated ids, and an OpenRouter last segment is tried the same way. Declared `OpenAI` does not merge with prefix `openai` (`plan.rs:108`). A minimax endpoint is lab `minimax` (`claude.rs:836`); a model id `minimax-…` is its own lab. `CHANGELOG.md:148` says `kimi` and `kimi-claude` are one lab; that holds only when the model, host or declared lab agrees. `c3 router explain` says the highest-weight drawable entry takes the lab's seat (`crates/c3/src/cli/router.rs:233`) even when the panel seated a required or weighty sibling, or seated nobody from that lab. Replay multiplies scores rounded to 4 decimals (`plan.rs:1007`, `crates/c3/src/router/replay.rs:92`), so a close draw can replay to other seats. `ext.c3.seating` is stored only when eligible labs collide or a tier rule fired (`plan.rs:986`).

**Q3.** The check can hang, and a run title can enter the session. `c3 finish` runs git with no deadline before the 10s `gh` wait (`crates/c3/src/ci_check.rs:111`, `ci_check.rs:224`). The hook allows 5s and then returns without killing that thread's git or `gh` (`crates/c3/src/hook/mod.rs:93`, `hook/mod.rs:106`). `run_gh` kills its child only while that function is still running (`ci_check.rs:180`). The hook skips a C3 child (`ci_check.rs:321`); finish does not. `gh` is an argv spawn (`ci_check.rs:145`). `ci-check.json` is written with no lock before the call (`ci_check.rs:343`), so two sessions can both call `gh` and the later write can drop the other repository. The line is a live result. It can still name the wrong branch or run: `default_branch` falls back to local `main` (`crates/c3/src/setup/branches.rs:291`), `createdAt` is ordered as text (`ci_check.rs:77`), and `--limit 5` can hide the latest completed run (`ci_check.rs:205`). The title is only whitespace-collapsed (`crates/c3-core/src/lib.rs:65`, `ci_check.rs:87`). ESC, other controls and instruction text from `displayTitle` are printed, and the hook scripts forward that line.

**Q4.** `c3 hook` emits at most eight prefixed lines (`hook/mod.rs:41`), so the cap of 10 does not drop CI today. Both scripts keep order, drop exact duplicates, cut at 400, print the waiting rule, and exit 0 (`plugin/hooks/c3-hook.sh:61`, `plugin/hooks/c3-hook.ps1:85`). They diverge on non-ASCII: shell `awk` length is bytes on mawk, PowerShell counts UTF-16 units, and a cut can split a character. The shell script also strips every CR. A PowerShell exception skips the waiting rule (`c3-hook.ps1:107`). Session start is not failed.

**Q5.** The delegation append keeps existing bytes and uses CRLF for the added section when the file already contains CRLF (`crates/c3/src/setup/rules.rs:100`). A link, a second hard link and a concurrent save are refused (`rules.rs:161`, `crates/c3/src/setup/repo_models.rs:271`). The temp file is created before that second check (`repo_models.rs:325`), so a parent replaced by a link in that window still receives a file (F02-1). A crash can leave `.02-delegation.md.c3-repair-*.tmp`. The original is not truncated. Kimi treats every Windows reparse point as a link, does not descend, and skips `worktree remove`, prune and `remove_dir_all` after any unlink error (`crates/c3/src/engines/kimi.rs:263`, `kimi.rs:305`). The check is by path. `canonicalize` follows a junction that appears after `symlink_metadata` (`kimi.rs:319`) and adopts it as the walk root. `git worktree remove --force` and `remove_dir_all` then run on that path with no directory handle held (`kimi.rs:268`).

**Q6.** A Claude-shaped id is sent only from `CLAUDE_MODELS` or the published set (`crates/c3/src/telemetry/classes.rs:351`). `[1m]` is stripped in any case (`classes.rs:273`). Endpoint and engine lists are excluded (`crates/c3/src/maintain/models.rs:760`). Catalog trust is by host: the source key is `web:<host>` (`crates/c3/src/maintain/web.rs:84`), only the first URL for that host is fetched (`web.rs:161`), and the host is trusted if any URL on it is `"public": true` or it is the built-in catalog (`web.rs:130`). Ids from the other URL, including `<slug>/<model>` for an OpenRouter slug, are sent for that class (`models.rs:766`). The set is computed once per process (`models.rs:784`).

**Q7.** Tests do not call real `gh`, a real plugin session, a real panel or the telemetry server. Uncovered: two catalog URLs on one host, a title containing ESC, stalled git during `c3 finish`, a junction swapped onto the worktree path, and explain versus a required or weighty seat. I did not read the new test bodies, the README past the panel section, the coordinate skill, or code outside this delta.

## Requested checks
RC1. Dir: repo root. Permission: workspace-write. Seed maintain state from `https://models.example/private` listed before `https://models.example/public` with `"public": true` and id `openai/private-ft-1`. Call `published_from` and `model_token_with` for class openai. Settles finding #1 if the token is `private-ft-1`. Budget: 5 minutes.
RC2. Dir: scratch repo whose real default is `develop`, with local `main` and no `origin/HEAD`. Permission: workspace-write. A fake `gh` prints one completed failure for `main` and sleeps 30s. Run `c3 hook`, then a finish path whose git is delayed. Settles findings #2 and #3 if the hook exits while `gh` is still alive, finish waits past 10s, or the line names `main`. Budget: 2 minutes.
RC3. Dir: disposable Windows repo. Permission: workspace-write. After the unlink pass, replace the kimi worktree path with a junction to a sentinel directory and let removal continue. Settles finding #4 if a sentinel file disappears. Budget: 15 minutes.
RC4. Dir: repo root. Permission: read-only. Replay a one-per-lab record whose two weights differ only past the 4th decimal. Settles finding #5 if the replayed seat differs from the record. Budget: 5 minutes.

---

### Findings

- **F16-1** [major] `crates/c3/src/maintain/web.rs:84`, `crates/c3/src/maintain/web.rs:161`, `crates/c3/src/maintain/web.rs:136`, `crates/c3/src/maintain/models.rs:766` - Telemetry can send a private model name when two catalog URLs share a host and only one is marked public. Catalog state is keyed by web:<host>, collect keeps the first URL for that host, and trusted_sources trusts the host if any URL on it has public:true (the built-in openrouter.ai host is always trusted). A non-public URL fetched first therefore supplies the ids that published_from sends, including vendor/model ids bound through an OpenRouter slug. Trigger: model_catalogs lists https://models.example/private (public false) before https://models.example/public (public true), and the private body contains openai/private-ft-1. A later openai-class event uses that model. Evidence: read-code: catalog_source returns web:<host>. collect skips a second URL with the same source. trusted_sources adds a host when any catalog entry has public true. published_from binds a slash id by OPENROUTER_VENDORS slug even when the host has no class. Verify: Run RC1 and inspect the model token. Remedy: Key published trust by the exact fetched URL, and publish a source only when that URL's own config entry is public:true. Do not let a built-in host trust a replacement URL on the same host. Supersedes: F02-19.
- **F16-2** [major] `crates/c3/src/ci_check.rs:87`, `crates/c3-core/src/lib.rs:65`, `plugin/hooks/c3-hook.sh:61`, `plugin/hooks/c3-hook.ps1:87` - A GitHub run title is copied into the SessionStart context and into c3 finish output after only whitespace collapsing. Control characters other than whitespace, bidi marks and instruction text in displayTitle survive and the hook scripts forward the CI line. Trigger: The latest completed non-cancelled run on the queried branch has conclusion failure, timed_out or startup_failure, and its displayTitle contains ESC or a sentence aimed at the coordinator. Evidence: read-code: one_line keeps every non-whitespace character. failure_line puts that title into the CI line, capped at 100 characters. The hook scripts forward lines that start with CI: . Verify: Feed failure_line a JSON title containing U+001B and the text ignore previous instructions, and print the hook-script forwarding of that line. Remedy: Strip C0/C1 controls and bidi overrides from the title, and render it as quoted data with a fixed prefix so it cannot look like a c3 instruction.
- **F16-3** [major] `crates/c3/src/ci_check.rs:224`, `crates/c3/src/ci_check.rs:111`, `crates/c3/src/hook/mod.rs:106`, `crates/c3/src/hook/mod.rs:93` - The CI check's deadline does not cover the work it claims to bound. c3 finish waits on git with no timeout before gh's 10s limit. The hook's 5s wrapper abandons the worker thread without killing its git or gh children, so a gh that has already started is not killed when the hook returns. Trigger: git remote or default-branch resolution stalls during c3 finish, or the hook's pre-gh git work consumes the 5s budget after gh has been spawned. Evidence: read-code: finish_line calls check synchronously. git_out has no deadline. line_within uses recv_timeout and does not kill the spawned thread or its children. run_gh kills gh only on its own loop. Verify: Run RC2 with a git shim that sleeps in rev-parse and a gh shim that sleeps. Remedy: Put one deadline around every git and gh process in both paths, and on expiry kill the child process tree before returning.
- **F16-4** [minor] `crates/c3/src/setup/branches.rs:291`, `crates/c3/src/ci_check.rs:77`, `crates/c3/src/ci_check.rs:205` - The CI line can name a branch or run that is not the latest completed run of the repository's real default branch. default_branch falls back to a local main when origin/HEAD and init.defaultBranch do not resolve. failure_line orders createdAt as text, and gh is asked for only five runs, so a newer completed failure can be omitted or an older offset timestamp can sort first. Trigger: The GitHub default is develop, origin/HEAD is unset, and local main exists; or the five newest runs are in progress or cancelled while an older completed run failed; or createdAt values use mixed numeric offsets. Evidence: read-code: After c3.defaultBranch, origin/HEAD and init.defaultBranch, the function returns the first existing main or master. failure_line sorts createdAt with string cmp and then reads --limit 5. Verify: In the RC2 repo, compare the branch named in the CI line with develop, and pass failure_line two completed runs whose offsets reverse chronological order. Remedy: Ask gh for the repository default branch, parse createdAt as timestamps, and page until a completed non-cancelled run is found or a small cap of decisive runs is reached.
- **F16-5** [major] `crates/c3/src/engines/kimi.rs:319`, `crates/c3/src/engines/kimi.rs:268`, `crates/c3/src/engines/kimi.rs:278` - Kimi worktree removal can still delete outside the worktree if the path becomes a junction after the non-following metadata check. canonicalize then follows it and that result becomes the walk root, and git worktree remove --force and remove_dir_all later receive the path with no directory handle tying them to the directory that was checked. Trigger: The throwaway path is a real directory at the start of removal, and a junction to another directory replaces it before canonicalize or before the destructive calls. Evidence: read-code: real_dir_canonical uses symlink_metadata and then canonicalize. A clean pass calls git worktree remove --force and, if the path remains, remove_dir_all. An unlink error returns before those calls. Verify: Run RC3 and compare the sentinel before and after. Remedy: Hold a directory handle opened without following reparse points, refuse the removal if the path's identity changes, and do not call git or remove_dir_all on a path that is not that handle. Supersedes: F02-21, F03-2.
- **F16-6** [minor] `crates/c3/src/cli/router.rs:233`, `crates/c3/src/panel/plan.rs:1007`, `crates/c3/src/router/replay.rs:92` - router explain and router replay can show a different seat from the panel that ran. Explain marks the highest-weight drawable entry as taking the lab's seat even when a required or weighty sibling was seated, or when that lab won no seat. Replay rebuilds the draw from scores and multipliers rounded to 4 decimals, so a close live ordering can replay to another winner. Trigger: A required or weighty entry is seated ahead of a higher-weight sibling of the same lab, or two eligible weights differ only past the fourth decimal. Evidence: read-code: pick_of chooses max weight among drawable rows and the mark says that position takes its seat. The ledger stores round(score, 4) and round(mult, 4). Replay multiplies those stored values. Verify: Run RC4, and explain a roster where a required zhipu entry is lighter than its unpinned sibling. Remedy: Say which entry represents the lab in the draw, and name the position the panel actually seated. Replay from the unrounded weight, or record that weight.
- **F16-7** [minor] `crates/c3/src/panel/plan.rs:64`, `crates/c3/src/panel/plan.rs:108`, `crates/c3-core/src/claude.rs:836`, `CHANGELOG.md:148` - Lab identity merges some distinct makers and splits others, and the changelog overstates the rule. Prefix matching is starts-with with no boundary. Declared labs are not case-folded. lab_of_host returns minimax for api.minimax.io, but LAB_VENDORS has no minimax prefix, so a non-endpoint minimax model is its own lab. kimi and kimi-claude share a lab only when the model prefix, endpoint host or declared lab agrees. Trigger: One entry's model is opus-custom with an empty lab, another is claude-opus-4-8; or one lab is OpenAI and the other resolves to openai; or one minimax entry is an api.minimax.io endpoint and the other is model minimax-m2 with no declared lab. Evidence: read-code: by_prefix uses starts_with and an o-plus-digit rule. entry_lab returns the declared lab unchanged. lab_of_host maps api.minimax.io to minimax. CHANGELOG.md:148 states that kimi and kimi-claude are one lab. Verify: Call member_lab on those four entries and compare the lab strings. Remedy: Match a prefix only at a token boundary, case-fold declared labs, add minimax to the prefix table, and describe the kimi case as model, host or declared lab rather than provider label.
- **F16-8** [minor] `crates/c3/src/panel/routing.rs:800`, `crates/c3/src/panel/plan.rs:782`, `CHANGELOG.md:157` - On a weighty purpose, a panel of two or more seats can finish with no weighty member when the only eligible weighty entries share a lab with a required entry that is not weighty. The changelog says a weighty seat is given unless a required entry already is one. The tier draw excludes labs already held by pins, and no warning says the guarantee was missed. Trigger: Purpose framing, panel size at least 2, one required glm entry, and the only panel:weighty entry is another glm model. Evidence: read-code: tier_owed is true when no pin is in the tier list. open(only_tier) drops candidates whose lab is already seated. An empty pool adds no tier seat. Verify: Seat that fixture through select_panel_routing_with and inspect picked rules and warnings. Remedy: When the tier is owed and no tier seat is taken, warn that every weighty candidate's lab is already held, and name those entries.
- **F16-9** [minor] `crates/c3/src/ci_check.rs:343`, `crates/c3/src/ci_check.rs:302` - Two SessionStart hooks can both pass the daily gate and both call gh. Each writes ci-check.json from the snapshot it read, so the later rename drops the other repository's attempt timestamp and that repository is asked again immediately. Trigger: Two sessions start in different repositories, or two sessions start in one repository, before either write is visible to the other. Evidence: read-code: hook_line reads the file, checks due, then write_state of with_attempt. write_state is a temp file and rename with no lock. Verify: Start two hook_line calls together against one state file and count gh invocations and remaining repo keys. Remedy: Lock the state file for the read-modify-write, or store one file per repository.
- **F16-10** [note] `plugin/hooks/c3-hook.sh:66`, `plugin/hooks/c3-hook.ps1:91` - The two hook scripts are not byte-identical for a non-ASCII line longer than 400 characters. The shell script cuts with awk length and substr, which count bytes on mawk, and it deletes every CR. PowerShell cuts UTF-16 code units and can split a surrogate pair. The changelog says the two print byte-identical text. Trigger: c3 hook prints a prefixed line whose UTF-8 length exceeds 400, or a line contains an embedded CR or an emoji. Evidence: read-code: sh uses tr -d '\r' and awk substr. ps1 uses TrimEnd and Substring(0, 400). Both append ... on the prefixed path. Verify: Run both scripts against a fake c3 that prints one 401-character line containing a multibyte character and compare the bytes. Remedy: Cut on Unicode scalars in both scripts, strip only a trailing CR, and keep the known byte-cut limitation out of the changelog until that test passes.

### Prior findings

- F02-1 - still-open - replace_file renames a new file, so a second hard link keeps the old bytes, but the temp is created before the second link check.
- F02-2 - fixed - A failed write removes the temp; the original is replaced only by rename.
- F02-3 - not-checked - Not read in this pass.
- F02-4 - not-checked - Not read in this pass.
- F02-5 - fixed - The auth probe sets C3_CHILD after env_clear, and detached_environment forces C3_CHILD=1.
- F02-6 - not-checked - Not read in this pass.
- F02-7 - not-checked - Not read in this pass.
- F02-8 - not-checked - Not read in this pass.
- F02-9 - not-checked - Not read in this pass.
- F02-10 - not-checked - Not read in this pass.
- F02-11 - not-checked - Not read in this pass.
- F02-12 - not-checked - Not read in this pass.
- F02-13 - not-checked - Not read in this pass.
- F02-14 - not-checked - Not read in this pass.
- F02-15 - not-checked - Not read in this pass.
- F02-16 - not-checked - Not read in this pass.
- F02-17 - not-checked - Not read in this pass.
- F02-18 - not-checked - Not read in this pass.
- F02-19 - still-open - Per-class public:true gating is present; same-host URLs still share one trusted source. See the new telemetry finding.
- F02-20 - fixed - model_token_with has no Claude-shape exception.
- F02-21 - still-open - An unlink error no longer force-removes. A root swapped for a junction can still be adopted. See the new kimi finding.
- F03-1 - fixed - child_environment sets C3_CHILD=1 after filtering the parent value.
- F03-2 - still-open - Reparse points are classified as links and not descended. A link created after the check can still be acted on.
- F03-3 - not-checked - Not read in this pass.
- F03-4 - not-checked - Not read in this pass.
- F03-5 - fixed - strip_1m compares the suffix with eq_ignore_ascii_case.
- F03-6 - not-checked - Not read in this pass.
- F04-1 - not-checked - Not read in this pass.
- F04-2 - not-checked - Not read in this pass.
- F04-3 - not-checked - Not read in this pass.
- F04-4 - fixed - doctor --fix skips delegation repair and worktree removal when in_c3_child is set.
- F04-5 - fixed - remove_worktree_dir returns before git worktree remove when the unlink pass reports an error.
- F04-6 - fixed - claude_auth probe sets C3_CHILD=1, and the detached maintain environment test expects that mark.
- F04-7 - still-open - default_branch still falls through to local main and then master.
- F04-8 - fixed - The Claude shape is not sent by name. Catalog GET uses SameOrigin and the 4 MiB cap; the kimi host pair stays on the installer policy.
- F04-9 - not-checked - Not read in this pass.
- F10-1 - not-checked - Not read in this pass.
- F10-2 - not-checked - Not read in this pass.
- F10-3 - not-checked - Not read in this pass.
- F10-4 - not-checked - Not read in this pass.
- F10-5 - not-checked - Not read in this pass.
- F10-7 - not-checked - Not read in this pass.
- F11-1 - not-checked - Not read in this pass.
- F11-2 - not-checked - Not read in this pass.
- F11-3 - not-checked - Not read in this pass.
- F11-4 - not-checked - Not read in this pass.
- F11-5 - not-checked - Not read in this pass.
- F11-6 - not-checked - Not read in this pass.
- F11-7 - not-checked - Not read in this pass.
- F11-8 - not-checked - Not read in this pass.
- F11-9 - not-checked - Not read in this pass.
- F11-10 - not-checked - Not read in this pass.
- F11-11 - not-checked - Not read in this pass.
- F11-12 - not-checked - Not read in this pass.
- F12-1 - not-checked - Not read in this pass.
- F12-2 - not-checked - Not read in this pass.
- F12-3 - not-checked - Not read in this pass.
- F12-4 - not-checked - Not read in this pass.
- F13-1 - not-checked - Not read in this pass.
- F13-2 - still-open - find_repo_root still treats any existing .git entry as the repository root.
- F13-3 - not-checked - Not read in this pass.
- F13-4 - not-checked - Not read in this pass.
- F13-5 - still-open - line_within still stops waiting without killing the scan thread or its git children.
- F13-6 - not-checked - Not read in this pass.
- F13-7 - not-checked - Not read in this pass.
- F13-8 - not-checked - Not read in this pass.
- F14-1 - not-checked - Not read in this pass.
- F14-2 - not-checked - Not read in this pass.
- F14-3 - not-checked - Not read in this pass.
- F14-4 - not-checked - Not read in this pass.
- F14-5 - not-checked - Not read in this pass.
- F15-1 - not-checked - Not read in this pass.
- F15-2 - not-checked - Not read in this pass.
- F15-3 - not-checked - Not read in this pass.
- F15-5 - not-checked - Not read in this pass.
- F15-6 - not-checked - Not read in this pass.

## Verdict: HOLD

The one-lab draw and the Claude-id telemetry gate are in place, but a same-host catalog can still publish a private model name, the CI check can hang or inject a run title, and kimi removal can still act on a path replaced by a link.

### Blockers

- **F02-1** (prior, still-open) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, fixed) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, still-open) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, still-open) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, fixed) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, not-checked) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, not-checked) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.

### Unproven scenarios

- Whether git worktree remove --force follows a junction on the Windows version this release ships.
- Whether GitHub's createdAt is always a Zulu timestamp, which would make the text sort safe.
- Whether Rust remove_dir_all on the project's toolchain refuses to follow a junction.
- The numeric effect of 4-decimal rounding on a real panel seed.
- Same-host catalog behavior against a live maintain state file rather than the functions as read.
- Prior findings marked not-checked were not re-read.

### First-run checklist (observable)

_(none)_
