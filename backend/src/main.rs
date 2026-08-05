mod api;
mod db;
mod extractor;
mod media;
mod models;
mod scanner;
mod sorter;

use anyhow::Result;
use api::AppState;
use db::Database;
use std::{net::SocketAddr, path::PathBuf};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vid2audio=info,tower_http=info".into()),
        )
        .init();
    let db_path = std::env::var("VID2AUDIO_DB").unwrap_or_else(|_| "data/vid2audio.db".into());
    let static_dir = std::env::var("VID2AUDIO_STATIC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let local = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
            if local.is_dir() {
                local
            } else {
                PathBuf::from("/app/static")
            }
        });
    let database = Database::open(db_path).await?;
    let app = api::router(AppState { db: database }, static_dir);
    let address: SocketAddr = std::env::var("VID2AUDIO_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8000".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(%address, "Vid2Audio started");
    axum::serve(listener, app).await?;
    Ok(())
}
