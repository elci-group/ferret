//! Retrieval-based inference for review predictions.

use ferret_core::{Category, PredictedFinding, ReviewSnapshot, Severity};
use ferret_store::Store;

pub struct RetrievalPredictor;

impl RetrievalPredictor {
    pub async fn predict(
        store: &Store,
        snapshot: &ReviewSnapshot,
    ) -> anyhow::Result<Vec<PredictedFinding>> {
        let mut predictions = Vec::new();

        for hunk in &snapshot.hunks {
            let prior = store.similar_hunk_messages(&hunk.file_path, 5).await?;
            if prior.is_empty() {
                continue;
            }

            let confidence = (prior.len() as f32 * 0.15).min(0.85);
            predictions.push(PredictedFinding {
                hunk_id: Some(hunk.id.clone()),
                category: Category::Style,
                severity: Severity::Minor,
                message: format!("Similar hunks were flagged: {}", prior[0]),
                confidence,
                rationale_refs: prior,
            });
        }

        Ok(predictions)
    }
}
