use crate::models::AudioTrack;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path, process::Command};

pub fn command_available(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let path = dir.join(name);
            path.is_file() || cfg!(windows) && path.with_extension("exe").is_file()
        })
    })
}

pub fn require_command(name: &str) -> Result<()> {
    if command_available(name) {
        Ok(())
    } else {
        bail!("未找到 {name}，请在系统或 Docker 镜像中安装 FFmpeg。")
    }
}

pub fn probe_video(path: &Path) -> Result<(Option<f64>, String, String, Vec<AudioTrack>)> {
    require_command("ffprobe")?;
    let output = Command::new("ffprobe")
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

pub fn detect_hardware_acceleration() -> Value {
    if !command_available("ffmpeg") {
        return json!({"available": false, "supported": [], "backends": [], "recommended": "safe", "note": "未找到 ffmpeg。本地运行请先安装 FFmpeg；默认 Docker 镜像已内置。", "ffmpeg_version": null});
    }
    let version_output = Command::new("ffmpeg").arg("-version").output().ok();
    let version_text = version_output
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stdout))
        .unwrap_or_default();
    let version = version_text
        .lines()
        .find(|line| line.starts_with("ffmpeg version"))
        .and_then(|line| line.split_whitespace().nth(2));
    let accel_output = Command::new("ffmpeg")
        .args(["-hide_banner", "-hwaccels"])
        .output()
        .ok();
    let accel_text = accel_output
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stdout))
        .unwrap_or_default();
    let mut supported: Vec<String> = accel_text
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty() && !v.to_lowercase().starts_with("hardware"))
        .map(str::to_string)
        .collect();
    let decoder_output = Command::new("ffmpeg")
        .args(["-hide_banner", "-decoders"])
        .output()
        .ok();
    if decoder_output.as_ref().is_some_and(|o| {
        String::from_utf8_lossy(&o.stdout)
            .to_lowercase()
            .contains("rkmpp")
    }) {
        supported.push("rkmpp".into());
    }
    let mut seen = HashSet::new();
    supported.retain(|v| seen.insert(v.clone()));
    let preferred = ["qsv", "vaapi", "cuda", "rkmpp", "videotoolbox"]
        .into_iter()
        .find(|candidate| supported.iter().any(|v| v.eq_ignore_ascii_case(candidate)))
        .unwrap_or("safe");
    let definitions = [
        (
            "safe",
            "CPU 软解",
            "纯 CPU 处理，兼容性最好，适合所有环境",
            "cpu",
            "",
        ),
        (
            "qsv",
            "Intel QSV",
            "Intel 核显加速，适合 Intel NAS 和 PC",
            "chip",
            "/dev/dri/renderD128",
        ),
        (
            "vaapi",
            "VAAPI",
            "Linux 通用视频加速接口，支持 Intel/AMD GPU",
            "chip",
            "/dev/dri/renderD128",
        ),
        (
            "cuda",
            "NVIDIA CUDA",
            "NVIDIA 显卡加速，需要 NVIDIA Container Toolkit",
            "gpu",
            "",
        ),
        (
            "rkmpp",
            "Rockchip MPP",
            "瑞芯微 ARM SoC 硬件解码，适合 ARM NAS",
            "arm",
            "/dev/mpp_service",
        ),
        (
            "videotoolbox",
            "Apple VideoToolbox",
            "macOS 原生硬件加速",
            "apple",
            "",
        ),
    ];
    let backends: Vec<Value> = definitions.into_iter().map(|(id, name, description, icon, device)| json!({
        "id": id, "name": name, "description": description, "icon": icon,
        "detected": id == "safe" || supported.iter().any(|v| v.eq_ignore_ascii_case(id)),
        "is_recommended": id == preferred, "device_hint": device,
        "note": if id == "safe" { "音频提取主要处理音频流，CPU 模式通常已足够快" } else if id == "rkmpp" { "基于编解码器的加速，音频提取时不强制视频解码" } else { "" }
    })).collect();
    let note = if preferred == "safe" {
        "音频提取主要处理音频流，建议保持安全模式。"
    } else {
        "检测到可用硬件后端；失败时会按配置回退 CPU。"
    };
    json!({"available": !supported.is_empty(), "supported": supported, "backends": backends, "recommended": preferred, "note": note, "ffmpeg_version": version})
}

pub fn resolve_hardware_acceleration(mode: &str) -> String {
    if !mode.eq_ignore_ascii_case("auto") {
        return mode.to_lowercase();
    }
    detect_hardware_acceleration()["recommended"]
        .as_str()
        .unwrap_or("safe")
        .into()
}

pub fn acceleration_args(mode: &str, device: &str) -> Vec<String> {
    let mode = resolve_hardware_acceleration(mode);
    match mode.as_str() {
        "vaapi" => {
            let mut args = vec!["-hwaccel".into(), "vaapi".into()];
            if !device.is_empty() {
                args.extend(["-hwaccel_device".into(), device.into()]);
            }
            args
        }
        "qsv" | "cuda" | "videotoolbox" | "dxva2" | "d3d11va" => vec!["-hwaccel".into(), mode],
        _ => vec![],
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_acceleration_args() {
        assert_eq!(acceleration_args("qsv", ""), ["-hwaccel", "qsv"]);
        assert_eq!(
            acceleration_args("vaapi", "/dev/dri/renderD128"),
            [
                "-hwaccel",
                "vaapi",
                "-hwaccel_device",
                "/dev/dri/renderD128"
            ]
        );
        assert!(acceleration_args("rkmpp", "").is_empty());
    }
}
