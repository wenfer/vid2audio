use crate::{
    db::Database, extractor, media, models::*, platform, scanner::scan_paths, sorter::compare_names,
};
use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::Row;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tower::ServiceExt;
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::{Span, info_span, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}
impl ApiError {
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }
    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }
}
impl<E: std::fmt::Display> From<E> for ApiError {
    fn from(error: E) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"detail": self.message}))).into_response()
    }
}
type ApiResult<T> = std::result::Result<T, ApiError>;

pub fn router(state: AppState, static_dir: PathBuf) -> Router {
    let api = Router::new()
        .route("/settings", get(get_settings).put(update_settings))
        .route("/files", get(browse_files))
        .route("/files/copy", post(copy_files))
        .route("/files/move", post(move_files))
        .route("/files/rename", post(rename_file))
        .route("/files/delete", post(delete_files))
        .route("/files/fat-sort", post(fat_sort_directory))
        .route("/files/archive", get(download_archive))
        .route("/files/archive-to", post(archive_to_path))
        .route("/scan/start", post(start_scan))
        .route("/collections", get(list_collections))
        .route(
            "/collections/{id}",
            get(get_collection).delete(delete_collection),
        )
        .route("/collections/{id}/scan", post(rescan_collection))
        .route("/extract", post(create_job))
        .route("/extract/jobs", get(list_jobs))
        .route("/extract/jobs/{id}", get(get_job).delete(delete_job))
        .route("/extract/jobs/{id}/cancel", post(cancel_job))
        .route("/extract/jobs/{id}/pause", post(pause_job))
        .route("/extract/jobs/{id}/resume", post(resume_job))
        .route(
            "/extract/jobs/{job_id}/items/{item_id}/audio",
            get(job_audio),
        )
        .route("/preview/{video_id}", get(preview_audio))
        .route("/system/status", get(system_status));
    Router::new()
        .nest("/api/v1", api)
        .nest_service("/static", ServeDir::new(&static_dir))
        .route_service("/", ServeFile::new(static_dir.join("index.html")))
        .layer(cors_layer())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    info_span!(
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                    )
                })
                .on_response(|response: &Response<_>, latency: Duration, _span: &Span| {
                    if response.status().is_client_error()
                        || response.status().is_server_error()
                        || latency > Duration::from_secs(1)
                    {
                        warn!(
                            status = %response.status(),
                            latency_ms = latency.as_secs_f64() * 1000.0,
                            "request completed with warning",
                        );
                    }
                }),
        )
        .with_state(Arc::new(state))
}

async fn get_settings(State(state): State<Arc<AppState>>) -> ApiResult<Json<AppSettings>> {
    Ok(Json(state.db.load_settings().await?))
}
async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(values): Json<Map<String, Value>>,
) -> ApiResult<Json<AppSettings>> {
    Ok(Json(state.db.update_settings(values).await?))
}

#[derive(Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}
async fn browse_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BrowseQuery>,
) -> ApiResult<Json<Value>> {
    let settings = state.db.load_settings().await?;
    let requested = expand_home(&PathBuf::from(
        query
            .path
            .or_else(|| settings.scan_directories.first().cloned())
            .unwrap_or_else(|| platform::default_input_dir().to_string_lossy().into_owned()),
    ));
    let (current, warning) = if requested.exists() {
        (
            if requested.is_file() {
                requested.parent().unwrap_or(Path::new(".")).to_path_buf()
            } else {
                requested.clone()
            },
            String::new(),
        )
    } else {
        // 回落到默认输入目录而不是当前工作目录：从快捷方式启动时 CWD 可能是
        // System32 或安装目录，用户一进来看到的是系统文件。
        let fallback = platform::default_input_dir();
        let fallback = if fallback.is_dir() {
            fallback
        } else {
            std::env::current_dir()?
        };
        (
            fallback,
            format!("路径不存在，已打开默认目录: {}", requested.display()),
        )
    };
    let mut entries = Vec::new();
    let mut children = std::fs::read_dir(&current)
        .map_err(|_| ApiError::forbidden(format!("没有权限读取目录: {}", current.display())))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    children.sort_by(|a, b| {
        b.path().is_dir().cmp(&a.path().is_dir()).then_with(|| {
            compare_names(
                &a.file_name().to_string_lossy(),
                &b.file_name().to_string_lossy(),
                &settings.filesystem_sorting,
            )
        })
    });
    let allowed: std::collections::HashSet<_> = settings
        .video_extensions
        .iter()
        .map(|v| v.to_lowercase())
        .collect();
    let ignored: std::collections::HashSet<_> = settings
        .ignored_extensions
        .iter()
        .map(|v| v.to_lowercase())
        .collect();
    let min_size = (settings.min_file_size_mb.max(0.0) * 1024.0 * 1024.0) as u64;
    for entry in children {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if platform::is_hidden(&path, &name) {
            continue;
        }
        let is_dir = path.is_dir();
        let suffix = path
            .extension()
            .and_then(|v| v.to_str())
            .map(|v| format!(".{}", v.to_lowercase()))
            .unwrap_or_default();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let is_video = path.is_file() && allowed.contains(&suffix);
        let is_ignored = ignored.contains(&suffix);
        let too_small = is_video && size < min_size;
        let selectable = is_dir || is_video && !is_ignored && !too_small;
        let reason = if is_ignored {
            "已按后缀过滤"
        } else if too_small {
            "小于最小文件大小"
        } else if path.is_file() && !is_video {
            "非视频文件"
        } else {
            ""
        };
        entries.push(json!({"name": name, "path": canonical(&path), "type": if is_dir {"directory"} else {"file"}, "size": size, "extension": suffix, "is_video": is_video, "selectable": selectable, "reason": reason}));
    }
    let roots = platform::filesystem_roots()
        .into_iter()
        .map(|(name, path)| json!({"name": name, "path": path.to_string_lossy()}))
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"path": canonical(&current), "requested_path": requested.to_string_lossy(), "parent": current.parent().map(canonical), "warning": warning, "sorting": settings.filesystem_sorting, "roots": roots, "entries": entries}),
    ))
}

#[derive(Deserialize)]
struct FileTransferRequest {
    sources: Vec<String>,
    destination: String,
}

#[derive(Deserialize)]
struct FileRenameRequest {
    path: String,
    new_name: String,
}

#[derive(Deserialize)]
struct FileDeleteRequest {
    paths: Vec<String>,
}

#[derive(Deserialize)]
struct FatSortRequest {
    path: String,
}

#[derive(Deserialize)]
struct ArchiveQuery {
    path: String,
}

async fn download_archive(Query(query): Query<ArchiveQuery>) -> ApiResult<Response> {
    let source = expand_home(&PathBuf::from(query.path));
    validate_source(&source)?;
    let archive_name = format!(
        "{}.zip",
        source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("archive")
    );
    let bytes = tokio::task::spawn_blocking(move || create_archive(&source)).await??;
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            sanitize_header_value(&archive_name)
        ))?,
    );
    response_headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&bytes.len().to_string())?,
    );
    Ok((response_headers, Body::from(bytes)).into_response())
}

fn sanitize_header_value(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn create_archive(source: &Path) -> ApiResult<Vec<u8>> {
    use std::io::Cursor;

    Ok(write_archive(source, Cursor::new(Vec::new()))?.into_inner())
}

/// 把 `source` 打包写进 `sink`，返回写完的 sink。
///
/// 之所以对写入端做成泛型：「另存为」要直接落盘。桌面版打包的目录动辄几个 GB，
/// 先在内存里攒完整份 zip 再写出去毫无必要，而 `File` 同样满足 `Write + Seek`。
fn write_archive<W: std::io::Write + std::io::Seek>(source: &Path, sink: W) -> ApiResult<W> {
    use zip::{ZipWriter, write::SimpleFileOptions};

    let source = source.canonicalize()?;
    let root_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive")
        .to_string();
    let mut archive = ZipWriter::new(sink);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    if source.is_file() {
        archive.start_file(&root_name, options)?;
        copy_into_archive(&source, &mut archive)?;
    } else {
        for entry in walkdir::WalkDir::new(&source).follow_links(false) {
            let entry = entry.map_err(|error| anyhow::anyhow!(error))?;
            let path = entry.path();
            let relative = path
                .strip_prefix(&source)
                .map_err(|error| anyhow::anyhow!(error))?;
            let archive_path = if relative.as_os_str().is_empty() {
                root_name.clone()
            } else {
                format!(
                    "{}/{}",
                    root_name,
                    relative.to_string_lossy().replace('\\', "/")
                )
            };
            if entry.file_type().is_symlink() {
                continue;
            }
            if entry.file_type().is_dir() {
                archive.add_directory(format!("{archive_path}/"), options)?;
            } else {
                archive.start_file(archive_path, options)?;
                copy_into_archive(path, &mut archive)?;
            }
        }
    }
    Ok(archive.finish()?)
}

/// 逐块拷贝而不是 `fs::read` 整个读进内存：单个视频就可能上 GB。
fn copy_into_archive(path: &Path, sink: &mut impl std::io::Write) -> ApiResult<()> {
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, sink)?;
    Ok(())
}

#[derive(Deserialize)]
struct ArchiveToRequest {
    path: String,
    destination: String,
}

/// 把打包结果直接写到本地路径。
///
/// 桌面版专用：WebView2 里 `<a download>` 最多把文件塞进浏览器的下载目录，
/// 而用户要的是自己选的 U 盘 / SD 卡路径。所以前端先用系统「另存为」拿到目标
/// 路径，再由后端写过去，全程不经过 WebView。
async fn archive_to_path(Json(request): Json<ArchiveToRequest>) -> ApiResult<Json<Value>> {
    let source = expand_home(&PathBuf::from(request.path));
    validate_source(&source)?;
    let destination = expand_home(&PathBuf::from(request.destination));
    if destination.is_dir() {
        return Err(ApiError::forbidden(format!(
            "目标已经是一个目录: {}",
            destination.display()
        )));
    }
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| ApiError::forbidden("目标路径缺少所在目录".to_string()))?;
    if !parent.is_dir() {
        return Err(ApiError::not_found(format!(
            "目标目录不存在: {}",
            parent.display()
        )));
    }
    // 保存到正在打包的目录里，WalkDir 会把刚写出的那半个 zip 也收进去，越打越大。
    if parent.canonicalize()?.starts_with(source.canonicalize()?) {
        return Err(ApiError::forbidden(
            "不能把压缩包保存在正在打包的文件夹里".to_string(),
        ));
    }
    let target = destination.clone();
    let size = tokio::task::spawn_blocking(move || create_archive_at(&source, &target)).await??;
    Ok(Json(
        json!({"success": true, "path": canonical(&destination), "size": size}),
    ))
}

fn create_archive_at(source: &Path, destination: &Path) -> ApiResult<u64> {
    match write_archive_to_file(source, destination) {
        Ok(size) => Ok(size),
        Err(error) => {
            // 中途失败（磁盘满、U 盘被拔）会在用户自己选的位置留下一个半截的 zip，
            // 看上去和成功一样。删掉再把错误报出去。
            let _ = std::fs::remove_file(destination);
            Err(error)
        }
    }
}

fn write_archive_to_file(source: &Path, destination: &Path) -> ApiResult<u64> {
    use std::io::Write;

    let mut file = write_archive(source, std::fs::File::create(destination)?)?;
    // 显式 flush 再读长度：zip 的中央目录是最后写的。
    file.flush()?;
    Ok(file.metadata()?.len())
}

async fn copy_files(Json(request): Json<FileTransferRequest>) -> ApiResult<Json<Value>> {
    let count = request.sources.len();
    tokio::task::spawn_blocking(move || transfer_files(request, false)).await??;
    Ok(Json(json!({"success": true, "count": count})))
}

async fn move_files(Json(request): Json<FileTransferRequest>) -> ApiResult<Json<Value>> {
    let count = request.sources.len();
    tokio::task::spawn_blocking(move || transfer_files(request, true)).await??;
    Ok(Json(json!({"success": true, "count": count})))
}

async fn rename_file(Json(request): Json<FileRenameRequest>) -> ApiResult<Json<Value>> {
    let path = expand_home(&PathBuf::from(request.path));
    validate_source(&path)?;
    validate_file_name(&request.new_name)?;
    let target = path
        .parent()
        .ok_or_else(|| ApiError::forbidden("不能重命名文件系统根目录"))?
        .join(&request.new_name);
    if target.exists() {
        return Err(ApiError::conflict(format!(
            "目标已存在: {}",
            target.display()
        )));
    }
    std::fs::rename(&path, &target)?;
    Ok(Json(json!({"success": true, "path": canonical(&target)})))
}

async fn delete_files(Json(request): Json<FileDeleteRequest>) -> ApiResult<Json<Value>> {
    if request.paths.is_empty() {
        return Err(ApiError::not_found("没有选择要删除的文件"));
    }
    let count = request.paths.len();
    tokio::task::spawn_blocking(move || {
        for value in request.paths {
            let path = expand_home(&PathBuf::from(value));
            validate_source(&path)?;
            if is_filesystem_root(&path) {
                return Err(ApiError::forbidden("不能删除文件系统根目录"));
            }
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        }
        Ok::<(), ApiError>(())
    })
    .await??;
    Ok(Json(json!({"success": true, "count": count})))
}

async fn fat_sort_directory(Json(request): Json<FatSortRequest>) -> ApiResult<Json<Value>> {
    let path = expand_home(&PathBuf::from(request.path));
    if is_filesystem_root(&path) {
        return Err(ApiError::forbidden("不能对文件系统根目录执行 FAT 排序"));
    }
    if path.exists() && !path.is_dir() {
        return Err(ApiError::forbidden("FAT 排序只能对文件夹执行"));
    }
    // 目录不存在时不在这里拒绝：上次执行若在最后一步中断，条目还在临时目录里，
    // reorder_directory_fat 会先尝试恢复，恢复不了才报「文件夹不存在」。
    let (count, recovered) =
        tokio::task::spawn_blocking(move || reorder_directory_fat(&path)).await??;
    Ok(Json(
        json!({"success": true, "count": count, "recovered": recovered}),
    ))
}

fn transfer_files(request: FileTransferRequest, move_source: bool) -> ApiResult<()> {
    if request.sources.is_empty() {
        return Err(ApiError::not_found("没有选择要操作的文件"));
    }
    let destination = expand_home(&PathBuf::from(request.destination));
    if !destination.is_dir() {
        return Err(ApiError::not_found(format!(
            "目标目录不存在: {}",
            destination.display()
        )));
    }
    let destination = destination.canonicalize()?;
    for source_value in request.sources {
        let source = expand_home(&PathBuf::from(source_value));
        validate_source(&source)?;
        let source = source.canonicalize()?;
        let name = source
            .file_name()
            .ok_or_else(|| ApiError::forbidden("不能操作文件系统根目录"))?;
        let target = destination.join(name);
        if source == target {
            return Err(ApiError::conflict("源文件和目标文件相同"));
        }
        if source.is_dir() && destination.starts_with(&source) {
            return Err(ApiError::conflict("不能将文件夹复制或移动到自身内部"));
        }
        if target.exists() {
            return Err(ApiError::conflict(format!(
                "目标已存在: {}",
                target.display()
            )));
        }
        if move_source {
            if std::fs::rename(&source, &target).is_err() {
                copy_path(&source, &target)?;
                if source.is_dir() {
                    std::fs::remove_dir_all(&source)?;
                } else {
                    std::fs::remove_file(&source)?;
                }
            }
        } else {
            copy_path(&source, &target)?;
        }
    }
    Ok(())
}

fn validate_source(path: &Path) -> ApiResult<()> {
    if !path.exists() {
        return Err(ApiError::not_found(format!(
            "文件不存在: {}",
            path.display()
        )));
    }
    if is_filesystem_root(path) {
        return Err(ApiError::forbidden("不能操作文件系统根目录"));
    }
    Ok(())
}

/// 跨域策略。
///
/// 界面和 API 同源，正常使用不需要跨域。之前放开 `allow_origin(Any)` 意味着任何网页里的
/// JS 都能调用本机的 `/files/delete`、`/files/move`——桌面场景下这是任意文件删除。
/// 默认只允许同源；`VID2AUDIO_CORS_ORIGINS` 可以逗号分隔地列出额外来源（前端 dev server 用）。
fn cors_layer() -> CorsLayer {
    let origins: Vec<HeaderValue> = std::env::var("VID2AUDIO_CORS_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse().ok())
        .collect();
    if origins.is_empty() {
        return CorsLayer::new();
    }
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(Any)
        .allow_headers(Any)
}

/// 是否指向某个文件系统的根。
///
/// 只看 `parent().is_none()` 不够：Windows 上 `C:` 是「盘符相对路径」，
/// parent 返回 `Some("")` 而实际指向该盘的当前目录，能绕过这层拦截去删整个目录。
/// 但普通相对路径（`videos`）的 parent 同样是空串，所以要靠有没有盘符前缀来区分，
/// 不能用路径长度。
fn is_filesystem_root(path: &Path) -> bool {
    match path.parent() {
        None => true,
        Some(parent) if !parent.as_os_str().is_empty() => false,
        // parent 是空串：只有带盘符前缀（`C:`）才算根，`videos` 只是相对路径。
        Some(_) => matches!(
            path.components().next(),
            Some(std::path::Component::Prefix(_))
        ),
    }
}

fn validate_file_name(name: &str) -> ApiResult<()> {
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
    {
        return Err(ApiError::conflict("请输入不包含路径分隔符的有效名称"));
    }
    // 所有平台都拦：`:` 在 Windows 上会让 parent.join(name) 跳出父目录，
    // 也可能写成 NTFS 备用数据流。在 Linux 上建出这种名字，文件拷到 U 盘同样打不开。
    if let Some(reason) = platform::reject_windows_unsafe_name(name) {
        return Err(ApiError::conflict(reason));
    }
    Ok(())
}

fn copy_path(source: &Path, target: &Path) -> ApiResult<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        return Err(ApiError::forbidden(format!(
            "暂不支持复制符号链接: {}",
            source.display()
        )));
    }
    if metadata.is_dir() {
        std::fs::create_dir(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_path(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(source, target)?;
    }
    Ok(())
}

/// 目录项在 FAT/exFAT 上按写入顺序排列，故事机等设备据此决定播放顺序。
/// 这里在同级新建临时目录，把条目按自然数字顺序逐个 rename 进去，再换回原名，
/// 使目录项顺序与文件名顺序一致。全程同盘 rename，不复制数据。
fn reorder_directory_fat(dir: &Path) -> ApiResult<(usize, bool)> {
    let parent = dir
        .parent()
        .ok_or_else(|| ApiError::forbidden("不能对文件系统根目录执行 FAT 排序"))?;
    let temp = parent.join(".vid2audio-fatsort.tmp");

    // 软链接指向的目录不归这里管，重建时会把链接换成真目录，直接拒绝。
    if std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(ApiError::forbidden("暂不支持对符号链接执行 FAT 排序"));
    }

    // 上次执行中断时，条目可能仍留在临时目录里，先尝试恢复。
    if temp.is_dir() {
        let original_empty = match std::fs::read_dir(dir) {
            Ok(mut entries) => entries.next().is_none(),
            Err(_) => !dir.exists(),
        };
        if dir.exists() && !original_empty {
            return Err(ApiError::conflict(format!(
                "发现上次中断遗留的临时目录，请手动检查后再试: {}",
                temp.display()
            )));
        }
        if dir.exists() {
            std::fs::remove_dir(dir)?;
        }
        std::fs::rename(&temp, dir)?;
        let count = std::fs::read_dir(dir)?.filter_map(Result::ok).count();
        return Ok((count, true));
    }

    if !dir.is_dir() {
        return Err(ApiError::not_found(format!(
            "文件夹不存在: {}",
            dir.display()
        )));
    }

    let mut names: Vec<OsString> = std::fs::read_dir(dir)
        .map_err(|_| ApiError::forbidden(format!("没有权限读取目录: {}", dir.display())))?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    if names.is_empty() {
        return Ok((0, false));
    }
    sort_entry_names(dir, &mut names);

    let permissions = std::fs::metadata(dir).ok().map(|meta| meta.permissions());
    std::fs::create_dir(&temp)?;
    let mut moved: Vec<OsString> = Vec::with_capacity(names.len());
    for name in &names {
        if let Err(error) = std::fs::rename(dir.join(name), temp.join(name)) {
            rollback_fat_sort(dir, &temp, &moved);
            return Err(ApiError::from(format!(
                "FAT 排序失败，已恢复原状: {} ({error})",
                name.to_string_lossy()
            )));
        }
        moved.push(name.clone());
    }
    if let Err(error) = std::fs::remove_dir(dir) {
        rollback_fat_sort(dir, &temp, &moved);
        return Err(ApiError::from(format!("FAT 排序失败，已恢复原状: {error}")));
    }
    if let Err(error) = std::fs::rename(&temp, dir) {
        rollback_fat_sort(dir, &temp, &moved);
        return Err(ApiError::from(format!("FAT 排序失败，已恢复原状: {error}")));
    }
    if let Some(permissions) = permissions {
        let _ = std::fs::set_permissions(dir, permissions);
    }
    Ok((names.len(), false))
}

/// 隐藏文件排在最后，其余按自然数字顺序（2 在 10 之前）。
/// 按自然顺序排列目录项，隐藏条目排到最后。
///
/// 隐藏判断必须走 `platform::is_hidden`：Windows 的隐藏性是文件属性而不是点前缀，
/// 只看点前缀会把 `desktop.ini`、`Thumbs.db` 排进中间，直接打乱故事机的播放顺序。
fn sort_entry_names(dir: &Path, names: &mut [OsString]) {
    names.sort_by(|a, b| {
        let left = a.to_string_lossy();
        let right = b.to_string_lossy();
        platform::is_hidden(&dir.join(a), &left)
            .cmp(&platform::is_hidden(&dir.join(b), &right))
            .then_with(|| compare_names(&left, &right, "natural"))
    });
}

/// 尽力把已移入临时目录的条目搬回原目录，失败时不再向上抛错。
fn rollback_fat_sort(dir: &Path, temp: &Path, moved: &[OsString]) {
    if !dir.exists() {
        let _ = std::fs::create_dir(dir);
    }
    for name in moved.iter().rev() {
        let _ = std::fs::rename(temp.join(name), dir.join(name));
    }
    let _ = std::fs::remove_dir(temp);
}

async fn start_scan(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ScanRequest>,
) -> ApiResult<Json<ScanResult>> {
    let settings = state.db.load_settings().await?;
    let paths = request
        .source_paths
        .filter(|values| !values.is_empty())
        .or_else(|| request.directories.filter(|values| !values.is_empty()))
        .unwrap_or_else(|| settings.scan_directories.clone());
    let scan_settings = settings.clone();
    let (mut collections, warnings) =
        tokio::task::spawn_blocking(move || scan_paths(&paths, &scan_settings)).await?;
    state.db.save_collections(&mut collections).await?;
    Ok(Json(ScanResult {
        scan_id: Uuid::new_v4().to_string(),
        files_found: collections.iter().map(|c| c.episode_count).sum(),
        collections,
        warnings,
    }))
}

async fn list_collections(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<Collection>>> {
    Ok(Json(state.db.list_collections().await?))
}
async fn get_collection(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<Collection>> {
    state
        .db
        .get_collection(&id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("合集不存在"))
}
async fn delete_collection(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let active: i64 = sqlx::query(
        "SELECT COUNT(*) FROM extract_jobs WHERE collection_id=? AND status IN ('queued','processing')",
    )
    .bind(&id)
    .fetch_one(&state.db.pool)
    .await?
    .get(0);
    if active > 0 {
        return Err(ApiError::conflict("合集仍有进行中的提取任务，请先取消任务"));
    }
    if state.db.delete_collection(&id).await? {
        Ok(Json(json!({"deleted": true})))
    } else {
        Err(ApiError::not_found("合集不存在"))
    }
}
async fn rescan_collection(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<ScanResult>> {
    let collection = state
        .db
        .get_collection(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("合集不存在"))?;
    start_scan(
        State(state),
        Json(ScanRequest {
            source_paths: Some(vec![collection.source_path]),
            directories: None,
        }),
    )
    .await
}

async fn create_job(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<ExtractRequest>,
) -> ApiResult<Json<ExtractJob>> {
    let settings = state.db.load_settings().await?;
    request.generate_intro &= settings.tts_enabled;
    if request.intro_voice.is_empty() {
        request.intro_voice = settings.tts_voice.clone();
    }
    if request.tts_provider.is_none() {
        request.tts_provider = Some(settings.tts_provider.clone());
    }
    if request.tts_rate.is_none() {
        request.tts_rate = Some(settings.tts_rate.clone());
    }
    if request.tts_failure_mode.is_none() {
        request.tts_failure_mode = Some(settings.tts_failure_mode.clone());
    }
    if request.filesystem_sorting.is_none() {
        request.filesystem_sorting = Some(settings.filesystem_sorting.clone());
    }
    if request.padding_digits.as_deref().is_none_or(str::is_empty) {
        request.padding_digits = Some(settings.padding_digits.clone());
    }
    let collection = if let Some(source) = request.source_path.clone() {
        let scan_settings = settings.clone();
        let scan_source = source.clone();
        let (mut found, warnings) =
            tokio::task::spawn_blocking(move || scan_paths(&[scan_source], &scan_settings)).await?;
        if found.is_empty() {
            return Ok(Json(
                create_failed_job(
                    &state.db,
                    &request,
                    request.job_name.clone().unwrap_or_else(|| {
                        Path::new(&source)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    }),
                    source,
                    format!("没有找到符合过滤条件的视频文件。{}", warnings.join("; ")),
                )
                .await?,
            ));
        }
        if let Some(name) = &request.job_name {
            found[0].name = name.clone();
        }
        state.db.save_collections(&mut found).await?;
        request.collection_id = Some(found[0].id.clone());
        state
            .db
            .get_collection(&found[0].id)
            .await?
            .unwrap_or_else(|| found.remove(0))
    } else if let Some(id) = request.collection_id.clone() {
        match state.db.get_collection(&id).await? {
            Some(c) => c,
            None => {
                return Ok(Json(
                    create_failed_job(
                        &state.db,
                        &request,
                        request
                            .job_name
                            .clone()
                            .unwrap_or_else(|| "未命名任务".into()),
                        String::new(),
                        "合集不存在或源路径无效".into(),
                    )
                    .await?,
                ));
            }
        }
    } else {
        return Ok(Json(
            create_failed_job(
                &state.db,
                &request,
                "未命名任务".into(),
                String::new(),
                "合集不存在或源路径无效".into(),
            )
            .await?,
        ));
    };
    if request.intro_text.is_none() {
        request.intro_text = Some(settings.intro_text_template.replace(
            "{collection_name}",
            request.job_name.as_deref().unwrap_or(&collection.name),
        ));
    }
    let selected = request
        .selected_video_ids
        .as_ref()
        .filter(|values| !values.is_empty())
        .map(|v| v.iter().collect::<std::collections::HashSet<_>>());
    let videos: Vec<_> = collection
        .video_files
        .iter()
        .filter(|v| selected.as_ref().is_none_or(|ids| ids.contains(&v.id)))
        .collect();
    let job = ExtractJob {
        id: Uuid::new_v4().to_string(),
        collection_id: Some(collection.id.clone()),
        name: request
            .job_name
            .clone()
            .unwrap_or_else(|| collection.name.clone()),
        source_path: collection.source_path.clone(),
        status: "queued".into(),
        output_format: request.output_format.clone(),
        quality_setting: request.quality.clone(),
        selected_track_index: request.track_index,
        trim_start_seconds: request.trim_start_seconds,
        trim_end_seconds: request.trim_end_seconds,
        total_count: videos.len() as i64,
        summary: json!({"warnings": []}),
        ..Default::default()
    };
    state.db.insert_job(&job).await?;
    let items: Vec<_> = videos
        .iter()
        .map(|v| ExtractJobItem {
            id: Uuid::new_v4().to_string(),
            job_id: job.id.clone(),
            video_file_id: Some(v.id.clone()),
            source_path: v.filepath.clone(),
            title: v.episode_title.clone(),
            status: "pending".into(),
            ..Default::default()
        })
        .collect();
    state.db.insert_items(&items).await?;
    state.db.save_job_request(&job.id, &request).await?;
    let response = state.db.get_job(&job.id).await?.unwrap_or(job);
    let worker_db = state.db.clone();
    let worker_job = response.id.clone();
    tokio::spawn(async move {
        extractor::run_job(worker_db, settings, collection, request, worker_job).await;
    });
    Ok(Json(response))
}

async fn create_failed_job(
    db: &Database,
    request: &ExtractRequest,
    name: String,
    source: String,
    message: String,
) -> Result<ExtractJob> {
    let placeholder = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO collections(id,name,source_path,episode_count,status,settings) VALUES(?,?,?,0,'error','{}')").bind(&placeholder).bind(&name).bind(&source).execute(&db.pool).await?;
    let job = ExtractJob {
        id: Uuid::new_v4().to_string(),
        collection_id: Some(placeholder),
        name,
        source_path: source,
        status: "failed".into(),
        progress: 100,
        selected_track_index: request.track_index,
        output_format: request.output_format.clone(),
        quality_setting: request.quality.clone(),
        error_message: Some(message.clone()),
        summary: json!({"failures": [{"error": message}], "success_count": 0, "failure_count": 0, "total_count": 0}),
        ..Default::default()
    };
    db.insert_job(&job).await?;
    sqlx::query("UPDATE extract_jobs SET completed_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(&job.id)
        .execute(&db.pool)
        .await?;
    Ok(db.get_job(&job.id).await?.unwrap_or(job))
}

async fn list_jobs(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<ExtractJob>>> {
    Ok(Json(state.db.list_jobs().await?))
}
async fn get_job(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<ExtractJobDetail>> {
    state
        .db
        .get_job_detail(&id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("任务不存在"))
}
async fn delete_job(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    if state
        .db
        .get_job(&id)
        .await?
        .is_some_and(|job| matches!(job.status.as_str(), "queued" | "processing"))
    {
        return Err(ApiError::conflict("任务仍在运行，请先暂停或取消后再删除"));
    }
    if state.db.delete_job(&id).await? {
        Ok(Json(json!({"deleted": true})))
    } else {
        Err(ApiError::not_found("任务不存在"))
    }
}
async fn cancel_job(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<ExtractJob>> {
    if state.db.get_job(&id).await?.is_none() {
        return Err(ApiError::not_found("任务不存在"));
    }
    sqlx::query(
        "UPDATE extract_jobs SET status='cancelled',completed_at=CURRENT_TIMESTAMP WHERE id=?",
    )
    .bind(&id)
    .execute(&state.db.pool)
    .await?;
    sqlx::query("UPDATE extract_job_items SET status='cancelled',completed_at=CURRENT_TIMESTAMP WHERE job_id=? AND status IN ('pending','processing')")
        .bind(&id)
        .execute(&state.db.pool)
        .await?;
    Ok(Json(state.db.get_job(&id).await?.unwrap()))
}

/// 暂停只改状态，正在跑的 ffmpeg 进程会把当前这个文件提取完再退出，
/// 剩下的明细退回 pending，等「继续」时接着做。
async fn pause_job(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<ExtractJob>> {
    let job = state
        .db
        .get_job(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("任务不存在"))?;
    if !matches!(job.status.as_str(), "queued" | "processing") {
        return Err(ApiError::conflict("只有排队中或进行中的任务可以暂停"));
    }
    sqlx::query("UPDATE extract_jobs SET status='paused' WHERE id=?")
        .bind(&id)
        .execute(&state.db.pool)
        .await?;
    Ok(Json(state.db.get_job(&id).await?.unwrap()))
}

/// 继续：用创建任务时存下的参数重新排队，已完成的明细不会重做。
async fn resume_job(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<ExtractJob>> {
    let job = state
        .db
        .get_job(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("任务不存在"))?;
    if !matches!(job.status.as_str(), "paused" | "failed" | "cancelled") {
        return Err(ApiError::conflict("只有已暂停、已取消或失败的任务可以继续"));
    }
    let request = state
        .db
        .load_job_request(&id)
        .await?
        .ok_or_else(|| ApiError::conflict("该任务缺少原始参数，无法继续，请重新创建任务"))?;
    let collection_id = job
        .collection_id
        .clone()
        .or_else(|| request.collection_id.clone())
        .ok_or_else(|| ApiError::conflict("任务关联的合集已被删除，无法继续"))?;
    let collection = state
        .db
        .get_collection(&collection_id)
        .await?
        .ok_or_else(|| ApiError::conflict("任务关联的合集已被删除，无法继续"))?;
    let settings = state.db.load_settings().await?;
    sqlx::query("UPDATE extract_job_items SET status='pending',error_message=NULL,started_at=NULL,completed_at=NULL,duration_seconds=NULL WHERE job_id=? AND status!='completed'")
        .bind(&id)
        .execute(&state.db.pool)
        .await?;
    sqlx::query(
        "UPDATE extract_jobs SET status='queued',error_message=NULL,completed_at=NULL WHERE id=?",
    )
    .bind(&id)
    .execute(&state.db.pool)
    .await?;
    let worker_db = state.db.clone();
    let worker_job = id.clone();
    tokio::spawn(async move {
        extractor::run_job(worker_db, settings, collection, request, worker_job).await;
    });
    Ok(Json(state.db.get_job(&id).await?.unwrap()))
}

async fn job_audio(
    State(state): State<Arc<AppState>>,
    AxumPath((job_id, item_id)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let detail = state
        .db
        .get_job_detail(&job_id)
        .await?
        .ok_or_else(|| ApiError::not_found("任务不存在"))?;
    let item = detail
        .items
        .into_iter()
        .find(|v| v.id == item_id)
        .ok_or_else(|| ApiError::not_found("任务文件不存在"))?;
    if item.status != "completed" {
        return Err(ApiError::conflict("音频文件尚未生成"));
    }
    stream_file(
        PathBuf::from(
            item.output_path
                .ok_or_else(|| ApiError::not_found("音频文件不存在"))?,
        ),
        headers,
    )
    .await
}

#[derive(Deserialize)]
struct PreviewQuery {
    #[serde(default)]
    track: i64,
    #[serde(default = "default_preview_duration")]
    duration: i64,
    #[serde(default)]
    start: f64,
}
fn default_preview_duration() -> i64 {
    10
}
async fn preview_audio(
    State(state): State<Arc<AppState>>,
    AxumPath(video_id): AxumPath<String>,
    Query(query): Query<PreviewQuery>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let row = sqlx::query("SELECT filepath FROM video_files WHERE id=?")
        .bind(&video_id)
        .fetch_optional(&state.db.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("视频不存在"))?;
    let source: String = row.get(0);
    // 用系统临时目录：硬编码 /tmp 在 Windows 上会解析成 <当前盘>:\tmp，
    // 在 C 盘根制造垃圾目录。
    let output = std::env::temp_dir()
        .join("vid2audio")
        .join(format!("preview_{video_id}_{}.mp3", query.track));
    extractor::preview(&source, query.track, &output, query.duration, query.start).await?;
    stream_file(output, headers).await
}

async fn stream_file(path: PathBuf, headers: HeaderMap) -> ApiResult<Response> {
    if !path.is_file() {
        return Err(ApiError::not_found("音频文件不存在"));
    }
    let mut request = Request::new(Body::empty());
    *request.headers_mut() = headers;
    let response = ServeFile::new(path).oneshot(request).await?;
    Ok(response.map(Body::new))
}
async fn system_status(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let settings = state.db.load_settings().await?;
    Ok(Json(
        json!({"version": env!("CARGO_PKG_VERSION"), "ffmpeg_available": media::command_available("ffmpeg"), "ffprobe_available": media::command_available("ffprobe"), "database_path": state.db.path, "input_directories": settings.scan_directories, "output_directory": settings.output_directory}),
    ))
}
fn canonical(path: &Path) -> String {
    platform::strip_extended_prefix(
        &path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy(),
    )
}

fn expand_home(path: &Path) -> PathBuf {
    platform::expand_home(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn settings_routes_use_the_compatible_json_shape() {
        let root = std::env::temp_dir().join(format!("vid2audio-api-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let db = Database::open(root.join("test.db")).await.unwrap();
        let app = router(AppState { db }, root.join("static"));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"min_file_size_mb":2.5}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/settings")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: AppSettings =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body.min_file_size_mb, 2.5);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn audio_stream_supports_http_ranges() {
        let root = std::env::temp_dir().join(format!("vid2audio-range-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let audio = root.join("sample.mp3");
        tokio::fs::write(&audio, b"0123456789").await.unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("range", "bytes=2-5".parse().unwrap());
        let response = stream_file(audio, headers).await.unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            "2345"
        );
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn archive_to_path_writes_a_readable_zip_and_refuses_saving_inside_the_source() {
        let root = std::env::temp_dir().join(format!("vid2audio-archive-{}", Uuid::new_v4()));
        let source = root.join("合集");
        std::fs::create_dir_all(source.join("bonus")).unwrap();
        std::fs::write(source.join("001_植树节.mp3"), b"first").unwrap();
        std::fs::write(source.join("bonus/extra.mp3"), b"extra").unwrap();

        let destination = root.join("导出.zip");
        let response = archive_to_path(Json(ArchiveToRequest {
            path: source.to_string_lossy().into_owned(),
            destination: destination.to_string_lossy().into_owned(),
        }))
        .await
        .unwrap();
        assert_eq!(response.0["success"], json!(true));
        assert!(response.0["size"].as_u64().unwrap() > 0);

        // 直接写文件走的是 `Write + Seek` 的另一条路径（原来只用过 `Cursor`），
        // 所以要真读回来确认 zip 目录结构没写坏。
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&destination).unwrap()).unwrap();
        let mut names: Vec<String> = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "合集/",
                "合集/001_植树节.mp3",
                "合集/bonus/",
                "合集/bonus/extra.mp3",
            ]
        );

        // 存到正在打包的目录里会把半个 zip 打进自己，必须拦掉。
        let error = archive_to_path(Json(ArchiveToRequest {
            path: source.to_string_lossy().into_owned(),
            destination: source.join("inside.zip").to_string_lossy().into_owned(),
        }))
        .await
        .unwrap_err();
        assert_eq!(error.status, StatusCode::FORBIDDEN);
        assert!(!source.join("inside.zip").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fat_sort_writes_entries_in_natural_order() {
        // 写入顺序就是 FAT 目录项顺序，所以排序结果本身是这个功能的核心契约。
        // （read_dir 的返回顺序只在 FAT/exFAT 上等于写入顺序，ext4 会走哈希，故不断言它。）
        let mut names: Vec<OsString> = ["10.mp3", "2.mp3", ".hidden", "1.mp3", "bonus", "20.mp3"]
            .into_iter()
            .map(OsString::from)
            .collect();
        // 这些条目并不真实存在，is_hidden 取不到属性时按点前缀判断，正是这里要的语义。
        sort_entry_names(Path::new("/nonexistent"), &mut names);
        assert_eq!(
            names
                .iter()
                .map(|n| n.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["1.mp3", "2.mp3", "10.mp3", "20.mp3", "bonus", ".hidden"]
        );
    }

    #[test]
    fn fat_sort_preserves_every_entry() {
        let root = std::env::temp_dir().join(format!("vid2audio-fatsort-{}", Uuid::new_v4()));
        let target = root.join("合集");
        std::fs::create_dir_all(&target).unwrap();
        for name in ["10.mp3", "2.mp3", "1.mp3", "20.mp3", "3.mp3"] {
            std::fs::write(target.join(name), name.as_bytes()).unwrap();
        }
        std::fs::create_dir(target.join("bonus")).unwrap();
        std::fs::write(target.join("bonus/extra.mp3"), b"extra").unwrap();

        let (count, recovered) = reorder_directory_fat(&target).unwrap();
        assert_eq!(count, 6);
        assert!(!recovered);

        let mut remaining: Vec<String> = std::fs::read_dir(&target)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        remaining.sort();
        assert_eq!(
            remaining,
            vec!["1.mp3", "10.mp3", "2.mp3", "20.mp3", "3.mp3", "bonus"]
        );
        // 文件内容与子目录完整保留，临时目录已清理。
        assert_eq!(
            std::fs::read_to_string(target.join("bonus/extra.mp3")).unwrap(),
            "extra"
        );
        assert_eq!(
            std::fs::read_to_string(target.join("10.mp3")).unwrap(),
            "10.mp3"
        );
        assert!(!root.join(".vid2audio-fatsort.tmp").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fat_sort_recovers_an_interrupted_run() {
        let root = std::env::temp_dir().join(format!("vid2audio-fatsort-crash-{}", Uuid::new_v4()));
        let target = root.join("合集");
        std::fs::create_dir_all(&target).unwrap();
        // 模拟中断：条目已全部移入临时目录，原目录空着还没被替换。
        let temp = root.join(".vid2audio-fatsort.tmp");
        std::fs::create_dir(&temp).unwrap();
        std::fs::write(temp.join("1.mp3"), b"one").unwrap();
        std::fs::write(temp.join("2.mp3"), b"two").unwrap();

        let (count, recovered) = reorder_directory_fat(&target).unwrap();
        assert_eq!(count, 2);
        assert!(recovered);
        assert!(!temp.exists());
        assert_eq!(
            std::fs::read_to_string(target.join("1.mp3")).unwrap(),
            "one"
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fat_sort_refuses_when_the_original_directory_still_has_files() {
        let root = std::env::temp_dir().join(format!("vid2audio-fatsort-busy-{}", Uuid::new_v4()));
        let target = root.join("合集");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.mp3"), b"keep").unwrap();
        let temp = root.join(".vid2audio-fatsort.tmp");
        std::fs::create_dir(&temp).unwrap();
        std::fs::write(temp.join("stray.mp3"), b"stray").unwrap();

        // 两边都有文件时无法判断哪份是完整的，必须报错而不是删数据。
        let error = reorder_directory_fat(&target).unwrap_err();
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert!(target.join("keep.mp3").exists());
        assert!(temp.join("stray.mp3").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fat_sort_moves_nothing_when_the_temp_path_is_blocked() {
        let root =
            std::env::temp_dir().join(format!("vid2audio-fatsort-blocked-{}", Uuid::new_v4()));
        let target = root.join("合集");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("1.mp3"), b"one").unwrap();
        std::fs::write(target.join("2.mp3"), b"two").unwrap();
        // 同名普通文件挡住临时目录，create_dir 必须先失败，此时一个文件都不能被搬走。
        std::fs::write(root.join(".vid2audio-fatsort.tmp"), b"blocker").unwrap();

        assert!(reorder_directory_fat(&target).is_err());
        let mut remaining: Vec<String> = std::fs::read_dir(&target)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        remaining.sort();
        assert_eq!(remaining, vec!["1.mp3", "2.mp3"]);

        std::fs::remove_dir_all(root).unwrap();
    }
}
