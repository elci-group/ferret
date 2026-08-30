//! SQLite persistence for the Ferret review corpus.

mod schema;

use ferret_core::{
    Category, Disposition, Outcome, RepoId, RepoProfile, ReviewFinding, ReviewSnapshot, Severity,
};
use serde::Serialize;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

pub struct Store {
    pool: SqlitePool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CorpusStats {
    pub repos: u64,
    pub snapshots: u64,
    pub hunks: u64,
    pub findings: u64,
    pub outcomes: u64,
}

impl Store {
    pub async fn open(path: &str) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::from_str(path)?.create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::query(schema::SCHEMA).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upsert_repo(&self, profile: &RepoProfile) -> anyhow::Result<RepoId> {
        let id = RepoId::from_slug(&profile.slug);
        sqlx::query(
            r#"
            INSERT INTO repos (id, slug, profile_json)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(slug) DO UPDATE SET profile_json = excluded.profile_json
            "#,
        )
        .bind(id.0.to_string())
        .bind(&profile.slug)
        .bind(serde_json::to_string(profile)?)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn save_snapshot(&self, snapshot: &ReviewSnapshot) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO snapshots
            (id, repo_id, pr_number, base_sha, head_sha, teachers_json, captured_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(snapshot.id.0.to_string())
        .bind(snapshot.repo_id.0.to_string())
        .bind(snapshot.pr_number.map(|n| n as i64))
        .bind(&snapshot.base_sha)
        .bind(&snapshot.head_sha)
        .bind(serde_json::to_string(&snapshot.teachers)?)
        .bind(snapshot.captured_at)
        .execute(&self.pool)
        .await?;

        for hunk in &snapshot.hunks {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO hunks
                (id, snapshot_id, file_path, old_start, old_lines, new_start, new_lines, content, language)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
            )
            .bind(&hunk.id)
            .bind(snapshot.id.0.to_string())
            .bind(&hunk.file_path)
            .bind(hunk.old_start as i64)
            .bind(hunk.old_lines as i64)
            .bind(hunk.new_start as i64)
            .bind(hunk.new_lines as i64)
            .bind(&hunk.content)
            .bind(&hunk.language)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn save_finding(&self, finding: &ReviewFinding) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO findings
            (id, snapshot_id, teacher, hunk_id, category, severity, message, suggestion, rule_id, captured_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(finding.id.0.to_string())
        .bind(finding.snapshot_id.0.to_string())
        .bind(finding.teacher.as_str())
        .bind(&finding.hunk_id)
        .bind(category_str(finding.category))
        .bind(severity_str(finding.severity))
        .bind(&finding.message)
        .bind(&finding.suggestion)
        .bind(&finding.rule_id)
        .bind(finding.captured_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn save_outcome(&self, outcome: &Outcome) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO outcomes
            (finding_id, disposition, fixed_in_sha, human_override, recorded_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(outcome.finding_id.0.to_string())
        .bind(disposition_str(outcome.disposition))
        .bind(&outcome.fixed_in_sha)
        .bind(outcome.human_override)
        .bind(outcome.recorded_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn corpus_stats(&self) -> anyhow::Result<CorpusStats> {
        let repos: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM repos")
            .fetch_one(&self.pool)
            .await?;
        let snapshots: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM snapshots")
            .fetch_one(&self.pool)
            .await?;
        let hunks: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM hunks")
            .fetch_one(&self.pool)
            .await?;
        let findings: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM findings")
            .fetch_one(&self.pool)
            .await?;
        let outcomes: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outcomes")
            .fetch_one(&self.pool)
            .await?;

        Ok(CorpusStats {
            repos: repos.0 as u64,
            snapshots: snapshots.0 as u64,
            hunks: hunks.0 as u64,
            findings: findings.0 as u64,
            outcomes: outcomes.0 as u64,
        })
    }

    pub async fn similar_hunk_messages(
        &self,
        file_path: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT f.message
            FROM findings f
            JOIN hunks h ON f.hunk_id = h.id
            WHERE h.file_path LIKE ?1
            LIMIT ?2
            "#,
        )
        .bind(format!("%{}%", file_path))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(m,)| m).collect())
    }

    pub async fn recent_findings(&self, limit: i64) -> anyhow::Result<Vec<ReviewFinding>> {
        let rows = sqlx::query_as::<_, FindingRow>(
            r#"
            SELECT id, snapshot_id, teacher, hunk_id, category, severity, message, suggestion, rule_id, captured_at
            FROM findings
            ORDER BY datetime(captured_at) DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            out.push(r.into_finding()?);
        }
        Ok(out)
    }

    pub async fn findings_for_snapshot(
        &self,
        snapshot_id: &str,
    ) -> anyhow::Result<Vec<ReviewFinding>> {
        let rows = sqlx::query_as::<_, FindingRow>(
            r#"
            SELECT id, snapshot_id, teacher, hunk_id, category, severity, message, suggestion, rule_id, captured_at
            FROM findings
            WHERE snapshot_id = ?1
            ORDER BY datetime(captured_at) DESC
            "#,
        )
        .bind(snapshot_id)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            out.push(r.into_finding()?);
        }
        Ok(out)
    }
}

fn category_str(category: Category) -> &'static str {
    match category {
        Category::Style => "style",
        Category::Safety => "safety",
        Category::Architecture => "architecture",
        Category::Performance => "performance",
        Category::Testing => "testing",
        Category::Documentation => "documentation",
        Category::Other => "other",
    }
}

fn severity_str(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Minor => "minor",
        Severity::Major => "major",
        Severity::Critical => "critical",
    }
}

fn disposition_str(disposition: Disposition) -> &'static str {
    match disposition {
        Disposition::Accepted => "accepted",
        Disposition::Rejected => "rejected",
        Disposition::Ignored => "ignored",
        Disposition::Fixed => "fixed",
        Disposition::Pending => "pending",
    }
}

#[derive(Debug, sqlx::FromRow)]
struct FindingRow {
    id: String,
    snapshot_id: String,
    teacher: String,
    hunk_id: Option<String>,
    category: String,
    severity: String,
    message: String,
    suggestion: Option<String>,
    rule_id: Option<String>,
    captured_at: chrono::DateTime<chrono::Utc>,
}

impl FindingRow {
    fn into_finding(self) -> anyhow::Result<ReviewFinding> {
        use std::str::FromStr as _;
        let category = match self.category.as_str() {
            "style" => Category::Style,
            "safety" => Category::Safety,
            "architecture" => Category::Architecture,
            "performance" => Category::Performance,
            "testing" => Category::Testing,
            "documentation" => Category::Documentation,
            _ => Category::Other,
        };
        let severity = match self.severity.as_str() {
            "info" => Severity::Info,
            "minor" => Severity::Minor,
            "major" => Severity::Major,
            "critical" => Severity::Critical,
            _ => Severity::Info,
        };
        let teacher = ferret_core::TeacherKind::parse(&self.teacher)
            .unwrap_or(ferret_core::TeacherKind::Ferret);
        let id = uuid::Uuid::from_str(&self.id)
            .map(ferret_core::FindingId)
            .unwrap_or_else(|_| ferret_core::FindingId::new());
        let snapshot_id = uuid::Uuid::from_str(&self.snapshot_id)
            .map(ferret_core::SnapshotId)
            .unwrap_or_else(|_| ferret_core::SnapshotId::new());
        Ok(ReviewFinding {
            id,
            snapshot_id,
            teacher,
            hunk_id: self.hunk_id,
            category,
            severity,
            message: self.message,
            suggestion: self.suggestion,
            rule_id: self.rule_id,
            captured_at: self.captured_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ferret_core::{DiffHunk, FindingId, SnapshotId, TeacherKind};

    #[tokio::test]
    async fn store_roundtrip() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        let profile = RepoProfile::new("elci-group/ferret");
        let repo_id = store.upsert_repo(&profile).await.unwrap();

        let snapshot = ReviewSnapshot {
            id: SnapshotId::new(),
            repo_id,
            pr_number: Some(1),
            base_sha: "abc".into(),
            head_sha: "def".into(),
            hunks: vec![DiffHunk {
                id: "hunk1".into(),
                file_path: "src/lib.rs".into(),
                old_start: 1,
                old_lines: 0,
                new_start: 1,
                new_lines: 3,
                content: "+fn main() {}".into(),
                language: Some("rust".into()),
            }],
            teachers: vec![TeacherKind::GitHub],
            captured_at: Utc::now(),
        };
        store.save_snapshot(&snapshot).await.unwrap();

        let finding = ReviewFinding {
            id: FindingId::new(),
            snapshot_id: snapshot.id,
            teacher: TeacherKind::GitHub,
            hunk_id: Some("hunk1".into()),
            category: Category::Style,
            severity: Severity::Minor,
            message: "prefer Result over panic".into(),
            suggestion: None,
            rule_id: None,
            captured_at: Utc::now(),
        };
        store.save_finding(&finding).await.unwrap();

        let stats = store.corpus_stats().await.unwrap();
        assert_eq!(stats.repos, 1);
        assert_eq!(stats.snapshots, 1);
        assert_eq!(stats.findings, 1);
    }
}
