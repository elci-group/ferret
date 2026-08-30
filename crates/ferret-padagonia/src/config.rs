use std::fmt;

/// Typed configuration for Padagonia integration.
///
/// Mirrors enterprise baseline: versioned namespace, TLS at edge,
/// credential separation, namespace quotas, timeouts, and retention.
#[derive(Debug, Clone)]
pub struct PadagoniaConfig {
    /// Versioned namespace in form `<product>.<environment>.<tenant>` e.g. `ferret.prod.acme`
    pub namespace: FerretNamespace,
    /// Padagonia HTTP endpoint (or `memory://` for embedded Store).
    pub endpoint: String,
    /// API key injected via secret (never checked in).
    pub api_key: Option<String>,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Maximum retries for idempotent writes.
    pub max_retries: u32,
    /// Maximum nodes per namespace (bounded graph growth).
    pub max_nodes_per_namespace: usize,
    /// Maximum edges per namespace.
    pub max_edges_per_namespace: usize,
    /// Retention class for graph facts.
    pub retention_class: RetentionClass,
    /// Whether TLS is required (production must be true).
    pub require_tls: bool,
}

impl Default for PadagoniaConfig {
    fn default() -> Self {
        Self {
            namespace: FerretNamespace {
                product: "ferret".to_string(),
                environment: "dev".to_string(),
                tenant: "default".to_string(),
            },
            endpoint: "memory://".to_string(),
            api_key: None,
            timeout_secs: 10,
            max_retries: 3,
            max_nodes_per_namespace: 100_000,
            max_edges_per_namespace: 500_000,
            retention_class: RetentionClass::Ephemeral,
            require_tls: false,
        }
    }
}

impl PadagoniaConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.is_empty() {
            return Err("endpoint must not be empty".to_string());
        }
        if self.require_tls && self.endpoint.starts_with("http://") {
            return Err("TLS required but endpoint is http://".to_string());
        }
        if self.timeout_secs == 0 {
            return Err("timeout_secs must be > 0".to_string());
        }
        if self.max_retries > 10 {
            return Err("max_retries too large (bounded retry)".to_string());
        }
        self.namespace.validate()?;
        Ok(())
    }

    /// Returns true if running in embedded (offline-safe) mode.
    pub fn is_embedded(&self) -> bool {
        self.endpoint.starts_with("memory://")
    }
}

/// Versioned namespace `<product>.<environment>.<tenant>`.
///
/// Enforced at every write boundary; stored as Padagonia `NamespaceId`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FerretNamespace {
    pub product: String,
    pub environment: String,
    pub tenant: String,
}

impl FerretNamespace {
    pub fn new(product: &str, environment: &str, tenant: &str) -> Result<Self, String> {
        for part in [product, environment, tenant] {
            if part.is_empty() || part.len() > 64 {
                return Err(format!("namespace part '{part}' empty or too long"));
            }
            if part
                .bytes()
                .any(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' )))
            {
                return Err(format!("namespace part '{part}' contains invalid chars"));
            }
        }
        if product != "ferret" {
            return Err("product must be 'ferret' for this integration".to_string());
        }
        Ok(Self {
            product: product.to_string(),
            environment: environment.to_string(),
            tenant: tenant.to_string(),
        })
    }

    /// Parse from dotted string `ferret.<env>.<tenant>`
    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("namespace must be product.env.tenant, got '{s}'"));
        }
        Self::new(parts[0], parts[1], parts[2])
    }

    pub fn as_dotted(&self) -> String {
        format!("{}.{}.{}", self.product, self.environment, self.tenant)
    }

    pub fn to_padagonia_namespace(
        &self,
    ) -> Result<padagonia::identity::NamespaceId, String> {
        padagonia::identity::NamespaceId::new(self.as_dotted())
            .map_err(|e| e.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        // Re-validate parts
        FerretNamespace::new(&self.product, &self.environment, &self.tenant).map(|_| ())
    }
}

impl fmt::Display for FerretNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.product, self.environment, self.tenant)
    }
}

/// Retention classification for facts (enterprise: explicit policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionClass {
    Ephemeral,
    ThirtyDays,
    NinetyDays,
    OneYear,
    Permanent,
}

impl RetentionClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ephemeral => "ephemeral",
            Self::ThirtyDays => "30d",
            Self::NinetyDays => "90d",
            Self::OneYear => "1y",
            Self::Permanent => "permanent",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_parse_and_validate() {
        let ns = FerretNamespace::parse("ferret.prod.acme").unwrap();
        assert_eq!(ns.as_dotted(), "ferret.prod.acme");
        assert!(FerretNamespace::parse("badformat").is_err());
        assert!(FerretNamespace::parse("other.prod.acme").is_err());
    }

    #[test]
    fn config_validates_tls() {
        let mut cfg = PadagoniaConfig::default();
        cfg.require_tls = true;
        cfg.endpoint = "http://example.com".to_string();
        assert!(cfg.validate().is_err());
        cfg.endpoint = "https://example.com".to_string();
        assert!(cfg.validate().is_ok());
    }
}
