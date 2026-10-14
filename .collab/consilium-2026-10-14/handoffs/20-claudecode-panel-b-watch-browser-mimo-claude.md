# Handoff 20 - Claude (claude): panel-b-watch-browser-mimo-claude

Date: 2026-10-14 20:13 local. Author: Claude (claude) (model mimo-v2.6-pro, effort high), claude-cli 2.1.299.
Reviewer: mimo-claude :: mimo-v2.6-pro [claude] (provider from roster, model from roster; engine claude (C:\users\dmytro\.local\bin\claude.exe), endpoint https://token-plan-ams.xiaomimimo.com/anthropic (token from env MIMO_API_KEY); provider fingerprint f615a41b2fe4; harness claude-cli 2.1.299).
Preflight: ok: env MIMO_API_KEY set.
Effort: high sent (requested high, mapping claude-v1, by caps-v1: engine:claude, any model; not confirmed by the provider). Consultation id: 501bdb14-3162-4112-93a0-f32e6748b7c5.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `claude -p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model mimo-v2.6-pro --effort high --json-schema "{"$schema":"http://json-schema.org/draft-07/schema#","title":"codex-consult reply v1","type":"object","additionalProperties":false,"required":["schema_version","verdict","verdict_reason","reply_markdown","findings","prior_findings","unproven","first_run_checklist"],"properties":{"schema_version":{"type":"string","enum":["1"]},"verdict":{"type":"string","enum":["ACCEPT","HOLD","REJECT","ADVISE"]},"verdict_reason":{"type":"string"},"reply_markdown":{"type":"string"},"findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["severity","locations","claim","trigger","evidence","verification","remedy","supersedes"],"properties":{"severity":{"type":"string","enum":["blocker","major","minor","note"]},"locations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["path","line"],"properties":{"path":{"type":"string"},"line":{"type":["integer","null"]}}}},"claim":{"type":"string"},"trigger":{"type":"string"},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","reference","observation"],"properties":{"kind":{"type":"string","enum":["read-code","ran-command","inferred","assumed"]},"reference":{"type":"string"},"observation":{"type":"string"}}}},"verification":{"type":"string"},"remedy":{"type":"string"},"supersedes":{"type":"array","items":{"type":"string"}}}}},"prior_findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","status","note"],"properties":{"id":{"type":"string"},"status":{"type":"string","enum":["fixed","still-open","not-checked","unknown-id"]},"note":{"type":"string"}}}},"unproven":{"type":"array","items":{"type":"string"}},"first_run_checklist":{"type":"array","items":{"type":"string"}}}}" --session-id b8daa511-20d5-4b26-9f53-2d56fac5e7a6` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `b8daa511-20d5-4b26-9f53-2d56fac5e7a6` (source: events).
Brief: `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md` (sha256 a34288e066ed). Reviewed: 8a133f8, base 8a133f8cf5a0b6c5086a180c99d00664b34d7673, tree sha256 0916394b9b25, 0 changed files.
Bridge outcome: usable reply. Wall time: 458.8 s. Tokens: in 368765 (cached 329088), out 16290, reasoning 0.
Engine turns: 1 (claude -p, auth endpoint; model mimo-v2.6-pro; init tools Glob, Grep, Read, StructuredOutput; permission denials 0).
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - Framing consultation on the watch and the browser engine: the sketches are directionally sound (A1+B' preferred) but rest on an incomplete event model and an unstated trust root, so I answer inside the frame after naming what the frame omits. Findings: 0 blocker, 6 major, 1 minor, 0 note (F20-1..F20-7, tracked in `findings.json`). Structured reply: `handoffs/20-claudecode-panel-b-watch-browser-mimo-claude.reply.json`.
Raw event stream: `handoffs/20-claudecode-panel-b-watch-browser-mimo-claude.events.jsonl`.
Verbatim reply follows.

---

**Framing first.** The brief frames (A) as "poll better" and (B) as "where does the adapter run". Both hide choices.

*Unlisted for (A):* a write-side change journal — producers append one closed-class line per state change; the watch tails it. "Since <instant>" becomes exact instead of a fold of four heterogeneous readers, and a host `Monitor` gets one file to watch. The sketch's Monitor-on-the-status-directory cannot work as written: status files live per task (`detached_paths(task_dir, id)`), not in one directory. *Unlisted for (B):* wherever the vendor has an API, use the existing `http` engine instead — a web-chat reviewer is a ToS and reliability liability the API path does not carry; and a declarative (selector-YAML) adapter form for trivial sites before WASM.

*What I need to know to choose:* the host's maximum schedulable sleep and prompt-cache TTL (they bound `--next`); whether `Monitor` can watch a file C3 maintains; how the Chrome extension is installed and updated (Web Store review?); who signs the adapter catalogue and how signing keys revoke; whether reading a chat's reply stream is acceptable under the vendor's terms; whether C3 must fold runs on other machines.

**Q1.** Fold, per task: detached status records (state, `updated`, `finished`, members), background liveness (`pid`/`start_time`), worker STATE tails as closed-class tokens only, the newest ledger quota entry (`used_percent`, `resets_at`), and unread health-journal lines. Fold *staleness* as change: a `running` record whose `updated` is old or whose pid is gone prints `changed: stalled/died` — otherwise "nothing since" is a lie (finding #2). Cheapest reliable `--next`: min(budget end = `started + budget_sec`, quota `resets_at`, health `retry_after`), hard-capped at the host's max sleep and at the staleness deadline. A skipped wake loses: host cache expiry (invisible to the bridge), a silent death (no write to fold), and a remote run (finding #3).

**Q2.** B' is right: WASM adapters in C3's process, the extension a dumb, signed DOM bridge. The extension is the only artifact users install and Chrome reviews — keep it code-free and update-rare; the deny-by-default gate then lives where the policy, the ledger and the send-grant live; the catalogue signature is verified once, in C3. Minimal host API: `open(url)`, `composer_put(text)`, `send()`, `read_reply()`, `close()`; capabilities nav|compose|send|read granted per adapter per run, `send` denied by default, every grant recorded on the run record. JS only behind the same API.

**Q3.** Same chain as `subprocess`: a run record (per-run token, adapter hash, capability grants, pinned tab id + profile) and extension-connection liveness; a terminal record for every ending — `done(reply)`, `user_declined` (the draft never sent: the *default* outcome of draft-only, not an error; `--wait` must terminate on it), `abandoned` (tab closed / disconnect mid-reply, with a salvaged `.partial.md`), `budget_end`/kill written *before* the kill (`on_kill` convention), `schema_fail`. No kill without a record.

**Q4.** (1) The watch's one-liner is a prompt-injection path: worker STATE tails are free text written by workers; emit closed-class tokens and file names only (cf. F08-6). (2) Exit-code collision: `--wait` already returns 3 for "still running after timeout" (`detached.rs:1336`); pick unclaimed codes. (3) Cross-host: judgement reports `elsewhere` and cannot check remote liveness (`detached.rs:356`) — the watch must read unknown, never quiet. (4) Trust root and keying: a signed catalogue needs a named signer and revocation, and a usefulness table keyed on adapter+vendor silently re-labels rows when an adapter updates (cf. F08-3) — key on the hash. (5) `composer_put` may reach the vendor before the user presses send (autosave/composer RPC): draft-only is not privacy.

## Requested checks
- **RC1** (host, Q1): in a Claude Code session, ask `ScheduleWakeup` for 3600 s and observe the clamped delay; then compare two identical wakes 30 min apart for cached-read cost. Read-only; 60 min. Settles the `--next` bound and the cache TTL. (finding #2)
- **RC2** (repo root, workspace-write): start one detached run, edit its status `host` field to another machine name, run `c3 consult --task <t> --status`, observe exit 2 `elsewhere` with no liveness. 10 min. (finding #3)
- **RC3** (repo root, read-only): `rg -n "return [0-9];" crates/c3/src/consult crates/c3/src/cli` and tabulate the taken exit codes before assigning `c3 watch`'s. 5 min. (finding #1)
- **RC4** (operator's `~/gemini-mcp`, read-only): dump which DOM regions and messages that extension can read. 30 min. Settles the minimal host API and scraper scope. (finding #7)
- **RC5** (Chrome + a chat vendor, read-only): with DevTools network open, run draft-only `composer_put` and observe whether any request carries the brief before send. 20 min. (finding #7)

---

### Findings

- **F20-1** [major] `crates/c3/src/consult/detached.rs:1336`, `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:51` - The sketch's watch exit codes (0 = nothing, 3 = changed) collide with the consult surface's existing exit-code space: `--wait` already returns 3 for 'still running after the wait limit', and the query surface documents 0/1/2/3/4, so any host script or skill that treats 3 as 'changed' will misread a timed-out wait, and vice versa if watch is folded under `c3 consult`. Trigger: A coordinator runs `c3 consult --wait --wait-timeout-sec N` on a long run and branches on exit 3, or a new `c3 watch` ships with 3 = changed and is scripted alongside the existing surface. Evidence: read-code: the --wait path returns 3 when the run is still running after the wait limit; the module header documents exit codes 0/1/2/3/4 for the query surface; read-code: the sketch specifies 'exit code 0 nothing, 3 changed'. Verify: RC3: inventory every `return <n>` in crates/c3/src/consult and crates/c3/src/cli and confirm which codes are free before assigning watch's. Remedy: Give `c3 watch` its own documented codes outside 0-4 (e.g. 0 nothing / 10 changed / 11 error) or a machine-readable first line the host parses instead of the exit code.
- **F20-2** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:52`, `crates/c3/src/consult/detached.rs:370` - 'Next known event' is not a complete schedule: a worker or detached run that dies silently writes nothing, so it has no future event, and scheduling the next wake at the earliest known budget end or quota reset can push discovery of the death past the run's own budget. The watch must fold staleness (record `updated` age vs budget, `pid`/`start_time` liveness) as change, and `--next` must be capped by a liveness deadline, not only by known events. Trigger: A background process is killed while its status record still says `running`; the coordinator schedules the next wake at the budget end and reads nothing until then. Evidence: read-code: judgement detects death only via pid_alive(pid, start_time) at query time - a death produces no status write at all; read-code: health carries retry_after and quota carries resets_at - the only forward-looking timestamps; none of them marks a silent death; inferred: the wake exists partly to notice things going quiet; a --next that skips to the budget end removes exactly that check. Verify: RC1: measure the host's real wake clamp and cache TTL; then simulate a killed background and check that the prototype watch prints changed: died rather than 'nothing (next: budget end)'. Remedy: Fold pid/start_time liveness and `updated` staleness into 'changed'; emit `--next` as min(known events, staleness deadline, host max sleep).
- **F20-3** [major] `crates/c3/src/consult/detached.rs:356`, `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:48` - Detached runs carry a `host` field and the judgement surface explicitly cannot check liveness for runs on other machines (state `elsewhere`, exit 2). A watch that folds 'the ledger's newest entry and the health journal' without classifying foreign-host runs will report 'nothing since <instant>' for a run it cannot see, converting an unknown into a false quiet. Trigger: A task has a detached run started on another machine (or a copied status file); the coordinator's watch on this host reads the status file and finds no local change. Evidence: read-code: judgement returns state 'elsewhere' with the text 'its liveness cannot be checked from this host'; read-code: DetachedRecord carries host, pid, start_time - all host-local semantics. Verify: RC2: run a detached run, set its status `host` to another machine name, and confirm --status reports elsewhere/exit 2 with no liveness - the watch must print unknown for it. Remedy: The watch prints `unknown: run on host <h>` (a third class) and never counts foreign-host records toward 'nothing'.
- **F20-4** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:49` - Folding 'the tails of the named worker STATE files' into one line that the host's wake reads is a free-text prompt-injection path into the coordinator (workers write those files) and, if the line is ever telemetered, breaks the closed-class privacy contract. The watch output vocabulary must be closed-class tokens and file names only, never quoted worker text. Trigger: A worker's STATE file contains an instruction-like sentence, or a watch line is copied into a telemetry event or handoff. Evidence: read-code: the sketch reads free-text STATE tails and prints 'changed: <what>' with no vocabulary constraint; inferred: the closed-class contract already forbids free-text carriers leaving the machine; the watch adds a new one. Verify: Grep the eventual watch implementation for any path that formats worker file content into its output line; feed a STATE fixture containing an instruction and observe the printed line. Remedy: Define a closed change vocabulary (stalled|died|finished|quota-reset|health|member-state:<token>) and print names/ids, never content; keep any quoting inside the local handoff file, never in telemetry.
- **F20-5** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:61` - The browser sketch has no trust root and no identity for adapters: a 'signed catalogue' names neither the signer nor revocation/rotation, and a usefulness table keyed on adapter + vendor silently re-labels historical rows when an adapter is updated (the same defect as an unversioned caps vocabulary, F08-3). Rows must key on the adapter content hash and the catalogue must name a signer with revocation. Trigger: An adapter is updated in the catalogue while old rows remain in the intake; or a catalogue signing key is compromised. Evidence: read-code: 'loaded from the signed catalogue by hash' and 'the usefulness table keys on adapter + vendor' - no signer, no revocation, no hash in the table key; inferred: an unversioned closed vocabulary already re-labels historical rows; adapter updates do the same. Verify: Ask the operator who signs the catalogue and how a key is revoked; then check that the proposed intake key includes the adapter hash. Remedy: Pin adapter hash per run record and per usefulness row; specify signer identity, expiry and a revocation list the loader enforces offline.
- **F20-6** [major] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:57` - The recovery chain for a browser engine has endings that no subprocess engine has, and the sketch does not name them: 'the user did not press send' is the default outcome of draft-only (a first-class terminal class, not an error), and a tab close, extension disconnect or navigation away mid-reply is an abandoned run needing a salvaged partial. Without these classes, `--wait` blocks forever on a draft nobody will send and kills are recorded as failures. Trigger: A reviewer is left in the composer awaiting the user past the run budget; or the user closes the tab while the reply streams. Evidence: read-code: the sketch lists liveness, kill and 'a reply that never comes' as open questions but assigns no outcome classes; read-code: existing engines write the recovery record naming the tree BEFORE every kill (on_kill); the browser engine has no tree to name but must keep the record-first rule for the relay/token; read-code: member outcome states already include failed/killed/blocked/orphan - the new endings must map into or extend this closed set. Verify: Implement the run record, then run three endings (user never sends; tab closed mid-reply; budget kill) and check that each writes exactly one terminal record and that --wait returns on all three. Remedy: Add outcome classes done|user_declined|abandoned|budget_end|schema_fail with a salvaged partial for abandoned/schema_fail; pin tab id + profile in the run record and refuse on navigation; write the kill record before tearing down the relay.
- **F20-7** [minor] `.collab/consilium-2026-10-14/handoffs/02-claude-watch-and-browser-engine.md:58` - Draft-only is not privacy: `composer_put` may send the brief to the vendor before the user presses send (composer autosave, RPC-backed editors), and `read_reply` scoped too widely turns the extension into a scraper of the user's chat history. Automating a logged-in web chat may also violate the vendor's terms even when the user presses send. Trigger: The brief is written into a vendor's composer, or the adapter's read capability is granted page-wide instead of thread-scoped. Evidence: read-code: the host API is open/put text/read the reply stream with per-capability gating, but no statement that put is local-only or that read is thread-scoped; assumed: not checked; some editors sync drafts server-side. Verify: RC5: with DevTools network open, run draft-only composer_put and observe whether any request carries the brief before send; RC4: check the prior extension's DOM reach. Remedy: Scope read to the current thread's reply region; make the send-grant explicit and per-adapter; document that composer_put can be vendor-visible and offer a clipboard/manual-paste mode as the privacy floor.

### Prior findings

- F03-1 - not-checked - Panel-A measurement design; out of scope for this panel-B framing consultation and not re-verified.
- F03-2 - not-checked - Panel-A measurement design; out of scope here, not re-verified.
- F03-3 - not-checked - Worker-telemetry assumption; out of scope here, not re-verified.
- F03-4 - not-checked - Scout-and-judge routing; out of scope here, not re-verified.
- F03-5 - not-checked - Worker outcome classes; out of scope here, not re-verified.
- F04-1 - not-checked - Outcome vocabulary clash; out of scope here, not re-verified.
- F04-2 - not-checked - Telemetry closer; out of scope here, not re-verified.
- F04-3 - not-checked - Tier score denominator; out of scope here, not re-verified.
- F04-4 - not-checked - Effort tier mapping; out of scope here, not re-verified.
- F04-5 - not-checked - SubagentStop bias; out of scope here, not re-verified.
- F04-6 - not-checked - Free-text note path; out of scope here, not re-verified.
- F04-7 - not-checked - Scout compression loss; out of scope here, not re-verified.
- F04-8 - not-checked - Confounded usefulness table; out of scope here, not re-verified.
- F05-1 - not-checked - Quota record semantics; out of scope here, not re-verified.
- F05-2 - not-checked - Causal identification; out of scope here, not re-verified.
- F05-3 - not-checked - SubagentStop counting; out of scope here, not re-verified.
- F05-4 - not-checked - Model tier classification; out of scope here, not re-verified.
- F05-5 - not-checked - effort_requested ambiguity; out of scope here, not re-verified.
- F05-6 - not-checked - Test-command classifier; out of scope here, not re-verified.
- F05-7 - not-checked - Judge call cap; out of scope here, not re-verified.
- F07-1 - not-checked - Window-share denominator; out of scope here, not re-verified.
- F07-2 - not-checked - caps-v1 mapping; out of scope here, not re-verified.
- F07-3 - not-checked - SubagentStop tier capture; out of scope here, not re-verified.
- F07-4 - not-checked - Task-shape leakage; out of scope here, not re-verified.
- F07-5 - not-checked - Scout falsification budget; out of scope here, not re-verified.
- F08-1 - not-checked - Window-share misattribution; out of scope here, not re-verified.
- F08-2 - not-checked - Effort fields on the wire; out of scope here, not re-verified.
- F08-3 - not-checked - Not re-verified here; my finding #5 cites it as a prior claim (unversioned vocabulary re-labels rows) rather than re-filing.
- F08-4 - not-checked - SubagentStop undercount; out of scope here, not re-verified.
- F08-5 - not-checked - Rework attribution; out of scope here, not re-verified.
- F08-6 - not-checked - Not re-verified here; my finding #4 cites it as a prior claim (free-text carriers) rather than re-filing.
- F08-7 - not-checked - Usefulness table noise; out of scope here, not re-verified.
- F08-8 - not-checked - Scout false negatives; out of scope here, not re-verified.
- F10-1 - not-checked - Calibration runs; out of scope here, not re-verified.
- F10-2 - not-checked - worker_role field; out of scope here, not re-verified.
- F11-1 - not-checked - Effort/cached tokens on events; out of scope here, not re-verified.
- F11-2 - not-checked - Window-share attribution; out of scope here, not re-verified.
- F11-3 - not-checked - close_event allowlist; out of scope here, not re-verified.
- F11-4 - not-checked - workers rate --note; out of scope here, not re-verified.
- F11-5 - not-checked - SubagentStop double counts; out of scope here, not re-verified.
- F11-6 - not-checked - Scout correlated blind spots; out of scope here, not re-verified.
- F12-1 - not-checked - close_event discards worker events; out of scope here, not re-verified.
- F12-2 - not-checked - validate_roster key allowlist; out of scope here, not re-verified.
- F12-3 - not-checked - caps-v1 has no per-purpose policy; out of scope here, not re-verified.
- F13-1 - not-checked - Assignment confounding; out of scope here, not re-verified.
- F13-2 - not-checked - No single cost denominator; out of scope here, not re-verified.
- F13-3 - not-checked - Semantic digest hides defects; out of scope here, not re-verified.
- F13-4 - not-checked - SubagentStop missing endings; out of scope here, not re-verified.
- F13-5 - not-checked - Effort frontmatter vs override; out of scope here, not re-verified.
- F13-6 - not-checked - Per-purpose effort on roster duplicates presets; out of scope here, not re-verified.

## Verdict: ADVISE

Framing consultation on the watch and the browser engine: the sketches are directionally sound (A1+B' preferred) but rest on an incomplete event model and an unstated trust root, so I answer inside the frame after naming what the frame omits.

### Blockers

- **F21-3** (prior, not-checked) `crates/c3/src/engines/codex.rs:1` - A browser reviewer cannot satisfy the engine contract the sketch promises (brief in, structured reply out, transcript, kill record): there is no child to kill, and the dominant state 'draft prepared, user has not pressed send' has no timeout, no partial, and no owner in the current recovery chain.

### Unproven scenarios

- The host's maximum schedulable sleep and prompt-cache TTL (ScheduleWakeup clamping, Monitor availability) - these bound every Q1 recommendation and need a live host probe (RC1).
- Whether a vendor's composer sends typed text to the server before the user presses send - site-dependent and untested here (RC5).
- How the private Chrome extension is distributed and updated (Web Store review, enterprise policy) - decides whether a code-free dumb bridge is as cheap as claimed.
- Whether multi-machine detached runs are a real deployment; if everything runs on one host, finding #3 is latent.
- Whether any adapter catalogue signer/revocation policy exists outside this repository.

### First-run checklist (observable)

_(none)_
