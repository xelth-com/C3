# Handoff 19 - Codex: panel-b-watch-browser-kimi

Date: 2026-10-14 20:12 local. Author: Codex (model k3, effort high), Codex CLI 0.155.1.
Reviewer: kimi :: k3 (provider from roster, model from roster; endpoint https://api.kimi.ai/coding/v1, wire_api: responses; provider fingerprint 8f7901d404b5; harness codex-cli 0.155.1).
Preflight: ok: env KIMI_API_KEY set.
Effort: high sent (requested high, mapping kimi-v1, by caps-v1: api.kimi.ai, k3; not confirmed by the provider). Consultation id: 63ee5f31-4113-4882-9eb4-6afb3bd09505.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `codex exec --sandbox read-only --color never --json -m k3 -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="kimi" -c model_context_window=256000 -c model_auto_compact_token_limit=204800 -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-fa607586516d4cab9abb25ebcc53a5c5.md -` (prompt on stdin).
Parent thread: (none - new thread; newest thread of lineage kimi :: k3 (consult n=1)). Result thread: `01a13b97-fd0c-7e90-ae58-fedae7c886eb` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` (sha256 a34288e066ed). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 350.8 s. Tokens: in 376608 (cached 258560), out 9265, reasoning 4426.
Quota (rollout): no window in the rollout; this run 376608 input tokens (258560 cached) in 6 calls, max 66135 per call of 243200; sub-agents 0; sub-agents off (features.multi_agent=false); budget 85 % of the 5-hour window.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Framing consultation: both sketches are directionally sound, but the watch framing ignores the wake's cache-warming job and dead-writer silence, and the browser engine needs an interactive-engine contract (plus a native-messaging transport option) rather than the autonomous subprocess contract. Findings: 0 blocker, 3 major, 2 minor, 1 note (F19-1..F19-6, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/19-codex-panel-b-watch-browser-kimi.reply.json`.
Raw event stream: `handoffs/19-codex-panel-b-watch-browser-kimi.events.jsonl`.
Verbatim reply follows.

---

## Framing challenges
- (A) is framed as "skip useless wakes", but the coordinate skill gives the wake a second job: keeping the prompt cache warm. A Monitor that replaces the periodic wake *entirely* removes that job and re-creates the cold-cache rewrite the rule exists to avoid. The saving is real only while a minimal wake still fires inside the cache TTL.
- (B) assumes "the same contract as the others", but draft-only makes the run non-autonomous: the stall/timeout/kill semantics of the subprocess contract do not map. Model `browser` as an interactive-engine subclass, not a fourth subprocess engine.
- Missing option for (B): Chrome **native messaging** instead of a loopback WebSocket relay (finding #4).

## Q1
Fold: detached status records (state + `updated` stamp, writes already atomic), ledger newest entry id, health-journal tail, named worker STATE file mtimes, pending kick files, and the quota reset times. Cheapest reliable next-known-event: the earliest of detached budget end, quota reset (`5h ... until 18:32`), and any named cooldown end (`retry_after`) — all already written down; never synthesize it from worker liveness. Where a skipped wake loses something: (a) a dead worker stops writing, so silence must fold as `changed` once a STATE file is stale-but-open (finding #1); (b) the cache expires, so cap consecutive skips by the cache TTL (finding #2); (c) an operator kick file must never be skipped. Watermark on the status records' `updated` stamps, not wall clock.

## Q2
Run the adapter in C3's process (B'), extension as a dumb signed DOM bridge — and for a harder reason than the brief gives: WASM in an MV3 content script is blocked by page CSP, and extension pages/offscreen documents need `wasm-unsafe-eval`, so "WASM in the extension" is likely infeasible exactly where the adapter must touch the DOM (finding #3). Minimal host API, extension side, deny-by-default per adapter: `open(url)`, `set_composer(text)`, `read_reply()` (opaque text/stream), `send()` (armed only by the per-adapter user grant), `ping()`. Parsing, pacing, output-schema validation and the signed catalogue all stay in C3, matching the other engines. To choose definitively I need: a spike confirming MV3 CSP behavior for WASM in each extension context, and whether per-adapter send consent needs extension-rendered UI (badge/popup) that C3 cannot render.

## Q3
Record: adapter id + catalogue hash + version; relay/native-messaging session establishment (peer, origin); `draft-written` at T; `awaiting-user-send` as a first-class, unbounded state that is *not* a stall; `sent {by: user|adapter}`; stream growth for partial salvage; `relay-lost` with last stream offset; kill = token revoke + pipe close + a written record (the user's tab itself is never killable). A reply that never comes because the user did not press send closes as `not-sent`, a distinct outcome class from `error` — and the waiting rule must not count a drafted run as running work forever (finding #5).

## Q4
Findings #1–#6. The largest: stale-silence on the watch (#1), the cache-warming contradiction in the Monitor path (#2), native messaging as the unlisted transport (#4), and the interactive contract with a `not-sent` class (#5). Catalogue signing also needs a key-rotation/revocation story before even the private plugin ships (#6).

## Requested checks
- RC1 (finding #3): In a scratch dir, load an unpacked MV3 extension whose content script (and, separately, an offscreen document) instantiates WebAssembly on a chat page; observe whether CSP refuses. Permission: workspace-write, scratch only. Budget: 1 h.
- RC2 (findings #1, #5): `c3 consult --detach` a throwaway run, kill the background child mid-turn, then diff the status file's mtime against its last `updated` stamp; observe whether any terminal record ever appears (cwd: repo root; workspace-write; one run).

---

### Findings

- **F19-1** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md`, `crates/c3/src/consult/detach.rs:55` - The watch's 'nothing since <instant>' answer is unsound for writers that die silently: a dead worker or killed detached background simply stops updating its STATE/status file, so absence of change is reported exactly when a death occurred; the fix is cheap because status writes already stamp `updated` atomically. Trigger: A worker process dies or is tree-killed between wakes; its STATE file's last mtime predates <instant> or sits unchanged with no terminal line, and the watch prints exit 0. Evidence: read-code: Every status write is an atomic replace that stamps `updated`; a dead writer therefore leaves a permanently stale stamp with no terminal record - detectable, but only if the watch treats stale-but-open as changed.; read-code: The waiting rule polls state-file tails; nothing in the sketched watch distinguishes 'quiet' from 'dead'. Verify: RC2: kill a detached run's background child mid-turn and check whether `c3 watch --since` before the kill reports exit 0 (nothing) - if so the defect is confirmed. Remedy: Fold staleness: a STATE/status file with no terminal record and an `updated` stamp older than the writer's expected heartbeat (or older than the wake interval) must yield `changed: <name> silent since <stamp>`, exit 3; also fold pending kick files so they are never skipped.
- **F19-2** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md`, `plugin/skills/coordinate/SKILL.md` - The sketch's claim that a Monitor on the status directory 'can replace the periodic wake entirely' contradicts the coordinate skill's stated purpose of the wake: keeping the prompt cache warm. Removing all wakes lets the context go cold and the next real interaction pays a full cache rewrite - the watch saves the read but forfeits the warm cache. Trigger: A host with a file Monitor adopts the event-driven path, no wake fires for longer than the cache TTL, and the next changed-event wake re-writes the whole large context. Evidence: read-code: 'Wait without blocking; keep the prompt cache warm... a large context must not go cold by oversight' - cache warming is an explicit function of the 20-minute wake, independent of change detection.; inferred: The sketch quantifies savings per skipped wake but never accounts for the cold-cache rewrite when wakes stop entirely. Verify: Ask the operator for the host's effective prompt-cache TTL and confirm whether a one-line `c3 watch` invocation alone is sufficient traffic to keep the cache warm. Remedy: State the invariant in the design: wakes may be reduced to the one-line watch but never eliminated below one per cache TTL; the `--next` hint must be clamped to that ceiling.
- **F19-3** [minor] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` - The brief's primary form 'adapters compiled to WASM running inside the Chrome extension' is likely infeasible in the context that touches the page DOM: MV3 content scripts are subject to the page's CSP and cannot instantiate WASM, and extension pages/offscreen documents require the 'wasm-unsafe-eval' CSP directive. This strengthens the B' preference but should be stated as a hard constraint, not a preference. Trigger: Implementing the WASM-in-extension form and discovering the adapter cannot run in a content script on the chat page. Evidence: assumed: Content-script WASM is blocked by page CSP; 'wasm-unsafe-eval' is required for extension contexts - spike needed to confirm per-context behavior. Verify: RC1: the MV3 WASM spike in a scratch extension, per context type (content script, extension page, offscreen document). Remedy: Adopt B' on the CSP constraint; if WASM in the extension is ever revisited, restrict it to an offscreen document with an explicit CSP entry and bridge to the DOM via messaging.
- **F19-4** [minor] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` - The alternatives list omits Chrome native messaging as the C3<->extension transport: the browser launches C3 (or a shim) and speaks stdio, giving per-run authentication, no loopback socket, no per-run token, no Origin checks, and kill = close the pipe - strictly less security surface than the sketched WebSocket relay. Trigger: Threat-modeling the relay: any local process that learns the per-run token can connect; native messaging has no such bearer token at all. Evidence: inferred: Every safeguard the sketch adds to the relay (loopback-only, per-run token) is a property native messaging provides by construction.; assumed: Prior art used a WebSocket relay; native messaging was not evaluated there either. Verify: Check whether native messaging supports the needed concurrency (multiple simultaneous runs) and host-side lifecycle on Windows before discarding it. Remedy: Add native messaging as alternative B''' and choose transport on measured constraints (concurrency, restart behavior), not by defaulting to the prior art's relay.
- **F19-5** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md`, `crates/c3/src/engines/subprocess.rs:43` - The browser engine breaks the engine contract's autonomy assumptions: stall_sec, timeout and tree-kill all presume a killable autonomous process, but a draft-only browser run can park unboundedly in 'waiting for user send' and the user's tab must never be killed; without an explicit interactive state machine (drafted / awaiting-send / streaming / relay-lost / not-sent) a never-sent run is indistinguishable from a hang and counts as running work forever. Trigger: A run where the user never presses send: the stall cut either fires (falsely recording a stall while waiting on a human) or is disabled (a forgotten run holds the wave open indefinitely). Evidence: read-code: The turn contract is built around an autonomous child: byte-growth stall detection, timeout tree-kill, kill records - none of it has a 'waiting on the operator' state.; read-code: 'Running work' keeps the wake armed; a drafted-but-never-sent browser run matches the definition indefinitely. Verify: Walk one draft-only run through the current contract on paper: name which existing state records 'brief written, awaiting send'; none exists. Remedy: Define the interactive-engine subclass before implementation: `awaiting-user-send` is a recorded, unbounded, non-stall state with an idle-rule exception and a `not-sent` terminal class; kill revokes the session token and closes the transport, never the tab.
- **F19-6** [note] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` - The signed adapter catalogue has no stated key-management story: who holds the signing key, how it rotates, and how a compromised adapter hash is revoked; this matters before even the private plugin ships because the signature is the only barrier between the catalogue and arbitrary DOM-driving code. Trigger: A signing-key loss or a malicious adapter published under a valid hash with no revocation path. Evidence: assumed: The rulings name 'a signed hash/pointer catalogue' but no signer, rotation or revocation. Verify: Ask the operator where the signing key lives and what the revocation story is. Remedy: Add a one-paragraph key-management ruling (signer, rotation cadence, revocation list checked at load) to the design before the private plugin ships.

### Prior findings

- F03-1 - not-checked - Panel A measurement scope, explicitly out of scope here.
- F03-2 - not-checked - Out of scope (panel A).
- F03-3 - not-checked - Out of scope (worker-hook telemetry).
- F03-4 - not-checked - Out of scope (scout-and-judge).
- F03-5 - not-checked - Out of scope (worker outcome classes for intake).
- F04-1 - not-checked - Out of scope (panel A).
- F04-2 - not-checked - Out of scope (telemetry closer).
- F04-3 - not-checked - Out of scope (panel A).
- F04-4 - not-checked - Out of scope (panel A).
- F04-5 - not-checked - Out of scope (SubagentStop telemetry).
- F04-6 - not-checked - Out of scope (panel A).
- F04-7 - not-checked - Out of scope (scout-and-judge).
- F04-8 - not-checked - Out of scope (panel A).
- F05-1 - not-checked - Out of scope (quota attribution).
- F05-2 - not-checked - Out of scope (panel A).
- F05-3 - still-open - Same class as new finding #1: a worker that dies unobserved is invisible to any change detector that reads only its own writes; nothing in this consultation fixes it.
- F05-4 - not-checked - Out of scope (panel A).
- F05-5 - not-checked - Out of scope (effort fields).
- F05-6 - not-checked - Out of scope (panel A).
- F05-7 - not-checked - Out of scope (scout digest).
- F07-1 - not-checked - Out of scope (panel A).
- F07-2 - not-checked - Out of scope (effort mapping).
- F07-3 - not-checked - Out of scope (SubagentStop attribution).
- F07-4 - not-checked - Out of scope (telemetry fingerprinting).
- F07-5 - not-checked - Out of scope (scout-and-judge).
- F08-1 - not-checked - Out of scope (window-share denominator).
- F08-2 - not-checked - Out of scope (event effort fields).
- F08-3 - not-checked - Out of scope (telemetry schema versioning).
- F08-4 - still-open - Reinforced by finding #1: silent worker death is exactly the case the watch must fold as changed; the hook-side undercount remains unaddressed.
- F08-5 - not-checked - Out of scope (rework attribution).
- F08-6 - not-checked - Out of scope (free-text carriers).
- F08-7 - not-checked - Out of scope (panel A).
- F08-8 - not-checked - Out of scope (scout-and-judge).
- F10-1 - not-checked - Out of scope (panel A).
- F10-2 - not-checked - Out of scope (worker_role).
- F11-1 - not-checked - Out of scope (effort fields).
- F11-2 - not-checked - Out of scope (quota attribution).
- F11-3 - not-checked - Out of scope (telemetry closer).
- F11-4 - not-checked - Out of scope (--note free text).
- F11-5 - still-open - Adjacent to finding #1 (stop-event visibility); not re-verified here but nothing has fixed it.
- F11-6 - not-checked - Out of scope (scout-and-judge).
- F12-1 - not-checked - Out of scope (telemetry closer).
- F12-2 - not-checked - Out of scope (roster validation).
- F12-3 - not-checked - Out of scope (caps-v1).
- F13-1 - not-checked - Out of scope (panel A).
- F13-2 - not-checked - Out of scope (cost denominator).
- F13-3 - not-checked - Out of scope (scout digest).
- F13-4 - still-open - Same undercount class as finding #1; host-crash backfill remains unbuilt as far as this consultation's reads show.
- F13-5 - not-checked - Out of scope (effort frontmatter).
- F13-6 - not-checked - Out of scope (roster policy duplication).

## Verdict: ADVISE

Framing consultation: both sketches are directionally sound, but the watch framing ignores the wake's cache-warming job and dead-writer silence, and the browser engine needs an interactive-engine contract (plus a native-messaging transport option) rather than the autonomous subprocess contract.

### Blockers

- **F21-3** (prior, not-checked) `crates/c3/src/engines/codex.rs:1` - A browser reviewer cannot satisfy the engine contract the sketch promises (brief in, structured reply out, transcript, kill record): there is no child to kill, and the dominant state 'draft prepared, user has not pressed send' has no timeout, no partial, and no owner in the current recovery chain.

### Unproven scenarios

- MV3 CSP behavior for WASM in each extension context (content script vs extension page vs offscreen document) - needs the RC1 spike before finding #3 is certain.
- Whether Chrome native messaging supports the bridge's concurrency and host-restart needs on Windows (finding #4's precondition).
- Whether a one-line `c3 watch` invocation is sufficient traffic to keep the host's prompt cache warm (finding #2's remedy depends on it).
- Whether worker STATE files have an expected heartbeat cadence at all; if not, the staleness threshold in finding #1 needs a per-writer hint.

### First-run checklist (observable)

_(none)_
