use crate::models::{
    AppSettings, AudioTrack, Collection, ExtractJob, ExtractJobDetail, ExtractJobItem,
    ExtractRequest, VideoFile,
};
use anyhow::Result;
use serde_json::{Map, Value};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{path::Path, time::Duration};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS collections (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, source_path TEXT NOT NULL, output_path TEXT,
 episode_count INTEGER, status TEXT DEFAULT 'pending', created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
 updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, settings TEXT DEFAULT '{}');
CREATE TABLE IF NOT EXISTS video_files (
 id TEXT PRIMARY KEY, collection_id TEXT NOT NULL, filename TEXT NOT NULL, filepath TEXT NOT NULL,
 file_size INTEGER, duration REAL, resolution TEXT, video_codec TEXT, episode_number INTEGER,
 episode_title TEXT, status TEXT DEFAULT 'pending', created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
 FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS audio_tracks (
 id TEXT PRIMARY KEY, video_file_id TEXT NOT NULL, track_index INTEGER NOT NULL, codec TEXT,
 language TEXT, language_full TEXT, channels INTEGER, sample_rate INTEGER, bitrate INTEGER,
 title TEXT, is_default INTEGER DEFAULT 0,
 FOREIGN KEY (video_file_id) REFERENCES video_files(id) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS extract_jobs (
 id TEXT PRIMARY KEY, collection_id TEXT, name TEXT, source_path TEXT, status TEXT DEFAULT 'queued',
 progress INTEGER DEFAULT 0, current_file TEXT, selected_track_index INTEGER, output_format TEXT,
 quality_setting TEXT, trim_start_seconds REAL DEFAULT 0, trim_end_seconds REAL DEFAULT 0,
 total_count INTEGER DEFAULT 0, success_count INTEGER DEFAULT 0, failure_count INTEGER DEFAULT 0,
 error_message TEXT, output_path TEXT, summary TEXT DEFAULT '{}', request TEXT,
 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
 started_at TIMESTAMP, completed_at TIMESTAMP, FOREIGN KEY (collection_id) REFERENCES collections(id));
CREATE TABLE IF NOT EXISTS extract_job_items (
 id TEXT PRIMARY KEY, job_id TEXT NOT NULL, video_file_id TEXT, source_path TEXT NOT NULL,
 output_path TEXT, title TEXT, status TEXT DEFAULT 'pending', error_message TEXT,
 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, started_at TIMESTAMP, completed_at TIMESTAMP,
 duration_seconds REAL,
 FOREIGN KEY (job_id) REFERENCES extract_jobs(id) ON DELETE CASCADE,
 FOREIGN KEY (video_file_id) REFERENCES video_files(id));
CREATE TABLE IF NOT EXISTS settings (
 key TEXT PRIMARY KEY, value TEXT, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP);
"#;

#[derive(Clone)]
pub struct Database {
    pub pool: SqlitePool,
    pub path: String,
}

impl Database {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let path_string = path.to_string_lossy().into_owned();
        // 用 filename() 而不是拼 "sqlite://" URL：URL 形式会按第一个 `?` 切 query，
        // 并对路径做 percent-decode，Windows 上 `D:\影片100%版\` 这种目录会被解错。
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(15))
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await?;
        for statement in SCHEMA.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(statement).execute(&pool).await?;
        }
        Self::migrate(&pool).await?;
        Ok(Self {
            pool,
            path: path_string,
        })
    }

    async fn migrate(pool: &SqlitePool) -> Result<()> {
        let rows = sqlx::query("PRAGMA table_info(extract_jobs)")
            .fetch_all(pool)
            .await?;
        let existing: std::collections::HashSet<String> =
            rows.iter().map(|row| row.get("name")).collect();
        for (name, definition) in [
            ("name", "TEXT"),
            ("source_path", "TEXT"),
            ("trim_start_seconds", "REAL DEFAULT 0"),
            ("trim_end_seconds", "REAL DEFAULT 0"),
            ("total_count", "INTEGER DEFAULT 0"),
            ("success_count", "INTEGER DEFAULT 0"),
            ("failure_count", "INTEGER DEFAULT 0"),
            ("summary", "TEXT DEFAULT '{}'"),
            ("request", "TEXT"),
        ] {
            if !existing.contains(name) {
                sqlx::query(&format!(
                    "ALTER TABLE extract_jobs ADD COLUMN {name} {definition}"
                ))
                .execute(pool)
                .await?;
            }
        }
        let item_rows = sqlx::query("PRAGMA table_info(extract_job_items)")
            .fetch_all(pool)
            .await?;
        let item_columns: std::collections::HashSet<String> =
            item_rows.iter().map(|row| row.get("name")).collect();
        for (name, definition) in [("started_at", "TIMESTAMP"), ("duration_seconds", "REAL")] {
            if !item_columns.contains(name) {
                sqlx::query(&format!(
                    "ALTER TABLE extract_job_items ADD COLUMN {name} {definition}"
                ))
                .execute(pool)
                .await?;
            }
        }
        sqlx::query(
            "DELETE FROM settings WHERE key IN ('hardware_acceleration', 'hardware_acceleration_device', 'hardware_acceleration_fallback')",
        )
        .execute(pool)
        .await?;
        Self::clear_dangling_references(pool).await?;
        Self::recover_interrupted_jobs(pool).await?;
        Ok(())
    }

    /// 提取任务只活在进程内存里，容器重启或崩溃后没有工作线程再去推进它们，
    /// 但库里的状态还停在 queued/processing——这类任务在界面上既不前进也删不掉。
    /// 启动时把它们改成 paused，用户可以「继续」重新排队，也可以直接删除。
    async fn recover_interrupted_jobs(pool: &SqlitePool) -> Result<()> {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "UPDATE extract_job_items SET status='pending',started_at=NULL,duration_seconds=NULL WHERE status='processing'",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE extract_jobs SET status='paused',current_file=NULL WHERE status IN ('queued','processing')",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// 老版本或异常退出可能留下父行已删、子行还在的悬空引用。
    /// 开启 foreign_keys 后这些行会让删除任务/合集报 FOREIGN KEY constraint failed，启动时清掉。
    /// 顺序很重要：先把引用置空或删掉，再删被引用的行。
    async fn clear_dangling_references(pool: &SqlitePool) -> Result<()> {
        let mut tx = pool.begin().await?;
        for statement in [
            "UPDATE extract_job_items SET video_file_id=NULL WHERE video_file_id IS NOT NULL AND video_file_id NOT IN (SELECT id FROM video_files WHERE collection_id IN (SELECT id FROM collections))",
            "DELETE FROM audio_tracks WHERE video_file_id NOT IN (SELECT id FROM video_files WHERE collection_id IN (SELECT id FROM collections))",
            "DELETE FROM video_files WHERE collection_id NOT IN (SELECT id FROM collections)",
            "DELETE FROM extract_job_items WHERE job_id NOT IN (SELECT id FROM extract_jobs)",
            "UPDATE extract_jobs SET collection_id=NULL WHERE collection_id IS NOT NULL AND collection_id NOT IN (SELECT id FROM collections)",
        ] {
            sqlx::query(statement).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn load_settings(&self) -> Result<AppSettings> {
        let rows = sqlx::query("SELECT key, value FROM settings")
            .fetch_all(&self.pool)
            .await?;
        let mut object = serde_json::to_value(AppSettings::default())?
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut persisted = std::collections::HashSet::new();
        for row in rows {
            let key: String = row.get("key");
            let raw: String = row.get("value");
            object.insert(
                key.clone(),
                serde_json::from_str(&raw).unwrap_or(Value::String(raw)),
            );
            persisted.insert(key);
        }
        if !persisted.contains("scan_directories")
            && let Ok(value) = std::env::var("VID2AUDIO_INPUT")
        {
            object.insert("scan_directories".into(), serde_json::json!([value]));
        }
        if !persisted.contains("output_directory")
            && let Ok(value) = std::env::var("VID2AUDIO_OUTPUT")
        {
            object.insert("output_directory".into(), Value::String(value));
        }
        if !persisted.contains("extraction_concurrency")
            && let Ok(value) = std::env::var("VID2AUDIO_EXTRACTION_CONCURRENCY")
            && let Ok(value) = value.parse::<i64>()
        {
            object.insert("extraction_concurrency".into(), Value::Number(value.into()));
        }
        let mut settings: AppSettings = serde_json::from_value(Value::Object(object))?;
        settings.extraction_concurrency = settings.extraction_concurrency.clamp(1, 32);
        Ok(settings)
    }

    pub async fn update_settings(&self, patch: Map<String, Value>) -> Result<AppSettings> {
        let mut current = serde_json::to_value(self.load_settings().await?)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        current.extend(patch);
        let mut settings: AppSettings = serde_json::from_value(Value::Object(current))?;
        settings.extraction_concurrency = settings.extraction_concurrency.clamp(1, 32);
        let values = serde_json::to_value(&settings)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut tx = self.pool.begin().await?;
        for (key, value) in values {
            sqlx::query("INSERT INTO settings(key,value,updated_at) VALUES(?,?,CURRENT_TIMESTAMP) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=CURRENT_TIMESTAMP").bind(key).bind(serde_json::to_string(&value)?).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(settings)
    }

    pub async fn save_collections(&self, collections: &mut [Collection]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for collection in collections {
            let existing =
                sqlx::query("SELECT id, output_path FROM collections WHERE source_path=?")
                    .bind(&collection.source_path)
                    .fetch_optional(&mut *tx)
                    .await?;
            if let Some(row) = existing {
                collection.id = row.get("id");
                collection.output_path = collection.output_path.take().or_else(|| {
                    row.try_get::<Option<String>, _>("output_path")
                        .unwrap_or(None)
                });
                for video in &mut collection.video_files {
                    video.collection_id = Some(collection.id.clone());
                }
                sqlx::query("UPDATE extract_job_items SET video_file_id=NULL WHERE video_file_id IN (SELECT id FROM video_files WHERE collection_id=?)").bind(&collection.id).execute(&mut *tx).await?;
                sqlx::query("DELETE FROM audio_tracks WHERE video_file_id IN (SELECT id FROM video_files WHERE collection_id=?)").bind(&collection.id).execute(&mut *tx).await?;
                sqlx::query("DELETE FROM video_files WHERE collection_id=?")
                    .bind(&collection.id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE collections SET name=?,output_path=?,episode_count=?,status=?,settings=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
                    .bind(&collection.name).bind(&collection.output_path).bind(collection.episode_count).bind(&collection.status).bind(serde_json::to_string(&collection.settings)?).bind(&collection.id).execute(&mut *tx).await?;
            } else {
                sqlx::query("INSERT INTO collections(id,name,source_path,output_path,episode_count,status,settings) VALUES(?,?,?,?,?,?,?)")
                    .bind(&collection.id).bind(&collection.name).bind(&collection.source_path).bind(&collection.output_path).bind(collection.episode_count).bind(&collection.status).bind(serde_json::to_string(&collection.settings)?).execute(&mut *tx).await?;
            }
            for video in &collection.video_files {
                sqlx::query("INSERT INTO video_files(id,collection_id,filename,filepath,file_size,duration,resolution,video_codec,episode_number,episode_title,status) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                    .bind(&video.id).bind(&collection.id).bind(&video.filename).bind(&video.filepath).bind(video.file_size).bind(video.duration).bind(&video.resolution).bind(&video.video_codec).bind(video.episode_number).bind(&video.episode_title).bind(&video.status).execute(&mut *tx).await?;
                for track in &video.audio_tracks {
                    sqlx::query("INSERT INTO audio_tracks(id,video_file_id,track_index,codec,language,language_full,channels,sample_rate,bitrate,title,is_default) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                        .bind(track.id.as_deref().unwrap_or_default()).bind(&video.id).bind(track.index).bind(&track.codec).bind(&track.language).bind(&track.language_full).bind(track.channels).bind(track.sample_rate).bind(track.bitrate).bind(&track.title).bind(track.is_default as i64).execute(&mut *tx).await?;
                }
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_collections(&self) -> Result<Vec<Collection>> {
        let rows = sqlx::query("SELECT * FROM collections ORDER BY updated_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(collection_from_row).collect())
    }

    pub async fn get_collection(&self, id: &str) -> Result<Option<Collection>> {
        let Some(row) = sqlx::query("SELECT * FROM collections WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };
        let mut collection = collection_from_row(&row);
        let videos = sqlx::query(
            "SELECT * FROM video_files WHERE collection_id=? ORDER BY episode_number,filename",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        for row in videos {
            let video_id: String = row.get("id");
            let tracks = sqlx::query(
                "SELECT * FROM audio_tracks WHERE video_file_id=? ORDER BY track_index",
            )
            .bind(&video_id)
            .fetch_all(&self.pool)
            .await?;
            collection.video_files.push(VideoFile {
                id: video_id.clone(),
                collection_id: row
                    .try_get::<Option<String>, _>("collection_id")
                    .unwrap_or(None),
                filename: row.get("filename"),
                filepath: row.get("filepath"),
                file_size: row.try_get("file_size").unwrap_or(0),
                duration: row.try_get::<Option<f64>, _>("duration").unwrap_or(None),
                resolution: row.try_get("resolution").unwrap_or_default(),
                video_codec: row.try_get("video_codec").unwrap_or_default(),
                episode_number: row.try_get("episode_number").unwrap_or(0),
                episode_title: row.try_get("episode_title").unwrap_or_default(),
                status: row.try_get("status").unwrap_or_else(|_| "pending".into()),
                audio_tracks: tracks.iter().map(audio_track_from_row).collect(),
            });
        }
        Ok(Some(collection))
    }

    /// 逐层显式删除子表，不依赖 ON DELETE CASCADE：老版本建的库可能没有级联，
    /// 直接删父行会报 FOREIGN KEY constraint failed。
    pub async fn delete_collection(&self, id: &str) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE extract_jobs SET collection_id=NULL WHERE collection_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE extract_job_items SET video_file_id=NULL WHERE video_file_id IN (SELECT id FROM video_files WHERE collection_id=?)").bind(id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM audio_tracks WHERE video_file_id IN (SELECT id FROM video_files WHERE collection_id=?)").bind(id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM video_files WHERE collection_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM collections WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn insert_job(&self, job: &ExtractJob) -> Result<()> {
        sqlx::query("INSERT INTO extract_jobs(id,collection_id,name,source_path,status,progress,selected_track_index,output_format,quality_setting,trim_start_seconds,trim_end_seconds,total_count,summary) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&job.id).bind(&job.collection_id).bind(&job.name).bind(&job.source_path).bind(&job.status).bind(job.progress).bind(job.selected_track_index).bind(&job.output_format).bind(&job.quality_setting).bind(job.trim_start_seconds).bind(job.trim_end_seconds).bind(job.total_count).bind(job.summary.to_string()).execute(&self.pool).await?;
        Ok(())
    }

    /// 保存创建任务时用的完整参数，「继续」时按原样重新排队，
    /// 不然重启后就没法还原音轨、裁剪、输出目录这些选项了。
    pub async fn save_job_request(&self, id: &str, request: &ExtractRequest) -> Result<()> {
        sqlx::query("UPDATE extract_jobs SET request=? WHERE id=?")
            .bind(serde_json::to_string(request)?)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_job_request(&self, id: &str) -> Result<Option<ExtractRequest>> {
        Ok(sqlx::query("SELECT request FROM extract_jobs WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .and_then(|row| row.try_get::<Option<String>, _>("request").unwrap_or(None))
            .and_then(|value| serde_json::from_str(&value).ok()))
    }

    pub async fn insert_items(&self, items: &[ExtractJobItem]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for item in items {
            sqlx::query("INSERT INTO extract_job_items(id,job_id,video_file_id,source_path,title,status) VALUES(?,?,?,?,?,?)").bind(&item.id).bind(&item.job_id).bind(&item.video_file_id).bind(&item.source_path).bind(&item.title).bind(&item.status).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_jobs(&self) -> Result<Vec<ExtractJob>> {
        let rows = sqlx::query("SELECT * FROM extract_jobs ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(job_from_row).collect())
    }
    pub async fn get_job(&self, id: &str) -> Result<Option<ExtractJob>> {
        Ok(sqlx::query("SELECT * FROM extract_jobs WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .as_ref()
            .map(job_from_row))
    }
    pub async fn get_job_detail(&self, id: &str) -> Result<Option<ExtractJobDetail>> {
        let Some(job) = self.get_job(id).await? else {
            return Ok(None);
        };
        let rows =
            sqlx::query("SELECT * FROM extract_job_items WHERE job_id=? ORDER BY created_at,title")
                .bind(id)
                .fetch_all(&self.pool)
                .await?;
        Ok(Some(ExtractJobDetail {
            job,
            items: rows.iter().map(item_from_row).collect(),
        }))
    }
    pub async fn delete_job(&self, id: &str) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM extract_job_items WHERE job_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM extract_jobs WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }
}

fn collection_from_row(row: &sqlx::sqlite::SqliteRow) -> Collection {
    Collection {
        id: row.get("id"),
        name: row.get("name"),
        source_path: row.get("source_path"),
        output_path: row
            .try_get::<Option<String>, _>("output_path")
            .unwrap_or(None),
        episode_count: row.try_get("episode_count").unwrap_or(0),
        status: row.try_get("status").unwrap_or_else(|_| "pending".into()),
        created_at: row
            .try_get::<Option<String>, _>("created_at")
            .unwrap_or(None),
        updated_at: row
            .try_get::<Option<String>, _>("updated_at")
            .unwrap_or(None),
        settings: row
            .try_get::<String, _>("settings")
            .ok()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default(),
        ..Default::default()
    }
}
fn audio_track_from_row(row: &sqlx::sqlite::SqliteRow) -> AudioTrack {
    AudioTrack {
        id: row.try_get::<Option<String>, _>("id").unwrap_or(None),
        video_file_id: row
            .try_get::<Option<String>, _>("video_file_id")
            .unwrap_or(None),
        index: row.get("track_index"),
        codec: row.try_get("codec").unwrap_or_default(),
        language: row.try_get("language").unwrap_or_else(|_| "und".into()),
        language_full: row
            .try_get("language_full")
            .unwrap_or_else(|_| "未知语言".into()),
        channels: row.try_get::<Option<i64>, _>("channels").unwrap_or(None),
        sample_rate: row.try_get::<Option<i64>, _>("sample_rate").unwrap_or(None),
        bitrate: row.try_get::<Option<i64>, _>("bitrate").unwrap_or(None),
        title: row.try_get("title").unwrap_or_default(),
        is_default: row.try_get::<i64, _>("is_default").unwrap_or(0) != 0,
    }
}
fn job_from_row(row: &sqlx::sqlite::SqliteRow) -> ExtractJob {
    ExtractJob {
        id: row.get("id"),
        collection_id: row
            .try_get::<Option<String>, _>("collection_id")
            .unwrap_or(None),
        name: row.try_get("name").unwrap_or_default(),
        source_path: row.try_get("source_path").unwrap_or_default(),
        status: row.try_get("status").unwrap_or_else(|_| "queued".into()),
        progress: row.try_get("progress").unwrap_or(0),
        current_file: row
            .try_get::<Option<String>, _>("current_file")
            .unwrap_or(None),
        selected_track_index: row.try_get("selected_track_index").unwrap_or(0),
        output_format: row
            .try_get("output_format")
            .unwrap_or_else(|_| "mp3".into()),
        quality_setting: row
            .try_get("quality_setting")
            .unwrap_or_else(|_| "standard".into()),
        trim_start_seconds: row.try_get("trim_start_seconds").unwrap_or(0.0),
        trim_end_seconds: row.try_get("trim_end_seconds").unwrap_or(0.0),
        total_count: row.try_get("total_count").unwrap_or(0),
        success_count: row.try_get("success_count").unwrap_or(0),
        failure_count: row.try_get("failure_count").unwrap_or(0),
        error_message: row
            .try_get::<Option<String>, _>("error_message")
            .unwrap_or(None),
        output_path: row
            .try_get::<Option<String>, _>("output_path")
            .unwrap_or(None),
        summary: row
            .try_get::<String, _>("summary")
            .ok()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_else(|| serde_json::json!({})),
        created_at: row
            .try_get::<Option<String>, _>("created_at")
            .unwrap_or(None),
        started_at: row
            .try_get::<Option<String>, _>("started_at")
            .unwrap_or(None),
        completed_at: row
            .try_get::<Option<String>, _>("completed_at")
            .unwrap_or(None),
    }
}
fn item_from_row(row: &sqlx::sqlite::SqliteRow) -> ExtractJobItem {
    ExtractJobItem {
        id: row.get("id"),
        job_id: row.get("job_id"),
        video_file_id: row
            .try_get::<Option<String>, _>("video_file_id")
            .unwrap_or(None),
        source_path: row.get("source_path"),
        output_path: row
            .try_get::<Option<String>, _>("output_path")
            .unwrap_or(None),
        title: row.try_get("title").unwrap_or_default(),
        status: row.try_get("status").unwrap_or_else(|_| "pending".into()),
        error_message: row
            .try_get::<Option<String>, _>("error_message")
            .unwrap_or(None),
        created_at: row
            .try_get::<Option<String>, _>("created_at")
            .unwrap_or(None),
        started_at: row
            .try_get::<Option<String>, _>("started_at")
            .unwrap_or(None),
        completed_at: row
            .try_get::<Option<String>, _>("completed_at")
            .unwrap_or(None),
        duration_seconds: row.try_get("duration_seconds").unwrap_or(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按当前建表语句造一个老版本数据库：去掉所有 ON DELETE CASCADE 和后加的列。
    /// `Database::open` 用的是 CREATE TABLE IF NOT EXISTS，这些老表会原样保留下来，
    /// 正好复现「删除过期任务报 FOREIGN KEY constraint failed」。
    async fn create_legacy(path: &Path, rows: &[&str]) {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(false);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let legacy = SCHEMA
            .replace(" ON DELETE CASCADE", "")
            .replace(" request TEXT,", "");
        for statement in legacy
            .split(';')
            .chain(rows.iter().copied())
            .map(str::trim)
            .filter(|statement| !statement.is_empty())
        {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool.close().await;
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("vid2audio-db-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    async fn count(db: &Database, sql: &str) -> i64 {
        sqlx::query(sql).fetch_one(&db.pool).await.unwrap().get(0)
    }

    const SAMPLE_ROWS: &[&str] = &[
        "INSERT INTO collections(id,name,source_path) VALUES('c1','合集','/videos')",
        "INSERT INTO video_files(id,collection_id,filename,filepath) VALUES('v1','c1','a.mp4','/videos/a.mp4')",
        "INSERT INTO audio_tracks(id,video_file_id,track_index) VALUES('t1','v1',1)",
        "INSERT INTO extract_jobs(id,collection_id,name) VALUES('j1','c1','旧任务')",
        "INSERT INTO extract_job_items(id,job_id,video_file_id,source_path) VALUES('i1','j1','v1','/videos/a.mp4')",
    ];

    #[tokio::test]
    async fn deleting_a_job_works_without_cascades() {
        let root = temp_root("job");
        let path = root.join("legacy.db");
        create_legacy(&path, SAMPLE_ROWS).await;
        let db = Database::open(&path).await.unwrap();

        assert!(db.delete_job("j1").await.unwrap());
        assert_eq!(count(&db, "SELECT COUNT(*) FROM extract_jobs").await, 0);
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM extract_job_items").await,
            0
        );
        // 只删任务，合集和视频记录保持原样。
        assert_eq!(count(&db, "SELECT COUNT(*) FROM video_files").await, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn deleting_a_collection_works_without_cascades() {
        let root = temp_root("collection");
        let path = root.join("legacy.db");
        create_legacy(&path, SAMPLE_ROWS).await;
        let db = Database::open(&path).await.unwrap();

        assert!(db.delete_collection("c1").await.unwrap());
        assert_eq!(count(&db, "SELECT COUNT(*) FROM collections").await, 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM video_files").await, 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM audio_tracks").await, 0);
        // 任务记录保留，只解除对合集和视频的引用。
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM extract_jobs WHERE collection_id IS NULL"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM extract_job_items WHERE video_file_id IS NULL"
            )
            .await,
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn opening_clears_dangling_references_left_by_older_versions() {
        let root = temp_root("dangling");
        let path = root.join("legacy.db");
        create_legacy(
            &path,
            &[
                "INSERT INTO collections(id,name,source_path) VALUES('c1','合集','/videos')",
                "INSERT INTO extract_jobs(id,collection_id,name) VALUES('j1','已删除的合集','旧任务')",
                "INSERT INTO video_files(id,collection_id,filename,filepath) VALUES('v9','已删除的合集','b.mp4','/videos/b.mp4')",
                "INSERT INTO audio_tracks(id,video_file_id,track_index) VALUES('t9','v9',1)",
                "INSERT INTO extract_job_items(id,job_id,video_file_id,source_path) VALUES('i1','j1','已删除的视频','/videos/a.mp4')",
                "INSERT INTO extract_job_items(id,job_id,video_file_id,source_path) VALUES('i9','已删除的任务',NULL,'/videos/b.mp4')",
            ],
        )
        .await;
        let db = Database::open(&path).await.unwrap();

        let violations = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&db.pool)
            .await
            .unwrap();
        assert!(violations.is_empty(), "启动后不应再有悬空引用");
        // 悬空的孤儿行被清掉，仍然有主人的任务和明细留下来。
        assert_eq!(count(&db, "SELECT COUNT(*) FROM video_files").await, 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM audio_tracks").await, 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM extract_jobs").await, 1);
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM extract_job_items").await,
            1
        );
        assert!(db.get_job("j1").await.unwrap().is_some());
        assert!(db.delete_job("j1").await.unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn opening_marks_interrupted_jobs_as_paused() {
        let root = temp_root("interrupted");
        let path = root.join("legacy.db");
        create_legacy(
            &path,
            &[
                "INSERT INTO collections(id,name,source_path) VALUES('c1','合集','/videos')",
                "INSERT INTO extract_jobs(id,collection_id,name,status,current_file) VALUES('j1','c1','跑到一半','processing','a.mp4')",
                "INSERT INTO extract_jobs(id,collection_id,name,status) VALUES('j2','c1','还在排队','queued')",
                "INSERT INTO extract_jobs(id,collection_id,name,status) VALUES('j3','c1','已完成','completed')",
                "INSERT INTO extract_job_items(id,job_id,source_path,status) VALUES('i1','j1','/videos/a.mp4','processing')",
                "INSERT INTO extract_job_items(id,job_id,source_path,status) VALUES('i2','j1','/videos/b.mp4','completed')",
            ],
        )
        .await;
        let db = Database::open(&path).await.unwrap();

        // 进程重启后没人再推进这些任务，状态必须变成可操作的 paused。
        assert_eq!(db.get_job("j1").await.unwrap().unwrap().status, "paused");
        assert_eq!(db.get_job("j2").await.unwrap().unwrap().status, "paused");
        assert_eq!(db.get_job("j3").await.unwrap().unwrap().status, "completed");
        assert!(
            db.get_job("j1")
                .await
                .unwrap()
                .unwrap()
                .current_file
                .is_none()
        );
        // 半路中断的明细退回 pending，已完成的不动。
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM extract_job_items WHERE status='pending'"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM extract_job_items WHERE status='completed'"
            )
            .await,
            1
        );
        // 这类任务此前既跑不动也删不掉，现在可以直接删。
        assert!(db.delete_job("j1").await.unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn job_request_round_trips_for_resume() {
        let root = temp_root("request");
        let path = root.join("legacy.db");
        create_legacy(
            &path,
            &[
                "INSERT INTO collections(id,name,source_path) VALUES('c1','合集','/videos')",
                "INSERT INTO extract_jobs(id,collection_id,name) VALUES('j1','c1','任务')",
            ],
        )
        .await;
        let db = Database::open(&path).await.unwrap();

        // 老库没有 request 列，migrate 要补上，否则「继续」永远拿不到原始参数。
        assert!(db.load_job_request("j1").await.unwrap().is_none());
        let request = ExtractRequest {
            collection_id: Some("c1".into()),
            source_path: None,
            job_name: Some("任务".into()),
            track_index: 3,
            output_format: "m4a".into(),
            quality: "premium".into(),
            sample_rate: 48_000,
            selected_video_ids: Some(vec!["v1".into()]),
            trim_start_seconds: 1.5,
            trim_end_seconds: 2.5,
            filesystem_sorting: Some("natural".into()),
            padding_digits: Some("3".into()),
            output_directory: Some("/mnt/usb".into()),
        };
        db.save_job_request("j1", &request).await.unwrap();

        let loaded = db.load_job_request("j1").await.unwrap().unwrap();
        assert_eq!(loaded.track_index, 3);
        assert_eq!(loaded.output_format, "m4a");
        assert_eq!(loaded.quality, "premium");
        assert_eq!(loaded.sample_rate, 48_000);
        assert_eq!(loaded.trim_start_seconds, 1.5);
        assert_eq!(loaded.trim_end_seconds, 2.5);
        assert_eq!(loaded.output_directory.as_deref(), Some("/mnt/usb"));
        assert_eq!(loaded.selected_video_ids, Some(vec!["v1".into()]));
        std::fs::remove_dir_all(root).unwrap();
    }
}
