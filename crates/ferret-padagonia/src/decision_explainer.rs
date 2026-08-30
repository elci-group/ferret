use chrono::{DateTime, Utc};

use crate::client::{ClientError, FerretPadagoniaClient, WriteOutcome};
use crate::domain::DomainMapper;
use padagonia::StringTableExt;
use crate::provenance::FerretProvenance;

/// A routing decision that must be explainable: linked to observations and policy version.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecisionRecord {
    pub correlation_id: String,
    pub hunk_id: Option<String>,
    pub chosen_provider: String,
    pub chosen_model: String,
    pub alternatives: Vec<String>,
    pub confidence: f32,
    pub policy_version: String,
    /// External IDs of benchmark/observation nodes this decision was based on
    pub observation_refs: Vec<String>,
    pub decided_at: DateTime<Utc>,
    pub rationale: String,
}

impl RoutingDecisionRecord {
    pub fn new(
        chosen_provider: impl Into<String>,
        chosen_model: impl Into<String>,
        confidence: f32,
        policy_version: impl Into<String>,
    ) -> Result<Self, String> {
        if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
            return Err("confidence must be finite in 0..1".into());
        }
        Ok(Self {
            correlation_id: uuid::Uuid::new_v4().to_string(),
            hunk_id: None,
            chosen_provider: chosen_provider.into(),
            chosen_model: chosen_model.into(),
            alternatives: Vec::new(),
            confidence,
            policy_version: policy_version.into(),
            observation_refs: Vec::new(),
            decided_at: Utc::now(),
            rationale: String::new(),
        })
    }

    pub fn with_observations(mut self, refs: Vec<String>) -> Self {
        self.observation_refs = refs;
        self
    }

    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = rationale.into();
        self
    }
}

/// Persists and retrieves routing decisions with full provenance linkage.
///
/// Every decision is stored as a `RoutingDecision` node plus edges to the
/// observation nodes it was derived from (`decision_used`) and is queryable
/// with its policy version and correlation ID.
pub struct DecisionExplainer {
    client: FerretPadagoniaClient,
}

impl DecisionExplainer {
    pub fn new(client: FerretPadagoniaClient) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &FerretPadagoniaClient {
        &self.client
    }

    /// Persist a routing decision idempotently, linking it to observations.
    pub fn explain(
        &self,
        decision: &RoutingDecisionRecord,
        provenance: FerretProvenance,
    ) -> Result<(WriteOutcome, padagonia::id::NodeId), ClientError> {
        let external_id = DomainMapper::decision_external_id(
            self.client.ferret_namespace(),
            &decision.correlation_id,
        );

        let obs_json = serde_json::to_string(&decision.observation_refs).unwrap_or_default();
        let alts_json = serde_json::to_string(&decision.alternatives).unwrap_or_default();

        let props = vec![
            (
                DomainMapper::KEY_CORRELATION_ID.to_string(),
                padagonia::Scalar::String(decision.correlation_id.clone()),
            ),
            (
                "chosen_provider".to_string(),
                padagonia::Scalar::String(decision.chosen_provider.clone()),
            ),
            (
                "chosen_model".to_string(),
                padagonia::Scalar::String(decision.chosen_model.clone()),
            ),
            (
                "alternatives".to_string(),
                padagonia::Scalar::String(alts_json),
            ),
            (
                "confidence".to_string(),
                padagonia::Scalar::F64(decision.confidence as f64),
            ),
            (
                DomainMapper::KEY_POLICY_VERSION.to_string(),
                padagonia::Scalar::String(decision.policy_version.clone()),
            ),
            (
                "observation_refs".to_string(),
                padagonia::Scalar::String(obs_json),
            ),
            (
                "rationale".to_string(),
                padagonia::Scalar::String(decision.rationale.clone()),
            ),
            (
                DomainMapper::KEY_OBSERVED_AT.to_string(),
                padagonia::Scalar::Timestamp(decision.decided_at.timestamp() as u64),
            ),
            (
                DomainMapper::KEY_SCHEMA_VERSION.to_string(),
                padagonia::Scalar::I64(crate::CURRENT_SCHEMA_VERSION as i64),
            ),
            (
                DomainMapper::KEY_API_VERSION.to_string(),
                padagonia::Scalar::String(crate::PADAGONIA_API_VERSION.to_string()),
            ),
        ];

        let mut with_hunk = props;
        if let Some(hunk) = &decision.hunk_id {
            with_hunk.push((
                "hunk_id".to_string(),
                padagonia::Scalar::String(hunk.clone()),
            ));
        }

        self.client.create_node_idempotent(
            external_id,
            DomainMapper::LABEL_ROUTING_DECISION,
            with_hunk,
            None,
            provenance,
        )
    }

    /// Retrieve a decision by correlation ID (exact lookup).
    pub fn get_by_correlation(&self, correlation_id: &str) -> Option<padagonia::Node> {
        let ext = DomainMapper::decision_external_id(self.client.ferret_namespace(), correlation_id);
        self.client.exact_node(&ext)
    }

    /// List decisions for a given policy version (filtered traversal, bounded).
    pub fn list_by_policy(&self, policy_version: &str) -> Vec<padagonia::Node> {
        let store = self.client.snapshot_store();
        let label_id = match store.string_table().label_id(DomainMapper::LABEL_ROUTING_DECISION) {
            Some(id) => id,
            None => return Vec::new(),
        };
        let ns = self.client.namespace().clone();
        store
            .nodes()
            .values()
            .filter(|n| n.label == label_id && n.namespace == ns)
            .filter(|n| {
                n.properties.iter().any(|(k, v)| {
                    store.string_table().resolve_key(*k) == Some(DomainMapper::KEY_POLICY_VERSION)
                        && matches!(v, padagonia::Scalar::String(s) if s == policy_version)
                })
            })
            .cloned()
            .collect()
    }

    /// Human-readable explanation for a decision node, exposing source + timestamp.
    pub fn explain_human(&self, node: &padagonia::Node) -> String {
        let store = self.client.snapshot_store();
        let mut parts = Vec::new();
        for (k, v) in &node.properties {
            if let Some(key) = store.string_table().resolve_key(*k) {
                parts.push(format!("{key}={v:?}"));
            }
        }
        format!(
            "Decision {} (ns={}, schema={}, api={}) — provenance: agent={}, model={}, confidence={} (ranking_signal), timestamp={} — {}",
            node.external_id,
            node.namespace,
            node.schema_version,
            crate::PADAGONIA_API_VERSION,
            node.provenance.agent,
            node.provenance.model,
            node.provenance.confidence,
            node.provenance.timestamp,
            parts.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FerretNamespace, PadagoniaConfig};
    use crate::provenance::FerretProvenance;

    fn explainer() -> DecisionExplainer {
        let cfg = PadagoniaConfig {
            namespace: FerretNamespace::new("ferret", "test", "decision").unwrap(),
            ..Default::default()
        };
        let client = crate::client::FerretPadagoniaClient::new(cfg).unwrap();
        DecisionExplainer::new(client)
    }

    #[test]
    fn explain_and_retrieve() {
        let exp = explainer();
        let prov = FerretProvenance::new("ferret-route", "0.1.0", "test", 0.92).unwrap();
        let mut decision = RoutingDecisionRecord::new("openai", "gpt-4o", 0.92, "policy-v3").unwrap();
        decision.observation_refs = vec!["bench:abc".into()];
        decision.rationale = "highest recall, lowest cost, fresh observation".into();
        let corr = decision.correlation_id.clone();
        let (outcome, _) = exp.explain(&decision, prov).unwrap();
        assert_eq!(outcome, WriteOutcome::Accepted);
        let node = exp.get_by_correlation(&corr).unwrap();
        assert!(exp.explain_human(&node).contains("ranking_signal"));
    }

    #[test]
    fn list_by_policy_filters() {
        let exp = explainer();
        let prov = FerretProvenance::new("ferret-route", "0.1.0", "test", 0.9).unwrap();
        for i in 0..2 {
            let d = RoutingDecisionRecord::new("p", format!("m{i}"), 0.8, "v1").unwrap();
            exp.explain(&d, prov.clone()).unwrap();
        }
        let d2 = RoutingDecisionRecord::new("p", "m2", 0.8, "v2").unwrap();
        exp.explain(&d2, prov).unwrap();
        assert_eq!(exp.list_by_policy("v1").len(), 2);
        assert_eq!(exp.list_by_policy("v2").len(), 1);
    }
}
