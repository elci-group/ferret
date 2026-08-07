use async_trait::async_trait;
use chrono::Utc;
use ferret_core::{
    Category, FindingId, ReviewFinding, ReviewSnapshot, Severity, Teacher, TeacherKind,
};
use serde::Deserialize;

/// Parses SARIF static analyzer output into normalized findings.
pub struct SarifTeacher;

#[derive(Debug, Deserialize)]
pub struct SarifLog {
    pub runs: Vec<SarifRun>,
}

#[derive(Debug, Deserialize)]
pub struct SarifRun {
    pub results: Option<Vec<SarifResult>>,
}

#[derive(Debug, Deserialize)]
pub struct SarifResult {
    pub rule_id: Option<String>,
    pub message: SarifMessage,
    pub level: Option<String>,
    pub locations: Option<Vec<SarifLocation>>,
}

#[derive(Debug, Deserialize)]
pub struct SarifMessage {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct SarifLocation {
    pub physical_location: Option<SarifPhysicalLocation>,
}

#[derive(Debug, Deserialize)]
pub struct SarifPhysicalLocation {
    pub artifact_location: Option<SarifArtifactLocation>,
}

#[derive(Debug, Deserialize)]
pub struct SarifArtifactLocation {
    pub uri: Option<String>,
}

#[async_trait]
impl Teacher for SarifTeacher {
    fn kind(&self) -> TeacherKind {
        TeacherKind::Sarif
    }

    async fn ingest_snapshot(&self, _snapshot: &ReviewSnapshot) -> Vec<ReviewFinding> {
        Vec::new()
    }
}

impl SarifTeacher {
    pub fn from_sarif(snapshot: &ReviewSnapshot, log: &SarifLog) -> Vec<ReviewFinding> {
        let mut findings = Vec::new();
        for run in &log.runs {
            if let Some(results) = &run.results {
                for result in results {
                    let file = result
                        .locations
                        .as_ref()
                        .and_then(|l| l.first())
                        .and_then(|l| l.physical_location.as_ref())
                        .and_then(|p| p.artifact_location.as_ref())
                        .and_then(|a| a.uri.clone());

                    let hunk_id = file.as_ref().and_then(|f| {
                        snapshot
                            .hunks
                            .iter()
                            .find(|h| h.file_path.ends_with(f))
                            .map(|h| h.id.clone())
                    });

                    findings.push(ReviewFinding {
                        id: FindingId::new(),
                        snapshot_id: snapshot.id,
                        teacher: TeacherKind::Sarif,
                        hunk_id,
                        category: Category::Safety,
                        severity: map_level(result.level.as_deref()),
                        message: result.message.text.clone(),
                        suggestion: None,
                        rule_id: result.rule_id.clone(),
                        captured_at: Utc::now(),
                    });
                }
            }
        }
        findings
    }
}

fn map_level(level: Option<&str>) -> Severity {
    match level.unwrap_or("warning") {
        "error" => Severity::Major,
        "note" => Severity::Info,
        _ => Severity::Minor,
    }
}
