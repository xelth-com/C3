//! Purposes and the weighty-purpose gate (`$script:WeightyPurposes`).

/// The purposes on which a `"weighty"` roster entry joins a panel.
pub const WEIGHTY_PURPOSES: &[&str] = &[
    "framing",
    "decision",
    "core-contract",
    "acceptance",
    "stuck",
];

/// Whether a purpose is weighty (a `"weighty"` reviewer runs on it without `-PanelAll`).
pub fn is_weighty(purpose: &str) -> bool {
    WEIGHTY_PURPOSES.contains(&purpose)
}

/// (0.2.4, reviewer-protection) The standing purposes: the acceptance work the standing partner is
/// kept for. A protected reviewer's reserve hold does not apply on these, and the bare default on
/// these purposes stays the first available roster entry (never a routed pick).
pub const STANDING_PURPOSES: &[&str] = &["acceptance", "core-contract"];

/// Whether a purpose is standing (acceptance / core-contract): the reserve hold is waived and the
/// bare default is the standing partner.
pub fn is_standing(purpose: &str) -> bool {
    STANDING_PURPOSES.contains(&purpose)
}
