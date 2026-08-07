#!/usr/bin/env bash
set -euo pipefail

DB="${1:-ferret.demo.db}"
rm -f "$DB"

sqlite3 "$DB" <<'SQL'
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

INSERT INTO repos (id, slug, profile_json) VALUES ('bc5fa277-ca39-700b-eafc-73e0cb57066c', 'elci-group/ferret', '{}');
INSERT INTO snapshots (id, repo_id, pr_number, base_sha, head_sha, teachers_json, captured_at) VALUES ('c3386ad6-ebd9-412b-b315-57cfd3ac3281', 'bc5fa277-ca39-700b-eafc-73e0cb57066c', 1, 'abc123', 'def456', '["github"]', '2026-08-07T20:00:00Z');
INSERT INTO hunks (id, snapshot_id, file_path, old_start, old_lines, new_start, new_lines, content, language) VALUES ('3afe03a9d321dbe4', 'c3386ad6-ebd9-412b-b315-57cfd3ac3281', 'src/lib.rs', 1, 3, 1, 4, '+fn new() {}', 'rs');
INSERT INTO findings (id, snapshot_id, teacher, hunk_id, category, severity, message, suggestion, rule_id, captured_at) VALUES
('f001', 'c3386ad6-ebd9-412b-b315-57cfd3ac3281', 'github', '3afe03a9d321dbe4', 'safety', 'major', 'avoid panic in production paths', 'return Result instead', NULL, '2026-08-07T20:00:00Z'),
('f002', 'c3386ad6-ebd9-412b-b315-57cfd3ac3281', 'coderabbit', '3afe03a9d321dbe4', 'style', 'minor', 'prefer explicit error types over unwrap', NULL, NULL, '2026-08-07T20:01:00Z'),
('f003', 'c3386ad6-ebd9-412b-b315-57cfd3ac3281', 'sarif', '3afe03a9d321dbe4', 'safety', 'major', 'potential panic via unwrap on Option', NULL, NULL, '2026-08-07T20:02:00Z');
SQL
