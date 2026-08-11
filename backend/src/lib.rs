//! Vid2Audio 的核心库。
//!
//! 拆出 lib 是为了让桌面版（`src-tauri`）能直接复用同一套 Axum 路由，
//! 而不是把现有接口重写成 Tauri command——那些返回 URL 的接口
//! （音频流、预览、打包下载）走 JSON-RPC 根本没法用。
//!
//! `main.rs` 仍是独立的 bin，Docker 构建路径不变。

pub mod api;
pub mod db;
pub mod extractor;
pub mod media;
pub mod models;
pub mod platform;
pub mod scanner;
pub mod sorter;

use anyhow::Result;
use axum::Router;
use std::path::PathBuf;

/// 数据库路径：环境变量优先，否则按平台落到可写目录。
pub fn database_path() -> PathBuf {
    std::env::var("VID2AUDIO_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| platform::default_data_dir().join("vid2audio.db"))
}

/// 前端静态资源目录。桌面版不走这里（资源由 WebView 直接加载），
/// 但保留同一套解析规则，方便本地调试时用浏览器打开。
pub fn static_path(manifest_local: PathBuf) -> PathBuf {
    std::env::var("VID2AUDIO_STATIC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| platform::default_static_dir(manifest_local))
}

/// 打开数据库并组装出完整的 Axum 路由。
///
/// 服务端版把它交给 `axum::serve`，桌面版把它交给自定义 URI scheme——
/// 两边共用同一个实例，行为不会漂移。
pub async fn build_router(db_path: PathBuf, static_dir: PathBuf) -> Result<Router> {
    let database = db::Database::open(db_path).await?;
    Ok(api::router(api::AppState { db: database }, static_dir))
}
