# Handoff 06 - Grok (grok): routing-review-grok

Date: 2026-11-15 16:51 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (~/.grok/auth.json) - subscription (grok.com oauth).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: c84c55c3-5772-4888-a39e-6328495792fa.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-0702ad1cce014dfaab27f4cfb7abbde7.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Parent thread: (none - new thread). Result thread: `01a1dfdb-9c19-7990-ac5b-4fe25965332a` (source: events).
Brief: `.collab/release-0.2.4/handoffs/05-claude-routing-review.md` (sha256 2b41e3e8248e). Reviewed: d7ce658 + uncommitted, base d7ce6581fd1bbee6af3e3589399145493b6adbf0, tree sha256 f62917127425, 22 changed files.
Bridge outcome: usable reply. Wall time: 824.1 s. Tokens: in 150054 (cached 2026496), out 48605, reasoning 41674.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Warnings: routing: no lab known for #2 grok :: grok-4.7 [grok] (its model id has no known vendor prefix) - it counts as a lab of its own; give its roster entry a "lab".
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The ordinary protected-entry and on-request paths match the spec, but a light or weighty panel flag lets a held or on-request reviewer run, and a bare resume follows the new draw. Findings: 0 blocker, 2 major, 2 minor, 0 note (F06-1..F06-4, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/06-grok-routing-review-grok.reply.json`.
Raw event stream: `handoffs/06-grok-routing-review-grok.events.jsonl`.
Verbatim reply follows.

---

## Q1
`reserve_hold` (`crates/c3/src/budget/mod.rs:122`) matches the spec. It holds on a window with `period_secs >= 24h-1s` (`:107`), `stale` false, and `resets_at` still ahead, when `used% >= min(elapsed%+N, 100-N)` (ratios are stored 0..1). Five hours is 18000s and is exempt. Elapsed is `1-(resets_at-now)/period`, clamped (`:845`). No data, reserve 0, a stale snapshot, or a passed reset does not hold. Callers waive `acceptance` / `core-contract` (`purpose.rs:24`) and `--spend-reserve`. The first long window over the line is the one reported. Missed holds come from finding 1 (light/weighty gates run first) and finding 4 (an old OpenAI `used_percent` is scored as current).

## Q2
Non-standing bare consults draw one seeded reviewer (`orchestrate/mod.rs:1527`, `panel/run.rs:2638`). Standing purposes keep the first available entry. Explicit `--provider` stays on that entry; preflight refuses a hold (`orchestrate/mod.rs:2035`) unless `--spend-reserve` or the purpose is standing. `--skip-preflight` skips that refusal (finding 2). `--thread` keeps the ledger reviewer and skips the hold; the stated hold list leaves continuation alone. `--mode resume` or `fork` with neither `--provider` nor `--thread` still draws (`:1509`); `select_parent_thread` (`:3143`) continues only that lineage, so the task can change reviewer or hit the parent-thread refusal while another lineage has a thread (finding 3).

## Q3
The walk (`providers.rs:486`), panels including `--panel-all` (`:859`), and smoke (`maintain/smoke.rs:65`) skip `on_request`. `--provider` plus `--model` runs that entry (`providers.rs:2884`). `--require #n` and `provider :: model` rejoin `request` (`plan.rs:594`). A bare `--require <label>` matches the whole label, so an on-request sibling is seated. A `panel` value of light is set while state is still run (`providers.rs:850`), before `on_request` (`:859`) and `reserve` (`:867`). `stand_in_light_members` (`:952`) promotes that row when the label has nobody running. On a weighty purpose a light on-request entry is seated, and a light protected entry is seated while the hold would apply. A weighty protected entry on a light purpose is stored as weighty, and `--require` rejoins it (`plan.rs:594`) with no reserve check (finding 1).

## Q4
`account_window_at` stays on two local days (`quota.rs:1186`). `openai_windows` (`budget/mod.rs:343`) then reads 8 days of rollouts and the health file. `keep_future` applies only to those fallbacks. A non-empty two-day result returns immediately, so a reset weekly window blocks them. Fallback windows use `stale: false` (`:455`): old `used_percent`, elapsed recomputed now. A low old share misses the hold and still boosts `budget_multiplier` (finding 4). The scan has no account id, and a future observation time wins the merge. Eight days fit `local_day_dirs` (`quota.rs:492`).

## Q5
New keys live on free-form `ext` (`roster.rs:676`); unknown ext keys are accepted. I did not run d7ce658. `spend_reserve` defaults in detach JSON (`detach.rs:215`) and unknown fields are ignored. Panel children trust the parent seat (`member.rs:260`). `c3 work` ignores the flags. `--panel-seed` on a single consult only seeds the bare draw (`args.rs:342`). Ledger fields are unchanged in this diff.

## Q6
Tests cover the formula (`budget/tests.rs:355`), an ordinary protected snapshot (panel skip, provider refusal, `--spend-reserve`, acceptance, `--require #2`), `--panel-all` vs on-request, and a weighty #1 losing bare diff-review (`reviewer_protection.rs`). They miss findings 1, 3, and 4, and `--skip-preflight`. The full c3-cli suite was still running.

---

### Findings

- **F06-1** [major] `crates/c3/src/providers.rs:850`, `crates/c3/src/providers.rs:859`, `crates/c3/src/providers.rs:867`, `crates/c3/src/providers.rs:952`, `crates/c3/src/panel/plan.rs:594`, `crates/c3/src/panel/run.rs:337` - panel_members assigns skip kind light or weighty while the row is still run, so the later on_request and reserve checks never see it. stand_in_light_members then promotes a light row to run when no sibling of that provider label is running, which seats a light on-request entry on every weighty purpose and seats a light protected entry even when reserve_hold would hold it. A weighty protected entry on a light purpose is stored as weighty; the required-outage check treats weighty as soft and select_panel_routing rejoins it, so --require of a held weighty entry runs it. Trigger: Purpose framing, decision, stuck, acceptance, or core-contract, with an entry whose panel is light and whose ext is on_request or a reserve_percent whose long window is over the line, and no other running entry of that provider label. Or purpose diff-review, panel weighty, reserve over the line, and --require of that entry. Evidence: read-code: The weighty gate and the light gate set state to skipped before the on_request and reserve blocks, both of which require state == run. stand_in_light_members sets skip_kind light back to run and clears skip_kind.; read-code: A required member with skip_kind weighty, light, or request is rejoined as run. The outage filter ignores those three kinds, so reserve is the only protection skip that refuses. Verify: Dry-run c3 consult --panel --purpose framing with one roster entry panel light and ext.on_request true, as the only entry of its label. Expect the member to stay skipped; today stand-in marks it run. Repeat with ext.reserve_percent 15 and a weekly snapshot at 97 percent used. Remedy: Evaluate on_request and reserve_hold before the light and weighty gates, and keep those two skip kinds. Do not promote or rejoin a row that is on_request or currently held.
- **F06-2** [minor] `crates/c3/src/consult/orchestrate/mod.rs:2035` - The protected-reserve refusal for an explicit --provider is skipped when --skip-preflight is set, so naming a held reviewer starts the run on a non-standing purpose without --spend-reserve. The walk and the panel still apply the hold. Trigger: A protected entry whose long window is over the reserve line, purpose diff-review, --provider and --model naming it, plus --skip-preflight, and no --spend-reserve. Evidence: read-code: The hold block requires preflight_refusal to be empty, skip_preflight false, spend_reserve false, a non-empty provider, and a non-standing purpose. skip_preflight short-circuits the whole refusal. Verify: With a 97 percent weekly snapshot on a reserve_percent 15 entry, run consult --provider <label> --model <id> --purpose diff-review --skip-preflight --dry-run and confirm it plans that reviewer. Remedy: Apply the reserve refusal even when credential preflight is skipped. Leave --spend-reserve and standing purposes as the only overrides.
- **F06-3** [major] `crates/c3/src/consult/orchestrate/mod.rs:1509`, `crates/c3/src/consult/orchestrate/mod.rs:1527`, `crates/c3/src/consult/orchestrate/mod.rs:3143` - A consult with --mode resume or --mode fork and neither --provider nor --thread still selects the reviewer by the purpose-aware routed draw, then select_parent_thread continues only that drawn lineage. The task's latest verified thread is not the parent. The run can fork a different reviewer than the last consult, or refuse because the drawn lineage has no thread while another lineage in the ledger does. Trigger: A task whose last consult was reviewer A with a live thread. Next command is c3 consult --mode resume --purpose diff-review with no --provider and no --thread, and the seeded draw picks reviewer B. Evidence: read-code: Only a non-empty provider or thread leaves the walk branch. Mode is not consulted there. Non-standing purposes call route_single_default before any parent lookup.; read-code: With an empty --thread, the automatic parent is the newest verified thread of the already chosen identity. Explicit resume or fork with no parent of that lineage returns an error. Verify: Ledger a diff-review on reviewer A, then dry-run --mode resume --purpose diff-review --panel-seed chosen so the draw picks B. Observe either B's lineage or the parent-thread refusal, not A's thread. Remedy: When mode is resume or fork and the user named neither provider nor thread, bind the reviewer from the latest verified ledger consult before the routed draw, or require --thread.
- **F06-4** [minor] `crates/c3/src/budget/mod.rs:343`, `crates/c3/src/budget/mod.rs:354`, `crates/c3/src/budget/mod.rs:455` - OpenAI fallback windows are built with stale false, so a used_percent up to about eight days old is compared with elapsed recomputed at now and also drives budget_multiplier. A low old share misses the reserve hold and boosts the seat. A non-empty two-day result is returned even when its reset has already passed, which skips the 8-day scan and the health-file fallback. A future observation timestamp wins merge_observation and can replace a real reading. The scan filters model_provider openai and does not separate accounts. Trigger: No rollout in the last two days, and a six-day-old weekly observation at about 20 percent used whose resets_at is still ahead, while current account usage is over the reserve line. Or a two-day rollout whose weekly resets_at is already past while a newer health record has the new window. Evidence: read-code: account_window_at returns immediately when windows_from_quota is non-empty, with no keep_future. The 8-day and recorded paths use keep_future. windows_from_quota always sets stale false.; read-code: budget_multiplier uses every non-stale window. merge_observation replaces a window when the candidate instant is strictly newer, with no future-timestamp cap. Verify: Point CODEX_HOME at a fixture whose only weekly rollout is six days old at 20 percent used with resets_at still ahead, reserve_percent 15, and assert reserve_hold is none and the multiplier is above 1. Then add a health record at 97 percent and assert the 8-day rollout still wins. Remedy: Treat a fallback window as stale when its observation is old relative to the period, and ignore it for both the hold and the multiplier. If every two-day window has resets_at in the past, fall through to the next source. Ignore observations whose time is in the future.

### Prior findings

- F02-1 - fixed - zai_get builds the agent with redirects(0) and accepts a body only when status < 300 (crates/c3/src/budget/mod.rs:620). A 3xx is no data, so the bearer token is not replayed.
- F02-4 - fixed - The model-swap smoke now holds the seat when seat_saturated, on_request, or reserve_percent is set before it builds the Target (crates/c3/src/maintain/mod.rs:1091). seat_saturated is the same 90 percent plan_headroom guard as pick_target.
- F03-9 - fixed - A read-insist LaunchFailed is stored as launch_fail and breaks the loop. It is not written into the dead-attempt record, and the comment keeps the guard reply and pointers (crates/c3/src/consult/orchestrate/secondary.rs:1396 and :1453).
- F03-12 - still-open - The smoke scratch repo is now under the OS temp dir and removed after the run (crates/c3/src/maintain/mod.rs:1026). redirect_target still treats any host with the same last two labels as the same site (crates/c3/src/maintain/env.rs:259), so a non-allowlisted kimi.ai subdomain is accepted.
- F04-7 - fixed - Same zai_get path as F02-1. redirects(0) plus status < 300 means a 302 body is not cached or parsed as TOKENS_LIMIT (crates/c3/src/budget/mod.rs:620-631).
- F04-8 - fixed - The Windows hook spawn is CreateProcessW of the c3 exe. The parent opens the log and passes the handle. The command line is only the quoted exe plus maintain --run, not cmd.exe /c (crates/c3/src/maintain/mod.rs:1316-1443). CODEX_HOME is lpCurrentDirectory, so & and | in it do not start another process.

## Verdict: HOLD

The ordinary protected-entry and on-request paths match the spec, but a light or weighty panel flag lets a held or on-request reviewer run, and a bare resume follows the new draw.

### Blockers

_(none)_

### Unproven scenarios

- The installed d7ce658 binary was not executed against a roster that contains the new ext keys.
- The operator's live roster was not read, so it is unknown whether the standing entry or Muse is panel light or panel weighty.
- No rollout fixture was built for the 8-day fallback, a future-dated observation, or two accounts in one CODEX_HOME.
- The full c3-cli integration suite result was not available.
- F03-9 was checked by reading the LaunchFailed branch only; the named fake-grok test was not run.

### First-run checklist (observable)

_(none)_
