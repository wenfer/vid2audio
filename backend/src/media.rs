use crate::{models::AudioTrack, platform};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::Path;

pub fn command_available(name: &str) -> bool {
    platform::find_command(name).is_some()
}

pub fn require_command(name: &str) -> Result<()> {
    if command_available(name) {
        Ok(())
    } else {
        bail!("未找到 {name}，请先安装 FFmpeg 并确保它在 PATH 中。")
    }
}

/// 解析命令路径后再构造，这样能用上随程序分发的 ffmpeg，也不会弹控制台窗口。
pub fn command(name: &str) -> Result<std::process::Command> {
    let program = platform::find_command(name)
        .ok_or_else(|| anyhow::anyhow!("未找到 {name}，请先安装 FFmpeg 并确保它在 PATH 中。"))?;
    Ok(platform::command(&program))
}

pub fn probe_video(path: &Path) -> Result<(Option<f64>, String, String, Vec<AudioTrack>)> {
    let output = command("ffprobe")?
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .arg(path)
        .output()
        .with_context(|| format!("无法启动 ffprobe: {}", path.display()))?;
    if !output.status.success() {
        bail!("ffprobe 解析失败: {}", last_error(&output.stderr));
    }
    let payload: Value =
        serde_json::from_slice(&output.stdout).context("ffprobe 返回了无效 JSON")?;
    let streams = payload["streams"].as_array().cloned().unwrap_or_default();
    let video = streams.iter().find(|s| s["codec_type"] == "video");
    let mut tracks = Vec::new();
    for stream in streams.iter().filter(|s| s["codec_type"] == "audio") {
        let language = stream["tags"]["language"]
            .as_str()
            .unwrap_or("und")
            .to_lowercase();
        tracks.push(AudioTrack {
            index: stream["index"].as_i64().unwrap_or(tracks.len() as i64),
            codec: stream["codec_name"].as_str().unwrap_or_default().into(),
            language_full: language_name(&language),
            language,
            channels: stream["channels"].as_i64(),
            sample_rate: parse_i64(&stream["sample_rate"]),
            bitrate: parse_i64(&stream["bit_rate"]),
            title: stream["tags"]["title"].as_str().unwrap_or_default().into(),
            is_default: stream["disposition"]["default"].as_i64().unwrap_or(0) != 0,
            ..Default::default()
        });
    }
    let duration = payload["format"]["duration"]
        .as_str()
        .and_then(|v| v.parse().ok());
    let codec = video
        .and_then(|v| v["codec_name"].as_str())
        .unwrap_or_default()
        .into();
    let resolution = video
        .and_then(|v| {
            Some(format!(
                "{}x{}",
                v["width"].as_i64()?,
                v["height"].as_i64()?
            ))
        })
        .unwrap_or_default();
    Ok((duration, codec, resolution, tracks))
}

fn parse_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}

fn language_name(code: &str) -> String {
    match code {
        "chi" | "zho" | "zh" | "chs" | "cht" => "中文",
        "eng" | "en" => "English",
        "jpn" | "ja" => "日本語",
        "kor" | "ko" => "한국어",
        "und" => "未知语言",
        other => other,
    }
    .into()
}

pub fn last_error(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .last()
        .unwrap_or("命令执行失败")
        .chars()
        .rev()
        .take(500)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}
