//! Detached ed25519 signatures over `priors.json` (M9 spec §3).
//!
//! `priors.json.sig` = `{ "alg": "ed25519", "key_id": "<id>", "sig": "<base64 of the 64-byte
//! signature over the exact file bytes>" }`. Trusted public keys are pinned in the binary
//! ([`PRIORS_KEYS`], empty until the maintainer publishes one) plus an optional user key from
//! `C3_PRIORS_KEY=<key_id>:<base64>`. A download whose signature does not verify against a
//! trusted key is discarded and the previous verified copy stays. No private key material is
//! ever stored, printed or written by C3 outside a test's memory.
//!
//! The `.sig` file is untrusted input: the pinned keys are looked up FIRST, so a user
//! `C3_PRIORS_KEY` can never shadow a maintainer key id (F1); `key_id` is validated to a
//! strict charset before use and no error message ever echoes bytes from the downloaded files
//! except a validated `key_id` (F2).

use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

/// The maintainer's pinned public keys: `(key_id, base64 of the 32-byte public key)`. Empty
/// until the maintainer publishes one; a user adds trust for their own hub with
/// `C3_PRIORS_KEY`, never by editing this list at runtime.
pub const PRIORS_KEYS: &[(&str, &str)] = &[];

/// The detached signature file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigFile {
    pub alg: String,
    pub key_id: String,
    /// Base64 of the 64-byte ed25519 signature.
    pub sig: String,
}

/// Why a signature was rejected. No variant carries bytes from the downloaded files except a
/// `key_id` already validated to `[A-Za-z0-9._-]{1,64}` ([`valid_key_id`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigError {
    /// The `.sig` did not parse, or `key_id` failed its charset check. No file bytes echoed.
    Parse,
    /// `alg` was not `ed25519`. The offending value is NOT echoed (it is untrusted input).
    BadAlg,
    /// The (validated) `key_id` is not a trusted key.
    UnknownKey(String),
    BadKeyEncoding,
    BadSigEncoding,
    Verify,
}

impl std::fmt::Display for SigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SigError::Parse => write!(f, "priors.json.sig did not parse or has an invalid key_id"),
            SigError::BadAlg => write!(f, "signature alg is not ed25519"),
            SigError::UnknownKey(id) => write!(f, "signature key_id '{id}' is not trusted"),
            SigError::BadKeyEncoding => write!(f, "a trusted public key is not 32 valid bytes"),
            SigError::BadSigEncoding => write!(f, "the signature is not 64 valid bytes"),
            SigError::Verify => write!(f, "the signature does not verify"),
        }
    }
}

impl std::error::Error for SigError {}

/// A `key_id` is trusted for the wire only when it is a short, plain token.
fn valid_key_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The trusted key for `key_id`: the pinned keys FIRST (a user key can never shadow a
/// maintainer key id, F1), then the optional user key from `C3_PRIORS_KEY` — but only when its
/// id is not one of the pinned ids. Returns the base64-encoded public key, or `None`.
fn trusted_key(pinned: &[(&str, &str)], key_id: &str) -> Option<String> {
    if let Some((_, b64)) = pinned.iter().find(|(id, _)| *id == key_id) {
        return Some(b64.to_string());
    }
    if let Ok(v) = std::env::var("C3_PRIORS_KEY") {
        if let Some((id, b64)) = v.split_once(':') {
            if id == key_id && !b64.is_empty() && !pinned.iter().any(|(pid, _)| *pid == id) {
                return Some(b64.to_string());
            }
        }
    }
    None
}

/// Verify a detached signature over `file_bytes` against the maintainer's pinned keys plus the
/// optional user key. Fail closed on every mismatch (F1/F2).
pub fn verify(file_bytes: &[u8], sig_json: &[u8]) -> Result<(), SigError> {
    verify_with(PRIORS_KEYS, file_bytes, sig_json)
}

/// [`verify`] against an explicit pinned-key slice (test injection of a non-empty pinned list).
pub fn verify_with(
    pinned: &[(&str, &str)],
    file_bytes: &[u8],
    sig_json: &[u8],
) -> Result<(), SigError> {
    // A parse failure must not echo any file bytes.
    let sf: SigFile = serde_json::from_slice(sig_json).map_err(|_| SigError::Parse)?;
    if !valid_key_id(&sf.key_id) {
        return Err(SigError::Parse);
    }
    if sf.alg != "ed25519" {
        return Err(SigError::BadAlg);
    }
    let key_b64 =
        trusted_key(pinned, &sf.key_id).ok_or_else(|| SigError::UnknownKey(sf.key_id.clone()))?;
    let key_bytes = base64::engine::general_purpose::STANDARD
        .decode(key_b64.trim())
        .map_err(|_| SigError::BadKeyEncoding)?;
    let key_arr: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| SigError::BadKeyEncoding)?;
    let vk = VerifyingKey::from_bytes(&key_arr).map_err(|_| SigError::BadKeyEncoding)?;
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(sf.sig.trim())
        .map_err(|_| SigError::BadSigEncoding)?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| SigError::BadSigEncoding)?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify_strict(file_bytes, &sig)
        .map_err(|_| SigError::Verify)
}
