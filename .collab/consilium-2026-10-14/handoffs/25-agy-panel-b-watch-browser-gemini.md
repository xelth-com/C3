# Handoff 25 - Gemini (agy): panel-b-watch-browser-gemini

Date: 2026-10-14 20:12 local. Author: Gemini (agy) (model gemini-3.8-flash-high, effort tier in the model id), agy-cli (version unknown).
Reviewer: gemini :: gemini-3.8-flash-high [agy] (provider from roster, model from roster; engine agy (C:\Users\Dmytro\AppData\Local\Microsoft\WinGet\Packages\Google.AntigravityCLI_Microsoft.Winget.Source_8wekyb3d8bbwe\agy.exe); provider fingerprint a612ba12e63e; harness agy-cli (version unknown)).
Preflight: ok: signed in (usable reply 36 min ago).
Effort: nothing sent (requested high, mapping model-tier, by caps-v1: engine agy, the tier is part of the model id; not confirmed by the provider). Consultation id: 3e0e7d86-41fb-4e0d-aba4-052c95db591c.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `agy -p= --input-format stream-json --output-format stream-json --model gemini-3.8-flash-high --json-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --print-timeout 0 --sandbox --disable-slash-commands` (prompt on stdin as one NDJSON line).
Parent thread: (none - new thread). Result thread: `5b73833d-8180-4248-a117-d940a9253435` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` (sha256 a34288e066ed). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 362.3 s. Tokens: in 94045 (cached 497843), out 29314, reasoning 23956.
Engine turns: 1.
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Framing requires bounding watch wakeups by prompt-cache TTL to prevent cache eviction and decoupling draft-only browser turns from detached runs to avoid coordinator deadlock. Findings: 0 blocker, 3 major, 1 minor, 0 note (F25-1..F25-4, tracked in `findings.json`). Structured reply: `handoffs/25-agy-panel-b-watch-browser-gemini.reply.json`.
Raw event stream: `handoffs/25-agy-panel-b-watch-browser-gemini.events.jsonl`.
Verbatim reply follows.

---

### Framing & Unlisted Options

The brief treats periodic wakes as waste if nothing changed, overlooking that the 20-minute heartbeat primarily maintains prompt-cache residency. Skipping wakes to save cheap cached reads (~10% input cost) risks cache eviction, converting routine wakes into expensive full-context re-ingestion.

**Unlisted Options:**
- **(A) Option 3: Blocking One-Shot Watch (`c3 watch --wait --timeout <s>`):** Uses OS filesystem events (`ReadDirectoryChangesW`/`inotify`) to block the tool call without polling or a daemon `c3 serve`.
- **(B) Option B''' (Native Messaging Host):** Replaces loopback WebSocket with Chrome Native Messaging, eliminating relay port binding, tokens, and firewall popups.
- **Decision criteria:** Choosing between B and B' requires knowing whether Chrome Web Store Manifest V3 approval is required (MV3 strictly bans dynamic remote WASM execution, forcing B' or sandboxed iframes).

---

### Q1. Watch Folding & The "Next Known Event"

**What to fold:**
1. Detached status JSONs (`<task>/.consult.detached-*.status.json`) - state transitions and member progress.
2. Worker state files **plus PID liveness**: A crashed or OOM-killed worker stops without appending to state; folding OS process liveness (`pid_alive`) prevents silent worker stalls.
3. Ledger head, health journal, and operator kick files (`.kick`).

**Cheapest reliable "next known event":**
`min(earliest_budget_end, quota_reset_ts, cache_ttl_bound)`.
The coordinator must clamp `--next` by `cache_ttl_bound` (~18–20 min / 1200s per `SKILL.md`). Skipping wakes past cache TTL saves cents on cached reads but incurs dollars on uncached context rebuilds. A skipped wake also loses early detection of silent worker deaths if PID probing is omitted.

---

### Q2. Adapter Placement & Minimal Host API

**Placement:** **Option B' (WASM in C3, dumb DOM bridge in Extension).**
- *Why:* Dynamic WASM loading by hash violates Chrome Extension Manifest V3 rules against remote code execution. Running WASM via Wasmtime inside C3 provides native memory sandboxing, auditable network isolation, and simplifies the extension to a static, easily verified DOM bridge.

**Minimal Host API (High-Level Semantic Primitives):**
To prevent the DOM bridge from becoming a generic arbitrary-DOM execution proxy:
1. `navigate(url)`: Validates URL against adapter origin allowlist.
2. `fill_draft(text)`: Inserts prompt into composer; cannot trigger submission.
3. `submit()`: Clicks send; strictly gated behind the `--send` capability flag.
4. `stream_reply()`: Observes assistant message mutations, yielding delta chunks.
5. `abort()`: Clicks the web UI "Stop" button.

---

### Q3. Browser Recovery Chain

The subprocess contract ("no kill without a record" via process trees) does not map to browser tabs:
- **Liveness:** Tracked via WebSocket ping/pong (every 5s) and tab visibility.
- **Kill/Abort:** C3 cannot `taskkill` Chrome. On timeout, stall, or kick, C3 dispatches an `abort` frame, waits for extension acknowledgment, and records a `BrowserKillCheck` before closing the relay. If unacknowledged, record as `OrphanedTab`.
- **Draft-Only Abandonment:** If the user never clicks send, the turn must not hang until global budget expiry. Enforce an independent `draft_timeout` (e.g., 180s); upon expiry, log `DraftAbandoned`, salvage partial context, and unlock detached runs.

---

### Q4. Unlisted Risks

1. **Detached Panel Deadlock:** Seating a draft-only browser reviewer in a detached panel (`c3 consult --detach`) blocks the entire panel waiting for an absent human operator. Draft-only reviewers must be refused in detached mode unless `--allow-interactive` is explicit.
2. **Web Chat Desynchronization:** CAPTCHAs, Cloudflare Turnstiles, re-authentication redirects, and vendor A/B UI changes break DOM selectors without error signals, causing silent reply corruption.
3. **Draft Privacy Leakage:** Host environment scrubbing must strip session tokens before writing to the composer; clipboard or third-party extensions must not access the draft buffer.

## Requested checks

- **RC1**: Check Chrome extension MV3 CSP behavior on WebAssembly instantiation from dynamic network buffers; Cwd: `crates/c3`; Permission: read-only; Settles: finding #4 / Q2; Budget: 5 min.

---

### Findings

- **F25-1** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:59`, `crates/c3/src/consult/detach.rs:80` - Seating a draft-only browser engine reviewer in detached consultations or panels stalls coordinator workflows indefinitely until budget exhaustion when the user is not actively attending the browser tab. Trigger: A detached run or panel (c3 consult --detach) schedules a browser engine reviewer under the default draft-only mode while the operator is away. Evidence: read-code: Detached runs execute in the background with stdout/stderr redirected to log files and no interactive console prompt.; read-code: Default browser engine mode is draft-only where the adapter prepares the message in composer and the user presses send. Verify: Initiate a detached panel including a mock draft-only browser engine without user input; confirm detached run hangs until budget timeout. Remedy: Add an explicit draft-wait timeout (e.g., 180s) distinct from engine execution timeout, and fail-fast or refuse draft-only reviewers on detached panels unless --allow-interactive is explicitly set.
- **F25-2** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:53`, `plugin/skills/coordinate/SKILL.md:27` - Scheduling coordinator wakeups solely against external events via --next without bounding by prompt-cache TTL causes cache eviction, turning cheap cached reads into expensive full-context re-ingestions. Trigger: A quiet period or long detached task schedules the next wake past host cache TTL (e.g. >20 minutes). Evidence: read-code: The waiting rule explicitly specifies ScheduleWakeup every ~1200s to keep the prompt cache warm and avoid cold context rewrites.; read-code: Sketch proposes scheduling the next wake at the earliest known event (e.g. detached run budget end or quota reset) instead of a fixed period. Verify: Inspect computed --next value during a long detached run (e.g. 3600s budget); verify if it exceeds 1200s without a cache-keepalive ceiling. Remedy: Clamp --next to min(earliest_event_delta, max_cache_ttl_margin) where max_cache_ttl_margin defaults to 1080-1200s.
- **F25-3** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:55`, `crates/c3/src/engines/subprocess.rs:136`, `crates/c3-core/src/engine.rs:411` - The browser engine lacks OS process supervision, breaking the 'no kill without a record' recovery invariant and leaving aborted web turns without verifiable termination. Trigger: A browser engine consultation times out, is kicked via --kick, or aborted while streaming. Evidence: read-code: Subprocess recovery requires writing a durable record naming the OS PID tree before killing; if write fails, the tree is left alive as evidence.; read-code: KillCheck models root_pid and verified process termination, which is inapplicable to a browser tab. Verify: Trigger a kick on an active browser engine turn; observe that subprocess KillCheck logic cannot verify termination of web generation. Remedy: Introduce a BrowserKillCheck protocol where C3 issues a signed abort message over the relay, waits for an extension ack confirming generation stop, and logs relay/tab state to a dedicated browser recovery record.
- **F25-4** [minor] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:57` - Dynamic loading of signed WASM adapters from an external catalogue inside a Chrome extension violates Chrome Manifest V3 policy prohibiting arbitrary remote executable code execution. Trigger: The Chrome extension attempts to fetch and instantiate a WASM binary dynamically from the local relay or signed catalogue. Evidence: read-code: Sketch specifies that the WASM adapter is loaded from the signed catalogue by hash and runs inside the extension.; assumed: MV3 forbids executing remote code or dynamically fetched WebAssembly outside sandboxed iframes or pre-packaged extension bundles. Verify: Load an unpacked MV3 extension attempting WebAssembly.instantiate on dynamically fetched bytes in a service worker; observe CSP rejection. Remedy: Adopt architecture B' (run WASM in C3 process) or run WASM inside a sandboxed iframe using postMessage, keeping the extension bundle completely static.

### Prior findings

- F03-1 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F03-2 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F03-3 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F03-4 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F03-5 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F04-1 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F04-2 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F04-3 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F04-4 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F04-5 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F04-6 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F04-7 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F04-8 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F05-1 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F05-2 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F05-3 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F05-4 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F05-5 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F05-6 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F05-7 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F07-1 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F07-2 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F07-3 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F07-4 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F07-5 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F08-1 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F08-2 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F08-3 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F08-4 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F08-5 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F08-6 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F08-7 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F08-8 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F10-1 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F10-2 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F11-1 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F11-2 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F11-3 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F11-4 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F11-5 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F11-6 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F12-1 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F12-2 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F12-3 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F13-1 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F13-2 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F13-3 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F13-4 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F13-5 - still-open - still open; panel A telemetry and tier changes remain unmerged
- F13-6 - still-open - still open; panel A telemetry and tier changes remain unmerged

## Verdict: ADVISE

Framing requires bounding watch wakeups by prompt-cache TTL to prevent cache eviction and decoupling draft-only browser turns from detached runs to avoid coordinator deadlock.

### Blockers

- **F21-3** (prior, not-checked) `crates/c3/src/engines/codex.rs:1` - A browser reviewer cannot satisfy the engine contract the sketch promises (brief in, structured reply out, transcript, kill record): there is no child to kill, and the dominant state 'draft prepared, user has not pressed send' has no timeout, no partial, and no owner in the current recovery chain.

### Unproven scenarios

- Extension Manifest V3 WebAssembly instantiation behavior across different Chromium versions.
- Latency and throughput overhead of high-frequency DOM streaming over loopback WebSocket in architecture B'.

### First-run checklist (observable)

_(none)_
