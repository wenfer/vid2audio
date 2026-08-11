//! 桌面版外壳：把 backend 的 Axum router 挂到 WebView 的自定义 URI scheme 上。
//!
//! 关键设计：**不开 TCP 端口**。前端所有请求都是相对路径（`/api/v1/...`），页面本身
//! 也由这个 router 提供（`/` 和 `/static/*`），于是相对 URL 自然落到同一个 scheme，
//! `fetch`、`<audio src>`、`<a href download>` 全部走同一条路——前端一行都不用改。
//!
//! 换成 localhost 服务器就得开端口，而 `/api/v1/files/delete` 这类接口一旦监听端口，
//! 任意网页的 JS 都能打进来。自定义 scheme 只有本进程的 WebView 能访问。

use std::sync::OnceLock;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, path::BaseDirectory};
use vid2audio_bridge::{SCHEME, entry_url, forward, plain_error};

/// 后端运行时。
///
/// 用 `Box::leak` 拿到 `&'static`：runtime 必须活到进程结束。
/// 一旦它被 drop，所有正在跑的提取任务会被静默取消——用户看到的是进度条
/// 停在半路、没有任何报错；更糟的是 `Runtime::drop` 在异步上下文里会 panic。
/// 进程退出时由 OS 回收，泄漏是刻意的。
struct Backend {
    runtime: tokio::runtime::Runtime,
    /// router 要等 app handle 就绪才能建（静态资源目录得由 Tauri 解析），
    /// 但协议处理器必须在 Builder 阶段注册，所以中间隔一个 OnceLock。
    router: OnceLock<axum::Router>,
}

pub fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vid2audio=info,vid2audio_desktop=info".into()),
        )
        .init();

    // Axum 和 sqlx 需要常驻 runtime；Tauri 的事件循环要占用主线程，所以自建一个。
    // 刻意 leak 成 'static：见 Backend 的说明。
    let backend: &'static Backend = Box::leak(Box::new(Backend {
        runtime: tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
        router: OnceLock::new(),
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol(SCHEME, move |_context, request, responder| {
            let Some(router) = backend.router.get().cloned() else {
                // 窗口是在 setup 里、router 就绪之后才建的，正常不会走到这里。
                responder.respond(plain_error("后端尚未就绪".into()));
                return;
            };
            backend
                .runtime
                .spawn(async move { responder.respond(forward(router, request).await) });
        })
        .setup(move |app| {
            // 前端资源和随包的 ffmpeg 都在 Tauri 的资源目录里，路径按平台解析
            // （Windows 的 NSIS 装在 exe 同级，macOS 在 Contents/Resources）。
            if let Ok(dir) = app.path().resolve("", BaseDirectory::Resource) {
                vid2audio::platform::set_bundled_bin_dir(dir);
            }
            let static_dir = app
                .path()
                .resolve("static", BaseDirectory::Resource)
                .unwrap_or_else(|_| "static".into());
            let router = backend
                .runtime
                .block_on(vid2audio::build_router(
                    vid2audio::database_path(),
                    static_dir,
                ))
                .map_err(|error| format!("后端启动失败: {error}"))?;
            let _ = backend.router.set(router);

            WebviewWindowBuilder::new(app, "main", WebviewUrl::CustomProtocol(entry_url()))
                .title("Vid2Audio")
                .inner_size(1280.0, 860.0)
                .min_inner_size(1024.0, 700.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())?;

    Ok(())
}
