pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS repos (
    id TEXT PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    profile_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS snapshots (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL REFERENCES repos(id),
    pr_number INTEGER,
    base_sha TEXT NOT NULL,
    head_sha TEXT NOT NULL,
    teachers_json TEXT NOT NULL DEFAULT '[]',
    captured_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS hunks (
    id TEXT PRIMARY KEY,
    snapshot_id TEXT NOT NULL REFERENCES snapshots(id),
    file_path TEXT NOT NULL,
    old_start INTEGER NOT NULL,
    old_lines INTEGER NOT NULL,
    new_start INTEGER NOT NULL,
    new_lines INTEGER NOT NULL,
    content TEXT NOT NULL,
    language TEXT
);

CREATE TABLE IF NOT EXISTS findings (
    id TEXT PRIMARY KEY,
    snapshot_id TEXT NOT NULL REFERENCES snapshots(id),
    teacher TEXT NOT NULL,
    hunk_id TEXT,
    category TEXT NOT NULL,
    severity TEXT NOT NULL,
    message TEXT NOT NULL,
    suggestion TEXT,
    rule_id TEXT,
    captured_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS outcomes (
    finding_id TEXT PRIMARY KEY REFERENCES findings(id),
    disposition TEXT NOT NULL,
    fixed_in_sha TEXT,
    human_override INTEGER NOT NULL DEFAULT 0,
    recorded_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_hunks_file_path ON hunks(file_path);
CREATE INDEX IF NOT EXISTS idx_findings_snapshot ON findings(snapshot_id);
CREATE INDEX IF NOT EXISTS idx_findings_hunk ON findings(hunk_id);
"#;
