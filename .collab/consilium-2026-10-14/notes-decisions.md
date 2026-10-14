# Consilium 2026-10-14 - the coordinator's record

Two framing panels through C3 0.2.1 (the first panels run by the Rust bridge as the daily driver), full roster, 12 seats each.
Panel A (brief 01, measuring reviewers and workers): 9 usable of 12 (gemini-3.1-pro-high: agy rejects the model id; qwen3.8-max: the Alibaba monthly quota at 100 % until 2026-10-27; mimo via codex: SSE idle timeout). Synthesis: notes-panel-a-synthesis.md.
Panel B (brief 02, the watch and a browser engine): 5 usable of 12 at the first pass - the fork guard refused four codex members because the panel forked the panel-A threads (0.8-1.2 MB each) - plus 5 single reruns with `--mode new` (Astra after the 21:23 window reset): 10 usable. Synthesis: notes-panel-b-synthesis.md.

## Decisions taken (coordinator, 2026-10-14 22:00)

1. Reviewer effort policy per purpose, to go into the roster: gpt-6-astra chore low, checkpoint/reading medium (paired run first), diff-review and acceptance high, core-contract/stuck xhigh; glm-5.3 chore low, checkpoint/framing/diff-review high, acceptance high (first paired-run candidate), stuck max.
2. The score of a reviewer tier: confirmed findings per 1M of the run's own input tokens (cached counted); the window share is a routing constraint, not the score; `cost_basis` recorded per event.
3. Telemetry: BEFORE any new field - the intake accepts and ignores unknown keys, `schema_v` / `vocab_version` on the wire; then `effort` (sent, closed), `effort_requested` (the pre-mapping tier), `effort_source`, `cost_basis`, `assignment` + `cohort_ref` (same-brief pairs), `panel_role`; a separate `worker` event (today `close_event` drops every type but consultation and rating; `validate_roster` refuses an `effort` key - both to fix first).
4. Workers: SubagentStart + SubagentStop paired with a coordinator spawn registry the hook reads (effort from the registry, never guessed); `disposition` (rework by the coordinator) separate from the execution outcome; nothing from the brief, files or repository in the event.
5. Scout and judge: the scout writes a per-file map without a verdict; coverage machine-checked; the judge always gets the diffstat and may fetch raw hunks; the cap counts calls; budget exhausted = "incomplete", never "accept"; 1 in 5 judgings gets the raw diff; scout and judge from different model families.
6. The watch: `c3 watch` output in a closed vocabulary (`nothing | changed | unknown`), never worker STATE text (an injection path); a silently dead worker is never `nothing`; open runs judged through `detached::judgement`, not mtimes; a SEQUENCE cursor, not a timestamp; a new exit code (3 is taken by the wait timeout); `--next` carries only events the bridge owns (a run's budget end), not a quota reset; a skipped wake still has to refresh the cache (a heartbeat job) - the saving is the context read, not the request.
7. The browser engine: B' - the extension is a dumb, signed DOM bridge; the adapters run in C3's process; the bridge protocol IS the host API (per-run token bound to adapter hash, origin and tab; no generic DOM calls); an interactive engine with its own record: `awaiting-user-send` pauses the stall timers, `not-sent` is a terminal outcome, kill = abandon recorded before acting, the user's tab is never closed; draft-only seats refused in detached runs and panels until the operator rules; baseline before any adapter: `c3 pack` + paste (the human relay); reply provenance (a replayed old reply is refused); the catalogue has a named signer and revocation; usefulness rows key on the adapter hash; composer autosave can send before the user does.

## Open (experiments)
- Paired runs (same brief, two efforts) for Astra medium-vs-high reading passes and GLM high-vs-max acceptance.
- Whether the hook sees the worker's effort at all (a spawn-registry prototype).
- How long a draft-only seat waits for the user; whether it may sit in a panel (operator's ruling).
- Quota visibility for third-party plans: none has a usage API (BytePlus three windows, Alibaba monthly, Kimi, z.ai) - a manual `out until` in the roster until then; the bridge classes an instant 429 on a monthly-capped plan as a 10-minute burst (wrong).

## C3 defects seen today (to file)
- A panel with a new brief forks the previous threads by default (the fork guard then refuses the heavy ones) - a panel should default to `--mode new` unless the brief is a follow-up.
- The detach "come back" line prints the PowerShell command (`codex-consult.ps1 -Status`) instead of `c3 consult --status`.
- Single runs of one task cannot run concurrently (the task lock), so reruns of failed panel members serialise.
- An instant 429 with no window text on a plan with a monthly cap should set the endpoint out until the plan's reset.

## Roster change (operator, 22:00)
Every provider with an Anthropic-compatible endpoint now runs through the claude engine (ZAI, mimo, byteplus x2, kimi, alibaba); the six codex entries are in `~/.codex/codex-consult-roster-0.6.codex-entries.json`. Model names as the endpoints report them (`deepseek-v4-1-flash`, `dola-seed-2-0-pro`, `k3-256k`). gemini-3.1-pro-high needs a current agy model id.
