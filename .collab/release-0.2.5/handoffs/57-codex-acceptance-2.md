# Handoff 57 - Codex: acceptance-2

Date: 2026-11-24 14:31 local. Author: Codex (model gpt-6.2-astra, effort high), Codex CLI 0.169.0.
Reviewer: openai :: gpt-6.2-astra (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.169.0).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - position 1 of 17.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 2c5fa2ce-1a2f-417b-891b-ad150a2f972b.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6.2-astra -c features.multi_agent=false -c agents.enabled=false -c features.plugins=false -c skills.include_instructions=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-ab9cf7f819d44d9f9ba515e8d2d749ff.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-3194401e19174a699bb7e29308261094` of revision 5031982434e26cd6a2127c984c21f42f24419da6 (--rev HEAD; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20db6-ff8e-7ce0-a818-13c0351c4b97` (source: events).
Brief: `.collab/release-0.2.5/handoffs/56-claude-acceptance-2.md` (sha256 8ac46cb4159c). Reviewed: 5031982, base 5031982434e26cd6a2127c984c21f42f24419da6, tree sha256 3e184e701768, 0 changed files. Live HEAD at launch: 5031982434e26cd6a2127c984c21f42f24419da6.
Bridge outcome: usable reply. Wall time: 660.1 s. Tokens: in 2654387 (cached 2529536), out 18256, reasoning 1372.
Quota (rollout): 5h 56% until 19:10; wk 78% until Nov 29; this run 2654387 input tokens (2529536 cached) in 28 calls, max 123328 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false, agents.enabled=false); budget 90 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s.
Verdict: REJECT - Revision 5031982434e26cd6a2127c984c21f42f24419da6 can publish a private model name, verify a Codex update on insufficient or misattributed evidence, and write consultation records outside the intended task directory. Findings: 6 blocker, 3 major, 1 minor, 0 note (F57-1..F57-10, tracked in `findings.json`). Structured reply: `handoffs/57-codex-acceptance-2.reply.json`.
Raw event stream: `handoffs/57-codex-acceptance-2.events.jsonl`.
Verbatim reply follows.

---

1. **Do not tag this revision.** Three blockers need no hostile input or unusual filesystem: reusing a configured page name can publish an old private model ID, a changed rollout schema can produce a false smoke pass, and an interrupted update can be recorded as verified for a binary that never ran. See findings #1–3.

   **Earlier blocker table, judged from source:** Closed for their stated triggers: F02-1, F02-16, F02-21, F03-1, F11-3, F32-1, F13-1 and F19-1. F02-2 is **not closed**: temp publication fixes truncation, but an editor can save after the last comparison and before rename (`setup/repo_models.rs:484`, `contain.rs:607`, both under `crates/c3/src/`). F02-19 is **not closed** for pages. F11-7's blank-thread and opted-in-seat cases are closed, but its broader per-turn proof requirement is **not closed**.

2. The persistent OS gate closes the original stale-record compare/remove race between takers and release. It leaves initial publication outside the gate. A process paused after exclusive creation can lose its empty lock and subsequently return as a second owner (finding #4). The **second place** was patch publication. Atomic rename publishes complete bytes, but it can replace a different file after inspection. Eight hex digits also mean two C3 patches with the same stem can have the same name (finding #5). Yes, a C3 writer can create `.c3` without the move lock: a process with migration disabled reaches `ensure` and `open_state_folder(..., true)` while another process is moving `.eck` (`state_dir.rs:394,419,494,930`). If the new folder gets its marker first, the mover leaves both folders; it cannot replace a nonempty directory.

3. Handle-relative staging and the post-checkout removal walk address the original link traversal. Two pathname operations still matter: task/handoffs creation below an already-checked collab root, and the interval between claiming a resumed worktree and opening its root after Git checkout (findings #6–7). A pre-existing linked **task** directory needs no timing race.

4. **Yes.** A recognized outer tool array marks a turn proven before its elements have been validated. A namespace with an unreadable inner tool list can therefore count as switch evidence. Separately, even valid evidence can be attributed to the wrong installed version (findings #2–3).

5. Single-revision patches default to tracked content; CLI and MCP can explicitly include untracked bytes. The ordinary HEAD isolation prompt counts new names and lists selected tracked names. Its path filter still admits Unicode format characters (finding #8). Catalog trust uses individual URLs; page trust reuses labels across configuration changes (finding #1). Public catalog GETs disable environment proxies and check addresses. D6's reviewer POST disables redirects and bypasses proxies for loopback. Its `localhost` exception still depends on the system resolver (finding #9).

6. **The last four code commits only partly meet their claims.** R7/R8 make agy's observed worktree changes decisive and bind shared-rollout evidence to turn boundaries. Deadline cleanup takes fresh process snapshots in bounded rounds. A daemon that has left both the process group and parent tree remains outside that guarantee. The layout marker permits retry after an interrupted move. D5 closes the original takeover race, D6's integration test inspects the actual POST body, and D7 refuses an already-occupied name at creation. None establishes the stronger guarantees contradicted above.

7. I would also hold automatic roster replacement under the brief's loss-of-operator-data criterion (finding #10). Missing live agy, real no-hard-link media and macOS coverage are gaps, not independently demonstrated failures. I did not run builds, tests, network calls or destructive fixtures. I read the brief, selected state-log and changelog sections, and selected implementation and test ranges. I did not read the entire release diff, findings store, CI logs, routing and branch implementations, every engine or every historical fix. The brief reports earlier green runs; this review has no final three-job CI result for `5031982`.

## Requested checks

- **RC1** — Repository root, workspace-write, one telemetry fixture: persist IDs from a non-public page, reuse its label for a different public URL, and serialize an event offline. The private ID must be absent.
- **RC2** — Repository root, workspace-write, two maintenance fixtures: feed one opaque nested tool list to the two-turn smoke, then recover a pending update with a different installed version. Expect `unproven` in the first and no verification of the pending target in the second.
- **RC3** — Disposable repository, workspace-write, 30 seconds: pause lock initialization after exclusive creation for longer than the garbage interval and run a second acquirer. Observe at most one process entering the protected section.
- **RC4** — Repository root, workspace-write, one patch fixture: use two distinct payloads at the same short name, inject unsupported hard links and interleave publication. Every successful ledger row must retain matching bytes.
- **RC5** — Disposable repository, workspace-write, one local fake consult: use a real `.c3/collab` with a task directory linked to an empty fixture directory. Expect refusal before any handoff, lock or ledger appears there.

---

### Findings

- **F57-1** [blocker] `crates/c3/src/maintain/web.rs:209`, `crates/c3/src/maintain/web.rs:244`, `crates/c3/src/maintain/models.rs:1025`, `crates/c3/src/maintain/models.rs:1075` - Changing a page's URL and public flag while retaining its name can publish IDs from the previously non-public page in telemetry. Trigger: Maintenance cached a private ID under a page label. A later configuration uses the same label for a public vendor page, and a new process emits telemetry before that cache is replaced. Evidence: read-code: Both collection and trust use page:<name>. Publication joins persisted IDs to the current configuration by that string and can assign a plain ID the new URL's vendor class.; read-code: The duplicate-name check applies within one configuration. It cannot distinguish successive uses of the same label. Verify: Run RC1 with a synthetic ID and inspect the serialized reviewer model without sending telemetry. Remedy: Persist the exact source URL and provenance with page IDs. Require them to match the trusted configuration and withhold legacy label-only records until refreshed. Supersedes: F02-19.
- **F57-2** [blocker] `crates/c3-core/src/quota.rs:941`, `crates/c3-core/src/quota.rs:1045`, `crates/c3/src/maintain/smoke.rs:861` - An uninterpretable nested tool inventory counts as positive switch evidence for a Codex smoke turn. Trigger: A completed turn has no explicit switch value and its only evidence is an additional_tools array containing an unrecognized entry or a namespace whose tools field has changed shape. Evidence: read-code: The parser marks the turn first. The recursive collector silently skips nameless tools and non-array namespace contents, returning no indication that the inventory was incomplete. The caller then accepts the evidence bit. Verify: Use RC2 with a namespace whose nested tools field is an object. Assert the update is held. Remedy: Have inventory parsing return complete, breached or unproven. Grant evidence only after validating every entry against a supported schema. Supersedes: F11-8.
- **F57-3** [blocker] `crates/c3/src/maintain/mod.rs:740`, `crates/c3/src/maintain/mod.rs:1191`, `crates/c3/src/maintain/mod.rs:1211` - Pending recovery can record the pending target version as verified after smoking a different installed version. Trigger: The installed CLI changes to a third version between interruption and recovery. Its smoke passes. Evidence: read-code: Pending selection matches the engine. Recovery handles equality with the previous version, then smokes the launcher and formats success using p.target. The caller stores that action and clears the pending entry. Verify: In RC2 set previous, target and installed to three distinct versions. Inspect both the action and pending record. Remedy: Bind verification to a measured installed version matching the target. Record a third or unknown version explicitly and never credit its smoke to the target.
- **F57-4** [blocker] `crates/c3/src/state_dir.rs:745`, `crates/c3/src/state_dir.rs:765`, `crates/c3/src/state_dir.rs:803`, `crates/c3/src/state_dir.rs:819` - A live lock creator can return success after another process reclaimed its still-empty file and acquired the same move lock. Trigger: On Unix the creator is suspended between exclusive creation and record publication for over ten seconds. A second process takes over the unparseable record before the creator resumes. Evidence: read-code: Exclusive creation, process-identity lookup, write and failure cleanup are outside the OS gate. An old unparseable file is reclaimable; writing the original open file after unlink need not fail, and success performs no ownership check. Verify: Run RC3 on Unix and count overlapping critical-section entries. Remedy: Use the same gate for creation through complete record publication and for write-failure cleanup, or hold an OS lock for the full ownership interval.
- **F57-5** [blocker] `crates/c3/src/consult/reviewed.rs:1451`, `crates/c3/src/consult/reviewed.rs:1516`, `crates/c3/src/consult/reviewed.rs:1548`, `crates/c3/src/consult/reviewed.rs:1735` - Two different patches can publish successfully at one short-hash path, leaving one consultation's full digest bound to the other consultation's bytes. Trigger: Concurrent placements with the same stem and first eight SHA-256 hex digits both see a missing name, and hard-link publication falls back to replacing rename. A foreign file arriving before rename is also overwritten without requiring a collision. Evidence: read-code: Only 32 digest bits enter the path. AlreadyExists from hard_link triggers another comparison, but other errors select rename_onto. A successful replacing rename returns success without a no-replace condition. Verify: Run RC4 with two supplied payloads and an injected shared destination name. Remedy: Use the full digest and a no-replace publication primitive. Where unavailable, publish under an exclusive per-run name and record that path. Supersedes: F19-3, F20-3.
- **F57-6** [blocker] `crates/c3/src/state_dir.rs:1544`, `crates/c3/src/consult/orchestrate/mod.rs:2531`, `crates/c3/src/consult/orchestrate/mod.rs:4041`, `crates/c3/src/consult/detach.rs:583`, `crates/c3/src/panel/run.rs:942` - A linked task directory below a real project collab directory can redirect handoff directory creation and consultation output. Trigger: The collab root is `.c3/collab` and its selected task component already points to another directory. Evidence: read-code: begin_run checks and prepares the collab root. The caller appends the task and handoffs components, then uses ordinary create_dir_all. The reply and events paths derive from that same spelling. Verify: Run RC5 and inspect the destination directory even if the consultation subsequently fails. Remedy: Open or create the task and handoffs components under the collab handle without following links. Keep outputs bound to those handles through publication.
- **F57-7** [major] `crates/c3/src/engines/worktree.rs:144`, `crates/c3/src/engines/worktree.rs:285`, `crates/c3/src/engines/worktree.rs:301`, `crates/c3/src/engines/worktree.rs:314` - The D7 resume claim proves that a pathname was free at mkdir time, but does not bind Git checkout or failure cleanup to the directory that mkdir created. Trigger: The recorded path reaches the temp directory through a mutable parent alias, or the claimed directory changes identity before Git opens it. On Unix the directory comparison also normalizes literal backslashes as separators. Evidence: read-code: The comparison transforms canonical paths to strings and returns the recorded spelling. mkdir, Git and failure cleanup each resolve it separately. The first retained Root is opened after checkout. Verify: With an injected identity change in a disposable checkout, assert the resume refuses and preserves the replacement directory. Remedy: Build the child path from the trusted temp parent and validated basename. Keep and verify the claimed directory identity through checkout and cleanup, and compare native identities for parent equality.
- **F57-8** [minor] `crates/c3/src/engines/worktree.rs:1297`, `crates/c3/src/engines/worktree.rs:1324` - The HEAD isolation prompt can display a tracked filename with a bidi override or another invisible format character. Trigger: A modified tracked filename contains a Unicode format character, passes the display cap, and the review uses the HEAD isolation paragraph. Evidence: read-code: The prompt filter checks backticks and is_control(), but does not use the new Unicode format-character filter. The explicit --rev paragraph is separate. Verify: Pass a synthetic tracked name containing U+202E to isolation_prompt_line and assert it is counted without displaying its bytes. Remedy: Apply the shared format-character predicate to tracked prompt names as well.
- **F57-9** [major] `crates/c3-core/src/roster_ext.rs:159`, `crates/c3-core/src/roster_ext.rs:183`, `crates/c3/src/http_engine/mod.rs:831`, `crates/c3/src/http_engine/mod.rs:851` - The test-mode plaintext exception validates the hostname spelling `localhost` without binding its connection to a loopback address. Trigger: Test mode is active and the system resolver supplies a non-loopback address for localhost. Evidence: read-code: The validator accepts the domain by name. The POST disables redirects and loopback proxying but uses ordinary URL resolution and can attach the configured bearer credential.; inferred: A non-loopback resolution would carry the plaintext request to that address. No such request was made in this review. Verify: Use an injected resolver and recording transport to assert refusal before any request bytes when localhost resolves off loopback. Remedy: Restrict plaintext test URLs to loopback IP literals, or pin the transport to validated loopback addresses for localhost.
- **F57-10** [major] `crates/c3/src/maintain/models.rs:1145`, `crates/c3/src/maintain/models.rs:1150` - The accepted roster-write limitation can lose a hand edit, including one absent from the daily backup. Trigger: The operator saves after the last basis comparison and before atomic replacement. Evidence: read-code: The backup uses the old basis. The final read is followed by a separate atomic-write call with the already-computed roster. Verify: At an injected pre-publication boundary, save an unrelated roster change and check whether that exact version remains recoverable. Remedy: Use a publication mechanism that preserves the displaced version, or leave a proposed roster for manual application where concurrent hand edits cannot be protected.

### Prior findings

- F02-1 - fixed - Repair publishes relative to an opened directory and replaces the hard-linked entry.
- F02-2 - still-open - Truncation is addressed. The final read at repo_models.rs:486 and rename at contain.rs:607 are separate.
- F02-3 - not-checked - Did not trace all custom IDs and context modifiers.
- F02-4 - not-checked - Did not verify the followable roster selection.
- F02-5 - not-checked - Checked review turns, not both auxiliary launchers.
- F02-6 - not-checked - Did not time both hook actions or trace every root lookup.
- F02-7 - not-checked - Did not inspect description round trips.
- F02-8 - not-checked - Did not trace doctor and listing index activity.
- F02-9 - not-checked - Did not verify ancestry failure classification.
- F02-10 - not-checked - Did not verify cherry-pick and revert reporting.
- F02-11 - fixed - The name must have the exact lowercase UUID shape and the directory must be under temp.
- F02-12 - not-checked - Did not trace the full redirect policy.
- F02-13 - not-checked - Did not measure redirect and streaming limits.
- F02-14 - not-checked - Did not review contradictory date fixtures.
- F02-15 - not-checked - Did not trace endpoint-list keys.
- F02-16 - fixed - bound_roster checks position, provider, old model and whole-entry fingerprint.
- F02-17 - not-checked - Did not verify escaped JSON field replacement.
- F02-18 - fixed - The integration case covers explicit models with and without dry-run under the off switch. Not rerun.
- F02-19 - still-open - Catalog URL provenance is fixed. Cached page-label provenance remains open in finding #1.
- F02-20 - fixed - The published Claude mapping requires a trusted source. A numeric shape alone no longer grants publication.
- F02-21 - fixed - The handle walk returns on unlink errors before removal, with no Git force fallback.
- F03-1 - fixed - The launcher sets the marker after rebuilding the child environment.
- F03-2 - fixed - The old recursive force fallback was removed. D7's pre-checkout claim is a distinct issue.
- F03-3 - not-checked - Did not trace branch config key handling.
- F03-4 - fixed - Repair uses the shared frontmatter parser and replaces the value span.
- F03-5 - not-checked - Did not verify uppercase suffix handling.
- F03-6 - not-checked - Did not inspect the complete listing budget.
- F04-1 - not-checked - Did not trace status through doctor and listing.
- F04-2 - not-checked - Did not trace all successor sources through the date gate.
- F04-3 - not-checked - Did not verify hostile catalog chronology.
- F04-4 - fixed - Doctor's fix path explicitly suppresses repair in a marked child.
- F04-5 - fixed - An incomplete unlink pass returns before content removal.
- F04-6 - not-checked - Did not trace auth probe and detached maintenance environments.
- F04-7 - not-checked - Checked worktree naming only, not the default-branch fallback.
- F04-8 - not-checked - Reviewed telemetry provenance but not every combined transport invariant.
- F04-9 - not-checked - Did not inspect config read-error handling.
- F10-1 - fixed - Parameter fragments no longer determine acceptance. Ag y's observed worktree changes determine the outcome.
- F10-2 - not-checked - Did not trace renamed or unindexed ACTIVE/DONE steps.
- F10-3 - not-checked - Did not trace all panel reseat failures.
- F10-4 - not-checked - Located snapshot coverage but did not verify concurrent store writes.
- F10-5 - fixed - Known multi-agent names count across MCP namespaces and opted-in seats return unproven.
- F10-7 - not-checked - Located panel death-record coverage, not the full detached flow.
- F11-1 - fixed - A changed isolated agy worktree fails independently of the parameter log. Clean trees can keep the reply.
- F11-2 - not-checked - Did not inspect the entire artifact binding and drift path.
- F11-3 - fixed - Staging uses the opened worktree root. D7's earlier checkout interval is distinct.
- F11-4 - not-checked - Did not verify the two panel preflight cases.
- F11-5 - fixed - New names are counted, rename sources come from HEAD and line breaks are withheld. Finding #8 concerns format characters.
- F11-6 - not-checked - Did not independently verify all prompt-to-commit snapshot transitions.
- F11-7 - fixed - Blank thread IDs and opted-in seats explicitly return Unproven. Finding #2 concerns a different false pass.
- F11-8 - still-open - Top-level array and session identity checks exist. Uninterpretable nested tool entries still count in finding #2.
- F11-9 - not-checked - Did not trace fast spawn detection through final polling.
- F11-10 - not-checked - Located secondary prompt coverage without reviewing all continuation builders.
- F11-11 - not-checked - Did not verify terminal-stream persistence across bridge death.
- F11-12 - not-checked - Did not trace reply outcome aggregation.
- F12-1 - fixed - Stale classification explicitly leaves uncheckable owners alone.
- F12-2 - not-checked - Did not trace all root-resolution Git calls.
- F12-3 - not-checked - Did not verify reparse-placeholder wording.
- F12-4 - not-checked - Did not review the final branch config write.
- F13-1 - fixed - Stale verdict excludes linked roots and cleanup uses non-following handles.
- F13-2 - not-checked - Did not trace fake .git handling.
- F13-3 - not-checked - Did not verify date parsing in foreign branch notes.
- F13-4 - not-checked - Did not check both updater and branch mutation guards.
- F13-5 - not-checked - Read shared stop logic without tracing the specific branch shim.
- F13-6 - not-checked - The brief acknowledges numerical following without dates; did not independently trace the fixture.
- F13-7 - fixed - Failed patch checks now clear patches_checked, with a targeted test.
- F13-8 - fixed - Owner recording is mandatory and stale cleanup excludes a missing owner.
- F13-9 - fixed - Final repair publication uses the held parent directory. The remaining editor race is F02-2.
- F14-1 - not-checked - Did not measure DNS, connect and trickle deadlines.
- F14-2 - not-checked - Located seated-model retention coverage but did not trace retention after reseating.
- F14-3 - not-checked - Did not inspect scratch roster file creation.
- F14-4 - not-checked - Did not verify all protection value types.
- F14-5 - not-checked - Did not inspect the unanchored future-date case.
- F15-1 - fixed - Public GETs check resolved addresses and peers, disable proxies and reject IP-literal source URLs.
- F15-2 - not-checked - Did not verify the full persistence lifecycle beyond the active-seat test.
- F15-3 - fixed - The entry-bound smoke verifies the old model and original entry fingerprint.
- F15-5 - not-checked - Did not trace the newer-release installation decision. Finding #3 concerns a passing recovery smoke.
- F15-6 - not-checked - Did not verify all filesystem effects of the off path.
- F16-1 - fixed - Catalog fetch and trust keys use the individual URL. Finding #1 is about page labels.
- F16-2 - fixed - Failure rendering omits the hostile display title; a fixture includes control and instruction text.
- F16-3 - not-checked - Did not trace the entire CI operation under its caller deadline.
- F16-4 - not-checked - Did not verify run pagination and timestamp sorting.
- F16-5 - fixed - Normal removal retains the opened root and no longer runs forced recursive deletion by pathname.
- F16-6 - not-checked - Did not review replay versus the actual draw.
- F16-7 - not-checked - Did not review the lab table.
- F16-8 - not-checked - Did not review required versus weighty lab conflicts.
- F16-9 - fixed - CI uses per-repository gate files and an exclusive daily claim.
- F16-10 - not-checked - Did not compare hook script bytes.
- F17-1 - fixed - The run title is excluded from the failure line.
- F17-2 - not-checked - Did not compare explain and panel eligibility.
- F17-3 - not-checked - Did not trace upper-tier warnings.
- F17-4 - not-checked - Did not inspect MiniMax and Meta-Llama normalization.
- F17-5 - fixed - Catalogs are URL-keyed. Retargeting a page label is a separate provenance problem.
- F17-6 - not-checked - Did not verify the ownership test for rewritten rules.
- F17-7 - not-checked - Did not exercise remote-only default selection.
- F17-8 - fixed - GitHub recognition matches parsed hostnames against the explicit GitHub host list.
- F17-9 - not-checked - Did not inspect script exception output.
- F17-10 - not-checked - Did not verify current wording against queued runs.
- F18-1 - not-checked - Did not trace all finding and ledger commit fields.
- F18-2 - fixed - Live status before checkout passes --no-optional-locks.
- F18-3 - not-checked - Did not inspect all pre/post collab exclusions.
- F18-4 - fixed - Untracked patch contents require explicit opt-in.
- F18-5 - fixed - The staging source and destination now use contained reads and writes.
- F18-6 - fixed - External links are swept before reviewer launch; failure aborts creation.
- F18-7 - not-checked - Did not verify all stream-name exclusion variants.
- F19-1 - fixed - HTTP pack source selects the revision tree and disables the live index. The loopback integration test inspects the body.
- F19-2 - fixed - Single-revision diffs omit untracked content by default.
- F19-3 - still-open - Ordinary distinct hashes separate runs. Short-hash collisions on fallback publication remain in finding #5.
- F19-4 - fixed - Owner-record failure aborts worktree creation before reviewer launch.
- F19-5 - not-checked - Did not trace child restrictions and revision syntax at all entry points.
- F19-6 - not-checked - Did not trace prior findings through historical-revision bookkeeping.
- F20-1 - fixed - PackExtras includes the written patch. The HTTP integration fixture checks its body and ledger path.
- F20-2 - fixed - Worktree creation uses --no-optional-locks for live status.
- F20-3 - still-open - Short-hash collision plus replacing rename can still invalidate a ledger binding, as narrowed in finding #5.
- F20-4 - fixed - ReviewTree takes both snapshots with revision_info_with_ignored.
- F20-5 - fixed - Explicit revision prompts are separate and HEAD isolation withholds untracked names. Finding #8 covers a remaining display case.
- F20-6 - fixed - Untracked content is opt-in.
- F24-1 - fixed - Staging uses an opened worktree directory and handle-relative file publication.
- F24-2 - fixed - Different noted and pending streams are compared; a nonempty main path alone no longer wins.
- F24-4 - fixed - Acceptance no longer depends on recursively merging parameter objects.
- F27-1 - fixed - The address tables include 0/8, CGNAT and site-local IPv6.
- F27-2 - fixed - The external-link sweep uses the shared handle walker.
- F27-3 - not-checked - Did not verify patch-path ignore matching in every live-tree engine.
- F28-1 - fixed - Fallback publishes a complete temp by rename. Finding #5 concerns replacement of a competing name.
- F28-2 - not-checked - Read the change description but did not trace large exclusion handling.
- F28-3 - not-checked - Did not trace non-UTF-8 diff bytes.
- F29-1 - still-open - Public fetches disable proxies. A timed-out DNS worker can remain until lookup returns, as acknowledged in the brief.
- F29-2 - fixed - Staging reads source bytes using the directory-handle path rather than validating and later copying an ambient name.
- F29-3 - not-checked - Did not trace HEAD movement through prior gating.
- F30-1 - not-checked - Did not inspect router explain preflights.
- F30-2 - not-checked - Did not inspect full-panel tier warnings.
- F30-3 - fixed - A per-repository daily claim serializes the decision.
- F30-4 - not-checked - Did not inspect malformed replay rows.
- F30-5 - not-checked - www.github.com is recognized; arbitrary SSH aliases were not checked.
- F30-6 - fixed - The sweep recognizes gate temp names and has stale-temp coverage.
- F31-1 - not-checked - Reviewed stop rounds but did not reproduce the Windows child-process case.
- F31-2 - not-checked - Did not verify missing-origin/HEAD CI lookup.
- F31-3 - fixed - Exclusive gate claim addresses the same-repository race.
- F31-4 - fixed - ssh.github.com is in the host table and fixtures.
- F31-5 - fixed - Gate temp files are included in pruning.
- F31-6 - not-checked - Did not compare roster-order panels with router explain.
- F32-1 - fixed - After checkout, staging is bound to a retained worktree root handle.
- F32-2 - fixed - Every completed segment needs its own evidence bit. Finding #2 concerns what sets that bit.
- F32-3 - fixed - Read-file parameters no longer turn filenames into writing steps.
- F32-4 - fixed - The bounded runner checks child exit independently of pipe closure and has a specific regression test.
- F32-5 - not-checked - Did not inspect large-prior member launch.
- F36-1 - fixed - Normal migration is serialized and the legacy handle path refuses to recreate a vanished .eck. Finding #4 is a different lock failure.
- F36-2 - fixed - Leftovers relocate before rename; a markerless destination is revisited.
- F36-3 - not-checked - Checked state-folder opens but not all three migration paths.
- F36-4 - fixed - Read-only collab resolution is distinct from collab_dir_moving.
- F36-5 - fixed - State path classification applies lexical normalization before mapping.
- F36-6 - not-checked - Did not verify every live-run directory error or start race.
- F36-7 - not-checked - Did not verify the current off-switch documentation against all collab writes.
- F36-8 - not-checked - Did not verify nested state exclusions in generated patches.
- F38-1 - not-checked - Read the coordinator's step-timeout report, not the CI workflow or cancellation logs.
- F38-2 - not-checked - The listed transition prefixes are covered; did not verify all ISATAP forms.
- F39-1 - fixed - The IPv6 table includes compatible, Teredo, benchmark and 6to4 ranges.
- F39-2 - not-checked - Did not verify decoy-row selection for the price denominator.
- F39-4 - not-checked - Did not trace catalog_alias maker attribution.
- F43-1 - fixed - Patch placement uses an anchored patch directory. Finding #6 concerns general consultation output.
- F43-2 - fixed - Fallback no longer creates claims.
- F43-3 - fixed - The claim failure cleanup was removed. Finding #5 concerns replacing publication.
- F43-5 - fixed - Agent discovery reads candidate bytes through the opened agents directory.
- F43-6 - still-open - An unlink failure can leave a partial worktree. The behavior is documented and further deletion stops.
- F43-7 - not-checked - Did not compare every cleanup guarantee in the changelog.
- F43-8 - not-checked - Did not exercise inside links and cloud placeholders.
- F44-1 - fixed - No empty final-name claim is created by fallback.
- F44-2 - not-checked - Did not audit the final text against every linked staging input.
- F44-3 - fixed - open_checked uses a nonblocking no-follow open and verifies the handle kind.
- F45-1 - not-checked - Did not review secret scanner patterns.
- F45-2 - still-open - A detached reparented daemon can escape; fallback improves the ordinary process-table failure case.
- F45-3 - fixed - The CI code expressly selects a repository and tests multiple remotes.
- F45-4 - not-checked - Did not inspect hook fallback capping.
- F45-5 - not-checked - Did not test capture of output preceding a timeout.
- F45-6 - not-checked - Did not inspect skip-checks diagnostics.
- F45-7 - fixed - stop drains children outside the registry lock and uses try_wait in bounded cleanup.
- F45-8 - fixed - The close path explicitly includes ignored and all untracked files in its cleanliness check.
- F45-9 - fixed - run_all_within applies both per-check and checks-phase deadlines.
- F46-1 - still-open - Fresh process snapshots and group kills improve ordinary cleanup. A reparented process outside the group remains beyond the sweep.
- F46-2 - not-checked - Did not verify keyword assignment redaction.
- F46-3 - fixed - Close now includes ignored files in its pre-removal status check.
- F46-4 - not-checked - GH_REPO and GH_HOST are removed; did not check every concurrent gate-field update.
- F46-5 - not-checked - Did not compare filesystem root detection with Git.
- F46-6 - not-checked - Did not verify malformed config with skipped finish checks.
- F46-7 - not-checked - Did not inspect Linux PID disappearance and permission cases.
- F46-8 - not-checked - Did not compare hook line deduplication.
- F47-1 - fixed - Markerless state folders enter finish_half_done without requiring the two legacy filenames.
- F47-2 - not-checked - Checked handle-based state writes, not every migrated operation. Finding #6 concerns consultation task creation.
- F47-3 - not-checked - Did not run Windows trailing-dot and trailing-space cases.
- F47-4 - fixed - A parseable local live owner's lock has no age takeover. Finding #4 is before the record parses.
- F48-1 - still-open - The direct linked-state-root write is guarded. The declared read-through exception remains; finding #6 is below a real root.
- F48-2 - fixed - Legacy state writes open without creation and local live records are not timed out. Finding #4 concerns initialization.
- F48-3 - fixed - A markerless anchor-only folder is eligible for relocation.
- F48-4 - not-checked - Did not verify short names and all filesystem aliases.
- F48-5 - not-checked - Did not trace every junction or state-file hard-link case.
- F51-1 - fixed - Parameter names no longer govern a changed agy worktree's outcome.
- F51-2 - not-checked - Staging uses captured source bytes; did not trace every ledger hash and fallback.
- F51-3 - fixed - The claim takeover and cleanup code has been removed.
- F51-4 - fixed - Bound-drift handling distinguishes unreadable from changed inputs and has explicit regression coverage.
- F51-5 - fixed - MCP defaults to tracked-only and exposes diff_untracked.
- F51-6 - fixed - The note wins when one stream lacks a usable mtime and the names differ.
- F52-1 - fixed - Changed isolated worktrees determine failure without command-shape inference.
- F52-2 - fixed - There is no timed patch claim to take over.
- F52-4 - not-checked - Confirmed equal and missing mtimes favor the note, but did not verify all same-name and pruned-stream scenarios.
- F53-1 - fixed - The HEAD isolation paragraph is constructed before conditional path lists.
- F53-2 - fixed - Path-key parameter merging no longer determines whether agy's changed worktree fails.
- F53-3 - not-checked - Did not inspect transient findings-store read failure.
- F53-4 - not-checked - Did not verify crash-debris sweeps for both index and patch artifacts.
- F53-5 - fixed - Diff untracked-name display uses the shared format-character filter. Finding #8 is a separate prompt formatter.
- F53-6 - fixed - The claim deletion path no longer exists.
- F53-7 - fixed - MCP and detached diffs default to tracked content. MCP callers must opt into untracked transmission.

## Verdict: REJECT

Revision 5031982434e26cd6a2127c984c21f42f24419da6 can publish a private model name, verify a Codex update on insufficient or misattributed evidence, and write consultation records outside the intended task directory.

### Blockers

- **F57-1** `crates/c3/src/maintain/web.rs:209`, `crates/c3/src/maintain/web.rs:244`, `crates/c3/src/maintain/models.rs:1025`, `crates/c3/src/maintain/models.rs:1075` - Changing a page's URL and public flag while retaining its name can publish IDs from the previously non-public page in telemetry. Verify: Run RC1 with a synthetic ID and inspect the serialized reviewer model without sending telemetry. Remedy: Persist the exact source URL and provenance with page IDs. Require them to match the trusted configuration and withhold legacy label-only records until refreshed.
- **F57-2** `crates/c3-core/src/quota.rs:941`, `crates/c3-core/src/quota.rs:1045`, `crates/c3/src/maintain/smoke.rs:861` - An uninterpretable nested tool inventory counts as positive switch evidence for a Codex smoke turn. Verify: Use RC2 with a namespace whose nested tools field is an object. Assert the update is held. Remedy: Have inventory parsing return complete, breached or unproven. Grant evidence only after validating every entry against a supported schema.
- **F57-3** `crates/c3/src/maintain/mod.rs:740`, `crates/c3/src/maintain/mod.rs:1191`, `crates/c3/src/maintain/mod.rs:1211` - Pending recovery can record the pending target version as verified after smoking a different installed version. Verify: In RC2 set previous, target and installed to three distinct versions. Inspect both the action and pending record. Remedy: Bind verification to a measured installed version matching the target. Record a third or unknown version explicitly and never credit its smoke to the target.
- **F57-4** `crates/c3/src/state_dir.rs:745`, `crates/c3/src/state_dir.rs:765`, `crates/c3/src/state_dir.rs:803`, `crates/c3/src/state_dir.rs:819` - A live lock creator can return success after another process reclaimed its still-empty file and acquired the same move lock. Verify: Run RC3 on Unix and count overlapping critical-section entries. Remedy: Use the same gate for creation through complete record publication and for write-failure cleanup, or hold an OS lock for the full ownership interval.
- **F57-5** `crates/c3/src/consult/reviewed.rs:1451`, `crates/c3/src/consult/reviewed.rs:1516`, `crates/c3/src/consult/reviewed.rs:1548`, `crates/c3/src/consult/reviewed.rs:1735` - Two different patches can publish successfully at one short-hash path, leaving one consultation's full digest bound to the other consultation's bytes. Verify: Run RC4 with two supplied payloads and an injected shared destination name. Remedy: Use the full digest and a no-replace publication primitive. Where unavailable, publish under an exclusive per-run name and record that path.
- **F57-6** `crates/c3/src/state_dir.rs:1544`, `crates/c3/src/consult/orchestrate/mod.rs:2531`, `crates/c3/src/consult/orchestrate/mod.rs:4041`, `crates/c3/src/consult/detach.rs:583`, `crates/c3/src/panel/run.rs:942` - A linked task directory below a real project collab directory can redirect handoff directory creation and consultation output. Verify: Run RC5 and inspect the destination directory even if the consultation subsequently fails. Remedy: Open or create the task and handoffs components under the collab handle without following links. Keep outputs bound to those handles through publication.
- **F02-1** (prior, fixed) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, still-open) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, fixed) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, still-open) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, fixed) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, fixed) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, fixed) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, fixed) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, fixed) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.
- **F19-1** (prior, fixed) `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision.
- **F32-1** (prior, fixed) `crates/c3/src/engines/worktree.rs:248` - Stage creation and publication use pathnames after validation, so a replaced directory component can send staged bytes outside the worktree and leave a temporary file there.

### Unproven scenarios

- No tests were executed in this read-only review. Final Windows, Linux and index-surreal CI results for 5031982 were unavailable.
- No live agy turn on this release, live D7 Kimi resume, production maintenance swap or rollback, or real project migration was observed.
- No macOS run, physical no-hard-link filesystem, filesystem fault injection or concurrent lock/publish interleaving was observed.
- No real network request or telemetry event was sent. The provenance and localhost conclusions are static analyses.
- The full release diff, findings store, CI logs, router, every process launcher, every platform-specific path and all historical scenarios marked not-checked were not independently reviewed.

### First-run checklist (observable)

- [ ] CI identifies the exact candidate SHA and shows completed Windows, Linux and index-surreal test jobs, including the Claude launcher test. A green result for an earlier SHA does not satisfy this check.
- [ ] First migration reports its source and destination. Compare an inventory and hashes of task records, journal, snapshot anchor and legacy files before and after. Observe one authoritative project folder and its layout marker; a second write must use that folder.
- [ ] A real Kimi new turn and resume show the same thread and worktree pathname, the intended revision SHA, a usable reply and clean worktree cleanup. A competing resume at an occupied path must refuse before launch.
- [ ] A real agy turn shows the intended SHA and isolated cwd. Inspect its worktree-change result alongside the actual files and reply disposition.
- [ ] Hash the retained patch and compare it with the full ledger SHA-256. Compare the received HTTP fixture body with the pinned revision and confirm the live-only fixture and unrequested untracked contents are absent.
- [ ] For a Codex update, record the installed version before and after, the selected entry, both consult IDs and matching rollouts. Each completed turn must show intelligible switch evidence with no collaboration tools or calls. Missing or opaque evidence must produce a hold or rollback; verify the resulting installed version.
- [ ] Inspect an offline telemetry serialization after retargeting a page label. The synthetic private ID must render as other. In the test HTTP transport log, the actual peer must be loopback, with no proxy or redirect hop.
