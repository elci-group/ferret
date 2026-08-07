use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{PredictedFinding, ReviewFinding, ReviewSnapshot, SnapshotId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeacherKind {
    CodeRabbit,
    GitHub,
    Human,
    Sarif,
    CiFixLoop,
    Ferret,
}

impl TeacherKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CodeRabbit => "coderabbit",
            Self::GitHub => "github",
            Self::Human => "human",
            Self::Sarif => "sarif",
            Self::CiFixLoop => "ci_fix_loop",
            Self::Ferret => "ferret",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "coderabbit" => Some(Self::CodeRabbit),
            "github" => Some(Self::GitHub),
            "human" => Some(Self::Human),
            "sarif" => Some(Self::Sarif),
            "ci_fix_loop" => Some(Self::CiFixLoop),
            "ferret" => Some(Self::Ferret),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationRequest {
    pub snapshot_id: SnapshotId,
    pub draft_findings: Vec<PredictedFinding>,
    pub uncertainty_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationResult {
    pub teacher: TeacherKind,
    pub findings: Vec<ReviewFinding>,
}

#[async_trait]
pub trait Teacher: Send + Sync {
    fn kind(&self) -> TeacherKind;

    async fn ingest_snapshot(&self, snapshot: &ReviewSnapshot) -> Vec<ReviewFinding>;

    async fn escalate(
        &self,
        request: EscalationRequest,
    ) -> Result<EscalationResult, crate::FerretError> {
        let _ = request;
        Err(crate::FerretError::InvalidInput(format!(
            "escalation not implemented for {}",
            self.kind().as_str()
        )))
    }
}
