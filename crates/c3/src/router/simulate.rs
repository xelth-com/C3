//! `c3 router simulate` (M9 §4): the RC2 offline routing simulation.
//!
//! Deterministic by `--seed`, no network, no `.collab` access. It builds a seeded world of
//! reviewers with a true usefulness on a logit scale, runs each policy through rounds of
//! sparse, delayed marks (the measured 53 of ~90 observed), and reports mean usefulness,
//! cumulative regret against the oracle top-k, coverage and lab diversity at horizons 30, 100
//! and 300, averaged over `--runs` seeds. All randomness is derived from SHA-256 (the repo's
//! own uniform source), so no `rand` dependency is added and every run is reproducible.

use sha2::{Digest, Sha256};

use super::priors::{PriorCell, Priors};
use super::score::smoothed_score;
use super::RouterParams;
use crate::panel::routing::{
    invoke_panel_draw, uniform53, Candidate, ROUTING_EXPLORE, ROUTING_NEUTRAL,
};

// --------------------------------------------------------------------------- world constants

const N_REV: usize = 10;
const N_LAB: usize = 6;
const N_PUR: usize = 6;
const N_TOP: usize = 8;
/// Ordered-logit cutpoints separating no / partly / yes.
const C1: f64 = -0.6;
const C2: f64 = 0.6;
/// Availability per reviewer per round.
const AVAIL: f64 = 0.8;
/// Max observation delay (rounds).
const MAX_DELAY: u64 = 20;
/// The two anti-correlated reviewers in the misleading-priors world.
const MISLEAD: [usize; 2] = [0, 3];

/// The default R14 panel size per purpose index (0..N_PUR); `0` = every available reviewer.
fn default_size(purpose: usize) -> usize {
    match purpose {
        0 => 1, // checkpoint
        1 => 2, // diff-review
        2 => 3, // framing
        3 => 3, // decision
        4 => 4, // core-contract / acceptance
        _ => 0, // stuck: all
    }
}

// --------------------------------------------------------------------------- PRNG (SHA-256)

/// A counter-based PRNG: `uniform53(sha256(seed || counter))`. No `rand` dependency.
struct Rng {
    seed: [u8; 32],
    counter: u64,
}

impl Rng {
    fn new(label: &str) -> Self {
        Rng {
            seed: Sha256::digest(label.as_bytes()).into(),
            counter: 0,
        }
    }
    fn u(&mut self) -> f64 {
        let mut buf = [0u8; 40];
        buf[..32].copy_from_slice(&self.seed);
        buf[32..].copy_from_slice(&self.counter.to_be_bytes());
        self.counter += 1;
        let h = Sha256::digest(buf);
        uniform53(&h, 0)
    }
    /// A standard normal via Box-Muller.
    fn normal(&mut self) -> f64 {
        let u1 = self.u().max(1e-12);
        let u2 = self.u();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
    fn range(&mut self, n: usize) -> usize {
        ((self.u() * n as f64).floor() as usize).min(n - 1)
    }
}

fn logistic(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// The expected reward (`0*no + 0.5*partly + 1*yes`) for a latent mean `mu`.
fn expected_reward(mu: f64) -> f64 {
    1.0 - 0.5 * logistic(C2 - mu) - 0.5 * logistic(C1 - mu)
}

// --------------------------------------------------------------------------- world

struct World {
    /// True latent usefulness `mu[r][p][t]` (logit scale).
    mu: Vec<Vec<Vec<f64>>>,
    /// The global population's latent usefulness (the priors' truth).
    mu_global: Vec<Vec<Vec<f64>>>,
    lab: Vec<usize>,
}

impl World {
    fn build(seed: u64, mislead: bool) -> World {
        let mut rng = Rng::new(&format!("world:{seed}"));
        let ability: Vec<f64> = (0..N_REV).map(|_| 0.9 * rng.normal()).collect();
        let pur: Vec<f64> = (0..N_PUR).map(|_| 0.4 * rng.normal()).collect();
        let top: Vec<f64> = (0..N_TOP).map(|_| 0.4 * rng.normal()).collect();
        let lab: Vec<usize> = (0..N_REV).map(|r| r % N_LAB).collect();

        let mut mu = vec![vec![vec![0.0; N_TOP]; N_PUR]; N_REV];
        let mut mu_global = mu.clone();
        for r in 0..N_REV {
            for p in 0..N_PUR {
                for t in 0..N_TOP {
                    let base = ability[r] + pur[p] + top[t] + 0.3 * rng.normal();
                    mu[r][p][t] = base;
                    let g = base + 0.5 * rng.normal();
                    mu_global[r][p][t] = if mislead && MISLEAD.contains(&r) {
                        -base
                    } else {
                        g
                    };
                }
            }
        }
        World { mu, mu_global, lab }
    }

    /// The latent mean for a consultation of `purpose` over a topic set (mean over topics, or
    /// the topic-averaged purpose value when no topic is named).
    fn latent(&self, table: &[Vec<Vec<f64>>], r: usize, p: usize, topics: &[usize]) -> f64 {
        if topics.is_empty() {
            let s: f64 = table[r][p].iter().sum();
            s / N_TOP as f64
        } else {
            let s: f64 = topics.iter().map(|&t| table[r][p][t]).sum();
            s / topics.len() as f64
        }
    }

    fn expected(&self, r: usize, p: usize, topics: &[usize]) -> f64 {
        expected_reward(self.latent(&self.mu, r, p, topics))
    }

    /// The signed priors population built from `mu_global`: a `(lineage, purpose)` cell and a
    /// `(lineage, purpose, topic)` cell for each reviewer, with a modest support.
    fn priors(&self) -> Priors {
        let mut cells = Vec::new();
        for r in 0..N_REV {
            for p in 0..N_PUR {
                let purpose_mu = self.latent(&self.mu_global, r, p, &[]);
                cells.push(PriorCell {
                    provider: format!("lab{}", self.lab[r]),
                    model: format!("rev{r}"),
                    engine: "codex".into(),
                    purpose: format!("p{p}"),
                    topic: String::new(),
                    mean: expected_reward(purpose_mu),
                    n: 30.0,
                });
                for t in 0..N_TOP {
                    cells.push(PriorCell {
                        provider: format!("lab{}", self.lab[r]),
                        model: format!("rev{r}"),
                        engine: "codex".into(),
                        purpose: format!("p{p}"),
                        topic: format!("t{t}"),
                        mean: expected_reward(self.mu_global[r][p][t]),
                        n: 20.0,
                    });
                }
            }
        }
        Priors {
            priors_version: 1,
            generated: String::new(),
            window_days: 90,
            cells,
        }
    }
}

// --------------------------------------------------------------------------- policies

#[derive(Clone)]
enum Kind {
    Uniform,
    Smoothed { params: RouterParams, priors: bool },
    Thompson { params: RouterParams, priors: bool },
}

#[derive(Clone)]
struct Policy {
    name: String,
    kind: Kind,
}

/// One observed mark in a reviewer's history.
#[derive(Clone)]
struct Obs {
    purpose: usize,
    topics: Vec<usize>,
    /// yes = 1.0, partly = 0.5, no = 0.0.
    reward: f64,
    round: u64,
}

/// Local basis counts (yes-equivalent weight, count) with optional half-life decay, mirroring
/// `panel::routing::routing_score`'s hierarchy: purpose+topics (>=3) → purpose (>=3) →
/// all-purpose (>=3) → none.
fn basis_counts(
    hist: &[Obs],
    purpose: usize,
    topics: &[usize],
    now: u64,
    half_life: f64,
) -> (f64, f64, f64) {
    let w = |o: &Obs| -> f64 {
        if half_life > 0.0 {
            0.5f64.powf((now.saturating_sub(o.round)) as f64 / half_life)
        } else {
            1.0
        }
    };
    // yes-equiv = reward (1 / 0.5 / 0), so numerator = sum(reward*weight).
    let want: Vec<usize> = topics.to_vec();
    if !want.is_empty() {
        let (mut num, mut n) = (0.0, 0.0);
        for o in hist
            .iter()
            .filter(|o| o.purpose == purpose && !o.topics.is_empty())
        {
            let len = o.topics.len() as f64;
            for t in &want {
                if o.topics.contains(t) {
                    let share = w(o) / len;
                    num += o.reward * share;
                    n += share;
                }
            }
        }
        if n >= 3.0 - 1e-9 {
            return (num, n, n);
        }
    }
    let on: Vec<&Obs> = hist.iter().filter(|o| o.purpose == purpose).collect();
    let pn: f64 = on.iter().map(|o| w(o)).sum();
    if pn >= 3.0 {
        let num: f64 = on.iter().map(|o| o.reward * w(o)).sum();
        return (num, pn, pn);
    }
    let all: f64 = hist.iter().map(&w).sum();
    if all >= 3.0 {
        let num: f64 = hist.iter().map(|o| o.reward * w(o)).sum();
        return (num, all, all);
    }
    (0.0, 0.0, 0.0)
}

/// The weight a policy gives a reviewer for the current question.
#[allow(clippy::too_many_arguments)]
fn weight(
    policy: &Policy,
    world: &World,
    priors: &Priors,
    hist: &[Obs],
    r: usize,
    purpose: usize,
    topics: &[usize],
    now: u64,
    rng: &mut Rng,
) -> f64 {
    match &policy.kind {
        Kind::Uniform => ROUTING_NEUTRAL,
        Kind::Smoothed {
            params,
            priors: use_p,
        } => {
            let half = params.half_life_days;
            let (num, n, _) = basis_counts(hist, purpose, topics, now, half);
            // num already folds partly at 0.5, so "yes-equivalent" = num, and partly = 0.
            let (pi, m) = prior_of(params, priors, world, r, purpose, topics, *use_p);
            smoothed_score(num, 0.0, n, pi, m)
        }
        Kind::Thompson {
            params,
            priors: use_p,
        } => {
            let half = params.half_life_days;
            let (num, n, _) = basis_counts(hist, purpose, topics, now, half);
            let (pi, m) = prior_of(params, priors, world, r, purpose, topics, *use_p);
            // Beta posterior regularised toward the prior mean with strength m.
            let alpha = num + m * pi;
            let beta = (n - num).max(0.0) + m * (1.0 - pi);
            let sample = sample_beta(alpha.max(1e-3), beta.max(1e-3), rng);
            0.25 + 1.75 * sample
        }
    }
}

/// The prior mean and strength for a reviewer, either from the priors population (v1 with
/// priors) or neutral (0.5) with the default strength.
fn prior_of(
    params: &RouterParams,
    priors: &Priors,
    world: &World,
    r: usize,
    purpose: usize,
    topics: &[usize],
    use_p: bool,
) -> (f64, f64) {
    if !use_p {
        return (0.5, params.prior_strength(0.0));
    }
    let topic_slugs: Vec<String> = topics.iter().map(|t| format!("t{t}")).collect();
    let hit = priors.lookup(
        &format!("lab{}", world.lab[r]),
        &format!("rev{r}"),
        "codex",
        &format!("p{purpose}"),
        &topic_slugs,
    );
    (hit.mean, params.prior_strength(hit.support))
}

/// A Beta(alpha, beta) sample via two Gamma samples (Marsaglia-Tsang), using the SHA-256 PRNG.
fn sample_beta(alpha: f64, beta: f64, rng: &mut Rng) -> f64 {
    let x = sample_gamma(alpha, rng);
    let y = sample_gamma(beta, rng);
    if x + y <= 0.0 {
        0.5
    } else {
        x / (x + y)
    }
}

fn sample_gamma(k: f64, rng: &mut Rng) -> f64 {
    if k < 1.0 {
        let g = sample_gamma(k + 1.0, rng);
        return g * rng.u().max(1e-12).powf(1.0 / k);
    }
    let d = k - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let x = rng.normal();
        let v = (1.0 + c * x).powi(3);
        if v <= 0.0 {
            continue;
        }
        let u = rng.u().max(1e-12);
        if u.ln() < 0.5 * x * x + d - d * v + d * v.ln() {
            return d * v;
        }
    }
}

// --------------------------------------------------------------------------- one run

#[derive(Clone, Default)]
struct Acc {
    reward_sum: f64,
    seats: f64,
    regret: f64,
    labs_sum: f64,
    panels: f64,
    /// Which reviewers were seated in the current 50-round block.
    seated_block: std::collections::HashSet<usize>,
    coverage_sum: f64,
    coverage_blocks: f64,
}

/// The metrics of one policy at the three horizons.
#[derive(Clone, Default)]
struct Metrics {
    mean_useful: [f64; 3],
    regret: [f64; 3],
    coverage: [f64; 3],
    labs: [f64; 3],
}

const HORIZONS: [u64; 3] = [30, 100, 300];

/// Run one seed of one policy through `rounds` rounds and return its metrics at the horizons.
fn run_one(policy: &Policy, world: &World, priors: &Priors, seed: u64, rounds: u64) -> Metrics {
    let mut scen = Rng::new(&format!("scen:{seed}"));
    let mut act = Rng::new(&format!("act:{seed}:{}", policy.name));
    let mut hist: Vec<Vec<Obs>> = vec![Vec::new(); N_REV];
    let mut pending: Vec<(u64, usize, usize, Vec<usize>, f64)> = Vec::new();
    let mut acc = Acc::default();
    let mut metrics = Metrics::default();
    let mut hi = 0usize;

    for t in 0..rounds {
        // Deliver due observations before scoring.
        let (due, rest): (Vec<_>, Vec<_>) = pending.into_iter().partition(|(d, ..)| *d <= t);
        pending = rest;
        for (_, r, p, tp, rw) in due {
            hist[r].push(Obs {
                purpose: p,
                topics: tp,
                reward: rw,
                round: t,
            });
        }

        // Scenario: purpose, 0-2 topics, availability.
        let purpose = scen.range(N_PUR);
        let n_topics = scen.range(3); // 0, 1 or 2
        let mut topics: Vec<usize> = Vec::new();
        for _ in 0..n_topics {
            let tt = scen.range(N_TOP);
            if !topics.contains(&tt) {
                topics.push(tt);
            }
        }
        let avail: Vec<usize> = (0..N_REV).filter(|_| scen.u() < AVAIL).collect();
        if avail.is_empty() {
            continue;
        }
        let mut k = default_size(purpose);
        if k == 0 || k > avail.len() {
            k = avail.len();
        }

        // Policy weights → the REAL seeded draw.
        let cands: Vec<Candidate> = avail
            .iter()
            .map(|&r| Candidate {
                position: r as i64,
                lab: format!("lab{}", world.lab[r]),
                weight: weight(
                    policy, world, priors, &hist[r], r, purpose, &topics, t, &mut act,
                ),
                pinned: false,
            })
            .collect();
        // A per-round seed for the draw (deterministic, policy-specific).
        let mut dseed = [0u8; 32];
        dseed.copy_from_slice(&Sha256::digest(
            format!("draw:{seed}:{}:{t}", policy.name).as_bytes(),
        ));
        let seats = invoke_panel_draw(&cands, k, &dseed, ROUTING_NEUTRAL, params_explore(policy));

        // Oracle top-k by expected reward among the available.
        let mut ranked = avail.clone();
        ranked.sort_by(|&a, &b| {
            world
                .expected(b, purpose, &topics)
                .partial_cmp(&world.expected(a, purpose, &topics))
                .unwrap()
        });
        let oracle_reward: f64 = ranked
            .iter()
            .take(k)
            .map(|&r| world.expected(r, purpose, &topics))
            .sum();
        let policy_reward: f64 = seats
            .iter()
            .map(|s| world.expected(s.position as usize, purpose, &topics))
            .sum();

        acc.regret += oracle_reward - policy_reward;
        let mut labs = std::collections::HashSet::new();
        for s in &seats {
            let r = s.position as usize;
            labs.insert(world.lab[r]);
            acc.seated_block.insert(r);
            // Sample the actual mark and schedule its (maybe) observation.
            let mu = world.latent(&world.mu, r, purpose, &topics);
            let reward = sample_mark(mu, &mut act);
            acc.reward_sum += reward;
            acc.seats += 1.0;
            if act.u() < 0.6 {
                let delay = (act.u() * (MAX_DELAY as f64 + 1.0)).floor() as u64;
                pending.push((t + delay, r, purpose, topics.clone(), reward));
            }
        }
        acc.labs_sum += labs.len() as f64;
        acc.panels += 1.0;

        // Coverage: at the end of each 50-round block, record the seated share.
        if (t + 1) % 50 == 0 {
            acc.coverage_sum += acc.seated_block.len() as f64 / N_REV as f64;
            acc.coverage_blocks += 1.0;
            acc.seated_block.clear();
        }

        // Snapshot at each horizon.
        if hi < HORIZONS.len() && t + 1 == HORIZONS[hi] {
            metrics.mean_useful[hi] = if acc.seats > 0.0 {
                acc.reward_sum / acc.seats
            } else {
                0.0
            };
            metrics.regret[hi] = acc.regret;
            metrics.labs[hi] = if acc.panels > 0.0 {
                acc.labs_sum / acc.panels
            } else {
                0.0
            };
            metrics.coverage[hi] = if acc.coverage_blocks > 0.0 {
                acc.coverage_sum / acc.coverage_blocks
            } else {
                acc.seated_block.len() as f64 / N_REV as f64
            };
            hi += 1;
        }
    }
    metrics
}

fn params_explore(policy: &Policy) -> f64 {
    match &policy.kind {
        Kind::Uniform => ROUTING_EXPLORE,
        Kind::Smoothed { params, .. } | Kind::Thompson { params, .. } => params.explore,
    }
}

/// Sample a mark's reward from a latent mean.
fn sample_mark(mu: f64, rng: &mut Rng) -> f64 {
    let p_no = logistic(C1 - mu);
    let p_partly = logistic(C2 - mu) - p_no;
    let u = rng.u();
    if u < p_no {
        0.0
    } else if u < p_no + p_partly {
        0.5
    } else {
        1.0
    }
}

// --------------------------------------------------------------------------- driver

/// The options of `c3 router simulate`.
#[derive(Debug, Clone)]
pub struct SimOptions {
    pub seed: u64,
    pub runs: u64,
    pub rounds: u64,
    pub json: bool,
    pub out: Option<String>,
}

impl Default for SimOptions {
    fn default() -> Self {
        SimOptions {
            seed: 1,
            runs: 200,
            rounds: 300,
            json: false,
            out: None,
        }
    }
}

/// The policy set (§4): uniform, r15, v1 no priors, v1+priors over the kappa/m_max sweep,
/// v1 half-life sweep, thompson.
fn policies() -> Vec<Policy> {
    let mut v = vec![
        Policy {
            name: "uniform".into(),
            kind: Kind::Uniform,
        },
        Policy {
            name: "r15".into(),
            kind: Kind::Smoothed {
                params: RouterParams::default(),
                priors: false,
            },
        },
        Policy {
            name: "v1-no-priors".into(),
            kind: Kind::Smoothed {
                params: RouterParams::default(),
                priors: false,
            },
        },
    ];
    for kappa in [0.0, 0.05, 0.1, 0.2] {
        for m_max in [4.0, 8.0, 16.0] {
            v.push(Policy {
                name: format!("v1-k{kappa}-mmax{m_max}"),
                kind: Kind::Smoothed {
                    params: RouterParams {
                        kappa,
                        m_max,
                        ..RouterParams::default()
                    },
                    priors: true,
                },
            });
        }
    }
    for half in [0.0, 45.0, 90.0] {
        v.push(Policy {
            name: format!("v1-halflife{half}"),
            kind: Kind::Smoothed {
                params: RouterParams {
                    kappa: 0.1,
                    half_life_days: half,
                    ..RouterParams::default()
                },
                priors: true,
            },
        });
    }
    v.push(Policy {
        name: "thompson".into(),
        kind: Kind::Thompson {
            params: RouterParams {
                kappa: 0.1,
                ..RouterParams::default()
            },
            priors: true,
        },
    });
    v
}

/// The averaged metrics of one policy over the runs (both prior worlds).
struct Row {
    name: String,
    good: Metrics,
    mislead: Metrics,
}

fn average(policy: &Policy, opts: &SimOptions, mislead: bool) -> Metrics {
    let mut sum = Metrics::default();
    for run in 0..opts.runs {
        let seed = opts.seed.wrapping_add(run);
        let world = World::build(seed, mislead);
        let priors = world.priors();
        let m = run_one(policy, &world, &priors, seed, opts.rounds);
        for h in 0..3 {
            sum.mean_useful[h] += m.mean_useful[h];
            sum.regret[h] += m.regret[h];
            sum.coverage[h] += m.coverage[h];
            sum.labs[h] += m.labs[h];
        }
    }
    let n = opts.runs.max(1) as f64;
    for h in 0..3 {
        sum.mean_useful[h] /= n;
        sum.regret[h] /= n;
        sum.coverage[h] /= n;
        sum.labs[h] /= n;
    }
    sum
}

/// Run the full simulation and render the report.
pub fn run(opts: &SimOptions) -> i32 {
    let rows: Vec<Row> = policies()
        .iter()
        .map(|p| Row {
            name: p.name.clone(),
            good: average(p, opts, false),
            mislead: average(p, opts, true),
        })
        .collect();

    let text = if opts.json {
        render_json(opts, &rows)
    } else {
        render_markdown(opts, &rows)
    };
    match &opts.out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, text.as_bytes()) {
                eprintln!("c3 router simulate: cannot write {path}: {e}");
                return 1;
            }
            println!("wrote {path}");
        }
        None => println!("{text}"),
    }
    0
}

fn render_json(opts: &SimOptions, rows: &[Row]) -> String {
    let mut arr = Vec::new();
    for r in rows {
        arr.push(serde_json::json!({
            "policy": r.name,
            "good": metrics_json(&r.good),
            "misleading": metrics_json(&r.mislead),
        }));
    }
    serde_json::to_string_pretty(&serde_json::json!({
        "seed": opts.seed, "runs": opts.runs, "rounds": opts.rounds,
        "horizons": HORIZONS, "policies": arr,
    }))
    .unwrap_or_default()
}

fn metrics_json(m: &Metrics) -> serde_json::Value {
    serde_json::json!({
        "mean_useful": m.mean_useful, "regret": m.regret,
        "coverage": m.coverage, "labs": m.labs,
    })
}

fn render_markdown(opts: &SimOptions, rows: &[Row]) -> String {
    let mut s = String::new();
    s.push_str("# RC2 routing simulation\n\n");
    s.push_str(&format!(
        "seed {} · {} runs · {} rounds · horizons 30/100/300\n\n",
        opts.seed, opts.runs, opts.rounds
    ));
    for (label, pick) in [
        ("Well-specified priors", false),
        ("Misleading priors (2 reviewers anti-correlated)", true),
    ] {
        s.push_str(&format!("## {label}\n\n"));
        for (hi, h) in HORIZONS.iter().enumerate() {
            s.push_str(&format!(
                "### Horizon {h}\n\n| policy | mean useful | regret | coverage | labs/panel |\n|---|---|---|---|---|\n"
            ));
            for r in rows {
                let m = if pick { &r.mislead } else { &r.good };
                s.push_str(&format!(
                    "| {} | {:.3} | {:.1} | {:.2} | {:.2} |\n",
                    r.name, m.mean_useful[hi], m.regret[hi], m.coverage[hi], m.labs[hi]
                ));
            }
            s.push('\n');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_is_deterministic_and_bounded() {
        let opts = SimOptions {
            seed: 7,
            runs: 3,
            rounds: 100,
            json: false,
            out: None,
        };
        let p = Policy {
            name: "r15".into(),
            kind: Kind::Smoothed {
                params: RouterParams::default(),
                priors: false,
            },
        };
        let a = average(&p, &opts, false);
        let b = average(&p, &opts, false);
        assert_eq!(
            a.mean_useful[0].to_bits(),
            b.mean_useful[0].to_bits(),
            "deterministic"
        );
        for h in 0..3 {
            assert!(a.mean_useful[h] >= 0.0 && a.mean_useful[h] <= 1.0);
            assert!(
                a.regret[h] >= -1e-6,
                "regret is non-negative: {}",
                a.regret[h]
            );
        }
    }
}
