use crate::{
    db::Database, extractor, media, models::*, scanner::scan_paths, sorter::compare_names,
};
use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::Row;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tower::ServiceExt;
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
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
        .route(
            "/extract/jobs/{job_id}/items/{item_id}/audio",
            get(job_audio),
        )
        .route("/preview/{video_id}", get(preview_audio))
        .route("/system/status", get(system_status))
        .route("/system/hardware-acceleration", get(hardware_acceleration))
        .route(
            "/system/hardware-acceleration/detect",
            post(hardware_acceleration),
        );
    Router::new()
        .nest("/api/v1", api)
        .nest_service("/static", ServeDir::new(&static_dir))
        .route_service("/", ServeFile::new(static_dir.join("index.html")))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
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
            .unwrap_or_else(|| "/app/input".into()),
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
        (
            std::env::current_dir()?,
            format!("路径不存在，已打开当前工作目录: {}", requested.display()),
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
        if name.starts_with('.') {
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
    Ok(Json(
        json!({"path": canonical(&current), "requested_path": requested.to_string_lossy(), "parent": current.parent().map(canonical), "warning": warning, "sorting": settings.filesystem_sorting, "entries": entries}),
    ))
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
        return Err(ApiError::conflict("任务仍在运行，请先取消后再删除"));
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
    let settings = state.db.load_settings().await?;
    let output =
        PathBuf::from("/tmp/vid2audio").join(format!("preview_{video_id}_{}.mp3", query.track));
    extractor::preview(
        &source,
        query.track,
        &output,
        query.duration,
        query.start,
        &settings,
    )
    .await?;
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
async fn hardware_acceleration() -> Json<Value> {
    Json(media::detect_hardware_acceleration())
}
async fn system_status(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let settings = state.db.load_settings().await?;
    Ok(Json(
        json!({"version": env!("CARGO_PKG_VERSION"), "ffmpeg_available": media::command_available("ffmpeg"), "ffprobe_available": media::command_available("ffprobe"), "database_path": state.db.path, "input_directories": settings.scan_directories, "output_directory": settings.output_directory, "hardware_acceleration": media::detect_hardware_acceleration()}),
    ))
}
fn canonical(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn expand_home(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    path.to_path_buf()
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
}
