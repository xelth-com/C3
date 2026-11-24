# Handoff 62 - Codex: acceptance-delta

Date: 2026-11-24 19:27 local. Author: Codex (model gpt-6.2-astra, effort high), Codex CLI 0.169.0.
Reviewer: openai :: gpt-6.2-astra (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.169.0).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - position 1 of 17.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 91abb49a-b61d-4a53-8449-099c37667fe1.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6.2-astra -c features.multi_agent=false -c agents.enabled=false -c features.plugins=false -c skills.include_instructions=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-299f08b3b41e4144b1a319eeaa864080.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-df1ad10cd9a44390b5a1d23539b31c65` of revision ea30cce0edb45d79bc6efde5220103b75ab351eb (--rev HEAD; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20ec5-d31f-7d42-af6f-70e1bfe422a0` (source: events).
Brief: `.collab/release-0.2.5/handoffs/58-claude-acceptance-delta.md` (sha256 dec0690c4004). Reviewed: ea30cce, base ea30cce0edb45d79bc6efde5220103b75ab351eb, tree sha256 fe74dae6e567, 0 changed files. Live HEAD at launch: ea30cce0edb45d79bc6efde5220103b75ab351eb.
Bridge outcome: usable reply. Wall time: 690.5 s. Tokens: in 1965255 (cached 1852160), out 13824, reasoning 938.
Quota (rollout): 5h 20% until 00:16; wk 86% until Nov 29; this run 1965255 input tokens (1852160 cached) in 22 calls, max 115972 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false, agents.enabled=false); budget 90 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s.
Verdict: REJECT - Recovery can credit a prerelease smoke to a final release, catalog trust can expose an ID collected while private, and an in-repository collab spelling can still direct records elsewhere. Findings: 5 blocker, 0 major, 1 minor, 0 note (F62-1..F62-6, tracked in `findings.json`). Structured reply: `handoffs/62-codex-acceptance-delta.reply.json`.
Raw event stream: `handoffs/62-codex-acceptance-delta.events.jsonl`.
Verbatim reply follows.

---

1. **Closure against handoff 58.**

   | Row | Decision at `ea30cce` |
   |---|---|
   | F57-1 | **Closed for pages.** Publication requires the collected public flag and the recorded canonical URL. Older label-only records are withheld. Catalogs have a separate defect in finding #3. |
   | F57-2 | **Closed for unreadable inventories.** Unknown kinds and malformed nested lists hold the smoke. A readable nested namespace can still produce a false pass, finding #5 (`quota.rs:1111`). |
   | F57-3 | **Not closed.** Distinct numeric versions and unreadable versions are handled, but a prerelease and its final release compare equal (`maintain/version.rs:79`, `maintain/mod.rs:1222`). |
   | F57-4 | **Closed** for cooperating local processes. The creator takes the gate before exclusive creation and holds it through record publication (`state_dir.rs:867`). |
   | Answer 2, switch-off writer | **Closed** for the reported split. Creation beside a real `.eck` takes the move lock and handles a folder created by the winner (`state_dir.rs:560`). |
   | F57-5 | **Closed for competing C3 placements.** Full digests separate content and the fallback returns the run-specific path. Its UUID is checked before a replacing rename, so exclusivity against an arbitrary writer is still an assumption (`consult/reviewed.rs:1626`). |
   | F57-6 | **Not fully closed.** Ordinary pre-existing linked task and handoff directories are refused. The directory handles expire at admission (`state_dir.rs:1706`), and finding #2 identifies a resolver mismatch before admission. |
   | F57-7 | **Not fully closed.** Temp-parent identity and occupied-path refusal are fixed. On Unix, Git still receives a pathname before the identity recheck (`engines/worktree.rs:485,497`). A replacement empty directory can receive checkout files before refusal. The race test puts `keep.txt` in the replacement (`:2998`). |
   | F57-8 | **Closed.** The prompt filter rejects Unicode format characters (`engines/worktree.rs:1487`). |
   | F57-9 | **Closed for the reported hostname issue.** Plain HTTP requires a loopback IP literal in test mode (`roster_ext.rs:162`). |
   | F57-10 / F02-2 | **Not fully closed.** Supported exchange preserves the usual racing save. Unsupported exchange still replaces by rename and reports the limitation afterward (`contain.rs:730`). Even on supported systems the displaced copy can collide, finding #4. |

2. **The delta still has release blockers.** Findings #1–5 identify false verification, misplaced records, stale private telemetry data, loss of a displaced copy, and false switch evidence. The full patch digest fixes the original collision. It also makes some previously valid brief names too long for patch creation, finding #6. Copying the live index's modification time from the same open handle as its bytes addresses the reported empty-diff cause.

3. **The external collab root is a reasonable explicit operator choice.** Treating an in-repository spelling as that external choice is a blocker. So are later path-based writes when directory identity is part of the containment promise, and automatic rename fallback when preservation of hand edits is required. The hosted-tool list, delayed page publication, per-run patch copies, and lack of distributed locking would not independently stop this tag under the stated operating assumptions.

4. **REJECT**, even assuming green three-job CI on the tag candidate. The evidence names green runs for `fe877b2` and `431b610`. The state log last describes CI for `ea30cce` as running. The intervening code/workflow diff contains only the manual flake workflow. I verified HEAD and reviewed selected delta hunks, prior handoffs, release notes and smoke records. I did not run tests, inspect remote CI, read every hunk or log, or re-audit unrelated earlier findings. No files changed.

## Requested checks

- **RC1** — Repository root, workspace-write, one recovery fixture. Repeat the third-version test with installed `1.0.80-rc.1` and target `1.0.80`. Inspect the action and pending record for false final-release credit.
- **RC2** — Disposable Unix repository, workspace-write, one resolver test. Compare the admitted directory identity for a link followed by `..` across consult, panel, detach and work. Require common lexical resolution or refusal before record creation.
- **RC3** — Repository root, workspace-write, one offline telemetry fixture. Reclassify a catalog URL after removing a synthetic private ID from its contents, retaining the old cache. The serialized model must remain anonymized until refresh.
- **RC4** — Repository root, workspace-write, one exchange fixture. Synchronize two displaced-copy placements after name selection. Compare both retained byte sequences and the paths reported to their callers.
- **RC5** — Repository root, workspace-write, one quota parser fixture. Put a child namespace with an unfamiliar function inside `collaboration`. The completed turn must yield breach or unproven.

---

### Findings

- **F62-1** [blocker] `crates/c3/src/maintain/version.rs:79`, `crates/c3/src/maintain/engine.rs:92`, `crates/c3/src/maintain/mod.rs:1222`, `crates/c3/src/maintain/mod.rs:1245`, `crates/c3-cli/tests/maintain.rs:793` - Pending recovery can mark a final CLI release verified when both installed-version probes and the smoke used only a prerelease. Trigger: The installed version and pending target share a numeric prefix but have different release identities, such as 1.0.80-rc.1 and 1.0.80. Evidence: read-code: Installed-version parsing discards the suffix from both the numeric parts and the retained text.; read-code: Both recovery comparisons use parts. The passing third-version fixture distinguishes versions by their numeric components. Verify: RC1. Require the state to identify the installed prerelease and withhold verification of the pending final release. Remedy: Keep an exact release identity for attribution and pre/post-smoke comparisons. Use numeric parts separately for upgrade ordering. Supersedes: F57-3.
- **F62-2** [blocker] `crates/c3/src/state_dir.rs:1883`, `crates/c3/src/state_dir.rs:2125`, `crates/c3/src/providers.rs:2572`, `crates/c3/src/state_dir.rs:1797`, `crates/c3/src/work/mod.rs:204` - Consult, panel and detach can classify a repository-relative collab argument as an external anchor because the link check and canonicalization interpret `..` differently. Trigger: An ordinary collab path contains a linked directory followed by `..`. The lexically collapsed chain is clear and the physically resolved collab directory exists outside the repository. Evidence: read-code: The link check first collapses parent components. For a non-state path the resolver retains the original spelling and canonicalizes that spelling after a clear check.; read-code: The external branch opens the resolved root as an ambient anchor. Work explicitly normalizes the path it subsequently uses. Verify: RC2. Compare identities of the checked directory, the admitted directory and the record parent for each entry point. Remedy: Use one normalized path from resolution through admission and publication, or reject ambiguous link/parent traversal before choosing an external anchor.
- **F62-3** [blocker] `crates/c3/src/maintain/web.rs:133`, `crates/c3/src/maintain/web.rs:244`, `crates/c3/src/maintain/web.rs:284`, `crates/c3/src/maintain/models.rs:1043` - Declaring a formerly private catalog URL public can publish cached IDs that appeared only in the earlier private response. Trigger: A catalog is cleaned of private IDs and then declared public at the same URL. Telemetry reads its older cache before the next successful maintenance refresh. Evidence: read-code: Catalog provenance returns the URL key without testing collection-time public status. Only page collection records that status.; read-code: Matching current trust to the old key admits cached vendor-prefixed model IDs to the published set. Verify: RC3. Compare serialized model fields before reclassification, immediately after it, and after refreshing the sanitized catalog. Remedy: Persist collection-time public eligibility for catalogs as well as pages. Require both that eligibility and current exact-URL trust, and withhold legacy custom catalog records until refreshed. Supersedes: F02-19.
- **F62-4** [blocker] `crates/c3/src/contain.rs:817`, `crates/c3/src/contain.rs:834`, `crates/c3/src/contain.rs:838` - The operation that preserves a displaced edit can overwrite an earlier displaced edit and report the same recovery path for both. Trigger: Two publications for the same leaf select the same free millisecond-based backup name before either rename, or the bounded suffix search reaches an occupied final candidate. Evidence: read-code: Name selection checks occupancy separately from a replacing rename. The loop also returns suffix 1000 without requiring it to be free. Verify: RC4. Require two distinct recovery paths with the exact bytes each exchange displaced. Remedy: Use atomic no-replace publication for the displaced copy. On collision retry, or retain and report the already unique exchange temporary name.
- **F62-5** [blocker] `crates/c3-core/src/quota.rs:956`, `crates/c3-core/src/quota.rs:1111`, `crates/c3-core/src/quota.rs:1121` - A nested namespace can erase its enclosing collaboration namespace during inventory traversal, causing an offered tool to count as clean switch evidence. Trigger: A recognized `collaboration` namespace contains another named namespace with a function whose leaf name is absent from the fixed multi-agent tool list. Evidence: read-code: The classifier identifies any tool under collaboration, but recursion passes only the immediate child namespace name. This fully readable tree produces neither an offered tool nor a parsing problem. Verify: RC5. Assert the enclosing namespace remains decisive at two or more levels. Remedy: Propagate collaboration ancestry during recursion, or classify unsupported namespace nesting as unproven.
- **F62-6** [minor] `crates/c3/src/consult/reviewed.rs:766`, `crates/c3/src/consult/reviewed.rs:1540`, `crates/c3/src/consult/reviewed.rs:1655`, `crates/c3/src/consult/reviewed.rs:1835` - The longer digest makes patch creation fail for valid handoff brief names that worked with the previous patch format. Trigger: On a filesystem with a 255-byte filename limit, a recognized 160-byte ASCII brief stem produces a 262-byte temporary filename. Evidence: read-code: The entire brief stem is retained. Temporary placement adds two separators, 64 digest characters, 32 UUID characters and the four-character .tmp extension. Verify: Run the patch placement fixture with a recognized 160-byte stem on a filesystem with a 255-byte component limit. Remedy: Bound the human-readable stem before constructing any final or temporary name. Preserve the full digest and truncate at a character boundary.

### Prior findings

- F02-1 - not-checked - Did not re-audit repair containment on both platforms.
- F02-2 - still-open - Staging prevents truncation. The unsupported-exchange fallback still loses a save at contain.rs:730.
- F02-3 - not-checked - Model rewrite selection outside delta.
- F02-4 - not-checked - Provider selection outside delta.
- F02-5 - not-checked - Child launcher environment outside delta.
- F02-6 - not-checked - Hook timing outside delta.
- F02-7 - not-checked - Branch descriptions outside delta.
- F02-8 - not-checked - Branch and doctor index use outside delta.
- F02-9 - not-checked - Branch classification outside delta.
- F02-10 - not-checked - Patch equivalence outside delta.
- F02-11 - not-checked - Branch listing outside delta.
- F02-12 - not-checked - Redirect policy outside delta.
- F02-13 - not-checked - Transport limits outside delta.
- F02-14 - not-checked - Catalog date rules outside delta.
- F02-15 - not-checked - Endpoint list partitioning outside delta.
- F02-16 - not-checked - Smoke entry binding not re-audited.
- F02-17 - not-checked - JSON field selection not re-audited.
- F02-18 - not-checked - Off-switch ordering not re-audited.
- F02-19 - still-open - Finding #3 narrows the remaining exposure to stale catalog IDs after reclassification.
- F02-20 - not-checked - Claude token allowlist not re-audited.
- F02-21 - not-checked - Removal walker not re-audited.
- F03-1 - not-checked - Claude environment outside delta.
- F03-2 - not-checked - Removal walker outside this review.
- F03-3 - not-checked - Branch descriptions outside delta.
- F03-4 - not-checked - Frontmatter formatting outside delta.
- F03-5 - not-checked - Suffix normalization outside delta.
- F03-6 - not-checked - Branch listing outside delta.
- F04-1 - not-checked - Doctor and branches outside delta.
- F04-2 - not-checked - Swap ordering not re-audited.
- F04-3 - not-checked - Catalog timestamp trust not re-audited.
- F04-4 - not-checked - Doctor child guard not re-audited.
- F04-5 - not-checked - Forced removal not re-audited.
- F04-6 - not-checked - Launcher environments outside delta.
- F04-7 - not-checked - Branch classification outside delta.
- F04-8 - not-checked - Transport and token rules not re-audited.
- F04-9 - not-checked - Branch descriptions outside delta.
- F10-1 - not-checked - Agy classifier outside delta.
- F10-2 - not-checked - Agy event merging outside delta.
- F10-3 - not-checked - Brief staging and reseating not re-audited.
- F10-4 - not-checked - Findings snapshot outside delta.
- F10-5 - not-checked - Known MCP leaf names are recognized. Opt-in seat selection was not retraced.
- F10-7 - not-checked - Death records outside delta.
- F11-1 - not-checked - Agy classification outside delta.
- F11-2 - not-checked - Bound input drift outside delta.
- F11-3 - not-checked - Staging destination not re-audited.
- F11-4 - not-checked - Seat preflight not re-audited.
- F11-5 - not-checked - Checked the format-character fix only.
- F11-6 - not-checked - Panel snapshots outside delta.
- F11-7 - not-checked - Opt-in and missing-thread cases not retraced.
- F11-8 - not-checked - Malformed nested arrays are fixed. Did not recheck all identity, path and partial-record scenarios.
- F11-9 - not-checked - Subprocess polling outside delta.
- F11-10 - not-checked - Secondary prompts outside delta.
- F11-11 - not-checked - Death records outside delta.
- F11-12 - not-checked - Provider breakdown outside delta.
- F12-1 - not-checked - Process liveness not re-audited.
- F12-2 - not-checked - Root-resolution environment not re-audited.
- F12-3 - not-checked - Cloud placeholder behavior not exercised.
- F12-4 - not-checked - Branch config publication outside delta.
- F13-1 - not-checked - Stale worktree removal not re-audited.
- F13-2 - not-checked - Hook root detection outside delta.
- F13-3 - not-checked - Branch record parser outside delta.
- F13-4 - not-checked - Update and branch child guards outside delta.
- F13-5 - not-checked - Branch-hook deadline outside delta.
- F13-6 - not-checked - AGENTS.md version selection outside delta.
- F13-7 - not-checked - Git cherry reporting outside delta.
- F13-8 - fixed - worktree.rs:516 treats an owner-record write failure as creation failure.
- F13-9 - not-checked - Did not establish Windows ancestor identity through exchange.
- F14-1 - not-checked - HTTP deadlines outside delta.
- F14-2 - not-checked - Older-pair storage outside delta.
- F14-3 - not-checked - Smoke roster creation not re-audited.
- F14-4 - not-checked - Quota protection parsing outside delta.
- F14-5 - not-checked - Future catalog dates outside delta.
- F15-1 - not-checked - Peer address checks outside delta.
- F15-2 - not-checked - Older-pair persistence outside delta.
- F15-3 - not-checked - Entry fingerprint not re-audited.
- F15-5 - not-checked - Post-rollback update selection not retraced.
- F15-6 - not-checked - Off-mode writes outside delta.
- F16-1 - fixed - Catalog keys include the canonical URL. Finding #3 concerns reuse of one URL over time.
- F16-2 - not-checked - CI output outside delta.
- F16-3 - not-checked - CI deadlines outside delta.
- F16-4 - not-checked - CI branch/run selection outside delta.
- F16-5 - not-checked - Removal walker not retraced.
- F16-6 - not-checked - Router replay outside delta.
- F16-7 - not-checked - Lab identity outside delta.
- F16-8 - not-checked - Tier guarantee outside delta.
- F16-9 - not-checked - CI gate outside delta.
- F16-10 - not-checked - Hook parity outside delta.
- F17-1 - not-checked - Workflow title handling outside delta.
- F17-2 - not-checked - Router explain outside delta.
- F17-3 - not-checked - Tier guarantee outside delta.
- F17-4 - not-checked - Vendor labs outside delta.
- F17-5 - fixed - Catalog fetch and trust use keys that distinguish paths on the same host.
- F17-6 - not-checked - Rule ownership not re-audited.
- F17-7 - not-checked - CI branch selection outside delta.
- F17-8 - not-checked - Remote host recognition outside delta.
- F17-9 - not-checked - Hook parity outside delta.
- F17-10 - not-checked - Run status wording outside delta.
- F18-1 - not-checked - Ledger revision binding not retraced.
- F18-2 - fixed - worktree.rs:450 passes --no-optional-locks to live status.
- F18-3 - not-checked - Collab exemption not re-audited end to end.
- F18-4 - not-checked - Untracked defaults not retraced.
- F18-5 - not-checked - Source staging not re-audited.
- F18-6 - not-checked - Outside-link sweep not re-audited.
- F18-7 - not-checked - Prompt staging claims not re-audited.
- F19-1 - not-checked - HTTP revision handling outside delta.
- F19-2 - not-checked - Untracked defaults not retraced.
- F19-3 - fixed - Full digests distinguish different rendered bytes and the fallback returns its chosen path.
- F19-4 - fixed - Owner-record failure aborts creation at worktree.rs:516.
- F19-5 - not-checked - Child and MCP boundary not re-audited.
- F19-6 - not-checked - Ledger revision binding outside delta.
- F20-1 - not-checked - HTTP patch packing outside delta.
- F20-2 - fixed - Live status disables optional locks at worktree.rs:450.
- F20-3 - fixed - Competing ranges no longer replace a shared stem-only patch.
- F20-4 - not-checked - Ignored-file detection not re-audited.
- F20-5 - not-checked - Format filtering checked. Selection of every displayed name was outside scope.
- F20-6 - not-checked - Untracked defaults not retraced.
- F24-1 - not-checked - Staging publication not re-audited.
- F24-2 - not-checked - Death-stream choice outside delta.
- F24-4 - not-checked - Agy parameter merge outside delta.
- F27-1 - not-checked - Address table outside delta.
- F27-2 - not-checked - Link sweep not re-audited.
- F27-3 - not-checked - Tree-check exemption not re-audited.
- F28-1 - fixed - The temporary patch is synced before publication. A placement failure attempts cleanup and reports cleanup failure.
- F28-2 - not-checked - Diff ignore cap not retraced.
- F28-3 - not-checked - Patch byte encoding not retraced.
- F29-1 - not-checked - Transport unchanged. Brief 56 retains the stalled-DNS limitation.
- F29-2 - not-checked - Source staging not re-audited.
- F29-3 - not-checked - Prior revision selection not retraced.
- F30-1 - not-checked - Router preflight outside delta.
- F30-2 - not-checked - Tier warning outside delta.
- F30-3 - not-checked - CI concurrency outside delta.
- F30-4 - not-checked - Router replay outside delta.
- F30-5 - not-checked - GitHub aliases outside delta.
- F30-6 - not-checked - CI temp cleanup outside delta.
- F31-1 - not-checked - Deadline cleanup outside delta.
- F31-2 - not-checked - CI branch discovery outside delta.
- F31-3 - not-checked - CI daily gate outside delta.
- F31-4 - not-checked - SSH remote recognition outside delta.
- F31-5 - not-checked - CI gate cleanup outside delta.
- F31-6 - not-checked - Router explain outside delta.
- F32-1 - not-checked - Staging containment not retraced.
- F32-2 - not-checked - Per-turn proven check exists. Turn-boundary pairing was not retraced.
- F32-3 - not-checked - Agy path handling outside delta.
- F32-4 - not-checked - Branch process registry outside delta.
- F32-5 - not-checked - Panel launch argument size outside delta.
- F36-1 - not-checked - Checked switch-off creation. Did not re-audit every losing writer.
- F36-2 - not-checked - Interrupted migration repair not retraced.
- F36-3 - not-checked - Migration of linked legacy descendants not retraced.
- F36-4 - not-checked - Read-command migration outside delta.
- F36-5 - not-checked - Legacy spelling remap not fully retraced.
- F36-6 - not-checked - Live-process discovery not retraced.
- F36-7 - not-checked - Legacy explicit collab policy not re-audited.
- F36-8 - not-checked - Nested diff exclusions outside delta.
- F38-1 - not-checked - CI cancellation outside delta.
- F38-2 - not-checked - IPv6 address table outside delta.
- F39-1 - not-checked - IPv6 address table outside delta.
- F39-2 - not-checked - Catalog price comparison outside delta.
- F39-4 - not-checked - Catalog alias selection outside delta.
- F43-1 - not-checked - Patch parent opening not fully retraced.
- F43-2 - fixed - No empty shared claim in the fallback.
- F43-3 - fixed - Fallback cleanup uses the placement's own temporary name.
- F43-5 - not-checked - Repair discovery not re-audited.
- F43-6 - not-checked - Failed recursive cleanup not retraced.
- F43-7 - not-checked - Historical documentation claims not fully re-audited.
- F43-8 - not-checked - Cloud placeholder and staging cases not exercised.
- F44-1 - fixed - No-hard-link placement no longer reserves the shared digest with an empty final file.
- F44-2 - not-checked - Inside-link staging documentation outside delta.
- F44-3 - not-checked - FIFO handling not retraced.
- F45-1 - not-checked - Secret scanning outside delta.
- F45-2 - not-checked - Daemon cleanup not re-audited.
- F45-3 - not-checked - CI remote selection outside delta.
- F45-4 - not-checked - Hook fallback outside delta.
- F45-5 - not-checked - Check timeout output outside delta.
- F45-6 - not-checked - Skip-checks reporting outside delta.
- F45-7 - not-checked - Child termination outside delta.
- F45-8 - not-checked - Branch close outside delta.
- F45-9 - not-checked - Finish time limits outside delta.
- F46-1 - not-checked - Process reaping not re-audited.
- F46-2 - not-checked - Redaction outside delta.
- F46-3 - not-checked - Branch close outside delta.
- F46-4 - not-checked - CI repository selection outside delta.
- F46-5 - not-checked - Git directory identification not retraced.
- F46-6 - not-checked - Skip-checks reporting outside delta.
- F46-7 - not-checked - Process identity outside delta.
- F46-8 - not-checked - Hook line truncation outside delta.
- F47-1 - not-checked - Legacy anchor repair not retraced.
- F47-2 - not-checked - Migration and every write path not retraced.
- F47-3 - not-checked - Windows trailing-component behavior not exercised.
- F47-4 - not-checked - Lock stale-age documentation not re-audited.
- F48-1 - not-checked - Ordinary linked root refusal checked. Historical read paths not re-audited.
- F48-2 - not-checked - All fallback writers and takeover cases not re-audited.
- F48-3 - not-checked - Interrupted legacy anchor repair not retraced.
- F48-4 - not-checked - All alias and legacy remap cases not exercised.
- F48-5 - not-checked - Legacy walk and hard-linked state writes not retraced.
- F51-1 - not-checked - Agy classifier outside delta.
- F51-2 - not-checked - Staged brief hash binding outside delta.
- F51-3 - fixed - The shared claim and its takeover are gone.
- F51-4 - not-checked - Drift handling outside delta.
- F51-5 - not-checked - MCP patch arguments not retraced.
- F51-6 - not-checked - Death-record choice outside delta.
- F52-1 - not-checked - Agy command classification outside delta.
- F52-2 - fixed - Placement does not take over another run's empty claim.
- F52-4 - not-checked - Death record timestamps outside delta.
- F53-1 - not-checked - Clean-tree prompt entry point not retraced.
- F53-2 - not-checked - Agy reader classification outside delta.
- F53-3 - not-checked - Findings-store read failure outside delta.
- F53-4 - not-checked - Did not verify cleanup of older-format debris or temporary indexes.
- F53-5 - not-checked - Reviewed the isolation prompt, not all ledger and stderr paths.
- F53-6 - fixed - Placement no longer removes a shared final claim on rename failure.
- F53-7 - not-checked - Detached and MCP disclosure not re-audited.
- F57-1 - fixed - A page record needs its collected public flag, canonical URL and current trust.
- F57-2 - fixed - Malformed inventory elements make the turn unproven. Finding #5 concerns a fully parsed inventory.
- F57-3 - still-open - Finding #1 narrows the remaining failure to distinct release identities with equal numeric parts.
- F57-4 - fixed - Gate acquisition precedes exclusive create and protects record publication.
- F57-5 - fixed - Different payloads use full digests. Cooperating fallback placements publish separate UUID paths.
- F57-6 - still-open - Pre-existing task links are refused. run_dirs returns unit and RunStart retains only the move lock, so later writes lack the requested directory binding.
- F57-7 - still-open - The check after Git cannot prevent an earlier checkout into a substituted empty directory. At worktree.rs:314–316 failure cleanup also drops the identity handles before removing by name.
- F57-8 - fixed - prompt_safe_path rejects is_format_char at worktree.rs:1487.
- F57-9 - fixed - The test-mode HTTP rule accepts parsed loopback literals. A hostname no longer passes.
- F57-10 - still-open - Ordinary supported exchange preserves the racing save. The fallback expressly permits losing it. Finding #4 is a distinct backup-name defect.
- F60-1 - fixed - The ordinary linked collab root is preserved by the resolver and refused by admission. Finding #2 concerns a different spelling.
- F60-2 - fixed - named_below reconstructs the suffix from an alias of the repo root. The reported tests cover the verbatim Windows and above-root alias cases.
- F60-3 - fixed - The CHANGELOG test clause describes linked task directories as refused.
- F61-1 - fixed - The consulted entry points now preserve an ordinary pre-existing linked root for refusal.
- F61-3 - fixed - Work normalizes the path before both admission and subsequent use at work/mod.rs:204. Finding #2 concerns the other entry points.

## Verdict: REJECT

Recovery can credit a prerelease smoke to a final release, catalog trust can expose an ID collected while private, and an in-repository collab spelling can still direct records elsewhere.

### Blockers

- **F62-1** `crates/c3/src/maintain/version.rs:79`, `crates/c3/src/maintain/engine.rs:92`, `crates/c3/src/maintain/mod.rs:1222`, `crates/c3/src/maintain/mod.rs:1245`, `crates/c3-cli/tests/maintain.rs:793` - Pending recovery can mark a final CLI release verified when both installed-version probes and the smoke used only a prerelease. Verify: RC1. Require the state to identify the installed prerelease and withhold verification of the pending final release. Remedy: Keep an exact release identity for attribution and pre/post-smoke comparisons. Use numeric parts separately for upgrade ordering.
- **F62-2** `crates/c3/src/state_dir.rs:1883`, `crates/c3/src/state_dir.rs:2125`, `crates/c3/src/providers.rs:2572`, `crates/c3/src/state_dir.rs:1797`, `crates/c3/src/work/mod.rs:204` - Consult, panel and detach can classify a repository-relative collab argument as an external anchor because the link check and canonicalization interpret `..` differently. Verify: RC2. Compare identities of the checked directory, the admitted directory and the record parent for each entry point. Remedy: Use one normalized path from resolution through admission and publication, or reject ambiguous link/parent traversal before choosing an external anchor.
- **F62-3** `crates/c3/src/maintain/web.rs:133`, `crates/c3/src/maintain/web.rs:244`, `crates/c3/src/maintain/web.rs:284`, `crates/c3/src/maintain/models.rs:1043` - Declaring a formerly private catalog URL public can publish cached IDs that appeared only in the earlier private response. Verify: RC3. Compare serialized model fields before reclassification, immediately after it, and after refreshing the sanitized catalog. Remedy: Persist collection-time public eligibility for catalogs as well as pages. Require both that eligibility and current exact-URL trust, and withhold legacy custom catalog records until refreshed.
- **F62-4** `crates/c3/src/contain.rs:817`, `crates/c3/src/contain.rs:834`, `crates/c3/src/contain.rs:838` - The operation that preserves a displaced edit can overwrite an earlier displaced edit and report the same recovery path for both. Verify: RC4. Require two distinct recovery paths with the exact bytes each exchange displaced. Remedy: Use atomic no-replace publication for the displaced copy. On collision retry, or retain and report the already unique exchange temporary name.
- **F62-5** `crates/c3-core/src/quota.rs:956`, `crates/c3-core/src/quota.rs:1111`, `crates/c3-core/src/quota.rs:1121` - A nested namespace can erase its enclosing collaboration namespace during inventory traversal, causing an offered tool to count as clean switch evidence. Verify: RC5. Assert the enclosing namespace remains decisive at two or more levels. Remedy: Propagate collaboration ancestry during recursion, or classify unsupported namespace nesting as unproven.
- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, still-open) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, still-open) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, not-checked) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, not-checked) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, not-checked) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.
- **F19-1** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision.
- **F32-1** (prior, not-checked) `crates/c3/src/engines/worktree.rs:248` - Stage creation and publication use pathnames after validation, so a replaced directory component can send staged bytes outside the worktree and leave a temporary file there.
- **F57-1** (prior, fixed) `crates/c3/src/maintain/web.rs:209` - Changing a page's URL and public flag while retaining its name can publish IDs from the previously non-public page in telemetry.
- **F57-2** (prior, fixed) `crates/c3-core/src/quota.rs:941` - An uninterpretable nested tool inventory counts as positive switch evidence for a Codex smoke turn.
- **F57-3** (prior, still-open) `crates/c3/src/maintain/mod.rs:740` - Pending recovery can record the pending target version as verified after smoking a different installed version.
- **F57-4** (prior, fixed) `crates/c3/src/state_dir.rs:745` - A live lock creator can return success after another process reclaimed its still-empty file and acquired the same move lock.
- **F57-5** (prior, fixed) `crates/c3/src/consult/reviewed.rs:1451` - Two different patches can publish successfully at one short-hash path, leaving one consultation's full digest bound to the other consultation's bytes.
- **F57-6** (prior, still-open) `crates/c3/src/state_dir.rs:1544` - A linked task directory below a real project collab directory can redirect handoff directory creation and consultation output.

### Unproven scenarios

- No tests, builds, network requests, race injections or filesystem mutations were run in this read-only review. The new findings are based on source inspection.
- The copied evidence does not establish the final three-job CI result for ea30cce0edb45d79bc6efde5220103b75ab351eb.
- No live agy review, real maintenance update/rollback, macOS exchange, real no-hard-link filesystem, or distributed filesystem lock behavior was verified here.
- Live Kimi new/resume records support the ordinary success path. They do not exercise Unix directory replacement during checkout or after run admission.
- The coordinator reports a successful migration and a live junction refusal. I did not inspect the ignored project state, original inventory, or external sentinel.
- The read covered selected delta hunks and document sections. Untouched subsystems, full integration suites, complete logs, and historical findings marked not-checked were not independently re-audited.

### First-run checklist (observable)

- [ ] The CI summary identifies the exact tag SHA and shows successful Windows, Linux and Linux index-surreal jobs with completed test steps.
- [ ] A new isolated Kimi handoff says usable, names the requested revision, and points to an existing patch whose independently computed full SHA-256 matches its filename and ledger. Resume shows the same session and worktree path. An occupied resume path yields pre-launch refusal and preserves its contents.
- [ ] A linked in-repository collab root, task or handoffs directory produces a refusal before launch. A directory listing or identity comparison confirms that records landed only under the intended root and the outside fixture is unchanged.
- [ ] Maintenance output and persisted state name the exact installed release before and after the smoke. A distinct prerelease, unreadable version or changed version cannot produce a verified-target action. Both Codex turns have complete, attributable switch evidence.
- [ ] An offline telemetry sample retains the anonymized model for private cached page and catalog IDs across configuration changes. A freshly fetched public ID appears only under the expected vendor.
- [ ] For a racing repair or roster save, the printed recovery path exists and hashes to the displaced user bytes. Concurrent recoveries have distinct paths. A rename-fallback notice is insufficient evidence of preservation.
- [ ] After migration, file inventories account for the old project records, legacy files and active snapshot anchor under the expected paths. A competing writer leaves one active state folder and its record is present there.
