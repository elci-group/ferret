use sha2::{Digest, Sha256};

/// Deterministic domain mapping from Ferret entities to Padagonia graph vocabulary.
///
/// Every label, relation, and property key is interned via Padagonia's ontology
/// `StringTable`. This module centralises the canonical set so that renames
/// are schema-versioned and reproducible across writers/readers.
pub struct DomainMapper;

impl DomainMapper {
    // -- labels (node kinds) --
    pub const LABEL_PROVIDER: &'static str = "Provider";
    pub const LABEL_MODEL: &'static str = "Model";
    pub const LABEL_CAPABILITY: &'static str = "Capability";
    pub const LABEL_BENCHMARK_RUN: &'static str = "BenchmarkRun";
    pub const LABEL_WORKLOAD: &'static str = "Workload";
    pub const LABEL_PRICING_OBSERVATION: &'static str = "PricingObservation";
    pub const LABEL_ROUTING_DECISION: &'static str = "RoutingDecision";
    pub const LABEL_HEALTH_OBSERVATION: &'static str = "HealthObservation";

    // -- relations (edge kinds) --
    pub const REL_PROVIDES_MODEL: &'static str = "provides_model";
    pub const REL_HAS_CAPABILITY: &'static str = "has_capability";
    pub const REL_BENCHMARKED_ON: &'static str = "benchmarked_on";
    pub const REL_OBSERVED_PRICING: &'static str = "observed_pricing";
    pub const REL_DECISION_USED: &'static str = "decision_used";
    pub const REL_DECISION_EXPLAINS: &'static str = "decision_explains";
    pub const REL_HEALTH_OF: &'static str = "health_of";
    pub const REL_SUPERSEDES: &'static str = "supersedes";
    pub const REL_DERIVED_FROM: &'static str = "derived_from";

    // -- property keys --
    pub const KEY_NAME: &'static str = "name";
    pub const KEY_ENDPOINT: &'static str = "endpoint";
    pub const KEY_VERSION: &'static str = "version";
    pub const KEY_MODEL_ID: &'static str = "model_id";
    pub const KEY_CAPABILITY: &'static str = "capability";
    pub const KEY_STATUS: &'static str = "status";
    pub const KEY_LATENCY_MS: &'static str = "latency_ms";
    pub const KEY_RECALL: &'static str = "recall";
    pub const KEY_COST: &'static str = "cost";
    pub const KEY_HARDWARE: &'static str = "hardware";
    pub const KEY_WORKLOAD_HASH: &'static str = "workload_hash";
    pub const KEY_CONTENT_HASH: &'static str = "content_hash";
    pub const KEY_CONFIDENCE_LOW: &'static str = "confidence_low";
    pub const KEY_CONFIDENCE_HIGH: &'static str = "confidence_high";
    pub const KEY_OBSERVED_AT: &'static str = "observed_at";
    pub const KEY_ASSERTED_AT: &'static str = "asserted_at";
    pub const KEY_CORRELATION_ID: &'static str = "correlation_id";
    pub const KEY_RETENTION_CLASS: &'static str = "retention_class";
    pub const KEY_POLICY_VERSION: &'static str = "policy_version";
    pub const KEY_SCHEMA_VERSION: &'static str = "schema_version";
    pub const KEY_API_VERSION: &'static str = "api_version";
    pub const KEY_STALE: &'static str = "is_stale";
    pub const KEY_FRESHNESS_CUTOFF: &'static str = "freshness_cutoff_secs";

    /// Canonical external_id for a provider within a namespace.
    pub fn provider_external_id(namespace: &crate::config::FerretNamespace, name: &str) -> String {
        canonical_external_id(namespace, "Provider", name)
    }

    pub fn model_external_id(
        namespace: &crate::config::FerretNamespace,
        provider: &str,
        model_id: &str,
    ) -> String {
        canonical_external_id(
            namespace,
            "Model",
            &format!("{provider}:{model_id}"),
        )
    }

    pub fn benchmark_external_id(
        namespace: &crate::config::FerretNamespace,
        workload_hash: &str,
        hardware: &str,
        model_id: &str,
    ) -> String {
        canonical_external_id(
            namespace,
            "BenchmarkRun",
            &format!("{workload_hash}:{hardware}:{model_id}"),
        )
    }

    pub fn decision_external_id(
        namespace: &crate::config::FerretNamespace,
        correlation_id: &str,
    ) -> String {
        canonical_external_id(namespace, "RoutingDecision", correlation_id)
    }

    /// All required node kinds for schema validation.
    pub fn required_labels() -> Vec<&'static str> {
        vec![
            Self::LABEL_PROVIDER,
            Self::LABEL_MODEL,
            Self::LABEL_BENCHMARK_RUN,
            Self::LABEL_ROUTING_DECISION,
        ]
    }

    pub fn required_relations() -> Vec<&'static str> {
        vec![
            Self::REL_PROVIDES_MODEL,
            Self::REL_BENCHMARKED_ON,
            Self::REL_DECISION_USED,
        ]
    }
}

/// Deterministic, namespace-scoped external ID (stable across processes).
///
/// Uses Padagonia's `stable_external_id` helper internally for consistency.
pub fn canonical_external_id(
    namespace: &crate::config::FerretNamespace,
    kind: &str,
    canonical: &str,
) -> String {
    let ns = match namespace.to_padagonia_namespace() {
        Ok(ns) => ns,
        Err(_) => return content_hash(&format!("{}:{kind}:{canonical}", namespace.as_dotted())),
    };
    padagonia::identity::stable_external_id(&ns, kind, canonical)
}

/// Content hash for reproducible benchmark provenance.
/// Canonical input is workload + hardware + model_version + params.
pub fn content_hash(canonical: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FerretNamespace;

    #[test]
    fn external_ids_are_stable_and_namespaced() {
        let a = FerretNamespace::new("ferret", "prod", "tenant-a").unwrap();
        let b = FerretNamespace::new("ferret", "prod", "tenant-b").unwrap();
        let id1 = DomainMapper::provider_external_id(&a, "openai");
        let id2 = DomainMapper::provider_external_id(&a, "openai");
        let id3 = DomainMapper::provider_external_id(&b, "openai");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn content_hash_deterministic() {
        assert_eq!(content_hash("hello"), content_hash("hello"));
        assert_ne!(content_hash("hello"), content_hash("world"));
    }
}
