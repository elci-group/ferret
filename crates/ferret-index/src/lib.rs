//! Corpus indexing hooks for retrieval.

use ferret_core::ReviewSnapshot;
use ferret_store::Store;

pub struct Indexer;

impl Indexer {
    pub async fn index_snapshot(_store: &Store, snapshot: &ReviewSnapshot) -> anyhow::Result<()> {
        // Milestone 2: tantivy / embedding index
        let _ = snapshot;
        Ok(())
    }
}
