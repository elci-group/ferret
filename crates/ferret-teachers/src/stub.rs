use async_trait::async_trait;
use ferret_core::{ReviewFinding, ReviewSnapshot, Teacher, TeacherKind};

/// Stub adapter for CodeRabbit webhook ingestion.
pub struct CodeRabbitTeacher;

#[async_trait]
impl Teacher for CodeRabbitTeacher {
    fn kind(&self) -> TeacherKind {
        TeacherKind::CodeRabbit
    }

    async fn ingest_snapshot(&self, _snapshot: &ReviewSnapshot) -> Vec<ReviewFinding> {
        Vec::new()
    }
}

/// Stub adapter for CI fix-loop signals.
pub struct CiFixLoopTeacher;

#[async_trait]
impl Teacher for CiFixLoopTeacher {
    fn kind(&self) -> TeacherKind {
        TeacherKind::CiFixLoop
    }

    async fn ingest_snapshot(&self, _snapshot: &ReviewSnapshot) -> Vec<ReviewFinding> {
        Vec::new()
    }
}
