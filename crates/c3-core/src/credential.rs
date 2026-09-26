//! The credential-check result (`New-CredentialResult`).

/// `ok` / `missing` / `unknown` plus a one-phrase reason and a display detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialResult {
    pub state: State,
    pub reason: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ok,
    Missing,
    Unknown,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Missing => "missing",
            State::Unknown => "unknown",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            State::Ok => "ok: ",
            State::Missing => "missing: ",
            State::Unknown => "unknown: ",
        }
    }
}

impl CredentialResult {
    pub fn new(state: State, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        let detail = format!("{}{}", state.prefix(), reason);
        CredentialResult {
            state,
            reason,
            detail,
        }
    }
    pub fn ok(reason: impl Into<String>) -> Self {
        Self::new(State::Ok, reason)
    }
    pub fn missing(reason: impl Into<String>) -> Self {
        Self::new(State::Missing, reason)
    }
    pub fn unknown(reason: impl Into<String>) -> Self {
        Self::new(State::Unknown, reason)
    }
    /// The "not checked" form the engine credential uses under `-NoNetwork`: state
    /// unknown, reason `sign-in not checked`, a custom detail.
    pub fn with_detail(state: State, reason: impl Into<String>, detail: impl Into<String>) -> Self {
        CredentialResult {
            state,
            reason: reason.into(),
            detail: detail.into(),
        }
    }
}
