use crate::{
    db::Database,
    media::{command_available, last_error, require_command},
    models::{AppSettings, Collection, ExtractRequest},
    sorter::{
        calculate_padding, compare_names, generate_filename, intro_filename, sanitize_filename_part,
    },
};
use tokio::sync::Semaphore;

static JOB_LIMIT: Semaphore = Semaphore::const_new(2);
use anyhow::{Context, Result, bail};
use serde_json::json;
use sqlx::Row;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub async fn run_job(
    db: Database,
    settings: AppSettings,
    collection: Collection,
    request: ExtractRequest,
    job_id: String,
) {
    let _permit = JOB_LIMIT.acquire().await.expect("job semaphore closed");
    if let Err(error) = run_job_inner(&db, &settings, &collection, &request, &job_id).await {
        mark_job_failed(&db, &job_id, &error.to_string()).await;
    }
}

async fn run_job_inner(
    db: &Database,
    settings: &AppSettings,
    collection: &Collection,
    request: &ExtractRequest,
    job_id: &str,
) -> Result<()> {
    require_command("ffmpeg")?;
    sqlx::query("UPDATE extract_jobs SET status='processing',progress=1,started_at=CURRENT_TIMESTAMP WHERE id=?").bind(job_id).execute(&db.pool).await?;
    let selected = request
        .selected_video_ids
        .as_ref()
        .filter(|values| !values.is_empty())
        .map(|v| v.iter().collect::<std::collections::HashSet<_>>());
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
    let output_dir =
        PathBuf::from(&settings.output_directory).join(sanitize_filename_part(&collection.name));
    tokio::fs::create_dir_all(&output_dir).await?;
    let bitrate = match request.quality.as_str() {
        "economy" => "64k",
        "premium" => "192k",
        "lossless" => "320k",
        _ => "128k",
    };
    let mut warnings = Vec::new();
    let mut output_files = Vec::new();
    let mut failures = Vec::new();

    if request.generate_intro {
        let path = output_dir.join(intro_filename(&collection.name, &extension));
        if let Some(warning) = generate_intro(
            request.intro_text.as_deref().unwrap_or(&collection.name),
            &path,
            &extension,
            bitrate,
            request.sample_rate,
            request,
            settings,
        )
        .await?
        {
            warnings.push(warning);
        }
    }
    let padding = calculate_padding(
        videos.len(),
        request.padding_digits.as_deref().unwrap_or("auto"),
    );
    for (position, video) in videos.iter().enumerate() {
        let status: String = sqlx::query("SELECT status FROM extract_jobs WHERE id=?")
            .bind(job_id)
            .fetch_one(&db.pool)
            .await?
            .get(0);
        if status == "cancelled" {
            return Ok(());
        }
        let progress = (((position as f64) / videos.len().max(1) as f64) * 95.0) as i64 + 2;
        sqlx::query("UPDATE extract_jobs SET progress=?,current_file=? WHERE id=?")
            .bind(progress)
            .bind(&video.filename)
            .bind(job_id)
            .execute(&db.pool)
            .await?;
        sqlx::query(
            "UPDATE extract_job_items SET status='processing' WHERE job_id=? AND video_file_id=?",
        )
        .bind(job_id)
        .bind(&video.id)
        .execute(&db.pool)
        .await?;
        let output_name =
            generate_filename(position + 1, &video.episode_title, &extension, padding);
        let output_path = output_dir.join(&output_name);
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
            settings,
        )
        .await
        {
            Ok(()) => {
                output_files.push(output_name);
                sqlx::query("UPDATE extract_job_items SET status='completed',output_path=?,completed_at=CURRENT_TIMESTAMP WHERE job_id=? AND video_file_id=?").bind(output_path.to_string_lossy().as_ref()).bind(job_id).bind(&video.id).execute(&db.pool).await?;
            }
            Err(error) => {
                let message = error.to_string();
                failures.push(json!({"source": video.filepath, "title": video.episode_title, "error": message}));
                sqlx::query("UPDATE extract_job_items SET status='failed',error_message=?,completed_at=CURRENT_TIMESTAMP WHERE job_id=? AND video_file_id=?").bind(&message).bind(job_id).bind(&video.id).execute(&db.pool).await?;
            }
        }
        refresh_counts(db, job_id).await?;
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
    let _ = sqlx::query("UPDATE extract_jobs SET status='failed',progress=100,error_message=?,summary=?,completed_at=CURRENT_TIMESTAMP,failure_count=(SELECT COUNT(*) FROM extract_job_items WHERE job_id=? AND status='failed') WHERE id=? AND status!='cancelled'").bind(message).bind(summary.to_string()).bind(job_id).bind(job_id).execute(&db.pool).await;
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
        "-c:a".into(),
        codec.into(),
    ]);
    if let Some(length) = duration
        .map(|d| (d - trim_start - trim_end).max(0.0))
        .filter(|d| *d > 0.0)
    {
        args.extend(["-t".into(), format!("{length:.3}").into()]);
    }
    if !matches!(extension, "flac" | "wav") {
        args.extend(["-b:a".into(), bitrate.into()]);
    }
    args.extend([
        "-ar".into(),
        sample_rate.to_string().into(),
        "-ac".into(),
        "2".into(),
        "-threads".into(),
        settings.ffmpeg_threads.to_string().into(),
        output.as_os_str().into(),
    ]);
    run_command("ffmpeg", args, None).await
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

async fn generate_intro(
    text: &str,
    output: &Path,
    extension: &str,
    bitrate: &str,
    sample_rate: i64,
    request: &ExtractRequest,
    settings: &AppSettings,
) -> Result<Option<String>> {
    let provider = request
        .tts_provider
        .as_deref()
        .unwrap_or(&settings.tts_provider);
    let failure_mode = request
        .tts_failure_mode
        .as_deref()
        .unwrap_or(&settings.tts_failure_mode);
    if provider == "disabled" {
        return Ok(Some("片头语音已禁用。".into()));
    }
    if provider == "silent" {
        silent_placeholder(output, extension, bitrate, sample_rate).await?;
        return Ok(Some("已按配置使用静音片头占位。".into()));
    }
    if provider == "piper" && command_available("piper") {
        let voice = if request.intro_voice.is_empty() {
            &settings.tts_voice
        } else {
            &request.intro_voice
        };
        if let Some(model) = resolve_piper_model(voice) {
            let raw = output.with_extension("piper.tmp.wav");
            let result = run_command(
                "piper",
                vec![
                    "--model".into(),
                    model.as_os_str().into(),
                    "--output_file".into(),
                    raw.as_os_str().into(),
                ],
                Some(text.as_bytes().to_vec()),
            )
            .await;
            if result.is_ok() {
                let args = vec![
                    "-y".into(),
                    "-i".into(),
                    raw.as_os_str().into(),
                    "-af".into(),
                    "loudnorm=I=-16:TP=-1.5:LRA=11".into(),
                    "-c:a".into(),
                    codec_for(extension)?.into(),
                    "-b:a".into(),
                    bitrate.into(),
                    "-ar".into(),
                    sample_rate.to_string().into(),
                    "-ac".into(),
                    "2".into(),
                    output.as_os_str().into(),
                ];
                let normalized = run_command("ffmpeg", args, None).await;
                let _ = tokio::fs::remove_file(&raw).await;
                normalized?;
                return Ok(None);
            }
        }
    }
    let reason = if provider == "piper" {
        "Piper TTS 未安装或语音模型不存在。"
    } else {
        "未知 TTS 通道。"
    };
    match failure_mode {
        "fail" => bail!("{reason}"),
        "skip" => Ok(Some(format!("片头语音生成失败，已跳过片头: {reason}"))),
        _ => {
            silent_placeholder(output, extension, bitrate, sample_rate).await?;
            Ok(Some(format!(
                "片头语音生成失败，已使用 1 秒静音占位: {reason}"
            )))
        }
    }
}

async fn silent_placeholder(
    output: &Path,
    extension: &str,
    bitrate: &str,
    sample_rate: i64,
) -> Result<()> {
    require_command("ffmpeg")?;
    let mut args: Vec<OsString> = vec![
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("anullsrc=channel_layout=stereo:sample_rate={sample_rate}").into(),
        "-t".into(),
        "1".into(),
        "-c:a".into(),
        codec_for(extension)?.into(),
    ];
    if !matches!(extension, "flac" | "wav") {
        args.extend(["-b:a".into(), bitrate.into()]);
    }
    args.push(output.as_os_str().into());
    run_command("ffmpeg", args, None).await
}

fn resolve_piper_model(voice: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(voice);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct);
    }
    for root in [
        PathBuf::from("/app/data/piper-voices"),
        PathBuf::from("data/piper-voices"),
    ] {
        for candidate in [
            root.join(format!("{voice}.onnx")),
            root.join(voice).join(format!("{voice}.onnx")),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if root.is_dir()
            && let Some(path) = walkdir::WalkDir::new(root)
                .into_iter()
                .filter_map(Result::ok)
                .map(|e| e.into_path())
                .find(|p| p.extension().is_some_and(|e| e == "onnx"))
        {
            return Some(path);
        }
    }
    None
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
        let mut command = Command::new(program);
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
