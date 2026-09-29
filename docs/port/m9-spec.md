# M9 spec — router v1, priors, RC2 simulation, replay

Design of record: `docs/DESIGN.md` §5 (routing), §8 (telemetry and priors), §11 (RC2);
decisions in `.collab/c3-design/state.md` (maintainer server = telemetry and priors only).

## 1. Principle: one code path, the plugin's numbers by default

The plugin's routed draw (wave 26, R15) is already ported bit-exact in `crates/c3/src/panel/routing.rs`
(`routing_score`, `panel_seed`, `invoke_panel_draw`). Router v1 does NOT replace the draw. It replaces
only the **weight source**, and it is a strict generalisation of R15:

    w     = (yes + 0.5·partly + m·π) / (n + m)
    score = 0.25 + 1.75·w

R15 is the special case π = 0.5, m = 2 (`ROUTING_PRIOR` 0.5: `2p` / `4p`). With no priors loaded and
the default parameters, v1 MUST return the same `f64` bits as `panel::routing::routing_score` for
every input (property test over random rating sets). Parity harnesses therefore keep passing.

Parameters (`RouterParams`), every one recorded with the draw:

| name | default | meaning |
|---|---|---|
| `m_min` | 2.0 | prior strength with no priors (R15) |
| `kappa` | 0.1 | prior strength per unit of global support: `m = clamp(m_min, kappa·n_global, m_max)` (RC2 default; with no priors `n_global = 0` so `m = m_min`, i.e. still R15) |
| `m_max` | 8.0 | cap on the prior strength |
| `half_life_days` | 0 (off) | age decay of a local mark: weight `0.5^(age/half_life)` |
| `outcome_weight` | 0.0 | contribution of settled finding outcomes (verified +, rejected −) to a mark |
| `explore` | 0.2 | the plugin's `ROUTING_EXPLORE`, unchanged |

Defaults change only on the evidence of the RC2 report (§4), by the supervisor. The unrated count as
missing, never as zero.

## 2. Score with priors

1. Local basis exactly as R15: pooled `purpose+topics` (>= 3) → `purpose` (>= 3) → `all-purpose` (>= 3) → none.
2. Prior mean π and support for the lineage `(provider, model, engine)`: the most specific published
   cell — `(lineage, purpose, topic)` (several topics: support-weighted mean) → `(lineage, purpose)` →
   `(lineage)` → none (π = 0.5, the record says `prior_basis: "none"`).
3. Local basis present: the formula above with the local counts. Local basis none: n = 0, so
   `score = 0.25 + 1.75·π`; `basis` is `prior` when π came from a cell, `neutral` otherwise.

The ledger record stays the plugin's. C3's additions go under ONE trailing key of `panel.routing`,
mirroring the roster convention: `ext: { c3: { router: "v1", params: {...}, priors: { version,
generated, sha256, source: "hub" | "file", key_id }, prior_use: [ { position, prior_basis, mean,
support, m } ] } }`. With no priors loaded and default parameters C3 writes NO `ext` key, so the
record is byte-identical to the plugin's.

## 3. `priors.json`

    { "priors_version": 1, "generated": "<utc iso>", "window_days": 90,
      "cells": [ { "provider": "openai", "model": "gpt-6-astra", "engine": "codex",
                   "purpose": "decision", "topic": "", "mean": 0.71, "n": 42 } ] }

- `mean` = (yes + 0.5·partly)/n over installations, in [0,1]; `n` the support. Empty `purpose` = the
  lineage cell; empty `topic` = the purpose cell. The server publishes a cell only above its
  k-anonymity threshold; that is the server's rule, the client does not rely on it.
- Validation, fail closed: `priors_version == 1`; file <= 1 MiB; <= 10 000 cells; strings <= 128 bytes and
  without control characters; `mean` finite in [0,1]; `n` finite >= 0; unknown keys ignored. A file that
  fails validation is not used at all (no partial use).
- Signature: detached `priors.json.sig` = `{ "alg": "ed25519", "key_id": "<id>", "sig": "<base64 of the
  64-byte signature over the exact file bytes>" }`. Trusted keys are pinned in the binary
  (`PRIORS_KEYS: &[(&str /*key_id*/, &str /*base64 public key*/)]`, empty until the maintainer publishes
  one) plus an optional user key `C3_PRIORS_KEY=<key_id>:<base64>` (a public key; a user who runs their
  own hub makes that trust decision). A download whose signature does not verify against a trusted
  key is discarded and the previous verified copy stays.
- A local file is the user's own: `C3_PRIORS=file:<path>` loads it without a signature,
  recorded as `source: "file"`.
- Switches: `C3_PRIORS=off` disables priors entirely; `C3_PRIORS=<https url>` overrides the hub URL
  (default `https://xelth.com/T/v2/priors.json`, provisional until the hub serves it — a 404 is the
  normal answer today and means "no priors"); `CODEX_CONSULT_TELEMETRY=off` also disables the download
  (a user who turned the hub off gets no hub traffic at all).
- Download: a plain GET with `User-Agent: c3/<version>`, no instance id, no cookies, no query string;
  3 s total budget; at most once per 24 h (`If-None-Match` with the stored ETag); never blocks or fails
  a consultation — it runs where the telemetry flush runs. Only `https://` URLs.
- Cache: `<c3 home>/priors/{priors.json, priors.json.sig, priors.meta.json}` where the c3 home is the
  directory the telemetry spool already uses; meta = `{url, fetched, etag, sha256, key_id}`. A verified
  copy older than 30 days is ignored (stale). Written atomically (temp file + rename).

## 4. RC2 simulation — `c3 router simulate`

Offline, deterministic by `--seed`, no network, no `.collab` access.

- World: 10 reviewers in 6 labs, 6 purposes, 8 topics; true usefulness `θ[r,p,t]` from a reviewer ability
  plus purpose and topic effects (logit scale), seeded. A global population for the priors:
  `θ_global = θ + noise`, with a support per cell.
- Round: a purpose, 0–2 topics, a panel size k from the R14 defaults, availability 80 % per reviewer;
  the policy's weights feed the REAL `invoke_panel_draw`; each seat yields a mark yes/partly/no from θ;
  a mark is observed with probability ρ (default 0.6, the measured 53 of ~90) after a delay of 0–20 rounds.
- Policies: `uniform`; `r15`; `v1` without priors; `v1` with priors, sweeping `kappa` ∈ {0, 0.05, 0.1, 0.2}
  and `m_max` ∈ {4, 8, 16}; `v1` with `half_life_days` ∈ {0, 45, 90}; `thompson` (Beta posterior sample,
  regularised toward the prior).
- Metrics at horizons 30, 100, 300 rounds, averaged over `--runs` (default 200) seeds: mean usefulness
  per seat, cumulative regret against the oracle top-k, coverage (share of reviewers seated at least
  once per 50 rounds), distinct labs per panel. Also the same with misleading priors (the global
  population anti-correlated for 2 reviewers) — a prior must not be able to lock a good reviewer out.
- Output: a markdown report to stdout or `--out <file>`, plus `--json`. The worker commits nothing; it
  writes `docs/port/rc2-simulation.md` with the tables, the exact command line, and a recommendation.

## 5. Replay — `c3 router replay --task <task> [--nn <NN>]`

Recomputes the seats from the ledger's own `panel.routing` record (seed, size, eligible scores, labs,
required, the recorded `explore`/neutral of the policy) with `invoke_panel_draw`, compares with
`picked`/`explored` and prints `replay: identical` (exit 0) or the first difference (exit 1). It reads
only `sessions.json`; no priors file, no ratings, no network. A record written by the plugin replays too.

## 6. CLI

    c3 router simulate [--seed N] [--runs N] [--rounds N] [--out <file>] [--json]
    c3 router replay   --task <task> [--nn <NN>] [--collab-dir <dir>]
    c3 router priors   [status | fetch | clear]
    c3 router explain  --purpose <p> [--topic <t>]...     # the score table the next draw would use

`priors status` prints source, version, generated, age, cell count, key id, and why priors are off when
they are. `explain` is read-only and prints one line per roster lineage: score, basis, local counts,
prior mean/support/m.

## 7. Telemetry side (allowlist only)

Check `telemetry/event.rs`: the consultation event must carry the routing fields of DESIGN §8 (lineage,
purpose, topic tag from the FIXED vocabulary — anything else is sent as `other`, never the free text —
usefulness mark, verified/rejected counts, wall seconds, tokens, panel size, outcome class). A mark
given later by `c3 findings --rate` must reach the hub as its own allowlisted event
(`kind: "rating"`: lineage, purpose, topic tags, mark, age in days of the consultation — no ids, no text).
Add what is missing, inside the typed allowlist, with the seeded-secret privacy test extended.

## 8. Integration seam (NOT part of this chunk)

`panel/plan.rs` keeps calling `panel::routing::routing_score` until the supervisor wires
`router::score(...)` in. This chunk exposes, in `crates/c3/src/router/`:

    pub fn score(ratings, lineage, purpose, topics, now, ctx: &RouterContext) -> Scored   // base RoutingScore + Option<PriorUse>
    pub fn routing_ext(ctx: &RouterContext, uses: &[PriorUse]) -> Option<serde_json::Value>  // None = write no `ext`
    pub fn load_context() -> RouterContext   // params + verified priors or none; never fails, never blocks
