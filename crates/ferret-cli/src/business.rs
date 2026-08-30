use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const DEFAULT_EXCLUSIONS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "vendor",
    ".cache",
    "tmp",
    "temp",
    "build",
    "dist",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Rust,
    Node,
    Python,
    Go,
    Php,
    Ruby,
    Elixir,
    Java,
    DotNet,
}

impl ProjectKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "rust" => Some(Self::Rust),
            "node" | "javascript" | "typescript" => Some(Self::Node),
            "python" => Some(Self::Python),
            "go" => Some(Self::Go),
            "php" => Some(Self::Php),
            "ruby" => Some(Self::Ruby),
            "elixir" => Some(Self::Elixir),
            "java" => Some(Self::Java),
            "dotnet" | ".net" | "csharp" => Some(Self::DotNet),
            _ => None,
        }
    }
}

impl fmt::Display for ProjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectIdentity {
    pub canonical_path: PathBuf,
    pub project_kind: ProjectKind,
    pub project_name: String,
    pub project_manifest: PathBuf,
    pub repository_root: Option<PathBuf>,
    pub git: Option<GitMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitMetadata {
    pub branch: Option<String>,
    pub dirty: bool,
    pub remote: Option<String>,
    pub last_commit: Option<String>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    pub max_depth: usize,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub project_kind: Option<ProjectKind>,
    pub follow_symlinks: bool,
    pub git: bool,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            max_depth: 12,
            include: Vec::new(),
            exclude: DEFAULT_EXCLUSIONS.iter().map(|s| (*s).to_owned()).collect(),
            project_kind: None,
            follow_symlinks: false,
            git: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingSummary {
    pub category: String,
    pub severity: String,
    pub message: String,
    pub rule_id: Option<String>,
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectAnalysis {
    pub files_scanned: usize,
    pub findings: Vec<FindingSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Passed,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectResult {
    #[serde(flatten)]
    pub project: ProjectIdentity,
    pub status: ProjectStatus,
    pub analysis: Option<ProjectAnalysis>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessFinding {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub projects: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessSummary {
    pub discovered: usize,
    pub scanned: usize,
    pub successful: usize,
    pub warnings: usize,
    pub errors: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessReport {
    pub schema: &'static str,
    pub root: PathBuf,
    pub projects: Vec<ProjectResult>,
    pub business_findings: Vec<BusinessFinding>,
    pub summary: BusinessSummary,
}

impl BusinessReport {
    pub fn exit_code(&self) -> u8 {
        if self.summary.errors > 0 {
            2
        } else if self.summary.warnings > 0 || !self.business_findings.is_empty() {
            1
        } else {
            0
        }
    }
}

pub fn project_at(target: &Path, git: bool) -> Result<ProjectIdentity> {
    let canonical = target
        .canonicalize()
        .with_context(|| format!("target does not exist: {}", target.display()))?;
    if !canonical.is_dir() {
        bail!("target is not a directory: {}", canonical.display());
    }
    let Some((kind, manifest)) = marker_at(&canonical) else {
        bail!(
            "target_not_project\ntarget: {}\nreason: no recognised project boundary\nhint: use `ferret business` for multi-project discovery",
            canonical.display()
        );
    };
    Ok(identity(canonical, kind, manifest, git))
}

pub fn discover(root: &Path, options: &DiscoveryOptions) -> Result<Vec<ProjectIdentity>> {
    let root = root
        .canonicalize()
        .with_context(|| format!("discovery root does not exist: {}", root.display()))?;
    if !root.is_dir() {
        bail!("discovery root is not a directory: {}", root.display());
    }

    let mut candidates = Vec::new();
    let mut stack = vec![(root.clone(), 0usize)];
    let mut visited = HashSet::new();
    visited.insert(root.clone());

    while let Some((directory, depth)) = stack.pop() {
        if let Some((kind, manifest)) = marker_at(&directory) {
            let relative = directory.strip_prefix(&root).unwrap_or(&directory);
            if options.project_kind.is_none_or(|wanted| wanted == kind)
                && matches_filters(relative, &options.include)
            {
                candidates.push((directory.clone(), kind, manifest));
            }
        }

        if depth >= options.max_depth {
            continue;
        }
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        let mut children = entries.flatten().collect::<Vec<_>>();
        children.sort_by_key(|entry| entry.file_name());
        for entry in children.into_iter().rev() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let relative = path.strip_prefix(&root).unwrap_or(&path);
            let relative = relative.to_string_lossy().replace('\\', "/");
            if options.exclude.iter().any(|excluded| {
                pattern_matches(excluded, &name) || pattern_matches(excluded, &relative)
            }) {
                continue;
            }
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            let next = if file_type.is_dir() {
                path
            } else if file_type.is_symlink() && options.follow_symlinks {
                let Ok(canonical) = path.canonicalize() else {
                    continue;
                };
                if !canonical.starts_with(&root) || !canonical.is_dir() {
                    continue;
                }
                canonical
            } else {
                continue;
            };
            let canonical = next.canonicalize().unwrap_or(next);
            if visited.insert(canonical.clone()) {
                stack.push((canonical, depth + 1));
            }
        }
    }

    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    let workspace_members = rust_workspace_members(&candidates);
    let mut projects = candidates
        .into_iter()
        .filter(|(path, kind, _)| *kind != ProjectKind::Rust || !workspace_members.contains(path))
        .map(|(path, kind, manifest)| identity(path, kind, manifest, options.git))
        .collect::<Vec<_>>();
    projects.sort_by(|left, right| left.canonical_path.cmp(&right.canonical_path));
    Ok(projects)
}

pub fn scan_business<F>(
    root: &Path,
    options: &DiscoveryOptions,
    parallel: usize,
    scan: F,
) -> Result<BusinessReport>
where
    F: Fn(&ProjectIdentity) -> Result<ProjectAnalysis> + Send + Sync + 'static,
{
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("discovery root does not exist: {}", root.display()))?;
    let projects = discover(&canonical_root, options)?;
    let discovered = projects.len();
    let scan = Arc::new(scan);
    let queue = Arc::new(Mutex::new(
        projects.into_iter().enumerate().collect::<Vec<_>>(),
    ));
    let output = Arc::new(Mutex::new(Vec::with_capacity(discovered)));
    let workers = parallel.max(1).min(discovered.max(1));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let scan = Arc::clone(&scan);
            let queue = Arc::clone(&queue);
            let output = Arc::clone(&output);
            scope.spawn(move || loop {
                let next = queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop();
                let Some((index, project)) = next else { break };
                let scan_result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scan(&project)));
                let result = match scan_result {
                    Ok(Ok(analysis)) => ProjectResult {
                        status: if analysis.findings.is_empty() {
                            ProjectStatus::Passed
                        } else {
                            ProjectStatus::Warning
                        },
                        project,
                        analysis: Some(analysis),
                        error: None,
                    },
                    Ok(Err(error)) => ProjectResult {
                        project,
                        status: ProjectStatus::Error,
                        analysis: None,
                        error: Some(format!("{error:#}")),
                    },
                    Err(_) => ProjectResult {
                        project,
                        status: ProjectStatus::Error,
                        analysis: None,
                        error: Some("project scanner panicked".to_owned()),
                    },
                };
                output
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push((index, result));
            });
        }
    });

    let output = Arc::try_unwrap(output)
        .map_err(|_| anyhow::anyhow!("business workers retained the result collection"))?;
    let mut indexed = output
        .into_inner()
        .map_err(|_| anyhow::anyhow!("business result collection was poisoned"))?;
    indexed.sort_by_key(|(index, _)| *index);
    let projects = indexed
        .into_iter()
        .map(|(_, result)| result)
        .collect::<Vec<_>>();
    let business_findings = aggregate_findings(&projects);
    let summary = BusinessSummary {
        discovered,
        scanned: projects.len(),
        successful: projects
            .iter()
            .filter(|p| p.status == ProjectStatus::Passed)
            .count(),
        warnings: projects
            .iter()
            .filter(|p| p.status == ProjectStatus::Warning)
            .count(),
        errors: projects
            .iter()
            .filter(|p| p.status == ProjectStatus::Error)
            .count(),
    };
    Ok(BusinessReport {
        schema: "ferret.business/v1",
        root: canonical_root,
        projects,
        business_findings,
        summary,
    })
}

fn marker_at(path: &Path) -> Option<(ProjectKind, PathBuf)> {
    const MARKERS: &[(&str, ProjectKind)] = &[
        ("Cargo.toml", ProjectKind::Rust),
        ("package.json", ProjectKind::Node),
        ("pyproject.toml", ProjectKind::Python),
        ("go.mod", ProjectKind::Go),
        ("composer.json", ProjectKind::Php),
        ("Gemfile", ProjectKind::Ruby),
        ("mix.exs", ProjectKind::Elixir),
        ("pom.xml", ProjectKind::Java),
        ("build.gradle", ProjectKind::Java),
        ("build.gradle.kts", ProjectKind::Java),
    ];
    for (marker, kind) in MARKERS {
        let manifest = path.join(marker);
        if manifest.is_file() {
            return Some((*kind, manifest));
        }
    }
    let mut dotnet = std::fs::read_dir(path)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate.is_file()
                && candidate
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| matches!(ext, "sln" | "csproj"))
        })
        .collect::<Vec<_>>();
    dotnet.sort();
    dotnet
        .into_iter()
        .next()
        .map(|manifest| (ProjectKind::DotNet, manifest))
}

fn identity(
    path: PathBuf,
    kind: ProjectKind,
    manifest: PathBuf,
    with_git: bool,
) -> ProjectIdentity {
    let project_name = manifest_project_name(&manifest, kind).unwrap_or_else(|| {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_owned()
    });
    let (repository_root, git) = if with_git {
        git_metadata(&path).unwrap_or((None, None))
    } else {
        (None, None)
    };
    ProjectIdentity {
        canonical_path: path,
        project_kind: kind,
        project_name,
        project_manifest: manifest,
        repository_root,
        git,
    }
}

fn manifest_project_name(manifest: &Path, kind: ProjectKind) -> Option<String> {
    let text = std::fs::read_to_string(manifest).ok()?;
    match kind {
        ProjectKind::Node => serde_json::from_str::<serde_json::Value>(&text)
            .ok()?
            .get("name")?
            .as_str()
            .map(ToOwned::to_owned),
        ProjectKind::Rust => {
            let mut in_package = false;
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with('[') {
                    in_package = line == "[package]";
                } else if in_package && line.starts_with("name") {
                    return quoted_value(line);
                }
            }
            None
        }
        _ => None,
    }
}

fn git_metadata(path: &Path) -> Option<(Option<PathBuf>, Option<GitMetadata>)> {
    let repo = git2::Repository::discover(path).ok()?;
    let root = repo.workdir().map(Path::to_path_buf);
    let branch = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(ToOwned::to_owned));
    let last_commit = repo
        .head()
        .ok()
        .and_then(|head| head.target())
        .map(|id| id.to_string());
    let dirty = repo
        .statuses(None)
        .map(|statuses| !statuses.is_empty())
        .unwrap_or(false);
    let remote = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(ToOwned::to_owned));
    Some((
        root,
        Some(GitMetadata {
            branch,
            dirty,
            remote,
            last_commit,
            ahead: None,
            behind: None,
        }),
    ))
}

fn rust_workspace_members(candidates: &[(PathBuf, ProjectKind, PathBuf)]) -> BTreeSet<PathBuf> {
    let rust_paths = candidates
        .iter()
        .filter(|(_, kind, _)| *kind == ProjectKind::Rust)
        .map(|(path, _, _)| path.clone())
        .collect::<Vec<_>>();
    let mut members = BTreeSet::new();
    for (workspace, kind, manifest) in candidates {
        if *kind != ProjectKind::Rust {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(manifest) else {
            continue;
        };
        let patterns = cargo_workspace_patterns(&text, "members");
        if patterns.is_empty() {
            continue;
        }
        let excludes = cargo_workspace_patterns(&text, "exclude");
        for candidate in &rust_paths {
            if candidate == workspace || !candidate.starts_with(workspace) {
                continue;
            }
            let relative = candidate.strip_prefix(workspace).unwrap_or(candidate);
            let relative = relative.to_string_lossy().replace('\\', "/");
            if patterns
                .iter()
                .any(|pattern| pattern_matches(pattern, &relative))
                && !excludes
                    .iter()
                    .any(|pattern| pattern_matches(pattern, &relative))
            {
                members.insert(candidate.clone());
            }
        }
    }
    members
}

fn cargo_workspace_patterns(text: &str, key: &str) -> Vec<String> {
    let mut in_workspace = false;
    let mut collecting = false;
    let mut value = String::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && !collecting {
            in_workspace = line == "[workspace]";
            continue;
        }
        if in_workspace && !collecting && line.starts_with(key) {
            if let Some((_, rhs)) = line.split_once('=') {
                value.push_str(rhs);
                collecting = !rhs.contains(']');
            }
        } else if collecting {
            value.push_str(line);
            collecting = !line.contains(']');
        }
        if !collecting && !value.is_empty() {
            break;
        }
    }
    let values = value
        .split_once('[')
        .map(|(_, values)| values)
        .unwrap_or(&value);
    let values = values
        .rsplit_once(']')
        .map(|(values, _)| values)
        .unwrap_or(values);
    values
        .split(',')
        .filter_map(|part| quoted_value(part.trim()))
        .collect()
}

fn quoted_value(text: &str) -> Option<String> {
    let (_, value) = text.split_once('=').unwrap_or(("", text));
    let value = value.trim();
    let quote = value.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let end = value[1..].find(quote)? + 1;
    Some(value[1..end].to_owned())
}

fn pattern_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.trim_matches('/');
    let value = value.trim_matches('/');
    if !pattern.contains('*') {
        return value == pattern || value.contains(pattern);
    }
    let parts = pattern.split('*').collect::<Vec<_>>();
    let mut remainder = value;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(position) = remainder.find(part) else {
            return false;
        };
        if index == 0 && !pattern.starts_with('*') && position != 0 {
            return false;
        }
        remainder = &remainder[position + part.len()..];
    }
    pattern.ends_with('*') || remainder.is_empty()
}

fn matches_filters(relative: &Path, includes: &[String]) -> bool {
    if includes.is_empty() {
        return true;
    }
    let relative = relative.to_string_lossy().replace('\\', "/");
    includes
        .iter()
        .any(|pattern| pattern_matches(pattern, &relative))
}

fn aggregate_findings(projects: &[ProjectResult]) -> Vec<BusinessFinding> {
    let mut findings = Vec::new();
    let mut names: BTreeMap<&str, Vec<PathBuf>> = BTreeMap::new();
    let mut remotes: BTreeMap<&str, Vec<PathBuf>> = BTreeMap::new();
    let mut dirty = Vec::new();
    for result in projects {
        names
            .entry(&result.project.project_name)
            .or_default()
            .push(result.project.canonical_path.clone());
        if let Some(git) = &result.project.git {
            if git.dirty {
                dirty.push(result.project.canonical_path.clone());
            }
            if let Some(remote) = git.remote.as_deref() {
                remotes
                    .entry(remote)
                    .or_default()
                    .push(result.project.canonical_path.clone());
            }
        }
    }
    for (name, paths) in names.into_iter().filter(|(_, paths)| paths.len() > 1) {
        findings.push(BusinessFinding {
            severity: "warn".to_owned(),
            code: "duplicate_project_name".to_owned(),
            message: format!("{} projects share the name {name}", paths.len()),
            projects: paths,
        });
    }
    for (_, paths) in remotes.into_iter().filter(|(_, paths)| paths.len() > 1) {
        findings.push(BusinessFinding {
            severity: "warn".to_owned(),
            code: "duplicate_repository_remote".to_owned(),
            message: format!("{} projects share the same repository remote", paths.len()),
            projects: paths,
        });
    }
    if !dirty.is_empty() {
        findings.push(BusinessFinding {
            severity: "info".to_owned(),
            code: "uncommitted_changes".to_owned(),
            message: format!("{} projects have uncommitted changes", dirty.len()),
            projects: dirty,
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempTree(PathBuf);
    impl TempTree {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("ferret-business-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn project(&self, relative: &str, manifest: &str, content: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join(manifest), content).unwrap();
            path
        }
    }
    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn discovers_multiple_projects_and_ignores_non_projects() {
        let tree = TempTree::new();
        tree.project("rusty", "Cargo.toml", "[package]\nname = \"rusty\"");
        tree.project("web", "package.json", r#"{"name":"web"}"#);
        fs::create_dir_all(tree.0.join("notes")).unwrap();
        let projects = discover(&tree.0, &DiscoveryOptions::default()).unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].project_name, "rusty");
        assert_eq!(projects[1].project_name, "web");
    }

    #[test]
    fn direct_target_requires_a_project_boundary() {
        let tree = TempTree::new();
        tree.project("child", "go.mod", "module child");
        let error = project_at(&tree.0, false).unwrap_err().to_string();
        assert!(error.contains("target_not_project"));
        assert!(project_at(&tree.0.join("child"), false).is_ok());
    }

    #[test]
    fn cargo_workspace_members_are_not_duplicate_projects() {
        let tree = TempTree::new();
        tree.project(
            "workspace",
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]",
        );
        tree.project(
            "workspace/crates/a",
            "Cargo.toml",
            "[package]\nname = \"a\"",
        );
        tree.project(
            "workspace/crates/b",
            "Cargo.toml",
            "[package]\nname = \"b\"",
        );
        tree.project(
            "workspace/tools/standalone",
            "Cargo.toml",
            "[package]\nname = \"standalone\"",
        );
        let projects = discover(&tree.0, &DiscoveryOptions::default()).unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects
            .iter()
            .any(|p| p.canonical_path.ends_with("workspace")));
        assert!(projects.iter().any(|p| p.project_name == "standalone"));
    }

    #[test]
    fn cargo_workspace_excludes_remain_independent_projects() {
        let tree = TempTree::new();
        tree.project(
            "workspace",
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/tool\"]",
        );
        tree.project(
            "workspace/crates/member",
            "Cargo.toml",
            "[package]\nname = \"member\"",
        );
        tree.project(
            "workspace/crates/tool",
            "Cargo.toml",
            "[package]\nname = \"tool\"",
        );
        let projects = discover(&tree.0, &DiscoveryOptions::default()).unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects
            .iter()
            .any(|project| project.project_name == "tool"));
    }

    #[test]
    fn exclusions_depth_and_kind_are_deterministic() {
        let tree = TempTree::new();
        tree.project(
            "target/hidden",
            "Cargo.toml",
            "[package]\nname = \"hidden\"",
        );
        tree.project("a/b/deep", "go.mod", "module deep");
        tree.project("node", "package.json", r#"{"name":"node"}"#);
        let shallow = DiscoveryOptions {
            max_depth: 1,
            ..Default::default()
        };
        assert_eq!(discover(&tree.0, &shallow).unwrap().len(), 1);
        let rust_only = DiscoveryOptions {
            project_kind: Some(ProjectKind::Rust),
            ..Default::default()
        };
        assert!(discover(&tree.0, &rust_only).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_by_default_and_cannot_escape_root() {
        use std::os::unix::fs::symlink;
        let tree = TempTree::new();
        let outside = TempTree::new();
        outside.project("project", "go.mod", "module outside");
        symlink(outside.0.join("project"), tree.0.join("linked")).unwrap();
        symlink(tree.0.join("missing"), tree.0.join("broken")).unwrap();
        assert!(discover(&tree.0, &DiscoveryOptions::default())
            .unwrap()
            .is_empty());
        let followed = DiscoveryOptions {
            follow_symlinks: true,
            ..Default::default()
        };
        assert!(discover(&tree.0, &followed).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn inaccessible_directories_do_not_hide_sibling_projects() {
        use std::os::unix::fs::PermissionsExt;
        let tree = TempTree::new();
        let blocked = tree.project("blocked/project", "go.mod", "module blocked");
        let blocked_parent = blocked.parent().unwrap();
        std::fs::set_permissions(blocked_parent, std::fs::Permissions::from_mode(0o000)).unwrap();
        tree.project("visible", "go.mod", "module visible");

        let projects = discover(&tree.0, &DiscoveryOptions::default()).unwrap();
        std::fs::set_permissions(blocked_parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(projects
            .iter()
            .any(|project| project.canonical_path.ends_with("visible")));
    }

    #[test]
    fn scan_failures_are_isolated_and_results_remain_sorted() {
        let tree = TempTree::new();
        tree.project("c", "go.mod", "module c");
        tree.project("a", "go.mod", "module a");
        tree.project("b", "go.mod", "module b");
        tree.project("d", "go.mod", "module d");
        let calls = Arc::new(AtomicUsize::new(0));
        let scan_calls = Arc::clone(&calls);
        let report = scan_business(&tree.0, &DiscoveryOptions::default(), 3, move |project| {
            scan_calls.fetch_add(1, Ordering::SeqCst);
            if matches!(project.project_name.as_str(), "b" | "c") {
                bail!("broken project")
            }
            Ok(ProjectAnalysis {
                files_scanned: 1,
                findings: Vec::new(),
            })
        })
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(report.summary.errors, 2);
        let names = report
            .projects
            .iter()
            .map(|p| p.project.project_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["a", "b", "c", "d"]);
        assert_eq!(report.exit_code(), 2);
    }

    #[test]
    fn scanner_panics_are_isolated() {
        let tree = TempTree::new();
        tree.project("a", "go.mod", "module a");
        tree.project("b", "go.mod", "module b");
        let report = scan_business(&tree.0, &DiscoveryOptions::default(), 2, |project| {
            if project.project_name == "a" {
                panic!("malformed input triggered scanner bug");
            }
            Ok(ProjectAnalysis {
                files_scanned: 1,
                findings: Vec::new(),
            })
        })
        .unwrap();
        assert_eq!(report.summary.errors, 1);
        assert_eq!(report.summary.successful, 1);
    }

    #[test]
    fn no_projects_is_a_successful_report() {
        let tree = TempTree::new();
        let report =
            scan_business(&tree.0, &DiscoveryOptions::default(), 4, |_| unreachable!()).unwrap();
        assert_eq!(report.summary.discovered, 0);
        assert_eq!(report.exit_code(), 0);
    }
}
