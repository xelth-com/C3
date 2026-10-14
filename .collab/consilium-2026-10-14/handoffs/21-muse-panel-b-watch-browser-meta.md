# Handoff 21 - Meta Muse (muse): panel-b-watch-browser-meta

Date: 2026-10-14 20:08 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.7-R6101.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.7-R6101.1).
Preflight: ok: signed in (usable reply 36 min ago).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: 8863a4fb-0f0e-47ca-bff8-4f69cb71f87c.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-5eeeebec261f4c9997193f102caa08dc.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Parent thread: (none - new thread). Result thread: `01a13b98-43cd-7742-b810-af5a821193f5` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` (sha256 a34288e066ed). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 126.2 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: muse: warning: rules file at \\?\C:\Users\Dmytro\C3\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Framing review with four blocking-level corrections to the watch cursor/next-event and browser-engine recovery contracts before implementation. Findings: 1 blocker, 3 major, 0 minor, 0 note (F21-1..F21-4, tracked in `findings.json`). Structured reply: `handoffs/21-muse-panel-b-watch-browser-meta.reply.json`.
Raw event stream: `handoffs/21-muse-panel-b-watch-browser-meta.events.jsonl`.
Verbatim reply follows.

---

## Framing challenge

The brief frames (A) as poll-vs-push and (B) as where-to-run-the-adapter, but both inherit harder constraints. For (A), the wake itself is the cost: the host must still fire to run `watch`, so the design saves context re-reads, not wakes, and every skipped wake trades cache warmth for savings — a cold cache re-reads everything anyway. Unlisted options: a host-native file Monitor as the primary signal with wake only as fallback; a background waiter that writes a wake file; folding at most status mtimes/ledger seq rather than tails of free-form worker STATE files. For (B), treating a user-tab chat as "the same contract as the others" is the framing risk: no spawn/kill/timeout semantics, unbounded human latency, auth/CAPTCHA/ToS breakage, and draft text crossing a trust boundary. Unlisted options: browser as pack-consumer plus human-pasted reply (no engine at all); a sidecar outside C3 core behind the private plugin; explicitly non-goal: full auto-send. To choose I would need: wake token cost vs cold re-read cost and cache lifetime; which hosts actually offer Monitor/file-watch; per-run token/audit requirements for the relay; and who owns adapter-selector breakage.

## Q1 — what should watch fold, cheapest reliable next event, what skipping loses

Fold only locally-authored, monotonic facts: detached status-file mtime+state transitions, ledger sequence, named worker-state mtimes, health-journal entries C3 wrote. Do not parse STATE tails or quota text for change detection. The only reliable "next" is the earliest locally-known deadline: detached budget end or member timeout. Quota reset is advisory (shared, moves without us — finding #2). A skipped wake loses two things: cache warmth (skip too many and the re-read costs more than the wakes saved — cap consecutive skips) and silent death (a worker that died without writing looks identical to "nothing changed"; the watch must distinguish "no evidence" from "evidence of nothing" and age out the former).

## Q2 — where should the adapter run, minimal host API

Prefer B': dumb signed DOM bridge in the extension, adapters in C3 — but only because audit, catalogue-pin, and consent live better in C3, not because it removes selector fragility (finding #4). Minimal API is not open/compose/read/done; it is: bind(tab, origin, run-token), checkCatalogueHash, putDraft, readStream, reportDone, appendAudit, revoke. Deny-by-default per capability, consent per adapter-hash (an update re-asks), loopback-only relay with per-run token and origin pin.

## Q3 — what the engine must record

New states, not the subprocess chain: prepared / awaiting-user / sent / streaming / abandoned, plus relay-liveness heartbeats. Kill becomes token-revoke; add explicit expire/abandon owned by the coordinator (`--kick` abandons the draft). Record adapter hash + vendor + draft-vs-sent + abandon reason; keep drafts and digests out of telemetry (closed outcome classes only). Unsent-across-N-wakes must have a defined expiry, and tab-close/relay-drop must map to a status-file state, never to silent running.

## Q4 — unlisted risk/design

Silent narrowing: a watch that answers in one line becomes the coordinator's only eyes, so a stale/broken watch (clock skew, pruned status files, full disk) fails closed to "nothing" and idles the session into handover. Design the watch to fail loud (unknown exit distinct from nothing/changed) and to print its cursor so the next wake can audit it. For the browser engine: version the usefulness key by adapter hash, and treat `--send` as the product's riskiest surface — rate limiting is not consent.

## Requested checks

- RC1 (read-only): grep the detached status reader for its exit-code table and the watch sketch for exit 3; confirm collision — settles finding #1.
- RC2 (read-only): diff two consecutive ledger quota entries against their token deltas on a shared-account day — settles finding #2.
- RC3 (read-only): list the enforced relay host-API calls and show where hash check, consent, and audit write occur — settles finding #4.

---

### Findings

- **F21-1** [major] `crates/c3/src/consult/detach.rs:1`, `plugin/skills/coordinate/SKILL.md:38` - The sketch's watch exit contract (0 nothing / 3 changed) collides with the existing detached-run status code space (0/1/2/3/4 in detach.rs) and its wall-clock --since cursor can miss a concurrent terminal write: the background atomically replaces the status file after the watch reads it, so a wake skipped on 'nothing' loses the completion it exists to catch. Trigger: A detached run completes between the watch read and the coordinator's skip decision, or a caller confuses watch exit 3 with a detached-run status code. Evidence: read-code: detach.rs header defines read-only --status/--wait/--prune surface with exit codes 0/1/2/3/4 reading only status files; background sink writes starting->running->done with atomic replace and updated stamp.; read-code: Coordinate skill defines the 20-min ScheduleWakeup heartbeat with small reads only and idle hand-over after two quiet wakes. Verify: Start a detached run, run the watch in a loop with --since at 1s granularity through completion, and check no completion is ever reported as nothing. Remedy: Do not reuse exit 3; emit a machine-readable cursor (status-file mtime+hash or ledger seq) instead of wall-clock --since, and document the watch as advisory with a bounded re-check, not a skip guarantee.
- **F21-2** [major] `docs/port/wave8-quota.md:1`, `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:53` - The cheapest schedulable '--next hint' (detached budget end / quota reset) is the least reliable signal: the quota window is account-wide and moves without C3 runs, and budget-stop is a wall estimate, so scheduling wakes against either reintroduces exactly the wasted wakes the watch removes. Trigger: Coordinator sleeps until a quota reset while the detached run finished in minutes, or wakes at budget end for a run that already stalled. Evidence: read-code: wave8-quota.md shows the live source is the newest rollout for the shared openai endpoint with 60s caching, plus fork/replay guard and budget-stop semantics carried into C3.; inferred: Prior rounds observed window movement with no local runs and nonlinear movement within one run, so neither reset time nor budget end is a tight lower bound on next change. Verify: Compare --next predictions against actual completion times over one wave of detached runs on a shared account. Remedy: Fold only locally-authored timestamps (status-file mtimes, ledger seq, worker state mtimes); report quota/budget as informational upper bounds labelled unreliable, never as --next.
- **F21-3** [blocker] `crates/c3/src/engines/codex.rs:1`, `crates/c3/src/engines/subprocess.rs:55` - A browser reviewer cannot satisfy the engine contract the sketch promises (brief in, structured reply out, transcript, kill record): there is no child to kill, and the dominant state 'draft prepared, user has not pressed send' has no timeout, no partial, and no owner in the current recovery chain. Trigger: A draft sits unsent across two wakes, or the user closes the tab; --status/--wait/--kick have no defined reading. Evidence: read-code: codex.rs adapter is one spawned turn with stdin prompt, .events.jsonl stream, classified failure/timeout/partial; subprocess.rs wait/kill measures wall time and kills the tree on timeout.; read-code: subprocess SpawnRequest documents on_running launch->running flip and on_kill tree-kill record before anything is stopped. Verify: Walk the recovery matrix (unsent, tab closed, relay dropped, reply truncated) and confirm each yields a defined status-file state and kick behaviour. Remedy: Define browser as a distinct engine kind with states (prepared/awaiting-user/sent/streaming/abandoned), an explicit abandon/expire path owned by the coordinator, and a kill record that means relay-token revoked, not process killed.
- **F21-4** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55` - The adapter-placement choice is under-constrained: WASM in the extension still needs a JS DOM bridge for every chat-site selector, so it pays the WASM toolchain without removing the fragile part; WASM in C3 (B') turns every DOM read into a WebSocket round trip with the same selector fragility plus relay trust. The minimal host API in the sketch (open/compose/read/done) omits the security-critical calls: bind tab+origin, prove catalogue hash, check per-run token, append to a local audit log, and expire draft-only consent per adapter hash. Trigger: An adapter update changes selectors without changing its name, or a --send consent granted for v1 silently drives v2. Evidence: inferred: Both B and B-prime keep a site-specific selector layer somewhere; deny-by-default, draft-only, and signed-catalogue rulings require enforcement calls the four-call API does not name.; read-code: Telemetry is outcome-classes only, never prompts/briefs/paths; usefulness ranking per adapter+vendor needs an adapter version or hash key to avoid mixing adapter revisions. Verify: List the enforced host-API calls with their deny defaults and show where consent, hash check, and audit write happen for one draft-only round. Remedy: Pick dumb signed DOM bridge in the extension plus adapters in C3 only if the relay is loopback, token-bound per run, and origin-pinned; version the usefulness key as adapter-hash + vendor and keep digests/drafts out of telemetry.

### Prior findings

- F03-1 - not-checked - Measurement-denominator dispute; outside this watch/browser framing review.
- F03-2 - not-checked - Effort-mapping logging; outside this review.
- F03-3 - not-checked - SubagentStop effort visibility; outside this review.
- F03-4 - not-checked - Scout-and-judge anchoring; outside this review.
- F03-5 - not-checked - Worker rework attribution; outside this review.
- F04-1 - not-checked - Outcome-vocabulary contradiction; outside this review.
- F04-2 - not-checked - Telemetry closer allowlist; outside this review.
- F04-3 - not-checked - Window-share computability; related to Q1 --next but not re-verified read-only.
- F04-4 - not-checked - Endpoint-local effort tiers; outside this review.
- F04-5 - not-checked - SubagentStop survivorship bias; outside this review.
- F04-6 - not-checked - Free-text note vs closed class; related to Q3 audit but not re-verified.
- F04-7 - not-checked - Scout compression/anchoring; outside this review.
- F04-8 - not-checked - Observational confounding; outside this review.
- F05-1 - not-checked - Quota-record vs incremental cost; related to Q1 but not re-verified.
- F05-2 - not-checked - Causality from usefulness table; outside this review.
- F05-3 - not-checked - Stop-equals-worker counting; outside this review.
- F05-4 - not-checked - Tier from agent name; outside this review.
- F05-5 - not-checked - effort_requested semantics; outside this review.
- F05-6 - not-checked - Test-pattern verification label; outside this review.
- F05-7 - not-checked - Digest-first judge risk; outside this review.
- F07-1 - not-checked - Window-share portability; related to Q1 but not re-verified.
- F07-2 - not-checked - Caps-v1 treatment destruction; outside this review.
- F07-3 - not-checked - SubagentStop attribution limits; outside this review.
- F07-4 - not-checked - Worker-event fingerprinting; outside this review.
- F07-5 - not-checked - Scout falsification budget; outside this review.
- F08-1 - not-checked - Window-share misattribution; related to Q1 but not re-verified.
- F08-2 - not-checked - Sent-only effort on z.ai; outside this review.
- F08-3 - not-checked - Unversioned closed vocabulary; related to Q3 keys but not re-verified.
- F08-4 - not-checked - Stop-hook undercounting; outside this review.
- F08-5 - not-checked - Rework misattribution; outside this review.
- F08-6 - not-checked - Free-text carriers in worker/scout designs; related to Q3 but not re-verified.
- F08-7 - not-checked - Sparse cells plus rater bias; outside this review.
- F08-8 - not-checked - Silent scout failure modes; outside this review.
- F10-1 - not-checked - Missing calibration runs; outside this review.
- F10-2 - not-checked - Missing worker_role; outside this review.
- F11-1 - not-checked - Requested-tier recovery; outside this review.
- F11-2 - not-checked - Window-share attribution; related to Q1 but not re-verified.
- F11-3 - not-checked - close_event worker drop; outside this review.
- F11-4 - not-checked - --note vs closed class; outside this review.
- F11-5 - not-checked - Double-counted stops; outside this review.
- F11-6 - not-checked - Lossy digest blind spots; outside this review.
- F12-1 - not-checked - close_event hardcoded types; outside this review.
- F12-2 - not-checked - Roster effort-key refusal; outside this review.
- F12-3 - not-checked - Caps-v1 static vocabulary; outside this review.
- F13-1 - not-checked - Missing assignment field; outside this review.
- F13-2 - not-checked - No single cost denominator; related to Q1 but not re-verified.
- F13-3 - not-checked - Semantic digest hiding hunk defects; outside this review.
- F13-4 - not-checked - Stop-hook missing kills; outside this review.
- F13-5 - not-checked - Frontmatter vs sent effort; outside this review.
- F13-6 - not-checked - Preset/roster drift; outside this review.

## Verdict: ADVISE

Framing review with four blocking-level corrections to the watch cursor/next-event and browser-engine recovery contracts before implementation.

### Blockers

- **F21-3** `crates/c3/src/engines/codex.rs:1`, `crates/c3/src/engines/subprocess.rs:55` - A browser reviewer cannot satisfy the engine contract the sketch promises (brief in, structured reply out, transcript, kill record): there is no child to kill, and the dominant state 'draft prepared, user has not pressed send' has no timeout, no partial, and no owner in the current recovery chain. Verify: Walk the recovery matrix (unsent, tab closed, relay dropped, reply truncated) and confirm each yields a defined status-file state and kick behaviour. Remedy: Define browser as a distinct engine kind with states (prepared/awaiting-user/sent/streaming/abandoned), an explicit abandon/expire path owned by the coordinator, and a kill record that means relay-token revoked, not process killed.

### Unproven scenarios

- Whether any host in scope offers a true file-watch Monitor that could fully replace periodic wakes.
- Actual token cost of one watch invocation versus one cold context re-read at current context sizes.
- Whether chat vendors' ToS permit relay-driven draft/send automation from the user's own tab.

### First-run checklist (observable)

_(none)_
