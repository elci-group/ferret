use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::FindingId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Accepted,
    Rejected,
    Ignored,
    Fixed,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub finding_id: FindingId,
    pub disposition: Disposition,
    pub fixed_in_sha: Option<String>,
    pub human_override: bool,
    pub recorded_at: DateTime<Utc>,
}
