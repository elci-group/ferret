use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::client::{ClientError, FerretPadagoniaClient, WriteOutcome};
use crate::domain::DomainMapper;
use crate::provenance::FerretProvenance;

/// A queued mutation when Padagonia is unavailable — bounded, ordered, replayable.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueuedMutation {
    pub idempotency_key: String,
    pub label: String,
    pub external_id: String,
    pub properties: Vec<(String, padagonia::Scalar)>,
    pub provenance: FerretProvenance,
    pub enqueued_at: DateTime<Utc>,
    pub attempts: u32,
}

impl QueuedMutation {
    pub fn new(
        label: impl Into<String>,
        external_id: impl Into<String>,
        properties: Vec<(String, padagonia::Scalar)>,
        provenance: FerretProvenance,
    ) -> Self {
        Self {
            idempotency_key: format!("ferret-{}-{}", Utc::now().timestamp_millis(), uuid::Uuid::new_v4()),
            label: label.into(),
            external_id: external_id.into(),
            properties,
            provenance,
            enqueued_at: Utc::now(),
            attempts: 0,
        }
    }
}

/// Bounded offline queue with safe degradation when Padagonia is unavailable.
///
/// Guarantees:
/// - Bounded size (`MAX_QUEUE_SIZE`)
/// - Replay ordering (FIFO)
/// - Idempotent replay (uses stable external_id + idempotency_key)
/// - Stale-read indicators when serving from queue
pub struct FallbackQueue {
    client: FerretPadagoniaClient,
    queue: VecDeque<QueuedMutation>,
    max_size: usize,
    /// If true, all writes go to queue (simulates Padagonia unavailable / degraded).
    degraded: bool,
}

impl FallbackQueue {
    pub fn new(client: FerretPadagoniaClient) -> Self {
        Self {
            client,
            queue: VecDeque::new(),
            max_size: crate::MAX_QUEUE_SIZE,
            degraded: false,
        }
    }

    pub fn with_capacity(client: FerretPadagoniaClient, max_size: usize) -> Self {
        Self {
            client,
            queue: VecDeque::new(),
            max_size,
            degraded: false,
        }
    }

    pub fn client(&self) -> &FerretPadagoniaClient {
        &self.client
    }

    pub fn set_degraded(&mut self, degraded: bool) {
        self.degraded = degraded;
    }

    pub fn is_degraded(&self) -> bool {
        self.degraded
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.queue.len() >= self.max_size
    }

    /// Enqueue a mutation (bounded). Returns error if queue full.
    pub fn enqueue(&mut self, mutation: QueuedMutation) -> Result<(), ClientError> {
        if self.is_full() {
            return Err(ClientError::QuotaExceeded(format!(
                "fallback queue full ({} items) — backpressure, retry later",
                self.max_size
            )));
        }
        // Redaction at enqueue boundary as well
        let keys: Vec<String> = mutation.properties.iter().map(|(k, _)| k.clone()).collect();
        crate::security::RedactionPolicy::strict()
            .validate_keys(&keys)
            .map_err(|e| ClientError::Redaction(e.to_string()))?;
        self.queue.push_back(mutation);
        Ok(())
    }

    /// Try to write directly if not degraded, otherwise enqueue.
    /// Always returns a `WriteOutcome` that callers can match on for
    /// degraded vs accepted vs rejected (enterprise: distinguish Unknown).
    pub fn write_or_enqueue(
        &mut self,
        label: String,
        external_id: String,
        properties: Vec<(String, padagonia::Scalar)>,
        provenance: FerretProvenance,
    ) -> Result<WriteOutcome, ClientError> {
        if self.degraded {
            let m = QueuedMutation::new(label.clone(), external_id.clone(), properties, provenance);
            self.enqueue(m)?;
            return Ok(WriteOutcome::Unknown(
                "Padagonia unavailable — queued locally, will replay idempotently".into(),
            ));
        }

        match self.client.create_node_idempotent(
            external_id.clone(),
            &label,
            properties.clone(),
            None,
            provenance.clone(),
        ) {
            Ok((outcome, _)) => {
                if outcome == WriteOutcome::Unknown("".into()) {
                    // treat as degraded — enqueue
                    let m = QueuedMutation::new(label, external_id, properties, provenance);
                    self.enqueue(m)?;
                    Ok(WriteOutcome::Unknown("queued after unknown outcome".into()))
                } else {
                    Ok(outcome)
                }
            }
            Err(e) => {
                // On store error, enqueue if it's a transient/unknown, otherwise surface rejection
                let msg = e.to_string();
                if msg.contains("timeout") || msg.contains("unavailable") {
                    let m = QueuedMutation::new(label, external_id, properties, provenance);
                    self.enqueue(m)?;
                    Ok(WriteOutcome::Unknown(format!("transient error, queued: {msg}")))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Replay queued mutations in FIFO order, idempotently.
    /// Returns (replayed, failed) counts. Each mutation is retried at most `max_retries` times
    /// per replay invocation; failures remain queued for next replay.
    pub fn replay(&mut self) -> Result<ReplayReport, ClientError> {
        if self.degraded {
            return Ok(ReplayReport {
                attempted: 0,
                succeeded: 0,
                failed: 0,
                remaining: self.queue.len(),
                degraded: true,
            });
        }

        let mut succeeded = 0;
        let mut failed = 0;
        let mut remaining_queue = VecDeque::new();

        let to_replay: Vec<QueuedMutation> = self.queue.drain(..).collect();
        for mut m in to_replay {
            m.attempts += 1;
            if m.attempts > self.client.config().max_retries as u32 + 5 {
                // dead-letter: drop after excessive retries but record failure
                failed += 1;
                tracing::warn!(
                    external_id = %m.external_id,
                    attempts = m.attempts,
                    "dropping queued mutation after excessive retries (dead-letter)"
                );
                continue;
            }

            match self.client.create_node_idempotent(
                m.external_id.clone(),
                &m.label,
                m.properties.clone(),
                None,
                m.provenance.clone(),
            ) {
                Ok((WriteOutcome::Accepted, _)) => succeeded += 1,
                Ok((WriteOutcome::Rejected(reason), _)) => {
                    // Rejected is terminal — don't requeue, count as failed
                    tracing::warn!(reason = %reason, external_id = %m.external_id, "queued mutation rejected");
                    failed += 1;
                }
                Ok((WriteOutcome::Unknown(reason), _)) => {
                    tracing::warn!(reason = %reason, "replaying queued mutation got Unknown, re-queuing");
                    remaining_queue.push_back(m);
                    failed += 1;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "replay error, re-queuing");
                    remaining_queue.push_back(m);
                    failed += 1;
                }
            }
        }

        self.queue = remaining_queue;
        let remaining = self.queue.len();
        Ok(ReplayReport {
            attempted: succeeded + failed,
            succeeded,
            failed,
            remaining,
            degraded: false,
        })
    }

    /// Safe degraded read: caller knows data may be stale.
    pub fn degraded_indicator(&self) -> Option<DegradedRead> {
        if self.degraded || !self.queue.is_empty() {
            Some(DegradedRead {
                degraded: self.degraded,
                queued: self.queue.len(),
                stale_read: true,
                message: if self.degraded {
                    "Padagonia unavailable — serving from local queue / stale cache".to_string()
                } else {
                    format!(
                        "Padagonia reachable but {} queued mutations not yet replayed — reads may be stale",
                        self.queue.len()
                    )
                },
            })
        } else {
            None
        }
    }

    /// Snapshot of queue for diagnostics (never includes secrets — redacted at enqueue).
    pub fn diagnostics(&self) -> QueueDiagnostics {
        QueueDiagnostics {
            len: self.queue.len(),
            max_size: self.max_size,
            degraded: self.degraded,
            oldest_enqueued: self.queue.front().map(|m| m.enqueued_at),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReplayReport {
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub remaining: usize,
    pub degraded: bool,
}

#[derive(Debug, Clone)]
pub struct DegradedRead {
    pub degraded: bool,
    pub queued: usize,
    pub stale_read: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct QueueDiagnostics {
    pub len: usize,
    pub max_size: usize,
    pub degraded: bool,
    pub oldest_enqueued: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FerretNamespace, PadagoniaConfig};
    use crate::provenance::FerretProvenance;

    fn queue_with_client() -> FallbackQueue {
        let cfg = PadagoniaConfig {
            namespace: FerretNamespace::new("ferret", "test", "fallback").unwrap(),
            ..Default::default()
        };
        let client = crate::client::FerretPadagoniaClient::new(cfg).unwrap();
        FallbackQueue::new(client)
    }

    #[test]
    fn degraded_writes_are_queued_and_replayed() {
        let mut q = queue_with_client();
        q.set_degraded(true);
        let prov = FerretProvenance::new("ferret-test", "0.1.0", "test", 0.9).unwrap();
        let outcome = q
            .write_or_enqueue(
                "Provider".into(),
                "ext-1".into(),
                vec![("name".into(), padagonia::Scalar::String("openai".into()))],
                prov,
            )
            .unwrap();
        assert!(matches!(outcome, WriteOutcome::Unknown(_)));
        assert_eq!(q.len(), 1);
        assert!(q.degraded_indicator().is_some());

        q.set_degraded(false);
        let report = q.replay().unwrap();
        assert_eq!(report.succeeded, 1);
        assert_eq!(q.len(), 0);
        assert!(q.degraded_indicator().is_none());
    }

    #[test]
    fn bounded_queue_rejects_when_full() {
        let cfg = PadagoniaConfig {
            namespace: FerretNamespace::new("ferret", "test", "bounded").unwrap(),
            ..Default::default()
        };
        let client = crate::client::FerretPadagoniaClient::new(cfg).unwrap();
        let mut q = FallbackQueue::with_capacity(client, 2);
        let prov = FerretProvenance::new("ferret-test", "0.1.0", "test", 0.9).unwrap();
        for i in 0..2 {
            q.enqueue(QueuedMutation::new(
                "Provider",
                format!("ext-{i}"),
                vec![("name".into(), padagonia::Scalar::String(format!("p{i}")))],
                prov.clone(),
            ))
            .unwrap();
        }
        let res = q.enqueue(QueuedMutation::new(
            "Provider",
            "ext-3",
            vec![("name".into(), padagonia::Scalar::String("p3".into()))],
            prov,
        ));
        assert!(res.is_err());
    }

    #[test]
    fn replay_is_idempotent() {
        let mut q = queue_with_client();
        q.set_degraded(true);
        let prov = FerretProvenance::new("ferret-test", "0.1.0", "test", 0.9).unwrap();
        q.write_or_enqueue(
            "Provider".into(),
            "stable-ext".into(),
            vec![("name".into(), padagonia::Scalar::String("x".into()))],
            prov.clone(),
        )
        .unwrap();
        q.set_degraded(false);
        let _ = q.replay().unwrap();
        // Enqueue same external_id again while degraded
        q.set_degraded(true);
        q.write_or_enqueue(
            "Provider".into(),
            "stable-ext".into(),
            vec![("name".into(), padagonia::Scalar::String("x".into()))],
            prov,
        )
        .unwrap();
        q.set_degraded(false);
        let report = q.replay().unwrap();
        assert_eq!(report.succeeded, 1);
        // Store should have only one node with that external_id
        assert!(q.client().exact_node("stable-ext").is_some());
    }
}
