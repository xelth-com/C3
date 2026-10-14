# Wave 8 - quota windows (the plugin's wave 30, T15)

Branch `wave-8-quota` from main b2e9e88 (v0.2.0 + state). Worker: Opus. Spec: the plugin's
`bade7cb..a162b8d` (README "Quota windows and the reading budget", tests/harness-quota.ps1 the oracle).
Details and the port table: `docs/port/wave8-quota.md`.

## Done
- Commits: a273ff3 (core: `c3_core::quota`, health window + machine record, verdict, ledger keys,
  engine agent_config / StopKind::Budget), 16375f5 (runtime: --budget, replay guard, sub-agents
  off, run quota in ledger/handoff/summary/machine record, budget stop, QUOTA column, live source,
  telemetry class, reading discipline, dry-run lines, shim -Budget), and the docs commit (README
  section, CHANGELOG [Unreleased], skills, harness-shim note, wave8-quota.md, this file).
- Harnesses at the plugin's HEAD 33ba041 (pin `%TEMP%\c3-wave8\pin`, binary `%TEMP%\c3-wave8\c3.exe`,
  driver `%TEMP%\c3-wave8\drive.ps1`, logs `%TEMP%\c3-wave8\logs\`): quota 57/0, 0.3 229/0,
  claude 87/0 (also 87/0 from the checkout's own tests), roster 125/0, host 59/6 (n/a set),
  fixes 56/0, telemetry 47/96 (P7).
- cargo: 811 tests (795 + 16); clean except 11 load timings in the last full run (100 % CPU from
  other sessions), each binary green on a sequential rerun; clippy --all-targets -D warnings clean;
  rustfmt clean.

## Decided
- A new module `c3-core/src/quota.rs`; token_count lines parsed as JSON.
- The live source (the codex home's newest rollout for builtin openai) is turned on at c3 start when
  the machine-wide health is on; c3-core units never turn it on.
- A kicked codex run's partial footer names its own thread (the plugin's `$threadId` first).
- RC7's harness-claude E2 at HEAD: not reproducible (identical harness/fake at v0.6.1 and HEAD; two
  HEAD runs 87/0) - a transient in that pass, not a T15 behaviour.
- Disk: 12 GB of idle 9eck.com deps (older than 2026-10-07) + C3 target/debug/incremental deleted
  (C: was at 0.8 GB). A stray `find` (pid 21856, started 00:45) was eating a core - not touched.

## Next
- Coordinator: diff-review of `main..wave-8-quota`, merge, re-run RC suite as needed.

## Wave 8g (F58-1, on main)
- Done: 8b F46-1, 8c F48-1, 8d F50-1, 8e F52-1/2, 8f F55-1/2, 8g F58-1 (per-window observation
  instants: Observation.primary_at/secondary_at, EndpointWindow/AccountWindow.window_at; the
  aggregate observed_at is derived and display-only; an invalid window has no instant).
- Decided: a candidate window is compared only with the instant of the retained window of that kind.
- Next: Astra round 7; the plugin's T17 must mirror per-window rank, one instant, finite
  percentages, the raw account cache and per-window instants.
