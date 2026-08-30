use chrono::{DateTime, Utc};

use crate::client::FerretPadagoniaClient;
use crate::domain::DomainMapper;
use padagonia::StringTableExt;

/// Historical performance score with freshness semantics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelScore {
    pub model_id: String,
    pub provider: String,
    pub latency_ms: Option<u64>,
    pub recall: Option<f32>,
    pub cost: Option<f32>,
    pub confidence_interval: Option<(f32, f32)>,
    pub observed_at: DateTime<Utc>,
    pub is_stale: bool,
    pub staleness_reason: Option<String>,
    /// Source provenance references (evidence URIs + correlation IDs)
    pub provenance_refs: Vec<String>,
}

/// Freshness policy: observations older than threshold are explicit stale.
#[derive(Debug, Clone)]
pub struct FreshnessPolicy {
    /// Maximum age for benchmark observations before marked stale (seconds).
    pub benchmark_max_age_secs: u64,
    /// Maximum age for pricing observations before stale.
    pub pricing_max_age_secs: u64,
    /// Maximum age for health observations before stale.
    pub health_max_age_secs: u64,
}

impl Default for FreshnessPolicy {
    fn default() -> Self {
        Self {
            benchmark_max_age_secs: 30 * 24 * 3600, // 30 days
            pricing_max_age_secs: 7 * 24 * 3600,    // 7 days (stale-price handling)
            health_max_age_secs: 3600,              // 1 hour
        }
    }
}

impl FreshnessPolicy {
    pub fn is_stale(&self, observed_at: DateTime<Utc>, kind: FreshnessKind) -> (bool, Option<String>) {
        let max_age = match kind {
            FreshnessKind::Benchmark => self.benchmark_max_age_secs,
            FreshnessKind::Pricing => self.pricing_max_age_secs,
            FreshnessKind::Health => self.health_max_age_secs,
        };
        let age = Utc::now().signed_duration_since(observed_at).num_seconds().max(0) as u64;
        if age >= max_age {
            (
                true,
                Some(format!(
                    "observation age {}s exceeds {}s threshold for {:?}",
                    age, max_age, kind
                )),
            )
        } else {
            (false, None)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FreshnessKind {
    Benchmark,
    Pricing,
    Health,
}

/// Reader for historical performance with explicit stale handling and conflict tolerance.
pub struct ScoringReader {
    client: FerretPadagoniaClient,
    freshness: FreshnessPolicy,
}

impl ScoringReader {
    pub fn new(client: FerretPadagoniaClient, freshness: FreshnessPolicy) -> Self {
        Self { client, freshness }
    }

    pub fn with_default_freshness(client: FerretPadagoniaClient) -> Self {
        Self::new(client, FreshnessPolicy::default())
    }

    pub fn client(&self) -> &FerretPadagoniaClient {
        &self.client
    }

    /// Retrieve historical performance for a model, returning stale-explicit scores.
    /// Tolerates missing, stale, or conflicting facts — never silently overwrites.
    pub fn score_model(&self, provider: &str, model_id: &str) -> Option<ModelScore> {
        let store = self.client.snapshot_store();
        let ns = self.client.namespace().clone();
        let label_id = store.string_table().label_id(DomainMapper::LABEL_BENCHMARK_RUN)?;

        // Filter to nodes in this namespace + label + matching model_id, pick most recent
        let mut candidates: Vec<&padagonia::Node> = store
            .nodes()
            .values()
            .filter(|n| n.label == label_id && n.namespace == ns)
            .filter(|n| {
                n.properties.iter().any(|(k, v)| {
                    store.string_table().resolve_key(*k) == Some("model_id")
                        && matches!(v, padagonia::Scalar::String(s) if s == model_id)
                })
            })
            .filter(|n| {
                n.properties.iter().any(|(k, v)| {
                    store.string_table().resolve_key(*k) == Some("provider")
                        && matches!(v, padagonia::Scalar::String(s) if s == provider)
                })
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Prefer highest provenance timestamp (conflict handling: expose all, pick latest)
        candidates.sort_by_key(|n| std::cmp::Reverse(n.provenance.timestamp));

        let node = candidates[0];
        let observed_at = extract_timestamp(node, &store).unwrap_or_else(|| {
            DateTime::<Utc>::from_timestamp(node.provenance.timestamp as i64, 0)
                .unwrap_or_else(Utc::now)
        });

        let (is_stale, staleness_reason) =
            self.freshness.is_stale(observed_at, FreshnessKind::Benchmark);

        let latency_ms = extract_i64(node, &store, DomainMapper::KEY_LATENCY_MS).map(|v| v as u64);
        let recall = extract_f64(node, &store, DomainMapper::KEY_RECALL).map(|v| v as f32);
        let cost = extract_f64(node, &store, DomainMapper::KEY_COST).map(|v| v as f32);
        let confidence_interval = match (
            extract_f64(node, &store, DomainMapper::KEY_CONFIDENCE_LOW),
            extract_f64(node, &store, DomainMapper::KEY_CONFIDENCE_HIGH),
        ) {
            (Some(lo), Some(hi)) => Some((lo as f32, hi as f32)),
            _ => None,
        };

        // Expose source and timestamp (enterprise: every reader must expose source/timestamp)
        let provenance_refs = vec![
            format!("provenance.agent={}", node.provenance.agent),
            format!("provenance.model={}", node.provenance.model),
            format!("provenance.timestamp={}", node.provenance.timestamp),
            format!("provenance.confidence={} (ranking_signal)", node.provenance.confidence),
        ];

        Some(ModelScore {
            model_id: model_id.to_string(),
            provider: provider.to_string(),
            latency_ms,
            recall,
            cost,
            confidence_interval,
            observed_at,
            is_stale,
            staleness_reason,
            provenance_refs,
        })
    }

    /// Aggregate cost for a namespace/label/property (cost aggregation module).
    /// Returns count/sum/min/max with explicit staleness note if data is old.
    pub fn aggregate_cost(&self, _provider: Option<&str>) -> CostAggregate {
        let store = self.client.snapshot_store();
        let ns = self.client.namespace().clone();
        let label_id = match store.string_table().label_id(DomainMapper::LABEL_BENCHMARK_RUN) {
            Some(id) => id,
            None => return CostAggregate::default(),
        };
        let key_id = match store.string_table().key_id(DomainMapper::KEY_COST) {
            Some(id) => id,
            None => return CostAggregate::default(),
        };

        let engine = padagonia::QueryEngine::new(&store);
        let agg = engine.aggregate_numeric_property(Some(&ns), Some(label_id), key_id);

        // Determine staleness of the underlying observations
        let mut max_age_secs = 0u64;
        for node in store.nodes().values().filter(|n| n.label == label_id && n.namespace == ns) {
            if let Some(ts) = extract_timestamp(node, &store) {
                let age = Utc::now().signed_duration_since(ts).num_seconds().max(0) as u64;
                max_age_secs = max_age_secs.max(age);
            }
        }
        let is_stale = max_age_secs > self.freshness.pricing_max_age_secs;
        CostAggregate {
            count: agg.count,
            sum: agg.sum,
            min: agg.min,
            max: agg.max,
            is_stale,
            staleness_reason: if is_stale {
                Some(format!(
                    "max observation age {}s exceeds pricing threshold {}s",
                    max_age_secs, self.freshness.pricing_max_age_secs
                ))
            } else {
                None
            },
        }
    }

    /// List all scores, including stale-explicit, for conflict visibility.
    pub fn all_scores(&self) -> Vec<ModelScore> {
        let store = self.client.snapshot_store();
        let ns = self.client.namespace().clone();
        let label_id = match store.string_table().label_id(DomainMapper::LABEL_BENCHMARK_RUN) {
            Some(id) => id,
            None => return Vec::new(),
        };
        let mut out = Vec::new();
        for node in store.nodes().values().filter(|n| n.label == label_id && n.namespace == ns) {
            let model_id = extract_string(node, &store, "model_id").unwrap_or_default();
            let provider = extract_string(node, &store, "provider").unwrap_or_default();
            if model_id.is_empty() {
                continue;
            }
            if let Some(score) = self.score_model(&provider, &model_id) {
                // Deduplicate by model_id (keep first per model)
                if !out.iter().any(|s: &ModelScore| s.model_id == model_id && s.provider == provider) {
                    out.push(score);
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default)]
pub struct CostAggregate {
    pub count: usize,
    pub sum: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub is_stale: bool,
    pub staleness_reason: Option<String>,
}

fn extract_timestamp(node: &padagonia::Node, store: &padagonia::Store) -> Option<DateTime<Utc>> {
    let ts = extract_i64_raw(node, store, DomainMapper::KEY_OBSERVED_AT)
        .or_else(|| extract_timestamp_from_provenance(node))?;
    DateTime::<Utc>::from_timestamp(ts, 0)
}

fn extract_timestamp_from_provenance(node: &padagonia::Node) -> Option<i64> {
    Some(node.provenance.timestamp as i64)
}

fn extract_i64(node: &padagonia::Node, store: &padagonia::Store, key: &str) -> Option<i64> {
    extract_i64_raw(node, store, key)
}

fn extract_i64_raw(node: &padagonia::Node, store: &padagonia::Store, key: &str) -> Option<i64> {
    for (k, v) in &node.properties {
        if store.string_table().resolve_key(*k) == Some(key) {
            match v {
                padagonia::Scalar::I64(i) => return Some(*i),
                padagonia::Scalar::Timestamp(t) => return Some(*t as i64),
                _ => continue,
            }
        }
    }
    None
}

fn extract_f64(node: &padagonia::Node, store: &padagonia::Store, key: &str) -> Option<f64> {
    for (k, v) in &node.properties {
        if store.string_table().resolve_key(*k) == Some(key) {
            match v {
                padagonia::Scalar::F64(f) => return Some(*f),
                padagonia::Scalar::I64(i) => return Some(*i as f64),
                _ => continue,
            }
        }
    }
    None
}

fn extract_string(node: &padagonia::Node, store: &padagonia::Store, key: &str) -> Option<String> {
    for (k, v) in &node.properties {
        if store.string_table().resolve_key(*k) == Some(key) {
            if let padagonia::Scalar::String(s) = v {
                return Some(s.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark_ingest::{BenchmarkIngest, WorkloadSpec, BenchmarkRun};
    use crate::config::{FerretNamespace, PadagoniaConfig};
    use crate::provenance::FerretProvenance;

    fn setup() -> (ScoringReader, BenchmarkIngest) {
        let cfg = PadagoniaConfig {
            namespace: FerretNamespace::new("ferret", "test", "scoring").unwrap(),
            ..Default::default()
        };
        let client = crate::client::FerretPadagoniaClient::new(cfg).unwrap();
        let ing = BenchmarkIngest::new(client.clone());
        let reader = ScoringReader::with_default_freshness(client);
        (reader, ing)
    }

    #[test]
    fn fresh_score_not_stale() {
        let (reader, ing) = setup();
        let prov = FerretProvenance::new("ferret-test", "0.1.0", "test", 0.8).unwrap();
        let wl = WorkloadSpec {
            name: "w".into(),
            dataset: "d".into(),
            params_json: "{}".into(),
            seed: 1,
        };
        let run = BenchmarkRun::new(wl, "a100", "m1", "openai", 50)
            .with_recall(0.92)
            .with_cost(0.02);
        ing.ingest(&run, prov).unwrap();
        let score = reader.score_model("openai", "m1").unwrap();
        assert!(!score.is_stale);
        assert!(score.provenance_refs.iter().any(|s| s.contains("ranking_signal")));
    }

    #[test]
    fn stale_observation_explicit() {
        let cfg = crate::config::PadagoniaConfig {
            namespace: FerretNamespace::new("ferret", "test", "stale").unwrap(),
            ..Default::default()
        };
        let client = crate::client::FerretPadagoniaClient::new(cfg).unwrap();
        let mut freshness = FreshnessPolicy::default();
        freshness.benchmark_max_age_secs = 0; // everything stale
        let reader = ScoringReader::new(client.clone(), freshness);
        let ing = BenchmarkIngest::new(client);
        let prov = FerretProvenance::new("ferret-test", "0.1.0", "test", 0.8).unwrap();
        let wl = WorkloadSpec {
            name: "w".into(),
            dataset: "d".into(),
            params_json: "{}".into(),
            seed: 1,
        };
        let run = BenchmarkRun::new(wl, "a100", "m1", "openai", 50);
        ing.ingest(&run, prov).unwrap();
        let score = reader.score_model("openai", "m1").unwrap();
        assert!(score.is_stale);
        assert!(score.staleness_reason.is_some());
    }

    #[test]
    fn missing_returns_none_not_panic() {
        let (reader, _) = setup();
        assert!(reader.score_model("nope", "missing").is_none());
    }
}
