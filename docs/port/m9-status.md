# M9 status — router v1, priors, replay, RC2

Implements `docs/port/m9-spec.md`. One code path, the plugin's numbers by default: router v1
generalises R15 (`m`, `pi` from a global prior) and, with no priors and default params,
returns `panel::routing::routing_score` bit for bit, so the ledger record stays
byte-identical to the plugin's.

## What landed

- `crates/c3/src/router/params.rs` — `RouterParams` (`m_min=2`, `kappa=0`, `m_max=8`,
  `half_life_days=0`, `outcome_weight=0`, `explore=0.2`), `prior_strength` clamp, `is_default`.
- `crates/c3/src/router/score.rs` — `smoothed_score`, with a bit-parity unit test vs `routing_rate`.
- `crates/c3/src/router/priors.rs` — `PriorCell`/`Priors`, fail-closed `validate` (§3), and
  the `(lineage,purpose,topic) → (lineage,purpose) → (lineage) → neutral` lookup.
- `crates/c3/src/router/sign.rs` — detached ed25519 verify, pinned `PRIORS_KEYS` (empty) plus
  the optional `C3_PRIORS_KEY` user key. No private key material is ever stored or printed.
- `crates/c3/src/router/download.rs` — env switches, `https`-only, 24 h cadence, 30-day
  staleness, atomic cache writes, injectable `Fetcher` (real `UreqFetcher`).
- `crates/c3/src/router/mod.rs` — `score`, `routing_ext` (None ⇒ no `ext`), `load_context`
  (never fetches/blocks), `maybe_refresh_priors` (wired into the telemetry background flush).
- `crates/c3/src/router/replay.rs` — recompute a routed panel's seats from its own record.
- `crates/c3/src/router/simulate.rs` — the offline RC2 simulation.
- `crates/c3/src/cli/router.rs` — `c3 router simulate|replay|priors|explain`.
- `crates/c3/src/telemetry/event.rs` — the `rating` event, the fixed topic vocabulary and
  `topic_label`; `telemetry::record_rating` enqueues it.

## Notes

- The default `kappa` is `0.1` (RC2 decision D-a); with no priors loaded `n_global = 0`, so
  `m = m_min = 2` and the score stays R15 bit for bit and writes no `ext` key.
- `half_life_days` and `outcome_weight` are exercised only inside the simulator in this chunk;
  the real `router::score` path keeps them at their defaults (off), so behaviour is unchanged
  until the supervisor wires decayed/outcome-weighted local counts in a later chunk.

## Verification

See `docs/port/rc2-simulation.md` for the RC2 numbers and the recommended defaults.
