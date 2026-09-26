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
