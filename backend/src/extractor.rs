use crate::{
    db::Database,
    media::{last_error, require_command},
    models::{AppSettings, Collection, ExtractRequest},
    sorter::{calculate_padding, compare_names, generate_filename, sanitize_filename_part},
};
use anyhow::{Context, Result, bail};
use serde_json::json;
use sqlx::Row;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};
use tokio::sync::Notify;

struct JobLimiter {
    state: Mutex<LimiterState>,
    notify: Notify,
}

struct LimiterState {
    active: usize,
    limit: usize,
}

struct JobPermit {
    limiter: Arc<JobLimiter>,
}

impl Drop for JobPermit {
    fn drop(&mut self) {
        let mut state = self.limiter.state.lock().expect("job limiter poisoned");
        state.active = state.active.saturating_sub(1);
        drop(state);
        self.limiter.notify.notify_one();
    }
}

impl JobLimiter {
    async fn acquire(self: &Arc<Self>, requested_limit: i64) -> JobPermit {
        let limit = requested_limit.clamp(1, 32) as usize;
        loop {
            let notified = self.notify.notified();
            {
                let mut state = self.state.lock().expect("job limiter poisoned");
                state.limit = limit;
                if state.active < state.limit {
                    state.active += 1;
                    return JobPermit {
                        limiter: Arc::clone(self),
                    };
                }
            }
            notified.await;
        }
    }
}

fn job_limiter() -> &'static Arc<JobLimiter> {
    static LIMITER: OnceLock<Arc<JobLimiter>> = OnceLock::new();
    LIMITER.get_or_init(|| {
        Arc::new(JobLimiter {
            state: Mutex::new(LimiterState {
                active: 0,
                limit: 2,
            }),
            notify: Notify::new(),
        })
    })
}

/// 每次运行一个任务都会拿到新的世代号。暂停后工作线程要跑完当前这个文件才会退出，
/// 如果用户在这个空档里点了「继续」，新旧两个线程会同时提取同一批文件——
/// 旧线程发现世代号变了就自己退出。
fn job_epochs() -> &'static Mutex<std::collections::HashMap<String, u64>> {
    static EPOCHS: OnceLock<Mutex<std::collections::HashMap<String, u64>>> = OnceLock::new();
    EPOCHS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn begin_run(job_id: &str) -> u64 {
    let mut epochs = job_epochs().lock().expect("job epochs poisoned");
    let epoch = epochs.get(job_id).copied().unwrap_or(0) + 1;
    epochs.insert(job_id.to_string(), epoch);
    epoch
}

fn is_current_run(job_id: &str, epoch: u64) -> bool {
    job_epochs()
        .lock()
        .expect("job epochs poisoned")
        .get(job_id)
        .copied()
        == Some(epoch)
}

fn end_run(job_id: &str, epoch: u64) {
    let mut epochs = job_epochs().lock().expect("job epochs poisoned");
    if epochs.get(job_id).copied() == Some(epoch) {
        epochs.remove(job_id);
    }
}

pub async fn run_job(
    db: Database,
    settings: AppSettings,
    collection: Collection,
    request: ExtractRequest,
    job_id: String,
) {
    let epoch = begin_run(&job_id);
    let _permit = job_limiter().acquire(settings.extraction_concurrency).await;
    if !is_current_run(&job_id, epoch) {
        return;
    }
    if let Err(error) = run_job_inner(&db, &settings, &collection, &request, &job_id, epoch).await {
        mark_job_failed(&db, &job_id, &error.to_string()).await;
    }
    end_run(&job_id, epoch);
}

async fn run_job_inner(
    db: &Database,
    settings: &AppSettings,
    collection: &Collection,
    request: &ExtractRequest,
    job_id: &str,
    epoch: u64,
) -> Result<()> {
    require_command("ffmpeg")?;
    // 排队等并发额度时用户可能已经暂停、取消或删除了任务，这时不要把状态改回 processing。
    let queued_status = sqlx::query("SELECT status FROM extract_jobs WHERE id=?")
        .bind(job_id)
        .fetch_optional(&db.pool)
        .await?
        .map(|row| row.get::<String, _>(0));
    if !matches!(queued_status.as_deref(), Some("queued" | "processing")) {
        return Ok(());
    }
    sqlx::query("UPDATE extract_jobs SET status='processing',progress=1,started_at=CURRENT_TIMESTAMP WHERE id=?").bind(job_id).execute(&db.pool).await?;
    let selected = request
        .selected_video_ids
        .as_ref()
        .filter(|values| !values.is_empty())
        .map(|v| v.iter().collect::<std::collections::HashSet<_>>());
    // 继续执行时，已经成功提取过的文件不再重来一遍。
    let finished: std::collections::HashSet<String> = sqlx::query(
        "SELECT video_file_id FROM extract_job_items WHERE job_id=? AND status='completed' AND video_file_id IS NOT NULL",
    )
    .bind(job_id)
    .fetch_all(&db.pool)
    .await?
    .iter()
    .filter_map(|row| row.try_get::<Option<String>, _>(0).unwrap_or(None))
    .collect();
    let mut videos: Vec<_> = collection
        .video_files
        .iter()
        .filter(|v| selected.as_ref().is_none_or(|ids| ids.contains(&v.id)))
        .collect();
    videos.sort_by(|a, b| {
        a.episode_number.cmp(&b.episode_number).then_with(|| {
            compare_names(
                &a.filename,
                &b.filename,
                request.filesystem_sorting.as_deref().unwrap_or("ntfs"),
            )
        })
    });
    let mut extension = request
        .output_format
        .to_lowercase()
        .trim_start_matches('.')
        .to_string();
    if extension == "aac" {
        extension = "m4a".into();
    }
    let output_base = request
        .output_directory
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&settings.output_directory);
    let output_dir = PathBuf::from(output_base).join(sanitize_filename_part(&collection.name));
    tokio::fs::create_dir_all(&output_dir).await?;
    let bitrate = match request.quality.as_str() {
        "economy" => "64k",
        "premium" => "192k",
        "lossless" => "320k",
        _ => "128k",
    };
    let warnings = Vec::new();
    let mut output_files = Vec::new();
    let mut failures = Vec::new();

    let padding = calculate_padding(
        videos.len(),
        request.padding_digits.as_deref().unwrap_or("auto"),
    );
    for (position, video) in videos.iter().enumerate() {
        // 用户在暂停的空档里点了「继续」，新线程已经接手，这个旧线程直接退出。
        if !is_current_run(job_id, epoch) {
            return Ok(());
        }
        // 任务可能已被删除，这时安静退出，不要报错也不要重建任何行。
        let Some(row) = sqlx::query("SELECT status FROM extract_jobs WHERE id=?")
            .bind(job_id)
            .fetch_optional(&db.pool)
            .await?
        else {
            return Ok(());
        };
        let status: String = row.get(0);
        if status == "cancelled" {
            return Ok(());
        }
        if status == "paused" {
            // 暂停：把还没开始的明细放回 pending，等用户点「继续」时再接着跑。
            sqlx::query("UPDATE extract_job_items SET status='pending',started_at=NULL WHERE job_id=? AND status='processing'")
                .bind(job_id)
                .execute(&db.pool)
                .await?;
            sqlx::query("UPDATE extract_jobs SET current_file=NULL WHERE id=?")
                .bind(job_id)
                .execute(&db.pool)
                .await?;
            return Ok(());
        }
        // 位置编号按完整列表算，跳过已完成项也不会让文件序号错位。
        if finished.contains(&video.id) {
            continue;
        }
        let progress = (((position as f64) / videos.len().max(1) as f64) * 95.0) as i64 + 2;
        sqlx::query("UPDATE extract_jobs SET progress=?,current_file=? WHERE id=?")
            .bind(progress)
            .bind(&video.filename)
            .bind(job_id)
            .execute(&db.pool)
            .await?;
        sqlx::query("UPDATE extract_job_items SET status='processing',started_at=CURRENT_TIMESTAMP,duration_seconds=NULL WHERE job_id=? AND video_file_id=?")
        .bind(job_id)
        .bind(&video.id)
        .execute(&db.pool)
        .await?;
        let extraction_started = Instant::now();
        let output_name =
            generate_filename(position + 1, &video.episode_title, &extension, padding);
        let output_path = output_dir.join(&output_name);
        let source_track = video
            .audio_tracks
            .iter()
            .find(|track| track.index == request.track_index);
        let stream_copy = can_stream_copy_mp3(
            &extension,
            source_track,
            bitrate,
            request.sample_rate,
            request.trim_start_seconds,
            request.trim_end_seconds,
        );
        match extract_one(
            video.filepath.clone(),
            request.track_index,
            output_path.clone(),
            &extension,
            bitrate,
            request.sample_rate,
            video.duration,
            request.trim_start_seconds,
            request.trim_end_seconds,
            stream_copy,
            settings,
        )
        .await
        {
            Ok(()) => {
                let duration_seconds = extraction_started.elapsed().as_secs_f64();
                output_files.push(output_name);
                sqlx::query("UPDATE extract_job_items SET status='completed',output_path=?,completed_at=CURRENT_TIMESTAMP,duration_seconds=? WHERE job_id=? AND video_file_id=?").bind(output_path.to_string_lossy().as_ref()).bind(duration_seconds).bind(job_id).bind(&video.id).execute(&db.pool).await?;
            }
            Err(error) => {
                let duration_seconds = extraction_started.elapsed().as_secs_f64();
                let message = error.to_string();
                failures.push(json!({"source": video.filepath, "title": video.episode_title, "error": message}));
                sqlx::query("UPDATE extract_job_items SET status='failed',error_message=?,completed_at=CURRENT_TIMESTAMP,duration_seconds=? WHERE job_id=? AND video_file_id=?").bind(&message).bind(duration_seconds).bind(job_id).bind(&video.id).execute(&db.pool).await?;
            }
        }
        refresh_counts(db, job_id).await?;
    }
    // 继续执行的任务里，上一轮产出的文件也要留在汇总中。
    if !finished.is_empty() {
        let previous = sqlx::query(
            "SELECT output_path FROM extract_job_items WHERE job_id=? AND status='completed' AND output_path IS NOT NULL",
        )
        .bind(job_id)
        .fetch_all(&db.pool)
        .await?;
        for row in previous {
            let Some(path) = row.try_get::<Option<String>, _>(0).unwrap_or(None) else {
                continue;
            };
            let name = Path::new(&path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if !name.is_empty() && !output_files.contains(&name) {
                output_files.push(name);
            }
        }
    }
    output_files.sort_by(|a, b| compare_names(a, b, "ntfs"));
    let row =
        sqlx::query("SELECT success_count,failure_count,total_count FROM extract_jobs WHERE id=?")
            .bind(job_id)
            .fetch_one(&db.pool)
            .await?;
    let success_count: i64 = row.get(0);
    let failure_count: i64 = row.get(1);
    let total_count: i64 = row.get(2);
    let final_status = if failure_count == 0 {
        "completed"
    } else {
        "failed"
    };
    let summary = json!({"output_files": output_files, "failures": failures, "warnings": warnings, "success_count": success_count, "failure_count": failure_count, "total_count": total_count});
    sqlx::query("UPDATE extract_jobs SET status=?,progress=100,current_file=?,output_path=?,success_count=?,failure_count=?,summary=?,completed_at=CURRENT_TIMESTAMP WHERE id=? AND status='processing'")
        .bind(final_status).bind(format!("完成: 成功 {success_count}，失败 {failure_count}")).bind(output_dir.to_string_lossy().as_ref()).bind(success_count).bind(failure_count).bind(summary.to_string()).bind(job_id).execute(&db.pool).await?;
    sqlx::query(
        "UPDATE collections SET status=?,output_path=?,updated_at=CURRENT_TIMESTAMP WHERE id=?",
    )
    .bind(if failure_count == 0 {
        "completed"
    } else {
        "error"
    })
    .bind(output_dir.to_string_lossy().as_ref())
    .bind(&collection.id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

async fn refresh_counts(db: &Database, job_id: &str) -> Result<()> {
    let row = sqlx::query("SELECT SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END),SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END) FROM extract_job_items WHERE job_id=?").bind(job_id).fetch_one(&db.pool).await?;
    let success: i64 = row.try_get::<Option<i64>, _>(0)?.unwrap_or(0);
    let failure: i64 = row.try_get::<Option<i64>, _>(1)?.unwrap_or(0);
    let total: i64 = sqlx::query("SELECT total_count FROM extract_jobs WHERE id=?")
        .bind(job_id)
        .fetch_one(&db.pool)
        .await?
        .get(0);
    let progress = (((success + failure) as f64 / total.max(1) as f64) * 95.0) as i64 + 2;
    sqlx::query("UPDATE extract_jobs SET success_count=?,failure_count=?,progress=? WHERE id=?")
        .bind(success)
        .bind(failure)
        .bind(progress.min(99))
        .bind(job_id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

async fn mark_job_failed(db: &Database, job_id: &str, message: &str) {
    let _ = sqlx::query("UPDATE extract_job_items SET status='failed',error_message=?,completed_at=CURRENT_TIMESTAMP WHERE job_id=? AND status IN ('pending','processing')").bind(message).bind(job_id).execute(&db.pool).await;
    let summary = json!({"failures": [{"error": message}]});
    // 已暂停或已取消的任务不要被改成 failed，否则用户点「继续」时看到的状态是错的。
    let _ = sqlx::query("UPDATE extract_jobs SET status='failed',progress=100,error_message=?,summary=?,completed_at=CURRENT_TIMESTAMP,failure_count=(SELECT COUNT(*) FROM extract_job_items WHERE job_id=? AND status='failed') WHERE id=? AND status NOT IN ('cancelled','paused')").bind(message).bind(summary.to_string()).bind(job_id).bind(job_id).execute(&db.pool).await;
}

#[allow(clippy::too_many_arguments)]
async fn extract_one(
    source: String,
    track: i64,
    output: PathBuf,
    extension: &str,
    bitrate: &str,
    sample_rate: i64,
    duration: Option<f64>,
    trim_start: f64,
    trim_end: f64,
    stream_copy: bool,
    settings: &AppSettings,
) -> Result<()> {
    if let Some(duration) = duration {
        if duration - trim_start - trim_end <= 0.0 {
            bail!("裁剪范围无效：开头与结尾裁剪之和超过视频时长");
        }
    } else if trim_end > 0.0 {
        bail!("无法获取视频时长，不能应用结尾裁剪");
    }
    let codec = codec_for(extension)?;
    let mut args: Vec<OsString> = vec!["-y".into()];
    if trim_start > 0.0 {
        args.extend(["-ss".into(), trim_start.to_string().into()]);
    }
    args.extend([
        "-i".into(),
        source.clone().into(),
        "-map".into(),
        format!("0:{track}").into(),
        "-vn".into(),
    ]);
    if stream_copy {
        args.extend(["-c:a".into(), "copy".into()]);
    } else {
        args.extend(["-c:a".into(), codec.into()]);
    }
    if let Some(length) = duration
        .map(|d| (d - trim_start - trim_end).max(0.0))
        .filter(|d| *d > 0.0)
    {
        args.extend(["-t".into(), format!("{length:.3}").into()]);
    }
    if !stream_copy && !matches!(extension, "flac" | "wav") {
        args.extend(["-b:a".into(), bitrate.into()]);
    }
    if !stream_copy {
        args.extend([
            "-ar".into(),
            sample_rate.to_string().into(),
            "-ac".into(),
            "2".into(),
            "-threads".into(),
            settings.ffmpeg_threads.to_string().into(),
        ]);
    }
    args.push(output.as_os_str().into());
    run_command("ffmpeg", args, None).await
}

fn can_stream_copy_mp3(
    extension: &str,
    track: Option<&crate::models::AudioTrack>,
    bitrate: &str,
    sample_rate: i64,
    trim_start: f64,
    trim_end: f64,
) -> bool {
    let Some(track) = track else { return false };
    let requested_bitrate = bitrate
        .strip_suffix('k')
        .and_then(|value| value.parse::<i64>().ok())
        .map(|value| value * 1000);
    extension.eq_ignore_ascii_case("mp3")
        && track.codec.eq_ignore_ascii_case("mp3")
        && trim_start <= f64::EPSILON
        && trim_end <= f64::EPSILON
        && track.sample_rate == Some(sample_rate)
        && track.channels == Some(2)
        && requested_bitrate.is_some_and(|requested| {
            track
                .bitrate
                .is_some_and(|actual| (actual - requested).abs() <= 8_000)
        })
}

pub async fn preview(
    source: &str,
    track: i64,
    output: &Path,
    duration: i64,
    start: f64,
) -> Result<()> {
    require_command("ffmpeg")?;
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut args: Vec<OsString> = vec!["-y".into()];
    if start > 0.0 {
        args.extend(["-ss".into(), start.to_string().into()]);
    }
    args.extend([
        "-i".into(),
        source.into(),
        "-map".into(),
        format!("0:{track}").into(),
        "-t".into(),
        duration.clamp(1, 60).to_string().into(),
        "-c:a".into(),
        "libmp3lame".into(),
        "-b:a".into(),
        "128k".into(),
        output.as_os_str().into(),
    ]);
    run_command("ffmpeg", args, None).await
}

fn codec_for(extension: &str) -> Result<&'static str> {
    Ok(match extension {
        "mp3" => "libmp3lame",
        "m4a" | "aac" => "aac",
        "ogg" => "libvorbis",
        "flac" => "flac",
        "wav" => "pcm_s16le",
        "opus" => "libopus",
        _ => bail!("不支持的输出格式: {extension}"),
    })
}

async fn run_command(
    program: &'static str,
    args: Vec<OsString>,
    stdin: Option<Vec<u8>>,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        let mut command = crate::media::command(program)?;
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("无法启动 {program}"))?;
        if let Some(data) = stdin {
            use std::io::Write;
            child
                .stdin
                .take()
                .context("无法写入命令输入")?
                .write_all(&data)?;
        }
        let output = child.wait_with_output()?;
        if output.status.success() {
            Ok(())
        } else {
            bail!("{}", last_error(&output.stderr))
        }
    })
    .await??;
    Ok(())
}
