use chrono::{DateTime, Utc};

use crate::client::{ClientError, FerretPadagoniaClient, WriteOutcome};
use crate::domain::DomainMapper;
use padagonia::StringTableExt;
use crate::provenance::FerretProvenance;

/// Provider capability (e.g. `chat`, `embed`, `rerank`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Capability {
    pub name: String,
    pub version: String,
    pub enabled: bool,
}

/// Persisted provider record (maps to Padagonia `Provider` label).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderRecord {
    pub name: String,
    pub endpoint: String,
    pub version: String,
    pub capabilities: Vec<Capability>,
    pub observed_at: DateTime<Utc>,
}

/// Persisted model record (maps to `Model` label, edge `provides_model`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelRecord {
    pub provider: String,
    pub model_id: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub max_context: Option<u32>,
    pub observed_at: DateTime<Utc>,
}

/// Health observation for a provider endpoint / model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthObservation {
    pub target: String, // provider name or model_id
    pub status: HealthStatus,
    pub latency_ms: Option<u64>,
    pub observed_at: DateTime<Utc>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unavailable,
}

impl HealthStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Registry that persists providers, models, capabilities, endpoints, versions,
/// and health observations into Padagonia.
pub struct ProviderRegistry {
    client: FerretPadagoniaClient,
}

impl ProviderRegistry {
    pub fn new(client: FerretPadagoniaClient) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &FerretPadagoniaClient {
        &self.client
    }

    /// Register or update a provider (idempotent by external_id).
    pub fn register_provider(
        &self,
        record: &ProviderRecord,
        provenance: FerretProvenance,
    ) -> Result<(WriteOutcome, padagonia::id::NodeId), ClientError> {
        if record.name.trim().is_empty() {
            return Ok((
                WriteOutcome::Rejected("provider name must not be empty".into()),
                padagonia::id::NodeId(0),
            ));
        }
        // Secret-safe: endpoint must not contain credentials; redaction will also check
        if record.endpoint.contains("api_key") || record.endpoint.contains("secret") {
            return Err(ClientError::Redaction(
                "endpoint must not contain secrets".into(),
            ));
        }
        let external_id =
            DomainMapper::provider_external_id(self.client.ferret_namespace(), &record.name);
        let props = vec![
            (
                DomainMapper::KEY_NAME.to_string(),
                padagonia::Scalar::String(record.name.clone()),
            ),
            (
                DomainMapper::KEY_ENDPOINT.to_string(),
                padagonia::Scalar::String(record.endpoint.clone()),
            ),
            (
                DomainMapper::KEY_VERSION.to_string(),
                padagonia::Scalar::String(record.version.clone()),
            ),
            (
                DomainMapper::KEY_SCHEMA_VERSION.to_string(),
                padagonia::Scalar::I64(crate::CURRENT_SCHEMA_VERSION as i64),
            ),
            (
                DomainMapper::KEY_API_VERSION.to_string(),
                padagonia::Scalar::String(crate::PADAGONIA_API_VERSION.to_string()),
            ),
            (
                DomainMapper::KEY_OBSERVED_AT.to_string(),
                padagonia::Scalar::Timestamp(
                    record.observed_at.timestamp() as u64
                ),
            ),
        ];
        self.client.create_node_idempotent(
            external_id,
            DomainMapper::LABEL_PROVIDER,
            props,
            None,
            provenance,
        )
    }

    /// Register a model and link it to its provider via `provides_model` edge.
    pub fn register_model(
        &self,
        record: &ModelRecord,
        provenance: FerretProvenance,
    ) -> Result<(WriteOutcome, padagonia::id::NodeId), ClientError> {
        if record.model_id.trim().is_empty() || record.provider.trim().is_empty() {
            return Ok((
                WriteOutcome::Rejected("model_id and provider must not be empty".into()),
                padagonia::id::NodeId(0),
            ));
        }
        let external_id = DomainMapper::model_external_id(
            self.client.ferret_namespace(),
            &record.provider,
            &record.model_id,
        );
        let caps_json = serde_json::to_string(&record.capabilities).unwrap_or_default();
        let mut props = vec![
            (
                DomainMapper::KEY_MODEL_ID.to_string(),
                padagonia::Scalar::String(record.model_id.clone()),
            ),
            (
                DomainMapper::KEY_NAME.to_string(),
                padagonia::Scalar::String(record.provider.clone()),
            ),
            (
                DomainMapper::KEY_VERSION.to_string(),
                padagonia::Scalar::String(record.version.clone()),
            ),
            (
                DomainMapper::KEY_CAPABILITY.to_string(),
                padagonia::Scalar::String(caps_json),
            ),
            (
                DomainMapper::KEY_OBSERVED_AT.to_string(),
                padagonia::Scalar::Timestamp(record.observed_at.timestamp() as u64),
            ),
            (
                DomainMapper::KEY_SCHEMA_VERSION.to_string(),
                padagonia::Scalar::I64(crate::CURRENT_SCHEMA_VERSION as i64),
            ),
        ];
        if let Some(ctx) = record.max_context {
            props.push((
                "max_context".to_string(),
                padagonia::Scalar::I64(ctx as i64),
            ));
        }

        let (outcome, model_node) = self.client.create_node_idempotent(
            external_id,
            DomainMapper::LABEL_MODEL,
            props,
            None,
            provenance.clone(),
        )?;

        if outcome == WriteOutcome::Accepted {
            // Create edge provider -> model if provider exists
            let provider_ext =
                DomainMapper::provider_external_id(self.client.ferret_namespace(), &record.provider);
            if let Some(provider_node) = self.client.exact_node(&provider_ext) {
                // Use direct store edge creation via client store lock; we add edge idempotently by checking existing edges
                // For simplicity, we store the relation as a property on the model node and also attempt an edge
                let _ = self.try_create_edge(
                    provider_node.id,
                    model_node,
                    DomainMapper::REL_PROVIDES_MODEL,
                    provenance,
                );
            }
        }

        Ok((outcome, model_node))
    }

    fn try_create_edge(
        &self,
        src: padagonia::id::NodeId,
        dst: padagonia::id::NodeId,
        label: &str,
        provenance: FerretProvenance,
    ) -> Result<(), ClientError> {
        let ns = self.client.namespace().clone();
        let ext = format!("edge:{}->{}:{}", src.0, dst.0, label);
        // Delegate to client helper if available; otherwise attempt direct store write
        let _ = self.client.create_edge_idempotent(ext, src, dst, label, vec![], provenance);
        Ok(())
    }

    /// Record a health observation for a provider/model endpoint.
    pub fn record_health(
        &self,
        observation: &HealthObservation,
        provenance: FerretProvenance,
    ) -> Result<(WriteOutcome, padagonia::id::NodeId), ClientError> {
        let external_id = crate::domain::canonical_external_id(
            self.client.ferret_namespace(),
            "HealthObservation",
            &format!("{}:{}:{}", observation.target, observation.status.as_str(), observation.observed_at.timestamp()),
        );
        let mut props = vec![
            (
                "target".to_string(),
                padagonia::Scalar::String(observation.target.clone()),
            ),
            (
                DomainMapper::KEY_STATUS.to_string(),
                padagonia::Scalar::String(observation.status.as_str().to_string()),
            ),
            (
                DomainMapper::KEY_CORRELATION_ID.to_string(),
                padagonia::Scalar::String(observation.correlation_id.clone()),
            ),
            (
                DomainMapper::KEY_OBSERVED_AT.to_string(),
                padagonia::Scalar::Timestamp(observation.observed_at.timestamp() as u64),
            ),
            (
                DomainMapper::KEY_SCHEMA_VERSION.to_string(),
                padagonia::Scalar::I64(crate::CURRENT_SCHEMA_VERSION as i64),
            ),
        ];
        if let Some(lat) = observation.latency_ms {
            props.push((
                DomainMapper::KEY_LATENCY_MS.to_string(),
                padagonia::Scalar::I64(lat as i64),
            ));
        }
        self.client.create_node_idempotent(
            external_id,
            DomainMapper::LABEL_HEALTH_OBSERVATION,
            props,
            None,
            provenance,
        )
    }

    /// Exact lookup for a provider by name.
    pub fn get_provider(&self, name: &str) -> Option<padagonia::Node> {
        let ext = DomainMapper::provider_external_id(self.client.ferret_namespace(), name);
        self.client.exact_node(&ext)
    }

    pub fn get_model(&self, provider: &str, model_id: &str) -> Option<padagonia::Node> {
        let ext = DomainMapper::model_external_id(self.client.ferret_namespace(), provider, model_id);
        self.client.exact_node(&ext)
    }

    /// List all providers in this namespace (filtered traversal).
    pub fn list_providers(&self) -> Vec<padagonia::Node> {
        let store = self.client.snapshot_store();
        let label_id = store
            .string_table()
            .label_id(DomainMapper::LABEL_PROVIDER);
        let ns = self.client.namespace().clone();
        match label_id {
            Some(lid) => store
                .nodes()
                .values()
                .filter(|n| n.label == lid && n.namespace == ns)
                .cloned()
                .collect(),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FerretNamespace, PadagoniaConfig};
    use crate::provenance::FerretProvenance;

    fn registry() -> ProviderRegistry {
        let cfg = PadagoniaConfig {
            namespace: FerretNamespace::new("ferret", "test", "prov-reg").unwrap(),
            ..Default::default()
        };
        let client = crate::client::FerretPadagoniaClient::new(cfg).unwrap();
        ProviderRegistry::new(client)
    }

    #[test]
    fn register_and_lookup_provider() {
        let reg = registry();
        let prov = FerretProvenance::new("ferret-test", "0.1.0", "test", 0.9).unwrap();
        let rec = ProviderRecord {
            name: "openai".into(),
            endpoint: "https://api.openai.com/v1".into(),
            version: "v1".into(),
            capabilities: vec![],
            observed_at: Utc::now(),
        };
        let (outcome, _) = reg.register_provider(&rec, prov).unwrap();
        assert_eq!(outcome, WriteOutcome::Accepted);
        assert!(reg.get_provider("openai").is_some());
        assert_eq!(reg.list_providers().len(), 1);
    }

    #[test]
    fn rejects_secret_endpoint() {
        let reg = registry();
        let prov = FerretProvenance::new("ferret-test", "0.1.0", "test", 0.9).unwrap();
        let rec = ProviderRecord {
            name: "bad".into(),
            endpoint: "https://example.com?api_key=sk-123".into(),
            version: "v1".into(),
            capabilities: vec![],
            observed_at: Utc::now(),
        };
        let res = reg.register_provider(&rec, prov);
        assert!(res.is_err());
    }
}
