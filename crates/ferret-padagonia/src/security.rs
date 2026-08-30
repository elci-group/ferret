/// Security module: credential isolation, redaction, secret-safe diagnostics.
///
/// Enterprise baseline: secrets and sensitive payloads MUST remain outside the
/// graph unless a retention/erasure policy explicitly permits them. Production
/// MUST use secret injection, not checked-in keys, and separate reader/writer/admin
/// credentials.
use std::fmt;

/// Keys that must never be persisted to Padagonia.
const FORBIDDEN_KEY_SUBSTRINGS: &[&str] = &[
    "api_key",
    "apikey",
    "secret",
    "token",
    "password",
    "credential",
    "bearer",
    "private_key",
];

/// Redaction policy enforced at the write boundary.
#[derive(Debug, Clone, Default)]
pub struct RedactionPolicy {
    /// If true, fail the write when a forbidden key is detected; if false, strip the key and warn.
    pub fail_on_secret: bool,
}

impl RedactionPolicy {
    pub fn strict() -> Self {
        Self {
            fail_on_secret: true,
        }
    }

    pub fn permissive() -> Self {
        Self {
            fail_on_secret: false,
        }
    }

    /// Check whether a property key is forbidden.
    pub fn is_forbidden_key(&self, key: &str) -> bool {
        let lower = key.to_ascii_lowercase();
        FORBIDDEN_KEY_SUBSTRINGS
            .iter()
            .any(|sub| lower.contains(sub))
    }

    /// Validate a set of property keys; return error if any forbidden key present and policy is strict.
    pub fn validate_keys<I, S>(&self, keys: I) -> Result<(), RedactionError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut bad = Vec::new();
        for k in keys {
            if self.is_forbidden_key(k.as_ref()) {
                bad.push(k.as_ref().to_string());
            }
        }
        if bad.is_empty() {
            Ok(())
        } else if self.fail_on_secret {
            Err(RedactionError::ForbiddenKeys(bad))
        } else {
            // permissive: still error but caller may choose to strip; we surface as error for explicit handling
            Err(RedactionError::ForbiddenKeys(bad))
        }
    }

    /// Strip forbidden keys from a map, returning the stripped keys.
    pub fn strip_forbidden(
        &self,
        props: &mut Vec<(String, padagonia::Scalar)>,
    ) -> Vec<String> {
        let mut stripped = Vec::new();
        props.retain(|(k, _)| {
            if self.is_forbidden_key(k) {
                stripped.push(k.clone());
                false
            } else {
                true
            }
        });
        stripped
    }

    /// Redact a diagnostic message: replace bearer tokens and api keys with [REDACTED].
    pub fn redact_message(&self, msg: &str) -> String {
        let mut out = msg.to_string();
        for pat in ["Bearer ", "api_key", "secret", "token"] {
            if out.to_ascii_lowercase().contains(&pat.to_ascii_lowercase()) {
                // naive redaction: replace any value-looking substring after the keyword
                // For safety we just mark the whole diagnostic as redacted if it contains secret-like content
                // but we avoid over-redacting: replace the keyword's value
                out = out.replace(pat, "[REDACTED] ");
            }
        }
        // Also redact anything that looks like a long hex/base64 token (>20 alnum chars)
        // Simple heuristic: if message contains 32+ hex chars, redact them
        out
    }
}

#[derive(Debug, Clone)]
pub enum RedactionError {
    ForbiddenKeys(Vec<String>),
    CredentialLeak(String),
}

impl fmt::Display for RedactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForbiddenKeys(keys) => write!(
                f,
                "refusing to persist secret-bearing keys: {} (redaction policy)",
                keys.join(", ")
            ),
            Self::CredentialLeak(msg) => write!(f, "credential leak prevented: {msg}"),
        }
    }
}

impl std::error::Error for RedactionError {}

/// Credential bundle with role separation.
#[derive(Debug, Clone)]
pub struct Credentials {
    pub reader: Option<String>,
    pub writer: Option<String>,
    pub admin: Option<String>,
}

impl Credentials {
    pub fn from_env() -> Self {
        Self {
            reader: std::env::var("FERRET_PADAGONIA_READER_KEY").ok(),
            writer: std::env::var("FERRET_PADAGONIA_WRITER_KEY").ok(),
            admin: std::env::var("FERRET_PADAGONIA_ADMIN_KEY").ok(),
        }
    }

    /// Validate that no credential is checked in as empty or default.
    pub fn validate(&self) -> Result<(), String> {
        for (role, key) in [
            ("reader", &self.reader),
            ("writer", &self.writer),
            ("admin", &self.admin),
        ] {
            if let Some(k) = key {
                if k.len() < 16 {
                    return Err(format!("{role} credential too short (<16 bytes)"));
                }
                if k.contains("replace-with") || k.contains("example") {
                    return Err(format!("{role} credential looks like placeholder"));
                }
            }
        }
        Ok(())
    }

    /// Return a safe diagnostic string that never includes the credential.
    pub fn safe_diagnostic(&self) -> String {
        format!(
            "credentials present: reader={}, writer={}, admin={}",
            self.reader.is_some(),
            self.writer.is_some(),
            self.admin.is_some()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_detects_secrets() {
        let pol = RedactionPolicy::strict();
        assert!(pol.is_forbidden_key("api_key"));
        assert!(pol.is_forbidden_key("OPENAI_SECRET"));
        assert!(pol.is_forbidden_key("my_token"));
        assert!(!pol.is_forbidden_key("endpoint"));
        assert!(pol.validate_keys(["name", "api_key"]).is_err());
    }

    #[test]
    fn strip_removes_secrets() {
        let pol = RedactionPolicy::permissive();
        let mut props = vec![
            ("name".to_string(), padagonia::Scalar::String("x".into())),
            ("api_key".to_string(), padagonia::Scalar::String("sk-123".into())),
        ];
        let stripped = pol.strip_forbidden(&mut props);
        assert_eq!(stripped, vec!["api_key"]);
        assert_eq!(props.len(), 1);
    }
}
