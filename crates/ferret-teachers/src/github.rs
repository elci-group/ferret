use async_trait::async_trait;
use chrono::Utc;
use ferret_core::{
    Category, FindingId, ReviewFinding, ReviewSnapshot, Severity, Teacher, TeacherKind,
};
use serde::Deserialize;
use std::fmt;

#[derive(Debug)]
pub enum GitHubError {
    Parse(String),
}

impl fmt::Display for GitHubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(s) => write!(
                f,
                "parse error: {s}. Check the comment payload and try again."
            ),
        }
    }
}

impl std::error::Error for GitHubError {}

/// Parses GitHub review comment payloads into normalized findings.
pub struct GitHubTeacher;

#[derive(Debug, Deserialize)]
pub struct GitHubReviewComment {
    pub path: Option<String>,
    pub body: String,
    pub line: Option<u32>,
}

#[async_trait]
impl Teacher for GitHubTeacher {
    fn kind(&self) -> TeacherKind {
        TeacherKind::GitHub
    }

    async fn ingest_snapshot(&self, snapshot: &ReviewSnapshot) -> Vec<ReviewFinding> {
        snapshot
            .hunks
            .iter()
            .filter(|h| h.content.contains("panic!"))
            .map(|h| ReviewFinding {
                id: FindingId::new(),
                snapshot_id: snapshot.id,
                teacher: TeacherKind::GitHub,
                hunk_id: Some(h.id.clone()),
                category: Category::Safety,
                severity: Severity::Major,
                message: "avoid panic in production paths".into(),
                suggestion: Some("return Result instead".into()),
                rule_id: None,
                captured_at: Utc::now(),
            })
            .collect()
    }
}

impl GitHubTeacher {
    pub fn from_comment(snapshot: &ReviewSnapshot, comment: &GitHubReviewComment) -> ReviewFinding {
        let hunk_id = snapshot
            .hunks
            .iter()
            .find(|h| comment.path.as_deref() == Some(h.file_path.as_str()))
            .map(|h| h.id.clone());

        ReviewFinding {
            id: FindingId::new(),
            snapshot_id: snapshot.id,
            teacher: TeacherKind::GitHub,
            hunk_id,
            category: Category::Other,
            severity: Severity::Minor,
            message: comment.body.clone(),
            suggestion: None,
            rule_id: None,
            captured_at: Utc::now(),
        }
    }
}
