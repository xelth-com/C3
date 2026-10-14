Write in English.

# Handoff 45 - claude: wave 8 "quota windows" (the plugin's wave 30 ported to C3) - diff review

Date: 2026-10-14. Base commit: `bf1bd4a` (branch `main`; version still 0.2.0 - to become 0.2.1 on your verdict).
The range under review is `a7f2626..bf1bd4a` (the v0.2.0 tag to the merged wave 8); skip `.collab/`. The wave
note: `docs/port/wave8-quota.md`; `CHANGELOG.md` `[Unreleased]`; README section "Quota windows and the reading
budget"; the reference is the plugin's wave 30 (claude-codex-consult `bade7cb..a162b8d`, its README section of the
same name and its `tests/harness-quota.ps1`, which C3 now passes 57/0 through the shim).

## Question

Is wave 8 a faithful port of the plugin's quota-window behaviour, safe to tag as C3 0.2.1? ACCEPT, HOLD (blockers
by id) or ADVISE. Context you lived through: your own first acceptance attempt (n=20) spent 96 % of a 5-hour
window in 186 s (a forked 1.2 MB thread, three sub-agents); the rounds after the plugin's wave 30 cost 15-23 % each.

## Delta (`crates/c3-core/src/{quota.rs (new), health.rs, verdict.rs, ledger.rs, engine.rs, handoff.rs}`, `crates/c3/src/{engines/{subprocess,codex}.rs, consult/*, panel/run.rs, providers.rs, telemetry/classes.rs}`, `c3-cli/src/main.rs`, `tests/shim`)

- **The rollout reader:** after every codex run the thread's rollout (`<codex home>/sessions/YYYY/MM/DD/rollout-<ts>-<thread>.jsonl`,
  newest match) gives the LAST `rate_limits` (primary = 5-hour, secondary = weekly) and the token totals; sub-agent
  rollouts (session_meta `subagent.thread_spawn.parent_thread_id`) are summed. Ledger keys `quota`, `tokens`
  (input, cached, output, thread_input, calls, max_input, context_window, subagents), `subagents_allowed`,
  `budget` - appended after `commit_wait_ms` (harness-0.3's key order 229/0).
- **Verdicts:** a window >= 98 % with its reset ahead = `unavailable (5-hour window 99 % used, resets <iso>)`, treated
  like a usage limit with a known reset in the roster walk, `--require`, panel, providers and the short line; >= 80 %
  = `low quota: ...`; a reset in the past = `5h reset (last 96% until 18:32)`; the built-in openai endpoint also takes
  the codex home's newest rollout (today/yesterday) as an observation; `c3 providers` QUOTA column last, JSON `quota`.
- **Replay guard:** `--mode fork|resume` with a parent rollout over `CODEX_CONSULT_FORK_MAX_KB` (512) is refused
  before anything starts (the plugin's text); 128 KB..limit warns; the partial's hint suggests `--mode new`.
- **Sub-agents off:** `-c features.multi_agent=false` right after `-m` unless the user's codex config names the key.
- **`--budget <percent>`** (env `CODEX_CONSULT_QUOTA_BUDGET`, default 90, 0 off): the stall watch reads the rollout
  every 15 s and stops the main turn at the budget (partial kept, no continuation, `failed: stopped at the quota
  budget: ...`, telemetry class `quota`); detached runs and panel members the same. `StopKind::Budget`.
- **Reading discipline** block in the diff-review, acceptance, checkpoint and core-contract prompts (the plugin's
  sentences); the dry run prints `quota :`, `budget :`, `sub-agents :`, `replay :`.
- Behaviour change noted by the worker: a kicked codex run's partial footer now names the run's own thread for a
  resume, as the plugin does.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | bf1bd4a | 0 | 811 (795 + 16); a run under a 100 % CPU load had 11 timing failures, each green alone; clean | completed |
| harnesses at plugin HEAD 33ba041 (shim, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | bf1bd4a | 0 | quota 57/0, 0.3 229/0, claude 87/0, roster 125/0, host 59/6 (n/a rows), fixes 56/0, telemetry 47/96 (P7) | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff a7f2626..bf1bd4a --stat`, then hunks of one path at a time
(`quota.rs` and the subprocess budget watch first); at most 10 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** The verdict rules and the budget stop: a state where the bridge still starts or continues a run the window
  cannot carry, or stops one it should not (a past `resets_at`, a null `primary`, a rollout of another provider)?
- **Q2.** The ledger keys and the handoff header: any interchange risk with plugin 0.6.1 readers (which ignore the
  four new keys) or with the plugin's wave 30 (same names, same order)?
- **Q3.** Verdict: ACCEPT (tag v0.2.1), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 500 words.
