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
