use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use console::{style, Style};
use ferret_core::{PredictedFinding, RepoProfile, ReviewSnapshot, SnapshotId, TeacherKind};
use ferret_infer::RetrievalPredictor;
use ferret_ingest::{extract_hunks, hunks_from_unified_diff};
use ferret_route::{Route, Router, RouterConfig};
use ferret_store::Store;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

const LOGO: &str = include_str!("../../../.brandi/assets/logo.ansi");
const TAGLINE: &str = "Review memory that learns.";

#[derive(Parser)]
#[command(name = "ferret", about = "Repository-specific review distillation")]
struct Cli {
    #[arg(short, long, default_value = "ferret.db")]
    database: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the Ferret logo and brand mark.
    Logo,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let db = format!("sqlite:{}", cli.database.display());

    match cli.command {
        Command::Logo => {
            print_logo();
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
    }

    Ok(())
}

fn print_logo() {
    println!("{}", LOGO);
    println!("  {}", style(TAGLINE).dim().italic());
    println!();
}

fn spinner(msg: &str) -> ProgressBar {
    let bar = ProgressBar::new_spinner();
    bar.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner:.cyan} {msg}")
            .expect("spinner template"),
    );
    bar.set_message(msg.to_string());
    bar.enable_steady_tick(Duration::from_millis(80));
    bar
}

async fn hunk_progress(len: usize, msg: &str) -> ProgressBar {
    let bar = ProgressBar::new(len.max(1) as u64);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} {msg} [{bar:40.cyan/blue}] {pos}/{len}")
            .expect("progress template")
            .progress_chars("#>-")
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    bar.set_message(msg.to_string());
    for _ in 0..len {
        bar.inc(1);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    bar
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
