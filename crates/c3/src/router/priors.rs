//! `priors.json`: the maintainer's published, signed global priors, and their fail-closed
//! validation and lineage lookup (M9 spec §3).
//!
//! A prior is a hint, never authority: a file that fails any validation rule is discarded
//! whole (no partial use), the client never trusts the server's k-anonymity, and the mean
//! only shifts the smoothed score's centre — a good local reviewer always outweighs a bad
//! prior once it has three marks.

use serde::{Deserialize, Serialize};

/// The hard validation bounds (§3, fail closed).
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_CELLS: usize = 10_000;
pub const MAX_STRING_BYTES: usize = 128;

/// One published cell: a lineage `(provider, model, engine)` optionally narrowed to a
/// `purpose` and a `topic`. Empty `purpose` = the lineage cell; empty `topic` = the purpose
/// cell. Unknown keys are ignored (no `deny_unknown_fields`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorCell {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub topic: String,
    /// `(yes + 0.5*partly)/n` over installations, in `[0, 1]`.
    pub mean: f64,
    /// The support behind the mean.
    pub n: f64,
}

/// The whole `priors.json`. `priors_version` must be `1`; unknown top-level keys ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Priors {
    pub priors_version: i64,
    #[serde(default)]
    pub generated: String,
    #[serde(default)]
    pub window_days: i64,
    #[serde(default)]
    pub cells: Vec<PriorCell>,
}

/// Which cell in the hierarchy supplied a prior mean, recorded in the draw's `ext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorBasis {
    /// `(lineage, purpose, topic)` — the most specific.
    LineagePurposeTopic,
    /// `(lineage, purpose)`.
    LineagePurpose,
    /// `(lineage)`.
    Lineage,
    /// No cell matched; the mean is the neutral 0.5.
    None,
}

impl PriorBasis {
    /// The `prior_basis` string written into the ledger's `ext.c3.router.prior_use[]`.
    pub fn as_str(self) -> &'static str {
        match self {
            PriorBasis::LineagePurposeTopic => "lineage+purpose+topic",
            PriorBasis::LineagePurpose => "lineage+purpose",
            PriorBasis::Lineage => "lineage",
            PriorBasis::None => "none",
        }
    }
}

/// The prior mean, its support, and which cell supplied it.
#[derive(Debug, Clone, Copy)]
pub struct PriorHit {
    pub mean: f64,
    pub support: f64,
    pub basis: PriorBasis,
}

impl PriorHit {
    /// The neutral prior: mean 0.5, no support, no cell.
    pub fn neutral() -> Self {
        PriorHit {
            mean: 0.5,
            support: 0.0,
            basis: PriorBasis::None,
        }
    }
}

/// Why a `priors.json` was rejected. A rejected file is never used, even in part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriorsError {
    TooLarge(usize),
    /// The file did not parse. No downloaded bytes are carried in the error (F2).
    Parse,
    BadVersion(i64),
    TooManyCells(usize),
    StringTooLong,
    BadControlChar,
    BadMean,
    BadSupport,
}

impl std::fmt::Display for PriorsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PriorsError::TooLarge(n) => write!(f, "priors.json is {n} bytes (over 1 MiB)"),
            PriorsError::Parse => write!(f, "priors.json did not parse"),
            PriorsError::BadVersion(v) => write!(f, "priors_version {v} is not 1"),
            PriorsError::TooManyCells(n) => write!(f, "{n} cells (over 10000)"),
            PriorsError::StringTooLong => write!(f, "a cell string is over 128 bytes"),
            PriorsError::BadControlChar => write!(f, "a cell string has a control character"),
            PriorsError::BadMean => write!(f, "a cell mean is not finite in [0,1]"),
            PriorsError::BadSupport => write!(f, "a cell support is not finite and >= 0"),
        }
    }
}

impl std::error::Error for PriorsError {}

impl Priors {
    /// Parse and validate `priors.json` bytes, fail closed (§3). Every rule that fails
    /// discards the whole file.
    pub fn validate(bytes: &[u8]) -> Result<Priors, PriorsError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(PriorsError::TooLarge(bytes.len()));
        }
        let priors: Priors = serde_json::from_slice(bytes).map_err(|_| PriorsError::Parse)?;
        if priors.priors_version != 1 {
            return Err(PriorsError::BadVersion(priors.priors_version));
        }
        if priors.cells.len() > MAX_CELLS {
            return Err(PriorsError::TooManyCells(priors.cells.len()));
        }
        for c in &priors.cells {
            for s in [&c.provider, &c.model, &c.engine, &c.purpose, &c.topic] {
                if s.len() > MAX_STRING_BYTES {
                    return Err(PriorsError::StringTooLong);
                }
                if s.chars().any(|ch| ch.is_control()) {
                    return Err(PriorsError::BadControlChar);
                }
            }
            if !c.mean.is_finite() || c.mean < 0.0 || c.mean > 1.0 {
                return Err(PriorsError::BadMean);
            }
            if !c.n.is_finite() || c.n < 0.0 {
                return Err(PriorsError::BadSupport);
            }
        }
        Ok(priors)
    }

    /// The most specific published prior for a lineage and question (§2.2): `(lineage,
    /// purpose, topic)` (several topics → support-weighted mean) → `(lineage, purpose)` →
    /// `(lineage)` → neutral. Provider and model match case-sensitively, the engine case
    /// insensitively (mirroring `panel::routing::routing_score`).
    pub fn lookup(
        &self,
        provider: &str,
        model: &str,
        engine: &str,
        purpose: &str,
        topics: &[String],
    ) -> PriorHit {
        let engine = if engine.is_empty() { "codex" } else { engine };
        let lineage_match = |c: &PriorCell| -> bool {
            c.provider == provider
                && c.model == model
                && (if c.engine.is_empty() {
                    "codex"
                } else {
                    &c.engine
                })
                .eq_ignore_ascii_case(engine)
        };

        // (lineage, purpose, topic): support-weighted mean over the requested topics.
        let want: Vec<&String> = topics.iter().filter(|t| !t.is_empty()).collect();
        if !purpose.is_empty() && !want.is_empty() {
            let mut num = 0.0;
            let mut den = 0.0;
            for c in &self.cells {
                if lineage_match(c)
                    && c.purpose == purpose
                    && !c.topic.is_empty()
                    && want.iter().any(|t| **t == c.topic)
                {
                    num += c.mean * c.n;
                    den += c.n;
                }
            }
            if den > 0.0 {
                return PriorHit {
                    mean: num / den,
                    support: den,
                    basis: PriorBasis::LineagePurposeTopic,
                };
            }
        }

        // (lineage, purpose): the purpose cell (empty topic).
        if !purpose.is_empty() {
            if let Some(c) = self
                .cells
                .iter()
                .find(|c| lineage_match(c) && c.purpose == purpose && c.topic.is_empty())
            {
                return PriorHit {
                    mean: c.mean,
                    support: c.n,
                    basis: PriorBasis::LineagePurpose,
                };
            }
        }

        // (lineage): the lineage cell (empty purpose and topic).
        if let Some(c) = self
            .cells
            .iter()
            .find(|c| lineage_match(c) && c.purpose.is_empty() && c.topic.is_empty())
        {
            return PriorHit {
                mean: c.mean,
                support: c.n,
                basis: PriorBasis::Lineage,
            };
        }

        PriorHit::neutral()
    }
}
