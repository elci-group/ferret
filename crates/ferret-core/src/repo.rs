use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Category;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepoId(pub uuid::Uuid);

impl RepoId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_slug(slug: &str) -> Self {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(slug.as_bytes());
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&hash[..16]);
        Self(uuid::Uuid::from_bytes(bytes))
    }
}

impl Default for RepoId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoProfile {
    pub slug: String,
    pub strictness: HashMap<Category, f32>,
    pub teacher_reliability: HashMap<String, f32>,
    pub total_snapshots: u64,
    pub total_findings: u64,
}

impl RepoProfile {
    pub fn new(slug: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            ..Default::default()
        }
    }
}
