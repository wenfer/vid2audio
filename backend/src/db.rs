use crate::models::{
    AppSettings, AudioTrack, Collection, ExtractJob, ExtractJobDetail, ExtractJobItem, VideoFile,
};
use anyhow::Result;
use serde_json::{Map, Value};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{path::Path, str::FromStr, time::Duration};

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
 error_message TEXT, output_path TEXT, summary TEXT DEFAULT '{}', created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
 started_at TIMESTAMP, completed_at TIMESTAMP, FOREIGN KEY (collection_id) REFERENCES collections(id));
CREATE TABLE IF NOT EXISTS extract_job_items (
 id TEXT PRIMARY KEY, job_id TEXT NOT NULL, video_file_id TEXT, source_path TEXT NOT NULL,
 output_path TEXT, title TEXT, status TEXT DEFAULT 'pending', error_message TEXT,
 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, completed_at TIMESTAMP,
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
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{path_string}"))?
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
        ] {
            if !existing.contains(name) {
                sqlx::query(&format!(
                    "ALTER TABLE extract_jobs ADD COLUMN {name} {definition}"
                ))
                .execute(pool)
                .await?;
            }
        }
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
        if !persisted.contains("hardware_acceleration")
            && let Ok(value) = std::env::var("VID2AUDIO_HWACCEL")
        {
            object.insert("hardware_acceleration".into(), Value::String(value));
        }
        if !persisted.contains("hardware_acceleration_device")
            && let Ok(value) = std::env::var("VID2AUDIO_HWACCEL_DEVICE")
        {
            object.insert("hardware_acceleration_device".into(), Value::String(value));
        }
        if !persisted.contains("hardware_acceleration_fallback")
            && let Ok(value) = std::env::var("VID2AUDIO_HWACCEL_FALLBACK")
        {
            object.insert(
                "hardware_acceleration_fallback".into(),
                Value::Bool(matches!(
                    value.to_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )),
            );
        }
        Ok(serde_json::from_value(Value::Object(object))?)
    }

    pub async fn update_settings(&self, patch: Map<String, Value>) -> Result<AppSettings> {
        let mut current = serde_json::to_value(self.load_settings().await?)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        current.extend(patch);
        let settings: AppSettings = serde_json::from_value(Value::Object(current))?;
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

    pub async fn delete_collection(&self, id: &str) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE extract_jobs SET collection_id=NULL WHERE collection_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE extract_job_items SET video_file_id=NULL WHERE video_file_id IN (SELECT id FROM video_files WHERE collection_id=?)").bind(id).execute(&mut *tx).await?;
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
        Ok(sqlx::query("DELETE FROM extract_jobs WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected()
            > 0)
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
        completed_at: row
            .try_get::<Option<String>, _>("completed_at")
            .unwrap_or(None),
    }
}
