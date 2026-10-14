# Handoff 29 - Codex: panel-b-deepseek-2

Date: 2026-10-14 21:19 local. Author: Codex (model deepseek-v4.1-flash, effort high), Codex CLI 0.155.1.
Reviewer: byteplus :: deepseek-v4.1-flash (provider from -Provider, model from -Model; endpoint https://ark.ap-southeast.bytepluses.com/api/coding/v3, wire_api: responses; provider fingerprint ed61f9eb93fe; harness codex-cli 0.155.1).
Preflight: ok: env BYTEPLUS_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 6 of 12 for -Provider byteplus (codex_config applied).
Effort: high sent (requested high, mapping ark-v1, by caps-v1: ark.ap-southeast.bytepluses.com, deepseek-v4.1-flash; not confirmed by the provider). Consultation id: ed9f952f-42f8-4aee-9af0-3d627dccd8a3.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `codex exec --sandbox read-only --color never --json -m deepseek-v4.1-flash -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="byteplus" -c model_supports_reasoning_summaries=true -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-c43128b5d6db4295a3d375e0cc692efd.md -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13bd7-6809-73c3-9290-5f9a3fded52a` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` (sha256 a34288e066ed). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 218.1 s. Tokens: in 766454 (cached 653184), out 15839, reasoning 7996.
Quota (rollout): no window in the rollout; this run 766454 input tokens (653184 cached) in 9 calls, max 94532 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 85 % of the 5-hour window.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Panel B frames too narrow an option set and an unexamined cost model; the watch and the browser engine are both answerable, but two unlisted baselines and one ruling contradiction must be resolved first. Findings: 1 blocker, 4 major, 0 minor, 0 note (F29-1..F29-5, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/29-codex-panel-b-deepseek-2.reply.json`.
Raw event stream: `handoffs/29-codex-panel-b-deepseek-2.events.jsonl`.
Verbatim reply follows.

---

# Panel B - framing review

**Challenge the framing.** The brief fixes the cost model and the option set before the questions are answerable. Three things are unnamed: (i) the zero-build baseline for (B) - a human relay (`c3 pack` -> paste into the web chat -> ingest the reply), which already exists and needs no extension, relay, signing key or CSP fight; (ii) a *declarative* adapter (a signed selector/DOM map interpreted by one fixed driver already shipped in the extension) as a third placement that is neither "WASM in the extension" nor "WASM in C3"; (iii) that the operator's rulings *draft-only by default* and *panels/detached runs* are mutually exclusive, so Q3's recovery contract cannot be specified until that is settled.

What I would need to choose: the host's effective prompt-cache TTL; whether automation of the vendor chats is acceptable under their terms (it decides whether (B) is legal at all); who holds the catalogue signing key; and whether a browser member may ever run detached. Those are operator answers, not panel ones.

**Q1 - fold, next event, where a skipped wake loses.**
Fold exactly what `--status` already judges (`crates/c3/src/consult/detached.rs:274`): `done|running|elsewhere|starting|died|never-started|unreadable`, derived from pid + start_time + host, never from mtimes. Fold the health journal and ledger only for *new terminal* entries. Do **not** fold worker STATE tails into the line: workers author that text, so it is a prompt-injection and privacy channel into the coordinator - print a closed-vocabulary token plus a file path, never quoted prose. The cheapest *reliable* next event is not budget end or quota reset (both are wall estimates on a shared account); it is min(cache TTL ceiling, earliest liveness deadline), with the wake still armed at ~1200 s per the coordinate skill. A skipped wake loses (a) a cache that expired - the next real interaction pays a full context rewrite, costing more than the wakes saved - and (b) exactly the silent death the watch exists to catch: a tree-killed background writes nothing, so "nothing since T" is false comfort. The watch needs a positive liveness judgement (a lease), never absence-of-change.

**Q2 - where the adapter runs, minimal host API.**
B' is the only placement that survives the constraints: MV3 forbids remote code, content scripts inherit the page CSP, and extension pages need `wasm-unsafe-eval`. "WASM in the extension" is a feasibility blocker, not a preference. But B' is not sufficient as sketched: the *bridge protocol becomes the host API*, so it must be capability-scoped per adapter and origin, not generic DOM, and the catalogue must name a signer with rotation/revocation and key intake rows on the adapter content hash, not its name. Minimal API, deny-by-default: `bind(tab, origin) -> handle`; `verify_catalogue(hash)`; `compose(handle, text)`; `read_reply(handle, thread_scope)`; `collect()`; `release(handle)`. No `query`/`click`/`eval`, no page-wide read. Given that, price the declarative adapter before any WASM toolchain.

**Q3 - what the engine must record.**
A browser run is not a process, so timeout/stall/tree-kill do not apply. It needs a persisted interactive state machine with explicit terminal classes: `drafted / awaiting_send / sent / streaming / relay_lost / tab_closed / expired / stopped / salvaged_partial`. "The user did not press send" is a first-class *terminal* class with its own deadline (a consent lease), not an error and not a stall. Every ending writes exactly one durable record and returns `--wait`; `--kick` must never touch the user's tab; draft-mode retry (format repair, denial retry) must re-ask for consent rather than re-send.

**Q4 - unlisted risks.**
The manual-relay baseline; vendor terms may forbid automation even with a user press; composer autosave/RPC editors can transmit the brief before send; and a panel of draft-only members serializes on one human, so `panel_concurrency` stops meaning throughput and any usefulness table partly measures the operator's availability.

## Requested checks
- **RC1 (read-only, 5 min):** `rg -n "return [0-9]|exit\(?[0-9]" crates/c3/src/consult crates/c3-cli/src` and `rg -n "Exit" plugin/skills/*/reference.md`; observation: which numeric codes are already taken by `--status`/`--wait`/`--kick` before any `watch` code is assigned. Settles finding #4.
- **RC2 (workspace-write, 30 min):** scratch unpacked MV3 extension; attempt `WebAssembly.instantiate` of bundled and fetched bytes in an extension page, a content script and an offscreen document, recording CSP console errors. Settles finding #2 and F28-5.
- **RC3 (workspace-write, 20 min):** start a detached run, kill its background child out of band, then compare an mtime scan with `c3 consult --task <t> --status`. Settles finding #5.
- **RC4 (operator question, 2 min):** ask (a) the host's effective prompt-cache TTL, (b) whether any vendor-chat automation is acceptable, (c) whether a browser member may run detached, (d) who signs the catalogue and how a hash is revoked. Settles the Q3 ruling conflict and finding #1.

---

### Findings

- **F29-1** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:66` - The alternatives list for (B) omits the zero-trust baseline: a human relay that already exists - `c3 pack` builds a sanitized brief for a chat model, the operator pastes it into the web chat and pastes/ingests the reply. It needs no extension, no relay, no per-run token, no signing key and no MV3 CSP fight, so the browser engine must be justified as an increment over it, not as the only way to reach web-only reviewers. Trigger: Choosing an architecture for (B) before the manual relay is priced; the extension silently becomes load-bearing for a capability that already ships. Evidence: read-code: `c3 pack --brief <b> --out <o>` builds a sanitizing reviewer pack for the http engine or for a chat model, and `c3 explain` sends an explainer pack for one claim; both are the human-relay path already implemented.; read-code: Three options are listed (WASM in extension; B'; B'' DevTools) and no do-nothing/human-relay option appears. Verify: Ask the operator to name, for the last month, how many reviews actually required a web-only reviewer that `c3 pack` + paste could not reach; if the count is small, the engine is optional and the baseline must be listed. Remedy: Add the human-relay baseline as alternative 0 in the brief and state the capability the browser engine buys that it does not (unattended reading of a long reply, streaming, structured parse, pacing).
- **F29-2** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55` - A third adapter form is missing from (B): a *declarative* adapter - a signed data file mapping vendor chat elements to the few host operations - interpreted by one fixed, audited driver compiled into the extension. It keeps the signed-hash catalogue while running no remotely supplied code, so it sidesteps MV3's remote-code restriction, the `wasm-unsafe-eval` CSP requirement and the Rust-to-WASM toolchain, and it makes the catalogue entry reviewable as data rather than as a binary. Trigger: Implementing the operator's 'adapters compiled to WASM' ruling and colliding with MV3 remote-code policy or with a selector change that requires shipping new WASM. Evidence: read-code: Engine modules are codex, agy, claude, claude_auth, muse, http, subprocess, tree_check - every engine is in-process Rust with no pluggable adapter loading, so an interpreter for declarative adapters fits the existing model.; inferred: The ruling fixes the adapter *language*, not the capability boundary; a declarative adapter satisfies 'signed hash/pointer catalogue' without remote executable code. Verify: Write one vendor's adapter twice - as WASM and as a declarative map over the six host calls - and count lines changed when one selector moves; if only the data file changes, the declarative form is strictly cheaper to audit and to update. Remedy: Add the declarative adapter as an explicit option under Q2, with the rule that adapters may not embed executable code unless the operator re-affirms the WASM requirement after RC2.
- **F29-3** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:59` - Two operator rulings are mutually exclusive as written and the brief does not say which wins: 'draft-only by default' makes every browser turn wait on a human press, while detached runs and panels are unattended by construction. Q3's recovery contract therefore cannot be specified - a detached panel containing a draft-only member has no defined ending, and the same contradiction determines whether the usefulness table may seat browser members at all. Trigger: A detached run or panel schedules a browser engine member under default draft-only while the operator is away. Evidence: read-code: Draft-only is stated as the default and panels are named as the product ('a panel of draft-only browser reviewers' appears in the risks discussion); detached runs are the existing bridge means, with `--status`/`--wait`/`--detach` in crates/c3/src/consult/detach.rs.; read-code: A detached consultation or panel is listed as running work that keeps the wake armed; that machinery assumes an unattended run can reach a terminal state. Verify: Ask the operator one question: may a browser member be seated in a detached run or a panel, or is it single-run interactive only? Either answer is implementable; the current brief supports both and specifies neither. Remedy: State the ruling explicitly and, if detached seating is allowed, make it a per-run flag that can never default on while the operator is away.
- **F29-4** [major] `crates/c3/src/consult/detach.rs:1110`, `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:51` - The watch should not be a new command with its own exit-code space. `c3 consult --status` already reads the same status files and already owns 0/1/2 (`--kick` adds 3 for 'not acknowledged in 10 s'), so a separate `c3 watch` whose 3 means 'changed' reintroduces an ambiguity that no naming convention can repair; the honest minimal change is a `--since <seq>` flag on the existing `--status` surface over a monotonic append-only journal, which answers 'what changed' in the same command and the same code space. Trigger: A host wrapper or skill branches on c3 exit codes and runs watch alongside `consult --wait` or `--kick`. Evidence: read-code: The comment above `query` states the read-only query surface's exit codes are 0/1/2/3/4.; read-code: `--status` is documented 0 = done and usable, 1 = failure or died, 2 = still running; `--kick` exits 0 once acknowledged, 3 if not within 10 s - so 3 already means a timeout-ish outcome. Verify: Run RC1 and list every numeric exit code reachable from `c3 consult` and `c3-cli`; confirm no free code carries the 'changed' meaning unambiguously next to `--wait`'s timeout semantics. Remedy: Fold the watch into `c3 consult --status --since <seq>`, or, if a separate subcommand ships, reserve 10+ for it and document that watch codes never overlap consult codes. Supersedes: F20-1, F24-9, F27-3, F28-2.
- **F29-5** [blocker] `crates/c3/src/consult/detached.rs:274`, `crates/c3/src/consult/detach.rs:60` - A watch built on file writes, mtimes or `updated` stamps cannot soundly answer 'did anything change': a killed or crashed writer simply stops updating, so absence-of-write is reported exactly when a death occurred, and an atomic terminal replacement landing after the watch's read is lost. The judgement that already derives `died` and `elsewhere` from pid, start time and host must be the watch's source, and writers must publish a positive lease (a heartbeat with a monotonic sequence) so that a missing lease is itself reportable change. Trigger: A detached background or worker is tree-killed while its status still says `running`, or a status file names another host; the watch prints exit 0 (nothing). Evidence: read-code: `judgement` already returns `done|running|elsewhere|starting|died|never-started|unreadable` and derives `died` from pid + start_time on this host, `elsewhere` with exit 2 for a foreign host - the watch sketch instead diffs writes and `updated`.; read-code: `write_status` stamps `updated` on atomic replace, so the stamp is available but is not evidence of liveness. Verify: Run RC3: start a detached run, kill its background child out of band, and compare an mtime-based watch answer with `c3 consult --task <t> --status`; the watch must say died/unknown, never nothing. Remedy: Build the watch on the existing judgement over all open runs plus a shared monotonic journal (one append per state change, read by sequence), with a lease/heartbeat per writer so staleness is positive evidence rather than inferred absence. Supersedes: F19-1, F23-1, F24-2, F27-1, F28-1.

### Prior findings

- F03-1 - not-checked - quota.rs not read in this pass.
- F03-2 - still-open - Details in crates/c3/src/telemetry/event.rs still carries no effort field.
- F03-3 - not-checked - SubagentStop hook not exercised.
- F03-4 - not-checked - Scout digest design absent from this brief.
- F03-5 - still-open - No reworked class in any vocabulary in the brief.
- F04-1 - not-checked - Handoff 01 vs state.md outcome lists not diffed here.
- F04-2 - still-open - classes.rs:869-870 still returns None for any event_type other than consultation|rating.
- F04-3 - not-checked - Ledger quota attribution not tested.
- F04-4 - still-open - caps-v1 remains endpoint-local; zai maps medium->high.
- F04-5 - not-checked - Hook payload not dumped.
- F04-6 - still-open - Free-text note still present in the state.md worker-rate sketch.
- F04-7 - not-checked - No digest experiment run.
- F04-8 - not-checked - Requires intake cell data.
- F05-1 - not-checked - Quota incremental cost not measured.
- F05-2 - not-checked - Causal identification not tested.
- F05-3 - not-checked - SubagentStart/Stop histories not collected.
- F05-4 - not-checked - Resolved model identity not observed.
- F05-5 - still-open - ledger.rs keeps effort_requested/effort_sent/effort_mapping; the event carries none of them.
- F05-6 - not-checked - Test-command classifier not fed a failing fixture.
- F05-7 - not-checked - Call-cap judging not exercised.
- F07-1 - not-checked - Window-share recomputation not done.
- F07-2 - still-open - caps-v1 collapse still unfixed; no requested-tier field on the wire.
- F07-3 - not-checked - Hook payloads not sampled.
- F07-4 - not-checked - Join re-identification not attempted.
- F07-5 - not-checked - Falsification-budget experiment not run.
- F08-1 - not-checked - Quota ledger rounds not compared.
- F08-2 - still-open - event.rs Details has no effort key; ledger has effort_requested/effort_mapping.
- F08-3 - still-open - CAPS_VERSION is local only; Details is a fixed struct serialized whole.
- F08-4 - not-checked - Kill-mid-flight hook test not run.
- F08-5 - not-checked - Rework accounting not audited.
- F08-6 - still-open - Both free-text carriers still exist in the sketches.
- F08-7 - not-checked - Cell counts not obtained from the operator.
- F08-8 - not-checked - Scout digest replay not run.
- F10-1 - not-checked - Side-by-side calibration fields not searched for.
- F10-2 - still-open - No worker_role field exists in any sketch or struct.
- F11-1 - still-open - Confirmed: event.rs has no effort and no requested-tier field.
- F11-2 - not-checked - Ledger quota deltas not diffed.
- F11-3 - still-open - classes.rs:870 allowlist unchanged; a worker event returns None.
- F11-4 - still-open - --note still in the state.md sketch, contradicting its own closed-class rule.
- F11-5 - not-checked - Resume/stop doubling not observed.
- F11-6 - not-checked - Scout/judge family-correlation not tested.
- F12-1 - still-open - Verified at crates/c3/src/telemetry/classes.rs:870.
- F12-2 - still-open - Verified: crates/c3-core/src/roster.rs:486 allowlist has no effort/effort_policy key.
- F12-3 - still-open - Verified: crates/c3-core/src/effort.rs caps table has no per-purpose policy.
- F13-1 - still-open - No assignment/randomisation field in event.rs.
- F13-2 - not-checked - Cross-endpoint cost comparability not computed.
- F13-3 - not-checked - Digest fidelity not measured.
- F13-4 - not-checked - Host-kill backfill not observed.
- F13-5 - not-checked - Call-site effort override not diffed against CLI resolution.
- F13-6 - not-checked - README preset/roster duplication not inspected.
- F19-1 - still-open - Merged into finding #5; nothing in the brief adds silent-death detection.
- F19-2 - still-open - coordinate SKILL.md still states the wake's purpose is keeping the cache warm.
- F19-3 - still-open - Brief still names WASM-in-extension as the primary form.
- F19-4 - still-open - Native messaging still absent from the alternatives.
- F19-5 - still-open - No interactive state machine in the sketch.
- F19-6 - still-open - No key-management story for the catalogue.
- F20-1 - still-open - Superseded by finding #4 (merged exit-code claim).
- F20-2 - still-open - --next still event-only, uncapped by liveness.
- F20-3 - still-open - Foreign-host/elsewhere classification absent from the watch sketch.
- F20-4 - still-open - Brief line 49 still folds worker STATE tails.
- F20-5 - still-open - No signer identity, revocation or adapter hash key.
- F20-6 - still-open - No terminal classes for unsent/tab-closed/relay-lost.
- F20-7 - still-open - Draft-only still treated as a privacy boundary.
- F21-1 - still-open - Read-before-skip race and code-space collision unaddressed.
- F21-2 - still-open - --next still points at budget end/quota reset.
- F21-3 - still-open - Engine contract still unmatched by a browser member.
- F21-4 - still-open - Host API still omits bind/hash/token/audit/consent calls.
- F23-1 - still-open - Merged into finding #5.
- F23-2 - still-open - No crates/c3/src/engines/browser.rs exists yet; the deny-by-default gap stands.
- F23-3 - still-open - No jitter on --next hints.
- F24-1 - still-open - Scheduler out of scope; headline saving still unattainable in-scope.
- F24-2 - still-open - Merged into finding #5.
- F24-3 - still-open - Journal mechanism still unlisted; adopted in finding #4/#5 remedies.
- F24-4 - still-open - Bridge protocol still not the specified host API.
- F24-5 - still-open - Draft-mode retry still unaddressed.
- F24-6 - still-open - Relay/extension trust boundary still unspecified.
- F24-7 - still-open - No persisted browser run record.
- F24-8 - still-open - Panel serialization on one user still unmeasured.
- F24-9 - still-open - Superseded by finding #4.
- F25-1 - still-open - Detached + draft-only contradiction still unruled; framed in finding #3.
- F25-2 - still-open - No cache-TTL ceiling on --next.
- F25-3 - still-open - No OS supervision for a browser turn.
- F25-4 - still-open - Dynamic remote WASM still proposed as primary.
- F27-1 - still-open - Merged into finding #5.
- F27-2 - still-open - Watch-first wake economics unaddressed.
- F27-3 - still-open - Superseded by finding #4.
- F27-4 - still-open - STATE-tail injection path still in the sketch.
- F27-5 - still-open - Placement/capability still unproven; finding #2 offers the declarative alternative.
- F27-6 - still-open - No terminal classes for a browser turn.
- F27-7 - still-open - Catalogue trust root still unspecified.
- F27-8 - still-open - Draft-only privacy and panel availability still open.
- F27-9 - still-open - Both unlisted mechanisms still missing; finding #4/#5 adopt the journal.
- F28-1 - still-open - Merged into finding #5.
- F28-2 - still-open - Superseded by finding #4.
- F28-3 - still-open - Economy, TTL ceiling and jitter all still open.
- F28-4 - still-open - STATE-tail injection channel still in the sketch.
- F28-5 - still-open - MV3 feasibility still unresolved pending RC2.
- F28-6 - still-open - Native messaging still omitted from the alternatives.
- F28-7 - still-open - Browser vs subprocess contract still unmatched.
- F28-8 - still-open - Catalogue/bridge trust boundary still incomplete.
- F28-9 - still-open - Draft-only privacy and manual-send panels still open.

## Verdict: ADVISE

Panel B frames too narrow an option set and an unexamined cost model; the watch and the browser engine are both answerable, but two unlisted baselines and one ruling contradiction must be resolved first.

### Blockers

- **F29-5** `crates/c3/src/consult/detached.rs:274`, `crates/c3/src/consult/detach.rs:60` - A watch built on file writes, mtimes or `updated` stamps cannot soundly answer 'did anything change': a killed or crashed writer simply stops updating, so absence-of-write is reported exactly when a death occurred, and an atomic terminal replacement landing after the watch's read is lost. The judgement that already derives `died` and `elsewhere` from pid, start time and host must be the watch's source, and writers must publish a positive lease (a heartbeat with a monotonic sequence) so that a missing lease is itself reportable change. Verify: Run RC3: start a detached run, kill its background child out of band, and compare an mtime-based watch answer with `c3 consult --task <t> --status`; the watch must say died/unknown, never nothing. Remedy: Build the watch on the existing judgement over all open runs plus a shared monotonic journal (one append per state change, read by sequence), with a lease/heartbeat per writer so staleness is positive evidence rather than inferred absence.
- **F21-3** (prior, still-open) `crates/c3/src/engines/codex.rs:1` - A browser reviewer cannot satisfy the engine contract the sketch promises (brief in, structured reply out, transcript, kill record): there is no child to kill, and the dominant state 'draft prepared, user has not pressed send' has no timeout, no partial, and no owner in the current recovery chain.
- **F28-1** (prior, still-open) `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:48` - A wall-clock/mtime watch over status files and worker tails is not a sound change detector: it can report nothing for a dead detached background, for a foreign-host run whose liveness cannot be judged, or when an atomic terminal write lands after the read. Existing status judgement already derives died and elsewhere from pid, start time and host, so the watch must reuse that judgement and use a monotonic sequence cursor rather than timestamps.
- **F28-5** (prior, still-open) `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55` - Making a signed catalogue supply WASM adapters to the extension is not a viable primary form under current Chrome MV3 policy: remote WASM is remote-hosted code, and extension-page WASM requires the explicitly permitted `wasm-unsafe-eval` CSP. Adapter placement in C3 is therefore a feasibility constraint, not merely a preference.
- **F28-7** (prior, still-open) `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55` - A browser reviewer cannot honestly implement the current autonomous subprocess contract: timeout, stall and tree-kill assume a killable child, while draft-only's dominant state is waiting for a human and a tab or relay ending may abandon a stream. Without an interactive persisted state machine and terminal classes, `--wait`, `--kick`, budget stop and crash recovery are undefined.
- **F28-8** (prior, still-open) `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:57` - The signed catalogue and dumb bridge have no complete trust boundary: the signer, key rotation, revocation, extension/relay update pinning, adapter content identity and per-operation capability scope are unspecified. A table keyed only by adapter name plus vendor also re-labels history when an adapter changes, and a generic DOM bridge can drive or read beyond the catalogued vendor scope.

### Unproven scenarios

- Whether MV3 actually blocks the intended WASM instantiation in each context (content script, extension page, offscreen document) - not executed here.
- Whether a killed detached background leaves its status at `running` such that an mtime watch prints exit 0 - not executed here.
- The host's real prompt-cache TTL and whether a wake alone keeps the cache warm.
- Whether the vendor chats' terms permit any automation, and whether composer autosave transmits a draft before send.
- Whether the catalogue's signing key exists yet, and who holds it.
- Whether a browser member is ever seated detached or in a panel (operator ruling).
- Actual intake cell counts and the exit-code inventory of the c3 CLI.

### First-run checklist (observable)

_(none)_
