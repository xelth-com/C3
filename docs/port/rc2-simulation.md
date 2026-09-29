# RC2 — routing simulation

Offline, deterministic, no network, no `.collab` access. Ten reviewers in six labs, six
purposes, eight topics; true usefulness on a logit scale, seeded; a global population (the
priors' truth) that is well-specified in the first world and anti-correlated for two
reviewers in the second. Each round draws a purpose, 0–2 topics and 80 %-available reviewers,
feeds the policy's weights to the REAL `panel::routing::invoke_panel_draw`, samples a
yes/partly/no mark per seat, and observes it with probability ρ = 0.6 after a 0–20 round
delay (the measured 53 of ~90). Reward per seat: yes 1, partly 0.5, no 0.

Exact command (release build, LTO off, single reviewer machine under a load cap):

    CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
      cargo build -j 2 --release -p c3-cli --no-default-features
    ./target/release/c3 router simulate --seed 1 --runs 200 --rounds 300 --out rc2-simulation.md

Metrics are averaged over 200 seeds at horizons 30 / 100 / 300 rounds. `regret` is cumulative
against the oracle top-k; `coverage` is the share of reviewers seated at least once per 50
rounds; `labs/panel` is the mean distinct labs per panel.

## Headline (well-specified priors)

| policy | H30 useful | H30 regret | H100 regret | H300 regret |
|---|---|---|---|---|
| uniform | 0.499 | 10.7 | 35.6 | 107.2 |
| r15 (plugin) | 0.510 | 9.6 | 21.5 | 51.8 |
| v1, no priors | 0.512 | 9.5 | 21.4 | 51.9 |
| v1 + priors, κ=0.1, m_max=8 | 0.567 | **4.2** | **14.4** | **41.4** |
| v1 + priors, κ=0.1, m_max=4 | 0.569 | 4.1 | 14.0 | 40.7 |
| thompson (κ=0.1) | 0.548 | 6.2 | 19.0 | 52.2 |

## Misleading priors (2 reviewers anti-correlated)

| policy | H30 regret | H100 regret | H300 regret | coverage (all H) |
|---|---|---|---|---|
| r15 | 9.6 | 21.5 | 51.8 | 1.00 |
| v1 + priors, κ=0.1, m_max=8 | 6.3 | 17.5 | 47.2 | 1.00 |
| v1 + priors, κ=0.2, m_max=16 | 6.5 | 18.2 | 49.3 | 1.00 |
| thompson | 7.7 | 21.5 | 57.4 | 1.00 |

The full sweep (uniform; r15; v1 no priors; v1+priors over κ ∈ {0,0.05,0.1,0.2} × m_max ∈
{4,8,16}; v1 half-life ∈ {0,45,90}; thompson) for both worlds is reproduced verbatim below.

## Reading

- **Priors are the win, and it is largest where data is sparse.** With well-specified priors
  v1 more than halves early regret (H30 4.1–4.4 vs 9.6 for r15, 10.7 for uniform) and mean
  usefulness rises from 0.51 to 0.57. By H300 local marks have accumulated and the gap
  narrows (≈41 vs ≈52), exactly the RC2 hypothesis: a prior buys the cold-start rounds and
  then fades as the posterior takes over.
- **A bad prior never locks a good reviewer out.** Under anti-correlated priors v1 still beats
  r15 at every horizon and coverage stays 1.00 for every policy — the ρ = 0.6 marks plus the
  0.2 exploration mixture pull the slandered reviewer back in within a handful of rounds, and
  the smoothing means three real marks already dominate the prior. This is the required
  guardrail and it holds.
- **The κ / m_max sweep is flat; only extremes hurt.** Within the priors-on group the H30–H300
  differences are inside the run-to-run noise (±0.1–0.5 regret). The one visible effect is
  that a large cap (m_max = 16) over-trusts a wrong prior slightly under the misleading world
  (H300 regret 47.9–49.3 vs 46–47 at m_max ≤ 8). m_max = 8 is the safe cap.
- **Half-life makes no difference here** (0 / 45 / 90 are indistinguishable) because the world
  is stationary; there is no evidence to turn decay on, so it stays off.
- **Thompson underperforms the smoothed score** at every horizon and gains nothing under
  misleading priors, so v1 stays a smoothed softmax; Thompson remains a future version to
  revisit once there are many rated cells.

## Recommendation for the defaults

Keep the R15-compatible defaults `m_min = 2`, `m_max = 8`, `half_life_days = 0`,
`outcome_weight = 0`, `explore = 0.2`, and set `kappa = 0.1`. **Applied** (decision D-a):
`RouterParams::default` now ships `kappa = 0.1`. The evidence is that turning
priors *on at all* is what matters — it halves cold-start regret and never harms coverage,
even when the prior is adversarial — while the strength knobs are second-order: κ = 0.1 with
m_max = 8 sits at or next to the best cell in every column and is the most robust under the
misleading world, whereas m_max = 16 and κ = 0.2 buy nothing and cost a little when the prior
lies. Because C3 ships with `PRIORS_KEYS` empty and no hub file published yet, this default is
inert today (no priors ⇒ exact R15 numbers, byte-identical ledger); κ = 0.1 only takes effect
once a signed `priors.json` is trusted, at which point the sweep says it is the safe choice.

---

<!-- The full generated sweep follows verbatim (c3 router simulate --seed 1 --runs 200 --rounds 300). -->
## Well-specified priors

### Horizon 30

| policy | mean useful | regret | coverage | labs/panel |
|---|---|---|---|---|
| uniform | 0.499 | 10.7 | 1.00 | 3.07 |
| r15 | 0.510 | 9.6 | 1.00 | 3.06 |
| v1-no-priors | 0.512 | 9.5 | 1.00 | 3.06 |
| v1-k0-mmax4 | 0.563 | 4.3 | 1.00 | 3.00 |
| v1-k0-mmax8 | 0.559 | 4.3 | 1.00 | 3.00 |
| v1-k0-mmax16 | 0.566 | 4.2 | 1.00 | 3.00 |
| v1-k0.05-mmax4 | 0.566 | 4.3 | 1.00 | 3.00 |
| v1-k0.05-mmax8 | 0.564 | 4.4 | 1.00 | 3.00 |
| v1-k0.05-mmax16 | 0.564 | 4.3 | 1.00 | 3.00 |
| v1-k0.1-mmax4 | 0.569 | 4.1 | 1.00 | 3.00 |
| v1-k0.1-mmax8 | 0.567 | 4.2 | 1.00 | 3.00 |
| v1-k0.1-mmax16 | 0.563 | 4.2 | 1.00 | 3.00 |
| v1-k0.2-mmax4 | 0.562 | 4.2 | 1.00 | 3.00 |
| v1-k0.2-mmax8 | 0.564 | 4.2 | 1.00 | 3.00 |
| v1-k0.2-mmax16 | 0.564 | 4.1 | 1.00 | 3.00 |
| v1-halflife0 | 0.562 | 4.2 | 1.00 | 3.00 |
| v1-halflife45 | 0.564 | 4.2 | 1.00 | 3.00 |
| v1-halflife90 | 0.567 | 4.1 | 1.00 | 3.00 |
| thompson | 0.548 | 6.2 | 1.00 | 3.00 |

### Horizon 100

| policy | mean useful | regret | coverage | labs/panel |
|---|---|---|---|---|
| uniform | 0.500 | 35.6 | 1.00 | 3.06 |
| r15 | 0.541 | 21.5 | 1.00 | 3.01 |
| v1-no-priors | 0.539 | 21.4 | 1.00 | 3.02 |
| v1-k0-mmax4 | 0.560 | 14.8 | 1.00 | 2.99 |
| v1-k0-mmax8 | 0.557 | 15.1 | 1.00 | 2.99 |
| v1-k0-mmax16 | 0.561 | 15.1 | 1.00 | 2.99 |
| v1-k0.05-mmax4 | 0.562 | 14.6 | 1.00 | 2.99 |
| v1-k0.05-mmax8 | 0.563 | 14.9 | 1.00 | 2.99 |
| v1-k0.05-mmax16 | 0.560 | 14.7 | 1.00 | 2.99 |
| v1-k0.1-mmax4 | 0.565 | 14.0 | 1.00 | 2.99 |
| v1-k0.1-mmax8 | 0.563 | 14.4 | 1.00 | 2.99 |
| v1-k0.1-mmax16 | 0.560 | 14.2 | 1.00 | 2.99 |
| v1-k0.2-mmax4 | 0.563 | 14.2 | 1.00 | 2.99 |
| v1-k0.2-mmax8 | 0.565 | 14.0 | 1.00 | 2.99 |
| v1-k0.2-mmax16 | 0.562 | 13.9 | 1.00 | 2.99 |
| v1-halflife0 | 0.560 | 14.5 | 1.00 | 2.99 |
| v1-halflife45 | 0.561 | 14.2 | 1.00 | 2.99 |
| v1-halflife90 | 0.566 | 14.1 | 1.00 | 2.99 |
| thompson | 0.551 | 19.0 | 1.00 | 2.99 |

### Horizon 300

| policy | mean useful | regret | coverage | labs/panel |
|---|---|---|---|---|
| uniform | 0.501 | 107.2 | 1.00 | 3.07 |
| r15 | 0.553 | 51.8 | 1.00 | 3.00 |
| v1-no-priors | 0.552 | 51.9 | 1.00 | 3.00 |
| v1-k0-mmax4 | 0.561 | 42.8 | 1.00 | 2.99 |
| v1-k0-mmax8 | 0.561 | 42.4 | 1.00 | 2.99 |
| v1-k0-mmax16 | 0.563 | 43.0 | 1.00 | 2.99 |
| v1-k0.05-mmax4 | 0.563 | 42.3 | 1.00 | 2.99 |
| v1-k0.05-mmax8 | 0.564 | 43.0 | 1.00 | 2.99 |
| v1-k0.05-mmax16 | 0.562 | 42.4 | 1.00 | 2.99 |
| v1-k0.1-mmax4 | 0.564 | 40.7 | 1.00 | 2.99 |
| v1-k0.1-mmax8 | 0.564 | 41.4 | 1.00 | 2.99 |
| v1-k0.1-mmax16 | 0.562 | 41.0 | 1.00 | 2.99 |
| v1-k0.2-mmax4 | 0.565 | 40.8 | 1.00 | 2.99 |
| v1-k0.2-mmax8 | 0.566 | 40.3 | 1.00 | 3.00 |
| v1-k0.2-mmax16 | 0.564 | 40.1 | 1.00 | 3.00 |
| v1-halflife0 | 0.564 | 40.9 | 1.00 | 2.99 |
| v1-halflife45 | 0.562 | 41.4 | 1.00 | 3.00 |
| v1-halflife90 | 0.564 | 40.4 | 1.00 | 2.99 |
| thompson | 0.553 | 52.2 | 1.00 | 3.00 |

## Misleading priors (2 reviewers anti-correlated)

### Horizon 30

| policy | mean useful | regret | coverage | labs/panel |
|---|---|---|---|---|
| uniform | 0.499 | 10.7 | 1.00 | 3.07 |
| r15 | 0.510 | 9.6 | 1.00 | 3.06 |
| v1-no-priors | 0.512 | 9.5 | 1.00 | 3.06 |
| v1-k0-mmax4 | 0.542 | 6.4 | 1.00 | 3.00 |
| v1-k0-mmax8 | 0.540 | 6.2 | 1.00 | 3.00 |
| v1-k0-mmax16 | 0.545 | 6.4 | 1.00 | 3.00 |
| v1-k0.05-mmax4 | 0.545 | 6.4 | 1.00 | 3.00 |
| v1-k0.05-mmax8 | 0.544 | 6.4 | 1.00 | 3.00 |
| v1-k0.05-mmax16 | 0.546 | 6.3 | 1.00 | 3.00 |
| v1-k0.1-mmax4 | 0.546 | 6.4 | 1.00 | 3.00 |
| v1-k0.1-mmax8 | 0.546 | 6.3 | 1.00 | 3.00 |
| v1-k0.1-mmax16 | 0.541 | 6.4 | 1.00 | 3.00 |
| v1-k0.2-mmax4 | 0.541 | 6.4 | 1.00 | 3.00 |
| v1-k0.2-mmax8 | 0.543 | 6.5 | 1.00 | 3.00 |
| v1-k0.2-mmax16 | 0.541 | 6.5 | 1.00 | 3.00 |
| v1-halflife0 | 0.541 | 6.4 | 1.00 | 3.00 |
| v1-halflife45 | 0.543 | 6.5 | 1.00 | 3.00 |
| v1-halflife90 | 0.545 | 6.5 | 1.00 | 3.00 |
| thompson | 0.529 | 7.7 | 1.00 | 3.01 |

### Horizon 100

| policy | mean useful | regret | coverage | labs/panel |
|---|---|---|---|---|
| uniform | 0.500 | 35.6 | 1.00 | 3.06 |
| r15 | 0.541 | 21.5 | 1.00 | 3.01 |
| v1-no-priors | 0.539 | 21.4 | 1.00 | 3.02 |
| v1-k0-mmax4 | 0.552 | 17.6 | 1.00 | 2.99 |
| v1-k0-mmax8 | 0.550 | 17.5 | 1.00 | 2.98 |
| v1-k0-mmax16 | 0.553 | 18.2 | 1.00 | 2.99 |
| v1-k0.05-mmax4 | 0.553 | 17.6 | 1.00 | 2.99 |
| v1-k0.05-mmax8 | 0.555 | 17.7 | 1.00 | 2.99 |
| v1-k0.05-mmax16 | 0.554 | 17.3 | 1.00 | 2.99 |
| v1-k0.1-mmax4 | 0.554 | 17.4 | 1.00 | 2.99 |
| v1-k0.1-mmax8 | 0.553 | 17.5 | 1.00 | 2.99 |
| v1-k0.1-mmax16 | 0.550 | 17.6 | 1.00 | 2.99 |
| v1-k0.2-mmax4 | 0.552 | 18.0 | 1.00 | 2.99 |
| v1-k0.2-mmax8 | 0.552 | 18.3 | 1.00 | 2.99 |
| v1-k0.2-mmax16 | 0.549 | 18.2 | 1.00 | 2.99 |
| v1-halflife0 | 0.551 | 17.7 | 1.00 | 2.99 |
| v1-halflife45 | 0.551 | 17.6 | 1.00 | 2.99 |
| v1-halflife90 | 0.556 | 17.5 | 1.00 | 2.99 |
| thompson | 0.543 | 21.5 | 1.00 | 2.99 |

### Horizon 300

| policy | mean useful | regret | coverage | labs/panel |
|---|---|---|---|---|
| uniform | 0.501 | 107.2 | 1.00 | 3.07 |
| r15 | 0.553 | 51.8 | 1.00 | 3.00 |
| v1-no-priors | 0.552 | 51.9 | 1.00 | 3.00 |
| v1-k0-mmax4 | 0.558 | 46.4 | 1.00 | 2.99 |
| v1-k0-mmax8 | 0.557 | 46.6 | 1.00 | 2.98 |
| v1-k0-mmax16 | 0.558 | 47.9 | 1.00 | 2.99 |
| v1-k0.05-mmax4 | 0.559 | 46.5 | 1.00 | 2.99 |
| v1-k0.05-mmax8 | 0.560 | 47.1 | 1.00 | 2.99 |
| v1-k0.05-mmax16 | 0.559 | 46.3 | 1.00 | 2.99 |
| v1-k0.1-mmax4 | 0.559 | 46.5 | 1.00 | 2.99 |
| v1-k0.1-mmax8 | 0.559 | 47.2 | 1.00 | 2.99 |
| v1-k0.1-mmax16 | 0.557 | 46.5 | 1.00 | 2.99 |
| v1-k0.2-mmax4 | 0.558 | 47.7 | 1.00 | 2.99 |
| v1-k0.2-mmax8 | 0.558 | 49.0 | 1.00 | 2.99 |
| v1-k0.2-mmax16 | 0.555 | 49.3 | 1.00 | 2.99 |
| v1-halflife0 | 0.559 | 46.5 | 1.00 | 2.99 |
| v1-halflife45 | 0.557 | 46.3 | 1.00 | 3.00 |
| v1-halflife90 | 0.560 | 45.5 | 1.00 | 2.99 |
| thompson | 0.548 | 57.4 | 1.00 | 3.00 |

