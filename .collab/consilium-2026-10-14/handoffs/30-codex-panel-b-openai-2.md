# Handoff 30 - Codex: panel-b-openai-2

Date: 2026-10-14 21:31 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 52253f0b-6ca6-45ab-86eb-8c69aafca86c.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-345c535772e04e199ceac8337847599a.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13be1-51df-7a03-bbb3-11b458533e3f` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` (sha256 a34288e066ed). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 276.5 s. Tokens: in 354962 (cached 275200), out 10349, reasoning 1055.
Quota (rollout): 5h 29% until 02:23; wk 85% until Oct 20; this run 354962 input tokens (275200 cached) in 6 calls, max 78932 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 85 % of the 5-hour window.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Proceed with a bounded watch and interactive browser prototype, but separate notification, execution, transport and recovery contracts before selecting the architecture. Findings: 0 blocker, 6 major, 1 minor, 0 note (F30-1..F30-7, tracked in `findings.json`). Structured reply: `handoffs/30-codex-panel-b-openai-2.reply.json`.
Raw event stream: `handoffs/30-codex-panel-b-openai-2.events.jsonl`.
Verbatim reply follows.

---

The framing combines independent decisions. A status reader cannot itself prevent a model invocation; adapter placement does not determine transport; draft preparation is an interactive workflow. Compare against existing background `--wait` plus Monitor and manual `c3 pack` plus paste before adding infrastructure.

**Q1.** Fold authoritative detached-run judgements, registered worker/build/peer completion records, unreadable or foreign-host states, and health changes that unblock this task. Do not copy worker prose or wake for unrelated ledger activity. Reuse `detached::judgement`, which already distinguishes `died`, `elsewhere` and `unreadable`.

Return a versioned result with `changed`, `unknown`, an observation cursor and a next-check deadline. A per-source revision vector is a smaller alternative to a global journal. Neither journal nor filesystem notification detects silent death without reconciliation. Subscribe before the initial scan and rescan after notification loss. A write after the observation belongs to the next cursor, rather than being inherently lost.

The cheapest reliable deadline is the earliest relevant lease/liveness check or explicit timeout; quota reset is only a reason to recheck availability. Cache refresh is a separate host policy. Measure actual cache lifetime and wake costs before preserving every heartbeat. Subcommand-specific exit codes are valid; use structured output to avoid wrapper ambiguity.

**Q2.** Prefer Rust/WASM in a bounded helper process controlled by C3, with the extension enforcing capabilities again at the DOM boundary. This preserves the operator’s WASM ruling while isolating adapter resource failure. Minimum operations: bind a user-selected tab/document/thread, prepare a bounded draft, observe submission, subscribe to the matched reply, request stop, release; automated send is a separate grant. No arbitrary script evaluation or unrestricted DOM reads.

Compare authenticated loopback with native messaging independently. Native messaging requires browser-initiated host attachment and origin validation; it is not automatic authentication of an existing C3 run. [Chrome documentation](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging).

Earlier CSP claims need correction: isolated content scripts permit WASM; main-world scripts inherit page CSP. Store restrictions on remotely supplied executable code are distinct from runtime feasibility. [Content scripts](https://developer.chrome.com/docs/extensions/develop/concepts/content-scripts), [remote code policy](https://developer.chrome.com/docs/extensions/develop/migrate/remote-hosted-code).

Choose after establishing distribution channel, installation tolerance, unattended-use requirement, adapter resource limits and actual web-only review volume. Declarative adapters remain an alternative requiring an explicit change to the primary-form ruling.

**Q3.** Persist run/attempt identity, adapter hash, pack/revision digest, tab/document/thread binding, consent scope, human-wait duration, observed submission, reply identity/offsets, partial output and terminal reason. Separate `awaiting-send`, `submission-unknown`, `streaming`, `disconnected` and terminal states. Never automatically resend an uncertain submission. Record stop intent before acting and remote cancellation confirmation separately; closing a pipe does not prove generation stopped. Apply a human-wait deadline separately from generation stall detection. Start with interactive single runs; unattended seating requires explicit capability policy.

**Q4.** Missing designs include a finite task supervisor instead of a permanent daemon, revision-vector watching, and the isolated adapter helper. Missing risks include accepting another turn’s valid JSON and giving a web reviewer only repository paths it cannot access. Reuse the HTTP pack boundary. Signing alone supplies neither resource limits nor review coverage.

## Requested checks

- **RC1:** Repository root; workspace-write: record cached-input and output usage for six matched status-first/watch-first wakes; establish whether invocation cost changes. Budget: one hour.
- **RC2:** Repository root; workspace-write: exercise a watch harness with silent death, foreign host, same-second writes and a write during subscription setup; require eventual delivery or explicit unknown. Budget: one hour.
- **RC3:** Repository root; workspace-write: use a fake browser bridge to disconnect after send but before acknowledgement, then replay an old schema-valid reply; require no resend and no false acceptance. Budget: 90 minutes.
- **RC4:** Repository root; workspace-write: run looping and memory-exhausting test adapters in the proposed helper; require bounded termination with C3 responsive and recovery recorded. Budget: 30 minutes.

---

### Findings

- **F30-1** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:48`, `crates/c3/src/consult/detached.rs:274`, `crates/c3/src/consult/detach.rs:60` - The watch lacks a defined observation and cursor contract. Timestamp-only comparisons can miss equal-timestamp changes and cannot infer silent death, but a write after a scan is not inherently lost if the returned cursor covers only observed state. A journal alone also cannot detect death, and independently updating status plus journal introduces a crash gap. Separate watch exit codes are not inherently invalid; ambiguity arises in callers that ignore which command produced them. Trigger: A writer dies without updating status, two replacements share a timestamp, a write occurs during scan/subscription setup, or a process crashes between updating status and appending its notification. Evidence: read-code: Status writes atomically replace the file and stamp updated with second-resolution wall-clock text.; read-code: Judgement derives unreadable, elsewhere and died independently of file changes.; read-code: The consult wait path returns 3 for a wait timeout; this establishes that command's meaning, not a universal prohibition on other subcommands using 3.; inferred: Neither cursor advancement nor atomicity between a proposed journal and existing status writes is specified. Verify: Run RC2 and check that every transition is eventually observed or explicitly classified unknown, including a crash between status publication and notification. Remedy: Specify snapshot/cursor semantics and command-scoped results; use per-source revisions or a transactional authoritative journal, reconcile liveness, detect cursor expiry, and subscribe before scanning. Treat notifications as hints unless their durable delivery is proven. Supersedes: F20-1, F21-1, F24-3, F24-9, F27-1, F27-3, F28-1, F28-2, F29-4, F29-5.
- **F30-2** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:57` - Extension adapter feasibility depends on execution context and distribution channel; treating all extension WASM as impossible or inherently unmediated is incorrect. The actual unresolved constraint is dynamically supplying executable catalogue adapters to a Store-distributed extension, alongside correctly configuring and limiting its execution context. Trigger: Architecture selection rejects bundled isolated-world WASM on page-CSP grounds, or assumes a successful unpacked-extension experiment establishes Store eligibility for downloaded adapters. Evidence: read-code: The brief specifies a private plugin first and signed catalogue WASM, without defining extension distribution or whether adapter bytes are bundled.; ran-command: Official documentation gives isolated content scripts a CSP containing wasm-unsafe-eval and distinguishes the main world, where page CSP applies.; ran-command: Official documentation identifies externally loaded JavaScript and WASM as remotely hosted code and describes the Store's bundling requirement. Verify: For the intended distribution channel, document whether executable bytes are bundled or fetched, then test the selected execution context separately from reviewing Store policy. Remedy: Retain B' as a preference based on update and capability boundaries, not a false universal CSP constraint. Explicitly select the distribution model; sandbox adapter imports regardless of placement. Supersedes: F19-3, F23-2, F25-4, F27-5, F28-5.
- **F30-3** [minor] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:56` - Native messaging is a useful omitted transport, but it does not automatically authenticate an existing consultation or prove remote cancellation. Chrome starts a native host; the sketch instead starts C3 first. Selecting native messaging therefore still requires a run-attachment protocol, content-script sender validation and explicit disconnect semantics. Trigger: A browser-launched host must attach to an already-running detached C3 consultation, or a port closes while the vendor continues generating. Evidence: read-code: C3 initiates the proposed relay and owns the consultation lifecycle.; ran-command: Chrome launches the host, communicates over framed stdio, limits host-to-browser messages, and explicitly requires validation of content-script senders before forwarding.; inferred: Transport closure establishes a local disconnect, not cessation of a remote vendor turn. Verify: Prototype browser-initiated attachment to two existing consultation IDs and require refusal of a mismatched run binding; disconnect one port and inspect the remote-turn status separately. Remedy: Compare native messaging and loopback using the same run binding, capability and recovery requirements; include native-host installation, framing and chunking costs. Supersedes: F19-4, F28-6.
- **F30-4** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55`, `crates/c3-core/src/engine.rs:981`, `crates/c3/src/engines/subprocess.rs:54` - The browser engine needs an interactive recovery capability, not necessarily an OS process tree. The common Engine interface already admits HTTP plans; the missing contract is persisted human waiting, uncertain submission, bounded retries and distinction between local abandonment and confirmed remote stop. Draft-only and detached execution are compatible only with an explicit attendance/deadline policy. Trigger: The user never sends, a repair needs another manual submission, the relay disappears, or a kick occurs while generation continues remotely. Evidence: read-code: Engine exposes planning, precheck and turn execution, and its plan documentation includes an HTTP request.; read-code: The HTTP engine explicitly has no subprocess.; read-code: Stall handling and durable kill callbacks belong to the subprocess runner.; read-code: The sketch promises the same recovery contract but does not specify human-wait states or remote cancellation evidence. Verify: Run RC3, adding unsent expiry, manual format repair, operator stop and restart; require one durable local terminal result per attempt while preserving unknown remote status. Remedy: Add interactive capabilities and state transitions above subprocess-specific hooks. Persist stop intent before action, distinguish attempted/confirmed/unknown cancellation, and give human waiting a separate bounded deadline. Supersedes: F19-5, F20-6, F21-3, F24-5, F24-7, F25-1, F25-3, F27-6, F28-7, F29-3.
- **F30-5** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:58` - The browser protocol lacks turn ownership and submission reconciliation. Schema-valid JSON from a previous or concurrent turn can be accepted as the current review, and reconnecting after an unacknowledged send can duplicate a submission. Output schema validation establishes shape, not provenance. Trigger: The user edits the draft, submits another message, regenerates a reply, navigates within the same tab, or the relay disconnects after send but before acknowledgement. Evidence: read-code: The listed operations prepare text, read a stream and signal done; no attempt binding, submission acknowledgement or message identity is defined.; read-code: The prior implementation persists a current task ID and requests replay on reconnect, confirming that reconnect identity is a real concern in this environment.; inferred: An old response can satisfy the same schema without answering the current brief. Verify: Run RC3 and require rejection of the replayed old reply and no automatic second send after the ambiguous acknowledgement. Remedy: Bind every operation to run, attempt, tab, document and thread; record the prepared and observed submitted payload identities locally. Require evidence linking the assistant turn to that submission. Use submission-unknown and operator reconciliation when idempotency cannot be established.
- **F30-6** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55`, `crates/c3/src/consult/orchestrate.rs:2665`, `crates/c3/src/consult/orchestrate.rs:4951` - The browser sketch specifies brief transport but omits the repository-evidence boundary. CLI reviewers can inspect local files; a web chat with the proposed DOM-only API cannot. Passing a normal path-based brief can therefore yield a structured review with no access to the evidence it was asked to assess. Trigger: A browser reviewer receives a brief referring to repository files or a commit range without their contents being included. Evidence: read-code: The implementation explicitly distinguishes repository-reading CLI engines from the HTTP engine's reviewer pack.; read-code: HTTP builds and retains a reviewer pack and records its hash in provider configuration.; read-code: Browser transport is specified as writing the brief; no pack construction or evidence-coverage contract is stated. Verify: Give a mock browser seat a brief naming one local file and inspect the prepared payload; require the file's relevant content and revision identity or an explicit incomplete-coverage refusal. Remedy: Reuse the sanitized HTTP pack path, retain its hash and source revision, and expose coverage limits to both reviewer and coordinator. Validate the exported context before composing the draft.
- **F30-7** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:72` - Running signed user-land adapters inside C3 without a resource-containment contract can disable the supervisor itself. A capability allowlist limits accessible operations but does not bound computation, memory, queued messages or blocking host calls. Trigger: An otherwise valid adapter loops indefinitely, allocates excessively, floods reply chunks or blocks in a permitted bridge operation. Evidence: read-code: The preference places adapters in C3's process and specifies signatures and denied capabilities, but no execution or resource limits.; inferred: An unbounded adapter shares availability with the component responsible for status and recovery. Verify: Run RC4 and verify that adapter failure cannot prevent status queries or durable recovery recording. Remedy: Use a supervised helper with bounded WASM memory and execution, bounded IPC queues and payloads, deadlines on host operations, and a killable JavaScript runtime if JavaScript adapters are enabled.

### Prior findings

- F03-1 - not-checked - Panel A cost comparison was not revalidated.
- F03-2 - not-checked - Requested-effort provenance was outside this review.
- F03-3 - not-checked - No Claude hook experiment performed.
- F03-4 - not-checked - No scout/judge replay performed.
- F03-5 - not-checked - Worker rework attribution was not inspected.
- F04-1 - not-checked - The two worker outcome sketches were not compared.
- F04-2 - still-open - Read the closer and Details: only consultation/rating types and fixed fields are accepted.
- F04-3 - not-checked - No quota-attribution experiment performed.
- F04-4 - not-checked - Effort mapping policy was not traced.
- F04-5 - not-checked - No worker start/stop payloads captured.
- F04-6 - not-checked - Worker rating and intake paths were not inspected.
- F04-7 - not-checked - No compression-loss experiment performed.
- F04-8 - not-checked - Observational scoring design was outside scope.
- F05-1 - not-checked - No isolated quota measurements performed.
- F05-2 - not-checked - No blind paired review experiment performed.
- F05-3 - not-checked - Worker lifecycle histories were not exercised.
- F05-4 - not-checked - Resolved worker models were not observed.
- F05-5 - not-checked - Effort provenance across launch and rating was not traced.
- F05-6 - not-checked - Verification classification was not inspected.
- F05-7 - not-checked - No digest-only known-defect test performed.
- F07-1 - not-checked - Cross-plan scoring was not recalculated.
- F07-2 - not-checked - No paired z.ai effort requests performed.
- F07-3 - not-checked - Worker telemetry payloads were not sampled.
- F07-4 - not-checked - No telemetry re-identification test performed.
- F07-5 - not-checked - Scout falsification coverage was not tested.
- F08-1 - not-checked - Quota denominator attribution was not independently checked.
- F08-2 - still-open - Details has no effort fields; the proposed distinction cannot currently reach the wire.
- F08-3 - not-checked - Intake compatibility and schema-version handling were not inspected.
- F08-4 - not-checked - No host-kill hook experiment performed.
- F08-5 - not-checked - Worker output survival was not traced.
- F08-6 - not-checked - Worker note and scout export paths were not inspected.
- F08-7 - not-checked - Intake sample sizes and rating bias were not measured.
- F08-8 - not-checked - No stale-digest or known-defect replay performed.
- F10-1 - not-checked - Calibration design was outside this brief.
- F10-2 - not-checked - Worker role metadata was not inspected; a role alone would not establish rework attribution.
- F11-1 - still-open - Read Details: effort and cached-token fields remain absent; requested-tier design still needs resolution.
- F11-2 - not-checked - No account-window attribution measurements performed.
- F11-3 - still-open - close_event still rejects event types other than consultation and rating.
- F11-4 - not-checked - The worker-note sketches were not compared.
- F11-5 - not-checked - No resumed-worker stop sequence captured.
- F11-6 - not-checked - No scout/judge defect replay performed.
- F12-1 - still-open - Read the explicit consultation/rating allowlist at classes.rs:870.
- F12-2 - still-open - Read roster validation: effort and effort_policy are absent from allowed entry keys.
- F12-3 - not-checked - Capability and policy responsibilities were not reassessed.
- F13-1 - not-checked - Assignment confounding was not measured.
- F13-2 - not-checked - Cross-engine cost scoring was outside this review.
- F13-3 - not-checked - No digest-only reproduction performed.
- F13-4 - not-checked - No crash/backfill hook experiment performed.
- F13-5 - not-checked - Call-site effort overrides were not observed.
- F13-6 - not-checked - Preset and roster policy precedence was not examined.
- F19-1 - still-open - Existing judgement derives death without a status write; updated timestamps alone are insufficient.
- F19-2 - still-open - The skill explicitly requires cache-warming wakes; actual TTL and economic benefit remain unmeasured.
- F19-3 - still-open - Corrected and replaced by finding #2; the blanket content-script CSP claim is unsupported.
- F19-4 - still-open - Corrected and replaced by finding #3; native messaging remains an option, not automatic run authentication.
- F19-5 - still-open - Narrowed and replaced by finding #4; processlessness does not itself violate Engine.
- F19-6 - still-open - The brief still omits signer custody, rotation and revocation.
- F20-1 - still-open - Replaced by finding #1; ambiguity is caller-specific, not a globally forbidden numeric reuse.
- F20-2 - still-open - The sketch still lacks bounded liveness reconciliation.
- F20-3 - still-open - Read elsewhere judgement: foreign-host liveness is explicitly unavailable.
- F20-4 - still-open - Worker tails remain proposed inputs; raw prose must not become trusted instructions or telemetry.
- F20-5 - still-open - Adapter identity and catalogue lifecycle remain unspecified.
- F20-6 - still-open - Merged into the corrected interactive recovery contract in finding #4.
- F20-7 - still-open - Pre-send disclosure and read scope remain unresolved risks; vendor network behaviour and terms were not tested.
- F21-1 - still-open - Corrected by finding #1: cursor semantics determine loss; a concurrent write is not automatically lost.
- F21-2 - still-open - Budget and quota deadlines are recheck reasons, not completion predictions.
- F21-3 - still-open - Replaced by finding #4; the common interface already accommodates a non-subprocess engine.
- F21-4 - still-open - Bridge scope remains unspecified; per-read round trips are avoidable through bounded subscriptions or batches.
- F23-1 - still-open - Silent-death detection remains absent from the watch sketch; indefinite waiting is not established for every caller.
- F23-2 - still-open - Corrected by finding #2; extension placement does not inherently grant WASM unrestricted DOM access.
- F23-3 - not-checked - No multi-coordinator load evidence; jitter is not a prerequisite at demonstrated scale.
- F24-1 - still-open - A tool call inside an already-invoked model cannot remove that invocation's input cost; RC1 measures magnitude.
- F24-2 - still-open - Confirmed the existing judgement path derives died from process identity.
- F24-3 - still-open - Corrected by finding #1: a journal needs atomic publication/reconciliation and cannot itself observe silent death.
- F24-4 - still-open - The extension must independently enforce run, document and origin capabilities.
- F24-5 - still-open - Merged into finding #4's interactive retry and deadline contract.
- F24-6 - still-open - Extension update trust and relay authentication remain unspecified; no relay penetration test performed.
- F24-7 - still-open - Corrected by finding #4: durable abandonment is required, but an OS process tree is not universal.
- F24-8 - still-open - Human-wait time remains an unresolved panel metric and scheduling concern.
- F24-9 - still-open - Replaced by finding #1's command-scoped exit contract.
- F25-1 - still-open - Replaced by finding #4: bounded interactive waiting can coexist with detach under an explicit policy.
- F25-2 - still-open - Cache policy is unresolved; a fixed 1200-second ceiling is not validated by this review.
- F25-3 - still-open - Corrected by finding #4: local transport termination and remote cancellation need distinct evidence.
- F25-4 - still-open - Corrected by finding #2; unpacked runtime success/failure cannot alone establish Store compliance.
- F27-1 - still-open - Replaced by finding #1's precise liveness and observation contract.
- F27-2 - still-open - Host invocation cost and cache lifetime remain measurement gates.
- F27-3 - still-open - Replaced by finding #1; command-scoped codes are legitimate.
- F27-4 - still-open - Untrusted worker prose must remain outside trusted watch summaries and telemetry.
- F27-5 - still-open - Corrected by finding #2's context/distribution distinction.
- F27-6 - still-open - Merged into finding #4's interactive recovery contract.
- F27-7 - still-open - Signer lifecycle, adapter identity and extension trust remain unspecified.
- F27-8 - still-open - Draft disclosure, read scope and human-wait accounting remain unresolved; no real vendor run performed.
- F27-9 - still-open - Both alternatives remain relevant; findings #1 and #3 qualify their guarantees.
- F28-1 - still-open - Corrected by finding #1; sequence cursors are one solution, not the only sound option.
- F28-2 - still-open - Corrected by finding #1; numeric meanings need not be globally unique across subcommands.
- F28-3 - still-open - Wake/cache economics remain unresolved; the bundled jitter requirement lacks scale evidence.
- F28-4 - still-open - The sketch still lacks a safe structured boundary for worker state.
- F28-5 - still-open - Corrected by finding #2; extension placement is not universally infeasible.
- F28-6 - still-open - Replaced by finding #3's qualified native-messaging option.
- F28-7 - still-open - Corrected and merged into finding #4.
- F28-8 - still-open - Catalogue lifecycle, update identity and operation scope remain unspecified.
- F28-9 - still-open - The disclosure and attendance risks remain; vendor-specific behaviours and terms were not checked.
- F29-1 - still-open - The coordinate skill confirms pack support for chat models; a complete paste/import workflow was not verified.
- F29-2 - still-open - Declarative adapters remain worth comparing, but require changing the primary-form ruling and are not proven strictly cheaper.
- F29-3 - still-open - Corrected by finding #4; the policies are underspecified rather than inherently mutually exclusive.
- F29-4 - still-open - Corrected by finding #1; a separate watch command is valid with explicit command-scoped semantics.
- F29-5 - still-open - Corrected by finding #1; positive leases are useful but not mandatory where reliable local process reconciliation exists.

## Verdict: ADVISE

Proceed with a bounded watch and interactive browser prototype, but separate notification, execution, transport and recovery contracts before selecting the architecture.

### Blockers

- **F21-3** (prior, still-open) `crates/c3/src/engines/codex.rs:1` - A browser reviewer cannot satisfy the engine contract the sketch promises (brief in, structured reply out, transcript, kill record): there is no child to kill, and the dominant state 'draft prepared, user has not pressed send' has no timeout, no partial, and no owner in the current recovery chain.
- **F28-1** (prior, still-open) `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:48` - A wall-clock/mtime watch over status files and worker tails is not a sound change detector: it can report nothing for a dead detached background, for a foreign-host run whose liveness cannot be judged, or when an atomic terminal write lands after the read. Existing status judgement already derives died and elsewhere from pid, start time and host, so the watch must reuse that judgement and use a monotonic sequence cursor rather than timestamps.
- **F28-5** (prior, still-open) `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55` - Making a signed catalogue supply WASM adapters to the extension is not a viable primary form under current Chrome MV3 policy: remote WASM is remote-hosted code, and extension-page WASM requires the explicitly permitted `wasm-unsafe-eval` CSP. Adapter placement in C3 is therefore a feasibility constraint, not merely a preference.
- **F28-7** (prior, still-open) `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55` - A browser reviewer cannot honestly implement the current autonomous subprocess contract: timeout, stall and tree-kill assume a killable child, while draft-only's dominant state is waiting for a human and a tab or relay ending may abandon a stream. Without an interactive persisted state machine and terminal classes, `--wait`, `--kick`, budget stop and crash recovery are undefined.
- **F28-8** (prior, still-open) `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:57` - The signed catalogue and dumb bridge have no complete trust boundary: the signer, key rotation, revocation, extension/relay update pinning, adapter content identity and per-operation capability scope are unspecified. A table keyed only by adapter name plus vendor also re-labels history when an adapter changes, and a generic DOM bridge can drive or read beyond the catalogued vendor scope.
- **F29-5** (prior, still-open) `crates/c3/src/consult/detached.rs:274` - A watch built on file writes, mtimes or `updated` stamps cannot soundly answer 'did anything change': a killed or crashed writer simply stops updating, so absence-of-write is reported exactly when a death occurred, and an atomic terminal replacement landing after the watch's read is lost. The judgement that already derives `died` and `elsewhere` from pid, start time and host must be the watch's source, and writers must publish a positive lease (a heartbeat with a monotonic sequence) so that a missing lease is itself reportable change.

### Unproven scenarios

- No watch or browser prototype was executed, and no files were changed.
- Host cache TTL, cache refresh behaviour and per-wake token savings were not measured.
- Chrome documentation was checked, but extension runtime contexts, Store review and native-host installation were not exercised.
- The prior gemini-mcp extension was inspected selectively, not audited for security or correctness.
- No vendor composer network behaviour, automation terms or remote cancellation semantics were tested.
- Most Panel A findings were outside this brief and are explicitly marked not-checked.
- Superseded findings retain still-open status because the underlying design decision remains unresolved; their original overbroad claims are not endorsed.

### First-run checklist (observable)

_(none)_
