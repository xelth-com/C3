# Handoff 67 - Codex: acceptance-delta-2

Date: 2026-11-24 22:16 local. Author: Codex (model gpt-6.2-astra, effort high), Codex CLI 0.169.0.
Reviewer: openai :: gpt-6.2-astra (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.169.0).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - position 1 of 17.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 62589e21-c3ad-4bb0-8130-444e0a9701c7.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6.2-astra -c features.multi_agent=false -c agents.enabled=false -c features.plugins=false -c skills.include_instructions=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-4cc0c53314c54527be89d21e05f2f7c7.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-dec61c58d81a466aa45240a91c7015fb` of revision c4a74af81fa88f58bcb7b32699dda95a328172a9 (--rev HEAD; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20f65-71b4-7543-a5e8-fb15d19c94db` (source: events).
Brief: `.collab/release-0.2.5/handoffs/66-claude-acceptance-delta-2.md` (sha256 f81789147d78). Reviewed: c4a74af, base c4a74af81fa88f58bcb7b32699dda95a328172a9, tree sha256 90d10cbe7ab5, 0 changed files. Live HEAD at launch: c4a74af81fa88f58bcb7b32699dda95a328172a9.
Bridge outcome: usable reply. Wall time: 383.2 s. Tokens: in 1266582 (cached 1155200), out 10659, reasoning 434.
Quota (rollout): 5h 63% until 00:16; wk 93% until Nov 29; this run 1266582 input tokens (1155200 cached) in 16 calls, max 112631 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false, agents.enabled=false); budget 90 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s.
Verdict: REJECT - An ordinary successful update can credit the target and clear its pending record even when a different CLI release ran the smoke. Findings: 1 blocker, 0 major, 3 minor, 0 note (F67-1..F67-4, tracked in `findings.json`). Structured reply: `handoffs/67-codex-acceptance-delta-2.reply.json`.
Raw event stream: `handoffs/67-codex-acceptance-delta-2.events.jsonl`.
Verbatim reply follows.

---

1. **Closure of the eight rows.**

   | Row | Decision at `c4a74af` |
   |---|---|
   | F62-1, recovery identity | **Closed.** Recovery compares exact identities before crediting a pass and after rollback. The prerelease/final fixtures cover both directions. Finding #1 concerns the separate ordinary-install path. |
   | F62-2, collab resolver | **Closed.** `providers.rs:2569` normalizes the spelling before canonicalization. The consult dry-run checks the reported task directory, and the work test checks actual records. Panel and detach share the resolver. |
   | F62-3, catalog provenance | **Closed.** `maintain/web.rs:162` requires collection-time trust and a matching catalog URL. The legacy exception uses the exact built-in catalog name. The binary fixtures inspect serialized telemetry across reclassification and refresh. |
   | F62-4, displaced edits | **Closed for the reported collision.** `contain.rs:848` claims numbered names by hard link. The test preserves three distinct byte sequences. The random-name fallback at `contain.rs:887` still uses a checked rename. |
   | F62-5, nested collaboration tools | **Closed.** `quota.rs:1112` carries collaboration ancestry through recursive descent. Fixtures cover unfamiliar functions, hosted tools and malformed nested lists. |
   | F62-6, patch names | **Closed.** `consult/reviewed.rs:785` bounds the readable stem at a UTF-8 character boundary. The full digest survives in all three filename forms. |
   | Answer 3, unsupported exchange | **Closed.** `contain.rs:774` returns the refusal and requests temporary-file cleanup. The forced-refusal test checks the saved edit and the directory listing. |
   | Answer 3, later writes and Unix checkout | **Implementation limits remain open.** Later writes resolve paths after admission, and Git can populate a replacement directory before its identity is checked. The release notes describe both points at `CHANGELOG.md:693` and `CHANGELOG.md:896`. |

2. **One release blocker remains.** If the first post-install probe sees the requested target, `maintain/mod.rs:1052` selects a success path that trusts that probe through the entire smoke. A concurrent version change can leave `→ target ok` in the report and clear the pending record. Recovery already performs the missing final identity check at `maintain/mod.rs:1315`. This gap predates the delta but defeats its exact-attribution guarantee. I found no additional blocker in catalog provenance, displaced naming or exchange refusal. Findings #2 and #3 cover prerelease ordering and test portability.

3. **The two disclosed path limits do not independently block the tag** for an operator-controlled checkout and temp directory. The admission guarantee and the timing of the Unix identity check are explicit. The claim about permission equivalence at `CHANGELOG.md:901` needs qualification for shared checkouts (finding #4).

4. **REJECT**, even granting green three-job CI on the tag commit. A passing suite can miss finding #1. I verified HEAD and reviewed selected hunks against handoffs 56, 62–66 and the release notes. The recorded CI status for `82d2abe` and `c4a74af` was still running. I did not run builds or tests, inspect remote CI, read every hunk or detached log, or re-audit the earlier release. No files changed.

## Requested checks

- **RC1** — Repository root, workspace-write, one fake-Grok integration test. Have the post-install probe return the target, then switch the fixture’s installed version before the passing smoke launches. Inspect the action and pending journal. Acceptance requires an unresolved action, a retained record and no target success line.
- **RC2** — Repository root, workspace-write, one version unit test. Compare `1.0.0-18446744073709551616` with `1.0.0-100000000000000000000`. The second must sort newer.
- **RC3** — Repository root on a filesystem without exchange support, workspace-write, one test filter. Run `cargo test -p c3 without_an_exchange_the_write_is_refused_and_the_file_is_as_it_was`. The real unsupported operation after the hook is dropped must produce an asserted refusal and preserve the directory contents.

---

### Findings

- **F67-1** [blocker] `crates/c3/src/maintain/mod.rs:1052`, `crates/c3/src/maintain/mod.rs:1116`, `crates/c3/src/maintain/mod.rs:1157`, `crates/c3/src/maintain/mod.rs:999` - The ordinary target-install path can record a passing smoke against the wrong release and discard the only pending recovery record. Trigger: The post-install probe sees the target. A manual or automatic update replaces it before the smoke launches, and the replacement passes. Evidence: read-code: The target branch retains the pre-smoke version in after_text. A passing run_one_smoke goes directly to an ok result with resolved=true.; read-code: The caller clears a resolved record. The pending-recovery path explicitly measures identity again after the smoke and retains the record on drift. Verify: RC1. Compare the fake launcher's actual smoke-time version, the state action and the pending journal in the same run. Remedy: Apply the recovery path's final exact-identity check to the ordinary target success path. A changed or unreadable release should retain the pending record and receive no success attribution.
- **F67-2** [minor] `crates/c3/src/maintain/version.rs:102`, `crates/c3/src/maintain/version.rs:118` - Numeric prerelease identifiers larger than u64 can sort in reverse numerical order and make the updater skip a newer prerelease. Trigger: Two releases have equal core versions and numeric prerelease identifiers 18446744073709551616 and 100000000000000000000. Evidence: read-code: Both all-digit identifiers overflow the u64 parse and reach lexical comparison. The smaller integer's second digit is 8, so its string sorts later. Verify: RC2. Assert precedence and is_newer_than in both directions. Remedy: Recognize numeric identifiers independently of machine-integer conversion and compare significant digit counts followed by bytes.
- **F67-3** [minor] `crates/c3/src/contain.rs:1048`, `crates/c3/src/contain.rs:1860`, `crates/c3/src/contain.rs:2029`, `crates/c3/src/contain.rs:2081` - Exchange tests still unwrap success on systems where the new production contract requires refusal. Even the refusal test expects a subsequent first write to succeed unconditionally. Trigger: The test filesystem returns Unsupported for exchange, or the target uses the unconditional unsupported implementation. Evidence: read-code: Unsupported returns an error. The helper and direct test calls unwrap before any HAS_EXCHANGE assertion can distinguish that outcome. Verify: RC3 on a target using the unsupported implementation. Also run the exchange-preservation test filter there. Remedy: Branch test expectations on the actual operation outcome. Assert preservation and cleanup on Unsupported, and assert successful displacement only where the filesystem supports it.
- **F67-4** [minor] `CHANGELOG.md:901` - The changelog treats permission to replace one directory in a working tree as equivalent to all of the operator's filesystem rights. Trigger: A checkout has a group-writable records parent while the operator also has access to a private directory. Evidence: read-code: The entry accurately discloses subsequent pathname-based writes, then states that anyone able to swap a directory already has the operator's rights and gains nothing.; inferred: Write permission on a shared parent does not grant access to the operator's other directories. Verify: Review the sentence against a shared checkout whose operators have separate private home directories. Remedy: State the operating assumption directly: later path-based writes require trusted writers on the relevant directory parents. Keep the admission-only guarantee.

### Prior findings

- F02-1 - not-checked - Earlier repair containment.
- F02-2 - fixed - Staged publication preserves a displaced save or refuses unsupported exchange.
- F02-3 - not-checked - Earlier model repair.
- F02-4 - not-checked - Earlier model repair.
- F02-5 - not-checked - Earlier child environment.
- F02-6 - not-checked - Earlier hook timing.
- F02-7 - not-checked - Earlier branch metadata.
- F02-8 - not-checked - Earlier index handling.
- F02-9 - not-checked - Earlier branch reporting.
- F02-10 - not-checked - Earlier branch reporting.
- F02-11 - not-checked - Earlier worktree reporting.
- F02-12 - not-checked - Earlier redirect policy.
- F02-13 - not-checked - Earlier transport limits.
- F02-14 - not-checked - Earlier model dating.
- F02-15 - not-checked - Earlier endpoint binding.
- F02-16 - not-checked - Earlier smoke seat binding.
- F02-17 - not-checked - Earlier roster editing.
- F02-18 - not-checked - Earlier off switch.
- F02-19 - not-checked - Full earlier telemetry claim outside delta.
- F02-20 - not-checked - Earlier Claude token policy.
- F02-21 - not-checked - Earlier cleanup path.
- F03-1 - not-checked - Earlier child environment.
- F03-2 - not-checked - Earlier removal path.
- F03-3 - not-checked - Earlier branch metadata.
- F03-4 - not-checked - Earlier frontmatter parsing.
- F03-5 - not-checked - Earlier suffix normalization.
- F03-6 - not-checked - Earlier branch scanning.
- F04-1 - not-checked - Earlier index handling.
- F04-2 - not-checked - Earlier date gate.
- F04-3 - not-checked - Earlier date gate.
- F04-4 - not-checked - Earlier child guard.
- F04-5 - not-checked - Earlier removal path.
- F04-6 - not-checked - Earlier process environment.
- F04-7 - not-checked - Earlier branch classification.
- F04-8 - not-checked - Full transport and telemetry claim outside delta.
- F04-9 - not-checked - Earlier branch metadata.
- F10-1 - not-checked - Earlier agy classifier.
- F10-2 - not-checked - Earlier event merge.
- F10-3 - not-checked - Earlier brief staging.
- F10-4 - not-checked - Earlier panel findings.
- F10-5 - not-checked - Full switch-proof policy outside delta.
- F10-7 - not-checked - Earlier death records.
- F11-1 - not-checked - Earlier agy classifier.
- F11-2 - not-checked - Earlier input drift.
- F11-3 - not-checked - Earlier staging containment.
- F11-4 - not-checked - Earlier reserve selection.
- F11-5 - not-checked - Earlier prompt paths.
- F11-6 - not-checked - Earlier panel findings.
- F11-7 - not-checked - Full smoke-switch path outside delta.
- F11-8 - not-checked - Full rollout selection outside delta.
- F11-9 - not-checked - Earlier launch guard.
- F11-10 - not-checked - Earlier secondary prompts.
- F11-11 - not-checked - Earlier death records.
- F11-12 - not-checked - Earlier provider reporting.
- F12-1 - not-checked - Earlier liveness policy.
- F12-2 - not-checked - Earlier Git environment.
- F12-3 - not-checked - Earlier reparse handling.
- F12-4 - not-checked - Earlier branch writes.
- F13-1 - not-checked - Earlier cleanup path.
- F13-2 - not-checked - Earlier root discovery.
- F13-3 - not-checked - Earlier record parsing.
- F13-4 - not-checked - Earlier child environment.
- F13-5 - not-checked - Earlier hook deadline.
- F13-6 - not-checked - Earlier agent-model selection.
- F13-7 - not-checked - Earlier patch reporting.
- F13-8 - not-checked - Earlier owner records.
- F13-9 - not-checked - Earlier repair containment.
- F14-1 - not-checked - Earlier HTTP deadline.
- F14-2 - not-checked - Earlier refusal retention.
- F14-3 - not-checked - Earlier scratch roster.
- F14-4 - not-checked - Earlier quota protection.
- F14-5 - not-checked - Earlier date gate.
- F15-1 - not-checked - Earlier network policy.
- F15-2 - not-checked - Earlier refusal retention.
- F15-3 - not-checked - Earlier seat fingerprint.
- F15-5 - not-checked - Earlier recovery version selection.
- F15-6 - not-checked - Earlier off-switch filesystem effects.
- F16-1 - fixed - Exact catalog URL keys and the same-host private-first binary fixture cover the claim.
- F16-2 - not-checked - Earlier CI line.
- F16-3 - not-checked - Earlier process deadline.
- F16-4 - not-checked - Earlier CI branch selection.
- F16-5 - not-checked - Earlier worktree removal.
- F16-6 - not-checked - Earlier router explanation.
- F16-7 - not-checked - Earlier lab classification.
- F16-8 - not-checked - Earlier tier seating.
- F16-9 - not-checked - Earlier gate concurrency.
- F16-10 - not-checked - Earlier script parity.
- F17-1 - not-checked - Earlier CI output.
- F17-2 - not-checked - Earlier router explanation.
- F17-3 - not-checked - Earlier tier seating.
- F17-4 - not-checked - Earlier lab classification.
- F17-5 - fixed - Same-host catalog records retain separate URL-specific trust.
- F17-6 - not-checked - Earlier rule ownership.
- F17-7 - not-checked - Earlier CI branch selection.
- F17-8 - not-checked - Earlier remote recognition.
- F17-9 - not-checked - Earlier script parity.
- F17-10 - not-checked - Earlier CI wording.
- F18-1 - not-checked - Earlier revision bookkeeping.
- F18-2 - not-checked - Earlier index handling.
- F18-3 - not-checked - Earlier tree-check exclusions.
- F18-4 - not-checked - Earlier diff content policy.
- F18-5 - not-checked - Earlier staging links.
- F18-6 - not-checked - Earlier worktree links.
- F18-7 - not-checked - Earlier staging notice.
- F19-1 - not-checked - Earlier HTTP revision binding.
- F19-2 - not-checked - Earlier diff content policy.
- F19-3 - not-checked - Earlier patch publication.
- F19-4 - not-checked - Earlier owner recording.
- F19-5 - not-checked - Earlier child and MCP controls.
- F19-6 - not-checked - Earlier revision bookkeeping.
- F20-1 - not-checked - Earlier HTTP pack construction.
- F20-2 - not-checked - Earlier index handling.
- F20-3 - not-checked - Earlier patch publication.
- F20-4 - not-checked - Earlier worktree checks.
- F20-5 - not-checked - Earlier prompt paths.
- F20-6 - not-checked - Earlier diff content policy.
- F24-1 - not-checked - Earlier staging containment.
- F24-2 - not-checked - Earlier death records.
- F24-4 - not-checked - Earlier event merge.
- F27-1 - not-checked - Earlier address policy.
- F27-2 - not-checked - Earlier unlink walk.
- F27-3 - not-checked - Earlier tree-check exclusions.
- F28-1 - not-checked - Earlier patch publication.
- F28-2 - not-checked - Earlier ignored-path handling.
- F28-3 - not-checked - Earlier patch encoding.
- F29-1 - not-checked - Earlier transport policy.
- F29-2 - not-checked - Earlier staging reads.
- F29-3 - not-checked - Earlier findings binding.
- F30-1 - not-checked - Earlier router preflight.
- F30-2 - not-checked - Earlier tier seating.
- F30-3 - not-checked - Earlier gate concurrency.
- F30-4 - not-checked - Earlier draw replay.
- F30-5 - not-checked - Earlier remote recognition.
- F30-6 - not-checked - Earlier gate cleanup.
- F31-1 - not-checked - Earlier deadline reaping.
- F31-2 - not-checked - Earlier CI branch selection.
- F31-3 - not-checked - Earlier gate concurrency.
- F31-4 - not-checked - Earlier remote recognition.
- F31-5 - not-checked - Earlier gate cleanup.
- F31-6 - not-checked - Earlier router explanation.
- F32-1 - not-checked - Earlier staging containment.
- F32-2 - not-checked - Earlier per-turn smoke proof.
- F32-3 - not-checked - Earlier agy classification.
- F32-4 - not-checked - Earlier deadline reaping.
- F32-5 - not-checked - Earlier panel argument size.
- F36-1 - not-checked - Earlier migration concurrency.
- F36-2 - not-checked - Earlier migration recovery.
- F36-3 - not-checked - Earlier state links.
- F36-4 - not-checked - Earlier read-path migration.
- F36-5 - not-checked - Legacy spelling cases outside this resolver check.
- F36-6 - not-checked - Earlier migration liveness.
- F36-7 - not-checked - Earlier migration-off behavior.
- F36-8 - not-checked - Earlier diff exclusions.
- F38-1 - not-checked - Earlier CI cancellation.
- F38-2 - not-checked - Earlier address policy.
- F39-1 - not-checked - Earlier address policy.
- F39-2 - not-checked - Earlier price gate.
- F39-4 - not-checked - Earlier catalog aliases.
- F43-1 - not-checked - Earlier patch containment.
- F43-2 - not-checked - Earlier patch claims.
- F43-3 - not-checked - Earlier patch cleanup.
- F43-5 - not-checked - Earlier repair discovery.
- F43-6 - not-checked - Earlier removal errors.
- F43-7 - not-checked - Earlier cleanup guarantees.
- F43-8 - not-checked - Earlier staging compatibility.
- F44-1 - not-checked - Earlier patch claims.
- F44-2 - not-checked - Earlier staging compatibility.
- F44-3 - not-checked - Earlier FIFO handling.
- F45-1 - not-checked - Earlier redaction policy.
- F45-2 - not-checked - Earlier descendant reaping.
- F45-3 - not-checked - Earlier CI repository selection.
- F45-4 - not-checked - Earlier hook output.
- F45-5 - not-checked - Earlier timeout reporting.
- F45-6 - not-checked - Earlier finish-check reporting.
- F45-7 - not-checked - Earlier deadline waits.
- F45-8 - not-checked - Earlier branch closure.
- F45-9 - not-checked - Earlier finish deadline.
- F46-1 - not-checked - Earlier descendant reaping.
- F46-2 - not-checked - Earlier redaction policy.
- F46-3 - not-checked - Earlier branch closure.
- F46-4 - not-checked - Earlier CI repository selection.
- F46-5 - not-checked - Earlier root discovery.
- F46-6 - not-checked - Earlier finish-check reporting.
- F46-7 - not-checked - Earlier liveness policy.
- F46-8 - not-checked - Earlier hook output.
- F47-1 - not-checked - Earlier legacy anchor.
- F47-2 - not-checked - Earlier migration containment.
- F47-3 - not-checked - Earlier Windows spellings.
- F47-4 - not-checked - Earlier lock takeover.
- F48-1 - not-checked - Full state-link behavior outside delta.
- F48-2 - not-checked - Earlier migration concurrency.
- F48-3 - not-checked - Earlier legacy anchor.
- F48-4 - not-checked - Earlier Windows path aliases.
- F48-5 - not-checked - Earlier migration containment.
- F51-1 - not-checked - Earlier agy classification.
- F51-2 - not-checked - Earlier brief hashing.
- F51-3 - not-checked - Earlier patch claims.
- F51-4 - not-checked - Earlier input drift.
- F51-5 - not-checked - Earlier MCP diff options.
- F51-6 - not-checked - Earlier death records.
- F52-1 - not-checked - Earlier agy classification.
- F52-2 - not-checked - Earlier patch claims.
- F52-4 - not-checked - Earlier death records.
- F53-1 - not-checked - Earlier isolation prompt.
- F53-2 - not-checked - Earlier agy classification.
- F53-3 - not-checked - Earlier findings snapshot.
- F53-4 - not-checked - Earlier crash cleanup.
- F53-5 - not-checked - Earlier filename display.
- F53-6 - not-checked - Earlier patch claims.
- F53-7 - not-checked - Earlier MCP diff disclosure.
- F57-1 - fixed - Pages retain collection URL and trust. Current trust is checked by that URL.
- F57-2 - fixed - Malformed nested tools yield Unproven in the inventory fixture.
- F57-3 - fixed - Pending recovery verifies the measured release again after the smoke. Finding #1 is outside recovery.
- F57-4 - not-checked - Earlier migration lock.
- F57-5 - not-checked - Verified digest retention in names only.
- F57-6 - fixed - Admission addresses the reported pre-existing linked task. Later pathname writes remain a documented limit.
- F57-7 - still-open - Unix Git checkout still takes a pathname before the identity recheck. Accepted under the narrowed operating assumption.
- F57-8 - not-checked - Earlier prompt display.
- F57-9 - not-checked - Earlier transport binding.
- F57-10 - fixed - Unsupported exchange refuses. The supported path retains a displaced edit.
- F60-1 - not-checked - Full static-link entry-point matrix outside delta.
- F60-2 - not-checked - Windows alias cases outside delta.
- F60-3 - not-checked - Earlier test wording.
- F61-1 - not-checked - Full static-link matrix outside delta.
- F61-3 - fixed - Work integration fixture checks actual records inside and an empty outside directory.
- F62-1 - fixed - Pending recovery distinguishes prerelease and final in both directions.
- F62-2 - fixed - Shared resolver applies lexical normalization before canonicalization.
- F62-3 - fixed - Current trust and collection-time trust are both required for custom catalogs.
- F62-4 - fixed - Hard-link claims protect occupied numbered names. The documented random-name fallback removes the deterministic collision.
- F62-5 - fixed - Nested namespaces retain collaboration ancestry.
- F62-6 - fixed - Bounded readable stem leaves room for the full digest and suffix.
- F65-1 - fixed - README and referenced comments now describe refusal. The remaining search hit concerns a different rename property.
- F65-2 - fixed - A readable unexpected release reaches the recovery smoke in the same pass.
- F65-3 - fixed - A consult dry-run now exercises the shared resolver. Panel and detach still lack equivalent end-to-end coverage.
- F65-4 - fixed - Fixtures cover target mismatch, same-number rollback mismatch and exact prerelease date.
- F65-5 - fixed - The nested-namespace test has its own matching comment.
- F65-6 - fixed - A binary telemetry fixture uses an exact built-in legacy record without url or public.

## Verdict: REJECT

An ordinary successful update can credit the target and clear its pending record even when a different CLI release ran the smoke.

### Blockers

- **F67-1** `crates/c3/src/maintain/mod.rs:1052`, `crates/c3/src/maintain/mod.rs:1116`, `crates/c3/src/maintain/mod.rs:1157`, `crates/c3/src/maintain/mod.rs:999` - The ordinary target-install path can record a passing smoke against the wrong release and discard the only pending recovery record. Verify: RC1. Compare the fake launcher's actual smoke-time version, the state action and the pending journal in the same run. Remedy: Apply the recovery path's final exact-identity check to the ordinary target success path. A changed or unreadable release should retain the pending record and receive no success attribution.
- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, fixed) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, not-checked) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, not-checked) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, not-checked) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.
- **F19-1** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision.
- **F32-1** (prior, not-checked) `crates/c3/src/engines/worktree.rs:248` - Stage creation and publication use pathnames after validation, so a replaced directory component can send staged bytes outside the worktree and leave a temporary file there.
- **F57-1** (prior, fixed) `crates/c3/src/maintain/web.rs:209` - Changing a page's URL and public flag while retaining its name can publish IDs from the previously non-public page in telemetry.
- **F57-2** (prior, fixed) `crates/c3-core/src/quota.rs:941` - An uninterpretable nested tool inventory counts as positive switch evidence for a Codex smoke turn.
- **F57-3** (prior, fixed) `crates/c3/src/maintain/mod.rs:740` - Pending recovery can record the pending target version as verified after smoking a different installed version.
- **F57-4** (prior, not-checked) `crates/c3/src/state_dir.rs:745` - A live lock creator can return success after another process reclaimed its still-empty file and acquired the same move lock.
- **F57-5** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:1451` - Two different patches can publish successfully at one short-hash path, leaving one consultation's full digest bound to the other consultation's bytes.
- **F57-6** (prior, fixed) `crates/c3/src/state_dir.rs:1544` - A linked task directory below a real project collab directory can redirect handoff directory creation and consultation output.
- **F62-1** (prior, fixed) `crates/c3/src/maintain/version.rs:79` - Pending recovery can mark a final CLI release verified when both installed-version probes and the smoke used only a prerelease.
- **F62-2** (prior, fixed) `crates/c3/src/state_dir.rs:1883` - Consult, panel and detach can classify a repository-relative collab argument as an external anchor because the link check and canonicalization interpret `..` differently.
- **F62-3** (prior, fixed) `crates/c3/src/maintain/web.rs:133` - Declaring a formerly private catalog URL public can publish cached IDs that appeared only in the earlier private response.
- **F62-4** (prior, fixed) `crates/c3/src/contain.rs:817` - The operation that preserves a displaced edit can overwrite an earlier displaced edit and report the same recovery path for both.
- **F62-5** (prior, fixed) `crates/c3-core/src/quota.rs:956` - A nested namespace can erase its enclosing collaboration namespace during inventory traversal, causing an offered tool to count as clean switch evidence.

### Unproven scenarios

- No independent test execution or exact-head CI result was available in this read-only review. The brief reports Windows test results and green earlier three-job runs.
- The ordinary-update version change in finding #1 was established from control flow. RC1 has not been executed.
- Real maintenance updates, rollback, vendor version spellings, and filesystem behavior without exchange or hard-link support were not exercised here.
- Panel and detached consult were not run end to end with the link/../collab spelling. The reviewed displaced-name test uses successive placements, not synchronized processes.
- The inherited live agy, Kimi resume, state migration, macOS and intermittent Windows launcher scenarios were not revalidated.

### First-run checklist (observable)

- [ ] The CI run identifies the exact proposed tag SHA and records completed green Windows, Linux and Linux index-surreal jobs.
- [ ] For an actual update, capture the target and the launcher's exact release before and after both smoke turns. Both readings and the state action must agree before accepting a target success line. Drift must leave the pending record present.
- [ ] If the updater leaves another release, the same pass names that release, records its own smoke outcome and withholds target credit. A rollback line agrees with a fresh version probe including prerelease and build suffixes.
- [ ] For a Codex smoke, both turns have terminal records and interpretable switch evidence. An offered collaboration tool at any depth prevents a pass.
- [ ] For a catalog refresh, the stored record identifies its canonical URL and collection-time trust. A previously private synthetic ID remains absent from the serialized telemetry payload after reclassification.
- [ ] For a collab run using a normalized path, the reported task path and actual records agree and the outside fixture directory remains empty. For an exchange refusal, the report names the refusal and the operator's saved bytes remain intact.
- [ ] For a displaced edit, each reported recovery pathname opens to the expected distinct bytes. For a long-stem diff, the reported full digest matches the published patch.
