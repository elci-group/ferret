use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Category, RepoId, TeacherKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub Uuid);

impl SnapshotId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SnapshotId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSnapshot {
    pub id: SnapshotId,
    pub repo_id: RepoId,
    pub pr_number: Option<u32>,
    pub base_sha: String,
    pub head_sha: String,
    pub hunks: Vec<DiffHunk>,
    pub teachers: Vec<TeacherKind>,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub id: String,
    pub file_path: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub content: String,
    pub language: Option<String>,
}

impl DiffHunk {
    pub fn summary(&self) -> String {
        format!("{}:{}-{}", self.file_path, self.new_start, self.new_lines)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedFinding {
    pub hunk_id: Option<String>,
    pub category: Category,
    pub severity: crate::Severity,
    pub message: String,
    pub confidence: f32,
    pub rationale_refs: Vec<String>,
}
