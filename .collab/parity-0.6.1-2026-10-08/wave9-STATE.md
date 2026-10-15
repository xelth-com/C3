# Wave 9 state (the four consilium defects, 0.2.2 candidate)
Doc: docs/port/wave9-consilium-defects.md. All on main (no tag, no version bump).

## Done
- D1 panel default `--mode new` (header line `mode: ...`); plugin-compat (`C3_PLUGIN_COMPAT=1`, set by tests/shim) keeps the automatic fork because harness-roster PANEL pins it.
- D2 c3 spellings via `c3_core::spelling` (detach come-back, -List line, findings hint, providers hints); plugin spelling under the compat switch (harness-detach / roster pin the .ps1 texts).
- D3 detached single run: task lock only for bookkeeping (numbers + member-style pending record), commit under the write lock; `Recovery.concurrent`.
- D4 `ext.c3.plan_caps` / `reset_hint` per roster entry; `plan_cap_verdict` in `Ctx::plan_verdict`; `EndpointHealth.instant_429s`, `Record.wall_secs`.

## Decided
- One compat switch (`C3_PLUGIN_COMPAT`) rather than editing the plugin harness: the shims set it.
- A foreground single run still holds the lock for the whole run (only `--detach` changed).

## Next
- The coordinator bumps 0.2.2 at acceptance; the plugin could adopt the D2/D3/D4 rules if wanted.

## Wave 9b (the review, handoff 66)
- Done: F66-1 (concurrent = live writer + ordinary state + no survivors/unverified/kill_unconfirmed), F66-2 (persist_reservation: atomic + read-back, lock held on failure), F66-3 (MachineEndpoint.consult_id; hits deduped), F66-4 (ASCII-only hint), F66-5 (Invoke-C3Compat scoped env), F66-6 (policy documented + pinned).
- Decided: legacy machine rows without an id are matched to a ledger row by endpoint within 5 s; no redesign of the cap heuristic.
- Next: Astra re-review; the coordinator bumps 0.2.2 at acceptance.

## Wave 9c (round 2, handoff 68)
- Done: F68-1 hit dedupe by identity only (no time windows; legacy rows need endpoint + failure text + the second to agree), F68-2 portable sleeper in the admission test.
- Next: Astra round 3; the coordinator bumps 0.2.2 at acceptance.

## Wave 9d (acceptance, handoff 72)
- Done: F72-1 (24 h read-time bound, CAP_PAIR_MAX_AGE_MINUTES), F72-2 (no-id rows take no part in the pair; the message comparison is gone). No-hint outage = until the pair is 24 h old.
- Next: Astra re-acceptance; the coordinator bumps 0.2.2.
