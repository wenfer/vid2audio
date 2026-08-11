use anyhow::Result;
use std::{net::SocketAddr, path::PathBuf};
use tracing::info;
use vid2audio::{build_router, database_path, static_path};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vid2audio=info,tower_http=info".into()),
        )
        .init();
    let static_dir = static_path(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"));
    let app = build_router(database_path(), static_dir).await?;
    // 默认只监听回环：文件管理接口能删改任意路径，绑到 0.0.0.0 等于把它交给整个局域网。
    // 容器部署要对外暴露时显式设 VID2AUDIO_BIND=0.0.0.0:8000。
    let address: SocketAddr = std::env::var("VID2AUDIO_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8000".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(%address, "Vid2Audio started");
    axum::serve(listener, app).await?;
    Ok(())
}
