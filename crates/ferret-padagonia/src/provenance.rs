use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fmt;

/// Immutable provenance attached to every graph fact.
///
/// Enterprise baseline requires: producer, producer-version, source reference,
/// observed-at, asserted-at, confidence semantics, correlation ID, retention class.
/// Confidence is *ranking signal* — caller-supplied values are never treated as
/// calibrated probability.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FerretProvenance {
    /// Logical producer (e.g. `ferret-provider-registry`, `ferret-benchmark-ingest`)
    pub producer: String,
    /// Producer version (git SHA or semver)
    pub producer_version: String,
    /// Source reference (workload file, PR, run ID, human)
    pub source_ref: String,
    /// When the observation was made (wall time)
    pub observed_at: DateTime<Utc>,
    /// When the fact was asserted into the graph
    pub asserted_at: DateTime<Utc>,
    /// Confidence semantics: always "ranking_signal" in this integration
    pub confidence_semantics: String,
    /// Caller-supplied confidence in [0,1] — treated as ranking signal
    pub confidence: f32,
    /// Cost observation if applicable
    pub cost: Option<f32>,
    /// Correlation ID for tracing (uuid v4)
    pub correlation_id: String,
    /// Retention class
    pub retention_class: String,
    /// Extra evidence references (URIs, hashes)
    pub evidence: Vec<String>,
}

impl FerretProvenance {
    pub fn new(
        producer: impl Into<String>,
        producer_version: impl Into<String>,
        source_ref: impl Into<String>,
        confidence: f32,
    ) -> Result<Self, ProvenanceError> {
        let producer = producer.into();
        let producer_version = producer_version.into();
        let source_ref = source_ref.into();
        if producer.trim().is_empty() || producer_version.trim().is_empty() {
            return Err(ProvenanceError::InvalidField(
                "producer and producer_version must not be empty".into(),
            ));
        }
        if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
            return Err(ProvenanceError::InvalidConfidence(confidence));
        }
        let now = Utc::now();
        Ok(Self {
            producer,
            producer_version,
            source_ref,
            observed_at: now,
            asserted_at: now,
            confidence_semantics: "ranking_signal".to_string(),
            confidence,
            cost: None,
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retention_class: crate::config::RetentionClass::Ephemeral.as_str().to_string(),
            evidence: Vec::new(),
        })
    }

    pub fn with_cost(mut self, cost: f32) -> Result<Self, ProvenanceError> {
        if !cost.is_finite() || cost < 0.0 {
            return Err(ProvenanceError::InvalidCost(cost));
        }
        self.cost = Some(cost);
        Ok(self)
    }

    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = id.into();
        self
    }

    pub fn with_evidence(mut self, evidence: Vec<String>) -> Self {
        self.evidence = evidence;
        self
    }

    /// Convert to Padagonia's native `Provenance` for storage.
    pub fn to_padagonia(&self) -> padagonia::Provenance {
        let timestamp = self.observed_at.timestamp() as u64;
        let mut ev = vec![
            format!("producer:{}", self.producer),
            format!("producer_version:{}", self.producer_version),
            format!("source_ref:{}", self.source_ref),
            format!("correlation_id:{}", self.correlation_id),
            format!("confidence_semantics:{}", self.confidence_semantics),
            format!("retention_class:{}", self.retention_class),
        ];
        ev.extend(self.evidence.clone());
        padagonia::Provenance {
            agent: self.producer.clone(),
            model: self.producer_version.clone(),
            confidence: self.confidence,
            cost: self.cost.unwrap_or(0.0),
            timestamp,
            evidence: ev,
        }
    }

    /// Validate that this provenance will be accepted by Padagonia's HTTP validation
    /// (agent/model non-empty, confidence finite in 0..1, cost non-negative, evidence bounded).
    pub fn validate(&self) -> Result<(), ProvenanceError> {
        if self.producer.trim().is_empty() || self.producer_version.trim().is_empty() {
            return Err(ProvenanceError::InvalidField("producer fields empty".into()));
        }
        if !self.confidence.is_finite() || !(0.0..=1.0).contains(&self.confidence) {
            return Err(ProvenanceError::InvalidConfidence(self.confidence));
        }
        if let Some(c) = self.cost {
            if !c.is_finite() || c < 0.0 {
                return Err(ProvenanceError::InvalidCost(c));
            }
        }
        if self.evidence.len() > 1024 {
            return Err(ProvenanceError::TooManyEvidence);
        }
        Ok(())
    }
}

/// Content hash for reproducible benchmark/workload provenance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContentHash(pub String);

impl ContentHash {
    pub fn from_canonical(canonical: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        Self(format!("{:x}", hasher.finalize()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(format!("{:x}", hasher.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone)]
pub enum ProvenanceError {
    InvalidField(String),
    InvalidConfidence(f32),
    InvalidCost(f32),
    TooManyEvidence,
}

impl fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(s) => write!(f, "invalid provenance field: {s}"),
            Self::InvalidConfidence(v) => write!(
                f,
                "confidence must be finite in 0..1, got {v} (ranking_signal, not probability)"
            ),
            Self::InvalidCost(v) => write!(f, "cost must be finite and >=0, got {v}"),
            Self::TooManyEvidence => write!(f, "too many evidence items (>1024)"),
        }
    }
}

impl std::error::Error for ProvenanceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_confidence_is_ranking_signal() {
        let p = FerretProvenance::new("ferret-test", "0.1.0", "src:test", 0.92).unwrap();
        assert_eq!(p.confidence_semantics, "ranking_signal");
        let pad = p.to_padagonia();
        assert!((pad.confidence - 0.92).abs() < 1e-6);
    }

    #[test]
    fn provenance_rejects_invalid_confidence() {
        assert!(FerretProvenance::new("a", "b", "c", 2.0).is_err());
        assert!(FerretProvenance::new("a", "b", "c", f32::NAN).is_err());
    }

    #[test]
    fn content_hash_stable() {
        let h1 = ContentHash::from_canonical("workload:one;hardware:a100");
        let h2 = ContentHash::from_canonical("workload:one;hardware:a100");
        assert_eq!(h1, h2);
    }
}
