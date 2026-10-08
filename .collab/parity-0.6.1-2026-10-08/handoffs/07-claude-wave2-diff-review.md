Write in English.

# Handoff 07 - claude: wave 2 of the compatibility track (telemetry) - diff review

Date: 2026-10-09. Base commit: `3605395` (branch `main`; the range under review is `e508a76..3605395` - wave 2 in
three commits 3d33a54 / 136cbf8 / b1ba064; skip `.collab/`). Plan: `state.md` P1-P6; your framing 02 (F02-1..F02-4
are this wave's).

## Question

Wave 2 brings C3's telemetry to the plugin's 0.6.1 contract and answers F02-1..F02-4. Is it a faithful port, is the
outbox sound, and - the design question you raised in 02/Q3 - should C3 keep its OWN telemetry identity (app id
`c3`, its own root `<codex home>/c3/telemetry/`, salt, instance id) or share the plugin's (`codex-consult`,
`~/.codex/telemetry-spool`, the plugin's salt and instance)? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta

- **2a - the 0.6.1 rating semantics and the ledger shape** (`ledger.rs`, `findings.rs`, `findings_tool/mod.rs`,
  `orchestrate.rs`, `dryrun.rs`, `telemetry/{classes,backfill,event,mod}.rs`, `cli/*`): the ledger entry gets
  `consult_ref` (random GUID after `consult_id`; a panel member its own), `context_window` (from `-c
  model_context_window`) and `compactions`, in the plugin's key order; `--rate` saves `rating_rev` (1 + the
  entry's highest, under the task lock) and `judge {provider, model, source}` resolved at rating time
  (`CODEX_CONSULT_COORDINATOR` of the rating process -> `rating_actor`; else the entry's coordinator ->
  `consult_coordinator`; else other/other/unknown) in the mark; the mark is committed before the spool, then
  `telemetry_sent`; `c3 telemetry --backfill-ratings [--dry-run]` sends marks without `telemetry_sent` with their
  own judge/rev/time. Rating details as sent: `engine, provider, model, purpose, mark, age_days, bridge_version,
  os, ps_version:"unknown", judge{...}, rating_rev?, consult_ref?`, `client_time` = the mark's `when`; the
  consultation details end with `consult_ref`. The dry-run preview in the plugin's order (`topics`, `role`,
  `format_retry`).
- **2b - durability** (`spool.rs` rewritten as an outbox, `complaint.rs`, `cli/telemetry.rs`): every append and
  the sender's read+rewrite hold an OS lock on `spool.lock`; a separate `flush.lock` admits one sender; the
  sender reads a snapshot, posts, then removes exactly the delivered/dropped lines from the CURRENT file and swaps
  atomically - an event appended during the POST stays queued (tested); a crash before the swap leaves the old
  file (worst case a duplicate); the 7-day drop counts from queue time. `forget-me`: a failed DELETE keeps a
  pending-deletion record (salt, instance, refs) until the intake acknowledged; `--local` only after a confirmed
  delete or without a stored reference.
- **2c - classes and the shim** (`classes.rs`, `event.rs`, the shims): reviewer, judge, purpose and outcome
  classes from the plugin's closed vendor-host and model lists; C3's extension: OpenRouter host -> `openrouter`,
  a `vendor/model` id only when the vendor is in the closed list, else `other`; `safe_label` can no longer let a
  private label out; `tests/shim/codex-telemetry.ps1` (`-Status`, `-Forget`, `-Local`, `-Yes`, `-PublicRef`,
  `-BackfillRatings`, `-DryRun`); `docs/port/wave2-telemetry.md`, README telemetry section, harness-shim.md.
- Behaviour changes: a run's `--telemetry` wins over the environment (as the plugin); panels pass the switch to
  members; a usable consultation is no longer sent as `failed:unknown`; `forget-me` without a stored reference
  needs `--local`.

## Known differences / not in this wave

- C3 keeps its own telemetry root and identity (the question above); `ps_version` is `unknown`; `topic_tags`
  dropped from the rating event to match the plugin's exact details; C3 flushes at the start of the next
  consultation, not right after the commit; detached runs still force telemetry off.
- The sender lacks the plugin's 429/400/413 handling, the 60 s deadline and the lock owner record (older gap).
- Launcher quoting: Rust's std quotes any argument containing `=` for a `.cmd` launcher (`-c
  "model_context_window=256000"`, `"-p="`); fix via `raw_arg` in the shared spawn path - wave 2b.
- The per-producer not-spooled count and the `.last` fold: wave 3.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test --workspace -j 2` etc., after each commit | b1ba064 | 0 | 576 (558 + 18); clean | completed |
| harness-telemetry (shim v0.6.1 + the telemetry shim) | `run-all.ps1 -ScriptsDir ... -Only harness-telemetry` | b1ba064 | 1 | 45/98 (RC2: 23/20 with a crash); 81 of the 98 read the plugin's own spool/salt/instance/app allowlist, 3 wave 3, 11 older (COMPLAIN x10, HOOK), 2 docs, 1 shim | completed |
| harness-roster | the same | b1ba064 | 1 | 124/1 (WALK, RATE x2 green; FILE = wave 4) | completed |
| harness-0.3 / companions / engines / muse | the same | b1ba064 | 1 | 227/2, 35/7, 87/10, 62/12 (the LEDGER/ROLE/RUN orders green; the rest = older gaps, wave 2b triage) | completed |
| harness-format / fixes28b / fixes28c / panel | the same | b1ba064 | - | 37/0, 13/7, 13/2 (COMPACT x3 green), 60/2 | completed |

## Open findings

F02-1..F02-4 `implemented` (wave 2); F02-7 `implemented` (wave 1b); F06-1 minor (Samoa fixture, wave 3).

## Questions

- **Q1.** The outbox: a path that loses or re-sends an event beyond "a duplicate after a crash before the swap"
  (two producers, the sender and `forget-me`, the 7-day drop)?
- **Q2.** The judge and consult_ref: a path by which a label, a host or the raw coordinator value reaches an
  event; is `ps_version: "unknown"` acceptable for the intake?
- **Q3.** The identity: own `c3` app/root/salt/instance (two bridges on one machine = two instances; the plugin's
  harness-telemetry can then never be C3's oracle) versus sharing the plugin's spool and identity (one instance,
  per-event `app_id`; the plugin's sender would deliver C3's events and vice versa). Which, and what must hold?
- **Q4.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 700 words.
