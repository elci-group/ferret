use anyhow::{Context, Result};
mod business;
mod chase;
mod progress_compat;
mod style_compat;

// Route every human-facing Ferret result line through the shared 3form chase.
// The sink is a plain println in machine/JSON/non-TTY modes.
macro_rules! println {
    () => {
        crate::chase::print_line("")
    };
    ($($arg:tt)*) => {
        crate::chase::print_line(&format!($($arg)*))
    };
}

use chrono::Utc;
use clap::{Parser, Subcommand};
use crate::style_compat::{style, Style};
use ferret_core::{PredictedFinding, RepoProfile, ReviewSnapshot, SnapshotId, TeacherKind};
use ferret_infer::RetrievalPredictor;
use ferret_ingest::{extract_hunks, hunks_from_unified_diff, stable_hunk_id};
use ferret_route::{Route, Router, RouterConfig};
use ferret_store::Store;
use crate::progress_compat::{hunk_progress, spinner};
use std::path::{Path, PathBuf};
use tracing::info;

use business::{
    BusinessReport, DiscoveryOptions, FindingSummary, ProjectAnalysis, ProjectIdentity,
    ProjectKind, ProjectStatus,
};

const LOGO: &str = include_str!("../../../.brandi/assets/logo.ansi");
const TAGLINE: &str = "Review memory that learns.";

#[derive(Parser)]
#[command(
    name = "ferret",
    about = "Deterministic project scanner",
    long_about = "ferret requires a project target; ferret business discovers projects within an environment and scans them independently."
)]
struct Cli {
    #[arg(short, long, default_value = "ferret.db")]
    database: PathBuf,

    /// Scan one coherent project (defaults to the current directory).
    #[arg(value_name = "TARGET")]
    target: Option<PathBuf>,

    /// Emit machine-readable JSON for project and business scans.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Discover and independently scan coherent projects beneath ROOT.
    Business {
        /// Discovery boundary (defaults to $HOME).
        #[arg(value_name = "ROOT")]
        root: Option<PathBuf>,
        /// Maximum directory depth beneath ROOT.
        #[arg(long, default_value_t = 12)]
        max_depth: usize,
        /// Include only matching relative project paths (`*` is supported).
        #[arg(long)]
        include: Vec<String>,
        /// Exclude matching directory names or paths (`*` is supported).
        #[arg(long)]
        exclude: Vec<String>,
        /// Disable the built-in build/cache/vendor exclusions.
        #[arg(long)]
        no_default_excludes: bool,
        /// Discover only one kind: rust, node, python, go, php, ruby, elixir, java, dotnet.
        #[arg(long)]
        project_kind: Option<String>,
        /// Maximum simultaneous project scans.
        #[arg(long)]
        parallel: Option<usize>,
        /// Do not collect Git repository metadata.
        #[arg(long)]
        no_git: bool,
        /// Follow in-boundary directory symlinks with canonical loop detection.
        #[arg(long)]
        follow_symlinks: bool,
    },
    /// Print the Ferret logo and brand mark.
    Logo,
    /// Clone the latest ferret source and install it with `baby --user`.
    Update,
    /// Initialize the local Ferret database.
    Init,
    /// Register or update a repository profile.
    Register {
        #[arg(long)]
        slug: String,
    },
    /// Capture a review snapshot from a git diff.
    Snapshot {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        diff: Option<PathBuf>,
        #[arg(long)]
        pr: Option<u32>,
    },
    /// Show corpus statistics.
    Stats,
    /// Predict findings for a snapshot using retrieval.
    Predict {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        diff: Option<PathBuf>,
    },
    /// Show routing decisions for predicted findings.
    Route {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        diff: Option<PathBuf>,
    },
    /// Track a CodeRabbit review on the target directory (or pwd) and learn from it.
    Track {
        /// Target directory to review (defaults to current directory)
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,
        /// Also treat as positional for convenience: ferret track ./mydir
        #[arg(value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// Convert recent Ferret findings into Dreamseq .dreams entries in the target directory.
    Dream {
        /// Target directory whose .dreams file will be written (defaults to current directory)
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,
        /// Positional directory for convenience: ferret dream ./mydir
        #[arg(value_name = "DIR")]
        dir: Option<PathBuf>,
        /// Maximum number of recent findings to convert (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Also write a legacy .dream file (single file) alongside .dreams/active.dreams
        #[arg(long)]
        legacy: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    chase::configure(cli.json);
    let db = format!("sqlite:{}", cli.database.display());

    if let Some(target) = cli.target.as_deref() {
        let exit = run_project_command(target, cli.json);
        std::process::exit(exit.into());
    }

    let Some(command) = cli.command else {
        let target = std::env::current_dir().context("resolve current directory")?;
        let exit = run_project_command(&target, cli.json);
        std::process::exit(exit.into());
    };

    match command {
        Command::Business {
            root,
            max_depth,
            include,
            exclude,
            no_default_excludes,
            project_kind,
            parallel,
            no_git,
            follow_symlinks,
        } => {
            let root = match root.or_else(home_directory) {
                Some(root) => root,
                None => {
                    eprintln!(
                        "ERROR business_invocation_failed\nreason: HOME is not set; provide ROOT"
                    );
                    std::process::exit(3);
                }
            };
            let project_kind = match project_kind {
                Some(kind) => match ProjectKind::parse(&kind) {
                    Some(kind) => Some(kind),
                    None => {
                        eprintln!("ERROR business_invocation_failed\nreason: unknown project kind `{kind}`");
                        std::process::exit(3);
                    }
                },
                None => None,
            };
            let mut options = DiscoveryOptions {
                max_depth,
                include,
                exclude: if no_default_excludes {
                    Vec::new()
                } else {
                    business::DEFAULT_EXCLUSIONS
                        .iter()
                        .map(|value| (*value).to_owned())
                        .collect()
                },
                project_kind,
                follow_symlinks,
                git: !no_git,
            };
            options.exclude.extend(exclude);
            let parallel = parallel.unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(1)
            });
            if parallel == 0 {
                eprintln!(
                    "ERROR business_invocation_failed\nreason: --parallel must be at least 1"
                );
                std::process::exit(3);
            }
            match business::scan_business(&root, &options, parallel, scan_project) {
                Ok(report) => {
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        print_business_report(&report);
                    }
                    std::process::exit(report.exit_code().into());
                }
                Err(error) => {
                    eprintln!("ERROR business_discovery_failed\nreason: {error:#}");
                    std::process::exit(3);
                }
            }
        }
        Command::Logo => {
            print_logo();
        }
        Command::Update => {
            run_update()?;
        }
        Command::Init => {
            print_logo();
            let spinner = spinner("Opening database");
            let _store = Store::open(&db).await?;
            spinner.finish_with_message(style("Database ready").green().to_string());
            info!(path = %cli.database.display(), "database initialized");
        }
        Command::Register { slug } => {
            let spinner = spinner("Registering repository");
            let store = Store::open(&db).await?;
            store.upsert_repo(&RepoProfile::new(&slug)).await?;
            spinner.finish_with_message(style("Repository registered").green().to_string());
            println!("{} {}", style("→").cyan(), style(slug).bold());
        }
        Command::Snapshot {
            repo,
            base,
            head,
            path,
            diff,
            pr,
        } => {
            let extract_spinner = spinner("Extracting hunks");
            let store = Store::open(&db).await?;
            let repo_id = store.upsert_repo(&RepoProfile::new(&repo)).await?;
            let hunks = load_hunks(path.as_deref(), diff.as_deref(), &base, &head)?;
            extract_spinner
                .finish_with_message(format!("{} hunks extracted", style(hunks.len()).bold()));

            let save_spinner = spinner("Saving snapshot");
            let snapshot = ReviewSnapshot {
                id: SnapshotId::new(),
                repo_id,
                pr_number: pr,
                base_sha: base,
                head_sha: head,
                hunks,
                teachers: vec![
                    TeacherKind::GitHub,
                    TeacherKind::Sarif,
                    TeacherKind::CodeRabbit,
                ],
                captured_at: Utc::now(),
            };
            store.save_snapshot(&snapshot).await?;
            save_spinner.finish_with_message(style("Snapshot saved").green().to_string());
            print_snapshot_summary(&snapshot);
        }
        Command::Stats => {
            let spinner = spinner("Counting corpus");
            let store = Store::open(&db).await?;
            let stats = store.corpus_stats().await?;
            spinner.finish_and_clear();
            print_stats(&stats);
        }
        Command::Predict {
            repo,
            base,
            head,
            path,
            diff,
        } => {
            let store = Store::open(&db).await?;
            let repo_id = store.upsert_repo(&RepoProfile::new(&repo)).await?;
            let hunks = load_hunks(path.as_deref(), diff.as_deref(), &base, &head)?;
            let snapshot = ReviewSnapshot {
                id: SnapshotId::new(),
                repo_id,
                pr_number: None,
                base_sha: base,
                head_sha: head,
                hunks,
                teachers: vec![TeacherKind::Ferret],
                captured_at: Utc::now(),
            };

            let pb = hunk_progress(snapshot.hunks.len(), "Predicting findings").await;
            let predictions = RetrievalPredictor::predict(&store, &snapshot).await?;
            pb.finish_and_clear();
            print_predictions(&predictions);
        }
        Command::Route {
            repo,
            base,
            head,
            path,
            diff,
        } => {
            let store = Store::open(&db).await?;
            let repo_id = store.upsert_repo(&RepoProfile::new(&repo)).await?;
            let hunks = load_hunks(path.as_deref(), diff.as_deref(), &base, &head)?;
            let snapshot = ReviewSnapshot {
                id: SnapshotId::new(),
                repo_id,
                pr_number: None,
                base_sha: base,
                head_sha: head,
                hunks,
                teachers: vec![TeacherKind::Ferret],
                captured_at: Utc::now(),
            };

            let pb = hunk_progress(snapshot.hunks.len(), "Routing findings").await;
            let predictions = RetrievalPredictor::predict(&store, &snapshot).await?;
            pb.finish_and_clear();
            let router = Router::new(RouterConfig::default());
            let routed = router.route(predictions);
            print_routed(&routed);
        }
        Command::Track { path, dir } => {
            let target = resolve_track_target(path.or(dir));
            run_track(&db, &target).await?;
        }
        Command::Dream {
            path,
            dir,
            limit,
            legacy,
        } => {
            let target = resolve_track_target(path.or(dir));
            run_dream(&db, &target, limit, legacy).await?;
        }
    }

    Ok(())
}

fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn run_project_command(target: &Path, json: bool) -> u8 {
    let project = match business::project_at(target, true) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("ERROR {error:#}");
            return 3;
        }
    };
    match scan_project(&project) {
        Ok(analysis) => {
            let exit = u8::from(!analysis.findings.is_empty());
            if json {
                let output = serde_json::json!({
                    "schema": "ferret.project/v1",
                    "project": project,
                    "analysis": analysis,
                });
                match serde_json::to_string_pretty(&output) {
                    Ok(output) => println!("{output}"),
                    Err(error) => {
                        eprintln!("ERROR serialize_project_report\nreason: {error}");
                        return 3;
                    }
                }
            } else {
                println!("FERRET\n");
                println!("Project:  {}", project.project_name);
                println!("Path:     {}", project.canonical_path.display());
                println!("Kind:     {}", project.project_kind);
                println!("Files:    {}", analysis.files_scanned);
                println!("Findings: {}", analysis.findings.len());
                if !analysis.findings.is_empty() {
                    println!("\nPROJECT FINDINGS\n");
                    for finding in &analysis.findings {
                        println!(
                            "{}  {}",
                            finding.severity.to_ascii_uppercase(),
                            finding.message
                        );
                    }
                }
            }
            exit
        }
        Err(error) => {
            eprintln!(
                "ERROR project_scan_failed\ntarget: {}\nreason: {error:#}",
                project.canonical_path.display()
            );
            2
        }
    }
}

fn scan_project(project: &ProjectIdentity) -> Result<ProjectAnalysis> {
    let manifest = std::fs::read_to_string(&project.project_manifest)
        .with_context(|| format!("read manifest {}", project.project_manifest.display()))?;
    if manifest.trim().is_empty() {
        anyhow::bail!("manifest is empty: {}", project.project_manifest.display());
    }
    if project.project_kind == ProjectKind::Node {
        serde_json::from_str::<serde_json::Value>(&manifest)
            .with_context(|| format!("parse manifest {}", project.project_manifest.display()))?;
    }

    let hunks = collect_hunks_for_track(&project.canonical_path)?;
    let snapshot = ReviewSnapshot {
        id: SnapshotId::new(),
        repo_id: ferret_core::RepoId::from_slug(&project.canonical_path.to_string_lossy()),
        pr_number: None,
        base_sha: "scan-base".into(),
        head_sha: "scan-head".into(),
        hunks,
        teachers: vec![TeacherKind::CodeRabbit, TeacherKind::Ferret],
        captured_at: Utc::now(),
    };
    let files_by_hunk = snapshot
        .hunks
        .iter()
        .map(|hunk| (hunk.id.as_str(), hunk.file_path.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut findings = coderabbit_review(&snapshot)
        .into_iter()
        .map(|finding| FindingSummary {
            category: format!("{:?}", finding.category).to_ascii_lowercase(),
            severity: format!("{:?}", finding.severity).to_ascii_lowercase(),
            message: finding.message,
            rule_id: finding.rule_id,
            file: finding
                .hunk_id
                .as_deref()
                .and_then(|id| files_by_hunk.get(id).copied())
                .map(ToOwned::to_owned),
        })
        .collect::<Vec<_>>();
    findings.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then_with(|| left.rule_id.cmp(&right.rule_id))
            .then_with(|| left.message.cmp(&right.message))
    });
    Ok(ProjectAnalysis {
        files_scanned: snapshot.hunks.len(),
        findings,
    })
}

fn print_business_report(report: &BusinessReport) {
    println!("FERRET BUSINESS\n");
    println!("Root: {}\n", report.root.display());
    println!("Projects discovered: {}", report.summary.discovered);
    println!("Projects scanned:    {}", report.summary.scanned);
    println!("Successful:          {}", report.summary.successful);
    println!("Warnings:            {}", report.summary.warnings);
    println!("Errors:              {}", report.summary.errors);
    println!("\nPROJECTS\n");
    for result in &report.projects {
        let (symbol, outcome) = match result.status {
            ProjectStatus::Passed => ("✓", "PASS".to_owned()),
            ProjectStatus::Warning => (
                "⚠",
                format!(
                    "{} findings",
                    result
                        .analysis
                        .as_ref()
                        .map_or(0, |analysis| analysis.findings.len())
                ),
            ),
            ProjectStatus::Error => ("✗", "ERROR".to_owned()),
        };
        println!(
            "{symbol} {:<20} {:<8} {outcome}",
            result.project.project_name, result.project.project_kind
        );
        if let Some(error) = &result.error {
            println!("  {}", style(error).red());
        }
    }
    if !report.business_findings.is_empty() {
        println!("\nBUSINESS FINDINGS\n");
        for finding in &report.business_findings {
            println!(
                "{:<5} {}",
                finding.severity.to_ascii_uppercase(),
                finding.message
            );
        }
    }
}

fn resolve_track_target(input: Option<PathBuf>) -> PathBuf {
    match input {
        Some(p) if p.as_os_str() == "." || p.as_os_str().is_empty() => {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        }
        Some(p) => p,
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

async fn run_track(db: &str, target: &Path) -> Result<()> {
    use ferret_core::{Category, Severity};

    print_logo();
    println!(
        "{} {}",
        style("Tracking CodeRabbit review on:").cyan().bold(),
        style(target.display()).bold()
    );

    // Resolve canonical target and slug
    let canonical_target = target
        .canonicalize()
        .unwrap_or_else(|_| target.to_path_buf());
    let slug = infer_slug(&canonical_target);
    println!("{} {}", style("→ slug:").dim(), style(&slug).bold());

    let scan_spinner = spinner("Scanning environment");
    let store = Store::open(db).await?;
    let repo_id = store.upsert_repo(&RepoProfile::new(&slug)).await?;

    // Collect hunks: try git diff first if target is a git repo, else scan files
    let hunks = collect_hunks_for_track(&canonical_target)?;
    scan_spinner.finish_with_message(format!(
        "{} files/hunks collected from {}",
        style(hunks.len()).bold(),
        style(canonical_target.display()).dim()
    ));

    if hunks.is_empty() {
        println!("{}", style("No files found to review.").yellow());
        return Ok(());
    }

    // Build snapshot that will be tracked
    let snapshot = ReviewSnapshot {
        id: SnapshotId::new(),
        repo_id,
        pr_number: None,
        base_sha: "track-base".into(),
        head_sha: "track-head".into(),
        hunks: hunks.clone(),
        teachers: vec![TeacherKind::CodeRabbit, TeacherKind::Ferret],
        captured_at: Utc::now(),
    };
    store.save_snapshot(&snapshot).await?;
    info!(snapshot = %snapshot.id.0, hunks = hunks.len(), "snapshot saved for track");

    // Conduct CodeRabbit review (heuristic + stub teacher) while Ferret tracks
    let review_spinner = spinner("Running CodeRabbit review (Ferret tracking)");
    let findings = coderabbit_review(&snapshot);
    review_spinner.finish_with_message(format!(
        "{} CodeRabbit findings",
        style(findings.len()).bold()
    ));

    // Ferret learns: persist findings and learn environment + review habits
    let learn_spinner = spinner("Ferret learning environment & review habits");
    let mut env_profile = RepoProfile::new(&slug);
    // Learn environment: languages, file counts
    let mut lang_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for h in &hunks {
        let lang = h.language.clone().unwrap_or_else(|| "unknown".into());
        *lang_counts.entry(lang).or_insert(0) += 1;
    }
    // Learn review habits: category distribution and severity
    let mut cat_counts: std::collections::HashMap<Category, usize> =
        std::collections::HashMap::new();
    let mut sev_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in &findings {
        store.save_finding(f).await?;
        *cat_counts.entry(f.category).or_insert(0) += 1;
        *sev_counts.entry(format!("{:?}", f.severity)).or_insert(0) += 1;
    }

    // Update strictness: higher strictness for categories CodeRabbit flagged often
    let total = findings.len().max(1) as f32;
    for (cat, count) in &cat_counts {
        let strictness = (*count as f32 / total).clamp(0.0, 1.0);
        env_profile.strictness.insert(*cat, strictness);
    }
    // Teacher reliability: CodeRabbit observed
    env_profile
        .teacher_reliability
        .insert("coderabbit".into(), 0.85);
    env_profile
        .teacher_reliability
        .insert("ferret".into(), 0.75);
    env_profile.total_snapshots = 1;
    env_profile.total_findings = findings.len() as u64;

    // Merge with existing profile if present: load previous and increment
    // (upsert will replace, so we fetch existing via stats and previous profile)
    // For simplicity, we add to the newly learned profile and upsert
    store.upsert_repo(&env_profile).await?;
    learn_spinner.finish_with_message(style("Ferret updated").green().to_string());

    // Print summary
    println!();
    let header = Style::new().bold().underlined();
    println!("{}", header.apply_to("Track summary"));
    println!(
        "  {} {}",
        style("target:").cyan().bold(),
        style(canonical_target.display()).bold()
    );
    println!(
        "  {} {}",
        style("hunks:").cyan().bold(),
        style(hunks.len()).bold()
    );
    println!(
        "  {} {}",
        style("findings:").cyan().bold(),
        style(findings.len()).bold()
    );
    if !lang_counts.is_empty() {
        let langs = lang_counts
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {} {}",
            style("languages:").cyan().bold(),
            style(langs).dim()
        );
    }
    if !cat_counts.is_empty() {
        let cats = cat_counts
            .iter()
            .map(|(k, v)| format!("{k:?}:{v}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {} {}", style("habits:").cyan().bold(), style(cats).dim());
    }
    println!(
        "  {} {}",
        style("teacher:").cyan().bold(),
        style("coderabbit (tracking)").dim()
    );

    if findings.is_empty() {
        println!(
            "{}",
            style("No issues found — Ferret learned clean-review pattern.").dim()
        );
    } else {
        println!();
        println!(
            "{}",
            header.apply_to(format!("CodeRabbit findings ({})", findings.len()))
        );
        for f in &findings {
            let sev_style = match f.severity {
                Severity::Critical => Style::new().red().bold(),
                Severity::Major => Style::new().red().bold(),
                Severity::Minor => Style::new().yellow().bold(),
                Severity::Info => Style::new().dim(),
            };
            println!(
                "  {} [{}] {} — {}",
                style("▸").cyan(),
                sev_style.apply_to(format!("{:?}", f.severity)),
                style(f.hunk_id.as_deref().unwrap_or("-")).dim(),
                f.message
            );
            if let Some(s) = &f.suggestion {
                println!("      {} {}", style("↳").dim(), style(s).dim().italic());
            }
        }
    }

    // Show that Ferret is now remembering for next time
    let stats = store.corpus_stats().await?;
    println!();
    println!(
        "{}",
        style(format!(
            "Ferret corpus: {} snapshots, {} findings total",
            stats.snapshots, stats.findings
        ))
        .dim()
    );

    Ok(())
}

async fn run_dream(db: &str, target: &Path, limit: usize, legacy: bool) -> Result<()> {
    let canonical_target = target
        .canonicalize()
        .unwrap_or_else(|_| target.to_path_buf());
    if !canonical_target.exists() {
        anyhow::bail!("target does not exist: {}", canonical_target.display());
    }
    let target_dir = if canonical_target.is_file() {
        canonical_target
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    } else {
        canonical_target.clone()
    };

    let store = Store::open(db).await?;
    let findings = store
        .recent_findings(limit as i64)
        .await
        .unwrap_or_default();

    let spinner = spinner(&format!(
        "Converting {} recent findings to .dreams",
        findings.len()
    ));
    // Map severity -> priority, category -> workflow-friction
    let output = render_ferret_dreams(&target_dir, &findings).await;
    let dreams_dir = target_dir.join(".dreams");
    std::fs::create_dir_all(&dreams_dir)
        .with_context(|| format!("create {}", dreams_dir.display()))?;
    std::fs::create_dir_all(dreams_dir.join("history"))?;
    for lifecycle in ["completed.dreams", "rejected.dreams"] {
        let p = dreams_dir.join(lifecycle);
        if !p.exists() {
            // private: 0o600 on unix
            std::fs::write(&p, b"version: 1\ndreams: []\n")
                .with_context(|| format!("init {}", p.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))
                    .with_context(|| format!("protect {}", p.display()))?;
            }
        }
    }
    let active_path = dreams_dir.join("active.dreams");
    // atomic write via temp file + rename
    let tmp = dreams_dir.join(format!(".active.dreams.tmp.{}", std::process::id()));
    std::fs::write(&tmp, output.as_bytes()).with_context(|| format!("write {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("protect {}", tmp.display()))?;
    }
    std::fs::rename(&tmp, &active_path)
        .with_context(|| format!("rename to {}", active_path.display()))?;

    // history copy
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let id = uuid::Uuid::new_v4()
        .to_string()
        .chars()
        .take(8)
        .collect::<String>();
    let hist_path = dreams_dir
        .join("history")
        .join(format!("{date}-{id}.dreams"));
    std::fs::write(&hist_path, output.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hist_path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("protect {}", hist_path.display()))?;
    }

    if legacy {
        let legacy_path = target_dir.join(".dream");
        std::fs::write(&legacy_path, output.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&legacy_path, std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("protect {}", legacy_path.display()))?;
        }
    }

    spinner.finish_with_message(style("Dream file written").green().to_string());

    println!(
        "{} {}",
        style("→ target:").cyan().bold(),
        style(target_dir.display()).bold()
    );
    println!(
        "  {} {}",
        style("findings:").cyan().bold(),
        style(findings.len()).bold()
    );
    println!(
        "  {} {}",
        style("file:").cyan().bold(),
        style(active_path.display()).dim()
    );
    if legacy {
        println!(
            "  {} {}",
            style("legacy:").cyan().bold(),
            style(target_dir.join(".dream").display()).dim()
        );
    }
    if findings.is_empty() {
        println!(
            "{}",
            style(
                "No recent findings — wrote empty dreams list. Run `ferret track` to learn first."
            )
            .dim()
        );
    } else {
        let header = Style::new().bold().underlined();
        println!("\n{}", header.apply_to("Dreams preview"));
        for (i, f) in findings.iter().take(5).enumerate() {
            println!(
                "  {} {} [{}] {}",
                style(format!("dream-{:03}", i + 1)).cyan(),
                style(format!("{:?}", f.severity)).dim(),
                style(format!("{:?}", f.category)).dim(),
                style(&f.message).bold()
            );
        }
        if findings.len() > 5 {
            println!(
                "  {}",
                style(format!("… and {} more", findings.len() - 5)).dim()
            );
        }
    }
    Ok(())
}

async fn render_ferret_dreams(target: &Path, findings: &[ferret_core::ReviewFinding]) -> String {
    use ferret_core::{Category, Severity};
    let project = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let avg_conf = 0.85; // ferret confidence is ranking signal
    let mut out = String::new();
    out.push_str("version: 1\n");
    out.push_str(&format!("project: {}\n", yaml_scalar(project)));
    out.push_str("generated:\n  by: ferret\n");
    out.push_str(&format!("  date: {date}\n  confidence: {avg_conf:.2}\n"));
    out.push_str("source:\n");
    out.push_str(&format!("  ferret_findings: {}\n", findings.len()));
    out.push_str(&format!("  snapshots: {}\n", 0)); // placeholder, not needed for spec
    out.push_str("  note: \"converted from ferret recent findings\"\n");
    out.push_str("\ndreams:\n");
    if findings.is_empty() {
        out.push_str("  []\n");
        return out;
    }
    for (idx, f) in findings.iter().enumerate() {
        let priority = match f.severity {
            Severity::Critical => "HIGH",
            Severity::Major => "HIGH",
            Severity::Minor => "MEDIUM",
            Severity::Info => "LOW",
        };
        let category = match f.category {
            Category::Safety => "safety-friction",
            Category::Architecture => "architecture-friction",
            Category::Performance => "performance-friction",
            Category::Testing => "testing-friction",
            Category::Style => "style-friction",
            Category::Documentation => "documentation-friction",
            Category::Other => "workflow-friction",
        };
        let title = truncate(&f.message, 120);
        let problem = &f.message;
        let reason = f
            .suggestion
            .as_deref()
            .unwrap_or("Recurring review friction detected by Ferret/CodeRabbit. Converted finding suggests a capability or guardrail to prevent recurrence.");
        let affected = if let Some(hunk) = &f.hunk_id {
            format!("      - {}\n", yaml_scalar(hunk))
        } else {
            "      - none identified\n".to_string()
        };
        let action = "create capability";
        out.push_str(&format!(
            "\n  - id: dream-{:03}\n    priority: {priority}\n    category: {category}\n    title: {}\n    problem: {}\n    evidence:\n      reason: {}\n      confidence: {:.2}\n      mutation_fitness: {:.2}\n    affected:\n{}    proposed_mutation:\n      action: {action}\n      capability: {}\n    acceptance:\n      - Evidence-backed intervention reduces recurrence of this friction\n      - Outcome is verified by tests or deterministic operational checks\n    expected_value: {}\n",
            idx + 1,
            yaml_scalar(&title),
            yaml_scalar(problem),
            yaml_scalar(reason),
            0.80,
            0.75,
            affected,
            yaml_scalar(&title),
            yaml_scalar("Reduces review churn; codifies team habit")
        ));
    }
    out
}

fn yaml_scalar(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

fn infer_slug(target: &Path) -> String {
    // Prefer git remote slug if available, else directory name
    if let Ok(repo) = git2::Repository::open(target) {
        if let Ok(remote) = repo.find_remote("origin") {
            if let Some(url) = remote.url() {
                // crude parse: github.com:org/repo or https://github.com/org/repo.git
                let cleaned = url
                    .trim_end_matches(".git")
                    .rsplit('/')
                    .take(2)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("/");
                if !cleaned.is_empty() && cleaned.contains('/') {
                    return cleaned;
                }
            }
        }
    }
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("local")
        .to_string();
    format!("local/{name}")
}

fn collect_hunks_for_track(target: &Path) -> Result<Vec<ferret_core::DiffHunk>> {
    // If target is a git repo and has diff, prefer git hunks for realism
    if target.is_dir() {
        std::fs::read_dir(target)
            .with_context(|| format!("read project directory {}", target.display()))?;
        // Try to collect files via recursive walk (skip hidden/large)
        let mut hunks = Vec::new();
        let mut stack = vec![target.to_path_buf()];
        let mut count = 0usize;
        while let Some(dir) = stack.pop() {
            if count > 300 {
                break;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let mut entries = entries.flatten().collect::<Vec<_>>();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries.into_iter().rev() {
                let path = entry.path();
                let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if fname.starts_with('.')
                    || fname == "target"
                    || fname == "node_modules"
                    || fname == ".git"
                {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else if path.is_file() {
                    // Filter by extension allowlist
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                    let interesting = matches!(
                        ext,
                        "rs" | "py"
                            | "ts"
                            | "js"
                            | "tsx"
                            | "jsx"
                            | "go"
                            | "java"
                            | "rb"
                            | "php"
                            | "md"
                            | "toml"
                            | "yaml"
                            | "yml"
                            | "json"
                            | "sh"
                    );
                    if !interesting {
                        continue;
                    }
                    if let Ok(meta) = std::fs::metadata(&path) {
                        if meta.len() > 100_000 {
                            continue;
                        }
                    }
                    let content = std::fs::read_to_string(&path).unwrap_or_default();
                    if content.trim().is_empty() {
                        continue;
                    }
                    let rel = path
                        .strip_prefix(target)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned();
                    // Create a synthetic hunk: whole file as one hunk
                    let id = stable_hunk_id(&rel, &content);
                    let language = path.extension().map(|e| e.to_string_lossy().into_owned());
                    hunks.push(ferret_core::DiffHunk {
                        id,
                        file_path: rel,
                        old_start: 1,
                        old_lines: 0,
                        new_start: 1,
                        new_lines: content.lines().count() as u32,
                        content: content.chars().take(4000).collect(),
                        language,
                    });
                    count += 1;
                    if count >= 100 {
                        break;
                    }
                }
            }
        }
        hunks.sort_by(|left, right| left.file_path.cmp(&right.file_path));
        if !hunks.is_empty() {
            return Ok(hunks);
        }
    } else if target.is_file() {
        let content = std::fs::read_to_string(target)
            .with_context(|| format!("read project file {}", target.display()))?;
        let fname = target
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let id = stable_hunk_id(fname, &content);
        return Ok(vec![ferret_core::DiffHunk {
            id,
            file_path: fname.to_string(),
            old_start: 1,
            old_lines: 0,
            new_start: 1,
            new_lines: content.lines().count() as u32,
            content: content.chars().take(4000).collect(),
            language: target.extension().map(|e| e.to_string_lossy().into_owned()),
        }]);
    }

    // Fallback: try git diff if we can find a repo
    if let Some(parent) = target.parent().filter(|p| p.exists()) {
        if let Ok(repo) = git2::Repository::open(parent) {
            if let Ok(head) = repo.head() {
                if let Some(oid) = head.target() {
                    // try HEAD vs HEAD~1
                    if let Ok(base) = repo.revparse_single(&format!("{oid}~1")) {
                        if let Ok(hunks) =
                            extract_hunks(parent, &base.id().to_string(), &oid.to_string())
                        {
                            if !hunks.is_empty() {
                                return Ok(hunks);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(Vec::new())
}

/// Remove `#[cfg(test)]`-gated items (typically `mod tests { ... }`) from Rust
/// source text before it's scanned for production-code smells. Panics and
/// `.unwrap()`/`.expect()` calls inside test code are idiomatic Rust, not a
/// safety issue in the shipped binary, so Rule 1 must not see them.
///
/// This is a textual, brace-depth-based strip rather than a full parse: it
/// looks for `#[cfg(test)]`, then removes everything from the next `{` to
/// its matching `}`. That covers the common `#[cfg(test)] mod tests { ... }`
/// and `#[cfg(test)] fn ... { ... }` shapes; it does not attempt to handle
/// `#[cfg(test)]` on brace-less items (e.g. a bare `use` statement).
fn strip_cfg_test_regions(content: &str) -> String {
    const MARKER: &str = "#[cfg(test)]";
    let mut out = String::with_capacity(content.len());
    let mut i = 0usize;
    while i < content.len() {
        if content[i..].starts_with(MARKER) {
            if let Some(rel_brace) = content[i..].find('{') {
                let brace_start = i + rel_brace;
                let bytes = content.as_bytes();
                let mut depth = 0i32;
                let mut j = brace_start;
                while j < content.len() {
                    match bytes[j] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                j += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
        }
        let ch = content[i..].chars().next().unwrap_or('\u{0}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Whether `content` contains a `println!`/`console.log` call that is not
/// gated behind a condition mentioning `verbose`. CLI tools routinely print
/// user-facing status behind `if args.verbose { ... }` / `if verbose { ... }`;
/// that's intentional output, not a debug statement left over from
/// development, so Rule 3 must not flag it.
/// Whether `text` calls the real `println!` macro. A plain
/// `text.contains("println!")` also matches `eprintln!` — Ferret's own
/// recommended stderr-diagnostics channel, not a stray debug print — since
/// `"eprintln!"` contains `"println!"` as a substring. This checks that the
/// character immediately before the match isn't an identifier character, so
/// `eprintln!`/`sprintln!`-style macros don't count.
fn contains_bare_println(text: &str) -> bool {
    text.match_indices("println!").any(|(idx, _)| {
        !text[..idx]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

fn has_unguarded_debug_print(content: &str) -> bool {
    let mut guard_stack: Vec<bool> = Vec::new();
    let mut currently_guarded = false;
    for raw_line in content.lines() {
        let line = raw_line.trim();

        if (contains_bare_println(line) || line.contains("console.log")) && !currently_guarded {
            return true;
        }

        let opens_verbose_guard = line.to_ascii_lowercase().contains("verbose");
        for _ in 0..line.matches('{').count() {
            guard_stack.push(currently_guarded || opens_verbose_guard);
            currently_guarded = *guard_stack.last().unwrap();
        }
        for _ in 0..line.matches('}').count() {
            guard_stack.pop();
            currently_guarded = guard_stack.last().copied().unwrap_or(false);
        }
    }
    false
}

/// Whether `content` both (a) discards a fallible call's error via
/// `.unwrap_or(`, `.unwrap_or_default()`, or `.unwrap_or_else(`, and (b)
/// performs a destructive filesystem write (`fs::write`, `fs::copy`,
/// `.write_all(`, or a `truncate(true)` open). This is a coarse,
/// hunk-scoped co-occurrence check — like the other CodeRabbit-style rules
/// here, it does not trace whether the defaulted value is the one actually
/// written, trading precision for zero-setup detection.
fn has_error_swallow_before_write(content: &str) -> bool {
    let swallows_error = content.contains(".unwrap_or_default()")
        || content.contains(".unwrap_or(")
        || content.contains(".unwrap_or_else(");
    let writes_to_disk = content.contains("fs::write(")
        || content.contains("fs::copy(")
        || content.contains(".write_all(")
        || content.contains("truncate(true)");
    swallows_error && writes_to_disk
}

fn coderabbit_review(snapshot: &ReviewSnapshot) -> Vec<ferret_core::ReviewFinding> {
    use ferret_core::{Category, FindingId, ReviewFinding, Severity};
    let mut findings = Vec::new();
    for hunk in &snapshot.hunks {
        let content = &hunk.content;
        let file = &hunk.file_path;

        // Heuristic rules that mimic CodeRabbit style + Ferret tracking
        // Rule 1: panic/unwrap in Rust (excluding #[cfg(test)] code, see
        // strip_cfg_test_regions)
        let production_content = strip_cfg_test_regions(content);
        if production_content.contains("panic!")
            || production_content.contains(".unwrap()")
            || production_content.contains(".expect(")
        {
            findings.push(ReviewFinding {
                id: FindingId::new(),
                snapshot_id: snapshot.id,
                teacher: TeacherKind::CodeRabbit,
                hunk_id: Some(hunk.id.clone()),
                category: Category::Safety,
                severity: Severity::Major,
                message: format!("Avoid panic/unwrap in production paths ({file})"),
                suggestion: Some("Return Result instead".into()),
                rule_id: Some("safety/no-panic".into()),
                captured_at: Utc::now(),
            });
        }
        // Rule 2: TODO/FIXME
        if content.contains("TODO") || content.contains("FIXME") {
            findings.push(ReviewFinding {
                id: FindingId::new(),
                snapshot_id: snapshot.id,
                teacher: TeacherKind::CodeRabbit,
                hunk_id: Some(hunk.id.clone()),
                category: Category::Documentation,
                severity: Severity::Info,
                message: format!("TODO/FIXME left in {file} — track or resolve"),
                suggestion: None,
                rule_id: Some("docs/todo".into()),
                captured_at: Utc::now(),
            });
        }
        // Rule 3: console.log / println! debugging, excluding output gated
        // behind a `verbose` condition (see has_unguarded_debug_print)
        let looks_like_debug_print = (content.contains("console.log")
            || contains_bare_println(content))
            && file.ends_with(".rs")
            && content.lines().count() > 5;
        if looks_like_debug_print {
            // only flag if not in CLI main, and not verbose-gated output
            if !file.contains("main.rs") && has_unguarded_debug_print(content) {
                findings.push(ReviewFinding {
                    id: FindingId::new(),
                    snapshot_id: snapshot.id,
                    teacher: TeacherKind::CodeRabbit,
                    hunk_id: Some(hunk.id.clone()),
                    category: Category::Style,
                    severity: Severity::Minor,
                    message: format!("Debug print left in {file}"),
                    suggestion: Some("Remove or gate behind debug flag".into()),
                    rule_id: Some("style/no-debug-print".into()),
                    captured_at: Utc::now(),
                });
            }
        }
        // Rule 4: large file / long hunk
        if hunk.new_lines > 100 {
            findings.push(ReviewFinding {
                id: FindingId::new(),
                snapshot_id: snapshot.id,
                teacher: TeacherKind::CodeRabbit,
                hunk_id: Some(hunk.id.clone()),
                category: Category::Architecture,
                severity: Severity::Minor,
                message: format!(
                    "Large hunk ({} lines) in {file} — consider splitting",
                    hunk.new_lines
                ),
                suggestion: None,
                rule_id: Some("arch/large-hunk".into()),
                captured_at: Utc::now(),
            });
        }
        // Rule 5: missing error handling
        if file.ends_with(".rs")
            && content.contains("Result")
            && !content.contains("?")
            && content.contains("fn ")
        {
            findings.push(ReviewFinding {
                id: FindingId::new(),
                snapshot_id: snapshot.id,
                teacher: TeacherKind::CodeRabbit,
                hunk_id: Some(hunk.id.clone()),
                category: Category::Safety,
                severity: Severity::Minor,
                message: format!("Function returns Result but doesn't use `?` in {file}"),
                suggestion: Some("Propagate errors with `?`".into()),
                rule_id: Some("safety/error-propagation".into()),
                captured_at: Utc::now(),
            });
        }
        // Rule 6: error-swallowing feeding a destructive write. A fallible
        // read/parse whose error is discarded via unwrap_or/unwrap_or_default/
        // unwrap_or_else, then used as the payload of a filesystem write in
        // the same hunk, silently replaces the target's contents (often with
        // an empty string) instead of surfacing the failure. Surfaced by
        // scanning oip's src/main.rs: `clipboard.get_text().unwrap_or_default()`
        // feeds `fs::write(target, processed)`, wiping the target file to
        // empty whenever the clipboard read fails, with no warning.
        if file.ends_with(".rs") && has_error_swallow_before_write(&production_content) {
            findings.push(ReviewFinding {
                id: FindingId::new(),
                snapshot_id: snapshot.id,
                teacher: TeacherKind::CodeRabbit,
                hunk_id: Some(hunk.id.clone()),
                category: Category::Safety,
                severity: Severity::Major,
                message: format!(
                    "Error from a fallible call is swallowed via unwrap_or/unwrap_or_default/unwrap_or_else and then written to disk in {file} — a read failure will silently overwrite the target instead of aborting"
                ),
                suggestion: Some("Propagate the error (`?`) instead of defaulting before a destructive write".into()),
                rule_id: Some("safety/error-swallow-before-write".into()),
                captured_at: Utc::now(),
            });
        }
    }

    // Also run the stub teacher's ingest for completeness (currently empty)
    // and cap to 20 findings max (router limit)
    findings.truncate(20);
    findings
}

fn print_logo() {
    println!("{}", LOGO);
    println!("  {}", style(TAGLINE).dim().italic());
    println!();
}

/// Clone the latest ferret source and install it with `baby --user`.
fn run_update() -> Result<()> {
    let repo_url = env!("CARGO_PKG_REPOSITORY");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let checkout_dir =
        std::env::temp_dir().join(format!("ferret-update-{}-{nonce}", std::process::id()));

    if checkout_dir.exists() {
        std::fs::remove_dir_all(&checkout_dir)
            .with_context(|| format!("failed to clean up {}", checkout_dir.display()))?;
    }

    println!("Cloning latest ferret from {repo_url}…");
    let clone_status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", repo_url])
        .arg(&checkout_dir)
        .status()
        .context("failed to run git clone")?;
    if !clone_status.success() {
        anyhow::bail!("git clone of {repo_url} failed");
    }

    println!("Installing with 'baby --user'…");
    let install_result = std::process::Command::new("baby")
        .arg("--user")
        .current_dir(&checkout_dir)
        .status();

    let _ = std::fs::remove_dir_all(&checkout_dir);

    let install_status =
        install_result.context("failed to run 'baby --user' (is baby installed and on PATH?)")?;
    if !install_status.success() {
        anyhow::bail!("'baby --user' install failed");
    }

    println!("ferret updated to the latest version");
    Ok(())
}

fn print_snapshot_summary(snapshot: &ReviewSnapshot) {
    let label = Style::new().cyan().bold();
    println!("{} {}", label.apply_to("snapshot:"), snapshot.id.0);
    println!("{} {}", label.apply_to("hunks:"), snapshot.hunks.len());
    println!(
        "{} {}",
        label.apply_to("teachers:"),
        snapshot
            .teachers
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn print_stats(stats: &ferret_store::CorpusStats) {
    let header = Style::new().bold().underlined();
    println!("{}", header.apply_to("Corpus stats"));
    println!(
        "  {} {}",
        style("repos").cyan().bold(),
        style(stats.repos).bold()
    );
    println!(
        "  {} {}",
        style("snapshots").cyan().bold(),
        style(stats.snapshots).bold()
    );
    println!(
        "  {} {}",
        style("hunks").cyan().bold(),
        style(stats.hunks).bold()
    );
    println!(
        "  {} {}",
        style("findings").cyan().bold(),
        style(stats.findings).bold()
    );
    println!(
        "  {} {}",
        style("outcomes").cyan().bold(),
        style(stats.outcomes).bold()
    );
}

fn print_predictions(predictions: &[PredictedFinding]) {
    if predictions.is_empty() {
        println!(
            "{}",
            style("No predictions. The corpus needs more findings.").dim()
        );
        return;
    }
    let header = Style::new().bold().underlined();
    println!(
        "{}",
        header.apply_to(format!("Predictions ({})", predictions.len()))
    );
    for p in predictions {
        let conf_style = confidence_style(p.confidence);
        println!(
            "  {} [{}] {} — {}",
            style("▸").cyan(),
            conf_style.apply_to(format!("{:.2}", p.confidence)),
            style(p.hunk_id.as_deref().unwrap_or("-")).dim(),
            p.message
        );
    }
}

fn print_routed(routed: &[ferret_route::RoutedFinding]) {
    if routed.is_empty() {
        println!("{}", style("Nothing to route.").dim());
        return;
    }
    let header = Style::new().bold().underlined();
    println!(
        "{}",
        header.apply_to(format!("Routing decisions ({})", routed.len()))
    );
    for r in routed {
        let (icon, route_style) = match r.route {
            Route::Solo => ("●", Style::new().green().bold()),
            Route::SoloWithOptionalVerify => ("◐", Style::new().yellow().bold()),
            Route::Escalate => ("○", Style::new().red().bold()),
        };
        let escalate = r
            .escalate_to
            .map(|t| format!(" → {}", t.as_str()))
            .unwrap_or_default();
        println!(
            "  {} {} {:.2} — {}{}",
            route_style.apply_to(icon),
            route_style.apply_to(format!("{:?}", r.route)),
            r.finding.confidence,
            r.finding.message,
            style(escalate).dim()
        );
    }
}

fn confidence_style(confidence: f32) -> Style {
    if confidence >= 0.98 {
        Style::new().green().bold()
    } else if confidence >= 0.75 {
        Style::new().yellow().bold()
    } else {
        Style::new().red().bold()
    }
}

fn load_hunks(
    repo_path: Option<&std::path::Path>,
    diff_path: Option<&std::path::Path>,
    base: &str,
    head: &str,
) -> Result<Vec<ferret_core::DiffHunk>> {
    if let Some(diff_path) = diff_path {
        let text = std::fs::read_to_string(diff_path)
            .with_context(|| format!("read diff {}", diff_path.display()))?;
        return Ok(hunks_from_unified_diff(&text));
    }
    if let Some(repo_path) = repo_path {
        return extract_hunks(repo_path, base, head);
    }
    anyhow::bail!("provide --path (git repo) or --diff (unified diff file)")
}

#[cfg(test)]
mod project_business_regression_tests {
    use super::*;

    struct TempTree(PathBuf);

    impl TempTree {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ferret-project-regression-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn direct_and_business_use_equivalent_project_analysis() {
        let tree = TempTree::new();
        let project_path = tree.0.join("app");
        std::fs::create_dir_all(project_path.join("src")).unwrap();
        std::fs::write(project_path.join("go.mod"), "module example/app\n").unwrap();
        std::fs::write(
            project_path.join("src/main.go"),
            "package main\nfunc main() {}\n",
        )
        .unwrap();

        let direct_project = business::project_at(&project_path, false).unwrap();
        let direct = scan_project(&direct_project).unwrap();
        let report = business::scan_business(
            &tree.0,
            &DiscoveryOptions {
                git: false,
                ..Default::default()
            },
            2,
            scan_project,
        )
        .unwrap();
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.projects[0].analysis.as_ref(), Some(&direct));
    }

    #[test]
    fn malformed_project_does_not_prevent_other_real_scans() {
        let tree = TempTree::new();
        let broken = tree.0.join("broken");
        let valid = tree.0.join("valid");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::write(broken.join("package.json"), "{not-json").unwrap();
        std::fs::write(valid.join("go.mod"), "module valid\n").unwrap();

        let report = business::scan_business(
            &tree.0,
            &DiscoveryOptions {
                git: false,
                ..Default::default()
            },
            2,
            scan_project,
        )
        .unwrap();
        assert_eq!(report.summary.errors, 1);
        assert_eq!(report.summary.successful, 1);
        assert_eq!(report.summary.scanned, 2);
    }
}

#[cfg(test)]
mod coderabbit_rule_tests {
    use super::*;

    // Rule 1 (safety/no-panic) false positive: unwrap() that only appears
    // inside a #[cfg(test)] module is idiomatic Rust, not a production
    // panic risk. Surfaced by scanning bank's src/main.rs and src/creation.rs,
    // whose #[cfg(test)] mod tests blocks were flagged as "production" code.
    #[test]
    fn strip_cfg_test_regions_removes_test_only_unwrap() {
        let content = r#"
fn real_fn() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let x = Some(1).unwrap();
        assert_eq!(x, 1);
    }
}
"#;
        let stripped = strip_cfg_test_regions(content);
        assert!(!stripped.contains(".unwrap()"));
        assert!(stripped.contains("fn real_fn"));
    }

    #[test]
    fn strip_cfg_test_regions_keeps_production_unwrap() {
        let content = r#"
fn real_fn() {
    let x = Some(1).unwrap();
}
"#;
        let stripped = strip_cfg_test_regions(content);
        assert!(stripped.contains(".unwrap()"));
    }

    #[test]
    fn strip_cfg_test_regions_handles_multiple_test_blocks() {
        let content = r#"
#[cfg(test)]
mod tests_a {
    fn t() { let _ = Some(1).unwrap(); }
}

fn production() {}

#[cfg(test)]
mod tests_b {
    fn t() { let _ = Some(2).unwrap(); }
}
"#;
        let stripped = strip_cfg_test_regions(content);
        assert!(!stripped.contains(".unwrap()"));
        assert!(stripped.contains("fn production"));
    }

    // Rule 3 (style/no-debug-print) false positive: println! gated behind
    // `if args.verbose { ... }` is intentional CLI output, not a debug
    // statement left over from development. Surfaced by scanning bank's
    // src/creation.rs and src/permissions.rs verbose-mode output.
    #[test]
    fn verbose_gated_println_is_not_flagged() {
        let content = r#"
pub fn create_file(path: &Path, args: &Args) -> Result<()> {
    if path.exists() {
        if args.verbose {
            println!("File already exists: {}", colored_path(path, Color::Yellow));
        }
    } else {
        fs::File::create(path)?;
    }
    Ok(())
}
"#;
        assert!(!has_unguarded_debug_print(content));
    }

    #[test]
    fn unguarded_println_is_still_flagged() {
        let content = r#"
pub fn create_file(path: &Path) -> Result<()> {
    println!("debug: creating {:?}", path);
    fs::File::create(path)?;
    Ok(())
}
"#;
        assert!(has_unguarded_debug_print(content));
    }

    #[test]
    fn println_after_verbose_block_closes_is_still_flagged() {
        let content = r#"
pub fn f(args: &Args) {
    if args.verbose {
        println!("a");
    }
    println!("b");
}
"#;
        assert!(has_unguarded_debug_print(content));
    }

    // Regression: "eprintln!" contains "println!" as a substring
    // (eprintln! = "e" + "println!"), so a naive `.contains("println!")`
    // flagged every legitimate stderr-diagnostics call as a leftover debug
    // print. Observed against uni's src/run.rs, which is entirely eprintln!
    // telemetry and has no println! at all.
    #[test]
    fn eprintln_is_not_flagged_as_debug_print() {
        let content = r#"
pub fn f(e: &str) {
    eprintln!("uni: tool=amber stage=parse outcome=json_error error={e}");
    eprintln!("uni: internal error: a tool task panicked: {e}");
}
"#;
        assert!(!has_unguarded_debug_print(content));
        assert!(!contains_bare_println(content));
    }

    #[test]
    fn bare_println_is_still_detected_by_the_substring_guard() {
        assert!(contains_bare_println("println!(\"debug\");"));
        assert!(!contains_bare_println("eprintln!(\"telemetry\");"));
    }

    // Rule 6 (safety/error-swallow-before-write): a fallible read whose
    // error is discarded via unwrap_or_default(), then written straight to
    // disk, silently wipes the target on failure instead of erroring out.
    // Surfaced by scanning oip's src/main.rs.
    #[test]
    fn unwrap_or_default_feeding_fs_write_is_flagged() {
        let content = r#"
fn apply_clipboard_to_file(target: &PathBuf) {
    let mut clipboard = Clipboard::new().expect("Clipboard unavailable");
    let text = clipboard.get_text().unwrap_or_default();
    fs::write(target, text).expect("Write failed");
}
"#;
        assert!(has_error_swallow_before_write(content));
    }

    #[test]
    fn unwrap_or_else_feeding_write_all_is_flagged() {
        let content = r#"
fn save(mut f: File, data: Result<Vec<u8>, Error>) {
    let bytes = data.unwrap_or_else(|_| Vec::new());
    f.write_all(&bytes).expect("write failed");
}
"#;
        assert!(has_error_swallow_before_write(content));
    }

    #[test]
    fn unwrap_or_default_without_a_write_is_not_flagged() {
        let content = r#"
fn count(text: Option<&str>) -> usize {
    let s = text.unwrap_or_default();
    s.len()
}
"#;
        assert!(!has_error_swallow_before_write(content));
    }

    #[test]
    fn fs_write_without_error_swallowing_is_not_flagged() {
        let content = r#"
fn apply(target: &PathBuf, text: String) -> anyhow::Result<()> {
    fs::write(target, text)?;
    Ok(())
}
"#;
        assert!(!has_error_swallow_before_write(content));
    }
}

#[cfg(test)]
mod coderabbit_false_positive_regression_tests {
    use super::*;

    fn snapshot_for(file: &str, content: &str) -> ReviewSnapshot {
        ReviewSnapshot {
            id: SnapshotId::new(),
            repo_id: ferret_core::RepoId::from_slug("test/repo"),
            pr_number: None,
            base_sha: "base".into(),
            head_sha: "head".into(),
            hunks: vec![ferret_core::DiffHunk {
                id: "hunk".into(),
                file_path: file.to_string(),
                old_start: 1,
                old_lines: 0,
                new_start: 1,
                new_lines: content.lines().count() as u32,
                content: content.to_string(),
                language: file.rsplit_once('.').map(|(_, ext)| ext.to_string()),
            }],
            teachers: vec![TeacherKind::CodeRabbit],
            captured_at: Utc::now(),
        }
    }

    fn rule_ids(file: &str, content: &str) -> Vec<String> {
        coderabbit_review(&snapshot_for(file, content))
            .into_iter()
            .filter_map(|f| f.rule_id)
            .collect()
    }

    // deckhand's deliver.toml declares forbidden patterns as data:
    // `forbid_regex = ["TODO", "FIXME", "unimplemented!", "panic!"]`.
    // That is a config file listing pattern names, not code containing them.
    const DELIVER_TOML: &str = r#"[[file]]
path = "Cargo.toml"
required = true

[[file]]
path = "src/main.rs"
required = true
forbid_regex = ["TODO", "FIXME", "unimplemented!", "panic!"]
"#;

    #[test]
    fn config_file_listing_panic_pattern_is_not_a_panic_finding() {
        let ids = rule_ids("deliver.toml", DELIVER_TOML);
        assert!(!ids.iter().any(|id| id == "safety/no-panic"), "{ids:?}");
    }

    #[test]
    fn config_file_quoting_todo_pattern_is_not_a_todo_finding() {
        let ids = rule_ids("deliver.toml", DELIVER_TOML);
        assert!(!ids.iter().any(|id| id == "docs/todo"), "{ids:?}");
    }

    #[test]
    fn todo_comment_in_config_file_is_still_flagged() {
        let ids = rule_ids("deliver.toml", "# TODO: add the lib target\n[[file]]\npath = \"x\"\n");
        assert!(ids.iter().any(|id| id == "docs/todo"), "{ids:?}");
    }

    #[test]
    fn rust_string_literals_naming_patterns_are_not_findings() {
        let content = r#"
fn rules() -> Vec<&'static str> {
    vec!["panic!", ".unwrap()", "TODO", "FIXME"]
}
"#;
        let ids = rule_ids("src/rules.rs", content);
        assert!(!ids.iter().any(|id| id == "safety/no-panic"), "{ids:?}");
        assert!(!ids.iter().any(|id| id == "docs/todo"), "{ids:?}");
    }

    #[test]
    fn real_unwrap_in_rust_source_is_still_flagged() {
        let content = "fn f(x: Option<u8>) -> u8 {\n    x.unwrap()\n}\n";
        let ids = rule_ids("src/lib.rs", content);
        assert!(ids.iter().any(|id| id == "safety/no-panic"), "{ids:?}");
    }

    #[test]
    fn real_panic_in_rust_source_with_raw_strings_is_still_flagged() {
        let content = "fn f() {\n    let s = r#\"quoted \"inner\" text\"#;\n    panic!(\"boom {s}\");\n}\n";
        let ids = rule_ids("src/lib.rs", content);
        assert!(ids.iter().any(|id| id == "safety/no-panic"), "{ids:?}");
    }

    #[test]
    fn todo_comment_in_rust_source_is_still_flagged() {
        let content = "fn f() {\n    // TODO: handle the error\n}\n";
        let ids = rule_ids("src/lib.rs", content);
        assert!(ids.iter().any(|id| id == "docs/todo"), "{ids:?}");
    }

    // deckhand's src/auto_start.rs: canonicalize() failing falls back to the
    // un-canonicalized path; nothing is read from the file being written.
    const AUTO_START: &str = r##"
pub fn install(opts: InstallOptions) -> Result<()> {
    let unit_dir = systemd::user_unit_dir()?;
    let unit_path = unit_dir.join(UNIT_FILE);
    let deckhand_exe = std::env::current_exe().context("failed to locate the deckhand executable")?;
    let deckhand_exe = deckhand_exe.canonicalize().unwrap_or(deckhand_exe);

    let unit = format!(
        r#"[Unit]
ExecStart={} auto-clean
"#,
        deckhand_exe.display(),
    );

    fs::write(&unit_path, unit)
        .with_context(|| format!("failed to write {}", unit_path.display()))?;
    Ok(())
}
"##;

    #[test]
    fn non_read_fallback_before_write_is_not_flagged() {
        assert!(!has_error_swallow_before_write(AUTO_START));
        let ids = rule_ids("src/auto_start.rs", AUTO_START);
        assert!(
            !ids.iter().any(|id| id == "safety/error-swallow-before-write"),
            "{ids:?}"
        );
    }

    #[test]
    fn defaulted_read_written_back_is_still_flagged() {
        let content = r#"
fn bump(p: &Path) -> Result<()> {
    let cfg = fs::read_to_string(p).unwrap_or_default();
    fs::write(p, cfg + "x")?;
    Ok(())
}
"#;
        assert!(has_error_swallow_before_write(content));
        let ids = rule_ids("src/config.rs", content);
        assert!(
            ids.iter().any(|id| id == "safety/error-swallow-before-write"),
            "{ids:?}"
        );
    }

    #[test]
    fn defaulted_read_flowing_through_a_derived_binding_is_flagged() {
        let content = r#"
fn bump(p: &Path) -> Result<()> {
    let cfg = fs::read_to_string(p)
        .unwrap_or_else(|_| String::new());
    let updated = cfg.replace("a", "b");
    fs::write(p, updated)?;
    Ok(())
}
"#;
        assert!(has_error_swallow_before_write(content));
    }

    #[test]
    fn defaulted_parse_of_read_content_written_back_is_flagged() {
        let content = r#"
fn bump(p: &Path) -> Result<()> {
    let text = fs::read_to_string(p)?;
    let cfg: Config = toml::from_str(&text).unwrap_or_default();
    fs::write(p, toml::to_string(&cfg)?)?;
    Ok(())
}
"#;
        assert!(has_error_swallow_before_write(content));
    }

    #[test]
    fn defaulted_read_in_another_function_does_not_reach_the_write() {
        let content = r#"
fn load(p: &Path) -> usize {
    let text = fs::read_to_string(p).unwrap_or_default();
    text.len()
}

fn save(p: &Path, text: String) -> Result<()> {
    fs::write(p, text)?;
    Ok(())
}
"#;
        assert!(!has_error_swallow_before_write(content));
    }

    #[test]
    fn user_facing_cli_println_is_not_a_debug_print() {
        let content = r#"
pub fn install(unit_path: &Path) -> Result<()> {
    println!(
        "{} Installed and enabled {}",
        emoji::e(emoji::SUCCESS),
        SERVICE_NAME
    );
    println!("  {} unit: {}", emoji::e(emoji::LOCK), unit_path.display());
    Ok(())
}
"#;
        let ids = rule_ids("src/auto_start.rs", content);
        assert!(!ids.iter().any(|id| id == "style/no-debug-print"), "{ids:?}");
    }

    #[test]
    fn debug_formatted_println_and_dbg_are_still_flagged() {
        let content = r#"
pub fn install(unit_path: &Path) -> Result<()> {
    let x = compute();
    println!(
        "{:?}",
        x
    );
    Ok(())
}
"#;
        let ids = rule_ids("src/auto_start.rs", content);
        assert!(ids.iter().any(|id| id == "style/no-debug-print"), "{ids:?}");
        let content = "pub fn f() {\n    let x = 1;\n    dbg!(x);\n    let y = 2;\n    let z = 3;\n}\n";
        let ids = rule_ids("src/lib.rs", content);
        assert!(ids.iter().any(|id| id == "style/no-debug-print"), "{ids:?}");
    }
}
