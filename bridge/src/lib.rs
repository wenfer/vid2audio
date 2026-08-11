//! WebView 请求与 Axum router 之间的转发层。
//!
//! 刻意不引用 `tauri`：这样这一层的单元测试能在没有 GTK/WebView2 的机器上跑，
//! 而 Windows 目标的编译验证又跑不了测试（交叉编译产物没法执行）。
//! 转发的正确性——状态码、响应头、请求体——恰好是最容易写错也最需要测的部分。

use axum::http::{Request, Response, StatusCode};

/// 自定义 scheme 名。改这里要同步改 `tauri.conf.json` 的 `frontendDist`。
pub const SCHEME: &str = "v2a";

/// 页面入口 URL。
///
/// Windows/Android 上 wry 把自定义协议映射成 `http://<scheme>.localhost/`，
/// macOS/Linux 上是 `<scheme>://localhost/`——两边形态不同，只能按平台构造。
pub fn entry_url() -> url::Url {
    let raw = if cfg!(any(windows, target_os = "android")) {
        format!("http://{SCHEME}.localhost/")
    } else {
        format!("{SCHEME}://localhost/")
    };
    raw.parse().expect("入口 URL 应当合法")
}

/// 把 WebView 的请求交给 Axum router，再把响应搬回来。
///
/// 响应体必须完整缓冲成 `Vec<u8>`：WebView 的自定义协议接口不支持流式。
/// 对本项目够用——最大的响应是 ZIP 打包下载和音频试听，都是一次性文件。
pub async fn forward(router: axum::Router, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let (parts, body) = request.into_parts();
    let request = Request::from_parts(parts, Body::from(body));

    // oneshot 要 router 的所有权，所以每次请求克隆一份——Router 内部是 Arc，很便宜。
    let response = match router.oneshot(request).await {
        Ok(response) => response,
        Err(error) => return plain_error(format!("路由内部错误: {error}")),
    };

    let (parts, body) = response.into_parts();
    match body.collect().await {
        Ok(collected) => Response::from_parts(parts, collected.to_bytes().to_vec()),
        Err(error) => plain_error(format!("读取响应体失败: {error}")),
    }
}

pub fn plain_error(message: String) -> Response<Vec<u8>> {
    tracing::error!("{message}");
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header("content-type", "text/plain; charset=utf-8")
        .body(message.into_bytes())
        .expect("构造错误响应不应失败")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        response::IntoResponse,
        routing::{get, post},
    };

    /// 音频试听靠 HTTP Range：`<audio>` 会发 `Range: bytes=...`，后端回 206 +
    /// `Content-Range`。转发层把响应体缓冲成 `Vec<u8>`，必须保证状态码和头
    /// 原样穿过——丢掉 206 会让 WebView 以为拿到了整个文件，拖动进度就错位。
    #[tokio::test]
    async fn forward_preserves_partial_content_status_and_headers() {
        let router = Router::new().route(
            "/audio",
            get(|| async {
                Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header("content-range", "bytes 2-5/10")
                    .header("accept-ranges", "bytes")
                    .body(axum::body::Body::from("2345"))
                    .unwrap()
            }),
        );
        let request = Request::builder()
            .uri("http://v2a.localhost/audio")
            .header("range", "bytes=2-5")
            .body(Vec::new())
            .unwrap();

        let response = forward(router, request).await;

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get("content-range").unwrap(),
            "bytes 2-5/10"
        );
        assert_eq!(response.body(), b"2345");
    }

    /// 请求体要完整送到后端，否则所有 POST（建任务、删文件）都会收到空 body。
    /// 用带反斜杠和中文的 Windows 路径做载荷，顺带确认字节没被改写。
    #[tokio::test]
    async fn forward_passes_the_request_body_through() {
        let router = Router::new().route(
            "/echo",
            post(|body: String| async move { format!("收到:{body}") }),
        );
        let payload = r#"{"path":"D:\\videos\\萌鸡小队"}"#.as_bytes().to_vec();
        let request = Request::builder()
            .method("POST")
            .uri("http://v2a.localhost/echo")
            .body(payload.clone())
            .unwrap();

        let response = forward(router, request).await;

        assert_eq!(response.status(), StatusCode::OK);
        let mut expected = "收到:".as_bytes().to_vec();
        expected.extend_from_slice(&payload);
        assert_eq!(response.body(), &expected);
    }

    /// 404 之类的状态要如实转发，不能被包装成 500——
    /// 前端靠状态码区分「文件不存在」和「后端炸了」。
    #[tokio::test]
    async fn forward_keeps_error_status_codes() {
        let request = Request::builder()
            .uri("http://v2a.localhost/missing")
            .body(Vec::new())
            .unwrap();

        let response = forward(Router::new(), request).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    /// 空响应体（204）不能在缓冲时被搞成非空。
    #[tokio::test]
    async fn forward_handles_empty_bodies() {
        let router = Router::new().route(
            "/nothing",
            get(|| async { StatusCode::NO_CONTENT.into_response() }),
        );
        let request = Request::builder()
            .uri("http://v2a.localhost/nothing")
            .body(Vec::new())
            .unwrap();

        let response = forward(router, request).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(response.body().is_empty());
    }

    #[test]
    fn entry_url_matches_the_platform_specific_custom_scheme_form() {
        let url = entry_url();
        if cfg!(any(windows, target_os = "android")) {
            assert_eq!(url.scheme(), "http");
            assert_eq!(url.host_str(), Some("v2a.localhost"));
        } else {
            assert_eq!(url.scheme(), "v2a");
            assert_eq!(url.host_str(), Some("localhost"));
        }
    }
}
