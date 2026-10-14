# Handoff 27 - Codex: panel-b-mimo

Date: 2026-10-14 20:35 local. Author: Codex (model mimo-v2.6-pro, effort high), Codex CLI 0.155.1.
Reviewer: mimo :: mimo-v2.6-pro (provider from -Provider, model from roster; endpoint https://token-plan-ams.xiaomimimo.com/v1, wire_api: responses; provider fingerprint 47cd6ee7e4ff; harness codex-cli 0.155.1).
Preflight: ok: env MIMO_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 3 of 12 for -Provider mimo (model, codex_config applied).
Effort: high sent (requested high, mapping mimo-v1, by caps-v1: token-plan-ams.xiaomimimo.com, mimo-v2.6-pro; not confirmed by the provider). Consultation id: 4568674b-7341-4afe-be08-e65d4ed1d18a.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `codex exec --sandbox read-only --color never --json -m mimo-v2.6-pro -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="mimo" -c model_catalog_json="C:/Users/Dmytro/.codex/model-catalogs.json" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-29af2d1d9f164ec0ae25836837463bad.md -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13ba4-b3a7-7ad0-b47f-e4ed77dfe39e` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` (sha256 a34288e066ed). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 916.9 s. Tokens: in 693769 (cached 190976), out 20896, reasoning 5497.
Quota (rollout): no window in the rollout; this run 693769 input tokens (190976 cached) in 7 calls, max 122944 per call of 996147; sub-agents 0; sub-agents off (features.multi_agent=false); budget 85 % of the 5-hour window.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - The architecture is promising, but the watch liveness and cache contract and the browser CSP, recovery and trust model must be resolved before implementation. Findings: 0 blocker, 5 major, 4 minor, 0 note (F27-1..F27-9, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/27-codex-panel-b-mimo.reply.json`.
Raw event stream: `handoffs/27-codex-panel-b-mimo.events.jsonl`.
Verbatim reply follows.

---

## Framing challenge
The sketch assumes a watch is a file diff and a browser reviewer can satisfy the existing engine contract. Both are contestable. I would choose only after knowing the host cache TTL and Monitor semantics, the detached-record schema and pid liveness rules, the reserved exit-code space, MV3 CSP per context, vendor composer network behavior, and the vendor automation terms. Omitted options include native messaging, an append-only event journal, a host push daemon, and an attended-manual flow outside panels.

## Q1
Fold a monotonic event sequence (status writes, terminal and kill records, ledger and health appends), each run state plus pid/start_time/host and freshness deadline, and worker state only as closed tokens and file names. The cheapest reliable next hint is the minimum of a liveness deadline, the cache-TTL ceiling, and known budget or reset events; never quota reset alone. A skipped wake loses a terminal write that lands after the read, a silently dead worker, a foreign-host run (must be unknown), and the warm cache if the gap exceeds TTL.

## Q2
Run adapters in C3 (B') with a dumb extension bridge. Extension-side WASM is not safe to assume: content scripts face page CSP, extension pages need wasm-unsafe-eval, and dynamic remote executable code is restricted. The minimal deny-by-default API is bind(tab, origin, adapter-hash, run-token), open or prepare, compose, scoped read of one thread stream, explicit send, done or abort, audit and rate-limit. Every call needs a per-run capability and origin allowlist; generic DOM query or click is too much.

## Q3
Record states created, drafting, drafted, awaiting-user-send, streaming, replied, partial, relay-lost, tab-closed, not-sent, abandoned, timeout and killed, with timestamps, tab and origin, adapter hash, consent, schema validation, retries, liveness, and a durable kill or abandon record. User did not press send is a terminal outcome, not a hang; tab close or relay loss salvages a partial.

## Q4
The biggest omitted risk is trust and lifecycle: native messaging may remove the bearer-token relay, while a journal may replace mtime heuristics. Also decide who signs and revokes adapters, whether composer_put uploads before send, how read scopes are limited, and whether manual-send wait time is excluded from reviewer quality.

## Requested checks
- **RC1** (workspace-write, 30 min): MV3 spike in content script, extension page and offscreen document; record WebAssembly.instantiate results (finding #5).
- **RC2** (workspace-write, 20 min): kill a detached background mid-run; compare watch output with c3 consult --status (finding #1).
- **RC3** (read-only, 15 min): inventory all c3 exit codes and run a wrapper using wait plus watch (finding #3).
- **RC5** (workspace-write, 30 min): DevTools capture of draft-only composer_put and a two-seat panel wait-time log (finding #8).

---

### Findings

- **F27-1** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:48`, `crates/c3/src/consult/detached.rs:274`, `crates/c3/src/consult/detach.rs:60` - The proposed watch answers changed-since from writes and mtimes, so it reports quiet for a worker or detached background that dies silently; it also has a read-before-skip race and treats foreign-host runs as quiet instead of unknown. Trigger: A process is tree-killed or crashes while its status record remains running, or a terminal write lands after the watch read. Evidence: read-code: judgement derives died only from pid liveness after reading a record; elsewhere is explicitly uncheckable.; read-code: write_status atomically replaces the record and stamps updated, but there is no event journal or liveness deadline folded into a change answer.; inferred: the sketch folds file tails and prints nothing when no file changed, which cannot distinguish death from inactivity. Verify: RC1: start a detached run, kill its background child out of band, then run the proposed watch and c3 consult --status; require the watch to report died/changed, never nothing. Remedy: Use a monotonic append-only event journal plus pid/start_time liveness, staleness deadlines, and explicit unknown for foreign hosts; make nothing only after those checks. Supersedes: F19-1, F20-2, F20-3, F21-1, F23-1, F24-2.
- **F27-2** [major] `plugin/skills/coordinate/SKILL.md:27`, `plugin/skills/coordinate/SKILL.md:38`, `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:53` - A watch-first wake does not by itself save the expensive cached read, and a next hint can outlive the prompt-cache TTL or point at a noisy quota reset; the design needs a cache ceiling and a real push or Monitor path. Trigger: A long quiet detached run schedules the next wake beyond the cache lifetime, or the coordinator wakes at a quota reset after the run already finished. Evidence: read-code: the skill says the next request rewrites the whole context after the cache lifetime and keeps a roughly 1200-second heartbeat.; inferred: the brief defers the push or daemon alternative while promising skipped wakes from a command. Verify: RC2: instrument watch-first and status-first wakes and measure cached input tokens; compare next hints with actual completions over one wave. Remedy: Treat watch as an action filter, retain a cache-TTL heartbeat ceiling, and add an event push or Monitor-backed journal; jitter and liveness-bound all hints. Supersedes: F19-2, F21-2, F23-3, F24-1, F25-2.
- **F27-3** [minor] `crates/c3/src/consult/detached.rs:274`, `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:51` - Watch exit 3 for changed collides with the existing consult convention where 3 means wait timeout and 4 means refusal, so shared host scripts can misread one outcome as the other. Trigger: A coordinator wrapper branches on c3 exit codes and runs watch alongside wait. Evidence: read-code: the detached query surface documents 0/1/2/3/4.; inferred: the sketch assigns 3=changed without reserving a disjoint namespace. Verify: RC3: inventory every exit code in c3 consult and cli, then run a wrapper using wait plus watch; require unambiguous codes. Remedy: Give watch a distinct subcommand namespace and codes and document them separately. Supersedes: F20-1, F21-1, F24-9.
- **F27-4** [minor] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:49` - Folding worker STATE tails into one watch line creates a prompt-injection and privacy channel: worker-written free text can instruct the coordinator and can leak into handoffs or telemetry. Trigger: A worker writes instruction-like prose in STATE, or a watch line is copied into an event. Evidence: read-code: the sketch explicitly names the tails of worker STATE files as input to the printed line.; inferred: the closed-class telemetry contract has no free-text field for such content. Verify: Feed a STATE fixture containing an instruction through the watch formatter and grep the output and event constructors for it. Remedy: Emit closed tokens and file names only; keep raw STATE text local and never telemeter it. Supersedes: F20-4, F08-6.
- **F27-5** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55`, `crates/c3/src/engines/subprocess.rs:43` - WASM-in-extension is not a safe default: content scripts face the page CSP and may not instantiate WASM, remote or dynamic executable code is restricted under MV3, and B' still needs a narrowly scoped DOM bridge. Placement and capability scope must be proven before choosing. Trigger: An adapter runs in a content script or the extension fetches catalogue bytes dynamically. Evidence: assumed: not tested here; requires the RC1 spike for content script versus extension page and offscreen contexts.; read-code: the existing contract assumes a spawned, killable process with timeout, stall and kill hooks. Verify: RC1: run a scratch MV3 extension spike in each context and record whether WebAssembly.instantiate succeeds and under which CSP. Remedy: Prefer adapters in C3 with a dumb bridge; bind every bridge call to tab, origin, adapter hash and per-run capability token, and deny by default. Supersedes: F19-3, F23-2, F24-4, F25-4.
- **F27-6** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:57`, `crates/c3/src/engines/subprocess.rs:40` - A browser engine has no process tree to kill and no state for draft awaiting user send; without explicit terminal classes it will report human waiting as a stall, block wait forever, or lose a partial after tab close or relay loss. Trigger: The user never presses send, closes the tab mid-stream, or C3 or the relay crashes. Evidence: read-code: timeout, stall and tree-kill are defined for a child process.; inferred: the sketch calls do not name draft, awaiting-send, abandoned or partial states. Verify: Walk unsent, tab-closed, relay-lost and truncated-reply endings; each must write exactly one terminal record and return from wait. Remedy: Define a browser run state machine and recovery record covering drafted, awaiting-send, streaming, relay-lost, not-sent, partial, replied and killed; never arm a stall timer while awaiting the user. Supersedes: F19-5, F20-6, F21-3, F24-5, F24-7, F25-1, F25-3.
- **F27-7** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:56` - The signed catalogue has no trust root or lifecycle: signer identity, key rotation, revocation, adapter content hash in rows, and extension or relay update pinning are unspecified. Trigger: An adapter or extension is updated, or a catalogue signing key is compromised. Evidence: inferred: the ruling says signed hash or pointer catalogue but names no signer, revocation or hash-keyed telemetry.; assumed: extension update provenance and native messaging lifecycle were not inspected. Verify: RC3: ask who signs the catalogue, test revocation of one hash, and confirm intake rows key on adapter content hash. Remedy: Name the signer and rotation or revocation process, pin the extension and relay, and key measurements on adapter hash plus catalogue version. Supersedes: F19-6, F20-5, F24-6.
- **F27-8** [minor] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:58`, `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:61` - Draft-only is not a privacy boundary and panels of manual-send reviewers measure user availability, not reviewer quality; composer autosave and broad read scopes can transmit or expose the brief before send. Trigger: A vendor editor autosaves, read_reply is granted page-wide, or several draft-only seats run concurrently. Evidence: assumed: network behavior of vendor composers and page reach of the prior extension were not inspected.; inferred: manual send latency is not separated from reviewer quality in the proposed table. Verify: RC5: capture network during composer_put before send and audit read capability reach; seat two draft-only members and log waiting-for-user time. Remedy: Scope reads to one thread, verify no pre-send upload where possible, obtain per-adapter consent, and report user-wait separately. Supersedes: F20-7, F24-8.
- **F27-9** [minor] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:62` - Two unlisted mechanisms could materially change the decision: Chrome native messaging as the C3 to extension transport, and an append-only closed-vocabulary event journal as the watch source. Trigger: Threat-modeling bearer tokens and loopback trust, or needing cross-file ordering and silent-death detection. Evidence: inferred: the brief lists WebSocket relay and watch or serve, but neither native messaging nor a journal is evaluated.; read-code: status writes already provide a natural append point for a journal. Verify: Prototype one journal append and read plus a native-messaging handshake; compare security surface and concurrency with the relay. Remedy: Evaluate both before locking the transport and watch format; choose the smallest auditable trust boundary. Supersedes: F19-4, F24-3.

### Prior findings

- F03-1 - not-checked - Outside this watch/browser framing; not re-verified.
- F03-2 - not-checked - Outside this watch/browser framing; not re-verified.
- F03-3 - not-checked - Outside this watch/browser framing; not re-verified.
- F03-4 - not-checked - Outside this watch/browser framing; not re-verified.
- F03-5 - not-checked - Outside this watch/browser framing; not re-verified.
- F04-1 - not-checked - Outside this watch/browser framing; not re-verified.
- F04-2 - not-checked - Outside this watch/browser framing; not re-verified.
- F04-3 - not-checked - Outside this watch/browser framing; not re-verified.
- F04-4 - not-checked - Outside this watch/browser framing; not re-verified.
- F04-5 - not-checked - Outside this watch/browser framing; not re-verified.
- F04-6 - not-checked - Outside this watch/browser framing; not re-verified.
- F04-7 - not-checked - Outside this watch/browser framing; not re-verified.
- F04-8 - not-checked - Outside this watch/browser framing; not re-verified.
- F05-1 - not-checked - Outside this watch/browser framing; not re-verified.
- F05-2 - not-checked - Outside this watch/browser framing; not re-verified.
- F05-3 - not-checked - Outside this watch/browser framing; not re-verified.
- F05-4 - not-checked - Outside this watch/browser framing; not re-verified.
- F05-5 - not-checked - Outside this watch/browser framing; not re-verified.
- F05-6 - not-checked - Outside this watch/browser framing; not re-verified.
- F05-7 - not-checked - Outside this watch/browser framing; not re-verified.
- F07-1 - not-checked - Outside this watch/browser framing; not re-verified.
- F07-2 - not-checked - Outside this watch/browser framing; not re-verified.
- F07-3 - not-checked - Outside this watch/browser framing; not re-verified.
- F07-4 - not-checked - Outside this watch/browser framing; not re-verified.
- F07-5 - not-checked - Outside this watch/browser framing; not re-verified.
- F08-1 - not-checked - Outside this watch/browser framing; not re-verified.
- F08-2 - not-checked - Outside this watch/browser framing; not re-verified.
- F08-3 - not-checked - Outside this watch/browser framing; not re-verified.
- F08-4 - not-checked - Outside this watch/browser framing; not re-verified.
- F08-5 - not-checked - Outside this watch/browser framing; not re-verified.
- F08-6 - not-checked - Outside this watch/browser framing; not re-verified.
- F08-7 - not-checked - Outside this watch/browser framing; not re-verified.
- F08-8 - not-checked - Outside this watch/browser framing; not re-verified.
- F10-1 - not-checked - Outside this watch/browser framing; not re-verified.
- F10-2 - not-checked - Outside this watch/browser framing; not re-verified.
- F11-1 - not-checked - Outside this watch/browser framing; not re-verified.
- F11-2 - not-checked - Outside this watch/browser framing; not re-verified.
- F11-3 - not-checked - Outside this watch/browser framing; not re-verified.
- F11-4 - not-checked - Outside this watch/browser framing; not re-verified.
- F11-5 - not-checked - Outside this watch/browser framing; not re-verified.
- F11-6 - not-checked - Outside this watch/browser framing; not re-verified.
- F12-1 - not-checked - Outside this watch/browser framing; not re-verified.
- F12-2 - not-checked - Outside this watch/browser framing; not re-verified.
- F12-3 - not-checked - Outside this watch/browser framing; not re-verified.
- F13-1 - not-checked - Outside this watch/browser framing; not re-verified.
- F13-2 - not-checked - Outside this watch/browser framing; not re-verified.
- F13-3 - not-checked - Outside this watch/browser framing; not re-verified.
- F13-4 - not-checked - Outside this watch/browser framing; not re-verified.
- F13-5 - not-checked - Outside this watch/browser framing; not re-verified.
- F13-6 - not-checked - Outside this watch/browser framing; not re-verified.
- F19-1 - still-open - Still open; covered or challenged by the framing findings.
- F19-2 - still-open - Still open; covered or challenged by the framing findings.
- F19-3 - still-open - Still open; covered or challenged by the framing findings.
- F19-4 - still-open - Still open; covered or challenged by the framing findings.
- F19-5 - still-open - Still open; covered or challenged by the framing findings.
- F19-6 - still-open - Still open; covered or challenged by the framing findings.
- F20-1 - still-open - Still open; covered or challenged by the framing findings.
- F20-2 - still-open - Still open; covered or challenged by the framing findings.
- F20-3 - still-open - Still open; covered or challenged by the framing findings.
- F20-4 - still-open - Still open; covered or challenged by the framing findings.
- F20-5 - still-open - Still open; covered or challenged by the framing findings.
- F20-6 - still-open - Still open; covered or challenged by the framing findings.
- F20-7 - still-open - Still open; covered or challenged by the framing findings.
- F21-1 - still-open - Still open; covered or challenged by the framing findings.
- F21-2 - still-open - Still open; covered or challenged by the framing findings.
- F21-3 - still-open - Still open; covered or challenged by the framing findings.
- F21-4 - still-open - Still open; covered or challenged by the framing findings.
- F23-1 - still-open - Still open; covered or challenged by the framing findings.
- F23-2 - still-open - Still open; covered or challenged by the framing findings.
- F23-3 - still-open - Still open; covered or challenged by the framing findings.
- F24-1 - still-open - Still open; covered or challenged by the framing findings.
- F24-2 - still-open - Still open; covered or challenged by the framing findings.
- F24-3 - still-open - Still open; covered or challenged by the framing findings.
- F24-4 - still-open - Still open; covered or challenged by the framing findings.
- F24-5 - still-open - Still open; covered or challenged by the framing findings.
- F24-6 - still-open - Still open; covered or challenged by the framing findings.
- F24-7 - still-open - Still open; covered or challenged by the framing findings.
- F24-8 - still-open - Still open; covered or challenged by the framing findings.
- F24-9 - still-open - Still open; covered or challenged by the framing findings.
- F25-1 - still-open - Still open; covered or challenged by the framing findings.
- F25-2 - still-open - Still open; covered or challenged by the framing findings.
- F25-3 - still-open - Still open; covered or challenged by the framing findings.
- F25-4 - still-open - Still open; covered or challenged by the framing findings.

## Verdict: ADVISE

The architecture is promising, but the watch liveness and cache contract and the browser CSP, recovery and trust model must be resolved before implementation.

### Blockers

- **F21-3** (prior, still-open) `crates/c3/src/engines/codex.rs:1` - A browser reviewer cannot satisfy the engine contract the sketch promises (brief in, structured reply out, transcript, kill record): there is no child to kill, and the dominant state 'draft prepared, user has not pressed send' has no timeout, no partial, and no owner in the current recovery chain.

### Unproven scenarios

- MV3 CSP behavior for WASM in each extension context.
- Native messaging concurrency and lifecycle on Windows.
- Vendor composer autosave and network behavior and automation terms.
- The host effective prompt-cache TTL and Monitor wake semantics.

### First-run checklist (observable)

_(none)_
