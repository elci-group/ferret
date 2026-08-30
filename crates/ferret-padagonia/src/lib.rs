//! PADAGONIA integration for Ferret — review memory that learns.
//!
//! Implements the five mandated modules from `PADAGONIA_INTEGRATION.md`:
//! - `provider_registry`: persist provider/model capabilities, endpoints, versions, health
//! - `benchmark_ingest`: reproducible workloads with content hashes
//! - `scoring_reader`: freshness-bounded historical performance retrieval
//! - `decision_explainer`: link routing decisions to observations and policy versions
//! - `fallback_queue`: bounded offline queue with idempotent replay
//!
//! Plus supporting infrastructure required by the enterprise directives:
//! - `config`: typed client, versioned namespace, quotas, timeouts, credentials
//! - `domain`: deterministic mapping from Ferret entities to graph labels/relations
//! - `provenance`: immutable fact provenance with redaction and content hashes
//! - `security`: secret-safe diagnostics and credential isolation
//! - `client`: bounded retries, idempotency, correlation IDs, error mapping
//!
//! # Enterprise baseline compliance
//! - Every write uses a versioned namespace `<product>.<environment>.<tenant>`
//!   and attaches immutable provenance (producer, version, source, timestamps,
//!   confidence semantics, correlation ID, retention class).
//! - Caller-supplied confidence is treated as ranking signal, never calibrated probability.
//! - Writes use idempotency keys, bounded sizes, bounded graph growth, timeouts,
//!   and retries that distinguish Accepted/Rejected/Unknown.
//! - Readers tolerate missing/stale/conflicting facts and expose source + timestamp.
//! - Secrets are never persisted; redaction is enforced at the write boundary.
//! - Routing remains safe when Padagonia is unavailable via `FallbackQueue`.

pub mod benchmark_ingest;
pub mod client;
pub mod config;
pub mod decision_explainer;
pub mod domain;
pub mod fallback_queue;
pub mod provenance;
pub mod provider_registry;
pub mod scoring_reader;
pub mod security;

pub use benchmark_ingest::{BenchmarkIngest, BenchmarkRun, WorkloadSpec};
pub use client::{ClientError, FerretPadagoniaClient, WriteOutcome};
pub use config::{FerretNamespace, PadagoniaConfig};
pub use decision_explainer::{DecisionExplainer, RoutingDecisionRecord};
pub use fallback_queue::{FallbackQueue, QueuedMutation};
pub use provenance::{ContentHash, FerretProvenance, ProvenanceError};
pub use provider_registry::{ModelRecord, ProviderRecord, ProviderRegistry};
pub use scoring_reader::{ModelScore, ScoringReader};
pub use security::{RedactionError, RedactionPolicy};

/// Current schema version for all Ferret graph entities.
pub const CURRENT_SCHEMA_VERSION: u16 = 1;

/// API version negotiated with Padagonia.
pub const PADAGONIA_API_VERSION: &str = "v1";

/// Maximum properties per node/edge (enterprise bounded request size).
pub const MAX_PROPERTIES: usize = 1024;

/// Maximum embedding dimension (enterprise bounded vector).
pub const MAX_EMBEDDING_DIM: usize = 4096;

/// Maximum queued mutations when Padagonia is unavailable.
pub const MAX_QUEUE_SIZE: usize = 10_000;
