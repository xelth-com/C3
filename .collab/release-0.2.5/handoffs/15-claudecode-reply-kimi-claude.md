# Handoff 15 - Claude (claude): reply-kimi-claude

Date: 2026-11-23 00:41 local. Author: Claude (claude) (model k3.1-pro, effort high), claude-cli 2.1.330.
Reviewer: kimi-claude :: k3.1-pro [claude] (provider from roster, model from roster; engine claude (C:\users\dmytro\.local\bin\claude.exe), endpoint https://api.kimi.ai/coding/ (token from env KIMI_API_KEY); provider fingerprint ed7090cdf97a; harness claude-cli 2.1.330).
Preflight: ok: env KIMI_API_KEY set.
Effort: high sent (requested high, mapping claude-v1, by caps-v1: engine:claude, any model; not confirmed by the provider). Consultation id: 2791aeef-c6dc-426d-84aa-eaac3b1a5f17.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `claude -p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model k3.1-pro --effort high --json-schema "{"$schema":"http://json-schema.org/draft-07/schema#","title":"codex-consult reply v1","type":"object","additionalProperties":false,"required":["schema_version","verdict","verdict_reason","reply_markdown","findings","prior_findings","unproven","first_run_checklist"],"properties":{"schema_version":{"type":"string","enum":["1"]},"verdict":{"type":"string","enum":["ACCEPT","HOLD","REJECT","ADVISE"]},"verdict_reason":{"type":"string"},"reply_markdown":{"type":"string"},"findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["severity","locations","claim","trigger","evidence","verification","remedy","supersedes"],"properties":{"severity":{"type":"string","enum":["blocker","major","minor","note"]},"locations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["path","line"],"properties":{"path":{"type":"string"},"line":{"type":["integer","null"]}}}},"claim":{"type":"string"},"trigger":{"type":"string"},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","reference","observation"],"properties":{"kind":{"type":"string","enum":["read-code","ran-command","inferred","assumed"]},"reference":{"type":"string"},"observation":{"type":"string"}}}},"verification":{"type":"string"},"remedy":{"type":"string"},"supersedes":{"type":"array","items":{"type":"string"}}}}},"prior_findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","status","note"],"properties":{"id":{"type":"string"},"status":{"type":"string","enum":["fixed","still-open","not-checked","unknown-id"]},"note":{"type":"string"}}}},"unproven":{"type":"array","items":{"type":"string"}},"first_run_checklist":{"type":"array","items":{"type":"string"}}}}" --session-id 7d91838e-116a-457f-8ad8-212f697437c7` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-fa9e6af1721846a7b8f3f19b3cfe2ea5` of revision 8e6fd3ee772711d34c6826a6a0f11fb03cc105e1 (--rev 8e6fd3ee772711d34c6826a6a0f11fb03cc105e1; removed after the run).
Parent thread: (none - new thread; no thread of lineage kimi-claude :: k3.1-pro in this task's ledger; other lineage(s): grok :: grok-4.7 [grok], ZAI-claude :: glm-5.3 [claude], openai :: gpt-6.2-sol, meta :: muse-spark-1.3-contributor [muse]). Result thread: `7d91838e-116a-457f-8ad8-212f697437c7` (source: events).
Brief: `.collab/release-0.2.5/handoffs/07-claude-diff-review-maintain.md` (sha256 6852a0955330). Reviewed: 4f43074, base 4f430740c2ce1936fdff3649d67ed21d7ca1e5d4, tree sha256 24740c634eea, 0 changed files.
Bridge outcome: usable reply. Wall time: 549.6 s. Tokens: in 2332501 (cached 2168832), out 25330, reasoning 0.
Engine turns: 1 (claude -p, auth endpoint; model k3.1-pro; init tools Glob, Grep, Read, StructuredOutput; permission denials 0).
Warnings: the live collab directory changed during the run (1 file: .collab/release-0.2.5/handoffs/07-claude-diff-review-maintain.patch) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The hardening is substantial, but F04-3 remains reproducible on first-seen false date evidence and new refusal-eviction, smoke-binding, roster-race, and pending-update races can still produce unsafe automatic changes. Findings: 0 blocker, 5 major, 1 minor, 0 note (F15-1..F15-6, tracked in `findings.json`). Structured reply: `handoffs/15-claudecode-reply-kimi-claude.reply.json`.
Raw event stream: `handoffs/15-claudecode-reply-kimi-claude.events.jsonl`.
Verbatim reply follows.

---

## Verdict: HOLD

**Q1 — Fetch.** Redirect scheme, host, port and userinfo checks are correctly re-applied; public bodies stop at 4 MiB and 5,000 retained ids. No API key is attached to public requests, although `User-Agent: c3/<version>` identifies the client by design. Finding #1 remains: URL checks do not constrain the resolved address, so a custom hostname can resolve/rebind to loopback or a private network. Whether stalled DNS escapes the absolute 15-second deadline is unproven without exercising ureq.

**Q2 — Hostile body.** I found no command/path/report injection: accepted ids are ASCII-only, at most 100 bytes, contain no whitespace/quotes/backslashes, and are passed as argv and JSON strings. `/`, `:` and `..` are permitted but are not used as paths. Public parsing is bounded; endpoint lists instead have an 8 MiB cap and no 5,000-id/deduplication limit.

**Q3 — Date gate and family.** Future dates and contradictory currently visible sources are handled, and family comparison is numeric with identical prefix/suffix. **F04-3 is still open:** on the first false catalog assertion, `created(candidate) > created(seated)` still qualifies immediately (`models.rs:478-481`); only previously observed negative evidence is remembered. Finding #2 adds that 501 newer refusal records evict that memory, allowing a re-stamped older model back.

**Q4 — Smoke binding and money.** The ordinary model-swap path uses one fresh snapshot and refuses protected, inactive and on-request seats. Finding #3 covers engine-update smokes: a same-provider roster replacement is accepted because the old model is never checked, potentially changing endpoint/plan after quota selection. Normal returns remove the temporary roster; process death or removal failure can leave it. Finding #6 shows `off --models` nevertheless creates the state directory and lock. Dry-run itself performs no smoke or state write.

**Q5 — Roster writer and races.** Parsed spans, escaped values, duplicate-path refusal, semantic verification, backup and atomic rename are sound; BOM is refused safely and CRLF outside the literal survives. Finding #4 remains: an edit landing after the final comparison but before rename is overwritten.

**Q6 — Pending update and claims.** The newer path preserves `previous`, writes before installation and smokes afterward; spawned children are marked `C3_CHILD=1`. Finding #5 shows it compares the vendor release with a stale pre-smoke/pre-rollback version and can downgrade a concurrently replaced install. The CHANGELOG’s `off ... writes nothing` is also broader than reality (#6).

**Q7 — Untested.** No commands were available or run. Real DNS/redirect/TLS behavior, `model_pages`, a live roster swap, crash cleanup, and vendor recovery remain unexercised.

## Requested checks

- **RC1 (finding #1):** From a workspace-write test copy, add a controlled HTTPS hostname whose resolution can switch to 127.0.0.1 and a delayed resolver fixture; run public fetch. Observe the connected peer and elapsed time, including whether DNS exceeds 15 s. Budget: 15 minutes.
- **RC2 (finding #2):** Add a test creating 501 distinct older pairs, then re-date the evicted highest successor newer with a scripted passing smoke; run `cargo test -p c3 older_refusal_survives_cap -- --nocapture`. It currently should expose a candidate. Budget: 5 minutes.
- **RC3 (finding #3):** In a workspace-write integration fixture, replace the selected position’s model/endpoint after `pick_target` but before smoke, capturing the fake reviewer URL. Observe whether the changed endpoint receives turns. Budget: 10 minutes.
- **RC4 (finding #4):** Add a test seam between `commit_swap`’s final read and rename, write a hand edit there, and run the swap test. Observe whether that edit survives. Budget: 10 minutes.
- **RC5 (finding #5):** Make fake rollback install 1.0.90 but exit nonzero while the original failure is 1.0.80 and vendor latest is 1.0.82; run the pending-update integration test and observe the attempted downgrade. Budget: 10 minutes.

---

### Findings

- **F15-1** [major] `crates/c3/src/c3config.rs:329`, `crates/c3/src/maintain/env.rs:272`, `crates/c3/src/maintain/env.rs:379` - Public-source origin validation constrains URL strings but not the resolved peer address: an HTTPS IP literal, localhost/private address, or DNS rebinding can receive the keyless GET even when the configured hostname was public. Same-origin redirects revalidate only host text and port, so they do not prevent this address change. Trigger: A configured catalog/page hostname resolves or rebinds to loopback, link-local, or a private-network address (or is written as an IP literal), and maintain fetches it or a same-host redirect. Evidence: read-code: is_public_https_url checks scheme, nonempty host and absence of userinfo only; IP class is unrestricted.; read-code: Start and redirect checks compare host strings and ports, with no address resolution, IP-range rejection, or peer pinning. Verify: Use a controlled HTTPS hostname that resolves first to a test server and then to 127.0.0.1 across a same-host redirect; record the actual peer accepting each connection. Remedy: Resolve under the fetch deadline, reject disallowed loopback/private/link-local destinations for public sources, and pin each accepted connection to the validated address; document an explicit opt-in if internal catalogs must remain supported.
- **F15-2** [major] `crates/c3/src/maintain/models.rs:63`, `crates/c3/src/maintain/models.rs:66`, `crates/c3/src/maintain/models.rs:453`, `crates/c3/src/maintain/mod.rs:1623` - The permanent older-pair refusal is forgotten after more than 500 distinct pairs: merge_older drops the oldest entries, after which a source can re-stamp that candidate newer and make it eligible for smoke and seating again. Trigger: A pass accumulates at least 501 older pairs, evicting an earlier pair; a later catalog dates that candidate after the seated id and the candidate's smoke passes. Evidence: read-code: merge_older drains from the front whenever the vector exceeds 500; existing pairs are not refreshed or otherwise preserved.; read-code: Once recorded_older no longer finds the pair, ordinary positive created-date evidence can return Ok.; read-code: An eligible successor is smoked and seated, while only the capped merged vector is persisted. Verify: Build 501 distinct older pairs including a highest-version target, persist them, then supply a later catalog that re-dates the evicted target newer and assert whether it becomes a SwapCandidate. Remedy: Do not represent permanent refusals as a lossy FIFO; retain durable negative evidence per seated/candidate pair (or use a non-evicting cryptographic/source-date design) and add a regression crossing the cap.
- **F15-3** [major] `crates/c3/src/maintain/smoke.rs:251`, `crates/c3/src/maintain/smoke.rs:264`, `crates/c3/src/maintain/smoke.rs:302`, `crates/c3/src/maintain/mod.rs:1023`, `crates/c3/src/maintain/mod.rs:1335` - An entry-bound engine smoke does not verify that the roster position still contains the selected old model. If that position is edited to another same-provider entry before roster_text_of reads it, bound_roster copies the new endpoint/plan/key configuration and overwrites only its model with the stale target, spending on an entry other than the one quota selection judged. Trigger: After pick_target selects reviewers[N], an operator or another process changes reviewers[N].model, endpoint or plan while retaining its provider before an installed-version or post-update smoke. Evidence: read-code: bound_roster compares only provider, then replaces model unconditionally; it has no expected-old-model argument.; read-code: Targets come from the pass-start parsed roster, while roster text is re-read later without revalidating model or saturation. Verify: Change the selected position's model and endpoint between target selection and smoke in a fake-reviewer integration test, and inspect which endpoint receives both turns. Remedy: Revalidate provider, expected old model, active/protection state and quota against the exact fresh snapshot before every bound smoke; fail closed or re-pick when the position changed.
- **F15-4** [major] `crates/c3/src/maintain/models.rs:828`, `crates/c3/src/maintain/models.rs:838`, `crates/c3/src/maintain/models.rs:843`, `crates/c3-core/src/store.rs:596`, `crates/c3-core/src/store.rs:613` - The roster compare-then-rename sequence still has a write race: a hand edit or another writer landing after the final equality check but before fs::rename is silently replaced by the stale updated roster. Trigger: Another process writes the roster in the interval between commit_swap's read_to_string comparison and write_text_atomic's rename. Evidence: read-code: The last comparison and atomic replacement are separate filesystem operations with no roster lock held across them.; read-code: write_text_atomic writes a temporary file and later renames it without another content comparison or compare-and-swap guard. Verify: Pause commit_swap after its final equality check, modify the roster, resume the rename, and compare the resulting file with the intervening edit. Remedy: Introduce a roster write lock shared by all C3 writers and hold it across final validation and replacement; where external editors cannot participate, document the residual race or use a platform-supported transactional/CAS mechanism rather than claiming complete protection.
- **F15-5** [major] `crates/c3/src/maintain/mod.rs:1006`, `crates/c3/src/maintain/mod.rs:1225`, `crates/c3/src/maintain/mod.rs:1242`, `crates/c3/src/maintain/mod.rs:1246` - Pending recovery chooses a newer release using the version captured before the recovery smoke and rollback, not the version actually installed afterward. A failed rollback or concurrent manual update that leaves a newer binary can therefore make C3 install an older vendor release. Trigger: The initial failing version is 1.0.80, rollback/concurrent activity leaves 1.0.90 without a successful rollback result, and the vendor reports 1.0.82; latest_v is greater than stale 1.0.80, so the downgrade proceeds. Evidence: read-code: handle_pending passes its original installed snapshot into try_newer_release after recovery and rollback work.; read-code: try_newer_release compares latest only with that stale snapshot and never re-reads installed_version before set_pending/do_update.; read-code: The code itself recognizes rollback may leave a different now_t version, but does not return that version to the newer-release decision. Verify: Script a failed rollback that leaves version 1.0.90 after an initial 1.0.80 failure, offer 1.0.82 as latest, and record the version passed to do_update. Remedy: Re-read and parse the installed version immediately before selecting and recording the recovery target; require latest to be strictly newer than that current binary, and abort if it changes again before installation.
- **F15-6** [minor] `crates/c3/src/maintain/mod.rs:619`, `crates/c3/src/maintain/mod.rs:378`, `crates/c3/src/maintain/mod.rs:797`, `CHANGELOG.md:91` - C3_MAINTAIN_MODELS=off with a non-dry-run --models still creates the maintain state directory and writes a lock record before reaching the off return, contrary to the documented 'writes nothing'; it can also remove a stale lock during acquisition. Trigger: Run c3 maintain --models with C3_MAINTAIN_MODELS=off and no existing pass lock, with or without an existing maintain state directory. Evidence: read-code: run_pass acquires the lock before models_pass evaluates ModelsMode::Off.; read-code: acquire_lock creates the directory and lock file and writes JSON; stale-lock takeover may remove an existing file.; read-code: The user-facing claim says off fetches nothing and writes nothing without excluding lock/state-directory effects. Verify: In a fresh temporary CODEX_HOME, run the off --models command and inspect the maintain directory and filesystem writes, rather than checking only state.json. Remedy: Handle models_only plus ModelsMode::Off in run_pass before lock acquisition, or narrow the documentation and test to state.json specifically.

### Prior findings

- F02-1 - not-checked - Repository repair is outside this maintain-only diff.
- F02-2 - not-checked - Repository repair is outside this maintain-only diff.
- F02-3 - not-checked - Repository repair is outside this maintain-only diff.
- F02-4 - not-checked - Repository repair is outside this maintain-only diff.
- F02-5 - not-checked - The general process-marking fix was not re-reviewed here.
- F02-6 - not-checked - Hook Git/deadline behavior is outside this diff.
- F02-7 - not-checked - Branch descriptions are outside this diff.
- F02-8 - not-checked - Branch/doctor index behavior is outside this diff.
- F02-9 - not-checked - Branch reporting is outside this diff.
- F02-10 - not-checked - Branch patch reporting is outside this diff.
- F02-11 - not-checked - Worktree reporting is outside this diff.
- F02-12 - fixed - Public redirects now use start-host/port SameOrigin without installer exceptions; userinfo and downgrades are refused. Resolved-address handling is new finding #1.
- F02-13 - fixed - Public reads use the 4 MiB reader cap and remaining time per hop; real stalled-DNS behavior remains unproven.
- F02-14 - fixed - Future timestamps and contradictory current-source negative evidence are gated; bounded-history eviction is new finding #2.
- F02-15 - fixed - Endpoint sources now include canonical scheme/host/port/path, key variable and plan.
- F02-16 - fixed - The original same-label model-swap case is bound to the exact position and fresh basis; stale engine-target replacement is new finding #3.
- F02-17 - fixed - The writer parses reviewers[position].model, replaces the complete literal and verifies the semantic delta.
- F02-18 - fixed - Explicit --models no longer overrides off or performs fetches; residual lock-directory writes are new finding #6.
- F02-19 - not-checked - Telemetry disclosure was not re-reviewed in this maintain diff.
- F02-20 - not-checked - Telemetry classification was not re-reviewed in this maintain diff.
- F02-21 - not-checked - Kimi worktree removal is outside this diff.
- F03-1 - not-checked - Claude child-environment behavior is outside this diff.
- F03-2 - not-checked - Kimi worktree removal is outside this diff.
- F03-3 - not-checked - Branch descriptions are outside this diff.
- F03-4 - not-checked - Agent frontmatter repair is outside this diff.
- F03-5 - not-checked - Telemetry normalization is outside this diff.
- F03-6 - not-checked - Branch listing cost and index writes are outside this diff.
- F04-1 - not-checked - Read-only Git status behavior is outside this diff.
- F04-2 - fixed - Own-list candidates now receive negative date evidence when a public source dates both ids.
- F04-3 - still-open - models.rs:478-481 still accepts a first-seen candidate solely because its catalog created stamp is later than the seated id; history helps only after an older observation. Finding #2 additionally covers eviction.
- F04-4 - not-checked - doctor --fix child behavior is outside this diff.
- F04-5 - not-checked - Kimi worktree removal is outside this diff.
- F04-6 - not-checked - General child-process marking was not re-reviewed here.
- F04-7 - not-checked - Branch/worktree classification is outside this diff.
- F04-8 - not-checked - The fetch half is hardened, but the Anthropic telemetry half was not rechecked.
- F04-9 - not-checked - Branch description preservation is outside this diff.
- F10-1 - not-checked - Agy tool classification is outside this diff.
- F10-2 - not-checked - Agy event merging is outside this diff.
- F10-3 - not-checked - Kimi staging is outside this diff.
- F10-4 - not-checked - Panel snapshots are outside this diff.
- F10-5 - not-checked - Sub-agent smoke coverage was not re-reviewed here.
- F10-7 - not-checked - Detached death records are outside this diff.
- F11-1 - not-checked - Agy write classification is outside this diff.
- F11-2 - not-checked - Agy artifact binding is outside this diff.
- F11-3 - not-checked - Kimi staging is outside this diff.
- F11-4 - not-checked - Kimi brief availability is outside this diff.
- F11-5 - not-checked - Kimi prompt disclosure is outside this diff.
- F11-6 - not-checked - Panel blocker snapshots are outside this diff.
- F11-7 - not-checked - Codex sub-agent switch proof was not re-reviewed here.
- F11-8 - not-checked - Codex rollout parsing was not re-reviewed here.
- F11-9 - not-checked - Spawn/rollout attribution is outside this diff.
- F11-10 - not-checked - Secondary prompts are outside this diff.
- F11-11 - not-checked - Detached death records are outside this diff.
- F11-12 - not-checked - Provider reply accounting is outside this diff.
- F12-1 - not-checked - Stale-worktree liveness is outside this diff.
- F12-2 - not-checked - Git environment claims are outside this diff.
- F12-3 - not-checked - Windows repair reparse points are outside this diff.
- F12-4 - not-checked - The analogous branch-description race was not re-reviewed; roster races are new finding #4.
- F13-1 - not-checked - Stale-worktree removal is outside this diff.
- F13-2 - not-checked - SessionStart repository-root detection is outside this diff.
- F13-3 - not-checked - Branch record parsing is outside this diff.
- F13-4 - not-checked - Update/branch child guards are outside this diff.
- F13-5 - not-checked - Hook scan deadlines are outside this diff.
- F13-6 - not-checked - AGENTS.md model repair is outside this diff.
- F13-7 - not-checked - Branch cherry reporting is outside this diff.
- F13-8 - not-checked - Kimi owner-file staleness is outside this diff.

## Verdict: HOLD

The hardening is substantial, but F04-3 remains reproducible on first-seen false date evidence and new refusal-eviction, smoke-binding, roster-race, and pending-update races can still produce unsafe automatic changes.

### Blockers

- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, fixed) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, not-checked) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, not-checked) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, not-checked) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.

### Unproven scenarios

- ureq 2.12.1's actual behavior for stalled DNS resolution and TLS setup under the request deadline was not executed.
- No real catalog, redirecting HTTPS host, model_pages source, live roster swap, or vendor release recovery was exercised.
- Crash/kill and filesystem-error cleanup of the scratch repository and one-entry roster was not exercised.
- The repository test suite was not run because this consultation provided no command tool.
- Windows ACL/permission behavior of atomic roster replacement was not exercised.

### First-run checklist (observable)

_(none)_
