from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

from backend.app.models.schemas import AudioTrack


LANGUAGE_NAMES = {
    "chi": "中文",
    "zho": "中文",
    "zh": "中文",
    "chs": "中文",
    "cht": "中文",
    "eng": "English",
    "en": "English",
    "jpn": "日本語",
    "ja": "日本語",
    "kor": "한국어",
    "ko": "한국어",
    "und": "未知语言",
}


def command_available(name: str) -> bool:
    return shutil.which(name) is not None


def require_command(name: str) -> None:
    if not command_available(name):
        raise RuntimeError(f"未找到 {name}，请在系统或 Docker 镜像中安装 FFmpeg。")


def detect_hardware_acceleration() -> dict[str, object]:
    if not command_available("ffmpeg"):
        return {
            "available": False,
            "supported": [],
            "recommended": "safe",
            "note": "未找到 ffmpeg。Docker 镜像会安装 FFmpeg，本机运行需先安装。",
        }
    result = subprocess.run(["ffmpeg", "-hide_banner", "-hwaccels"], capture_output=True, text=True, check=False)
    supported = []
    for line in result.stdout.splitlines():
        value = line.strip()
        if value and not value.lower().startswith("hardware"):
            supported.append(value)
    decoder_result = subprocess.run(["ffmpeg", "-hide_banner", "-decoders"], capture_output=True, text=True, check=False)
    if "rkmpp" in decoder_result.stdout.lower():
        supported.append("rkmpp")
    supported = sorted(set(supported), key=supported.index)
    preferred = _preferred_acceleration(supported)
    return {
        "available": bool(supported),
        "supported": supported,
        "recommended": preferred,
        "note": _acceleration_note(preferred, supported),
    }


def ffmpeg_acceleration_args(mode: str, device: str = "") -> list[str]:
    normalized = resolve_hardware_acceleration(mode)
    if normalized in {"safe", "disabled", "off", "none"}:
        return []
    if normalized == "vaapi":
        args = ["-hwaccel", "vaapi"]
        if device:
            args.extend(["-hwaccel_device", device])
        return args
    if normalized in {"qsv", "cuda", "videotoolbox", "dxva2", "d3d11va"}:
        return ["-hwaccel", normalized]
    if normalized == "rkmpp":
        # Rockchip MPP is exposed through codec-specific FFmpeg decoders such
        # as h264_rkmpp/hevc_rkmpp, not a generic -hwaccel flag. Vid2Audio
        # currently maps audio streams only, so no video decoder is forced.
        return []
    return []


def resolve_hardware_acceleration(mode: str | None) -> str:
    normalized = (mode or "auto").lower()
    if normalized != "auto":
        return normalized
    detected = detect_hardware_acceleration()
    recommended = str(detected.get("recommended") or "safe").lower()
    if recommended in {"qsv", "vaapi", "cuda", "videotoolbox", "dxva2", "d3d11va", "rkmpp"}:
        return recommended
    return "safe"


def _preferred_acceleration(supported: list[str]) -> str:
    lowered = {item.lower() for item in supported}
    for candidate in ["qsv", "vaapi", "cuda", "rkmpp", "videotoolbox"]:
        if candidate in lowered:
            return candidate
    return "safe"


def _acceleration_note(preferred: str, supported: list[str]) -> str:
    if not supported:
        return "当前 FFmpeg 未报告硬件加速后端。自动模式会使用 CPU 路径。"
    if preferred == "safe":
        return "音频提取主要处理音频流，硬件视频解码通常收益有限，建议保持安全模式。"
    if preferred == "rkmpp":
        return (
            "检测到 Rockchip rkmpp 解码器。自动模式会识别该能力；但音频提取通常只映射音频流，"
            "不会强制视频解码。若镜像 FFmpeg 或设备映射不完整，会继续使用 CPU 路径。"
        )
    return (
        f"检测到 {preferred}。自动模式会优先尝试该后端；如果 NAS 驱动或容器设备映射不完整，会自动回退到 CPU。"
    )


def probe_video(path: str | Path) -> tuple[dict[str, object], list[AudioTrack]]:
    require_command("ffprobe")
    command = [
        "ffprobe",
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_streams",
        "-show_format",
        str(path),
    ]
    result = subprocess.run(command, capture_output=True, text=True, check=True)
    payload = json.loads(result.stdout or "{}")
    streams = payload.get("streams", [])
    audio_tracks: list[AudioTrack] = []
    video_stream = next((s for s in streams if s.get("codec_type") == "video"), {})
    for stream in streams:
        if stream.get("codec_type") != "audio":
            continue
        tags = stream.get("tags") or {}
        language = (tags.get("language") or "und").lower()
        audio_tracks.append(
            AudioTrack(
                index=int(stream.get("index", len(audio_tracks))),
                codec=stream.get("codec_name") or "",
                language=language,
                language_full=LANGUAGE_NAMES.get(language, language),
                channels=stream.get("channels"),
                sample_rate=_int_or_none(stream.get("sample_rate")),
                bitrate=_int_or_none(stream.get("bit_rate")),
                title=tags.get("title") or "",
                default=bool((stream.get("disposition") or {}).get("default")),
            )
        )
    metadata = {
        "duration": _float_or_none((payload.get("format") or {}).get("duration")),
        "video_codec": video_stream.get("codec_name") or "",
        "resolution": _resolution(video_stream),
    }
    return metadata, audio_tracks


def _resolution(stream: dict[str, object]) -> str:
    width = stream.get("width")
    height = stream.get("height")
    if width and height:
        return f"{width}x{height}"
    return ""


def _int_or_none(value: object) -> int | None:
    try:
        return int(value) if value is not None else None
    except (TypeError, ValueError):
        return None


def _float_or_none(value: object) -> float | None:
    try:
        return float(value) if value is not None else None
    except (TypeError, ValueError):
        return None
