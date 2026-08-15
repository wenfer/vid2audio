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
        bail!(
            "ffprobe 解析失败: {}",
            command_error(output.status.code(), &output.stderr)
        );
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
    let text = String::from_utf8_lossy(bytes);
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| {
            // 每行最多 500 字符，从行尾截断（ffmpeg 报错的关键信息常在行尾）。
            line.chars()
                .rev()
                .take(500)
                .collect::<String>()
                .chars()
                .rev()
                .collect()
        })
        .collect();
    if lines.is_empty() {
        return "命令执行失败".into();
    }
    // ffmpeg 的报错常是两行：`Error opening output file <路径>: ...` 后接
    // `Error opening output files: ...`。只留最后一行会丢掉具体路径，留最后两行。
    let keep = lines.len().min(2);
    lines.drain(..lines.len() - keep);
    lines.join("\n")
}

/// 命令失败后给用户看的错误文本。
///
/// stderr 为空时必须退回退出码。Windows 上缺依赖 DLL 的进程在 main 之前就被加载器
/// 杀掉，一个字节都写不出来；随包 ffmpeg 少一个 DLL 时，这里原来一律显示「命令执行
/// 失败」，把唯一的线索（0xC0000135）丢掉了，只能靠翻 PE 导入表才定位到。
pub fn command_error(code: Option<i32>, stderr: &[u8]) -> String {
    if !String::from_utf8_lossy(stderr).trim().is_empty() {
        return last_error(stderr);
    }
    match code {
        // NTSTATUS。加载器在启动阶段就失败，基本只有缺 DLL 和版本不匹配两种可能。
        Some(code) if code as u32 == 0xC000_0135 => {
            "缺少依赖 DLL，进程未能启动（0xC0000135）".into()
        }
        Some(code) if code as u32 == 0xC000_0139 => {
            "依赖 DLL 缺少入口点，进程未能启动（0xC0000139）".into()
        }
        Some(code) => {
            format!(
                "命令异常退出，退出码 {code}（0x{:08X}），且没有输出错误信息",
                code as u32
            )
        }
        None => "命令被信号终止，且没有输出错误信息".into(),
    }
}

/// 提取失败时存进任务明细的完整错误：两行摘要 + 完整 stderr（截断）。
///
/// `last_error` 只留最后两行，排查时常常不够——比如 ffmpeg 报
/// "Error opening output files: Invalid argument" 时，具体是哪条路径、为什么
/// 打不开，要看 stderr 前面的行。完整日志让任务详情页能直接展示。
pub fn full_command_error(code: Option<i32>, stderr: &[u8]) -> String {
    let summary = command_error(code, stderr);
    let lossy = String::from_utf8_lossy(stderr);
    let detail = lossy.trim();
    if detail.is_empty() {
        return summary;
    }
    let mut full = summary;
    full.push_str("\n\n--- 完整日志 ---\n");
    let take: String = detail.chars().take(3000).collect();
    full.push_str(&take);
    if detail.chars().count() > 3000 {
        full.push_str("\n…（日志过长已截断）");
    }
    full
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_wins_when_present() {
        let text = command_error(Some(1), b"first line\nreal ffmpeg complaint\n");
        assert_eq!(text, "first line\nreal ffmpeg complaint");
    }

    #[test]
    fn empty_stderr_falls_back_to_exit_code() {
        // 装出来那个坏包的真实表现：stderr 全空，只有退出码能说明问题。
        let text = command_error(Some(0xC000_0135u32 as i32), b"");
        assert!(text.contains("缺少依赖 DLL"), "{text}");
        assert!(text.contains("0xC0000135"), "{text}");
    }

    #[test]
    fn blank_stderr_is_treated_as_empty() {
        let text = command_error(Some(3), b"  \n\n");
        assert!(text.contains("退出码 3"), "{text}");
        assert!(text.contains("0x00000003"), "{text}");
    }

    #[test]
    fn unknown_status_still_says_something() {
        assert!(command_error(None, b"").contains("信号"));
    }
}
