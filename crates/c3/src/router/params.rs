//! [`RouterParams`]: the v1 policy parameters, every one recorded with the draw (M9 spec
//! §1). Router v1 is a strict generalisation of the plugin's R15 smoothed score; the
//! defaults here (`m_min = 2`, `kappa = 0`, `m_max = 8`, everything else off) reproduce R15
//! bit for bit, so with no priors loaded the draw is byte-identical to the plugin's.

use serde::{Deserialize, Serialize};

/// The v1 router parameters. The defaults are R15: `m = 2`, `pi = 0.5`, no age decay, no
/// outcome contribution, the plugin's `ROUTING_EXPLORE`. The supervisor changes a default
/// only on the evidence of the RC2 report.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RouterParams {
    /// Prior strength with no priors (R15's `m = 2`).
    pub m_min: f64,
    /// Prior strength per unit of global support: `m = clamp(m_min, kappa*n_global, m_max)`.
    pub kappa: f64,
    /// Cap on the prior strength.
    pub m_max: f64,
    /// Age decay of a local mark, in days: weight `0.5^(age/half_life)`. `0` disables decay.
    pub half_life_days: f64,
    /// Contribution of settled finding outcomes (verified +, rejected -) to a mark. `0` off.
    pub outcome_weight: f64,
    /// The plugin's `ROUTING_EXPLORE`, unchanged.
    pub explore: f64,
}

impl Default for RouterParams {
    fn default() -> Self {
        RouterParams {
            m_min: 2.0,
            // RC2 default (docs/port/rc2-simulation.md): a mild prior-strength scaling. With no
            // priors loaded `n_global = 0`, so `m = clamp(m_min, kappa*0, m_max) = m_min = 2` and
            // the score stays R15 bit for bit; kappa only takes effect once a signed prior loads.
            kappa: 0.1,
            m_max: 8.0,
            half_life_days: 0.0,
            outcome_weight: 0.0,
            explore: crate::panel::routing::ROUTING_EXPLORE,
        }
    }
}

impl RouterParams {
    /// True when every parameter is its R15 default, so the draw is byte-identical to the
    /// plugin's and C3 writes no `ext` key ([`super::routing_ext`]).
    pub fn is_default(&self) -> bool {
        *self == RouterParams::default()
    }

    /// The prior strength `m = clamp(m_min, kappa*n_global, m_max)` for a cell whose global
    /// support is `n_global`. With the defaults (`kappa = 0`) this is exactly `m_min = 2`.
    pub fn prior_strength(&self, n_global: f64) -> f64 {
        (self.kappa * n_global).max(self.m_min).min(self.m_max)
    }
}
