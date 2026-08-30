use chrono::{DateTime, Utc};

use crate::client::{ClientError, FerretPadagoniaClient, WriteOutcome};
use crate::domain::DomainMapper;
use padagonia::StringTableExt;
use crate::provenance::{ContentHash, FerretProvenance};

/// Reproducible workload specification.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkloadSpec {
    pub name: String,
    pub dataset: String,
    pub params_json: String,
    pub seed: u64,
}

impl WorkloadSpec {
    /// Canonical string for hashing (deterministic, versioned).
    pub fn canonical(&self) -> String {
        format!(
            "workload:{};dataset:{};params:{};seed:{};schema:{}",
            self.name,
            self.dataset,
            self.params_json,
            self.seed,
            crate::CURRENT_SCHEMA_VERSION
        )
    }

    pub fn content_hash(&self) -> ContentHash {
        ContentHash::from_canonical(&self.canonical())
    }
}

/// Full benchmark run with hardware, latency, recall, cost, and confidence interval.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkRun {
    pub workload: WorkloadSpec,
    pub hardware: String, // e.g. "a100-40gb", "cpu-c7i.4xlarge"
    pub model_id: String,
    pub provider: String,
    pub latency_ms: u64,
    pub recall: Option<f32>,
    pub cost: Option<f32>,
    pub confidence_low: Option<f32>,
    pub confidence_high: Option<f32>,
    pub content_hash: ContentHash,
    pub observed_at: DateTime<Utc>,
}

impl BenchmarkRun {
    pub fn new(
        workload: WorkloadSpec,
        hardware: impl Into<String>,
        model_id: impl Into<String>,
        provider: impl Into<String>,
        latency_ms: u64,
    ) -> Self {
        let hardware = hardware.into();
        let model_id = model_id.into();
        let provider = provider.into();
        let content_hash = ContentHash::from_canonical(&format!(
            "{};hardware:{};model:{};provider:{}",
            workload.canonical(),
            hardware,
            model_id,
            provider
        ));
        Self {
            workload,
            hardware,
            model_id,
            provider,
            latency_ms,
            recall: None,
            cost: None,
            confidence_low: None,
            confidence_high: None,
            content_hash,
            observed_at: Utc::now(),
        }
    }

    pub fn with_recall(mut self, recall: f32) -> Self {
        self.recall = Some(recall);
        self
    }

    pub fn with_cost(mut self, cost: f32) -> Self {
        self.cost = Some(cost);
        self
    }

    pub fn with_confidence_interval(mut self, low: f32, high: f32) -> Self {
        self.confidence_low = Some(low);
        self.confidence_high = Some(high);
        self
    }

    /// Verify that stored content hash matches recomputed canonical hash.
    pub fn verify_hash(&self) -> bool {
        let recomputed = ContentHash::from_canonical(&format!(
            "{};hardware:{};model:{};provider:{}",
            self.workload.canonical(),
            self.hardware,
            self.model_id,
            self.provider
        ));
        recomputed == self.content_hash
    }
}

/// Ingest for reproducible benchmark runs.
pub struct BenchmarkIngest {
    client: FerretPadagoniaClient,
}

impl BenchmarkIngest {
    pub fn new(client: FerretPadagoniaClient) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &FerretPadagoniaClient {
        &self.client
    }

    /// Store a benchmark run idempotently (external_id = workload_hash:hardware:model_id).
    /// Enforces bounded Sizes, redaction, quota, and provenance validation via client.
    pub fn ingest(
        &self,
        run: &BenchmarkRun,
        provenance: FerretProvenance,
    ) -> Result<(WriteOutcome, padagonia::id::NodeId), ClientError> {
        if !run.verify_hash() {
            return Ok((
                WriteOutcome::Rejected("content_hash mismatch: recomputed hash differs".into()),
                padagonia::id::NodeId(0),
            ));
        }
        if let Some(r) = run.recall {
            if !r.is_finite() || !(0.0..=1.0).contains(&r) {
                return Ok((
                    WriteOutcome::Rejected("recall must be finite in 0..1".into()),
                    padagonia::id::NodeId(0),
                ));
            }
        }
        if let Some(c) = run.cost {
            if !c.is_finite() || c < 0.0 {
                return Ok((
                    WriteOutcome::Rejected("cost must be finite >=0".into()),
                    padagonia::id::NodeId(0),
                ));
            }
        }
        if let (Some(lo), Some(hi)) = (run.confidence_low, run.confidence_high) {
            if !lo.is_finite() || !hi.is_finite() || lo > hi {
                return Ok((
                    WriteOutcome::Rejected("confidence interval invalid".into()),
                    padagonia::id::NodeId(0),
                ));
            }
        }

        let external_id = DomainMapper::benchmark_external_id(
            self.client.ferret_namespace(),
            run.content_hash.as_str(),
            &run.hardware,
            &run.model_id,
        );

        let mut props = vec![
            (
                DomainMapper::KEY_WORKLOAD_HASH.to_string(),
                padagonia::Scalar::String(run.workload.content_hash().to_string()),
            ),
            (
                DomainMapper::KEY_CONTENT_HASH.to_string(),
                padagonia::Scalar::String(run.content_hash.to_string()),
            ),
            (
                DomainMapper::KEY_HARDWARE.to_string(),
                padagonia::Scalar::String(run.hardware.clone()),
            ),
            (
                "model_id".to_string(),
                padagonia::Scalar::String(run.model_id.clone()),
            ),
            (
                "provider".to_string(),
                padagonia::Scalar::String(run.provider.clone()),
            ),
            (
                DomainMapper::KEY_LATENCY_MS.to_string(),
                padagonia::Scalar::I64(run.latency_ms as i64),
            ),
            (
                DomainMapper::KEY_OBSERVED_AT.to_string(),
                padagonia::Scalar::Timestamp(run.observed_at.timestamp() as u64),
            ),
            (
                "workload_name".to_string(),
                padagonia::Scalar::String(run.workload.name.clone()),
            ),
            (
                "workload_canonical".to_string(),
                padagonia::Scalar::String(run.workload.canonical()),
            ),
            (
                DomainMapper::KEY_SCHEMA_VERSION.to_string(),
                padagonia::Scalar::I64(crate::CURRENT_SCHEMA_VERSION as i64),
            ),
        ];
        if let Some(r) = run.recall {
            props.push((
                DomainMapper::KEY_RECALL.to_string(),
                padagonia::Scalar::F64(r as f64),
            ));
        }
        if let Some(c) = run.cost {
            props.push((
                DomainMapper::KEY_COST.to_string(),
                padagonia::Scalar::F64(c as f64),
            ));
        }
        if let Some(lo) = run.confidence_low {
            props.push((
                DomainMapper::KEY_CONFIDENCE_LOW.to_string(),
                padagonia::Scalar::F64(lo as f64),
            ));
        }
        if let Some(hi) = run.confidence_high {
            props.push((
                DomainMapper::KEY_CONFIDENCE_HIGH.to_string(),
                padagonia::Scalar::F64(hi as f64),
            ));
        }

        self.client.create_node_idempotent(
            external_id,
            DomainMapper::LABEL_BENCHMARK_RUN,
            props,
            None,
            provenance,
        )
    }

    /// Lookup a benchmark run by its content hash triple.
    pub fn get(&self, content_hash: &str, hardware: &str, model_id: &str) -> Option<padagonia::Node> {
        let ext = DomainMapper::benchmark_external_id(
            self.client.ferret_namespace(),
            content_hash,
            hardware,
            model_id,
        );
        self.client.exact_node(&ext)
    }

    /// List all benchmark runs for a model (filtered traversal).
    pub fn list_for_model(&self, model_id: &str) -> Vec<padagonia::Node> {
        let store = self.client.snapshot_store();
        let lid = store.string_table().label_id(DomainMapper::LABEL_BENCHMARK_RUN);
        let ns = self.client.namespace().clone();
        let Some(lid) = lid else {
            return Vec::new();
        };
        store
            .nodes()
            .values()
            .filter(|n| n.label == lid && n.namespace == ns)
            .filter(|n| {
                n.properties.iter().any(|(k, v)| {
                    store
                        .string_table()
                        .resolve_key(*k)
                        == Some("model_id")
                        && matches!(v, padagonia::Scalar::String(s) if s == model_id)
                })
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FerretNamespace, PadagoniaConfig};
    use crate::provenance::FerretProvenance;

    fn ingest() -> BenchmarkIngest {
        let cfg = PadagoniaConfig {
            namespace: FerretNamespace::new("ferret", "test", "bench").unwrap(),
            ..Default::default()
        };
        let client = crate::client::FerretPadagoniaClient::new(cfg).unwrap();
        BenchmarkIngest::new(client)
    }

    #[test]
    fn ingest_is_idempotent_and_hash_verified() {
        let ing = ingest();
        let prov = FerretProvenance::new("ferret-bench", "0.1.0", "test", 0.85).unwrap();
        let wl = WorkloadSpec {
            name: "retrieval-qna".into(),
            dataset: "nq-open".into(),
            params_json: r#"{"k":10}"#.into(),
            seed: 42,
        };
        let run = BenchmarkRun::new(wl, "a100", "gpt-4o", "openai", 123);
        assert!(run.verify_hash());
        let (o1, id1) = ing.ingest(&run, prov.clone()).unwrap();
        assert_eq!(o1, WriteOutcome::Accepted);
        let (o2, id2) = ing.ingest(&run, prov).unwrap();
        assert_eq!(o2, WriteOutcome::Accepted);
        assert_eq!(id1, id2);
    }

    #[test]
    fn rejects_hash_mismatch() {
        let ing = ingest();
        let prov = FerretProvenance::new("ferret-bench", "0.1.0", "test", 0.85).unwrap();
        let wl = WorkloadSpec {
            name: "w".into(),
            dataset: "d".into(),
            params_json: "{}".into(),
            seed: 1,
        };
        let mut run = BenchmarkRun::new(wl, "cpu", "m1", "p1", 10);
        run.content_hash = ContentHash("bad_hash".into());
        assert!(!run.verify_hash());
        let (o, _) = ing.ingest(&run, prov).unwrap();
        assert_eq!(o, WriteOutcome::Rejected("content_hash mismatch: recomputed hash differs".into()));
    }

    #[test]
    fn workload_canonical_stable() {
        let a = WorkloadSpec {
            name: "w".into(),
            dataset: "d".into(),
            params_json: "{}".into(),
            seed: 1,
        };
        let b = WorkloadSpec {
            name: "w".into(),
            dataset: "d".into(),
            params_json: "{}".into(),
            seed: 1,
        };
        assert_eq!(a.canonical(), b.canonical());
        assert_eq!(a.content_hash(), b.content_hash());
    }
}
