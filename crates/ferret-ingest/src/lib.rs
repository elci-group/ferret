//! Git diff extraction and hunk normalization.

use anyhow::{Context, Result};
use ferret_core::DiffHunk;
use git2::Repository;
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};
use std::path::{Path, PathBuf};

pub fn extract_hunks(repo_path: &Path, base_sha: &str, head_sha: &str) -> Result<Vec<DiffHunk>> {
    let repo = Repository::open(repo_path).context("open git repository")?;
    let base = repo.revparse_single(base_sha)?.peel_to_tree()?;
    let head = repo.revparse_single(head_sha)?.peel_to_tree()?;
    let diff = repo.diff_tree_to_tree(Some(&base), Some(&head), None)?;

    let mut hunks = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if delta.status() == git2::Delta::Deleted {
                return true;
            }
            true
        },
        None,
        Some(&mut |delta, hunk| {
            if let Some(path) = delta.new_file().path() {
                let file_path = path.to_string_lossy().into_owned();
                let content = format_hunk(repo_path, &file_path, base_sha, head_sha);
                let id = stable_hunk_id(&file_path, &content);
                hunks.push(DiffHunk {
                    id,
                    file_path: file_path.clone(),
                    old_start: hunk.old_start() as u32,
                    old_lines: hunk.old_lines() as u32,
                    new_start: hunk.new_start() as u32,
                    new_lines: hunk.new_lines() as u32,
                    content,
                    language: infer_language(&file_path),
                });
            }
            true
        }),
        None,
    )?;
    Ok(hunks)
}

pub fn hunks_from_unified_diff(diff_text: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current_file = String::new();

    for line in diff_text.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            current_file = path.to_string();
        } else if line.starts_with("@@") {
            let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(line);
            hunks.push(DiffHunk {
                id: stable_hunk_id(&current_file, line),
                file_path: current_file.clone(),
                old_start,
                old_lines,
                new_start,
                new_lines,
                content: line.to_string(),
                language: infer_language(&current_file),
            });
        }
    }

    hunks
}

fn format_hunk(repo_path: &Path, file_path: &str, base_sha: &str, head_sha: &str) -> String {
    let base = read_blob_at(repo_path, base_sha, file_path).unwrap_or_default();
    let head = read_blob_at(repo_path, head_sha, file_path).unwrap_or_default();
    let diff = TextDiff::from_lines(&base, &head);
    let mut out = String::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        for line in change.value().lines() {
            out.push_str(sign);
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn read_blob_at(repo_path: &Path, sha: &str, file_path: &str) -> Result<String> {
    let repo = Repository::open(repo_path)?;
    let object = repo.revparse_single(sha)?;
    let tree = object.peel_to_tree()?;
    let entry = tree.get_path(Path::new(file_path))?;
    let blob = repo.find_blob(entry.id())?;
    Ok(String::from_utf8_lossy(blob.content()).into_owned())
}

fn parse_hunk_header(line: &str) -> (u32, u32, u32, u32) {
    // @@ -old_start,old_lines +new_start,new_lines @@
    let parts: Vec<&str> = line.split_whitespace().collect();
    let old = parts.get(1).unwrap_or(&"+0,0").trim_start_matches('-');
    let new = parts.get(2).unwrap_or(&"+0,0").trim_start_matches('+');
    let (old_start, old_lines) = parse_range(old);
    let (new_start, new_lines) = parse_range(new);
    (old_start, old_lines, new_start, new_lines)
}

fn parse_range(part: &str) -> (u32, u32) {
    let mut split = part.split(',');
    let start = split.next().unwrap_or("0").parse().unwrap_or(0);
    let lines = split.next().unwrap_or("1").parse().unwrap_or(1);
    (start, lines)
}

pub fn stable_hunk_id(file_path: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file_path.as_bytes());
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_string()
}

fn infer_language(file_path: &str) -> Option<String> {
    PathBuf::from(file_path)
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_id_is_deterministic() {
        let a = stable_hunk_id("src/lib.rs", "+fn main() {}");
        let b = stable_hunk_id("src/lib.rs", "+fn main() {}");
        assert_eq!(a, b);
    }

    #[test]
    fn parses_unified_diff() {
        let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 fn old() {}
+fn new() {}
";
        let hunks = hunks_from_unified_diff(diff);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].file_path, "src/lib.rs");
    }
}
