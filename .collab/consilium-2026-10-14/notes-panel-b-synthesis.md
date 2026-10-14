# Panel B synthesis: the watch and the browser engine (consilium 2026-10-14)

Members: 19 kimi-k3, 20 mimo (claude), 21 muse-spark, 23 dola-seed-2.0-pro, 24 glm-5.3 (claude), 25 gemini-3.8-flash, 27 mimo (codex), 28 glm-5.3 (codex), 29 deepseek-v4.1-flash, 30 gpt-6-astra. All ADVISE. 27-30 saw the findings of 19-25; credit goes to the first proposer. Checked: `detach.rs:1336` returns 3 for a wait timeout.

## 1. Per question

### Q1: watch fold, next event, skipped-wake losses
**Consensus.** Silent death must never read `nothing`: a killed background leaves `running` on disk, so open runs go through `detached::judgement`, not mtimes (20, 24, 27-30; 19, 23 stale `updated`; 25 pid). Foreign-host and unreadable records are `unknown`, a third class (20, 21, 27-30). No worker STATE prose (prompt injection): closed tokens and file names (20, 21, 27-30). Cursor = sequence/revision, not an instant (21, 24, 27-30). A watch-first wake still pays the cached read; skipping needs a Monitor or push (21, 24, 27, 28, 30). Losses: cache expiry (most), silent death (all), foreign runs (20, 27), kick files (19, 25). Exit 3 collides with `--wait` (20, 21, 24, 27-29).
**Dissent.** Quota reset and budget end are advisory bounds, not `--next` inputs (21, 27-30) vs in the min (19, 20, 23-25). 30: a write after the scan belongs to the next cursor; a journal alone detects no death and has a crash gap after the status write; per-subcommand codes are fine with `--json`; measure the cache before keeping every heartbeat. 29: `c3 consult --status --since <seq>`, no new command. 23 (echoed 27, 28): jitter, moot with one coordinator per task.
**New elements.**
- Append-only closed-vocabulary journal, `--since <seq>`, one file for a Monitor (20, 24, 27-29), appended in `write_status` (24), with writer/run id (28). Smaller: per-source revision vector; subscribe before scan, rescan after a lost notification, cursor expiry (30).
- Positive lease per writer (23, 29). Fail loud: a broken watch (clock skew, pruned files, full disk) says `unknown` and prints its cursor for audit (21).
- `--next` from health `until`/`retry_after` (20, 24), off-peak start (24); clamped to host max sleep (20), the heartbeat (19, 24, 25, 28), the idle threshold (24).
- `--all` over the collab root for peer replies (24); status files are per task, so "the status directory" does not exist (20).
- Blocking one-shot watch on OS file events (25); finite task supervisor instead of a daemon (30); codes 10+ (20, 29) plus versioned `--json` (28, 30).

### Q2: adapter placement, host API
**Consensus.** B' by all ten. The bridge protocol is the real host API (24): every op carries a per-run token bound to adapter hash, origin and tab; no query/click/eval; reads scoped to one thread; `send` a separate grant, denied by default (all but 19, 23). Signature, consent, audit, pacing stay in C3.
**Dissent.** 19, 25, 27-29 call extension WASM infeasible (page CSP, `wasm-unsafe-eval`, remote code); 30, citing Chrome docs: isolated-world content scripts permit WASM, the real limit is Store policy on fetched code - B' wins on update and capability boundaries, not CSP. 30: adapters in a bounded helper process (24, 28 an option). 21: B' keeps selector fragility. Native messaging (19, 25, 27, 28) vs 30: Chrome launches the host, so a running C3 run needs an attach protocol. Declarative adapters (20, 29) need a ruling change (30). 24: defer JS.
**New elements.**
- Alternative 0: `c3 pack` + paste, the relay the engine must beat (21, 29, 30); the `http` engine where a vendor API exists (20).
- Evidence boundary: a web chat cannot read the repository - send the sanitized http pack with hash and revision, or refuse (30).
- Containment: bounded WASM memory/execution, IPC queues, host-call deadlines (30).
- Extension build hash pinned; loopback token >= 128 bits, one connection per run (24). Consent per adapter hash (21). Signature checked per run (23). `abort()` presses the vendor's Stop (25).

### Q3: recovery chain
**Consensus.** Interactive, not a subprocess: a persisted per-member record like the detached one; `awaiting-user-send` first-class, stall/budget clocks suspended; "never sent" a terminal class, not an error, and `--wait` returns on it; kill = abandon (token revoke, transport close) recorded first, mirroring `KillEvent::Before`; the user's tab never closed; partials salvaged (all; 23 thinnest).
**Dissent.** Human-wait bound: none (19) vs fixed 180 s (25, too short) vs a separate consent lease (24, 28-30). Stop proof: extension ack else `OrphanedTab` (25) vs 30: a closed pipe proves nothing remote - record attempted/confirmed/unknown. Detached/panel seats: refused unless explicit (25, 28, 30) vs 29: the rulings conflict, operator decides.
**New elements.** `submission-unknown`, never auto-resend, ops bound to run/attempt/thread so a replayed old reply is rejected (30); draft retries need the user (24, 29); human-wait recorded, draft rows stratified (24, 27, 28, 30); pending drafts in the SessionStart phrase (24); a parked draft is not running work (19).

### Q4: unlisted risks
Unnamed trust root: signer, rotation, revocation; rows and consent keyed on content hash + catalogue revision (19-21, 27-29). Composer autosave can upload before send (20, 27-29). Vendor terms (20, 21, 28, 29). Draft-only panels serialize on one human (24, 27-29). CAPTCHA/re-auth/A-B UI break selectors silently (25).

## 2. Decisions the coordinator can take now

**(A) Watch.**
1. Fold: judgement over open runs; worker/build/peer completion records as tokens; health lines unblocking the task; terminal ledger entries; kick files.
2. Output: one line + versioned `--json`: `nothing | changed:<token> | unknown:<why>`, cursor, next deadline. Exit 0 / 10 changed / 11 unknown; never 3.
3. Cursor v1: per-source revisions - no new writer, no crash gap. The journal arrives with the Monitor path, written atomically with the status or treated as a hint.
4. `--next` = min(liveness deadline, `budget_sec` end, member timeouts, `retry_after`/`until`, off-peak start), clamped to <= 1200 s and host max sleep; quota reset informational; no jitter.
5. Skipping loses the cache (keep the heartbeat until measured), silent deaths, foreign runs, kick files, peers outside `--task`.
6. No `c3 serve`: background `--wait` + Monitor is today's push; a finite task supervisor before any daemon.

**(B) Browser engine.**
1. B': adapters in a supervised C3 helper (bounded memory, fuel, IPC); extension static, bundled, signed, hash-pinned, re-checking capabilities. Nobody argued for CDP.
2. Host API (run token + adapter hash on every op; off-origin/tab refused): `bind(tab, origin)`, `open(url)`, `compose(text)`, `observe_submission()`, `read()` (bound thread), `send()` (grant), `stop()`, `release()`. C3-only: signature + revocation, consent, audit, pacing.
3. Record: run/attempt id, adapter hash + catalogue revision, extension hash, pack digest, binding, mode, grants; timed `prepared -> draft-written -> awaiting-send -> (submission-unknown) -> sent -> streaming`; human-wait; partial; one terminal of `completed | not-sent | tab-closed | relay-lost | stopped | schema-fail | failed`; stop intent first; relay heartbeat; no stall while awaiting the user.
4. Rules: draft-only default; `--send` per adapter hash; never auto-resend; retries ask the user; briefs as the http pack; single interactive runs until the operator rules on detached/panel seats.

## 3. Open questions (question -> settling experiment)

- watch saving, cache lifetime -> six matched status-first/watch-first wakes, cached-input tokens (24, 28, 30)
- watch soundness, journal vs revisions -> harness: out-of-band kill, foreign host, same-second writes, crash between status and journal (19, 24, 29, 30)
- WASM per context (30 vs 19/25/27-29) -> scratch MV3 extension: isolated, main world, extension page, offscreen; bundled vs fetched
- native messaging vs loopback -> three ports attaching to two runs, mismatch refused (28, 30); unauthenticated loopback connect (24)
- pre-send upload -> DevTools capture during `compose` (20, 27, 28)
- reply as DOM text -> read `~/gemini-mcp` (20, 24)
- provenance; containment -> fake bridge replaying an old reply; a looping adapter in the helper (30)
- draft retries -> schema-invalid reply, `format_retry 2` (24)
- declarative vs WASM -> one adapter both ways (29)
- operator: detached/panel seats; signer; vendor terms; web-only volume vs `c3 pack` (29)

## 4. Ratings

| # | useful | why | best idea |
|---|---|---|---|
| 19 kimi | yes | first on the heartbeat's cache job, native messaging, `not-sent`; CSP claim overbroad | native messaging |
| 20 mimo (claude) | yes | exit-3 collision cited, `elsewhere` as unknown, STATE injection, autosave, five runnable RCs | closed-vocabulary watch output |
| 21 muse | yes | sequence cursor, unreliable quota reset, consent per hash, first interactive-contract blocker | fail loud, print the cursor |
| 23 dola | partly | restates the sketch; "no liveness" wrong (judgement has pid); invented `browser.rs`; blanket priors | per-run signature check |
| 24 glm (claude) | yes | most original: wakes not skipped, journal in `write_status`, retries need the user | bridge protocol = host API |
| 25 gemini | partly | one-shot OS-event watch, Stop-button abort; 180 s draft timeout defeats draft-only | refuse draft-only detached seats |
| 27 mimo (codex) | partly | supersedes map of F19-F25, nothing new; adopted jitter | the dedup map |
| 28 glm (codex) | partly | consolidation plus searched Chrome docs; overbroad CSP blocker; 2.1M input | terminal-class list |
| 29 deepseek | yes | `c3 pack` baseline, declarative adapter, ruling conflict, `--status --since` | alternative 0: the human relay |
| 30 astra | yes | corrected the CSP consensus from docs; attach gap; provenance; evidence boundary; containment | web chats cannot read the repo |

**Same model, two routes.** mimo: 20 (claude, 459 s) original; 27 (codex, 917 s, 694k input) re-filed earlier findings - claude wins, though 27 had seen 20. glm: 24 (claude, 305 s) most original; 28 (codex, 602 s, 2.1M input, format repair) consolidated, but only the codex route had web search - ideas via claude, evidence via codex.
