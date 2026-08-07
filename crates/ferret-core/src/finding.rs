use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{SnapshotId, TeacherKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FindingId(pub Uuid);

impl FindingId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for FindingId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Style,
    Safety,
    Architecture,
    Performance,
    Testing,
    Documentation,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Minor,
    Major,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub id: FindingId,
    pub snapshot_id: SnapshotId,
    pub teacher: TeacherKind,
    pub hunk_id: Option<String>,
    pub category: Category,
    pub severity: Severity,
    pub message: String,
    pub suggestion: Option<String>,
    pub rule_id: Option<String>,
    pub captured_at: DateTime<Utc>,
}
