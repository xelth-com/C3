//! The single outbound sanitizer (DESIGN §3 invariant 5, D8).
//!
//! Every outbound writer in the pack pipeline — snapshots, reviewer packs, explainer
//! packs — MUST pass file bodies through [`redact`] before they reach a file or a
//! provider. The `http` engine, telemetry and error strings reuse the same rules.
//!
//! The patterns are ported from eckSnapshot's `SecretScanner` (`fileUtils.js`) and the
//! xelth.rs `scanner.rs`, with the OpenRouter / JWT / bearer-header / private-key kinds
//! the C3 design names added. A match is replaced by `[REDACTED:<kind>]`; for the
//! keyword-assignment and bearer kinds only the secret *value* is replaced so the
//! surrounding line stays readable. A second, entropy-gated pass catches arbitrary
//! high-entropy literals assigned to key-shaped variable names.
//!
//! Limits (documented on purpose): this is a lexical scanner, not a vault. It cannot
//! find a secret that is split across lines, base64-of-a-secret, or a low-entropy
//! password that does not sit next to a keyword. Hard-ignoring `.env*` and key files at
//! discovery (see [`super::discover`]) is the first line of defence; this pass is the
//! second.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;

/// What one redaction pass found and produced.
#[derive(Debug, Clone, Default)]
pub struct Redaction {
    /// The text with every secret replaced.
    pub text: String,
    /// Total number of replacements.
    pub count: usize,
    /// Replacements per kind, kind → count.
    pub by_kind: BTreeMap<String, usize>,
}

/// The single sanitizer. Returns the redacted text and the number of replacements
/// made — the count a pack records in its header and sidecar.
pub fn redact(text: &str) -> (String, usize) {
    let r = scan(text);
    (r.text, r.count)
}

/// The full report, kept for the header/sidecar and for the per-kind tests.
pub fn scan(text: &str) -> Redaction {
    let mut out = text.to_string();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();

    for p in patterns() {
        let placeholder = format!("[REDACTED:{}]", p.kind);
        let mut hits = 0usize;
        out =
            p.re.replace_all(&out, |caps: &regex::Captures| {
                // A capture-group pattern redacts only the secret value inside the match;
                // a whole-match pattern replaces everything it matched.
                match p.group {
                    Some(g) => {
                        let full = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                        match caps.get(g) {
                            Some(secret) if !secret.as_str().is_empty() => {
                                hits += 1;
                                full.replacen(secret.as_str(), &placeholder, 1)
                            }
                            _ => full.to_string(),
                        }
                    }
                    None => {
                        hits += 1;
                        placeholder.clone()
                    }
                }
            })
            .into_owned();
        if hits > 0 {
            *by_kind.entry(p.kind.to_string()).or_insert(0) += hits;
        }
    }

    // Entropy-gated pass: long random literals assigned to KEY/TOKEN/SECRET/PASSWORD-shaped
    // names, even without a known service prefix (ported from the eckSnapshot second pass).
    let mut entropy_hits = 0usize;
    out = entropy_re()
        .replace_all(&out, |caps: &regex::Captures| {
            let full = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            match caps.get(2) {
                Some(secret)
                    if shannon_entropy(secret.as_str()) > 4.5
                        && !secret.as_str().contains("REDACTED") =>
                {
                    entropy_hits += 1;
                    full.replacen(secret.as_str(), "[REDACTED:high-entropy]", 1)
                }
                _ => full.to_string(),
            }
        })
        .into_owned();
    if entropy_hits > 0 {
        *by_kind.entry("high-entropy".to_string()).or_insert(0) += entropy_hits;
    }

    let count = by_kind.values().sum();
    Redaction {
        text: out,
        count,
        by_kind,
    }
}

struct Pat {
    kind: &'static str,
    re: Regex,
    /// `Some(n)` = redact only capture group `n`; `None` = redact the whole match.
    group: Option<usize>,
}

fn patterns() -> &'static [Pat] {
    static PATS: OnceLock<Vec<Pat>> = OnceLock::new();
    PATS.get_or_init(|| {
        vec![
            // Private key blocks first (whole match, spans the BEGIN line).
            Pat {
                kind: "private-key",
                re: Regex::new(
                    r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY-----",
                )
                .unwrap(),
                group: None,
            },
            // OpenRouter keys (sk-or-...) before the generic OpenAI sk- rule.
            Pat {
                kind: "openrouter-key",
                re: Regex::new(r"sk-or-[a-zA-Z0-9-]{20,}").unwrap(),
                group: None,
            },
            Pat {
                kind: "openai-key",
                re: Regex::new(r"sk-[a-zA-Z0-9]{32,}").unwrap(),
                group: None,
            },
            Pat {
                kind: "stripe-key",
                re: Regex::new(r"sk_live_[0-9a-zA-Z]{24}").unwrap(),
                group: None,
            },
            Pat {
                kind: "aws-key",
                re: Regex::new(r"(?:AKIA|ASIA)[0-9A-Z]{16}").unwrap(),
                group: None,
            },
            Pat {
                kind: "google-key",
                re: Regex::new(r"AIza[0-9A-Za-z\-_]{35}").unwrap(),
                group: None,
            },
            Pat {
                kind: "github-token",
                re: Regex::new(r"gh[pousr]_[a-zA-Z0-9]{36,}").unwrap(),
                group: None,
            },
            Pat {
                kind: "slack-token",
                re: Regex::new(r"xox[baprs]-[0-9a-zA-Z-]{10,}").unwrap(),
                group: None,
            },
            Pat {
                kind: "npm-token",
                re: Regex::new(r"npm_[a-zA-Z0-9]{36}").unwrap(),
                group: None,
            },
            // JSON Web Tokens: header.payload.signature, each base64url.
            Pat {
                kind: "jwt",
                re: Regex::new(
                    r"eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}",
                )
                .unwrap(),
                group: None,
            },
            // Authorization: Bearer <token> — redact the token only.
            Pat {
                kind: "bearer",
                re: Regex::new(r"(?i)bearer\s+([a-zA-Z0-9._~+/-]{20,}={0,2})").unwrap(),
                group: Some(1),
            },
            // key = "value" / key: 'value' near a sensitive keyword — redact the value only.
            Pat {
                kind: "assignment",
                re: Regex::new(
                    r#"(?i)(?:api[_-]?key|secret|password|passwd|pwd|token|auth|credential)\s*[:=]\s*["']([^"'\r\n]{8,})["']"#,
                )
                .unwrap(),
                group: Some(1),
            },
        ]
    })
}

fn entropy_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)(?:const|let|var|set|export|define|fn|static)\s+([A-Za-z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD)[A-Za-z0-9_]*)\s*[:=]\s*["']([a-zA-Z0-9+/=_-]{20,128})["']"#,
        )
        .unwrap()
    })
}

/// Shannon entropy in bits per symbol (base 2).
fn shannon_entropy(s: &str) -> f64 {
    let mut freq: BTreeMap<char, usize> = BTreeMap::new();
    for c in s.chars() {
        *freq.entry(c).or_insert(0) += 1;
    }
    let len = s.chars().count() as f64;
    if len == 0.0 {
        return 0.0;
    }
    freq.values().fold(0.0, |acc, &n| {
        let p = n as f64 / len;
        acc - p * p.log2()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_redacted(input: &str, kind: &str, leaked: &str) {
        let r = scan(input);
        assert!(
            r.by_kind.contains_key(kind),
            "kind {kind} not found in {:?} for input {input:?}",
            r.by_kind
        );
        assert!(r.count >= 1, "count should be >=1 for {input:?}");
        assert!(
            !r.text.contains(leaked),
            "secret {leaked:?} survived in {:?}",
            r.text
        );
        assert!(r.text.contains(&format!("[REDACTED:{kind}]")));
    }

    #[test]
    fn github_token() {
        assert_redacted(
            "let t = \"ghp_123456789012345678901234567890123456\";",
            "github-token",
            "ghp_1234567890",
        );
    }

    #[test]
    fn aws_access_key() {
        assert_redacted(
            "aws = AKIAIOSFODNN7EXAMPLE",
            "aws-key",
            "AKIAIOSFODNN7EXAMPLE",
        );
    }

    #[test]
    fn openai_key() {
        assert_redacted(
            "OPENAI=sk-abcdefghijklmnopqrstuvwxyz0123456789",
            "openai-key",
            "sk-abcdefghij",
        );
    }

    #[test]
    fn openrouter_key() {
        assert_redacted(
            "key=sk-or-v1-0123456789abcdef0123456789abcdef",
            "openrouter-key",
            "sk-or-v1-0123",
        );
    }

    #[test]
    fn google_key() {
        assert_redacted(
            "g = AIzaSyA1234567890abcdefghijklmnopqrstuvw",
            "google-key",
            "AIzaSyA1234567890",
        );
    }

    #[test]
    fn jwt_token() {
        assert_redacted(
            "auth eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N",
            "jwt",
            "eyJhbGciOiJIUzI1NiI",
        );
    }

    #[test]
    fn bearer_header() {
        assert_redacted(
            "Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345",
            "bearer",
            "abcdefghijklmnopqrstuvwxyz012345",
        );
    }

    #[test]
    fn private_key_block() {
        assert_redacted(
            "-----BEGIN OPENSSH PRIVATE KEY-----\nbody\n",
            "private-key",
            "BEGIN OPENSSH PRIVATE KEY",
        );
    }

    #[test]
    fn keyword_assignment() {
        assert_redacted(
            "password = \"hunter2hunter2hunter2\"",
            "assignment",
            "hunter2hunter2hunter2",
        );
    }

    #[test]
    fn high_entropy_literal() {
        // A KEY-shaped name (not api_key/secret/...) so the assignment rule does not fire first.
        assert_redacted(
            "const DEPLOY_KEY = \"jK8vLp3sN9qW2zX5mC4bY7hF1gR0tD6\";",
            "high-entropy",
            "jK8vLp3sN9qW2zX5mC4bY7hF1gR0tD6",
        );
    }

    #[test]
    fn clean_code_is_untouched() {
        let src = "fn hello() { println!(\"hi\"); }\nlet x = 3 + 4;\n";
        let (out, n) = redact(src);
        assert_eq!(n, 0);
        assert_eq!(out, src);
    }

    #[test]
    fn count_sums_kinds() {
        let src = "AKIAIOSFODNN7EXAMPLE and ghp_123456789012345678901234567890123456";
        let r = scan(src);
        assert_eq!(r.count, 2);
        assert_eq!(r.by_kind.len(), 2);
    }
}
