mod curly_expand;
use axum::{routing::get, Json, Router};
use clap::Parser;
use ferret_store::Store;
use std::sync::Arc;
use tracing::info;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8787")]
    listen: String,
    #[arg(long, default_value = "ferret.db")]
    db_path: String,
}

#[tokio::main]
async fn __curly_original_main() -> anyhow::Result<()> {
    let args = Args::parse();
    run(&args.listen, &args.db_path).await
}

struct AppState {
    store: Store,
}

pub async fn run(listen: &str, db_path: &str) -> anyhow::Result<()> {
    let store = Store::open(db_path).await?;
    let state = Arc::new(AppState { store });

    let app = Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats))
        .with_state(state);

    info!(listen, "ferretd starting");
    let listener = tokio::net::TcpListener::bind(listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn stats(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Json<ferret_store::CorpusStats> {
    let stats = state.store.corpus_stats().await.unwrap_or_default();
    Json(stats)
}

fn main() {
    let raw_args: Vec<String> = std::env::args().collect();
    let mut positions: Vec<usize> = Vec::new();
    let mut fields: Vec<Vec<String>> = Vec::new();
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--listen" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--listen=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--listen={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--db-path" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--db-path=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--db-path={}", v))
                    .collect(),
            );
            break;
        }
    }

    if fields.is_empty() || fields.iter().all(|f| f.len() <= 1) {
        __curly_original_main();
        return;
    }

    let combos = curly_expand::cartesian(&fields);
    let exe = std::env::current_exe().expect("resolve current exe");
    let mut had_failure = false;
    for combo in &combos {
        let mut new_args = raw_args.clone();
        for (slot, value) in positions.iter().zip(combo.iter()) {
            new_args[*slot] = value.clone();
        }
        let status = std::process::Command::new(&exe)
            .args(&new_args[1..])
            .status()
            .expect("failed to re-exec self");
        if !status.success() {
            had_failure = true;
        }
    }
    if had_failure {
        std::process::exit(1);
    }
}
