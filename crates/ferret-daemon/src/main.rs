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
async fn main() -> anyhow::Result<()> {
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
