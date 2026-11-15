# Handoff 07 - Meta Muse (muse): routing-review-meta

Date: 2026-11-15 16:42 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.20-R8619.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.20-R8619.1).
Preflight: ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: 51c59dd2-1414-4c47-a82c-2609bfaa51b1.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-600b96feb36f48b281484a0a3817eff1.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Parent thread: (none - new thread; no thread of lineage meta :: muse-spark-1.3-contributor in this task's ledger; other lineage(s): grok :: grok-4.7 [grok], meta :: muse-wonder-1.0 [muse], ZAI-claude :: glm-5.3 [claude]). Result thread: `01a1dfdb-a293-70b1-bcc8-39eeb4acc070` (source: events).
Brief: `.collab/release-0.2.4/handoffs/05-claude-routing-review.md` (sha256 2b41e3e8248e). Reviewed: d7ce658 + uncommitted, base d7ce6581fd1bbee6af3e3589399145493b6adbf0, tree sha256 f62917127425, 22 changed files.
Bridge outcome: usable reply. Wall time: 245.5 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: routing: no lab known for #2 grok :: grok-4.7 [grok] (its model id has no known vendor prefix) - it counts as a lab of its own; give its roster entry a "lab"; muse: warning: rules file at \\?\C:\Users\Dmytro\C3\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - Stale fallback/cache data can drive the hold, and the hold is bypassable via --skip-preflight/--thread. Findings: 0 blocker, 2 major, 1 minor, 0 note (F07-1..F07-3, tracked in `findings.json`). Structured reply: `handoffs/07-muse-routing-review-meta.reply.json`.
Raw event stream: `handoffs/07-muse-routing-review-meta.events.jsonl`.
Verbatim reply follows.

---

## Q1 Hold formula — mostly right, one real staleness hole

Formula `used% >= min(elapsed% + N, 100 - N)` matches the brief (`budget/mod.rs:131-137`); 24 h boundary uses `>= 86399 s` so a daily window counts (`mod.rs:107-114`); stale and `resets_at <= now` are excluded (`mod.rs:132`). Two long windows use ANY-holds, per spec. `elapsed_ratio` truncates to whole seconds (`mod.rs:849`) — negligible. **Hole:** the 8-day openai fallback and offline cache bodies are never marked stale (finding #1): `windows_from_quota` hardcodes `stale:false` and cache parses stamp `observed_at:now`. Week-old `used%` vs today's `elapsed%` can wrongly hold or wrongly release.

## Q2 Purpose-aware bare default — no standing leak, but two bypasses

Standing purposes skip the routed pick (`orchestrate/mod.rs:1527-1529`); walk still skips held/on-request even with `--skip-preflight` (`providers.rs:486-522`). No reroute of explicit `--provider`: it runs or is refused. **Bypasses (finding #2):** `--skip-preflight` skips the explicit hold refusal (`mod.rs:2036`), and `--thread`/`resume`/`fork` never checks the hold (check is `!o.provider.is_empty()`). A thread pinned to a held lineage still runs. No-resume-silently-changes-reviewer and no-candidate-falls-back-to-walk look correct (`mod.rs:1539-1557`).

## Q3 on_request — all automatic paths skip it

Walk (`providers.rs:486-500`), panel incl. `--panel-all` (`providers.rs:859-863`), routed single pick (via `panel_members`), and smoke/swap-smoke (`smoke.rs:65`, `maintain/mod.rs:1091`) all skip `on_request`. Exact naming runs: `--provider/--model` resolves the entry (`mod.rs:1428`), `-Require` rejoins `request` and only `reserve` is an outage (`mod.rs:1659`, `plan.rs:594-600`, `run.rs:333-343`). No leak found. `--model`-alone cannot reach an on-request seat (walk skips first) — matches the brief.

## Q4 Openai fallback — old observation can stand in for current

Chain today+yesterday → 8-day newest → recorded health (`mod.rs:346-390`) with `keep_future` reset-ahead filter is sound, and provider filter stays `openai` (`quota.rs:1170-1181`). **Caveats:** the 8-day window keeps its old `used%` but recomputes `elapsed` at `now` with `stale:false` (finding #1); a prior ChatGPT account's rollout in the same home is indistinguishable from the current account's. A reset since the observation drops the window (safe: no data → not held). Future-dated records were not audited.

## Q5 Compatibility — low risk, two nits

Roster `ext` stays free-form so old binaries load (roster parsing validates `ext` as object, new keys optional). Detached files add `spend_reserve`/`panel_seed` with `#[serde(default)]` (`detach.rs:215,245`), so old→new defaults to held/non-reproducible seed — safe direction. New→old with a `:`-form seed may trip the old seed validator (unverified). Display/readiness skew: `router explain` marks `[held]` with live reads while the run decides from cache (finding #3). Detached/panel/`c3 work` replay paths were not fully traced.

## Q6 Untested

Unit/integration cover corners of the formula, stale/reset, `--spend-reserve`, standing exemption, on-request draw/require (budget tests; `reviewer_protection.rs` 4 tests). Not covered: 8-day-fallback staleness vs hold; `--skip-preflight` + held explicit; `--thread` pinned to held/on-request; cache-vs-live skew between pick and preflight; two-long-window governing choice; old-binary roster/detached replay.

**Not read:** full `reviewer_protection.rs`, `panel/plan.rs` routing math, `quota.rs` merge selection, docs/`SKILL.md`; no shell run (consultation is read-only).

---

### Findings

- **F07-1** [major] `crates/c3/src/budget/mod.rs:359`, `crates/c3/src/budget/mod.rs:455`, `crates/c3/src/budget/mod.rs:589` - The 8-day openai fallback and the offline cache path drive the reserve hold with windows that are never marked stale: windows_from_quota always sets stale:false and the kimi/zai/muse parsers stamp observed_at=now even when the body came from a days-old cache, so week-old used% is judged against today's elapsed%. Trigger: No Codex session for 3-8 days with a high weekly used% on record, or a stale kimi/zai cache on disk while offline; a protected entry is then refused (exit 1/5) on stale data, or held when live quota already reset. Evidence: read-code: windows_from_quota sets stale:false unconditionally; the 8-day fallback passes an 8-day-old observation through it and only filters by resets_at.; read-code: Cache-hit with network==false parses the cached body with observed_at=now and stale:false; snapshot is the only source that sets stale from observed_at age.; read-code: reserve_hold trusts stale==false and only checks resets_at>now, so a stale-fed window holds. Verify: Seed a codex home whose only rollout observation is 7 days old at 97% weekly with reset still ahead, run a bare non-standing consult naming the protected entry, and check whether it is held; repeat with a stale kimi cache and network off. Remedy: Propagate the real observation age: set stale from (now-observed_at)>STALE_DAYS for the 8-day fallback, and thread the cache fetched_at through parse_kimi/parse_zai/parse_muse (or mark cache-fed windows stale when fetched_at is old).
- **F07-2** [major] `crates/c3/src/consult/orchestrate/mod.rs:2035`, `crates/c3/src/consult/orchestrate/mod.rs:1507` - The explicit-provider hold refusal is skipped with --skip-preflight, and a --thread/--mode resume|fork run that pins a held reviewer never checks the hold at all, so a protected reviewer runs when it should be refused. Trigger: --provider <protected> --model <id> --skip-preflight on a non-standing purpose while held; or --thread <thread of held reviewer> after the hold engages. Evidence: read-code: Hold refusal requires !o.skip_preflight and !o.provider.is_empty(); a held entry with --skip-preflight passes preflight.; read-code: The --thread branch resolves roster_entry from the ledger lineage but no hold check applies to that path (check is provider-gated). Verify: With the protected entry at 97% week / 57% elapsed, run explicit --provider with and without --skip-preflight and a --thread pinned to that lineage; compare refusal vs run. Remedy: Apply the reserve-hold refusal to any resolved roster entry on a non-standing purpose without --spend-reserve regardless of skip_preflight/provider/thread, or document that --skip-preflight/--thread intentionally bypass the reserve.
- **F07-3** [minor] `crates/c3/src/panel/run.rs:2681`, `crates/c3/src/cli/router.rs:285`, `crates/c3/src/providers.rs:505` - The hold decision and the routed draw/display disagree by network mode: panel_members and the walk evaluate the hold with network=false (cache only) while route_single_default weights with multipliers_for(network=true) and router explain marks [held] with network=true. Trigger: Fresh live quota says idle but the 5-min disk cache still says 97%, or vice versa: the draw picks a seat the preflight then refuses, or explain shows [held] for a seat the run just used. Evidence: read-code: Hold inside panel_members and walk_full_ctx calls entry_reserve_hold(...,false).; read-code: route_single_default weights with multipliers_for(...,true); router explain checks entry_reserve_hold(...,true).; read-code: router explain uses network=true for the same mark. Verify: Prime a stale held cache, refresh live quota to idle (or the reverse), then compare a bare routed pick, its preflight, and router explain on the same purpose. Remedy: Evaluate hold, multiplier and explain from one window set per run (single network mode / one fetched snapshot), or document which surface is authoritative.

### Prior findings

- F02-1 - fixed - zai_get now builds with redirects(0) and treats 3xx as no data; same for kimi_get. Redirect-with-key path closed.
- F02-4 - fixed - Swap path now checks seat_saturated || on_request || reserve_percent>0 before smoking (maintain/mod.rs:1091); saturation guard applies.
- F03-9 - not-checked - Secondary re-ask accounting not in this diff surface; not examined in this review.
- F03-12 - not-checked - Smoke-target half fixed (smoke.rs:65 skips protected/on-request; kimi_get redirects(0)); scratch-dir-under-state half not examined read-only.
- F04-7 - fixed - Same fix as F02-1: zai_get redirects(0), 3xx yields no window.
- F04-8 - not-checked - Windows detach spawn quoting not in this diff surface; not examined in this review.

## Verdict: HOLD

Stale fallback/cache data can drive the hold, and the hold is bypassable via --skip-preflight/--thread.

### Blockers

_(none)_

### Unproven scenarios

- Whether an old binary rejects a colon-form --panel-seed written by the new binary (old validator text not compared).
- Whether a future-dated resets_at/observed_at record can produce a negative elapsed or a permanent hold (parsers clamp elapsed to 0..1 but the scenario was not traced).
- Whether old detached/panel/c3-work runners ignore or choke on the new fields in practice (serde defaults read; cross-version run not executed).

### First-run checklist (observable)

_(none)_
