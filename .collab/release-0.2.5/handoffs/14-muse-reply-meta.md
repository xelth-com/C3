# Handoff 14 - Meta Muse (muse): reply-meta

Date: 2026-11-23 00:37 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.25-R9361.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.25-R9361.1).
Preflight: ok: signed in (usable reply 43 min ago).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: 243d8de0-8cdc-42a0-b819-c0964b80a2ea.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-0b3f611aa1df40a6a9a336286804bbf6.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-37ba10fcbfdc4c219650ba29a4047b61` of revision 8e6fd3ee772711d34c6826a6a0f11fb03cc105e1 (--rev 8e6fd3ee772711d34c6826a6a0f11fb03cc105e1; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20599-ec3f-7543-984a-393ce064175d` (source: events).
Brief: `.collab/release-0.2.5/handoffs/07-claude-diff-review-maintain.md` (sha256 6852a0955330). Reviewed: 4f43074, base 4f430740c2ce1936fdff3649d67ed21d7ca1e5d4, tree sha256 24740c634eea, 0 changed files.
Bridge outcome: usable reply. Wall time: 328.7 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: muse: warning: rules file at \\?\C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-37ba10fcbfdc4c219650ba29a4047b61\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules; muse: warning: project MCP servers from .mcp.json, if any, are not loaded because the workspace is untrusted; restart with --trust-workspace to load them; the live collab directory changed during the run (1 file: .collab/release-0.2.5/handoffs/07-claude-diff-review-maintain.patch) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The hardening is real, but the deadline, permanence, temp-write, and protection-check residuals above must be fixed or scoped before release. Findings: 0 blocker, 3 major, 2 minor, 0 note (F14-1..F14-5, tracked in `findings.json`). Structured reply: `handoffs/14-muse-reply-meta.reply.json`.
Raw event stream: `handoffs/14-muse-reply-meta.events.jsonl`.
Verbatim reply follows.

---

Q1. Redirects now stay on the start host+port with no userinfo (`env.rs:352`, `web.rs:192`) and keys never follow (`Redirects::Never`). Residuals: (a) no SSRF guard — any configured https host, including IP literals/loopback/private nets, is fetchable and each hop re-resolves DNS with no pinning; (b) the 15 s deadline is checked only between hops while `timeout_connect` stays at the full window (`env.rs:163`) and `read_capped` streams with no deadline check — finding #1; (c) only `User-Agent: c3/<version>` is sent besides endpoint `Authorization` — no key on public GETs, but the version is identifying.

Q2. Hostile bodies look contained: ids must pass `valid_model_id` (ASCII, no quote/backslash/newline, ≤100) before storage/comparison/roster edit (`version.rs:90`, `web.rs:210`, `models.rs:797`); vendor slugs are restricted and `:variant`s dropped (`web.rs:245`); bodies over the cap parse to nothing. No roster/CLI/path injection found in this part.

Q3. Date gate is much stronger (negative evidence wins, both-dated own-list gate, recorded pairs). Residuals: the 500-pair cap evicts the oldest refusal so permanence is false (finding #2); a future stamp within 24 h still seats an unanchored candidate (finding #5); future refusals are not recorded so a hostile future stamp retries every pass (by design, noisy but safe).

Q4. Seat binding is real: `Bind::Entry` position check, fresh `basis` re-read, same-snapshot smoke+swap, one-entry roster removed on drop, `--dry-run` writes nothing (`mod.rs:780`, `models_pass:815`). Residuals: predictable sibling temp write with no exclusive create (finding #3); `Bind::Label` explicit overrides resolve by label with no protection check; narrow `bound_roster` type checks (finding #4).

Q5. Writer is good: parsed span, provider+old recheck, whole-literal JSON replacement, semantic re-parse equality, double `current==basis` check, atomic write, once-per-day backup (`models.rs:786`). Residual: a microsecond TOCTOU remains between the final re-read and the atomic rename — concurrent writers can still lose; also deep (>64) or BOM'd rosters fail closed (safe).

Q6. Newer-release path keeps the original `previous`, re-records before install, honours cool-down/`--force`/in-use, and smokes post-update (`mod.rs:1272`). No `C3_CHILD` issue attaches to this part's children beyond the scratch smoke repo. CHANGELOG overclaims only in Q3's 'every later pass' (false past 500) — otherwise the sentences match the code read.

Q7. Untested that matters: real catalog/redirecting host/`model_pages`/roster swap/vendor release/real reviewer CLI (brief says so); slow-connect/trickle bodies; 500-cap eviction; symlink-at-roster-path; float/string protection variants; crash between newer-record write and install (retries same target each pass); Windows temp/rename behaviour.

## Requested checks
RC1. Time a catalog fetch through a 20 s stalled second-hop connect and a 1-byte/s body (read-only config + local stub; needs network-loopback permission); observation settling finding #1: both must end by ~15 s with a deadline error. Budget: minutes.
RC2. Seed state.older with 500 pairs then add the (seated,candidate) pair and run plan_swaps (workspace-write for state fixture); observation settling finding #2: refusal must persist. Budget: minutes.
RC3. Pre-create the expected sibling `<work>.roster.json` symlink to a sentinel and run a Bind::Entry smoke (workspace-write temp only); observation settling finding #3: sentinel unchanged and smoke fails closed. Budget: minutes.

---

### Findings

- **F14-1** [major] `crates/c3/src/maintain/env.rs:163`, `crates/c3/src/maintain/env.rs:240` - The claimed ONE absolute 15 s deadline does not bound the connect phase or a stalled streaming body: the ureq agent is built once with timeout_connect(policy.timeout) and the deadline is only re-checked between hops, while read_capped blocks in read_to_end with no deadline check. Trigger: A catalog redirect chain or body that stalls in DNS/connect or trickles one byte at a time; elapsed wall time exceeds 15 s while fetch reports no deadline error. Evidence: read-code: AgentBuilder uses .timeout_connect(policy.timeout) once; per-hop only .timeout(remaining) is updated, so a slow DNS/connect on a later hop gets a fresh full window.; read-code: fetch computes remaining and checks the deadline only at the top of each hop loop; Hop::Body then calls read_capped(reader, cap) which reads to cap+1 with no intermediate deadline test.; read-code: read_capped does r.take(cap+1).read_to_end with no timeout parameter; a trickling body can keep the call inside one hop past the deadline. Verify: Serve a catalog whose second hop stalls 20 s in connect, and one whose body trickles; time the fetch and confirm both end by 15 s with a deadline error. Remedy: Derive timeout_connect from remaining per hop (or cap it at remaining) and enforce the absolute deadline inside the body read (chunked read with per-chunk deadline checks).
- **F14-2** [major] `crates/c3/src/maintain/models.rs:1574` - The 500-pair cap evicts the oldest recorded refusals, so 'stays refused in every later pass' is false once more than 500 pairs accumulate: merge_older drains from the front, keeping only the newest. Trigger: More than 500 distinct (seated, candidate) older-pairs accumulate; the earliest refusal is evicted and a re-stamped old model becomes a candidate again. Evidence: read-code: merge_older appends new pairs then drains ..excess from the front; the capped test asserts only the newest are kept. Verify: Seed state.older with 500 other pairs, add one more older pair for the seated id, and assert plan_swaps still refuses it. Remedy: Either keep refusals unbounded (small records), pin eviction to never drop pairs for currently-seated ids, or correct the claim/docs to 'the newest 500 refusals are remembered'.
- **F14-3** [major] `crates/c3/src/maintain/smoke.rs:400` - The smoke's one-entry roster is written beside the scratch repo with a plain fs::write to a predictable sibling path and no exclusive create, so a pre-created symlink at that path redirects the write to an arbitrary file. Trigger: A symlink exists at the expected sibling <work>.roster.json path when a Bind::Entry smoke starts; the write follows it. Evidence: read-code: Path is work.with_file_name(<work-basename>.roster.json) in the shared temp dir; fs::write follows symlinks; cleanup relies only on RemoveOnDrop of the same path. Verify: Pre-create that sibling path as a symlink to a sentinel file, run a Bind::Entry smoke, and check the sentinel is unchanged. Remedy: Create the bound-roster file exclusively (O_EXCL / create_new) inside the scratch workdir or a uniquely created temp dir, and fail closed if it already exists.
- **F14-4** [minor] `crates/c3/src/maintain/smoke.rs:283` - bound_roster protection checks are narrower than 'never smoked': reserve_percent read via as_i64().unwrap_or(0) misses float/non-integer values, on_request requires literal boolean true, and the inactive match is exact case-sensitive provider/plan equality. Trigger: A roster entry carries "reserve_percent": 0.5 (float) or on_request as a string, or an inactive list with different case; bound_roster returns Ok and the seat is smoked. Evidence: read-code: reserve_percent uses .and_then(as_i64).unwrap_or(0) > 0, so 0.5 or "15" is treated as 0; on_request uses as_bool()==Some(true), so "true" passes; inactive compares trimmed exact strings only. Verify: Call bound_roster on fixtures with float reserve_percent, string on_request, and case-variant inactive entries and assert refusal. Remedy: Accept any numeric >0 (as_f64) for reserve_percent, any truthy on_request, and case-insensitive provider/plan matching consistent with roster validation.
- **F14-5** [minor] `crates/c3/src/maintain/models.rs:1747` - A future date within the 24 h slack still seats an unanchored candidate: negative evidence only fires past now+86400 while the unanchored rule accepts anything >= now-180d, so clock skew or a +23 h stamp is believed. Trigger: A catalog lists only the candidate with created = now + 23 h while the seated id is absent; plan_swaps returns it as a candidate. Evidence: read-code: negative_evidence refuses future only when c > now + FUTURE_SLACK_SECS (86400).; read-code: Unanchored branch qualifies when c >= now - 180d, which a +23 h stamp satisfies; the within-slack test documents +3600 as believed. Verify: Call plan_swaps with an unanchored candidate stamped now+23h and assert it is rejected, not a candidate. Remedy: Refuse any candidate dated after now (plus a small minutes-scale skew allowance) in the qualifying branches, not only past 24 h.

### Prior findings

- F02-1 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-2 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-3 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-4 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-5 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-6 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-7 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-8 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-9 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-10 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-11 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-12 - fixed - SameOrigin policy, per-hop revalidation, port/userinfo pinning and tests read in the patch.
- F02-13 - fixed - Absolute deadline and cap+1 reader plus tests read; residual connect/stream stall filed separately.
- F02-14 - fixed - Future/negative-evidence gate and recorded-older persistence plus tests read in the patch.
- F02-15 - fixed - Endpoint source keyed by endpoint|key|plan plus keying tests read in the patch.
- F02-16 - fixed - Bind::Entry one-entry roster, position check, and two-stub-endpoint test read in the patch.
- F02-17 - fixed - Parse-located literal replacement with semantic verify plus tests read in the patch.
- F02-18 - fixed - Authoritative off early-return in models_pass/run_models plus off test read in the patch.
- F02-19 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-20 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F02-21 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F03-1 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F03-2 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F03-3 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F03-4 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F03-5 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F03-6 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F04-1 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F04-2 - fixed - own_verdict both-dated gate plus own-list test read in the patch.
- F04-3 - fixed - Recorded-older persistence, merge_older cap, and restamp test read in the patch.
- F04-4 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F04-5 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F04-6 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F04-7 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F04-8 - fixed - PUBLIC_GET SameOrigin/4MiB policy plus redirect tests read in the patch.
- F04-9 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F10-1 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F10-2 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F10-3 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F10-4 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F10-5 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F10-7 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-1 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-2 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-3 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-4 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-5 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-6 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-7 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-8 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-9 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-10 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-11 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F11-12 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F12-1 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F12-2 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F12-3 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F12-4 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F13-1 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F13-2 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F13-3 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F13-4 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F13-5 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F13-6 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F13-7 - not-checked - Outside this part's paths; not reviewed in this consultation.
- F13-8 - not-checked - Outside this part's paths; not reviewed in this consultation.

## Verdict: HOLD

The hardening is real, but the deadline, permanence, temp-write, and protection-check residuals above must be fixed or scoped before release.

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

- Whether ureq's per-request timeout actually aborts a mid-body stall on all platforms (read-only review could not execute it).
- Whether the 500-pair eviction is reachable in practice before seated ids rotate (no live catalog history observed).
- Whether temp-dir symlink planting is exploitable in the operators' real environments.

### First-run checklist (observable)

_(none)_
